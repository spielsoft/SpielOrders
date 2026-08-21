//! Decision sources: Define the stable M2 diagnostic vocabulary and evidence
//! contract; Define the complete M2 Package read projection and load-state
//! contract; Define the SpielOrders package topology.

mod support;

use spielorders_core::metadata::IndexDocument;
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use spielorders_core::readme::{
    DiagnosticCode, DiagnosticEvidence, EvidenceScope, FileSystemOperation, Location,
    LocationError, OperationEvidence, OperationFailureCode, ReadmeKind, ReadmeRead,
    ReadmeReference, ReadmeReferenceError, ReadmeUnavailableReason, Severity, read_readme,
};
use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
};
use support::{RealFileSystem, TemporaryTree};

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SCENARIOS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/scenarios.json"
));

const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const FULL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
));

#[derive(Clone, Debug, Eq, PartialEq)]
enum Effect {
    Read(PathBuf),
    Write(PathBuf),
    WriteNew(PathBuf),
    Sync(PathBuf),
    Replace {
        source: PathBuf,
        destination: PathBuf,
    },
    Remove(PathBuf),
    Canonicalize(PathBuf),
    ObserveKind(PathBuf),
    ListDirectory(PathBuf),
}

#[derive(Default)]
struct RecordingRealFileSystem {
    effects: RefCell<Vec<Effect>>,
}

impl FileSystem for RecordingRealFileSystem {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Read(path.to_path_buf()));
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Write(path.to_path_buf()));
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::WriteNew(path.to_path_buf()));
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Sync(path.to_path_buf()));
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.effects.borrow_mut().push(Effect::Replace {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
        });
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Remove(path.to_path_buf()));
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Canonicalize(path.to_path_buf()));
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveKind(path.to_path_buf()));
        RealFileSystem.symlink_metadata(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ListDirectory(path.to_path_buf()));
        RealFileSystem.read_directory(path)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedError {
    Observe,
    ReadPrimary,
    ReadAlternate,
}

#[derive(Default)]
struct TestFileSystem {
    kind: Option<FileSystemEntryKind>,
    bytes: Vec<u8>,
    read_error: Option<InjectedError>,
    fail_observe: bool,
    observed: RefCell<Vec<PathBuf>>,
    reads: RefCell<Vec<PathBuf>>,
}

impl TestFileSystem {
    fn readable(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: Some(FileSystemEntryKind::File),
            bytes: bytes.into(),
            ..Self::default()
        }
    }

    fn with_kind(kind: FileSystemEntryKind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::default()
        }
    }

    fn unreadable(read_error: InjectedError) -> Self {
        Self {
            kind: Some(FileSystemEntryKind::File),
            read_error: Some(read_error),
            ..Self::default()
        }
    }

    fn observation_failure() -> Self {
        Self {
            fail_observe: true,
            ..Self::default()
        }
    }
}

impl FileSystem for TestFileSystem {
    type Error = InjectedError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.reads.borrow_mut().push(path.to_path_buf());
        if let Some(error) = self.read_error {
            Err(error)
        } else {
            Ok(self.bytes.clone())
        }
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("README reads never write")
    }

    fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("README reads never stage")
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("README reads never sync")
    }

    fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
        unreachable!("README reads never replace")
    }

    fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("README reads never clean up")
    }

    fn canonicalize(&self, _path: &Path) -> Result<PathBuf, Self::Error> {
        unreachable!("the supplied README location is already absolute")
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.observed.borrow_mut().push(path.to_path_buf());
        if self.fail_observe {
            Err(InjectedError::Observe)
        } else {
            Ok(self.kind)
        }
    }

    fn read_directory(&self, _path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        unreachable!("README reads never list a directory")
    }
}

#[test]
fn absent_package_readme_is_an_empty_document_without_a_diagnostic() {
    assert!(
        scenario("README absence")["tree"]
            .as_array()
            .expect("absence recipe has a tree")
            .is_empty()
    );
    let location = Location::within_package("/package/README.md", "README.md")
        .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::PackageReadme, location);
    let filesystem = TestFileSystem::default();

    let result = read_readme(&reference, &filesystem).expect("absence is an ordinary result");

    assert_eq!(result.reference(), &reference);
    assert!(!result.exists());
    assert_eq!(result.text(), Some(""));
    assert!(result.diagnostics().is_empty());
    assert_eq!(
        filesystem.observed.into_inner(),
        vec![PathBuf::from("/package/README.md")]
    );
    assert!(filesystem.reads.into_inner().is_empty());
}

#[test]
fn readable_order_readme_returns_exact_utf8_text_from_the_requested_path() {
    let location = Location::within_package(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    )
    .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::OrderReadme, location);
    let filesystem = TestFileSystem::readable("# Café optics\r\n\r\nΔλ stays exact.\n");

    let result = read_readme(&reference, &filesystem).expect("ordinary UTF-8 is readable");

    assert_eq!(result.reference(), &reference);
    assert!(result.exists());
    assert_eq!(
        result.text(),
        Some("# Café optics\r\n\r\nΔλ stays exact.\n")
    );
    assert!(result.diagnostics().is_empty());
    assert_eq!(
        filesystem.observed.into_inner(),
        vec![PathBuf::from("/package/Orders/Optics/README.md")]
    );
    assert_eq!(
        filesystem.reads.into_inner(),
        vec![PathBuf::from("/package/Orders/Optics/README.md")]
    );
}

#[test]
fn present_non_file_readme_is_unavailable_without_following_or_reading_it() {
    for kind in [
        FileSystemEntryKind::Directory,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        let location = Location::within_package("/package/README.md", "README.md")
            .expect("fixture paths establish Package membership");
        let reference = readme_reference(ReadmeKind::PackageReadme, location.clone());
        let filesystem = TestFileSystem::with_kind(kind);

        let result = read_readme(&reference, &filesystem).expect("wrong kind is a document result");

        assert_unavailable(
            &result,
            &location,
            ReadmeKind::PackageReadme,
            ReadmeUnavailableReason::WrongKind,
        );
        assert_eq!(
            filesystem.observed.into_inner(),
            vec![PathBuf::from("/package/README.md")]
        );
        assert!(filesystem.reads.into_inner().is_empty());
    }
}

#[test]
fn injected_read_failure_is_a_stable_unreadable_document_warning() {
    let location = Location::within_package(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    )
    .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::OrderReadme, location.clone());
    let filesystem = TestFileSystem::unreadable(InjectedError::ReadPrimary);

    let result = read_readme(&reference, &filesystem)
        .expect("an unreadable README is an unavailable document result");

    assert_unavailable(
        &result,
        &location,
        ReadmeKind::OrderReadme,
        ReadmeUnavailableReason::Unreadable,
    );
    assert_eq!(
        filesystem.observed.into_inner(),
        vec![PathBuf::from("/package/Orders/Optics/README.md")]
    );
    assert_eq!(
        filesystem.reads.into_inner(),
        vec![PathBuf::from("/package/Orders/Optics/README.md")]
    );
}

#[test]
fn invalid_utf8_is_unavailable_instead_of_lossily_decoded() {
    let recipe = scenario("README unreadable");
    assert_eq!(
        recipe["tree"][0]["source"],
        "fixtures/assets/supporting-b.bin"
    );
    assert_eq!(recipe["tree"][0]["readability"], "non_utf8_recipe");
    let location = Location::within_package(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    )
    .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::OrderReadme, location.clone());
    let mut non_utf8_recipe = SUPPORTING_B.to_vec();
    non_utf8_recipe.extend_from_slice(&[0xff, 0xfe, 0xfd]);
    let filesystem = TestFileSystem::readable(non_utf8_recipe);

    let result = read_readme(&reference, &filesystem)
        .expect("invalid encoding is an unavailable document result");

    assert_unavailable(
        &result,
        &location,
        ReadmeKind::OrderReadme,
        ReadmeUnavailableReason::InvalidEncoding,
    );
}

#[test]
fn observation_failure_returns_stable_operation_evidence_without_fabricating_existence() {
    let location = Location::within_package("/package/README.md", "README.md")
        .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::PackageReadme, location.clone());
    let filesystem = TestFileSystem::observation_failure();

    let failure = read_readme(&reference, &filesystem)
        .expect_err("failed observation cannot produce a document result");

    assert_eq!(
        failure.code(),
        OperationFailureCode::FilesystemOperationFailed
    );
    assert_eq!(failure.scope(), EvidenceScope::Operation);
    assert_eq!(failure.primary_location(), &location);
    assert!(failure.related_locations().is_empty());
    assert_eq!(
        failure.evidence(),
        &OperationEvidence::FileSystem {
            operation: FileSystemOperation::ObserveKind,
        }
    );
    assert_eq!(
        filesystem.observed.into_inner(),
        vec![PathBuf::from("/package/README.md")]
    );
    assert!(filesystem.reads.into_inner().is_empty());
}

#[test]
fn explicit_read_never_accepts_version_one_index_readme_text_as_current_evidence() {
    let index = IndexDocument::from_json_bytes(FULL_INDEX).expect("canonical index fixture parses");
    let cached_text = &index.orders[0].readme;
    let location = Location::within_package(
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
        "Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    )
    .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::OrderReadme, location);
    let filesystem = TestFileSystem::readable("current README bytes\n");

    let result = read_readme(&reference, &filesystem).expect("current README is readable");

    assert_ne!(result.text(), Some(cached_text.as_str()));
    assert_eq!(result.text(), Some("current README bytes\n"));
    assert_eq!(filesystem.reads.borrow().len(), 1);
}

#[test]
fn locations_require_absolute_native_and_normalized_package_relative_paths() {
    let outside = Location::absolute("/outside/README.md")
        .expect("absolute outside location is representable");
    assert_eq!(outside.absolute_path(), Path::new("/outside/README.md"));
    assert_eq!(outside.package_relative_path(), None);

    let inside = Location::within_package(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    )
    .expect("normalized established Package location is representable");
    assert_eq!(
        inside.package_relative_path(),
        Some("Orders/Optics/README.md")
    );

    assert_eq!(
        Location::absolute("relative/README.md"),
        Err(LocationError::AbsolutePathRequired)
    );
    for invalid in [
        "",
        "/README.md",
        "C:/README.md",
        "C:README.md",
        "Orders\\README.md",
        "Orders//README.md",
        "Orders/./README.md",
        "Orders/../README.md",
    ] {
        assert_eq!(
            Location::within_package("/package/README.md", invalid),
            Err(LocationError::InvalidPackageRelativePath),
            "{invalid:?} must not become Package-relative evidence"
        );
    }
}

#[test]
fn readme_references_require_exact_established_conventional_locations() {
    let unestablished = Location::absolute("/package/README.md")
        .expect("absolute locations may exist before Package membership is established");
    assert_eq!(
        ReadmeReference::new(ReadmeKind::PackageReadme, unestablished),
        Err(ReadmeReferenceError::PackageRelativeLocationRequired)
    );

    let ordinary_document = Location::within_package("/package/notes.md", "notes.md")
        .expect("ordinary document location is structurally valid");
    assert_eq!(
        ReadmeReference::new(ReadmeKind::PackageReadme, ordinary_document),
        Err(ReadmeReferenceError::NotConventionalReadme)
    );

    let mismatched = Location::within_package("/package/Orders/A/README.md", "Orders/B/README.md")
        .expect("both individual paths are normalized");
    assert_eq!(
        ReadmeReference::new(ReadmeKind::OrderReadme, mismatched),
        Err(ReadmeReferenceError::LocationMismatch)
    );

    let order_location =
        Location::within_package("/package/Orders/A/README.md", "Orders/A/README.md")
            .expect("Order README location is valid");
    assert_eq!(
        ReadmeReference::new(ReadmeKind::PackageReadme, order_location),
        Err(ReadmeReferenceError::DocumentKindMismatch)
    );
    let package_location = Location::within_package("/package/README.md", "README.md")
        .expect("Package README location is valid");
    assert_eq!(
        ReadmeReference::new(ReadmeKind::OrderReadme, package_location),
        Err(ReadmeReferenceError::DocumentKindMismatch)
    );
}

#[test]
fn real_read_touches_only_readme_and_preserves_both_opaque_sentinels() {
    let temporary = TemporaryTree::new("readme-preservation");
    let root = temporary.path().join("package");
    let order = root.join("Orders/Optics");
    let readme = order.join("README.md");
    let supporting_a = order.join("quote.bin");
    let supporting_b = order.join("nested/drawing.bin");
    std::fs::create_dir_all(supporting_b.parent().expect("sentinel has a parent"))
        .expect("Order tree is created");
    std::fs::write(&readme, b"current README\n").expect("README is written");
    std::fs::write(&supporting_a, SUPPORTING_A).expect("first sentinel is written");
    std::fs::write(&supporting_b, SUPPORTING_B).expect("second sentinel is written");
    let before_a = std::fs::read(&supporting_a).expect("first sentinel is readable before");
    let before_b = std::fs::read(&supporting_b).expect("second sentinel is readable before");
    let location = Location::within_package(&readme, "Orders/Optics/README.md")
        .expect("fixture paths establish Package membership");
    let reference = readme_reference(ReadmeKind::OrderReadme, location);
    let filesystem = RecordingRealFileSystem::default();

    let result = read_readme(&reference, &filesystem).expect("README is readable");

    assert_eq!(result.text(), Some("current README\n"));
    assert_eq!(
        filesystem.effects.into_inner(),
        vec![Effect::ObserveKind(readme.clone()), Effect::Read(readme)]
    );
    assert_eq!(std::fs::read(&supporting_a).unwrap(), before_a);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), before_b);
}

#[test]
fn diagnostic_equality_and_order_use_only_stable_typed_evidence() {
    let location_a = Location::within_package("/z-root/Orders/A/README.md", "Orders/A/README.md")
        .expect("first fixture location is valid");
    let reference_a = readme_reference(ReadmeKind::OrderReadme, location_a);
    let primary = TestFileSystem::unreadable(InjectedError::ReadPrimary);
    let alternate = TestFileSystem::unreadable(InjectedError::ReadAlternate);

    let primary_warning = read_readme(&reference_a, &primary)
        .expect("read failure becomes a warning")
        .diagnostics()[0]
        .clone();
    let alternate_warning = read_readme(&reference_a, &alternate)
        .expect("alternate read failure becomes a warning")
        .diagnostics()[0]
        .clone();

    assert_eq!(primary_warning, alternate_warning);

    let location_b = Location::within_package("/a-root/Orders/B/README.md", "Orders/B/README.md")
        .expect("second fixture location is valid");
    let reference_b = readme_reference(ReadmeKind::OrderReadme, location_b);
    let warning_b = read_readme(
        &reference_b,
        &TestFileSystem::unreadable(InjectedError::ReadAlternate),
    )
    .expect("read failure becomes a warning")
    .diagnostics()[0]
        .clone();
    let mut warnings = [warning_b, primary_warning.clone()];

    warnings.sort();

    assert_eq!(warnings[0], primary_warning);
}

#[test]
fn public_evidence_vocabulary_uses_the_closed_stable_spellings() {
    assert_eq!(
        DiagnosticCode::ReadmeUnavailable.as_str(),
        "readme_unavailable"
    );
    assert_eq!(Severity::Warning.as_str(), "warning");
    assert_eq!(EvidenceScope::Document.as_str(), "document");
    assert_eq!(EvidenceScope::Operation.as_str(), "operation");
    assert_eq!(ReadmeKind::PackageReadme.as_str(), "package_readme");
    assert_eq!(ReadmeKind::OrderReadme.as_str(), "order_readme");
    assert_eq!(ReadmeUnavailableReason::WrongKind.as_str(), "wrong_kind");
    assert_eq!(ReadmeUnavailableReason::Unreadable.as_str(), "unreadable");
    assert_eq!(
        ReadmeUnavailableReason::InvalidEncoding.as_str(),
        "invalid_encoding"
    );
    assert_eq!(
        OperationFailureCode::FilesystemOperationFailed.as_str(),
        "filesystem_operation_failed"
    );
    assert_eq!(FileSystemOperation::ObserveKind.as_str(), "observe_kind");
}

fn readme_reference(kind: ReadmeKind, location: Location) -> ReadmeReference {
    ReadmeReference::new(kind, location).expect("fixture is an exact conventional README")
}

fn assert_unavailable(
    result: &ReadmeRead,
    location: &Location,
    document_kind: ReadmeKind,
    reason: ReadmeUnavailableReason,
) {
    assert!(result.exists());
    assert_eq!(result.text(), None);
    let [warning] = result.diagnostics() else {
        panic!("unavailable README must emit exactly one warning")
    };
    assert_eq!(warning.code(), DiagnosticCode::ReadmeUnavailable);
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.scope(), EvidenceScope::Document);
    assert_eq!(warning.primary_location(), location);
    assert!(warning.related_locations().is_empty());
    assert_eq!(
        warning.evidence(),
        &DiagnosticEvidence::ReadmeUnavailable {
            document_kind,
            reason,
        }
    );
}

fn scenario(name: &str) -> serde_json::Value {
    let corpus: serde_json::Value =
        serde_json::from_str(SCENARIOS).expect("canonical scenario corpus parses");
    corpus["scenarios"]
        .as_array()
        .expect("canonical corpus has scenarios")
        .iter()
        .find(|scenario| scenario["name"] == name)
        .unwrap_or_else(|| panic!("canonical scenario {name:?} exists"))
        .clone()
}
