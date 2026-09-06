//! `--root` names the subject's root exactly.
//!
//! The option defaults to `.`, names the SpielOrders Root itself, and never searches an
//! ancestor. Everything here is asserted from a real process, because "which directory
//! did this command decide to read" is only observable from outside it.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{Invocation, directory_that_is_not_a_root, initialized_package, response};

/// The defect: a command that searched upward would find the Package above and answer
/// about it, so `--root` would name a starting point rather than the Root. A Package
/// sitting directly above the target is what makes the difference observable — without
/// it, "found nothing" and "searched nothing" look the same.
#[test]
fn an_ancestor_package_is_not_the_root_that_was_named() {
    let (_tree, root) = initialized_package("root-not-searched-upward");
    let inside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&inside)
        .run();

    assert_eq!(
        outcome.status,
        Some(1),
        "a directory that is not a SpielOrders Root is a domain failure, not a search"
    );
    assert_eq!(outcome.stdout, b"", "a failure writes nothing to stdout");
    assert!(
        outcome.stderr_text().contains("is not a SpielOrders Root"),
        "the error says the named directory is not a Root: {}",
        outcome.stderr_text()
    );
    assert!(
        !outcome.stderr_text().contains("README.md"),
        "no README was resolved, least of all the ancestor Package's: {}",
        outcome.stderr_text()
    );
}

/// The same fact in the contract, where a consumer can branch on it.
#[test]
fn a_directory_that_is_not_a_root_is_a_typed_failure() {
    let (_tree, root) = initialized_package("root-unmanaged-json");
    let inside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&inside)
        .argument("--json")
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["ok"], false);
    assert!(document["data"].is_null());
    assert_eq!(document["error"]["code"], "package_root_unmanaged");
    assert_eq!(document["error"]["scope"], "package");
    assert_eq!(
        document["error"]["paths"][0]["path"],
        inside.to_str().expect("the test path is UTF-8"),
        "the failure names the directory that was targeted, not its ancestor"
    );
    assert_eq!(
        document["error"]["paths"][0]["spelling"], "resolved",
        "the directory exists, so this application resolved it"
    );
}

/// A relative `--root` resolves the way every other relative path a process is handed
/// resolves: against the process working directory. The adapter joins nothing — the
/// filesystem does it, once, inside the resolution the core asks for.
#[test]
fn a_relative_root_resolves_against_the_working_directory() {
    let (_tree, root) = initialized_package("root-relative-resolves");
    let inside = root.join("nested");
    std::fs::create_dir(&inside).expect("a directory to stand in is created");

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(std::path::Path::new(".."))
        .in_directory(&inside)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", root.join("README.md").display()),
        "the answer is the canonical path, which is the core's own spelling of the Root"
    );
}

/// The defect this catches is a silent one: the executable used to join the process
/// working directory onto a relative `--root` before the core saw it, so a root the
/// filesystem cannot resolve never reached the core's own answer for it — and the core's
/// answer, when it was finally reached, aborted the process. Both halves are fixed, and
/// this asserts the surviving contract: exit `1`, and the path named in the document
/// rather than only in the human sentence.
#[test]
fn a_relative_root_that_does_not_resolve_is_named_in_the_response() {
    let (tree, _root) = initialized_package("root-relative-unresolvable");

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(std::path::Path::new("./no-such-directory"))
        .in_directory(tree.path())
        .argument("--json")
        .run();
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(1),
        "an unresolvable root is an operational failure, not a crash and not a success"
    );
    assert_eq!(document["ok"], false);
    assert_eq!(document["error"]["code"], "package_unreadable");
    assert_eq!(
        document["error"]["paths"][0]["path"], "./no-such-directory",
        "the response names the path the caller wrote, which is all anything here knows"
    );
    assert_eq!(
        document["error"]["paths"][0]["spelling"], "as_given",
        "nothing resolved it, and the contract says so rather than calling it absolute"
    );
    assert!(
        document["error"]["paths"][0]["package_relative_path"].is_null(),
        "an unresolved path is not known to be inside any Package"
    );
}

/// The human mode carries the same path. It is not a parsing contract, so this asserts
/// only that the two modes agree about the failure rather than the exact sentence: a
/// path the human sees and the document omits is the asymmetry the contract forbids.
#[test]
fn the_human_failure_names_no_path_the_document_omits() {
    let (tree, _root) = initialized_package("root-relative-unresolvable-human");

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(std::path::Path::new("./no-such-directory"))
        .in_directory(tree.path())
        .run();

    assert_eq!(outcome.status, Some(1));
    assert_eq!(outcome.stdout, b"");
    assert!(
        outcome.stderr_text().contains("./no-such-directory"),
        "the human sentence names the path: {}",
        outcome.stderr_text()
    );
}

/// `--order-root` does not exist. Targeting is one option, and an Order command will
/// mean the Order Directory by the same `--root` rather than adding a second spelling.
/// A decided absence needs a test, or it comes back as a convenience.
#[test]
fn there_is_no_second_targeting_option() {
    let (_tree, root) = initialized_package("no-order-root");

    let outcome = Invocation::new(&["package", "readme", "--order-root"])
        .argument(&root)
        .run();

    assert_eq!(outcome.status, Some(2), "the option is unrecognized");
    assert_eq!(outcome.stdout, b"");
}
