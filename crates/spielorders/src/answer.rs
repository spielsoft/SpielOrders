//! What every capability answers with: its result or the one failure it refused with, the
//! findings beside either, and the one warning a committed mutation may carry.
//!
//! Every refusal any capability makes is one [`Failure`], so one condition carries one
//! code whichever capability met it, and a condition met inside another — a failed
//! operation inside a failed commit — is that same code where it is nested. Every warning
//! is one [`Warning`].

use crate::{
    canonical::{CurrentPackageCandidate, DamagedPackageProjection, ManagedContext, OrderPaths},
    diagnosis::Diagnosis,
    evidence::EvidenceOutcome,
    metadata::OrderId,
    mutation::{CleanupFailure, CommitFailure, RejectedAssignment},
    operations::{FailedPath, OperationFailedProjection},
    order_edits::OrderFactKind,
    paths::{ConfiguredPath, ConfiguredPathFailure},
    ports::FileSystemEntryKind,
    readme::{Diagnostic, Location, ReadmeUnavailableReason, RootedLocation},
    repair::PackageConstraint,
    repair_cases::RepairAction,
    supporting_material::ProviderDiagnostic,
};
use std::path::PathBuf;

/// One capability's answer: its result or its failure, and the findings about its subject
/// that the result or failure does not itself carry.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer<T> {
    pub result: Result<T, Failure>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> Answer<T> {
    /// The answer `body` settles on, with no findings beside it.
    pub(crate) fn of(body: impl FnOnce() -> Result<T, Failure>) -> Self {
        body().into()
    }

    pub(crate) fn map<U>(self, f: impl FnOnce(T) -> U) -> Answer<U> {
        Answer {
            result: self.result.map(f),
            diagnostics: self.diagnostics,
        }
    }

    pub(crate) fn beside(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics.extend(diagnostics);
        self
    }
}

impl<T> From<Result<T, Failure>> for Answer<T> {
    fn from(result: Result<T, Failure>) -> Self {
        Self {
            result,
            diagnostics: Vec::new(),
        }
    }
}

/// Why a capability refused, or why a step inside one failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    // A selected root.
    /// The selected root is a link. Links are never followed, so nothing behind it was
    /// read, renamed, disposed of, or edited.
    RootIsALink {
        root: FailedPath,
    },
    /// The exact path given is a resolved directory and is not a SpielOrders Root.
    UnmanagedPackage(Box<crate::canonical::UnmanagedPackage>),
    /// The Package declares a metadata version this build does not interpret, so it is
    /// diagnostic-only and no repair applies.
    PackageMetadataUnsupported(Box<DamagedPackageProjection>),
    /// The Package could be read and not trusted. Its diagnostics are the diagnosis.
    DamagedPackage(Box<DamagedPackageProjection>),
    /// The filesystem denied access to what had to be observed, read, or written.
    PackageAccessDenied(Box<OperationFailedProjection>),
    /// A filesystem operation failed for any other reason.
    PackageOperationFailed(Box<OperationFailedProjection>),

    // A validation verdict: some finding carries error severity. The detail is the
    // diagnosis that found it.
    PackageInvalid(Box<Diagnosis>),
    OrderInvalid(Box<Diagnosis>),

    /// The caller cancelled this read before it settled.
    Cancelled,

    // An Order Directory named by a caller.
    /// What stands at the selected path is not an Order Directory.
    NotAnOrderDirectory {
        directory: FailedPath,
        found: Option<FileSystemEntryKind>,
    },
    /// The directory is not one of its Package's Orders. `root` is the Root it was looked
    /// for in, where one resolved.
    OrderNotFound {
        directory: FailedPath,
        root: Option<PathBuf>,
    },
    /// The Order record could not be read as one. `field` names the first field at fault
    /// where the record was still JSON to inspect.
    DamagedRecord {
        directory: FailedPath,
        field: Option<String>,
    },
    /// The record's own technical locator is missing or malformed.
    LocatorUnusable {
        directory: FailedPath,
    },
    /// The locator is well formed and the place it names holds no such Order.
    /// `package_root` is the locator as stored.
    LocatorReachesNoPackageHoldingThisOrder {
        directory: FailedPath,
        package_root: String,
    },
    /// The directory is one of its Package's Damaged Orders; `diagnostics` is the
    /// diagnosis the load quarantined at its scope.
    DamagedOrder {
        context: Box<ManagedContext>,
        paths: Box<OrderPaths>,
        order_id: Option<OrderId>,
        diagnostics: Vec<Diagnostic>,
    },

    // Vendors.
    /// No Vendor carries this Name.
    UnknownVendor {
        name: String,
        scope: RootedLocation,
    },
    /// The Package has no Vendors at all to resolve a Name against.
    PackageHasNoVendors {
        context: Box<ManagedContext>,
        name: String,
    },
    /// No Vendor carries this Vendor ID.
    UnknownVendorId {
        vendor_id: String,
    },
    /// The Order references a Vendor ID its Package does not hold.
    UnresolvedVendor {
        vendor_id: String,
    },
    /// Another Vendor already carries this Name.
    VendorNameInUse {
        name: String,
        vendor_id: String,
        scope: RootedLocation,
    },
    /// A Vendor Name is required and blank.
    MissingRequiredName {
        scope: RootedLocation,
    },
    /// The Vendor aggregate this mutation would write does not validate.
    InvalidAggregate,

    // Assignments.
    /// Every assignment refused, with why.
    InvalidAssignments(Vec<RejectedAssignment>),
    /// A new Order lacks these required facts.
    MissingRequiredFacts {
        fields: Vec<String>,
    },
    /// A new Item lacks these required facts.
    MissingRequiredItemFacts {
        fields: Vec<String>,
    },
    /// A fact's value does not parse as its kind.
    InvalidFactValue {
        fact: OrderFactKind,
        value: String,
    },
    /// The record these facts would write does not validate.
    InvalidFacts,
    /// A new Reusable Free-Entry Value was proposed without confirmation.
    UnconfirmedReusableValue {
        field: String,
        proposed: String,
    },
    /// A whole draft broke these rules, each at the fact or the row it names, the
    /// record's own facts first.
    InvalidDraft(Vec<DraftFailure>),

    // Planning a destination.
    /// The Order's facts project a basename with an empty required component.
    EmptyProjectedComponent,
    /// Something already occupies the destination; nothing invents a suffix.
    DestinationCollision {
        root: Location,
        location: Location,
        found: Option<FileSystemEntryKind>,
    },
    /// A staging artifact already occupies the stage this plan would create.
    StagingCollision {
        location: Location,
    },
    /// The adoption candidate is not a direct child of the Orders Directory.
    NotDirectChild {
        location: Location,
    },
    /// The adoption candidate is a hidden entry.
    HiddenCandidate {
        path: PathBuf,
    },
    /// The adoption candidate is not an ordinary directory.
    IneligibleCandidate {
        path: PathBuf,
        kind: Option<FileSystemEntryKind>,
    },
    /// The adoption candidate's name cannot be an Order Directory name.
    NameUnusableCandidate {
        location: Location,
    },
    /// The adoption candidate already holds an Order.
    AlreadyAnOrder {
        location: Location,
    },
    /// The adoption candidate holds reserved state that is not an Order.
    ConflictingReservedState {
        location: Location,
    },
    /// The selected root is not an ordinary directory.
    RootNotOrdinaryDirectory,
    /// The selected root is already a SpielOrders Root.
    AlreadyAPackage {
        root: Location,
    },
    /// A configured managed path is not one a Package may configure.
    InvalidConfiguredPath {
        root: Location,
        path: ConfiguredPath,
        failure: ConfiguredPathFailure,
    },
    /// The two configured managed paths resolve to one directory.
    ManagedPathsOverlap {
        root: Box<Location>,
    },
    /// A configured path resolves onto a path this plan creates.
    PlannedPathCollision {
        root: Location,
        configured: ConfiguredPath,
        path: Location,
    },
    /// A configured path passes through a link.
    DisallowedLink {
        root: Location,
        path: Location,
    },
    /// The README destination cannot be written.
    UnwritableDestination {
        path: Location,
        reason: ReadmeUnavailableReason,
    },
    /// An Order README exists and its text was not established, so an index entry has
    /// nothing truthful to record for it. The Package travels with the refusal.
    OrderReadmeTextUnavailable(Box<CurrentPackageCandidate>),

    // Committing a plan.
    /// The state this plan was made against changed before its first effect.
    PreconditionsChanged {
        location: Location,
    },
    /// The facts this plan was authorized against no longer stand, as the evidence seam
    /// answered.
    PlanEvidenceUnconfirmed {
        outcome: EvidenceOutcome,
    },
    /// The identity a commit minted is not a valid identity.
    InvalidGeneratedIdentity {
        generated: String,
    },
    /// The Order ID a commit minted is already in use.
    GeneratedIdentityCollision {
        order_id: String,
    },
    /// The Vendor ID a commit minted is already in use.
    GeneratedVendorIdentityCollision {
        vendor_id: String,
    },
    /// A commit stopped partway and removed everything it created.
    CommitFailed {
        failure: Box<CommitFailure>,
    },
    /// A commit stopped partway and could not remove what is named.
    CommitFailedCleanupIncomplete {
        failure: Box<CommitFailure>,
        cleanup_failures: Vec<CleanupFailure>,
    },
    /// The rebuilt Package Index could not be written; the detail is the failed commit.
    IndexReplacementFailed(Box<Failure>),

    // Repair.
    /// No diagnosed case carries this handle.
    UnknownCase {
        handle: String,
        scope: RootedLocation,
    },
    /// The diagnosed case does not offer this action.
    UnauthorizedAction {
        handle: String,
        action: RepairAction,
        scope: RootedLocation,
    },
    /// The destination is not a managed artifact repair recognizes.
    UnrecognizedArtifact {
        destination: RootedLocation,
    },
    /// The candidate does not validate as a record of its kind.
    InvalidCandidate {
        candidate: RootedLocation,
    },
    /// Promoting the candidate would damage the Package.
    CandidateDamagesPackage {
        constraint: PackageConstraint,
        candidate: RootedLocation,
    },
    /// Staging evidence beside the destination must be resolved first.
    StagingEvidencePresent {
        staged: RootedLocation,
    },
    /// The evidence the repair was planned against changed.
    EvidenceChanged {
        evidence: RootedLocation,
    },
    /// The action requires values that were not supplied.
    RepairResolutionMissing {
        action: RepairAction,
        missing: Vec<&'static str>,
    },
    /// A value was supplied that the action does not take.
    RepairResolutionUnexpected {
        action: RepairAction,
        field: String,
    },

    // A Supporting Material provider that could not look.
    /// The platform has an index and it is switched off.
    IndexingDisabled(ProviderUnavailable),
    /// The Package's volume is not indexed, or is one the provider does not index.
    VolumeNotIndexed(ProviderUnavailable),
    /// Access to the index, or to the directories an answer would be about, was denied.
    AccessDenied(ProviderUnavailable),
    /// The provider refused this query.
    QueryRefused(ProviderUnavailable),
    /// This build has no Supporting Material index provider for this platform.
    Unsupported(ProviderUnavailable),
}

impl Failure {
    /// The stable code a caller branches on: one code per condition, whichever capability
    /// met it.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::RootIsALink { .. } => "root_is_a_link",
            Self::UnmanagedPackage(..) => "unmanaged_package",
            Self::PackageMetadataUnsupported(..) => "package_metadata_unsupported",
            Self::DamagedPackage(..) => "damaged_package",
            Self::PackageAccessDenied(..) => "package_access_denied",
            Self::PackageOperationFailed(..) => "package_operation_failed",
            Self::PackageInvalid(..) => "package_invalid",
            Self::OrderInvalid(..) => "order_invalid",
            Self::Cancelled => "cancelled",
            Self::NotAnOrderDirectory { .. } => "not_an_order_directory",
            Self::OrderNotFound { .. } => "order_not_found",
            Self::DamagedRecord { .. } => "damaged_record",
            Self::LocatorUnusable { .. } => "locator_unusable",
            Self::LocatorReachesNoPackageHoldingThisOrder { .. } => {
                "locator_reaches_no_package_holding_this_order"
            }
            Self::DamagedOrder { .. } => "damaged_order",
            Self::UnknownVendor { .. } => "unknown_vendor",
            Self::PackageHasNoVendors { .. } => "package_has_no_vendors",
            Self::UnknownVendorId { .. } => "unknown_vendor_id",
            Self::UnresolvedVendor { .. } => "unresolved_vendor",
            Self::VendorNameInUse { .. } => "vendor_name_in_use",
            Self::MissingRequiredName { .. } => "missing_required_name",
            Self::InvalidAggregate => "invalid_aggregate",
            Self::InvalidAssignments(..) => "invalid_assignments",
            Self::MissingRequiredFacts { .. } => "missing_required_facts",
            Self::MissingRequiredItemFacts { .. } => "missing_required_item_facts",
            Self::InvalidFactValue { .. } => "invalid_fact_value",
            Self::InvalidFacts => "invalid_facts",
            Self::UnconfirmedReusableValue { .. } => "unconfirmed_reusable_value",
            Self::InvalidDraft(..) => "invalid_draft",
            Self::EmptyProjectedComponent => "empty_projected_component",
            Self::DestinationCollision { .. } => "destination_collision",
            Self::StagingCollision { .. } => "staging_collision",
            Self::NotDirectChild { .. } => "not_direct_child",
            Self::HiddenCandidate { .. } => "hidden_candidate",
            Self::IneligibleCandidate { .. } => "ineligible_candidate",
            Self::NameUnusableCandidate { .. } => "name_unusable_candidate",
            Self::AlreadyAnOrder { .. } => "already_an_order",
            Self::ConflictingReservedState { .. } => "conflicting_reserved_state",
            Self::RootNotOrdinaryDirectory => "root_not_ordinary_directory",
            Self::AlreadyAPackage { .. } => "already_a_package",
            Self::InvalidConfiguredPath { .. } => "invalid_configured_path",
            Self::ManagedPathsOverlap { .. } => "managed_paths_overlap",
            Self::PlannedPathCollision { .. } => "planned_path_collision",
            Self::DisallowedLink { .. } => "disallowed_link",
            Self::UnwritableDestination { .. } => "unwritable_destination",
            Self::OrderReadmeTextUnavailable(..) => "order_readme_text_unavailable",
            Self::PreconditionsChanged { .. } => "preconditions_changed",
            Self::PlanEvidenceUnconfirmed { .. } => "plan_evidence_unconfirmed",
            Self::InvalidGeneratedIdentity { .. } => "invalid_generated_identity",
            Self::GeneratedIdentityCollision { .. } => "generated_identity_collision",
            Self::GeneratedVendorIdentityCollision { .. } => "generated_vendor_identity_collision",
            Self::CommitFailed { .. } => "commit_failed",
            Self::CommitFailedCleanupIncomplete { .. } => "commit_failed_cleanup_incomplete",
            Self::IndexReplacementFailed(..) => "index_replacement_failed",
            Self::UnknownCase { .. } => "unknown_case",
            Self::UnauthorizedAction { .. } => "unauthorized_action",
            Self::UnrecognizedArtifact { .. } => "unrecognized_artifact",
            Self::InvalidCandidate { .. } => "invalid_candidate",
            Self::CandidateDamagesPackage { .. } => "candidate_damages_package",
            Self::StagingEvidencePresent { .. } => "staging_evidence_present",
            Self::EvidenceChanged { .. } => "evidence_changed",
            Self::RepairResolutionMissing { .. } => "repair_resolution_missing",
            Self::RepairResolutionUnexpected { .. } => "repair_resolution_unexpected",
            Self::IndexingDisabled(..) => "indexing_disabled",
            Self::VolumeNotIndexed(..) => "volume_not_indexed",
            Self::AccessDenied(..) => "access_denied",
            Self::QueryRefused(..) => "query_refused",
            Self::Unsupported(..) => "unsupported",
        }
    }
}

/// One rule a whole draft broke, at the row of its Item or Contact list that broke it, or
/// at the record's own facts where `row` is `None`. The failure names the field.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftFailure {
    /// One-based, in the draft's own order.
    pub row: Option<usize>,
    pub failure: Failure,
}

impl DraftFailure {
    /// The one failure a single edit refuses with: the first its draft broke.
    pub(crate) fn first(faults: Vec<Self>) -> Failure {
        faults
            .into_iter()
            .next()
            .expect("a refusal names a fault")
            .failure
    }

    /// The record `each` makes of every row, adding every failure of each row to
    /// `faults`.
    pub(crate) fn rows<T>(
        rows: &[crate::Assignments],
        faults: &mut Vec<Self>,
        each: impl Fn(&crate::Assignments) -> Result<T, Vec<Failure>>,
    ) -> Vec<T> {
        let mut records = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            match each(row) {
                Ok(record) => records.push(record),
                Err(failures) => faults.extend(failures.into_iter().map(|failure| Self {
                    row: Some(index + 1),
                    failure,
                })),
            }
        }
        records
    }
}

/// A Supporting Material provider that could not look, and what it said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderUnavailable {
    /// The provider that could not look, or `None` where there was none to ask.
    pub provider: Option<String>,
    pub diagnostics: Vec<ProviderDiagnostic>,
}

/// Something a committed mutation reports beside its result. The mutation stays
/// committed whatever it warns of.
#[derive(Clone, Debug, PartialEq)]
pub enum Warning {
    /// The derived Package Index could not be refreshed after the commit.
    IndexRefreshFailed(Failure),
    /// No load reached a current Package holding the committed subject, so the refresh
    /// could not run or confirm it.
    RefreshUnavailable,
    /// The repaired scope could not be diagnosed again after the commit.
    RediagnosisUnavailable,
    /// A stage the commit created could not be removed after it was promoted.
    StagingCleanupFailed(CleanupFailure),
    /// A removed Order's technical locator could not be rewritten at its new path; the
    /// detail is the failed commit.
    LocatorUpdateFailed(Failure),
}

impl Warning {
    /// The stable code a caller branches on.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::IndexRefreshFailed(..) => "index_refresh_failed",
            Self::RefreshUnavailable => "refresh_unavailable",
            Self::RediagnosisUnavailable => "rediagnosis_unavailable",
            Self::StagingCleanupFailed(..) => "staging_cleanup_failed",
            Self::LocatorUpdateFailed(..) => "locator_update_failed",
        }
    }
}
