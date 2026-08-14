//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

mod support;

use spielorders_core::filesystem::{ValidatedReplacementFailure, replace_validated_file};
use spielorders_core::metadata::OrderDocument;
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use support::{RealFileSystem, TemporaryTree};

const OLD_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const NEW_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const SUPPORTING_MATERIAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    StagingWrite,
    ValidationRead,
    Durability,
    Replacement,
    Cleanup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedFileSystemError {
    Boundary(Boundary),
    Missing,
    AlreadyExists,
}

struct InjectedFileSystem {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    fail_at: Boundary,
    reads: RefCell<Vec<PathBuf>>,
}

impl InjectedFileSystem {
    fn new(destination: &Path, fail_at: Boundary) -> Self {
        let files = BTreeMap::from([
            (destination.to_path_buf(), OLD_ORDER.to_vec()),
            (
                destination.parent().unwrap().join("quote.bin"),
                SUPPORTING_MATERIAL.to_vec(),
            ),
            (
                destination.parent().unwrap().join("unrelated.txt"),
                b"unrelated bytes".to_vec(),
            ),
        ]);
        Self {
            files: RefCell::new(files),
            fail_at,
            reads: RefCell::new(Vec::new()),
        }
    }

    fn bytes(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.borrow().get(path).cloned()
    }
}

impl FileSystem for InjectedFileSystem {
    type Error = InjectedFileSystemError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.reads.borrow_mut().push(path.to_path_buf());
        if self.fail_at == Boundary::ValidationRead {
            return Err(InjectedFileSystemError::Boundary(Boundary::ValidationRead));
        }
        self.bytes(path).ok_or(InjectedFileSystemError::Missing)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if self.fail_at == Boundary::StagingWrite {
            return Err(InjectedFileSystemError::Boundary(Boundary::StagingWrite));
        }
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(InjectedFileSystemError::AlreadyExists);
        }
        files.insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        if self.fail_at == Boundary::Durability {
            Err(InjectedFileSystemError::Boundary(Boundary::Durability))
        } else {
            Ok(())
        }
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if self.fail_at == Boundary::Replacement {
            return Err(InjectedFileSystemError::Boundary(Boundary::Replacement));
        }
        let mut files = self.files.borrow_mut();
        let candidate = files
            .remove(source)
            .ok_or(InjectedFileSystemError::Missing)?;
        files.insert(destination.to_path_buf(), candidate);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_at == Boundary::Cleanup {
            return Err(InjectedFileSystemError::Boundary(Boundary::Cleanup));
        }
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or(InjectedFileSystemError::Missing)
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

struct InjectedScenario {
    destination: PathBuf,
    staging: PathBuf,
    supporting: PathBuf,
    unrelated: PathBuf,
    filesystem: InjectedFileSystem,
}

impl InjectedScenario {
    fn new(fail_at: Boundary) -> Self {
        let destination = PathBuf::from("/package/Order/.spielorders/order.json");
        let staging = destination.with_file_name(".order.json.spielorders-stage");
        let supporting = destination.parent().unwrap().join("quote.bin");
        let unrelated = destination.parent().unwrap().join("unrelated.txt");
        let filesystem = InjectedFileSystem::new(&destination, fail_at);
        Self {
            destination,
            staging,
            supporting,
            unrelated,
            filesystem,
        }
    }

    fn assert_existing_content_is_preserved(&self) {
        assert_eq!(self.filesystem.bytes(&self.destination).unwrap(), OLD_ORDER);
        assert_eq!(
            self.filesystem.bytes(&self.supporting).unwrap(),
            SUPPORTING_MATERIAL
        );
        assert_eq!(
            self.filesystem.bytes(&self.unrelated).unwrap(),
            b"unrelated bytes"
        );
    }
}

#[test]
fn valid_order_atomically_becomes_the_exact_destination_bytes() {
    let temporary = TemporaryTree::new("atomic-replacement");
    let destination = temporary.path().join("Order/.spielorders/order.json");
    let unrelated = temporary.path().join("unrelated.txt");
    let supporting = temporary.path().join("Order/quote.bin");
    std::fs::create_dir_all(destination.parent().expect("destination has a parent"))
        .expect("metadata directory is created");
    std::fs::write(&destination, OLD_ORDER).expect("old canonical Order is written");
    std::fs::write(&unrelated, b"unrelated bytes").expect("unrelated file is written");
    std::fs::write(&supporting, SUPPORTING_MATERIAL).expect("Supporting Material is written");

    replace_validated_file(&destination, NEW_ORDER, &RealFileSystem, |bytes| {
        OrderDocument::from_json_bytes(bytes).map(|_| ())
    })
    .expect("valid staged Order replaces the canonical destination");

    assert_eq!(std::fs::read(&destination).unwrap(), NEW_ORDER);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated bytes");
    assert_eq!(std::fs::read(&supporting).unwrap(), SUPPORTING_MATERIAL);
}

#[test]
fn invalid_candidate_preserves_the_old_destination_and_cleans_staging() {
    let temporary = TemporaryTree::new("atomic-replacement");
    let destination = temporary.path().join("Order/.spielorders/order.json");
    std::fs::create_dir_all(destination.parent().expect("destination has a parent"))
        .expect("metadata directory is created");
    std::fs::write(&destination, OLD_ORDER).expect("old canonical Order is written");

    let error = replace_validated_file(&destination, b"{ malformed", &RealFileSystem, |bytes| {
        OrderDocument::from_json_bytes(bytes).map(|_| ())
    })
    .expect_err("invalid staged bytes must be rejected");

    assert!(matches!(
        error.failure(),
        ValidatedReplacementFailure::CandidateValidation(_)
    ));
    assert_eq!(std::fs::read(&destination).unwrap(), OLD_ORDER);
    assert!(
        !destination
            .with_file_name(".order.json.spielorders-stage")
            .exists()
    );
}

#[test]
fn validation_read_failure_preserves_every_existing_file_and_cleans_staging() {
    let scenario = InjectedScenario::new(Boundary::ValidationRead);

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |_bytes| Ok::<_, &'static str>(()),
    )
    .expect_err("validation read failure must reject replacement");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::ValidationRead(InjectedFileSystemError::Boundary(
            Boundary::ValidationRead
        ))
    );
    scenario.assert_existing_content_is_preserved();
    assert_eq!(
        scenario.filesystem.reads.borrow().as_slice(),
        std::slice::from_ref(&scenario.staging)
    );
    assert_eq!(scenario.filesystem.bytes(&scenario.staging), None);
}

#[test]
fn durability_failure_preserves_every_existing_file_and_cleans_staging() {
    let scenario = InjectedScenario::new(Boundary::Durability);

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |bytes| OrderDocument::from_json_bytes(bytes).map(|_| ()),
    )
    .expect_err("durability failure must reject replacement");

    assert!(matches!(
        error.failure(),
        ValidatedReplacementFailure::Durability(InjectedFileSystemError::Boundary(
            Boundary::Durability
        ))
    ));
    scenario.assert_existing_content_is_preserved();
    assert_eq!(
        scenario.filesystem.reads.borrow().as_slice(),
        std::slice::from_ref(&scenario.staging)
    );
    assert_eq!(scenario.filesystem.bytes(&scenario.staging), None);
}

#[test]
fn replacement_failure_preserves_every_existing_file_and_cleans_staging() {
    let scenario = InjectedScenario::new(Boundary::Replacement);

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |bytes| OrderDocument::from_json_bytes(bytes).map(|_| ()),
    )
    .expect_err("atomic replacement failure must reject replacement");

    assert!(matches!(
        error.failure(),
        ValidatedReplacementFailure::Replacement(InjectedFileSystemError::Boundary(
            Boundary::Replacement
        ))
    ));
    scenario.assert_existing_content_is_preserved();
    assert_eq!(
        scenario.filesystem.reads.borrow().as_slice(),
        std::slice::from_ref(&scenario.staging)
    );
    assert_eq!(scenario.filesystem.bytes(&scenario.staging), None);
}

#[test]
fn staging_write_failure_never_touches_the_destination_or_other_content() {
    let scenario = InjectedScenario::new(Boundary::StagingWrite);

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |_bytes| Ok::<_, &'static str>(()),
    )
    .expect_err("staging write failure must reject replacement");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::StagingWrite(InjectedFileSystemError::Boundary(
            Boundary::StagingWrite
        ))
    );
    scenario.assert_existing_content_is_preserved();
    assert!(scenario.filesystem.reads.borrow().is_empty());
    assert_eq!(scenario.filesystem.bytes(&scenario.staging), None);
}

#[test]
fn cleanup_failure_is_reported_and_leaves_beside_destination_evidence() {
    let scenario = InjectedScenario::new(Boundary::Cleanup);

    let error = replace_validated_file(
        &scenario.destination,
        b"invalid candidate",
        &scenario.filesystem,
        |_bytes| Err::<(), _>("invalid candidate"),
    )
    .expect_err("candidate and cleanup failures must both be reported");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::CandidateValidation("invalid candidate")
    );
    assert_eq!(
        error.cleanup_failure(),
        Some(&InjectedFileSystemError::Boundary(Boundary::Cleanup))
    );
    scenario.assert_existing_content_is_preserved();
    assert_eq!(scenario.staging.parent(), scenario.destination.parent());
    assert_eq!(
        scenario.filesystem.bytes(&scenario.staging).unwrap(),
        b"invalid candidate"
    );
}

#[test]
fn existing_staging_evidence_is_never_overwritten_or_deleted() {
    let scenario = InjectedScenario::new(Boundary::Cleanup);
    scenario
        .filesystem
        .files
        .borrow_mut()
        .insert(scenario.staging.clone(), b"abandoned evidence".to_vec());

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |_bytes| Ok::<_, &'static str>(()),
    )
    .expect_err("existing staging evidence blocks a new replacement");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::StagingWrite(InjectedFileSystemError::AlreadyExists)
    );
    assert_eq!(
        scenario.filesystem.bytes(&scenario.destination).unwrap(),
        OLD_ORDER
    );
    assert_eq!(
        scenario.filesystem.bytes(&scenario.staging).unwrap(),
        b"abandoned evidence"
    );
    assert_eq!(error.cleanup_failure(), None);
}
