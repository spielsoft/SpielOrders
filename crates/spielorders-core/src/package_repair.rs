//! Applying exactly one currently diagnosed, user-confirmed repair of a Package's own
//! managed artifact.
//!
//! This capability is [`crate::metadata_repair`]'s counterpart at the Package's own
//! scope: it executes only what [`crate::package_diagnosis`] authorized, for one case at
//! a time, against evidence it revalidates immediately before acting. It never salvages
//! malformed JSON, guesses a missing fact, selects among staging candidates, or cleans
//! anything up on its own.
//!
//! **Which candidate is safe is decided here, per artifact kind.** A Package's four
//! managed artifacts are four different documents — the marker, the configuration, the
//! Vendor aggregate, and the derived index — and each is held to its own schema and its
//! own semantics before a byte is written. An adapter judging that would be a second place
//! deciding what this core may publish.

use crate::{
    canonical::{PackageIndexRecord, PackageLoad, open_package},
    filesystem::{
        DestinationExpectation, StagedPromotionFailure, ValidatedReplacementError,
        ValidatedReplacementFailure, promote_staged_file, replace_validated_file_expecting,
    },
    metadata::{Config, IndexDocument, VendorsDocument, VersionDocument},
    metadata_repair::{RepairConfirmation, RepairResolution},
    mutation::MutationReport,
    operations::OperationFailure,
    package::vendor_aggregate_is_package_valid,
    package_diagnosis::{PackageDiagnosis, PackageDiagnosisError, diagnose_package},
    paths::resolve_package_layout,
    ports::FileSystem,
    readme::{Location, RootedLocation, located_in_package, rooted_at},
    repair_cases::{RepairAction, RepairCase},
};
use std::path::{Path, PathBuf};

/// Which managed Package artifact a candidate would become.
///
/// The kind decides the judgement, so it is resolved from the destination's own managed
/// name rather than supplied: `.spielorders/` holds exactly these four and nothing else is
/// a destination this capability writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageArtifact {
    /// `.spielorders/version.json` — the Package marker.
    Marker,
    /// `.spielorders/config.toml` — the configured managed paths.
    Configuration,
    /// `.spielorders/vendors.json` — the complete Vendor aggregate.
    VendorAggregate,
    /// `.spielorders/index.json` — the derived Package Index.
    DerivedIndex,
}

impl PackageArtifact {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "marker",
            Self::Configuration => "configuration",
            Self::VendorAggregate => "vendor_aggregate",
            Self::DerivedIndex => "derived_index",
        }
    }

    fn of(destination: &Path) -> Option<Self> {
        match destination.file_name()?.to_str()? {
            "version.json" => Some(Self::Marker),
            "config.toml" => Some(Self::Configuration),
            "vendors.json" => Some(Self::VendorAggregate),
            "index.json" => Some(Self::DerivedIndex),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageRepairOperationKind {
    ReplaceManagedArtifact,
    PromoteStagingCandidate,
    RefreshPackageIndex,
}

impl PackageRepairOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceManagedArtifact => "replace_managed_artifact",
            Self::PromoteStagingCandidate => "promote_staging_candidate",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRepairOperation {
    kind: PackageRepairOperationKind,
    location: Location,
}

impl PackageRepairOperation {
    pub fn kind(&self) -> PackageRepairOperationKind {
        self.kind
    }

    /// Where this operation writes, as the Package that holds it names it.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

/// What the diagnosis found at the destination this repair writes.
///
/// Carried into the write so that a change arriving after the diagnosis is refused rather
/// than overwritten. Absence is one of the two answers rather than a gap: an interrupted
/// replacement can leave a durable stage beside an artifact it never created, and
/// promoting that candidate is exactly the repair offered for it.
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

/// One complete, validated repair of one diagnosed Package-level case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRepairPlan {
    root: PathBuf,
    handle: String,
    action: RepairAction,
    artifact: PackageArtifact,
    destination_path: PathBuf,
    candidate: Vec<u8>,
    destination: DiagnosedDestination,
    source: Option<PathBuf>,
    operations: Vec<PackageRepairOperation>,
}

impl PackageRepairPlan {
    /// The SpielOrders Root this repair acts in.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The exact diagnosed case this repair carries out.
    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn action(&self) -> RepairAction {
        self.action
    }

    /// Which managed Package artifact this repair publishes.
    pub fn artifact(&self) -> PackageArtifact {
        self.artifact
    }

    /// The staging candidate a promotion reads its bytes from, when the action is a
    /// promotion.
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn operations(&self) -> &[PackageRepairOperation] {
        &self.operations
    }
}

/// Why no Package-level repair could be planned.
///
/// **Every refusal below the diagnosis carries a [`RootedLocation`]**, because every one of
/// them is raised after [`diagnose_package`] established the Root: the scope was resolved,
/// so the response owes both the Root it was resolved as and the Package's own spelling of
/// whatever the refusal is about. Only [`Self::Diagnosis`] is raised before there is a
/// Package to name, and it carries the load's own answer to that.
#[derive(Debug, PartialEq)]
pub enum PackageRepairError<E> {
    /// The Package's own scope could not be diagnosed at all.
    Diagnosis(PackageDiagnosisError),
    /// No currently diagnosed case answers to this handle. A corrected or changed
    /// condition stops appearing, so a handle from an older diagnosis lands here.
    UnknownCase {
        handle: String,
        scope: RootedLocation,
    },
    /// The named case exists but does not authorize this action.
    UnauthorizedAction {
        handle: String,
        action: RepairAction,
        scope: RootedLocation,
    },
    /// This capability applies only what a Package's own artifacts authorize; the
    /// semantic resolutions belong to one Order and to their own capability.
    UnsupportedAction {
        action: RepairAction,
        scope: RootedLocation,
    },
    /// The supplied resolution does not match what the action requires.
    InvalidResolution {
        action: RepairAction,
        scope: RootedLocation,
    },
    /// The destination is not one of this Package's four managed artifacts, so there is
    /// no kind to judge a candidate against and nothing here writes it.
    UnrecognizedArtifact {
        destination: RootedLocation,
    },
    /// A supplied replacement is not a complete valid artifact of its kind. Nothing is
    /// written; a partial salvage is never made.
    InvalidReplacement {
        scope: RootedLocation,
    },
    /// The diagnosed staging candidate is not itself a complete valid artifact of its
    /// kind, so it is never promoted.
    InvalidStagingCandidate {
        staged: RootedLocation,
    },
    /// A replacement stages beside its destination, and abandoned staging evidence already
    /// occupies that path. Writing there would destroy evidence this core is forbidden to
    /// overwrite or silently clean, so the staging case is resolved first and the
    /// replacement is refused until then.
    StagingEvidencePresent {
        staged: RootedLocation,
    },
    FileSystem {
        subject: RootedLocation,
        error: E,
    },
}

/// Plans exactly one confirmed Package-level repair, revalidating the case and the
/// supplied facts against current evidence before proposing any effect.
pub fn plan_package_repair<F: FileSystem>(
    selected_root: &Path,
    confirmation: &RepairConfirmation,
    resolution: &RepairResolution,
    filesystem: &F,
) -> Result<PackageRepairPlan, PackageRepairError<F::Error>> {
    let diagnosis =
        diagnose_package(selected_root, filesystem).map_err(PackageRepairError::Diagnosis)?;
    let root = diagnosis.root().absolute_path().to_path_buf();
    let scope = || rooted_at(&root, root.clone());
    let case = current_case(&diagnosis, confirmation, &scope)?;
    let evidence = case.location().absolute_path().to_path_buf();

    let write =
        match confirmation.action() {
            RepairAction::ReplaceManagedRecord => {
                let RepairResolution::ReplacementRecord(record) = resolution else {
                    return Err(PackageRepairError::InvalidResolution {
                        action: confirmation.action(),
                        scope: scope(),
                    });
                };
                let artifact = PackageArtifact::of(&evidence).ok_or_else(|| {
                    PackageRepairError::UnrecognizedArtifact {
                        destination: rooted_at(&root, evidence.clone()),
                    }
                })?;
                // A replacement is accepted only as a complete, fully valid artifact of its
                // own kind. Unknown or invalid facts disappear only because the user confirmed
                // an artifact without them, never because this core removed them.
                let candidate = validated_candidate(artifact, record, &root, filesystem)
                    .ok_or_else(|| PackageRepairError::InvalidReplacement { scope: scope() })?;
                let staging = staging_path_of(&evidence);
                match filesystem.symlink_metadata(&staging) {
                    Ok(None) => {}
                    Ok(Some(_)) => {
                        return Err(PackageRepairError::StagingEvidencePresent {
                            staged: rooted_at(&root, staging),
                        });
                    }
                    Err(error) => {
                        return Err(PackageRepairError::FileSystem {
                            subject: rooted_at(&root, staging),
                            error,
                        });
                    }
                }
                PlannedWrite {
                    destination: diagnosed_destination(&root, &evidence, filesystem)?,
                    destination_path: evidence,
                    artifact,
                    candidate,
                    source: None,
                    kind: PackageRepairOperationKind::ReplaceManagedArtifact,
                }
            }
            RepairAction::PromoteStagingCandidate => {
                if resolution != &RepairResolution::None {
                    return Err(PackageRepairError::InvalidResolution {
                        action: confirmation.action(),
                        scope: scope(),
                    });
                }
                // The diagnosed candidate is the only one that may be promoted, and it is
                // reread and validated now rather than trusted from the diagnosis.
                let staged = evidence;
                let destination = promoted_destination(&staged).ok_or_else(|| {
                    PackageRepairError::InvalidStagingCandidate {
                        staged: rooted_at(&root, staged.clone()),
                    }
                })?;
                let artifact = PackageArtifact::of(&destination).ok_or_else(|| {
                    PackageRepairError::UnrecognizedArtifact {
                        destination: rooted_at(&root, destination.clone()),
                    }
                })?;
                let bytes =
                    filesystem
                        .read(&staged)
                        .map_err(|error| PackageRepairError::FileSystem {
                            subject: rooted_at(&root, staged.clone()),
                            error,
                        })?;
                let candidate = validated_candidate(artifact, &bytes, &root, filesystem)
                    .ok_or_else(|| PackageRepairError::InvalidStagingCandidate {
                        staged: rooted_at(&root, staged.clone()),
                    })?;
                PlannedWrite {
                    destination: diagnosed_destination(&root, &destination, filesystem)?,
                    destination_path: destination,
                    artifact,
                    candidate,
                    source: Some(staged),
                    kind: PackageRepairOperationKind::PromoteStagingCandidate,
                }
            }
            action => {
                return Err(PackageRepairError::UnsupportedAction {
                    action,
                    scope: scope(),
                });
            }
        };

    Ok(plan_for(&diagnosis, root, confirmation, write))
}

/// The one managed write a repair performs, and the evidence it performs it against.
struct PlannedWrite {
    artifact: PackageArtifact,
    destination_path: PathBuf,
    candidate: Vec<u8>,
    destination: DiagnosedDestination,
    /// The staging candidate a promotion renames into place. Absent for an ordinary
    /// replacement, which stages its own.
    source: Option<PathBuf>,
    kind: PackageRepairOperationKind,
}

fn plan_for(
    diagnosis: &PackageDiagnosis,
    root: PathBuf,
    confirmation: &RepairConfirmation,
    write: PlannedWrite,
) -> PackageRepairPlan {
    let operations = vec![
        PackageRepairOperation {
            kind: write.kind,
            location: located(diagnosis, &write.destination_path),
        },
        PackageRepairOperation {
            kind: PackageRepairOperationKind::RefreshPackageIndex,
            location: diagnosis.context().index().clone(),
        },
    ];
    PackageRepairPlan {
        root,
        handle: confirmation.handle().to_owned(),
        action: confirmation.action(),
        artifact: write.artifact,
        destination_path: write.destination_path,
        candidate: write.candidate,
        destination: write.destination,
        source: write.source,
        operations,
    }
}

/// The Package's own name for one of its four managed artifacts.
///
/// Taken from the loaded context where it has it, so a path this repair reports carries
/// the Package-relative spelling the rest of the load already decided on, and composed
/// through the one helper that answers that question otherwise.
fn located(diagnosis: &PackageDiagnosis, destination: &Path) -> Location {
    let context = diagnosis.context();
    for candidate in [
        context.version(),
        context.config(),
        context.vendors(),
        context.index(),
    ] {
        if candidate.absolute_path() == destination {
            return candidate.clone();
        }
    }
    located_in_package(context.root().absolute_path(), destination.to_path_buf())
}

/// Exactly what stands at the destination now, so the write can refuse anything else.
fn diagnosed_destination<F: FileSystem>(
    root: &Path,
    destination: &Path,
    filesystem: &F,
) -> Result<DiagnosedDestination, PackageRepairError<F::Error>> {
    let unreadable = |error| PackageRepairError::FileSystem {
        subject: rooted_at(root, destination.to_path_buf()),
        error,
    };
    let observed = filesystem
        .symlink_metadata(destination)
        .map_err(unreadable)?;
    match observed {
        None => Ok(DiagnosedDestination::Absent),
        Some(_) => filesystem
            .read(destination)
            .map(DiagnosedDestination::Content)
            .map_err(unreadable),
    }
}

/// Finds the currently diagnosed case the confirmation names, and requires that case to
/// authorize the confirmed action. A handle whose condition has since changed no longer
/// appears, so it is refused rather than acted on.
fn current_case<'a, E>(
    diagnosis: &'a PackageDiagnosis,
    confirmation: &RepairConfirmation,
    scope: &impl Fn() -> RootedLocation,
) -> Result<&'a RepairCase, PackageRepairError<E>> {
    let case =
        diagnosis
            .case(confirmation.handle())
            .ok_or_else(|| PackageRepairError::UnknownCase {
                handle: confirmation.handle().to_owned(),
                scope: scope(),
            })?;
    if !case.actions().contains(&confirmation.action()) {
        return Err(PackageRepairError::UnauthorizedAction {
            handle: confirmation.handle().to_owned(),
            action: confirmation.action(),
            scope: scope(),
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

/// A staging artifact is named `.<destination>.spielorders-stage` beside the destination
/// it belongs to, so promotion never has to choose among candidates.
fn promoted_destination(staged: &Path) -> Option<PathBuf> {
    let name = staged.file_name()?.to_str()?;
    let name = name.strip_prefix('.')?.strip_suffix(".spielorders-stage")?;
    Some(staged.with_file_name(name))
}

/// Accepts bytes only as a complete artifact of this kind that parses and satisfies that
/// kind's own semantics. Anything less is refused rather than partially salvaged.
///
/// **Four kinds, four judgements**, each the one this repository already publishes for
/// that artifact. The Vendor aggregate is put to
/// [`vendor_aggregate_is_package_valid`](crate::package); the configuration is resolved as
/// this Package's layout, because a configured path that escapes the Root or occupies the
/// reserved metadata subtree is exactly what damages a Package; and the marker's declared
/// version is settled by the type it deserializes into.
///
/// The canonical bytes are returned rather than the caller's, so what is published is the
/// document this build round-trips — with the derived index the exception, whose bytes a
/// rebuild owns and which is therefore republished exactly as the candidate had them.
fn validated_candidate<F: FileSystem>(
    artifact: PackageArtifact,
    bytes: &[u8],
    root: &Path,
    filesystem: &F,
) -> Option<Vec<u8>> {
    match artifact {
        PackageArtifact::Marker => VersionDocument::from_json_bytes(bytes)
            .ok()?
            .to_json_bytes()
            .ok(),
        PackageArtifact::Configuration => {
            let config = Config::from_toml_bytes(bytes).ok()?;
            // Resolved as this Package's own layout, because a configured path that
            // escapes the Root, overlaps its sibling, or occupies the reserved metadata
            // subtree is exactly what damages a Package. A filesystem that would not
            // answer is refused for the same reason a broken path is: nothing shows this
            // configuration is usable, and the safe answer to both is no.
            resolve_package_layout(root, &config, filesystem).ok()?;
            // The user's own TOML, because a configuration is a document a person edits
            // and this core does not reformat one to publish it.
            Some(bytes.to_vec())
        }
        PackageArtifact::VendorAggregate => {
            let vendors = VendorsDocument::from_json_bytes(bytes).ok()?;
            vendor_aggregate_is_package_valid(&vendors)
                .then(|| vendors.to_json_bytes().ok())
                .flatten()
        }
        PackageArtifact::DerivedIndex => {
            IndexDocument::from_json_bytes(bytes).ok()?;
            Some(bytes.to_vec())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageRepairResult {
    diagnosis: Option<PackageDiagnosis>,
    index: Option<PackageIndexRecord>,
}

impl PackageRepairResult {
    /// Everything still diagnosed in the repaired scope afterwards, when the scope could
    /// be rediagnosed at all.
    pub fn remaining(&self) -> Option<&PackageDiagnosis> {
        self.diagnosis.as_ref()
    }

    /// The derived index as the repair left it, when it could be refreshed at all.
    pub fn index(&self) -> Option<&PackageIndexRecord> {
        self.index.as_ref()
    }
}

/// This replacement only ever carries the candidate `validated_candidate` returned.
///
/// Only the replacement path reaches it at all: a confirmed replacement is validated while
/// planning and refused there as [`PackageRepairError::InvalidReplacement`]. The promotion
/// path never calls this primitive — it renames the already-durable stage with
/// `replace_file` — and it too validates while planning. So no candidate validation
/// failure is possible here, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageRepairCandidateFailure {}

/// Why a planned repair was refused at the write, or failed there.
///
/// **Every one carries a [`RootedLocation`]**, for the same reason
/// [`PackageRepairError`] does: a rejection is raised with the plan in hand, so the Root
/// and the Package's own spelling of the path are both known and a response owes both.
#[derive(Debug, PartialEq)]
pub enum PackageRepairRejection<E> {
    /// The staging candidate this repair was confirmed to promote changed between planning
    /// and commit, so it is no longer the candidate that was diagnosed.
    EvidenceChanged { evidence: RootedLocation },
    /// The destination stopped holding what the diagnosis observed there, so someone wrote
    /// it after this repair was authorized. Nothing was written and their work stands.
    /// Distinct from a write failure: nothing here failed.
    DestinationChanged {
        destination: RootedLocation,
        /// The staging file this refusal created and could not remove again, which is
        /// what remains on disk — the destination holds exactly what it held. `None`
        /// where the refusal left nothing behind.
        uncleaned_stage: Option<RootedLocation>,
    },
    ReplacementFailed {
        destination: RootedLocation,
        /// The staging file this failure created and could not remove again, exactly as
        /// on a refusal.
        uncleaned_stage: Option<RootedLocation>,
        error: ValidatedReplacementError<E, PackageRepairCandidateFailure>,
    },
    PromotionFailed {
        destination: RootedLocation,
        error: E,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageRepairWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
    /// The repair itself succeeded, but the scope could not be rediagnosed afterwards, so
    /// no remaining-diagnostic answer accompanies it.
    RediagnosisUnavailable,
}

pub type PackageRepairReport<E> = MutationReport<
    PackageRepairPlan,
    PackageRepairResult,
    PackageRepairRejection<E>,
    PackageRepairWarning,
>;

pub fn commit_package_repair<F: FileSystem>(
    plan: PackageRepairPlan,
    filesystem: &F,
) -> PackageRepairReport<F::Error> {
    // A promotion rereads its candidate immediately before acting; a stage that changed
    // since planning is no longer the candidate that was diagnosed.
    if let Some(source) = plan.source.as_ref() {
        match filesystem.read(source) {
            Ok(bytes)
                if validated_candidate(plan.artifact, &bytes, &plan.root, filesystem)
                    .is_some_and(|candidate| candidate == plan.candidate) => {}
            _ => {
                let evidence = rooted_at(&plan.root, source.clone());
                return MutationReport::Rejected {
                    plan,
                    rejection: PackageRepairRejection::EvidenceChanged { evidence },
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
                &plan.destination_path,
                plan.destination.expectation(),
                filesystem,
            ) {
                let destination = rooted_at(&plan.root, plan.destination_path.clone());
                let rejection = match failure {
                    // Nothing failed: the destination stopped being the one this promotion
                    // was authorized against, and it is left alone.
                    StagedPromotionFailure::DestinationChanged => {
                        PackageRepairRejection::DestinationChanged {
                            destination,
                            // A promotion writes no staging file of its own, so this
                            // refusal has none left over.
                            uncleaned_stage: None,
                        }
                    }
                    StagedPromotionFailure::DestinationUnreadable(error)
                    | StagedPromotionFailure::Durability(error)
                    | StagedPromotionFailure::Replacement(error) => {
                        PackageRepairRejection::PromotionFailed { destination, error }
                    }
                };
                return MutationReport::Rejected { plan, rejection };
            }
        }
        None => {
            let candidate = plan.candidate.clone();
            if let Err(error) = replace_validated_file_expecting(
                &plan.destination_path,
                &candidate,
                plan.destination.expectation(),
                filesystem,
                |_| Ok::<(), PackageRepairCandidateFailure>(()),
            ) {
                let destination = rooted_at(&plan.root, plan.destination_path.clone());
                let uncleaned_stage = error
                    .cleanup_failure()
                    .map(|_| rooted_at(&plan.root, staging_path_of(&plan.destination_path)));
                let rejection = if matches!(
                    error.failure(),
                    ValidatedReplacementFailure::DestinationChanged
                ) {
                    PackageRepairRejection::DestinationChanged {
                        destination,
                        uncleaned_stage,
                    }
                } else {
                    PackageRepairRejection::ReplacementFailed {
                        destination,
                        uncleaned_stage,
                        error,
                    }
                };
                return MutationReport::Rejected { plan, rejection };
            }
        }
    }

    let mut warnings = Vec::new();
    let index = match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => Some(package.index().clone()),
        PackageLoad::IndexReplacementFailed(failed) => {
            warnings.push(PackageRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            Some(failed.canonical_evidence().index().clone())
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(PackageRepairWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
            None
        }
        _ => {
            warnings.push(PackageRepairWarning::RefreshUnavailable);
            None
        }
    };

    let diagnosis = match diagnose_package(&plan.root, filesystem) {
        Ok(diagnosis) => Some(diagnosis),
        Err(_) => {
            warnings.push(PackageRepairWarning::RediagnosisUnavailable);
            None
        }
    };

    let result = PackageRepairResult { diagnosis, index };
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
