//! Applying exactly one currently diagnosed, user-confirmed managed-metadata or
//! staging repair.
//!
//! This capability executes only what
//! [`crate::order_diagnosis`] authorized, for one case at a time, against evidence
//! it revalidates immediately before acting. It never salvages malformed JSON,
//! guesses a missing fact, selects among staging candidates, or cleans anything up
//! on its own.

use crate::{
    canonical::{
        CanonicalPackageLoad, OrderPaths, PackageIndexRecord, PackageLoad,
        inspect_canonical_package, open_package,
    },
    filesystem::{
        DestinationExpectation, StagedPromotionFailure, ValidatedReplacementError,
        ValidatedReplacementFailure, promote_staged_file, replace_validated_file_expecting,
    },
    metadata::{OrderDocument, OrderId, VendorId},
    mutation::MutationReport,
    operations::OperationFailure,
    order_diagnosis::{OrderDiagnosis, OrderDiagnosisError, RepairAction, diagnose_order},
    paths::locator_resolves_to_root,
    ports::FileSystem,
};
use std::path::{Path, PathBuf};

/// The caller's explicit confirmation of exactly one diagnosed case. A repair is
/// never applied without one, and a confirmation naming a different case is refused,
/// so one call can only ever carry out one case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairConfirmation {
    handle: String,
    action: RepairAction,
}

impl RepairConfirmation {
    /// Confirms one action of one diagnosed case, named by the handle that diagnosis
    /// reported.
    pub fn new(handle: impl Into<String>, action: RepairAction) -> Self {
        Self {
            handle: handle.into(),
            action,
        }
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn action(&self) -> RepairAction {
        self.action
    }
}

/// The complete facts a confirmed replacement supplies. A replacement is the user's
/// own record; this core neither composes nor edits it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairResolution {
    /// The complete replacement record, exactly as the user confirmed it.
    ReplacementRecord(Vec<u8>),
    /// Promotion needs no supplied value: the diagnosed candidate is the only one
    /// that may be promoted.
    None,
}

/// A Package-scoped constraint a record must satisfy on top of being a valid Order
/// on its own.
///
/// Each of these is a cross-record fact, so no record can be judged against them from
/// its own bytes. A repair that skipped them would answer one diagnosed condition by
/// creating another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageConstraint {
    /// A different Order Directory of this Package already holds this Order ID.
    OrderIdentityTaken,
    /// The Vendor Reference names no Vendor of this Package.
    VendorReferenceUnresolved,
    /// The technical locator does not lead from this Order Directory to this
    /// Package's SpielOrders Root.
    PackageRootMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataRepairOperationKind {
    ReplaceManagedArtifact,
    PromoteStagingCandidate,
    RefreshPackageIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRepairOperation {
    kind: MetadataRepairOperationKind,
    path: PathBuf,
}

impl MetadataRepairOperation {
    pub fn kind(&self) -> MetadataRepairOperationKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// What the diagnosis found at the destination this repair writes.
///
/// Carried into the write so that a change arriving after the diagnosis is refused
/// rather than overwritten. Absence is one of the two answers rather than a gap: an
/// interrupted replacement can leave a durable stage beside a canonical file it never
/// created, and promoting that candidate is exactly the repair offered for it.
///
/// The observation is bytes, not parsed meaning, because a diagnosed destination
/// generally has no parsed meaning — a malformed record or one carrying an unknown
/// field does not become a record at all. Bytes are the subject here for the same
/// reason they are for a README or a Metadata Upgrade.
#[derive(Clone, Debug, Eq, PartialEq)]
enum DiagnosedDestination {
    Absent,
    Content(Vec<u8>),
}

impl DiagnosedDestination {
    fn expectation(&self) -> DestinationExpectation<'_> {
        match self {
            Self::Absent => DestinationExpectation::Absent,
            Self::Content(observed) => DestinationExpectation::Exactly(observed),
        }
    }
}

/// One complete, validated repair of one diagnosed case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRepairPlan {
    directory: PathBuf,
    root: PathBuf,
    handle: String,
    action: RepairAction,
    artifact: PathBuf,
    candidate: Vec<u8>,
    destination: DiagnosedDestination,
    source: Option<PathBuf>,
    operations: Vec<MetadataRepairOperation>,
}

impl MetadataRepairPlan {
    /// The Order scope this repair acts in.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The exact diagnosed case this repair carries out.
    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn action(&self) -> RepairAction {
        self.action
    }

    /// The single managed artifact this repair replaces.
    pub fn artifact(&self) -> &Path {
        &self.artifact
    }

    /// The staging candidate a promotion reads its bytes from, when the action is a
    /// promotion.
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn operations(&self) -> &[MetadataRepairOperation] {
        &self.operations
    }
}

#[derive(Debug, PartialEq)]
pub enum MetadataRepairError<E> {
    /// The scope could not be diagnosed at all.
    Diagnosis(OrderDiagnosisError<E>),
    /// No currently diagnosed case answers to this handle. A corrected or changed
    /// condition stops appearing, so a handle from an older diagnosis lands here.
    UnknownCase {
        handle: String,
    },
    /// The named case exists but does not authorize this action.
    UnauthorizedAction {
        handle: String,
        action: RepairAction,
    },
    /// This capability applies only the managed-metadata and staging repairs; the
    /// semantic resolutions belong to their own capability.
    UnsupportedAction {
        action: RepairAction,
    },
    /// The supplied resolution does not match what the action requires.
    InvalidResolution {
        action: RepairAction,
    },
    /// A supplied replacement is not a fully valid record under its declared schema
    /// and its own Order semantics. Nothing is written; a partial salvage is never
    /// made.
    InvalidReplacement,
    /// A supplied replacement is a valid Order on its own, and would still leave this
    /// Package damaged. Nothing is written.
    ReplacementDamagesPackage {
        constraint: PackageConstraint,
    },
    /// The diagnosed staging candidate is not itself a fully valid record, so it is
    /// never promoted.
    InvalidStagingCandidate {
        path: PathBuf,
    },
    /// The diagnosed staging candidate is a valid Order on its own, and would still
    /// leave this Package damaged, so it is never promoted.
    StagingCandidateDamagesPackage {
        path: PathBuf,
        constraint: PackageConstraint,
    },
    /// The Package could not be read as a current Package while the candidate was
    /// being judged against it. Nothing shows the candidate is safe, so it is refused
    /// rather than written against evidence nobody has.
    PackageEvidenceUnavailable {
        root: PathBuf,
    },
    /// A replacement stages beside its destination, and abandoned staging evidence
    /// already occupies that path. Writing there would destroy evidence this core is
    /// forbidden to overwrite or silently clean, so the staging case is resolved
    /// first and the replacement is refused until then.
    StagingEvidencePresent {
        path: PathBuf,
    },
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

/// Plans exactly one confirmed repair, revalidating the case and the supplied facts
/// against current evidence before proposing any effect.
pub fn plan_metadata_repair<F: FileSystem>(
    order_directory: &Path,
    confirmation: &RepairConfirmation,
    resolution: &RepairResolution,
    filesystem: &F,
) -> Result<MetadataRepairPlan, MetadataRepairError<F::Error>> {
    let diagnosis =
        diagnose_order(order_directory, filesystem).map_err(MetadataRepairError::Diagnosis)?;
    let case = current_case(&diagnosis, confirmation)?;
    let artifact = case.location().absolute_path().to_path_buf();
    let directory = diagnosis.directory().to_path_buf();
    // The Package the diagnosis was taken against, never one inferred by counting
    // parents: the Orders Directory is configured and may sit at any depth.
    let root = diagnosis.package_root().to_path_buf();

    let write = match confirmation.action() {
        RepairAction::ReplaceManagedRecord => {
            let RepairResolution::ReplacementRecord(record) = resolution else {
                return Err(MetadataRepairError::InvalidResolution {
                    action: confirmation.action(),
                });
            };
            // A replacement is accepted only as a complete, fully valid record.
            // Unknown or invalid facts disappear only because the user confirmed a
            // record without them, never because this core removed them.
            let (document, candidate) =
                validated_record(record).ok_or(MetadataRepairError::InvalidReplacement)?;
            // Valid as an Order says nothing about the Package it lands in. A record
            // whose identity, Vendor Reference, or locator breaks a cross-record
            // constraint would answer this diagnosed condition by creating another.
            if let Some(constraint) =
                package_evidence(&root, &directory, filesystem)?.broken_by(&document)
            {
                return Err(MetadataRepairError::ReplacementDamagesPackage { constraint });
            }
            let staging = staging_path_of(&artifact);
            match filesystem.symlink_metadata(&staging) {
                Ok(None) => {}
                Ok(Some(_)) => {
                    return Err(MetadataRepairError::StagingEvidencePresent { path: staging });
                }
                Err(error) => {
                    return Err(MetadataRepairError::FileSystem {
                        path: staging,
                        error,
                    });
                }
            }
            PlannedWrite {
                destination: diagnosed_destination(&artifact, filesystem)?,
                artifact,
                candidate,
                source: None,
                kind: MetadataRepairOperationKind::ReplaceManagedArtifact,
            }
        }
        RepairAction::PromoteStagingCandidate => {
            if resolution != &RepairResolution::None {
                return Err(MetadataRepairError::InvalidResolution {
                    action: confirmation.action(),
                });
            }
            // The diagnosed candidate is the only one that may be promoted, and it is
            // reread and validated now rather than trusted from the diagnosis.
            let staged = artifact.clone();
            let destination = promoted_destination(&staged).ok_or(
                MetadataRepairError::InvalidStagingCandidate {
                    path: staged.clone(),
                },
            )?;
            let bytes =
                filesystem
                    .read(&staged)
                    .map_err(|error| MetadataRepairError::FileSystem {
                        path: staged.clone(),
                        error,
                    })?;
            let (document, candidate) =
                validated_record(&bytes).ok_or(MetadataRepairError::InvalidStagingCandidate {
                    path: staged.clone(),
                })?;
            if let Some(constraint) =
                package_evidence(&root, &directory, filesystem)?.broken_by(&document)
            {
                return Err(MetadataRepairError::StagingCandidateDamagesPackage {
                    path: staged,
                    constraint,
                });
            }
            PlannedWrite {
                destination: diagnosed_destination(&destination, filesystem)?,
                artifact: destination,
                candidate,
                source: Some(staged),
                kind: MetadataRepairOperationKind::PromoteStagingCandidate,
            }
        }
        action => return Err(MetadataRepairError::UnsupportedAction { action }),
    };

    Ok(plan_for(directory, root, confirmation, write))
}

/// The one managed write a repair performs, and the evidence it performs it against.
struct PlannedWrite {
    /// The managed artifact this repair replaces.
    artifact: PathBuf,
    candidate: Vec<u8>,
    /// What the diagnosis found at that artifact.
    destination: DiagnosedDestination,
    /// The staging candidate a promotion renames into place. Absent for an ordinary
    /// replacement, which stages its own.
    source: Option<PathBuf>,
    kind: MetadataRepairOperationKind,
}

fn plan_for(
    directory: PathBuf,
    root: PathBuf,
    confirmation: &RepairConfirmation,
    write: PlannedWrite,
) -> MetadataRepairPlan {
    let operations = vec![
        MetadataRepairOperation {
            kind: write.kind,
            path: write.artifact.clone(),
        },
        MetadataRepairOperation {
            kind: MetadataRepairOperationKind::RefreshPackageIndex,
            path: root.join(".spielorders").join("index.json"),
        },
    ];
    MetadataRepairPlan {
        directory,
        root,
        handle: confirmation.handle().to_owned(),
        action: confirmation.action(),
        artifact: write.artifact,
        candidate: write.candidate,
        destination: write.destination,
        source: write.source,
        operations,
    }
}

/// Exactly what stands at the destination now, so the write can refuse anything else.
fn diagnosed_destination<F: FileSystem>(
    destination: &Path,
    filesystem: &F,
) -> Result<DiagnosedDestination, MetadataRepairError<F::Error>> {
    let observed = filesystem.symlink_metadata(destination).map_err(|error| {
        MetadataRepairError::FileSystem {
            path: destination.to_path_buf(),
            error,
        }
    })?;
    match observed {
        None => Ok(DiagnosedDestination::Absent),
        Some(_) => filesystem
            .read(destination)
            .map(DiagnosedDestination::Content)
            .map_err(|error| MetadataRepairError::FileSystem {
                path: destination.to_path_buf(),
                error,
            }),
    }
}

/// Finds the currently diagnosed case the confirmation names, and requires that case
/// to authorize the confirmed action. A handle whose condition has since changed no
/// longer appears, so it is refused rather than acted on.
fn current_case<'a, E>(
    diagnosis: &'a OrderDiagnosis,
    confirmation: &RepairConfirmation,
) -> Result<&'a crate::order_diagnosis::RepairCase, MetadataRepairError<E>> {
    let case =
        diagnosis
            .case(confirmation.handle())
            .ok_or_else(|| MetadataRepairError::UnknownCase {
                handle: confirmation.handle().to_owned(),
            })?;
    if !case.actions().contains(&confirmation.action()) {
        return Err(MetadataRepairError::UnauthorizedAction {
            handle: confirmation.handle().to_owned(),
            action: confirmation.action(),
        });
    }
    Ok(case)
}

/// The beside-destination staging path a replacement of this artifact would use.
fn staging_path_of(artifact: &Path) -> PathBuf {
    let name = artifact
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    artifact.with_file_name(format!(".{name}.spielorders-stage"))
}

/// A staging artifact is named `.<destination>.spielorders-stage` beside the
/// destination it belongs to, so promotion never has to choose among candidates.
fn promoted_destination(staged: &Path) -> Option<PathBuf> {
    let name = staged.file_name()?.to_str()?;
    let name = name.strip_prefix('.')?.strip_suffix(".spielorders-stage")?;
    Some(staged.with_file_name(name))
}

/// Accepts bytes only as a complete record that parses and validates on its own.
/// Anything less is refused rather than partially salvaged.
///
/// Returns the parsed record alongside the bytes, because the Package-scoped
/// constraints are asked of the record's meaning while the write is of its bytes.
fn validated_record(bytes: &[u8]) -> Option<(OrderDocument, Vec<u8>)> {
    let document = OrderDocument::from_json_bytes(bytes).ok()?;
    document.validate().ok()?;
    let bytes = document.to_json_bytes().ok()?;
    Some((document, bytes))
}

/// The Package facts a candidate for one Order Directory is judged against.
///
/// Read from the Package rather than from the diagnosis, because a diagnosis answers
/// for one Order scope while these constraints are about the rest of the Package:
/// which identities the other Order Directories hold, and which Vendors exist.
struct PackageEvidence {
    root: PathBuf,
    directory: PathBuf,
    identities_held_elsewhere: Vec<OrderId>,
    vendor_ids: Vec<VendorId>,
}

/// Reads that evidence without changing anything, so it can be consulted while
/// planning.
fn package_evidence<F: FileSystem>(
    root: &Path,
    directory: &Path,
    filesystem: &F,
) -> Result<PackageEvidence, MetadataRepairError<F::Error>> {
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(root, filesystem)
    else {
        // The diagnosis this repair answers established a current Package moments
        // ago. Reaching here means that stopped holding, and a candidate no evidence
        // can clear is refused rather than written.
        return Err(MetadataRepairError::PackageEvidenceUnavailable {
            root: root.to_path_buf(),
        });
    };
    let another_order = |paths: &OrderPaths| paths.directory().absolute_path() != directory;
    // Every discovered Order Directory except the one being repaired. A quarantined
    // Order still holds the identity it was quarantined for, so consulting the
    // healthy set alone would call a contested identity free.
    let identities_held_elsewhere = package
        .healthy_orders()
        .iter()
        .filter(|order| another_order(order.paths()))
        .map(|order| order.document().order_id.clone())
        .chain(
            package
                .damaged_orders()
                .iter()
                .filter(|order| another_order(order.paths()))
                .filter_map(|order| order.order_id().cloned()),
        )
        .collect();
    let vendor_ids = package
        .vendors()
        .iter()
        .map(|vendor| vendor.vendor_id.clone())
        .collect();
    Ok(PackageEvidence {
        root: root.to_path_buf(),
        directory: directory.to_path_buf(),
        identities_held_elsewhere,
        vendor_ids,
    })
}

impl PackageEvidence {
    /// The first Package constraint this candidate breaks, when it breaks one.
    fn broken_by(&self, candidate: &OrderDocument) -> Option<PackageConstraint> {
        if self.identities_held_elsewhere.contains(&candidate.order_id) {
            return Some(PackageConstraint::OrderIdentityTaken);
        }
        if let Some(vendor_id) = &candidate.vendor_id
            && !self.vendor_ids.contains(vendor_id)
        {
            return Some(PackageConstraint::VendorReferenceUnresolved);
        }
        // Asked through the same predicate the Package loader applies to a stored
        // record, so a candidate cannot be accepted on terms the loader would reject.
        if !locator_resolves_to_root(&self.directory, &candidate.package_root, &self.root) {
            return Some(PackageConstraint::PackageRootMismatch);
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetadataRepairResult {
    diagnosis: Option<OrderDiagnosis>,
    index: Option<PackageIndexRecord>,
}

impl MetadataRepairResult {
    /// Everything still diagnosed in the repaired scope after the repair, when the
    /// scope could be rediagnosed at all.
    pub fn remaining(&self) -> Option<&OrderDiagnosis> {
        self.diagnosis.as_ref()
    }

    /// The derived index as the repair left it, when it could be refreshed at all.
    pub fn index(&self) -> Option<&PackageIndexRecord> {
        self.index.as_ref()
    }
}

/// This replacement only ever carries the candidate `validated_record` returned.
///
/// Only the replacement path reaches it at all: a confirmed replacement record is
/// validated while planning and refused there as
/// `MetadataRepairError::InvalidReplacement`. The promotion path never calls this
/// primitive — it renames the already-durable stage with `replace_file` — and it too
/// validates while planning, refusing as `MetadataRepairError::InvalidStagingCandidate`.
/// So no candidate validation failure is possible here, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum MetadataRepairRejection<E> {
    /// The staging candidate this repair was confirmed to promote changed between
    /// planning and commit, so it is no longer the candidate that was diagnosed.
    EvidenceChanged {
        path: PathBuf,
    },
    /// The destination stopped holding what the diagnosis observed there, so someone
    /// wrote it after this repair was authorized. Nothing was written and their work
    /// stands. Distinct from a write failure: nothing here failed.
    DestinationChanged {
        path: PathBuf,
        /// A staging file this refusal created and could not remove afterwards.
        cleanup_failure: Option<E>,
    },
    ReplacementFailed {
        error: ValidatedReplacementError<E, RepairCandidateFailure>,
    },
    PromotionFailed {
        path: PathBuf,
        error: E,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataRepairWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
    /// The repair itself succeeded, but the scope could not be rediagnosed
    /// afterwards, so no remaining-diagnostic answer accompanies it.
    RediagnosisUnavailable,
}

pub type MetadataRepairReport<E> = MutationReport<
    MetadataRepairPlan,
    MetadataRepairResult,
    MetadataRepairRejection<E>,
    MetadataRepairWarning,
>;

pub fn commit_metadata_repair<F: FileSystem>(
    plan: MetadataRepairPlan,
    filesystem: &F,
) -> MetadataRepairReport<F::Error> {
    // A promotion rereads its candidate immediately before acting; a stage that
    // changed since planning is no longer the candidate that was diagnosed.
    if let Some(source) = plan.source.as_ref() {
        match filesystem.read(source) {
            Ok(bytes)
                if validated_record(&bytes)
                    .is_some_and(|(_, candidate)| candidate == plan.candidate) => {}
            _ => {
                let path = source.clone();
                return MutationReport::Rejected {
                    plan,
                    rejection: MetadataRepairRejection::EvidenceChanged { path },
                };
            }
        }
    }

    match plan.source.as_ref() {
        // A staging candidate already sits durably beside its destination: it is the
        // beside-destination stage of an interrupted replacement. Promoting it is
        // completing that replacement, which also consumes the leftover rather than
        // cleaning it up separately.
        Some(source) => {
            if let Err(failure) = promote_staged_file(
                source,
                &plan.artifact,
                plan.destination.expectation(),
                filesystem,
            ) {
                let path = plan.artifact.clone();
                let rejection = match failure {
                    // Nothing failed: the destination stopped being the one this
                    // promotion was authorized against, and it is left alone.
                    StagedPromotionFailure::DestinationChanged => {
                        MetadataRepairRejection::DestinationChanged {
                            path,
                            // A promotion writes no staging file of its own, so this
                            // refusal has none to clean up.
                            cleanup_failure: None,
                        }
                    }
                    StagedPromotionFailure::DestinationUnreadable(error)
                    | StagedPromotionFailure::Durability(error)
                    | StagedPromotionFailure::Replacement(error) => {
                        MetadataRepairRejection::PromotionFailed { path, error }
                    }
                };
                return MutationReport::Rejected { plan, rejection };
            }
        }
        None => {
            let candidate = plan.candidate.clone();
            if let Err(error) = replace_validated_file_expecting(
                &plan.artifact,
                &candidate,
                plan.destination.expectation(),
                filesystem,
                |_| Ok::<(), RepairCandidateFailure>(()),
            ) {
                let rejection = if matches!(
                    error.failure(),
                    ValidatedReplacementFailure::DestinationChanged
                ) {
                    MetadataRepairRejection::DestinationChanged {
                        path: plan.artifact.clone(),
                        cleanup_failure: error.into_cleanup_failure(),
                    }
                } else {
                    MetadataRepairRejection::ReplacementFailed { error }
                };
                return MutationReport::Rejected { plan, rejection };
            }
        }
    }

    let mut warnings = Vec::new();
    let index = match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => Some(package.index().clone()),
        PackageLoad::IndexReplacementFailed(failed) => {
            warnings.push(MetadataRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            Some(failed.canonical_evidence().index().clone())
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(MetadataRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            None
        }
        _ => {
            warnings.push(MetadataRepairWarning::RefreshUnavailable);
            None
        }
    };

    let diagnosis = match diagnose_order(&plan.directory, filesystem) {
        Ok(diagnosis) => Some(diagnosis),
        Err(_) => {
            warnings.push(MetadataRepairWarning::RediagnosisUnavailable);
            None
        }
    };

    let result = MetadataRepairResult { diagnosis, index };
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
