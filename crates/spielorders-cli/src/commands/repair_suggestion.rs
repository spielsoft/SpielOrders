//! The one place this adapter names a repair command.
//!
//! **Naming a command promises that command would answer**, and whether it would is the
//! core's answer, asked of `repair_cases::repair_actions`. Composed at each site that
//! wanted one, that promise was made by sites which never asked: a Package whose only
//! damage this build salvages nothing for still told its reader to run `spielorders
//! repair`, and the sentence travels in `error.message` under `--json`, so it was a false
//! statement in a machine response rather than an awkward one on a terminal. The same
//! defect was then found again at each further site, because each finding named the
//! sentence in front of it and there was another behind it.
//!
//! So there is one. [`RepairSuggestion`] is the only value of such a sentence this crate
//! can hold — the field is private, and the constructor that promises a case asks the core
//! before it composes — and the two command names are spelled in [`RepairCommand`] and
//! nowhere else, so a site that wants to promise something has to come here for the words.
//!
//! What is decided here is the command line, because a command line is this adapter's own
//! vocabulary and the core has no notion of one. What is *not* decided here is which
//! conditions authorize an action: that is the repair boundary, and this module only reads
//! it.
//!
//! The command reference in [`crate::cli`] names the sibling command too, and deliberately
//! stays there: it describes where the scope boundary falls for a reader of `--help`, it is
//! composed before any Package is resolved, and there is nothing for it to ask.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries.

use crate::commands::shared::shell_word;
use spielorders_core::{
    readme::{Diagnostic, DiagnosticCode, EvidenceScope},
    repair_cases::repair_actions,
};
use std::{ffi::OsStr, fmt, path::Path};

/// Where the response carrying a suggestion is already rooted.
///
/// The only thing it decides is whether an Order command this sentence names still has to
/// say *which* Order Directory to run in. A Package-level response names Orders it does
/// not target, so it does; a response that is one Order's own is already rooted there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Rooting {
    /// A Package-level response, which targets no one Order.
    Package,
    /// One Order's own response, rooted at the Directory an Order command would take.
    Order,
}

/// One of this application's two repair commands.
///
/// Private, and the only place either name is spelled. `repair` diagnoses a Package's own
/// artifacts and `order repair` diagnoses one Order's, so which of them would answer for a
/// finding follows from the finding's scope — and a sentence that picked one for itself
/// would be a second place deciding that.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepairCommand {
    Package,
    Order,
}

impl RepairCommand {
    fn name(self) -> &'static str {
        match self {
            Self::Package => "spielorders repair",
            Self::Order => "spielorders order repair",
        }
    }

    /// That command, rooted at the place it acts in, spelled so a shell reads it as one
    /// command line.
    ///
    /// `--root` is named explicitly rather than left to its default, so the string is
    /// exact wherever it is run from — which is what makes it runnable verbatim.
    fn rooted_at(self, root: &Path) -> String {
        format!(
            "{} --root {}",
            self.name(),
            shell_word(&root.display().to_string())
        )
    }
}

/// One sentence naming a repair command, and the only spelling of one this crate holds.
///
/// **The field is private**, so the value cannot be built outside this module: a site
/// obtains one of these or it says nothing about repair. Every constructor that promises a
/// case consults `repair_actions` before naming the command that would answer it, which is
/// what keeps the promise true.
pub(crate) struct RepairSuggestion(&'static str);

impl fmt::Display for RepairSuggestion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl RepairSuggestion {
    /// What repair offers about a whole set of findings a response carries.
    ///
    /// Severity does not enter: a case follows from the evidence family, so the sentence
    /// says what repair would actually answer rather than what the verdict was.
    pub(crate) fn for_findings(rooting: Rooting, diagnostics: &[Diagnostic]) -> Self {
        let actionable = |scope: EvidenceScope| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic.scope() == scope && !repair_actions(diagnostic.code()).is_empty()
            })
        };
        Self::offered(
            rooting,
            actionable(EvidenceScope::Package),
            actionable(EvidenceScope::Order),
        )
    }

    /// The same, for one condition a refusal knows the code and the scope of.
    ///
    /// A refusal that carries no diagnostics still knows what it left behind — an
    /// interrupted write's staging artifact, an Order whose locator leads nowhere — and
    /// that condition is a diagnosed one with a code. Asking with the code is the same
    /// question [`Self::for_findings`] asks, so the day the repair boundary moves under one
    /// of these conditions the sentence moves with it.
    ///
    /// **The code and the scope have to be the ones the core would assign** to what the
    /// refusal is actually about. A pair invented to reach a command is the false
    /// suggestion again, with the consultation performed on evidence nobody has: a
    /// condition this application never diagnoses has no repair, and a refusal about one
    /// names no command at all rather than asking with a code that belongs to something
    /// else.
    pub(crate) fn for_condition(
        rooting: Rooting,
        code: DiagnosticCode,
        scope: EvidenceScope,
    ) -> Self {
        let actionable = !repair_actions(code).is_empty();
        Self::offered(
            rooting,
            actionable && scope == EvidenceScope::Package,
            actionable && scope == EvidenceScope::Order,
        )
    }

    /// Where an action one repair scope does not take is applied.
    ///
    /// **Nothing is consulted about a case, because this sentence promises none.** The
    /// caller named an action rather than a finding, and what it is owed is where that
    /// action lives: the two scopes divide this application's actions between them, so an
    /// action `repair` refuses is `order repair`'s. It is composed here for the reason
    /// every other sentence in this module is — the words are this module's.
    pub(crate) fn applied_by_the_order_command() -> Self {
        Self("`spielorders order repair` is where that action is applied.")
    }

    /// The sentence for what the two scopes were found to authorize.
    ///
    /// **The clause names a command only where that command would answer with a case.**
    /// `repair` diagnoses a Package's own artifacts and `order repair` diagnoses one
    /// Order's, so a Package whose only actionable finding is inside an Order is one
    /// `repair` answers no cases for — and a sentence saying otherwise sends a reader to a
    /// command with nothing to do.
    fn offered(rooting: Rooting, package: bool, order: bool) -> Self {
        Self(match (package, order) {
            (true, true) => match rooting {
                Rooting::Package => {
                    "`spielorders repair` offers the Package's actions and `spielorders \
                     order repair` offers each Order's."
                }
                Rooting::Order => {
                    "`spielorders repair` offers the Package's actions and `spielorders \
                     order repair` offers this Order's."
                }
            },
            (true, false) => "`spielorders repair` offers the actions.",
            (false, true) => match rooting {
                Rooting::Package => {
                    "`spielorders order repair`, rooted at the Order Directory a finding \
                     names, offers the actions."
                }
                Rooting::Order => "`spielorders order repair` offers the actions.",
            },
            // Nothing here guesses a fact, salvages malformed JSON, interprets an
            // unsupported version, or adopts missing metadata, so there are findings with
            // no safe action at all — and naming a command for them would be the false
            // suggestion this module exists to avoid. What is left is what a reader can
            // do, which is the rest of the sentence.
            (false, false) => {
                "this build offers no repair for them, and correcting them is work outside \
                 this application."
            }
        })
    }
}

/// The command line to run about one diagnostic, or `None` where this application has none.
///
/// **Whether a safe action exists is the core's answer**, asked of `repair_actions`: a
/// surface deciding it again would be a second place deciding what the core may do, and
/// would go silently wrong the moment the repair boundary moved.
///
/// **Every suggestion names its target**, because a reader runs it from wherever they are
/// standing and `--root` defaults to `.`. A Package-scoped one with no Root left to name is
/// no suggestion at all rather than a bare command that would diagnose whatever directory
/// the shell happens to be in.
///
/// A finding at Document scope never gets one: an unreadable README is corrected outside
/// this application, and repair never replaces or converts a user-authored document.
pub(crate) fn command_for(root: Option<&Path>, diagnostic: &Diagnostic) -> Option<String> {
    if repair_actions(diagnostic.code()).is_empty() {
        return None;
    }
    match diagnostic.scope() {
        EvidenceScope::Package => Some(RepairCommand::Package.rooted_at(root?)),
        // Rooted at the Order Directory the finding is about, because that is what an
        // `order` command's `--root` names. The group's own location would be the narrower
        // path — the record or the staged file — so the Directory is taken from the
        // diagnostic's primary location here; the finding is about a path under one Order,
        // and naming that path is what a reader needs.
        EvidenceScope::Order => {
            Some(RepairCommand::Order.rooted_at(order_directory_of(diagnostic)))
        }
        EvidenceScope::Document | EvidenceScope::Operation => None,
    }
}

/// The command line that applies one of a Package's own diagnosed cases, at its Root.
///
/// Not a suggestion and not consulted: the caller is `repair` itself, holding the case this
/// line applies. What it takes from this module is the spelling.
pub(crate) fn package_repair_at(root: &Path) -> String {
    RepairCommand::Package.rooted_at(root)
}

/// The same, for one of an Order's diagnosed cases, at its Directory.
pub(crate) fn order_repair_at(directory: &Path) -> String {
    RepairCommand::Order.rooted_at(directory)
}

/// The Order Directory one Order-scoped finding is about.
///
/// Every Order-scoped diagnostic's primary location is the Order's own `order.json` or a
/// staged file beside it inside `.spielorders`, so the Directory is the ancestor above that
/// reserved subdirectory. The path itself is the fallback where it is neither, which is a
/// suggestion a reader can still act on rather than a sentence with a hole in it.
fn order_directory_of(diagnostic: &Diagnostic) -> &Path {
    let path = diagnostic.primary_location().absolute_path();
    path.ancestors()
        .find(|candidate| candidate.file_name() == Some(OsStr::new(".spielorders")))
        .and_then(Path::parent)
        .unwrap_or(path)
}
