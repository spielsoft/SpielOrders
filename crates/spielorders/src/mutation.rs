//! Shared result vocabulary for side-effecting domain mutations.

use crate::{
    Failure, Warning,
    canonical::{CurrentPackageCandidate, PackageLoad, Touched, load_package_and_refresh_index},
    filesystem::{
        MultiFileReplacementError, MultiFileReplacementFailure, ReplacementPlanFailure,
        RollbackFailure, ValidatedReplacementError, staging_path,
    },
    ports::FileSystem,
    readme::{Location, located_in_package},
};
use serde::Serialize;
use std::path::Path;

/// What committing one capability's plan did, before [`crate::commit`] reports it as a
/// [`crate::Report`]. A commit that was refused is a [`Failure`], not a report.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MutationReport<R> {
    Unchanged { result: R },
    Committed { result: R },
    CommittedWithWarning { result: R, warnings: Vec<Warning> },
}

impl<R> MutationReport<R> {
    /// A commit is committed whatever its warnings say; they only decide which of the
    /// two committed outcomes reports it.
    pub(crate) fn committed(result: R, warnings: Vec<Warning>) -> Self {
        if warnings.is_empty() {
            Self::Committed { result }
        } else {
            Self::CommittedWithWarning { result, warnings }
        }
    }
}

/// A value with no instances, for a candidate failure a validated candidate never meets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Never {}

/// What the Package Index refresh after a canonical commit left behind, reported as
/// `docs/architecture/durable-writes.md` decides for a commit whose refresh failed.
pub(crate) struct IndexRefresh {
    package: Option<CurrentPackageCandidate>,
    warning: Option<Warning>,
}

impl IndexRefresh {
    /// Refreshes the Package Index at `root` after a committed write of the records
    /// `touched` names, which only a committed write may do.
    pub(crate) fn after_commit<F: FileSystem>(
        root: &Path,
        touched: &Touched,
        filesystem: &F,
    ) -> Self {
        let (package, warning) = match load_package_and_refresh_index(root, touched, filesystem) {
            Ok(PackageLoad::Available(package)) => (Some(package), None),
            Ok(PackageLoad::PackageIndexReplacementFailed(failed)) => {
                let (evidence, failure) = failed.into_parts();
                (
                    Some(evidence),
                    Some(Warning::IndexRefreshFailed(
                        Failure::IndexReplacementFailed(Box::new(failure)),
                    )),
                )
            }
            Err(
                failed @ (Failure::PackageAccessDenied(_)
                | Failure::PackageOperationFailed(_)
                | Failure::FilesystemUnsupported(_)),
            ) => (None, Some(Warning::IndexRefreshFailed(failed))),
            // An unmanaged or damaged load attempted no rebuild.
            Err(_) => (None, Some(Warning::RefreshUnavailable)),
        };
        Self { package, warning }
    }

    /// The Package the refresh loaded: rebuilt, or the canonical evidence whose index
    /// could not be replaced. `None` when no load reached a current Package.
    pub(crate) fn package(&self) -> Option<&CurrentPackageCandidate> {
        self.package.as_ref()
    }

    /// The loaded Package does not hold the committed subject, so the refresh cannot
    /// confirm it.
    pub(crate) fn unconfirmed(&mut self) {
        self.warning.get_or_insert(Warning::RefreshUnavailable);
    }

    pub(crate) fn warning(self) -> Option<Warning> {
        self.warning
    }

    /// Reports a commit whose only possible warning is this refresh's.
    pub(crate) fn report<R>(self, result: R) -> MutationReport<R> {
        MutationReport::committed(result, self.warning.into_iter().collect())
    }
}

/// Why one canonical `field=value` assignment was refused. An atomic assignment
/// set reports every rejected field and changes nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectedAssignmentReason {
    UnknownField,
    ImmutableIdentity,
    ManagedTechnicalField,
    OrderedCollection,
    Duplicate,
    BlankRequiredValue,
    NotAReusableValue,
    NotAFiniteNumber,
}

impl crate::code::Coded for RejectedAssignmentReason {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedAssignment {
    pub field: String,
    pub reason: RejectedAssignmentReason,
}

impl RejectedAssignment {
    pub(crate) fn new(field: impl Into<String>, reason: RejectedAssignmentReason) -> Self {
        Self {
            field: field.into(),
            reason,
        }
    }
}

/// Where a commit stopped.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitFailure {
    /// The path this commit stopped on, as the Package it writes inside locates it.
    pub location: Location,
}

impl CommitFailure {
    pub(crate) fn new(location: Location) -> Self {
        Self { location }
    }
}

/// One artifact a commit created, or one destination it replaced, that its cleanup could
/// not remove or put back.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanupFailure {
    /// What the cleanup left behind, as the Package it writes inside locates it.
    pub location: Location,
}

impl CleanupFailure {
    pub(crate) fn new(location: Location) -> Self {
        Self { location }
    }
}

/// A commit that stopped partway, under the code its cleanup earns: where everything it
/// created was removed again it can simply be run again, and where it was not, what
/// remains is named.
impl Failure {
    pub(crate) fn commit(failure: CommitFailure, cleanup_failures: Vec<CleanupFailure>) -> Self {
        if cleanup_failures.is_empty() {
            Self::CommitFailed {
                failure: Box::new(failure),
            }
        } else {
            Self::CommitFailedCleanupIncomplete {
                failure: Box::new(failure),
                cleanup_failures,
            }
        }
    }

    /// This refusal, or, where it left nothing behind and the filesystem cannot perform
    /// the operation that failed on `path` at all, that answer instead, so the caller
    /// learns the volume is the obstacle.
    pub(crate) fn unless_unsupported<F: FileSystem>(
        self,
        root: &Path,
        path: &Path,
        error: &F::Error,
        filesystem: &F,
    ) -> Self {
        match self {
            Self::CommitFailed { .. } if filesystem.is_unsupported(error) => {
                crate::operations::FailedOperation::inside(
                    root,
                    path.to_path_buf(),
                    error,
                    filesystem,
                )
                .into()
            }
            other => other,
        }
    }

    /// A replacement of the file at `destination` inside the Root at `root` that did not
    /// publish it, naming the stage beside it where the replacement could not remove it
    /// again.
    pub(crate) fn of_replacement(
        root: &Path,
        destination: &Path,
        error: ValidatedReplacementError<Never>,
    ) -> Self {
        let cleanup = error.cleanup_failure;
        let stage = staging_path(destination).unwrap_or_else(|| destination.to_path_buf());
        Self::commit(
            CommitFailure::new(located_in_package(root, destination.to_path_buf())),
            cleanup
                .map(|_| stage_left(root, &stage))
                .into_iter()
                .collect(),
        )
    }

    /// A promotion of a stage that already sat beside `destination` inside the Root at
    /// `root`. The stage was not this commit's to remove, so nothing is left to clean up.
    pub(crate) fn of_promotion(root: &Path, destination: &Path) -> Self {
        let failure = CommitFailure::new(located_in_package(root, destination.to_path_buf()));
        Self::commit(failure, Vec::new())
    }

    /// A replacement of several files at once inside the Root at `root`, `destinations`
    /// in the order it wrote them. Every stage it could not remove and every destination
    /// it could not put back is a cleanup failure.
    pub(crate) fn of_replacements<V>(
        root: &Path,
        destinations: &[&Path],
        error: MultiFileReplacementError<V>,
    ) -> Self {
        let locate = |path: &Path| located_in_package(root, path.to_path_buf());
        let stage_of = |destination: &Path| {
            staging_path(destination).expect("a replaced destination has a stage name")
        };
        let MultiFileReplacementError {
            failure,
            cleanup_failures,
            rollback_failures,
            refused_rollbacks,
        } = error;
        let mut cleanup_failures = cleanup_failures
            .into_iter()
            .map(|failure| stage_left(root, failure.projection().path.as_path()))
            .collect::<Vec<_>>();
        let failure = match *failure {
            MultiFileReplacementFailure::InvalidPlan(failures) => {
                let index = match failures
                    .into_iter()
                    .next()
                    .expect("an invalid plan names what is invalid in it")
                {
                    ReplacementPlanFailure::DestinationIdentity {
                        operation_index, ..
                    }
                    | ReplacementPlanFailure::CandidateValidation { operation_index }
                    | ReplacementPlanFailure::InvalidDestination { operation_index }
                    | ReplacementPlanFailure::DuplicateDestination { operation_index } => {
                        operation_index
                    }
                };
                CommitFailure::new(locate(destinations[index]))
            }
            MultiFileReplacementFailure::DestinationChanged { destination }
            | MultiFileReplacementFailure::PriorRead { destination, .. } => {
                CommitFailure::new(locate(&destination))
            }
            MultiFileReplacementFailure::Replacement { destination, error } => {
                cleanup_failures.extend(
                    error
                        .cleanup_failure
                        .map(|_| stage_left(root, &stage_of(&destination))),
                );
                CommitFailure::new(locate(&destination))
            }
        };
        for rollback in rollback_failures {
            let RollbackFailure { destination, error } = rollback;
            cleanup_failures.push(CleanupFailure::new(locate(&destination)));
            cleanup_failures.extend(
                error
                    .cleanup_failure
                    .map(|_| stage_left(root, &stage_of(&destination))),
            );
        }
        cleanup_failures.extend(
            refused_rollbacks
                .into_iter()
                .map(|refused| CleanupFailure::new(locate(&refused.destination))),
        );
        Self::commit(failure, cleanup_failures)
    }
}

/// The stage at `stage` inside the Root at `root`, which a replacement could not remove.
fn stage_left(root: &Path, stage: &Path) -> CleanupFailure {
    CleanupFailure::new(located_in_package(root, stage.to_path_buf()))
}

/// One managed act a mutation plan performs, named the same whichever capability
/// performs it.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    CreateStagingDirectory,
    CreateMetadataDirectory,
    CreateRemovedOrdersDirectory,
    WriteOrderRecord,
    WriteReadme,
    StageOrderRecord,
    StageReadme,
    CommitReadme,
    CommitOrderRecord,
    CommitDirectory,
    RemoveStaging,
    RenameOrderDirectory,
    MoveOrderDirectory,
    ReplaceOrderRecord,
    ReplaceVendorRecord,
    ReplaceReadmeDocument,
    ReplaceManagedArtifact,
    PromoteStagingCandidate,
    RefreshPackageIndex,
    MoveMergedEntry,
    DiscardMergedEntry,
}

impl crate::code::Coded for OperationKind {}

/// One managed operation a plan performs, where the Package locates it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedOperation {
    pub kind: OperationKind,
    pub location: Location,
    /// Whether the entry the operation acts on is a directory, for the operations that act
    /// on a merged Order's entries.
    pub directory: bool,
}

/// Whether committing `operations` replaces the Vendor record.
pub(crate) fn replaces_vendors(operations: &[PlannedOperation]) -> bool {
    operations
        .iter()
        .any(|operation| operation.kind == OperationKind::ReplaceVendorRecord)
}

impl PlannedOperation {
    pub(crate) fn new(kind: OperationKind, location: Location) -> Self {
        Self {
            kind,
            location,
            directory: false,
        }
    }
}

/// The identity a planning candidate carries in place of one the commit mints.
pub(crate) const PLANNING_IDENTITY: &str = "00000000-0000-4000-8000-000000000000";
