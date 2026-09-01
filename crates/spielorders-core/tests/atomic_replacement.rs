//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::filesystem::{
    DestinationExpectation, StagedPromotionFailure, ValidatedReplacementFailure,
    promote_staged_file, replace_validated_file, replace_validated_file_expecting,
};
use spielorders_core::metadata::OrderDocument;
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
    /// Every path this filesystem was asked to modify, in order, so a test can prove
    /// an operation touched nothing rather than infer it from surviving state.
    writes: RefCell<Vec<PathBuf>>,
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
            writes: RefCell::new(Vec::new()),
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
        self.bytes(path).ok_or(InjectedFileSystemError::Missing)
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
        self.writes.borrow_mut().push(destination.to_path_buf());
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
        self.writes.borrow_mut().push(path.to_path_buf());
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
fn an_invalid_candidate_preserves_the_old_destination_and_stages_nothing() {
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

/// A candidate that is not valid never reaches the filesystem.
///
/// Validation used to run on a read-back of the staging file, which meant an invalid
/// candidate was written before anyone looked at it. It now runs on the candidate
/// itself, so the operation is refused with nothing to clean up.
#[test]
fn an_invalid_candidate_is_refused_before_anything_is_written() {
    let scenario = InjectedScenario::new(Boundary::Cleanup);

    let error = replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |_candidate| Err::<(), _>("this candidate is not acceptable"),
    )
    .expect_err("an invalid candidate must reject replacement");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::CandidateValidation("this candidate is not acceptable")
    );
    assert!(
        error.cleanup_failure().is_none(),
        "there is nothing to clean up when nothing was written"
    );
    // The effect log is the proof: not one path was modified.
    assert!(
        scenario.filesystem.writes.borrow().is_empty(),
        "an invalid candidate performs no write at all: {:?}",
        scenario.filesystem.writes.borrow()
    );
    assert_eq!(scenario.filesystem.bytes(&scenario.staging), None);
    scenario.assert_existing_content_is_preserved();
}

/// The validated candidate is written without being read back. The read-back compared
/// the bytes just written against the bytes passed in, which detects only a filesystem
/// that lies about writes — and one that lies about writes can lie about reads too.
#[test]
fn a_successful_replacement_never_reads_the_staging_file() {
    let scenario = InjectedScenario::new(Boundary::Cleanup);

    replace_validated_file(
        &scenario.destination,
        NEW_ORDER,
        &scenario.filesystem,
        |_candidate| Ok::<_, &'static str>(()),
    )
    .expect("a valid candidate is written");

    assert_eq!(
        scenario.filesystem.bytes(&scenario.destination).as_deref(),
        Some(NEW_ORDER)
    );
    assert!(
        scenario.filesystem.reads.borrow().is_empty(),
        "a replacement without destination evidence reads nothing at all: {:?}",
        scenario.filesystem.reads.borrow()
    );
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
    assert!(
        scenario.filesystem.reads.borrow().is_empty(),
        "a replacement without destination evidence reads nothing at all: {:?}",
        scenario.filesystem.reads.borrow()
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
    assert!(
        scenario.filesystem.reads.borrow().is_empty(),
        "a replacement without destination evidence reads nothing at all: {:?}",
        scenario.filesystem.reads.borrow()
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

    // A refusal that happens after staging, so there is a staging file to clean up.
    // Candidate validation no longer reaches this point: it runs before any write.
    let error = replace_validated_file_expecting(
        &scenario.destination,
        b"invalid candidate",
        DestinationExpectation::Exactly(b"bytes the destination never held"),
        &scenario.filesystem,
        |_candidate| Ok::<(), &'static str>(()),
    )
    .expect_err("refusal and cleanup failures must both be reported");

    assert_eq!(
        error.failure(),
        &ValidatedReplacementFailure::DestinationChanged
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

/// A destination that cannot be read when the guard checks it. Staging reads still
/// work, so the operation reaches the check rather than failing earlier.
struct UnreadableDestination {
    destination: PathBuf,
}

impl FileSystem for UnreadableDestination {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path == self.destination {
            return Err(std::io::Error::other("injected destination read failure"));
        }
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
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(
        &self,
        path: &Path,
    ) -> Result<spielorders_core::ports::FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
}

/// A single-file replacement carrying what the caller observed refuses a destination
/// someone else has since written, and leaves their bytes in place.
#[test]
fn a_single_file_replacement_refuses_a_destination_that_no_longer_matches() {
    let tree = TemporaryTree::new("atomic-single-file-changed");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";
    std::fs::write(&destination, CONCURRENT).unwrap();

    // The caller observed OLD_ORDER; the destination now holds someone else's bytes.
    let error = replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Exactly(OLD_ORDER),
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect_err("a destination that no longer matches is refused");

    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal says the destination changed: {:?}",
        error.failure()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        CONCURRENT,
        "the other writer's bytes are untouched"
    );
    assert!(
        error.cleanup_failure().is_none(),
        "the staged candidate is cleaned up: {:?}",
        error.cleanup_failure()
    );
}

/// A destination the guard cannot read is refused too, but reported as unreadable
/// rather than as someone else's write. A caller deciding what to do next needs to
/// tell "someone changed this" from "I could not tell".
#[test]
fn a_destination_that_cannot_be_read_is_refused_without_claiming_it_changed() {
    let tree = TemporaryTree::new("atomic-single-file-unreadable");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();

    let error = replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Exactly(OLD_ORDER),
        &UnreadableDestination {
            destination: destination.clone(),
        },
        |_| Ok::<(), String>(()),
    )
    .expect_err("an unreadable destination is refused");

    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationUnreadable(_)
        ),
        "the refusal does not claim a concurrent write it never observed: {:?}",
        error.failure()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        OLD_ORDER,
        "nothing was written"
    );
}

/// Matching evidence changes nothing about the ordinary path.
#[test]
fn a_single_file_replacement_with_matching_evidence_commits_normally() {
    let tree = TemporaryTree::new("atomic-single-file-uncontended");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();

    replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Exactly(OLD_ORDER),
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect("a destination still holding what the caller observed is replaced");

    assert_eq!(std::fs::read(&destination).unwrap(), NEW_ORDER);
}

/// Whether the destination still parses to the Order the caller observed — the
/// smallest predicate that exercises `DestinationExpectation::Equivalent`, which is
/// what these tests are about.
///
/// It is deliberately *not* the predicate the record capabilities carry. `order_edits`
/// and `vendors` parse and then `validate()`, comparing the projection rather than the
/// raw parse, because the documents they planned against are themselves projections. A
/// raw parse is enough to drive the primitive here and would be wrong at a capability;
/// that distinction is pinned in each capability's own suite, not in this one.
fn still_parses_to(observed: &OrderDocument) -> impl Fn(&[u8]) -> bool {
    move |current: &[u8]| {
        OrderDocument::from_json_bytes(current).is_ok_and(|document| &document == observed)
    }
}

fn parsed(bytes: &[u8]) -> OrderDocument {
    OrderDocument::from_json_bytes(bytes).expect("the fixture record parses")
}

/// A semantic expectation refuses a destination someone else has given a different
/// meaning, exactly as byte evidence does, and leaves their record in place.
#[test]
fn a_semantic_expectation_refuses_a_destination_whose_meaning_changed() {
    let tree = TemporaryTree::new("atomic-semantic-meaning-changed");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, NEW_ORDER).unwrap();

    // The caller observed the minimal Order; the destination now holds a different one.
    let observed = parsed(OLD_ORDER);
    let error = replace_validated_file_expecting(
        &destination,
        OLD_ORDER,
        DestinationExpectation::Equivalent(&still_parses_to(&observed)),
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect_err("a destination that no longer means what the caller observed is refused");

    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal names a changed destination rather than a write failure: {:?}",
        error.failure()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        NEW_ORDER,
        "the other writer's record is untouched"
    );
    assert!(
        error.cleanup_failure().is_none(),
        "the staged candidate is cleaned up: {:?}",
        error.cleanup_failure()
    );
    assert!(
        !destination
            .with_file_name(".order.json.spielorders-stage")
            .exists(),
        "no staging file is left beside the destination"
    );
}

/// The property that separates a semantic expectation from byte evidence: a
/// destination reformatted without changing what it means still satisfies it, so the
/// replacement proceeds. The formatting is what gets overwritten, not a meaning.
#[test]
fn a_semantic_expectation_accepts_a_destination_reformatted_without_changing_meaning() {
    let tree = TemporaryTree::new("atomic-semantic-reformatted");
    let destination = tree.path().join("order.json");
    let reformatted = [OLD_ORDER, b"\n"].concat();
    assert_ne!(
        reformatted.as_slice(),
        OLD_ORDER,
        "the reformatted record differs in bytes"
    );
    std::fs::write(&destination, &reformatted).unwrap();

    let observed = parsed(OLD_ORDER);
    replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Equivalent(&still_parses_to(&observed)),
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect("a destination that still parses to what the caller observed is replaced");

    assert_eq!(
        std::fs::read(&destination).unwrap(),
        NEW_ORDER,
        "the replacement landed"
    );
}

/// Byte evidence answers a different question, and this is the case that shows it: the
/// same reformatted destination is refused under `Exactly`. Both variants remain
/// available because both questions are still asked in this codebase.
#[test]
fn byte_evidence_still_refuses_the_reformatting_a_semantic_expectation_accepts() {
    let tree = TemporaryTree::new("atomic-byte-evidence-reformatted");
    let destination = tree.path().join("order.json");
    let reformatted = [OLD_ORDER, b"\n"].concat();
    std::fs::write(&destination, &reformatted).unwrap();

    let error = replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Exactly(OLD_ORDER),
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect_err("byte evidence refuses any byte difference at all");

    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal names a changed destination: {:?}",
        error.failure()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        reformatted,
        "nothing was written"
    );
}

/// A semantic expectation the guard cannot even read is reported as unreadable rather
/// than as someone else's write, exactly as byte evidence is.
#[test]
fn a_semantic_expectation_reports_an_unreadable_destination_distinctly() {
    let tree = TemporaryTree::new("atomic-semantic-unreadable");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();

    let observed = parsed(OLD_ORDER);
    let error = replace_validated_file_expecting(
        &destination,
        NEW_ORDER,
        DestinationExpectation::Equivalent(&still_parses_to(&observed)),
        &UnreadableDestination {
            destination: destination.clone(),
        },
        |_| Ok::<(), String>(()),
    )
    .expect_err("an unreadable destination is refused");

    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationUnreadable(_)
        ),
        "the refusal does not claim a concurrent write it never observed: {:?}",
        error.failure()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        OLD_ORDER,
        "nothing was written"
    );
}

/// The beside-destination name a stage occupies, which is what makes promotion a
/// rename rather than another staged write.
fn stage_of(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(
        ".{}.spielorders-stage",
        destination.file_name().unwrap().to_str().unwrap()
    ))
}

/// An interrupted replacement left a durable candidate beside its destination.
/// Finishing it renames that candidate into place and consumes it.
#[test]
fn a_stage_beside_its_destination_is_published_by_renaming_it_into_place() {
    let tree = TemporaryTree::new("atomic-promote-uncontended");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();
    let stage = stage_of(&destination);
    std::fs::write(&stage, NEW_ORDER).unwrap();

    promote_staged_file(
        &stage,
        &destination,
        DestinationExpectation::Exactly(OLD_ORDER),
        &RealFileSystem,
    )
    .expect("a destination still holding what the caller observed accepts the stage");

    assert_eq!(std::fs::read(&destination).unwrap(), NEW_ORDER);
    assert!(
        !stage.exists(),
        "the candidate became the destination rather than being copied to it"
    );
}

/// A promotion refuses a destination someone else has since written, and leaves both
/// their bytes and the candidate exactly where they are. The stage is evidence this
/// operation did not create, so a refusal never cleans it up.
#[test]
fn a_promotion_refuses_a_changed_destination_and_keeps_the_candidate() {
    let tree = TemporaryTree::new("atomic-promote-changed");
    let destination = tree.path().join("order.json");
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";
    std::fs::write(&destination, CONCURRENT).unwrap();
    let stage = stage_of(&destination);
    std::fs::write(&stage, NEW_ORDER).unwrap();

    let failure = promote_staged_file(
        &stage,
        &destination,
        DestinationExpectation::Exactly(OLD_ORDER),
        &RealFileSystem,
    )
    .expect_err("a destination that no longer matches is refused");

    assert!(
        matches!(failure, StagedPromotionFailure::DestinationChanged),
        "the refusal says the destination changed: {failure:?}"
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        CONCURRENT,
        "the other writer's bytes are untouched"
    );
    assert_eq!(
        std::fs::read(&stage).unwrap(),
        NEW_ORDER,
        "the candidate this operation did not create survives its refusal"
    );
}

/// A destination the guard cannot read is refused as unreadable rather than as
/// someone else's write, exactly as it is for a staged replacement.
#[test]
fn a_promotion_reports_an_unreadable_destination_distinctly() {
    let tree = TemporaryTree::new("atomic-promote-unreadable");
    let destination = tree.path().join("order.json");
    std::fs::write(&destination, OLD_ORDER).unwrap();
    let stage = stage_of(&destination);
    std::fs::write(&stage, NEW_ORDER).unwrap();

    let failure = promote_staged_file(
        &stage,
        &destination,
        DestinationExpectation::Exactly(OLD_ORDER),
        &UnreadableDestination {
            destination: destination.clone(),
        },
    )
    .expect_err("an unreadable destination is refused");

    assert!(
        matches!(failure, StagedPromotionFailure::DestinationUnreadable(_)),
        "the refusal does not claim a concurrent write it never observed: {failure:?}"
    );
    assert_eq!(std::fs::read(&destination).unwrap(), OLD_ORDER);
    assert_eq!(std::fs::read(&stage).unwrap(), NEW_ORDER);
}

/// An observation that nothing was there is evidence like any other, and it means the
/// same thing to both primitives: a write that arrived since is preserved, and an
/// untouched empty destination is written.
#[test]
fn an_expectation_of_absence_refuses_a_destination_something_has_appeared_at() {
    let tree = TemporaryTree::new("atomic-absent-appeared");
    let occupied = tree.path().join("appeared.json");
    const CONCURRENT: &[u8] = b"{\n  \"written\": \"by someone else\"\n}\n";
    std::fs::write(&occupied, CONCURRENT).unwrap();

    let error = replace_validated_file_expecting(
        &occupied,
        NEW_ORDER,
        DestinationExpectation::Absent,
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect_err("a destination that stopped being empty is refused");
    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal says the destination changed: {:?}",
        error.failure()
    );
    assert_eq!(std::fs::read(&occupied).unwrap(), CONCURRENT);

    let stage = stage_of(&occupied);
    std::fs::write(&stage, NEW_ORDER).unwrap();
    let failure = promote_staged_file(
        &stage,
        &occupied,
        DestinationExpectation::Absent,
        &RealFileSystem,
    )
    .expect_err("a promotion onto a destination that stopped being empty is refused");
    assert!(
        matches!(failure, StagedPromotionFailure::DestinationChanged),
        "both primitives read absence the same way: {failure:?}"
    );
    assert_eq!(std::fs::read(&occupied).unwrap(), CONCURRENT);

    // And a destination nothing has appeared at still accepts the write.
    let free = tree.path().join("free.json");
    replace_validated_file_expecting(
        &free,
        NEW_ORDER,
        DestinationExpectation::Absent,
        &RealFileSystem,
        |_| Ok::<(), String>(()),
    )
    .expect("a destination still holding nothing is written");
    assert_eq!(std::fs::read(&free).unwrap(), NEW_ORDER);
}
