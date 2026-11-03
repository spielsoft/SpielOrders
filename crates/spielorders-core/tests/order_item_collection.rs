//! Decision sources: Define the Order record and lifecycle contract; Define the
//! Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{
    OrderAdoptionInput, OrderItemInput, commit_order_adoption, plan_order_adoption,
};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderDocument, OrderStatus};
use spielorders_core::mutation::{MutationReport, RejectedAssignmentReason};
use spielorders_core::operations::OperationFailureCode;
use spielorders_core::order_edits::{
    CanonicalItemOutcome, ItemAssignments, ItemPositionRole, NewItemAssignmentFailure,
    OrderEditOperationKind, OrderEditPlanError, OrderEditRejection, OrderEditResult,
    OrderEditWarning, OrderItemMutationKind, OrderItemMutationPlan, commit_order_item_mutation,
    list_order_items, plan_order_item_add, plan_order_item_edit, plan_order_item_move,
    plan_order_item_remove,
};
use spielorders_core::package::ReusableValue;
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, UuidGenerator,
};
use spielorders_core::vendors::{
    CanonicalContactOutcome, ContactPositionRole, VendorAssignments, commit_vendor_mutation,
    plan_vendor_creation,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

fn initialized_package(label: &str) -> TemporaryTree {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::default(),
        &RealFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    tree
}

/// Four deliberately asymmetric Items. Every Description, Quantity, Unit Price,
/// Item Total, and Catalog Number differs, so a position confused for its
/// neighbour, a swap standing in for a move, or a total taken from the wrong row
/// changes an assertion instead of surviving it.
fn adopted_order(root: &Path, items: Vec<OrderItemInput>) -> PathBuf {
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    let mut input = OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold)
        .orderer("Ada Lovelace")
        .currency("USD");
    for item in items {
        input = input.item(item);
    }
    let plan = plan_order_adoption(root, &directory, &input, &RealFileSystem)
        .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn four_items() -> Vec<OrderItemInput> {
    vec![
        OrderItemInput::new("Mirror mount", 2.0, 84.5).catalog_number("TM-100"),
        OrderItemInput::new("Lens", 3.0, 10.0),
        OrderItemInput::new("Optical post", 5.0, 1.5).catalog_number("TR3"),
        OrderItemInput::new("Breadboard", 1.0, 220.0),
    ]
}

fn four_item_order(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = initialized_package(label);
    let directory = adopted_order(tree.path(), four_items());
    (tree, directory)
}

fn descriptions(items: &[spielorders_core::metadata::OrderItem]) -> Vec<&str> {
    items.iter().map(|item| item.description.as_str()).collect()
}

#[test]
fn listing_reports_the_exact_user_order_and_recalculated_unstored_totals() {
    let (_tree, directory) = four_item_order("order-items-list");

    let listed = list_order_items(&directory, &RealFileSystem).expect("the Items can be listed");

    assert_eq!(
        descriptions(&listed.document().items),
        ["Mirror mount", "Lens", "Optical post", "Breadboard"],
        "listing preserves the exact user-controlled order without sorting"
    );
    assert_eq!(listed.item_totals(), [169.0, 30.0, 7.5, 220.0]);
    assert_eq!(listed.order_total(), 426.5);
    assert_eq!(
        listed.document().items[0].catalog_number.as_deref(),
        Some("TM-100")
    );
    assert_eq!(listed.document().items[1].catalog_number, None);

    // Totals are calculated, never stored: the canonical record carries no total.
    let canonical = fs::read_to_string(directory.join(".spielorders/order.json")).unwrap();
    assert!(!canonical.contains("total"), "totals are never stored");
    assert_eq!(listed.document().order_id.as_str(), ORDER_ID);
}

#[test]
fn an_appended_item_plan_reports_the_refreshed_order_and_writes_nothing() {
    let (tree, directory) = four_item_order("order-items-add-plan");
    let order_path = directory.join(".spielorders/order.json");
    let index_path = tree.path().join(".spielorders/index.json");
    let order_before = fs::read(&order_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let plan = plan_order_item_add(
        &directory,
        &ItemAssignments::new()
            .assign("description", "  Fiber collimator  ")
            .assign("quantity", "4")
            .assign("unit_price", "3.0"),
        None,
        &RealFileSystem,
    )
    .expect("an Item can be appended to an existing Order");

    // This fixture carries an Orderer and no Project, so retention has to report
    // both an existing value and a genuinely absent one.
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::Reused("Ada Lovelace".into())
    );
    assert_eq!(plan.reusable_values().project(), &ReusableValue::Absent);

    assert_eq!(plan.kind(), OrderItemMutationKind::Add);
    assert_eq!(plan.source_position(), None);
    assert_eq!(
        plan.destination_position(),
        Some(5),
        "an omitted position appends after the complete pre-mutation collection"
    );
    assert_eq!(plan.subject_item().description.as_str(), "Fiber collimator");
    assert_eq!(plan.subject_item().quantity.get(), 4.0);
    assert_eq!(plan.subject_item().unit_price.get(), 3.0);
    assert_eq!(plan.subject_item().catalog_number, None);

    assert_eq!(
        descriptions(plan.facts().items()),
        [
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard",
            "Fiber collimator"
        ]
    );
    assert_eq!(
        plan.facts().item_totals(),
        [169.0, 30.0, 7.5, 220.0, 12.0],
        "every Item Total is recalculated, not only the appended one"
    );
    assert_eq!(plan.facts().order_total(), 438.5);

    // An Item mutation is not a fact edit: every scalar fact is retained exactly.
    assert_eq!(plan.facts().order_id(), ORDER_ID);
    assert_eq!(plan.facts().title(), "Laser components");
    assert_eq!(plan.facts().date_ordered(), "2026-08-22");
    assert_eq!(plan.facts().status(), OrderStatus::Hold);
    assert_eq!(plan.facts().orderer(), Some("Ada Lovelace"));
    assert_eq!(plan.facts().currency(), Some("USD"));

    assert!(plan.changes_canonical_order());
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| (
                operation.kind(),
                operation
                    .path()
                    .strip_prefix(plan.root())
                    .unwrap()
                    .to_owned()
            ))
            .collect::<Vec<_>>(),
        [
            (
                OrderEditOperationKind::ReplaceOrderRecord,
                Path::new("Orders/Legacy_laser_purchase/.spielorders/order.json").to_owned()
            ),
            (
                OrderEditOperationKind::RefreshPackageIndex,
                Path::new(".spielorders/index.json").to_owned()
            ),
        ]
    );
    assert_eq!(plan.directory(), fs::canonicalize(&directory).unwrap());
    assert_eq!(fs::read(&order_path).unwrap(), order_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

fn committed(plan: OrderItemMutationPlan) -> OrderEditResult {
    let report = commit_order_item_mutation(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the Item mutation commits: {report:?}");
    };
    result
}

fn persisted(directory: &Path) -> OrderDocument {
    OrderDocument::from_json_bytes(&fs::read(directory.join(".spielorders/order.json")).unwrap())
        .expect("the canonical Order parses")
}

fn indexed(root: &Path) -> OrderDocument {
    let index =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    index.orders[0].order.clone()
}

#[test]
fn appending_commits_the_item_last_and_refreshes_the_derived_index() {
    let (tree, directory) = four_item_order("order-items-append-commit");
    let plan = plan_order_item_add(
        &directory,
        &ItemAssignments::new()
            .assign("description", "Fiber collimator")
            .assign("quantity", "4")
            .assign("unit_price", "3.0"),
        None,
        &RealFileSystem,
    )
    .expect("the Item can be appended");

    let result = committed(plan);

    assert_eq!(
        descriptions(&result.order().document().items),
        [
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard",
            "Fiber collimator"
        ]
    );
    assert_eq!(
        result.order().item_totals(),
        [169.0, 30.0, 7.5, 220.0, 12.0]
    );
    assert_eq!(result.order().order_total(), 438.5);
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);

    assert_eq!(
        descriptions(&persisted(&directory).items),
        [
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard",
            "Fiber collimator"
        ],
        "the canonical record carries the committed order"
    );
    assert_eq!(
        descriptions(&indexed(tree.path()).items),
        [
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard",
            "Fiber collimator"
        ]
    );
    assert!(
        !fs::read_to_string(directory.join(".spielorders/order.json"))
            .unwrap()
            .contains("total"),
        "a committed Item mutation stores no calculated total"
    );
}

fn assert_invalid_position(
    error: &OrderEditPlanError<io::Error>,
    expected_role: ItemPositionRole,
    expected_position: usize,
    expected_maximum: usize,
) {
    let OrderEditPlanError::InvalidItemPosition {
        role,
        position,
        minimum,
        maximum,
    } = error
    else {
        panic!("an out-of-range position is an invalid Item position: {error:?}");
    };
    assert_eq!(
        (*role, *position, *minimum, *maximum),
        (expected_role, expected_position, 1, expected_maximum)
    );
}

fn collimator() -> ItemAssignments {
    ItemAssignments::new()
        .assign("description", "Fiber collimator")
        .assign("quantity", "4")
        .assign("unit_price", "3.0")
}

fn added_at(directory: &Path, position: Option<usize>) -> OrderEditResult {
    let plan = plan_order_item_add(directory, &collimator(), position, &RealFileSystem)
        .unwrap_or_else(|error| panic!("an Item can be added at {position:?}: {error:?}"));
    committed(plan)
}

#[test]
fn an_explicit_position_inserts_before_the_pre_mutation_item_at_that_position() {
    let (_tree, directory) = four_item_order("order-items-insert-middle");

    let result = added_at(&directory, Some(2));

    assert_eq!(
        descriptions(&result.order().document().items),
        [
            "Mirror mount",
            "Fiber collimator",
            "Lens",
            "Optical post",
            "Breadboard"
        ],
        "position 2 places the Item before the pre-mutation second Item"
    );
    assert_eq!(
        result.order().item_totals(),
        [169.0, 12.0, 30.0, 7.5, 220.0]
    );
    assert_eq!(result.order().order_total(), 438.5);
    assert_eq!(descriptions(&persisted(&directory).items).len(), 5);
}

#[test]
fn insert_boundaries_cover_the_first_position_and_the_one_past_the_last() {
    let (_tree, first) = four_item_order("order-items-insert-first");
    let result = added_at(&first, Some(1));
    assert_eq!(
        descriptions(&result.order().document().items),
        [
            "Fiber collimator",
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard"
        ]
    );

    // An add alone may name one position beyond the last Item; that is the append.
    let (_tree, last) = four_item_order("order-items-insert-past-last");
    let result = added_at(&last, Some(5));
    assert_eq!(
        descriptions(&result.order().document().items),
        [
            "Mirror mount",
            "Lens",
            "Optical post",
            "Breadboard",
            "Fiber collimator"
        ]
    );

    let (_tree, rejected) = four_item_order("order-items-insert-out-of-range");
    let before = fs::read(rejected.join(".spielorders/order.json")).unwrap();
    for position in [0, 6] {
        let error = plan_order_item_add(&rejected, &collimator(), Some(position), &RealFileSystem)
            .expect_err("a position outside the collection is refused");
        assert_invalid_position(&error, ItemPositionRole::Destination, position, 5);
    }
    assert_eq!(
        fs::read(rejected.join(".spielorders/order.json")).unwrap(),
        before
    );
}

#[test]
fn editing_one_position_changes_only_that_item_and_recalculates_every_total() {
    let (_tree, directory) = four_item_order("order-items-edit");

    let plan = plan_order_item_edit(
        &directory,
        3,
        &ItemAssignments::new()
            .assign("quantity", "8")
            .assign("unit_price", "2.0"),
        &RealFileSystem,
    )
    .expect("the third Item can be edited");

    assert_eq!(plan.kind(), OrderItemMutationKind::Edit);
    assert_eq!(plan.source_position(), Some(3));
    assert_eq!(plan.destination_position(), Some(3));
    // The subject is the edited Item at that position, carrying the assigned facts.
    assert_eq!(plan.subject_item().description.as_str(), "Optical post");
    assert_eq!(plan.subject_item().quantity.get(), 8.0);
    assert_eq!(plan.subject_item().unit_price.get(), 2.0);
    assert_eq!(plan.subject_item().catalog_number.as_deref(), Some("TR3"));

    let result = committed(plan);
    let items = &result.order().document().items;
    assert_eq!(
        descriptions(items),
        ["Mirror mount", "Lens", "Optical post", "Breadboard"],
        "an edit changes no position"
    );
    assert_eq!(items[2].description.as_str(), "Optical post");
    assert_eq!(items[2].quantity.get(), 8.0);
    assert_eq!(items[2].unit_price.get(), 2.0);
    assert_eq!(
        items[2].catalog_number.as_deref(),
        Some("TR3"),
        "an unassigned Item fact is retained"
    );
    // The three untouched Items keep their exact stored facts.
    assert_eq!(items[0].quantity.get(), 2.0);
    assert_eq!(items[1].unit_price.get(), 10.0);
    assert_eq!(items[3].unit_price.get(), 220.0);
    assert_eq!(result.order().item_totals(), [169.0, 30.0, 16.0, 220.0]);
    assert_eq!(result.order().order_total(), 435.0);
    assert_eq!(persisted(&directory).items[2].quantity.get(), 8.0);
}

#[test]
fn editing_clears_the_optional_catalog_number_and_keeps_the_required_facts() {
    let (_tree, directory) = four_item_order("order-items-clear-catalog");

    let plan = plan_order_item_edit(
        &directory,
        1,
        &ItemAssignments::new().assign("catalog_number", "   "),
        &RealFileSystem,
    )
    .expect("the optional Catalog Number can be cleared");

    let result = committed(plan);
    let item = &result.order().document().items[0];
    assert_eq!(
        item.catalog_number, None,
        "an empty value clears the optional fact"
    );
    assert_eq!(item.description.as_str(), "Mirror mount");
    assert_eq!(item.quantity.get(), 2.0);
    assert_eq!(item.unit_price.get(), 84.5);
    assert_eq!(result.order().item_totals(), [169.0, 30.0, 7.5, 220.0]);

    // The third Item's Catalog Number is untouched, so clearing is not collection-wide.
    assert_eq!(
        result.order().document().items[2].catalog_number.as_deref(),
        Some("TR3")
    );
    assert_eq!(persisted(&directory).items[0].catalog_number, None);
}

fn moved(directory: &Path, source: usize, destination: usize, subject: &str) -> OrderEditResult {
    let plan = plan_order_item_move(directory, source, destination, &RealFileSystem)
        .unwrap_or_else(|error| panic!("Item {source} can move to {destination}: {error:?}"));
    assert_eq!(plan.kind(), OrderItemMutationKind::Move);
    assert_eq!(plan.source_position(), Some(source));
    assert_eq!(plan.destination_position(), Some(destination));
    // The plan names the Item that actually moved, not whichever Item happens to
    // sit at a fixed end of the collection.
    assert_eq!(plan.subject_item().description.as_str(), subject);
    committed(plan)
}

#[test]
fn moving_an_item_reorders_against_the_pre_mutation_positions() {
    let (_tree, directory) = four_item_order("order-items-move");

    // Moving 1 to 3 lifts the Item out and reinserts it at the named position.
    // A swap would give Optical post, Lens, Mirror mount, Breadboard; an index
    // adjusted for the removal would give Lens, Mirror mount, Optical post,
    // Breadboard. Four distinct Items separate all three outcomes.
    let result = moved(&directory, 1, 3, "Mirror mount");

    assert_eq!(
        descriptions(&result.order().document().items),
        ["Lens", "Optical post", "Mirror mount", "Breadboard"]
    );
    assert_eq!(result.order().item_totals(), [30.0, 7.5, 169.0, 220.0]);
    assert_eq!(
        result.order().order_total(),
        426.5,
        "reordering changes no total"
    );
    assert_eq!(
        descriptions(&persisted(&directory).items),
        ["Lens", "Optical post", "Mirror mount", "Breadboard"]
    );
}

#[test]
fn moving_between_the_first_and_last_positions_covers_both_boundaries() {
    let (_tree, to_front) = four_item_order("order-items-move-to-front");
    assert_eq!(
        descriptions(
            &moved(&to_front, 4, 1, "Breadboard")
                .order()
                .document()
                .items
        ),
        ["Breadboard", "Mirror mount", "Lens", "Optical post"]
    );

    let (_tree, to_back) = four_item_order("order-items-move-to-back");
    assert_eq!(
        descriptions(
            &moved(&to_back, 1, 4, "Mirror mount")
                .order()
                .document()
                .items
        ),
        ["Lens", "Optical post", "Breadboard", "Mirror mount"]
    );

    // A move names two positions that both exist, so neither end reaches past the
    // last Item the way an append may.
    let (_tree, rejected) = four_item_order("order-items-move-out-of-range");
    let before = fs::read(rejected.join(".spielorders/order.json")).unwrap();
    for (source, destination, role, offending) in [
        (0, 2, ItemPositionRole::Source, 0),
        (5, 2, ItemPositionRole::Source, 5),
        (2, 0, ItemPositionRole::Destination, 0),
        (2, 5, ItemPositionRole::Destination, 5),
    ] {
        let error = plan_order_item_move(&rejected, source, destination, &RealFileSystem)
            .expect_err("a position outside the collection is refused");
        assert_invalid_position(&error, role, offending, 4);
    }
    assert_eq!(
        fs::read(rejected.join(".spielorders/order.json")).unwrap(),
        before
    );
}

#[test]
fn moving_an_item_to_its_own_position_reports_unchanged_without_replacing_bytes() {
    let (tree, directory) = four_item_order("order-items-move-no-op");
    let order_before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let index_before = fs::read(tree.path().join(".spielorders/index.json")).unwrap();

    let plan = plan_order_item_move(&directory, 2, 2, &RealFileSystem)
        .expect("a same-position move is planned like any other");
    assert!(!plan.changes_canonical_order());
    assert!(
        plan.operations().is_empty(),
        "a plan that replaces nothing announces no managed operation"
    );

    let report = commit_order_item_mutation(plan, &RealFileSystem);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a same-position move is an accepted no-op: {report:?}");
    };
    assert_eq!(
        descriptions(&result.order().document().items),
        ["Mirror mount", "Lens", "Optical post", "Breadboard"]
    );
    assert_eq!(result.order().order_total(), 426.5);
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        order_before
    );
    assert_eq!(
        fs::read(tree.path().join(".spielorders/index.json")).unwrap(),
        index_before
    );

    // Reassigning an Item's stored facts is the same accepted no-op.
    let plan = plan_order_item_edit(
        &directory,
        1,
        &ItemAssignments::new()
            .assign("description", "Mirror mount")
            .assign("quantity", "2")
            .assign("unit_price", "84.5")
            .assign("catalog_number", "TM-100"),
        &RealFileSystem,
    )
    .expect("an edit that changes nothing is planned like any other");
    assert!(!plan.changes_canonical_order());
    let report = commit_order_item_mutation(plan, &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Unchanged { .. }),
        "an edit that changes nothing is an accepted no-op: {report:?}"
    );
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        order_before
    );
}

#[test]
fn removing_one_position_drops_exactly_that_item_and_recalculates_totals() {
    let (_tree, directory) = four_item_order("order-items-remove");

    let plan = plan_order_item_remove(&directory, 2, &RealFileSystem)
        .expect("the second Item can be removed");

    assert_eq!(plan.kind(), OrderItemMutationKind::Remove);
    assert_eq!(plan.source_position(), Some(2));
    assert_eq!(plan.destination_position(), None);
    assert_eq!(
        plan.subject_item().description.as_str(),
        "Lens",
        "the removed Item is reported from the collection it is leaving"
    );

    let result = committed(plan);
    assert_eq!(
        descriptions(&result.order().document().items),
        ["Mirror mount", "Optical post", "Breadboard"]
    );
    assert_eq!(result.order().item_totals(), [169.0, 7.5, 220.0]);
    assert_eq!(result.order().order_total(), 396.5);
    assert_eq!(
        descriptions(&persisted(&directory).items),
        ["Mirror mount", "Optical post", "Breadboard"]
    );
}

#[test]
fn removing_the_last_item_leaves_an_empty_collection_addressable_only_by_an_append() {
    let tree = initialized_package("order-items-remove-last");
    let directory = adopted_order(
        tree.path(),
        vec![OrderItemInput::new("Mirror mount", 2.0, 84.5)],
    );

    let result = committed(
        plan_order_item_remove(&directory, 1, &RealFileSystem).expect("the only Item is removable"),
    );

    assert!(result.order().document().items.is_empty());
    assert_eq!(result.order().item_totals(), [] as [f64; 0]);
    assert_eq!(result.order().order_total(), 0.0);
    assert!(persisted(&directory).items.is_empty());

    // An empty collection has no position to edit, move, or remove, and exactly one
    // position an add may name.
    for position in [1, 2] {
        let error = plan_order_item_edit(
            &directory,
            position,
            &ItemAssignments::new().assign("quantity", "1"),
            &RealFileSystem,
        )
        .expect_err("an empty collection has no editable position");
        assert_invalid_position(&error, ItemPositionRole::Source, position, 0);
    }
    let error = plan_order_item_remove(&directory, 1, &RealFileSystem)
        .expect_err("an empty collection has no removable position");
    assert_invalid_position(&error, ItemPositionRole::Source, 1, 0);
    let error = plan_order_item_move(&directory, 1, 1, &RealFileSystem)
        .expect_err("an empty collection has no movable position");
    assert_invalid_position(&error, ItemPositionRole::Source, 1, 0);
    let error = plan_order_item_add(&directory, &collimator(), Some(2), &RealFileSystem)
        .expect_err("an empty collection accepts only position 1");
    assert_invalid_position(&error, ItemPositionRole::Destination, 2, 1);

    let result = added_at(&directory, Some(1));
    assert_eq!(
        descriptions(&result.order().document().items),
        ["Fiber collimator"]
    );
    assert_eq!(result.order().order_total(), 12.0);
}

#[test]
fn zero_and_negative_quantities_and_unit_prices_are_contract_valid() {
    let (_tree, directory) = four_item_order("order-items-signs");

    let zero = committed(
        plan_order_item_add(
            &directory,
            &ItemAssignments::new()
                .assign("description", "Loaner mount")
                .assign("quantity", "0")
                .assign("unit_price", "125.50"),
            None,
            &RealFileSystem,
        )
        .expect("a zero Quantity is contract valid"),
    );
    assert_eq!(zero.order().item_totals()[4], 0.0);
    assert_eq!(zero.order().order_total(), 426.5);

    // Fees, shipping, and discounts are ordinary Items, so a negative Unit Price
    // and a negative Quantity are both accepted and summed.
    let signed = committed(
        plan_order_item_add(
            &directory,
            &ItemAssignments::new()
                .assign("description", "Volume discount")
                .assign("quantity", "1")
                .assign("unit_price", "-40.25"),
            None,
            &RealFileSystem,
        )
        .expect("a negative Unit Price is contract valid"),
    );
    assert_eq!(signed.order().item_totals()[5], -40.25);
    assert_eq!(signed.order().order_total(), 386.25);

    let returned = committed(
        plan_order_item_add(
            &directory,
            &ItemAssignments::new()
                .assign("description", "Returned lens")
                .assign("quantity", "-2")
                .assign("unit_price", "10.0"),
            None,
            &RealFileSystem,
        )
        .expect("a negative Quantity is contract valid"),
    );
    assert_eq!(returned.order().item_totals()[6], -20.0);
    assert_eq!(returned.order().order_total(), 366.25);
    assert_eq!(persisted(&directory).items[4].quantity.get(), 0.0);
    assert_eq!(persisted(&directory).items[6].quantity.get(), -2.0);
}

#[test]
fn totals_are_exact_products_and_sums_without_rounding_or_conversion() {
    let tree = initialized_package("order-items-exact-totals");
    let directory = adopted_order(
        tree.path(),
        vec![
            OrderItemInput::new("Mirror mount", 1.0, 0.1),
            OrderItemInput::new("Lens", 3.0, 0.2),
        ],
    );

    let listed = list_order_items(&directory, &RealFileSystem).expect("the Items can be listed");

    // Two decimal places are a display concern. Both the Item Total and the Order
    // Total are the exact floating-point product and sum, so an implementation that
    // rounded either one would report 0.6 and 0.7 here.
    assert_eq!(listed.item_totals(), [1.0f64 * 0.1, 3.0f64 * 0.2]);
    assert_ne!(listed.item_totals()[1], 0.6);
    assert_eq!(listed.order_total(), 1.0f64 * 0.1 + 3.0f64 * 0.2);
    assert_ne!(listed.order_total(), 0.7);

    // The Order Currency decorates those numbers; it never converts them.
    let result = committed(
        plan_order_item_edit(
            &directory,
            1,
            &ItemAssignments::new().assign("quantity", "3"),
            &RealFileSystem,
        )
        .expect("the first Item can be edited"),
    );
    assert_eq!(
        result
            .order()
            .document()
            .currency
            .as_ref()
            .map(|currency| currency.as_str()),
        Some("USD")
    );
    assert_eq!(result.order().item_totals(), [3.0f64 * 0.1, 3.0f64 * 0.2]);
    assert_eq!(result.order().order_total(), 3.0f64 * 0.1 + 3.0f64 * 0.2);
    assert_eq!(persisted(&directory).items[1].unit_price.get(), 0.2);
}

fn assert_rejected(
    error: &OrderEditPlanError<io::Error>,
    expected: &[(&str, RejectedAssignmentReason)],
) {
    let OrderEditPlanError::InvalidAssignments(rejected) = error else {
        panic!("an invalid assignment set is refused as invalid assignments: {error:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|rejected| (rejected.field(), rejected.reason()))
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(field, reason)| (*field, *reason))
            .collect::<Vec<_>>()
    );
}

#[test]
fn blank_required_facts_and_unrepresentable_numbers_reject_atomically() {
    use RejectedAssignmentReason::{BlankRequiredValue, NotAFiniteNumber};

    let (tree, directory) = four_item_order("order-items-invalid-values");
    let order_before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let index_before = fs::read(tree.path().join(".spielorders/index.json")).unwrap();

    for (field, value, reason) in [
        ("description", "   ", BlankRequiredValue),
        ("quantity", "  ", BlankRequiredValue),
        ("unit_price", "", BlankRequiredValue),
        ("quantity", "abc", NotAFiniteNumber),
        ("quantity", "NaN", NotAFiniteNumber),
        ("unit_price", "inf", NotAFiniteNumber),
        // Finite as decimal text, but not representable as the supported type.
        ("unit_price", "1e400", NotAFiniteNumber),
    ] {
        // Each offending assignment travels with a valid one that would otherwise
        // have replaced bytes, so the whole set must be refused together.
        let assignments = ItemAssignments::new()
            .assign("catalog_number", "REPLACED")
            .assign(field, value);
        let error = plan_order_item_edit(&directory, 1, &assignments, &RealFileSystem)
            .err()
            .unwrap_or_else(|| panic!("{field}={value:?} must be refused"));
        assert_rejected(&error, &[(field, reason)]);

        // The same offending value refuses an otherwise complete added Item.
        let mut added = ItemAssignments::new();
        for (required, valid) in [
            ("description", "Fiber collimator"),
            ("quantity", "4"),
            ("unit_price", "3.0"),
        ] {
            let supplied = if required == field { value } else { valid };
            added = added.assign(required, supplied);
        }
        let error = plan_order_item_add(&directory, &added, None, &RealFileSystem)
            .err()
            .unwrap_or_else(|| panic!("an added Item refuses {field}={value:?}"));
        assert_rejected(&error, &[(field, reason)]);
    }

    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        order_before,
        "a refused Item assignment set preserves the prior canonical bytes"
    );
    assert_eq!(
        fs::read(tree.path().join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn every_offending_item_assignment_is_reported_at_once() {
    let (_tree, directory) = four_item_order("order-items-invalid-assignments");

    let error = plan_order_item_edit(
        &directory,
        1,
        &ItemAssignments::new()
            .assign("description", "Mirror mount")
            .assign("description", "Second description")
            .assign("order_id", "8b6e3f0a-8ec5-4d47-95e1-412652a56d16")
            .assign("items", "[]")
            .assign("item_total", "169.0")
            .assign("quantity", "   "),
        &RealFileSystem,
    )
    .expect_err("an invalid Item assignment set is refused");

    assert_rejected(
        &error,
        &[
            ("description", RejectedAssignmentReason::Duplicate),
            ("order_id", RejectedAssignmentReason::UnknownField),
            ("items", RejectedAssignmentReason::UnknownField),
            // A calculated total is not a fact an Item stores.
            ("item_total", RejectedAssignmentReason::UnknownField),
            ("quantity", RejectedAssignmentReason::BlankRequiredValue),
        ],
    );
}

#[test]
fn adding_without_every_required_item_fact_is_rejected() {
    let (_tree, directory) = four_item_order("order-items-missing-facts");

    let error = plan_order_item_add(
        &directory,
        &ItemAssignments::new().assign("description", "Fiber collimator"),
        None,
        &RealFileSystem,
    )
    .expect_err("an Item without a Quantity and Unit Price is refused");
    let OrderEditPlanError::MissingRequiredItemFacts { fields } = &error else {
        panic!("a partial Item names its missing required facts: {error:?}");
    };
    assert_eq!(fields, &["quantity", "unit_price"]);

    let error = plan_order_item_add(
        &directory,
        &ItemAssignments::new().assign("catalog_number", "TM-100"),
        None,
        &RealFileSystem,
    )
    .expect_err("an Item with only an optional fact is refused");
    let OrderEditPlanError::MissingRequiredItemFacts { fields } = &error else {
        panic!("a bare Item names all three missing required facts: {error:?}");
    };
    assert_eq!(fields, &["description", "quantity", "unit_price"]);

    // An edit resupplies nothing, so an existing Item keeps every required fact.
    let result = committed(
        plan_order_item_edit(
            &directory,
            2,
            &ItemAssignments::new().assign("catalog_number", "LA1050"),
            &RealFileSystem,
        )
        .expect("an edit needs no required fact resupplied"),
    );
    let item = &result.order().document().items[1];
    assert_eq!(item.description.as_str(), "Lens");
    assert_eq!(item.quantity.get(), 3.0);
    assert_eq!(item.unit_price.get(), 10.0);
    assert_eq!(item.catalog_number.as_deref(), Some("LA1050"));
}

fn created_vendor(root: &Path, name: &str) -> String {
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", name),
        &RealFileSystem,
    )
    .expect("the fixture Vendor can be planned");
    let report = commit_vendor_mutation(plan, &mut FixedUuid(VENDOR_ID.into()), &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    result.vendor().vendor_id.as_str().to_owned()
}

/// An Order carrying every neighbouring artifact an Item mutation must leave alone:
/// a live Vendor association, an exact user-authored README, and two opaque
/// Supporting Material sentinels, one of them nested.
fn surrounded_order(label: &str) -> (TemporaryTree, PathBuf, String) {
    let tree = initialized_package(label);
    let root = tree.path();
    let vendor = created_vendor(root, "Tarnwick");
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();

    let mut input = OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold)
        .vendor_name("Tarnwick")
        .orderer("Ada Lovelace")
        .ordering_institution("Northwind")
        .currency("USD")
        .project("Telescope")
        .order_group("Summer build");
    for item in four_items() {
        input = input.item(item);
    }
    let plan = plan_order_adoption(root, &directory, &input, &RealFileSystem)
        .expect("the surrounded fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the surrounded fixture Order commits: {report:?}"
    );
    (tree, directory, vendor)
}

#[test]
fn no_item_operation_changes_identity_lifecycle_facts_directory_readme_vendors_or_material() {
    let (tree, directory, vendor) = surrounded_order("order-items-preservation");
    let root = tree.path();
    let basename_before = directory.file_name().unwrap().to_owned();
    let readme_before = fs::read(directory.join("README.md")).unwrap();
    let vendors_before = fs::read(root.join(".spielorders/vendors.json")).unwrap();
    let version_before = fs::read(root.join(".spielorders/version.json")).unwrap();
    let quote_before = fs::read(directory.join("quote.bin")).unwrap();
    let drawing_before = fs::read(directory.join("Quotes/drawing.bin")).unwrap();

    // Each operation is planned against the state its own commit will find, so a
    // stale plan never stands in for a preserved fact.
    type Operation = (&'static str, fn(&Path) -> OrderItemMutationPlan);
    let operations: [Operation; 4] = [
        ("add", |directory| {
            plan_order_item_add(directory, &collimator(), Some(2), &RealFileSystem)
                .expect("the Item is added")
        }),
        ("edit", |directory| {
            plan_order_item_edit(
                directory,
                1,
                &ItemAssignments::new().assign("unit_price", "90.0"),
                &RealFileSystem,
            )
            .expect("the first Item is edited")
        }),
        ("move", |directory| {
            plan_order_item_move(directory, 4, 1, &RealFileSystem).expect("the Item is moved")
        }),
        ("remove", |directory| {
            plan_order_item_remove(directory, 3, &RealFileSystem).expect("the Item is removed")
        }),
    ];

    for (label, plan) in operations {
        let plan = plan(&directory);
        // No Item operation assigns a scalar, so every Reusable Free-Entry Value is
        // retained with the spelling this Order already stores.
        let values = plan.reusable_values();
        assert_eq!(
            [
                values.orderer(),
                values.ordering_institution(),
                values.project(),
                values.order_group(),
            ],
            [
                &ReusableValue::Reused("Ada Lovelace".into()),
                &ReusableValue::Reused("Northwind".into()),
                &ReusableValue::Reused("Telescope".into()),
                &ReusableValue::Reused("Summer build".into()),
            ],
            "{label}"
        );
        let result = committed(plan);
        let document = result.order().document();
        assert_eq!(
            document.order_id.as_str(),
            ORDER_ID,
            "{label} keeps the Order ID"
        );
        assert_eq!(document.title.as_str(), "Laser components", "{label}");
        assert_eq!(document.date_ordered.as_str(), "2026-08-22", "{label}");
        assert_eq!(document.status, OrderStatus::Hold, "{label}");
        assert_eq!(
            document.vendor_id.as_ref().map(|id| id.as_str()),
            Some(vendor.as_str()),
            "{label} keeps the Vendor association"
        );
        assert_eq!(document.orderer.as_deref(), Some("Ada Lovelace"), "{label}");
        assert_eq!(
            document.ordering_institution.as_deref(),
            Some("Northwind"),
            "{label} keeps the reusable value's stored canonical spelling"
        );
        assert_eq!(
            document.project.as_deref(),
            Some("Telescope"),
            "{label}"
        );
        assert_eq!(
            document.order_group.as_deref(),
            Some("Summer build"),
            "{label}"
        );
        assert_eq!(
            document.currency.as_ref().map(|c| c.as_str()),
            Some("USD"),
            "{label}"
        );

        assert!(directory.is_dir(), "{label} keeps the Order Directory");
        assert_eq!(directory.file_name().unwrap(), basename_before, "{label}");
        assert_eq!(
            fs::read(directory.join("README.md")).unwrap(),
            readme_before,
            "{label}"
        );
        assert_eq!(
            fs::read(root.join(".spielorders/vendors.json")).unwrap(),
            vendors_before,
            "{label} rewrites no Vendor aggregate"
        );
        assert_eq!(
            fs::read(root.join(".spielorders/version.json")).unwrap(),
            version_before,
            "{label}"
        );
        assert_eq!(
            fs::read(directory.join("quote.bin")).unwrap(),
            quote_before,
            "{label} leaves Supporting Material byte-identical"
        );
        assert_eq!(
            fs::read(directory.join("Quotes/drawing.bin")).unwrap(),
            drawing_before,
            "{label} leaves nested Supporting Material byte-identical"
        );
        assert!(directory.join("Quotes").is_dir(), "{label}");
    }

    // The four committed operations composed into one final collection.
    assert_eq!(
        descriptions(&persisted(&directory).items),
        ["Optical post", "Mirror mount", "Lens", "Breadboard"]
    );
    assert_eq!(SUPPORTING_A, quote_before.as_slice());
    assert_eq!(SUPPORTING_B, drawing_before.as_slice());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemBoundary {
    ObserveRecord,
    ReadRecord,
    WriteStage,
    SyncStage,
    Replace,
    IndexReplacement,
    RefreshListing,
}

const ORDER_RECORD: &str = ".spielorders/order.json";
const ORDER_STAGE: &str = ".spielorders/.order.json.spielorders-stage";

struct FailingItems {
    boundary: ItemBoundary,
    fired: Cell<bool>,
    fail_cleanup: bool,
    replaced: Cell<bool>,
}

impl FailingItems {
    fn at(boundary: ItemBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: false,
            replaced: Cell::new(false),
        }
    }

    fn with_cleanup_failure(boundary: ItemBoundary) -> Self {
        Self {
            fail_cleanup: true,
            ..Self::at(boundary)
        }
    }

    fn fail_once(&self, boundary: ItemBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: ItemBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingItems {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with(ORDER_RECORD) && self.fail_once(ItemBoundary::ReadRecord) {
            return Err(Self::error(ItemBoundary::ReadRecord));
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_once(ItemBoundary::WriteStage) {
            return Err(Self::error(ItemBoundary::WriteStage));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_once(ItemBoundary::SyncStage) {
            return Err(Self::error(ItemBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/order.json") {
            if self.fail_once(ItemBoundary::Replace) {
                return Err(Self::error(ItemBoundary::Replace));
            }
            self.replaced.set(true);
        }
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(ItemBoundary::IndexReplacement)
        {
            return Err(Self::error(ItemBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_cleanup {
            return Err(io::Error::other("injected staging cleanup failure"));
        }
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        if path.ends_with(ORDER_RECORD) && self.fail_once(ItemBoundary::ObserveRecord) {
            return Err(Self::error(ItemBoundary::ObserveRecord));
        }
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        // Only the post-commit refresh load is failed, never the precommit checks.
        if self.replaced.get() && self.boundary == ItemBoundary::RefreshListing {
            return Err(Self::error(ItemBoundary::RefreshListing));
        }
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn injected_precommit_failures_preserve_the_prior_item_collection_exactly() {
    for boundary in [
        ItemBoundary::WriteStage,
        ItemBoundary::SyncStage,
        ItemBoundary::Replace,
    ] {
        let (_tree, directory) = four_item_order("order-items-precommit-failure");
        let order_path = directory.join(".spielorders/order.json");
        let before = fs::read(&order_path).unwrap();
        let plan = plan_order_item_remove(&directory, 2, &RealFileSystem)
            .expect("the removal can be planned");

        let report = commit_order_item_mutation(plan, &FailingItems::at(boundary));

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let OrderEditRejection::ReplacementFailed { error } = &rejection else {
            panic!("an injected {boundary:?} failure is a replacement failure: {rejection:?}");
        };
        assert!(
            matches!(
                (boundary, error.failure()),
                (
                    ItemBoundary::WriteStage,
                    ValidatedReplacementFailure::StagingWrite(_)
                ) | (
                    ItemBoundary::SyncStage,
                    ValidatedReplacementFailure::Durability(_)
                ) | (
                    ItemBoundary::Replace,
                    ValidatedReplacementFailure::Replacement(_)
                )
            ),
            "an injected {boundary:?} failure reports its own phase: {:?}",
            error.failure()
        );
        assert!(error.cleanup_failure().is_none());
        assert_eq!(
            fs::read(&order_path).unwrap(),
            before,
            "the complete prior Item collection survives an injected {boundary:?} failure"
        );
        assert_eq!(
            descriptions(&persisted(&directory).items),
            ["Mirror mount", "Lens", "Optical post", "Breadboard"]
        );
        assert!(!directory.join(ORDER_STAGE).exists());
    }
}

#[test]
fn a_staging_cleanup_failure_is_retained_beside_the_item_rejection() {
    let (_tree, directory) = four_item_order("order-items-cleanup-failure");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    let plan = plan_order_item_add(&directory, &collimator(), None, &RealFileSystem)
        .expect("the append can be planned");

    let report = commit_order_item_mutation(
        plan,
        &FailingItems::with_cleanup_failure(ItemBoundary::SyncStage),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    let OrderEditRejection::ReplacementFailed { error } = &rejection else {
        panic!("a durability failure is a replacement failure: {rejection:?}");
    };
    assert!(error.cleanup_failure().is_some());
    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert!(directory.join(ORDER_STAGE).exists());
    fs::remove_file(directory.join(ORDER_STAGE)).unwrap();
}

#[test]
fn a_post_commit_index_failure_keeps_the_committed_item_change() {
    for (boundary, refresh) in [
        (
            ItemBoundary::IndexReplacement,
            PackageIndexRefresh::ReplacementFailed,
        ),
        (
            ItemBoundary::RefreshListing,
            PackageIndexRefresh::NotAttempted,
        ),
    ] {
        let (_tree, directory) = four_item_order("order-items-index-failure");
        let plan = plan_order_item_add(&directory, &collimator(), Some(1), &RealFileSystem)
            .expect("the insert can be planned");

        let report = commit_order_item_mutation(plan, &FailingItems::at(boundary));

        let MutationReport::CommittedWithWarning {
            result, warnings, ..
        } = report
        else {
            panic!("a failed {boundary:?} refresh remains committed: {report:?}");
        };
        assert_eq!(
            descriptions(&result.order().document().items)[0],
            "Fiber collimator"
        );
        assert_eq!(result.order().order_total(), 438.5);
        // Either way the derived index is left behind the canonical record, and the
        // evidence says which phase failed.
        assert_eq!(result.index().state(), PackageIndexState::Stale);
        assert_eq!(
            result.index().stale_reasons(),
            [PackageIndexStaleReason::OrderMetadataChanged]
        );
        assert_eq!(result.index().refresh(), refresh);
        let [OrderEditWarning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
            panic!("the refresh failure is retained: {warnings:?}");
        };
        if boundary == ItemBoundary::IndexReplacement {
            assert_eq!(
                failure.code(),
                OperationFailureCode::PackageIndexReplacementFailed
            );
        }

        // The canonical mutation stands, so retrying it is not what a caller should
        // do: the committed collection already carries the new Item.
        assert_eq!(
            descriptions(&persisted(&directory).items),
            [
                "Fiber collimator",
                "Mirror mount",
                "Lens",
                "Optical post",
                "Breadboard"
            ]
        );
    }
}

#[test]
fn every_item_operation_refuses_a_damaged_order_without_touching_its_record() {
    // A missing, unparseable, or semantically invalid record is one Damaged Order.
    // An Item operation never repairs, adopts, or partially salvages it.
    enum Damage {
        Replaced(&'static [u8]),
        Absent,
        /// Parseable and well shaped, but not a real calendar date.
        InvalidDateOrdered,
    }

    for (label, damage) in [
        ("malformed syntax", Damage::Replaced(b"{ not json")),
        (
            "unknown field",
            Damage::Replaced(br#"{"schema_version":1,"nope":true}"#),
        ),
        ("absent record", Damage::Absent),
        ("invalid calendar date", Damage::InvalidDateOrdered),
    ] {
        let (_tree, directory) = four_item_order("order-items-damaged");
        let order_path = directory.join(".spielorders/order.json");
        let corrupt = match damage {
            Damage::Replaced(bytes) => Some(bytes.to_vec()),
            Damage::Absent => None,
            Damage::InvalidDateOrdered => Some(
                fs::read_to_string(&order_path)
                    .unwrap()
                    .replace("2026-08-22", "2026-02-30")
                    .into_bytes(),
            ),
        };
        match &corrupt {
            Some(bytes) => fs::write(&order_path, bytes).unwrap(),
            None => fs::remove_file(&order_path).unwrap(),
        }

        let refusals: Vec<OrderEditPlanError<io::Error>> = vec![
            list_order_items(&directory, &RealFileSystem)
                .err()
                .unwrap_or_else(|| panic!("{label} cannot be listed")),
            plan_order_item_add(&directory, &collimator(), None, &RealFileSystem)
                .err()
                .unwrap_or_else(|| panic!("{label} cannot be added to")),
            plan_order_item_edit(
                &directory,
                1,
                &ItemAssignments::new().assign("quantity", "9"),
                &RealFileSystem,
            )
            .err()
            .unwrap_or_else(|| panic!("{label} cannot be edited")),
            plan_order_item_move(&directory, 1, 2, &RealFileSystem)
                .err()
                .unwrap_or_else(|| panic!("{label} cannot be reordered")),
            plan_order_item_remove(&directory, 1, &RealFileSystem)
                .err()
                .unwrap_or_else(|| panic!("{label} cannot be removed from")),
        ];

        for error in &refusals {
            assert!(
                matches!(error, OrderEditPlanError::DamagedOrder { .. }),
                "{label} is one Damaged Order: {error:?}"
            );
        }
        match &corrupt {
            Some(bytes) => assert_eq!(&fs::read(&order_path).unwrap(), bytes),
            None => assert!(!order_path.exists()),
        }
    }
}

#[test]
fn a_concurrent_item_collection_change_rejects_before_any_write() {
    let (_tree, directory) = four_item_order("order-items-concurrent");
    let order_path = directory.join(".spielorders/order.json");
    let plan =
        plan_order_item_remove(&directory, 4, &RealFileSystem).expect("the removal can be planned");

    // Another writer reorders the same collection between plan and commit.
    let intervening = committed(
        plan_order_item_move(&directory, 1, 2, &RealFileSystem).expect("the move is planned"),
    );
    assert_eq!(
        descriptions(&intervening.order().document().items),
        ["Lens", "Mirror mount", "Optical post", "Breadboard"]
    );
    let before = fs::read(&order_path).unwrap();

    let report = commit_order_item_mutation(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a concurrent collection change rejects: {report:?}");
    };
    let OrderEditRejection::PreconditionsChanged { location } = &rejection else {
        panic!("a concurrent collection change is a precondition change: {rejection:?}");
    };
    assert!(
        location
            .absolute_path()
            .ends_with(".spielorders/order.json")
    );
    assert!(
        location
            .package_relative_path()
            .is_some_and(|relative| relative.ends_with(".spielorders/order.json")),
        "a refusal names the record in the Package's own spelling: {location:?}"
    );
    assert_eq!(
        fs::read(&order_path).unwrap(),
        before,
        "the intervening reordering is not silently overwritten"
    );
}

/// **The two ordered collections share one vocabulary**, which is what the roles and the
/// outcomes are asserted against here: a Vendor's Contacts and an Order's Items are
/// addressed the same way, mutated by the same four verbs, and refuse a position the same
/// way, so a caller reading which end was refused or what happened to the member reads one
/// set of spellings rather than one per collection.
#[test]
fn order_item_code_vocabularies_keep_their_stable_spellings() {
    for (kind, spelling) in [
        (OrderItemMutationKind::Add, "add"),
        (OrderItemMutationKind::Edit, "edit"),
        (OrderItemMutationKind::Move, "move"),
        (OrderItemMutationKind::Remove, "remove"),
    ] {
        assert_eq!(kind.as_str(), spelling);
    }
    for (role, contact, item) in [
        (
            "the source end",
            ContactPositionRole::Source,
            ItemPositionRole::Source,
        ),
        (
            "the destination end",
            ContactPositionRole::Destination,
            ItemPositionRole::Destination,
        ),
    ] {
        assert_eq!(item.as_str(), contact.as_str(), "{role}");
    }
    for (item, contact) in [
        (CanonicalItemOutcome::Added, CanonicalContactOutcome::Added),
        (
            CanonicalItemOutcome::Edited,
            CanonicalContactOutcome::Edited,
        ),
        (CanonicalItemOutcome::Moved, CanonicalContactOutcome::Moved),
        (
            CanonicalItemOutcome::Removed,
            CanonicalContactOutcome::Removed,
        ),
    ] {
        assert_eq!(item.as_str(), contact.as_str());
    }
    assert_eq!(
        RejectedAssignmentReason::NotAFiniteNumber.as_str(),
        "not_a_finite_number"
    );
}

#[test]
fn planning_every_item_operation_writes_nothing() {
    let (tree, directory) = four_item_order("order-items-dry-run");
    let order_before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let index_before = fs::read(tree.path().join(".spielorders/index.json")).unwrap();

    // Every planner validates the same complete plan a commit would and reports the
    // same intended managed operations, while performing none of them.
    let plans = [
        plan_order_item_add(&directory, &collimator(), Some(3), &RealFileSystem).unwrap(),
        plan_order_item_edit(
            &directory,
            2,
            &ItemAssignments::new().assign("quantity", "11"),
            &RealFileSystem,
        )
        .unwrap(),
        plan_order_item_move(&directory, 2, 4, &RealFileSystem).unwrap(),
        plan_order_item_remove(&directory, 1, &RealFileSystem).unwrap(),
    ];

    for plan in &plans {
        assert!(plan.changes_canonical_order());
        assert_eq!(
            plan.operations()
                .iter()
                .map(|operation| operation.kind())
                .collect::<Vec<_>>(),
            [
                OrderEditOperationKind::ReplaceOrderRecord,
                OrderEditOperationKind::RefreshPackageIndex,
            ],
            "{:?} announces one record replacement and one index refresh",
            plan.kind()
        );
    }
    assert_eq!(
        [
            plans[0].facts().order_total(),
            plans[1].facts().order_total(),
            plans[2].facts().order_total(),
            plans[3].facts().order_total(),
        ],
        [438.5, 506.5, 426.5, 257.5],
        "each plan reports the totals its own commit would produce"
    );
    assert!(!directory.join(ORDER_STAGE).exists());
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        order_before
    );
    assert_eq!(
        fs::read(tree.path().join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

/// Every Item entry point resolved through the shared Order seam, each with its own
/// injecting filesystem so one boundary fires once per entry point.
fn item_entry_point_refusals(
    directory: &Path,
    boundary: ItemBoundary,
) -> Vec<(&'static str, OrderEditPlanError<io::Error>)> {
    fn refusal<T>(
        label: &'static str,
        result: Result<T, OrderEditPlanError<io::Error>>,
    ) -> (&'static str, OrderEditPlanError<io::Error>) {
        match result {
            Err(error) => (label, error),
            Ok(_) => panic!("{label} must refuse an Order it cannot resolve"),
        }
    }
    vec![
        refusal(
            "list",
            list_order_items(directory, &FailingItems::at(boundary)),
        ),
        refusal(
            "add",
            plan_order_item_add(directory, &collimator(), None, &FailingItems::at(boundary)),
        ),
        refusal(
            "edit",
            plan_order_item_edit(
                directory,
                1,
                &ItemAssignments::new().assign("quantity", "9"),
                &FailingItems::at(boundary),
            ),
        ),
        refusal(
            "move",
            plan_order_item_move(directory, 1, 2, &FailingItems::at(boundary)),
        ),
        refusal(
            "remove",
            plan_order_item_remove(directory, 1, &FailingItems::at(boundary)),
        ),
    ]
}

#[test]
fn every_item_entry_point_separates_an_unreadable_record_from_an_unobservable_one() {
    let (_tree, directory) = four_item_order("order-items-unreadable-record");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();

    // A record that is present and well formed but cannot be read damages only its
    // own Order, exactly as a missing, malformed, or wrongly typed one does.
    for (label, error) in item_entry_point_refusals(&directory, ItemBoundary::ReadRecord) {
        assert!(
            matches!(error, OrderEditPlanError::DamagedOrder { .. }),
            "{label} reports an unreadable record as one Damaged Order: {error:?}"
        );
    }

    // Failing to observe the path at all says nothing about the record's content, so
    // it stays an operation failure rather than quarantining a healthy Order.
    for (label, error) in item_entry_point_refusals(&directory, ItemBoundary::ObserveRecord) {
        assert!(
            matches!(error, OrderEditPlanError::FileSystem { .. }),
            "{label} reports an unobservable record path as an operation failure: {error:?}"
        );
    }

    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert_eq!(
        descriptions(&persisted(&directory).items),
        ["Mirror mount", "Lens", "Optical post", "Breadboard"]
    );
}

#[test]
fn every_committed_item_report_without_a_refresh_carries_the_orders_real_readme() {
    type Operation = (&'static str, fn(&Path) -> OrderItemMutationPlan);
    let operations: [Operation; 4] = [
        ("add", |directory| {
            plan_order_item_add(directory, &collimator(), Some(2), &RealFileSystem)
                .expect("the Item is added")
        }),
        ("edit", |directory| {
            plan_order_item_edit(
                directory,
                1,
                &ItemAssignments::new().assign("unit_price", "90.0"),
                &RealFileSystem,
            )
            .expect("the first Item is edited")
        }),
        ("move", |directory| {
            plan_order_item_move(directory, 4, 1, &RealFileSystem).expect("the Item is moved")
        }),
        ("remove", |directory| {
            plan_order_item_remove(directory, 3, &RealFileSystem).expect("the Item is removed")
        }),
    ];

    for (label, plan) in operations {
        // A fresh Order per operation, each carrying a nonempty user-authored README.
        let (_tree, directory, _vendor) = surrounded_order("order-items-refresh-readme");

        let report = commit_order_item_mutation(
            plan(&directory),
            &FailingItems::at(ItemBoundary::RefreshListing),
        );

        let MutationReport::CommittedWithWarning { result, .. } = report else {
            panic!("{label} remains committed when the refresh load fails: {report:?}");
        };
        // The projection is built locally here, so it owes the caller the complete
        // Order including the README text rather than an empty document.
        assert_eq!(
            result.order().readme_text().map(str::as_bytes),
            Some(README),
            "{label} reports the Order's real README"
        );
        assert_eq!(result.index().state(), PackageIndexState::Stale);
        // Reporting the README is never writing it.
        assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
    }
}

/// **One Item-assignment vocabulary, whichever capability takes the set.**
///
/// Adoption is the only mutation that may carry Items at creation — an Order created from
/// nothing starts with none — and it adjudicates the set the four Item mutations
/// adjudicate: the same four fields, the same refusals, the same required facts. Two lists
/// is how a field comes to be assignable on one command and unknown on another, so the
/// adopted Item and the added one are compared with each other here rather than each being
/// checked against its own expectations.
#[test]
fn an_adopted_item_and_an_added_one_come_from_one_assignment_vocabulary() {
    let assignments = ItemAssignments::new()
        .assign("description", "Fiber collimator")
        .assign("quantity", "4")
        .assign("unit_price", "3.0")
        .assign("catalog_number", "F220APC-1550");

    let tree = initialized_package("order-item-one-vocabulary");
    let adopted = OrderItemInput::from_assignments(&assignments)
        .expect("one assignment set names one adopted Item");
    let directory = adopted_order(tree.path(), vec![adopted]);
    let added = committed(
        plan_order_item_add(&directory, &assignments, None, &RealFileSystem)
            .expect("the same set names one added Item"),
    );

    let items = &added.order().document().items;
    assert_eq!(
        items.first(),
        items.last(),
        "the adopted Item and the added one are the same Item: {items:?}"
    );
    assert_eq!(
        added.order().item_totals(),
        [12.0, 12.0],
        "and each carries the same calculated total, which is stored nowhere"
    );
}

/// The same set of rules refuses a set that names no Item, whichever capability was handed
/// it.
///
/// Compared refusal by refusal rather than merely both failing: an unknown field, a
/// repeated one, a blank required fact, and a Quantity the record could not hold as a
/// finite number each have to reach the same reason on both routes, or the two capabilities
/// have two vocabularies that happen to agree on the accepted case.
#[test]
fn one_set_of_item_rules_refuses_an_adoption_and_an_addition_alike() {
    let (_tree, directory) = four_item_order("order-item-one-refusal");

    for (label, assignments, expected) in [
        (
            "an unknown field",
            collimator().assign("colour", "red"),
            vec![("colour", RejectedAssignmentReason::UnknownField)],
        ),
        (
            "a repeated field",
            collimator().assign("quantity", "5"),
            vec![("quantity", RejectedAssignmentReason::Duplicate)],
        ),
        (
            "a blank required fact",
            ItemAssignments::new()
                .assign("description", "  ")
                .assign("quantity", "1")
                .assign("unit_price", "2"),
            vec![("description", RejectedAssignmentReason::BlankRequiredValue)],
        ),
        (
            "a Quantity that is no finite number",
            ItemAssignments::new()
                .assign("description", "Lens")
                .assign("quantity", "quite a few")
                .assign("unit_price", "inf"),
            vec![
                ("quantity", RejectedAssignmentReason::NotAFiniteNumber),
                ("unit_price", RejectedAssignmentReason::NotAFiniteNumber),
            ],
        ),
    ] {
        let adoption = OrderItemInput::from_assignments(&assignments)
            .expect_err("the set names no adopted Item");
        let NewItemAssignmentFailure::InvalidAssignments(refused) = &adoption else {
            panic!("{label} is a refused assignment set for adoption: {adoption:?}");
        };
        assert_eq!(
            refused
                .iter()
                .map(|assignment| (assignment.field(), assignment.reason()))
                .collect::<Vec<_>>(),
            expected,
            "{label}: adoption names every offending field and its reason"
        );

        let addition = plan_order_item_add(&directory, &assignments, None, &RealFileSystem)
            .expect_err("the set names no added Item");
        let OrderEditPlanError::InvalidAssignments(refused) = &addition else {
            panic!("{label} is a refused assignment set for an addition: {addition:?}");
        };
        assert_eq!(
            refused
                .iter()
                .map(|assignment| (assignment.field(), assignment.reason()))
                .collect::<Vec<_>>(),
            expected,
            "{label}: an addition refuses the same set for the same reasons"
        );
    }

    // A set naming none of the required facts is the other refusal both routes have, and
    // both report every missing fact at once rather than one round trip at a time.
    let assignments = ItemAssignments::new().assign("catalog_number", "F220APC-1550");
    let adoption =
        OrderItemInput::from_assignments(&assignments).expect_err("no required fact was named");
    assert_eq!(
        adoption,
        NewItemAssignmentFailure::MissingRequiredFacts {
            fields: vec![
                "description".to_owned(),
                "quantity".to_owned(),
                "unit_price".to_owned()
            ]
        }
    );
    let addition = plan_order_item_add(&directory, &assignments, None, &RealFileSystem)
        .expect_err("no required fact was named");
    let OrderEditPlanError::MissingRequiredItemFacts { fields } = &addition else {
        panic!("an addition reports the missing Item facts: {addition:?}");
    };
    assert_eq!(fields, &["description", "quantity", "unit_price"]);
}
