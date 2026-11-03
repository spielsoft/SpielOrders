//! The canonical assignment language for an Order's own facts, and planning and
//! persistence for the Order record's own mutations: one atomic scalar-fact assignment
//! set, and the ordered embedded Order Item collection.
//!
//! **The assignment vocabulary lives here for every capability that takes one.** Which
//! fields an Order has, which of them may be assigned, and which rule each refusal names
//! are Package rules, so an edit and the two capabilities that give an Order its first
//! record — creation and adoption — adjudicate one set against one list. Two lists is how
//! a field comes to be assignable on one command and unknown on another.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, HealthyOrder, PackageIndexRecord,
        PackageLoad, ReusableValueProjection, inspect_canonical_package, open_package,
        project_committed_order,
    },
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, replace_validated_file_expecting,
    },
    metadata::{Currency, DateOrdered, OrderDocument, OrderItem, OrderStatus, Vendor},
    mutation::{MutationReport, RejectedAssignment, RejectedAssignmentReason},
    operations::{FailedPath, OperationFailure},
    package::{ReusableValue, canonicalize_reusable_value},
    ports::{FileSystem, FileSystemEntryKind, LocalCalendar},
    readme::{Location, located_in_package},
    vendors::vendor_by_name,
};
use std::path::{Path, PathBuf};

const REUSABLE_FIELDS: [&str; 4] = ["orderer", "ordering_institution", "project", "order_group"];

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

/// One order-independent atomic assignment set for an Order's mutable scalar facts.
/// A genuinely new Reusable Free-Entry Value requires `introduce` as its explicit
/// confirmation; `assign` alone reuses an existing canonical spelling or clears.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrderAssignments {
    entries: Vec<(String, String, AssignmentIntent)>,
}

impl OrderAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries
            .push((field.into(), value.into(), AssignmentIntent::Assign));
        self
    }

    pub fn introduce(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries
            .push((field.into(), value.into(), AssignmentIntent::Introduce));
        self
    }

    /// One assignment whose caller has already confirmed a genuinely new value, where
    /// the field is a Reusable Free-Entry Value at all.
    ///
    /// **Which fields those are stays here**, which is the whole reason this exists
    /// beside [`Self::introduce`]: a caller for whom assigning *is* the confirmation
    /// — a CLI command line is the decided one — would otherwise have to keep its own
    /// copy of the four categories to choose between the two methods, and would have
    /// `introduce` refused on every other field it assigned.
    pub fn confirm(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        let field = field.into();
        let intent = if REUSABLE_FIELDS.contains(&field.as_str()) {
            AssignmentIntent::Introduce
        } else {
            AssignmentIntent::Assign
        };
        self.entries.push((field, value.into(), intent));
        self
    }
}

/// Why one canonical assignment set does not name the facts of a new Order.
///
/// **For the two capabilities that give an Order its first record**: creating one, and
/// adopting an existing directory as one. An edit refuses on the same assignment rules and
/// reports them through [`OrderEditPlanError`] instead, because an edit has a record to
/// refuse *about* — these two have a Package and a set of proposed facts, and nothing else.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NewOrderAssignmentFailure {
    /// At least one assignment broke a rule, so the set changed nothing. Every refused
    /// assignment of the set is reported, because a set is adjudicated as a whole.
    InvalidAssignments(Vec<RejectedAssignment>),
    /// The set named none of these required facts. Their spellings are the record's own
    /// field names, in the order the capability requires them.
    MissingRequiredFacts { fields: Vec<String> },
    /// A fact the record constrains was assigned a value it cannot hold. Its own reason
    /// rather than a refused assignment: the field is assignable and the value is not one
    /// the record accepts, which is the one thing a caller has to correct.
    ///
    /// **The same reason an edit reports through
    /// [`OrderEditPlanError::InvalidFactValue`]**, over the same [`OrderFactKind`] and
    /// decided by the same predicate — so a Date Ordered that is not a real calendar day is
    /// refused identically whether it is assigned to an Order that exists or to one that
    /// does not.
    InvalidFactValue { fact: OrderFactKind, value: String },
}

/// The accepted facts of one canonical assignment set naming an Order with no record yet.
///
/// **Intent is not read here.** [`OrderAssignments::introduce`] is an *edit's* explicit
/// confirmation of a genuinely new Reusable Free-Entry Value; creating or adopting an Order
/// reports reuse versus introduction for each of the four categories instead of requiring
/// a confirmation, so an assignment is itself the confirmation and the two intents name the
/// same thing.
pub struct NewOrderFacts {
    entries: Vec<(String, String)>,
}

impl NewOrderFacts {
    /// Adjudicates one set against the Package's own rules, or reports every refusal.
    pub fn of(assignments: &OrderAssignments) -> Result<Self, NewOrderAssignmentFailure> {
        let accepted = accepted_assignments(assignments)
            .map_err(NewOrderAssignmentFailure::InvalidAssignments)?;
        // The same adjudication an edit makes, in the same order, so the three constrained
        // facts are named by whichever capability is given one the record cannot hold. An
        // assembled candidate that failed to parse can only say that some fact was wrong.
        if let Some((fact, value)) = unholdable_fact(&accepted) {
            return Err(NewOrderAssignmentFailure::InvalidFactValue { fact, value });
        }
        Ok(Self {
            entries: accepted
                .into_iter()
                .map(|(field, value, _)| (field, value))
                .collect(),
        })
    }

    /// The values of every required fact, or every one this set did not name.
    ///
    /// Reported together rather than one at a time, for the reason a refused assignment
    /// set names every refused field: a caller correcting one omission and being told about
    /// the next is being made to discover the requirement one round trip at a time.
    pub fn required<const N: usize>(
        &self,
        fields: [&str; N],
    ) -> Result<[&str; N], NewOrderAssignmentFailure> {
        let missing: Vec<String> = fields
            .iter()
            .filter(|field| self.value(field).is_none())
            .map(|field| (*field).to_owned())
            .collect();
        if !missing.is_empty() {
            return Err(NewOrderAssignmentFailure::MissingRequiredFacts { fields: missing });
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderFactKind {
    Status,
    DateOrdered,
    Currency,
}

impl OrderFactKind {
    /// The record's own field name, which is also the token a caller assigns.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::DateOrdered => "date_ordered",
            Self::Currency => "currency",
        }
    }

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

/// Where the committed Date Ordered came from, making the `ordered` entry rule
/// observable without inspecting the stored value alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateOrderedSource {
    Retained,
    Assigned,
    AppliedOnEnteringOrdered,
}

impl DateOrderedSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retained => "retained",
            Self::Assigned => "assigned",
            Self::AppliedOnEnteringOrdered => "applied_on_entering_ordered",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrderFacts {
    document: OrderDocument,
    item_totals: Vec<f64>,
    order_total: f64,
}

impl PlannedOrderFacts {
    pub fn order_id(&self) -> &str {
        self.document.order_id.as_str()
    }

    pub fn title(&self) -> &str {
        self.document.title.as_str()
    }

    pub fn date_ordered(&self) -> &str {
        self.document.date_ordered.as_str()
    }

    pub fn status(&self) -> OrderStatus {
        self.document.status
    }

    pub fn vendor_id(&self) -> Option<&str> {
        self.document.vendor_id.as_ref().map(|value| value.as_str())
    }

    pub fn orderer(&self) -> Option<&str> {
        self.document.orderer.as_deref()
    }

    pub fn ordering_institution(&self) -> Option<&str> {
        self.document.ordering_institution.as_deref()
    }

    pub fn currency(&self) -> Option<&str> {
        self.document.currency.as_ref().map(|value| value.as_str())
    }

    pub fn project(&self) -> Option<&str> {
        self.document.project.as_deref()
    }

    pub fn order_group(&self) -> Option<&str> {
        self.document.order_group.as_deref()
    }

    /// The complete embedded Item collection in its exact user-controlled order.
    pub fn items(&self) -> &[OrderItem] {
        &self.document.items
    }

    pub fn item_totals(&self) -> &[f64] {
        &self.item_totals
    }

    pub fn order_total(&self) -> f64 {
        self.order_total
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderEditReusableValues {
    orderer: ReusableValue,
    ordering_institution: ReusableValue,
    project: ReusableValue,
    order_group: ReusableValue,
}

impl OrderEditReusableValues {
    pub fn orderer(&self) -> &ReusableValue {
        &self.orderer
    }

    pub fn ordering_institution(&self) -> &ReusableValue {
        &self.ordering_institution
    }

    pub fn project(&self) -> &ReusableValue {
        &self.project
    }

    pub fn order_group(&self) -> &ReusableValue {
        &self.order_group
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderEditOperationKind {
    ReplaceOrderRecord,
    RefreshPackageIndex,
}

impl OrderEditOperationKind {
    /// The stable spelling of one managed operation, in the vocabulary every capability
    /// that replaces a managed record reports — see
    /// [`crate::vendors::VendorMutationOperationKind::as_str`], whose two operations are
    /// these two over the other managed record.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceOrderRecord => "replace_order_record",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderEditOperation {
    kind: OrderEditOperationKind,
    location: Location,
}

impl OrderEditOperation {
    pub fn kind(&self) -> OrderEditOperationKind {
        self.kind
    }

    /// Where this operation writes, as the load located it — so a caller reporting it
    /// carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditPlan {
    root: PathBuf,
    directory: PathBuf,
    order_path: PathBuf,
    facts: PlannedOrderFacts,
    projected: HealthyOrder,
    prior_document: OrderDocument,
    vendor: Option<Vendor>,
    reusable_values: OrderEditReusableValues,
    date_ordered_source: DateOrderedSource,
    prior_index: PackageIndexRecord,
    operations: Vec<OrderEditOperation>,
}

impl OrderEditPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The Order Directory, whose name an edit never changes.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn facts(&self) -> &PlannedOrderFacts {
        &self.facts
    }

    /// The complete Order this plan would leave, projected exactly as a load projects a
    /// committed one.
    ///
    /// **The same projection every read of an Order answers with**, for the reason
    /// [`OrderCreationPlan::projected_order`] carries one: a run asked to plan only owes
    /// its caller the Order it would leave, and assembling that from the planned facts a
    /// second time is how a dry run comes to differ from the run it describes. The Order
    /// README is the one the load read, because no Order mutation writes it.
    ///
    /// [`OrderCreationPlan::projected_order`]: crate::creation::OrderCreationPlan::projected_order
    pub fn projected_order(&self) -> &HealthyOrder {
        &self.projected
    }

    /// The derived Package Index as it stood before this mutation.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        &self.prior_index
    }

    /// The Vendor the Order this plan would leave references, or `None` where it
    /// references none.
    ///
    /// Resolved here for the reason [`OrderCreationPlan::vendor`] resolves one: a mutation
    /// answers with the complete Order projection, that projection resolves the Vendor, and
    /// a run asked to plan only has no committed record to resolve it from. An edit can
    /// change the Vendor Reference, so it is resolved against what the plan would store
    /// rather than against what the Order references now.
    ///
    /// [`OrderCreationPlan::vendor`]: crate::creation::OrderCreationPlan::vendor
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    pub fn reusable_values(&self) -> &OrderEditReusableValues {
        &self.reusable_values
    }

    pub fn date_ordered_source(&self) -> DateOrderedSource {
        self.date_ordered_source
    }

    pub fn operations(&self) -> &[OrderEditOperation] {
        &self.operations
    }

    /// Whether committing this plan would change the canonical Order record.
    pub fn changes_canonical_order(&self) -> bool {
        self.facts.document != self.prior_document
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderEditPlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound {
        path: PathBuf,
    },
    DamagedOrder {
        path: PathBuf,
    },
    InvalidAssignments(Vec<RejectedAssignment>),
    UnconfirmedReusableValue {
        field: String,
        proposed: String,
    },
    /// No Vendor of this Package carries the Name the mutation assigned. An Order
    /// mutation chooses a Vendor and never creates one.
    UnknownVendor {
        name: String,
    },
    /// A one-based Item position outside the pre-mutation collection the operation
    /// evaluates it against. An append alone may name one position beyond the last.
    InvalidItemPosition {
        role: ItemPositionRole,
        position: usize,
        minimum: usize,
        maximum: usize,
    },
    MissingRequiredItemFacts {
        fields: Vec<String>,
    },
    /// One assigned fact whose value the Order record cannot hold, named. See
    /// [`OrderFactKind`], which is where the three and the reason for naming them are.
    InvalidFactValue {
        fact: OrderFactKind,
        value: String,
    },
    InvalidFacts,
    /// **A [`FailedPath`] rather than a `PathBuf`**, because the paths that reach here do
    /// not agree about whether a Package surrounds them: an operation inside a Root this
    /// capability established carries that Root's spelling, and one performed while
    /// resolving the Order Directory up to its Package happens before there is a Root to
    /// spell it against. The type is what tells them apart.
    FileSystem {
        path: FailedPath,
        error: E,
    },
}

pub fn plan_order_edit<F: FileSystem>(
    order_directory: &Path,
    assignments: &OrderAssignments,
    calendar: &impl LocalCalendar,
    filesystem: &F,
) -> Result<OrderEditPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let assignments =
        accepted_assignments(assignments).map_err(OrderEditPlanError::InvalidAssignments)?;
    let prior_document = order.document().clone();

    if let Some((fact, value)) = unholdable_fact(&assignments) {
        return Err(OrderEditPlanError::InvalidFactValue { fact, value });
    }
    // Every assigned value the record cannot hold was refused just above, so an assigned
    // `status` parses here — and a set that assigned none keeps the Order's own.
    let status = assigned(&assignments, OrderFactKind::Status.as_str())
        .and_then(parse_status)
        .unwrap_or(prior_document.status);
    let (date_ordered, date_ordered_source) = match assigned(&assignments, "date_ordered") {
        Some(value) => (value.trim().to_owned(), DateOrderedSource::Assigned),
        None if status == OrderStatus::Ordered && prior_document.status != OrderStatus::Ordered => {
            let today = calendar.today();
            (
                format!("{:04}-{:02}-{:02}", today.year, today.month, today.day),
                DateOrderedSource::AppliedOnEnteringOrdered,
            )
        }
        None => (
            prior_document.date_ordered.as_str().to_owned(),
            DateOrderedSource::Retained,
        ),
    };

    let mut object = serde_json::to_value(&prior_document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
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
                name => {
                    let vendor = vendor_by_name(name, package.vendors()).ok_or_else(|| {
                        OrderEditPlanError::UnknownVendor {
                            name: name.to_owned(),
                        }
                    })?;
                    object.insert(
                        "vendor_id".into(),
                        serde_json::Value::String(vendor.vendor_id.as_str().to_owned()),
                    );
                }
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

    let reusable_values = resolve_reusable_values(&assignments, &mut object, &package)?;

    order_record_plan(
        &package,
        &order,
        prior_document,
        object,
        reusable_values,
        date_ordered_source,
    )
}

/// Assembles the one complete validated Order-record replacement every Order
/// mutation commits, from a candidate object its own capability built. An unchanged
/// record is committed by performing nothing, so its plan must not announce managed
/// operations its commit will never reach.
fn order_record_plan<E>(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    prior_document: OrderDocument,
    candidate: serde_json::Map<String, serde_json::Value>,
    reusable_values: OrderEditReusableValues,
    date_ordered_source: DateOrderedSource,
) -> Result<OrderEditPlan, OrderEditPlanError<E>> {
    let validated = serde_json::to_vec(&serde_json::Value::Object(candidate))
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .and_then(|document| document.validate().ok())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
    let facts = PlannedOrderFacts {
        document: validated.order().clone(),
        item_totals: validated.item_totals().to_vec(),
        order_total: validated.order_total(),
    };

    let order_path = order.paths().order().absolute_path().to_path_buf();
    let vendor = resolved_vendor(facts.vendor_id(), package);
    let operations = if facts.document == prior_document {
        Vec::new()
    } else {
        vec![
            OrderEditOperation {
                kind: OrderEditOperationKind::ReplaceOrderRecord,
                location: order.paths().order().clone(),
            },
            OrderEditOperation {
                kind: OrderEditOperationKind::RefreshPackageIndex,
                location: package.context().index().clone(),
            },
        ]
    };
    let projected = project_committed_order(
        package.context().root().absolute_path(),
        order.paths().directory().absolute_path(),
        &facts.document,
        order.readme_text().map(str::to_owned),
    );

    Ok(OrderEditPlan {
        root: package.context().root().absolute_path().to_path_buf(),
        directory: order.paths().directory().absolute_path().to_path_buf(),
        order_path,
        facts,
        projected,
        prior_document,
        vendor,
        reusable_values,
        date_ordered_source,
        prior_index: package.index().clone(),
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
        .vendors()
        .iter()
        .find(|vendor| vendor.vendor_id.as_str() == vendor_id)
        .cloned()
}

/// Canonicalizes every Reusable Free-Entry Value the resulting Order carries and
/// refuses a genuinely new value the caller did not explicitly introduce.
fn resolve_reusable_values<E>(
    assignments: &[(String, String, AssignmentIntent)],
    object: &mut serde_json::Map<String, serde_json::Value>,
    package: &CurrentPackageCandidate,
) -> Result<OrderEditReusableValues, OrderEditPlanError<E>> {
    let mut outcomes = Vec::new();
    for field in REUSABLE_FIELDS {
        let existing = existing_values(field, package);
        let present = object.get(field).and_then(serde_json::Value::as_str);
        let outcome = canonicalize_reusable_value(present, &existing);
        if let ReusableValue::New(proposed) = &outcome
            && !assignments.iter().any(|(assigned, _, intent)| {
                assigned == field && *intent == AssignmentIntent::Introduce
            })
        {
            return Err(OrderEditPlanError::UnconfirmedReusableValue {
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
    let mut outcomes = outcomes.into_iter();

    Ok(OrderEditReusableValues {
        orderer: outcomes.next().expect("orderer is a reusable field"),
        ordering_institution: outcomes
            .next()
            .expect("ordering_institution is a reusable field"),
        project: outcomes.next().expect("project is a reusable field"),
        order_group: outcomes.next().expect("order_group is a reusable field"),
    })
}

fn existing_values<'a>(field: &str, package: &'a CurrentPackageCandidate) -> Vec<&'a str> {
    let projections = match field {
        "orderer" => package.orderers(),
        "ordering_institution" => package.ordering_institutions(),
        "project" => package.projects(),
        _ => package.order_groups(),
    };
    projections
        .iter()
        .map(ReusableValueProjection::value)
        .collect()
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

fn parse_status(value: &str) -> Option<OrderStatus> {
    serde_json::from_value(serde_json::Value::String(value.trim().to_owned())).ok()
}

/// The first assigned fact of one set whose value the Order record cannot hold, if any.
///
/// **The three are adjudicated together, before anything is assembled**, so a set carrying
/// an unacceptable value is refused naming the fact rather than by the assembled candidate
/// failing to parse — see [`OrderFactKind`].
///
/// An empty value is not adjudicated: it clears an optional fact, and a blank required one
/// is already a refused assignment.
fn unholdable_fact(
    assignments: &[(String, String, AssignmentIntent)],
) -> Option<(OrderFactKind, String)> {
    [
        OrderFactKind::Status,
        OrderFactKind::DateOrdered,
        OrderFactKind::Currency,
    ]
    .into_iter()
    .find_map(|fact| {
        let value = assigned(assignments, fact.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        (!fact.holds(value)).then(|| (fact, value.to_owned()))
    })
}

/// The assignments of one set that broke no rule, or every one that did.
///
/// **One list of assignable fields and one set of refusal reasons**, shared by an edit and
/// by the two capabilities that give an Order its first record — see the module
/// documentation. A caller reports the refusals in its own failure type; the adjudication
/// is not any of theirs.
fn accepted_assignments(
    assignments: &OrderAssignments,
) -> Result<Vec<(String, String, AssignmentIntent)>, Vec<RejectedAssignment>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String, AssignmentIntent)>::new();
    for (field, value, intent) in &assignments.entries {
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
            field
                if *intent == AssignmentIntent::Introduce && !REUSABLE_FIELDS.contains(&field) =>
            {
                Some(RejectedAssignmentReason::NotAReusableValue)
            }
            "title" | "date_ordered" | "status" if value.trim().is_empty() => {
                Some(RejectedAssignmentReason::BlankRequiredValue)
            }
            _ => None,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone(), *intent)),
        }
    }
    if !rejected.is_empty() {
        return Err(rejected);
    }
    Ok(accepted)
}

/// Why one Order Directory could not be resolved to a healthy Order of its own
/// Package. Every Order-scoped capability refuses on these same terms, so they are
/// named once here rather than restated per capability.
#[derive(Debug, PartialEq)]
pub(crate) enum OrderResolutionFailure<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound {
        path: PathBuf,
    },
    DamagedOrder {
        path: PathBuf,
    },
    /// **A [`FailedPath`] rather than a `PathBuf`.** This resolution walks from the Order
    /// Directory a caller named up to the Package that holds it — reading the record for
    /// its locator and canonicalizing the directory are how the Root is *found* — so an
    /// operation that fails here fails before any Root is resolved. There is no Package to
    /// spell the path relative to, and the type is what says so.
    FileSystem {
        path: FailedPath,
        error: E,
    },
}

impl<E> From<OrderResolutionFailure<E>> for OrderEditPlanError<E> {
    fn from(failure: OrderResolutionFailure<E>) -> Self {
        match failure {
            OrderResolutionFailure::UnmanagedPackage => Self::UnmanagedPackage,
            OrderResolutionFailure::DamagedPackage => Self::DamagedPackage,
            OrderResolutionFailure::UpgradePending => Self::UpgradePending,
            OrderResolutionFailure::PackageOperationFailed => Self::PackageOperationFailed,
            OrderResolutionFailure::OrderNotFound { path } => Self::OrderNotFound { path },
            OrderResolutionFailure::DamagedOrder { path } => Self::DamagedOrder { path },
            OrderResolutionFailure::FileSystem { path, error } => Self::FileSystem { path, error },
        }
    }
}

/// Resolves the Order Directory's own Package through its normalized locator, then
/// requires the directory to be one healthy Order of that Package.
pub(crate) fn resolve_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<(CurrentPackageCandidate, HealthyOrder), OrderResolutionFailure<F::Error>> {
    let order_path = order_directory.join(".spielorders").join("order.json");
    // A missing, unreadable, wrongly typed, or unparseable record is one Damaged
    // Order, exactly as a semantically invalid one is; an edit never repairs or
    // adopts it. Only a failure to observe the path at all remains an operation
    // failure, matching how the Package loader classifies a managed artifact.
    if filesystem.symlink_metadata(&order_path).map_err(|error| {
        OrderResolutionFailure::FileSystem {
            path: FailedPath::of(&order_path),
            error,
        }
    })? != Some(FileSystemEntryKind::File)
    {
        return Err(OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        });
    }
    let bytes = filesystem
        .read(&order_path)
        .map_err(|_| OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        })?;
    let document = OrderDocument::from_json_bytes(&bytes).map_err(|_| {
        OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        }
    })?;
    let directory = filesystem.canonicalize(order_directory).map_err(|error| {
        OrderResolutionFailure::FileSystem {
            path: FailedPath::of(order_directory),
            error,
        }
    })?;
    // A record with no locator this build can follow is a Damaged Order, exactly as one
    // that would not parse is: the locator is this resolution's only route to a Package,
    // and nothing here searches for one.
    let parent_count = document
        .package_root
        .spelled()
        .ok_or_else(|| OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        })?
        .as_str()
        .split('/')
        .count();
    let root = directory
        .ancestors()
        .nth(parent_count)
        .ok_or_else(|| OrderResolutionFailure::OrderNotFound {
            path: directory.clone(),
        })?
        .to_path_buf();

    let package = match inspect_canonical_package(&root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        CanonicalPackageLoad::Unmanaged(_) => return Err(OrderResolutionFailure::UnmanagedPackage),
        CanonicalPackageLoad::DiagnosticOnly(_) => {
            return Err(OrderResolutionFailure::DamagedPackage);
        }
        CanonicalPackageLoad::UpgradePending(_) => {
            return Err(OrderResolutionFailure::UpgradePending);
        }
        CanonicalPackageLoad::OperationFailed(_) => {
            return Err(OrderResolutionFailure::PackageOperationFailed);
        }
    };
    if let Some(order) = package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.paths().directory().absolute_path() == directory)
        .cloned()
    {
        return Ok((package, order));
    }
    if package
        .damaged_orders()
        .iter()
        .any(|candidate| candidate.paths().directory().absolute_path() == directory)
    {
        return Err(OrderResolutionFailure::DamagedOrder { path: directory });
    }
    Err(OrderResolutionFailure::OrderNotFound { path: directory })
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditResult {
    order: HealthyOrder,
    vendor: Option<Vendor>,
    index: PackageIndexRecord,
}

impl OrderEditResult {
    /// The complete refreshed Order projection, including calculated totals.
    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    /// The Vendor the committed Order references, or `None` where it references none.
    ///
    /// Beside the Order because the refreshed projection resolves it — see
    /// [`OrderEditPlan::vendor`] — and an edit is what can change which Vendor that is.
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

/// An Order edit's candidate is `plan.facts.document` serialized, and that document
/// was produced by parsing and validating in `order_record_plan`, which refuses with
/// `OrderEditPlanError::InvalidFacts` when it cannot. There is no way to reach a
/// commit holding a candidate that was never validated, so no candidate validation
/// failure is possible, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum OrderEditRejection<E> {
    /// **A `Location` rather than a `PathBuf`**, because this refusal happens under a Root
    /// the plan established, about a path inside it — the Root, the Order Directory, its
    /// record, or the Vendor aggregate — and a response owes that Root's spelling for what
    /// it names, in a refusal exactly as in a success. Handing back a bare path is what
    /// left an adapter to re-derive the spelling or drop it.
    PreconditionsChanged { location: Location },
    ReplacementFailed {
        error: ValidatedReplacementError<E, OrderCandidateFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderEditWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

pub type OrderEditReport<E> =
    MutationReport<OrderEditPlan, OrderEditResult, OrderEditRejection<E>, OrderEditWarning>;

pub fn commit_order_edit<F: FileSystem>(
    plan: OrderEditPlan,
    filesystem: &F,
) -> OrderEditReport<F::Error> {
    let current = match inspect_canonical_package(&plan.root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        _ => {
            let path = plan.root.clone();
            return preconditions_changed(plan, path);
        }
    };
    let Some(order) = current
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.paths().directory().absolute_path() == plan.directory)
    else {
        let path = plan.directory.clone();
        return preconditions_changed(plan, path);
    };
    if order.document() != &plan.prior_document {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
    }
    if let Some(vendor_id) = plan.facts.vendor_id()
        && !current
            .vendors()
            .iter()
            .any(|vendor| vendor.vendor_id.as_str() == vendor_id)
    {
        let path = current.context().vendors().absolute_path().to_path_buf();
        return preconditions_changed(plan, path);
    }
    if !reusable_values_still_current(&plan, &current) {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
    }

    if !plan.changes_canonical_order() {
        let result = OrderEditResult {
            order: order.clone(),
            vendor: resolved_vendor(plan.facts.vendor_id(), &current),
            index: current.index().clone(),
        };
        return MutationReport::Unchanged { plan, result };
    }
    // An Order mutation replaces only the record, so what the load knew before the
    // replacement is what the committed Order holds. A projection built without a
    // refreshed Package still owes the caller the complete Order, README included.
    let readme_text = order.readme_text().map(str::to_owned);
    let Ok(candidate) = plan.facts.document.to_json_bytes() else {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
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
        replace_validated_file_expecting(
            &plan.order_path,
            &candidate,
            DestinationExpectation::Equivalent(&still_holds_the_revalidated_order),
            filesystem,
            |_| Ok::<(), OrderCandidateFailure>(()),
        )
    };
    if let Err(error) = replacement {
        return MutationReport::Rejected {
            plan,
            rejection: OrderEditRejection::ReplacementFailed { error },
        };
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => {
            let result = result_from_package(&plan, &package, readme_text.as_deref());
            MutationReport::Committed { plan, result }
        }
        PackageLoad::IndexReplacementFailed(failed) => {
            let result =
                result_from_package(&plan, failed.canonical_evidence(), readme_text.as_deref());
            let warning = OrderEditWarning::IndexRefreshFailed(failed.failure().clone());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        PackageLoad::OperationFailed(failed) => {
            let warning = OrderEditWarning::IndexRefreshFailed(failed.failure().clone());
            let result = result_without_refresh(&plan, readme_text.as_deref());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        _ => {
            let result = result_without_refresh(&plan, readme_text.as_deref());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![OrderEditWarning::RefreshUnavailable],
            }
        }
    }
}

/// Re-canonicalizes every planned Reusable Free-Entry Value against current Package
/// state so a concurrently introduced canonical spelling cannot commit stale text.
fn reusable_values_still_current(plan: &OrderEditPlan, current: &CurrentPackageCandidate) -> bool {
    REUSABLE_FIELDS.iter().all(|field| {
        let planned = match *field {
            "orderer" => plan.facts.orderer(),
            "ordering_institution" => plan.facts.ordering_institution(),
            "project" => plan.facts.project(),
            _ => plan.facts.order_group(),
        };
        let existing = existing_values(field, current);
        match canonicalize_reusable_value(planned, &existing) {
            ReusableValue::Absent => planned.is_none(),
            ReusableValue::Reused(value) | ReusableValue::New(value) => {
                planned == Some(value.as_str())
            }
        }
    })
}

/// One path this plan named inside its own Package, as that Package locates it.
fn preconditions_changed<E>(plan: OrderEditPlan, path: PathBuf) -> OrderEditReport<E> {
    let location = located_in_package(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: OrderEditRejection::PreconditionsChanged { location },
    }
}

fn result_from_package(
    plan: &OrderEditPlan,
    package: &CurrentPackageCandidate,
    readme_text: Option<&str>,
) -> OrderEditResult {
    let order = package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.document().order_id == plan.facts.document.order_id)
        .cloned()
        .unwrap_or_else(|| projected_order(plan, readme_text));
    OrderEditResult {
        order,
        vendor: resolved_vendor(plan.facts.vendor_id(), package),
        index: package.index().clone(),
    }
}

/// The committed Order as this plan leaves it, where no load could be refreshed.
///
/// The Vendor is the plan's own resolution, for the reason the Order is the plan's own
/// projection: an Order mutation replaces only the record, so the Vendor the load that
/// ran this commit's preconditions resolved is the Vendor the committed Order references.
fn result_without_refresh(plan: &OrderEditPlan, readme_text: Option<&str>) -> OrderEditResult {
    OrderEditResult {
        order: projected_order(plan, readme_text),
        vendor: plan.vendor.clone(),
        index: plan
            .prior_index
            .after_committed_order_metadata_change_without_refresh(),
    }
}

fn projected_order(plan: &OrderEditPlan, readme_text: Option<&str>) -> HealthyOrder {
    project_committed_order(
        &plan.root,
        &plan.directory,
        &plan.facts.document,
        readme_text.map(str::to_owned),
    )
}

const ITEM_FIELDS: [&str; 4] = ["description", "quantity", "unit_price", "catalog_number"];

const REQUIRED_ITEM_FIELDS: [&str; 3] = ["description", "quantity", "unit_price"];

/// One order-independent canonical `field=value` assignment set for one Order Item.
/// An empty value clears the optional Catalog Number; a required Item fact is never
/// blank.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ItemAssignments {
    entries: Vec<(String, String)>,
}

impl ItemAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.push((field.into(), value.into()));
        self
    }
}

/// Why one canonical assignment set does not name an Order Item.
///
/// **For the capability that gives an Order Items before it has a record**: adoption, the
/// only mutation that may carry Items at creation. The four Item mutations refuse on these
/// same rules and report them through [`OrderEditPlanError`] instead, because each has a
/// collection to refuse a *position* against as well.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NewItemAssignmentFailure {
    /// At least one assignment broke a rule, so the set named no Item. Every refused
    /// assignment is reported, because a set is adjudicated as a whole.
    InvalidAssignments(Vec<RejectedAssignment>),
    /// The set named none of these required Item facts, in the record's own spellings.
    MissingRequiredFacts { fields: Vec<String> },
    /// The Item this set assembled is not one the record can hold.
    ///
    /// Every accepted assignment is already a value the record's own type accepted and
    /// every required fact was found, so nothing a caller can write reaches this today. It
    /// is a refusal rather than a panic because assembling an Item is one function shared
    /// with the Item mutations, which assemble one over an existing Item's stored facts.
    InvalidFacts,
}

/// The Order Item one canonical assignment set names, or every reason it names none.
///
/// **One list of Item fields and one set of refusal reasons**, shared by the four Item
/// mutations and by adoption — see the module documentation. So an unknown field, a
/// repeated one, a blank Description, and a Quantity the record could not hold as a finite
/// number are refused identically whichever capability was handed the set.
pub fn new_order_item(
    assignments: &ItemAssignments,
) -> Result<OrderItem, NewItemAssignmentFailure> {
    let accepted = accepted_item_assignments(assignments)
        .map_err(NewItemAssignmentFailure::InvalidAssignments)?;
    planned_item(None, &accepted).map_err(|failure| match failure {
        PlannedItemFailure::MissingRequiredFacts { fields } => {
            NewItemAssignmentFailure::MissingRequiredFacts { fields }
        }
        PlannedItemFailure::Invalid => NewItemAssignmentFailure::InvalidFacts,
    })
}

/// Which end of an ordered Item operation a one-based position names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemPositionRole {
    Source,
    Destination,
}

impl ItemPositionRole {
    /// The stable spelling of one role.
    ///
    /// **The same two spellings a Contact position's role carries** — see
    /// [`crate::vendors::ContactPositionRole::as_str`]. The two ordered collections are
    /// addressed the same way and refuse a position the same way, so a caller reading which
    /// end was refused reads one vocabulary rather than one per collection.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source_position",
            Self::Destination => "destination_position",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderItemMutationKind {
    Add,
    Edit,
    Move,
    Remove,
}

impl OrderItemMutationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Edit => "edit",
            Self::Move => "move",
            Self::Remove => "remove",
        }
    }

    /// What committing this mutation does to the addressed Item.
    ///
    /// Read off the plan rather than off the result, as
    /// [`crate::vendors::VendorContactMutationKind::canonical_outcome`] is: a dry run has
    /// no result to read an outcome off, so it reads it here, and a plan and the result of
    /// committing it cannot come to disagree about what the mutation was. Every Item
    /// mutation replaces the owning Order record, so the *Order's* canonical change is the
    /// same one whichever of the four ran; this is the Item's own.
    pub fn canonical_outcome(self) -> CanonicalItemOutcome {
        match self {
            Self::Add => CanonicalItemOutcome::Added,
            Self::Edit => CanonicalItemOutcome::Edited,
            Self::Move => CanonicalItemOutcome::Moved,
            Self::Remove => CanonicalItemOutcome::Removed,
        }
    }
}

/// What committing one Item mutation does to the addressed Item.
///
/// **The vocabulary the two ordered collections share.** A Vendor's Contacts and an
/// Order's Items are addressed the same way and mutated by the same four verbs, so what a
/// report says happened to one of them is spelled the same way as what it says happened to
/// the other — see [`crate::vendors::CanonicalContactOutcome`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalItemOutcome {
    Added,
    Edited,
    Moved,
    Removed,
}

impl CanonicalItemOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Edited => "edited",
            Self::Moved => "moved",
            Self::Removed => "removed",
        }
    }
}

/// A complete, validated mutation of one Order's embedded Item collection. It
/// rewrites the whole owning Order record once and changes no other fact.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderItemMutationPlan {
    kind: OrderItemMutationKind,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    subject_item: OrderItem,
    order_plan: OrderEditPlan,
}

impl OrderItemMutationPlan {
    pub fn root(&self) -> &Path {
        self.order_plan.root()
    }

    /// The Order Directory, whose name an Item mutation never changes.
    pub fn directory(&self) -> &Path {
        self.order_plan.directory()
    }

    pub fn kind(&self) -> OrderItemMutationKind {
        self.kind
    }

    pub fn source_position(&self) -> Option<usize> {
        self.source_position
    }

    pub fn destination_position(&self) -> Option<usize> {
        self.destination_position
    }

    /// The added, edited, moved, or removed Item itself.
    pub fn subject_item(&self) -> &OrderItem {
        &self.subject_item
    }

    /// The complete refreshed Order this mutation would commit, with its exact
    /// recalculated Item and Order totals.
    pub fn facts(&self) -> &PlannedOrderFacts {
        self.order_plan.facts()
    }

    /// The complete Order this mutation would leave, projected as a load projects a
    /// committed one. See [`OrderEditPlan::projected_order`].
    pub fn projected_order(&self) -> &HealthyOrder {
        self.order_plan.projected_order()
    }

    /// The Vendor the owning Order references. An Item mutation assigns no Vendor
    /// Reference, so this is the one the Order already carried.
    pub fn vendor(&self) -> Option<&Vendor> {
        self.order_plan.vendor()
    }

    /// The derived Package Index as it stood before this mutation.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        self.order_plan.prior_index()
    }

    /// Every Reusable Free-Entry Value the Order carries. An Item mutation assigns
    /// no scalar fact, so each one is retained rather than reused or introduced.
    pub fn reusable_values(&self) -> &OrderEditReusableValues {
        self.order_plan.reusable_values()
    }

    pub fn operations(&self) -> &[OrderEditOperation] {
        self.order_plan.operations()
    }

    pub fn changes_canonical_order(&self) -> bool {
        self.order_plan.changes_canonical_order()
    }
}

/// Lists one Order's embedded Items in their exact user-controlled order, together
/// with the complete owning Order and its calculated, never stored, totals.
pub fn list_order_items<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<HealthyOrder, OrderEditPlanError<F::Error>> {
    let (_package, order) = resolve_order(order_directory, filesystem)?;
    Ok(order)
}

/// Appends one Item, or inserts it before the pre-mutation Item at an explicit
/// one-based position.
pub fn plan_order_item_add<F: FileSystem>(
    order_directory: &Path,
    assignments: &ItemAssignments,
    position: Option<usize>,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let count = prior_document.items.len();
    let destination_position = position.unwrap_or(count + 1);
    let destination_index = item_index(
        ItemPositionRole::Destination,
        destination_position,
        count + 1,
    )?;
    let assignments =
        accepted_item_assignments(assignments).map_err(OrderEditPlanError::InvalidAssignments)?;
    let item = planned_item(None, &assignments)?;
    let mut items = prior_document.items.clone();
    items.insert(destination_index, item);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[destination_index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Add,
        source_position: None,
        destination_position: Some(destination_position),
        subject_item,
        order_plan,
    })
}

/// Replaces the facts of the Item at one pre-mutation one-based position, leaving
/// its position and every other Item exactly as stored.
pub fn plan_order_item_edit<F: FileSystem>(
    order_directory: &Path,
    position: usize,
    assignments: &ItemAssignments,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let index = item_index(
        ItemPositionRole::Source,
        position,
        prior_document.items.len(),
    )?;
    let assignments =
        accepted_item_assignments(assignments).map_err(OrderEditPlanError::InvalidAssignments)?;
    let item = planned_item(Some(&prior_document.items[index]), &assignments)?;
    let mut items = prior_document.items.clone();
    items[index] = item;

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Edit,
        source_position: Some(position),
        destination_position: Some(position),
        subject_item,
        order_plan,
    })
}

/// Lifts the Item at one pre-mutation one-based position and reinserts it at
/// another. Both positions name Items that already exist, so a move can neither
/// reach past the last Item nor change how many Items the Order has.
pub fn plan_order_item_move<F: FileSystem>(
    order_directory: &Path,
    source_position: usize,
    destination_position: usize,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let count = prior_document.items.len();
    let source_index = item_index(ItemPositionRole::Source, source_position, count)?;
    let destination_index = item_index(ItemPositionRole::Destination, destination_position, count)?;
    let mut items = prior_document.items.clone();
    let item = items.remove(source_index);
    items.insert(destination_index, item);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[destination_index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Move,
        source_position: Some(source_position),
        destination_position: Some(destination_position),
        subject_item,
        order_plan,
    })
}

/// Drops the Item at one pre-mutation one-based position. Its subject is absent
/// from the resulting collection, so it is reported from the one it left.
pub fn plan_order_item_remove<F: FileSystem>(
    order_directory: &Path,
    position: usize,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let index = item_index(
        ItemPositionRole::Source,
        position,
        prior_document.items.len(),
    )?;
    let mut items = prior_document.items.clone();
    let subject_item = items.remove(index);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Remove,
        source_position: Some(position),
        destination_position: None,
        subject_item,
        order_plan,
    })
}

/// Rewrites the whole owning Order record with a candidate Item collection,
/// retaining every scalar fact exactly as stored.
fn order_item_plan<E>(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    prior_document: OrderDocument,
    items: Vec<OrderItem>,
) -> Result<OrderEditPlan, OrderEditPlanError<E>> {
    let mut object = serde_json::to_value(&prior_document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
    object.insert(
        "items".into(),
        serde_json::to_value(&items).map_err(|_| OrderEditPlanError::InvalidFacts)?,
    );
    let reusable_values = retained_reusable_values(&prior_document);
    order_record_plan(
        package,
        order,
        prior_document,
        object,
        reusable_values,
        DateOrderedSource::Retained,
    )
}

/// An Item mutation assigns no scalar fact, so every Reusable Free-Entry Value the
/// Order already carries is retained with its stored canonical spelling.
fn retained_reusable_values(document: &OrderDocument) -> OrderEditReusableValues {
    let retained = |value: Option<&String>| match value {
        Some(value) => ReusableValue::Reused(value.clone()),
        None => ReusableValue::Absent,
    };
    OrderEditReusableValues {
        orderer: retained(document.orderer.as_ref()),
        ordering_institution: retained(document.ordering_institution.as_ref()),
        project: retained(document.project.as_ref()),
        order_group: retained(document.order_group.as_ref()),
    }
}

fn item_index<E>(
    role: ItemPositionRole,
    position: usize,
    maximum: usize,
) -> Result<usize, OrderEditPlanError<E>> {
    if position == 0 || position > maximum {
        Err(OrderEditPlanError::InvalidItemPosition {
            role,
            position,
            minimum: 1,
            maximum,
        })
    } else {
        Ok(position - 1)
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
/// Reported as the refusals themselves rather than as one caller's error type, because
/// the adjudication is shared: the four Item mutations carry these into
/// [`OrderEditPlanError`] and adoption into [`NewItemAssignmentFailure`].
fn accepted_item_assignments(
    assignments: &ItemAssignments,
) -> Result<Vec<(String, ItemValue)>, Vec<RejectedAssignment>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, ItemValue)>::new();
    for (field, value) in &assignments.entries {
        let duplicate = accepted.iter().any(|(accepted, _)| accepted == field);
        match item_assignment(field, value, duplicate) {
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
) -> Result<ItemValue, RejectedAssignmentReason> {
    if !ITEM_FIELDS.contains(&field) {
        return Err(RejectedAssignmentReason::UnknownField);
    }
    if duplicate {
        return Err(RejectedAssignmentReason::Duplicate);
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
        "quantity" | "unit_price" => value
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
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

impl<E> From<PlannedItemFailure> for OrderEditPlanError<E> {
    fn from(failure: PlannedItemFailure) -> Self {
        match failure {
            PlannedItemFailure::MissingRequiredFacts { fields } => {
                Self::MissingRequiredItemFacts { fields }
            }
            PlannedItemFailure::Invalid => Self::InvalidFacts,
        }
    }
}

pub type OrderItemMutationReport<E> =
    MutationReport<OrderItemMutationPlan, OrderEditResult, OrderEditRejection<E>, OrderEditWarning>;

/// Commits one already validated Item plan. The collection lives inside its owning
/// Order, so this is the same single validated record replacement, rollback, and
/// derived-index refresh a scalar edit performs.
pub fn commit_order_item_mutation<F: FileSystem>(
    plan: OrderItemMutationPlan,
    filesystem: &F,
) -> OrderItemMutationReport<F::Error> {
    let OrderItemMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_item,
        order_plan,
    } = plan;
    let wrap = |order_plan| OrderItemMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_item,
        order_plan,
    };
    match commit_order_edit(order_plan, filesystem) {
        MutationReport::Rejected { plan, rejection } => MutationReport::Rejected {
            plan: wrap(plan),
            rejection,
        },
        MutationReport::Unchanged { plan, result } => MutationReport::Unchanged {
            plan: wrap(plan),
            result,
        },
        MutationReport::Committed { plan, result } => MutationReport::Committed {
            plan: wrap(plan),
            result,
        },
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => MutationReport::CommittedWithWarning {
            plan: wrap(plan),
            result,
            warnings,
        },
    }
}
