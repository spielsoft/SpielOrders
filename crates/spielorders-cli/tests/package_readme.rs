//! `spielorders package readme` — the conventional Package README location.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{Invocation, initialized_package};

/// The path is conventional, so it is the answer whether or not the optional document
/// is presently there: the one line a human composes `cat` or `vi` with. A command that
/// printed nothing for an absent README would make `vi $(spielorders package readme)` —
/// the way the document gets created — impossible.
#[test]
fn the_conventional_path_is_printed_whether_or_not_the_document_exists() {
    for present in [true, false] {
        let (_tree, root) = initialized_package("readme-path");
        if !present {
            std::fs::remove_file(root.join("README.md")).expect("the optional README is removed");
        }

        let outcome = Invocation::new(&["package", "readme"])
            .rooted_at(&root)
            .run();

        assert_eq!(outcome.status, Some(0), "present: {present}");
        assert_eq!(
            outcome.stdout_text(),
            format!("{}\n", root.join("README.md").display()),
            "present: {present}"
        );
        assert_eq!(
            outcome.stderr_text(),
            "",
            "present: {present}: a successful read says nothing on standard error"
        );
    }
}

/// `--root` defaults to `.`, so a user standing in a Package needs no option at all.
#[test]
fn the_target_defaults_to_the_working_directory() {
    let (_tree, root) = initialized_package("readme-default-root");

    let outcome = Invocation::new(&["package", "readme"])
        .in_directory(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", root.join("README.md").display())
    );
}
