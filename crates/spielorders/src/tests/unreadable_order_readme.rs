//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries.

use crate::Failure;
use crate::Package;
use crate::tests::support::{
    FixedUuid, Index, Operation, PlatformFileSystem, RefreshBreakage, ScriptedFileSystem,
    TemporaryTree, adoption, align_order, breaking_refresh_after, committed, edit_order, effects,
    initialize, opened, rebuild_index, record_vanishing_after_adoption, remove_order,
};
use crate::{Assignments, ChangeResult, Report};

use crate::{
    canonical::CurrentPackageCandidate, canonical::PackageIndexStaleReason,
    canonical::PackageIndexState,
};

use crate::readme::DiagnosticCode;
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
/// Deliberately not empty, and not what an empty README would produce, so a
/// projection that collapsed the two states is visible in the assertion.
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";

/// A Package holding one adopted Order, with the derived index removed so the next
/// load must read the README rather than serve a cached copy of its text.
fn package_with_unindexed_order(label: &str, readme: Option<&[u8]>) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = crate::plan(&initialize(tree.path()), &effects(&PlatformFileSystem))
        .result
        .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = fs::canonicalize(tree.path()).expect("the test Package root resolves");
    let directory = root.join("Orders").join("2026_08_22_Laser_components");
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    if let Some(readme) = readme {
        fs::write(directory.join("README.md"), readme).expect("the fixture README is written");
    }
    let plan = crate::plan(
        &adoption(
            root.clone(),
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
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(FixedUuid(ORDER_ID.into()))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    fs::remove_file(root.join(".spielorders/index.json")).expect("the derived index is removed");
    (tree, root)
}

fn order_directory(root: &Path) -> PathBuf {
    root.join("Orders").join("2026_08_22_Laser_components")
}

fn diagnostic_codes(package: &CurrentPackageCandidate) -> Vec<DiagnosticCode> {
    package
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn available(load: Result<Package, Failure>) -> Package {
    match load {
        Ok(package) => package,
        other => panic!("a README problem never damages the Package: {other:?}"),
    }
}

/// Rebuilds the derived index of the Package at `root`, returning the README text it
/// records for the Package's one Order.
fn rebuilt_index(root: &Path) -> String {
    committed(
        &rebuild_index(root.to_path_buf()),
        &effects(&PlatformFileSystem),
    );
    let stored = fs::read(root.join(".spielorders/index.json")).expect("a rebuild writes an index");
    Index::from_json_bytes(&stored)
        .expect("the rebuilt index parses")
        .orders[0]
        .readme
        .clone()
}

/// Real effects, except that every Order README read fails.
fn unreadable_readme() -> ScriptedFileSystem {
    ScriptedFileSystem::new().failing_ending_in(Operation::Read, "README.md")
}

/// An unreadable README, and the Order record removed by something outside
/// SpielOrders the moment an adoption links it into place, before its refresh.
fn vanishing_after_adopt() -> ScriptedFileSystem {
    let filesystem = unreadable_readme();
    record_vanishing_after_adoption(&filesystem);
    filesystem
}

/// An unreadable README, and the refresh after a mutation's decisive replacement or
/// rename failing its listing.
fn failing_the_refresh_load() -> ScriptedFileSystem {
    let filesystem = unreadable_readme();
    let listing = RefreshBreakage::ListingFails;
    breaking_refresh_after(
        &filesystem,
        Operation::ReplaceFile,
        ".spielorders/order.json",
        listing,
    );
    breaking_refresh_after(&filesystem, Operation::RenameNew, "", listing);
    filesystem
}

#[test]
fn a_readme_that_cannot_be_read_is_projected_as_unknown_rather_than_empty() {
    let (_tree, root) = package_with_unindexed_order("readme-unknown", Some(README));

    let package = available(opened(&root, &unreadable_readme()).result);

    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("an unreadable README leaves one healthy Order")
    };
    assert_eq!(
        order.readme_text.as_deref(),
        None,
        "an Order README that exists but could not be read carries no text"
    );
    assert_eq!(
        diagnostic_codes(&package.contents),
        vec![DiagnosticCode::ReadmeUnavailable],
        "the load still reports the README diagnostic it has always reported"
    );
    assert_eq!(
        fs::read(order_directory(&root).join("README.md")).unwrap(),
        README,
        "reading an Order never writes its README"
    );
}

#[test]
fn an_absent_readme_stays_distinguishable_from_one_that_could_not_be_read() {
    let (_tree, root) = package_with_unindexed_order("readme-absent", None);

    let package = available(opened(&root, &PlatformFileSystem).result);

    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("an absent README leaves one healthy Order")
    };
    assert_eq!(
        order.readme_text.as_deref(),
        Some(""),
        "an absent Order README reads as an empty one, which is a text this load knows"
    );
    assert!(
        diagnostic_codes(&package.contents).is_empty(),
        "an absent README is ordinary and carries no diagnostic"
    );
}

#[test]
fn an_unreadable_readme_is_not_recorded_in_the_index_as_an_empty_one() {
    let (_tree, root) = package_with_unindexed_order("readme-index", Some(README));
    let index_path = root.join(".spielorders/index.json");

    let package = available(opened(&root, &unreadable_readme()).result);

    assert!(
        !index_path.exists(),
        "a load that could not read an Order README writes no index claiming its text"
    );
    assert_eq!(
        package.contents.index.state,
        PackageIndexState::Missing,
        "the index the load declined to write is reported as it was found"
    );
}

#[test]
fn a_stale_stored_index_is_left_intact_rather_than_overwritten_with_an_empty_readme() {
    let (_tree, root) = package_with_unindexed_order("readme-index-intact", Some(README));
    let index_path = root.join(".spielorders/index.json");
    let stored = {
        assert_eq!(rebuilt_index(&root), std::str::from_utf8(README).unwrap());
        fs::read(&index_path).expect("a readable README produces an index")
    };
    // Whatever made the README unreadable changed it after the index was written, as a
    // permission change does, so the load must read it rather than trust the cache.
    fs::write(
        root.join("Orders/2026_08_22_Laser_components/README.md"),
        README,
    )
    .unwrap();
    let package = available(opened(&root, &unreadable_readme()).result);

    assert_eq!(
        fs::read(&index_path).unwrap(),
        stored,
        "the index a readable load wrote is left exactly as it was"
    );
    assert_eq!(
        package.contents.index.state,
        PackageIndexState::Stale,
        "an index this load could not refresh is reported stale rather than current"
    );
    assert_eq!(
        package.contents.index.stale_reasons,
        [PackageIndexStaleReason::ChangeEvidenceUncertain],
        "unreadable text has unknown freshness"
    );
    assert_eq!(
        package.contents.healthy_orders[0].readme_text, None,
        "the load reports what it could read, not what the stale cache still holds"
    );
    let index = Index::from_json_bytes(&stored).expect("the stored index parses");
    assert_eq!(
        index.orders[0].readme,
        std::str::from_utf8(README).unwrap(),
        "the text a load actually read stays in the index"
    );
}

#[test]
fn an_unreadable_readme_is_never_itself_the_evidence_that_the_stored_text_changed() {
    let (_tree, root) = package_with_unindexed_order("readme-stale-reason", Some(README));
    let stored = {
        assert_eq!(
            rebuilt_index(&root),
            std::str::from_utf8(README).unwrap(),
            "the index a rebuild writes holds the text it read"
        );
        fs::read(root.join(".spielorders/index.json")).expect("a readable rebuild writes an index")
    };
    // A known record change and an unreadable README have distinct evidence.
    let record = order_directory(&root).join(".spielorders/order.json");
    let prior = fs::read_to_string(&record).unwrap();
    let changed = prior.replace("Laser components", "Lenses");
    assert_ne!(prior, changed);
    fs::write(&record, changed).unwrap();

    let package = available(opened(&root, &unreadable_readme()).result);

    assert_eq!(
        package.contents.index.stale_reasons,
        [
            PackageIndexStaleReason::OrderMetadataChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain
        ],
        "a README this load could not read adds no reason of its own — reporting \
         order_readme_changed here would name a comparison the load never made"
    );
    assert_eq!(package.contents.healthy_orders[0].readme_text, None);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        stored,
        "and the index is still not rewritten"
    );
}

#[test]
fn an_unreadable_readme_never_blocks_an_adoption_that_cannot_be_confirmed() {
    let tree = TemporaryTree::new("readme-adopt");
    let plan = crate::plan(&initialize(tree.path()), &effects(&PlatformFileSystem))
        .result
        .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = fs::canonicalize(tree.path()).expect("the test Package root resolves");
    let directory = root.join("Orders").join("Legacy_laser_purchase");
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).expect("the fixture README is written");
    let plan = crate::plan(
        &adoption(
            root.clone(),
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
    .expect("the directory can be adopted");

    let report = crate::commit(
        plan,
        &effects(&vanishing_after_adopt()).minting(FixedUuid(ORDER_ID.into())),
    )
    .result;

    let Ok(Report::CommittedWithWarning { .. }) = report else {
        panic!("an adoption whose refresh cannot confirm it is still committed: {report:?}")
    };
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        README,
        "adoption never writes a README the directory already held"
    );
}

#[test]
fn once_the_readme_reads_again_every_later_load_reports_its_text() {
    let (_tree, root) = package_with_unindexed_order("readme-recovers", Some(README));

    let unreadable = available(opened(&root, &unreadable_readme()).result);
    assert_eq!(unreadable.contents.healthy_orders[0].readme_text, None);

    let recovered = available(opened(&root, &PlatformFileSystem).result);

    assert_eq!(
        recovered.contents.healthy_orders[0].readme_text.as_deref(),
        Some(std::str::from_utf8(README).unwrap()),
        "once the README reads again, the load reports its text"
    );
    rebuilt_index(&root);
    let again = available(opened(&root, &PlatformFileSystem).result);
    assert_eq!(
        again.contents.healthy_orders[0].readme_text.as_deref(),
        Some(std::str::from_utf8(README).unwrap()),
        "a load that proves the index current serves the text that index actually holds"
    );
    assert_eq!(again.contents.index.state, PackageIndexState::Current);
}

#[test]
fn an_unreadable_readme_never_blocks_an_order_edit() {
    let (_tree, root) = package_with_unindexed_order("readme-edit", Some(README));
    let directory = order_directory(&root);

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&unreadable_readme()).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");
    // The record is replaced and the Package then fails to reload, so the report is
    // the one the edit projects for itself. A refreshed load would supply its own
    // Order instead of consulting that projection.
    let report = crate::commit(plan, &effects(&failing_the_refresh_load())).result;

    let (Ok(Report::Committed { .. }) | Ok(Report::CommittedWithWarning { .. })) = report else {
        panic!("an unreadable README never blocks an Order edit: {report:?}")
    };
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        README,
        "an Order edit never writes the README"
    );
}

#[test]
fn an_unreadable_readme_never_blocks_an_alignment_and_moves_with_it() {
    let (_tree, root) = package_with_unindexed_order("readme-align", Some(README));
    let directory = order_directory(&root);
    // Drift the directory name so the alignment has a rename to make.
    let drifted = root.join("Orders").join("Laser_components_drifted");
    fs::rename(&directory, &drifted).expect("the fixture Order Directory drifts");

    let plan = crate::plan(
        &align_order(drifted.clone()),
        &effects(&unreadable_readme()),
    )
    .result
    .expect("the alignment plans");
    // The rename succeeds and the Package then fails to reload.
    let report = crate::commit(plan, &effects(&failing_the_refresh_load())).result;

    let (Ok(Report::Committed { result, .. }) | Ok(Report::CommittedWithWarning { result, .. })) =
        report
    else {
        panic!("an unreadable README never blocks an alignment: {report:?}")
    };
    let aligned = &variant!(&result, ChangeResult::AlignOrder)
        .directory
        .absolute_path;
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
}

#[test]
fn an_unreadable_readme_never_blocks_a_removal_and_moves_with_it() {
    let (_tree, root) = package_with_unindexed_order("readme-remove", Some(README));
    let directory = order_directory(&root);

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&unreadable_readme()),
    )
    .result
    .expect("the removal plans");
    let report = crate::commit(plan, &effects(&unreadable_readme())).result;

    let (Ok(Report::Committed { result, .. }) | Ok(Report::CommittedWithWarning { result, .. })) =
        report
    else {
        panic!("an unreadable README never blocks a removal: {report:?}")
    };
    let moved = variant!(&result, ChangeResult::RemoveOrder)
        .final_location
        .absolute_path
        .as_path();
    assert_eq!(
        fs::read(moved.join("README.md")).unwrap(),
        README,
        "a removal moves the README with its directory and never writes it"
    );
}
