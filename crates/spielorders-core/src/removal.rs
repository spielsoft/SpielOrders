//! Planning and persistence for one explicit whole-Order disposition.
//!
//! This is the only M3 operation permitted to move Supporting Material, and it moves
//! it only as part of one intact Order Directory. Nothing here enumerates, reads, or
//! rewrites a directory's children, and nothing here erases anything.

use crate::{
    canonical::{
        CurrentPackageCandidate, HealthyOrder, PackageIndexRecord, PackageLoad, open_package,
        project_committed_order,
    },
    evidence::{EvidenceFingerprint, EvidenceOutcome},
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, ValidatedReplacementFailure,
        replace_validated_file_expecting, staging_path,
    },
    metadata::{OrderDocument, PackageRoot},
    mutation::MutationReport,
    operations::{FailedPath, OperationFailure},
    order_edits::{OrderResolutionFailure, resolve_order},
    ports::{
        DirectoryFileSystem, FileSystem, FileSystemEntryKind, NativeTrash,
        NewDirectoryRenameOutcome,
    },
    readme::{Location, package_relative_path_text},
};
use std::path::{Path, PathBuf};

/// Where the caller asks the Order Directory to go.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemovalDisposition {
    /// Into the configured Removed Orders Directory, where it stays recoverable by
    /// ordinary means.
    RecoverableDirectory,
    /// Into the platform Trash or Recycle Bin.
    PlatformTrash,
}

impl RemovalDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecoverableDirectory => "recoverable_directory",
            Self::PlatformTrash => "platform_trash",
        }
    }
}

/// What the removal does to the Order's technical Package locator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocatorOutcome {
    /// The disposition still belongs to the Package and the relative relationship is
    /// identical, so the stored locator stays correct untouched.
    Unchanged { locator: String },
    /// The disposition still belongs to the Package but sits at a different depth.
    Updated { from: String, to: String },
    /// The disposition leaves the Package entirely, so no Package-relative locator
    /// applies at all.
    NotApplicable,
}

impl LocatorOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unchanged { .. } => "unchanged",
            Self::Updated { .. } => "updated",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderRemovalOperationKind {
    CreateRemovedOrdersDirectory,
    MoveOrderDirectory,
    SubmitDirectoryToTrash,
    ReplaceOrderRecord,
    RefreshPackageIndex,
}

impl OrderRemovalOperationKind {
    /// This operation's one stable spelling.
    ///
    /// Beside the operation rather than in an adapter, so an operation reported by the CLI
    /// and the same operation reported to the desktop shell cannot come to be named
    /// differently.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateRemovedOrdersDirectory => "create_removed_orders_directory",
            Self::MoveOrderDirectory => "move_order_directory",
            Self::SubmitDirectoryToTrash => "submit_directory_to_trash",
            Self::ReplaceOrderRecord => "replace_order_record",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderRemovalOperation {
    kind: OrderRemovalOperationKind,
    location: Location,
}

impl OrderRemovalOperation {
    pub fn kind(&self) -> OrderRemovalOperationKind {
        self.kind
    }

    /// Where this operation acts, as this Package locates it — so a caller reporting it
    /// carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

/// One complete, validated disposition of one whole Order Directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRemovalPlan {
    root: PathBuf,
    source: Location,
    destination: Option<Location>,
    removed_directory: Option<Location>,
    creates_removed_directory: bool,
    disposition: RemovalDisposition,
    locator: LocatorOutcome,
    document: OrderDocument,
    readme_text: Option<String>,
    prior_index: PackageIndexRecord,
    evidence: EvidenceFingerprint,
    operations: Vec<OrderRemovalOperation>,
}

impl OrderRemovalPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The Order Directory as it stands now, as this Package locates it.
    ///
    /// A `Location` rather than a path, for the reason
    /// [`OrderRemovalOperation::location`] carries one: a caller reporting where this
    /// directory is carries the Package-relative spelling as well as the absolute path,
    /// and the normalized Package-relative grammar is this crate's.
    pub fn source(&self) -> &Location {
        &self.source
    }

    /// Where the directory would land, when the disposition keeps it on this
    /// filesystem at a path this core chooses. Trash disposition has no such path.
    pub fn destination(&self) -> Option<&Location> {
        self.destination.as_ref()
    }

    pub fn disposition(&self) -> RemovalDisposition {
        self.disposition
    }

    pub fn locator(&self) -> &LocatorOutcome {
        &self.locator
    }

    /// Whether committing would lazily create the Removed Orders Directory.
    pub fn creates_removed_orders_directory(&self) -> bool {
        self.creates_removed_directory
    }

    pub fn order_id(&self) -> &str {
        self.document.order_id.as_str()
    }

    pub fn title(&self) -> &str {
        self.document.title.as_str()
    }

    pub fn operations(&self) -> &[OrderRemovalOperation] {
        &self.operations
    }

    /// The derived Package Index as it stood when this removal was planned.
    ///
    /// The state a run that never commits has to report: committing refreshes the index
    /// and [`OrderRemovalResult::index`] is that refreshed record, so a dry run has none
    /// of its own and answers with what it observed instead.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        &self.prior_index
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderRemovalPlanError<E> {
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
    /// Something already occupies the Order's basename inside the Removed Orders
    /// Directory. Removal stops rather than inventing a suffix.
    ///
    /// **A `Location` rather than a `PathBuf`**, because the projected destination is a
    /// path inside this Package and [`located`] has already established its
    /// Package-relative spelling — which a refusal owes exactly as a success does.
    DestinationCollision {
        location: Location,
    },
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

impl<E> From<OrderResolutionFailure<E>> for OrderRemovalPlanError<E> {
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

pub fn plan_order_removal<F: FileSystem>(
    order_directory: &Path,
    disposition: RemovalDisposition,
    filesystem: &F,
) -> Result<OrderRemovalPlan, OrderRemovalPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let root = package.context().root().absolute_path().to_path_buf();
    let source = order.paths().directory().clone();
    let document = order.document().clone();
    // A removal moves the README with its directory, so what the load knew is what
    // the removed Order carries.
    let readme_text = order.readme_text().map(str::to_owned);
    let prior_index = package.index().clone();
    let index = package.context().index().clone();

    let mut operations = Vec::new();
    let (destination, removed_directory, creates_removed_directory, locator) = match disposition {
        RemovalDisposition::PlatformTrash => {
            operations.push(OrderRemovalOperation {
                kind: OrderRemovalOperationKind::SubmitDirectoryToTrash,
                location: source.clone(),
            });
            (None, None, false, LocatorOutcome::NotApplicable)
        }
        RemovalDisposition::RecoverableDirectory => {
            // A resolved Package always names its Removed Orders Directory, and the
            // Package load has already refused a configured path that is anything
            // other than absent or an ordinary root-child directory, so an unusable
            // one damages the Package before a removal is ever planned.
            let removed_relative_depth = package
                .context()
                .removed_orders_directory()
                .and_then(|location| location.package_relative_path())
                .map(|relative| relative.split('/').count())
                .ok_or(OrderRemovalPlanError::DamagedPackage)?;
            let removed = package
                .context()
                .removed_orders_directory()
                .cloned()
                .ok_or_else(|| OrderRemovalPlanError::DamagedPackage)?;
            // It is created lazily, so it may legitimately be absent.
            let creates = match filesystem.symlink_metadata(removed.absolute_path()) {
                Ok(None) => true,
                Ok(Some(_)) => false,
                Err(error) => {
                    return Err(OrderRemovalPlanError::FileSystem {
                        path: FailedPath::Located(removed.clone()),
                        error,
                    });
                }
            };
            // The same projection the evidence re-observes, so plan-time and
            // commit-time answers cannot drift apart through two spellings of one rule.
            let destination = projected_destination(source.absolute_path(), &package)
                .map(|destination| located(&root, destination))
                .ok_or_else(|| OrderRemovalPlanError::OrderNotFound {
                    path: source.absolute_path().to_path_buf(),
                })?;
            if !creates {
                match filesystem.symlink_metadata(destination.absolute_path()) {
                    Ok(None) => {}
                    Ok(Some(_)) => {
                        return Err(OrderRemovalPlanError::DestinationCollision {
                            location: destination.clone(),
                        });
                    }
                    Err(error) => {
                        return Err(OrderRemovalPlanError::FileSystem {
                            path: FailedPath::Located(destination.clone()),
                            error,
                        });
                    }
                }
            }
            let locator = locator_outcome(removed_relative_depth + 1, order.package_root());
            if creates {
                operations.push(OrderRemovalOperation {
                    kind: OrderRemovalOperationKind::CreateRemovedOrdersDirectory,
                    location: removed.clone(),
                });
            }
            operations.push(OrderRemovalOperation {
                kind: OrderRemovalOperationKind::MoveOrderDirectory,
                location: destination.clone(),
            });
            if matches!(locator, LocatorOutcome::Updated { .. }) {
                operations.push(OrderRemovalOperation {
                    kind: OrderRemovalOperationKind::ReplaceOrderRecord,
                    location: located(&root, order_record(destination.absolute_path())),
                });
            }
            (Some(destination), Some(removed), creates, locator)
        }
    };
    operations.push(OrderRemovalOperation {
        kind: OrderRemovalOperationKind::RefreshPackageIndex,
        location: index,
    });

    let evidence = removal_evidence(&package, &order, disposition);
    Ok(OrderRemovalPlan {
        root,
        source,
        destination,
        removed_directory,
        creates_removed_directory,
        disposition,
        locator,
        document,
        readme_text,
        prior_index,
        evidence,
        operations,
    })
}

/// The path a recoverable disposition would move `source` to, projected from the
/// Package's configured Removed Orders Directory as it stands in `package`. `None` when
/// the Package names no usable one or the source has no basename to carry over, which
/// is a refusal and never a move to some fallback.
fn projected_destination(source: &Path, package: &CurrentPackageCandidate) -> Option<PathBuf> {
    let removed = package
        .context()
        .removed_orders_directory()?
        .absolute_path();
    Some(removed.join(source.file_name()?))
}

/// A path as its exact bytes. A path is not required to be UTF-8, and a comparison
/// that silently repaired one would lose exactly the difference this seam looks for.
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// The facts a removal plan is authorized against, as `order` and `package` stand now.
///
/// A removal's effects are decided by *which* Order is being disposed of and *where*
/// the disposition would put it. Neither is visible in the path, so neither is covered
/// by confirming that the path still exists: `symlink_metadata` reports an ordinary
/// directory whether the Order standing there is the planned one or a different one
/// left in its place. Each fact is recorded here and re-observed through
/// [`EvidenceFingerprint::confirm`] immediately before the first effect.
fn removal_evidence(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    disposition: RemovalDisposition,
) -> EvidenceFingerprint {
    // Read from the resolved Order rather than taken from the plan, because comparing
    // the plan's own copy of its source against itself would confirm nothing.
    let document = order.document();
    let source = order.paths().directory().absolute_path();
    let fingerprint = EvidenceFingerprint::new()
        // Identity, because the path is location and not identity: an Order ID is what
        // separates this Order from any other that could stand at the same path.
        .observed("order_id", document.order_id.as_str())
        .observed("source", path_bytes(source))
        // The Package the source belongs to, resolved through the Order's own locator.
        // An Order that has come to belong somewhere else is not this plan's subject
        // even where its identity and its path both still match.
        .observed(
            "package_root",
            path_bytes(package.context().root().absolute_path()),
        );
    match disposition {
        // The platform owns where a trashed directory goes, so there is no projected
        // destination to bind to — recorded as observing nothing rather than left out,
        // which would confirm silently. A trashed directory is handed over whole and
        // nothing rewrites its record, so this disposition relies on which Order it is
        // and not on what the record says.
        RemovalDisposition::PlatformTrash => fingerprint.absent("destination"),
        RemovalDisposition::RecoverableDirectory => {
            let fingerprint = match projected_destination(source, package) {
                Some(destination) => fingerprint.observed("destination", path_bytes(&destination)),
                None => fingerprint.absent("destination"),
            };
            // The record as it stands, because this disposition may rewrite the plan's
            // own copy of it at the destination — see `rewrite_locator`, which a depth
            // change reaches. That copy is the only one left once the directory has
            // moved, so a plan bound to identity alone would carry an edit made after
            // planning back out and destroy it. It is recorded only here because only
            // here does anything write it.
            fingerprint.observed("order_record", format!("{document:?}"))
        }
    }
}

/// The locator is a normalized relative path from the Order Directory to its Package
/// root, so it depends only on how deep the directory sits. A disposition that keeps
/// the same depth keeps the same locator.
///
/// It takes the locator rather than the record, because a removal plans for an Order the
/// Package load judged healthy and [`HealthyOrder::package_root`] is where that Order's
/// locator is already settled.
fn locator_outcome(depth: usize, locator: &PackageRoot) -> LocatorOutcome {
    let stored = locator.as_str().to_owned();
    let projected = vec![".."; depth].join("/");
    if projected == stored {
        LocatorOutcome::Unchanged { locator: stored }
    } else {
        LocatorOutcome::Updated {
            from: stored,
            to: projected,
        }
    }
}

/// The removed Order as it stood, plus where it went.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRemovalResult {
    order: HealthyOrder,
    disposition: RemovalDisposition,
    final_location: Option<PathBuf>,
    index: PackageIndexRecord,
}

impl OrderRemovalResult {
    /// The Order that was removed, as it stood at removal.
    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    pub fn disposition(&self) -> RemovalDisposition {
        self.disposition
    }

    /// Where the directory finally landed, when that is knowable. The platform Trash
    /// does not always report a location.
    pub fn final_location(&self) -> Option<&Path> {
        self.final_location.as_deref()
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

/// What a rejected recoverable removal left of the Removed Orders Directory it created.
///
/// That directory is created lazily, so a rejection after the creation would otherwise
/// leave a directory in the user's Package that no removal ever put anything in.
/// Undoing it is owned here rather than left to the caller, which cannot tell the
/// directory this operation created from one that was already there.
#[derive(Debug, PartialEq)]
pub enum RemovedDirectoryCleanup<E> {
    /// This operation created no Removed Orders Directory, so its rejection left none.
    /// Either one was already there or the rejection came before the creation.
    NothingCreated,
    /// The Removed Orders Directory this operation created is no longer there, so the
    /// rejection left nothing of it behind. This reports that end state rather than
    /// claiming the rollback is what removed it: a directory already gone when the
    /// rollback looked reaches the same state by another route.
    Removed { location: Location },
    /// Left in place deliberately: it is no longer the empty ordinary directory this
    /// operation created. Something arrived inside it, or something else stands at its
    /// name, and neither is this rollback's to discard.
    Retained { location: Location },
    /// The rollback could not be completed, so the directory this operation created may
    /// still be there. Reported beside the failure that initiated it rather than in
    /// place of that failure: the removal was refused, and this is what refusing left.
    Failed { location: Location, error: E },
}

#[derive(Debug, PartialEq)]
pub enum OrderRemovalRejection<E> {
    /// **A `Location` rather than a `PathBuf`**, for the reason
    /// [`OrderRemovalPlanError::DestinationCollision`] carries one: every path named here
    /// is one this plan derived inside the Root it established — the Order Directory, the
    /// Removed Orders Directory, or the destination inside it — and a response owes that
    /// Root's spelling for what it names, in a refusal exactly as in a success.
    PreconditionsChanged {
        location: Location,
    },
    /// The facts this removal was authorized against no longer stand, so the plan no
    /// longer names the right Order or the right place to put it. The seam's own answer
    /// is carried rather than re-encoded, so a caller can say which fact moved; it is
    /// only ever constructed from an outcome that was not [`EvidenceOutcome::Unchanged`].
    PlanEvidenceUnconfirmed {
        outcome: EvidenceOutcome,
    },
    DestinationCollision {
        location: Location,
        cleanup: RemovedDirectoryCleanup<E>,
    },
    RemovedDirectoryCreationFailed {
        location: Location,
        error: E,
    },
    MoveFailed {
        location: Location,
        error: E,
        cleanup: RemovedDirectoryCleanup<E>,
    },
    TrashSubmissionFailed {
        location: Location,
        error: E,
    },
}

/// A locator rewrite's candidate is the plan's own confirmed record re-emitted with
/// one technical field replaced, and `relocated_record` hands it back only after it has
/// parsed again as an Order record and been re-serialized from that parse. There is no
/// way to reach the replacement holding a candidate that was never validated, so no
/// candidate validation failure is possible, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderRelocationCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum OrderRemovalWarning<E> {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
    /// The directory moved, but its technical locator could not be rewritten at the
    /// new path. The Order is already outside active scope, so this damages nothing
    /// the Package loads; it is reported so a caller can correct it.
    LocatorUpdateFailed(LocatorUpdateFailure<E>),
}

impl<E> OrderRemovalWarning<E> {
    /// This warning's one stable spelling.
    ///
    /// Beside the warning rather than in an adapter, and the first two are deliberately
    /// the same words `crate::creation`, `crate::adoption`, and `crate::alignment` use:
    /// a committed mutation whose derived refresh failed is one condition however the
    /// Package came to need refreshing. The third is this capability's own, because only
    /// a disposition rewrites a locator.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IndexRefreshFailed(_) => "index_refresh_failed",
            Self::RefreshUnavailable => "refresh_unavailable",
            Self::LocatorUpdateFailed(_) => "locator_update_failed",
        }
    }
}

/// Why the moved Order's technical locator was not rewritten, and what the attempt
/// left beside the record.
///
/// The replacement's own error is carried whole rather than re-spelled, so this reads
/// the way [`crate::order_edits`] and [`crate::vendors`] already report the same
/// primitive's refusals. Its accessors are that error's own pair — [`Self::failure`]
/// beside [`Self::cleanup_failure`], never in place of it, which is what
/// `docs/architecture/durable-writes.md` asks for. What the error cannot supply is a
/// path, so the two a caller has to act on are named here.
#[derive(Debug, PartialEq)]
pub struct LocatorUpdateFailure<E> {
    record: PathBuf,
    orphaned_stage: Option<PathBuf>,
    replacement: Option<ValidatedReplacementError<E, OrderRelocationCandidateFailure>>,
}

impl<E> LocatorUpdateFailure<E> {
    /// The Order record this rewrite was to replace.
    ///
    /// A path to look at, not a claim about what is in it. It still holds the locator
    /// from before the move wherever the replacement never reached the destination —
    /// but a [`ValidatedReplacementFailure::DestinationChanged`] refusal means another
    /// writer's record is standing there, and a
    /// [`ValidatedReplacementFailure::DestinationUnreadable`] one means nobody knows
    /// what is. Correcting the locator means reading this path again first; writing
    /// over it on the strength of this report would undo exactly what the refusal
    /// protected.
    pub fn record(&self) -> &Path {
        &self.record
    }

    /// Why the replacement refused or failed, so a record someone else changed can be
    /// told from a write that did not land. `None` where the replacement was never
    /// reached because the relocated record could not be produced from the plan's own
    /// document: in that case nothing was staged and nothing was written.
    pub fn failure(
        &self,
    ) -> Option<&ValidatedReplacementFailure<E, OrderRelocationCandidateFailure>> {
        self.replacement
            .as_ref()
            .map(ValidatedReplacementError::failure)
    }

    /// The staging file this replacement created and could not remove afterwards.
    ///
    /// `Some` exactly when [`Self::cleanup_failure`] is, because a stage that was
    /// removed is not left behind and is not named. This file sits inside a directory
    /// the removal has just taken out of active scope, which no diagnosis reaches, so
    /// a caller that is never told about it is a caller that never removes it.
    pub fn orphaned_stage(&self) -> Option<&Path> {
        self.orphaned_stage.as_deref()
    }

    /// The error that stopped the replacement removing its own staging file, reported
    /// beside [`Self::failure`] rather than in place of it.
    pub fn cleanup_failure(&self) -> Option<&E> {
        self.replacement.as_ref()?.cleanup_failure()
    }
}

pub type OrderRemovalReport<E> = MutationReport<
    OrderRemovalPlan,
    OrderRemovalResult,
    OrderRemovalRejection<E>,
    OrderRemovalWarning<E>,
>;

pub fn commit_order_removal<F, T>(
    plan: OrderRemovalPlan,
    filesystem: &F,
    trash: &T,
) -> OrderRemovalReport<F::Error>
where
    F: FileSystem + DirectoryFileSystem,
    T: NativeTrash<Error = F::Error>,
{
    // The source must still be the ordinary directory this plan validated. Checked here
    // rather than left to the resolution below, which canonicalizes: a link standing
    // where the directory was would resolve to whatever it points at and then be
    // disposed of in its place.
    match filesystem.symlink_metadata(plan.source.absolute_path()) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        _ => {
            let source = plan.source.clone();
            return preconditions_changed(plan, source);
        }
    }

    // Re-resolve the subject rather than settle for having found something there. A
    // source that no longer resolves to a healthy Order of its own Package is neither
    // this plan's Order nor identifiable as any other.
    let Ok((package, order)) = resolve_order(plan.source.absolute_path(), filesystem) else {
        let source = plan.source.clone();
        return preconditions_changed(plan, source);
    };

    // Immediately before the first effect, which is the lazy creation of the Removed
    // Orders Directory, the move that follows it, or the Trash submission.
    let outcome = plan
        .evidence
        .confirm(&removal_evidence(&package, &order, plan.disposition));
    if !outcome.is_unchanged() {
        return MutationReport::Rejected {
            plan,
            rejection: OrderRemovalRejection::PlanEvidenceUnconfirmed { outcome },
        };
    }

    let mut warnings = Vec::new();
    let final_location = match plan.disposition {
        RemovalDisposition::PlatformTrash => {
            // Exactly one narrow native effect, with the exact directory. There is no
            // filesystem delete fallback: if the platform cannot take it, nothing is
            // removed.
            match trash.submit_directory(plan.source.absolute_path()) {
                Ok(location) => location,
                Err(error) => {
                    let source = plan.source.clone();
                    return MutationReport::Rejected {
                        plan,
                        rejection: OrderRemovalRejection::TrashSubmissionFailed {
                            location: source,
                            error,
                        },
                    };
                }
            }
        }
        RemovalDisposition::RecoverableDirectory => {
            let destination = plan
                .destination
                .clone()
                .expect("a recoverable removal plans a destination");
            // What this operation itself creates, and therefore the only thing its own
            // rejection may undo. A directory that was already there is never recorded
            // here and so is never a candidate.
            let mut created = None;
            if plan.creates_removed_directory
                && let Some(removed) = plan.removed_directory.as_ref()
            {
                if let Err(error) = filesystem.create_directory(removed.absolute_path()) {
                    let location = removed.clone();
                    return MutationReport::Rejected {
                        plan,
                        rejection: OrderRemovalRejection::RemovedDirectoryCreationFailed {
                            location,
                            error,
                        },
                    };
                }
                created = Some(removed.clone());
            }
            // One rename moves the complete tree. Nothing copies, enumerates, or
            // rewrites a child, so README, managed metadata, and Supporting Material
            // travel byte-identical.
            match filesystem
                .rename_new_directory(plan.source.absolute_path(), destination.absolute_path())
            {
                Ok(NewDirectoryRenameOutcome::Renamed) => {}
                Ok(NewDirectoryRenameOutcome::DestinationExists) => {
                    let cleanup = undo_created_directory(created, filesystem);
                    return MutationReport::Rejected {
                        plan,
                        rejection: OrderRemovalRejection::DestinationCollision {
                            location: destination,
                            cleanup,
                        },
                    };
                }
                Err(error) => {
                    let cleanup = undo_created_directory(created, filesystem);
                    return MutationReport::Rejected {
                        plan,
                        rejection: OrderRemovalRejection::MoveFailed {
                            location: destination,
                            error,
                            cleanup,
                        },
                    };
                }
            }
            // The locator is rewritten only after the directory has left active
            // scope, so a failure here cannot damage the Package the caller still
            // uses.
            if let LocatorOutcome::Updated { to, .. } = &plan.locator
                && let Err(failure) =
                    rewrite_locator(destination.absolute_path(), &plan.document, to, filesystem)
            {
                warnings.push(OrderRemovalWarning::LocatorUpdateFailed(failure));
            }
            Some(destination.absolute_path().to_path_buf())
        }
    };

    let order = project_committed_order(
        &plan.root,
        plan.source.absolute_path(),
        &plan.document,
        plan.readme_text.clone(),
    );
    let index = match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => package.index().clone(),
        PackageLoad::IndexReplacementFailed(failed) => {
            warnings.push(OrderRemovalWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            failed.canonical_evidence().index().clone()
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(OrderRemovalWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            index_after_disposition(&plan)
        }
        _ => {
            warnings.push(OrderRemovalWarning::RefreshUnavailable);
            index_after_disposition(&plan)
        }
    };

    let result = OrderRemovalResult {
        order,
        disposition: plan.disposition,
        final_location,
        index,
    };
    // The directory has already gone. A warning describes what could not be refreshed
    // afterwards, never a reason to retry the removal itself.
    if warnings.is_empty() {
        MutationReport::Committed { plan, result }
    } else {
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        }
    }
}

/// Undoes the one Removed Orders Directory this operation created, after the removal
/// that created it was rejected before anything of the Order moved into it.
///
/// It removes only that directory, and only while it is still the empty ordinary
/// directory this operation created. A pre-existing directory is never a candidate,
/// because `created` is `None` for one. Anything that arrived inside it in the meantime
/// is evidence this operation did not create, so its presence retains the directory
/// rather than being swept up with it — the whole point of undoing the creation is to
/// leave the Package as the rejected removal found it, not to take something else away.
fn undo_created_directory<F: FileSystem + DirectoryFileSystem>(
    created: Option<Location>,
    filesystem: &F,
) -> RemovedDirectoryCleanup<F::Error> {
    let Some(location) = created else {
        return RemovedDirectoryCleanup::NothingCreated;
    };
    let path = location.absolute_path().to_path_buf();
    match filesystem.symlink_metadata(&path) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        // Already gone: nothing this operation created is left to undo, which is the
        // state this reports rather than a claim about who removed it.
        Ok(None) => return RemovedDirectoryCleanup::Removed { location },
        // A file or a link now stands at that name. It is not what this operation
        // created and this rollback does not remove it.
        Ok(Some(_)) => return RemovedDirectoryCleanup::Retained { location },
        Err(error) => return RemovedDirectoryCleanup::Failed { location, error },
    }
    // Emptiness is established before the removal rather than inferred from it, so an
    // arrival this rollback can see is named as retained evidence instead of surfacing
    // as an unexplained removal error. This narrows that window rather than closing it:
    // something arriving inside it still makes the removal fail, which is reported as a
    // failed cleanup naming a directory that really is still there.
    match filesystem.read_directory(&path) {
        Ok(entries) if entries.is_empty() => {}
        Ok(_) => return RemovedDirectoryCleanup::Retained { location },
        Err(error) => return RemovedDirectoryCleanup::Failed { location, error },
    }
    match filesystem.remove_directory(&path) {
        Ok(()) => RemovedDirectoryCleanup::Removed { location },
        Err(error) => RemovedDirectoryCleanup::Failed { location, error },
    }
}

/// The derived index as it stands after a disposition no refresh could confirm.
///
/// Either disposition takes the whole Order out of the Orders Directory — into the
/// Removed Orders Directory or into the platform Trash — so the indexed set of Order
/// paths no longer matches the discovered set, which is what `OrderMembershipChanged`
/// names. It is not `OrderMetadataChanged`: a removal writes no Order record. This is
/// the reason the detector reaches on its own for both dispositions when it can
/// inspect the Package, and the inverse of what `creation.rs` and `adoption.rs`
/// already report when an Order arrives.
fn index_after_disposition(plan: &OrderRemovalPlan) -> PackageIndexRecord {
    plan.prior_index
        .after_committed_membership_change_without_refresh()
}

fn rewrite_locator<F: FileSystem>(
    destination: &Path,
    document: &OrderDocument,
    locator: &str,
    filesystem: &F,
) -> Result<(), LocatorUpdateFailure<F::Error>> {
    let record = order_record(destination);
    let Some(candidate) = relocated_record(document, locator) else {
        return Err(LocatorUpdateFailure {
            record,
            orphaned_stage: None,
            replacement: None,
        });
    };
    // The record this plan confirmed is the one the rewrite is authorized to replace,
    // and the copy it replaces is that record's own, carried here by the move. This is
    // the only guard over it: the Order has left the caller's view, and nothing looks
    // at that record again. Meaning is what is compared, not bytes — the rule that
    // sends a managed Order record replaced one at a time to `Equivalent` is in
    // `docs/architecture/durable-writes.md`.
    //
    // Projected the way the load projects it, because `document` is the load's own
    // projection rather than a raw parse: a record whose stored facts carry untrimmed
    // text projects to the same Order, and comparing against the raw parse would
    // refuse to relocate a record a person had tidied by hand.
    let still_holds_the_planned_record = |current: &[u8]| {
        OrderDocument::from_json_bytes(current)
            .ok()
            .and_then(|parsed| parsed.validate().ok())
            .is_some_and(|projected| projected.order() == document)
    };
    let replacement = replace_validated_file_expecting(
        &record,
        &candidate,
        DestinationExpectation::Equivalent(&still_holds_the_planned_record),
        filesystem,
        |_| Ok::<(), OrderRelocationCandidateFailure>(()),
    );
    replacement.map_err(|error| LocatorUpdateFailure {
        // Named only where the primitive says one is still there. A stage it took
        // back is not left behind, and reporting its path anyway would send a caller
        // after a file that is not on disk.
        orphaned_stage: error
            .cleanup_failure()
            .is_some()
            .then(|| staging_path(&record).expect("a managed Order record has a staging path")),
        record,
        replacement: Some(error),
    })
}

/// The plan's own confirmed record with its technical locator replaced, emitted as
/// this application's canonical bytes.
///
/// The round trip through the parse is what makes the result a validated candidate
/// rather than an assembled one: the replaced value has to be readable back as a
/// locator before anything is staged, which is asked of the field itself because that
/// is where the verdict now lives. `None` therefore means the plan's own record stopped
/// round-tripping, which is an internal bug rather than a state of the Package — and
/// after a whole directory has already moved, reporting it is a better answer than
/// ending the process.
fn relocated_record(document: &OrderDocument, locator: &str) -> Option<Vec<u8>> {
    let mut object = serde_json::to_value(document).ok()?.as_object().cloned()?;
    object.insert(
        "package_root".into(),
        serde_json::Value::String(locator.to_owned()),
    );
    let bytes = serde_json::to_vec(&serde_json::Value::Object(object)).ok()?;
    let relocated = OrderDocument::from_json_bytes(&bytes).ok()?;
    relocated.package_root.spelled()?;
    relocated.to_json_bytes().ok()
}

/// One planned path, as this Package locates it.
///
/// The Package-relative spelling holds by construction, as it does for a planned creation
/// artifact: the source is a discovered Order Directory whose own `Location` the load
/// already carries, the Removed Orders Directory is a configured root child the load has
/// already confirmed has one, and the destination is that directory joined with the
/// source's own basename or with this module's literal managed names.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned removal path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned removal path is normalized")
}

/// The managed Order record inside one Order Directory.
///
/// One spelling for the operation this removal plans and the file it later replaces, so
/// the path it reports and the path it writes cannot come apart.
fn order_record(directory: &Path) -> PathBuf {
    directory.join(".spielorders").join("order.json")
}

fn preconditions_changed<E>(plan: OrderRemovalPlan, location: Location) -> OrderRemovalReport<E> {
    MutationReport::Rejected {
        plan,
        rejection: OrderRemovalRejection::PreconditionsChanged { location },
    }
}
