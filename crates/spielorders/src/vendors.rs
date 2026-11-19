//! Planning and persistence for the complete Vendor aggregate lifecycle.

use crate::{
    Answer, Assignments, Failure,
    answer::DraftFailure,
    canonical::{CurrentPackageCandidate, Touched, load_every_record, load_package_to_change},
    change::{Assignment, PlannedAct},
    integrity::{Aggregate, Records, Substitution},
    journal::{Journal, ManagedRecord, Subject},
    metadata::{StoredValues, Vendor, VendorContact, VendorId, VendorsDocument, Version1},
    mutation::{MutationReport, OperationKind, RejectedAssignment, RejectedAssignmentReason},
    ports::FileSystem,
    readme::{Location, RootedLocation, located_in_package, rooted_at},
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_VENDOR_ID: &str = crate::mutation::PLANNING_IDENTITY;

/// Every fact of a Vendor a caller may assign, in the order an editor lists them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VendorField {
    Name,
    Phone,
    Email,
    Website,
    Address,
    PurchasingNotes,
}

impl VendorField {
    pub(crate) const ALL: [Self; 6] = [
        Self::Name,
        Self::Phone,
        Self::Email,
        Self::Website,
        Self::Address,
        Self::PurchasingNotes,
    ];

    /// The name a caller assigns this field by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Phone => "phone",
            Self::Email => "email",
            Self::Website => "website",
            Self::Address => "address",
            Self::PurchasingNotes => "purchasing_notes",
        }
    }

    /// The field `name` assigns.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }
}

/// Every fact of a Vendor Contact a caller may assign, in the order an editor lists them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContactField {
    Name,
    Role,
    Phone,
    Email,
}

impl ContactField {
    pub(crate) const ALL: [Self; 4] = [Self::Name, Self::Role, Self::Phone, Self::Email];

    /// The name a caller assigns this field by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Role => "role",
            Self::Phone => "phone",
            Self::Email => "email",
        }
    }

    /// The field `name` assigns.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }
}

/// One Vendor's planned shared facts. A created Vendor's identity is generated only at
/// commit.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedVendorFacts {
    pub(crate) document: Vendor,
}

/// The visibility kinds govern whether a Vendor is offered for new use; Delete removes
/// its record only when no readable Order reference names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorMutationKind {
    Create,
    Edit,
    Hide,
    Unhide,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VendorMutationPlan {
    root: PathBuf,
    vendors_path: Location,
    kind: VendorMutationKind,
    pub(crate) subject: PlannedVendorFacts,
    vendor_id: String,
    candidate_vendors: Vec<Vendor>,
    prior_vendors: Vec<Vendor>,
    /// The values the record kept when planned, which the commit writes back unchanged.
    prior_values: StoredValues,
    original_vendor: Option<Vendor>,
    pub(crate) journal: Journal,
}

impl VendorMutationPlan {
    /// Require that the editor's original Vendor still matches at commit.
    pub(crate) fn require_original(mut self, original: Option<&Vendor>) -> Self {
        if let Some(original) = original {
            self.original_vendor = Some(original.clone());
        }
        self
    }

    /// The Vendor aggregate committing this plan writes, a created Vendor carrying the
    /// planning identity.
    pub(crate) fn candidate_vendors(&self) -> &[Vendor] {
        &self.candidate_vendors
    }

    /// Whether committing this plan would change the canonical Vendor aggregate.
    /// A dry run of an unchanged mutation reports the same result a commit does.
    pub fn changes_canonical_vendors(&self) -> bool {
        self.candidate_vendors != self.prior_vendors
    }
}

/// The Contact one assignment set names whole, as a new Contact of a Vendor in `package`.
fn new_contact(
    assignments: &Assignments,
    package: &CurrentPackageCandidate,
) -> Result<VendorContact, Failure> {
    let mut facts = accepted_contact_assignments(assignments)?;
    facts.retain(|(_, value)| !value.trim().is_empty());
    if !facts.iter().any(|(field, _)| field == "name") {
        return Err(Failure::MissingRequiredName {
            scope: vendors_location(package),
        });
    }
    let mut object = serde_json::Map::new();
    for (field, value) in facts {
        object.insert(field, serde_json::Value::String(value));
    }
    serde_json::from_value::<VendorContact>(serde_json::Value::Object(object))
        .map_err(|_| Failure::InvalidAggregate)
}

/// One row of a whole draft as its Contact, or every fault the row has: a missing Name
/// is reported beside the values the row rejected, not hidden by them.
fn draft_contact(
    assignments: &Assignments,
    package: &CurrentPackageCandidate,
) -> Result<VendorContact, Vec<Failure>> {
    new_contact(assignments, package).map_err(|failure| {
        let named = assignments
            .entries
            .iter()
            .any(|entry| entry.field == "name");
        match failure {
            rejected @ Failure::InvalidAssignments(_) if !named => vec![
                rejected,
                Failure::MissingRequiredName {
                    scope: vendors_location(package),
                },
            ],
            failure => vec![failure],
        }
    })
}

pub(crate) fn plan_vendor_creation<F: FileSystem>(
    selected_root: &Path,
    assignments: &Assignments,
    contacts: &[Assignments],
    filesystem: &F,
) -> Answer<VendorMutationPlan> {
    Answer::of(|| {
        let package = load_package_to_change(
            selected_root,
            &Touched::default().with_vendors(true),
            filesystem,
        )?;
        vendor_creation_in(&package, assignments, contacts)
    })
}

/// Plans creating a Vendor with `assignments` and `contacts` in `package` as loaded.
pub(crate) fn vendor_creation_in(
    package: &CurrentPackageCandidate,
    assignments: &Assignments,
    contacts: &[Assignments],
) -> Result<VendorMutationPlan, Failure> {
    let at_facts = |failure| DraftFailure { row: None, failure };
    let mut faults = Vec::new();
    // A blank optional value means absence on creation, so nothing is left to clear.
    // A blank Name never reaches here; `accepted_assignments` already rejected it.
    let mut facts = accepted_assignments(assignments).unwrap_or_else(|failure| {
        faults.push(at_facts(failure));
        Vec::new()
    });
    facts.retain(|(_, value)| !value.trim().is_empty());
    match facts.iter().find(|(field, _)| field == "name") {
        Some((_, name)) => {
            if let Err(failure) = reject_duplicate_name(name, None, package) {
                faults.push(at_facts(failure));
            }
        }
        None if faults.is_empty() => faults.push(at_facts(Failure::MissingRequiredName {
            scope: vendors_location(package),
        })),
        None => {}
    }
    let rows = DraftFailure::rows(contacts, &mut faults, |row| draft_contact(row, package));
    if !faults.is_empty() {
        // A draft with rows is refused with every rule it broke, as a new Order is.
        return Err(if contacts.is_empty() {
            DraftFailure::first(faults)
        } else {
            Failure::InvalidDraft(faults)
        });
    }

    let mut object = json!({ "vendor_id": PLANNING_VENDOR_ID, "contacts": rows });
    let target = object
        .as_object_mut()
        .expect("the planned Vendor candidate is an object");
    for (field, value) in &facts {
        target.insert(field.clone(), serde_json::Value::String(value.clone()));
    }
    let mut vendors = package.vendors.to_vec();
    vendors.push(serde_json::from_value::<Vendor>(object).map_err(|_| Failure::InvalidAggregate)?);

    plan_candidate(
        package,
        VendorMutationKind::Create,
        vendors,
        PLANNING_VENDOR_ID,
    )
}

/// Plans an editor's whole draft of one Vendor: its facts and its complete Contact list.
pub(crate) fn plan_vendor_edit<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    assignments: &Assignments,
    contacts: &[Assignments],
    filesystem: &F,
) -> Answer<VendorMutationPlan> {
    Answer::of(|| {
        let package = load_package_to_change(
            selected_root,
            &Touched::default().with_vendors(true),
            filesystem,
        )?;
        let position = vendor_position(vendor_id, &package.vendors)?;
        edited_vendor(&package, position, assignments, Some(contacts))
            .map_err(Failure::InvalidDraft)
    })
}

/// The plan that rewrites the Vendor at `position` with `assignments` over its stored
/// facts and, where given, `contacts` in place of its stored Contacts; or every rule that
/// broke, the facts' first and then each row's.
pub(crate) fn edited_vendor(
    package: &CurrentPackageCandidate,
    position: usize,
    assignments: &Assignments,
    contacts: Option<&[Assignments]>,
) -> Result<VendorMutationPlan, Vec<DraftFailure>> {
    let at_facts = |failure| DraftFailure { row: None, failure };
    let existing = &package.vendors[position];
    let (assignments, rejected) = adjudicated_assignments(assignments);
    let mut faults = Vec::new();
    if !rejected.is_empty() {
        faults.push(at_facts(Failure::InvalidAssignments(rejected)));
    }
    let Some(mut object) = serde_json::to_value(existing)
        .ok()
        .and_then(|value| value.as_object().cloned())
    else {
        faults.push(at_facts(Failure::InvalidAggregate));
        return Err(faults);
    };
    for (field, value) in &assignments {
        if value.trim().is_empty() {
            object.remove(field);
        } else {
            object.insert(field.clone(), serde_json::Value::String(value.clone()));
        }
    }
    let mut edited = match serde_json::from_value::<Vendor>(serde_json::Value::Object(object)) {
        Ok(edited) => {
            if let Err(failure) =
                reject_duplicate_name(edited.name.as_str(), Some(&existing.vendor_id), package)
            {
                faults.push(at_facts(failure));
            }
            Some(edited)
        }
        Err(_) => {
            faults.push(at_facts(Failure::InvalidAggregate));
            None
        }
    };

    if let Some(rows) = contacts {
        let contacts = DraftFailure::rows(rows, &mut faults, |row| draft_contact(row, package));
        if let Some(edited) = &mut edited {
            edited.contacts = contacts;
        }
    }

    match edited {
        Some(edited) if faults.is_empty() => {
            let mut vendors = package.vendors.to_vec();
            vendors[position] = edited;
            plan_candidate(
                package,
                VendorMutationKind::Edit,
                vendors,
                existing.vendor_id.as_str(),
            )
            .map_err(|failure| vec![at_facts(failure)])
        }
        _ => Err(faults),
    }
}

/// Stops offering one Vendor for new use. Its record and its Vendor ID survive, so every
/// Order referencing it keeps resolving; hiding is never refused on reference grounds.
pub(crate) fn plan_vendor_hide<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Answer<VendorMutationPlan> {
    Answer::of(|| {
        plan_vendor_visibility(
            selected_root,
            vendor_id,
            VendorMutationKind::Hide,
            filesystem,
        )
    })
}

/// Offers a Hidden Vendor for new use again. Hiding is a reversible toggle.
pub(crate) fn plan_vendor_unhide<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Answer<VendorMutationPlan> {
    Answer::of(|| {
        plan_vendor_visibility(
            selected_root,
            vendor_id,
            VendorMutationKind::Unhide,
            filesystem,
        )
    })
}

/// Removes one Vendor and its Contacts, refused while any Order, removed ones included, names
/// it, so that no Order is left with an unresolved Vendor Reference.
pub(crate) fn plan_vendor_delete<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Answer<VendorMutationPlan> {
    Answer::of(|| {
        // Which Orders reference the Vendor is what their records hold.
        let package = load_every_record(selected_root, filesystem)?;
        let position = vendor_position(vendor_id, &package.vendors)?;
        let orders = references(&package, &package.vendors[position].vendor_id, filesystem)?;
        if !orders.is_empty() {
            return Err(Failure::VendorReferenced {
                vendor_id: vendor_id.to_owned(),
                orders,
            });
        }
        let mut vendors = package.vendors.to_vec();
        let subject = vendors.remove(position);
        plan_candidate_with(
            &package,
            VendorMutationKind::Delete,
            vendors,
            vendor_id,
            Some(subject),
        )
    })
}

/// Every Order naming `vendor_id`: healthy, damaged, and removed; or the failure that kept
/// the removed Orders from being read.
fn references<F: FileSystem>(
    package: &CurrentPackageCandidate,
    vendor_id: &VendorId,
    filesystem: &F,
) -> Result<Vec<Location>, Failure> {
    Ok(Records::of(package)
        .with_removed(&package.context, filesystem)?
        .naming(std::slice::from_ref(vendor_id))
        .iter()
        .map(|holder| holder.directory().clone())
        .collect())
}

fn plan_vendor_visibility<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    kind: VendorMutationKind,
    filesystem: &F,
) -> Result<VendorMutationPlan, Failure> {
    let package = load_package_to_change(
        selected_root,
        &Touched::default().with_vendors(true),
        filesystem,
    )?;
    let position = vendor_position(vendor_id, &package.vendors)?;
    let mut vendors = package.vendors.to_vec();
    // Visible is spelled as absence, so unhiding restores exactly the record hiding
    // replaced and a redundant change compares equal rather than rewriting bytes.
    vendors[position].hidden = matches!(kind, VendorMutationKind::Hide).then_some(true);
    plan_candidate(&package, kind, vendors, vendor_id)
}

fn vendor_position(vendor_id: &str, vendors: &[Vendor]) -> Result<usize, Failure> {
    vendors
        .iter()
        .position(|vendor| vendor.vendor_id.as_str() == vendor_id)
        .ok_or_else(|| Failure::UnknownVendorId {
            vendor_id: vendor_id.to_owned(),
        })
}

fn plan_candidate(
    package: &CurrentPackageCandidate,
    kind: VendorMutationKind,
    vendors: Vec<Vendor>,
    vendor_id: &str,
) -> Result<VendorMutationPlan, Failure> {
    plan_candidate_with(package, kind, vendors, vendor_id, None)
}

/// As [`plan_candidate`], with `removed` the subject a deletion takes out
/// of the aggregate.
fn plan_candidate_with(
    package: &CurrentPackageCandidate,
    kind: VendorMutationKind,
    vendors: Vec<Vendor>,
    vendor_id: &str,
    removed: Option<Vendor>,
) -> Result<VendorMutationPlan, Failure> {
    let vendors = validated_aggregate(package, vendors)?;
    // Every Vendor mutation but a deletion keeps its subject in the aggregate it is
    // planning, so the subject's planned facts are read from that aggregate.
    let subject_document = match removed {
        Some(removed) => removed,
        None => vendors
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id.as_str() == vendor_id)
            .cloned()
            .ok_or(Failure::InvalidAggregate)?,
    };
    // An unchanged aggregate is committed by performing nothing, so a dry run of it
    // must not announce managed operations its commit will never reach.
    let prior_vendors = package.vendors.to_vec();
    let root = &package.context.root.absolute_path;
    let journal = if vendors.vendors == prior_vendors {
        Journal::new(root)
    } else {
        vendors_journal(
            root,
            &prior_record(package, &prior_vendors, &package.values),
            vendors.to_json_bytes().unwrap_or_default(),
            package.context.index.clone(),
        )
    };

    Ok(VendorMutationPlan {
        root: package.context.root.absolute_path.clone(),
        vendors_path: package.context.vendors.clone(),
        kind,
        subject: PlannedVendorFacts {
            document: subject_document,
        },
        vendor_id: vendor_id.to_owned(),
        candidate_vendors: vendors.vendors,
        prior_vendors,
        prior_values: package.values.clone(),
        original_vendor: None,
        journal,
    })
}

/// The Vendor record of `package` as a plan holding `vendors` and `values` was made
/// against.
fn prior_record(
    package: &CurrentPackageCandidate,
    vendors: &[Vendor],
    values: &StoredValues,
) -> ManagedRecord {
    ManagedRecord::vendors(
        package.context.vendors.absolute_path.clone(),
        VendorsDocument {
            schema_version: Version1,
            vendors: vendors.to_vec(),
            values: values.clone(),
        },
    )
}

/// The commit replacing the Vendor record planned as `record` with `candidate`.
///
/// The aggregate revalidated before it is the one the commit is authorized to replace, so
/// a writer landing between that revalidation and the rename is refused rather than
/// overwritten. Its meaning is what is compared, not its bytes: an aggregate whose stored
/// facts carry untrimmed text projects to the same Vendors, and refusing it would mean
/// refusing to edit a file a person reformatted by hand.
fn vendors_journal(
    root: &Path,
    record: &ManagedRecord,
    candidate: Vec<u8>,
    index: Location,
) -> Journal {
    let mut journal = Journal::new(root);
    journal.replace(
        record.path().to_path_buf(),
        candidate,
        record.expectation(),
        PlannedAct::Managed(OperationKind::ReplaceVendorRecord),
        Subject::Vendors,
    );
    journal.refreshing(index);
    journal
}

fn validated_aggregate(
    package: &CurrentPackageCandidate,
    vendors: Vec<Vendor>,
) -> Result<VendorsDocument, Failure> {
    let candidate = VendorsDocument {
        schema_version: Version1,
        vendors,
        values: package.values.clone(),
    };
    let aggregate = Aggregate::judge(&candidate).map_err(|_| Failure::InvalidAggregate)?;
    let substitution = Substitution {
        vendors: Some(&aggregate),
        ..Substitution::default()
    };
    match Records::of(package).broken_by(&substitution) {
        Some(_) => Err(Failure::InvalidAggregate),
        None => Ok(aggregate.into_document()),
    }
}

/// Accepts one order-independent assignment set or reports every rejected field.
/// An empty value is retained here; only an edit interprets it as explicit clearing.
fn accepted_assignments(assignments: &Assignments) -> Result<Vec<(String, String)>, Failure> {
    let (accepted, rejected) = adjudicated_assignments(assignments);
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(Failure::InvalidAssignments(rejected))
    }
}

/// Why a Vendor refuses `field=value` whatever else its set assigns.
pub(crate) fn vendor_assignment_refusal(
    field: &str,
    value: &str,
) -> Option<RejectedAssignmentReason> {
    match field {
        "vendor_id" => Some(RejectedAssignmentReason::ImmutableIdentity),
        "schema_version" => Some(RejectedAssignmentReason::ManagedTechnicalField),
        "contacts" => Some(RejectedAssignmentReason::OrderedCollection),
        field if VendorField::named(field).is_none() => {
            Some(RejectedAssignmentReason::UnknownField)
        }
        "name" if value.trim().is_empty() => Some(RejectedAssignmentReason::BlankRequiredValue),
        _ => None,
    }
}

/// Why a Vendor Contact refuses `field=value` whatever else its set assigns.
pub(crate) fn contact_assignment_refusal(
    field: &str,
    value: &str,
) -> Option<RejectedAssignmentReason> {
    match field {
        field if ContactField::named(field).is_none() => {
            Some(RejectedAssignmentReason::UnknownField)
        }
        "name" if value.trim().is_empty() => Some(RejectedAssignmentReason::BlankRequiredValue),
        _ => None,
    }
}

/// The assignments of one set that broke no rule, and every one that did.
fn adjudicated_assignments(
    assignments: &Assignments,
) -> (Vec<(String, String)>, Vec<RejectedAssignment>) {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String)>::new();
    for Assignment {
        field,
        value,
        confirmed,
    } in &assignments.entries
    {
        let reason = match vendor_assignment_refusal(field, value) {
            Some(RejectedAssignmentReason::BlankRequiredValue) | None
                if accepted.iter().any(|(accepted, _)| accepted == field) =>
            {
                Some(RejectedAssignmentReason::Duplicate)
            }
            // No Vendor or Contact fact is a Reusable Free-Entry Value.
            Some(RejectedAssignmentReason::BlankRequiredValue) | None if *confirmed => {
                Some(RejectedAssignmentReason::NotAReusableValue)
            }
            reason => reason,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone())),
        }
    }
    (accepted, rejected)
}

fn accepted_contact_assignments(
    assignments: &Assignments,
) -> Result<Vec<(String, String)>, Failure> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String)>::new();
    for Assignment {
        field,
        value,
        confirmed,
    } in &assignments.entries
    {
        let reason = match contact_assignment_refusal(field, value) {
            Some(RejectedAssignmentReason::BlankRequiredValue) | None
                if accepted.iter().any(|(accepted, _)| accepted == field) =>
            {
                Some(RejectedAssignmentReason::Duplicate)
            }
            // No Vendor or Contact fact is a Reusable Free-Entry Value.
            Some(RejectedAssignmentReason::BlankRequiredValue) | None if *confirmed => {
                Some(RejectedAssignmentReason::NotAReusableValue)
            }
            reason => reason,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone())),
        }
    }
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(Failure::InvalidAssignments(rejected))
    }
}

/// The one Vendor Name comparison: trimmed, and without regard to letter case.
///
/// Vendor Names are unique within a Package under exactly this comparison, so the same
/// predicate resolves a Name to its Vendor and refuses a duplicate one. Two spellings of
/// it would be two rules: a Name a read resolved and a create still accepted.
pub(crate) fn vendor_names_match(left: &str, right: &str) -> bool {
    focaccia::unicode_full_case_eq(left.trim(), right.trim())
}

/// The one Vendor of a Package carrying this Vendor Name, or `None` where none does.
///
/// At most one can, because Names are package-unique under [`vendor_names_match`] — a
/// Package whose records name one Vendor twice is damaged and never loads as a current
/// candidate. Hidden Vendors are searched like any other: hiding governs what a Package
/// offers for new use and never what resolves.
pub(crate) fn vendor_by_name<'a>(name: &str, vendors: &'a [Vendor]) -> Option<&'a Vendor> {
    vendors
        .iter()
        .find(|vendor| vendor_names_match(name, vendor.name.as_str()))
}

fn reject_duplicate_name(
    name: &str,
    exclude: Option<&VendorId>,
    package: &CurrentPackageCandidate,
) -> Result<(), Failure> {
    package
        .vendors
        .iter()
        .filter(|vendor| exclude != Some(&vendor.vendor_id))
        .find(|vendor| vendor_names_match(name, vendor.name.as_str()))
        .map_or(Ok(()), |vendor| {
            Err(Failure::VendorNameInUse {
                name: vendor.name.as_str().to_owned(),
                vendor_id: vendor.vendor_id.as_str().to_owned(),
                scope: vendors_location(package),
            })
        })
}

/// The Vendor aggregate a Vendor refusal was judged against, under its Root.
pub(crate) fn vendors_location(package: &CurrentPackageCandidate) -> RootedLocation {
    let context = &package.context;
    rooted_at(
        &context.root.absolute_path,
        context.vendors.absolute_path.clone(),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct VendorMutationResult {
    /// The Vendor the mutation saved.
    pub vendor_id: VendorId,
}

pub(crate) type VendorMutationReport = MutationReport<VendorMutationResult>;

pub(crate) fn commit_vendor_mutation<F: FileSystem>(
    plan: VendorMutationPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<VendorMutationReport> {
    Answer::of(|| {
        let current = if plan.kind == VendorMutationKind::Delete {
            load_every_record(&plan.root, filesystem)?
        } else {
            load_package_to_change(
                &plan.root,
                &Touched::default().with_vendors(true),
                filesystem,
            )?
        };
        if plan.kind == VendorMutationKind::Delete
            && !references(&current, &plan.subject.document.vendor_id, filesystem)?.is_empty()
        {
            return Err(preconditions_changed(&plan, ChangedPrecondition::Package));
        }
        if current.vendors != plan.prior_vendors || current.values != plan.prior_values {
            return Err(preconditions_changed(
                &plan,
                ChangedPrecondition::VendorRecord,
            ));
        }
        if plan.original_vendor.as_ref().is_some_and(|original| {
            original.vendor_id.as_str() != plan.vendor_id
                || current
                    .vendors
                    .iter()
                    .find(|vendor| vendor.vendor_id == original.vendor_id)
                    != Some(original)
        }) {
            return Err(preconditions_changed(
                &plan,
                ChangedPrecondition::VendorRecord,
            ));
        }

        let mut vendors = plan.candidate_vendors.clone();
        let subject_vendor_id = match plan.kind {
            VendorMutationKind::Create => {
                let generated = new_uuid();
                let Some(identity) = parsed_vendor_id(&generated) else {
                    return Err(Failure::InvalidGeneratedIdentity { generated });
                };
                if current
                    .vendors
                    .iter()
                    .any(|vendor| vendor.vendor_id == identity)
                {
                    return Err(Failure::GeneratedVendorIdentityCollision {
                        vendor_id: generated,
                    });
                }
                let subject = vendors
                    .iter_mut()
                    .find(|vendor| vendor.vendor_id.as_str() == plan.vendor_id)
                    .expect("the planned aggregate contains its own created Vendor");
                subject.vendor_id = identity;
                generated
            }
            VendorMutationKind::Edit
            | VendorMutationKind::Hide
            | VendorMutationKind::Unhide
            | VendorMutationKind::Delete => plan.vendor_id.clone(),
        };

        let Ok(vendors) = validated_aggregate(&current, vendors) else {
            return Err(preconditions_changed(&plan, ChangedPrecondition::Package));
        };
        let Ok(candidate) = vendors.to_json_bytes() else {
            return Err(preconditions_changed(
                &plan,
                ChangedPrecondition::VendorRecord,
            ));
        };
        // A deletion's subject is gone from the aggregate; its identity is the one planned.
        let committed_vendor = match plan.kind {
            VendorMutationKind::Delete => plan.subject.document.vendor_id.clone(),
            _ => {
                vendors
                    .vendors
                    .iter()
                    .find(|vendor| vendor.vendor_id.as_str() == subject_vendor_id)
                    .cloned()
                    .expect("the validated aggregate retains its own subject Vendor")
                    .vendor_id
            }
        };

        if !plan.changes_canonical_vendors() {
            let result = VendorMutationResult {
                vendor_id: committed_vendor,
            };
            return Ok(MutationReport::Unchanged { result });
        }
        let record = prior_record(&current, &plan.prior_vendors, &plan.prior_values);
        let committed = vendors_journal(
            &plan.root,
            &record,
            candidate,
            current.context.index.clone(),
        )
        .run(filesystem)?;
        let refresh = committed.refresh(&plan.root, filesystem);
        Ok(MutationReport::committed(
            VendorMutationResult {
                vendor_id: committed_vendor,
            },
            committed.warnings(refresh),
        ))
    })
}

/// Which observed precondition stopped a commit before its first write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangedPrecondition {
    Package,
    VendorRecord,
}

fn preconditions_changed(plan: &VendorMutationPlan, changed: ChangedPrecondition) -> Failure {
    let path = match changed {
        ChangedPrecondition::Package => plan.root.clone(),
        ChangedPrecondition::VendorRecord => plan.vendors_path.absolute_path.clone(),
    };
    Failure::PreconditionsChanged {
        location: located_in_package(&plan.root, path),
    }
}

fn parsed_vendor_id(generated: &str) -> Option<VendorId> {
    serde_json::from_value(serde_json::Value::String(generated.to_owned())).ok()
}
