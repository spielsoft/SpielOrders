//! Reading one Order from its Directory alone.
//!
//! The capability the CLI's `order` family consumes. Two of its claims are the reason it
//! exists at all and are checked here rather than through an adapter: that an Order's
//! diagnostics are *this* Order's and not its siblings', and that the route to the Package
//! is the record's own locator rather than a walk up the tree.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record and
//! lifecycle contract; Define durable metadata schemas and version evolution.

use crate::Failure;
use crate::readme::DiagnosticCode;
use crate::tests::support::{
    Operation, PlatformFileSystem, QueuedUuids, ScriptedFileSystem, TemporaryTree, adopted_order,
    adoption, effects, failed_path, initialization, initialized_package, json, order_at,
    order_record,
};
use crate::{Assignments, Report};
use std::path::PathBuf;

const FIRST_ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SECOND_ORDER_ID: &str = "3f2a1c05-7b19-4e63-9d84-5a0c6e8b2f71";

/// A Package at a configured Orders Directory several levels below its Root.
///
/// The default layout puts an Order Directory two levels below the Root, so a route that
/// counted a fixed two parents would be right by accident. Here it is four.
fn nested_package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = crate::plan(
        &initialization(tree.path(), &("archive/2026/Orders", "Removed")),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the nested test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = std::fs::canonicalize(tree.path()).expect("the nested Package root resolves");
    (tree, root)
}

/// The route to the Package is the record's own locator, so the depth of the Orders
/// Directory is the locator's business and nothing else's.
#[test]
fn an_order_reaches_its_package_through_its_own_locator_at_any_depth() {
    let (_tree, root) = nested_package("core-order-read-nested");
    // Adopted directly rather than through the shared helper, whose Order Directory is a
    // child of `Orders/` — the layout this test exists to be different from.
    let directory = root.join("archive/2026/Orders/Legacy_laser_purchase");
    std::fs::create_dir(&directory).expect("the nested Order Directory is created");
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
    .expect("the nested directory can be adopted");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[FIRST_ORDER_ID]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let read = order_at(&directory, &PlatformFileSystem)
        .result
        .expect("the nested Order reads");

    assert_eq!(read.context.root.absolute_path, root);
    assert_eq!(read.order.paths.directory.absolute_path, directory);
    assert_eq!(
        json(&read.order.document.package_root),
        "../../../..",
        "four parent segments, which is what the locator is for"
    );
}

/// **One Order's diagnostics are that Order's.**
///
/// The reason this projection exists: an Order's diagnostics live in the Package load's
/// groups, and a caller that picked them out itself would be deciding which of them belong
/// to an Order. Here the sibling is damaged and the subject merely has an unreadable
/// README, so a selection that took the whole load would report the sibling's error
/// against this Order — and a caller acting on it would repair the wrong record.
#[test]
fn one_orders_diagnostics_exclude_every_other_orders() {
    let (_tree, root) = initialized_package("core-order-read-sibling");
    let subject = adopted_order(&root, "Subject_order", FIRST_ORDER_ID);
    let sibling = adopted_order(&root, "Sibling_order", SECOND_ORDER_ID);

    // The subject's README exists and is not a readable document: a warning about this
    // Order, and the only diagnostic a healthy Order can carry.
    let readme = subject.join("README.md");
    std::fs::remove_file(&readme).expect("the plain README is removed");
    std::fs::create_dir(&readme).expect("a directory takes its place");
    // The sibling's record is not readable at all: an error about a different Order.
    std::fs::write(order_record(&sibling), b"{ not json").expect("the sibling is damaged");

    let read = order_at(&subject, &PlatformFileSystem)
        .result
        .expect("the subject Order still reads");

    let codes: Vec<DiagnosticCode> = read
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        [DiagnosticCode::ReadmeUnavailable],
        "only this Order's own observation: {:?}",
        read.diagnostics
    );
    assert_eq!(read.diagnostics[0].primary_location.absolute_path, readme);
}

/// A filesystem that will not say what stands at the record's path leaves the question
/// unanswered, and the read says so rather than guessing either way.
#[test]
fn an_unobservable_record_is_an_operational_failure_rather_than_a_verdict() {
    let (_tree, root) = initialized_package("core-order-read-unobservable");
    let directory = adopted_order(&root, "Legacy_laser_purchase", FIRST_ORDER_ID);
    let record = order_record(&directory);

    let failure = order_at(
        &directory,
        &ScriptedFileSystem::new().failing(Operation::SymlinkMetadata, &record),
    )
    .result
    .expect_err("the record's kind could not be observed");

    let Failure::PackageOperationFailed(operation) = failure else {
        panic!("an unobservable path is an operational failure: {failure:?}");
    };
    assert_eq!(failed_path(&operation.path), record);
}
