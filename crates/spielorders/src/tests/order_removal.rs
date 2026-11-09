//! Decision sources: Define the SpielOrders package topology; Define the Order record
//! and lifecycle contract; Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping.

use crate::tests::support::index_is_current;
use crate::tests::support::operation_path;
use crate::tests::support::{
    Answer, EditPoint, FailedCommit, FixedUuid, Index, Operation, PlatformFileSystem,
    RefreshBreakage, ScriptedFileSystem, TemporaryTree, adoption, another_writer_replacing,
    edit_order, effects, opened, operation_kinds, order_at, package_tree_with, remove_order, text,
};
use crate::tests::support::{assert_reported_beside, spelled, warning_entries};
use crate::{Assignments, Change, ChangeResult, OrderRef, Report};
use std::ops::Deref;

use crate::{evidence::EvidenceOutcome, evidence::FactChange};

use crate::metadata::OrderDocument;

use crate::ports::NewDirectoryRenameOutcome;
use crate::{Failure, Warning};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SUBSTITUTE_ORDER_ID: &str = "1f0c9a52-4d3b-4a71-8e0c-6b2f5d9a7c31";
const BASENAME: &str = "2026_08_22_Laser_components";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

/// An Order carrying a README, managed metadata, and both opaque sentinels, one
/// nested, so a disposition can be shown to move the complete tree intact.
fn surrounded_order(root: &Path, orders_path: &str) -> PathBuf {
    surrounded_order_with_id(root, orders_path, ORDER_ID)
}

fn surrounded_order_with_id(root: &Path, orders_path: &str, order_id: &str) -> PathBuf {
    let directory = root.join(orders_path).join(BASENAME);
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();

    let plan = crate::plan(
        &adoption(
            PathBuf::from(root),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(order_id.into())),
    )
    .result;
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn default_package(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = package_tree_with(label, &("Orders", "Removed"));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Orders");
    (tree, root, directory)
}

/// A Package whose Orders Directory sits one level deeper than its Removed Orders
/// Directory, so a recoverable disposition genuinely changes the Order's locator and
/// the record rewrite is reached at all. Every shallower layout skips it.
fn package_with_a_deeper_orders_directory(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = package_tree_with(label, &("Archive/Orders", "Removed"));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");
    (tree, root, directory)
}

/// The record the moved Order carries, and the staging file a rewrite of it stages
/// beside it.
fn moved_record(removed: &Path) -> (PathBuf, PathBuf) {
    let metadata = removed.join(".spielorders");
    (
        metadata.join("order.json"),
        metadata.join(".order.json.spielorders-stage"),
    )
}

fn indexed_order_ids(root: &Path) -> Vec<String> {
    let index = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the derived index parses");
    index
        .orders
        .iter()
        .map(|order| order.order.order_id.deref().to_owned())
        .collect()
}

/// Every byte inside the Order Directory that must survive a disposition intact.
fn contents(directory: &Path) -> Vec<(String, Vec<u8>)> {
    [
        "README.md",
        "quote.bin",
        "Quotes/drawing.bin",
        ".spielorders/order.json",
    ]
    .into_iter()
    .map(|relative| {
        (
            relative.to_owned(),
            fs::read(directory.join(relative))
                .unwrap_or_else(|error| panic!("{relative} is readable: {error}")),
        )
    })
    .collect()
}

#[test]
fn a_removal_moves_the_intact_directory_into_the_removed_orders_directory() {
    let (_tree, root, directory) = default_package("removal-recoverable");
    let removed = root.join("Removed").join(BASENAME);
    let before = contents(&directory);
    assert!(!root.join("Removed").exists(), "Removed starts absent");

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an active Order can be removed recoverably");

    assert_eq!(operation_path(&plan, "move_order_directory"), removed);
    assert_eq!(
        operation_kinds(&plan),
        [
            "create_removed_orders_directory",
            "move_order_directory",
            "refresh_package_index",
        ],
        "a same-depth disposition rewrites no record"
    );
    assert!(directory.is_dir(), "planning moves nothing");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid recoverable removal commits: {report:?}");
    };
    assert_eq!(
        variant!(&result, ChangeResult::RemoveOrder)
            .final_location
            .absolute_path,
        removed
    );

    assert!(!directory.exists(), "the Order left the Orders Directory");
    assert!(removed.is_dir());
    assert_eq!(
        contents(&removed),
        before,
        "every byte inside the directory survives the move"
    );

    // The removed Order leaves active scope entirely.
    assert_eq!(indexed_order_ids(&root), Vec::<String>::new());
    assert!(index_is_current(&root));
    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("the Package still loads after the removal");
    };
    assert!(
        package.contents.healthy_orders.is_empty(),
        "a later load discovers no Order"
    );
    assert!(
        package.contents.damaged_orders.is_empty(),
        "and diagnoses none"
    );
}

#[test]
fn an_existing_removed_orders_directory_is_reused_rather_than_clobbered() {
    let (_tree, root, directory) = default_package("removal-removed-exists");
    let removed_root = root.join("Removed");
    fs::create_dir(&removed_root).unwrap();
    // An earlier removal already left something here; reuse must not disturb it.
    fs::create_dir(removed_root.join("2020_01_01_Older_order")).unwrap();
    fs::write(
        removed_root.join("2020_01_01_Older_order/keep.txt"),
        b"an earlier removal\n",
    )
    .unwrap();

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed into an existing Removed directory");

    assert_eq!(
        operation_kinds(&plan),
        ["move_order_directory", "refresh_package_index",],
        "an existing Removed Orders Directory is reused, not created"
    );

    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    assert!(removed_root.join(BASENAME).is_dir());
    assert_eq!(
        fs::read(removed_root.join("2020_01_01_Older_order/keep.txt")).unwrap(),
        b"an earlier removal\n",
        "the earlier removal is untouched"
    );
}

#[test]
fn the_locator_outcome_covers_unchanged_updated_and_not_applicable() {
    // Unchanged: Orders and Removed are both direct children, so the relative path
    // back to the Package root keeps its depth.
    let (_tree, root, directory) = default_package("removal-locator-unchanged");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let removed = root.join("Removed").join(BASENAME);
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert_eq!(
        stored_locator(&removed),
        "../..",
        "an unchanged locator is left exactly as stored"
    );

    // Updated: a nested Orders Directory sits one level deeper than Removed, so the
    // moved Order's relative path back to the root genuinely changes.
    let tree = package_tree_with("removal-locator-updated", &("Archive/Orders", "Removed"));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");
    assert_eq!(stored_locator(&directory), "../../..");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert_eq!(
        operation_kinds(&plan),
        [
            "create_removed_orders_directory",
            "move_order_directory",
            "replace_order_record",
            "refresh_package_index",
        ],
        "a depth change adds the record rewrite the shallower cases do not need"
    );
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let removed = root.join("Removed").join(BASENAME);
    assert_eq!(
        stored_locator(&removed),
        "../..",
        "the locator is rewritten to match where the directory now sits"
    );
}

fn stored_locator(directory: &Path) -> String {
    serde_json::from_slice::<OrderDocument>(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .map(|order| text(&order.package_root))
    .expect("the canonical Order parses")
}

#[test]
fn no_disposition_enumerates_the_directory_it_moves() {
    let (_tree, root, directory) = default_package("removal-no-enumeration");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let observing = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&observing)).result;

    assert!(matches!(report, Ok(Report::Committed { .. })));
    // The refresh that follows lists the Orders Directory, which is its declared
    // work. Nothing lists inside the Order that moved, and nothing reads its opaque
    // Supporting Material.
    let removed = root.join("Removed").join(BASENAME);
    for listed in observing.paths(Operation::ReadDirectory) {
        assert!(
            !listed.starts_with(&directory) && !listed.starts_with(&removed),
            "no disposition enumerates the Order Directory, but listed {listed:?}"
        );
    }
    for read in observing.paths(Operation::Read) {
        let read = read.to_string_lossy().to_string();
        assert!(
            !read.contains("quote.bin") && !read.contains("drawing.bin"),
            "Supporting Material is never inspected, but read {read}"
        );
    }
}

/// Real effects, except that the whole-directory move fails.
fn move_erroring() -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    filesystem.on(Operation::RenameNewDirectory).fails();
    filesystem
}

/// The move fails, and something arrives inside the Removed Orders Directory at that
/// moment, the one window in which something can land in a directory this operation
/// created before its rollback looks at it.
fn move_erroring_after_arrival(arrival: &'static str) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::RenameNewDirectory)
        .preceded_by(move |outside| {
            let destination = &outside.call().path;
            outside.write(&destination.parent().unwrap().join(arrival), b"not ours\n");
        })
        .fails();
    filesystem
}

fn move_erroring_with_undo_failing() -> ScriptedFileSystem {
    let filesystem = move_erroring();
    filesystem.on(Operation::RemoveDirectory).fails();
    filesystem
}

fn move_reporting(outcome: NewDirectoryRenameOutcome) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::RenameNewDirectory)
        .answers(Answer::Renamed(outcome));
    filesystem
}

fn index_failing() -> ScriptedFileSystem {
    ScriptedFileSystem::new().failing_ending_in(Operation::ReplaceFile, ".spielorders/index.json")
}

fn creation_failing() -> ScriptedFileSystem {
    ScriptedFileSystem::new().failing_ending_in(Operation::CreateDirectory, "")
}

/// The locator rewrite fails at the rename that publishes the rewritten record, and
/// the durable stage it wrote cannot be removed afterwards, so the orphan is real.
fn locator_rewrite_failing_with_cleanup_failing() -> ScriptedFileSystem {
    ScriptedFileSystem::new()
        .failing_ending_in(Operation::ReplaceFile, ".spielorders/order.json")
        .failing_ending_in(Operation::RemoveFile, ".order.json.spielorders-stage")
}

/// Breaks the refresh once the removal has taken the Order from `source`, and never
/// the planning reads before that.
fn breaking_refresh_once_gone(source: PathBuf, breakage: RefreshBreakage) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    let gone = move || !source.exists();
    match breakage {
        RefreshBreakage::ListingFails => filesystem.on(Operation::ReadDirectory).when(gone).fails(),
        _ => filesystem
            .on(Operation::Read)
            .ending_in(".spielorders/version.json")
            .when(gone)
            .answers(Answer::Bytes(b"{ not json".to_vec())),
    }
    filesystem
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// A refusal is only useful if it names *which* fact moved. The rejection variant
/// alone would be the same unexplained precondition failure for a substituted Order
/// and for a source that simply vanished.
fn unconfirmed_facts(report: &Result<Report, Failure>) -> Vec<(&str, FactChange)> {
    let Err(Failure::PlanEvidenceUnconfirmed { outcome }) = report else {
        panic!("a plan whose evidence moved is refused before any effect: {report:?}");
    };
    let EvidenceOutcome::Changed(stale) = outcome else {
        panic!("the refusal names the evidence that changed: {outcome:?}");
    };
    stale
        .changed
        .iter()
        .map(|fact| (fact.name.as_str(), fact.change))
        .collect()
}

/// Authorizing a removal against the wrong Order disposes of an Order the user never
/// selected.
#[test]
fn a_different_order_left_at_the_planned_source_is_never_disposed_of() {
    // The removal names every fact it relies on, including the Order record it may
    // rewrite at the destination.
    let expected = vec![
        ("order_id", FactChange::Differs),
        ("order_record", FactChange::Differs),
    ];
    let (_tree, root, directory) = default_package("removal-substituted-order");
    let plan = crate::plan(
        &Change::RemoveOrder {
            order: OrderRef::Directory(directory.clone()),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The planned Order is gone and a different Order stands at exactly the same
    // path. Every check the commit could make about the *path* still passes: it is
    // present, it is an ordinary directory, and it is a healthy Order of this
    // Package. Only its identity differs, and disposing of it would apply one
    // Order's authorized removal to another Order's directory.
    fs::remove_dir_all(&directory).unwrap();
    let substituted = surrounded_order_with_id(&root, "Orders", SUBSTITUTE_ORDER_ID);
    assert_eq!(substituted, directory);
    assert!(
        fs::symlink_metadata(&directory).unwrap().is_dir(),
        "the source is still an ordinary directory, so existence alone cannot refuse"
    );
    let before = contents(&directory);

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        expected,
        "the refusal names the identity that moved, not the path"
    );
    // The Order that happens to occupy the path is left exactly as it stands.
    assert!(directory.is_dir());
    assert_eq!(contents(&directory), before);
    assert!(
        !root.join("Removed").exists(),
        "a refused removal creates no Removed Orders Directory"
    );
    let resident = order_at(&directory, &PlatformFileSystem)
        .result
        .expect("the substituted Order is untouched and still reads on its own terms");
    assert_eq!(
        resident.order.document.order_id.deref(),
        SUBSTITUTE_ORDER_ID
    );
}

/// A depth-changing recoverable disposition rewrites the Order record at the
/// destination from the plan's own copy of it, and that copy is the only one left once
/// the directory has moved. A plan bound to identity alone would carry an edit made
/// after planning back out and destroy it.
#[test]
fn an_order_edited_after_planning_is_never_carried_back_out_stale() {
    let tree = package_tree_with("removal-stale-record", &("Archive/Orders", "Removed"));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // An ordinary fact edit. It deliberately leaves the directory where it is, so the
    // source, the identity, and the projected destination the plan recorded all still
    // stand; only the record the removal would rewrite has moved.
    let edit = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the title can be edited");
    assert!(matches!(
        crate::commit(edit, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [("order_record", FactChange::Differs)],
        "the refusal names the record that moved, not the path it still occupies"
    );
    assert!(directory.is_dir(), "nothing moved");
    assert!(!root.join("Removed").exists());
    assert_eq!(
        serde_json::from_slice::<OrderDocument>(
            &fs::read(directory.join(".spielorders/order.json")).unwrap()
        )
        .expect("the canonical Order parses")
        .title
        .deref(),
        "Laser mounts",
        "the edit survives the refused removal"
    );
}

#[test]
fn planning_a_removal_moves_nothing() {
    let (_tree, root, directory) = default_package("removal-dry-run");
    let before = contents(&directory);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap_or_else(|error| panic!("a removal can be planned: {error:?}"));
    assert!(!plan.operations().is_empty());

    // A dry run creates no Removed Orders Directory, moves nothing, and rebuilds no
    // index.
    assert!(!root.join("Removed").exists());
    assert!(directory.is_dir());
    assert_eq!(contents(&directory), before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_destination_collision_stops_removal_without_inventing_a_suffix() {
    let (_tree, root, directory) = default_package("removal-collision");
    let removed_root = root.join("Removed");
    let occupied = removed_root.join(BASENAME);
    fs::create_dir_all(&occupied).unwrap();
    fs::write(occupied.join("sibling.txt"), b"an earlier removal\n").unwrap();
    let before = contents(&directory);

    let error = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a taken destination stops removal");

    let Failure::DestinationCollision { location, .. } = &error else {
        panic!("a collision is reported as such: {error:?}");
    };
    assert_eq!(location.absolute_path, occupied);
    assert!(
        location.package_relative_path.is_some(),
        "a refused destination inside the Package carries the Package's spelling for it"
    );
    assert!(directory.is_dir(), "the Order stays active");
    assert_eq!(contents(&directory), before);
    assert_eq!(
        fs::read(occupied.join("sibling.txt")).unwrap(),
        b"an earlier removal\n"
    );
    let names = fs::read_dir(&removed_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, [BASENAME], "no suffixed variant was created");
}

#[test]
fn a_removed_path_that_is_not_an_ordinary_directory_damages_the_package() {
    // Configured-path validity belongs to the Package load, which refuses anything
    // other than an absent or ordinary root-child directory. An unusable Removed path
    // therefore damages the Package before a removal can be planned at all, which is
    // a stronger refusal than a removal-local one.
    let (_tree, root, directory) = default_package("removal-removed-is-file");
    fs::write(root.join("Removed"), b"not a directory\n").unwrap();
    let error = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a file at the Removed path is not usable");
    assert!(
        matches!(error, Failure::DamagedPackage(_)),
        "a file at the Removed path damages the Package: {error:?}"
    );
    assert!(directory.is_dir());
    assert_eq!(
        fs::read(root.join("Removed")).unwrap(),
        b"not a directory\n"
    );

    // A link is likewise not an ordinary directory this core will move into.
    #[cfg(unix)]
    {
        let (_tree, root, directory) = default_package("removal-removed-is-link");
        let elsewhere = root.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("Removed")).unwrap();
        let error = crate::plan(
            &remove_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("a link at the Removed path is not usable");
        assert!(
            matches!(error, Failure::DamagedPackage(_)),
            "a link at the Removed path damages the Package: {error:?}"
        );
        assert!(directory.is_dir());
        assert_eq!(
            fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "the link target is never written through"
        );
    }
}

#[test]
fn a_wrong_scope_or_vanished_source_never_removes_anything() {
    // A directory that is not an Order of its Package.
    let (_tree, root, _directory) = default_package("removal-wrong-scope");
    let stranger = root.join("Orders").join("not_an_order");
    fs::create_dir(&stranger).unwrap();
    let error = crate::plan(
        &remove_order(stranger.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a directory without an Order record is not removable");
    assert!(
        matches!(error, Failure::NotAnOrderDirectory { .. }),
        "a non-Order directory is not an Order Directory: {error:?}"
    );
    assert!(stranger.is_dir());
    assert!(!root.join("Removed").exists());

    // A source that vanished between planning and commit.
    let (_tree, root, directory) = default_package("removal-vanished-source");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    fs::remove_dir_all(&directory).unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert!(
        matches!(report, Err(Failure::PreconditionsChanged { .. })),
        "a vanished source is a precondition change: {report:?}"
    );
    assert!(
        !root.join("Removed").exists(),
        "a refused removal creates no Removed Orders Directory"
    );
}

#[test]
fn injected_disposition_failures_leave_the_order_active_and_whole() {
    // The move itself fails.
    let (_tree, root, directory) = default_package("removal-move-failure");
    let before = contents(&directory);
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(plan, &effects(&move_erroring())).result;
    assert!(
        matches!(report, Err(Failure::CommitFailed { .. })),
        "an injected move failure rejects: {report:?}"
    );
    assert_reported_beside(failed_commit(&report));
    assert!(directory.is_dir(), "the Order stays where it was");
    assert_eq!(contents(&directory), before, "no child moved on its own");
    assert!(!root.join("Removed").join(BASENAME).exists());
    assert_eq!(indexed_order_ids(&root), [ORDER_ID.to_owned()]);

    // The destination is taken between planning and the move.
    let (_tree, _root, directory) = default_package("removal-move-collision-race");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(
        plan,
        &effects(&move_reporting(
            NewDirectoryRenameOutcome::DestinationExists,
        )),
    )
    .result;
    assert!(
        report.is_err(),
        "a taken destination at commit is a collision: {report:?}"
    );
    assert!(directory.is_dir());

    // The lazy Removed Orders Directory cannot be created.
    let (_tree, root, directory) = default_package("removal-create-failure");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(plan, &effects(&creation_failing())).result;
    assert!(
        matches!(report, Err(Failure::CommitFailed { .. })),
        "a failed Removed creation rejects: {report:?}"
    );
    assert_reported_beside(failed_commit(&report));
    assert!(directory.is_dir());
    assert!(!root.join("Removed").exists());
}

/// The commit failure a rejected removal reports.
fn failed_commit(report: &Result<Report, Failure>) -> &Failure {
    let Err(failed) = report else {
        panic!("a disposition that could not happen is a failed commit: {report:?}");
    };
    failed
}

/// A recoverable removal creates the Removed Orders Directory lazily, so a rejection
/// after that creation would otherwise leave a directory in the user's Package that no
/// removal ever put anything in — and the user has no way to tell it from one they
/// created themselves.
#[test]
fn a_rejected_move_leaves_no_removed_orders_directory_it_created() {
    // The move errors outright, and the destination is taken between this operation's
    // own creation and its move — the only window in which a genuine collision can
    // follow a creation this operation is responsible for.
    for (label, injected) in [
        ("removal-rollback-move-error", move_erroring()),
        (
            "removal-rollback-move-collision",
            move_reporting(NewDirectoryRenameOutcome::DestinationExists),
        ),
    ] {
        let (_tree, root, directory) = default_package(label);
        let removed_root = root.join("Removed");
        assert!(!removed_root.exists(), "{label}: Removed starts absent");
        let plan = crate::plan(
            &remove_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();

        let report = crate::commit(plan, &effects(&injected)).result;

        assert!(
            matches!(failed_commit(&report), Failure::CommitFailed { .. }),
            "{label}: the rejection's cleanup completed: {report:?}"
        );
        assert!(
            !removed_root.exists(),
            "{label}: the rejected operation left no directory behind"
        );
        assert!(directory.is_dir(), "{label}: the Order stays active");
        assert_eq!(indexed_order_ids(&root), [ORDER_ID.to_owned()], "{label}");
    }
}

#[test]
fn a_rejected_removal_never_undoes_a_removed_orders_directory_it_did_not_create() {
    let (_tree, root, directory) = default_package("removal-rollback-preexisting");
    let removed_root = root.join("Removed");
    // Empty and already there. Emptiness alone would not protect it, so this is the
    // case where only knowing it created nothing keeps the rollback off a directory the
    // user made themselves.
    fs::create_dir(&removed_root).unwrap();

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(!operation_kinds(&plan).contains(&"create_removed_orders_directory".to_owned()));

    let report = crate::commit(plan, &effects(&move_erroring())).result;

    assert!(
        matches!(failed_commit(&report), Failure::CommitFailed { .. }),
        "this operation created nothing to undo: {report:?}"
    );
    assert!(
        removed_root.is_dir(),
        "a directory this operation did not create survives its rejection"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

#[test]
fn a_rejected_removal_retains_a_directory_it_created_that_evidence_arrived_in() {
    let (_tree, root, directory) = default_package("removal-rollback-arrival");
    let removed_root = root.join("Removed");
    let arrived = removed_root.join("arrived.txt");

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(plan, &effects(&move_erroring_after_arrival("arrived.txt"))).result;

    let [retained] = failed_commit(&report).cleanup_failures() else {
        panic!("evidence this operation did not create retains the directory: {report:?}");
    };
    assert_eq!(retained.location.absolute_path, removed_root);
    assert_eq!(
        fs::read(&arrived).unwrap(),
        b"not ours\n",
        "the rollback never discards what it did not create"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

#[test]
fn a_cleanup_that_fails_is_reported_beside_the_failure_that_initiated_it() {
    let (_tree, root, directory) = default_package("removal-rollback-undo-failure");
    let removed_root = root.join("Removed");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(plan, &effects(&move_erroring_with_undo_failing())).result;

    // Beside, not instead of: the removal was refused because the move failed, and the
    // caller separately learns that the directory that refusal created is still there.
    let Failure::CommitFailedCleanupIncomplete {
        failure: _,
        cleanup_failures,
    } = failed_commit(&report)
    else {
        panic!("the initiating failure and the cleanup failure are both reported: {report:?}");
    };
    assert_reported_beside(failed_commit(&report));
    let [cleanup] = cleanup_failures.as_slice() else {
        panic!("one directory was left: {cleanup_failures:?}");
    };
    assert_eq!(cleanup.location.absolute_path, removed_root);
    assert!(
        removed_root.is_dir(),
        "the report names a directory that really is still there"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

/// The record the plan observed is the one the rewrite is authorized to replace, and
/// the copy it replaces is that record's own, carried along by the move. A writer that
/// lands on it between the move and the rename — a restore, an adoption, a repair —
/// meets no other guard: the Order has left the caller's view and nothing looks at that
/// record again.
#[test]
fn a_record_given_a_different_meaning_after_the_move_is_not_overwritten() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-destination-changed");
    let (record, stage) = moved_record(&root.join("Removed").join(BASENAME));
    // The same Order, retitled by someone else: valid, parseable, different meaning.
    let theirs = String::from_utf8(fs::read(directory.join(".spielorders/order.json")).unwrap())
        .unwrap()
        .replace("Laser components", "Retitled by someone else")
        .into_bytes();

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let other_writer = another_writer_replacing(&record, &theirs, EditPoint::StagingWrite);
    let report = crate::commit(plan, &effects(&other_writer)).result;

    assert!(
        other_writer.disturbed(),
        "their record really did land under this rewrite"
    );
    assert_eq!(
        fs::read(&record).unwrap(),
        theirs,
        "their record survives byte for byte, stale locator and all"
    );
    // The directory moved, so the removal committed; what it declined to finish
    // afterwards is the warning.
    let Ok(Report::CommittedWithWarning { warnings, .. }) = &report else {
        panic!("the disposition happened, so the removal commits: {report:?}");
    };
    let [Warning::LocatorUpdateFailed(failure)] = warnings.as_slice() else {
        panic!("the refused rewrite is the one warning: {warnings:?}");
    };
    let Failure::CommitFailed { failure: _ } = failure else {
        panic!("nothing is reported as left behind, because nothing was: {failure:?}");
    };
    assert!(
        !stage.exists(),
        "a refused replacement takes its own staging file back"
    );
}

/// The cost of a semantic expectation, stated rather than left to be discovered: a
/// record someone reformatted, or one whose stored facts were never trimmed, still
/// means what this removal planned against, so the locator rewrite proceeds and their
/// formatting is what is given up. A byte expectation, or a raw parse compared against
/// the load's normalized projection, would refuse to relocate a record a person had
/// tidied by hand.
#[test]
fn a_record_reformatted_after_the_move_is_still_relocated() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-destination-reformatted");
    let (record, _) = moved_record(&root.join("Removed").join(BASENAME));
    let ours = fs::read(directory.join(".spielorders/order.json")).unwrap();
    // Re-emitted with indentation, and with the title left untrimmed the way a hand
    // edit leaves it.
    let untrimmed = String::from_utf8(ours.clone())
        .unwrap()
        .replace("\"Laser components\"", "\"  Laser components  \"");
    let value: serde_json::Value = serde_json::from_str(&untrimmed).unwrap();
    let theirs = serde_json::to_vec_pretty(&value).unwrap();
    assert_ne!(theirs, ours, "their bytes differ from ours");

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let other_writer = another_writer_replacing(&record, &theirs, EditPoint::StagingWrite);
    let report = crate::commit(plan, &effects(&other_writer)).result;

    assert!(
        other_writer.disturbed(),
        "their reformatting really did land under this rewrite"
    );
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "a record that still means what was planned against is relocated: {report:?}"
    );
    assert_eq!(
        stored_locator(&root.join("Removed").join(BASENAME)),
        "../..",
        "the locator is rewritten to match where the directory now sits"
    );
}

/// The locator rewrite is a durable replacement like any other, so a failure of it
/// leaves a staging file beside the record. That file sits inside a directory the user
/// has just removed from active scope, where no diagnosis reaches it, so the warning is
/// the only thing that can name it. A warning that names nothing leaves an orphan
/// nobody removes.
#[test]
fn a_locator_rewrite_that_cannot_clean_up_names_the_stage_it_left() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-cleanup-failure");
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&locator_rewrite_failing_with_cleanup_failing()),
    )
    .result;

    let removed = root.join("Removed").join(BASENAME);
    let (record, stage) = moved_record(&removed);
    assert!(
        stage.is_file(),
        "the failed replacement really did leave a staging file behind"
    );

    // The directory moved, so the removal committed. What could not be finished
    // afterwards is a warning, and it is the only one: the refresh still succeeds.
    let Ok(Report::CommittedWithWarning { warnings, .. }) = &report else {
        panic!("the disposition happened, so the removal commits: {report:?}");
    };
    let [Warning::LocatorUpdateFailed(failure)] = warnings.as_slice() else {
        panic!("the rewrite that failed is the one warning: {warnings:?}");
    };
    let Failure::CommitFailedCleanupIncomplete {
        failure: initiating,
        cleanup_failures,
    } = failure
    else {
        panic!("the stage it could not take back is reported: {failure:?}");
    };
    assert_eq!(
        initiating.location.absolute_path, record,
        "the warning names the record that still carries the locator from before the move"
    );
    // Beside, not instead of: the caller learns why the rewrite did not happen and,
    // separately, that the attempt left a file it could not take back.
    let [cleanup] = cleanup_failures.as_slice() else {
        panic!("one stage is left behind: {cleanup_failures:?}");
    };
    assert_eq!(
        cleanup.location.absolute_path, stage,
        "the report names a file that really is there: {cleanup:?}"
    );
    assert_eq!(
        warning_entries(&warnings[0]),
        [
            (
                crate::SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED,
                spelled(&initiating.location)
            ),
            (
                crate::SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED,
                spelled(&cleanup.location)
            ),
        ],
        "the interface reports the stage left behind beside the warning"
    );
    assert_eq!(
        stored_locator(&removed),
        "../../..",
        "the record is left exactly as the move found it"
    );
}

#[test]
fn a_post_move_index_failure_stays_committed_and_is_not_a_retry_signal() {
    let (_tree, root, directory) = default_package("removal-index-failure");
    let removed = root.join("Removed").join(BASENAME);
    let before = contents(&directory);
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(plan, &effects(&index_failing())).result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed index refresh remains a committed removal: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    // The removal facts are complete even though the index could not be rebuilt.
    assert_eq!(
        variant!(&result, ChangeResult::RemoveOrder)
            .final_location
            .absolute_path,
        removed
    );

    // The move already happened, so this warning is never a reason to retry the
    // removal: the Order is no longer where a retry would look for it.
    assert!(!directory.exists());
    assert_eq!(contents(&removed), before);
    let retried = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    );
    assert!(
        retried.result.is_err(),
        "retrying the removal finds no Order to remove"
    );
}

/// The reason describes what left the Orders Directory: the whole Order.
#[test]
fn a_removal_the_refresh_cannot_confirm_still_names_what_changed() {
    let (_tree, _root, directory) = default_package("removal-unconfirmed-refresh");

    // The Package cannot be listed at all after the Order has gone.
    let plan = crate::plan(
        &Change::RemoveOrder {
            order: OrderRef::Directory(directory.clone()),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(
        plan,
        &effects(&breaking_refresh_once_gone(
            directory.clone(),
            RefreshBreakage::ListingFails,
        )),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the Order has already gone, so is committed: {report:?}");
    };
    assert!(
        matches!(warnings.as_slice(), [Warning::IndexRefreshFailed(_)]),
        "{warnings:?}"
    );
    assert!(!directory.exists(), "really disposed of it");
}

#[test]
fn a_removal_the_refresh_cannot_assess_at_all_still_names_what_changed() {
    let (_tree, _root, directory) = default_package("removal-unassessable-package");

    // The Package's own version marker stops parsing once the Order has gone, so
    // the refresh comes back with no current Package to inspect at all.
    let plan = crate::plan(
        &Change::RemoveOrder {
            order: OrderRef::Directory(directory.clone()),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(
        plan,
        &effects(&breaking_refresh_once_gone(
            directory.clone(),
            RefreshBreakage::PackageDamaged,
        )),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the Order has already gone, so is committed: {report:?}");
    };
    assert!(
        matches!(warnings.as_slice(), [Warning::RefreshUnavailable]),
        "{warnings:?}"
    );
    assert!(!directory.exists(), "really disposed of it");
}
