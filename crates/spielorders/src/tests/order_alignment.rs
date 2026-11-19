//! Decision sources: Define the SpielOrders package topology; Define the Order
//! record and lifecycle contract; Define Vendor identity and historical semantics;
//! Define the Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries.

use crate::tests::support::assert_reported_beside;
use crate::tests::support::committed_order;
use crate::tests::support::index_is_current;
use crate::tests::support::operation_path;
use crate::tests::support::{
    Answer, FailedCommit, FixedUuid, Index, Operation, PlatformFileSystem, RefreshBreakage,
    ScriptedFileSystem, TemporaryTree, adoption, align_order, breaking_refresh_after,
    created_vendor, edit_order, edit_vendor, effects, operation_kinds, operations, order_at,
    package_tree,
};
use crate::{Assignments, Report, VendorRef};
use std::ops::Deref;

use crate::metadata::OrderStatus;
use crate::{evidence::EvidenceOutcome, evidence::FactChange};

use crate::ports::RenameOutcome;

use crate::{Failure, Warning};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
/// A second Order identity, for the Order left standing where another Order was
/// planned. Everything but the identity matches, so only the identity can refuse.
const SUBSTITUTE_ORDER_ID: &str = "3f5c1d84-2b90-4a6e-8c17-6d0b95e4a731";
/// A deliberately drifted directory name, so alignment always has work to do.
const DRIFTED: &str = "Legacy_laser_purchase";

/// Adopts one Order at a drifted directory name, optionally associating a Vendor and
/// an Orderer, so alignment has to project from exactly those facts.
fn drifted_order(
    root: &Path,
    basename: &str,
    vendor: Option<&str>,
    orderer: Option<&str>,
) -> PathBuf {
    let directory = root.join("Orders").join(basename);
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    let mut input = (
        Assignments::new()
            .assign("title", "Laser components")
            .assign("date_ordered", "2026-08-22")
            .assign("status", "hold"),
        Vec::new(),
    );
    if let Some(vendor) = vendor {
        input.0 = input.0.assign("vendor", vendor);
    }
    if let Some(orderer) = orderer {
        input.0 = input.0.introduce("orderer", orderer);
    }
    let plan = crate::plan(
        &adoption(root, &directory, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(ORDER_ID.into())),
    )
    .result;
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "the fixture Order commits: {report:?}"
    );
    directory
}

#[test]
fn the_projection_enumerates_every_optional_component_combination() {
    // The decided basename is `YYYY_MM_DD[_Vendor]_Title[_Orderer]`, with the Vendor
    // and Orderer components omitted when absent. That is four presence cases, and a
    // fixture carrying both could not separate a correct projection from one that
    // mishandles an absent component or emits an empty one.
    for (label, with_vendor, orderer, expected) in [
        ("neither", false, None, "2026_08_22_Laser_components"),
        (
            "vendor only",
            true,
            None,
            "2026_08_22_Tarnwick_Laser_components",
        ),
        (
            "orderer only",
            false,
            Some("Ada Lovelace"),
            "2026_08_22_Laser_components_Ada_Lovelace",
        ),
        (
            "both",
            true,
            Some("Ada Lovelace"),
            "2026_08_22_Tarnwick_Laser_components_Ada_Lovelace",
        ),
    ] {
        let tree = package_tree("alignment-projection");
        let root = &fs::canonicalize(tree.path()).unwrap();
        let name = with_vendor.then(|| {
            created_vendor(root, "Tarnwick", VENDOR_ID);
            "Tarnwick"
        });
        let directory = drifted_order(root, DRIFTED, name, orderer);

        let plan = crate::plan(
            &align_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap_or_else(|error| panic!("{label} can be aligned: {error:?}"));

        assert_eq!(
            operation_path(&plan, "rename_order_directory")
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            expected,
            "{label}"
        );
        assert_eq!(
            operation_path(&plan, "rename_order_directory"),
            root.join("Orders").join(expected),
            "{label}"
        );
        assert_eq!(
            operation_kinds(&plan),
            ["rename_order_directory", "refresh_package_index",],
            "{label}"
        );
        // Planning renames nothing.
        assert!(directory.is_dir(), "{label}");
        assert!(!root.join("Orders").join(expected).exists(), "{label}");
    }
}

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const ALIGNED: &str = "2026_08_22_Tarnwick_Laser_components_Ada_Lovelace";

/// A drifted Order carrying a README and two opaque Supporting Material sentinels,
/// one nested, so a rename can be shown to move the whole tree intact.
fn surrounded_drifted_order(label: &str) -> (TemporaryTree, PathBuf, String) {
    let tree = package_tree(label);
    let root = fs::canonicalize(tree.path()).unwrap();
    let vendor = created_vendor(&root, "Tarnwick", VENDOR_ID);
    let directory = root.join("Orders").join(DRIFTED);
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();

    let plan = crate::plan(
        &adoption(
            root.clone(),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold")
                    .assign("vendor", "Tarnwick")
                    .introduce("orderer", "Ada Lovelace"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(ORDER_ID.into())),
    )
    .result;
    assert!(matches!(report, Ok(Report::Committed { .. })));
    (tree, directory, vendor)
}

fn indexed_directories(root: &Path) -> Vec<String> {
    let index = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the derived index parses");
    index
        .orders
        .iter()
        .map(|order| order.order_directory.as_str().to_owned())
        .collect()
}

#[test]
fn committing_renames_the_whole_tree_and_leaves_the_order_resolvable_at_its_new_name() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-commit");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    assert_eq!(
        indexed_directories(&root),
        [format!("Orders/{DRIFTED}")],
        "the index starts at the drifted name"
    );

    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be aligned");
    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("an explicit alignment commits: {report:?}");
    };
    assert!(!directory.exists(), "the drifted name is gone");
    assert!(aligned.is_dir());
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        aligned,
        "the result reports the Order at its new path"
    );

    // Identity and every domain fact survive a rename; the path is location, not
    // identity.
    let document = &committed_order(&result).document;
    assert_eq!(document.order_id.deref(), ORDER_ID);
    assert_eq!(document.title.deref(), "Laser components");
    assert_eq!(document.date_ordered.deref(), "2026-08-22");
    assert_eq!(document.status, OrderStatus::Hold);
    assert_eq!(document.vendor_id.as_deref(), Some(vendor.as_str()));
    assert_eq!(document.orderer.as_deref(), Some("Ada Lovelace"));

    // The technical locator still resolves from the new location, and the derived
    // index names the new directory. A rename that left either stale would produce
    // exactly the Damaged Order the integrity decision describes.
    assert_eq!(indexed_directories(&root), [format!("Orders/{ALIGNED}")]);
    assert!(index_is_current(&root));
    let reloaded = crate::plan(&align_order(aligned.clone()), &effects(&PlatformFileSystem))
        .result
        .expect("the Order still resolves through its own locator at the new path");
    assert!(reloaded.operations().is_empty(), "the Order is now aligned");

    // The whole directory moved as one opaque tree.
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(aligned.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(aligned.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );
}

#[test]
fn an_already_aligned_order_is_unchanged_and_renames_nothing() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-no-op");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let plan = crate::plan(&align_order(aligned.clone()), &effects(&PlatformFileSystem))
        .result
        .expect("an aligned Order is planned like any other");

    assert!(
        operations(&plan).is_empty(),
        "an alignment that renames nothing announces no managed operation"
    );

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Unchanged { result, .. }) = report else {
        panic!("an already aligned Order is an accepted no-op: {report:?}");
    };
    assert_eq!(committed_order(&result).document.order_id.deref(), ORDER_ID);
    assert!(aligned.is_dir());
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a no-op alignment rebuilds no index"
    );
}

#[test]
fn a_collision_stops_alignment_without_inventing_a_suffix() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-collision");
    let root = fs::canonicalize(tree.path()).unwrap();
    let occupied = root.join("Orders").join(ALIGNED);
    // Something already stands at the projected name.
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("sibling.txt"), b"someone else's directory\n").unwrap();
    let record_before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let error = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a collision stops alignment");

    let Failure::DestinationCollision { location, .. } = &error else {
        panic!("a collision is reported as such: {error:?}");
    };
    assert_eq!(location.absolute_path, occupied);
    assert!(
        location.package_relative_path.is_some(),
        "a refused destination inside the Package carries the Package's spelling for it"
    );

    // Nothing moved, nothing was replaced, and no suffixed variant was invented.
    assert!(directory.is_dir(), "the drifted name is retained");
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        record_before
    );
    assert_eq!(
        fs::read(occupied.join("sibling.txt")).unwrap(),
        b"someone else's directory\n",
        "the colliding sibling is untouched"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
    let names = fs::read_dir(root.join("Orders"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        [ALIGNED.to_owned(), DRIFTED.to_owned()]
            .into_iter()
            .collect(),
        "no suffixed variant such as _2 was created"
    );
}

#[test]
fn an_injected_rename_failure_leaves_the_whole_source_untouched() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-rename-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&ScriptedFileSystem::new().failing_ending_in(Operation::RenameNew, "")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("an injected rename failure rejects: {report:?}");
    };
    let failed = rejection;
    assert_reported_beside(&failed);
    assert_eq!(failed.failure().location.absolute_path, aligned);
    assert!(directory.is_dir(), "the source keeps its drifted name");
    assert!(!aligned.exists(), "no partial destination is left behind");
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(directory.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_destination_taken_between_planning_and_commit_stops_the_rename() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-collision-race");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The rename itself is the collision check, so a destination that appears after
    // planning is refused by the rename rather than overwritten.
    let report = crate::commit(
        plan,
        &effects(&{
            let filesystem = ScriptedFileSystem::new();
            filesystem
                .on(Operation::RenameNew)
                .answers(Answer::Renamed(RenameOutcome::DestinationExists));
            filesystem
        }),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a taken destination rejects: {report:?}");
    };
    let Failure::DestinationCollision { location, .. } = &rejection else {
        panic!("a taken destination is a collision: {rejection:?}");
    };
    assert_eq!(location.absolute_path, root.join("Orders").join(ALIGNED));
    assert_eq!(
        location.package_relative_path.as_deref(),
        Some(format!("Orders/{ALIGNED}").as_str()),
        "a refusal names the destination in the Package's own spelling"
    );
    assert!(directory.is_dir());
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
}

#[test]
fn a_source_that_disappeared_or_became_a_link_rejects_before_renaming() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-source-vanished");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    fs::remove_dir_all(&directory).unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Err(rejection) = report else {
        panic!("a vanished source rejects: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::PreconditionsChanged { .. }),
        "a vanished source is a precondition change: {rejection:?}"
    );
    assert!(!root.join("Orders").join(ALIGNED).exists());

    // A source replaced by a link is not the Order Directory this plan validated.
    #[cfg(unix)]
    {
        let (tree, directory, _vendor) = surrounded_drifted_order("alignment-source-link");
        let root = fs::canonicalize(tree.path()).unwrap();
        let plan = crate::plan(
            &align_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();
        let elsewhere = root.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::remove_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &directory).unwrap();

        let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

        assert!(
            matches!(report, Err(Failure::PreconditionsChanged { .. })),
            "a linked source is a precondition change: {report:?}"
        );
        assert!(!root.join("Orders").join(ALIGNED).exists());
        assert!(fs::symlink_metadata(&directory).unwrap().is_symlink());
    }
}

#[test]
fn editing_order_facts_never_renames_the_directory_implicitly() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-not-implicit");
    let root = fs::canonicalize(tree.path()).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    // Changing a projected fact makes the drift larger, and that is allowed state.
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    assert!(
        directory.is_dir(),
        "a fact edit renames no directory on its own"
    );
    assert_eq!(directory.file_name().unwrap(), DRIFTED);
    assert_ne!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "the edit itself did refresh the index"
    );
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);

    // Only the explicit action renames, and it projects from the edited facts.
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert_eq!(
        operation_path(&plan, "rename_order_directory")
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "2026_08_22_Tarnwick_Laser_mounts_Ada_Lovelace"
    );
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert!(
        root.join("Orders")
            .join("2026_08_22_Tarnwick_Laser_mounts_Ada_Lovelace")
            .is_dir()
    );
}

#[test]
fn a_post_commit_index_failure_is_a_warning_rather_than_a_rename_retry_signal() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-index-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(
            &ScriptedFileSystem::new()
                .failing_ending_in(Operation::ReplaceFile, ".spielorders/index.json"),
        ),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    assert_eq!(committed_order(&result).document.order_id.deref(), ORDER_ID);
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        aligned,
        "the result reports the Order where the rename actually left it"
    );
    // A projection built without a refreshed Package still owes the caller the
    // complete Order, README included, rather than an empty document.
    assert_eq!(
        committed_order(&result)
            .readme_text
            .as_deref()
            .map(str::as_bytes),
        Some(README),
        "the aligned Order carries its real README"
    );

    // The rename already happened. Retrying the alignment would find the Order
    // aligned and do nothing, so the warning must not read as a rename retry signal.
    assert!(aligned.is_dir());
    assert!(!directory.exists());
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
    let retried = crate::plan(&align_order(aligned.clone()), &effects(&PlatformFileSystem))
        .result
        .expect("the renamed Order still resolves");
    assert!(operations(&retried).is_empty());
}

#[test]
fn a_report_built_without_a_refreshed_package_still_carries_the_orders_readme() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-refresh-load-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The rename succeeds and the Package then fails to reload, so this report is
    // projected locally. It is the one path that cannot borrow a refreshed Order.
    let report = crate::commit(
        plan,
        &effects(&breaking_refresh_after_rename(
            RefreshBreakage::ListingFails,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    assert_eq!(
        committed_order(&result)
            .readme_text
            .as_deref()
            .map(str::as_bytes),
        Some(README),
        "the aligned Order still carries its real README"
    );
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        aligned,
        "and reports it at the path the rename produced"
    );
    assert_eq!(committed_order(&result).document.order_id.deref(), ORDER_ID);
    assert!(aligned.is_dir());
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
}

#[test]
fn a_completed_rename_is_never_reported_as_work_that_did_not_happen() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-undiscoverable");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The rename succeeds, the Package still loads, but this Order's record stops
    // parsing before the refresh, so it comes back as a Damaged Order and is absent
    // from the healthy set the report would otherwise borrow from.
    let report = crate::commit(
        plan,
        &effects(&breaking_refresh_after_rename(
            RefreshBreakage::RecordDamaged,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("the directory was renamed on disk, so the report is committed: {report:?}");
    };
    assert_eq!(warnings, [Warning::RefreshUnavailable]);
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        aligned,
        "the report names the path the rename actually produced"
    );
    assert_eq!(
        committed_order(&result)
            .readme_text
            .as_deref()
            .map(str::as_bytes),
        Some(README)
    );
    // The rename is real on disk, which is exactly why it must not be reported as a
    // rejection: a caller told "nothing happened" would be misled.
    assert!(aligned.is_dir());
    assert!(!directory.exists());
}

#[test]
fn a_rename_the_refresh_cannot_assess_at_all_still_names_what_changed() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-unassessable-package");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The rename succeeds and only then does the Package's own version marker stop
    // parsing, so the refresh comes back with no current Package to inspect. This is
    // the one arm no other test reaches, and it is where a stale reason is easiest to
    // get wrong precisely because nothing was observing it.
    let report = crate::commit(
        plan,
        &effects(&breaking_refresh_after_rename(
            RefreshBreakage::PackageDamaged,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("the directory was renamed on disk, so the report is committed: {report:?}");
    };
    assert_eq!(warnings, [Warning::RefreshUnavailable]);
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        aligned,
        "the report names the path the rename actually produced"
    );
    assert!(aligned.is_dir());
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// A refusal is only useful if it names *which* fact moved. The rejection variant
/// alone would be the same unexplained precondition failure for a substituted Order,
/// an edited title, and a renamed Vendor.
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

#[test]
fn a_different_order_left_at_the_source_path_is_refused_though_it_is_still_a_directory() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-substituted-order");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The planned Order is gone and a different Order stands at exactly the same
    // path, carrying the same projecting facts. Every check the commit could make
    // about the *path* still passes: it is present, it is an ordinary directory, and
    // it is a healthy Order of this Package. Only its identity differs, and renaming
    // it would apply one Order's authorized alignment to another Order's directory.
    fs::remove_dir_all(&directory).unwrap();
    fs::create_dir(&directory).unwrap();
    let substituted = crate::plan(
        &adoption(
            root.clone(),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold")
                    .assign("vendor", "Tarnwick")
                    .introduce("orderer", "Ada Lovelace"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the substituted directory can be adopted");
    assert!(matches!(
        crate::commit(
            substituted,
            &effects(&PlatformFileSystem).minting(FixedUuid(SUBSTITUTE_ORDER_ID.into()))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    assert!(
        fs::symlink_metadata(&directory).unwrap().is_dir(),
        "the source is still an ordinary directory, so existence alone cannot refuse"
    );

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [("order_id", FactChange::Differs)],
        "the refusal names the identity that moved, not the path"
    );
    // Nothing was renamed, and the Order that happens to occupy the path keeps its
    // own name and its own record.
    assert!(!aligned.exists(), "no rename occurred");
    assert!(directory.is_dir());
    let resident = order_at(&directory, &PlatformFileSystem)
        .result
        .expect("the substituted Order is untouched and still reads on its own terms");
    assert_eq!(
        resident.order.document.order_id.deref(),
        SUBSTITUTE_ORDER_ID
    );
}

#[test]
fn editing_a_basename_governing_fact_after_planning_is_refused() {
    // Every fact the aligned basename projects from, and the fact name the refusal is
    // expected to give for it. A test that only moved the title would leave the
    // others free to change unnoticed between planning and the rename. Orderer is a
    // Reusable Free-Entry Value, so a genuinely new one is introduced rather than
    // assigned; that is the confirmation, not a different kind of edit.
    for (label, field, value, reusable, expected) in [
        (
            "alignment-stale-title",
            "title",
            "Laser mounts",
            false,
            [
                ("destination", FactChange::Differs),
                ("title", FactChange::Differs),
            ],
        ),
        (
            "alignment-stale-date",
            "date_ordered",
            "2026-08-23",
            false,
            [
                ("date_ordered", FactChange::Differs),
                ("destination", FactChange::Differs),
            ],
        ),
        (
            "alignment-stale-orderer",
            "orderer",
            "Grace Hopper",
            true,
            [
                ("destination", FactChange::Differs),
                ("orderer", FactChange::Differs),
            ],
        ),
    ] {
        let (tree, directory, _vendor) = surrounded_drifted_order(label);
        let root = fs::canonicalize(tree.path()).unwrap();
        let plan = crate::plan(
            &align_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();
        let planned_destination = operation_path(&plan, "rename_order_directory");

        // An ordinary fact edit. It deliberately does not rename the directory, so
        // the source the plan recorded is still exactly where it was — and the name
        // the plan would rename it to is no longer the one these facts project.
        let assignments = if reusable {
            Assignments::new().introduce(field, value)
        } else {
            Assignments::new().assign(field, value)
        };
        let edit = crate::plan(
            &edit_order(directory.clone(), assignments.clone()),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .expect("the fact can be edited");
        assert!(matches!(
            crate::commit(edit, &effects(&PlatformFileSystem)).result,
            Ok(Report::Committed { .. })
        ));

        let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

        assert_eq!(
            unconfirmed_facts(&report),
            expected,
            "editing {field} after planning is refused and named"
        );
        assert!(
            !planned_destination.exists(),
            "{field}: the stale projected name was never created"
        );
        assert!(directory.is_dir(), "{field}: the source is untouched");
        assert_eq!(
            indexed_directories(&root),
            [format!("Orders/{DRIFTED}")],
            "{field}: the derived index still names the drifted directory"
        );
    }
}

#[test]
fn clearing_the_vendor_reference_after_planning_is_refused() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-cleared-vendor");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let planned_destination = operation_path(&plan, "rename_order_directory");

    // Clearing the association drops the Vendor component from the projection
    // entirely, so the plan's name is wrong in a way no Vendor *value* comparison
    // would notice.
    let edit = crate::plan(
        &edit_order(directory.clone(), Assignments::new().assign("vendor", "")),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the Vendor Reference can be cleared");
    assert!(matches!(
        crate::commit(edit, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_id", FactChange::Vanished),
            ("vendor_name", FactChange::Vanished),
        ],
        "a cleared association is named as a fact that vanished, not one that differs"
    );
    assert!(!planned_destination.exists());
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);
}

#[test]
fn associating_a_vendor_after_planning_is_refused() {
    let tree = package_tree("alignment-appeared-vendor");
    let root = fs::canonicalize(tree.path()).unwrap();
    // An Order with no Vendor at all, so its projection omits the Vendor component.
    let directory = drifted_order(&root, DRIFTED, None, Some("Ada Lovelace"));
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert_eq!(
        operation_path(&plan, "rename_order_directory")
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "2026_08_22_Laser_components_Ada_Lovelace"
    );
    let planned_destination = operation_path(&plan, "rename_order_directory");

    // The mirror of clearing a reference: a plan authorized against "this Order has no
    // Vendor" must not proceed once it has one, and the projection it would rename to
    // gains a whole component.
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let edit = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("vendor", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("a Vendor can be associated");
    assert!(matches!(
        crate::commit(edit, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_id", FactChange::Appeared),
            ("vendor_name", FactChange::Appeared),
        ],
        "an association that appeared is named as such, not as one that differs"
    );
    assert!(!planned_destination.exists());
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);
}

#[test]
fn renaming_the_vendor_after_planning_is_refused() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-stale-vendor-name");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let planned_destination = operation_path(&plan, "rename_order_directory");
    assert_eq!(planned_destination, root.join("Orders").join(ALIGNED));

    // An Order keeps no Vendor snapshot, so an ordinary Vendor rename changes the
    // name this Order projects while every Order fact, the Vendor ID, and the source
    // path all stand unchanged. This is the case a fact-only comparison cannot see.
    let rename = crate::plan(
        &edit_vendor(
            root.clone(),
            VendorRef::Id(String::from(&vendor)),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be renamed");
    assert!(matches!(
        crate::commit(
            rename,
            &effects(&PlatformFileSystem).minting(FixedUuid(VENDOR_ID.into()))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_name", FactChange::Differs),
        ],
        "the refusal names the live Vendor Name the projection resolved"
    );
    assert!(
        !planned_destination.exists(),
        "the name projected from the old Vendor Name was never created"
    );
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);

    // Planning again against the current Vendor Name succeeds: the refusal is about
    // this plan being stale, not about the Order having become unalignable.
    let replanned = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a fresh plan is authorized");
    assert_eq!(
        operation_path(&replanned, "rename_order_directory")
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "2026_08_22_Nethermoor_Laser_components_Ada_Lovelace"
    );
    assert!(matches!(
        crate::commit(replanned, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
}

#[test]
fn an_already_aligned_order_whose_facts_moved_is_refused_rather_than_reported_unchanged() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-stale-no-op");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    assert!(matches!(
        crate::commit(
            crate::plan(
                &align_order(directory.clone()),
                &effects(&PlatformFileSystem)
            )
            .result
            .unwrap(),
            &effects(&PlatformFileSystem)
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    // This plan renames nothing, so it has no rename to refuse. It still carries a
    // claim: that this Order's facts project the name it already has. Reporting
    // `Unchanged` after a title edit would tell the caller the Order is aligned at
    // the moment it stopped being aligned.
    let plan = crate::plan(&align_order(aligned.clone()), &effects(&PlatformFileSystem))
        .result
        .unwrap();
    assert!(operations(&plan).is_empty());
    let edit = crate::plan(
        &edit_order(
            aligned.clone(),
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
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("title", FactChange::Differs)
        ]
    );
    assert!(aligned.is_dir(), "a refused no-op renames nothing either");
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a refused no-op rebuilds no index"
    );
}

#[test]
fn a_source_that_stopped_being_a_healthy_order_is_refused_before_the_rename() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-damaged-source");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The path is still an ordinary directory holding the user's Supporting Material,
    // but its canonical record no longer parses, so nothing there confirms which
    // Order this is. A rename here would move a Damaged Order to the aligned name of
    // an Order nobody can now identify.
    fs::write(directory.join(".spielorders/order.json"), b"{ not json").unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Err(rejection) = report else {
        panic!("an unidentifiable source rejects: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::PreconditionsChanged { .. }),
        "a source that no longer resolves is a precondition change: {rejection:?}"
    );
    assert!(!aligned.exists(), "no rename occurred");
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
}

/// Renames for real, then breaks the refresh that follows, so the report has to be
/// built from the plan rather than from a refreshed Package.
fn breaking_refresh_after_rename(breakage: RefreshBreakage) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    breaking_refresh_after(&filesystem, Operation::RenameNew, "", breakage);
    filesystem
}
