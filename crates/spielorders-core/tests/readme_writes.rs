//! Decision sources: Define the SpielOrders package topology; Define the Rust CLI
//! capability contract; Define integrity, validation, and repair boundaries; Define
//! durable metadata schemas and version evolution.

use crate::support::{ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderStatus};
use spielorders_core::mutation::MutationReport;
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, UuidGenerator,
};
use spielorders_core::readme::{ReadmeKind, ReadmeUnavailableReason, read_readme};
use spielorders_core::readme_writes::{
    ReadmeWriteOperationKind, ReadmeWritePlan, ReadmeWritePlanError, ReadmeWriteRejection,
    ReadmeWriteWarning, commit_readme_write, plan_order_readme_write, plan_package_readme_write,
};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Deliberately non-normalized text. It carries CRLF, a lone CR, trailing spaces, a
/// multi-byte character, and no trailing newline, so a writer that normalized line
/// endings, trimmed, or re-encoded would change the bytes instead of preserving them.
const AWKWARD: &str =
    "# Läser components\r\n\r\nlone CR next:\rtrailing spaces:   \nno final newline";

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

fn package_readme(root: &Path) -> Vec<u8> {
    fs::read(root.join("README.md")).expect("the Package README exists")
}

#[test]
fn a_package_readme_write_round_trips_exact_supplied_bytes() {
    let tree = initialized_package("readme-write-package-exact");
    let root = tree.path();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();
    assert_eq!(
        package_readme(root),
        b"",
        "initialization leaves an empty conventional document"
    );

    let plan = plan_package_readme_write(root, AWKWARD, &RealFileSystem)
        .expect("an established Package README can be written");

    assert_eq!(plan.kind(), ReadmeKind::PackageReadme);
    assert_eq!(plan.text(), AWKWARD);
    assert!(plan.prior_exists());
    assert_eq!(plan.prior_text(), Some(""));
    assert!(plan.changes_document());
    // A Package README is not indexed, so its write owns no derived-index action.
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
        [(
            ReadmeWriteOperationKind::ReplaceReadmeDocument,
            Path::new("README.md").to_owned()
        )]
    );
    assert_eq!(package_readme(root), b"", "planning writes nothing");

    let report = commit_readme_write(plan, &RealFileSystem);

    let MutationReport::Committed { plan, result } = report else {
        panic!("a valid Package README write commits: {report:?}");
    };
    assert_eq!(
        package_readme(root),
        AWKWARD.as_bytes(),
        "the document holds the exact supplied bytes"
    );
    assert_eq!(result.document().text(), AWKWARD);
    assert!(result.document().exists());

    // Reading back through the established read capability returns the same bytes.
    let read = read_readme(plan.reference(), &RealFileSystem).expect("the README reads");
    assert_eq!(read.text(), Some(AWKWARD));
    assert!(read.exists());
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a Package README write replaces no derived index"
    );
}

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

/// An adopted Order whose README already carries user-authored text, so a write has
/// prior content to replace rather than only an empty document to fill.
fn order_with_readme(root: &Path, readme: &[u8]) -> PathBuf {
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), readme).expect("the fixture README is written");
    let plan = plan_order_adoption(
        root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn indexed_readme(root: &Path) -> String {
    let index =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    index.orders[0].readme.clone()
}

fn committed(
    plan: ReadmeWritePlan,
) -> (PathBuf, spielorders_core::readme_writes::ReadmeWriteResult) {
    let path = plan.reference().location().absolute_path().to_path_buf();
    let report = commit_readme_write(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid README write commits: {report:?}");
    };
    (path, result)
}

#[test]
fn an_order_readme_write_replaces_its_document_and_refreshes_the_derived_index() {
    let tree = initialized_package("readme-write-order-index");
    let root = tree.path();
    let directory = order_with_readme(
        root,
        b"prior order notes
",
    );
    assert_eq!(
        indexed_readme(root),
        "prior order notes
"
    );

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("a healthy Order README can be written");

    assert_eq!(plan.kind(), ReadmeKind::OrderReadme);
    assert_eq!(
        plan.prior_text(),
        Some(
            "prior order notes
"
        )
    );
    assert!(plan.prior_exists());
    // The Order README is projected into the derived index, so this write owns the
    // refresh the Package README write does not.
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
                ReadmeWriteOperationKind::ReplaceReadmeDocument,
                Path::new("Orders/Legacy_laser_purchase/README.md").to_owned()
            ),
            (
                ReadmeWriteOperationKind::RefreshPackageIndex,
                Path::new(".spielorders/index.json").to_owned()
            ),
        ]
    );

    let (path, result) = committed(plan);

    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
    assert_eq!(result.document().text(), AWKWARD);
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    // Criterion 4: a later load observes the new text through canonical evidence
    // rather than reporting the cached prior text as current.
    assert_eq!(indexed_readme(root), AWKWARD);
    assert_eq!(
        package_readme(root),
        b"",
        "an Order README write leaves the Package README alone"
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Effect {
    Read(PathBuf),
    Write(PathBuf),
    WriteNew(PathBuf),
    Sync(PathBuf),
    Replace { destination: PathBuf },
    Remove(PathBuf),
    Canonicalize(PathBuf),
    ObserveKind(PathBuf),
    ListDirectory(PathBuf),
}

/// Records every filesystem effect a capability performs so a test can prove which
/// paths it did and did not touch. All effects are otherwise real.
#[derive(Default)]
struct RecordingFileSystem {
    effects: RefCell<Vec<Effect>>,
}

impl RecordingFileSystem {
    fn effects(&self) -> Vec<Effect> {
        self.effects.borrow().clone()
    }

    fn record(&self, effect: Effect) {
        self.effects.borrow_mut().push(effect);
    }

    fn replaced(&self, root: &Path) -> Vec<String> {
        self.effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Replace { destination } => Some(relative(root, destination)),
                _ => None,
            })
            .collect()
    }

    fn listings(&self) -> Vec<PathBuf> {
        self.effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::ListDirectory(path) => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    fn touched(&self, root: &Path) -> Vec<String> {
        self.effects()
            .iter()
            .map(|effect| match effect {
                Effect::Read(path)
                | Effect::Write(path)
                | Effect::WriteNew(path)
                | Effect::Sync(path)
                | Effect::Remove(path)
                | Effect::Canonicalize(path)
                | Effect::ObserveKind(path)
                | Effect::ListDirectory(path) => relative(root, path),
                Effect::Replace { destination } => relative(root, destination),
            })
            .collect()
    }
}

/// Effect paths arrive canonicalized, while a fixture root may still contain a
/// symlinked prefix, so both are canonicalized before comparison.
fn relative(root: &Path, path: &Path) -> String {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

impl FileSystem for RecordingFileSystem {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.record(Effect::Read(path.to_path_buf()));
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.record(Effect::Write(path.to_path_buf()));
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.record(Effect::WriteNew(path.to_path_buf()));
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.record(Effect::Sync(path.to_path_buf()));
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.record(Effect::Replace {
            destination: destination.to_path_buf(),
        });
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.record(Effect::Remove(path.to_path_buf()));
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.record(Effect::Canonicalize(path.to_path_buf()));
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.record(Effect::ObserveKind(path.to_path_buf()));
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.record(Effect::ListDirectory(path.to_path_buf()));
        RealFileSystem.read_directory(path)
    }
}

const CANONICAL_METADATA: [&str; 4] = ["order.json", "vendors.json", "version.json", "config.toml"];

fn assert_no_metadata_write(effects: &RecordingFileSystem, label: &str) {
    for effect in effects.effects() {
        let written = match &effect {
            Effect::Write(path) | Effect::WriteNew(path) => path.clone(),
            Effect::Replace { destination } => destination.clone(),
            _ => continue,
        };
        let name = written.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            !CANONICAL_METADATA.contains(&name.as_str()),
            "{label} writes no canonical metadata, but wrote {name}"
        );
    }
}

fn assert_no_supporting_material_touch(effects: &RecordingFileSystem, root: &Path, label: &str) {
    for touched in effects.touched(root) {
        assert!(
            !touched.contains("quote.bin") && !touched.contains("Quotes"),
            "{label} never inspects Supporting Material, but touched {touched}"
        );
    }
}

#[test]
fn a_package_readme_commit_replaces_only_its_own_document() {
    let tree = initialized_package("readme-write-package-effects");
    let root = tree.path();
    let plan = plan_package_readme_write(root, "new package prose\n", &RealFileSystem)
        .expect("the Package README can be written");

    let effects = RecordingFileSystem::default();
    let report = commit_readme_write(plan, &effects);

    assert!(matches!(report, MutationReport::Committed { .. }));
    // A Package README is not indexed, so its write replaces exactly one path and
    // never reaches the derived index. Byte equality alone could not show this: a
    // rebuilt index of unchanged Orders is byte-identical to the one it replaced.
    assert_eq!(effects.replaced(root), ["README.md"]);
    assert_eq!(
        effects.listings(),
        Vec::<PathBuf>::new(),
        "a Package README write lists no directory"
    );
    assert_no_metadata_write(&effects, "a Package README write");
    for touched in effects.touched(root) {
        assert!(
            touched.starts_with("README.md") || touched.starts_with(".README.md"),
            "a Package README write touches only its own document, but touched {touched}"
        );
    }
}

#[test]
fn an_order_readme_commit_replaces_only_its_document_and_the_derived_index() {
    let tree = initialized_package("readme-write-order-effects");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), b"opaque supporting material").unwrap();
    let plan = plan_order_readme_write(&directory, "new order prose\n", &RealFileSystem)
        .expect("the Order README can be written");

    let effects = RecordingFileSystem::default();
    let report = commit_readme_write(plan, &effects);

    assert!(matches!(report, MutationReport::Committed { .. }));
    // Exactly the document and the derived index it is projected into. The refresh
    // lists the Orders Directory, which is that declared derived-index action.
    assert_eq!(
        effects.replaced(root),
        [
            "Orders/Legacy_laser_purchase/README.md",
            ".spielorders/index.json"
        ]
    );
    assert_no_metadata_write(&effects, "an Order README write");
    assert_no_supporting_material_touch(&effects, root, "an Order README write");
}

#[test]
fn planning_either_readme_write_performs_no_stage_write_or_cleanup() {
    let tree = initialized_package("readme-write-dry-run");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let package_before = package_readme(root);
    let order_before = fs::read(directory.join("README.md")).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    for (label, plan) in [
        (
            "package",
            plan_package_readme_write(root, AWKWARD, &RealFileSystem).unwrap(),
        ),
        (
            "order",
            plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap(),
        ),
    ] {
        assert!(plan.changes_document(), "{label}");
        assert!(!plan.operations().is_empty(), "{label}");
    }

    let effects = RecordingFileSystem::default();
    let _ = plan_package_readme_write(root, AWKWARD, &effects).unwrap();
    let _ = plan_order_readme_write(&directory, AWKWARD, &effects).unwrap();

    for effect in effects.effects() {
        assert!(
            !matches!(
                effect,
                Effect::Write(_)
                    | Effect::WriteNew(_)
                    | Effect::Sync(_)
                    | Effect::Replace { .. }
                    | Effect::Remove(_)
            ),
            "a dry run stages, writes, and cleans up nothing, but performed {effect:?}"
        );
    }
    assert_eq!(package_readme(root), package_before);
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), order_before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

fn modified_at(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .expect("the document exists")
        .modified()
        .expect("the platform reports a modification time")
}

#[test]
fn writing_identical_text_is_unchanged_and_replaces_nothing() {
    let tree = initialized_package("readme-write-no-op");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let order_readme = directory.join("README.md");
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());
    let modified_before = modified_at(&order_readme);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("identical text is planned like any other write");

    assert!(!plan.changes_document());
    assert!(
        plan.operations().is_empty(),
        "a write that replaces nothing announces no managed operation"
    );

    let effects = RecordingFileSystem::default();
    let report = commit_readme_write(plan, &effects);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("identical text is an accepted no-op: {report:?}");
    };
    assert_eq!(result.document().text(), AWKWARD);
    // Byte equality cannot show this on its own, since rewriting identical content is
    // byte-identical. The effect log proves no replacement happened at all, and the
    // untouched modification time is the independent second signal.
    assert_eq!(effects.replaced(root), Vec::<String>::new());
    assert!(!effects.effects().iter().any(|effect| matches!(
        effect,
        Effect::Write(_) | Effect::WriteNew(_) | Effect::Sync(_) | Effect::Remove(_)
    )));
    assert_eq!(modified_at(&order_readme), modified_before);
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn empty_text_is_an_ordinary_document_in_both_directions() {
    let tree = initialized_package("readme-write-empty");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");

    // Clearing a populated document to empty is a write, not a no-op.
    let plan = plan_order_readme_write(&directory, "", &RealFileSystem)
        .expect("empty text can be written");
    assert!(plan.changes_document());
    let (path, result) = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), b"");
    assert_eq!(result.document().text(), "");
    assert!(result.document().exists(), "an empty document still exists");
    assert_eq!(indexed_readme(root), "");

    // Writing empty text over an already empty document changes nothing.
    let plan = plan_order_readme_write(&directory, "", &RealFileSystem).unwrap();
    assert!(!plan.changes_document());
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Unchanged { .. }
    ));

    // And the document can be filled again from empty.
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();
    assert_eq!(plan.prior_text(), Some(""));
    let (path, _) = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
}

#[test]
fn an_absent_readme_is_a_creatable_conventional_document() {
    let tree = initialized_package("readme-write-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    // A missing README is normal; the user may simply have deleted it.
    fs::remove_file(&order_readme).unwrap();
    fs::remove_file(root.join("README.md")).unwrap();

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("an absent Order README is creatable");
    assert!(!plan.prior_exists());
    assert_eq!(plan.prior_text(), Some(""));
    assert!(plan.changes_document());
    let (path, result) = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
    assert!(result.document().exists());

    let plan = plan_package_readme_write(root, "created package prose\n", &RealFileSystem)
        .expect("an absent Package README is creatable");
    assert!(!plan.prior_exists());
    let (path, _) = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), b"created package prose\n");

    // Absence and emptiness are different states, so creating an empty document is
    // still a change. A writer that compared only bytes would leave it absent.
    let tree = initialized_package("readme-write-absent-empty");
    let root = tree.path();
    let package_readme_path = root.join("README.md");
    fs::remove_file(&package_readme_path).unwrap();
    let plan = plan_package_readme_write(root, "", &RealFileSystem)
        .expect("an absent Package README accepts empty text");
    assert!(!plan.prior_exists());
    assert!(
        plan.changes_document(),
        "creating an empty document is a change, not a no-op"
    );
    let (path, result) = committed(plan);
    assert!(path.exists(), "the empty document is created");
    assert!(result.document().exists());
    assert_eq!(fs::read(&path).unwrap(), b"");
}

const README_STAGE: &str = ".README.md.spielorders-stage";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteBoundary {
    WriteStage,
    SyncStage,
    Replace,
    IndexReplacement,
    RefreshListing,
    DamagePackageAfterWrite,
}

struct FailingWrite {
    boundary: WriteBoundary,
    fired: Cell<bool>,
    fail_cleanup: bool,
    replaced: Cell<bool>,
}

impl FailingWrite {
    fn at(boundary: WriteBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: false,
            replaced: Cell::new(false),
        }
    }

    fn with_cleanup_failure(boundary: WriteBoundary) -> Self {
        Self {
            fail_cleanup: true,
            ..Self::at(boundary)
        }
    }

    fn fail_once(&self, boundary: WriteBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: WriteBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingWrite {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        // Only the post-commit refresh load sees the damaged marker.
        if self.replaced.get()
            && self.boundary == WriteBoundary::DamagePackageAfterWrite
            && path.ends_with("version.json")
        {
            return Ok(b"{ not json".to_vec());
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(README_STAGE) && self.fail_once(WriteBoundary::WriteStage) {
            return Err(Self::error(WriteBoundary::WriteStage));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(README_STAGE) && self.fail_once(WriteBoundary::SyncStage) {
            return Err(Self::error(WriteBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with("README.md") {
            if self.fail_once(WriteBoundary::Replace) {
                return Err(Self::error(WriteBoundary::Replace));
            }
            self.replaced.set(true);
        }
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(WriteBoundary::IndexReplacement)
        {
            return Err(Self::error(WriteBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(README_STAGE) && self.fail_cleanup {
            return Err(io::Error::other("injected staging cleanup failure"));
        }
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        // Only the post-commit refresh load is failed, never the precommit checks.
        if self.replaced.get() && self.boundary == WriteBoundary::RefreshListing {
            return Err(Self::error(WriteBoundary::RefreshListing));
        }
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn injected_write_failures_preserve_the_prior_document_exactly() {
    for boundary in [
        WriteBoundary::WriteStage,
        WriteBoundary::SyncStage,
        WriteBoundary::Replace,
    ] {
        let tree = initialized_package("readme-write-failure");
        let root = tree.path();
        let directory = order_with_readme(root, AWKWARD.as_bytes());
        let order_readme = directory.join("README.md");
        let plan = plan_order_readme_write(&directory, "replacement prose\n", &RealFileSystem)
            .expect("the write can be planned");

        let report = commit_readme_write(plan, &FailingWrite::at(boundary));

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let ReadmeWriteRejection::ReplacementFailed { error } = &rejection else {
            panic!("an injected {boundary:?} failure is a replacement failure: {rejection:?}");
        };
        assert!(
            matches!(
                (boundary, error.failure()),
                (
                    WriteBoundary::WriteStage,
                    ValidatedReplacementFailure::StagingWrite(_)
                ) | (
                    WriteBoundary::SyncStage,
                    ValidatedReplacementFailure::Durability(_)
                ) | (
                    WriteBoundary::Replace,
                    ValidatedReplacementFailure::Replacement(_)
                )
            ),
            "an injected {boundary:?} failure reports its own phase: {:?}",
            error.failure()
        );
        assert!(error.cleanup_failure().is_none());
        assert_eq!(
            fs::read(&order_readme).unwrap(),
            AWKWARD.as_bytes(),
            "the prior document survives an injected {boundary:?} failure byte for byte"
        );
        assert!(!directory.join(README_STAGE).exists());
    }
}

#[test]
fn an_injected_failure_over_an_absent_document_preserves_its_absence() {
    let tree = initialized_package("readme-write-failure-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    fs::remove_file(&order_readme).unwrap();
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("an absent document can be planned");

    let report = commit_readme_write(plan, &FailingWrite::at(WriteBoundary::Replace));

    assert!(matches!(report, MutationReport::Rejected { .. }));
    assert!(
        !order_readme.exists(),
        "a failed creation leaves the document absent rather than half written"
    );
    assert!(!directory.join(README_STAGE).exists());
}

#[test]
fn a_staging_cleanup_failure_is_retained_beside_the_rejection() {
    let tree = initialized_package("readme-write-cleanup-failure");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let plan = plan_order_readme_write(&directory, "replacement prose\n", &RealFileSystem).unwrap();

    let report = commit_readme_write(
        plan,
        &FailingWrite::with_cleanup_failure(WriteBoundary::SyncStage),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a corrupted staged document rejects: {report:?}");
    };
    let ReadmeWriteRejection::ReplacementFailed { error } = &rejection else {
        panic!("a corrupted staged document is a replacement failure: {rejection:?}");
    };
    assert!(error.cleanup_failure().is_some());
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes()
    );
    assert!(directory.join(README_STAGE).exists());
    fs::remove_file(directory.join(README_STAGE)).unwrap();
}

#[test]
fn a_post_commit_index_failure_keeps_the_committed_order_readme() {
    for (boundary, refresh) in [
        (
            WriteBoundary::IndexReplacement,
            PackageIndexRefresh::ReplacementFailed,
        ),
        (
            WriteBoundary::RefreshListing,
            PackageIndexRefresh::NotAttempted,
        ),
    ] {
        let tree = initialized_package("readme-write-index-failure");
        let root = tree.path();
        let directory = order_with_readme(root, b"prior order notes\n");
        let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();

        let report = commit_readme_write(plan, &FailingWrite::at(boundary));

        let MutationReport::CommittedWithWarning {
            result, warnings, ..
        } = report
        else {
            panic!("a failed {boundary:?} refresh remains committed: {report:?}");
        };
        assert_eq!(result.document().text(), AWKWARD);
        assert_eq!(result.index().state(), PackageIndexState::Stale);
        assert_eq!(result.index().refresh(), refresh);
        // What went stale is the README text the index projects, not the Order's
        // canonical metadata, which this write never touched. A caller repairing the
        // index acts on the reason, so naming the wrong one misdirects the repair.
        // Both boundaries must agree: the replacement-failure path reads real
        // canonical evidence, so it is the detector's own verdict on this Package.
        assert_eq!(
            result.index().stale_reasons(),
            [PackageIndexStaleReason::OrderReadmeChanged],
            "the {boundary:?} boundary names what actually changed"
        );
        assert!(matches!(
            warnings.as_slice(),
            [ReadmeWriteWarning::IndexRefreshFailed(_)]
        ));
        // The document write stands, so retrying it is not what a caller should do.
        assert_eq!(
            fs::read(directory.join("README.md")).unwrap(),
            AWKWARD.as_bytes()
        );
    }
}

#[test]
fn a_changed_target_rejects_before_any_write_without_touching_siblings() {
    let tree = initialized_package("readme-write-target-race");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();

    // Another writer replaces the same document between planning and commit.
    fs::write(&order_readme, b"someone else's prose\n").unwrap();
    let package_before = package_readme(root);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let effects = RecordingFileSystem::default();
    let report = commit_readme_write(plan, &effects);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a changed target rejects: {report:?}");
    };
    let ReadmeWriteRejection::PreconditionsChanged { path } = &rejection else {
        panic!("a changed target is a precondition change: {rejection:?}");
    };
    assert!(path.ends_with("README.md"));
    assert_eq!(
        fs::read(&order_readme).unwrap(),
        b"someone else's prose\n",
        "the intervening write is not silently overwritten"
    );
    assert_eq!(effects.replaced(root), Vec::<String>::new());
    assert_eq!(package_readme(root), package_before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_target_that_became_absent_or_wrongly_typed_rejects() {
    // Planned against a present document, committed after it vanished.
    let tree = initialized_package("readme-write-target-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();
    fs::remove_file(directory.join("README.md")).unwrap();
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Rejected {
            rejection: ReadmeWriteRejection::PreconditionsChanged { .. },
            ..
        }
    ));
    assert!(!directory.join("README.md").exists());

    // Planned against an absent document, committed after one appeared.
    let tree = initialized_package("readme-write-target-appeared");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::remove_file(directory.join("README.md")).unwrap();
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();
    fs::write(directory.join("README.md"), b"appeared\n").unwrap();
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Rejected {
            rejection: ReadmeWriteRejection::PreconditionsChanged { .. },
            ..
        }
    ));
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        b"appeared\n"
    );
}

#[test]
fn a_wrongly_typed_destination_is_refused_without_conversion() {
    let tree = initialized_package("readme-write-wrong-kind");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::remove_file(directory.join("README.md")).unwrap();
    fs::create_dir(directory.join("README.md")).unwrap();
    fs::write(directory.join("README.md/inner.txt"), b"not a README\n").unwrap();

    let error = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect_err("a directory at the conventional path is not a writable document");

    let ReadmeWritePlanError::UnwritableDestination { reason, .. } = &error else {
        panic!("a wrongly typed destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::WrongKind);
    // Never converted, never removed.
    assert!(directory.join("README.md").is_dir());
    assert_eq!(
        fs::read(directory.join("README.md/inner.txt")).unwrap(),
        b"not a README\n"
    );
}

#[test]
fn a_damaged_scope_refuses_the_write_without_touching_the_document() {
    let tree = initialized_package("readme-write-damaged-order");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let order_readme = directory.join("README.md");
    fs::write(directory.join(".spielorders/order.json"), b"{ not json").unwrap();

    let error = plan_order_readme_write(&directory, "replacement\n", &RealFileSystem)
        .expect_err("a Damaged Order's README is not writable");
    assert!(
        matches!(error, ReadmeWritePlanError::DamagedOrder { .. }),
        "a damaged Order scope refuses: {error:?}"
    );
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());

    // A damaged Package refuses its own README the same way.
    let tree = initialized_package("readme-write-damaged-package");
    let root = tree.path();
    fs::write(root.join(".spielorders/version.json"), b"{ not json").unwrap();
    let error = plan_package_readme_write(root, "replacement\n", &RealFileSystem)
        .expect_err("a Damaged Package's README is not writable");
    assert!(
        matches!(error, ReadmeWritePlanError::DamagedPackage),
        "a damaged Package scope refuses: {error:?}"
    );
    assert_eq!(package_readme(root), b"");
}

/// Fails only the read of the destination document itself, so the plan sees a file
/// it cannot read rather than one that is missing or wrongly typed.
struct UnreadableDestination;

impl FileSystem for UnreadableDestination {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with("README.md") {
            return Err(io::Error::other("injected unreadable destination"));
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

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn an_unreadable_destination_is_refused_without_writing_over_it() {
    let tree = initialized_package("readme-write-unreadable-destination");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());

    let error = plan_order_readme_write(&directory, "replacement\n", &UnreadableDestination)
        .expect_err("a destination that cannot be read is not blindly replaced");

    let ReadmeWritePlanError::UnwritableDestination { reason, .. } = &error else {
        panic!("an unreadable destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::Unreadable);
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes(),
        "the document it could not read is left exactly as it was"
    );
}

#[cfg(unix)]
#[test]
fn a_link_at_the_conventional_path_is_refused_without_following_it() {
    let tree = initialized_package("readme-write-link-destination");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let target = directory.join("elsewhere.md");
    fs::write(&target, b"linked prose\n").unwrap();
    fs::remove_file(directory.join("README.md")).unwrap();
    std::os::unix::fs::symlink(&target, directory.join("README.md")).unwrap();

    let error = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect_err("a link at the conventional path is not a writable document");

    let ReadmeWritePlanError::UnwritableDestination { reason, .. } = &error else {
        panic!("a link destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::WrongKind);
    // The link is neither followed nor replaced, so its target is untouched.
    assert_eq!(fs::read(&target).unwrap(), b"linked prose\n");
    assert!(
        fs::symlink_metadata(directory.join("README.md"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn a_prior_document_that_is_not_valid_utf8_can_still_be_replaced() {
    let tree = initialized_package("readme-write-invalid-utf8-prior");
    let root = tree.path();
    let directory = order_with_readme(root, b"placeholder\n");
    // Externally authored bytes that are not valid UTF-8. Replacing a document is not
    // reading it as prose, so this is an ordinary write rather than a refusal.
    fs::write(directory.join("README.md"), [0xff, 0xfe, 0x00, 0x41]).unwrap();

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("a document holding invalid UTF-8 is still replaceable");

    assert!(plan.prior_exists());
    assert_eq!(
        plan.prior_text(),
        None,
        "prior text is absent because the prior bytes are not text"
    );
    assert!(plan.changes_document());
    let (path, _) = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
}

#[test]
fn a_package_readme_write_reports_an_index_it_never_assessed() {
    let tree = initialized_package("readme-write-package-index-record");
    let root = tree.path();

    let plan = plan_package_readme_write(root, "package prose\n", &RealFileSystem).unwrap();
    let (_, result) = committed(plan);

    // The Package README is not indexed, so this write neither refreshes nor
    // assesses the derived index, and says exactly that.
    assert_eq!(result.index().state(), PackageIndexState::NotAssessed);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
}

#[test]
fn a_package_that_becomes_unloadable_after_the_write_reports_an_unavailable_refresh() {
    let tree = initialized_package("readme-write-refresh-unavailable");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();

    // The document is replaced, and only then does the Package stop loading. The
    // canonical write stands and the report says the refresh could not be attempted.
    let report = commit_readme_write(
        plan,
        &FailingWrite::at(WriteBoundary::DamagePackageAfterWrite),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("an unloadable Package after the write remains committed: {report:?}");
    };
    assert_eq!(result.document().text(), AWKWARD);
    assert_eq!(warnings, [ReadmeWriteWarning::RefreshUnavailable]);
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    assert_eq!(
        result.index().stale_reasons(),
        [PackageIndexStaleReason::OrderReadmeChanged]
    );
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes()
    );
}

/// A README rewritten between this write's revalidation and its rename belongs to
/// whoever wrote it. The write refuses rather than replacing their text.
#[test]
fn a_readme_rewritten_after_revalidation_is_refused_rather_than_overwritten() {
    let tree = initialized_package("readme-write-concurrent-edit");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let destination = directory.join("README.md");
    const THEIRS: &[u8] = b"# written by someone else\n";

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem).unwrap();

    let report = commit_readme_write(
        plan,
        &ConcurrentWriter::replacing(&destination, THEIRS, EditPoint::StagingWrite),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a README rewritten underneath this write is refused: {report:?}");
    };
    // Refused because someone else changed it — distinct from a failed write.
    let ReadmeWriteRejection::ReplacementFailed { error } = rejection else {
        panic!("the refusal comes from the replacement, not a precondition: {rejection:?}");
    };
    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal names a changed destination rather than a write failure: {:?}",
        error.failure()
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        THEIRS,
        "the other writer's README survives"
    );
}

/// Observing nothing at the destination is an observation, so a creation is authorized
/// against it exactly as a rewrite is authorized against the bytes it read. A README
/// that appears in the window — an editor saving, an adoption, a restore from backup —
/// is user-authored text nobody asked this write to replace.
#[test]
fn a_readme_created_after_this_write_observed_none_is_refused_rather_than_destroyed() {
    let tree = initialized_package("readme-write-concurrent-creation");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let destination = directory.join("README.md");
    fs::remove_file(&destination).unwrap();
    const THEIRS: &[u8] = b"# restored from a backup\n";

    let plan = plan_order_readme_write(&directory, AWKWARD, &RealFileSystem)
        .expect("an absent Order README is creatable");
    assert!(!plan.prior_exists(), "this is the creation path");

    let filesystem = ConcurrentWriter::replacing(&destination, THEIRS, EditPoint::StagingWrite);
    let report = commit_readme_write(plan, &filesystem);

    assert!(filesystem.fired(), "the outside write really landed");
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a README created underneath this write is refused: {report:?}");
    };
    let ReadmeWriteRejection::ReplacementFailed { error } = rejection else {
        panic!("the refusal comes from the replacement, not a precondition: {rejection:?}");
    };
    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal names a changed destination rather than a write failure: {:?}",
        error.failure()
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        THEIRS,
        "the other writer's README survives"
    );
    assert!(
        !directory.join(README_STAGE).exists(),
        "the refused creation leaves no staging file of its own behind"
    );
}
