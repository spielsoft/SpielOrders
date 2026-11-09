//! `search supporting-material`: what a platform's own index answers about a Package, and
//! what a platform with no index says instead.
//!
//! Ordinary checks run with and without a native index provider. Provider presence and the
//! runtime index state are separate facts — see
//! [`crate::support::supporting_material_provider`]. The CLI's own Spotlight query is held
//! to its rules in `src/platform.rs`, and each runtime index state's answer in
//! `src/commands.rs`, from supplied provider facts.
//!
//! No run here asks the machine's own index: the executable runs over Packages with no
//! Order, where there is nothing to ask. The real index is watched in
//! `scripts/smoke-spotlight.sh`.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{
    Invocation, directory_that_is_not_a_root, initialized_package, material_answered,
    populated_package, response, supporting_material_provider,
};

/// A word no file in the fixture Package carries, in its name or its contents.
///
/// Every run below that reaches a real index asks this, so what the index holds cannot decide
/// which files can be reported. Runtime availability still decides whether it can answer.
const UNMATCHABLE: &str = "zirconium";

/// A Package with no Order has nowhere for Supporting Material to be, so a build with a
/// provider answers that it found nothing without asking, and a build without one says so.
#[test]
fn a_package_with_no_order_answers_without_asking_an_index() {
    let (_tree, root) = initialized_package("supporting-material-nowhere");
    let outcome = Invocation::new(&["search", "supporting-material", UNMATCHABLE])
        .rooted_at(&root)
        .argument("--json")
        .run();
    let document = response(&outcome);
    let provider = supporting_material_provider();
    assert_eq!(
        material_answered(&outcome, &document, provider),
        provider.is_some()
    );
    if provider.is_some() {
        assert_eq!(document["data"]["files"], serde_json::json!([]));
        assert_eq!(document["data"]["state"], "complete");
    }
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
    let (_tree, root) = initialized_package("supporting-material-streams");
    for mode in [None, Some("--quiet"), Some("--verbose")] {
        let mut run =
            Invocation::new(&["search", "supporting-material", UNMATCHABLE]).rooted_at(&root);
        if let Some(mode) = mode {
            run = run.argument(mode);
        }
        let outcome = run.run();
        let answers = supporting_material_provider().is_some();
        assert_eq!(
            outcome.status,
            Some(if answers { 0 } else { 1 }),
            "{mode:?}"
        );
        if !answers || mode == Some("--quiet") {
            assert_eq!(outcome.stderr.is_empty(), answers, "{mode:?}");
        }
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
    assert_eq!(document["error"]["code"], "unmanaged_package");
    assert_eq!(
        document["error"]["detail"]["path"],
        outside.to_str().expect("the test path is UTF-8"),
        "the refusal is about the directory named, not the Package above it"
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
