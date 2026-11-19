//! The constant each library value is published as, chosen by its type. Every match is
//! exhaustive, so a value with no constant does not compile rather than answering -1.

use crate::interface::{SpielordersConstant, constants::*};
use crate::{
    Failure, Warning,
    canonical::{PackageIndexMalformedReason, PackageIndexStaleReason, PackageIndexState},
    change::PlannedAct,
    journal::CreationAct,
    model::{RepairAction, Severity},
    mutation::{OperationKind, RejectedAssignmentReason},
    readme::DiagnosticCode,
};

/// The `SPIELORDERS_FAILURE_*` `failure` is.
pub(crate) fn failure(failure: &Failure) -> SpielordersConstant {
    match failure {
        Failure::RootIsALink { .. } => SPIELORDERS_FAILURE_ROOT_IS_A_LINK,
        Failure::UnmanagedPackage(..) => SPIELORDERS_FAILURE_UNMANAGED_PACKAGE,
        Failure::PackageMetadataUnsupported(..) => SPIELORDERS_FAILURE_PACKAGE_METADATA_UNSUPPORTED,
        Failure::DamagedPackage(..) => SPIELORDERS_FAILURE_DAMAGED_PACKAGE,
        Failure::PackageAccessDenied(..) => SPIELORDERS_FAILURE_PACKAGE_ACCESS_DENIED,
        Failure::PackageOperationFailed(..) => SPIELORDERS_FAILURE_PACKAGE_OPERATION_FAILED,
        Failure::FilesystemUnsupported(..)
        | Failure::FilesystemUnsupportedCleanupIncomplete { .. } => {
            SPIELORDERS_FAILURE_FILESYSTEM_UNSUPPORTED
        }
        Failure::PackageInvalid(..) => SPIELORDERS_FAILURE_PACKAGE_INVALID,
        Failure::OrderInvalid(..) => SPIELORDERS_FAILURE_ORDER_INVALID,
        Failure::Cancelled => SPIELORDERS_FAILURE_CANCELLED,
        Failure::NotAnOrderDirectory { .. } => SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY,
        Failure::OrderNotFound { .. } => SPIELORDERS_FAILURE_ORDER_NOT_FOUND,
        Failure::DamagedRecord { .. } => SPIELORDERS_FAILURE_DAMAGED_RECORD,
        Failure::LocatorUnusable { .. } => SPIELORDERS_FAILURE_LOCATOR_UNUSABLE,
        Failure::LocatorReachesNoPackageHoldingThisOrder { .. } => {
            SPIELORDERS_FAILURE_LOCATOR_REACHES_NO_PACKAGE_HOLDING_THIS_ORDER
        }
        Failure::DamagedOrder { .. } => SPIELORDERS_FAILURE_DAMAGED_ORDER,
        Failure::UnknownVendor { .. } => SPIELORDERS_FAILURE_UNKNOWN_VENDOR,
        Failure::PackageHasNoVendors { .. } => SPIELORDERS_FAILURE_PACKAGE_HAS_NO_VENDORS,
        Failure::UnknownVendorId { .. } => SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID,
        Failure::UnresolvedVendor { .. } => SPIELORDERS_FAILURE_UNRESOLVED_VENDOR,
        Failure::VendorNameInUse { .. } => SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE,
        Failure::VendorReferenced { .. } => SPIELORDERS_FAILURE_VENDOR_REFERENCED,
        Failure::UnknownValue { .. } => SPIELORDERS_FAILURE_UNKNOWN_VALUE,
        Failure::NotBatchable => SPIELORDERS_FAILURE_NOT_BATCHABLE,
        Failure::ValueInUse { .. } => SPIELORDERS_FAILURE_VALUE_IN_USE,
        Failure::BlankValue { .. } => SPIELORDERS_FAILURE_BLANK_VALUE,
        Failure::CurrencyDiffers { .. } => SPIELORDERS_FAILURE_CURRENCY_DIFFERS,
        Failure::OrderIdentityInUse { .. } => SPIELORDERS_FAILURE_ORDER_IDENTITY_IN_USE,
        Failure::MissingRequiredName { .. } => SPIELORDERS_FAILURE_MISSING_REQUIRED_NAME,
        Failure::InvalidAggregate => SPIELORDERS_FAILURE_INVALID_AGGREGATE,
        Failure::InvalidAssignments(..) => SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS,
        Failure::MissingRequiredFacts { .. } => SPIELORDERS_FAILURE_MISSING_REQUIRED_FACTS,
        Failure::MissingRequiredItemFacts { .. } => SPIELORDERS_FAILURE_MISSING_REQUIRED_ITEM_FACTS,
        Failure::InvalidFactValue { .. } => SPIELORDERS_FAILURE_INVALID_FACT_VALUE,
        Failure::InvalidFacts => SPIELORDERS_FAILURE_INVALID_FACTS,
        Failure::UnconfirmedReusableValue { .. } => SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE,
        Failure::InvalidDraft(..) => SPIELORDERS_FAILURE_INVALID_DRAFT,
        Failure::EmptyProjectedComponent => SPIELORDERS_FAILURE_EMPTY_PROJECTED_COMPONENT,
        Failure::DestinationCollision { .. } => SPIELORDERS_FAILURE_DESTINATION_COLLISION,
        Failure::StagingCollision { .. } => SPIELORDERS_FAILURE_STAGING_COLLISION,
        Failure::NotDirectChild { .. } => SPIELORDERS_FAILURE_NOT_DIRECT_CHILD,
        Failure::HiddenCandidate { .. } => SPIELORDERS_FAILURE_HIDDEN_CANDIDATE,
        Failure::IneligibleCandidate { .. } => SPIELORDERS_FAILURE_INELIGIBLE_CANDIDATE,
        Failure::NameUnusableCandidate { .. } => SPIELORDERS_FAILURE_NAME_UNUSABLE_CANDIDATE,
        Failure::AlreadyAnOrder { .. } => SPIELORDERS_FAILURE_ALREADY_AN_ORDER,
        Failure::ConflictingReservedState { .. } => SPIELORDERS_FAILURE_CONFLICTING_RESERVED_STATE,
        Failure::RootNotOrdinaryDirectory => SPIELORDERS_FAILURE_ROOT_NOT_ORDINARY_DIRECTORY,
        Failure::AlreadyAPackage { .. } => SPIELORDERS_FAILURE_ALREADY_A_PACKAGE,
        Failure::InvalidConfiguredPath { .. } => SPIELORDERS_FAILURE_INVALID_CONFIGURED_PATH,
        Failure::ManagedPathsOverlap { .. } => SPIELORDERS_FAILURE_MANAGED_PATHS_OVERLAP,
        Failure::PlannedPathCollision { .. } => SPIELORDERS_FAILURE_PLANNED_PATH_COLLISION,
        Failure::DisallowedLink { .. } => SPIELORDERS_FAILURE_DISALLOWED_LINK,
        Failure::UnwritableDestination { .. } => SPIELORDERS_FAILURE_UNWRITABLE_DESTINATION,
        Failure::OrderReadmeTextUnavailable(..) => {
            SPIELORDERS_FAILURE_ORDER_README_TEXT_UNAVAILABLE
        }
        Failure::PreconditionsChanged { .. } => SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED,
        Failure::PlanEvidenceUnconfirmed { .. } => SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED,
        Failure::InvalidGeneratedIdentity { .. } => SPIELORDERS_FAILURE_INVALID_GENERATED_IDENTITY,
        Failure::GeneratedIdentityCollision { .. } => {
            SPIELORDERS_FAILURE_GENERATED_IDENTITY_COLLISION
        }
        Failure::GeneratedVendorIdentityCollision { .. } => {
            SPIELORDERS_FAILURE_GENERATED_VENDOR_IDENTITY_COLLISION
        }
        Failure::CommitFailed { .. } => SPIELORDERS_FAILURE_COMMIT_FAILED,
        Failure::CommitFailedCleanupIncomplete { .. } => {
            SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE
        }
        Failure::IndexReplacementFailed(..) => SPIELORDERS_FAILURE_INDEX_REPLACEMENT_FAILED,
        Failure::RemovedOrderUnreadable(failure) => self::failure(failure),
        Failure::UnknownCase { .. } => SPIELORDERS_FAILURE_UNKNOWN_CASE,
        Failure::UnauthorizedAction { .. } => SPIELORDERS_FAILURE_UNAUTHORIZED_ACTION,
        Failure::UnrecognizedArtifact { .. } => SPIELORDERS_FAILURE_UNRECOGNIZED_ARTIFACT,
        Failure::InvalidCandidate { .. } => SPIELORDERS_FAILURE_INVALID_CANDIDATE,
        Failure::CandidateDamagesPackage { .. } => SPIELORDERS_FAILURE_CANDIDATE_DAMAGES_PACKAGE,
        Failure::StagingEvidencePresent { .. } => SPIELORDERS_FAILURE_STAGING_EVIDENCE_PRESENT,
        Failure::EvidenceChanged { .. } => SPIELORDERS_FAILURE_EVIDENCE_CHANGED,
        Failure::RepairResolutionMissing { .. } => SPIELORDERS_FAILURE_REPAIR_RESOLUTION_MISSING,
        Failure::RepairResolutionUnexpected { .. } => {
            SPIELORDERS_FAILURE_REPAIR_RESOLUTION_UNEXPECTED
        }
        Failure::IndexingDisabled(..) => SPIELORDERS_FAILURE_INDEXING_DISABLED,
        Failure::VolumeNotIndexed(..) => SPIELORDERS_FAILURE_VOLUME_NOT_INDEXED,
        Failure::AccessDenied(..) => SPIELORDERS_FAILURE_ACCESS_DENIED,
        Failure::QueryRefused(..) => SPIELORDERS_FAILURE_QUERY_REFUSED,
        Failure::Unsupported(..) => SPIELORDERS_FAILURE_UNSUPPORTED,
    }
}

/// The `SPIELORDERS_WARNING_*` `warning` is.
pub(crate) fn warning(warning: &Warning) -> SpielordersConstant {
    match warning {
        Warning::IndexRefreshFailed(..) => SPIELORDERS_WARNING_INDEX_REFRESH_FAILED,
        Warning::RefreshUnavailable => SPIELORDERS_WARNING_REFRESH_UNAVAILABLE,
        Warning::RediagnosisUnavailable => SPIELORDERS_WARNING_REDIAGNOSIS_UNAVAILABLE,
        Warning::StagingCleanupFailed(..) => SPIELORDERS_WARNING_STAGING_CLEANUP_FAILED,
        Warning::LocatorUpdateFailed(..) => SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED,
        Warning::OrderNotRemoved(..) => SPIELORDERS_WARNING_ORDER_NOT_REMOVED,
    }
}

/// The `SPIELORDERS_FINDING_*` `code` is.
pub(crate) fn finding(code: DiagnosticCode) -> SpielordersConstant {
    match code {
        DiagnosticCode::ConfigurationPathInvalid => SPIELORDERS_FINDING_CONFIGURATION_PATH_INVALID,
        DiagnosticCode::ConfigurationPathsOverlap => {
            SPIELORDERS_FINDING_CONFIGURATION_PATHS_OVERLAP
        }
        DiagnosticCode::ConfiguredOrdersDirectoryNameUnusable => {
            SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_NAME_UNUSABLE
        }
        DiagnosticCode::ConfiguredOrdersDirectoryUnavailable => {
            SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_UNAVAILABLE
        }
        DiagnosticCode::ManagedArtifactInvalidEncoding => {
            SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_ENCODING
        }
        DiagnosticCode::ManagedArtifactInvalidShape => {
            SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE
        }
        DiagnosticCode::ManagedArtifactInvalidSyntax => {
            SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SYNTAX
        }
        DiagnosticCode::ManagedArtifactMissing => SPIELORDERS_FINDING_MANAGED_ARTIFACT_MISSING,
        DiagnosticCode::ManagedArtifactUnreadable => {
            SPIELORDERS_FINDING_MANAGED_ARTIFACT_UNREADABLE
        }
        DiagnosticCode::ManagedArtifactWrongKind => SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_KIND,
        DiagnosticCode::ManagedArtifactWrongVersion => {
            SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_VERSION
        }
        DiagnosticCode::OrderIdDuplicate => SPIELORDERS_FINDING_ORDER_ID_DUPLICATE,
        DiagnosticCode::OrderPackageLocatorMismatch => {
            SPIELORDERS_FINDING_ORDER_PACKAGE_LOCATOR_MISMATCH
        }
        DiagnosticCode::OrderSemanticInvalid => SPIELORDERS_FINDING_ORDER_SEMANTIC_INVALID,
        DiagnosticCode::OrdersDirectoryEntryDisallowed => {
            SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_DISALLOWED
        }
        DiagnosticCode::OrdersDirectoryEntryNameUnusable => {
            SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_NAME_UNUSABLE
        }
        DiagnosticCode::ReadmeUnavailable => SPIELORDERS_FINDING_README_UNAVAILABLE,
        DiagnosticCode::ReservedContentUnexpected => {
            SPIELORDERS_FINDING_RESERVED_CONTENT_UNEXPECTED
        }
        DiagnosticCode::StagingArtifactPresent => SPIELORDERS_FINDING_STAGING_ARTIFACT_PRESENT,
        DiagnosticCode::VendorIdDuplicate => SPIELORDERS_FINDING_VENDOR_ID_DUPLICATE,
        DiagnosticCode::VendorNameDuplicate => SPIELORDERS_FINDING_VENDOR_NAME_DUPLICATE,
        DiagnosticCode::VendorReferenceUnresolved => {
            SPIELORDERS_FINDING_VENDOR_REFERENCE_UNRESOLVED
        }
    }
}

/// The `SPIELORDERS_SEVERITY_*` `severity` is.
pub(crate) fn severity(severity: Severity) -> SpielordersConstant {
    match severity {
        Severity::Error => SPIELORDERS_SEVERITY_ERROR,
        Severity::Warning => SPIELORDERS_SEVERITY_WARNING,
    }
}

/// The `SPIELORDERS_REPAIR_ACTION_*` `action` is.
pub(crate) fn repair_action(action: RepairAction) -> SpielordersConstant {
    match action {
        RepairAction::ReplaceManagedRecord => SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD,
        RepairAction::PromoteStagingCandidate => {
            SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE
        }
        RepairAction::AssignExistingVendor => SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR,
        RepairAction::CreateAndAssignVendor => SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR,
        RepairAction::ClearVendorReference => SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE,
        RepairAction::AdoptNewOrderIdentity => SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY,
        RepairAction::RepairPackageLocator => SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR,
    }
}

/// The `SPIELORDERS_REASON_*` `reason` is.
pub(crate) fn reason(reason: RejectedAssignmentReason) -> SpielordersConstant {
    match reason {
        RejectedAssignmentReason::UnknownField => SPIELORDERS_REASON_UNKNOWN_FIELD,
        RejectedAssignmentReason::ImmutableIdentity => SPIELORDERS_REASON_IMMUTABLE_IDENTITY,
        RejectedAssignmentReason::ManagedTechnicalField => {
            SPIELORDERS_REASON_MANAGED_TECHNICAL_FIELD
        }
        RejectedAssignmentReason::OrderedCollection => SPIELORDERS_REASON_ORDERED_COLLECTION,
        RejectedAssignmentReason::Duplicate => SPIELORDERS_REASON_DUPLICATE,
        RejectedAssignmentReason::BlankRequiredValue => SPIELORDERS_REASON_BLANK_REQUIRED_VALUE,
        RejectedAssignmentReason::NotAReusableValue => SPIELORDERS_REASON_NOT_A_REUSABLE_VALUE,
        RejectedAssignmentReason::NotAFiniteNumber => SPIELORDERS_REASON_NOT_A_FINITE_NUMBER,
    }
}

/// The `SPIELORDERS_INDEX_STATE_*` `state` is.
pub(crate) fn index_state(state: PackageIndexState) -> SpielordersConstant {
    match state {
        PackageIndexState::Current => SPIELORDERS_INDEX_STATE_CURRENT,
        PackageIndexState::Missing => SPIELORDERS_INDEX_STATE_MISSING,
        PackageIndexState::Malformed => SPIELORDERS_INDEX_STATE_MALFORMED,
        PackageIndexState::WrongVersion => SPIELORDERS_INDEX_STATE_WRONG_VERSION,
        PackageIndexState::Stale => SPIELORDERS_INDEX_STATE_STALE,
        PackageIndexState::NotAssessed => SPIELORDERS_INDEX_STATE_NOT_ASSESSED,
    }
}

/// The `SPIELORDERS_INDEX_REASON_*` a stale `reason` is.
pub(crate) fn stale_reason(reason: PackageIndexStaleReason) -> SpielordersConstant {
    match reason {
        PackageIndexStaleReason::OrderMembershipChanged => {
            SPIELORDERS_INDEX_REASON_ORDER_MEMBERSHIP_CHANGED
        }
        PackageIndexStaleReason::OrderPathMismatch => SPIELORDERS_INDEX_REASON_ORDER_PATH_MISMATCH,
        PackageIndexStaleReason::ManagedInputWrongKind => {
            SPIELORDERS_INDEX_REASON_MANAGED_INPUT_WRONG_KIND
        }
        PackageIndexStaleReason::VendorsChanged => SPIELORDERS_INDEX_REASON_VENDORS_CHANGED,
        PackageIndexStaleReason::OrderMetadataChanged => {
            SPIELORDERS_INDEX_REASON_ORDER_METADATA_CHANGED
        }
        PackageIndexStaleReason::OrderReadmeChanged => {
            SPIELORDERS_INDEX_REASON_ORDER_README_CHANGED
        }
        PackageIndexStaleReason::ChangeEvidenceUncertain => {
            SPIELORDERS_INDEX_REASON_CHANGE_EVIDENCE_UNCERTAIN
        }
    }
}

/// The `SPIELORDERS_INDEX_REASON_*` a malformed `reason` is.
pub(crate) fn malformed_reason(reason: PackageIndexMalformedReason) -> SpielordersConstant {
    match reason {
        PackageIndexMalformedReason::WrongKind => SPIELORDERS_INDEX_REASON_WRONG_KIND,
        PackageIndexMalformedReason::Unreadable => SPIELORDERS_INDEX_REASON_UNREADABLE,
        PackageIndexMalformedReason::InvalidEncoding => SPIELORDERS_INDEX_REASON_INVALID_ENCODING,
        PackageIndexMalformedReason::InvalidSyntax => SPIELORDERS_INDEX_REASON_INVALID_SYNTAX,
        PackageIndexMalformedReason::InvalidShape => SPIELORDERS_INDEX_REASON_INVALID_SHAPE,
        PackageIndexMalformedReason::InvalidSemantics => SPIELORDERS_INDEX_REASON_INVALID_SEMANTICS,
        PackageIndexMalformedReason::NonDeterministicSerialization => {
            SPIELORDERS_INDEX_REASON_NON_DETERMINISTIC_SERIALIZATION
        }
    }
}

/// The `SPIELORDERS_OPERATION_*` `act` is.
pub(crate) fn operation(act: PlannedAct) -> SpielordersConstant {
    match act {
        PlannedAct::Managed(OperationKind::CreateStagingDirectory) => {
            SPIELORDERS_OPERATION_CREATE_STAGING_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::CreateMetadataDirectory) => {
            SPIELORDERS_OPERATION_CREATE_METADATA_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::CreateRemovedOrdersDirectory) => {
            SPIELORDERS_OPERATION_CREATE_REMOVED_ORDERS_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::WriteOrderRecord) => {
            SPIELORDERS_OPERATION_WRITE_ORDER_RECORD
        }
        PlannedAct::Managed(OperationKind::WriteReadme) => SPIELORDERS_OPERATION_WRITE_README,
        PlannedAct::Managed(OperationKind::StageOrderRecord) => {
            SPIELORDERS_OPERATION_STAGE_ORDER_RECORD
        }
        PlannedAct::Managed(OperationKind::StageReadme) => SPIELORDERS_OPERATION_STAGE_README,
        PlannedAct::Managed(OperationKind::CommitReadme) => SPIELORDERS_OPERATION_COMMIT_README,
        PlannedAct::Managed(OperationKind::CommitOrderRecord) => {
            SPIELORDERS_OPERATION_COMMIT_ORDER_RECORD
        }
        PlannedAct::Managed(OperationKind::CommitDirectory) => {
            SPIELORDERS_OPERATION_COMMIT_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::RemoveStaging) => SPIELORDERS_OPERATION_REMOVE_STAGING,
        PlannedAct::Managed(OperationKind::RenameOrderDirectory) => {
            SPIELORDERS_OPERATION_RENAME_ORDER_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::MoveOrderDirectory) => {
            SPIELORDERS_OPERATION_MOVE_ORDER_DIRECTORY
        }
        PlannedAct::Managed(OperationKind::ReplaceOrderRecord) => {
            SPIELORDERS_OPERATION_REPLACE_ORDER_RECORD
        }
        PlannedAct::Managed(OperationKind::ReplaceVendorRecord) => {
            SPIELORDERS_OPERATION_REPLACE_VENDOR_RECORD
        }
        PlannedAct::Managed(OperationKind::ReplaceReadmeDocument) => {
            SPIELORDERS_OPERATION_REPLACE_README_DOCUMENT
        }
        PlannedAct::Managed(OperationKind::ReplaceManagedArtifact) => {
            SPIELORDERS_OPERATION_REPLACE_MANAGED_ARTIFACT
        }
        PlannedAct::Managed(OperationKind::PromoteStagingCandidate) => {
            SPIELORDERS_OPERATION_PROMOTE_STAGING_CANDIDATE
        }
        PlannedAct::Managed(OperationKind::RefreshPackageIndex) => {
            SPIELORDERS_OPERATION_REFRESH_PACKAGE_INDEX
        }
        PlannedAct::Managed(OperationKind::MoveMergedEntry) => {
            SPIELORDERS_OPERATION_MOVE_MERGED_ENTRY
        }
        PlannedAct::Managed(OperationKind::DiscardMergedEntry) => {
            SPIELORDERS_OPERATION_DISCARD_MERGED_ENTRY
        }
        PlannedAct::Creation(CreationAct::CreateDirectory) => {
            SPIELORDERS_OPERATION_CREATE_DIRECTORY
        }
        PlannedAct::Creation(CreationAct::CreateFile) => SPIELORDERS_OPERATION_CREATE_FILE,
        PlannedAct::Creation(CreationAct::PreserveDirectory) => {
            SPIELORDERS_OPERATION_PRESERVE_DIRECTORY
        }
        PlannedAct::Creation(CreationAct::PreserveFile) => SPIELORDERS_OPERATION_PRESERVE_FILE,
        PlannedAct::Creation(CreationAct::ReplaceFile) => SPIELORDERS_OPERATION_REPLACE_FILE,
    }
}
