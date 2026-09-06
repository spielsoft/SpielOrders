//! Reading one Order from its Directory alone, and from an Order ID.
//!
//! The capability the CLI's `order` family consumes. Two of its claims are the reason it
//! exists at all and are checked here rather than through an adapter: that an Order's
//! diagnostics are *this* Order's and not its siblings', and that the route to the Package
//! is the record's own locator rather than a walk up the tree.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record and
//! lifecycle contract; Define durable metadata schemas and version evolution.

use crate::support::{
    QueuedUuids, RealFileSystem, TemporaryTree, adopted_order, initialized_package, order_record,
};
use spielorders_core::{
    adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption},
    initialization::{
        InitializationOptions, commit_package_initialization, plan_package_initialization,
    },
    metadata::OrderStatus,
    mutation::MutationReport,
    operations::{FileSystemOperation, OperationEvidence},
    order_reads::{OrderReadFailure, read_order, read_order_by_id},
    ports::{DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind},
    readme::DiagnosticCode,
};
use std::{
    io,
    path::{Path, PathBuf},
};

const FIRST_ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SECOND_ORDER_ID: &str = "3f2a1c05-7b19-4e63-9d84-5a0c6e8b2f71";

/// A Package at a configured Orders Directory several levels below its Root.
///
/// The default layout puts an Order Directory two levels below the Root, so a route that
/// counted a fixed two parents would be right by accident. Here it is four.
fn nested_package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::new("archive/2026/Orders", "Removed"),
        &RealFileSystem,
    )
    .expect("the nested test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = std::fs::canonicalize(tree.path()).expect("the nested Package root resolves");
    (tree, root)
}

/// A filesystem that refuses to observe one path's kind and behaves normally otherwise.
///
/// The one operational failure this read can reach before it has anything to name: the
/// record's own kind cannot be observed, so whether the directory is an Order Directory
/// at all is unknown, and guessing either way would be inventing an answer.
struct RefusingToObserve {
    path: PathBuf,
}

impl FileSystem for RefusingToObserve {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        if path == self.path {
            return Err(io::Error::other("the kind of this path cannot be observed"));
        }
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
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
    let plan = plan_order_adoption(
        &root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the nested directory can be adopted");
    assert!(matches!(
        commit_order_adoption(
            plan,
            &mut QueuedUuids::of(&[FIRST_ORDER_ID]),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    let read = read_order(&directory, &RealFileSystem).expect("the nested Order reads");

    assert_eq!(read.context().root().absolute_path(), root);
    assert_eq!(read.order().paths().directory().absolute_path(), directory);
    assert_eq!(
        read.order().package_root().as_str(),
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

    let read = read_order(&subject, &RealFileSystem).expect("the subject Order still reads");

    let codes: Vec<DiagnosticCode> = read
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect();
    assert_eq!(
        codes,
        [DiagnosticCode::ReadmeUnavailable],
        "only this Order's own observation: {:?}",
        read.diagnostics()
    );
    assert_eq!(
        read.diagnostics()[0].primary_location().absolute_path(),
        readme
    );
}

/// A Damaged Order whose identity the load could still trust answers as a Damaged Order
/// rather than as an absent one, whichever route asked for it.
///
/// Reporting it missing would send a caller looking for an Order that is there, and whose
/// record is the problem.
#[test]
fn a_damaged_order_is_found_by_its_identity_rather_than_reported_absent() {
    let (_tree, root) = initialized_package("core-order-read-damaged-by-id");
    let directory = adopted_order(&root, "Damaged_order", FIRST_ORDER_ID);
    // A well-formed record whose Date Ordered is a day that does not exist: it
    // deserializes, so its identity is trustworthy, and it does not validate.
    let record = order_record(&directory);
    let text = std::fs::read_to_string(&record).expect("the record can be read");
    let rewritten = text.replace("\"2026-08-22\"", "\"2026-02-30\"");
    assert_ne!(text, rewritten, "the Date Ordered was found");
    std::fs::write(&record, rewritten).expect("the rewritten record is written");

    let by_identity = read_order_by_id(&root, FIRST_ORDER_ID, &RealFileSystem)
        .expect_err("a Damaged Order has no facts to read");
    let by_directory =
        read_order(&directory, &RealFileSystem).expect_err("and neither route invents any");

    for (route, failure) in [("--uuid", by_identity), ("the Directory", by_directory)] {
        let OrderReadFailure::DamagedOrder {
            order_id,
            diagnostics,
            ..
        } = failure
        else {
            panic!("{route} reports the Order as damaged: {failure:?}");
        };
        assert_eq!(
            order_id.as_ref().map(|order_id| order_id.as_str()),
            Some(FIRST_ORDER_ID),
            "{route}: the identity the load could trust"
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code())
                .collect::<Vec<_>>(),
            [DiagnosticCode::OrderSemanticInvalid],
            "{route}: the diagnosis the load quarantined at this Order's own scope"
        );
    }
}

/// An Order ID that names no Order of the Package is that, and not a damaged anything.
#[test]
fn an_unknown_order_id_reports_the_package_it_was_resolved_against() {
    let (_tree, root) = initialized_package("core-order-read-unknown-id");
    adopted_order(&root, "Present_order", FIRST_ORDER_ID);

    let failure = read_order_by_id(&root, SECOND_ORDER_ID, &RealFileSystem)
        .expect_err("no Order carries that identity");

    let OrderReadFailure::OrderIdNotFound { context, order_id } = failure else {
        panic!("an unknown identity is reported as such: {failure:?}");
    };
    assert_eq!(order_id, SECOND_ORDER_ID);
    assert_eq!(context.root().absolute_path(), root);
}

/// A filesystem that will not say what stands at the record's path leaves the question
/// unanswered, and the read says so rather than guessing either way.
#[test]
fn an_unobservable_record_is_an_operational_failure_rather_than_a_verdict() {
    let (_tree, root) = initialized_package("core-order-read-unobservable");
    let directory = adopted_order(&root, "Legacy_laser_purchase", FIRST_ORDER_ID);
    let record = order_record(&directory);

    let failure = read_order(
        &directory,
        &RefusingToObserve {
            path: record.clone(),
        },
    )
    .expect_err("the record's kind could not be observed");

    let OrderReadFailure::Operation(operation) = failure else {
        panic!("an unobservable path is an operational failure: {failure:?}");
    };
    assert_eq!(operation.primary_path().as_path(), record);
    assert_eq!(
        operation.evidence(),
        &OperationEvidence::FileSystem {
            operation: FileSystemOperation::ObserveKind
        }
    );
}
