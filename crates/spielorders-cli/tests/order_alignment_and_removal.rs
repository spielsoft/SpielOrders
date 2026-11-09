//! `order align` and `order remove`, the two commands that move a whole Order Directory.
//!
//! **What both are really about is that a directory arrives whole or does not move at
//! all.** So the claims here are made against the bytes under the directory rather than
//! against the report: a move that enumerated and copied children would satisfy every
//! sentence in a response document and still be the defect. `managed_snapshot` is how
//! each of them is asserted — every entry under a directory with its contents, so a file
//! added, dropped, or rewritten along the way is a difference.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology.

use crate::support::{
    Invocation, managed_snapshot, order_directory, package_with_a_drifted_order, populated_package,
    projected_alignment, response, supporting_material_in,
};
use std::path::PathBuf;

/// `--uuid` aligns the Order that identity names.
///
/// The second targeting route reaches these mutations as it reaches the reads: the
/// identity resolves against the current-directory Package, and what it resolves to is
/// the Order Directory the core capability moves.
#[test]
fn an_alignment_can_be_targeted_by_order_id() {
    let (_tree, root, directory) = package_with_a_drifted_order("align-by-uuid");
    let aligned = projected_alignment(&directory);
    let order_id = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    )["data"]["order_id"]
        .as_str()
        .expect("the drifted Order carries an Order ID")
        .to_owned();

    let outcome = Invocation::new(&["order", "align", "--uuid", &order_id])
        .in_directory(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert!(
        aligned.is_dir(),
        "the Order that identity names was aligned"
    );
}

/// `--delete` removes the Order, then moves the intact removed directory to the platform
/// Trash, and erases nothing.
///
/// **Asked of the real effect**, which is the only way to tell a Trash move from a delete:
/// both leave the original path empty. So the tree is compared where the platform put it,
/// and the trashed copy is then removed, because this machine's Trash belongs to its owner.
/// Every observation is taken before that cleanup, so a failing assertion cannot skip it.
#[cfg(target_vendor = "apple")]
#[test]
#[ignore = "writes to the owner's real Trash; scripts/smoke-spotlight.sh runs it"]
fn deleting_moves_the_intact_removed_directory_to_the_platform_trash() {
    let (_tree, root) = populated_package("remove-delete");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let submitted = managed_snapshot(&directory);

    let outcome = Invocation::new(&["order", "remove", "--delete", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    let landed = document["data"]["trash_location"]["absolute_path"]
        .as_str()
        .map(PathBuf::from);
    let arrived = landed.as_ref().map(|path| managed_snapshot(path));
    let cleaned = landed
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| std::fs::remove_dir_all(path).is_ok());

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert!(!directory.exists(), "the Order left the Orders Directory");
    assert!(
        !root
            .join("Removed")
            .join(
                directory
                    .file_name()
                    .expect("an Order Directory has a name")
            )
            .exists(),
        "and the directory its removal produced went on to the Trash"
    );
    assert_eq!(
        arrived,
        Some(submitted),
        "the complete tree arrived and nothing was erased"
    );
    assert_eq!(
        cleaned,
        Some(true),
        "and this suite took its own directory back out of the owner's Trash"
    );
}

/// A Trash move that fails leaves the Order removed and recoverable, and says where.
///
/// The Trash is made to fail for real: on Apple platforms the executable runs in a
/// sandbox that may not write to any Trash, which also keeps this run out of the owner's.
/// A platform with no Trash this build can ask fails the same way without one.
#[test]
fn a_failed_trash_move_leaves_the_order_recoverable_in_the_removed_orders_directory() {
    let (_tree, root) = populated_package("remove-delete-refused");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let submitted = managed_snapshot(&directory);
    let removed = root.join("Removed").join(
        directory
            .file_name()
            .expect("an Order Directory has a name"),
    );

    let mut command = if cfg!(target_vendor = "apple") {
        let mut sandboxed = std::process::Command::new("/usr/bin/sandbox-exec");
        sandboxed.args([
            "-p",
            r#"(version 1)(allow default)(deny file-write* (regex #"/\.Trash(es)?(/|$)"))"#,
            env!("CARGO_BIN_EXE_spielorders"),
        ]);
        sandboxed
    } else {
        std::process::Command::new(env!("CARGO_BIN_EXE_spielorders"))
    };
    let output = command
        .args(["order", "remove", "--delete", "--json", "--root"])
        .arg(&directory)
        .output()
        .expect("the spielorders executable runs");
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document");

    assert_eq!(output.status.code(), Some(1), "{document}");
    assert_eq!(document["error"]["code"], "trash_failed", "{document}");
    assert_eq!(
        document["error"]["detail"]["path"],
        removed.to_str().expect("the test path is UTF-8"),
        "the failure names where the Order is"
    );
    assert!(!directory.exists(), "the removal itself committed");
    assert_eq!(
        managed_snapshot(&removed),
        submitted,
        "the whole Order is recoverable in the Removed Orders Directory"
    );
}

/// A dry run of `--delete` reports the removal and the Trash move that follows it, and
/// moves nothing: the directory is still exactly where it was.
#[test]
fn a_dry_run_of_a_delete_moves_nothing() {
    let (_tree, root) = populated_package("remove-delete-dry-run");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "remove", "--delete", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["outcome"], "planned");
    assert_eq!(document["data"]["committed"], false);
    let removed = root.join("Removed").join(
        directory
            .file_name()
            .expect("an Order Directory has a name"),
    );
    assert_eq!(
        document["data"]["trash_move"]["source"],
        removed.to_str().expect("the test path is UTF-8"),
        "the plan says the directory the removal produces then moves to the Trash"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Order Directory is exactly where it was, with everything in it"
    );
}

/// Each command answers a human, and `--quiet` withholds the result without withholding
/// the mutation.
///
/// Human output is never a parsing contract, so what is asserted is the discipline rather
/// than the wording: the result goes to standard output, nothing incidental goes to
/// standard error, `--verbose` adds detail, and `--quiet` says nothing on success. A
/// relocation that answered only under `--json` would be a command with no human answer
/// at all.
///
/// `--delete` is exercised as a dry run, because a committed one would leave a directory
/// in this machine's Trash that no path in human output could be used to clean up — and
/// human output is deliberately not a path a test may parse.
#[test]
fn each_relocation_answers_a_human_and_withholds_only_the_result_under_quiet() {
    for (command, subject, arrival) in [
        (vec!["order", "align"], "outcome: committed\n", None),
        (
            vec!["order", "remove"],
            "outcome: committed\n",
            Some("final path: "),
        ),
        (
            vec!["order", "remove", "--delete", "--dry-run"],
            "outcome: planned\n",
            Some("trash move:"),
        ),
    ] {
        let label = command.join("-");
        let (_tree, _root, directory) = package_with_a_drifted_order(&format!("human-{label}"));
        let mut verbose: Vec<&str> = command.clone();
        verbose.push("--verbose");

        let detailed = Invocation::new(&verbose).rooted_at(&directory).run();

        assert_eq!(
            detailed.status,
            Some(0),
            "{label}: {}",
            detailed.stderr_text()
        );
        assert_eq!(
            detailed.stderr, b"",
            "{label}: a successful relocation writes nothing to standard error"
        );
        assert!(
            detailed.stdout_text().contains(subject),
            "{label}: the result says what this run did: {}",
            detailed.stdout_text()
        );
        assert!(
            detailed.stdout_text().contains("operations:"),
            "{label}: --verbose adds the individual managed detail: {}",
            detailed.stdout_text()
        );
        if let Some(arrival) = arrival {
            // A reader who skimmed one line of a plan and took it for a result is the
            // whole hazard of reporting a plan in the same shape as a result, so the
            // line that says where the directory is says which of the two it is.
            assert!(
                detailed.stdout_text().contains(arrival),
                "{label}: the arrival line is `{arrival}`: {}",
                detailed.stdout_text()
            );
        }
    }

    for command in [vec!["order", "align"], vec!["order", "remove"]] {
        let label = command.join("-");
        let (_tree, _root, directory) = package_with_a_drifted_order(&format!("quiet-{label}"));
        let mut quiet: Vec<&str> = command.clone();
        quiet.push("--quiet");

        let silent = Invocation::new(&quiet).rooted_at(&directory).run();

        assert_eq!(silent.status, Some(0), "{label}: {}", silent.stderr_text());
        assert_eq!(silent.stdout, b"", "{label}: --quiet withholds the result");
        assert_eq!(silent.stderr, b"", "{label}: and reports no failure");
        assert!(
            !directory.exists(),
            "{label}: the directory moved all the same"
        );
    }
}
