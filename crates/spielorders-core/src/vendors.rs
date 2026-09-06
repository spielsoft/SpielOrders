//! Planning and persistence for the complete Vendor aggregate lifecycle.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, DamagedPackageProjection, HealthyOrder,
        OperationFailedProjection, PackageIndexRecord, PackageLoad, UnmanagedPackage,
        UpgradeProjection, inspect_canonical_package, open_package,
    },
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, replace_validated_file_expecting,
    },
    metadata::{
        DateOrdered, NonBlankString, OrderDocument, OrderId, OrderStatus, Vendor, VendorContact,
        VendorId, VendorsDocument, Version1, VersionDocument,
    },
    mutation::{MutationReport, RejectedAssignment, RejectedAssignmentReason},
    operations::{FailedPath, OperationFailure},
    package::PackageMetadata,
    ports::{FileSystem, UuidGenerator},
    readme::{Location, located_in_package},
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_VENDOR_ID: &str = "00000000-0000-4000-8000-000000000000";

const ASSIGNABLE_FIELDS: [&str; 6] = [
    "name",
    "phone",
    "email",
    "website",
    "address",
    "purchasing_notes",
];

/// One order-independent canonical `field=value` assignment set for one Vendor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VendorAssignments {
    entries: Vec<(String, String)>,
}

impl VendorAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.push((field.into(), value.into()));
        self
    }
}

/// One order-independent canonical `field=value` assignment set for one Contact.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContactAssignments {
    entries: Vec<(String, String)>,
}

impl ContactAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.push((field.into(), value.into()));
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedVendorIdentity {
    GeneratedAtCommit,
    Retained(VendorId),
}

/// One Vendor's planned shared facts. The created Vendor's identity is generated
/// only at commit, so identity is reported separately from the facts.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedVendorFacts {
    identity: PlannedVendorIdentity,
    document: Vendor,
}

impl PlannedVendorFacts {
    pub fn identity(&self) -> &PlannedVendorIdentity {
        &self.identity
    }

    /// The Vendor ID these facts will carry, where the mutation is not minting one.
    ///
    /// `None` for a creation, whose identity is generated at the commit. A caller
    /// reporting planned facts therefore has no identity to report, which is the honest
    /// answer: the placeholder the planned candidate carries is not the Vendor ID the
    /// commit will mint, and this keeps it out of reach.
    pub fn vendor_id(&self) -> Option<&str> {
        match &self.identity {
            PlannedVendorIdentity::GeneratedAtCommit => None,
            PlannedVendorIdentity::Retained(vendor_id) => Some(vendor_id.as_str()),
        }
    }

    pub fn name(&self) -> &str {
        self.document.name.as_str()
    }

    pub fn phone(&self) -> Option<&str> {
        self.document.phone.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.document.email.as_deref()
    }

    pub fn website(&self) -> Option<&str> {
        self.document.website.as_deref()
    }

    pub fn address(&self) -> Option<&str> {
        self.document.address.as_deref()
    }

    pub fn purchasing_notes(&self) -> Option<&str> {
        self.document.purchasing_notes.as_deref()
    }

    pub fn contacts(&self) -> &[VendorContact] {
        &self.document.contacts
    }

    /// Whether this Vendor would be a Hidden Vendor after the plan commits.
    pub fn hidden(&self) -> bool {
        self.document.is_hidden()
    }
}

/// One live Order that references the subject Vendor by Vendor ID.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferencingOrderSummary {
    order_id: OrderId,
    title: NonBlankString,
    date_ordered: DateOrdered,
    status: OrderStatus,
    directory: Location,
}

impl ReferencingOrderSummary {
    pub fn order_id(&self) -> &str {
        self.order_id.as_str()
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn date_ordered(&self) -> &str {
        self.date_ordered.as_str()
    }

    pub fn status(&self) -> OrderStatus {
        self.status
    }

    /// The Order Directory, as the load located it — so a caller reporting it carries the
    /// Package-relative spelling as well as the absolute path.
    pub fn directory(&self) -> &Location {
        &self.directory
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorMutationOperationKind {
    ReplaceVendorRecord,
    RefreshPackageIndex,
}

impl VendorMutationOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceVendorRecord => "replace_vendor_record",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VendorMutationOperation {
    kind: VendorMutationOperationKind,
    location: Location,
}

impl VendorMutationOperation {
    pub fn kind(&self) -> VendorMutationOperationKind {
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

/// A Vendor is never deleted. The visibility kinds are what replaced deletion: they
/// govern whether a Vendor is offered for new use, never whether it resolves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorMutationKind {
    Create,
    Edit,
    Hide,
    Unhide,
}

impl VendorMutationKind {
    /// What committing a mutation of this kind does to the canonical Vendor record.
    ///
    /// One mapping rather than one per caller, because a plan and the result of
    /// committing it must not disagree about what the mutation was: a dry run has no
    /// result to read the outcome off, and reads it here instead.
    pub fn canonical_outcome(self) -> CanonicalVendorOutcome {
        match self {
            Self::Create => CanonicalVendorOutcome::Created,
            Self::Edit => CanonicalVendorOutcome::Edited,
            Self::Hide => CanonicalVendorOutcome::Hidden,
            Self::Unhide => CanonicalVendorOutcome::Unhidden,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VendorMutationPlan {
    root: PathBuf,
    vendors_path: PathBuf,
    kind: VendorMutationKind,
    subject: PlannedVendorFacts,
    subject_vendor_id: String,
    planned_vendors: Vec<PlannedVendorFacts>,
    candidate_vendors: Vec<Vendor>,
    prior_vendors: Vec<Vendor>,
    referencing_orders: Vec<ReferencingOrderSummary>,
    prior_index: PackageIndexRecord,
    operations: Vec<VendorMutationOperation>,
}

/// Which end of a Contact mutation a position names.
///
/// A `move` addresses two: the Contact it takes, and where that Contact goes. The others
/// address one — the pre-mutation entry for an `edit` or a `remove`, and the insertion
/// point for an `add` — so a refused position says which end it was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactPositionRole {
    Source,
    Destination,
}

impl ContactPositionRole {
    /// The stable spelling of one role.
    ///
    /// Beside the role rather than in an adapter, for the reason
    /// [`VendorMutationWarning::as_str`] carries the same rule: a position refused by the
    /// CLI and the same position refused for the desktop shell cannot come to be named
    /// differently.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source_position",
            Self::Destination => "destination_position",
        }
    }
}

impl VendorMutationPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn vendors_path(&self) -> &Path {
        &self.vendors_path
    }

    pub fn kind(&self) -> VendorMutationKind {
        self.kind
    }

    /// The created, edited, hidden, or unhidden Vendor itself.
    pub fn subject(&self) -> &PlannedVendorFacts {
        &self.subject
    }

    /// The complete validated post-mutation Vendor aggregate in presentation order.
    pub fn planned_vendors(&self) -> &[PlannedVendorFacts] {
        &self.planned_vendors
    }

    pub fn referencing_orders(&self) -> &[ReferencingOrderSummary] {
        &self.referencing_orders
    }

    pub fn operations(&self) -> &[VendorMutationOperation] {
        &self.operations
    }

    /// Whether committing this plan would change the canonical Vendor aggregate.
    /// A dry run of an unchanged mutation reports the same result a commit does.
    pub fn changes_canonical_vendors(&self) -> bool {
        self.candidate_vendors != self.prior_vendors
    }

    /// The derived Package Index as it stood when this mutation was planned.
    ///
    /// The state a run that never commits has to report: committing refreshes the index
    /// and [`VendorMutationResult::index`] is that refreshed record, so a dry run has
    /// none of its own and answers with what it observed instead.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        &self.prior_index
    }
}

/// Why a Vendor capability refused a request before performing any write. Every
/// planner and the live read share these reasons; none of them changes a byte.
///
/// **The four Package-load states travel as the core's own projections**, for the reason
/// [`crate::vendor_reads::VendorReadFailure`] carries them: a caller reporting a refused
/// mutation classifies the Package exactly as a Package-level read does, rather than
/// folding every one of them into "unreadable" — and a damaged Package's diagnosis is
/// what a refusal on those grounds is *for*, since `repair` is the action behind it.
#[derive(Debug, PartialEq)]
pub enum VendorCapabilityError<E> {
    /// The exact path given is a resolved directory and is not a SpielOrders Root.
    UnmanagedPackage(Box<UnmanagedPackage>),
    /// The Package could be read and not trusted. `root` is the best name this refusal
    /// has for the Root it loaded, which is all there is to report where the damage left
    /// the Package with no managed paths of its own.
    DamagedPackage {
        root: FailedPath,
        projection: Box<DamagedPackageProjection>,
    },
    /// The Package is mid-upgrade or awaiting one.
    UpgradePending(Box<UpgradeProjection>),
    /// The Package could not be read past its Root.
    PackageOperationFailed(Box<OperationFailedProjection>),
    UnknownVendor {
        vendor_id: String,
    },
    InvalidContactPosition {
        role: ContactPositionRole,
        position: usize,
        minimum: usize,
        maximum: usize,
    },
    MissingRequiredName,
    InvalidAssignments(Vec<RejectedAssignment>),
    DuplicateName {
        vendor_id: String,
    },
    InvalidAggregate,
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

/// One Vendor's current shared facts together with the Orders that reference it.
/// Every fact resolves live; an Order retains no historical Vendor snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct VendorProjection {
    vendor: Vendor,
    referencing_orders: Vec<ReferencingOrderSummary>,
}

impl VendorProjection {
    pub fn vendor(&self) -> &Vendor {
        &self.vendor
    }

    pub fn referencing_orders(&self) -> &[ReferencingOrderSummary] {
        &self.referencing_orders
    }
}

pub fn show_vendor<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Result<VendorProjection, VendorCapabilityError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    let position = vendor_position(vendor_id, package.vendors())?;

    Ok(VendorProjection {
        vendor: package.vendors()[position].clone(),
        referencing_orders: referencing_orders(vendor_id, package.healthy_orders()),
    })
}

/// Lists one Vendor's embedded Contacts in their exact user-controlled order,
/// together with the complete owning Vendor and its live referencing Orders.
pub fn list_vendor_contacts<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Result<VendorProjection, VendorCapabilityError<F::Error>> {
    show_vendor(selected_root, vendor_id, filesystem)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorContactMutationKind {
    Add,
    Edit,
    Move,
    Remove,
}

impl VendorContactMutationKind {
    /// What committing a mutation of this kind does to the addressed Contact.
    ///
    /// One mapping rather than one per caller, for the reason
    /// [`VendorMutationKind::canonical_outcome`] has one: a dry run has no result to read
    /// the outcome off, so it reads it here, and a plan and the result of committing it
    /// cannot come to disagree about what the mutation was. Every Contact mutation
    /// replaces the owning Vendor record, so the *Vendor's* canonical outcome is `edited`
    /// whichever of these four ran; this is the Contact's own.
    pub fn canonical_outcome(self) -> CanonicalContactOutcome {
        match self {
            Self::Add => CanonicalContactOutcome::Added,
            Self::Edit => CanonicalContactOutcome::Edited,
            Self::Move => CanonicalContactOutcome::Moved,
            Self::Remove => CanonicalContactOutcome::Removed,
        }
    }
}

/// What committing one Contact mutation does to the addressed Contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalContactOutcome {
    Added,
    Edited,
    Moved,
    Removed,
}

impl CanonicalContactOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Edited => "edited",
            Self::Moved => "moved",
            Self::Removed => "removed",
        }
    }
}

/// A complete, validated mutation of one Vendor's embedded Contact collection.
#[derive(Clone, Debug, PartialEq)]
pub struct VendorContactMutationPlan {
    kind: VendorContactMutationKind,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    subject_contact: VendorContact,
    vendor_plan: VendorMutationPlan,
}

impl VendorContactMutationPlan {
    pub fn root(&self) -> &Path {
        self.vendor_plan.root()
    }

    pub fn vendors_path(&self) -> &Path {
        self.vendor_plan.vendors_path()
    }

    pub fn kind(&self) -> VendorContactMutationKind {
        self.kind
    }

    pub fn source_position(&self) -> Option<usize> {
        self.source_position
    }

    pub fn destination_position(&self) -> Option<usize> {
        self.destination_position
    }

    pub fn subject_contact(&self) -> &VendorContact {
        &self.subject_contact
    }

    pub fn vendor(&self) -> &PlannedVendorFacts {
        self.vendor_plan.subject()
    }

    pub fn referencing_orders(&self) -> &[ReferencingOrderSummary] {
        self.vendor_plan.referencing_orders()
    }

    pub fn operations(&self) -> &[VendorMutationOperation] {
        self.vendor_plan.operations()
    }

    pub fn changes_canonical_vendors(&self) -> bool {
        self.vendor_plan.changes_canonical_vendors()
    }

    /// The derived Package Index as it stood when this mutation was planned. See
    /// [`VendorMutationPlan::prior_index`], whose record this is.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        self.vendor_plan.prior_index()
    }
}

pub type VendorContactMutationReport<E> = MutationReport<
    VendorContactMutationPlan,
    VendorMutationResult,
    VendorMutationRejection<E>,
    VendorMutationWarning,
>;

/// Commits one already validated Contact plan. Contact mutations retain the
/// owning Vendor's identity, so this capability needs no UUID effect.
pub fn commit_vendor_contact_mutation<F: FileSystem>(
    plan: VendorContactMutationPlan,
    filesystem: &F,
) -> VendorContactMutationReport<F::Error> {
    struct UnreachableUuid;

    impl UuidGenerator for UnreachableUuid {
        fn generate_v4(&mut self) -> String {
            unreachable!("a Contact mutation retains its owning Vendor identity")
        }
    }

    let VendorContactMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_contact,
        vendor_plan,
    } = plan;
    let wrap = |vendor_plan| VendorContactMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_contact,
        vendor_plan,
    };
    match commit_vendor_mutation(vendor_plan, &mut UnreachableUuid, filesystem) {
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

pub fn plan_vendor_contact_add<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    assignments: &ContactAssignments,
    position: Option<usize>,
    filesystem: &F,
) -> Result<VendorContactMutationPlan, VendorCapabilityError<F::Error>> {
    let (package, vendor_position) = current_contact_target(selected_root, vendor_id, filesystem)?;
    let contact_count = package.vendors()[vendor_position].contacts.len();
    let destination_position = position.unwrap_or(contact_count + 1);
    let destination_index = contact_index(
        ContactPositionRole::Destination,
        destination_position,
        contact_count + 1,
    )?;
    let mut facts = accepted_contact_assignments(assignments)?;
    facts.retain(|(_, value)| !value.trim().is_empty());
    if !facts.iter().any(|(field, _)| field == "name") {
        return Err(VendorCapabilityError::MissingRequiredName);
    }
    let mut object = serde_json::Map::new();
    for (field, value) in facts {
        object.insert(field, serde_json::Value::String(value));
    }
    let contact = serde_json::from_value::<VendorContact>(serde_json::Value::Object(object))
        .map_err(|_| VendorCapabilityError::InvalidAggregate)?;
    let mut vendors = package.vendors().to_vec();
    vendors[vendor_position]
        .contacts
        .insert(destination_index, contact);
    let vendor_plan = contact_vendor_plan(&package, vendor_position, vendor_id, vendors)?;
    let subject_contact = vendor_plan.subject().contacts()[destination_index].clone();
    Ok(contact_mutation_plan(
        VendorContactMutationKind::Add,
        None,
        Some(destination_position),
        subject_contact,
        vendor_plan,
    ))
}

pub fn plan_vendor_contact_edit<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    position: usize,
    assignments: &ContactAssignments,
    filesystem: &F,
) -> Result<VendorContactMutationPlan, VendorCapabilityError<F::Error>> {
    let (package, vendor_position) = current_contact_target(selected_root, vendor_id, filesystem)?;
    let contact_count = package.vendors()[vendor_position].contacts.len();
    let contact_index = contact_index(ContactPositionRole::Source, position, contact_count)?;
    let assignments = accepted_contact_assignments(assignments)?;
    let existing = &package.vendors()[vendor_position].contacts[contact_index];
    let mut object = serde_json::to_value(existing)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(VendorCapabilityError::InvalidAggregate)?;
    for (field, value) in assignments {
        if value.trim().is_empty() {
            object.remove(&field);
        } else {
            object.insert(field, serde_json::Value::String(value));
        }
    }
    let contact = serde_json::from_value::<VendorContact>(serde_json::Value::Object(object))
        .map_err(|_| VendorCapabilityError::InvalidAggregate)?;
    let mut vendors = package.vendors().to_vec();
    vendors[vendor_position].contacts[contact_index] = contact;
    let vendor_plan = contact_vendor_plan(&package, vendor_position, vendor_id, vendors)?;
    let subject_contact = vendor_plan.subject().contacts()[contact_index].clone();
    Ok(contact_mutation_plan(
        VendorContactMutationKind::Edit,
        Some(position),
        Some(position),
        subject_contact,
        vendor_plan,
    ))
}

pub fn plan_vendor_contact_move<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    source_position: usize,
    destination_position: usize,
    filesystem: &F,
) -> Result<VendorContactMutationPlan, VendorCapabilityError<F::Error>> {
    let (package, vendor_position) = current_contact_target(selected_root, vendor_id, filesystem)?;
    let contact_count = package.vendors()[vendor_position].contacts.len();
    let source_index = contact_index(ContactPositionRole::Source, source_position, contact_count)?;
    let destination_index = contact_index(
        ContactPositionRole::Destination,
        destination_position,
        contact_count,
    )?;
    let mut vendors = package.vendors().to_vec();
    let contact = vendors[vendor_position].contacts.remove(source_index);
    vendors[vendor_position]
        .contacts
        .insert(destination_index, contact);
    let vendor_plan = contact_vendor_plan(&package, vendor_position, vendor_id, vendors)?;
    let subject_contact = vendor_plan.subject().contacts()[destination_index].clone();
    Ok(contact_mutation_plan(
        VendorContactMutationKind::Move,
        Some(source_position),
        Some(destination_position),
        subject_contact,
        vendor_plan,
    ))
}

pub fn plan_vendor_contact_remove<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    position: usize,
    filesystem: &F,
) -> Result<VendorContactMutationPlan, VendorCapabilityError<F::Error>> {
    let (package, vendor_position) = current_contact_target(selected_root, vendor_id, filesystem)?;
    let contact_count = package.vendors()[vendor_position].contacts.len();
    let contact_index = contact_index(ContactPositionRole::Source, position, contact_count)?;
    let mut vendors = package.vendors().to_vec();
    let subject_contact = vendors[vendor_position].contacts.remove(contact_index);
    let vendor_plan = contact_vendor_plan(&package, vendor_position, vendor_id, vendors)?;
    Ok(contact_mutation_plan(
        VendorContactMutationKind::Remove,
        Some(position),
        None,
        subject_contact,
        vendor_plan,
    ))
}

fn current_contact_target<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Result<(CurrentPackageCandidate, usize), VendorCapabilityError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    let vendor_position = vendor_position(vendor_id, package.vendors())?;
    Ok((package, vendor_position))
}

fn contact_index<E>(
    role: ContactPositionRole,
    position: usize,
    maximum: usize,
) -> Result<usize, VendorCapabilityError<E>> {
    if position == 0 || position > maximum {
        Err(VendorCapabilityError::InvalidContactPosition {
            role,
            position,
            minimum: 1,
            maximum,
        })
    } else {
        Ok(position - 1)
    }
}

fn contact_vendor_plan<E>(
    package: &CurrentPackageCandidate,
    vendor_position: usize,
    vendor_id: &str,
    vendors: Vec<Vendor>,
) -> Result<VendorMutationPlan, VendorCapabilityError<E>> {
    plan_from_candidate_aggregate(
        package,
        VendorMutationKind::Edit,
        vendors,
        vendor_id,
        PlannedVendorIdentity::Retained(package.vendors()[vendor_position].vendor_id.clone()),
    )
}

fn contact_mutation_plan(
    kind: VendorContactMutationKind,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    subject_contact: VendorContact,
    vendor_plan: VendorMutationPlan,
) -> VendorContactMutationPlan {
    VendorContactMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_contact,
        vendor_plan,
    }
}

pub fn plan_vendor_creation<F: FileSystem>(
    selected_root: &Path,
    assignments: &VendorAssignments,
    filesystem: &F,
) -> Result<VendorMutationPlan, VendorCapabilityError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    // A blank optional value means absence on creation, so nothing is left to clear.
    // A blank Name never reaches here; `accepted_assignments` already rejected it.
    let mut facts = accepted_assignments(assignments)?;
    facts.retain(|(_, value)| !value.trim().is_empty());
    let name = facts
        .iter()
        .find(|(field, _)| field == "name")
        .map(|(_, value)| value.as_str())
        .ok_or(VendorCapabilityError::MissingRequiredName)?;
    reject_duplicate_name(name, None, package.vendors())?;

    let mut object = json!({ "vendor_id": PLANNING_VENDOR_ID, "contacts": [] });
    let target = object
        .as_object_mut()
        .expect("the planned Vendor candidate is an object");
    for (field, value) in &facts {
        target.insert(field.clone(), serde_json::Value::String(value.clone()));
    }
    let mut vendors = package.vendors().to_vec();
    vendors.push(
        serde_json::from_value::<Vendor>(object)
            .map_err(|_| VendorCapabilityError::InvalidAggregate)?,
    );

    plan_from_candidate_aggregate(
        &package,
        VendorMutationKind::Create,
        vendors,
        PLANNING_VENDOR_ID,
        PlannedVendorIdentity::GeneratedAtCommit,
    )
}

pub fn plan_vendor_edit<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    assignments: &VendorAssignments,
    filesystem: &F,
) -> Result<VendorMutationPlan, VendorCapabilityError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    let position = vendor_position(vendor_id, package.vendors())?;
    let existing = &package.vendors()[position];
    let assignments = accepted_assignments(assignments)?;
    let mut object = serde_json::to_value(existing)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(VendorCapabilityError::InvalidAggregate)?;
    for (field, value) in &assignments {
        if value.trim().is_empty() {
            object.remove(field);
        } else {
            object.insert(field.clone(), serde_json::Value::String(value.clone()));
        }
    }
    let edited = serde_json::from_value::<Vendor>(serde_json::Value::Object(object))
        .map_err(|_| VendorCapabilityError::InvalidAggregate)?;
    reject_duplicate_name(
        edited.name.as_str(),
        Some(&existing.vendor_id),
        package.vendors(),
    )?;

    let mut vendors = package.vendors().to_vec();
    vendors[position] = edited;
    plan_from_candidate_aggregate(
        &package,
        VendorMutationKind::Edit,
        vendors,
        vendor_id,
        PlannedVendorIdentity::Retained(package.vendors()[position].vendor_id.clone()),
    )
}

/// Stops offering one Vendor for new use. Its record and its Vendor ID survive, so every
/// Order referencing it keeps resolving; hiding is never refused on reference grounds.
pub fn plan_vendor_hide<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Result<VendorMutationPlan, VendorCapabilityError<F::Error>> {
    plan_vendor_visibility(
        selected_root,
        vendor_id,
        VendorMutationKind::Hide,
        filesystem,
    )
}

/// Offers a Hidden Vendor for new use again. Hiding is a reversible toggle.
pub fn plan_vendor_unhide<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    filesystem: &F,
) -> Result<VendorMutationPlan, VendorCapabilityError<F::Error>> {
    plan_vendor_visibility(
        selected_root,
        vendor_id,
        VendorMutationKind::Unhide,
        filesystem,
    )
}

fn plan_vendor_visibility<F: FileSystem>(
    selected_root: &Path,
    vendor_id: &str,
    kind: VendorMutationKind,
    filesystem: &F,
) -> Result<VendorMutationPlan, VendorCapabilityError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    let position = vendor_position(vendor_id, package.vendors())?;
    let mut vendors = package.vendors().to_vec();
    let identity = vendors[position].vendor_id.clone();
    // Visible is spelled as absence, so unhiding restores exactly the record hiding
    // replaced and a redundant change compares equal rather than rewriting bytes.
    vendors[position].hidden = matches!(kind, VendorMutationKind::Hide).then_some(true);
    plan_from_candidate_aggregate(
        &package,
        kind,
        vendors,
        vendor_id,
        PlannedVendorIdentity::Retained(identity),
    )
}

fn vendor_position<E>(
    vendor_id: &str,
    vendors: &[Vendor],
) -> Result<usize, VendorCapabilityError<E>> {
    vendors
        .iter()
        .position(|vendor| vendor.vendor_id.as_str() == vendor_id)
        .ok_or_else(|| VendorCapabilityError::UnknownVendor {
            vendor_id: vendor_id.to_owned(),
        })
}

fn plan_from_candidate_aggregate<E>(
    package: &CurrentPackageCandidate,
    kind: VendorMutationKind,
    vendors: Vec<Vendor>,
    subject_vendor_id: &str,
    subject_identity: PlannedVendorIdentity,
) -> Result<VendorMutationPlan, VendorCapabilityError<E>> {
    let vendors = validated_aggregate(package, vendors)?;
    // Every Vendor mutation keeps its subject in the aggregate it is planning, so the
    // subject's planned facts are read from that aggregate.
    let subject_document = vendors
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id.as_str() == subject_vendor_id)
        .cloned()
        .ok_or(VendorCapabilityError::InvalidAggregate)?;
    // An unchanged aggregate is committed by performing nothing, so a dry run of it
    // must not announce managed operations its commit will never reach.
    let prior_vendors = package.vendors().to_vec();
    let operations = if vendors.vendors == prior_vendors {
        Vec::new()
    } else {
        vec![
            VendorMutationOperation {
                kind: VendorMutationOperationKind::ReplaceVendorRecord,
                location: package.context().vendors().clone(),
            },
            VendorMutationOperation {
                kind: VendorMutationOperationKind::RefreshPackageIndex,
                location: package.context().index().clone(),
            },
        ]
    };

    Ok(VendorMutationPlan {
        root: package.context().root().absolute_path().to_path_buf(),
        vendors_path: package.context().vendors().absolute_path().to_path_buf(),
        kind,
        subject: PlannedVendorFacts {
            identity: subject_identity.clone(),
            document: subject_document,
        },
        subject_vendor_id: subject_vendor_id.to_owned(),
        planned_vendors: vendors
            .vendors
            .iter()
            .map(|vendor| PlannedVendorFacts {
                identity: if vendor.vendor_id.as_str() == subject_vendor_id {
                    subject_identity.clone()
                } else {
                    PlannedVendorIdentity::Retained(vendor.vendor_id.clone())
                },
                document: vendor.clone(),
            })
            .collect(),
        candidate_vendors: vendors.vendors,
        prior_vendors,
        referencing_orders: referencing_orders(subject_vendor_id, package.healthy_orders()),
        prior_index: package.index().clone(),
        operations,
    })
}

fn validated_aggregate<E>(
    package: &CurrentPackageCandidate,
    vendors: Vec<Vendor>,
) -> Result<VendorsDocument, VendorCapabilityError<E>> {
    let version = VersionDocument {
        metadata_version: Version1,
    };
    let orders = package
        .healthy_orders()
        .iter()
        .map(|order| order.document().clone())
        .collect::<Vec<OrderDocument>>();
    let candidate = VendorsDocument {
        schema_version: Version1,
        vendors,
    };
    PackageMetadata::new(&version, &orders, &candidate)
        .validate()
        .map(|validated| validated.vendors().clone())
        .map_err(|_| VendorCapabilityError::InvalidAggregate)
}

/// The live Orders resolving to one Vendor, in the load's own Order order.
///
/// The one place a Vendor Reference is matched, shared by every Vendor capability that
/// reports what a Vendor is used by — the reads here, the mutation plans, and their
/// results. A second spelling of it would be a second answer to "what references this
/// Vendor", which is the fact a user checks before hiding one.
pub(crate) fn referencing_orders(
    vendor_id: &str,
    orders: &[HealthyOrder],
) -> Vec<ReferencingOrderSummary> {
    orders
        .iter()
        .filter(|order| {
            order
                .document()
                .vendor_id
                .as_ref()
                .is_some_and(|reference| reference.as_str() == vendor_id)
        })
        .map(|order| ReferencingOrderSummary {
            order_id: order.document().order_id.clone(),
            title: order.document().title.clone(),
            date_ordered: order.document().date_ordered.clone(),
            status: order.document().status,
            directory: order.paths().directory().clone(),
        })
        .collect()
}

/// Accepts one order-independent assignment set or reports every rejected field.
/// An empty value is retained here; only an edit interprets it as explicit clearing.
fn accepted_assignments<E>(
    assignments: &VendorAssignments,
) -> Result<Vec<(String, String)>, VendorCapabilityError<E>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String)>::new();
    for (field, value) in &assignments.entries {
        let reason = match field.as_str() {
            "vendor_id" => Some(RejectedAssignmentReason::ImmutableIdentity),
            "schema_version" => Some(RejectedAssignmentReason::ManagedTechnicalField),
            "contacts" => Some(RejectedAssignmentReason::OrderedCollection),
            field if !ASSIGNABLE_FIELDS.contains(&field) => {
                Some(RejectedAssignmentReason::UnknownField)
            }
            _ if accepted.iter().any(|(accepted, _)| accepted == field) => {
                Some(RejectedAssignmentReason::Duplicate)
            }
            "name" if value.trim().is_empty() => Some(RejectedAssignmentReason::BlankRequiredValue),
            _ => None,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone())),
        }
    }
    if !rejected.is_empty() {
        return Err(VendorCapabilityError::InvalidAssignments(rejected));
    }
    Ok(accepted)
}

fn accepted_contact_assignments<E>(
    assignments: &ContactAssignments,
) -> Result<Vec<(String, String)>, VendorCapabilityError<E>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String)>::new();
    for (field, value) in &assignments.entries {
        let reason = match field.as_str() {
            field if !["name", "role", "phone", "email"].contains(&field) => {
                Some(RejectedAssignmentReason::UnknownField)
            }
            _ if accepted.iter().any(|(accepted, _)| accepted == field) => {
                Some(RejectedAssignmentReason::Duplicate)
            }
            "name" if value.trim().is_empty() => Some(RejectedAssignmentReason::BlankRequiredValue),
            _ => None,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone())),
        }
    }
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(VendorCapabilityError::InvalidAssignments(rejected))
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

fn reject_duplicate_name<E>(
    name: &str,
    exclude: Option<&VendorId>,
    vendors: &[Vendor],
) -> Result<(), VendorCapabilityError<E>> {
    vendors
        .iter()
        .filter(|vendor| exclude != Some(&vendor.vendor_id))
        .find(|vendor| vendor_names_match(name, vendor.name.as_str()))
        .map_or(Ok(()), |vendor| {
            Err(VendorCapabilityError::DuplicateName {
                vendor_id: vendor.vendor_id.as_str().to_owned(),
            })
        })
}

fn current_package<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, VendorCapabilityError<F::Error>> {
    match inspect_canonical_package(selected_root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => Ok(package),
        CanonicalPackageLoad::Unmanaged(package) => {
            Err(VendorCapabilityError::UnmanagedPackage(Box::new(package)))
        }
        CanonicalPackageLoad::DiagnosticOnly(package) => {
            Err(VendorCapabilityError::DamagedPackage {
                root: FailedPath::of(selected_root),
                projection: Box::new(package),
            })
        }
        CanonicalPackageLoad::UpgradePending(package) => {
            Err(VendorCapabilityError::UpgradePending(Box::new(package)))
        }
        CanonicalPackageLoad::OperationFailed(package) => Err(
            VendorCapabilityError::PackageOperationFailed(Box::new(package)),
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalVendorOutcome {
    Created,
    Edited,
    Hidden,
    Unhidden,
}

impl CanonicalVendorOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Edited => "edited",
            Self::Hidden => "hidden",
            Self::Unhidden => "unhidden",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VendorMutationResult {
    outcome: CanonicalVendorOutcome,
    vendor: Vendor,
    referencing_orders: Vec<ReferencingOrderSummary>,
    index: PackageIndexRecord,
}

impl VendorMutationResult {
    pub fn outcome(&self) -> CanonicalVendorOutcome {
        self.outcome
    }

    /// The refreshed Vendor, including its visibility after the mutation.
    pub fn vendor(&self) -> &Vendor {
        &self.vendor
    }

    pub fn referencing_orders(&self) -> &[ReferencingOrderSummary] {
        &self.referencing_orders
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

/// A Vendor mutation's candidate is the aggregate `validated_aggregate` returned, and
/// that runs `PackageMetadata::validate` and refuses with
/// `VendorCapabilityError::InvalidAggregate` when it fails at plan time. The commit
/// revalidates against the current Package and refuses there too, as
/// `preconditions_changed` rather than under the plan-time name. There is no way to
/// reach a commit holding an aggregate that was never validated, so no candidate
/// validation failure is possible, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum VendorMutationRejection<E> {
    /// **A `Location` rather than a path**, because this refusal happens under a Root the
    /// plan established and a response owes that Root's spelling for what it names — in a
    /// refusal exactly as in a success. Handing back a bare path is what left every
    /// adapter to re-derive the spelling or drop it.
    PreconditionsChanged {
        location: Location,
    },
    InvalidGeneratedIdentity {
        generated: String,
    },
    GeneratedIdentityCollision {
        vendor_id: String,
    },
    ReplacementFailed {
        error: ValidatedReplacementError<E, VendorCandidateFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VendorMutationWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

impl VendorMutationWarning {
    /// The stable spelling of one warning a committed Vendor mutation can carry.
    ///
    /// Beside the warning rather than in an adapter, so a warning reported by the CLI
    /// and the same warning shown by the desktop shell cannot come to be named
    /// differently. Neither says the mutation failed: the canonical record was
    /// replaced, and what did not happen is the derived refresh.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IndexRefreshFailed(_) => "index_refresh_failed",
            Self::RefreshUnavailable => "refresh_unavailable",
        }
    }
}

pub type VendorMutationReport<E> = MutationReport<
    VendorMutationPlan,
    VendorMutationResult,
    VendorMutationRejection<E>,
    VendorMutationWarning,
>;

pub fn commit_vendor_mutation<F: FileSystem>(
    plan: VendorMutationPlan,
    uuid_generator: &mut impl UuidGenerator,
    filesystem: &F,
) -> VendorMutationReport<F::Error> {
    let current = match inspect_canonical_package(&plan.root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        _ => return preconditions_changed(plan, ChangedPrecondition::Package),
    };
    if current.vendors() != plan.prior_vendors {
        return preconditions_changed(plan, ChangedPrecondition::VendorRecord);
    }

    let mut vendors = plan.candidate_vendors.clone();
    let subject_vendor_id = match plan.kind {
        VendorMutationKind::Create => {
            let generated = uuid_generator.generate_v4();
            let Some(identity) = parsed_vendor_id(&generated) else {
                return MutationReport::Rejected {
                    plan,
                    rejection: VendorMutationRejection::InvalidGeneratedIdentity { generated },
                };
            };
            if current
                .vendors()
                .iter()
                .any(|vendor| vendor.vendor_id == identity)
            {
                return MutationReport::Rejected {
                    plan,
                    rejection: VendorMutationRejection::GeneratedIdentityCollision {
                        vendor_id: generated,
                    },
                };
            }
            let subject = vendors
                .iter_mut()
                .find(|vendor| vendor.vendor_id.as_str() == plan.subject_vendor_id)
                .expect("the planned aggregate contains its own created Vendor");
            subject.vendor_id = identity;
            generated
        }
        VendorMutationKind::Edit | VendorMutationKind::Hide | VendorMutationKind::Unhide => {
            plan.subject_vendor_id.clone()
        }
    };

    let Ok(vendors) = validated_aggregate::<F::Error>(&current, vendors) else {
        return preconditions_changed(plan, ChangedPrecondition::Package);
    };
    let Ok(candidate) = vendors.to_json_bytes() else {
        return preconditions_changed(plan, ChangedPrecondition::VendorRecord);
    };
    let committed = vendors
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id.as_str() == subject_vendor_id)
        .cloned()
        .expect("the validated aggregate retains its own subject Vendor");

    let outcome = plan.kind.canonical_outcome();
    if !plan.changes_canonical_vendors() {
        let result = VendorMutationResult {
            outcome,
            vendor: committed,
            referencing_orders: referencing_orders(&subject_vendor_id, current.healthy_orders()),
            index: current.index().clone(),
        };
        return MutationReport::Unchanged { plan, result };
    }
    // The aggregate revalidated above is the one this mutation is authorized to
    // replace, so a writer landing between that revalidation and the rename is refused
    // rather than overwritten. Meaning is what is compared, not bytes — which rule to
    // carry where, and what comparing meaning gives up, are in
    // `docs/architecture/durable-writes.md`.
    let replacement = {
        // Projected the way the load projects it, because that projection — not the
        // raw parse — is what the revalidation above compared. An aggregate whose
        // stored facts carry untrimmed text projects to the same Vendors, and refusing
        // it would mean refusing to edit a file a person reformatted by hand. Equality
        // with an aggregate that already validated is itself proof of validity, so no
        // separate check is owed here.
        //
        // The empty Order slice is not a shortcut. It is precisely what the load
        // passes — see the `normalized_vendors` binding in
        // `inspect_canonical_package_with_breadth` — to build the projection that
        // becomes `plan.prior_vendors`, so both sides of this comparison are projected
        // by the same call. It is also the only sound argument: this predicate is asked
        // whether one file still means what was read out of it, and the checks an
        // Order slice would add — duplicate Order ids, unresolved Vendor references —
        // are about state that does not live in `vendors.json` at all.
        let still_holds_the_revalidated_aggregate = |current: &[u8]| {
            VendorsDocument::from_json_bytes(current)
                .ok()
                .and_then(|document| {
                    let version = VersionDocument {
                        metadata_version: Version1,
                    };
                    PackageMetadata::new(&version, &[], &document)
                        .validate()
                        .ok()
                })
                .is_some_and(|projected| projected.vendors().vendors == plan.prior_vendors)
        };
        replace_validated_file_expecting(
            &plan.vendors_path,
            &candidate,
            DestinationExpectation::Equivalent(&still_holds_the_revalidated_aggregate),
            filesystem,
            |_| Ok::<(), VendorCandidateFailure>(()),
        )
    };
    if let Err(error) = replacement {
        return MutationReport::Rejected {
            plan,
            rejection: VendorMutationRejection::ReplacementFailed { error },
        };
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => {
            let result = result_from_package(outcome, &committed, &package);
            MutationReport::Committed { plan, result }
        }
        PackageLoad::IndexReplacementFailed(failed) => {
            let result = result_from_package(outcome, &committed, failed.canonical_evidence());
            let warning = VendorMutationWarning::IndexRefreshFailed(failed.failure().clone());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        PackageLoad::OperationFailed(failed) => {
            let warning = VendorMutationWarning::IndexRefreshFailed(failed.failure().clone());
            let result = result_without_refresh(outcome, &committed, &plan);
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        _ => {
            let result = result_without_refresh(outcome, &committed, &plan);
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![VendorMutationWarning::RefreshUnavailable],
            }
        }
    }
}

/// Which observed precondition stopped a commit before its first write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangedPrecondition {
    Package,
    VendorRecord,
}

fn preconditions_changed<E>(
    plan: VendorMutationPlan,
    changed: ChangedPrecondition,
) -> VendorMutationReport<E> {
    let path = match changed {
        ChangedPrecondition::Package => plan.root.clone(),
        ChangedPrecondition::VendorRecord => plan.vendors_path.clone(),
    };
    let location = located_in_package(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: VendorMutationRejection::PreconditionsChanged { location },
    }
}

fn parsed_vendor_id(generated: &str) -> Option<VendorId> {
    serde_json::from_value(serde_json::Value::String(generated.to_owned())).ok()
}

fn result_from_package(
    outcome: CanonicalVendorOutcome,
    committed: &Vendor,
    package: &CurrentPackageCandidate,
) -> VendorMutationResult {
    let vendor = package
        .vendors()
        .iter()
        .find(|vendor| vendor.vendor_id == committed.vendor_id)
        .cloned()
        .unwrap_or_else(|| committed.clone());
    VendorMutationResult {
        outcome,
        referencing_orders: referencing_orders(
            committed.vendor_id.as_str(),
            package.healthy_orders(),
        ),
        vendor,
        index: package.index().clone(),
    }
}

fn result_without_refresh(
    outcome: CanonicalVendorOutcome,
    committed: &Vendor,
    plan: &VendorMutationPlan,
) -> VendorMutationResult {
    VendorMutationResult {
        outcome,
        vendor: committed.clone(),
        referencing_orders: plan.referencing_orders.clone(),
        index: plan
            .prior_index
            .after_committed_vendor_change_without_refresh(),
    }
}
