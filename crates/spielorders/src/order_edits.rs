//! The canonical assignment language for an Order's own facts, and planning and
//! persistence for the Order record's own mutations: one atomic scalar-fact assignment
//! set, and the ordered embedded Order Item collection.
//!
//! **The assignment vocabulary lives here for every capability that takes one.** Which
//! fields an Order has, which of them may be assigned, and which rule each refusal names
//! are Package rules, so an edit and the two capabilities that give an Order its first
//! record — creation and adoption — adjudicate one set against one list. Two lists is how
//! a field comes to be assignable on one command and unknown on another.

use crate::code::Coded;
use crate::vendors::vendor_aggregate;
use crate::{
    Answer, Failure,
    answer::DraftFailure,
    canonical::{CurrentPackageCandidate, HealthyOrder, Touched, load_package_to_change},
    filesystem::{
        DestinationExpectation, FileReplacement, replace_validated_file_expecting,
        replace_validated_files,
    },
    metadata::{
        Currency, DateOrdered, OrderDocument, OrderItem, OrderStatus, StoredValues, Vendor,
        order_shape_field,
    },
    mutation::{
        IndexRefresh, MutationReport, Never, OperationKind, PlannedOperation, RejectedAssignment,
        RejectedAssignmentReason,
    },
    operations::{FailedOperation, FailedPath, refuse_linked_root},
    package::{ReusableValue, ReusableValues, canonicalize_reusable_value},
    ports::{FileSystem, FileSystemEntryKind, LocalDate},
    readme::{Location, located_in_package},
    vendors::vendor_by_name,
};
use crate::{Assignments, change::Assignment};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub(crate) const REUSABLE_FIELDS: [&str; 4] =
    ["orderer", "ordering_institution", "project", "order_group"];

/// Every field an Order mutation may assign, in the vocabulary a caller writes.
///
/// `vendor` is the Vendor *Name*, which is the ordinary Vendor selector. The record stores
/// the Vendor ID it resolves to; nothing here takes an identity, because a surface that
/// holds identities back offers no way to learn one.
const ASSIGNABLE_FIELDS: [&str; 9] = [
    "title",
    "date_ordered",
    "status",
    "vendor",
    "orderer",
    "ordering_institution",
    "currency",
    "project",
    "order_group",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssignmentIntent {
    Assign,
    Introduce,
}

/// The accepted facts of one canonical assignment set naming an Order with no record yet,
/// and the fields whose value [`Assignments::introduce`] asks to be added.
pub(crate) struct NewOrderFacts {
    entries: Vec<(String, String)>,
    introduced: Vec<String>,
}

impl NewOrderFacts {
    /// Adjudicates one set against the Package's own rules, or reports every refusal.
    pub fn of(assignments: &Assignments) -> Result<Self, Failure> {
        let accepted = accepted_assignments(assignments).map_err(Failure::InvalidAssignments)?;
        // The same adjudication an edit makes, in the same order, so the three constrained
        // facts are named by whichever capability is given one the record cannot hold. An
        // assembled candidate that failed to parse can only say that some fact was wrong.
        if let Some(unholdable) = unholdable_facts(&accepted).into_iter().next() {
            return Err(unholdable);
        }
        Ok(Self {
            introduced: accepted
                .iter()
                .filter(|(_, _, intent)| *intent == AssignmentIntent::Introduce)
                .map(|(field, _, _)| field.clone())
                .collect(),
            entries: accepted
                .into_iter()
                .map(|(field, value, _)| (field, value))
                .collect(),
        })
    }

    /// The fields whose value this set asks to be added where the Package lacks it.
    pub fn introduced(&self) -> Vec<String> {
        self.introduced.clone()
    }

    /// The values of every required fact, or every one this set did not name.
    ///
    /// Reported together rather than one at a time, for the reason a refused assignment
    /// set names every refused field: a caller correcting one omission and being told about
    /// the next is being made to discover the requirement one round trip at a time.
    pub fn required<const N: usize>(&self, fields: [&str; N]) -> Result<[&str; N], Failure> {
        let missing: Vec<String> = fields
            .iter()
            .filter(|field| self.value(field).is_none())
            .map(|field| (*field).to_owned())
            .collect();
        if !missing.is_empty() {
            return Err(Failure::MissingRequiredFacts { fields: missing });
        }
        Ok(fields.map(|field| {
            self.value(field)
                .expect("every required fact was found above")
        }))
    }

    /// One optional fact, where an empty value is an absent fact rather than an empty one.
    ///
    /// An Order that does not exist yet has nothing to clear, so `field=` on one of these
    /// capabilities is the same command line as omitting the field.
    pub fn optional(&self, field: &str) -> Option<String> {
        self.value(field)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    /// The Order Status this set assigned, where it assigned one.
    ///
    /// Infallible, because [`Self::of`] refuses a `status` the record cannot hold before
    /// this set exists — the same point at which it refuses a Date Ordered or an Order
    /// Currency it cannot hold.
    pub fn status(&self) -> Option<OrderStatus> {
        self.value("status").and_then(parse_status)
    }

    fn value(&self, field: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(assigned, _)| assigned == field)
            .map(|(_, value)| value.as_str())
    }
}

/// One Order fact whose value the record constrains rather than merely stores.
///
/// **A refusal has to name which fact was wrong, and the assembled record cannot.** A
/// candidate Order either parses and validates or it does not, so a refusal read off it
/// says only that some fact of the set was unacceptable — and an atomic set is exactly
/// where that is least useful. These three are the facts a caller can get wrong that way:
/// an Order Status outside the established five, a Date Ordered that is not a real
/// calendar day, and an Order Currency that is not a three-letter code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderFactKind {
    Status,
    DateOrdered,
    Currency,
}

impl OrderFactKind {
    /// Whether the Order record can hold this text as this fact.
    ///
    /// Asked of the record's own value types, and for a Date Ordered of the same calendar
    /// rule [`OrderDocument::validate`] applies — so a value refused here and a stored
    /// value a Package load rejects cannot come to disagree.
    fn holds(self, value: &str) -> bool {
        let held = |value: &str| serde_json::Value::String(value.to_owned());
        match self {
            Self::Status => parse_status(value).is_some(),
            Self::DateOrdered => serde_json::from_value::<DateOrdered>(held(value))
                .is_ok_and(|date| date.is_real_gregorian()),
            Self::Currency => serde_json::from_value::<Currency>(held(value)).is_ok(),
        }
    }
}

impl crate::code::Coded for OrderFactKind {}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrderFacts {
    pub(crate) document: OrderDocument,
}

impl PlannedOrderFacts {
    pub fn vendor_id(&self) -> Option<&str> {
        self.document.vendor_id.as_ref().map(|value| value.as_str())
    }

    pub fn orderer(&self) -> Option<&str> {
        self.document.orderer.as_deref()
    }

    pub fn ordering_institution(&self) -> Option<&str> {
        self.document.ordering_institution.as_deref()
    }

    pub fn project(&self) -> Option<&str> {
        self.document.project.as_deref()
    }

    pub fn order_group(&self) -> Option<&str> {
        self.document.order_group.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditPlan {
    root: PathBuf,
    directory: Location,
    order_path: PathBuf,
    pub(crate) facts: PlannedOrderFacts,
    prior_document: OrderDocument,
    pub(crate) vendor: Option<Vendor>,
    pub(crate) reusable_values: ReusableValues,
    /// The new values this edit adds to those the Package keeps.
    additions: StoredValues,
    pub(crate) operations: Vec<PlannedOperation>,
}

impl OrderEditPlan {
    /// Bind an editor's plan to the original document its draft was based on.
    /// Commit adjudicates this expectation against the current canonical document.
    pub(crate) fn require_original(mut self, original: Option<&OrderDocument>) -> Self {
        if let Some(original) = original {
            self.prior_document = original.clone();
        }
        self
    }

    /// The new values this edit adds to those the Package keeps.
    pub(crate) fn additions(&self) -> &StoredValues {
        &self.additions
    }

    /// Whether committing this plan would change the canonical Order record.
    pub fn changes_canonical_order(&self) -> bool {
        self.facts.document != self.prior_document
    }
}

/// Plans an editor's whole draft of one Order: its facts and its complete Item list.
pub(crate) fn plan_order_save<F: FileSystem>(
    order_directory: &Path,
    assignments: &Assignments,
    items: &[Assignments],
    today: &dyn Fn() -> LocalDate,
    filesystem: &F,
) -> Answer<OrderEditPlan> {
    Answer::of(|| {
        let (package, order) = resolve_order(order_directory, filesystem)?;
        edited_order(&package, &order, assignments, Some(items), today)
            .map_err(Failure::InvalidDraft)
    })
}

/// The plan that rewrites `order` with `assignments` over its stored facts and, where
/// given, `items` in place of its stored Items; or every rule that broke, the facts' in
/// adjudication order and then each row's.
pub(crate) fn edited_order(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    assignments: &Assignments,
    items: Option<&[Assignments]>,
    today: &dyn Fn() -> LocalDate,
) -> Result<OrderEditPlan, Vec<DraftFailure>> {
    let at_facts = |failure| DraftFailure { row: None, failure };
    let (assignments, rejected) = adjudicated_assignments(assignments);
    let mut faults = Vec::new();
    if !rejected.is_empty() {
        faults.push(Failure::InvalidAssignments(rejected));
    }
    let prior_document = order.document.clone();
    faults.extend(unholdable_facts(&assignments));

    // An assigned `status` the record cannot hold was refused just above, so one that does
    // not parse here keeps the Order's own, as a set that assigned none does.
    let status = assigned(&assignments, OrderFactKind::Status.code())
        .and_then(parse_status)
        .unwrap_or(prior_document.status);
    let date_ordered = match assigned(&assignments, "date_ordered") {
        Some(value) => value.trim().to_owned(),
        None => date_ordered_after_status_change(
            prior_document.status,
            status,
            prior_document.date_ordered.as_str(),
            today,
        ),
    };

    let Some(mut object) = serde_json::to_value(&prior_document)
        .ok()
        .and_then(|value| value.as_object().cloned())
    else {
        faults.push(Failure::InvalidFacts);
        return Err(faults.into_iter().map(at_facts).collect());
    };
    object.insert(
        "date_ordered".into(),
        serde_json::Value::String(date_ordered),
    );
    for (field, value, _) in &assignments {
        if field == "date_ordered" {
            continue;
        }
        // The one assigned field whose name is not the record's. `vendor` selects by
        // Vendor Name and the record stores the Vendor ID that Name resolves to, so the
        // assignment is translated here rather than copied — including the empty value,
        // which clears the stored Reference.
        if field == "vendor" {
            match value.trim() {
                "" => {
                    object.remove("vendor_id");
                }
                name => match vendor_by_name(name, &package.vendors) {
                    Some(vendor) => {
                        object.insert(
                            "vendor_id".into(),
                            serde_json::Value::String(vendor.vendor_id.as_str().to_owned()),
                        );
                    }
                    None => faults.push(Failure::UnknownVendor {
                        name: name.to_owned(),
                        scope: vendor_aggregate(package),
                    }),
                },
            }
            continue;
        }
        if value.trim().is_empty() {
            object.remove(field);
        } else {
            object.insert(
                field.clone(),
                serde_json::Value::String(value.trim().to_owned()),
            );
        }
    }

    let reusable_values =
        resolve_reusable_values(&assignments, &mut object, &prior_document, package)
            .map_err(|unconfirmed| faults.extend(unconfirmed))
            .ok();
    let mut faults = faults.into_iter().map(at_facts).collect::<Vec<_>>();

    if let Some(rows) = items {
        let items = DraftFailure::rows(rows, &mut faults, draft_item);
        match serde_json::to_value(&items) {
            Ok(items) => {
                object.insert("items".into(), items);
            }
            Err(_) => faults.push(at_facts(Failure::InvalidFacts)),
        }
    }

    match reusable_values {
        Some(reusable_values) if faults.is_empty() => {
            order_record_plan(package, order, prior_document, object, reusable_values)
                .map_err(|failure| vec![at_facts(failure)])
        }
        _ => Err(faults),
    }
}

/// Preview the draft date using the same transition behavior as an Order edit.
pub(crate) fn date_ordered_after_status_change(
    previous: OrderStatus,
    next: OrderStatus,
    current: &str,
    today: &dyn Fn() -> LocalDate,
) -> String {
    if next == OrderStatus::Ordered && previous != OrderStatus::Ordered {
        let today = today();
        format!("{:04}-{:02}-{:02}", today.year, today.month, today.day)
    } else {
        current.to_owned()
    }
}

/// Assembles the one complete validated Order-record replacement every Order
/// mutation commits, from a candidate object its own capability built. An unchanged
/// record is committed by performing nothing, so its plan must not announce managed
/// operations its commit will never reach.
fn order_record_plan(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    prior_document: OrderDocument,
    candidate: serde_json::Map<String, serde_json::Value>,
    reusable_values: ReusableValues,
) -> Result<OrderEditPlan, Failure> {
    let validated = serde_json::to_vec(&serde_json::Value::Object(candidate))
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .and_then(|document| document.validate().ok())
        .ok_or(Failure::InvalidFacts)?;
    let facts = PlannedOrderFacts {
        document: validated.order().clone(),
    };

    let order_path = order.paths.order.absolute_path.clone();
    let vendor = resolved_vendor(facts.vendor_id(), package);
    let additions = crate::values::additions(&reusable_values);
    let mut operations = Vec::new();
    if facts.document != prior_document {
        operations.push(PlannedOperation::new(
            OperationKind::ReplaceOrderRecord,
            order.paths.order.clone(),
        ));
        if !additions.is_empty() {
            operations.push(PlannedOperation::new(
                OperationKind::ReplaceVendorRecord,
                package.context.vendors.clone(),
            ));
        }
        operations.push(PlannedOperation::new(
            OperationKind::RefreshPackageIndex,
            package.context.index.clone(),
        ));
    }
    Ok(OrderEditPlan {
        root: package.context.root.absolute_path.clone(),
        directory: order.paths.directory.clone(),
        order_path,
        facts,
        prior_document,
        vendor,
        reusable_values,
        additions,
        operations,
    })
}

/// The Vendor an Order's Reference resolves to in one loaded Package, or `None`.
///
/// `None` for an Order that references no Vendor, and for one whose stored Reference
/// resolves to nothing — an Unresolved Vendor Reference, which a Package load reports as
/// its own diagnostic and which every read of that Order answers the same way. An edit
/// that *assigns* a Vendor this Package does not hold is refused before this, so the two
/// cases cannot be confused for one another.
fn resolved_vendor(vendor_id: Option<&str>, package: &CurrentPackageCandidate) -> Option<Vendor> {
    let vendor_id = vendor_id?;
    package
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id.as_str() == vendor_id)
        .cloned()
}

/// Refuses each value of a new Order that the Package lacks and that `introduced` does
/// not ask to be added, naming every one.
pub(crate) fn refuse_unconfirmed(
    values: &ReusableValues,
    introduced: &[String],
) -> Result<(), Failure> {
    let mut unconfirmed = [
        ("orderer", &values.orderer),
        ("ordering_institution", &values.ordering_institution),
        ("project", &values.project),
        ("order_group", &values.order_group),
    ]
    .into_iter()
    .filter_map(|(field, value)| match value {
        ReusableValue::New(proposed) if !introduced.iter().any(|named| named == field) => {
            Some(Failure::UnconfirmedReusableValue {
                field: field.to_owned(),
                proposed: proposed.clone(),
            })
        }
        _ => None,
    })
    .collect::<Vec<_>>();
    match unconfirmed.len() {
        0 => Ok(()),
        1 => Err(unconfirmed.remove(0)),
        _ => Err(Failure::InvalidDraft(
            unconfirmed
                .into_iter()
                .map(|failure| DraftFailure { row: None, failure })
                .collect(),
        )),
    }
}

/// Canonicalizes every Reusable Free-Entry Value the resulting Order carries and
/// refuses each genuinely new value the caller did not explicitly introduce.
///
/// An Order Group is judged among the groups of the resulting Project. An Order moved to
/// another Project loses a group this edit did not name unless that Project already has
/// it; a named group is reused or introduced there like any other value.
fn resolve_reusable_values(
    assignments: &[(String, String, AssignmentIntent)],
    object: &mut serde_json::Map<String, serde_json::Value>,
    prior: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Result<ReusableValues, Vec<Failure>> {
    let mut outcomes = Vec::new();
    let mut unconfirmed = Vec::new();
    for field in REUSABLE_FIELDS {
        let present = object.get(field).and_then(serde_json::Value::as_str);
        let project = object.get("project").and_then(serde_json::Value::as_str);
        let outcome = canonicalize_reusable(field, present, project, package);
        let outcome = if field == "order_group" {
            let moved = !same_value(prior.project.as_deref(), project);
            let named = assignments
                .iter()
                .any(|(assigned, _, _)| assigned == "order_group");
            if moved && !named && matches!(outcome, ReusableValue::New(_)) {
                ReusableValue::Absent
            } else {
                outcome
            }
        } else {
            outcome
        };
        if let ReusableValue::New(proposed) = &outcome
            && !assignments.iter().any(|(assigned, _, intent)| {
                assigned == field && *intent == AssignmentIntent::Introduce
            })
        {
            unconfirmed.push(Failure::UnconfirmedReusableValue {
                field: field.to_owned(),
                proposed: proposed.clone(),
            });
        }
        match &outcome {
            ReusableValue::Absent => {
                object.remove(field);
            }
            ReusableValue::Reused(value) | ReusableValue::New(value) => {
                object.insert(field.into(), serde_json::Value::String(value.clone()));
            }
        }
        outcomes.push(outcome);
    }
    if !unconfirmed.is_empty() {
        return Err(unconfirmed);
    }
    let mut outcomes = outcomes.into_iter();

    Ok(ReusableValues {
        orderer: outcomes.next().expect("orderer is a reusable field"),
        ordering_institution: outcomes
            .next()
            .expect("ordering_institution is a reusable field"),
        project: outcomes.next().expect("project is a reusable field"),
        order_group: outcomes.next().expect("order_group is a reusable field"),
    })
}

fn canonicalize_reusable(
    field: &str,
    proposed: Option<&str>,
    project: Option<&str>,
    package: &CurrentPackageCandidate,
) -> ReusableValue {
    let groups;
    let projections = match field {
        "orderer" => &package.projections.orderers,
        "ordering_institution" => &package.projections.ordering_institutions,
        "project" => &package.projections.projects,
        _ => {
            groups = package.order_groups_of(project);
            &groups
        }
    };
    let existing: Vec<&str> = projections.iter().map(|item| item.value.as_str()).collect();
    canonicalize_reusable_value(proposed, &existing)
}

/// Whether two optional free-entry texts name the same value, as reuse compares them.
fn same_value(left: Option<&str>, right: Option<&str>) -> bool {
    fn present(text: Option<&str>) -> Option<&str> {
        text.map(str::trim).filter(|text| !text.is_empty())
    }
    match (present(left), present(right)) {
        (None, None) => true,
        (Some(left), Some(right)) => focaccia::unicode_full_case_eq(left, right),
        _ => false,
    }
}

fn assigned<'a>(
    assignments: &'a [(String, String, AssignmentIntent)],
    field: &str,
) -> Option<&'a str> {
    assignments
        .iter()
        .find(|(assigned, _, _)| assigned == field)
        .map(|(_, value, _)| value.as_str())
}

/// The finite number an Item's Quantity or Unit Price text holds.
pub(crate) fn item_number(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

pub(crate) fn parse_status(value: &str) -> Option<OrderStatus> {
    serde_json::from_value(serde_json::Value::String(value.trim().to_owned())).ok()
}

/// Each assigned fact of one set whose value the Order record cannot hold.
///
/// **The three are adjudicated together, before anything is assembled**, so a set carrying
/// an unacceptable value is refused naming the fact rather than by the assembled candidate
/// failing to parse — see [`OrderFactKind`].
///
/// An empty value is not adjudicated: it clears an optional fact, and a blank required one
/// is already a refused assignment.
fn unholdable_facts(assignments: &[(String, String, AssignmentIntent)]) -> Vec<Failure> {
    [
        OrderFactKind::Status,
        OrderFactKind::DateOrdered,
        OrderFactKind::Currency,
    ]
    .into_iter()
    .filter_map(|fact| {
        let value = assigned(assignments, fact.code())
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        (!fact.holds(value)).then(|| Failure::InvalidFactValue {
            fact,
            value: value.to_owned(),
        })
    })
    .collect()
}

/// The assignments of one set that broke no rule, or every one that did.
///
/// **One list of assignable fields and one set of refusal reasons**, shared by an edit and
/// by the two capabilities that give an Order its first record — see the module
/// documentation. A caller reports the refusals in its own failure type; the adjudication
/// is not any of theirs.
fn accepted_assignments(
    assignments: &Assignments,
) -> Result<Vec<(String, String, AssignmentIntent)>, Vec<RejectedAssignment>> {
    let (accepted, rejected) = adjudicated_assignments(assignments);
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(rejected)
    }
}

/// The assignments of one set that broke no rule, and every one that did.
fn adjudicated_assignments(
    assignments: &Assignments,
) -> (
    Vec<(String, String, AssignmentIntent)>,
    Vec<RejectedAssignment>,
) {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String, AssignmentIntent)>::new();
    for Assignment {
        field,
        value,
        confirmed,
    } in &assignments.entries
    {
        let intent = if *confirmed {
            AssignmentIntent::Introduce
        } else {
            AssignmentIntent::Assign
        };
        let reason = match field.as_str() {
            "order_id" => Some(RejectedAssignmentReason::ImmutableIdentity),
            "schema_version" | "package_root" => {
                Some(RejectedAssignmentReason::ManagedTechnicalField)
            }
            "items" => Some(RejectedAssignmentReason::OrderedCollection),
            field if !ASSIGNABLE_FIELDS.contains(&field) => {
                Some(RejectedAssignmentReason::UnknownField)
            }
            _ if accepted.iter().any(|(accepted, _, _)| accepted == field) => {
                Some(RejectedAssignmentReason::Duplicate)
            }
            field if intent == AssignmentIntent::Introduce && !REUSABLE_FIELDS.contains(&field) => {
                Some(RejectedAssignmentReason::NotAReusableValue)
            }
            "title" | "date_ordered" | "status" if value.trim().is_empty() => {
                Some(RejectedAssignmentReason::BlankRequiredValue)
            }
            _ => None,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone(), intent)),
        }
    }
    (accepted, rejected)
}

/// Resolves the Order Directory's own Package through its normalized locator, then
/// requires the directory to be one healthy Order of that Package.
pub(crate) fn resolve_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<(CurrentPackageCandidate, HealthyOrder), Failure> {
    refuse_linked_root(order_directory, filesystem)?;
    let order_path = order_directory.join(".spielorders").join("order.json");
    // Nothing has resolved the directory yet, so it is named exactly as it was given.
    let damaged_record = |field: Option<&str>| Failure::DamagedRecord {
        directory: FailedPath::Unlocated(order_directory.to_path_buf()),
        field: field.map(str::to_owned),
    };
    // A record reached through a linked metadata directory is not this Order's own.
    if let Ok(Some(kind)) = filesystem.symlink_metadata(&order_directory.join(".spielorders"))
        && kind != FileSystemEntryKind::Directory
    {
        return Err(damaged_record(None));
    }
    // A missing, unreadable, wrongly typed, or unparseable record is a damaged record;
    // an edit never repairs or adopts it. Only a failure to observe the path at all
    // remains an operation failure, matching how the Package loader classifies a
    // managed artifact.
    if filesystem.symlink_metadata(&order_path).map_err(|error| {
        Failure::from(FailedOperation::filesystem(
            FailedPath::of(&order_path),
            &error,
            filesystem,
        ))
    })? != Some(FileSystemEntryKind::File)
    {
        return Err(damaged_record(None));
    }
    let bytes = filesystem
        .read(&order_path)
        .map_err(|_| damaged_record(None))?;
    let document = OrderDocument::from_json_bytes(&bytes)
        .map_err(|_| damaged_record(order_shape_field(&bytes).as_deref()))?;
    let directory = filesystem.canonicalize(order_directory).map_err(|error| {
        Failure::from(FailedOperation::filesystem(
            FailedPath::of(order_directory),
            &error,
            filesystem,
        ))
    })?;
    // A record with no locator this build can follow is a damaged record, exactly as one
    // that would not parse is: the locator is this resolution's only route to a Package,
    // and nothing here searches for one.
    let parent_count = document
        .package_root
        .spelled()
        .ok_or_else(|| damaged_record(Some("package_root")))?
        .as_str()
        .split('/')
        .count();
    let Some(root) = directory
        .ancestors()
        .nth(parent_count)
        .map(Path::to_path_buf)
    else {
        return Err(Failure::OrderNotFound {
            directory: FailedPath::of(&directory),
            root: None,
        });
    };

    // The Order this resolves is the record a change will rewrite, so it is read from
    // disk with the Vendor record it may reference, never taken from the index.
    let changed = Touched::orders([directory.clone()]).with_vendors(true);
    let package = load_package_to_change(&root, &changed, filesystem)?;
    if let Some(order) = package
        .healthy_orders
        .iter()
        .find(|candidate| candidate.paths.directory.absolute_path == directory)
        .cloned()
    {
        return Ok((package, order));
    }
    if let Some(damaged) = package
        .damaged_orders
        .iter()
        .find(|candidate| candidate.paths.directory.absolute_path == directory)
    {
        return Err(Failure::DamagedOrder {
            context: Box::new(package.context.clone()),
            diagnostics: package.order_diagnostics(&damaged.paths),
            paths: Box::new(damaged.paths.clone()),
            order_id: damaged.order_id.clone(),
        });
    }
    Err(Failure::OrderNotFound {
        directory: FailedPath::inside(&root, directory),
        root: Some(root),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditResult {
    /// The edited Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderEditReport = MutationReport<OrderEditResult>;

pub(crate) fn commit_order_edit<F: FileSystem>(
    plan: OrderEditPlan,
    filesystem: &F,
) -> Answer<OrderEditReport> {
    Answer::of(|| commit_edit(plan, filesystem))
}

fn commit_edit<F: FileSystem>(
    plan: OrderEditPlan,
    filesystem: &F,
) -> Result<OrderEditReport, Failure> {
    let changed = Touched::orders([plan.directory.absolute_path.clone()]).with_vendors(true);
    let current = load_package_to_change(&plan.root, &changed, filesystem)?;
    let Some(order) = current
        .healthy_orders
        .iter()
        .find(|candidate| candidate.paths.directory.absolute_path == plan.directory.absolute_path)
    else {
        let path = plan.directory.absolute_path.clone();
        return Err(preconditions_changed(&plan, path));
    };
    if order.document != plan.prior_document {
        let path = plan.order_path.clone();
        return Err(preconditions_changed(&plan, path));
    }
    if let Some(vendor_id) = plan.facts.vendor_id()
        && !current
            .vendors
            .iter()
            .any(|vendor| vendor.vendor_id.as_str() == vendor_id)
    {
        let path = current.context.vendors.absolute_path.clone();
        return Err(preconditions_changed(&plan, path));
    }
    if !reusable_values_still_current(&plan, &current) {
        let path = plan.order_path.clone();
        return Err(preconditions_changed(&plan, path));
    }

    if !plan.changes_canonical_order() {
        let result = OrderEditResult {
            directory: plan.directory.clone(),
        };
        return Ok(MutationReport::Unchanged { result });
    }
    let Ok(candidate) = plan.facts.document.to_json_bytes() else {
        let path = plan.order_path.clone();
        return Err(preconditions_changed(&plan, path));
    };
    // The Order revalidated above is the one this edit is authorized to replace, so a
    // writer landing between that revalidation and the rename is refused rather than
    // overwritten. Meaning is what is compared, not bytes — which rule to carry where,
    // and what comparing meaning gives up, are in `docs/architecture/durable-writes.md`.
    let replacement = {
        // Projected the way the load projects it, because that projection — not the
        // raw parse — is what the revalidation above compared. A record whose stored
        // facts carry untrimmed text projects to the same Order, and refusing it would
        // mean refusing to edit a file a person reformatted by hand.
        let still_holds_the_revalidated_order = |current: &[u8]| {
            OrderDocument::from_json_bytes(current)
                .ok()
                .and_then(|document| document.validate().ok())
                .is_some_and(|projected| projected.order() == &plan.prior_document)
        };
        if let Some(values) =
            crate::values::ValuesWrite::prepare(&current, &plan.additions, filesystem)?
        {
            let observed = filesystem
                .read(&plan.order_path)
                .map_err(|_| preconditions_changed(&plan, plan.order_path.clone()))?;
            if !still_holds_the_revalidated_order(&observed) {
                return Err(preconditions_changed(&plan, plan.order_path.clone()));
            }
            let replacements = [
                FileReplacement::observed(&plan.order_path, &candidate, &observed),
                values.replacement(),
            ];
            replace_validated_files(&replacements, filesystem, |_, _| Ok::<(), Never>(())).map_err(
                |error| {
                    Failure::of_replacements(
                        &plan.root,
                        &[plan.order_path.as_path(), values.path()],
                        error,
                    )
                },
            )
        } else {
            replace_validated_file_expecting(
                &plan.order_path,
                &candidate,
                DestinationExpectation::Equivalent(&still_holds_the_revalidated_order),
                filesystem,
                |_| Ok::<(), Never>(()),
            )
            .map_err(|error| Failure::of_replacement(&plan.root, &plan.order_path, error))
        }
    };
    replacement?;

    let touched = Touched::orders([plan.directory.absolute_path.clone()])
        .with_vendors(crate::mutation::replaces_vendors(&plan.operations));
    let refresh = IndexRefresh::after_commit(&plan.root, &touched, filesystem);
    Ok(MutationReport::committed(
        OrderEditResult {
            directory: plan.directory.clone(),
        },
        refresh.warning().into_iter().collect(),
    ))
}

/// Re-canonicalizes every planned Reusable Free-Entry Value against current Package
/// state so a concurrently introduced canonical spelling cannot commit stale text.
fn reusable_values_still_current(plan: &OrderEditPlan, current: &CurrentPackageCandidate) -> bool {
    REUSABLE_FIELDS.iter().all(|field| {
        let text = match *field {
            "orderer" => plan.facts.orderer(),
            "ordering_institution" => plan.facts.ordering_institution(),
            "project" => plan.facts.project(),
            _ => plan.facts.order_group(),
        };
        let planned = match *field {
            "orderer" => &plan.reusable_values.orderer,
            "ordering_institution" => &plan.reusable_values.ordering_institution,
            "project" => &plan.reusable_values.project,
            _ => &plan.reusable_values.order_group,
        };
        canonicalize_reusable(field, text, plan.facts.project(), current) == *planned
    })
}

/// One path this plan named inside its own Package, as that Package locates it.
fn preconditions_changed(plan: &OrderEditPlan, path: PathBuf) -> Failure {
    Failure::PreconditionsChanged {
        location: located_in_package(&plan.root, path),
    }
}

const ITEM_FIELDS: [&str; 5] = [
    "description",
    "quantity",
    "unit_price",
    "catalog_number",
    "notes",
];

const REQUIRED_ITEM_FIELDS: [&str; 3] = ["description", "quantity", "unit_price"];

/// The Order Item one canonical assignment set names, or every reason it names none.
///
/// **One list of Item fields and one set of refusal reasons**, shared by the four Item
/// mutations and by adoption — see the module documentation. So an unknown field, a
/// repeated one, a blank Description, and a Quantity the record could not hold as a finite
/// number are refused identically whichever capability was handed the set.
pub(crate) fn new_order_item(assignments: &Assignments) -> Result<OrderItem, Failure> {
    let accepted = accepted_item_assignments(assignments).map_err(Failure::InvalidAssignments)?;
    Ok(planned_item(None, &accepted)?)
}

/// One row of a whole draft as its Item, or every fault the row has: a required fact
/// the row does not assign is reported beside the values it rejected, not hidden by them.
pub(crate) fn draft_item(assignments: &Assignments) -> Result<OrderItem, Vec<Failure>> {
    match accepted_item_assignments(assignments) {
        Ok(accepted) => planned_item(None, &accepted).map_err(|failure| vec![failure.into()]),
        Err(rejected) => {
            let missing: Vec<String> = REQUIRED_ITEM_FIELDS
                .iter()
                .filter(|field| {
                    !assignments
                        .entries
                        .iter()
                        .any(|entry| entry.field == **field)
                })
                .map(|field| (*field).to_owned())
                .collect();
            let mut faults = vec![Failure::InvalidAssignments(rejected)];
            if !missing.is_empty() {
                faults.push(Failure::MissingRequiredItemFacts { fields: missing });
            }
            Err(faults)
        }
    }
}

/// One accepted Item assignment: a candidate value, or the explicit clearing of an
/// optional Item fact.
enum ItemValue {
    Clear,
    Set(serde_json::Value),
}

/// The assignments of one Item set that broke no rule, or every one that did.
///
/// Reported as the refusals themselves, because the adjudication is shared by the four
/// Item mutations and adoption.
fn accepted_item_assignments(
    assignments: &Assignments,
) -> Result<Vec<(String, ItemValue)>, Vec<RejectedAssignment>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, ItemValue)>::new();
    for Assignment {
        field,
        value,
        confirmed,
    } in &assignments.entries
    {
        let duplicate = accepted.iter().any(|(accepted, _)| accepted == field);
        match item_assignment(field, value, duplicate, *confirmed) {
            Ok(value) => accepted.push((field.clone(), value)),
            Err(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
        }
    }
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(rejected)
    }
}

/// Accepts one Item assignment as the exact candidate value it will store, so an
/// accepted numeric value is one the durable record can already hold.
fn item_assignment(
    field: &str,
    value: &str,
    duplicate: bool,
    confirmed: bool,
) -> Result<ItemValue, RejectedAssignmentReason> {
    if !ITEM_FIELDS.contains(&field) {
        return Err(RejectedAssignmentReason::UnknownField);
    }
    if duplicate {
        return Err(RejectedAssignmentReason::Duplicate);
    }
    // No Item fact is a Reusable Free-Entry Value, so none takes a confirmation.
    if confirmed {
        return Err(RejectedAssignmentReason::NotAReusableValue);
    }
    let value = value.trim();
    if value.is_empty() {
        return if REQUIRED_ITEM_FIELDS.contains(&field) {
            Err(RejectedAssignmentReason::BlankRequiredValue)
        } else {
            Ok(ItemValue::Clear)
        };
    }
    match field {
        // Quantity and Unit Price accept any contract-valid zero or negative value,
        // and refuse text the durable record could not hold as a finite number.
        "quantity" | "unit_price" => item_number(value)
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .map(ItemValue::Set)
            .ok_or(RejectedAssignmentReason::NotAFiniteNumber),
        _ => Ok(ItemValue::Set(serde_json::Value::String(value.to_owned()))),
    }
}

/// Why one assembled Order Item is not one the record can hold.
///
/// The two ways assembling an Item can fail, named where the assembly is so that each
/// caller reports them in its own vocabulary.
enum PlannedItemFailure {
    MissingRequiredFacts { fields: Vec<String> },
    Invalid,
}

fn planned_item(
    existing: Option<&OrderItem>,
    assignments: &[(String, ItemValue)],
) -> Result<OrderItem, PlannedItemFailure> {
    let mut object = match existing {
        Some(item) => serde_json::to_value(item)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or(PlannedItemFailure::Invalid)?,
        None => serde_json::Map::new(),
    };
    for (field, value) in assignments {
        match value {
            ItemValue::Clear => {
                object.remove(field);
            }
            ItemValue::Set(value) => {
                object.insert(field.clone(), value.clone());
            }
        }
    }
    let missing = REQUIRED_ITEM_FIELDS
        .iter()
        .filter(|field| !object.contains_key(**field))
        .map(|field| (*field).to_owned())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PlannedItemFailure::MissingRequiredFacts { fields: missing });
    }
    serde_json::from_value::<OrderItem>(serde_json::Value::Object(object))
        .map_err(|_| PlannedItemFailure::Invalid)
}

impl From<PlannedItemFailure> for Failure {
    fn from(failure: PlannedItemFailure) -> Self {
        match failure {
            PlannedItemFailure::MissingRequiredFacts { fields } => {
                Self::MissingRequiredItemFacts { fields }
            }
            PlannedItemFailure::Invalid => Self::InvalidFacts,
        }
    }
}
