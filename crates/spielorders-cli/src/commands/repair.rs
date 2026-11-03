//! `repair` and `order repair`: the two commands that act on one diagnosed finding.
//!
//! **One module for both**, because they are one question over two scopes. Both diagnose
//! only their own scope, both name the same actions from the same vocabulary, and both
//! apply exactly one of them — so a condition cannot come to be actionable one way about a
//! Package and another way about one of its Orders.
//!
//! **Two moods, one command shape.** With no assignments a `repair` diagnoses and writes
//! nothing. `case=… action=…` applies exactly one action that diagnosis offered. There is
//! no `--fix`, no repair-all, and no inferred repair: each application names one case and
//! one action, and the values it needs arrive as ordinary `field=value` resolutions.
//!
//! **A case handle is ephemeral.** It is derived from the evidence the finding was
//! diagnosed from, so a handle from a diagnosis taken before the Package changed simply
//! stops naming a case and is refused rather than resolved against whatever stands there
//! now. It is not an identity, it is not persisted, and nothing offers it as one a script
//! may store.
//!
//! **A bare diagnosis is an answer rather than a verdict**, so it exits `0` however
//! damaged the subject is and carries what it found in its payload. The owner's dated
//! ruling is in the CLI capability decision's exit-status list, with the collision it
//! settled. `validate` is the command whose result *is* the verdict, and it still exits
//! `1` on the same subject; `valid` here is this scope's own, which is what the field says.
//!
//! What does exit `1` is a repair that could not answer or could not be applied: a
//! directory that is no Package, a handle naming no current case, an action the diagnosis
//! did not offer, a resolution still owed, a candidate that would damage the Package, and a
//! destination somebody else wrote first.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define Vendor identity and historical semantics.

use crate::{
    cli::{FieldAssignment, RepairRequest},
    commands::{
        order::{OrderFailed, Target, failed_order_context, targeted_directory},
        repair_suggestion::{RepairSuggestion, Rooting, order_repair_at, package_repair_at},
        shared::{
            Failed, IndexProjection, OfferedActionProjection, RootKind, UNSPELLABLE_PATH,
            diagnostics, failed_context, failed_document, human_failure, in_package, index,
            link_named_as_a_root, located, not_representable, offered_actions, operation_failure,
            order_diagnosis_failure, package_context, report_diagnostic, shell_word, text,
            undeliverable, unmanaged_failure,
        },
    },
    contract::ResponseContract,
    effects::{PlatformFileSystem, PlatformUuids},
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, Identity, LocationProjection, NotRepresentable,
        OrderContext, Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    metadata_repair::{
        MetadataRepairError, MetadataRepairOperation, MetadataRepairRejection,
        MetadataRepairWarning, PackageConstraint, RepairConfirmation, RepairResolution,
        commit_metadata_repair, plan_metadata_repair,
    },
    mutation::MutationReport,
    order_diagnosis::{OrderDiagnosis, diagnose_order},
    package_diagnosis::{PackageDiagnosis, PackageDiagnosisError, diagnose_package},
    package_repair::{
        PackageRepairError, PackageRepairOperation, PackageRepairRejection, PackageRepairWarning,
        commit_package_repair, plan_package_repair,
    },
    readme::{Diagnostic, DiagnosticCode, EvidenceScope, RootedLocation},
    repair_cases::{RepairAction, RepairCase, UnactionableFinding},
    semantic_repair::{
        SemanticRepairError, SemanticRepairOperation, SemanticRepairRejection,
        SemanticRepairWarning, SemanticResolution, commit_semantic_repair, plan_semantic_repair,
    },
};
use std::{
    io,
    path::{Path, PathBuf},
};

const PACKAGE_CONTRACT: ResponseContract = ResponseContract::Repair;
const ORDER_CONTRACT: ResponseContract = ResponseContract::OrderRepair;

// ---------------------------------------------------------------------------
// The payload both scopes answer with.
// ---------------------------------------------------------------------------

/// The `repair` payload: what may be done here, and what this run did.
///
/// **`cases` is what is actionable *now***, so after an application it is what remains.
/// The findings themselves are the envelope's `diagnostics`, which is the only channel a
/// failed response has; `unactionable` is the other half of the answer — the findings that
/// invalidate this scope and that nothing safe here can act on, so a caller can tell
/// "nothing is wrong" from "nothing safe can be done from here".
#[derive(Debug, Serialize)]
struct Repaired {
    scope: &'static str,
    /// Whether this scope carries no error-severity finding, as it stands *now* — so
    /// after an application it is the verdict on what the repair left.
    ///
    /// **In the payload rather than in `ok`**, which is where the two validating commands
    /// carry it. `ok` is the exit status, and this command's exit status says whether it
    /// could answer at all: a `repair` asked about a damaged Order answers, and answering
    /// is not the same as approving.
    valid: bool,
    cases: Vec<CaseProjection>,
    unactionable: Vec<UnactionableProjection>,
    /// What this run applied, and `null` for a bare diagnosis, which applies nothing.
    report: Option<Report>,
    /// The derived index this scope's repair left, and `null` where nothing was written.
    index: Option<IndexProjection>,
}

/// One ephemeral diagnosed case, and the exact commands that apply what it offers.
#[derive(Debug, Serialize)]
struct CaseProjection {
    /// The ephemeral handle. **Not an identity and never persisted**: it is derived from
    /// the evidence, so it stops naming this case as soon as that evidence moves.
    case: String,
    /// The stable diagnostic code this case answers, which is also how it is matched to
    /// the finding in `diagnostics` — by code and place, never by position.
    code: &'static str,
    /// The exact evidence this case was diagnosed from.
    location: LocationProjection,
    actions: Vec<OfferedActionProjection>,
}

/// One diagnosed condition that invalidates this scope and that nothing here can act on.
#[derive(Debug, Serialize)]
struct UnactionableProjection {
    code: &'static str,
    location: LocationProjection,
}

impl UnactionableProjection {
    fn of(finding: &UnactionableFinding) -> Result<Self, NotRepresentable> {
        Ok(Self {
            code: finding.code().as_str(),
            location: LocationProjection::of(finding.location())?,
        })
    }
}

/// The typed repair report, in the shape every mutation's report has.
///
/// **`changed` is always `true` for a plan that exists**, and it is carried rather than
/// omitted because every mutation report has it: a case is diagnosed only where what
/// stands on disk is not what the repair would leave, so there is no unchanged repair to
/// report. `committed` is the one member `--dry-run` decides.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    /// The case this run applied, and the action it applied to it.
    case: String,
    action: &'static str,
    operations: Vec<OperationProjection>,
    /// Present and empty for every repair whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// One planned managed operation, in the core's own vocabulary.
#[derive(Clone, Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    location: LocationProjection,
}

/// What one applied repair left behind, in the terms the payload reports.
struct Applied {
    committed: bool,
    /// The ephemeral handle this run was confirmed against. It names the case as the
    /// diagnosis that offered it did, which is what a caller reading the report matches.
    case: String,
    action: RepairAction,
    operations: Vec<OperationProjection>,
    warnings: Vec<&'static str>,
    index: Option<IndexProjection>,
}

// ---------------------------------------------------------------------------
// `repair`
// ---------------------------------------------------------------------------

/// What the command decided, before anything is presented.
enum PackageAnswer {
    Diagnosed {
        diagnosis: Box<PackageDiagnosis>,
        applied: Box<Option<Applied>>,
    },
    /// A path this repair would report has no JSON spelling.
    ///
    /// **Its own variant rather than a [`Failed`]**, because `path_not_representable` has
    /// one shape and one producer — `shared::not_representable`, which names no path and
    /// takes the Root from the context. A second construction here is exactly how one code
    /// came to have two shapes, one of them reporting the very path with no spelling.
    Unspellable {
        root: PathBuf,
    },
    Failed(Box<Failed>),
}

pub(crate) fn package(
    selected_root: &Path,
    request: &RepairRequest,
    dry_run: bool,
    output: &Output,
) -> Exit {
    present_package(decide_package(selected_root, request, dry_run), output)
}

fn decide_package(selected_root: &Path, request: &RepairRequest, dry_run: bool) -> PackageAnswer {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return PackageAnswer::Failed(Box::new(failed));
    }
    let (case, action, resolutions) = match request {
        RepairRequest::Diagnose => return diagnosed_package(selected_root, None),
        RepairRequest::Apply {
            case,
            action,
            resolutions,
        } => (case, *action, resolutions),
    };
    let resolution = match package_resolution(action, resolutions) {
        Ok(resolution) => resolution,
        Err(failed) => return PackageAnswer::Failed(failed),
    };
    let plan = match plan_package_repair(
        selected_root,
        &RepairConfirmation::new(case.clone(), action),
        &resolution,
        &PlatformFileSystem,
    ) {
        Ok(plan) => plan,
        Err(reason) => {
            return PackageAnswer::Failed(Box::new(package_refusal(reason)));
        }
    };
    // The Root planning resolved, kept because the commit consumes the plan: every refusal
    // this run reaches from here on happened under a Package, and names it.
    let root = plan.root().to_path_buf();
    let operations = match plan
        .operations()
        .iter()
        .map(package_operation)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(operations) => operations,
        Err(NotRepresentable) => return PackageAnswer::Unspellable { root },
    };
    if dry_run {
        return diagnosed_package(
            selected_root,
            Some(Applied {
                committed: false,
                case: case.to_owned(),
                action,
                operations,
                warnings: Vec::new(),
                index: None,
            }),
        );
    }
    let (result, warnings) = match commit_package_repair(plan, &PlatformFileSystem) {
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            (result, Vec::new())
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => (
            result,
            warnings.iter().map(package_warning).collect::<Vec<_>>(),
        ),
        MutationReport::Rejected { rejection, .. } => {
            return PackageAnswer::Failed(Box::new(package_rejection(rejection)));
        }
    };
    let index = match result.index().map(index).transpose() {
        Ok(index) => index,
        Err(NotRepresentable) => return PackageAnswer::Unspellable { root },
    };
    let applied = Applied {
        committed: true,
        case: case.clone(),
        action,
        operations,
        warnings,
        index,
    };
    // The scope as the repair left it. The core rediagnosed it at the commit and says so
    // in a warning where it could not, and a fresh diagnosis is what answers then.
    match result.remaining() {
        Some(diagnosis) => PackageAnswer::Diagnosed {
            diagnosis: Box::new(diagnosis.clone()),
            applied: Box::new(Some(applied)),
        },
        None => diagnosed_package(selected_root, Some(applied)),
    }
}

fn diagnosed_package(selected_root: &Path, applied: Option<Applied>) -> PackageAnswer {
    let applied = Box::new(applied);
    match diagnose_package(selected_root, &PlatformFileSystem) {
        Ok(diagnosis) => PackageAnswer::Diagnosed {
            diagnosis: Box::new(diagnosis),
            applied,
        },
        Err(PackageDiagnosisError::Unmanaged(package)) => {
            PackageAnswer::Failed(Box::new(unmanaged_failure(&package)))
        }
        Err(PackageDiagnosisError::OperationFailed(failed)) => {
            PackageAnswer::Failed(Box::new(operation_failure(failed.failure(), failed.root())))
        }
    }
}

/// The resolution a Package-level action takes.
///
/// Which values an action needs is the core's answer, asked of
/// [`RepairAction::required_values`]; what is checked here is only that the command line
/// supplied them and supplied nothing else. A caller that ran the suggested command
/// verbatim without filling in its assignment lands on the first of these, naming the
/// resolution it still owes.
fn package_resolution(
    action: RepairAction,
    resolutions: &[FieldAssignment],
) -> Result<RepairResolution, Box<Failed>> {
    let supplied = required_resolutions(action, resolutions)?;
    // Every variant is named rather than covered by a wildcard, so an action added to the
    // core's vocabulary is a compile error here rather than a repair applied with a
    // resolution nobody chose for it. An action this scope does not apply still reaches
    // the core, which refuses it by name.
    Ok(match action {
        RepairAction::ReplaceManagedRecord => {
            RepairResolution::ReplacementRecord(supplied["record"].clone().into_bytes())
        }
        RepairAction::PromoteStagingCandidate
        | RepairAction::AssignExistingVendor
        | RepairAction::CreateAndAssignVendor
        | RepairAction::ClearVendorReference
        | RepairAction::AdoptNewOrderIdentity
        | RepairAction::RepairPackageLocator => RepairResolution::None,
    })
}

/// Every resolution a command line supplied, by the name the action asked for it under.
type SuppliedResolutions = std::collections::BTreeMap<&'static str, String>;

/// Every value this action requires, refusing a command line that supplied one of them
/// blank or absent, or that supplied one the action does not take.
fn required_resolutions(
    action: RepairAction,
    resolutions: &[FieldAssignment],
) -> Result<SuppliedResolutions, Box<Failed>> {
    let required = action.required_values();
    let mut missing = Vec::new();
    let mut supplied = std::collections::BTreeMap::new();
    for field in required {
        match resolutions
            .iter()
            .find(|assignment| assignment.field == *field)
        {
            Some(assignment) if !assignment.value.trim().is_empty() => {
                supplied.insert(*field, assignment.value.clone());
            }
            // A blank value and no value at all are one condition here: neither is a
            // resolution, and the suggested command names the assignment unfilled so that
            // running it verbatim lands exactly here rather than half-applying anything.
            _ => missing.push(*field),
        }
    }
    if !missing.is_empty() {
        return Err(Box::new(Failed {
            code: FailureCode::RepairResolutionMissing,
            scope: FailureScope::Operation,
            message: format!(
                "`{}` requires {} and this command line supplied {}. Nothing was written.",
                action.as_str(),
                quoted(required),
                match missing.len() {
                    1 => format!("no {}", quoted(&missing)),
                    _ => format!("none of {}", quoted(&missing)),
                }
            ),
            paths: Vec::new(),
            identities: missing
                .iter()
                .map(|field| Identity::required_fact(field))
                .collect(),
            // The command line is adjudicated before anything resolves the selected root,
            // so no Package has been established to name.
            package_root: None,
            diagnostics: Vec::new(),
        }));
    }
    if let Some(unexpected) = resolutions
        .iter()
        .find(|assignment| !required.contains(&assignment.field.as_str()))
    {
        return Err(Box::new(Failed {
            code: FailureCode::RepairResolutionUnexpected,
            scope: FailureScope::Operation,
            message: format!(
                "`{}` takes {}, and this command line assigned `{}`. Nothing was written.",
                action.as_str(),
                if required.is_empty() {
                    "no resolution at all".to_owned()
                } else {
                    quoted(required)
                },
                unexpected.field
            ),
            paths: Vec::new(),
            identities: vec![Identity::repair_resolution(&unexpected.field)],
            // The same: this refusal is about the command line, and nothing has resolved a
            // Root yet.
            package_root: None,
            diagnostics: Vec::new(),
        }));
    }
    Ok(supplied)
}

fn quoted(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| format!("`{field}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn package_operation(
    operation: &PackageRepairOperation,
) -> Result<OperationProjection, NotRepresentable> {
    Ok(OperationProjection {
        operation: operation.kind().as_str(),
        location: LocationProjection::of(operation.location())?,
    })
}

fn package_warning(warning: &PackageRepairWarning) -> &'static str {
    match warning {
        PackageRepairWarning::IndexRefreshFailed(_) => "index_refresh_failed",
        PackageRepairWarning::RefreshUnavailable => "refresh_unavailable",
        PackageRepairWarning::RediagnosisUnavailable => "rediagnosis_unavailable",
    }
}

/// Why no Package-level repair could be planned.
///
/// **Every refusal past the diagnosis names the Package it was refused in**, in `paths`
/// with the Root's own spelling for it and in `package_root` beside it. The core carries
/// both facts because it resolved the scope before refusing, and reporting one without the
/// other would say a resolution that happened did not.
fn package_refusal<E: std::fmt::Display>(reason: PackageRepairError<E>) -> Failed {
    match reason {
        PackageRepairError::Diagnosis(PackageDiagnosisError::Unmanaged(package)) => {
            unmanaged_failure(&package)
        }
        PackageRepairError::Diagnosis(PackageDiagnosisError::OperationFailed(failed)) => {
            operation_failure(failed.failure(), failed.root())
        }
        PackageRepairError::UnknownCase { handle, scope } => unknown_case(&handle, &scope),
        PackageRepairError::UnauthorizedAction {
            handle,
            action,
            scope,
        } => unauthorized_action(&handle, action, &scope),
        PackageRepairError::UnsupportedAction { action, scope } => Failed {
            code: FailureCode::RepairActionUnsupported,
            scope: FailureScope::Package,
            message: format!(
                "`{}` acts on one Order, and this command's scope is the Package's own \
                 artifacts. {}",
                action.as_str(),
                RepairSuggestion::applied_by_the_order_command()
            ),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        PackageRepairError::InvalidResolution { action, scope } => {
            invalid_resolution(action, FailureScope::Package, &scope)
        }
        PackageRepairError::UnrecognizedArtifact { destination } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Package,
            message: format!(
                "{} is not one of this Package's managed artifacts, so there is no kind to \
                 judge a candidate against and nothing here writes it.",
                destination.location().absolute_path().display()
            ),
            paths: vec![in_package(destination.location())],
            identities: Vec::new(),
            package_root: Some(destination.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        PackageRepairError::InvalidReplacement { scope } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Package,
            message: "the replacement supplied is not a complete valid artifact of its \
                      kind. Nothing was written, and nothing was partially salvaged from it."
                .to_owned(),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        PackageRepairError::InvalidStagingCandidate { staged } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Package,
            message: format!(
                "the staging candidate at {} is not a complete valid artifact of its kind, \
                 so it is not promoted. It is left exactly where it is.",
                staged.location().absolute_path().display()
            ),
            paths: vec![in_package(staged.location())],
            identities: Vec::new(),
            package_root: Some(staged.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        PackageRepairError::StagingEvidencePresent { staged } => staging_evidence_present(&staged),
        PackageRepairError::FileSystem { subject, error } => filesystem_failure(&subject, &error),
    }
}

fn package_rejection<E: std::fmt::Display>(rejection: PackageRepairRejection<E>) -> Failed {
    match rejection {
        PackageRepairRejection::EvidenceChanged { evidence } => evidence_changed(&evidence),
        PackageRepairRejection::DestinationChanged {
            destination,
            uncleaned_stage,
        } => destination_changed(
            EvidenceScope::Package,
            &destination,
            uncleaned_stage.as_ref(),
        ),
        PackageRepairRejection::ReplacementFailed {
            destination,
            uncleaned_stage,
            ..
        } => replacement_failed(
            EvidenceScope::Package,
            &destination,
            uncleaned_stage.as_ref(),
        ),
        PackageRepairRejection::PromotionFailed { destination, error } => {
            promotion_failed(&destination, &error)
        }
    }
}

fn present_package(answer: PackageAnswer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match package_document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(PACKAGE_CONTRACT, package_context(package_root(&answer))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_package(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn package_root(answer: &PackageAnswer) -> Option<&Path> {
    match answer {
        PackageAnswer::Diagnosed { diagnosis, .. } => Some(diagnosis.root().absolute_path()),
        PackageAnswer::Unspellable { root } => Some(root),
        PackageAnswer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn package_document(
    answer: &PackageAnswer,
) -> Result<(Response<Repaired>, Exit), NotRepresentable> {
    let (diagnosis, applied) = match answer {
        PackageAnswer::Diagnosed { diagnosis, applied } => (diagnosis, applied),
        // The one producer renders it, from the context this answer's Root supplies.
        PackageAnswer::Unspellable { .. } => return Err(NotRepresentable),
        PackageAnswer::Failed(failed) => {
            return Ok((
                failed_document(PACKAGE_CONTRACT, failed_context(failed)?, failed)?,
                Exit::Failure,
            ));
        }
    };
    let root = diagnosis.root().absolute_path();
    let context = Context::rooted_at(root)?;
    let observed = diagnostics(diagnosis.diagnostics())?;
    Ok((
        Response::answered(
            PACKAGE_CONTRACT,
            context,
            repaired(
                EvidenceScope::Package.as_str(),
                diagnosis.is_valid(),
                diagnosis.cases(),
                diagnosis.unactionable(),
                applied,
                &package_repair_at(root),
            )?,
            observed,
        ),
        Exit::Success,
    ))
}

fn human_package(answer: &PackageAnswer, output: &Output) -> io::Result<Exit> {
    let (diagnosis, applied) = match answer {
        PackageAnswer::Diagnosed { diagnosis, applied } => (diagnosis, applied),
        PackageAnswer::Unspellable { .. } => {
            output.error(UNSPELLABLE_PATH)?;
            return Ok(Exit::Failure);
        }
        PackageAnswer::Failed(failed) => return human_failure(failed, output),
    };
    let root = diagnosis.root().absolute_path();
    human_scope(
        output,
        "Package Root",
        root.as_os_str(),
        root,
        diagnosis.cases(),
        diagnosis.unactionable(),
        diagnosis.diagnostics(),
        applied.as_ref().as_ref(),
        &package_repair_at(root),
        diagnosis.is_valid(),
    )?;
    Ok(Exit::Success)
}

// ---------------------------------------------------------------------------
// `order repair`
// ---------------------------------------------------------------------------

/// What the command decided, before anything is presented.
enum OrderAnswer {
    Diagnosed {
        diagnosis: Box<OrderDiagnosis>,
        applied: Box<Option<Applied>>,
    },
    /// A path this repair would report has no JSON spelling. [`PackageAnswer::Unspellable`]
    /// says why it is a variant here rather than a failure built beside the one producer.
    Unspellable {
        package_root: PathBuf,
        order_root: PathBuf,
    },
    Failed(Box<OrderFailed>),
}

pub(crate) fn order(
    target: &Target,
    request: &RepairRequest,
    dry_run: bool,
    output: &Output,
) -> Exit {
    present_order(decide_order(target, request, dry_run), output)
}

fn decide_order(target: &Target, request: &RepairRequest, dry_run: bool) -> OrderAnswer {
    let directory = match targeted_directory(target) {
        Ok(directory) => directory,
        Err(failed) => return OrderAnswer::Failed(failed),
    };
    let (case, action, resolutions) = match request {
        RepairRequest::Diagnose => return diagnosed_order(&directory, None),
        RepairRequest::Apply {
            case,
            action,
            resolutions,
        } => (case, *action, resolutions),
    };
    let supplied = match required_resolutions(action, resolutions) {
        Ok(supplied) => supplied,
        Err(failed) => {
            return OrderAnswer::Failed(Box::new(OrderFailed {
                order_root: Some(directory.clone()),
                failed: *failed,
            }));
        }
    };
    match routed(action, &supplied) {
        Routed::Record(resolution) => {
            metadata_repair(&directory, case, action, resolution, dry_run)
        }
        Routed::Semantic(resolution) => {
            semantic_repair(&directory, case, action, resolution, dry_run)
        }
    }
}

/// Which capability applies an Order-scoped action, and the typed resolution it takes.
enum Routed {
    /// [`crate::commands::repair::metadata_repair`], the Order's own managed record.
    Record(RepairResolution),
    /// [`crate::commands::repair::semantic_repair`], the Order's meaning.
    Semantic(SemanticResolution),
}

/// One table, so an action cannot be routed to one capability and resolved as another.
///
/// **Every variant is named rather than covered by a wildcard.** An action added to the
/// core's vocabulary is a compile error here, where the routing and the resolution are
/// decided together, rather than a run that falls through to whichever repair the last arm
/// happened to name — which would apply a repair the caller never asked for and report it
/// under the name of the one they did.
///
/// The values are read by the names [`RepairAction::required_values`] gives them, which
/// [`required_resolutions`] has already refused a command line for not supplying.
fn routed(action: RepairAction, supplied: &SuppliedResolutions) -> Routed {
    match action {
        RepairAction::ReplaceManagedRecord => Routed::Record(RepairResolution::ReplacementRecord(
            supplied["record"].clone().into_bytes(),
        )),
        RepairAction::PromoteStagingCandidate => Routed::Record(RepairResolution::None),
        RepairAction::AssignExistingVendor => {
            Routed::Semantic(SemanticResolution::AssignExistingVendor {
                name: supplied["vendor"].clone(),
            })
        }
        RepairAction::CreateAndAssignVendor => {
            Routed::Semantic(SemanticResolution::CreateAndAssignVendor {
                name: supplied["name"].clone(),
            })
        }
        RepairAction::ClearVendorReference => {
            Routed::Semantic(SemanticResolution::ClearVendorReference)
        }
        RepairAction::AdoptNewOrderIdentity => {
            Routed::Semantic(SemanticResolution::AdoptNewOrderIdentity)
        }
        RepairAction::RepairPackageLocator => {
            Routed::Semantic(SemanticResolution::RepairPackageLocator)
        }
    }
}

/// The two repairs of a managed record itself: a confirmed replacement, and promoting the
/// diagnosed staging candidate.
fn metadata_repair(
    directory: &Path,
    case: &str,
    action: RepairAction,
    resolution: RepairResolution,
    dry_run: bool,
) -> OrderAnswer {
    let plan = match plan_metadata_repair(
        directory,
        &RepairConfirmation::new(case.to_owned(), action),
        &resolution,
        &PlatformFileSystem,
    ) {
        Ok(plan) => plan,
        Err(reason) => {
            return OrderAnswer::Failed(Box::new(OrderFailed {
                order_root: Some(directory.to_path_buf()),
                failed: metadata_refusal(directory, reason),
            }));
        }
    };
    // The Root planning resolved, kept because the commit consumes the plan.
    let root = plan.root().to_path_buf();
    let operations = match plan
        .operations()
        .iter()
        .map(metadata_operation)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(operations) => operations,
        Err(NotRepresentable) => return unspellable_order(root, directory),
    };
    if dry_run {
        return diagnosed_order(
            directory,
            Some(Applied {
                committed: false,
                case: case.to_owned(),
                action,
                operations,
                warnings: Vec::new(),
                index: None,
            }),
        );
    }
    let (result, warnings) = match commit_metadata_repair(plan, &PlatformFileSystem) {
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            (result, Vec::new())
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => (
            result,
            warnings.iter().map(metadata_warning).collect::<Vec<_>>(),
        ),
        MutationReport::Rejected { plan, rejection } => {
            return OrderAnswer::Failed(Box::new(OrderFailed {
                order_root: Some(plan.directory().to_path_buf()),
                failed: metadata_rejection(rejection),
            }));
        }
    };
    let index = match result.index().map(index).transpose() {
        Ok(index) => index,
        Err(NotRepresentable) => return unspellable_order(root, directory),
    };
    applied_order(
        directory,
        result.remaining(),
        Applied {
            committed: true,
            case: case.to_owned(),
            action,
            operations,
            warnings,
            index,
        },
    )
}

/// The five explicitly decided semantic resolutions.
fn semantic_repair(
    directory: &Path,
    case: &str,
    action: RepairAction,
    resolution: SemanticResolution,
    dry_run: bool,
) -> OrderAnswer {
    let plan = match plan_semantic_repair(directory, case, &resolution, &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => {
            return OrderAnswer::Failed(Box::new(OrderFailed {
                order_root: Some(directory.to_path_buf()),
                failed: semantic_refusal(directory, reason),
            }));
        }
    };
    // The Root planning resolved, kept because the commit consumes the plan.
    let root = plan.root().to_path_buf();
    let operations = match plan
        .operations()
        .iter()
        .map(semantic_operation)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(operations) => operations,
        Err(NotRepresentable) => return unspellable_order(root, directory),
    };
    if dry_run {
        return diagnosed_order(
            directory,
            Some(Applied {
                committed: false,
                case: case.to_owned(),
                action,
                operations,
                warnings: Vec::new(),
                index: None,
            }),
        );
    }
    let mut uuids = PlatformUuids;
    let (result, warnings) = match commit_semantic_repair(plan, &mut uuids, &PlatformFileSystem) {
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            (result, Vec::new())
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => (
            result,
            warnings.iter().map(semantic_warning).collect::<Vec<_>>(),
        ),
        MutationReport::Rejected { plan, rejection } => {
            return OrderAnswer::Failed(Box::new(OrderFailed {
                order_root: Some(plan.directory().to_path_buf()),
                failed: semantic_rejection(plan.root(), rejection),
            }));
        }
    };
    let index = match result.index().map(index).transpose() {
        Ok(index) => index,
        Err(NotRepresentable) => return unspellable_order(root, directory),
    };
    applied_order(
        directory,
        result.remaining(),
        Applied {
            committed: true,
            case: case.to_owned(),
            action,
            operations,
            warnings,
            index,
        },
    )
}

/// The scope as the repair left it: the core's own rediagnosis where it took one, and a
/// fresh one where it says it could not.
fn applied_order(
    directory: &Path,
    remaining: Option<&OrderDiagnosis>,
    applied: Applied,
) -> OrderAnswer {
    match remaining {
        Some(diagnosis) => OrderAnswer::Diagnosed {
            diagnosis: Box::new(diagnosis.clone()),
            applied: Box::new(Some(applied)),
        },
        None => diagnosed_order(directory, Some(applied)),
    }
}

fn diagnosed_order(directory: &Path, applied: Option<Applied>) -> OrderAnswer {
    let applied = Box::new(applied);
    match diagnose_order(directory, &PlatformFileSystem) {
        Ok(diagnosis) => OrderAnswer::Diagnosed {
            diagnosis: Box::new(diagnosis),
            applied,
        },
        Err(reason) => OrderAnswer::Failed(Box::new(order_diagnosis_failure(directory, reason))),
    }
}

fn unspellable_order(package_root: PathBuf, directory: &Path) -> OrderAnswer {
    OrderAnswer::Unspellable {
        package_root,
        order_root: directory.to_path_buf(),
    }
}

fn metadata_operation(
    operation: &MetadataRepairOperation,
) -> Result<OperationProjection, NotRepresentable> {
    Ok(OperationProjection {
        operation: operation.kind().as_str(),
        location: LocationProjection::of(operation.location())?,
    })
}

fn semantic_operation(
    operation: &SemanticRepairOperation,
) -> Result<OperationProjection, NotRepresentable> {
    Ok(OperationProjection {
        operation: operation.kind().as_str(),
        location: LocationProjection::of(operation.location())?,
    })
}

fn metadata_warning(warning: &MetadataRepairWarning) -> &'static str {
    match warning {
        MetadataRepairWarning::IndexRefreshFailed(_) => "index_refresh_failed",
        MetadataRepairWarning::RefreshUnavailable => "refresh_unavailable",
        MetadataRepairWarning::RediagnosisUnavailable => "rediagnosis_unavailable",
    }
}

fn semantic_warning(warning: &SemanticRepairWarning) -> &'static str {
    match warning {
        SemanticRepairWarning::IndexRefreshFailed(_) => "index_refresh_failed",
        SemanticRepairWarning::RefreshUnavailable => "refresh_unavailable",
        SemanticRepairWarning::RediagnosisUnavailable => "rediagnosis_unavailable",
    }
}

fn metadata_refusal<E: std::fmt::Display>(
    directory: &Path,
    reason: MetadataRepairError<E>,
) -> Failed {
    match reason {
        MetadataRepairError::Diagnosis(reason) => order_diagnosis_failure(directory, reason).failed,
        MetadataRepairError::UnknownCase { handle, scope } => unknown_case(&handle, &scope),
        MetadataRepairError::UnauthorizedAction {
            handle,
            action,
            scope,
        } => unauthorized_action(&handle, action, &scope),
        MetadataRepairError::UnsupportedAction { action, scope } => Failed {
            code: FailureCode::RepairActionUnsupported,
            scope: FailureScope::Order,
            message: format!(
                "`{}` is not an action this Order's managed record takes.",
                action.as_str()
            ),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataRepairError::InvalidResolution { action, scope } => {
            invalid_resolution(action, FailureScope::Order, &scope)
        }
        MetadataRepairError::InvalidReplacement { scope } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Order,
            message: "the replacement supplied is not a complete valid Order record. \
                      Nothing was written, and nothing was partially salvaged from it."
                .to_owned(),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataRepairError::ReplacementDamagesPackage { constraint, scope } => {
            damages_package(constraint, &scope)
        }
        MetadataRepairError::InvalidStagingCandidate { staged } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Order,
            message: format!(
                "the staging candidate at {} is not a complete valid Order record, so it \
                 is not promoted. It is left exactly where it is.",
                staged.location().absolute_path().display()
            ),
            paths: vec![in_package(staged.location())],
            identities: Vec::new(),
            package_root: Some(staged.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        MetadataRepairError::StagingCandidateDamagesPackage { staged, constraint } => {
            damages_package(constraint, &staged)
        }
        MetadataRepairError::PackageEvidenceUnavailable { root } => {
            package_evidence_unavailable(&root)
        }
        MetadataRepairError::StagingEvidencePresent { staged } => staging_evidence_present(&staged),
        MetadataRepairError::FileSystem { subject, error } => filesystem_failure(&subject, &error),
    }
}

fn semantic_refusal<E: std::fmt::Display>(
    directory: &Path,
    reason: SemanticRepairError<E>,
) -> Failed {
    match reason {
        SemanticRepairError::Diagnosis(reason) => order_diagnosis_failure(directory, reason).failed,
        SemanticRepairError::UnknownCase { handle, scope } => unknown_case(&handle, &scope),
        SemanticRepairError::UnauthorizedAction {
            handle,
            action,
            scope,
        } => unauthorized_action(&handle, action, &scope),
        SemanticRepairError::UnknownVendor { name, scope } => Failed {
            code: FailureCode::RepairVendorUnknown,
            scope: FailureScope::Order,
            message: format!(
                "this Package holds no Vendor named `{name}`, under the trimmed, \
                 case-insensitive comparison that makes Names unique. A repair chooses an \
                 existing Vendor, deliberately creates a distinct one, or clears the \
                 reference; it never creates one by accident."
            ),
            paths: vec![in_package(scope.location())],
            identities: vec![Identity::vendor_name(&name)],
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        SemanticRepairError::VendorNameCollision { name, scope } => Failed {
            code: FailureCode::RepairVendorNameInUse,
            scope: FailureScope::Order,
            message: format!(
                "a Vendor of this Package already carries the Name `{name}`, under the \
                 trimmed, case-insensitive comparison that makes Names unique. There is no \
                 merge: assign that Vendor, or create one with a deliberately distinct Name."
            ),
            paths: vec![in_package(scope.location())],
            identities: vec![Identity::vendor_name(&name)],
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        SemanticRepairError::MissingVendorName { scope } => Failed {
            code: FailureCode::RepairVendorNameRequired,
            scope: FailureScope::Order,
            message: "a deliberately created Vendor needs a real Name. A placeholder is not \
                      a Vendor, and nothing was written."
                .to_owned(),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        SemanticRepairError::InvalidRepairedOrder { scope } => Failed {
            code: FailureCode::RepairCandidateInvalid,
            scope: FailureScope::Order,
            message: "the Order this resolution would leave behind is not a valid record, \
                      so nothing was written."
                .to_owned(),
            paths: vec![in_package(scope.location())],
            identities: Vec::new(),
            package_root: Some(scope.root().to_path_buf()),
            diagnostics: Vec::new(),
        },
        SemanticRepairError::FileSystem { subject, error } => filesystem_failure(&subject, &error),
    }
}

fn metadata_rejection<E: std::fmt::Display>(rejection: MetadataRepairRejection<E>) -> Failed {
    match rejection {
        MetadataRepairRejection::EvidenceChanged { evidence } => evidence_changed(&evidence),
        MetadataRepairRejection::DestinationChanged {
            destination,
            uncleaned_stage,
        } => destination_changed(EvidenceScope::Order, &destination, uncleaned_stage.as_ref()),
        MetadataRepairRejection::ReplacementFailed {
            destination,
            uncleaned_stage,
            ..
        } => replacement_failed(EvidenceScope::Order, &destination, uncleaned_stage.as_ref()),
        MetadataRepairRejection::PromotionFailed { destination, error } => {
            promotion_failed(&destination, &error)
        }
    }
}

/// One refused semantic repair, under the Root its plan resolved.
///
/// **The Root is an argument because two of these have none of their own.** A minted
/// identity is a value with no place in it, and a multi-file replacement that failed
/// having restored everything names no file — yet both happened inside a Package that was
/// read, planned against, and written to. Taking the Root here rather than reaching for it
/// in each arm is what keeps a refusal from reporting that it was about no Package.
fn semantic_rejection<E>(root: &Path, rejection: SemanticRepairRejection<E>) -> Failed {
    match rejection {
        SemanticRepairRejection::EvidenceChanged { evidence } => evidence_changed(&evidence),
        SemanticRepairRejection::GeneratedIdentityCollision { order_id } => {
            identity_collision(root, &Identity::order(&order_id))
        }
        SemanticRepairRejection::GeneratedVendorIdentityCollision { vendor_id } => {
            identity_collision(root, &Identity::vendor(&vendor_id))
        }
        SemanticRepairRejection::PackageEvidenceUnavailable { root } => {
            package_evidence_unavailable(&root)
        }
        SemanticRepairRejection::ReplacementFailed { unrestored, .. } => {
            multi_file_replacement_failed(root, &unrestored)
        }
    }
}

fn present_order(answer: OrderAnswer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match order_document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(ORDER_CONTRACT, order_context(&answer)),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_order(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn order_context(answer: &OrderAnswer) -> OrderContext {
    match answer {
        OrderAnswer::Diagnosed { diagnosis, .. } => {
            OrderContext::rooted_at(diagnosis.package_root(), diagnosis.directory())
        }
        OrderAnswer::Unspellable {
            package_root,
            order_root,
        } => OrderContext::rooted_at(package_root, order_root),
        OrderAnswer::Failed(failed) => failed_order_context(failed),
    }
}

fn order_document(
    answer: &OrderAnswer,
) -> Result<(Response<Repaired, OrderContext>, Exit), NotRepresentable> {
    let (diagnosis, applied) = match answer {
        OrderAnswer::Diagnosed { diagnosis, applied } => (diagnosis, applied),
        // The one producer renders it, from the context this answer's two roots supply.
        OrderAnswer::Unspellable { .. } => return Err(NotRepresentable),
        OrderAnswer::Failed(failed) => {
            return Ok((
                failed_document(ORDER_CONTRACT, order_context(answer), &failed.failed)?,
                Exit::Failure,
            ));
        }
    };
    let context = order_context(answer);
    let observed = diagnostics(diagnosis.diagnostics())?;
    Ok((
        Response::answered(
            ORDER_CONTRACT,
            context,
            repaired(
                EvidenceScope::Order.as_str(),
                diagnosis.is_valid(),
                diagnosis.cases(),
                diagnosis.unactionable(),
                applied,
                &order_repair_at(diagnosis.directory()),
            )?,
            observed,
        ),
        Exit::Success,
    ))
}

fn human_order(answer: &OrderAnswer, output: &Output) -> io::Result<Exit> {
    let (diagnosis, applied) = match answer {
        OrderAnswer::Diagnosed { diagnosis, applied } => (diagnosis, applied),
        OrderAnswer::Unspellable { .. } => {
            output.error(UNSPELLABLE_PATH)?;
            return Ok(Exit::Failure);
        }
        OrderAnswer::Failed(failed) => return human_failure(&failed.failed, output),
    };
    human_scope(
        output,
        "Order Directory",
        diagnosis.directory().as_os_str(),
        diagnosis.package_root(),
        diagnosis.cases(),
        diagnosis.unactionable(),
        diagnosis.diagnostics(),
        applied.as_ref().as_ref(),
        &order_repair_at(diagnosis.directory()),
        diagnosis.is_valid(),
    )?;
    Ok(Exit::Success)
}

// ---------------------------------------------------------------------------
// What both scopes report, and the failures both can reach.
// ---------------------------------------------------------------------------

fn repaired(
    scope: &'static str,
    valid: bool,
    cases: &[RepairCase],
    unactionable: &[UnactionableFinding],
    applied: &Option<Applied>,
    command: &str,
) -> Result<Repaired, NotRepresentable> {
    Ok(Repaired {
        scope,
        valid,
        cases: cases
            .iter()
            .map(|case| case_projection(case, command))
            .collect::<Result<_, _>>()?,
        unactionable: unactionable
            .iter()
            .map(UnactionableProjection::of)
            .collect::<Result<_, _>>()?,
        report: applied.as_ref().map(|applied| Report {
            committed: applied.committed,
            // A case is diagnosed only where what stands on disk is not what the repair
            // would leave, so an applied repair always moves something. `--dry-run`
            // decides `committed`; this says the plan is not a no-op.
            changed: true,
            case: applied.case.clone(),
            action: applied.action.as_str(),
            operations: applied.operations.clone(),
            warnings: applied.warnings.clone(),
        }),
        index: applied.as_ref().and_then(|applied| applied.index.clone()),
    })
}

fn case_projection(case: &RepairCase, command: &str) -> Result<CaseProjection, NotRepresentable> {
    Ok(CaseProjection {
        case: case.handle().to_owned(),
        code: case.code().as_str(),
        location: LocationProjection::of(case.location())?,
        actions: offered_actions(case.actions(), |action| {
            Some(suggested_application(command, case.handle(), action))
        }),
    })
}

/// The exact command that applies one action of one diagnosed case.
///
/// **Every value the action requires is named**, with nothing after the `=` where the
/// diagnosis cannot supply one — so the string is runnable verbatim for an action that
/// needs none, and for an action that needs one it names the assignment the caller must
/// fill in and is refused rather than half-applied when it is not.
fn suggested_application(command: &str, handle: &str, action: RepairAction) -> String {
    let mut suggested = format!(
        "{command} case={} action={}",
        shell_word(handle),
        action.as_str()
    );
    for field in action.required_values() {
        suggested.push_str(&format!(" {field}="));
    }
    suggested
}

#[expect(clippy::too_many_arguments, reason = "one human answer, ten facts")]
fn human_scope(
    output: &Output,
    subject: &str,
    subject_path: &std::ffi::OsStr,
    root: &Path,
    cases: &[RepairCase],
    unactionable: &[UnactionableFinding],
    observed: &[Diagnostic],
    applied: Option<&Applied>,
    command: &str,
    valid: bool,
) -> io::Result<()> {
    output.summary(subject, subject_path)?;
    output.summary("Valid", &text(if valid { "yes" } else { "no" }))?;
    if let Some(applied) = applied {
        output.summary(
            if applied.committed {
                "Applied"
            } else {
                "Would apply"
            },
            &text(applied.action.as_str()),
        )?;
    }
    output.summary("Cases", &text(&cases.len().to_string()))?;
    for case in cases {
        output.summary(case.code().as_str(), located(case.location()))?;
        // The suggested command is a summary rather than a detail, because it is what a
        // reader came for: the decided human output shows "the suggested next command" for
        // a finding, and a command line only `--verbose` printed would be one this
        // application knows and does not say.
        for action in case.actions() {
            output.summary(
                action.as_str(),
                &text(&suggested_application(command, case.handle(), *action)),
            )?;
        }
    }
    for finding in unactionable {
        output.warning(&format!(
            "{} at {} invalidates this scope and nothing here can act on it. Correcting it \
             is work outside this application, after which `spielorders validate` reports \
             the result.",
            finding.code().as_str(),
            finding.location().absolute_path().display()
        ))?;
    }
    for diagnostic in observed {
        report_diagnostic(output, Some(root), diagnostic)?;
    }
    Ok(())
}

fn unknown_case(handle: &str, scope: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairCaseUnknown,
        scope: FailureScope::Operation,
        message: format!(
            "no currently diagnosed case answers to `{handle}`. A case handle is derived \
             from the evidence it was diagnosed from, so a handle from an older diagnosis \
             stops naming a case as soon as that evidence moves — it is refused rather \
             than resolved against whatever stands there now. Diagnose the scope again."
        ),
        paths: vec![in_package(scope.location())],
        identities: Vec::new(),
        package_root: Some(scope.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn unauthorized_action(handle: &str, action: RepairAction, scope: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairActionUnauthorized,
        scope: FailureScope::Operation,
        message: format!(
            "the case `{handle}` does not offer `{}`. A repair applies only an action its \
             own diagnosis offered; the diagnosis names them.",
            action.as_str()
        ),
        paths: vec![in_package(scope.location())],
        identities: Vec::new(),
        package_root: Some(scope.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// The command line supplied a resolution the action does not take, in either scope.
fn invalid_resolution(
    action: RepairAction,
    failure_scope: FailureScope,
    scope: &RootedLocation,
) -> Failed {
    Failed {
        code: FailureCode::RepairResolutionUnexpected,
        scope: failure_scope,
        message: format!(
            "the resolution supplied is not the one `{}` takes. Nothing was written.",
            action.as_str()
        ),
        paths: vec![in_package(scope.location())],
        identities: Vec::new(),
        package_root: Some(scope.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// The candidate is valid on its own and breaks a Package-wide constraint.
///
/// **The constraint travels as an identity as well as in the sentence**, because which one
/// a candidate broke is the difference between picking another Vendor, taking a free
/// identity, and correcting a locator — a fact a caller branches on rather than reads.
fn damages_package(constraint: PackageConstraint, scope: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairCandidateDamagesPackage,
        scope: FailureScope::Package,
        message: format!(
            "the candidate is a valid record on its own and would still leave this Package \
             damaged: {}. Nothing was written: answering one diagnosed condition by \
             creating another is not a repair.",
            constraint.as_str()
        ),
        paths: vec![in_package(scope.location())],
        identities: vec![Identity::package_constraint(constraint.as_str())],
        package_root: Some(scope.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn staging_evidence_present(staged: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairStagingEvidencePresent,
        scope: FailureScope::Operation,
        message: format!(
            "a replacement stages beside its destination, and abandoned staging evidence \
             already occupies {}. Writing there would destroy evidence this application \
             may neither overwrite nor silently clean, so the staging case is resolved \
             first and this replacement is refused until then.",
            staged.location().absolute_path().display()
        ),
        paths: vec![in_package(staged.location())],
        identities: Vec::new(),
        package_root: Some(staged.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn package_evidence_unavailable(root: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairPackageEvidenceUnavailable,
        scope: FailureScope::Package,
        message: format!(
            "the Package at {} could not be read as a current Package while this repair's \
             candidate was being judged against it. Nothing shows the candidate is safe, \
             so it is refused rather than written against evidence nobody has. \
             `spielorders validate` reports what is wrong with the Package.",
            root.location().absolute_path().display()
        ),
        paths: vec![in_package(root.location())],
        identities: Vec::new(),
        package_root: Some(root.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn evidence_changed(evidence: &RootedLocation) -> Failed {
    Failed {
        code: FailureCode::RepairEvidenceChanged,
        scope: FailureScope::Operation,
        message: format!(
            "{} stopped holding what this repair was authorized against, so it is no \
             longer the case that was diagnosed. Nothing was written. Diagnose the scope \
             again.",
            evidence.location().absolute_path().display()
        ),
        paths: vec![in_package(evidence.location())],
        identities: Vec::new(),
        package_root: Some(evidence.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// Somebody else wrote the destination between the diagnosis and this write.
///
/// **The staging file is what a leftover names, not the destination.** The destination
/// holds exactly what it held — that is the whole refusal — so a message claiming the
/// remaining file is named would send a reader to look at the one file that is certainly
/// fine. Where a stage remains it is carried beside the destination and the message says
/// which is which.
fn destination_changed(
    scope: EvidenceScope,
    destination: &RootedLocation,
    stage: Option<&RootedLocation>,
) -> Failed {
    let mut paths = vec![in_package(destination.location())];
    paths.extend(stage.map(|stage| in_package(stage.location())));
    Failed {
        code: if stage.is_some() {
            FailureCode::RepairCleanupFailed
        } else {
            FailureCode::RepairDestinationChanged
        },
        scope: FailureScope::Operation,
        message: format!(
            "{} stopped holding what the diagnosis observed there, so someone wrote it \
             after this repair was authorized. Nothing was written and their work stands. \
             Nothing failed: this is a refusal, not a write error.{}",
            destination.location().absolute_path().display(),
            leftover_stage(scope, stage)
        ),
        paths,
        identities: Vec::new(),
        package_root: Some(destination.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// One managed artifact this repair could not replace.
///
/// **The typed failure decides the code and nothing is spelled by `Debug`.** Which step of
/// a durable replacement failed is not a fact a caller acts on — the two answers that
/// differ are "everything holds what it held" and "a staging artifact was left behind" —
/// so those are the two this reports, exactly as an Order edit reports the same primitive.
fn replacement_failed(
    scope: EvidenceScope,
    destination: &RootedLocation,
    stage: Option<&RootedLocation>,
) -> Failed {
    let mut paths = vec![in_package(destination.location())];
    paths.extend(stage.map(|stage| in_package(stage.location())));
    Failed {
        code: if stage.is_some() {
            FailureCode::RepairCleanupFailed
        } else {
            FailureCode::RepairFailed
        },
        scope: FailureScope::Operation,
        message: format!(
            "{} could not be replaced and holds what it held.{}",
            destination.location().absolute_path().display(),
            leftover_stage(scope, stage)
        ),
        paths,
        identities: Vec::new(),
        package_root: Some(destination.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// The already-durable staging candidate could not be published over its destination.
fn promotion_failed<E: std::fmt::Display>(destination: &RootedLocation, error: &E) -> Failed {
    Failed {
        code: FailureCode::RepairFailed,
        scope: FailureScope::Operation,
        message: format!(
            "{} could not be published: {error}. Every managed file holds what it held, \
             and the staging candidate stands where it stood.",
            destination.location().absolute_path().display()
        ),
        paths: vec![in_package(destination.location())],
        identities: Vec::new(),
        package_root: Some(destination.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// A repair of several managed files at once that could not be completed.
///
/// **What a caller needs is which files are not as they were**, so those are what this
/// carries. The ordinary failure restores every earlier destination and leaves nothing to
/// name; a restoration that failed, or one declined because someone else had since written
/// that destination, is a file standing in neither state and is named here.
///
/// Both branches report the Root, because both happened under one: naming a file is what
/// distinguishes them, and a Package is what a repair runs inside either way.
fn multi_file_replacement_failed(root: &Path, unrestored: &[RootedLocation]) -> Failed {
    if unrestored.is_empty() {
        return Failed {
            code: FailureCode::RepairFailed,
            scope: FailureScope::Operation,
            message: "the repair could not be written, and every managed file holds what it \
                      held."
                .to_owned(),
            paths: Vec::new(),
            identities: Vec::new(),
            package_root: Some(root.to_path_buf()),
            diagnostics: Vec::new(),
        };
    }
    Failed {
        code: FailureCode::RepairRestorationFailed,
        scope: FailureScope::Operation,
        message: "the repair could not be written, and the managed files named here could \
                  not be put back as they were. They hold neither what they held nor a \
                  complete repair; `spielorders validate` reports what stands now."
            .to_owned(),
        paths: unrestored
            .iter()
            .map(|location| in_package(location.location()))
            .collect(),
        identities: Vec::new(),
        package_root: Some(root.to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// The sentence a refusal or failure adds when it left a staging file behind.
///
/// **The scope is the caller's**, because both repair scopes reach these two failures and
/// the command that answers for a leftover stage is the one whose reserved directory it
/// stands in. A sentence deciding that for itself named the Package command for a stage
/// inside an Order — the false suggestion `repair_suggestion` exists to stop.
fn leftover_stage(scope: EvidenceScope, stage: Option<&RootedLocation>) -> String {
    match stage {
        Some(_) => format!(
            " The staging file this attempt created could not be removed again and is \
             named beside it; this application will not guess whether it may delete it. {}",
            RepairSuggestion::for_condition(
                match scope {
                    EvidenceScope::Order => Rooting::Order,
                    // The other three name no Order to root at, and a stage under one of
                    // them authorizes nothing anyway.
                    EvidenceScope::Package | EvidenceScope::Document | EvidenceScope::Operation =>
                        Rooting::Package,
                },
                DiagnosticCode::StagingArtifactPresent,
                scope,
            )
        ),
        None => String::new(),
    }
}

/// A minted identity a record of this Package already carries.
///
/// The message says "this Package" and the response names which one: the collision was
/// decided against the Package this repair was planned in, so it is the Package the
/// sentence is about.
fn identity_collision(root: &Path, identity: &Identity) -> Failed {
    Failed {
        code: FailureCode::RepairIdentityCollision,
        scope: FailureScope::Package,
        message: "the identity minted for this repair is already carried by a record of \
                  this Package. Nothing was written, and running the repair again mints \
                  another."
            .to_owned(),
        paths: Vec::new(),
        identities: vec![identity.clone()],
        package_root: Some(root.to_path_buf()),
        diagnostics: Vec::new(),
    }
}

fn filesystem_failure<E: std::fmt::Display>(subject: &RootedLocation, error: &E) -> Failed {
    Failed {
        code: FailureCode::PackageUnreadable,
        scope: FailureScope::Operation,
        message: format!(
            "{} could not be read: {error}. Nothing was written.",
            subject.location().absolute_path().display()
        ),
        paths: vec![in_package(subject.location())],
        identities: Vec::new(),
        package_root: Some(subject.root().to_path_buf()),
        diagnostics: Vec::new(),
    }
}
