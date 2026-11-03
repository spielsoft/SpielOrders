//! `init`: the one command whose subject is a directory that is not a Package yet.
//!
//! **The first mutation this executable performs**, so the mutation response shape is
//! established here for the mutations that follow: a typed report — `committed`,
//! `changed`, the managed operations it planned — beside the refreshed affected result,
//! which for an initialization is the Package that now stands there. Ticket 008 owns that
//! pair as the cross-command artifact; nothing here is a second mutation contract.
//!
//! **`--dry-run` is the same answer with `committed: false`.** It plans, validates, and
//! reports, and it never reaches the commit — so no directory, no managed file, no
//! staging artifact, and no cleanup effect exists afterward. That is why the plan and the
//! commit are separate core capabilities and why this module calls the second one only
//! when it was asked to.
//!
//! **Every Package rule is the core's.** Which artifacts a Package has, where they go,
//! what bytes they carry, which configured paths are legal, and what to roll back when a
//! write fails are all decided in `spielorders_core::initialization`. This module reads
//! the command line, asks, presents, and exits.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution.

use crate::{
    cli::Assignment,
    commands::shared::{
        Failed, RootKind, as_given, failed_context, failed_document, link_named_as_a_root,
        not_representable, package_context, resolved, text, undeliverable,
    },
    contract::ResponseContract,
    effects::PlatformFileSystem,
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, Identity, LocationProjection, NotRepresentable,
        Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    initialization::{
        InitializationCleanupFailure, InitializationCommitFailure, InitializationDestinationKind,
        InitializationOperation, InitializationOperationKind, InitializationOptions,
        InitializationPlanError, InitializationRejection, InitializationWarning,
        PackageInitializationPlan, commit_package_initialization, plan_package_initialization,
    },
    mutation::MutationReport,
    operations::FailedPath,
    readme::{ConfigurationPathKind, ConfigurationPathReason},
};
use std::{io, path::Path};

const CONTRACT: ResponseContract = ResponseContract::Init;

/// What the command decided, before anything is written anywhere.
///
/// Deciding and presenting are separate so that the four output modes present one answer
/// rather than each recomputing it — the shape in which two modes come to disagree about
/// the same run.
enum Answer {
    /// The plan this command produced, and whether it was carried out. A dry run's
    /// answer is this one with `committed` false: the plan is the report either way.
    Planned {
        plan: Box<PackageInitializationPlan>,
        committed: bool,
    },
    Failed(Failed),
}

/// The two configured paths this command line asked for, with the conventional default
/// standing in for each key it left out.
///
/// The defaults are the core's own, read off [`InitializationOptions::default`] rather
/// than spelled here: which directory a Package conventionally keeps its Orders in is a
/// Package fact, and an adapter with its own copy of it is a second place to change.
fn options(assignments: &[Assignment]) -> InitializationOptions {
    let conventional = InitializationOptions::default();
    let assigned = |key| {
        assignments
            .iter()
            .find(|assignment| assignment.key == key)
            .map(|assignment| assignment.value.as_str())
    };
    InitializationOptions::new(
        assigned(ConfigurationPathKind::OrdersPath).unwrap_or(conventional.orders_path()),
        assigned(ConfigurationPathKind::RemovedPath).unwrap_or(conventional.removed_path()),
    )
}

pub(crate) fn init(
    selected_root: &Path,
    assignments: &[Assignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    present(decide(selected_root, assignments, dry_run), output)
}

/// Plans, and commits unless this run was asked to plan only.
///
/// The target reaches the core exactly as the caller wrote it: the core canonicalizes the
/// selected root through the filesystem port because its own rules require it, and an
/// adapter that resolved it first would be a second place deciding what `--root` names.
fn decide(selected_root: &Path, assignments: &[Assignment], dry_run: bool) -> Answer {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return Answer::Failed(failed);
    }
    let plan = match plan_package_initialization(
        selected_root,
        &options(assignments),
        &PlatformFileSystem,
    ) {
        Ok(plan) => plan,
        Err(error) => return Answer::Failed(planning_refused(selected_root, error)),
    };
    if dry_run {
        return Answer::Planned {
            plan: Box::new(plan),
            committed: false,
        };
    }
    match commit_package_initialization(plan, &PlatformFileSystem) {
        // `Unchanged` beside `Committed` because both say the commit succeeded, and an
        // initialization cannot report the first: it is refused where a Root already
        // stands, so every plan it produces creates at least the metadata directory. The
        // report reads `changed` off the plan either way, so a plan that created nothing
        // would say so honestly rather than by this arm's choice.
        MutationReport::Committed { plan, .. } | MutationReport::Unchanged { plan, .. } => {
            Answer::Planned {
                plan: Box::new(plan),
                committed: true,
            }
        }
        MutationReport::CommittedWithWarning { plan, warnings, .. } => {
            // Named rather than folded above so that the first initialization warning
            // the core defines is a compile error here instead of a silence. There is
            // none today: `InitializationWarning` has no variants, which is why the
            // projection below can only produce an empty collection.
            let _: Vec<&str> = warnings.into_iter().map(warning).collect();
            Answer::Planned {
                plan: Box::new(plan),
                committed: true,
            }
        }
        MutationReport::Rejected { plan, rejection } => {
            Answer::Failed(commit_refused(&plan, rejection))
        }
    }
}

/// The stable code for one initialization warning.
///
/// Unreachable by construction: `InitializationWarning` is an empty enum, so there is no
/// value to pass. It exists so that adding a variant to it in the core fails to compile
/// here until this response says how the warning is spelled.
fn warning(warning: InitializationWarning) -> &'static str {
    match warning {}
}

// ---------------------------------------------------------------------------
// The refusals.
// ---------------------------------------------------------------------------

/// Why the plan was refused, as one typed failure.
///
/// **Each configuration path rule is its own code**, because the rule broken is what a
/// caller has to act on and each is a different correction; the key that broke it travels
/// beside it as an identity. Both spellings are the core's own, so a rule refused here
/// and the same rule diagnosed in a loaded Package are named identically.
fn planning_refused(selected_root: &Path, error: InitializationPlanError<io::Error>) -> Failed {
    match error {
        InitializationPlanError::RootNotOrdinaryDirectory => rootless(
            FailureCode::PackageRootNotADirectory,
            format!(
                "{} is not an ordinary directory, so there is nothing there to make a \
                 Package of. --root names the directory itself and no ancestor of it is \
                 consulted.",
                selected_root.display()
            ),
            selected_root,
        ),
        // The one refusal that arrives from either side of the resolution, so it is the
        // one whose spelling depends on which: a path the core resolved is reported
        // resolved and under the Root it resolved, and the selected path it never got
        // past is the caller's own text.
        InitializationPlanError::FileSystem { root, path, error } => {
            let message = format!(
                "{} could not be examined, so this initialization was not attempted: \
                 {error}",
                path.display()
            );
            match root {
                Some(root) => refused(
                    FailureCode::InitializationFilesystemFailed,
                    message,
                    // No Package surrounds it: `init` is what makes this directory a Root,
                    // and nothing has been written, so there is no Package to spell it in.
                    resolved(&path),
                    Vec::new(),
                    root.absolute_path(),
                ),
                None => rootless(FailureCode::InitializationFilesystemFailed, message, &path),
            }
        }
        InitializationPlanError::AlreadyManaged { root } => refused(
            FailureCode::PackageRootAlreadyManaged,
            format!(
                "{} is already a SpielOrders Root. It is left exactly as it is; \
                 `spielorders package show` reads it.",
                root.absolute_path().display()
            ),
            root.clone().into(),
            Vec::new(),
            root.absolute_path(),
        ),
        InitializationPlanError::InvalidConfiguredPath {
            root,
            path,
            failure,
        } => {
            let key = ConfigurationPathKind::from(path);
            let reason = ConfigurationPathReason::from(failure);
            refused(
                FailureCode::of_configuration_path(reason),
                format!(
                    "`{}` is not a usable configured path: {}. Each must be a nonempty \
                     `/`-separated path inside the Package, and `{}` must name an \
                     ordinary direct child of it.",
                    key.as_str(),
                    reason.as_str(),
                    ConfigurationPathKind::RemovedPath.as_str()
                ),
                root.clone().into(),
                vec![Identity::configuration_path(key)],
                root.absolute_path(),
            )
        }
        InitializationPlanError::ManagedPathsOverlap { root } => refused(
            FailureCode::ConfigurationPathsOverlap,
            format!(
                "`{}` and `{}` name one place, or one of them holds the other. They must \
                 be two separate directories.",
                ConfigurationPathKind::OrdersPath.as_str(),
                ConfigurationPathKind::RemovedPath.as_str()
            ),
            root.clone().into(),
            Vec::new(),
            root.absolute_path(),
        ),
        InitializationPlanError::PlannedPathCollision {
            root,
            configured,
            path,
        } => refused(
            FailureCode::ConfigurationPathReservedReadme,
            format!(
                "{} is where this Package keeps its own conventional README, so no \
                 configured managed path may name it.",
                path.absolute_path().display()
            ),
            path.into(),
            vec![Identity::configuration_path(configured.into())],
            root.absolute_path(),
        ),
        InitializationPlanError::DestinationCollision { root, path, actual } => refused(
            FailureCode::InitializationDestinationOccupied,
            format!(
                "{} already exists as {} and this initialization would have had to create \
                 it. Nothing was written.",
                path.absolute_path().display(),
                actual.as_str()
            ),
            path.into(),
            Vec::new(),
            root.absolute_path(),
        ),
        InitializationPlanError::DisallowedLink { root, path } => refused(
            FailureCode::InitializationDisallowedLink,
            format!(
                "{} resolves through a link this application does not follow, so what it \
                 names is not known to be inside the Package.",
                path.absolute_path().display()
            ),
            path.into(),
            Vec::new(),
            root.absolute_path(),
        ),
    }
}

/// Why the commit was refused, as one typed failure.
///
/// **A failed rollback is its own code**, because the action differs: where everything
/// created was removed again the directory is as it was and the command can simply be
/// run again, and where it was not, the named paths are what remains and this
/// application will not guess whether it may delete them. Each cleanup failure's path is
/// named beside the failure that caused it, which is the whole of what an honest report
/// of a partial rollback is.
fn commit_refused(
    plan: &PackageInitializationPlan,
    rejection: InitializationRejection<io::Error>,
) -> Failed {
    match rejection {
        InitializationRejection::PreconditionsChanged { path } => refused(
            FailureCode::PackageRootChanged,
            format!(
                "{} changed between planning this initialization and performing it, so \
                 what would have been written is no longer what was validated. Nothing \
                 was written.",
                path.display()
            ),
            // No Package surrounds it, for the reason the refusal above gives: this
            // initialization wrote nothing, so no Root stands here to spell it against.
            resolved(&path),
            Vec::new(),
            plan.root(),
        ),
        InitializationRejection::CommitFailed {
            failure,
            cleanup_failures,
        } => {
            let complete = cleanup_failures.is_empty();
            // No Package surrounds these: a commit that failed partway left no Root, so
            // the staged artifacts it names have nothing to be relative to.
            let mut paths = vec![resolved(failure.path())];
            paths.extend(
                cleanup_failures
                    .iter()
                    .map(|cleanup| resolved(cleanup.path())),
            );
            Failed {
                code: if complete {
                    FailureCode::PackageInitializationFailed
                } else {
                    FailureCode::PackageInitializationIncomplete
                },
                scope: FailureScope::Operation,
                message: commit_failure_message(&failure, &cleanup_failures),
                paths,
                identities: Vec::new(),
                package_root: Some(plan.root().to_path_buf()),
                diagnostics: Vec::new(),
            }
        }
    }
}

fn commit_failure_message(
    failure: &InitializationCommitFailure<io::Error>,
    cleanup_failures: &[InitializationCleanupFailure<io::Error>],
) -> String {
    let start = format!(
        "this initialization failed at {} while {}",
        failure.path().display(),
        failure.phase().as_str()
    );
    if cleanup_failures.is_empty() {
        return format!("{start}, and everything it had created was removed again.");
    }
    format!(
        "{start}, and {} of the artifacts it had created could not be removed again. \
         The paths this failure names after the first are what remains.",
        cleanup_failures.len()
    )
}

/// One refusal that happened after the selected root resolved.
///
/// The root is named in `context` whatever the outcome, because what the command was
/// *about* is the question `context` answers — see [`Context`] — and for `init` that is
/// the directory it was pointed at, which is not yet a Root and is the subject either
/// way.
fn refused(
    code: FailureCode,
    message: String,
    path: FailedPath,
    identities: Vec<Identity>,
    root: &Path,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths: vec![path],
        identities,
        package_root: Some(root.to_path_buf()),
        // Initialization assesses no Package: there is none there to observe until this
        // command makes one. Its refusals are typed failures rather than diagnostics.
        diagnostics: Vec::new(),
    }
}

/// One refusal that happened before, or during, the resolution of the selected root.
///
/// The path is the caller's own text or a path the core named on the way, and there is no
/// resolved root to report: `as_given` is what this adapter honestly has, because it
/// resolved nothing itself.
fn rootless(code: FailureCode, message: String, path: &Path) -> Failed {
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths: vec![as_given(path)],
        identities: Vec::new(),
        package_root: None,
        diagnostics: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The payload.
// ---------------------------------------------------------------------------

/// The `init` payload: what the mutation did, and the Package it leaves behind.
///
/// The two members the mutation contract calls for — the typed report, and the refreshed
/// affected result. The Root itself is in the envelope's `context`, so it is not repeated
/// here: one fact carried twice reads as two.
#[derive(Debug, Serialize)]
struct Initialized {
    report: Report,
    package: InitializedPackage,
}

/// The typed mutation report.
///
/// `committed` is whether the plan was carried out, and it is the one member `--dry-run`
/// changes. `changed` is whether the mutation changes the directory at all, which is a
/// fact about the plan and not about whether it ran — so a dry run reports what the real
/// run would, which is what "the same planned mutation report" means.
///
/// **Two members the mutation report calls for are absent here because this mutation has
/// nothing to put in them.** There are no old-and-new paths, because an initialization
/// moves nothing; and there is no separate index or derived-outcome member, because an
/// initialization *creates* the derived Package Index as one of its own operations rather
/// than refreshing an existing one — so it appears among them, as the canonical artifacts
/// do. A member with nothing to say would be a member every consumer had to learn to
/// ignore.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    operations: Vec<OperationProjection>,
    /// Present and empty. The core defines no initialization warning, so a committed
    /// initialization carries none; the member is here because a mutation report has it.
    warnings: Vec<&'static str>,
}

/// One planned managed operation, in the core's own vocabulary.
#[derive(Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    destination_kind: &'static str,
    location: LocationProjection,
}

impl OperationProjection {
    fn of(operation: &InitializationOperation) -> Result<Self, NotRepresentable> {
        Ok(Self {
            operation: operation.kind().as_str(),
            destination_kind: operation.destination_kind().as_str(),
            location: LocationProjection::of(operation.location())?,
        })
    }
}

/// The Package this initialization leaves behind, at the version this build writes.
///
/// The configured paths are reported as configured — the values `.spielorders/config.toml`
/// carries, or would — because that is what a caller supplied or left out. Where each one
/// lands is in the operations above.
#[derive(Debug, Serialize)]
struct InitializedPackage {
    metadata_version: u64,
    orders_path: String,
    removed_path: String,
}

fn initialized(
    plan: &PackageInitializationPlan,
    committed: bool,
) -> Result<Initialized, NotRepresentable> {
    Ok(Initialized {
        report: Report {
            committed,
            changed: creates_anything(plan),
            operations: plan
                .operations()
                .iter()
                .map(OperationProjection::of)
                .collect::<Result<_, _>>()?,
            warnings: Vec::new(),
        },
        package: InitializedPackage {
            metadata_version: plan.metadata_version(),
            orders_path: plan.orders_path().to_owned(),
            removed_path: plan.removed_path().to_owned(),
        },
    })
}

/// Whether this plan changes the directory at all.
///
/// Read off the plan rather than assumed, so the report's `changed` is the same answer
/// whether or not the plan ran.
fn creates_anything(plan: &PackageInitializationPlan) -> bool {
    plan.operations()
        .iter()
        .any(|operation| operation.kind() == InitializationOperationKind::Create)
}

// ---------------------------------------------------------------------------
// Presenting.
// ---------------------------------------------------------------------------

fn present(answer: Answer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(CONTRACT, package_context(root_of(&answer))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the plan resolved one.
fn root_of(answer: &Answer) -> Option<&Path> {
    match answer {
        Answer::Planned { plan, .. } => Some(plan.root()),
        Answer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn document(answer: &Answer) -> Result<(Response<Initialized>, Exit), NotRepresentable> {
    match answer {
        Answer::Planned { plan, committed } => Ok((
            Response::answered(
                CONTRACT,
                Context::rooted_at(plan.root())?,
                initialized(plan, *committed)?,
                // Initialization observes no Package, so there is nothing it saw to
                // report. Package health is `package show` and `validate`.
                Vec::new(),
            ),
            Exit::Success,
        )),
        Answer::Failed(failed) => Ok((
            failed_document(CONTRACT, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The human counterpart: what stands there now, and what this command put there.
///
/// Human output is never a parsing contract, so this may be reworded without advancing
/// the response contract. A dry run says what it *would* do in every line, because a
/// reader who skimmed one line of a plan and took it for a result is the whole hazard of
/// reporting a plan in the same shape as a result.
///
/// **The directories are the result and the managed files are detail**, which is the
/// decided split between what a mutation reports and what only `--verbose` shows. The
/// core's own `destination_kind` is what says which an operation is, so this adapter
/// classifies nothing: the places a user chose are named either way, and the managed
/// documents an initialization writes into them appear when asked for.
fn human(answer: &Answer, output: &Output) -> io::Result<Exit> {
    match answer {
        Answer::Planned { plan, committed } => {
            let would = |done: &str, planned: &str| {
                if *committed {
                    done.to_owned()
                } else {
                    planned.to_owned()
                }
            };
            output.summary(
                &would("Initialized", "Would initialize"),
                plan.root().as_os_str(),
            )?;
            output.summary(
                "Metadata version",
                &text(&plan.metadata_version().to_string()),
            )?;
            output.summary("Orders Directory", &text(plan.orders_path()))?;
            output.summary("Removed Orders Directory", &text(plan.removed_path()))?;
            for operation in plan.operations() {
                let label = match operation.kind() {
                    InitializationOperationKind::Create => would("Created", "Would create"),
                    InitializationOperationKind::Preserve => would("Preserved", "Would preserve"),
                };
                let path = operation.path().as_os_str();
                match operation.destination_kind() {
                    InitializationDestinationKind::Directory => output.summary(&label, path)?,
                    InitializationDestinationKind::File => output.detail(&label, path)?,
                }
            }
            Ok(Exit::Success)
        }
        Answer::Failed(failed) => {
            output.error(&failed.message)?;
            Ok(Exit::Failure)
        }
    }
}
