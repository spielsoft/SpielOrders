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
    Invocation, managed_snapshot, order_directory, package_with_a_drifted_order,
    package_with_an_occupied_aligned_name, package_with_an_occupied_removed_name,
    populated_package, projected_alignment, removed_orders_directory, response,
    supporting_material_in,
};
use std::path::PathBuf;

/// Alignment renames the Directory, and the Order is still an active, readable Order of
/// this Package at its new path.
///
/// The read afterwards is the whole point: the Order's own technical locator is what
/// reaches its Package from its Directory, and a rename that left the locator wrong would
/// pass every assertion about the rename itself.
#[test]
fn alignment_renames_the_directory_and_leaves_the_order_readable_there() {
    let (_tree, root, directory) = package_with_a_drifted_order("align-renames");
    let aligned = projected_alignment(&directory);

    let outcome = Invocation::new(&["order", "align"])
        .rooted_at(&directory)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert!(!directory.exists(), "the old Directory name is gone");
    assert!(aligned.is_dir(), "the Directory stands at its aligned name");

    let read = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&aligned)
        .run();
    let document = response(&read);
    assert_eq!(read.status, Some(0), "{}", read.stderr_text());
    assert_eq!(
        document["data"]["title"], "Laser heads",
        "the Order's own facts are untouched by the rename"
    );
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test Package root is UTF-8"),
        "and its locator still resolves to this Package"
    );
}

/// The whole tree travels with the directory, byte for byte.
///
/// Alignment and removal are the only commands that may move Supporting Material, and
/// they move it as part of the whole directory. Nothing enumerates, reads, hashes, or
/// rewrites it — which is why this compares the complete tree rather than the files a
/// test thought to name.
#[test]
fn alignment_carries_the_whole_directory_intact() {
    let (_tree, _root, directory) = package_with_a_drifted_order("align-carries-material");
    supporting_material_in(&directory);
    let before = managed_snapshot(&directory);
    let aligned = projected_alignment(&directory);

    let outcome = Invocation::new(&["order", "align"])
        .rooted_at(&directory)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        managed_snapshot(&aligned),
        before,
        "every entry under the Order Directory arrived unchanged"
    );
}

/// An Order already carrying the name its facts project is a success that renames
/// nothing.
///
/// Drift is ordinary allowed state and so is its absence: alignment is an organization
/// aid, not a validity condition, so asking for it twice is not an error the second time.
#[test]
fn aligning_an_already_aligned_order_changes_nothing() {
    let (_tree, root, directory) = package_with_a_drifted_order("align-idempotent");
    let aligned = projected_alignment(&directory);
    assert_eq!(
        Invocation::new(&["order", "align"])
            .rooted_at(&directory)
            .run()
            .status,
        Some(0)
    );
    let settled = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "align", "--json"])
        .rooted_at(&aligned)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(
        document["data"]["report"]["changed"], false,
        "there was no rename to perform"
    );
    assert_eq!(
        document["data"]["report"]["old_path"], document["data"]["report"]["new_path"],
        "an aligned Order projects the name it already carries"
    );
    assert_eq!(
        managed_snapshot(&root),
        settled,
        "and the Package is byte-identical"
    );
}

/// An alignment onto an occupied name is refused, and nothing is moved.
///
/// No suffix is invented: the collision is the answer, and both the Order and whatever
/// occupies the name are left exactly as they were.
#[test]
fn an_alignment_onto_an_occupied_name_is_refused_and_moves_nothing() {
    let (_tree, root, directory, occupied) =
        package_with_an_occupied_aligned_name("align-collision");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "align", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_directory_occupied");
    assert_eq!(
        document["error"]["paths"][0]["path"],
        occupied.to_str().expect("the test path is UTF-8"),
        "the failure names the name that is taken"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "nothing was renamed and nothing was created"
    );
}

/// A dry run of an alignment reports the rename it would perform and performs none of it.
///
/// **The criterion worth proving rather than asserting**: a dry run that actually renamed
/// would be caught by nothing else, because the report it writes is the same report the
/// real run writes. So the plan is compared against the commit of the same command line,
/// and the Package is compared against itself.
#[test]
fn a_dry_run_of_an_alignment_reports_the_rename_and_performs_none_of_it() {
    let (_tree, root, directory) = package_with_a_drifted_order("align-dry-run");
    supporting_material_in(&directory);
    let before = managed_snapshot(&root);

    let planned = Invocation::new(&["order", "align", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let plan = response(&planned);

    assert_eq!(planned.status, Some(0), "{}", planned.stderr_text());
    assert_eq!(plan["data"]["report"]["committed"], false);
    assert_eq!(
        managed_snapshot(&root),
        before,
        "no rename, no staged file, no created directory"
    );

    let committed = Invocation::new(&["order", "align", "--json"])
        .rooted_at(&directory)
        .run();
    let real = response(&committed);

    assert_eq!(committed.status, Some(0), "{}", committed.stderr_text());
    assert_eq!(
        plan["data"]["report"]["new_path"], real["data"]["report"]["new_path"],
        "the plan named the rename the commit performed"
    );
    assert_eq!(
        plan["data"]["report"]["basename"], real["data"]["report"]["basename"],
        "and the same projected name"
    );
    assert_eq!(
        plan["data"]["report"]["operations"], real["data"]["report"]["operations"],
        "and the same managed operations"
    );
}

/// A dry run of a refused alignment refuses identically and still writes nothing.
///
/// A refusal is a precondition, so `--dry-run` does not soften it: the collision is
/// reported at exit `1` either way, and neither run touches the Package.
#[test]
fn a_dry_run_of_a_refused_alignment_refuses_and_writes_nothing() {
    let (_tree, root, directory, _occupied) =
        package_with_an_occupied_aligned_name("align-dry-run-collision");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "align", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_directory_occupied");
    assert_eq!(managed_snapshot(&root), before);
}

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

/// An Order whose Directory already matches is still aligned by name, not by identity.
///
/// `populated_package`'s Orders are created aligned, which is the ordinary state: a
/// freshly created Order Directory is aligned automatically. So this is the run a user
/// makes most often, and it must be a quiet success rather than a refusal.
#[test]
fn a_freshly_created_order_is_already_aligned() {
    let (_tree, root) = populated_package("align-fresh-order");
    let directory = order_directory(&root, "Laser_components");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "align", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["changed"], false);
    assert_eq!(managed_snapshot(&root), before);
}

/// A removal moves the complete directory into the Removed Orders Directory, which is
/// created at that moment because it was not there.
///
/// The whole tree is compared, so a disposition that enumerated and copied children — or
/// dropped the nested sentinel — is a failure rather than a passing report.
#[test]
fn a_removal_moves_the_whole_directory_into_the_removed_orders_directory() {
    let (_tree, root) = populated_package("remove-moves");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let before = managed_snapshot(&directory);
    let removed = removed_orders_directory(&root);
    assert!(
        !removed.exists(),
        "the Removed Orders Directory is created lazily, so it is not there yet"
    );
    let destination = removed.join(directory.file_name().expect("the Order has a basename"));

    let outcome = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert!(!directory.exists(), "the Order left the Orders Directory");
    assert_eq!(
        managed_snapshot(&destination),
        before,
        "every entry arrived unchanged: managed metadata, README, and both sentinels"
    );
    assert_eq!(
        document["data"]["report"]["disposition"],
        "recoverable_directory"
    );
    assert_eq!(
        document["data"]["report"]["new_path"]["absolute_path"],
        destination.to_str().expect("the test path is UTF-8")
    );
    assert_eq!(
        document["data"]["removed"]["title"], "Laser components",
        "the response names the Order it took out"
    );
}

/// An existing Removed Orders Directory is used rather than created again.
///
/// The other half of "created at that moment only if it did not exist": the same
/// disposition, and no creation among its operations.
#[test]
fn a_removal_into_an_existing_removed_orders_directory_creates_nothing() {
    let (_tree, root) = populated_package("remove-existing-removed");
    let directory = order_directory(&root, "Laser_components");
    let removed = removed_orders_directory(&root);
    std::fs::create_dir(&removed).expect("the Removed Orders Directory is already there");

    let outcome = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    let operations: Vec<&str> = document["data"]["report"]["operations"]
        .as_array()
        .expect("a disposition reports its operations")
        .iter()
        .map(|operation| {
            operation["operation"]
                .as_str()
                .expect("each operation is named")
        })
        .collect();
    assert!(
        !operations.contains(&"create_removed_orders_directory"),
        "nothing was created: {operations:?}"
    );
}

/// After a removal the Order is gone from every active operation, and the Package is
/// otherwise unchanged.
///
/// `package show` is the complete bootstrap read and the index is what the desktop shell
/// loads, so an Order still in either is an Order still in the Package. The sibling Order
/// is what makes "otherwise unchanged" a claim rather than an absence.
#[test]
fn a_removed_order_leaves_every_active_operation() {
    let (_tree, root) = populated_package("remove-leaves-scope");
    let directory = order_directory(&root, "Laser_components");
    let sibling = order_directory(&root, "Mirror_mounts");
    let sibling_before = managed_snapshot(&sibling);

    let removal = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let reported = response(&removal);
    assert_eq!(removal.status, Some(0), "{}", removal.stdout_text());
    assert_eq!(
        reported["data"]["report"]["index"]["state"], "current",
        "the derived index was rebuilt around what remains"
    );

    for read in [
        vec!["package", "show", "--json"],
        vec!["package", "list", "orders", "--json"],
    ] {
        let document = response(&Invocation::new(&read).rooted_at(&root).run());
        let titles: Vec<&str> = document["data"]["orders"]
            .as_array()
            .expect("a Package reports its Orders")
            .iter()
            .map(|order| order["title"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            titles,
            vec!["Mirror mounts"],
            "{read:?}: the removed Order is outside active discovery and its sibling is not"
        );
    }
    assert_eq!(
        managed_snapshot(&sibling),
        sibling_before,
        "the Package is otherwise unchanged"
    );
}

/// A removal onto an occupied name is refused, and nothing is moved.
#[test]
fn a_removal_onto_an_occupied_name_is_refused_and_moves_nothing() {
    let (_tree, root, directory, occupied) =
        package_with_an_occupied_removed_name("remove-collision");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_directory_occupied");
    assert_eq!(
        document["error"]["paths"][0]["path"],
        occupied.to_str().expect("the test path is UTF-8")
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Order is still active and nothing was created"
    );
}

/// `--delete` submits the intact directory to the platform Trash, and erases nothing.
///
/// **Asserted against the real effect**, which is the only way to tell a submission from
/// a delete: both leave the original path empty. So the tree is compared where the
/// platform put it, and the trashed copy is then removed — this machine's Trash belongs
/// to its owner. Every observation is taken before that cleanup, so a failing assertion
/// cannot skip it. A platform with no supported Trash reports that specifically instead,
/// and this asserts the Order survived it.
#[test]
fn deleting_submits_the_intact_directory_to_the_platform_trash() {
    let (_tree, root) = populated_package("remove-delete");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let submitted = managed_snapshot(&directory);

    let outcome = Invocation::new(&["order", "remove", "--delete", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    if outcome.status == Some(1) {
        assert_eq!(
            document["error"]["code"],
            "order_trash_unsupported",
            "a platform with no supported Trash says so specifically: {}",
            outcome.stdout_text()
        );
        return assert_eq!(
            managed_snapshot(&directory),
            submitted,
            "and nothing was erased to discover it"
        );
    }

    let landed = document["data"]["report"]["new_path"]["absolute_path"]
        .as_str()
        .map(PathBuf::from);
    let arrived = landed.as_ref().map(|path| managed_snapshot(path));
    let cleaned = landed
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| std::fs::remove_dir_all(path).is_ok());

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["report"]["disposition"], "platform_trash");
    assert!(
        !directory.exists(),
        "the directory is no longer at its original path"
    );
    assert_eq!(
        arrived,
        Some(submitted),
        "the complete tree was submitted and nothing was erased"
    );
    assert_eq!(
        document["data"]["report"]["locator"]["outcome"], "not_applicable",
        "a directory that leaves the Package has no Package-relative locator"
    );
    assert_eq!(
        cleaned,
        Some(true),
        "and this suite took its own directory back out of the owner's Trash"
    );
}

/// A committed removal whose derived index refresh then fails stays committed, reports
/// the warning, and exits `0`.
///
/// The canonical half happened: the directory is gone from the Orders Directory. Nothing
/// about the report may look like something to retry, because re-running it would ask to
/// remove an Order that is no longer there.
#[test]
fn a_committed_removal_whose_index_refresh_fails_stays_committed() {
    let (_tree, root) = crate::support::package_with_a_blocked_index_refresh("remove-stale-index");
    let directory = order_directory(&root, "Laser_components");
    let destination = removed_orders_directory(&root)
        .join(directory.file_name().expect("the Order has a basename"));

    let outcome = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "a committed mutation with a derived-index warning exits 0: {}",
        outcome.stdout_text()
    );
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(
        document["data"]["report"]["warnings"]
            .as_array()
            .map(Vec::as_slice),
        Some(["index_refresh_failed".into()].as_slice())
    );
    assert!(destination.is_dir(), "the directory really did move");
    assert!(!directory.exists(), "and it is not in two places");
}

/// A dry run of a removal reports the move it would perform and performs none of it.
///
/// **The criterion worth proving rather than asserting**: a dry run of a removal that
/// actually removed would be found by nothing else. So the Package is compared against
/// itself — no move, no created Removed Orders Directory, no staged file — and the plan
/// is compared against the commit of the same command line.
#[test]
fn a_dry_run_of_a_removal_reports_the_move_and_performs_none_of_it() {
    let (_tree, root) = populated_package("remove-dry-run");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let before = managed_snapshot(&root);
    let removed = removed_orders_directory(&root);

    let planned = Invocation::new(&["order", "remove", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let plan = response(&planned);

    assert_eq!(planned.status, Some(0), "{}", planned.stdout_text());
    assert_eq!(plan["data"]["report"]["committed"], false);
    assert!(
        !removed.exists(),
        "the Removed Orders Directory was not created"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "no move, no created directory, no staged file"
    );

    let committed = Invocation::new(&["order", "remove", "--json"])
        .rooted_at(&directory)
        .run();
    let real = response(&committed);

    assert_eq!(committed.status, Some(0), "{}", committed.stdout_text());
    assert_eq!(
        plan["data"]["report"]["new_path"], real["data"]["report"]["new_path"],
        "the plan named the destination the commit used"
    );
    assert_eq!(
        plan["data"]["report"]["operations"], real["data"]["report"]["operations"],
        "and the same managed operations, the lazy creation among them"
    );
    assert_eq!(
        plan["data"]["removed"], real["data"]["removed"],
        "and the same Order"
    );
}

/// A dry run of a disposal asks the platform Trash for nothing.
///
/// The one effect no snapshot of the Package could catch: a directory really submitted to
/// the Trash is gone from the Package *and* sitting in the user's Trash, so what proves a
/// dry run asked for nothing is that the directory is still exactly where it was.
#[test]
fn a_dry_run_of_a_disposal_asks_the_trash_for_nothing() {
    let (_tree, root) = populated_package("remove-delete-dry-run");
    let directory = order_directory(&root, "Laser_components");
    supporting_material_in(&directory);
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "remove", "--delete", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["report"]["committed"], false);
    assert_eq!(document["data"]["report"]["disposition"], "platform_trash");
    assert!(
        document["data"]["report"]["new_path"].is_null(),
        "the platform owns where a trashed directory goes, and it was never asked"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Order Directory is exactly where it was, with everything in it"
    );
}

/// A dry run of a refused removal refuses identically and still writes nothing.
#[test]
fn a_dry_run_of_a_refused_removal_refuses_and_writes_nothing() {
    let (_tree, root, directory, _occupied) =
        package_with_an_occupied_removed_name("remove-dry-run-collision");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "remove", "--json", "--dry-run"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_directory_occupied");
    assert_eq!(managed_snapshot(&root), before);
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
/// The disposal is exercised as a dry run, because a committed one would leave a
/// directory in this machine's Trash that no path in human output could be used to clean
/// up — and human output is deliberately not a path a test may parse.
#[test]
fn each_relocation_answers_a_human_and_withholds_only_the_result_under_quiet() {
    for (command, subject, arrival) in [
        (vec!["order", "align"], "Aligned", None),
        (vec!["order", "remove"], "Removed", Some("Now at:")),
        (
            vec!["order", "remove", "--delete", "--dry-run"],
            "Would trash",
            Some("Would go to:"),
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
            detailed.stdout_text().contains("Old path:"),
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

/// A relocation reports the README the Order actually has, not one it assumes.
///
/// A README is optional and loads as empty when absent, so an Order without one is
/// ordinary — and both of these commands move whatever document is there with the
/// directory rather than writing one. A report that claimed one was present would be
/// telling a user something about their Order that is not true.
#[test]
fn a_relocation_reports_the_readme_the_order_actually_has() {
    for command in [vec!["order", "align"], vec!["order", "remove"]] {
        let label = command.join("-");
        let (_tree, _root, directory) =
            crate::support::package_with_an_empty_order(&format!("readme-{label}"));
        assert!(
            !directory.join("README.md").exists(),
            "{label}: this Order has no README"
        );

        let mut arguments: Vec<&str> = command.clone();
        arguments.push("--verbose");
        let outcome = Invocation::new(&arguments).rooted_at(&directory).run();

        assert_eq!(
            outcome.status,
            Some(0),
            "{label}: {}",
            outcome.stderr_text()
        );
        assert!(
            outcome.stdout_text().contains("README: absent"),
            "{label}: {}",
            outcome.stdout_text()
        );
    }
}
