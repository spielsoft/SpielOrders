//! `validate` and `order validate`: the two commands whose answer *is* the findings.
//!
//! **One module for both**, because they are one question over two scopes. Both return
//! every diagnostic at once rather than stopping at the first, both take their codes and
//! their evidence from the core's own diagnostics, and both decide their exit status on
//! severity alone — so a condition cannot come to be reported one way about a Package and
//! another way about one of its Orders.
//!
//! **Validity is `ok`, and the findings are `diagnostics`.** A result is invalid exactly
//! when some finding carries error severity, which is the predicate the core already
//! computes; the exit status is that predicate and `ok` is the exit status. So an invalid
//! result is a failed response whose diagnostics are the whole answer — which is the one
//! channel that carries them in both outcomes, since `data` is `null` on a failure. What
//! `data` carries instead is what the run examined, the one fact nothing else in the
//! document says.
//!
//! **Nothing here acts on a finding.** There is no `--fix`, no `--repair`, and no option
//! that takes an action: validation reports, and applying anything is `repair`'s.
//!
//! **What is reported beside a finding is the repair offered for it**, in `repair`'s own
//! action vocabulary, so a caller can see what may be done without a second command — and
//! see it for a valid Package whose warnings offer cleanup, which is exactly the case the
//! exit status deliberately says nothing about. Each offer names its diagnostic by the
//! code and place the envelope already carries, never by position, and the collection is
//! present and possibly empty. The ephemeral case handle a confirmation needs is not here:
//! it is `repair`'s own answer, and minting one would make a validation a mutation's
//! authorization.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::{
    commands::{
        order::{OrderFailed, Target, failed_order_context, targeted_directory},
        repair_suggestion::{RepairSuggestion, Rooting},
        shared::{
            Failed, OfferedRepairProjection, OrderPathsProjection, diagnostics, failed_context,
            failed_document, flattened, human_failure, not_representable, offered_repairs,
            operation_failure, order_diagnosis_failure, package_context, report_diagnostic,
            report_diagnostic_groups, text, undeliverable, unmanaged_failure,
        },
    },
    contract::ResponseContract,
    effects::PlatformFileSystem,
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, LocationProjection, NotRepresentable, OrderContext,
        Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    canonical::{
        ExhaustivePackageValidation, ExhaustiveValidationReport, validate_package_exhaustively,
    },
    order_diagnosis::{OrderDiagnosis, diagnose_order},
    readme::EvidenceScope,
};
use std::{io, path::Path};

const PACKAGE_CONTRACT: ResponseContract = ResponseContract::Validate;
const ORDER_CONTRACT: ResponseContract = ResponseContract::OrderValidate;

// ---------------------------------------------------------------------------
// `validate`
// ---------------------------------------------------------------------------

/// The `validate` payload: what this run examined.
///
/// **Neither the verdict nor the findings are here.** The verdict is `ok`, which is the
/// exit status, which is the severity predicate — one fact with one home. The findings
/// are the envelope's `diagnostics`, which is the only member that carries them when the
/// Package is invalid and `data` is therefore `null`.
///
/// What is left is what nothing else in the document says: which scope was validated, and
/// what was inside it. Without that, a run with no findings over an empty Orders Directory
/// and one over forty healthy Orders are the same response.
#[derive(Debug, Serialize)]
struct ExaminedPackage {
    scope: &'static str,
    /// The conventional Package README this run read.
    package_readme: LocationProjection,
    /// Every Order Directory this run examined, healthy or damaged, in the core's own
    /// deterministic order. Present and empty for a Package holding none.
    orders: Vec<OrderPathsProjection>,
    /// The repair offered for each finding that has one, in `repair`'s action vocabulary.
    offered_repairs: Vec<OfferedRepairProjection>,
}

/// What the command decided, before anything is presented.
enum PackageAnswer {
    Examined(Box<ExhaustiveValidationReport>),
    Failed(Box<Failed>),
}

pub(crate) fn package(selected_root: &Path, output: &Output) -> Exit {
    present_package(decide_package(selected_root), output)
}

/// The exhaustive validation, and the three states that have no findings to report.
///
/// `validate_package_exhaustively` is the core capability whose whole purpose this is: it
/// reuses the read model's own classification and then reads every document and every
/// reserved directory that the ordinary load deliberately does not. It writes nothing.
///
/// An unmanaged directory and an unreadable Root are classified exactly as every other
/// command classifies them, through the shared constructors — a validating command is not
/// licensed to invent a second answer for "there is no Package here".
fn decide_package(selected_root: &Path) -> PackageAnswer {
    match validate_package_exhaustively(selected_root, &PlatformFileSystem) {
        ExhaustivePackageValidation::Complete(report) => PackageAnswer::Examined(Box::new(report)),
        ExhaustivePackageValidation::Unmanaged(package) => {
            PackageAnswer::Failed(Box::new(unmanaged_failure(&package)))
        }
        ExhaustivePackageValidation::OperationFailed(failed) => {
            PackageAnswer::Failed(Box::new(operation_failure(failed.failure(), failed.root())))
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
        PackageAnswer::Examined(report) => Some(report.context().root().absolute_path()),
        PackageAnswer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn package_document(
    answer: &PackageAnswer,
) -> Result<(Response<ExaminedPackage>, Exit), NotRepresentable> {
    let report = match answer {
        PackageAnswer::Examined(report) => report,
        PackageAnswer::Failed(failed) => {
            return Ok((
                failed_document(PACKAGE_CONTRACT, failed_context(failed)?, failed)?,
                Exit::Failure,
            ));
        }
    };
    let context = Context::rooted_at(report.context().root().absolute_path())?;
    let observed = flattened(report.diagnostic_groups());
    if !report.is_valid() {
        // The diagnostics are the answer, so they travel in the envelope — where a
        // failed response still carries them. Severity alone decided this; whether any
        // finding offers a repair never does.
        return Ok((
            Response::failed(
                PACKAGE_CONTRACT,
                context,
                invalid_package(report).error()?,
                diagnostics(&observed)?,
            ),
            Exit::Failure,
        ));
    }
    Ok((
        Response::answered(
            PACKAGE_CONTRACT,
            context,
            examined_package(report)?,
            diagnostics(&observed)?,
        ),
        Exit::Success,
    ))
}

fn examined_package(
    report: &ExhaustiveValidationReport,
) -> Result<ExaminedPackage, NotRepresentable> {
    Ok(ExaminedPackage {
        scope: EvidenceScope::Package.as_str(),
        package_readme: LocationProjection::of(report.package_readme().reference().location())?,
        orders: report
            .examined_orders()
            .iter()
            .map(OrderPathsProjection::of)
            .collect::<Result<_, _>>()?,
        offered_repairs: offered_repairs(&flattened(report.diagnostic_groups()))?,
    })
}

/// The typed failure an invalid Package is reported as.
///
/// A separate code from the damaged-Package one on purpose: a Package the load could not
/// trust at all is `package_root_damaged`, and this is a Package the load read completely
/// and judged invalid — the diagnostics below it are a diagnosis rather than the reason
/// there is none.
fn invalid_package(report: &ExhaustiveValidationReport) -> Failed {
    let root = report.context().root();
    Failed {
        code: FailureCode::PackageInvalid,
        scope: FailureScope::Package,
        message: format!(
            "the Package at {} is invalid: at least one finding carries error severity. \
             The diagnostics are the answer; {}",
            root.absolute_path().display(),
            RepairSuggestion::for_findings(
                Rooting::Package,
                &flattened(report.diagnostic_groups())
            )
        ),
        // The core's own `Location`, so the Root carries its Package-relative spelling
        // — which is `.` — exactly as it does in every other response that names it.
        paths: vec![root.clone().into()],
        identities: Vec::new(),
        package_root: Some(root.absolute_path().to_path_buf()),
        diagnostics: flattened(report.diagnostic_groups()),
    }
}

/// The human answer: the verdict, what was examined, and every finding under the scope
/// and the place that owns it.
fn human_package(answer: &PackageAnswer, output: &Output) -> io::Result<Exit> {
    let report = match answer {
        PackageAnswer::Examined(report) => report,
        PackageAnswer::Failed(failed) => return human_failure(failed, output),
    };
    output.summary(
        "Package Root",
        report.context().root().absolute_path().as_os_str(),
    )?;
    output.summary("Valid", &text(if report.is_valid() { "yes" } else { "no" }))?;
    output.summary(
        "Orders examined",
        &text(&report.examined_orders().len().to_string()),
    )?;
    output.detail(
        "Package README",
        report
            .package_readme()
            .reference()
            .location()
            .absolute_path()
            .as_os_str(),
    )?;
    report_diagnostic_groups(
        output,
        report.context().root().absolute_path(),
        report.diagnostic_groups(),
    )?;
    if report.is_valid() {
        return Ok(Exit::Success);
    }
    output.error(&invalid_package(report).message)?;
    Ok(Exit::Failure)
}

// ---------------------------------------------------------------------------
// `order validate`
// ---------------------------------------------------------------------------

/// The `order validate` payload: what this run examined.
///
/// One member, for the reason [`ExaminedPackage`] has three fewer than a reader might
/// expect: the verdict is `ok`, the findings are the envelope's `diagnostics`, and the
/// Order Directory and the Package it was judged against are the envelope's `context` —
/// which an `order` command names in every outcome. What is left is the scope those
/// findings are about.
#[derive(Debug, Serialize)]
struct ExaminedOrder {
    scope: &'static str,
    /// The repair offered for each finding in this scope that has one, in `repair`'s
    /// action vocabulary.
    offered_repairs: Vec<OfferedRepairProjection>,
}

/// What the command decided, before anything is presented.
enum OrderAnswer {
    Examined(Box<OrderDiagnosis>),
    Failed(Box<OrderFailed>),
}

pub(crate) fn order(target: &Target, output: &Output) -> Exit {
    present_order(decide_order(target), output)
}

/// The Order Directory this command line named, and the scoped diagnosis of it.
///
/// `diagnose_order` is the core capability for exactly this: it takes the Package-wide
/// exhaustive validation and narrows it to one Order, so a scoped answer and a
/// Package-wide one cannot disagree about the same evidence or name one condition with two
/// codes. It reaches the Package by walking the Order Directory's own ancestors for a
/// marker, which is the route the integrity decision authorizes for an Order whose record
/// may carry no usable locator at all — and that Order is precisely the one this command
/// exists to judge.
fn decide_order(target: &Target) -> OrderAnswer {
    let directory = match targeted_directory(target) {
        Ok(directory) => directory,
        Err(failed) => return OrderAnswer::Failed(failed),
    };
    match diagnose_order(&directory, &PlatformFileSystem) {
        Ok(diagnosis) => OrderAnswer::Examined(Box::new(diagnosis)),
        Err(reason) => OrderAnswer::Failed(Box::new(order_diagnosis_failure(&directory, reason))),
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
        // A diagnosis established both roots: the Package it was taken against is the one
        // the Order Directory's own position resolved to, and reporting it is the whole
        // point of the diagnosis carrying it.
        OrderAnswer::Examined(diagnosis) => {
            OrderContext::rooted_at(diagnosis.package_root(), diagnosis.directory())
        }
        OrderAnswer::Failed(failed) => failed_order_context(failed),
    }
}

fn order_document(
    answer: &OrderAnswer,
) -> Result<(Response<ExaminedOrder, OrderContext>, Exit), NotRepresentable> {
    let diagnosis = match answer {
        OrderAnswer::Examined(diagnosis) => diagnosis,
        OrderAnswer::Failed(failed) => {
            return Ok((
                failed_document(ORDER_CONTRACT, order_context(answer), &failed.failed)?,
                Exit::Failure,
            ));
        }
    };
    let context = order_context(answer);
    if !diagnosis.is_valid() {
        return Ok((
            Response::failed(
                ORDER_CONTRACT,
                context,
                invalid_order(diagnosis).error()?,
                diagnostics(diagnosis.diagnostics())?,
            ),
            Exit::Failure,
        ));
    }
    Ok((
        Response::answered(
            ORDER_CONTRACT,
            context,
            ExaminedOrder {
                scope: EvidenceScope::Order.as_str(),
                offered_repairs: offered_repairs(diagnosis.diagnostics())?,
            },
            diagnostics(diagnosis.diagnostics())?,
        ),
        Exit::Success,
    ))
}

/// The typed failure an invalid Order scope is reported as.
fn invalid_order(diagnosis: &OrderDiagnosis) -> Failed {
    Failed {
        code: FailureCode::OrderInvalid,
        scope: FailureScope::Order,
        message: format!(
            "the Order at {} is invalid: at least one finding in its scope carries error \
             severity. The diagnostics are the answer; {}",
            diagnosis.directory().display(),
            // This Order's own findings, and this response is rooted where an Order
            // command would be run — so a command it names needs no Directory beside it.
            RepairSuggestion::for_findings(Rooting::Order, diagnosis.diagnostics())
        ),
        // The Package's own account of where this Order is, so the Directory carries its
        // Package-relative spelling in the refusal exactly as it does in a success.
        paths: vec![diagnosis.paths().directory().clone().into()],
        identities: Vec::new(),
        package_root: Some(diagnosis.package_root().to_path_buf()),
        diagnostics: diagnosis.diagnostics().to_vec(),
    }
}

fn human_order(answer: &OrderAnswer, output: &Output) -> io::Result<Exit> {
    let diagnosis = match answer {
        OrderAnswer::Examined(diagnosis) => diagnosis,
        OrderAnswer::Failed(failed) => return human_failure(&failed.failed, output),
    };
    output.summary("Order Directory", diagnosis.directory().as_os_str())?;
    output.summary(
        "Valid",
        &text(if diagnosis.is_valid() { "yes" } else { "no" }),
    )?;
    output.detail("Package Root", diagnosis.package_root().as_os_str())?;
    for diagnostic in diagnosis.diagnostics() {
        report_diagnostic(output, Some(diagnosis.package_root()), diagnostic)?;
    }
    if diagnosis.is_valid() {
        return Ok(Exit::Success);
    }
    output.error(&invalid_order(diagnosis).message)?;
    Ok(Exit::Failure)
}
