//! `spielorders package readme` — the conventional Package README location.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{Invocation, initialized_package};

#[test]
fn the_conventional_absolute_path_is_printed_on_standard_output() {
    let (_tree, root) = initialized_package("readme-present");
    assert!(
        root.join("README.md").is_file(),
        "initialization wrote the conventional README, so this Package has one"
    );

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", root.join("README.md").display()),
        "the one line a human composes `cat` or `vi` with"
    );
    assert_eq!(
        outcome.stderr_text(),
        "",
        "a successful read says nothing on standard error"
    );
}

/// The path is conventional, so it is the answer whether or not the optional document
/// is presently there. A command that printed nothing for an absent README would make
/// `vi $(spielorders package readme)` — the way the document gets created — impossible.
#[test]
fn an_absent_document_does_not_change_the_conventional_path() {
    let (_tree, root) = initialized_package("readme-absent");
    std::fs::remove_file(root.join("README.md")).expect("the optional README is removed");

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "an absent README is not a failure");
    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", root.join("README.md").display())
    );
    assert_eq!(outcome.stderr_text(), "");
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
