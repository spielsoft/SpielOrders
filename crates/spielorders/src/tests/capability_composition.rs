//! Capabilities composed through public Rust-core seams, proving claims no single
//! capability owns.
//!
//! Decision sources: Define the SpielOrders package topology; Define Vendor identity
//! and historical semantics; Define the Rust CLI capability contract; Define
//! integrity, validation, and repair boundaries.

use crate::tests::support::{
    EditPoint, FailedCommit, Operation, PlatformFileSystem, QueuedUuids, ScriptedFileSystem,
    TemporaryTree, add_contact, add_item, adoption, align_order, another_writer_replacing,
    committed, create_order, create_vendor, current_readme, diagnosed_order, edit_order, effects,
    initialized_package, opened, order_scope, package_scope, package_tree_with, rebuild_index,
    record_path, remove_order, repair, set_vendor_hidden, valid, write_readme,
};
use crate::tests::support::{Index, index_is_current};
use crate::tests::support::{assert_reported_beside, operation_path};
use crate::tests::support::{committed_order, committed_vendor};
use crate::{Assignments, ChangeResult, Report, VendorRef};
use crate::{search::CaseSensitivity, search::SearchQuery, search::VendorVisibility};
use std::ops::Deref;

use crate::canonical::PackageIndexState;

use crate::metadata::OrderDocument;

use crate::readme::DiagnosticCode;

use crate::repair_cases::RepairAction;
use crate::{Failure, Warning};
use std::fs;
use std::path::{Path, PathBuf};

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const ORDER_README: &str = "# Optical bench\n\nWritten through the core.\n";
/// A well-formed Vendor ID that no Vendor in the Package carries.
const ABSENT_VENDOR: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const FIRST_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SECOND_ID: &str = "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f";
const THIRD_ID: &str = "5d7a9b03-2e14-4c86-b0f5-73a8c9e1d240";
const VENDOR_ID: &str = "5a2f1c88-9b30-4c17-a4de-2f6b7c8d9e01";
/// A configured Orders Directory that no caller could guess from the default spelling,
/// and a Removed Directory renamed alongside it.
const NESTED_ORDERS: &str = "Admin/Purchase Orders";
const RETIRED: &str = "Retired";

/// Where a capability's single injected failure lands. Every capability suite grows
/// its own double for the boundaries that capability owns; the gate composes failures
/// across capabilities, so it names only the boundaries composition crosses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    /// The staging write fails, so the capability refuses before any canonical byte
    /// is written and has nothing to undo.
    StagingWrite,
    /// Staging durability fails and the teardown that answers it fails too, so a
    /// retained staging file is reported beside the refusal.
    StagingDurabilityAndCleanup,
    /// The canonical write succeeds and the derived index refresh after it fails.
    IndexRefresh,
    /// A whole-directory move fails, and undoing the directory that attempt created
    /// fails as well.
    DirectoryMoveAndUndo,
}

const INDEX: &str = ".spielorders/index.json";
const INDEX_STAGE: &str = ".spielorders/.index.json.spielorders-stage";

/// A capability's own staging file, as distinct from the derived index's. The two are
/// separated so a staging boundary and an index boundary can never fire for each other.
fn is_capability_stage(path: &Path) -> bool {
    !path.ends_with(INDEX_STAGE)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".spielorders-stage"))
}

fn package_with_layout(label: &str, options: &(&str, &str)) -> (TemporaryTree, PathBuf) {
    let tree = package_tree_with(label, options);
    let root = fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

fn adopted_order(root: &Path, basename: &str, uuid: &str, vendor: Option<&str>) -> PathBuf {
    adopted_order_at(root, "Orders", basename, uuid, vendor)
}

fn adopted_order_at(
    root: &Path,
    orders_path: &str,
    basename: &str,
    uuid: &str,
    vendor: Option<&str>,
) -> PathBuf {
    let directory = legacy_directory(root, orders_path, basename);
    let plan = crate::plan(
        &adoption(root, &directory, &legacy_facts(vendor)),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[uuid]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    directory
}

/// An unmanaged directory carrying a user-authored README and two opaque sentinels,
/// ready for adoption.
fn legacy_directory(root: &Path, orders_path: &str, basename: &str) -> PathBuf {
    let directory = root.join(orders_path).join(basename);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();
    directory
}

fn legacy_facts(vendor: Option<&str>) -> (Assignments, Vec<Assignments>) {
    let facts = Assignments::new()
        .assign("title", "Laser components")
        .assign("date_ordered", "2026-08-22")
        .assign("status", "hold");
    match vendor {
        Some(vendor) => (facts.assign("vendor", vendor), Vec::new()),
        None => (facts, Vec::new()),
    }
}

fn vendors_record(root: &Path) -> PathBuf {
    root.join(".spielorders/vendors.json")
}

/// A complete, valid Order record for a Package whose Orders sit two directories deep.
fn nested_replacement_record(order_id: &str) -> Vec<u8> {
    format!(
        "{{\n  \"schema_version\": 1,\n  \"package_root\": \"../../..\",\n  \"order_id\": \"{order_id}\",\n  \"title\": \"Laser components\",\n  \"date_ordered\": \"2026-08-22\",\n  \"status\": \"hold\",\n  \"items\": []\n}}\n"
    )
    .into_bytes()
}

/// Both opaque sentinels plus the user-authored README, wherever the Order Directory
/// currently sits.
fn opaque_contents(directory: &Path) -> Vec<Vec<u8>> {
    vec![
        fs::read(directory.join("README.md")).expect("README"),
        fs::read(directory.join("quote.bin")).expect("sentinel a"),
        fs::read(directory.join("Quotes/drawing.bin")).expect("nested sentinel b"),
    ]
}

/// A real filesystem with exactly one boundary failing.
fn failing_at(boundary: Boundary) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    match boundary {
        Boundary::StagingWrite => filesystem
            .on(Operation::WriteNew)
            .matching(is_capability_stage)
            .once()
            .fails(),
        Boundary::StagingDurabilityAndCleanup => {
            filesystem
                .on(Operation::SyncFile)
                .matching(is_capability_stage)
                .once()
                .fails();
            filesystem
                .on(Operation::RemoveFile)
                .matching(is_capability_stage)
                .fails();
        }
        Boundary::IndexRefresh => filesystem
            .on(Operation::ReplaceFile)
            .ending_in(INDEX)
            .fails(),
        Boundary::DirectoryMoveAndUndo => {
            filesystem.on(Operation::RenameNew).fails();
            filesystem.on(Operation::RemoveDirectory).fails();
        }
    }
    filesystem
}

#[test]
fn an_unresolved_vendor_reference_is_diagnosed_and_repairable_beside_another_case() {
    // SpielOrders itself can no longer create an Unresolved Vendor Reference — a Vendor
    // is hidden, never deleted, so its ID stays resolvable forever. One can still arrive
    // by hand-editing, external corruption, or a legacy adoption carrying a bogus ID,
    // which is exactly how this manufactures it. This gate confirms the resulting state
    // is diagnosed and repairable rather than stranded, alongside a second condition, so
    // neither masks the other.
    let (_tree, root) = initialized_package("m3-unresolved-vendor");
    let first = adopted_order(&root, "2026_08_22_First", FIRST_ID, None);
    let second = adopted_order(&root, "2026_08_22_Second", SECOND_ID, None);
    let text = fs::read_to_string(record_path(&first)).unwrap().replace(
        "\"items\": []",
        &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
    );
    fs::write(record_path(&first), text).unwrap();
    // A second, independent condition on the same Order: a duplicate Order identity.
    let text = fs::read_to_string(record_path(&second))
        .unwrap()
        .replace(SECOND_ID, FIRST_ID);
    fs::write(record_path(&second), text).unwrap();

    // The reference is unresolved, and diagnosis says so with the same code the
    // Package-wide read model uses.
    let diagnosis = diagnosed_order(&first, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses");
    let case = diagnosis
        .cases
        .iter()
        .find(|case| case.code == DiagnosticCode::VendorReferenceUnresolved)
        .expect("the orphaned reference is diagnosed");
    assert_eq!(
        case.actions,
        [
            RepairAction::AssignExistingVendor,
            RepairAction::CreateAndAssignVendor,
            RepairAction::ClearVendorReference,
        ],
        "all three authorized resolutions are offered"
    );
    // Both conditions are diagnosed together rather than one masking the other, so
    // recovery is not two-step: scoped diagnosis narrows the exhaustive Package
    // validation, which reports every diagnostic at once.
    assert_eq!(
        diagnosis
            .cases
            .iter()
            .map(|case| case.code)
            .collect::<Vec<_>>(),
        [
            DiagnosticCode::OrderIdDuplicate,
            DiagnosticCode::VendorReferenceUnresolved,
        ],
        "the orphaned reference and the identity conflict are both visible at once"
    );

    let handle = case.handle.to_owned();
    let plan = crate::plan(
        &repair(
            order_scope(first.clone()),
            &handle,
            RepairAction::ClearVendorReference,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the orphaned reference can be repaired");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    // Resolving one case leaves the other standing and repairable, so no canonical
    // byte was lost and no state is stranded.
    let second_before = fs::read(record_path(&second)).unwrap();
    let reloaded = diagnosed_order(&first, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses again");
    assert_eq!(
        reloaded
            .cases
            .iter()
            .map(|case| case.code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::OrderIdDuplicate],
        "the reference is resolved and only the identity conflict remains"
    );
    assert!(
        reloaded
            .cases
            .iter()
            .any(|case| case.actions == [RepairAction::AdoptNewOrderIdentity]),
        "and it is itself repairable"
    );
    assert_eq!(
        fs::read(record_path(&second)).unwrap(),
        second_before,
        "the other conflict participant is untouched"
    );
}

#[test]
fn every_whole_directory_relocation_carries_supporting_material_untouched() {
    // Two operations relocate an Order Directory as a whole: explicit alignment,
    // which renames it inside the Orders Directory and leaves the Order active, and
    // explicit whole-Order disposition, which takes it out of active scope. Neither
    // inspects, enumerates, or rewrites the opaque tree it carries.
    let (_tree, root) = initialized_package("m3-relocation");
    let directory = adopted_order(&root, "Legacy_drifted_name", FIRST_ID, None);
    let before = opaque_contents(&directory);
    assert_eq!(before[1], SUPPORTING_A);
    assert_eq!(before[2], SUPPORTING_B);

    // Alignment: renamed in place, still active, contents identical.
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order aligns");
    let aligned = operation_path(&plan, "rename_order_directory");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert!(!directory.exists());
    assert_eq!(
        opaque_contents(&aligned),
        before,
        "alignment carries the opaque tree byte for byte"
    );
    assert!(
        valid(
            &diagnosed_order(&aligned, &PlatformFileSystem)
                .result
                .expect("the aligned Order diagnoses")
        ),
        "and the Order is still active and healthy at its new name"
    );

    // Disposition: moved out of active scope, contents still identical.
    let plan = crate::plan(
        &remove_order(aligned.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed");
    let removed = operation_path(&plan, "move_order_directory");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert!(!aligned.exists());
    assert_eq!(
        opaque_contents(&removed),
        before,
        "disposition carries the same opaque tree byte for byte"
    );
    // And the removed Order has left active scope.
    let index: crate::tests::support::Index =
        serde_json::from_slice(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    assert!(index.orders.is_empty());
}

#[test]
fn every_write_capability_composes_over_a_configured_nested_layout() {
    // A configured Package is the ordinary case, not an edge case. Each capability here
    // resolves its own managed locations from the Package it was handed, so a seam that
    // fell back to the default spelling would write outside the layout the user chose.
    let (_tree, root) = package_with_layout("m3-nested-composition", &(NESTED_ORDERS, RETIRED));
    // A directory that merely happens to be named `Orders`. Nothing configures it.
    fs::create_dir(root.join("Orders")).unwrap();

    let plan = crate::plan(
        &create_vendor(
            root.clone(),
            Assignments::new().assign("name", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Vendor can be planned");
    let Ok(Report::Committed { .. }) = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[VENDOR_ID])),
    )
    .result
    else {
        panic!("the Vendor commits");
    };

    let plan = crate::plan(
        &add_contact(
            root.clone(),
            VendorRef::Id(String::from(VENDOR_ID)),
            Assignments::new()
                .assign("name", "Dana Reyes")
                .assign("role", "Sales"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Contact can be planned");
    let Ok(Report::Committed { result, .. }) =
        crate::commit(plan, &effects(&PlatformFileSystem)).result
    else {
        panic!("the Contact commits");
    };
    assert_eq!(committed_vendor(&root, &result).contacts.len(), 1);

    let plan = crate::plan(
        &create_order(
            root.clone(),
            Assignments::new()
                .assign("title", "Optical table")
                .assign("date_ordered", "2026-08-22")
                .assign("vendor", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("an Order can be planned");
    let created = operation_path(&plan, "commit_directory");
    assert!(
        created.starts_with(root.join(NESTED_ORDERS)),
        "creation places the Order under the configured Orders Directory: {created:?}"
    );
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[FIRST_ID]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let plan = crate::plan(
        &edit_order(
            created.clone(),
            Assignments::new().assign("title", "Optical bench"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the Order can be edited");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    let plan = crate::plan(
        &add_item(
            created.clone(),
            Assignments::new()
                .assign("description", "Mirror mount")
                .assign("quantity", "2")
                .assign("unit_price", "41.50"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an Item can be planned");
    let Ok(Report::Committed { result, .. }) =
        crate::commit(plan, &effects(&PlatformFileSystem)).result
    else {
        panic!("the Item commits");
    };
    assert_eq!(committed_order(&result).order_total, 83.0);

    let plan = crate::plan(
        &write_readme(
            order_scope(created.clone()),
            current_readme(&created),
            String::from(ORDER_README),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order README can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let plan = crate::plan(
        &write_readme(
            package_scope(root.clone()),
            current_readme(&root),
            "# Purchasing\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Package README can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));

    let drifted = adopted_order_at(&root, NESTED_ORDERS, "Legacy_drifted", SECOND_ID, None);
    let plan = crate::plan(&align_order(drifted.clone()), &effects(&PlatformFileSystem))
        .result
        .expect("the Order aligns");
    let aligned = operation_path(&plan, "rename_order_directory");
    assert!(
        aligned.starts_with(root.join(NESTED_ORDERS)),
        "alignment stays inside the configured Orders Directory: {aligned:?}"
    );
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let carried = opaque_contents(&aligned);

    let third = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Third", THIRD_ID, None);

    let plan = crate::plan(
        &set_vendor_hidden(root.clone(), VendorRef::Id(String::from(VENDOR_ID)), true),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can hide");
    let Ok(Report::Committed { .. }) = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result
    else {
        panic!("hiding commits");
    };

    let Ok(candidate) = opened(&root, &PlatformFileSystem).result else {
        panic!("the composed Package loads");
    };
    assert_eq!(
        candidate
            .contents
            .context
            .orders_directory
            .as_ref()
            .and_then(|location| location.package_relative_path.as_deref()),
        Some(NESTED_ORDERS)
    );
    assert_eq!(
        candidate
            .contents
            .context
            .removed_orders_directory
            .as_ref()
            .and_then(|location| location.package_relative_path.as_deref()),
        Some(RETIRED)
    );
    assert_eq!(candidate.contents.healthy_orders.len(), 3);
    assert!(candidate.contents.damaged_orders.is_empty());
    assert_eq!(candidate.contents.index.state, PackageIndexState::Current);
    // Hidden is not deleted: the Vendor the created Order references still resolves,
    // and is simply not offered for new use.
    let position = |directory: &Path| {
        candidate
            .contents
            .healthy_orders
            .iter()
            .position(|order| order.paths.directory.absolute_path == directory)
            .expect("the Order is projected")
    };
    assert_eq!(
        candidate
            .contents
            .vendor_for_order(position(&created))
            .map(|vendor| vendor.name.deref()),
        Some("Precision Optics")
    );
    assert!(
        candidate
            .vendors_matching(
                &SearchQuery::new("", CaseSensitivity::Insensitive),
                VendorVisibility::Offered
            )
            .result
            .unwrap()
            .is_empty()
    );
    // Nothing wrote into the decoy, and no capability disturbed Supporting Material.
    assert!(fs::read_dir(root.join("Orders")).unwrap().next().is_none());
    assert_eq!(fs::read(third.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(third.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );

    // Only explicit whole-Order removal moves the tree, and it lands under the
    // configured Removed Directory carrying the same opaque bytes.
    let plan = crate::plan(
        &remove_order(aligned.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed");
    let removed = operation_path(&plan, "move_order_directory");
    assert!(
        removed.starts_with(root.join(RETIRED)),
        "removal uses the configured Removed Directory: {removed:?}"
    );
    let Ok(Report::Committed { .. }) = crate::commit(plan, &effects(&PlatformFileSystem)).result
    else {
        panic!("the removal commits");
    };
    assert!(index_is_current(&root));
    assert_eq!(fs::read(removed.join("quote.bin")).unwrap(), carried[1]);
    assert_eq!(
        fs::read(removed.join("Quotes/drawing.bin")).unwrap(),
        carried[2]
    );
}

#[test]
fn the_three_conventional_readme_states_stand_side_by_side_in_one_package() {
    // Absent, readable, and unreadable are three answers, not two. The derived index
    // is the pressure that would collapse them: recording `""` for text it could not
    // read turns an unknown README into an authoritative empty document, and every
    // later reader believes it. All three stand in one projection here so a rule that
    // held for one Order in isolation cannot hold by accident.
    let (_tree, root) = package_with_layout("m3-readme-trichotomy", &(NESTED_ORDERS, RETIRED));
    let readable = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let absent = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Second", SECOND_ID, None);
    let unreadable = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Third", THIRD_ID, None);
    fs::remove_file(absent.join("README.md")).unwrap();
    fs::write(unreadable.join("README.md"), [0x66, 0xff, 0x0a]).unwrap();

    let Ok(candidate) = opened(&root, &PlatformFileSystem).result else {
        panic!("an unreadable README leaves the Package usable");
    };
    let readme = |directory: &Path| {
        candidate
            .contents
            .healthy_orders
            .iter()
            .find(|order| order.paths.directory.absolute_path == directory)
            .expect("the Order is projected")
            .readme_text
            .clone()
    };
    assert_eq!(
        readme(&readable).as_deref(),
        Some(std::str::from_utf8(README).unwrap()),
        "a readable README is exactly the text the user wrote"
    );
    assert_eq!(
        readme(&absent).as_deref(),
        Some(""),
        "an absent README is an empty document"
    );
    assert_eq!(
        readme(&unreadable),
        None,
        "an unreadable README is unknown rather than empty"
    );
    // None of the three costs the Package its usability, and the derived cache
    // declines rather than inventing the one answer it does not have.
    assert_eq!(candidate.contents.healthy_orders.len(), 3);
    assert!(candidate.contents.damaged_orders.is_empty());

    // Nothing needs repairing for the cache to recover: the moment the text reads, a
    // rebuild records it.
    fs::write(unreadable.join("README.md"), README).unwrap();
    let rebuilt = committed(&rebuild_index(root.clone()), &effects(&PlatformFileSystem));
    assert_eq!(rebuilt, ChangeResult::RebuildIndex);
    let index = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the rebuilt index parses");
    let recorded = index
        .orders
        .iter()
        .find(|entry| root.join(&entry.order_directory) == unreadable)
        .expect("the rebuilt index lists the Order");
    assert_eq!(recorded.readme.as_bytes(), README);
}

#[test]
fn a_refused_write_leaves_the_package_as_it_was_and_costs_no_other_capability() {
    // A refusal is inert. Each capability suite proves its own refusal preserves its
    // own destination; what no one of them can show is that a Package carrying four
    // refusals from four capabilities is still whole, still holds its opaque tree, and
    // still commits every one of those operations afterwards.
    let (_tree, root) = package_with_layout("m3-refusals-are-inert", &(NESTED_ORDERS, RETIRED));
    let directory = adopted_order_at(&root, NESTED_ORDERS, "Legacy_drifted", FIRST_ID, None);
    let vendors_before = fs::read(vendors_record(&root)).unwrap();
    let readme_before = fs::read(directory.join("README.md")).unwrap();
    let opaque_before = opaque_contents(&directory);

    // Precommit: the Vendor aggregate is never written, and the staging file the
    // attempt made is gone.
    let plan = crate::plan(
        &create_vendor(
            root.clone(),
            Assignments::new().assign("name", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Vendor can be planned");
    let report = crate::commit(
        plan,
        &effects(&failing_at(Boundary::StagingWrite)).minting(QueuedUuids::of(&[VENDOR_ID])),
    )
    .result;
    assert!(
        report.is_err(),
        "a failure before the canonical write is a rejection: {report:?}"
    );
    assert_eq!(fs::read(vendors_record(&root)).unwrap(), vendors_before);
    assert!(
        !root
            .join(".spielorders/.vendors.json.spielorders-stage")
            .exists()
    );

    // Changed evidence: someone else's record arrives after the plan was authorized,
    // and their work stands rather than being overwritten by a stale candidate.
    let theirs = String::from_utf8(fs::read(record_path(&directory)).unwrap())
        .unwrap()
        .replace("Laser components", "Someone else's title")
        .into_bytes();
    assert_ne!(theirs, fs::read(record_path(&directory)).unwrap());
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the Order can be edited");
    let other_writer =
        another_writer_replacing(&record_path(&directory), &theirs, EditPoint::StagingWrite);
    let report = crate::commit(plan, &effects(&other_writer)).result;
    assert!(other_writer.disturbed(), "the outside write really landed");
    assert!(
        report.is_err(),
        "a destination that changed under the plan is refused: {report:?}"
    );
    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        theirs,
        "the other writer's record survives"
    );

    // Cleanup failure: reported beside the refusal that asked for it, never instead
    // of it, and still without a canonical byte written.
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "# Replaced\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the README can be planned");
    let report = crate::commit(
        plan,
        &effects(&failing_at(Boundary::StagingDurabilityAndCleanup)),
    )
    .result;
    let Err(error) = &report else {
        panic!("a durability failure is a rejection: {report:?}");
    };
    assert!(
        !error.cleanup_failures().is_empty(),
        "the staging file it could not remove is reported too"
    );
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        readme_before
    );
    let retained = directory.join(".README.md.spielorders-stage");
    assert!(
        retained.exists(),
        "the report names a file really left behind"
    );
    fs::remove_file(&retained).unwrap();

    // Collision: alignment stops at an occupied name rather than inventing a suffix.
    let projected = operation_path(
        &crate::plan(
            &align_order(directory.clone()),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect("the Order aligns while its name is free"),
        "rename_order_directory",
    );
    fs::create_dir(&projected).unwrap();
    fs::write(projected.join("sibling.txt"), b"someone else's directory\n").unwrap();
    let error = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("an occupied destination stops alignment");
    assert!(
        matches!(&error, Failure::DestinationCollision { location, .. } if location.absolute_path == projected),
        "the collision names the occupied path: {error:?}"
    );
    fs::remove_dir_all(&projected).unwrap();

    // Four refusals later the Package is exactly what it was, opaque tree included.
    assert_eq!(
        opaque_contents(&directory),
        opaque_before,
        "no refusal read, moved, or rewrote Supporting Material"
    );
    let Ok(candidate) = opened(&root, &PlatformFileSystem).result else {
        panic!("a Package that only refused writes is still usable");
    };
    assert_eq!(candidate.contents.healthy_orders.len(), 1);
    assert!(candidate.contents.damaged_orders.is_empty());
    assert!(candidate.contents.vendors.is_empty());

    // And each refused operation is a retry, not a dead end.
    let plan = crate::plan(
        &create_vendor(
            root.clone(),
            Assignments::new().assign("name", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[VENDOR_ID]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "# Replaced\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let plan = crate::plan(
        &align_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let aligned = operation_path(&plan, "rename_order_directory");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert_eq!(
        fs::read(aligned.join("quote.bin")).unwrap(),
        opaque_before[1]
    );
}

#[test]
fn a_derived_index_that_cannot_be_refreshed_never_makes_a_commit_look_retryable() {
    // The Package Index is a cache SpielOrders owns. A caller that reads a failed
    // refresh as a rejection reruns the operation, and every one of these run twice is
    // a second canonical change: two Orders, two Items, two Vendors. So the rule has to
    // hold for every write capability at once, which is not something any one of them
    // can establish alone. Each plans against a healthy read and commits through a
    // filesystem whose index replacement fails. Artifact repair answers the same
    // question over its own diagnosed subject, in `artifact_repair.rs`.
    let (_tree, root) = package_with_layout("m3-index-refresh-failure", &(NESTED_ORDERS, RETIRED));
    let failing = || failing_at(Boundary::IndexRefresh);

    let plan = crate::plan(
        &create_vendor(
            root.clone(),
            Assignments::new().assign("name", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Vendor can be planned");
    let report = crate::commit(
        plan,
        &effects(&failing()).minting(QueuedUuids::of(&[VENDOR_ID])),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the Vendor is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let plan = crate::plan(
        &create_order(
            root.clone(),
            Assignments::new()
                .assign("title", "Optical table")
                .assign("date_ordered", "2026-08-22")
                .assign("vendor", "Precision Optics"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("an Order can be planned");
    let created = operation_path(&plan, "commit_directory");
    let report = crate::commit(
        plan,
        &effects(&failing()).minting(QueuedUuids::of(&[FIRST_ID])),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the Order is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let plan = crate::plan(
        &edit_order(
            created.clone(),
            Assignments::new().assign("title", "Optical bench"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the Order can be edited");
    let report = crate::commit(plan, &effects(&failing())).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the edit is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let plan = crate::plan(
        &add_item(
            created.clone(),
            Assignments::new()
                .assign("description", "Mirror mount")
                .assign("quantity", "2")
                .assign("unit_price", "41.50"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an Item can be planned");
    let report = crate::commit(plan, &effects(&failing())).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the Item is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let plan = crate::plan(
        &write_readme(
            order_scope(created.clone()),
            current_readme(&created),
            String::from(ORDER_README),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the README can be planned");
    let report = crate::commit(plan, &effects(&failing())).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the README is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let candidate = legacy_directory(&root, NESTED_ORDERS, "Legacy_drifted");
    let plan = crate::plan(
        &adoption(&root, &candidate, &legacy_facts(None)),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&failing()).minting(QueuedUuids::of(&[SECOND_ID])),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the adoption is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    // Semantic repair is the one capability here that writes two managed records
    // together — the Order and the Package's Vendor aggregate — so a refresh failure
    // read as a rejection costs a rerun that creates a second Vendor for a reference
    // already resolved. The reference is manufactured by hand because SpielOrders can
    // no longer produce one: a Vendor is hidden, never deleted.
    let text = fs::read_to_string(record_path(&candidate))
        .unwrap()
        .replace(
            "\"items\": []",
            &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
        );
    fs::write(record_path(&candidate), text).unwrap();
    let diagnosis = diagnosed_order(&candidate, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses");
    let handle = diagnosis
        .cases
        .iter()
        .find(|case| case.code == DiagnosticCode::VendorReferenceUnresolved)
        .expect("the orphaned reference is diagnosed")
        .handle
        .to_owned();
    let plan = crate::plan(
        &repair(
            order_scope(candidate.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Vacuum Systems"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the orphaned reference can be repaired");
    let report = crate::commit(
        plan,
        &effects(&failing()).minting(QueuedUuids::of(&[THIRD_ID])),
    )
    .result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("a two-record repair is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    // Both halves of the repair are on disk, so the caller that was told "warning"
    // rather than "rejected" was told the truth about every record it changed.
    let repaired =
        serde_json::from_slice::<OrderDocument>(&fs::read(record_path(&candidate)).unwrap())
            .expect("the repaired record parses");
    assert_eq!(
        repaired.vendor_id.as_deref(),
        Some(THIRD_ID),
        "the Order half of the repair really landed"
    );
    assert!(
        String::from_utf8(fs::read(vendors_record(&root)).unwrap())
            .unwrap()
            .contains(THIRD_ID),
        "and so did the Vendor half"
    );

    let plan = crate::plan(
        &align_order(candidate.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order aligns");
    let aligned = operation_path(&plan, "rename_order_directory");
    let report = crate::commit(plan, &effects(&failing())).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the rename is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    assert!(aligned.is_dir(), "the rename really happened");
    assert!(!candidate.exists());

    let plan = crate::plan(
        &remove_order(aligned.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed");
    let report = crate::commit(plan, &effects(&failing())).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the disposition is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    // Nine canonical mutations survived nine failed cache writes, each exactly once,
    // and one rebuild is all the cache needs to catch up.
    committed(&rebuild_index(root.clone()), &effects(&PlatformFileSystem));
    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a Package whose cache kept failing is still usable");
    };
    assert_eq!(package.contents.index.state, PackageIndexState::Current);
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(
        package
            .contents
            .vendors
            .iter()
            .map(|vendor| vendor.name.deref())
            .collect::<Vec<_>>(),
        ["Precision Optics", "Vacuum Systems"],
        "the created Vendor and the repair's Vendor each landed once"
    );
    let order = &package.contents.healthy_orders[0];
    assert_eq!(order.paths.directory.absolute_path, created);
    assert_eq!(order.document.title.deref(), "Optical bench");
    assert_eq!(
        order.document.items.len(),
        1,
        "the Item was added once, not once per retry"
    );
    assert_eq!(order.readme_text.as_deref(), Some(ORDER_README));
    assert!(
        root.join(RETIRED)
            .join(aligned.file_name().unwrap())
            .is_dir()
    );
}

#[test]
fn a_rollback_that_cannot_finish_leaves_the_order_active_and_still_disposable() {
    // Whole-Order disposition is the one operation allowed to move Supporting Material,
    // so the case that matters most is the one where it fails and cannot fully undo
    // itself. The Order must stay active, its opaque tree must be untouched, and the
    // disposition must still be available — an Order stranded half-out of scope is the
    // failure this forbids.
    let (_tree, root) = package_with_layout("m3-rollback-failure", &(NESTED_ORDERS, RETIRED));
    let directory = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let opaque_before = opaque_contents(&directory);
    let removed_root = root.join(RETIRED);
    assert!(
        !removed_root.exists(),
        "the Removed Directory is created by the first disposition, not by initialization"
    );

    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed");
    let report = crate::commit(plan, &effects(&failing_at(Boundary::DirectoryMoveAndUndo))).result;

    // Beside, not instead of: the move failed, and the caller separately learns that
    // undoing what the attempt created failed too.
    let Err(
        failure @ Failure::CommitFailedCleanupIncomplete {
            cleanup_failures, ..
        },
    ) = &report
    else {
        panic!("both failures are reported: {report:?}");
    };
    assert_reported_beside(failure);
    assert_eq!(
        cleanup_failures
            .iter()
            .map(|cleanup| cleanup.location.absolute_path.as_path())
            .collect::<Vec<_>>(),
        [removed_root.as_path()]
    );
    assert!(
        removed_root.is_dir(),
        "the report names a directory really still there"
    );

    // The Order never left active scope and nothing of it moved.
    assert!(directory.is_dir());
    assert_eq!(opaque_contents(&directory), opaque_before);
    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("the Package is still usable after a failed disposition");
    };
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert!(package.contents.damaged_orders.is_empty());

    // The same disposition succeeds once the filesystem does, and the tree it carries
    // is byte for byte the one the failed attempt left alone.
    let plan = crate::plan(
        &remove_order(directory.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order can be removed again");
    let removed = operation_path(&plan, "move_order_directory");
    let Ok(Report::Committed { .. }) = crate::commit(plan, &effects(&PlatformFileSystem)).result
    else {
        panic!("the retried disposition commits");
    };
    assert!(index_is_current(&root));
    assert_eq!(opaque_contents(&removed), opaque_before);
}

#[test]
fn a_record_the_build_cannot_read_is_repaired_over_a_configured_nested_layout() {
    // A current Package can still hold a record this build cannot interpret. It is a
    // Damaged Order, and integrity repair is what answers it.
    let (_tree, root) = package_with_layout("m3-current-damaged", &(NESTED_ORDERS, RETIRED));
    let healthy = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let damaged = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Second", SECOND_ID, None);
    let healthy_before = fs::read(record_path(&healthy)).unwrap();
    fs::write(
        record_path(&damaged),
        br#"{"schema_version": 1, "nope": true}"#,
    )
    .unwrap();

    let diagnosis = diagnosed_order(&damaged, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses");
    let case = diagnosis
        .cases
        .iter()
        .find(|case| case.code == DiagnosticCode::ManagedArtifactInvalidShape)
        .expect("the unreadable record is diagnosed");
    assert_eq!(case.actions, [RepairAction::ReplaceManagedRecord]);
    let plan = crate::plan(
        &repair(
            order_scope(damaged.clone()),
            String::from(&case.handle),
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign(
                "record",
                String::from_utf8(nested_replacement_record(SECOND_ID)).unwrap(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a confirmed replacement is planned");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let Ok(candidate) = opened(&root, &PlatformFileSystem).result else {
        panic!("the repaired Package loads");
    };
    assert_eq!(candidate.contents.healthy_orders.len(), 2);
    assert!(candidate.contents.damaged_orders.is_empty());
    assert_eq!(candidate.contents.index.state, PackageIndexState::Current);
    assert_eq!(
        fs::read(record_path(&healthy)).unwrap(),
        healthy_before,
        "the Order that was never damaged is untouched"
    );
}

/// A commit whose index refresh cannot cache an Order says so: an unreadable README keeps
/// the stored index as it was, and the edit is committed with a warning.
#[test]
fn an_edit_that_leaves_the_index_stale_is_committed_with_a_warning() {
    let (_tree, root) = initialized_package("refresh-unreadable-readme");
    let directory = crate::tests::support::created_order(
        &root,
        FIRST_ID,
        Assignments::new().assign("title", "Lenses"),
    );
    std::fs::remove_file(directory.join("README.md")).unwrap();
    std::fs::create_dir(directory.join("README.md")).unwrap();
    let effects = effects(&PlatformFileSystem);
    let plan = crate::plan(
        &crate::tests::support::edit_order(
            directory,
            Assignments::new().assign("title", "Mirrors"),
        ),
        &effects,
    )
    .result
    .expect("the edit is planned");

    let report = crate::commit(plan, &effects).result;
    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the edit is committed with a warning: {report:?}");
    };
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning, Warning::RefreshUnavailable)),
        "{warnings:?}"
    );
}
