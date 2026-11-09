//! Decision sources: Define Vendor identity and historical semantics; Define the
//! Rust CLI capability contract; Define integrity, validation, and repair boundaries;
//! Define durable metadata schemas and version evolution.

use crate::Failure;
use crate::repair_cases::RepairAction;
use crate::tests::support::assert_reported_beside;
use crate::tests::support::operation_path;
use crate::tests::support::{
    EditPoint, FailedCommit, Index, Operation, PlatformFileSystem, QueuedUuids, ScriptedFileSystem,
    TemporaryTree, Vendors, adoption, another_writer_replacing, create_vendor, created_vendor,
    diagnosed_order, effects, findings, json, operation_kinds, order_record, order_scope, repair,
    text, valid,
};
use crate::{Assignments, Change, Report};
use std::ops::Deref;

use crate::metadata::OrderDocument;

use crate::readme::DiagnosticCode;

use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const TWIN_ID: &str = "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f";
const THIRD_ID: &str = "2d5e3f7a-4b6c-4d8e-9f0a-1b2c3d4e5f60";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const NEW_VENDOR_ID: &str = "f1aa1e42-17bb-43fa-a8d9-199f4f905933";
const ABSENT_VENDOR: &str = "00000000-0000-4000-8000-000000000000";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

/// A configured Orders path the Package root cannot be reached from by counting
/// parents: two levels up from an Order Directory is an ordinary folder of the user's.
const NESTED_ORDERS: &str = "Purchases/Active";

fn initialized_package_at(label: &str, orders_path: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = crate::plan(
        &Change::InitializePackage {
            root: tree.path().into(),
            orders_path: Some(String::from(orders_path)),
            removed_path: Some(String::from("Removed")),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

fn adopted_order(root: &Path, basename: &str, uuid: &str) -> PathBuf {
    adopted_order_at(root, "Orders", basename, uuid)
}

fn adopted_order_at(root: &Path, orders_path: &str, basename: &str, uuid: &str) -> PathBuf {
    let directory = root.join(orders_path).join(basename);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("README.md"), README).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
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

fn document(directory: &Path) -> OrderDocument {
    serde_json::from_slice::<OrderDocument>(&fs::read(order_record(directory)).unwrap())
        .expect("the canonical Order parses")
}

fn vendors(root: &Path) -> Vendors {
    serde_json::from_slice(&fs::read(root.join(".spielorders/vendors.json")).unwrap())
        .expect("the Vendor aggregate parses")
}

fn handle_for(directory: &Path, code: DiagnosticCode) -> String {
    let diagnosis = diagnosed_order(directory, &PlatformFileSystem)
        .result
        .expect("the Order diagnoses");
    diagnosis
        .cases
        .iter()
        .find(|case| case.code == code)
        .unwrap_or_else(|| panic!("{code:?} has a case: {:?}", diagnosis.cases))
        .handle
        .to_owned()
}

/// An Order whose Vendor Reference names a Vendor this Package does not have.
fn unresolved_vendor_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    unresolved_vendor_order_at(label, "Orders")
}

fn unresolved_vendor_order_at(label: &str, orders_path: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = initialized_package_at(label, orders_path);
    let directory = adopted_order_at(&root, orders_path, "2026_08_22_Laser_components", ORDER_ID);
    let text = fs::read_to_string(order_record(&directory))
        .unwrap()
        .replace(
            "\"items\": []",
            &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
        );
    fs::write(order_record(&directory), text).unwrap();
    (tree, root, directory)
}

/// An Order whose stored technical locator no longer leads to its Package.
fn stale_locator_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    stale_locator_order_at(label, "Orders")
}

fn stale_locator_order_at(label: &str, orders_path: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = initialized_package_at(label, orders_path);
    let directory = adopted_order_at(&root, orders_path, "2026_08_22_Laser_components", ORDER_ID);
    // Deeper than any layout this fixture uses, so the stored value is stale under
    // every configured Orders path rather than only the default one.
    let text = fs::read_to_string(order_record(&directory))
        .unwrap()
        .replace(
            &format!("\"{}\"", text(&document(&directory).package_root)),
            "\"../../../../..\"",
        );
    fs::write(order_record(&directory), text).unwrap();
    (tree, root, directory)
}

#[test]
fn an_unresolved_vendor_reference_can_be_pointed_at_an_existing_vendor() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-assign");
    let existing = created_vendor(&root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let vendors_before = fs::read(root.join(".spielorders/vendors.json")).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::AssignExistingVendor,
            Assignments::new().assign("vendor", "  tarNWICK "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an existing Vendor can be assigned");

    assert_eq!(
        operation_kinds(&plan),
        ["replace_order_record", "refresh_package_index",],
        "assigning an existing Vendor rewrites no Vendor aggregate"
    );
    // A dry run writes nothing.
    assert_eq!(
        document(&directory).vendor_id.as_deref(),
        Some(ABSENT_VENDOR)
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("assigning an existing Vendor commits: {report:?}");
    };
    assert_eq!(
        document(&directory).vendor_id.as_deref(),
        Some(existing.as_str())
    );
    assert_eq!(vendors(&root).vendors.len(), 1, "no Vendor was created");
    assert!(valid(
        &diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .expect("rediagnosed")
    ));
    assert_eq!(
        fs::read(root.join(".spielorders/vendors.json")).unwrap(),
        vendors_before,
        "the Vendor aggregate is untouched"
    );
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
}

#[test]
fn a_deliberately_distinct_vendor_can_be_created_and_assigned_in_one_plan() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-create");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "  Nethermoor  "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a distinct Vendor can be created and assigned");

    assert_eq!(
        operation_kinds(&plan),
        [
            "replace_order_record",
            "replace_vendor_record",
            "refresh_package_index",
        ],
        "one prevalidated plan writes both artifacts"
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[NEW_VENDOR_ID])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("a deliberate Vendor creation commits: {report:?}");
    };
    let aggregate = vendors(&root);
    let created = aggregate
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id.deref() == NEW_VENDOR_ID)
        .expect("a Vendor was created under the minted identity");
    assert_eq!(created.name.deref(), "Nethermoor");
    assert_eq!(
        document(&directory).vendor_id.as_deref(),
        Some(NEW_VENDOR_ID)
    );
    // The existing Vendor is still there: this is a new Vendor, never a merge with
    // or a redirection to the one already present.
    assert_eq!(aggregate.vendors.len(), 2);
    assert!(
        aggregate
            .vendors
            .iter()
            .any(|vendor| vendor.name.deref() == "Tarnwick")
    );
    assert!(valid(
        &diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .expect("rediagnosed")
    ));
}

#[test]
fn an_unresolved_vendor_reference_can_be_cleared_outright() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-clear");
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let vendors_before = fs::read(root.join(".spielorders/vendors.json")).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ClearVendorReference,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the reference can be cleared");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("clearing commits: {report:?}");
    };
    assert_eq!(document(&directory).vendor_id, None);
    assert!(valid(
        &diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .expect("rediagnosed")
    ));
    assert_eq!(
        fs::read(root.join(".spielorders/vendors.json")).unwrap(),
        vendors_before
    );
}

#[test]
fn a_vendor_is_never_inferred_from_anything_but_the_explicit_choice() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-no-inference");
    // Everything an implementation might be tempted to infer from is present: an
    // existing Vendor, a sibling Order that resolves to it, and a directory name that
    // matches it.
    let existing = created_vendor(&root, "Tarnwick", VENDOR_ID);
    let sibling = adopted_order(&root, "2026_08_22_Tarnwick_Other", TWIN_ID);
    let text = fs::read_to_string(order_record(&sibling)).unwrap().replace(
        "\"items\": []",
        &format!("\"vendor_id\": \"{existing}\",\n  \"items\": []"),
    );
    fs::write(order_record(&sibling), text).unwrap();
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let before = fs::read(order_record(&directory)).unwrap();

    // A Vendor that does not exist is refused, rather than resolved to the one the
    // sibling uses or the one the directory name resembles.
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::AssignExistingVendor,
            Assignments::new().assign("vendor", "Nonesuch Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("an unknown Vendor choice is refused");
    assert!(
        matches!(error, Failure::UnknownVendor { .. }),
        "got {error:?}"
    );

    // A created Vendor whose Name is an existing Vendor's would be that Vendor, so it
    // is refused rather than quietly becoming a merge.
    for name in ["Tarnwick", "  tarnwick  "] {
        let error = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::CreateAndAssignVendor,
                Assignments::new().assign("name", name),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("a colliding Vendor Name is refused");
        assert!(
            matches!(error, Failure::VendorNameInUse { .. }),
            "{name:?} got {error:?}"
        );
    }

    // A placeholder is not a Vendor.
    for name in ["", "   "] {
        let error = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::CreateAndAssignVendor,
                Assignments::new().assign("name", name),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("a placeholder Vendor is refused");
        assert!(
            matches!(error, Failure::RepairResolutionMissing { .. }),
            "{name:?} got {error:?}"
        );
    }

    assert_eq!(fs::read(order_record(&directory)).unwrap(), before);
    assert_eq!(vendors(&root).vendors.len(), 1);
}

/// Three physical Orders sharing one identity, so resolving one conflict leaves a
/// real conflict standing rather than an empty set.
fn duplicate_identity_package(label: &str) -> (TemporaryTree, PathBuf, Vec<PathBuf>) {
    duplicate_identity_package_at(label, "Orders")
}

fn duplicate_identity_package_at(
    label: &str,
    orders_path: &str,
) -> (TemporaryTree, PathBuf, Vec<PathBuf>) {
    let (tree, root) = initialized_package_at(label, orders_path);
    let first = adopted_order_at(&root, orders_path, "2026_08_22_First", ORDER_ID);
    let second = adopted_order_at(&root, orders_path, "2026_08_22_Second", TWIN_ID);
    let third = adopted_order_at(&root, orders_path, "2026_08_22_Third", THIRD_ID);
    for directory in [&second, &third] {
        let original = if directory == &second {
            TWIN_ID
        } else {
            THIRD_ID
        };
        let text = fs::read_to_string(order_record(directory))
            .unwrap()
            .replace(original, ORDER_ID);
        fs::write(order_record(directory), text).unwrap();
    }
    (tree, root, vec![first, second, third])
}

#[test]
fn one_chosen_participant_adopts_a_fresh_identity_and_the_others_are_untouched() {
    let (_tree, root, participants) = duplicate_identity_package("semantic-duplicate");
    let [first, second, third] = <[PathBuf; 3]>::try_from(participants).unwrap();
    // The caller explicitly chooses which physical Order stops being the original.
    let chosen = &second;
    let handle = handle_for(chosen, DiagnosticCode::OrderIdDuplicate);
    let first_before = fs::read(order_record(&first)).unwrap();
    let third_before = fs::read(order_record(&third)).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(PathBuf::from(chosen)),
            &handle,
            RepairAction::AdoptNewOrderIdentity,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the chosen participant can adopt a new identity");

    let fresh = "9f8e7d6c-5b4a-4392-8271-605f4e3d2c1b";
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[fresh])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("adopting a new identity commits: {report:?}");
    };
    assert_eq!(document(chosen).order_id.deref(), fresh);

    // Never chooses the survivor, and never alters every participant: the other two
    // still carry the original identity, byte for byte as they were.
    assert_eq!(fs::read(order_record(&first)).unwrap(), first_before);
    assert_eq!(fs::read(order_record(&third)).unwrap(), third_before);
    assert_eq!(document(&first).order_id.deref(), ORDER_ID);
    assert_eq!(document(&third).order_id.deref(), ORDER_ID);

    // One resolution resolves one conflict. The remaining duplicate between the two
    // untouched participants is still reported.
    let remaining = diagnosed_order(&first, &PlatformFileSystem)
        .result
        .expect("the survivor diagnoses");
    assert!(
        remaining
            .cases
            .iter()
            .any(|case| case.code == DiagnosticCode::OrderIdDuplicate),
        "the conflict among the remaining participants still stands"
    );
    assert!(
        findings(
            &diagnosed_order(chosen, &PlatformFileSystem)
                .result
                .expect("rediagnosed")
        )
        .all(|diagnostic| diagnostic.code != DiagnosticCode::OrderIdDuplicate),
        "the repaired Order is no longer part of a conflict"
    );
    assert_eq!(root.join("Orders").read_dir().unwrap().count(), 3);
}

/// The commit-time candidate guard that **can** fire, driven end to end.
///
/// A semantic repair never validates the record it repairs: `read_document_bytes`
/// parses with `serde_json` alone, and the five resolutions rewrite only a Vendor
/// Reference, the physical locator, or the Order identity. So a Damaged Order carries
/// its defect straight through the repair and into the candidate.
///
/// The route in: an Order whose `date_ordered` is `2026-02-30` has the shape of a date
/// and is not a real one, so it is quarantined as `OrderSemanticInvalid` — **with its
/// Order id retained**. Quarantine is the admission mechanism here, not a gate: a
/// retained id still enters duplicate detection, so this Order is also reported
/// `OrderIdDuplicate`, whose one offered repair is `AdoptNewOrderIdentity`. Adopting a
/// fresh identity does nothing about the date, and the candidate guard is the first
/// thing on the path that calls `validate()`.
#[test]
fn a_damaged_order_with_an_unreal_date_is_refused_at_the_candidate_guard() {
    let (_tree, _root, participants) = duplicate_identity_package("semantic-unreal-date");
    let chosen = &participants[1];
    // A second sentinel, so the refusal is observed against both opaque assets.
    fs::write(chosen.join("datasheet.bin"), SUPPORTING_B).unwrap();
    // Shape of a date, not a real one: February has 28 days in 2026.
    let text = fs::read_to_string(order_record(chosen))
        .unwrap()
        .replace("2026-08-22", "2026-02-30");
    fs::write(order_record(chosen), &text).unwrap();
    let before = fs::read(order_record(chosen)).unwrap();
    assert!(
        serde_json::from_slice::<OrderDocument>(&before).is_ok(),
        "the record still parses — it is the semantics that are wrong, not the shape"
    );

    // The duplicate-identity repair is offered on a record that is also semantically
    // invalid. If this handle ever stops existing, the route is closed and the guard's
    // recorded verdict has to be revisited.
    let handle = handle_for(chosen, DiagnosticCode::OrderIdDuplicate);
    let plan = crate::plan(
        &repair(
            order_scope(PathBuf::from(chosen)),
            &handle,
            RepairAction::AdoptNewOrderIdentity,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Damaged Order's duplicate identity can still be planned for repair");

    let fresh = "9f8e7d6c-5b4a-4392-8271-605f4e3d2c1b";
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[fresh])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a repaired record that is still invalid is refused: {report:?}");
    };
    let Failure::CommitFailed { failure } = rejection else {
        panic!(
            "the replacement refused it, and nothing was written, so no managed file is \
             left standing in another state: {rejection:?}"
        );
    };
    assert_eq!(failure.location.absolute_path, order_record(chosen));

    assert_eq!(
        fs::read(order_record(chosen)).unwrap(),
        before,
        "the Order record is exactly as it was"
    );
    assert_eq!(
        document(chosen).order_id.deref(),
        ORDER_ID,
        "the identity was not adopted"
    );
    assert_eq!(fs::read(chosen.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(chosen.join("datasheet.bin")).unwrap(),
        SUPPORTING_B
    );
}

#[test]
fn a_generated_identity_that_already_exists_is_refused_before_writing() {
    let (_tree, _root, participants) = duplicate_identity_package("semantic-uuid-collision");
    let chosen = &participants[1];
    let handle = handle_for(chosen, DiagnosticCode::OrderIdDuplicate);
    let before = fs::read(order_record(chosen)).unwrap();
    let plan = crate::plan(
        &repair(
            order_scope(PathBuf::from(chosen)),
            &handle,
            RepairAction::AdoptNewOrderIdentity,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The generator hands back an identity the Package already carries. Resolving one
    // conflict by creating another is refused.
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[ORDER_ID])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a colliding identity is refused: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::GeneratedIdentityCollision { .. }),
        "got {rejection:?}"
    );
    assert_eq!(fs::read(order_record(chosen)).unwrap(), before);
}

#[test]
fn a_refused_repair_leaves_the_derived_package_index_exactly_as_it_found_it() {
    let (_tree, root, participants) = duplicate_identity_package("semantic-refused-index");
    let chosen = &participants[1];
    let handle = handle_for(chosen, DiagnosticCode::OrderIdDuplicate);
    let index_path = root.join(".spielorders/index.json");
    let index_before = fs::read(&index_path).unwrap();

    // The fixture rewrote two Order records after adoption last refreshed the index, so
    // the stored index still names identities no Order Directory carries any more. Any
    // rebuild therefore rewrites these bytes, which is what lets the comparison below
    // fail rather than pass for want of a difference.
    let stored = Index::from_json_bytes(&index_before).expect("the stored index parses");
    assert!(
        stored
            .orders
            .iter()
            .any(|entry| entry.order.order_id.deref() == TWIN_ID),
        "the stored index is out of date with the Package it describes"
    );

    let plan = crate::plan(
        &repair(
            order_scope(PathBuf::from(chosen)),
            &handle,
            RepairAction::AdoptNewOrderIdentity,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the chosen participant can adopt a new identity");

    // The generator hands back an identity the Package already carries, so the repair
    // refuses.
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[ORDER_ID])),
    )
    .result;

    assert!(
        report.is_err(),
        "a colliding identity is refused: {report:?}"
    );
    // Deciding whether to refuse must not itself rewrite a canonical artifact. A caller
    // told the repair was rejected has no reason to expect the Package changed, and the
    // derived Package Index is the Package's own durable record of itself.
    assert_eq!(
        fs::read(&index_path).unwrap(),
        index_before,
        "a refused repair rewrote the derived Package Index"
    );
}

#[test]
fn a_package_nothing_can_read_is_not_refused_as_a_generated_identity_collision() {
    let (_tree, root, participants) = duplicate_identity_package("semantic-no-package-evidence");
    let chosen = &participants[1];
    let handle = handle_for(chosen, DiagnosticCode::OrderIdDuplicate);
    let before = fs::read(order_record(chosen)).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(PathBuf::from(chosen)),
            &handle,
            RepairAction::AdoptNewOrderIdentity,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the chosen participant can adopt a new identity");

    // The Package version marker stops being readable after the plan was taken. Neither
    // the Order record nor the Vendor aggregate this commit revalidates is touched, so
    // the repair still reaches the question of whether its generated identity is free —
    // and no evidence can answer it.
    fs::write(
        root.join(".spielorders/version.json"),
        b"not a version marker",
    )
    .unwrap();

    let fresh = "3e6f4a8b-5c7d-4e9f-a0b1-2c3d4e5f6071";
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[fresh])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a Package nothing can read refuses the repair: {report:?}");
    };
    // Refusing is right: an identity that cannot be shown free is treated as taken
    // rather than assumed safe. Naming it a collision is not. A collision's remedy is to
    // retry with a fresh identity, and no identity whatsoever resolves a Package that
    // cannot be read, so a caller told that is sent to do something that can never work.
    // What the read established is the answer: here, a damaged Package.
    let Failure::DamagedPackage(damaged) = &rejection else {
        panic!("the refusal is what the Package's read established: {rejection:?}");
    };
    assert_eq!(damaged.context.root.absolute_path, root);
    assert_eq!(
        fs::read(order_record(chosen)).unwrap(),
        before,
        "the refusal wrote nothing"
    );
}

#[test]
fn a_created_vendor_cannot_take_a_name_an_existing_vendor_already_holds_in_another_case() {
    // Vendor Names are unique after trimming and ignoring letter case. Under full
    // Unicode case folding `Straße` and `STRASSE` are the same Name, so a repair that
    // created the second beside the first would write a package-duplicate Vendor Name
    // and damage the Package. ASCII lowercasing does not see it.
    for (existing, created) in [("Straße", "STRASSE"), ("STRASSE", "  straße  ")] {
        let (_tree, root, directory) = unresolved_vendor_order("semantic-folding");
        created_vendor(&root, existing, VENDOR_ID);
        let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
        let order_before = fs::read(order_record(&directory)).unwrap();

        let Err(error) = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::CreateAndAssignVendor,
                Assignments::new().assign("name", created),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        else {
            panic!("creating {created:?} beside {existing:?} is refused, not planned");
        };

        assert!(
            matches!(error, Failure::VendorNameInUse { .. }),
            "{created:?} beside {existing:?} got {error:?}"
        );
        assert_eq!(fs::read(order_record(&directory)).unwrap(), order_before);
        assert_eq!(vendors(&root).vendors.len(), 1);

        // The ordinary Vendor capability refuses the same pair, so the repair and
        // canonical Vendor validation cannot disagree about one Package's Names.
        assert!(
            crate::plan(
                &create_vendor(root.clone(), Assignments::new().assign("name", created)),
                &effects(&PlatformFileSystem)
            )
            .result
            .is_err(),
            "canonical Vendor creation refuses {created:?} beside {existing:?} too"
        );
    }
}

#[test]
fn a_generated_vendor_identity_that_already_exists_is_refused_before_either_record_is_written() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-vendor-uuid-collision");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let order_before = fs::read(order_record(&directory)).unwrap();
    let vendors_before = fs::read(root.join(".spielorders/vendors.json")).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a distinct Vendor can be planned");

    // The generator hands back an identity the Package's Vendor aggregate already
    // carries. Both Vendors are well-formed records, so the aggregate this assembles
    // parses perfectly and is still not a valid Package Vendor set: two Vendors sharing
    // one Vendor ID damage the whole Package. Deserializing the candidate cannot see
    // that, because it is a fact about the set rather than about either record.
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[VENDOR_ID])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a colliding Vendor identity is refused: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::GeneratedVendorIdentityCollision { .. }),
        "got {rejection:?}"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/vendors.json")).unwrap(),
        vendors_before,
        "the Vendor aggregate is exactly as it was"
    );
    assert_eq!(
        fs::read(order_record(&directory)).unwrap(),
        order_before,
        "the Order record is exactly as it was"
    );
    let aggregate = vendors(&root);
    assert_eq!(aggregate.vendors.len(), 1);
    assert!(
        diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .is_ok(),
        "the Package is still diagnosable, so no Vendor aggregate damage was introduced"
    );
}

#[test]
fn a_stale_locator_is_recomputed_from_the_physical_relationship_alone() {
    let (_tree, _root, directory) = stale_locator_order("semantic-locator");
    let handle = handle_for(&directory, DiagnosticCode::OrderPackageLocatorMismatch);

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::RepairPackageLocator,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a stale locator can be repaired");

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("a locator repair commits: {report:?}");
    };
    assert_eq!(json(&document(&directory).package_root), "../..");
    assert!(valid(
        &diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .expect("rediagnosed")
    ));
    // Every other fact is exactly as it was.
    assert_eq!(document(&directory).title.deref(), "Laser components");
    assert_eq!(document(&directory).order_id.deref(), ORDER_ID);
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
}

/// Plans and commits one repair under [`NESTED_ORDERS`], asserting that it reaches the
/// real Package rather than the ancestor a parent count infers.
fn repair_reaches_the_real_package(
    label: &str,
    root: &Path,
    directory: &Path,
    code: DiagnosticCode,
    resolution: &(RepairAction, Assignments),
    uuids: &[&str],
) {
    let handle = handle_for(directory, code);
    let plan = crate::plan(
        &repair(
            order_scope(directory),
            &handle,
            resolution.0,
            resolution.1.clone(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap_or_else(|error| panic!("{label} is planned under a nested Orders path: {error:?}"));

    assert_eq!(
        operation_path(&plan, "refresh_package_index"),
        root.join(".spielorders/index.json"),
        "{label} names the real Package's index rather than an inferred ancestor"
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(uuids)),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("{label} commits without warning under a nested Orders path: {report:?}");
    };
    let index = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the refreshed index parses");
    assert!(
        index
            .orders
            .iter()
            .any(|entry| root.join(&entry.order_directory) == directory),
        "{label} refreshed the real Package's derived index"
    );
    assert!(
        valid(
            &diagnosed_order(directory, &PlatformFileSystem)
                .result
                .expect("the scope rediagnoses")
        ),
        "{label} leaves the repaired scope healthy"
    );
    assert!(
        !root.join("Purchases/.spielorders").exists(),
        "{label} writes nothing to the ancestor a parent count would have inferred"
    );
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
}

#[test]
fn every_semantic_repair_reaches_the_real_package_under_a_nested_orders_path() {
    // The Orders Directory is configured, so an Order Directory can sit at any depth
    // below the SpielOrders Root. Counting parents from it names an ordinary folder of
    // the user's, and a repair that wrote there would rewrite the wrong Package or
    // refuse a healthy one.
    let (_tree, root, directory) = unresolved_vendor_order_at("semantic-nested", NESTED_ORDERS);
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    repair_reaches_the_real_package(
        "assigning an existing Vendor",
        &root,
        &directory,
        DiagnosticCode::VendorReferenceUnresolved,
        &(
            RepairAction::AssignExistingVendor,
            Assignments::new().assign("vendor", "Tarnwick"),
        ),
        &[],
    );

    let (_tree, root, directory) = unresolved_vendor_order_at("semantic-nested", NESTED_ORDERS);
    repair_reaches_the_real_package(
        "creating and assigning a Vendor",
        &root,
        &directory,
        DiagnosticCode::VendorReferenceUnresolved,
        &(
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &[NEW_VENDOR_ID],
    );
    assert_eq!(
        vendors(&root).vendors.len(),
        1,
        "the created Vendor joined the real Package's Vendor aggregate"
    );

    let (_tree, root, directory) = unresolved_vendor_order_at("semantic-nested", NESTED_ORDERS);
    repair_reaches_the_real_package(
        "clearing the Vendor Reference",
        &root,
        &directory,
        DiagnosticCode::VendorReferenceUnresolved,
        &(RepairAction::ClearVendorReference, Assignments::new()),
        &[],
    );

    let (_tree, root, participants) =
        duplicate_identity_package_at("semantic-nested", NESTED_ORDERS);
    let fresh = "9f8e7d6c-5b4a-4392-8271-605f4e3d2c1b";
    repair_reaches_the_real_package(
        "adopting a fresh Order identity",
        &root,
        &participants[1],
        DiagnosticCode::OrderIdDuplicate,
        &(RepairAction::AdoptNewOrderIdentity, Assignments::new()),
        &[fresh],
    );
    assert_eq!(document(&participants[1]).order_id.deref(), fresh);

    let (_tree, root, directory) = stale_locator_order_at("semantic-nested", NESTED_ORDERS);
    repair_reaches_the_real_package(
        "recomputing the technical locator",
        &root,
        &directory,
        DiagnosticCode::OrderPackageLocatorMismatch,
        &(RepairAction::RepairPackageLocator, Assignments::new()),
        &[],
    );
    assert_eq!(
        json(&document(&directory).package_root),
        "../../..",
        "the locator counts the real distance to the SpielOrders Root"
    );
}

#[test]
fn a_changed_conflict_set_or_record_rejects_before_acting() {
    // The record changed since the plan was built.
    let (_tree, _root, directory) = unresolved_vendor_order("semantic-record-changed");
    created_vendor(&_root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::AssignExistingVendor,
            Assignments::new().assign("vendor", "  tarNWICK "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let text = fs::read_to_string(order_record(&directory))
        .unwrap()
        .replace("Laser components", "Renamed by someone else");
    fs::write(order_record(&directory), &text).unwrap();

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a changed record rejects: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::EvidenceChanged { .. }),
        "got {rejection:?}"
    );
    assert_eq!(fs::read_to_string(order_record(&directory)).unwrap(), text);

    // The Vendor aggregate changed since the plan was built.
    let (_tree, root, directory) = unresolved_vendor_order("semantic-vendors-changed");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    created_vendor(&root, "Elderglass", NEW_VENDOR_ID);
    let record_before = fs::read(order_record(&directory)).unwrap();

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(QueuedUuids::of(&["3a4b5c6d-7e8f-4a0b-9c1d-2e3f4a5b6c7d"])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a changed Vendor aggregate rejects: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::EvidenceChanged { .. }),
        "got {rejection:?}"
    );
    assert_eq!(fs::read(order_record(&directory)).unwrap(), record_before);
    assert_eq!(vendors(&root).vendors.len(), 2, "no third Vendor was added");
}

#[test]
fn a_stale_handle_or_an_unauthorized_resolution_is_refused() {
    let (_tree, root, directory) = unresolved_vendor_order("semantic-stale");
    let existing = created_vendor(&root, "Tarnwick", VENDOR_ID);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);

    // The condition is corrected by other means; the handle no longer names a case.
    let text = fs::read_to_string(order_record(&directory))
        .unwrap()
        .replace(ABSENT_VENDOR, &existing);
    fs::write(order_record(&directory), &text).unwrap();
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ClearVendorReference,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a stale handle is refused");
    assert!(
        matches!(error, Failure::UnknownCase { .. }),
        "got {error:?}"
    );
    assert_eq!(fs::read_to_string(order_record(&directory)).unwrap(), text);

    // A locator case does not authorize a Vendor resolution.
    let (_tree, _root, directory) = stale_locator_order("semantic-unauthorized");
    let handle = handle_for(&directory, DiagnosticCode::OrderPackageLocatorMismatch);
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ClearVendorReference,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a locator case authorizes only the locator repair");
    assert!(
        matches!(error, Failure::UnauthorizedAction { .. }),
        "got {error:?}"
    );
}

/// The reachable data-loss path. A create-and-assign repair
/// writes the Order record and the Vendor aggregate together; if someone else rewrites
/// the Order after the repair confirmed it, the repair must not overwrite their work.
#[test]
fn a_repair_does_not_overwrite_an_order_someone_else_rewrote_mid_commit() {
    let (_tree, _root, directory) = unresolved_vendor_order("semantic-concurrent-order");
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let order_path = order_record(&directory);

    // What the other writer leaves behind: the same Order record with a different
    // Title. A realistic concurrent edit, not a corruption.
    let their_bytes = String::from_utf8(fs::read(&order_path).unwrap())
        .unwrap()
        .replace("Laser components", "Rewritten by someone else")
        .into_bytes();
    assert_ne!(
        their_bytes,
        fs::read(&order_path).unwrap(),
        "the fixture edit really differs"
    );

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Vendor can be created and assigned");

    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &order_path,
            &their_bytes,
            EditPoint::ReadEndingIn("vendors.json"),
        ))
        .minting(QueuedUuids::of(&[VENDOR_ID])),
    )
    .result;

    assert!(
        report.is_err(),
        "a repair whose subject was rewritten underneath it does not commit: {report:?}"
    );
    // Their bytes survive intact.
    assert_eq!(
        fs::read(&order_path).unwrap(),
        their_bytes,
        "the other writer's Order record is untouched"
    );
}

/// A create-and-assign repair writes the Order record and then the Vendor aggregate,
/// and the aggregate's replacement fails. `script` adds what else happens around it.
fn vendor_write_failing(
    label: &str,
    script: impl FnOnce(&ScriptedFileSystem, &Path),
) -> (
    TemporaryTree,
    PathBuf,
    Vec<u8>,
    Vec<u8>,
    Result<Report, Failure>,
) {
    let (tree, root, directory) = unresolved_vendor_order(label);
    let handle = handle_for(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let order_before = fs::read(order_record(&directory)).unwrap();
    let vendors_before = fs::read(root.join(".spielorders/vendors.json")).unwrap();
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Vendor can be created and assigned");
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::ReplaceFile)
        .ending_in(".spielorders/vendors.json")
        .fails();
    script(&filesystem, &order_record(&directory));
    let report = crate::commit(
        plan,
        &effects(&filesystem).minting(QueuedUuids::of(&[NEW_VENDOR_ID])),
    )
    .result;
    assert_eq!(
        fs::read(root.join(".spielorders/vendors.json")).unwrap(),
        vendors_before,
        "the failed aggregate replacement wrote nothing"
    );
    (tree, directory, order_before, vendors_before, report)
}

fn failed_commit(report: Result<Report, Failure>) -> Failure {
    let Err(failed) = report else {
        panic!("the write stopped: {report:?}");
    };
    failed
}

#[test]
fn a_failed_second_write_puts_the_first_back_as_it_was() {
    let (_tree, directory, order_before, _, report) =
        vendor_write_failing("semantic-rollback", |_, _| {});

    let failed = failed_commit(report);
    assert!(matches!(failed, Failure::CommitFailed { .. }), "{failed:?}");
    assert_reported_beside(&failed);
    assert!(
        failed
            .failure()
            .location
            .absolute_path
            .ends_with(".spielorders/vendors.json")
    );
    assert_eq!(fs::read(order_record(&directory)).unwrap(), order_before);
}

#[test]
fn a_restoration_that_fails_is_named_beside_the_failure_that_asked_for_it() {
    let (_tree, directory, order_before, _, report) =
        vendor_write_failing("semantic-rollback-fails", |filesystem, _| {
            filesystem
                .on(Operation::ReplaceFile)
                .ending_in(".spielorders/order.json")
                .after_an_injected_failure()
                .fails();
        });

    let failed = failed_commit(report);
    let [cleanup] = failed.cleanup_failures() else {
        panic!("one restoration failed: {failed:?}");
    };
    assert_eq!(cleanup.location.absolute_path, order_record(&directory));
    assert_ne!(
        fs::read(order_record(&directory)).unwrap(),
        order_before,
        "the record holds the repair it could not take back, as the failure says"
    );
}

#[test]
fn a_restoration_never_writes_over_a_record_someone_else_changed_since() {
    let their_bytes = b"{ \"theirs\": true }".to_vec();
    let theirs = their_bytes.clone();
    let (_tree, directory, _, _, report) =
        vendor_write_failing("semantic-rollback-refused", move |filesystem, order| {
            let order = order.to_path_buf();
            filesystem
                .on(Operation::ReplaceFile)
                .ending_in(".spielorders/vendors.json")
                .once()
                .preceded_by(move |outside| outside.write(&order, &theirs))
                .fails();
        });

    let failed = failed_commit(report);
    let [_cleanup] = failed.cleanup_failures() else {
        panic!("one restoration was declined: {failed:?}");
    };
    assert_eq!(fs::read(order_record(&directory)).unwrap(), their_bytes);
}
