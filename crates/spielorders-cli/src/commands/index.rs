//! `index status` and `index rebuild`: the two commands whose subject is the derived
//! Package Index.
//!
//! **The index is wholly derived, so it is rebuilt rather than repaired.** It is the one
//! managed artifact this application may replace on its own, because every fact in it
//! comes from a canonical record — and neither command here touches a canonical record or
//! inspects Supporting Material. A rebuild is never the answer to canonical damage, and
//! nothing here suggests it as one.
//!
//! **`index status` writes nothing.** Asking what state the index is in must not be the
//! thing that changes it: the load that assesses *and rebuilds* is reserved for the place
//! a canonical write has already happened, and a command that reported "current" because
//! asking made it current would answer a question nobody asked.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::{
    commands::shared::{
        Failed, IndexProjection, MetadataProjection, RootKind, as_given,
        assessed_package_candidate, damaged_failure, diagnostics, failed_context, failed_document,
        flattened, human_failure, index, link_named_as_a_root, located, metadata,
        not_representable, operation_failure, package_context, report_diagnostic, text,
        undeliverable, unmanaged_failure, upgrade_pending_failure,
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
    canonical::{
        CurrentPackageCandidate, PackageIndexRebuildOperation, PackageIndexRebuildPlan,
        PackageIndexRebuildPlanError, PackageIndexRebuildRejection, assess_package_index,
        commit_package_index_rebuild, plan_package_index_rebuild,
    },
    mutation::MutationReport,
};
use std::{io, path::Path};

const STATUS_CONTRACT: ResponseContract = ResponseContract::IndexStatus;
const REBUILD_CONTRACT: ResponseContract = ResponseContract::IndexRebuild;

// ---------------------------------------------------------------------------
// `index status`
// ---------------------------------------------------------------------------

/// What the command decided, before anything is presented.
enum StatusAnswer {
    Assessed(Box<CurrentPackageCandidate>),
    Failed(Box<Failed>),
}

pub(crate) fn status(selected_root: &Path, output: &Output) -> Exit {
    present_status(decide_status(selected_root), output)
}

/// The assessing load, and one typed answer for every state it can report.
///
/// **A non-current index is a successful answer, not a failure.** Saying that the index is
/// missing, malformed, at a version this build does not project, or stale *is* what this
/// command does — and the derived index being wrong is explicitly among the conditions
/// that exit `0`, which is the same reading that lets a committed mutation with a
/// noncanonical derived index exit `0`.
///
/// A Package whose canonical state cannot be trusted is the one failure, classified
/// exactly as every other command classifies it — through the shared mapping, which the
/// complete Package read reaches through the same assessing load: the index is a
/// projection of those records, so there is nothing to judge it against.
fn decide_status(selected_root: &Path) -> StatusAnswer {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return StatusAnswer::Failed(Box::new(failed));
    }
    match assessed_package_candidate(
        selected_root,
        assess_package_index(selected_root, &PlatformFileSystem),
    ) {
        Ok(package) => StatusAnswer::Assessed(Box::new(package)),
        Err(failed) => StatusAnswer::Failed(failed),
    }
}

fn present_status(answer: StatusAnswer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match status_document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(STATUS_CONTRACT, package_context(status_root(&answer))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_status(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn status_root(answer: &StatusAnswer) -> Option<&Path> {
    match answer {
        StatusAnswer::Assessed(package) => Some(package.context().root().absolute_path()),
        StatusAnswer::Failed(failed) => failed.package_root.as_deref(),
    }
}

/// The `index status` payload is the shared index projection itself.
///
/// **No wrapper**, because the index's state is the whole answer: every other response
/// reports the same projection beside something else — the Package it read, or the record
/// a mutation left — and a member named `index` inside a document whose contract is
/// `spielorders.index.status` would say the same word twice.
fn status_document(
    answer: &StatusAnswer,
) -> Result<(Response<IndexProjection>, Exit), NotRepresentable> {
    match answer {
        StatusAnswer::Assessed(package) => Ok((
            Response::answered(
                STATUS_CONTRACT,
                Context::rooted_at(package.context().root().absolute_path())?,
                index(package.index())?,
                // What the assessing load observed about the Package. A Damaged Order is
                // why the index reports a changed membership, and an answer that named
                // the state without it would leave a reader unable to say what changed.
                diagnostics(&flattened(package.diagnostic_groups()))?,
            ),
            Exit::Success,
        )),
        StatusAnswer::Failed(failed) => Ok((
            failed_document(STATUS_CONTRACT, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

fn human_status(answer: &StatusAnswer, output: &Output) -> io::Result<Exit> {
    let package = match answer {
        StatusAnswer::Assessed(package) => package,
        StatusAnswer::Failed(failed) => return human_failure(failed, output),
    };
    let record = package.index();
    output.summary("Index", &text(record.state().as_str()))?;
    for reason in record.malformed_reasons() {
        output.summary("Malformed because", &text(reason.as_str()))?;
    }
    for reason in record.stale_reasons() {
        output.summary("Stale because", &text(reason.as_str()))?;
    }
    if let (Some(observed), Some(expected)) = (record.observed_version(), record.expected_version())
    {
        output.summary(
            "Schema version",
            &text(&format!("{observed}, and this build projects {expected}")),
        )?;
    }
    output.detail("Index path", record.location().absolute_path().as_os_str())?;
    output.detail(
        "Package Root",
        package.context().root().absolute_path().as_os_str(),
    )?;
    for group in package.diagnostic_groups() {
        for diagnostic in group.diagnostics() {
            report_diagnostic(
                output,
                Some(package.context().root().absolute_path()),
                diagnostic,
            )?;
        }
    }
    Ok(Exit::Success)
}

// ---------------------------------------------------------------------------
// `index rebuild`
// ---------------------------------------------------------------------------

/// The `index rebuild` payload: what the rebuild did, and the Package it leaves.
#[derive(Debug, Serialize)]
struct Rebuilt {
    report: Report,
    metadata: MetadataProjection,
    /// The derived index this run leaves behind: the rebuilt record after a commit, and
    /// the state the plan found after a dry run, which is what a dry run leaves.
    index: IndexProjection,
}

/// The typed rebuild report, in the shape every mutation's report has.
///
/// `committed` is whether the plan was carried out, and it is the one member `--dry-run`
/// decides. `changed` is whether the projection itself moved: a rebuild replaces the file
/// either way, because replacing it is what makes the derived index provably current
/// again, and this says whether the bytes differ from the ones already stored.
/// `operations` is the typed managed operation, in the core's own vocabulary — one
/// operation, because the derived index is the only file a rebuild touches.
///
/// **`warnings` is present and always empty**, because a rebuild has no
/// committed-with-warnings outcome: the warning other mutations carry is that their
/// canonical write succeeded and the derived refresh did not, and the derived index is
/// this command's whole subject. The core states that with an uninhabited warning type.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    operations: Vec<OperationProjection>,
    /// The Order Directories the rebuilt index carries, in the projection's own order.
    ///
    /// **A Damaged Order is absent**, because its record could not be read and it has
    /// nothing to project — and it stays visible in this response's `diagnostics`, so a
    /// consumer can tell an Order that was excluded from one that is gone.
    indexed_orders: Vec<LocationProjection>,
    warnings: Vec<&'static str>,
}

/// One planned managed operation, in the core's own vocabulary.
#[derive(Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    location: LocationProjection,
}

impl OperationProjection {
    fn of(operation: &PackageIndexRebuildOperation) -> Result<Self, NotRepresentable> {
        Ok(Self {
            operation: operation.kind().as_str(),
            location: LocationProjection::of(operation.location())?,
        })
    }
}

/// What the command decided, before anything is written anywhere.
enum Rebuild {
    Planned {
        plan: Box<PackageIndexRebuildPlan>,
        /// The Package the commit left, or `None` for a run asked to plan only.
        committed: Option<Box<CurrentPackageCandidate>>,
    },
    Failed(Box<Failed>),
}

pub(crate) fn rebuild(selected_root: &Path, dry_run: bool, output: &Output) -> Exit {
    present_rebuild(decide_rebuild(selected_root, dry_run), output)
}

fn decide_rebuild(selected_root: &Path, dry_run: bool) -> Rebuild {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return Rebuild::Failed(Box::new(failed));
    }
    let plan = match plan_package_index_rebuild(selected_root, &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => return Rebuild::Failed(Box::new(rebuild_refusal(selected_root, reason))),
    };
    if dry_run {
        return Rebuild::Planned {
            plan: Box::new(plan),
            committed: None,
        };
    }
    match commit_package_index_rebuild(plan, &PlatformFileSystem) {
        // A rebuild's only product is the index, so there is no unchanged commit and no
        // committed-with-warnings one: `changed` is where "the bytes already matched" is
        // reported, and the warning type is uninhabited. Both arms are written because
        // both carry a plan and a result, so folding them is not a claim about either.
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            Rebuild::Planned {
                plan: Box::new(plan),
                committed: Some(Box::new(result)),
            }
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => match warnings.into_iter().next() {
            // Discharged by an empty match on an uninhabited value, which the compiler
            // checks — so this command has no warning spelling that nothing can emit.
            Some(warning) => match warning {},
            None => Rebuild::Planned {
                plan: Box::new(plan),
                committed: Some(Box::new(result)),
            },
        },
        MutationReport::Rejected { plan, rejection } => {
            Rebuild::Failed(Box::new(rebuild_rejection(&plan, rejection)))
        }
    }
}

/// Why no rebuild could be planned, in the shared classification.
///
/// Four of the five are Package-load states and go through the shared constructors
/// unchanged, so this command answers an unmanaged directory or a damaged Package exactly
/// as every other command does. The fifth is this command's own.
fn rebuild_refusal(selected_root: &Path, reason: PackageIndexRebuildPlanError) -> Failed {
    match reason {
        PackageIndexRebuildPlanError::Unmanaged(package) => unmanaged_failure(&package),
        PackageIndexRebuildPlanError::DamagedPackage(package) => {
            damaged_failure(&package, &as_given(selected_root))
        }
        PackageIndexRebuildPlanError::UpgradePending(package) => upgrade_pending_failure(&package),
        PackageIndexRebuildPlanError::OperationFailed(package) => {
            operation_failure(package.failure(), package.root())
        }
        PackageIndexRebuildPlanError::OrderReadmeTextUnavailable(package) => {
            let root = package.context().root();
            Failed {
                code: FailureCode::IndexProjectionIncomplete,
                scope: FailureScope::Document,
                message: format!(
                    "the derived Package Index at {} was not rebuilt: an Order README \
                     exists and could not be read, and the index records every Order's \
                     README text — so there is no truthful projection to write. The \
                     stored index is left exactly as it was.",
                    package.context().index().absolute_path().display()
                ),
                paths: vec![package.context().index().clone().into()],
                identities: Vec::new(),
                package_root: Some(root.absolute_path().to_path_buf()),
                // The document diagnostic behind the refusal is the whole reason for it.
                diagnostics: flattened(package.diagnostic_groups()),
            }
        }
    }
}

/// A rebuild whose replacement did not happen. The stored index holds what it held.
fn rebuild_rejection(
    plan: &PackageIndexRebuildPlan,
    rejection: PackageIndexRebuildRejection,
) -> Failed {
    let PackageIndexRebuildRejection::ReplacementFailed { failure } = rejection;
    let mut failed = operation_failure(&failure, Some(plan.package().context().root()));
    failed.code = FailureCode::IndexReplacementFailed;
    failed.message = format!(
        "{}. The stored index is preserved exactly as it was, and nothing canonical was \
         touched.",
        failed.message
    );
    failed
}

fn present_rebuild(rebuild: Rebuild, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match rebuild_document(&rebuild) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(REBUILD_CONTRACT, package_context(rebuild_root(&rebuild))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_rebuild(&rebuild, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn rebuild_root(rebuild: &Rebuild) -> Option<&Path> {
    match rebuild {
        Rebuild::Planned { plan, .. } => Some(plan.package().context().root().absolute_path()),
        Rebuild::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn rebuild_document(rebuild: &Rebuild) -> Result<(Response<Rebuilt>, Exit), NotRepresentable> {
    match rebuild {
        Rebuild::Planned { plan, committed } => {
            // The Package the answer is about: the one the commit left where there was
            // one, and the planned Package otherwise. One selection rather than one per
            // member, so the report and the index cannot come from different runs.
            let answered = committed.as_deref().unwrap_or_else(|| plan.package());
            Ok((
                Response::answered(
                    REBUILD_CONTRACT,
                    Context::rooted_at(answered.context().root().absolute_path())?,
                    Rebuilt {
                        report: Report {
                            committed: committed.is_some(),
                            changed: plan.changed(),
                            operations: plan
                                .operations()
                                .iter()
                                .map(OperationProjection::of)
                                .collect::<Result<_, _>>()?,
                            indexed_orders: plan
                                .indexed_orders()
                                .iter()
                                .map(LocationProjection::of)
                                .collect::<Result<_, _>>()?,
                            warnings: Vec::new(),
                        },
                        metadata: metadata(answered.metadata_version(), answered.metadata_state()),
                        index: index(answered.index())?,
                    },
                    // A Damaged Order is absent from the rebuilt index and reported here,
                    // which is what keeps its exclusion visible rather than silent.
                    diagnostics(&flattened(answered.diagnostic_groups()))?,
                ),
                Exit::Success,
            ))
        }
        Rebuild::Failed(failed) => Ok((
            failed_document(REBUILD_CONTRACT, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

fn human_rebuild(rebuild: &Rebuild, output: &Output) -> io::Result<Exit> {
    let (plan, committed) = match rebuild {
        Rebuild::Planned { plan, committed } => (plan, committed),
        Rebuild::Failed(failed) => return human_failure(failed, output),
    };
    let answered = committed.as_deref().unwrap_or_else(|| plan.package());
    // A dry run says what it *would* do, because a reader who skimmed one line of a plan
    // and took it for a result is the whole hazard of reporting a plan in a result's
    // shape.
    output.summary(
        match (committed.is_some(), plan.changed()) {
            (true, true) => "Rebuilt",
            (true, false) => "Rebuilt unchanged",
            (false, true) => "Would rebuild",
            (false, false) => "Would rebuild unchanged",
        },
        answered.index().location().absolute_path().as_os_str(),
    )?;
    output.summary(
        "Orders indexed",
        &text(&plan.indexed_orders().len().to_string()),
    )?;
    output.summary("Index", &text(answered.index().state().as_str()))?;
    for order in plan.indexed_orders() {
        output.detail("Indexed Order", located(order))?;
    }
    output.detail(
        "Package Root",
        answered.context().root().absolute_path().as_os_str(),
    )?;
    for group in answered.diagnostic_groups() {
        for diagnostic in group.diagnostics() {
            report_diagnostic(
                output,
                Some(answered.context().root().absolute_path()),
                diagnostic,
            )?;
        }
    }
    Ok(Exit::Success)
}
