//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::filesystem::{
    FileReplacement, MultiFileReplacementFailure, ReplacementPlanFailure,
    ValidatedReplacementFailure, replace_validated_files,
};
use spielorders_core::metadata::{OrderDocument, VendorsDocument};
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    path::{Path, PathBuf},
};

const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const MINIMAL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedError {
    Missing,
    AlreadyExists,
    ReplacementCall(usize),
}

#[derive(Default)]
struct TestFileSystem {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    failed_replacement_calls: BTreeSet<usize>,
    replacement_order: RefCell<Vec<PathBuf>>,
    writes: RefCell<Vec<PathBuf>>,
    /// A writer outside this operation, editing a destination partway through it.
    /// Keyed by the staging write it lands before, which puts the edit strictly
    /// between the operation's prior read and its write of that destination.
    edits_before_staging: RefCell<BTreeMap<usize, (PathBuf, Vec<u8>)>>,
    /// The same, keyed by the `replace_file` call it lands before, for reaching the
    /// window between a completed operation and the rollback that would undo it.
    edits_before_replace: RefCell<BTreeMap<usize, (PathBuf, Vec<u8>)>>,
    staging_writes: RefCell<usize>,
}

impl TestFileSystem {
    fn new(files: impl IntoIterator<Item = (PathBuf, Vec<u8>)>, failed_calls: &[usize]) -> Self {
        Self {
            files: RefCell::new(files.into_iter().collect()),
            failed_replacement_calls: failed_calls.iter().copied().collect(),
            replacement_order: RefCell::new(Vec::new()),
            writes: RefCell::new(Vec::new()),
            edits_before_staging: RefCell::new(BTreeMap::new()),
            edits_before_replace: RefCell::new(BTreeMap::new()),
            staging_writes: RefCell::new(0),
        }
    }

    /// Schedules an outside writer's edit immediately before the given staging write.
    fn editing_before_staging(self, before_call: usize, path: &Path, bytes: &[u8]) -> Self {
        self.edits_before_staging
            .borrow_mut()
            .insert(before_call, (path.to_path_buf(), bytes.to_vec()));
        self
    }

    /// Schedules an outside writer's edit immediately before the given `replace_file`.
    fn editing_before_replace(self, before_call: usize, path: &Path, bytes: &[u8]) -> Self {
        self.edits_before_replace
            .borrow_mut()
            .insert(before_call, (path.to_path_buf(), bytes.to_vec()));
        self
    }

    fn bytes(&self, path: &Path) -> Vec<u8> {
        self.files.borrow().get(path).cloned().unwrap()
    }
}

impl FileSystem for TestFileSystem {
    type Error = InjectedError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or(InjectedError::Missing)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.writes.borrow_mut().push(path.to_path_buf());
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        let call = {
            let mut count = self.staging_writes.borrow_mut();
            let call = *count;
            *count += 1;
            call
        };
        // The outside writer lands here: after this operation read prior contents,
        // and before it writes this destination. Nothing about it goes through this
        // operation's plan.
        if let Some((edited, bytes)) = self.edits_before_staging.borrow_mut().remove(&call) {
            self.files.borrow_mut().insert(edited, bytes);
        }
        self.writes.borrow_mut().push(path.to_path_buf());
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(InjectedError::AlreadyExists);
        }
        files.insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        let call = self.replacement_order.borrow().len();
        // The outside writer lands here: after this operation read prior contents,
        // and before it writes. Nothing about it goes through this operation's plan.
        if let Some((edited, bytes)) = self.edits_before_replace.borrow_mut().remove(&call) {
            self.files.borrow_mut().insert(edited, bytes);
        }
        self.writes.borrow_mut().push(destination.to_path_buf());
        self.replacement_order
            .borrow_mut()
            .push(destination.to_path_buf());
        if self.failed_replacement_calls.contains(&call) {
            return Err(InjectedError::ReplacementCall(call));
        }
        let bytes = self
            .files
            .borrow_mut()
            .remove(source)
            .ok_or(InjectedError::Missing)?;
        self.files
            .borrow_mut()
            .insert(destination.to_path_buf(), bytes);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.writes.borrow_mut().push(path.to_path_buf());
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or(InjectedError::Missing)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        Ok(path.to_path_buf())
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        Ok(self
            .files
            .borrow()
            .contains_key(path)
            .then_some(FileSystemEntryKind::File))
    }

    fn read_directory(&self, _path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        Ok(Vec::new())
    }
}

#[test]
fn invalid_complete_plan_reports_every_candidate_and_performs_no_writes() {
    let first = Path::new("/package/.spielorders/vendors.json");
    let second = Path::new("/package/.spielorders/version.json");
    let plan = [
        FileReplacement::new(first, b"invalid vendors"),
        FileReplacement::new(second, b"invalid version"),
    ];
    let filesystem = TestFileSystem::default();
    let validated = RefCell::new(Vec::new());

    let error = replace_validated_files(&plan, &filesystem, |destination, _bytes| {
        validated.borrow_mut().push(destination.to_path_buf());
        Err("invalid candidate")
    })
    .expect_err("the invalid complete plan must be rejected");

    assert_eq!(validated.into_inner(), vec![first, second]);
    assert_eq!(
        error.failure(),
        &MultiFileReplacementFailure::InvalidPlan(vec![
            ReplacementPlanFailure::CandidateValidation {
                operation_index: 0,
                error: "invalid candidate",
            },
            ReplacementPlanFailure::CandidateValidation {
                operation_index: 1,
                error: "invalid candidate",
            },
        ])
    );
    assert!(error.rollback_failures().is_empty());
    assert!(filesystem.writes.borrow().is_empty());
}

#[test]
fn structurally_ambiguous_plan_is_rejected_before_any_filesystem_effect() {
    let duplicate = Path::new("/package/.spielorders/vendors.json");
    let plan = [
        FileReplacement::new(Path::new("/"), FULL_VENDORS),
        FileReplacement::new(duplicate, MINIMAL_VENDORS),
        FileReplacement::new(duplicate, FULL_VENDORS),
    ];
    let filesystem = TestFileSystem::default();

    let error = replace_validated_files(&plan, &filesystem, |_destination, _bytes| {
        Ok::<_, Infallible>(())
    })
    .expect_err("invalid and duplicate destinations must reject the complete plan");

    assert_eq!(
        error.failure(),
        &MultiFileReplacementFailure::InvalidPlan(vec![
            ReplacementPlanFailure::InvalidDestination { operation_index: 0 },
            ReplacementPlanFailure::DuplicateDestination {
                first_operation_index: 1,
                operation_index: 2,
            },
        ])
    );
    assert!(filesystem.files.borrow().is_empty());
    assert!(filesystem.writes.borrow().is_empty());
}

#[test]
fn aliased_destinations_report_both_operations_before_any_write() {
    let temporary = TemporaryTree::new("aliased-multi-file-destination");
    let metadata = temporary.path().join(".spielorders");
    let alias_parent = metadata.join("alias-parent");
    let destination = metadata.join("vendors.json");
    let aliased_destination = alias_parent.join("../vendors.json");
    let unrelated = temporary.path().join("unrelated.txt");
    std::fs::create_dir_all(&alias_parent).expect("alias parent is created");
    std::fs::write(&destination, MINIMAL_VENDORS).expect("old Vendors are written");
    std::fs::write(&unrelated, b"unrelated bytes").expect("unrelated file is written");
    let plan = [
        FileReplacement::new(&destination, FULL_VENDORS),
        FileReplacement::new(&aliased_destination, FULL_VENDORS),
    ];

    let error = replace_validated_files(&plan, &RealFileSystem, validate_canonical_file)
        .expect_err("two spellings of one destination must reject the complete plan");

    assert!(matches!(
        error.failure(),
        MultiFileReplacementFailure::InvalidPlan(failures)
            if matches!(
                failures.as_slice(),
                [ReplacementPlanFailure::DuplicateDestination {
                    first_operation_index: 0,
                    operation_index: 1,
                }]
            )
    ));
    assert!(error.rollback_failures().is_empty());
    assert_eq!(std::fs::read(&destination).unwrap(), MINIMAL_VENDORS);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated bytes");
    assert!(!metadata.join(".vendors.json.spielorders-stage").exists());
}

#[test]
fn two_valid_canonical_files_commit_without_touching_unrelated_content() {
    let temporary = TemporaryTree::new("multi-file-replacement");
    let root_metadata = temporary.path().join(".spielorders");
    let order_directory = temporary.path().join("Orders/Order");
    let order_metadata = order_directory.join(".spielorders");
    let vendors = root_metadata.join("vendors.json");
    let order = order_metadata.join("order.json");
    let unrelated = temporary.path().join("unrelated.txt");
    let supporting_a = order_directory.join("quote.bin");
    let supporting_b = order_directory.join("nested/drawing.bin");
    std::fs::create_dir_all(&root_metadata).expect("root metadata directory is created");
    std::fs::create_dir_all(&order_metadata).expect("Order metadata directory is created");
    std::fs::create_dir_all(supporting_b.parent().unwrap())
        .expect("Supporting Material directory is created");
    std::fs::write(&vendors, MINIMAL_VENDORS).expect("old Vendors are written");
    std::fs::write(&order, MINIMAL_ORDER).expect("old Order is written");
    std::fs::write(&unrelated, b"unrelated bytes").expect("unrelated file is written");
    std::fs::write(&supporting_a, SUPPORTING_A).expect("first sentinel is written");
    std::fs::write(&supporting_b, SUPPORTING_B).expect("second sentinel is written");
    let plan = [
        FileReplacement::new(&vendors, FULL_VENDORS),
        FileReplacement::new(&order, FULL_ORDER),
    ];

    replace_validated_files(&plan, &RealFileSystem, validate_canonical_file)
        .expect("both valid canonical files commit");

    assert_eq!(std::fs::read(&vendors).unwrap(), FULL_VENDORS);
    assert_eq!(std::fs::read(&order).unwrap(), FULL_ORDER);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated bytes");
    assert_eq!(std::fs::read(&supporting_a).unwrap(), SUPPORTING_A);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), SUPPORTING_B);
}

#[test]
fn second_replacement_failure_restores_the_first_in_reverse_operation_order() {
    let first = PathBuf::from("/package/Orders/Order/.spielorders/order.json");
    let second = PathBuf::from("/package/.spielorders/vendors.json");
    let unrelated = PathBuf::from("/package/unrelated.txt");
    let supporting = PathBuf::from("/package/Orders/Order/quote.bin");
    let filesystem = TestFileSystem::new(
        [
            (first.clone(), MINIMAL_ORDER.to_vec()),
            (second.clone(), MINIMAL_VENDORS.to_vec()),
            (unrelated.clone(), b"unrelated bytes".to_vec()),
            (supporting.clone(), SUPPORTING_A.to_vec()),
        ],
        &[1],
    );
    let plan = [
        FileReplacement::new(&first, FULL_ORDER),
        FileReplacement::new(&second, FULL_VENDORS),
    ];

    let error = replace_validated_files(&plan, &filesystem, validate_canonical_file)
        .expect_err("the second atomic replacement must fail");

    assert!(matches!(
        error.failure(),
        MultiFileReplacementFailure::Replacement {
            operation_index: 1,
            destination,
            error,
        } if destination == &second
            && error.failure()
                == &ValidatedReplacementFailure::Replacement(
                    InjectedError::ReplacementCall(1)
                )
    ));
    assert!(error.rollback_failures().is_empty());
    assert_eq!(filesystem.bytes(&first), MINIMAL_ORDER);
    assert_eq!(filesystem.bytes(&second), MINIMAL_VENDORS);
    assert_eq!(filesystem.bytes(&unrelated), b"unrelated bytes");
    assert_eq!(filesystem.bytes(&supporting), SUPPORTING_A);
    assert_eq!(
        filesystem.replacement_order.into_inner(),
        vec![first.clone(), second.clone(), first]
    );
}

#[test]
fn every_reverse_restoration_failure_is_explicit_beside_the_initiating_failure() {
    let first = PathBuf::from("/package/Orders/First/.spielorders/order.json");
    let second = PathBuf::from("/package/Orders/Second/.spielorders/order.json");
    let third = PathBuf::from("/package/.spielorders/vendors.json");
    let filesystem = TestFileSystem::new(
        [
            (first.clone(), MINIMAL_ORDER.to_vec()),
            (second.clone(), MINIMAL_ORDER.to_vec()),
            (third.clone(), MINIMAL_VENDORS.to_vec()),
        ],
        &[2, 3, 4],
    );
    let plan = [
        FileReplacement::new(&first, FULL_ORDER),
        FileReplacement::new(&second, FULL_ORDER),
        FileReplacement::new(&third, FULL_VENDORS),
    ];

    let error = replace_validated_files(&plan, &filesystem, validate_canonical_file)
        .expect_err("the third replacement and both restorations must fail");

    assert!(matches!(
        error.failure(),
        MultiFileReplacementFailure::Replacement {
            operation_index: 2,
            destination,
            error,
        } if destination == &third
            && error.failure()
                == &ValidatedReplacementFailure::Replacement(
                    InjectedError::ReplacementCall(2)
                )
    ));
    let rollback_failures = error.rollback_failures();
    assert_eq!(rollback_failures.len(), 2);
    assert_eq!(rollback_failures[0].operation_index(), 1);
    assert_eq!(rollback_failures[0].destination(), second);
    assert_eq!(
        rollback_failures[0].error().failure(),
        &ValidatedReplacementFailure::Replacement(InjectedError::ReplacementCall(3))
    );
    assert_eq!(rollback_failures[1].operation_index(), 0);
    assert_eq!(rollback_failures[1].destination(), first);
    assert_eq!(
        rollback_failures[1].error().failure(),
        &ValidatedReplacementFailure::Replacement(InjectedError::ReplacementCall(4))
    );
    assert_eq!(filesystem.bytes(&first), FULL_ORDER);
    assert_eq!(filesystem.bytes(&second), FULL_ORDER);
    assert_eq!(filesystem.bytes(&third), MINIMAL_VENDORS);
    assert_eq!(
        filesystem.replacement_order.into_inner(),
        vec![first.clone(), second.clone(), third, second, first]
    );
}

fn validate_canonical_file(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    match destination.file_name().and_then(|name| name.to_str()) {
        Some("order.json") => OrderDocument::from_json_bytes(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Some("vendors.json") => VendorsDocument::from_json_bytes(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        _ => Err("unexpected canonical destination".into()),
    }
}

/// A destination edited by someone else after this operation read it must not be
/// written over. The edit is introduced through the effect port partway through the
/// operation, so the window this closes is the real one, not a narrowed one.
#[test]
fn a_destination_changed_after_the_prior_read_is_refused_rather_than_overwritten() {
    let tree = TemporaryTree::new("multi-file-concurrent-forward");
    let root = tree.path();
    let order = root.join("order.json");
    let vendors = root.join("vendors.json");
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";

    let filesystem = TestFileSystem::new(
        [
            (order.clone(), MINIMAL_ORDER.to_vec()),
            (vendors.clone(), MINIMAL_VENDORS.to_vec()),
        ],
        &[],
    )
    // Lands after both prior reads, before the second destination is staged, so it
    // sits squarely inside the window between observing and writing.
    .editing_before_staging(1, &vendors, CONCURRENT);

    let error = replace_validated_files(
        &[
            FileReplacement::new(&order, FULL_ORDER),
            FileReplacement::new(&vendors, FULL_VENDORS),
        ],
        &filesystem,
        |_, _| Ok::<(), Infallible>(()),
    )
    .expect_err("a destination changed by another writer is refused");

    assert!(
        matches!(
            error.failure(),
            MultiFileReplacementFailure::DestinationChanged { operation_index: 1, destination, .. }
                if destination == &vendors
        ),
        "the refusal names the destination and the operation: {:?}",
        error.failure()
    );
    // The other writer's bytes survive. That is the whole point.
    assert_eq!(
        filesystem.bytes(&vendors),
        CONCURRENT,
        "the concurrent writer's work is intact"
    );
    // And the batch still unwinds: the operation that did complete is put back,
    // because nobody else had touched it.
    assert_eq!(
        filesystem.bytes(&order),
        MINIMAL_ORDER,
        "the completed operation is rolled back"
    );
    assert!(
        error.rollback_failures().is_empty() && error.refused_rollbacks().is_empty(),
        "an uncontended restoration neither fails nor is refused: {:?} {:?}",
        error.rollback_failures(),
        error.refused_rollbacks()
    );
}

/// The same rule on the recovery path. A rollback restores only what this operation
/// actually put there; a destination someone else has since edited is left alone and
/// the refusal is reported rather than silently skipped.
#[test]
fn rollback_leaves_a_destination_another_writer_changed_and_reports_it() {
    let tree = TemporaryTree::new("multi-file-concurrent-rollback");
    let root = tree.path();
    let order = root.join("order.json");
    let vendors = root.join("vendors.json");
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";

    let filesystem = TestFileSystem::new(
        [
            (order.clone(), MINIMAL_ORDER.to_vec()),
            (vendors.clone(), MINIMAL_VENDORS.to_vec()),
        ],
        // The second destination fails, so operation 0 is rolled back.
        &[1],
    )
    // Lands after operation 0 wrote the Order and before the failing write, so the
    // rollback that follows would overwrite it.
    .editing_before_replace(1, &order, CONCURRENT);

    let error = replace_validated_files(
        &[
            FileReplacement::new(&order, FULL_ORDER),
            FileReplacement::new(&vendors, FULL_VENDORS),
        ],
        &filesystem,
        |_, _| Ok::<(), Infallible>(()),
    )
    .expect_err("the second replacement fails");

    assert!(
        matches!(
            error.failure(),
            MultiFileReplacementFailure::Replacement {
                operation_index: 1,
                ..
            }
        ),
        "the initiating failure is still the one that stopped the run: {:?}",
        error.failure()
    );
    // The rollback refused rather than clobbering, and said so in its own channel:
    // this is not a restoration that failed, it is one that must not be attempted.
    assert!(
        error
            .refused_rollbacks()
            .iter()
            .any(|refusal| refusal.destination() == order && refusal.operation_index() == 0),
        "the refused restoration is reported beside the initiating failure: {:?}",
        error.refused_rollbacks()
    );
    assert!(
        error.rollback_failures().is_empty(),
        "a refusal is not a restoration failure: {:?}",
        error.rollback_failures()
    );
    assert_eq!(
        filesystem.bytes(&order),
        CONCURRENT,
        "the concurrent writer's work survives the rollback"
    );
}

/// Caller-supplied evidence guards a wider window than the batch's own snapshot read.
///
/// Here the change lands before the operation starts, so the batch's snapshot already
/// sees the other writer's bytes and would happily replace them. Only the caller knows
/// what it actually authorized the commit against, and carrying that is what refuses.
#[test]
fn a_destination_changed_before_the_batch_began_is_refused_on_the_callers_evidence() {
    let tree = TemporaryTree::new("multi-file-stale-caller-evidence");
    let root = tree.path();
    let order = root.join("order.json");
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";

    // The caller observed MINIMAL_ORDER. Someone else has written since.
    let filesystem = TestFileSystem::new([(order.clone(), CONCURRENT.to_vec())], &[]);

    let error = replace_validated_files(
        &[FileReplacement::observed(&order, FULL_ORDER, MINIMAL_ORDER)],
        &filesystem,
        |_, _| Ok::<(), Infallible>(()),
    )
    .expect_err("a destination that no longer matches the caller's evidence is refused");

    assert!(
        matches!(
            error.failure(),
            MultiFileReplacementFailure::DestinationChanged {
                operation_index: 0,
                destination,
            } if destination == &order
        ),
        "the refusal names the destination: {:?}",
        error.failure()
    );
    assert_eq!(
        filesystem.bytes(&order),
        CONCURRENT,
        "the concurrent writer's work is intact"
    );
}
