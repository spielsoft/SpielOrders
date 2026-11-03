//! `search supporting-material`: what a platform's own index answers about a Package, and
//! what a platform with no index says instead.
//!
//! **Every claim here is one both kinds of build owe** — one with a native index provider
//! for its platform and one without — so nothing in this file is compiled away where the
//! second is the build. Which of the two answers a run reaches is asked of the build rather
//! than assumed — see [`crate::support::supporting_material_provider`] — and
//! [`this_build_ships_the_provider_its_platform_has`] is what makes asking it safe. The
//! provider and the opener a build has for one platform are
//! `tests/platform_index_and_opener.rs`, which is compiled where that build is.
//!
//! The runs that ask a real provider ask a word nothing in the fixture Package carries, so
//! the answer is the same whatever an index holds.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{
    Invocation, directory_that_is_not_a_root, managed_snapshot, populated_package, response,
    supporting_material_provider,
};

/// A word no file in the fixture Package carries, in its name or its contents.
///
/// Every run below that reaches a real index asks this, so what the index holds cannot decide
/// whether the run answers: an index that has never looked at a temporary directory and one
/// that has indexed all of it both report nothing for this word.
const UNMATCHABLE: &str = "zirconium";

/// The Supporting Material index provider a build for the platform this run is on ships.
///
/// **The one place this suite states which provider a platform has.** Apple platforms have
/// Spotlight and no other platform has an index this build can ask. Every other claim in the
/// suite asks the build which provider it composed, so this is the fact they all rest on.
const PLATFORM_PROVIDER: Option<&str> = if cfg!(target_vendor = "apple") {
    Some("spotlight")
} else {
    None
};

/// This build ships the provider its platform has, and none where its platform has none.
///
/// **What makes every branch on the build's provider a branch on a checked fact.** A build
/// that shipped no provider where its platform has one is not hypothetical — it is what this
/// executable was before the provider was written, and every run then reported the state
/// that prevented an answer. Without this, such a build would quietly take the answer of a
/// platform it is not running on and the whole suite would still pass.
#[test]
fn this_build_ships_the_provider_its_platform_has() {
    assert_eq!(
        supporting_material_provider(),
        PLATFORM_PROVIDER,
        "the provider this build composed is not the one this platform has"
    );
}

/// A build with a provider answers; a build with none says so and stops.
///
/// **The two answers, each asserted where it is the truth.** The first is an answer that
/// found nothing, which is an answer: the exit status and `ok` say so, and the payload names
/// the provider that looked. The second is no answer at all — nothing looked, so `data` is
/// null and the failure names the platform state, which is the one thing a caller can act on
/// and the one thing that tells them to look at the platform rather than at their query.
///
/// The defect this catches on either platform is a build that answers the other's sentence: a
/// build with a provider that reported the state as though it had none — which is what this
/// executable did before the provider was written — and a build with none that reported an
/// empty answer, which would say the index found no matching file when nothing had looked.
#[test]
fn a_build_answers_where_it_has_a_provider_and_says_so_where_it_has_none() {
    let (_tree, root) = populated_package("supporting-material-answers");

    let outcome = Invocation::new(&["search", "supporting-material", UNMATCHABLE])
        .rooted_at(&root)
        .argument("--json")
        .run();
    let document = response(&outcome);

    assert_eq!(
        document["contract"], "spielorders.search.supporting_material",
        "{document}"
    );
    match supporting_material_provider() {
        Some(provider) => {
            assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
            assert_eq!(document["ok"], true, "{document}");
            assert_eq!(document["data"]["provider"], provider, "{document}");
            assert_eq!(document["data"]["query"], UNMATCHABLE);
            assert_eq!(document["data"]["complete"], true);
            assert_eq!(
                document["data"]["orders"],
                serde_json::json!([]),
                "the index holds no file matching this word, which is an answer"
            );
        }
        None => {
            assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
            assert_eq!(document["ok"], false, "{document}");
            assert!(
                document["data"].is_null(),
                "nothing looked, so there is no answer to carry: {document}"
            );
            assert_eq!(
                document["error"]["code"], "supporting_material_unsupported",
                "{document}"
            );
            assert_eq!(
                document["error"]["identities"],
                serde_json::json!([]),
                "there was no provider to name: {document}"
            );
        }
    }
}

/// An interrupted query exits by the platform's convention and leaves nothing behind.
///
/// **What is asserted is the behaviour, not the mechanism.** This executable installs no
/// signal handler — a choice, recorded in `src/exit.rs`, rather than a rule — so an
/// interrupt ends the process by the platform's own default disposition, which is what a
/// shell reports as `130`. What it leaves behind is nothing: a search writes nothing, so the
/// Package is byte-identical to the one the run was pointed at, and there is no answer on
/// standard output for a caller to read as a completed query.
///
/// The defect this catches is a handler that swallowed the signal: a process that answered
/// `0`, or exited by an ordinary status, would tell a shell the query completed.
#[test]
fn an_interrupted_query_exits_by_the_platforms_convention_and_leaves_nothing_behind() {
    use std::os::unix::process::ExitStatusExt;

    let (_tree, root) = populated_package("supporting-material-interrupted");
    let before = managed_snapshot(&root);

    let child = std::process::Command::new(env!("CARGO_BIN_EXE_spielorders"))
        .args(["search", "supporting-material", UNMATCHABLE, "--json"])
        .arg("--root")
        .arg(&root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the spielorders executable runs");
    // Interrupted as the query is being asked: the process is still starting up or waiting
    // on the platform's own query, and neither of those has a handler for this.
    let interrupted = unsafe { libc::kill(child.id().cast_signed(), libc::SIGINT) };
    assert_eq!(interrupted, 0, "the running query is sent an interrupt");
    let outcome = child
        .wait_with_output()
        .expect("the interrupted process is reaped");

    assert_eq!(
        outcome.status.signal(),
        Some(libc::SIGINT),
        "the interrupt ended the process by the platform's own convention: {outcome:?}"
    );
    assert_eq!(
        outcome.status.code(),
        None,
        "a process the platform ended carries no status of its own"
    );
    assert_eq!(
        outcome.stdout, b"",
        "an interrupted query has no answer to write"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a search writes nothing, interrupted or not"
    );
}

// ---------------------------------------------------------------------------
// The command line around the answer.
// ---------------------------------------------------------------------------

/// The human surface: an answer reaches a human on standard output, the state that prevented
/// one reaches standard error, and `--quiet` withholds the first while keeping the second.
///
/// `--quiet`'s rule is no output on success while errors are retained, and a platform with no
/// provider is where the second half of that rule is load-bearing: a caller who asked
/// quietly and got nothing on either stream would read silence as an answer. Nothing here
/// parses the sentence — human output is never a parsing contract — only which stream carried
/// something and which did not.
#[test]
fn an_answer_goes_to_standard_output_and_a_state_that_prevented_one_to_standard_error() {
    let (_tree, root) = populated_package("supporting-material-streams");
    let answers = supporting_material_provider().is_some();

    for mode in [None, Some("--quiet"), Some("--verbose")] {
        let mut run =
            Invocation::new(&["search", "supporting-material", UNMATCHABLE]).rooted_at(&root);
        if let Some(mode) = mode {
            run = run.argument(mode);
        }
        let outcome = run.run();

        assert_eq!(
            outcome.status,
            Some(if answers { 0 } else { 1 }),
            "{mode:?}: {}",
            outcome.stderr_text()
        );
        assert_eq!(
            outcome.stderr.is_empty(),
            answers,
            "{mode:?}: an answer is not an error, and a state that prevented one is"
        );
        assert_eq!(
            outcome.stdout.is_empty(),
            !answers || mode == Some("--quiet"),
            "{mode:?}: `--quiet` says nothing on success, and neither does a failure"
        );
    }
}

/// A `--root` that is not a SpielOrders Root is refused as it is for every Package read,
/// and no ancestor is consulted.
///
/// The command loads the Package it is about before it asks anything, so it answers the
/// load's states in the shared vocabulary rather than inventing its own — and it reaches the
/// same refusal whether or not this build has a provider, because nothing has been asked yet.
#[test]
fn a_root_that_is_not_a_package_is_refused_before_anything_is_asked() {
    let (_tree, root) = populated_package("supporting-material-unmanaged-root");
    let outside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["search", "supporting-material", UNMATCHABLE])
        .rooted_at(&outside)
        .argument("--json")
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_unmanaged");
    assert!(
        document["context"]["package_root"].is_null(),
        "a resolved directory that is not a Root is no Root to name"
    );
}

/// Every way this command line can be asked wrongly carries the shared usage identity.
///
/// **A usage error is not this command's cell**, so what is asserted per command line is
/// how it answers being asked wrongly. A query is required, because a search with no query
/// has named no subject, and `--dry-run` is a mutation's option this read does not have.
#[test]
fn every_usage_error_this_command_reaches_carries_the_shared_identity() {
    let (_tree, root) = populated_package("supporting-material-usage-errors");

    for wrong in [
        vec!["search", "supporting-material", "--json"],
        vec![
            "search",
            "supporting-material",
            UNMATCHABLE,
            "--json",
            "--quiet",
        ],
        vec![
            "search",
            "supporting-material",
            UNMATCHABLE,
            "--json",
            "--all",
        ],
        vec![
            "search",
            "supporting-material",
            UNMATCHABLE,
            "--json",
            "--dry-run",
        ],
        vec!["search", "supporting-materail", UNMATCHABLE, "--json"],
        vec!["search", "--json"],
    ] {
        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
    }
}
