//! Decision sources: Define the SpielOrders package topology; Define the Rust CLI
//! capability contract; Define legacy Order adoption and metadata bootstrapping;
//! Define durable metadata schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::canonical::{PackageIndexState, PackageLoad, open_package};
use spielorders_core::initialization::{
    InitializationCleanupFailureCause, InitializationCleanupPhase,
    InitializationCommitFailureCause, InitializationCommitPhase, InitializationDestinationKind,
    InitializationOperationKind, InitializationOptions, InitializationPlanError,
    InitializationRejection, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::Config;
use spielorders_core::mutation::MutationReport;
use spielorders_core::paths::{ConfiguredPath, ConfiguredPathFailure};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeEvidence, FileChangeObservation, FileSystem,
    FileSystemEntryKind, NewDirectoryRenameOutcome, NewFileLinkOutcome,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedBoundary {
    CreateDirectory,
    WriteStage,
    WriteStageWithConcurrentEvidence,
    ReadStage,
    CorruptStage,
    SyncStage,
    InstallDestination,
    DestinationAppearsBeforeInstall,
    RemoveInstalledStage,
    WriteVersionStage,
    ObserveDestination,
    ObserveStageCleanup,
    ObserveCreatedFileCleanup,
    ObserveCreatedDirectoryCleanup,
    RemoveStage,
    RemoveCreatedFile,
    RemoveCreatedDirectory,
    ReplaceCreatedFileBeforeCleanup,
}

#[derive(Debug, Eq, PartialEq)]
enum InjectedError {
    Boundary(InjectedBoundary),
    Io(io::ErrorKind),
}

struct InjectedFileSystem {
    primary: InjectedBoundary,
    cleanup: Option<InjectedBoundary>,
    armed: Cell<bool>,
    primary_fired: Cell<bool>,
    cleanup_fired: Cell<bool>,
}

impl InjectedFileSystem {
    fn with_cleanup(primary: InjectedBoundary, cleanup: InjectedBoundary) -> Self {
        Self {
            primary,
            cleanup: Some(cleanup),
            armed: Cell::new(true),
            primary_fired: Cell::new(false),
            cleanup_fired: Cell::new(false),
        }
    }

    fn at(primary: InjectedBoundary) -> Self {
        Self {
            primary,
            cleanup: None,
            armed: Cell::new(true),
            primary_fired: Cell::new(false),
            cleanup_fired: Cell::new(false),
        }
    }

    fn precommit_observation_failure() -> Self {
        let filesystem = Self::at(InjectedBoundary::ObserveDestination);
        filesystem.armed.set(false);
        filesystem
    }

    fn arm(&self) {
        self.armed.set(true);
    }

    fn map(error: io::Error) -> InjectedError {
        InjectedError::Io(error.kind())
    }

    fn is_stage(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name.as_encoded_bytes().ends_with(b".spielorders-stage"))
    }

    fn fail_once(&self, boundary: InjectedBoundary) -> bool {
        self.armed.get() && self.primary == boundary && !self.primary_fired.replace(true)
    }

    fn fail_cleanup_once(&self, boundary: InjectedBoundary) -> bool {
        self.cleanup == Some(boundary) && !self.cleanup_fired.replace(true)
    }
}

impl FileSystem for InjectedFileSystem {
    type Error = InjectedError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if Self::is_stage(path) && self.fail_once(InjectedBoundary::ReadStage) {
            return Err(InjectedError::Boundary(InjectedBoundary::ReadStage));
        }
        if Self::is_stage(path) && self.fail_once(InjectedBoundary::CorruptStage) {
            return Ok(b"corrupted candidate".to_vec());
        }
        RealFileSystem.read(path).map_err(Self::map)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents).map_err(Self::map)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if Self::is_stage(path)
            && self.fail_once(InjectedBoundary::WriteStageWithConcurrentEvidence)
        {
            RealFileSystem
                .write_new(path, b"concurrent staging evidence")
                .map_err(Self::map)?;
            return Err(InjectedError::Boundary(
                InjectedBoundary::WriteStageWithConcurrentEvidence,
            ));
        }
        if Self::is_stage(path) && self.fail_once(InjectedBoundary::WriteStage) {
            return Err(InjectedError::Boundary(InjectedBoundary::WriteStage));
        }
        if path.ends_with(".version.json.spielorders-stage")
            && self.fail_once(InjectedBoundary::WriteVersionStage)
        {
            return Err(InjectedError::Boundary(InjectedBoundary::WriteVersionStage));
        }
        RealFileSystem.write_new(path, contents).map_err(Self::map)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if Self::is_stage(path) && self.fail_once(InjectedBoundary::SyncStage) {
            return Err(InjectedError::Boundary(InjectedBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path).map_err(Self::map)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem
            .replace_file(source, destination)
            .map_err(Self::map)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if Self::is_stage(path) && self.fail_once(InjectedBoundary::RemoveInstalledStage) {
            return Err(InjectedError::Boundary(
                InjectedBoundary::RemoveInstalledStage,
            ));
        }
        if Self::is_stage(path) && self.fail_cleanup_once(InjectedBoundary::RemoveStage) {
            return Err(InjectedError::Boundary(InjectedBoundary::RemoveStage));
        }
        if path.ends_with(".spielorders/config.toml")
            && self.fail_cleanup_once(InjectedBoundary::RemoveCreatedFile)
        {
            return Err(InjectedError::Boundary(InjectedBoundary::RemoveCreatedFile));
        }
        RealFileSystem.remove_file(path).map_err(Self::map)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path).map_err(Self::map)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        if path.ends_with(".spielorders") && self.fail_once(InjectedBoundary::ObserveDestination) {
            return Err(InjectedError::Boundary(
                InjectedBoundary::ObserveDestination,
            ));
        }
        RealFileSystem.symlink_metadata(path).map_err(Self::map)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        if path.ends_with(".spielorders/config.toml")
            && self.primary_fired.get()
            && self.fail_cleanup_once(InjectedBoundary::ObserveCreatedFileCleanup)
        {
            return Err(InjectedError::Boundary(
                InjectedBoundary::ObserveCreatedFileCleanup,
            ));
        }
        if path.ends_with("Orders")
            && self.primary_fired.get()
            && self.fail_cleanup_once(InjectedBoundary::ObserveCreatedDirectoryCleanup)
        {
            return Err(InjectedError::Boundary(
                InjectedBoundary::ObserveCreatedDirectoryCleanup,
            ));
        }
        if path.ends_with(".spielorders/config.toml")
            && self.primary_fired.get()
            && self.fail_cleanup_once(InjectedBoundary::ReplaceCreatedFileBeforeCleanup)
        {
            RealFileSystem
                .write(path, b"concurrent canonical bytes")
                .map_err(Self::map)?;
        }
        if Self::is_stage(path)
            && self.primary_fired.get()
            && self.fail_cleanup_once(InjectedBoundary::ObserveStageCleanup)
        {
            return Err(InjectedError::Boundary(
                InjectedBoundary::ObserveStageCleanup,
            ));
        }
        RealFileSystem.observe_file_change(path).map_err(Self::map)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path).map_err(Self::map)
    }
}

impl DirectoryFileSystem for InjectedFileSystem {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_once(InjectedBoundary::CreateDirectory) {
            return Err(InjectedError::Boundary(InjectedBoundary::CreateDirectory));
        }
        RealFileSystem.create_directory(path).map_err(Self::map)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_cleanup_once(InjectedBoundary::RemoveCreatedDirectory) {
            return Err(InjectedError::Boundary(
                InjectedBoundary::RemoveCreatedDirectory,
            ));
        }
        RealFileSystem.remove_directory(path).map_err(Self::map)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        if Self::is_stage(source)
            && self.fail_once(InjectedBoundary::DestinationAppearsBeforeInstall)
        {
            RealFileSystem
                .write_new(destination, b"concurrent destination evidence")
                .map_err(Self::map)?;
        }
        if Self::is_stage(source) && self.fail_once(InjectedBoundary::InstallDestination) {
            return Err(InjectedError::Boundary(
                InjectedBoundary::InstallDestination,
            ));
        }
        RealFileSystem
            .link_new_file(source, destination)
            .map_err(Self::map)
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        RealFileSystem
            .rename_new_directory(source, destination)
            .map_err(Self::map)
    }
}

/// Observes what an entry is but supplies no comparable change evidence, the position
/// of any platform whose timestamps are too coarse to tell one write from another.
struct UncertainEvidenceFileSystem(InjectedFileSystem);

impl FileSystem for UncertainEvidenceFileSystem {
    type Error = InjectedError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.0.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.0.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.0.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.0.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.0.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.0.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.0.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.0.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        Ok(FileChangeObservation::new(
            self.0.symlink_metadata(path)?,
            FileChangeEvidence::Uncertain,
        ))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.0.read_directory(path)
    }
}

impl DirectoryFileSystem for UncertainEvidenceFileSystem {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.0.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.0.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        self.0.link_new_file(source, destination)
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        self.0.rename_new_directory(source, destination)
    }
}

#[test]
fn default_dry_run_plan_is_complete_and_has_no_effects() {
    let tree = TemporaryTree::new("initialization-default-plan");
    let root = tree.path();
    fs::write(root.join("unrelated.txt"), b"unrelated bytes").unwrap();

    let before = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let plan =
        plan_package_initialization(root, &InitializationOptions::default(), &RealFileSystem)
            .expect("an unmanaged ordinary root can be planned");
    let after = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();

    assert_eq!(plan.metadata_version(), 1);
    assert_eq!(plan.orders_path(), "Orders");
    assert_eq!(plan.removed_path(), "Removed");
    assert_eq!(before, after, "planning must not create any artifact");
    assert_eq!(
        fs::read(root.join("unrelated.txt")).unwrap(),
        b"unrelated bytes"
    );

    let operations = plan
        .operations()
        .iter()
        .map(|operation| {
            (
                operation.kind(),
                operation.destination_kind(),
                operation
                    .path()
                    .strip_prefix(plan.root())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                operation.contents().map(ToOwned::to_owned),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        operations,
        [
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::Directory,
                ".spielorders".into(),
                None,
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::Directory,
                "Orders".into(),
                None,
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/config.toml".into(),
                Some(Vec::new()),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/version.json".into(),
                Some(b"{\n  \"metadata_version\": 1\n}\n".to_vec()),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/vendors.json".into(),
                Some(b"{\n  \"schema_version\": 1,\n  \"vendors\": []\n}\n".to_vec()),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/index.json".into(),
                Some(
                    b"{\n  \"schema_version\": 1,\n  \"orders\": [],\n  \"vendors\": []\n}\n"
                        .to_vec()
                ),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                "README.md".into(),
                Some(Vec::new()),
            ),
        ]
    );
}

#[test]
fn explicit_paths_and_existing_readme_are_preserved_in_the_complete_plan() {
    let tree = TemporaryTree::new("initialization-explicit-plan");
    let root = tree.path();
    fs::create_dir_all(root.join("Purchases/Active")).unwrap();
    fs::create_dir(root.join("Retired")).unwrap();
    fs::write(root.join("README.md"), b"exact user README\r\n\xff").unwrap();

    let plan = plan_package_initialization(
        root,
        &InitializationOptions::new("Purchases/Active", "Retired"),
        &RealFileSystem,
    )
    .expect("existing ordinary configured directories are reusable");

    let relative_operations = plan
        .operations()
        .iter()
        .map(|operation| {
            (
                operation.kind(),
                operation.destination_kind(),
                operation
                    .path()
                    .strip_prefix(plan.root())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(plan.root(), fs::canonicalize(root).unwrap());
    assert_eq!(plan.metadata_version(), 1);
    assert_eq!(plan.orders_path(), "Purchases/Active");
    assert_eq!(plan.removed_path(), "Retired");
    assert_eq!(
        relative_operations,
        vec![
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::Directory,
                ".spielorders".to_owned(),
            ),
            (
                InitializationOperationKind::Preserve,
                InitializationDestinationKind::Directory,
                "Purchases".to_owned(),
            ),
            (
                InitializationOperationKind::Preserve,
                InitializationDestinationKind::Directory,
                "Purchases/Active".to_owned(),
            ),
            (
                InitializationOperationKind::Preserve,
                InitializationDestinationKind::Directory,
                "Retired".to_owned(),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/config.toml".to_owned(),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/version.json".to_owned(),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/vendors.json".to_owned(),
            ),
            (
                InitializationOperationKind::Create,
                InitializationDestinationKind::File,
                ".spielorders/index.json".to_owned(),
            ),
            (
                InitializationOperationKind::Preserve,
                InitializationDestinationKind::File,
                "README.md".to_owned(),
            ),
        ]
    );
    let config = plan
        .operations()
        .iter()
        .find(|operation| operation.path().ends_with("config.toml"))
        .and_then(|operation| operation.contents())
        .expect("the plan exposes exact candidate configuration bytes");
    assert_eq!(
        config,
        b"orders_path = \"Purchases/Active\"\nremoved_path = \"Retired\"\n"
    );
    assert_eq!(
        fs::read(root.join("README.md")).unwrap(),
        b"exact user README\r\n\xff"
    );
}

#[test]
fn successful_commit_creates_a_healthy_current_package_and_preserves_other_content() {
    let tree = TemporaryTree::new("initialization-commit");
    let root = tree.path();
    fs::create_dir(root.join("ordinary-directory")).unwrap();
    fs::write(
        root.join("ordinary-directory/sentinel.bin"),
        b"\0opaque\xff",
    )
    .unwrap();
    fs::write(root.join("notes.txt"), b"existing notes\n").unwrap();
    let plan =
        plan_package_initialization(root, &InitializationOptions::default(), &RealFileSystem)
            .expect("the Package plan is valid");

    let report = commit_package_initialization(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid complete plan commits")
    };
    assert_eq!(result.root(), fs::canonicalize(root).unwrap());
    assert_eq!(fs::read(root.join("README.md")).unwrap(), b"");
    assert_eq!(
        fs::read(root.join("ordinary-directory/sentinel.bin")).unwrap(),
        b"\0opaque\xff"
    );
    assert_eq!(
        fs::read(root.join("notes.txt")).unwrap(),
        b"existing notes\n"
    );

    let PackageLoad::Available(package) = open_package(root, &RealFileSystem) else {
        panic!("the existing M2 loader accepts initialized canonical state")
    };
    assert_eq!(package.metadata_version(), 1);
    assert_eq!(package.index().state(), PackageIndexState::Current);
    assert!(package.healthy_orders().is_empty());
    assert!(package.vendors().is_empty());
}

#[test]
fn a_configured_path_collision_after_planning_rejects_before_the_first_write() {
    let tree = TemporaryTree::new("initialization-stale-plan");
    let root = tree.path();
    let plan =
        plan_package_initialization(root, &InitializationOptions::default(), &RealFileSystem)
            .expect("the initial dry-run plan is valid");
    fs::write(root.join("Removed"), b"late collision").unwrap();

    let report = commit_package_initialization(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a changed complete-plan precondition must reject")
    };
    let InitializationRejection::PreconditionsChanged { path } = rejection else {
        panic!("the rejection identifies a stale precondition")
    };
    assert_eq!(path, fs::canonicalize(root).unwrap().join("Removed"));
    assert!(!root.join(".spielorders").exists());
    assert!(!root.join("Orders").exists());
    assert!(!root.join("README.md").exists());
    assert_eq!(fs::read(root.join("Removed")).unwrap(), b"late collision");
}

#[test]
fn every_planned_configuration_candidate_round_trips_through_the_metadata_parser() {
    let tree = TemporaryTree::new("initialization-config-candidate");
    let options = InitializationOptions::new("Orders\u{1b}", "Removed\"");

    let plan = plan_package_initialization(tree.path(), &options, &RealFileSystem)
        .expect("the configured paths satisfy the closed relative-path grammar");
    let config = plan
        .operations()
        .iter()
        .find(|operation| operation.path().ends_with("config.toml"))
        .and_then(|operation| operation.contents())
        .expect("the plan includes candidate configuration bytes");
    let parsed = Config::from_toml_bytes(config)
        .expect("all candidate bytes are valid before the first write");

    assert_eq!(parsed.orders_path, "Orders\u{1b}");
    assert_eq!(parsed.removed_path, "Removed\"");
    assert!(!tree.path().join(".spielorders").exists());
}

#[test]
fn incomplete_staging_cleanup_is_explicit_in_the_rejection() {
    let tree = TemporaryTree::new("initialization-incomplete-cleanup");
    let filesystem = InjectedFileSystem::with_cleanup(
        InjectedBoundary::ReadStage,
        InjectedBoundary::ObserveStageCleanup,
    );
    let plan =
        plan_package_initialization(tree.path(), &InitializationOptions::default(), &filesystem)
            .expect("the fault is armed only for commit staging");

    let report = commit_package_initialization(plan, &filesystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("the injected read failure rejects initialization")
    };
    let InitializationRejection::CommitFailed {
        failure,
        cleanup_failures,
    } = rejection
    else {
        panic!("the rejection retains commit and rollback evidence")
    };
    assert_eq!(failure.phase(), InitializationCommitPhase::ReadStage);
    assert_eq!(cleanup_failures.len(), 2);
    assert_eq!(
        cleanup_failures[0].phase(),
        InitializationCleanupPhase::ObserveStage
    );
    assert_eq!(
        cleanup_failures[1].phase(),
        InitializationCleanupPhase::RemoveCreatedDirectory
    );
    assert!(
        tree.path()
            .join(".spielorders/.config.toml.spielorders-stage")
            .exists(),
        "unremoved staging evidence remains available for explicit recovery"
    );
}

#[test]
fn precommit_observation_failure_is_not_reported_as_an_ordinary_state_change() {
    let tree = TemporaryTree::new("initialization-precommit-observation");
    let filesystem = InjectedFileSystem::precommit_observation_failure();
    let plan =
        plan_package_initialization(tree.path(), &InitializationOptions::default(), &filesystem)
            .expect("planning succeeds before the observation fault is armed");
    filesystem.arm();

    let report = commit_package_initialization(plan, &filesystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("the injected observation failure rejects initialization")
    };
    let InitializationRejection::CommitFailed {
        failure,
        cleanup_failures,
    } = rejection
    else {
        panic!("an effect failure remains distinct from a changed precondition")
    };
    assert_eq!(
        failure.phase(),
        InitializationCommitPhase::ObserveDestination
    );
    assert!(cleanup_failures.is_empty());
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("Orders").exists());
}

#[test]
fn unsafe_roots_paths_collisions_and_links_reject_without_creating_artifacts() {
    let managed = TemporaryTree::new("initialization-reject-managed");
    fs::write(managed.path().join(".spielorders"), b"damaged evidence").unwrap();
    assert!(matches!(
        plan_package_initialization(
            managed.path(),
            &InitializationOptions::default(),
            &RealFileSystem,
        ),
        Err(InitializationPlanError::AlreadyManaged)
    ));
    assert_eq!(
        fs::read(managed.path().join(".spielorders")).unwrap(),
        b"damaged evidence"
    );

    let file_root = TemporaryTree::new("initialization-reject-file-root");
    let ordinary_file = file_root.path().join("not-a-directory");
    fs::write(&ordinary_file, b"ordinary file").unwrap();
    assert!(matches!(
        plan_package_initialization(
            &ordinary_file,
            &InitializationOptions::default(),
            &RealFileSystem,
        ),
        Err(InitializationPlanError::RootNotOrdinaryDirectory)
    ));
    assert_eq!(fs::read(&ordinary_file).unwrap(), b"ordinary file");

    let invalid = TemporaryTree::new("initialization-reject-invalid");
    for (options, refused, expected_failure) in [
        (
            InitializationOptions::new(".", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::CurrentDirectory,
        ),
        (
            InitializationOptions::new("../Orders", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ParentTraversal,
        ),
        (
            InitializationOptions::new(".spielorders/Orders", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ReservedMetadata,
        ),
        (
            InitializationOptions::new("/outside", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::Absolute,
        ),
        (
            InitializationOptions::new("Orders//Active", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::EmptyComponent,
        ),
        (
            InitializationOptions::new("Orders\\Active", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::PlatformSeparator,
        ),
        (
            InitializationOptions::new("Orders", "Removed/Nested"),
            ConfiguredPath::Removed,
            ConfiguredPathFailure::NotDirectChild,
        ),
    ] {
        let error = plan_package_initialization(invalid.path(), &options, &RealFileSystem)
            .expect_err("an invalid configured path is refused");
        let InitializationPlanError::InvalidConfiguredPath { path, failure } = error else {
            panic!("the refusal names the configured path at fault, not {error:?}")
        };
        assert_eq!(path, refused);
        assert_eq!(failure, expected_failure);
        assert_eq!(fs::read_dir(invalid.path()).unwrap().count(), 0);
    }

    let overlapping = TemporaryTree::new("initialization-reject-overlap");
    assert!(matches!(
        plan_package_initialization(
            overlapping.path(),
            &InitializationOptions::new("Removed/Active", "Removed"),
            &RealFileSystem,
        ),
        Err(InitializationPlanError::ManagedPathsOverlap)
    ));
    assert_eq!(fs::read_dir(overlapping.path()).unwrap().count(), 0);

    let collisions = TemporaryTree::new("initialization-reject-collision");
    fs::write(collisions.path().join("Orders"), b"ordinary file").unwrap();
    fs::create_dir(collisions.path().join("README.md")).unwrap();
    assert!(matches!(
        plan_package_initialization(
            collisions.path(),
            &InitializationOptions::default(),
            &RealFileSystem,
        ),
        Err(InitializationPlanError::DestinationCollision { .. })
    ));
    assert_eq!(
        fs::read(collisions.path().join("Orders")).unwrap(),
        b"ordinary file"
    );
    assert!(collisions.path().join("README.md").is_dir());
    assert!(!collisions.path().join(".spielorders").exists());

    #[cfg(unix)]
    {
        let linked = TemporaryTree::new("initialization-reject-link");
        let outside = TemporaryTree::new("initialization-link-target");
        std::os::unix::fs::symlink(outside.path(), linked.path().join("Orders")).unwrap();
        assert!(matches!(
            plan_package_initialization(
                linked.path(),
                &InitializationOptions::default(),
                &RealFileSystem,
            ),
            Err(InitializationPlanError::DisallowedLink { .. })
        ));
        assert!(!linked.path().join(".spielorders").exists());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[test]
fn configured_readme_and_staging_collisions_reject_during_planning() {
    for options in [
        InitializationOptions::new("README.md", "Removed"),
        InitializationOptions::new("README.md/Active", "Removed"),
        InitializationOptions::new("Orders", "README.md"),
        InitializationOptions::new(".README.md.spielorders-stage", "Removed"),
        InitializationOptions::new("Orders", ".README.md.spielorders-stage"),
        InitializationOptions::new("readme.md", "Removed"),
        InitializationOptions::new("Orders", "ReadMe.Md"),
        InitializationOptions::new(".readme.md.spielorders-stage", "Removed"),
    ] {
        let tree = TemporaryTree::new("initialization-readme-role-collision");
        assert!(matches!(
            plan_package_initialization(tree.path(), &options, &RealFileSystem),
            Err(InitializationPlanError::PlannedPathCollision { .. })
        ));
        assert_eq!(fs::read_dir(tree.path()).unwrap().count(), 0);
    }

    let tree = TemporaryTree::new("initialization-existing-readme-stage");
    let stage = tree.path().join(".README.md.spielorders-stage");
    fs::write(&stage, b"abandoned evidence").unwrap();
    assert!(matches!(
        plan_package_initialization(
            tree.path(),
            &InitializationOptions::default(),
            &RealFileSystem,
        ),
        Err(InitializationPlanError::DestinationCollision { .. })
    ));
    assert_eq!(fs::read(stage).unwrap(), b"abandoned evidence");
    assert!(!tree.path().join(".spielorders").exists());
}

#[test]
fn a_successful_commit_leaves_an_existing_readme_byte_identical() {
    let tree = TemporaryTree::new("initialization-commit-existing-readme");
    let root = tree.path();
    let existing = b"# Lab Orders\r\n\xffnot valid UTF-8\n";
    fs::write(root.join("README.md"), existing).unwrap();

    let plan =
        plan_package_initialization(root, &InitializationOptions::default(), &RealFileSystem)
            .expect("an unmanaged root holding a README can be planned");
    let readme = plan
        .operations()
        .iter()
        .find(|operation| operation.path().ends_with("README.md"))
        .expect("the plan accounts for the conventional Package README");
    assert_eq!(readme.kind(), InitializationOperationKind::Preserve);
    assert_eq!(
        readme.destination_kind(),
        InitializationDestinationKind::File
    );
    assert_eq!(readme.contents(), None);

    let report = commit_package_initialization(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid complete plan commits")
    };
    assert_eq!(result.root(), fs::canonicalize(root).unwrap());
    assert_eq!(fs::read(root.join("README.md")).unwrap(), existing);

    let PackageLoad::Available(package) = open_package(root, &RealFileSystem) else {
        panic!("the existing M2 loader accepts the initialized Package")
    };
    assert_eq!(package.metadata_version(), 1);
    assert_eq!(package.index().state(), PackageIndexState::Current);
}

#[test]
fn rollback_reports_rather_than_removes_artifacts_whose_creation_it_cannot_prove() {
    let tree = TemporaryTree::new("initialization-uncertain-evidence");
    let root = fs::canonicalize(tree.path()).unwrap();
    let filesystem =
        UncertainEvidenceFileSystem(InjectedFileSystem::at(InjectedBoundary::WriteVersionStage));

    let plan =
        plan_package_initialization(tree.path(), &InitializationOptions::default(), &filesystem)
            .expect("the fault is armed only for commit effects");

    let report = commit_package_initialization(plan, &filesystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a failed creation rejects")
    };
    let InitializationRejection::CommitFailed {
        failure,
        cleanup_failures,
    } = rejection
    else {
        panic!("a failed stage write is an effect failure")
    };
    assert_eq!(failure.phase(), InitializationCommitPhase::WriteStage);
    assert_eq!(
        failure.path(),
        root.join(".spielorders/.version.json.spielorders-stage")
            .as_path()
    );

    // Each artifact created so far is named, at its phase, as one this operation
    // declined to remove — rather than deleted on an ownership claim it cannot support.
    let reported = cleanup_failures
        .iter()
        .map(|cleanup| {
            assert_eq!(
                cleanup.cause(),
                &InitializationCleanupFailureCause::OwnershipUncertain
            );
            (cleanup.phase(), cleanup.path().to_path_buf())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reported,
        vec![
            (
                InitializationCleanupPhase::RemoveCreatedFile,
                root.join(".spielorders/config.toml"),
            ),
            (
                InitializationCleanupPhase::RemoveCreatedDirectory,
                root.join("Orders"),
            ),
            (
                InitializationCleanupPhase::RemoveCreatedDirectory,
                root.join(".spielorders"),
            ),
        ]
    );
    assert!(root.join(".spielorders/config.toml").is_file());
    assert!(root.join("Orders").is_dir());
    assert!(!root.join("README.md").exists());
}

#[test]
fn configured_paths_naming_the_metadata_directory_are_refused_before_any_write() {
    for (options, refused) in [
        (
            InitializationOptions::new(".spielorders", "Removed"),
            ConfiguredPath::Orders,
        ),
        (
            InitializationOptions::new(".SpielOrders", "Removed"),
            ConfiguredPath::Orders,
        ),
        (
            InitializationOptions::new(".SPIELORDERS", "Removed"),
            ConfiguredPath::Orders,
        ),
        (
            InitializationOptions::new(".SpielOrders/Active", "Removed"),
            ConfiguredPath::Orders,
        ),
        (
            InitializationOptions::new("Orders", ".spielorders"),
            ConfiguredPath::Removed,
        ),
        (
            InitializationOptions::new("Orders", ".SpielOrders"),
            ConfiguredPath::Removed,
        ),
        (
            InitializationOptions::new("Orders", ".SPIELORDERS"),
            ConfiguredPath::Removed,
        ),
    ] {
        let tree = TemporaryTree::new("initialization-reserved-metadata-name");
        let error = plan_package_initialization(tree.path(), &options, &RealFileSystem)
            .expect_err("a configured path naming the metadata directory is refused");

        let InitializationPlanError::InvalidConfiguredPath { path, failure } = error else {
            panic!("the refusal names the reserved configured path, not {error:?}")
        };
        assert_eq!(path, refused);
        assert_eq!(failure, ConfiguredPathFailure::ReservedMetadata);
        assert_eq!(fs::read_dir(tree.path()).unwrap().count(), 0);
    }
}

#[test]
fn every_creation_and_installation_phase_failure_rolls_back_cleanly() {
    // The first planned file is the configuration, so every staging fault lands on its
    // stage and the installation fault lands on the configuration itself.
    let first_stage = ".spielorders/.config.toml.spielorders-stage";
    let cases = [
        (
            InjectedBoundary::CreateDirectory,
            InitializationCommitPhase::CreateDirectory,
            ".spielorders",
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::CreateDirectory,
            )),
        ),
        (
            InjectedBoundary::WriteStage,
            InitializationCommitPhase::WriteStage,
            first_stage,
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::WriteStage,
            )),
        ),
        (
            InjectedBoundary::ReadStage,
            InitializationCommitPhase::ReadStage,
            first_stage,
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::ReadStage,
            )),
        ),
        (
            InjectedBoundary::CorruptStage,
            InitializationCommitPhase::ValidateCandidate,
            first_stage,
            InitializationCommitFailureCause::CandidateChanged,
        ),
        (
            InjectedBoundary::SyncStage,
            InitializationCommitPhase::SyncStage,
            first_stage,
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::SyncStage,
            )),
        ),
        (
            InjectedBoundary::InstallDestination,
            InitializationCommitPhase::InstallDestination,
            ".spielorders/config.toml",
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::InstallDestination,
            )),
        ),
        (
            InjectedBoundary::RemoveInstalledStage,
            InitializationCommitPhase::RemoveInstalledStage,
            first_stage,
            InitializationCommitFailureCause::FileSystem(InjectedError::Boundary(
                InjectedBoundary::RemoveInstalledStage,
            )),
        ),
    ];

    for (boundary, expected_phase, expected_relative_path, expected_cause) in cases {
        let tree = TemporaryTree::new("initialization-phase-rollback");
        fs::write(tree.path().join("untouched.bin"), b"untouched").unwrap();
        let root = fs::canonicalize(tree.path()).unwrap();
        let filesystem = InjectedFileSystem::at(boundary);
        let plan = plan_package_initialization(
            tree.path(),
            &InitializationOptions::default(),
            &filesystem,
        )
        .expect("the fault is armed only for commit effects");

        let report = commit_package_initialization(plan, &filesystem);

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("{boundary:?} must reject")
        };
        let InitializationRejection::CommitFailed {
            failure,
            cleanup_failures,
        } = rejection
        else {
            panic!("{boundary:?} is an effect failure")
        };
        assert_eq!(failure.phase(), expected_phase, "{boundary:?}");
        assert_eq!(
            failure.path(),
            root.join(expected_relative_path).as_path(),
            "{boundary:?}"
        );
        assert_eq!(failure.cause(), &expected_cause, "{boundary:?}");
        assert!(
            cleanup_failures.is_empty(),
            "{boundary:?}: {cleanup_failures:?}"
        );
        assert_eq!(
            fs::read(tree.path().join("untouched.bin")).unwrap(),
            b"untouched"
        );
        assert!(!tree.path().join(".spielorders").exists(), "{boundary:?}");
        assert!(!tree.path().join("Orders").exists(), "{boundary:?}");
        assert!(!tree.path().join("README.md").exists(), "{boundary:?}");
    }
}

#[test]
fn failure_after_one_canonical_creation_restores_the_absent_state_and_existing_readme() {
    let tree = TemporaryTree::new("initialization-partial-rollback");
    fs::write(tree.path().join("README.md"), b"user README\r\n\xff").unwrap();
    let filesystem = InjectedFileSystem::at(InjectedBoundary::WriteVersionStage);
    let plan =
        plan_package_initialization(tree.path(), &InitializationOptions::default(), &filesystem)
            .expect("the Package plan preserves the existing README");

    let report = commit_package_initialization(plan, &filesystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("the later staged-file failure rejects initialization")
    };
    let InitializationRejection::CommitFailed {
        failure,
        cleanup_failures,
    } = rejection
    else {
        panic!("the injected failure is operational")
    };
    assert_eq!(failure.phase(), InitializationCommitPhase::WriteStage);
    assert!(cleanup_failures.is_empty());
    assert_eq!(
        fs::read(tree.path().join("README.md")).unwrap(),
        b"user README\r\n\xff"
    );
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("Orders").exists());
}

#[test]
fn every_cleanup_removal_failure_is_reported_at_its_exact_phase() {
    let cases = [
        (
            InjectedBoundary::WriteVersionStage,
            InjectedBoundary::ObserveCreatedFileCleanup,
            InitializationCleanupPhase::ObserveCreatedFile,
        ),
        (
            InjectedBoundary::WriteStage,
            InjectedBoundary::ObserveCreatedDirectoryCleanup,
            InitializationCleanupPhase::ObserveCreatedDirectory,
        ),
        (
            InjectedBoundary::ReadStage,
            InjectedBoundary::RemoveStage,
            InitializationCleanupPhase::RemoveStage,
        ),
        (
            InjectedBoundary::WriteVersionStage,
            InjectedBoundary::RemoveCreatedFile,
            InitializationCleanupPhase::RemoveCreatedFile,
        ),
        (
            InjectedBoundary::WriteStage,
            InjectedBoundary::RemoveCreatedDirectory,
            InitializationCleanupPhase::RemoveCreatedDirectory,
        ),
    ];

    for (primary, cleanup, expected_phase) in cases {
        let tree = TemporaryTree::new("initialization-cleanup-phase");
        let filesystem = InjectedFileSystem::with_cleanup(primary, cleanup);
        let plan = plan_package_initialization(
            tree.path(),
            &InitializationOptions::default(),
            &filesystem,
        )
        .expect("the cleanup fault is armed only after commit begins");

        let MutationReport::Rejected { rejection, .. } =
            commit_package_initialization(plan, &filesystem)
        else {
            panic!("{cleanup:?} follows an injected commit rejection")
        };
        let InitializationRejection::CommitFailed {
            cleanup_failures, ..
        } = rejection
        else {
            panic!("{cleanup:?} remains operational evidence")
        };
        assert!(
            cleanup_failures
                .iter()
                .any(|failure| failure.phase() == expected_phase),
            "{cleanup:?}: {cleanup_failures:?}"
        );
    }
}

#[test]
fn rolling_back_a_nested_orders_directory_leaves_no_created_parent_behind() {
    let tree = TemporaryTree::new("initialization-nested-directory-rollback");
    let filesystem = InjectedFileSystem::at(InjectedBoundary::WriteStage);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::new("Purchases/Active", "Removed"),
        &filesystem,
    )
    .expect("the nested Orders plan is valid");

    let MutationReport::Rejected { rejection, .. } =
        commit_package_initialization(plan, &filesystem)
    else {
        panic!("the injected staging failure rejects")
    };
    let InitializationRejection::CommitFailed {
        cleanup_failures, ..
    } = rejection
    else {
        panic!("the injected failure is operational")
    };
    assert!(cleanup_failures.is_empty(), "{cleanup_failures:?}");
    assert!(!tree.path().join("Purchases").exists());
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("README.md").exists());
}

#[test]
fn rollback_never_deletes_unowned_staging_or_replaced_canonical_bytes() {
    let stage_tree = TemporaryTree::new("initialization-concurrent-stage");
    let stage_filesystem =
        InjectedFileSystem::at(InjectedBoundary::WriteStageWithConcurrentEvidence);
    let stage_plan = plan_package_initialization(
        stage_tree.path(),
        &InitializationOptions::default(),
        &stage_filesystem,
    )
    .unwrap();
    let MutationReport::Rejected {
        rejection:
            InitializationRejection::CommitFailed {
                cleanup_failures, ..
            },
        ..
    } = commit_package_initialization(stage_plan, &stage_filesystem)
    else {
        panic!("the injected staging collision rejects")
    };
    let stage = stage_tree
        .path()
        .join(".spielorders/.config.toml.spielorders-stage");
    assert_eq!(fs::read(&stage).unwrap(), b"concurrent staging evidence");
    assert!(cleanup_failures.iter().any(|failure| {
        failure.phase() == InitializationCleanupPhase::RemoveStage
            && failure.cause() == &InitializationCleanupFailureCause::OwnershipUncertain
    }));

    let canonical_tree = TemporaryTree::new("initialization-concurrent-canonical");
    let canonical_filesystem = InjectedFileSystem::with_cleanup(
        InjectedBoundary::WriteVersionStage,
        InjectedBoundary::ReplaceCreatedFileBeforeCleanup,
    );
    let canonical_plan = plan_package_initialization(
        canonical_tree.path(),
        &InitializationOptions::default(),
        &canonical_filesystem,
    )
    .unwrap();
    let MutationReport::Rejected {
        rejection:
            InitializationRejection::CommitFailed {
                cleanup_failures, ..
            },
        ..
    } = commit_package_initialization(canonical_plan, &canonical_filesystem)
    else {
        panic!("the later injected staging failure rejects")
    };
    let config = canonical_tree.path().join(".spielorders/config.toml");
    assert_eq!(fs::read(&config).unwrap(), b"concurrent canonical bytes");
    assert!(cleanup_failures.iter().any(|failure| {
        failure.phase() == InitializationCleanupPhase::RemoveCreatedFile
            && failure.cause() == &InitializationCleanupFailureCause::OwnershipUncertain
    }));
}

#[test]
fn concurrent_destination_appearance_is_never_overwritten_or_owned_by_rollback() {
    let tree = TemporaryTree::new("initialization-concurrent-destination");
    let filesystem = InjectedFileSystem::at(InjectedBoundary::DestinationAppearsBeforeInstall);
    let plan =
        plan_package_initialization(tree.path(), &InitializationOptions::default(), &filesystem)
            .expect("the destination appears only after the commit precondition check");

    let MutationReport::Rejected { rejection, .. } =
        commit_package_initialization(plan, &filesystem)
    else {
        panic!("an atomically detected destination collision rejects initialization")
    };
    let InitializationRejection::CommitFailed {
        failure,
        cleanup_failures,
    } = rejection
    else {
        panic!("the late collision retains exact commit evidence")
    };
    assert_eq!(
        failure.phase(),
        InitializationCommitPhase::InstallDestination
    );
    assert!(matches!(
        failure.cause(),
        spielorders_core::initialization::InitializationCommitFailureCause::DestinationChanged
    ));
    assert!(cleanup_failures.iter().any(|cleanup| {
        cleanup.phase() == InitializationCleanupPhase::RemoveCreatedDirectory
            && cleanup.path().ends_with(".spielorders")
    }));
    assert_eq!(
        fs::read(tree.path().join(".spielorders/config.toml")).unwrap(),
        b"concurrent destination evidence"
    );
    assert!(!tree.path().join(".spielorders/version.json").exists());
}
