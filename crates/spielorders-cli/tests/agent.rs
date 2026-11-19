//! `spielorders agent provision` and `agent defaults`: installed agent guidance.
//!
//! Decision sources: Define the SpielOrders agent skill contract; Define the Rust CLI
//! capability contract.

use crate::support::{
    Invocation, Outcome, initialized_package, managed_snapshot, material_answered, response,
    supporting_material_provider,
};
use std::{fs, process::Command};

#[test]
fn agent_provision_responses_match_their_contracts() {
    use crate::support::{ResponseContract, assert_matches_golden, unmanaged_directory};
    for (cell, status) in [("success", 0), ("invalid", 1), ("rejected", 1)] {
        // Any directory is a destination; no Package needs to stand there.
        let (_tree, mut root) = unmanaged_directory("provision-response");
        if cell == "invalid" {
            root = root.join("notes.txt");
            fs::write(&root, "not a directory").unwrap();
        }
        if cell == "rejected" {
            fs::write(root.join(".agents"), "owner content").unwrap();
        }
        let outcome = Invocation::new(&["agent", "provision", "--json"])
            .rooted_at(&root)
            .run();
        assert_eq!(
            outcome.status,
            Some(status),
            "{cell}: {}",
            outcome.stdout_text()
        );
        assert_matches_golden(ResponseContract::AgentProvision, cell, &outcome, &root);
    }
}

#[test]
fn agent_defaults_queries_an_installed_resource_tree_without_a_package() {
    use crate::support::{ResponseContract, assert_golden, unmanaged_directory};
    let (_tree, installation) = unmanaged_directory("agent-defaults-installation");
    let executable = installation.join("bin/spielorders");
    let resources = installation.join("share/spielorders/agent-guidance");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_spielorders"), &executable).unwrap();
    fs::create_dir_all(resources.join("references")).unwrap();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../spielorders/agent-guidance");
    fs::copy(source.join("SKILL.md"), resources.join("SKILL.md")).unwrap();
    for entry in fs::read_dir(source.join("references")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(
            entry.path(),
            resources.join("references").join(entry.file_name()),
        )
        .unwrap();
    }
    let output = Command::new(&executable)
        .args(["agent", "defaults", "--json"])
        .current_dir(&installation)
        .output()
        .unwrap();
    let outcome = crate::support::Outcome {
        status: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    };
    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    let document = response(&outcome);
    assert!(document["context"]["root"].is_null());
    assert_eq!(
        std::path::Path::new(document["data"]["skill_directory"].as_str().unwrap()),
        resources
    );
    assert_golden(
        ResponseContract::AgentDefaults,
        "success",
        outcome
            .stdout_text()
            .replace(resources.to_str().unwrap(), "<RESOURCE_DIRECTORY>"),
    );

    // A command registered on PATH by link still reports the installed prefix's guidance.
    let (_link_tree, elsewhere) = unmanaged_directory("agent-defaults-path-link");
    let link = elsewhere.join("spielorders");
    std::os::unix::fs::symlink(&executable, &link).unwrap();
    let linked = Command::new(&link)
        .args(["agent", "defaults", "--json"])
        .current_dir(&elsewhere)
        .output()
        .unwrap();
    assert_eq!(linked.status.code(), Some(0));
    let linked: serde_json::Value = serde_json::from_slice(&linked.stdout).unwrap();
    assert_eq!(
        std::path::Path::new(linked["data"]["skill_directory"].as_str().unwrap()),
        resources
    );

    let (_package_tree, package) = initialized_package("defaults-canonical-content");
    assert_eq!(
        Invocation::new(&["agent", "provision"])
            .rooted_at(&package)
            .run()
            .status,
        Some(0)
    );
    let guidance = package.join(".agents/skills/spielorders");
    assert_eq!(managed_snapshot(&resources), managed_snapshot(&guidance));
    for option in ["--root", "--dry-run"] {
        let mut command = Command::new(&executable);
        command.args(["agent", "defaults", "--json", option]);
        if option == "--root" {
            command.arg(&package);
        }
        let rejected = command.output().unwrap();
        assert_eq!(rejected.status.code(), Some(2));
    }
    let quiet = Command::new(&executable)
        .args(["agent", "defaults", "--quiet"])
        .output()
        .unwrap();
    assert_eq!(quiet.status.code(), Some(0));
    assert!(quiet.stdout.is_empty() && quiet.stderr.is_empty());
    fs::rename(&resources, resources.with_extension("unavailable")).unwrap();
    let output = Command::new(&executable)
        .args(["agent", "defaults", "--json"])
        .output()
        .unwrap();
    let outcome = crate::support::Outcome {
        status: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    };
    assert_eq!(outcome.status, Some(1));
    assert_golden(
        ResponseContract::AgentDefaults,
        "rejected",
        outcome
            .stdout_text()
            .replace(resources.to_str().unwrap(), "<RESOURCE_DIRECTORY>"),
    );
}

#[test]
fn agent_provision_dry_runs_refreshes_guidance_and_retries_after_failed_staging() {
    let (_tree, root) = initialized_package("agent-provision");
    let skill = root.join(".agents/skills/spielorders");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), "owner skill\n").unwrap();
    fs::write(root.join("AGENTS.md"), "owner instructions\n").unwrap();
    let outside_guidance = || {
        managed_snapshot(&root)
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".agents"))
            .collect::<Vec<_>>()
    };
    let managed = outside_guidance();
    let dry = Invocation::new(&["agent", "provision", "--dry-run", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(dry.status, Some(0), "{}", dry.stdout_text());
    assert!(!skill.join("references/orders-and-items.md").exists());
    assert_eq!(
        fs::read_to_string(skill.join("SKILL.md")).unwrap(),
        "owner skill\n",
        "a dry run replaces nothing"
    );

    // Exclusive write succeeds but its write-only staging file cannot be read back.
    // Failure after creation must remove that owned file so the next run can retry.
    let failed = Command::new("/bin/sh")
        .args([
            "-c",
            "umask 0444; exec \"$@\"",
            "sh",
            env!("CARGO_BIN_EXE_spielorders"),
            "agent",
            "provision",
            "--json",
            "--root",
        ])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(
        failed.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&failed.stdout)
    );
    assert!(
        fs::read_dir(skill.join("references"))
            .unwrap()
            .next()
            .is_none(),
        "no staged or published files remain"
    );
    let answer = Invocation::new(&["agent", "provision", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(answer.status, Some(0), "{}", answer.stdout_text());
    assert_eq!(response(&answer)["contract"], "spielorders.agent.provision");
    assert!(skill.join("references/orders-and-items.md").is_file());
    let shipped =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../spielorders/agent-guidance");
    assert_eq!(
        fs::read(skill.join("SKILL.md")).unwrap(),
        fs::read(shipped.join("SKILL.md")).unwrap(),
        "a requested refresh replaces the earlier copy"
    );
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "owner instructions\n"
    );
    assert_eq!(outside_guidance(), managed);
    let repeated = Invocation::new(&["agent", "provision", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(response(&repeated)["data"]["outcome"], "unchanged");
}

#[test]
fn agent_provision_neither_requires_nor_reads_package_metadata() {
    let (_tree, root) = initialized_package("agent-provision-damaged");
    fs::write(root.join(".spielorders/config.toml"), "orders_path = [").unwrap();
    let metadata = managed_snapshot(&root.join(".spielorders"));
    let answer = Invocation::new(&["agent", "provision", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(answer.status, Some(0), "{}", answer.stdout_text());
    assert_eq!(response(&answer)["data"]["outcome"], "committed");
    assert!(root.join(".agents/skills/spielorders/SKILL.md").is_file());
    assert_eq!(managed_snapshot(&root.join(".spielorders")), metadata);
}

/// Every shipped example runs on a disposable Package holding what its reference
/// assumes. Supporting Material search reports the native provider's actual state;
/// unlike a record command, it cannot promise success on every machine.
#[test]
fn guidance_examples_run_against_disposable_packages() {
    let references = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../spielorders/agent-guidance/references");
    let mut files: Vec<_> = fs::read_dir(&references)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    for file in files {
        let (_tree, package) = initialized_package("guidance-examples");
        let setup = |arguments: &[&str]| {
            let outcome = Invocation::new(arguments).rooted_at(&package).run();
            assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
        };
        setup(&["add-vendor", "Known Vendor"]);
        setup(&["add-order", "title=Existing order"]);
        let order = fs::read_dir(package.join("Orders"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let item = Invocation::new(&[
            "order",
            "item",
            "add",
            "description=Existing",
            "quantity=1",
            "unit_price=1",
        ])
        .rooted_at(&order)
        .run();
        assert_eq!(item.status, Some(0), "{}", item.stdout_text());
        let directory = if file.ends_with("packages-and-adoption.md") {
            let candidate = package.join("Orders/Legacy quote");
            fs::create_dir(&candidate).unwrap();
            candidate
        } else {
            order
        };
        let text = fs::read_to_string(&file).unwrap();
        let mut in_block = false;
        let mut ran = 0;
        for line in text.lines() {
            if line.starts_with("```") {
                in_block = !in_block;
                continue;
            }
            if !in_block {
                continue;
            }
            let material = line.starts_with("spielorders search supporting-material ");
            // Read this provider-dependent answer in its machine shape so a documented
            // refusal is checked precisely, not accepted as an arbitrary failed example.
            let command = if material {
                format!("{line} --json")
            } else {
                line.to_owned()
            };
            let output = Command::new("/bin/sh")
                .args([
                    "-c",
                    &format!("spielorders() {{ \"$SPIELORDERS\" \"$@\"; }}; {command}"),
                ])
                .env("SPIELORDERS", env!("CARGO_BIN_EXE_spielorders"))
                .env("package", &package)
                .env("directory", &directory)
                .output()
                .unwrap();
            if material {
                let outcome = Outcome {
                    status: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                };
                material_answered(
                    &outcome,
                    &response(&outcome),
                    supporting_material_provider(),
                );
                ran += 1;
                continue;
            }
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}: {line}\n{}{}",
                file.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            ran += 1;
        }
        assert!(ran > 0, "{} has examples", file.display());
    }
}
