//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

mod support;

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
use support::{RealFileSystem, TemporaryTree};

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
}

impl TestFileSystem {
    fn new(files: impl IntoIterator<Item = (PathBuf, Vec<u8>)>, failed_calls: &[usize]) -> Self {
        Self {
            files: RefCell::new(files.into_iter().collect()),
            failed_replacement_calls: failed_calls.iter().copied().collect(),
            replacement_order: RefCell::new(Vec::new()),
            writes: RefCell::new(Vec::new()),
        }
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
