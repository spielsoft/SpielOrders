//! `upgrade`: carrying one Package's metadata to the newest version this build knows.
//!
//! **An upgrade is not a repair.** Every artifact it reads or writes at a version this
//! repository publishes a schema for is held to that schema. What it cannot interpret it
//! pauses at, and the user decides how to resolve it — so a blocked artifact is reported
//! among the blockers rather than skipped past, and the Package marker never advances over
//! one.
//!
//! **There is no target to choose.** The target is the newest version this build knows,
//! reached only by consecutive registered steps. An arbitrary target, a per-file upgrade,
//! a downgrade, and a package-wide rollback have no command line here because they have no
//! representation in the core.
//!
//! **The running build knows exactly one durable metadata version**, so its registered
//! chain is empty and every Package this build can read is already at the newest version.
//! The orchestration is nonetheless the core's real one: this command plans and commits
//! through it rather than short-circuiting a Package it has decided in advance is
//! finished.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution; Define integrity, validation, and repair boundaries.

use crate::{
    commands::shared::{
        Failed, IndexProjection, as_given, failed_context, failed_document, human_failure,
        in_package, index, located, not_representable, package_context, resolved, text,
        undeliverable,
    },
    contract::ResponseContract,
    effects::PlatformFileSystem,
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, LocationProjection, NotRepresentable, Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    mutation::MutationReport,
    upgrade::{
        MetadataUpgradeError, MetadataUpgradePlan, MetadataUpgradeRejection, MetadataUpgradeResult,
        MetadataUpgradeWarning, TransformationRegistry, UpgradeArtifactPlan,
        commit_metadata_upgrade, plan_metadata_upgrade,
    },
};
use std::{io, path::Path};

const CONTRACT: ResponseContract = ResponseContract::Upgrade;

/// The `upgrade` payload: where this Package's metadata is, where this build would take
/// it, and what the run did.
///
/// **`artifacts` is the deterministic order the upgrade visits them in** — each Order
/// record in the Package's own order, then the Vendor aggregate — and each carries the
/// state that decides what happens to it. `blockers` is that same list narrowed to the
/// artifacts this build cannot interpret, present because a blocker is what stops a run
/// and a consumer should not have to filter for it.
#[derive(Debug, Serialize)]
struct Upgraded {
    /// The version the Package marker declares.
    source_version: u64,
    /// The newest version reachable from it by consecutive registered steps. It is never
    /// chosen by a caller.
    target_version: u64,
    /// Whether the marker already declares the target, which is the durable statement
    /// that every artifact reached it first.
    complete: bool,
    /// The Package marker: the artifact an upgrade advances last.
    marker: LocationProjection,
    artifacts: Vec<ArtifactProjection>,
    /// The artifacts this build cannot interpret, in the same order. A damaged record
    /// appears here rather than being passed over.
    blockers: Vec<ArtifactProjection>,
    report: Report,
    /// The derived index this run rebuilt, and `null` where it rebuilt none.
    ///
    /// The rebuild is the last step of an upgrade that actually carried something, so a
    /// run with nothing outstanding has no index to report. Asking what state the stored
    /// index is in is `index status`, which is the command whose subject it is — and
    /// which does not rebuild it to answer.
    index: Option<IndexProjection>,
}

/// One managed artifact an upgrade visits, and what this build makes of it.
#[derive(Debug, Serialize)]
struct ArtifactProjection {
    artifact: &'static str,
    state: &'static str,
    location: LocationProjection,
}

impl ArtifactProjection {
    fn of(artifact: &UpgradeArtifactPlan) -> Result<Self, NotRepresentable> {
        Ok(Self {
            artifact: artifact.artifact().as_str(),
            state: artifact.state().as_str(),
            location: LocationProjection::of(artifact.location())?,
        })
    }
}

/// The typed upgrade report, in the shape every mutation's report has.
///
/// `committed` is whether the plan was carried out, and it is the one member `--dry-run`
/// decides. `changed` is whether this run would move the Package at all: a Package whose
/// marker already declares the target has nothing outstanding, and reporting a run over it
/// as a change would tell a caller that something happened.
///
/// `upgraded` and `skipped` are the artifacts this run carried and the artifacts it found
/// already carried, in the order it visited them. `operations` is every durable write the
/// plan will attempt, in the core's own vocabulary.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    /// The version the Package marker declares after this run.
    reached_version: u64,
    marker_advanced: bool,
    upgraded: Vec<LocationProjection>,
    skipped: Vec<LocationProjection>,
    operations: Vec<OperationProjection>,
    /// Present and empty for a run that reached its target. A paused run names where it
    /// paused.
    warnings: Vec<&'static str>,
}

/// One planned durable write, in the core's own vocabulary.
#[derive(Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    location: LocationProjection,
}

/// What the command decided, before anything is written anywhere.
enum Upgrade<'a> {
    Planned {
        plan: Box<MetadataUpgradePlan<'a>>,
        /// What the commit did, or `None` for a run asked to plan only.
        result: Option<Box<MetadataUpgradeResult>>,
        warnings: Vec<&'static str>,
    },
    Failed(Box<Failed>),
}

pub(crate) fn upgrade(selected_root: &Path, dry_run: bool, output: &Output) -> Exit {
    let registry = TransformationRegistry::registered();
    present(decide(selected_root, dry_run, &registry), output)
}

fn decide<'a>(
    selected_root: &Path,
    dry_run: bool,
    registry: &TransformationRegistry<'a>,
) -> Upgrade<'a> {
    let plan = match plan_metadata_upgrade(selected_root, registry, &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => return Upgrade::Failed(Box::new(refusal(selected_root, reason))),
    };
    if dry_run {
        return Upgrade::Planned {
            plan: Box::new(plan),
            result: None,
            warnings: Vec::new(),
        };
    }
    match commit_metadata_upgrade(plan, &PlatformFileSystem) {
        // A Package already at the target is `Unchanged`, and one this run carried there
        // is `Committed`. Both are a plan and a result, so folding them is not a claim
        // about either; `changed` is where the difference is reported.
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            Upgrade::Planned {
                plan: Box::new(plan),
                result: Some(Box::new(result)),
                warnings: Vec::new(),
            }
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => Upgrade::Planned {
            plan: Box::new(plan),
            result: Some(Box::new(result)),
            warnings: warnings.iter().map(warning).collect(),
        },
        MutationReport::Rejected { plan, rejection } => {
            Upgrade::Failed(Box::new(rejected(&plan, rejection)))
        }
    }
}

/// One paused or degraded run, under its own stable spelling.
///
/// **Every variant is named rather than covered by a wildcard**, so a warning added to the
/// core is a compile error here instead of a run reported as clean.
fn warning<E>(warning: &MetadataUpgradeWarning<E>) -> &'static str {
    match warning {
        MetadataUpgradeWarning::PausedAtBlockedArtifact { .. } => "paused_at_blocked_artifact",
        MetadataUpgradeWarning::PausedAtRefusedTransformation { .. } => {
            "paused_at_refused_transformation"
        }
        MetadataUpgradeWarning::PausedAtUnusableOutput { .. } => "paused_at_unusable_output",
        MetadataUpgradeWarning::StoppedAtFailedArtifact { .. } => "stopped_at_failed_artifact",
        MetadataUpgradeWarning::MarkerAdvanceFailed { .. } => "marker_advance_failed",
        MetadataUpgradeWarning::IndexRefreshFailed(_) => "index_refresh_failed",
        MetadataUpgradeWarning::RefreshUnavailable => "refresh_unavailable",
    }
}

/// Why no upgrade could be planned.
///
/// A directory that is not a Root is classified through the shared constructor, so this
/// command answers it exactly as every other command does. The rest are this command's
/// own: a marker it will not interpret, a chain it does not have, and a configuration it
/// will not guess past.
///
/// **A refusal raised after the Root resolved names that Root**, in `paths` with the
/// Package's own spelling for the artifact and in `package_root` beside it. The three that
/// name neither are the three the core raises before there is a Package: an incoherent
/// registered chain, a directory holding no marker, and a Root that would not resolve.
fn refusal<E: std::fmt::Display>(selected_root: &Path, reason: MetadataUpgradeError<E>) -> Failed {
    match reason {
        MetadataUpgradeError::UnmanagedPackage { path } => Failed {
            code: FailureCode::PackageRootUnmanaged,
            scope: FailureScope::Package,
            message: format!(
                "{} is not a SpielOrders Root: it has no Package marker. --root names the \
                 Root itself and no ancestor of it is consulted.",
                path.display()
            ),
            // No Package surrounds it: having no Package marker is what refuses it.
            paths: vec![resolved(&path)],
            identities: Vec::new(),
            // A resolved directory, but not a Root. Naming it here would report a Package
            // that is precisely what is absent.
            package_root: None,
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::UnreadableMarker { marker } => Failed {
            code: FailureCode::PackageMetadataUnsupported,
            scope: FailureScope::Package,
            message: format!(
                "the Package marker at {} declares no metadata version, so there is no \
                 version to carry this Package from. It is diagnosed rather than \
                 interpreted; `spielorders validate` reports it.",
                marker.location().absolute_path().display()
            ),
            paths: vec![in_package(marker.location())],
            identities: Vec::new(),
            package_root: Some(marker.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::NewerThanBuild {
            observed,
            newest,
            root,
        } => Failed {
            code: FailureCode::PackageMetadataUnsupported,
            scope: FailureScope::Package,
            message: format!(
                "this Package declares metadata version {observed} and this build reads \
                 version {newest}. It is left untouched: a newer build reads it, and \
                 nothing here downgrades a Package.",
            ),
            paths: vec![in_package(root.location())],
            identities: Vec::new(),
            package_root: Some(root.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::NoCompleteChain {
            observed,
            reachable,
            root,
        } => Failed {
            code: FailureCode::PackageMetadataUnsupported,
            scope: FailureScope::Package,
            message: format!(
                "this Package declares metadata version {observed} and this build has no \
                 complete consecutive chain from it — it reaches version {reachable}. It \
                 is left untouched rather than partially migrated.",
            ),
            paths: vec![in_package(root.location())],
            identities: Vec::new(),
            package_root: Some(root.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::IncoherentChain { version } => Failed {
            code: FailureCode::PackageMetadataUnsupported,
            scope: FailureScope::Package,
            message: format!(
                "two registered transformations both read metadata version {version}, so \
                 this build has two answers for one step. Nothing was read and nothing was \
                 migrated: which transformations this build ships is not a question a \
                 Package can settle.",
            ),
            // The answer is about this build, so the core reaches it before resolving
            // anything. The caller's own text is all there honestly is to report.
            paths: vec![as_given(selected_root)],
            identities: Vec::new(),
            package_root: None,
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::UnusableConfiguration { configuration } => Failed {
            code: FailureCode::UpgradeConfigurationUnusable,
            scope: FailureScope::Package,
            message: format!(
                "the Package configuration at {} does not name a usable Orders Directory, \
                 so this upgrade refuses rather than assuming the conventional location: \
                 assuming wrong would find nothing pending and advance the marker past \
                 records it never migrated. Nothing was written.",
                configuration.location().absolute_path().display()
            ),
            paths: vec![in_package(configuration.location())],
            identities: Vec::new(),
            package_root: Some(configuration.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::FileSystem { subject, error } => Failed {
            code: FailureCode::PackageUnreadable,
            scope: FailureScope::Operation,
            message: format!(
                "{} could not be examined: {error}. Nothing about this Package's metadata \
                 version was decided.",
                subject.location().absolute_path().display()
            ),
            paths: vec![in_package(subject.location())],
            identities: Vec::new(),
            package_root: Some(subject.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataUpgradeError::UnresolvableRoot { path, error } => Failed {
            code: FailureCode::PackageUnreadable,
            scope: FailureScope::Operation,
            message: format!(
                "{} could not be resolved: {error}. Nothing about this Package's metadata \
                 version was decided.",
                path.display()
            ),
            // The resolution is what failed, so the caller's own text is all this
            // application has for it and there is no Root to name it under.
            paths: vec![as_given(&path)],
            identities: Vec::new(),
            package_root: None,
            diagnostics: Vec::new(),
        },
    }
}

/// An upgrade that wrote nothing at all. The Package holds what it held.
fn rejected<E: std::fmt::Display>(
    plan: &MetadataUpgradePlan<'_>,
    rejection: MetadataUpgradeRejection<E>,
) -> Failed {
    let (location, subject) = match &rejection {
        MetadataUpgradeRejection::ArtifactReplacementFailed { artifact, .. } => {
            (artifact, "record")
        }
        MetadataUpgradeRejection::MarkerAdvanceFailed { marker, .. } => (marker, "marker"),
    };
    Failed {
        code: FailureCode::UpgradeFailed,
        scope: FailureScope::Package,
        message: format!(
            "the Metadata Upgrade of the Package at {} did not start: the {subject} at {} \
             could not be replaced. Every managed file holds what it held, and rerunning \
             resumes.",
            plan.root().display(),
            location.absolute_path().display()
        ),
        paths: vec![location.clone().into()],
        identities: Vec::new(),
        package_root: Some(plan.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn present(upgrade: Upgrade<'_>, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match document(&upgrade) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(CONTRACT, package_context(root(&upgrade))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human(&upgrade, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn root<'a>(upgrade: &'a Upgrade<'_>) -> Option<&'a Path> {
    match upgrade {
        Upgrade::Planned { plan, .. } => Some(plan.root()),
        Upgrade::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn document(upgrade: &Upgrade<'_>) -> Result<(Response<Upgraded>, Exit), NotRepresentable> {
    let (plan, result, warnings) = match upgrade {
        Upgrade::Planned {
            plan,
            result,
            warnings,
        } => (plan, result, warnings),
        Upgrade::Failed(failed) => {
            return Ok((
                failed_document(CONTRACT, failed_context(failed)?, failed)?,
                Exit::Failure,
            ));
        }
    };
    Ok((
        Response::answered(
            CONTRACT,
            Context::rooted_at(plan.root())?,
            upgraded(plan, result.as_deref(), warnings)?,
            // The upgrade reads no Package to assess. What it observed about the
            // artifacts is `artifacts` and `blockers`, which is this command's answer
            // rather than a second report of Package health.
            Vec::new(),
        ),
        Exit::Success,
    ))
}

fn upgraded(
    plan: &MetadataUpgradePlan<'_>,
    result: Option<&MetadataUpgradeResult>,
    warnings: &[&'static str],
) -> Result<Upgraded, NotRepresentable> {
    let locations = |locations: &[spielorders_core::readme::Location]| {
        locations
            .iter()
            .map(LocationProjection::of)
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(Upgraded {
        source_version: plan.source_version(),
        target_version: plan.target_version(),
        complete: plan.is_complete(),
        marker: LocationProjection::of(plan.marker())?,
        artifacts: plan
            .artifacts()
            .iter()
            .map(ArtifactProjection::of)
            .collect::<Result<_, _>>()?,
        blockers: plan
            .blockers()
            .into_iter()
            .map(ArtifactProjection::of)
            .collect::<Result<_, _>>()?,
        report: Report {
            committed: result.is_some(),
            changed: !plan.is_complete(),
            reached_version: result.map_or_else(
                || plan.source_version(),
                MetadataUpgradeResult::reached_version,
            ),
            marker_advanced: result.is_some_and(MetadataUpgradeResult::marker_advanced),
            upgraded: locations(result.map_or(&[], MetadataUpgradeResult::upgraded))?,
            skipped: locations(result.map_or(&[], MetadataUpgradeResult::skipped))?,
            operations: plan
                .operations()
                .iter()
                .map(|operation| {
                    Ok(OperationProjection {
                        operation: operation.kind().as_str(),
                        location: LocationProjection::of(operation.location())?,
                    })
                })
                .collect::<Result<_, NotRepresentable>>()?,
            warnings: warnings.to_vec(),
        },
        index: result
            .and_then(MetadataUpgradeResult::index)
            .map(index)
            .transpose()?,
    })
}

fn human(upgrade: &Upgrade<'_>, output: &Output) -> io::Result<Exit> {
    let (plan, result) = match upgrade {
        Upgrade::Planned { plan, result, .. } => (plan, result),
        Upgrade::Failed(failed) => return human_failure(failed, output),
    };
    // A dry run says what it *would* do, because a reader who skimmed one line of a plan
    // and took it for a result is the whole hazard of reporting a plan in a result's
    // shape.
    output.summary(
        match (result.is_some(), plan.is_complete()) {
            (true, true) => "Already at metadata version",
            (true, false) => "Carried to metadata version",
            (false, true) => "Would leave metadata version",
            (false, false) => "Would carry to metadata version",
        },
        &text(&plan.target_version().to_string()),
    )?;
    output.summary(
        "Package metadata version",
        &text(&plan.source_version().to_string()),
    )?;
    output.summary("Artifacts", &text(&plan.artifacts().len().to_string()))?;
    for artifact in plan.artifacts() {
        output.detail(
            artifact.artifact().as_str(),
            &text(&format!(
                "{}: {}",
                artifact.state().as_str(),
                located(artifact.location()).to_string_lossy()
            )),
        )?;
    }
    // The command a blocked artifact earns is not this command's to name. Whether one has
    // a safe action at all is the core's answer, taken from a diagnosis rather than from a
    // plan: an artifact declaring a version this build will not interpret has none, and
    // sending a reader to `repair` for it would be a suggestion to run a command that
    // answers nothing. `validate` reports the finding and names its own command with it.
    for blocked in plan.blockers() {
        output.warning(&format!(
            "this build cannot interpret {} at the version it declares, so an upgrade \
             would pause there. `spielorders validate` reports it and says what can be \
             done about it.",
            blocked.path().display()
        ))?;
    }
    output.detail("Package Root", plan.root().as_os_str())?;
    Ok(Exit::Success)
}
