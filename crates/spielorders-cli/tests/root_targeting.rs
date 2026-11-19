//! `--root` names the subject's root exactly.
//!
//! The option defaults to `.`, names the SpielOrders Root itself, and never searches an
//! ancestor — a rule `cross_command_conformance.rs` sweeps over every command. Everything
//! here is asserted from a real process, because "which directory did this command decide
//! to read" is only observable from outside it.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{Invocation, initialized_package, response};

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

/// A relative `--root` the filesystem cannot resolve is an operational failure at exit `1`,
/// and both modes name the path the caller wrote: a path the human sees and the document
/// omits is the asymmetry the contract forbids. The human sentence is not a parsing
/// contract, so only that it names the path is asserted.
#[test]
fn a_relative_root_that_does_not_resolve_is_named_in_both_modes() {
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

    let human = Invocation::new(&["package", "readme"])
        .rooted_at(std::path::Path::new("./no-such-directory"))
        .in_directory(tree.path())
        .run();

    assert_eq!(human.status, Some(1));
    assert_eq!(human.stdout, b"");
    assert!(
        human.stderr_text().contains("./no-such-directory"),
        "the human sentence names the path: {}",
        human.stderr_text()
    );
}
