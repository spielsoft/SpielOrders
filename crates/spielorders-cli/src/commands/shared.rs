//! What more than one command family owes, decided once.
//!
//! **The load states a Package can be in are the same set for every command**, so the
//! mapping from one of them to a stable failure code, a scope, and an exit status lives
//! here rather than in each family. Every command family re-deriving it is how a
//! code comes to mean two things — and a Package-level command and an `order` command
//! disagreeing about an unmanaged directory is a defect that appears in neither
//! command's own diff.
//!
//! What is deliberately *not* here is the **classification**: which load state a
//! particular command treats as an answer and which as a failure is the command's own.
//! `package readme` answers a damaged Package because where its README would be is a fact
//! about topology; `package show` fails on one because the Package's contents are exactly
//! what it could not read.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries.

use crate::{
    commands::{
        order::OrderFailed,
        repair_suggestion::{RepairSuggestion, Rooting, command_for},
    },
    contract::ResponseContract,
    effects::PlatformFileSystem,
    exit::Exit,
    output::{Mode, Output},
    response::{
        Context, DiagnosticProjection, Failure, FailureCode, FailureScope, Identity,
        LocationProjection, NotRepresentable, Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, DamagedPackageProjection, DiagnosticGroup,
        HealthyOrder, ManagedContext, MetadataState, OrderPaths, PackageIndexRecord, PackageLoad,
        UnmanagedPackage, UpgradeProjection,
    },
    metadata::Vendor,
    operations::{FailedPath, OperationEvidence, OperationFailure},
    order_diagnosis::OrderDiagnosisError,
    ports::{FileSystem, FileSystemEntryKind},
    readme::{
        Diagnostic, DiagnosticEvidence, EvidenceScope, Location, OrderSemanticReason, ReadmeKind,
        ReadmeUnavailableReason, Severity,
    },
    repair_cases::{RepairAction, repair_actions},
};
use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

// ---------------------------------------------------------------------------
// One typed failure, however a command came to it.
// ---------------------------------------------------------------------------

/// One typed failure, however the command came to it.
///
/// Shared by every command because the load states a Package can be in are the same set
/// for all of them. Each field is a fact the response owes; the envelope's `context` is
/// built by the family that owns the command, because an `order` command has a second
/// root to name and a Package-level command has no notion of one.
pub(crate) struct Failed {
    pub code: FailureCode,
    pub scope: FailureScope,
    pub message: String,
    /// The paths the failure is about. A `FailedPath` rather than a `Location` so that a
    /// Package-relative spelling the core supplied survives into the response *and* a
    /// root the core could not resolve is still named — as the caller's own text, which
    /// is all anything here honestly knows about it.
    ///
    /// Ordinarily one. More than one where a failure is genuinely about more than one
    /// path: a mutation that failed partway and then could not remove everything it had
    /// created names what it left behind beside the failure that caused it. Empty only
    /// for a failure about an identity and no path at all.
    pub paths: Vec<FailedPath>,
    /// The identities the failure is about, for a failure about an entity the caller
    /// named rather than about a path. Empty for every failure about a path alone.
    pub identities: Vec<Identity>,
    /// The SpielOrders Root this failure happened under, where the load established one.
    ///
    /// A failure is not automatically rootless. Some of these happen *after* the Root
    /// resolved — the Package was a Root and could not be read past — and the context
    /// has a root to name then, which is the whole question `context` answers. `None`
    /// only where no Root was established, which means an unmanaged directory (a
    /// resolved path, but not a Root) or a load that exposes none.
    pub package_root: Option<PathBuf>,
    /// What the load observed about the Package on its way to failing.
    ///
    /// A failure carries diagnostics like any other response: a damaged Package's
    /// diagnostics *are* the answer to a read of its contents, which is why they travel
    /// in the envelope rather than only in `data` — `data` is `null` on every failure, so
    /// the envelope's `diagnostics` is the only channel a failed read has.
    pub diagnostics: Vec<Diagnostic>,
}

impl Failed {
    /// The typed `error` member this failure becomes.
    pub(crate) fn error(&self) -> Result<Failure, NotRepresentable> {
        let mut error = Failure::new(self.code, self.scope, &self.message);
        for path in &self.paths {
            error = error.at(path)?;
        }
        for identity in &self.identities {
            error = error.about(identity.clone());
        }
        Ok(error)
    }
}

/// A path exactly as the caller wrote it, which is all an adapter ever has of its own.
///
/// Deliberately not `FailedPath::of`: that constructor promotes an absolute path to a
/// `Location`, and this adapter resolved nothing, so claiming a resolved spelling for a
/// string that merely looks absolute would say the resolution happened when it did not.
pub(crate) fn as_given(path: &Path) -> FailedPath {
    FailedPath::Unlocated(path.to_path_buf())
}

/// One absolute path the core resolved, with no Package around it to be relative to.
///
/// **Not [`as_given`]**, which means the caller's own text: the core resolved these paths,
/// so quoting them back that way would say a resolution that happened did not. And **not
/// [`in_package`]**, which is what every path a Package *does* surround goes through: the
/// convention gives a path its Package-relative spelling whenever there is a Package to
/// spell it against, in a refusal exactly as in a success, so reaching for this where one
/// exists drops a fact the response holds.
///
/// So what is left here is the genuinely rootless case — a directory refused for not being
/// a Root, a candidate refused before eligibility, anything reached before a Root is
/// resolved — and **each call site says which**. The fallback to `as_given` is the honest
/// answer for a path that is not even absolute, which is what a failed resolution leaves.
pub(crate) fn resolved(path: &Path) -> FailedPath {
    Location::absolute(path.to_path_buf())
        .map(FailedPath::from)
        .unwrap_or_else(|_| as_given(path))
}

/// One path the core resolved inside a Package, in that Package's own spelling for it.
///
/// **Neither [`as_given`] nor [`resolved`]**: the capability that refused had already
/// established the Root, so the `Location` it hands back carries the Package-relative
/// spelling. A refusal owes that spelling exactly as a success does — the rule is in
/// [Decision traceability and contract-test conventions].
///
/// A `Location` rather than a `RootedLocation`, because a `RootedLocation` is one of these
/// beside the Root it was named under and only some capabilities have a reason to carry
/// the pair: an `order` command already holds both roots from the read that targeted its
/// Order. Either way the spelling is the core's, and the `package_root` half of the
/// response is taken from the same value the path came from at each call site.
///
/// [Decision traceability and contract-test conventions]: ../../../../docs/development/traceability.md
pub(crate) fn in_package(location: &Location) -> FailedPath {
    location.clone().into()
}

// ---------------------------------------------------------------------------
// What a `--root` may name.
// ---------------------------------------------------------------------------

/// What the `--root` of the command asking names.
///
/// Only the noun differs between the two, and it differs in one sentence, so the sentence
/// is written once and the noun travels to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RootKind {
    /// The SpielOrders Root, which is what `--root` names for `init`, for a Package-level
    /// command, and for a Vendor command.
    Package,
    /// The Order Directory, which is what `--root` names for an `order` command.
    Order,
}

impl RootKind {
    fn subject(self) -> &'static str {
        match self {
            Self::Package => "SpielOrders Root",
            Self::Order => "Order Directory",
        }
    }

    fn scope(self) -> FailureScope {
        match self {
            Self::Package => FailureScope::Package,
            Self::Order => FailureScope::Order,
        }
    }
}

/// The refusal a `--root` naming a link earns, or `None` where it names something else.
///
/// **Targeting, which is this adapter's own work.** Deciding what a `--root` may name is
/// not a Package rule — the core's own link refusals are about paths it *discovers*: an
/// entry of the Orders Directory, a configured managed path, an adoption candidate. This
/// one is about the path a command line handed in, and it is asked before the core is
/// invoked at all, so nothing behind the link is opened.
///
/// **Every command asks it, and asks it here.** Links are never followed and never
/// adopted; a command that decided for itself would be a second answer to one question,
/// and `tests/cross_command_conformance.rs` sweeps the whole surface for exactly that.
///
/// A path this adapter cannot observe at all is not refused here. Whether it exists, and
/// what stands there, is then the core's answer to give like any other.
///
/// Decision sources: Define the SpielOrders package topology; Define legacy Order
/// adoption and metadata bootstrapping.
pub(crate) fn link_named_as_a_root(selected_root: &Path, kind: RootKind) -> Option<Failed> {
    if PlatformFileSystem.symlink_metadata(selected_root).ok()?
        != Some(FileSystemEntryKind::SymbolicLink)
    {
        return None;
    }
    Some(Failed {
        code: FailureCode::RootIsALink,
        scope: kind.scope(),
        message: format!(
            "{} is a link. Links are never followed, so nothing behind this one is read, \
             renamed, disposed of, or edited. --root names the {} itself.",
            selected_root.display(),
            kind.subject()
        ),
        // The caller's own text: this refusal resolved nothing, which is the whole point.
        paths: vec![as_given(selected_root)],
        identities: Vec::new(),
        // Nothing was opened, so no Root was established — including the one the link
        // happens to point at, which is precisely the Package this command is not about.
        package_root: None,
        diagnostics: Vec::new(),
    })
}

/// A `--root` naming nothing, or naming something that is not an ordinary directory.
///
/// **One sentence for both routes to it.** The reads reach a wrong-kind Order root through
/// the record they would have opened under it, and the validating commands reach it by
/// observing the selected path itself; the caller wrote one path and gets one answer,
/// whichever command they ran. The caller's own text is the path, because nothing resolved
/// it — a path that is not a directory is one nothing can resolve past.
pub(crate) fn order_root_not_a_directory(directory: &Path) -> OrderFailed {
    OrderFailed::unresolved(
        FailureCode::OrderRootUnmanaged,
        format!(
            "{} is not an Order Directory: nothing stands there, or what does is not an \
             ordinary directory. For an `order` command --root names the Order Directory \
             itself.",
            directory.display()
        ),
        as_given(directory),
    )
}

/// The Package this command is about, or the typed reason there is none.
///
/// **The one classification of the five load states**, so a Package-level read, a Vendor
/// read, and a Vendor mutation cannot come to answer an unmanaged directory, a damaged
/// Package, or one at an uninterpretable metadata version differently. What each command
/// still decides for itself is whether it *asks* this question — `package readme` answers
/// a damaged Package, because where its README would be is a fact about topology.
///
/// The failure is boxed because it is the larger and rarer half: a `Failed` carries the
/// diagnosis of a damaged Package, and a `Result` as wide as its error would make every
/// successful load pay for it.
pub(crate) fn package_candidate(
    selected_root: &Path,
    load: CanonicalPackageLoad,
) -> Result<CurrentPackageCandidate, Box<Failed>> {
    match load {
        CanonicalPackageLoad::CurrentCandidate(package) => Ok(package),
        CanonicalPackageLoad::Unmanaged(package) => Err(Box::new(unmanaged_failure(&package))),
        CanonicalPackageLoad::DiagnosticOnly(package) => Err(Box::new(damaged_failure(
            &package,
            &as_given(selected_root),
        ))),
        CanonicalPackageLoad::UpgradePending(package) => {
            Err(Box::new(upgrade_pending_failure(&package)))
        }
        CanonicalPackageLoad::OperationFailed(package) => Err(Box::new(operation_failure(
            package.failure(),
            package.root(),
        ))),
    }
}

/// The same classification, for the load that also assessed the derived Package Index.
///
/// **One classification of these states, not two.** The assessing load answers with the
/// wider [`PackageLoad`], which is the shape every load that touches the index reports,
/// but the states it shares with the reading load are the same states and owe the same
/// codes. A second mapping written beside this one is exactly how two commands come to
/// answer an unmanaged directory differently.
///
/// The rebuild-failure state is unreachable from an assessment, which replaces nothing.
/// The arm is written rather than folded into a neighbour because the state is the core's
/// and a caller that reported it as damage would blame canonical records for a rebuild
/// that never happened.
pub(crate) fn assessed_package_candidate(
    selected_root: &Path,
    load: PackageLoad,
) -> Result<CurrentPackageCandidate, Box<Failed>> {
    match load {
        PackageLoad::Available(package) => Ok(package),
        PackageLoad::Unmanaged(package) => Err(Box::new(unmanaged_failure(&package))),
        PackageLoad::DiagnosticOnly(package) => Err(Box::new(damaged_failure(
            &package,
            &as_given(selected_root),
        ))),
        PackageLoad::UpgradePending(package) => Err(Box::new(upgrade_pending_failure(&package))),
        PackageLoad::OperationFailed(package) => Err(Box::new(operation_failure(
            package.failure(),
            package.root(),
        ))),
        PackageLoad::IndexReplacementFailed(failed) => Err(Box::new(operation_failure(
            failed.failure(),
            Some(failed.canonical_evidence().context().root()),
        ))),
    }
}

/// The exact path given is a resolved directory, and it is not a SpielOrders Root.
pub(crate) fn unmanaged_failure(package: &UnmanagedPackage) -> Failed {
    Failed {
        code: FailureCode::PackageRootUnmanaged,
        scope: FailureScope::Package,
        message: format!(
            "{} is not a SpielOrders Root: it has no .spielorders directory. \
             --root names the Root itself and no ancestor of it is consulted.",
            package.root().absolute_path().display()
        ),
        paths: vec![package.root().clone().into()],
        identities: Vec::new(),
        // A resolved directory, but not a SpielOrders Root. Naming it as
        // `package_root` would report a Package that is precisely what is absent.
        package_root: None,
        // Nothing managed stands there, so the load observed nothing about a Package.
        diagnostics: Vec::new(),
    }
}

/// One core operational failure, and the root the load had established when it happened.
///
/// The failure's own path is preferred because it is the one the operation failed on;
/// the load's root stands in only where the failure has no `Location` and the load does
/// — a nested operation that failed under a Root the load did resolve.
pub(crate) fn operation_failure(
    failure: &OperationFailure,
    load_root: Option<&Location>,
) -> Failed {
    let subject = failure.primary_path().as_path().display();
    let message = match failure.evidence() {
        OperationEvidence::FileSystem { operation } => format!(
            "{subject} could not be read: the {} operation failed",
            operation.as_str()
        ),
        OperationEvidence::PackageIndexReplacement { phase, .. } => format!(
            "the derived Package Index at {subject} could not be replaced: {}",
            phase.as_str()
        ),
    };
    let path = match (failure.primary_location(), load_root) {
        (None, Some(root)) => root.clone().into(),
        // Every other case reports the failure's own path, which is total: it is the
        // failure's `Location` where it has one, and otherwise the caller's own text —
        // which is exactly the root a resolution failure is about.
        _ => failure.primary_path().clone(),
    };
    Failed {
        code: FailureCode::PackageUnreadable,
        scope: FailureScope::of_evidence(failure.scope()),
        message,
        paths: vec![path],
        identities: Vec::new(),
        // The load resolved this Root and found it managed; what failed was reading
        // past it. `context` names the Root the command was about, so it names this.
        package_root: load_root.map(|root| root.absolute_path().to_path_buf()),
        // The load stopped at an operation rather than assessing the Package, so there
        // is nothing it observed about one.
        diagnostics: Vec::new(),
    }
}

/// A Package the load could read but not trust, and its diagnostics are the answer.
///
/// Two codes rather than one, taken from the core's own `MetadataState`: a Package whose
/// declared metadata version this build does not interpret is not the same problem as
/// one whose records are damaged, and the actions differ — one is `upgrade`, the other
/// `validate` and `repair`. The metadata state takes precedence because it is the more
/// specific answer: a Package at an uninterpretable version has diagnostics *because* of
/// its version, and reporting only "damaged" would send a user to the wrong command.
pub(crate) fn damaged_failure(
    package: &DamagedPackageProjection,
    loaded_root: &FailedPath,
) -> Failed {
    let root = package.context().map(ManagedContext::root);
    let subject = match root {
        Some(root) => root.absolute_path().display().to_string(),
        None => loaded_root.as_path().display().to_string(),
    };
    let observed = flattened(package.diagnostic_groups());
    let (code, message) = match package.metadata_state() {
        MetadataState::Unsupported => (
            FailureCode::PackageMetadataUnsupported,
            format!(
                "the Package at {subject} declares a metadata version this build does \
                 not interpret. Run `spielorders upgrade` if it is behind this build, \
                 and a newer build reads it if it is ahead."
            ),
        ),
        MetadataState::Current
        | MetadataState::UpgradeRequired
        | MetadataState::UpgradeResumable
        | MetadataState::UpgradeBlocked
        | MetadataState::NotAssessed => (
            FailureCode::PackageRootDamaged,
            format!(
                "the Package at {subject} is damaged and its contents cannot be \
                 reported. The diagnostics below are the diagnosis; {}",
                RepairSuggestion::for_findings(Rooting::Package, &observed)
            ),
        ),
    };
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths: vec![match root {
            Some(root) => root.clone().into(),
            None => loaded_root.clone(),
        }],
        identities: Vec::new(),
        // The load established this Root and then could not trust what stood under it,
        // so `context` has the Root the command was about. `None` only where the
        // projection exposes none.
        package_root: root.map(|root| root.absolute_path().to_path_buf()),
        // The whole answer. `data` is `null` on a failure, so the envelope's
        // `diagnostics` is where a damaged Package's diagnosis travels.
        diagnostics: observed,
    }
}

/// A Package mid-upgrade or awaiting one, which this build will not read as current.
///
/// **No load in the core constructs this variant today**, because the running build
/// knows exactly one durable metadata version, so its registered upgrade chain is empty.
/// The arm is written rather than folded into the damaged one because the state is a
/// decided one with its own command — a consumer branching on `package_upgrade_pending`
/// runs `spielorders upgrade`, and folding it in would send them to `repair`. A Package
/// whose declared version this build cannot interpret reaches
/// [`FailureCode::PackageMetadataUnsupported`] instead, which is the reachable half of
/// the same question.
pub(crate) fn upgrade_pending_failure(package: &UpgradeProjection) -> Failed {
    let root = package.context().root();
    Failed {
        code: FailureCode::PackageUpgradePending,
        scope: FailureScope::Package,
        message: format!(
            "the Package at {} is at metadata version {} and this build reads version \
             {}. Run `spielorders upgrade` to complete it.",
            root.absolute_path().display(),
            package.source_version(),
            package.target_version()
        ),
        paths: vec![root.clone().into()],
        identities: Vec::new(),
        package_root: Some(root.absolute_path().to_path_buf()),
        diagnostics: flattened(package.diagnostic_groups()),
    }
}

/// The failure half of a response document, shared so that a command cannot come to
/// spell the envelope's failure differently from its sibling.
///
/// The context arrives already built, because it is the one part of a failed response
/// that differs between families: a Package-level command names one root and an `order`
/// command names two.
pub(crate) fn failed_document<T, C>(
    contract: ResponseContract,
    context: C,
    failed: &Failed,
) -> Result<Response<T, C>, NotRepresentable> {
    Ok(Response::failed(
        contract,
        context,
        failed.error()?,
        diagnostics(&failed.diagnostics)?,
    ))
}

/// What every mode says when a path this response would carry has no JSON spelling.
///
/// One sentence in one place, because [`not_representable`] is the only thing that builds
/// the failure and a human arm that wrote its own would be a second answer to one question.
pub(crate) const UNSPELLABLE_PATH: &str = "a path this response would carry is not valid UTF-8, and a JSON response contract has \
     no spelling for it. The human output modes report the path instead.";

/// A path this response would carry has no JSON spelling.
///
/// **One code, one shape, one producer.** `path_not_representable` is built here and
/// nowhere else, so it cannot come to mean one thing where a presenter meets an
/// unspellable path and another where a command decides it has one. A command that meets
/// one says so in its own answer type and lets this render it.
///
/// **The context names every root the load established, not `null`.** A failure is not
/// automatically rootless, and this failure least of all: the unrepresentable path is
/// usually one *inside* a Package whose Root is perfectly representable — a managed
/// location, an Order Directory, or a diagnostic's subject. Reporting `null` would drop a
/// fact the response holds and tell a consumer the command was about no Package. Where
/// even the Root has no spelling there is genuinely none to name, and the context saying
/// so is how that case reports itself.
///
/// No path is named under `Failure::paths`, because the path this failure is about is
/// exactly the one with no spelling. The human output modes report it instead — a path
/// printed as the *result* of a command carries its own bytes, while one inside a
/// sentence is rendered as this platform displays it, because a message is a string.
pub(crate) fn not_representable<T, C>(contract: ResponseContract, context: C) -> Response<T, C> {
    Response::failed(
        contract,
        context,
        Failure::new(
            FailureCode::PathNotRepresentable,
            FailureScope::Operation,
            UNSPELLABLE_PATH,
        ),
        // A diagnostic would carry the same unrepresentable path.
        Vec::new(),
    )
}

/// The Package-level context for a failure, naming the Root wherever one was resolved.
pub(crate) fn failed_context(failed: &Failed) -> Result<Context, NotRepresentable> {
    match &failed.package_root {
        Some(root) => Context::rooted_at(root),
        None => Ok(Context::untargeted()),
    }
}

/// The context a Package-rooted response carries: the Root, wherever one was resolved.
///
/// `Context::rooted_at` can itself fail for a Root with no JSON spelling, and there is
/// genuinely no Root to name then — which is what `null` says. Shared by every
/// Package-rooted family, because "which Root was this about" is one question with one
/// answer however the command reached it.
pub(crate) fn package_context(package_root: Option<&Path>) -> Context {
    package_root
        .and_then(|root| Context::rooted_at(root).ok())
        .unwrap_or_else(Context::untargeted)
}

/// The result could not be delivered. There is nowhere left to report it but the other
/// stream, and the failure is real whether or not that write lands either.
pub(crate) fn undeliverable(output: &Output, error: io::Error) -> Exit {
    let _ = output.error(&format!("the result could not be written: {error}"));
    Exit::Failure
}

pub(crate) fn diagnostics(
    observed: &[Diagnostic],
) -> Result<Vec<DiagnosticProjection>, NotRepresentable> {
    observed.iter().map(DiagnosticProjection::of).collect()
}

/// Every diagnostic the load grouped, flattened into the one order the core sorted them
/// into.
///
/// The envelope's `diagnostics` is a flat list, and it is the only channel a failed read
/// has — so a Package whose grouping cannot be reported at all still reports every
/// observation. The grouping travels beside it in `data` where there is a `data`.
pub(crate) fn flattened(groups: &[DiagnosticGroup]) -> Vec<Diagnostic> {
    groups
        .iter()
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect()
}

/// One human sentence for one diagnostic, routed to the stream its severity earns.
///
/// An error-severity diagnostic is reported through the error path, so `--quiet` — which
/// says nothing on success and still reports errors — does not swallow the one thing
/// about a readable subject that a user needs to act on.
///
/// **Four facts, in one order, for every code**: the severity the stream carries, the
/// stable code a consumer would branch on, the explanation, and the next command to run
/// where this application has one to suggest. Human wording is not a parsing contract, so
/// this may be reworded freely — what it may not do is leave out one of the four.
///
/// `root` is the Root this finding was observed under, and it is a parameter because a
/// Package-scoped suggestion is a command line targeting a Package: the caller that
/// reported the finding is what knows which one. It is the same answer the response's
/// `context` gives, `None` and all — a failure that established no Root has none to
/// suggest against either.
pub(crate) fn report_diagnostic(
    output: &Output,
    root: Option<&Path>,
    diagnostic: &Diagnostic,
) -> io::Result<()> {
    let sentence = diagnostic_sentence(output.mode(), root, diagnostic);
    match diagnostic.severity() {
        Severity::Error => output.error(&sentence),
        Severity::Warning => output.warning(&sentence),
    }
}

/// The whole human line for one diagnostic: its code, what is wrong, the repair this
/// core offers for it, and what to run.
///
/// **The offered repair is named, not merely implied by the command.** A finding that
/// exits `0` — a leftover staging artifact beside a valid destination is the case — is
/// reported in full or it is hidden, and the exit status deliberately says nothing about
/// it. The actions are the core's own; applying one is `repair`'s, and the ephemeral case
/// handle a confirmation needs is `repair`'s own answer rather than a validation's.
fn diagnostic_sentence(mode: Mode, root: Option<&Path>, diagnostic: &Diagnostic) -> String {
    let code = diagnostic.code().as_str();
    let subject = diagnostic.primary_location().absolute_path().display();
    let explanation = diagnostic_explanation(diagnostic);
    let identity = match (mode, diagnostic_identity(diagnostic)) {
        (Mode::Verbose, Some(identity)) => format!(" {identity}"),
        _ => String::new(),
    };
    let offered = match repair_actions(diagnostic.code()) {
        [] => String::new(),
        actions => format!(
            " Offered: {}.",
            actions
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    match command_for(root, diagnostic) {
        Some(command) => {
            format!("{code} at {subject}: {explanation}{identity}{offered} Run `{command}`.")
        }
        None => format!("{code} at {subject}: {explanation}{identity}{offered}"),
    }
}

/// One argument of a suggested command line, spelled so a POSIX shell reads it as one word.
///
/// **A suggestion a caller cannot paste is not a suggestion**, and every variable part of
/// one is a name somebody chose: a Package Root, an Order Directory, and a case handle
/// derived from a Package-relative path. A Root with a space in it — `~/Google Drive/…` is
/// the ordinary case, not the exotic one — produced a string whose `--root` bound to the
/// first half and whose second half became a stray argument.
///
/// A value made only of characters a shell has no syntax for is returned exactly as it is,
/// so an ordinary suggestion stays the plain sentence a reader expects and nothing about
/// the usual case changes. Everything else is single-quoted, which suspends every
/// expansion a POSIX shell performs, with an embedded single quote written as the one
/// sequence that closes, escapes, and reopens.
pub(crate) fn shell_word(value: &str) -> Cow<'_, str> {
    const UNRESERVED: &str = "@%+=:,./-_";
    let bare = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || UNRESERVED.contains(character));
    if bare {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("'{}'", value.replace('\'', r"'\''")))
    }
}

/// The domain identity one finding is about, for the findings that have one.
///
/// **Held back to `--verbose`**, which is what the capability decision reserves domain
/// UUIDs for on the human surface. The same decision requires every diagnostic to carry an
/// explanation, and three explanations used to be the identity: they now say what is wrong
/// without it, so both sentences of that decision hold at once. Nothing is lost — the place
/// is named in every mode, and `--json` carries the identity in the diagnostic's own
/// evidence in every mode, which is where a consumer branches on one.
///
/// **Every variant is named rather than covered by a wildcard**, for the reason
/// [`diagnostic_explanation`] gives: a diagnostic added to the core is a compile error here
/// instead of an identity that quietly reaches every mode or none. The two that carry an
/// Order ID their sentence does not name answer `None` deliberately — an identity is added
/// here when the explanation would otherwise have been one.
fn diagnostic_identity(diagnostic: &Diagnostic) -> Option<String> {
    match diagnostic.evidence() {
        DiagnosticEvidence::VendorIdDuplicate { vendor_id } => {
            Some(format!("The Vendor ID is {}.", vendor_id.as_str()))
        }
        DiagnosticEvidence::VendorReferenceUnresolved { vendor_id, .. } => Some(format!(
            "The Vendor ID referenced is {}.",
            vendor_id.as_str()
        )),
        DiagnosticEvidence::OrderIdDuplicate { order_id, .. } => {
            Some(format!("The Order ID is {}.", order_id.as_str()))
        }
        DiagnosticEvidence::ManagedArtifact { .. }
        | DiagnosticEvidence::ConfigurationPathInvalid { .. }
        | DiagnosticEvidence::ConfigurationPathsOverlap { .. }
        | DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable { .. }
        | DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable { .. }
        | DiagnosticEvidence::OrdersDirectoryEntryDisallowed { .. }
        | DiagnosticEvidence::OrdersDirectoryEntryNameUnusable { .. }
        | DiagnosticEvidence::ReservedContentUnexpected { .. }
        | DiagnosticEvidence::StagingArtifactPresent { .. }
        | DiagnosticEvidence::VendorNameDuplicate { .. }
        | DiagnosticEvidence::OrderSemanticInvalid { .. }
        | DiagnosticEvidence::OrderPackageLocatorMismatch { .. }
        | DiagnosticEvidence::ReadmeUnavailable { .. } => None,
    }
}

/// What is wrong, in one sentence, from this diagnostic's own typed evidence.
///
/// **Every evidence variant is named rather than covered by a wildcard**, so a diagnostic
/// added to the core is a compile error here instead of a line that names a code and
/// explains nothing. The evidence's own members are what the sentence is built from: a
/// sentence that ignored them would be a restatement of the code.
fn diagnostic_explanation(diagnostic: &Diagnostic) -> String {
    match diagnostic.evidence() {
        DiagnosticEvidence::ManagedArtifact {
            artifact,
            field,
            observed_version,
            expected_version,
            ..
        } => {
            let artifact = artifact.as_str();
            match (field, observed_version, expected_version) {
                (_, Some(observed), Some(expected)) => format!(
                    "the {artifact} record declares metadata version {observed} and this \
                     build reads version {expected}."
                ),
                (Some(field), _, _) => format!(
                    "the {artifact} record could not be read as one; `{field}` is the first \
                     field at fault."
                ),
                _ => format!("the {artifact} record could not be read as one."),
            }
        }
        DiagnosticEvidence::ConfigurationPathInvalid {
            path,
            configured_value,
            reason,
        } => format!(
            "`{}` is configured as `{configured_value}`, which is not a managed path: {}.",
            path.as_str(),
            reason.as_str()
        ),
        DiagnosticEvidence::ConfigurationPathsOverlap {
            orders_path,
            removed_path,
        } => format!(
            "the configured `{orders_path}` and `{removed_path}` are one place, or one \
             holds the other."
        ),
        DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable {
            configured_value,
            reason,
        } => format!(
            "the configured Orders Directory `{configured_value}` cannot be used: {}.",
            reason.as_str()
        ),
        DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable { configured_value } => format!(
            "the configured Orders Directory `{configured_value}` resolves to a name this \
             application cannot express as a Package-relative path."
        ),
        DiagnosticEvidence::OrdersDirectoryEntryDisallowed { observed_kind } => format!(
            "a {} stands in the Orders Directory, which holds Order Directories; it is \
             ignored for Order discovery, so it is never one of this Package's Orders, \
             and nothing about it is moved, removed, or followed.",
            entry_kind(*observed_kind)
        ),
        DiagnosticEvidence::OrdersDirectoryEntryNameUnusable { observed_kind } => format!(
            "a {} in the Orders Directory has a name this application cannot express as a \
             Package-relative path, so it is never indexed and is left exactly as it is.",
            entry_kind(*observed_kind)
        ),
        DiagnosticEvidence::ReservedContentUnexpected { observed_kind } => format!(
            "a {} stands inside the reserved Package metadata directory; the required \
             records are intact and nothing here is deleted, moved, or adopted.",
            entry_kind(*observed_kind)
        ),
        DiagnosticEvidence::StagingArtifactPresent {
            destination_valid, ..
        } => match destination_valid {
            Some(true) => "an interrupted write left a staging artifact beside a valid \
                           destination. It is cleanup, not damage."
                .to_owned(),
            Some(false) => "an interrupted write left a staging artifact beside a \
                            destination that is missing or invalid."
                .to_owned(),
            None => "an interrupted write left a staging artifact beside a derived \
                     artifact, whose validity is not this finding's to judge."
                .to_owned(),
        },
        // The identity these three sentences are about is [`diagnostic_identity`]'s, which
        // the human surface holds back to `--verbose`.
        DiagnosticEvidence::VendorIdDuplicate { .. } => {
            "two Vendor records carry one Vendor ID, so the Vendor set cannot be trusted."
                .to_owned()
        }
        DiagnosticEvidence::VendorNameDuplicate { observed_names, .. } => format!(
            "more than one Vendor record carries the Name {}, under the trimmed, \
             case-insensitive comparison that makes Names unique.",
            quoted(observed_names)
        ),
        DiagnosticEvidence::VendorReferenceUnresolved { .. } => {
            "this Order references a Vendor, and this Package holds no such Vendor.".to_owned()
        }
        DiagnosticEvidence::OrderSemanticInvalid { field, reason, .. } => match reason {
            OrderSemanticReason::InvalidGregorianDate => {
                format!("`{field}` is not a day that exists.")
            }
            // No stored value is at fault: a non-finite Quantity or Unit Price never
            // loads, so the only route here is two finite numbers whose product or sum
            // overflows. See `order::CalculatedTotal`.
            OrderSemanticReason::NonFiniteTotal => format!(
                "the calculated {field} is not a finite number, because the stored \
                 Quantities and Unit Prices overflow when multiplied or added."
            ),
        },
        // The locator is quoted back where the record stores one to quote. Where it
        // stores none — an absent member, or one that is not text — the reason is the
        // whole of what there is to say about it.
        DiagnosticEvidence::OrderPackageLocatorMismatch {
            package_root: Some(package_root),
            reason,
            ..
        } => format!(
            "this Order's technical locator `{package_root}` does not lead from its \
             Directory to this Package: {}.",
            reason.as_str()
        ),
        DiagnosticEvidence::OrderPackageLocatorMismatch {
            package_root: None,
            reason,
            ..
        } => format!(
            "this Order's record carries no technical locator leading from its Directory \
             to this Package: {}.",
            reason.as_str()
        ),
        // Every directory carrying the Order ID is in `conflict_locations`, the subject's
        // own included, so the count is the list's length. The user picks the surviving
        // Order out of this sentence.
        DiagnosticEvidence::OrderIdDuplicate {
            conflict_locations, ..
        } => format!(
            "one Order ID is carried by {} Order Directories at once, so none of them is \
             identified.",
            conflict_locations.len()
        ),
        DiagnosticEvidence::ReadmeUnavailable {
            document_kind,
            reason,
        } => {
            let reason = match reason {
                ReadmeUnavailableReason::WrongKind => {
                    "something that is not a file stands at that path"
                }
                ReadmeUnavailableReason::Unreadable => "it could not be read",
                ReadmeUnavailableReason::InvalidEncoding => "it is not valid UTF-8",
            };
            format!(
                "the conventional {} exists and is not a readable document: {reason}. \
                 Editing it here is disabled and its Package or Order is undamaged.",
                match document_kind {
                    ReadmeKind::PackageReadme => "Package README",
                    ReadmeKind::OrderReadme => "Order README",
                }
            )
        }
    }
}

/// One observed filesystem entry kind, as a sentence names it.
fn entry_kind(kind: FileSystemEntryKind) -> &'static str {
    match kind {
        FileSystemEntryKind::File => "file",
        FileSystemEntryKind::Directory => "directory",
        FileSystemEntryKind::SymbolicLink => "link",
        FileSystemEntryKind::Other => "entry",
    }
}

/// A list of observed names, quoted, as one clause.
fn quoted(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Every diagnostic the load grouped, printed under the scope and the place that owns it.
///
/// **The grouping is part of the answer for a validating command**, so the heading is a
/// summary line on standard output while the findings keep the stream their severity
/// earns. Each finding still names its own absolute path, so a reader looking only at
/// standard error is never left without the place.
pub(crate) fn report_diagnostic_groups(
    output: &Output,
    root: &Path,
    groups: &[DiagnosticGroup],
) -> io::Result<()> {
    for group in groups {
        output.summary(
            match group.scope() {
                EvidenceScope::Package => "Package",
                EvidenceScope::Order => "Order",
                EvidenceScope::Document => "Document",
                EvidenceScope::Operation => "Operation",
            },
            located(group.location()),
        )?;
        for diagnostic in group.diagnostics() {
            report_diagnostic(output, Some(root), diagnostic)?;
        }
    }
    Ok(())
}

/// The failure half of a command's human output: the message, then what was observed.
///
/// Shared because every command owes the same two things in the same order, and a family
/// that wrote its own would be a second place deciding whether a failed read's
/// diagnostics reach a human at all.
pub(crate) fn human_failure(failed: &Failed, output: &Output) -> io::Result<Exit> {
    output.error(&failed.message)?;
    for diagnostic in &failed.diagnostics {
        // The Root the failure names, so a suggestion beside it targets the same Package
        // its `context` does — and offers none where the failure established no Root.
        report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
    }
    Ok(Exit::Failure)
}

/// One location, as a human line names it.
///
/// The Package-relative spelling where the location has one, and the absolute path where
/// it has none — an entry whose name this application cannot express is exactly the one a
/// reader most needs named, and an empty value names nothing.
pub(crate) fn located(location: &Location) -> &OsStr {
    match location.package_relative_path() {
        Some(relative) => OsStr::new(relative),
        None => location.absolute_path().as_os_str(),
    }
}

pub(crate) fn text(value: &str) -> OsString {
    OsString::from(value)
}

/// Every way an Order-scoped diagnosis can fail, mapped to one stable code.
///
/// **Shared by the two commands whose subject is one Order's own findings**, so a
/// validation and the repair that acts on it cannot come to answer a damaged Package, an
/// unmanaged directory, or a path that is no Order of this Package differently.
///
/// **A Package-level problem is reported as a Package problem.** The core refuses an
/// Order-scoped answer while the Package around it cannot be trusted, and it carries the
/// Package-scope findings with the refusal — so this reports them at Package scope rather
/// than attributing Package damage to the selected Order or answering that the Order looks
/// fine.
pub(crate) fn order_diagnosis_failure<E: std::fmt::Display>(
    directory: &Path,
    reason: OrderDiagnosisError<E>,
) -> OrderFailed {
    match reason {
        OrderDiagnosisError::UnmanagedPackage { directory } => OrderFailed {
            order_root: None,
            failed: Failed {
                code: FailureCode::PackageRootUnmanaged,
                scope: FailureScope::Package,
                message: format!(
                    "no SpielOrders Package holds {}: no directory above it carries a \
                     Package marker, so there is nothing to validate this Order against.",
                    directory.display()
                ),
                // No Package surrounds it: no ancestor of this directory carries a
                // Package marker, which is exactly what this refusal reports.
                paths: vec![resolved(&directory)],
                identities: Vec::new(),
                package_root: None,
                diagnostics: Vec::new(),
            },
        },
        OrderDiagnosisError::DamagedPackage { root, diagnostics } => OrderFailed {
            // The Order Directory is not named: the answer is about the Package, and
            // reporting the Order beside it would suggest the Order is what is wrong.
            order_root: None,
            failed: Failed {
                code: FailureCode::PackageRootDamaged,
                scope: FailureScope::Package,
                message: format!(
                    "the Package at {} is damaged, so this Order cannot be judged in \
                     isolation: an Order-scoped answer depends on Package-wide evidence. \
                     The diagnostics below are the Package's; `spielorders validate` \
                     reports the whole Package and {}",
                    root.absolute_path().display(),
                    // The Package's findings, not the selected Order's, and a Package-level
                    // answer that targets no one Order — so an Order command it names says
                    // where to root.
                    RepairSuggestion::for_findings(Rooting::Package, &diagnostics)
                ),
                paths: vec![root.clone().into()],
                identities: Vec::new(),
                package_root: Some(root.absolute_path().to_path_buf()),
                diagnostics,
            },
        },
        OrderDiagnosisError::PackageOperationFailed(failed) => OrderFailed {
            order_root: None,
            failed: operation_failure(failed.failure(), failed.root()),
        },
        OrderDiagnosisError::NotAnOrderDirectory { directory } => {
            order_root_not_a_directory(&directory)
        }
        OrderDiagnosisError::OrderNotFound { directory, root } => OrderFailed {
            // The selected directory is a resolved path and not an Order Directory, which
            // is precisely what this refusal reports.
            order_root: None,
            failed: Failed {
                code: FailureCode::OrderRootUnmanaged,
                scope: FailureScope::Order,
                message: format!(
                    "{} is not one of this Package's Orders: it is not an entry of the \
                     configured Orders Directory. For an `order` command --root names the \
                     Order Directory itself.",
                    directory.as_path().display()
                ),
                // The core's own spelling, which carries the Package-relative path for a
                // directory that is inside the Package and is not one of its Orders — and
                // the Root beside it, because that spelling is relative to it.
                paths: vec![directory],
                identities: Vec::new(),
                package_root: Some(root),
                diagnostics: Vec::new(),
            },
        },
        OrderDiagnosisError::FileSystem { path, error } => OrderFailed {
            order_root: None,
            failed: Failed {
                code: FailureCode::PackageUnreadable,
                scope: FailureScope::Operation,
                message: format!(
                    "{} could not be examined: {error}. Nothing about {} was judged.",
                    path.display(),
                    directory.display()
                ),
                // No Package surrounds it: every operation this diagnosis performs before
                // it fails is part of the walk that looks for the Root, so none of them has
                // one to be relative to.
                paths: vec![resolved(&path)],
                identities: Vec::new(),
                package_root: None,
                diagnostics: Vec::new(),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// The one vocabulary for what may be done about a finding.
// ---------------------------------------------------------------------------

/// One action a finding authorizes, in the core's own vocabulary.
///
/// **Shared by the validating commands and the repair commands**, because the ruling that
/// a validating command carries the offered repair "in `repair`'s action vocabulary" is
/// exactly a rule about one vocabulary: the spelling and the values each action requires
/// are the core's, asked once, so a finding cannot come to offer one thing in a validation
/// and another in the repair that acts on it.
#[derive(Debug, Serialize)]
pub(crate) struct OfferedActionProjection {
    action: &'static str,
    /// The values a caller must assign to apply this action. Present and possibly empty:
    /// an action with none is fully determined by the evidence already diagnosed.
    required_values: Vec<&'static str>,
    /// The exact command that applies this action, for a response that has an ephemeral
    /// case handle to name. **`null` for a validating command**, which reports what may be
    /// done and never mints a handle — the handle a confirmation needs is `repair`'s own
    /// answer, and a command line without one would apply nothing.
    command: Option<String>,
}

/// The offered actions for one finding, with whatever command the caller can name.
pub(crate) fn offered_actions(
    actions: &[RepairAction],
    command: impl Fn(RepairAction) -> Option<String>,
) -> Vec<OfferedActionProjection> {
    actions
        .iter()
        .map(|action| OfferedActionProjection {
            action: action.as_str(),
            required_values: action.required_values().to_vec(),
            command: command(*action),
        })
        .collect()
}

/// One repair this core offers for one diagnostic, as a validating command reports it.
///
/// **The offer identifies its diagnostic by code and place, never by position**, so a
/// consumer matches it to the finding in `diagnostics` by the facts the envelope already
/// carries and a reordering cannot silently re-point it.
#[derive(Debug, Serialize)]
pub(crate) struct OfferedRepairProjection {
    code: &'static str,
    scope: &'static str,
    location: LocationProjection,
    actions: Vec<OfferedActionProjection>,
}

/// Every offered repair among these findings, in their own order.
///
/// Present and possibly empty. **Whether a finding has one is the core's answer**, asked
/// of `repair_cases::repair_actions`: a surface deciding it again would be a second place
/// deciding what this application may do.
pub(crate) fn offered_repairs(
    observed: &[Diagnostic],
) -> Result<Vec<OfferedRepairProjection>, NotRepresentable> {
    observed
        .iter()
        .filter(|diagnostic| !repair_actions(diagnostic.code()).is_empty())
        .map(|diagnostic| {
            Ok(OfferedRepairProjection {
                code: diagnostic.code().as_str(),
                scope: diagnostic.scope().as_str(),
                location: LocationProjection::of(diagnostic.primary_location())?,
                actions: offered_actions(repair_actions(diagnostic.code()), |_| None),
            })
        })
        .collect()
}

/// One diagnostic group, as every response that reports the load's grouping projects it.
///
/// **Here rather than in the one command that reports it**, because the grouping is the
/// diagnostic vocabulary on the wire and a second definition would be a second shape for
/// one thing — which is how a `scope` comes to be spelled one way in one response and
/// another way in its neighbour. The complete Package read is the contract that carries the
/// grouping itself; the validating and repairing commands carry the same diagnostics
/// [`flattened`] into the envelope, so what they share with this is the vocabulary rather
/// than the shape.
#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticGroupProjection {
    scope: &'static str,
    /// The Package Root or Order Directory this group is about.
    location: LocationProjection,
    diagnostics: Vec<DiagnosticProjection>,
}

pub(crate) fn diagnostic_groups(
    groups: &[DiagnosticGroup],
) -> Result<Vec<DiagnosticGroupProjection>, NotRepresentable> {
    groups
        .iter()
        .map(|group| {
            Ok(DiagnosticGroupProjection {
                scope: group.scope().as_str(),
                location: LocationProjection::of(group.location())?,
                diagnostics: diagnostics(group.diagnostics())?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Projections more than one family reports.
// ---------------------------------------------------------------------------

/// One Vendor, in the least a client needs to render it: identity, Name, visibility.
///
/// A Vendor named in a listing, a Vendor named beside a Package's Order, and a Vendor
/// named beside the one Order an `order` command read are one thing, and three structs
/// with these three members would be three places for the spelling to drift. The
/// Vendor's shared facts and Contacts are the *complete* read, which is `package show`
/// and `vendor show`.
#[derive(Debug, Serialize)]
pub(crate) struct VendorSummary {
    vendor_id: String,
    name: String,
    hidden: bool,
}

impl VendorSummary {
    pub(crate) fn of(vendor: &Vendor) -> Self {
        Self {
            vendor_id: vendor.vendor_id.as_str().to_owned(),
            name: vendor.name.as_str().to_owned(),
            hidden: vendor.is_hidden(),
        }
    }
}

/// Whether a run given this `--all` presents one Vendor.
///
/// **The one home of the CLI's Vendor visibility default.** `--all` presents every Vendor
/// with its visibility and its absence presents the Vendors this Package offers for new
/// use. The core exposes the complete list and the offered-only one and filters neither,
/// so the default is this adapter's choice — and it is one choice for every command whose
/// answer is a list of Vendors, rather than three that happen to agree today. A predicate
/// rather than the core's offered list, because the entity search applies the default to
/// the Vendors a query retained rather than to the Package's whole list, and one rule
/// applied in two places has to be a rule that can be.
///
/// Resolution is untouched by it: an Order's Vendor comes from the core's
/// `vendor_for_order`, which searches the complete list, because hiding governs what is
/// offered and never what is resolved.
pub(crate) fn presented_vendor(vendor: &Vendor, all: bool) -> bool {
    all || !vendor.is_hidden()
}

/// One Order on one line, as a human reads it in a list, with its identity held back to
/// `--verbose`.
///
/// Shared for the reason [`OrderSummary`] is: an Order among the ones a Package holds and
/// an Order among the ones a query retained are one thing, and two lines carrying these
/// facts would be two places for the spelling to drift. The line names the calculated
/// Order Total, which no contract carrying [`OrderSummary`] does and which human output
/// being no parsing contract is what allows.
pub(crate) fn report_order(
    output: &Output,
    order: &HealthyOrder,
    resolved: Option<&Vendor>,
) -> io::Result<()> {
    let document = order.document();
    let vendor = match resolved {
        Some(vendor) => format!("  {}", vendor.name.as_str()),
        None => String::new(),
    };
    output.summary(
        "Order",
        &text(&format!(
            "{}  {}  {}{vendor}  {} item(s), total {}",
            document.date_ordered.as_str(),
            document.title.as_str(),
            document.status.as_str(),
            document.items.len(),
            // The core's unrounded total, spelled for a reader by the one thing that
            // spells money in this application. The number itself is untouched: a response
            // that carries a total carries it exactly as the core calculated it.
            crate::money::displayed_amount(order.order_total())
        )),
    )?;
    output.detail("Order ID", OsStr::new(document.order_id.as_str()))
}

/// One Vendor on one line, as a human reads it in a list, with its identity held back to
/// `--verbose`.
///
/// Shared for the reason [`report_order`] is. Visibility is on the line rather than in the
/// detail, because a reader given a list that omits Hidden Vendors by default most needs
/// to know which of the ones in front of them is hidden.
pub(crate) fn report_vendor(output: &Output, vendor: &Vendor) -> io::Result<()> {
    output.summary(
        "Vendor",
        &text(&if vendor.is_hidden() {
            format!("{} (hidden)", vendor.name.as_str())
        } else {
            vendor.name.as_str().to_owned()
        }),
    )?;
    output.detail("Vendor ID", OsStr::new(vendor.vendor_id.as_str()))
}

/// One Order, in the least a listing needs to name it.
///
/// The identity, the Order Directory root, the four facts an Order carries at every
/// status, and the Vendor the core resolved for it. The Items, the calculated totals, the
/// Order Currency, the Reusable Free-Entry Values, and the README text are the *complete*
/// read — `package show` and, for one Order, `order show` — and a contract that carried
/// them would be one of those under another name.
///
/// **The exclusion is about the contracts that carry this, not about the human mode.**
/// `package list orders` hands `present_contents` the same summary line `package show`
/// prints, which names the Item count and the calculated Order Total — sharing the line is
/// what keeps the two commands from describing one Order two ways, and human output is
/// never a parsing contract. A consumer that needs a total asks the command whose contract
/// carries one.
///
/// Shared for the reason [`VendorSummary`] is: an Order named in a listing and an Order
/// named beside the Supporting Material found inside its Directory are one thing, and two
/// structs with these members would be two places for the spelling to drift.
#[derive(Debug, Serialize)]
pub(crate) struct OrderSummary {
    /// The Order Directory root, which is what makes a listed Order actionable.
    ///
    /// Named directly rather than nested in a `paths` object holding one member: the
    /// Order's metadata directory, record, and README are locations the *complete* read
    /// reports, and a listing is about where the Order is.
    directory: LocationProjection,
    order_id: String,
    title: String,
    date_ordered: String,
    status: &'static str,
    /// The Vendor this Order names, as the record carries it. `null` when it names none.
    vendor_id: Option<String>,
    /// That Vendor as the core resolved it, Hidden Vendors included.
    ///
    /// **This command has no `--all`, and needs none.** Hiding governs what a Package
    /// offers for new use and never what is resolved, so resolution goes through the
    /// core's `vendor_for_order`, which searches the complete list. Resolving against the
    /// presented list is the defect the Vendor visibility default is most likely to
    /// cause, and a command with no such option would carry it silently.
    vendor: Option<VendorSummary>,
}

/// One Order summary, with the Vendor the core resolved for it.
///
/// The Vendor arrives as an argument for the same reason it does in `package show`: it
/// comes from `vendor_for_order`, which searches the complete Vendor list, and an adapter
/// that looked one up itself would be a second place resolving a Vendor.
pub(crate) fn order_summary(
    order: &HealthyOrder,
    resolved: Option<&Vendor>,
) -> Result<OrderSummary, NotRepresentable> {
    let document = order.document();
    Ok(OrderSummary {
        directory: LocationProjection::of(order.paths().directory())?,
        order_id: document.order_id.as_str().to_owned(),
        title: document.title.as_str().to_owned(),
        date_ordered: document.date_ordered.as_str().to_owned(),
        status: document.status.as_str(),
        vendor_id: document
            .vendor_id
            .as_ref()
            .map(|vendor_id| vendor_id.as_str().to_owned()),
        vendor: resolved.map(VendorSummary::of),
    })
}

/// The derived Package Index's state, with the typed reasons behind it.
///
/// Shared because two contracts report it and it is one fact: `package show` reports the
/// index of the Package it read, and a mutation reports the index its commit left behind.
/// Two shapes for that would be two answers to "what state is the derived index in".
///
/// `state` is `not_assessed` wherever the answer comes from a load that never opened the
/// stored index at all: every mutation that wrote nothing, which is one asked to plan only
/// and one whose plan changed nothing. A mutation that did replace a record answers with
/// the record its own refresh produced, or with the state it was left in where that
/// refresh failed.
///
/// **A read assesses.** The load that assesses *and* durably rebuilds is reserved by
/// `docs/architecture/durable-writes.md` for a place a canonical write has already
/// happened, but the core has a non-rebuilding assessment beside it, and that is what both
/// commands whose subject includes the index — the complete Package read and `index
/// status` — answer from. So asking what state the index is in is never the thing that
/// changes it, and the two cannot come to disagree.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct IndexProjection {
    location: LocationProjection,
    state: &'static str,
    refresh: &'static str,
    malformed_reasons: Vec<&'static str>,
    stale_reasons: Vec<&'static str>,
    observed_version: Option<u64>,
    expected_version: Option<u64>,
}

pub(crate) fn index(record: &PackageIndexRecord) -> Result<IndexProjection, NotRepresentable> {
    Ok(IndexProjection {
        location: LocationProjection::of(record.location())?,
        state: record.state().as_str(),
        refresh: record.refresh().as_str(),
        malformed_reasons: record
            .malformed_reasons()
            .iter()
            .map(|reason| reason.as_str())
            .collect(),
        stale_reasons: record
            .stale_reasons()
            .iter()
            .map(|reason| reason.as_str())
            .collect(),
        observed_version: record.observed_version(),
        expected_version: record.expected_version(),
    })
}

/// The Package's declared metadata version and what this build makes of it.
///
/// `state` is the core's own `MetadataState`, which is how a Package awaiting an upgrade
/// or declaring a version this build does not interpret says so. The per-file
/// `schema_version` state reaches a reader through this state, through each Order's own
/// `schema_version`, and through the index's observed and expected versions; the core
/// exposes no separate reader for the `vendors.json` version on a loaded Package.
///
/// Shared because two contracts report it and it is one fact: the complete Package read,
/// and the index rebuild that answers with the refreshed Package, version, and index
/// state.
#[derive(Debug, Serialize)]
pub(crate) struct MetadataProjection {
    metadata_version: u64,
    state: &'static str,
}

pub(crate) fn metadata(metadata_version: u64, state: MetadataState) -> MetadataProjection {
    MetadataProjection {
        metadata_version,
        state: state.as_str(),
    }
}

/// Every managed location one Order owns.
#[derive(Debug, Serialize)]
pub(crate) struct OrderPathsProjection {
    directory: LocationProjection,
    metadata_directory: LocationProjection,
    order: LocationProjection,
    readme: LocationProjection,
}

impl OrderPathsProjection {
    pub(crate) fn of(paths: &OrderPaths) -> Result<Self, NotRepresentable> {
        Ok(Self {
            directory: LocationProjection::of(paths.directory())?,
            metadata_directory: LocationProjection::of(paths.metadata())?,
            order: LocationProjection::of(paths.order())?,
            readme: LocationProjection::of(paths.readme().location())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Rust CLI capability contract.

    /// The ordinary suggestion is unchanged, and everything a shell reads as syntax is
    /// suspended. Asserted here because a process test cannot reach the quoting rules that
    /// no real path exercises — an embedded single quote above all.
    #[test]
    fn a_suggested_argument_survives_a_shell_unchanged() {
        for bare in [
            "/Users/x/Orders",
            "vendor_reference_unresolved:Orders/2026_08_22_Laser/.spielorders/order.json:f9ed",
            "assign_existing_vendor",
        ] {
            assert_eq!(shell_word(bare), bare, "nothing here has shell syntax");
        }
        assert_eq!(
            shell_word("/Users/x/Google Drive"),
            "'/Users/x/Google Drive'"
        );
        assert_eq!(
            shell_word(""),
            "''",
            "an empty argument is still an argument"
        );
        assert_eq!(shell_word("a'b"), r"'a'\''b'");
        assert_eq!(shell_word("$(rm -rf ~)"), "'$(rm -rf ~)'");
    }
}
