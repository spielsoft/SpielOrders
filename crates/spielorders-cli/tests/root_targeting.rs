//! `--root` names the subject's root exactly.
//!
//! The option defaults to `.`, names the SpielOrders Root itself, and never searches an
//! ancestor. Everything here is asserted from a real process, because "which directory
//! did this command decide to read" is only observable from outside it.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{Invocation, directory_that_is_not_a_root, initialized_package, response};

#[cfg(unix)]
#[test]
fn denied_package_reads_have_a_specific_machine_failure_across_callers() {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    struct RestoreMode(std::path::PathBuf, std::fs::Permissions);
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            std::fs::set_permissions(&self.0, self.1.clone()).unwrap();
        }
    }
    struct CopiedExecutable(Option<std::path::PathBuf>);
    impl Drop for CopiedExecutable {
        fn drop(&mut self) {
            if let Some(directory) = &self.0 {
                let _ = std::fs::remove_dir_all(directory);
            }
        }
    }
    let root_run = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .is_ok_and(|id| id.stdout.starts_with(b"0\n"));
    let mut copied = CopiedExecutable(None);
    let executable = if root_run {
        let directory = std::path::PathBuf::from("/tmp")
            .join(format!("spielorders-denied-child-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        copied.0 = Some(directory.clone());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        let executable = directory.join("spielorders");
        std::fs::copy(env!("CARGO_BIN_EXE_spielorders"), &executable).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        executable
    } else {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_spielorders"))
    };
    let (_tree, root) = initialized_package("package-access-denied");
    let root = if let Some(directory) = &copied.0 {
        let accessible = directory.join("Package");
        std::fs::rename(&root, &accessible).unwrap();
        accessible
    } else {
        root
    };
    let before = crate::support::managed_snapshot(&root);
    let metadata = root.join(".spielorders");
    let _restore = RestoreMode(
        metadata.clone(),
        std::fs::metadata(&metadata).unwrap().permissions(),
    );
    std::fs::set_permissions(&metadata, std::fs::Permissions::from_mode(0o0)).unwrap();
    let ask = |arguments: &[&str], json: bool| {
        let mut command = std::process::Command::new(&executable);
        command.args(arguments).arg("--root").arg(&root);
        if json {
            command.arg("--json");
        }
        if root_run {
            command.uid(65534);
        }
        command.output().unwrap()
    };
    for arguments in [
        vec!["package", "show"],
        vec!["package", "readme"],
        vec!["package", "list", "vendors"],
        vec!["search", "orders", "needle"],
        vec!["search", "supporting-material", "needle"],
        vec!["validate"],
        vec!["index", "status"],
        vec!["repair"],
    ] {
        let output = ask(&arguments, true);
        let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
        assert_eq!(document["ok"], false, "{arguments:?}");
        assert!(document["data"].is_null(), "{arguments:?}");
        assert_eq!(
            document["error"]["code"], "package_access_denied",
            "{arguments:?}"
        );
        assert!(
            document["error"]["detail"]["path"].is_string(),
            "{arguments:?}: the denial names the path it was denied: {document}"
        );
    }
    let human = ask(&["search", "supporting-material", "needle"], false);
    assert_eq!(human.status.code(), Some(1));
    assert!(human.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&human.stderr)
            .to_lowercase()
            .contains("denied")
    );
    drop(_restore);
    assert_eq!(crate::support::managed_snapshot(&root), before);

    let readme = root.join("README.md");
    std::fs::write(&readme, b"private Package notes").unwrap();
    let readme_mode = RestoreMode(
        readme.clone(),
        std::fs::metadata(&readme).unwrap().permissions(),
    );
    let before = crate::support::managed_snapshot(&root);
    std::fs::set_permissions(&readme, std::fs::Permissions::from_mode(0o0)).unwrap();
    let output = ask(&["validate"], true);
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "optional README denial stays nonfatal: {document}"
    );
    let warning = document["data"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["code"] == "readme_unavailable")
        .unwrap_or_else(|| panic!("the denial is a finding: {document}"));
    assert_eq!(warning["severity"], "warning");
    let human = ask(&["validate"], false);
    assert_eq!(human.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&human.stderr).contains("warning: readme unavailable"));
    drop(readme_mode);
    assert_eq!(crate::support::managed_snapshot(&root), before);
}

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
        outcome
            .stderr_text()
            .starts_with("error: unmanaged package"),
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
    assert_eq!(document["error"]["code"], "unmanaged_package");
    assert_eq!(
        document["error"]["detail"]["path"],
        inside.to_str().expect("the test path is UTF-8"),
        "the failure names the directory that was targeted, not its ancestor"
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
    assert_eq!(document["error"]["code"], "package_operation_failed");
    assert_eq!(
        document["error"]["detail"]["path"], "./no-such-directory",
        "the response names the path the caller wrote, which is all anything here knows"
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
