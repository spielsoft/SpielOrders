//! Planning and persistence for initializing one unmanaged SpielOrders Package.

use crate::{
    filesystem::staging_path,
    metadata::{IndexDocument, VendorsDocument, Version1, VersionDocument},
    mutation::MutationReport,
    paths::{
        ConfiguredPath, ConfiguredPathFailure, ManagedDirectory, paths_overlap,
        resolve_managed_directory, resolved_paths_overlap, validate_configured_path,
    },
    ports::{
        DirectoryFileSystem, FileChangeEvidence, FileChangeObservation, FileSystem,
        FileSystemEntryKind, NewFileLinkOutcome,
    },
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationOptions {
    orders_path: String,
    removed_path: String,
}

impl InitializationOptions {
    pub fn new(orders_path: impl Into<String>, removed_path: impl Into<String>) -> Self {
        Self {
            orders_path: orders_path.into(),
            removed_path: removed_path.into(),
        }
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::new("Orders", "Removed")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationOperationKind {
    Create,
    Preserve,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationDestinationKind {
    Directory,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationOperation {
    kind: InitializationOperationKind,
    destination_kind: InitializationDestinationKind,
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

impl InitializationOperation {
    pub fn kind(&self) -> InitializationOperationKind {
        self.kind
    }

    pub fn destination_kind(&self) -> InitializationDestinationKind {
        self.destination_kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn contents(&self) -> Option<&[u8]> {
        self.contents.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInitializationPlan {
    root: PathBuf,
    options: InitializationOptions,
    orders_directory: PathBuf,
    removed_directory: PathBuf,
    operations: Vec<InitializationOperation>,
    preconditions: Vec<InitializationPrecondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InitializationPrecondition {
    path: PathBuf,
    expected: Option<FileSystemEntryKind>,
}

impl PackageInitializationPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn metadata_version(&self) -> u64 {
        1
    }

    pub fn orders_path(&self) -> &str {
        &self.options.orders_path
    }

    pub fn removed_path(&self) -> &str {
        &self.options.removed_path
    }

    pub fn operations(&self) -> &[InitializationOperation] {
        &self.operations
    }
}

#[derive(Debug, PartialEq)]
pub enum InitializationPlanError<E> {
    FileSystem {
        path: PathBuf,
        error: E,
    },
    RootNotOrdinaryDirectory,
    AlreadyManaged,
    InvalidConfiguredPath {
        path: ConfiguredPath,
        failure: ConfiguredPathFailure,
    },
    ManagedPathsOverlap,
    PlannedPathCollision {
        path: PathBuf,
    },
    DestinationCollision {
        path: PathBuf,
        actual: FileSystemEntryKind,
    },
    DisallowedLink {
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationCommitPhase {
    ObserveDestination,
    CreateDirectory,
    /// After the Orders Directory exists and before anything durable is written, the
    /// one moment the filesystem can say whether the two configured managed paths
    /// name one directory.
    VerifyManagedPaths,
    WriteStage,
    ReadStage,
    ValidateCandidate,
    SyncStage,
    InstallDestination,
    RemoveInstalledStage,
}

#[derive(Debug, PartialEq)]
pub enum InitializationCommitFailureCause<E> {
    FileSystem(E),
    CandidateChanged,
    DestinationChanged,
    /// The two configured managed paths resolve to one directory now that the Orders
    /// Directory exists. Planning could not see this: neither path existed then, and
    /// no filesystem answers questions about names that are not there yet.
    ManagedPathsOverlap,
}

#[derive(Debug, PartialEq)]
pub struct InitializationCommitFailure<E> {
    phase: InitializationCommitPhase,
    path: PathBuf,
    cause: InitializationCommitFailureCause<E>,
}

impl<E> InitializationCommitFailure<E> {
    pub fn phase(&self) -> InitializationCommitPhase {
        self.phase
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cause(&self) -> &InitializationCommitFailureCause<E> {
        &self.cause
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationCleanupPhase {
    ObserveStage,
    ObserveCreatedFile,
    ObserveCreatedDirectory,
    RemoveStage,
    RemoveCreatedFile,
    RemoveCreatedDirectory,
}

#[derive(Debug, PartialEq)]
pub enum InitializationCleanupFailureCause<E> {
    FileSystem(E),
    OwnershipUncertain,
}

#[derive(Debug, PartialEq)]
pub struct InitializationCleanupFailure<E> {
    phase: InitializationCleanupPhase,
    path: PathBuf,
    cause: InitializationCleanupFailureCause<E>,
}

impl<E> InitializationCleanupFailure<E> {
    pub fn phase(&self) -> InitializationCleanupPhase {
        self.phase
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cause(&self) -> &InitializationCleanupFailureCause<E> {
        &self.cause
    }
}

#[derive(Debug, PartialEq)]
pub enum InitializationRejection<E> {
    PreconditionsChanged {
        path: PathBuf,
    },
    CommitFailed {
        failure: InitializationCommitFailure<E>,
        cleanup_failures: Vec<InitializationCleanupFailure<E>>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializedPackage {
    root: PathBuf,
}

impl InitializedPackage {
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitializationWarning {}

pub type PackageInitializationReport<E> = MutationReport<
    PackageInitializationPlan,
    InitializedPackage,
    InitializationRejection<E>,
    InitializationWarning,
>;

pub fn plan_package_initialization<F: FileSystem>(
    selected_root: &Path,
    options: &InitializationOptions,
    filesystem: &F,
) -> Result<PackageInitializationPlan, InitializationPlanError<F::Error>> {
    let selected_kind = observe(selected_root, filesystem)?;
    if selected_kind != Some(FileSystemEntryKind::Directory) {
        return Err(InitializationPlanError::RootNotOrdinaryDirectory);
    }
    let root = filesystem.canonicalize(selected_root).map_err(|error| {
        InitializationPlanError::FileSystem {
            path: selected_root.to_path_buf(),
            error,
        }
    })?;
    if observe(&root.join(".spielorders"), filesystem)?.is_some() {
        return Err(InitializationPlanError::AlreadyManaged);
    }

    let orders_components =
        validate_configured_path(&options.orders_path, false).map_err(|failure| {
            InitializationPlanError::InvalidConfiguredPath {
                path: ConfiguredPath::Orders,
                failure,
            }
        })?;
    let removed_components =
        validate_configured_path(&options.removed_path, true).map_err(|failure| {
            InitializationPlanError::InvalidConfiguredPath {
                path: ConfiguredPath::Removed,
                failure,
            }
        })?;
    if paths_overlap(&orders_components, &removed_components) {
        return Err(InitializationPlanError::ManagedPathsOverlap);
    }
    for component in [orders_components[0], removed_components[0]] {
        if component.eq_ignore_ascii_case("README.md")
            || component.eq_ignore_ascii_case(".README.md.spielorders-stage")
        {
            return Err(InitializationPlanError::PlannedPathCollision {
                path: root.join(component),
            });
        }
    }

    let mut operations = vec![create_directory(root.join(".spielorders"))];
    let mut orders_directory = root.clone();
    for component in orders_components {
        orders_directory.push(component);
        match observe(&orders_directory, filesystem)? {
            None => operations.push(create_directory(orders_directory.clone())),
            Some(FileSystemEntryKind::Directory) => {
                let resolved = filesystem
                    .canonicalize(&orders_directory)
                    .map_err(|error| InitializationPlanError::FileSystem {
                        path: orders_directory.clone(),
                        error,
                    })?;
                if !resolved.starts_with(&root) {
                    return Err(InitializationPlanError::DisallowedLink {
                        path: orders_directory,
                    });
                }
                operations.push(preserve_directory(orders_directory.clone()));
            }
            Some(FileSystemEntryKind::SymbolicLink) => {
                return Err(InitializationPlanError::DisallowedLink {
                    path: orders_directory,
                });
            }
            Some(actual) => {
                return Err(InitializationPlanError::DestinationCollision {
                    path: orders_directory,
                    actual,
                });
            }
        }
    }

    let removed_directory = root.join(removed_components[0]);
    let removed_kind = observe(&removed_directory, filesystem)?;
    // Two configured paths are two directories or one, and the filesystem is the only
    // thing that knows which. Both must already be there for it to answer: a path
    // with nothing at it is not the directory the other one resolves to, and where
    // neither exists yet the question has no answer until initialization creates the
    // Orders Directory, which is where the commit asks it again.
    match managed_paths_collide(&orders_directory, &removed_directory, filesystem) {
        Ok(false) => {}
        Ok(true) => return Err(InitializationPlanError::ManagedPathsOverlap),
        Err((path, error)) => return Err(InitializationPlanError::FileSystem { path, error }),
    }
    match removed_kind {
        None => {}
        Some(FileSystemEntryKind::Directory) => {
            operations.push(preserve_directory(removed_directory.clone()));
        }
        Some(FileSystemEntryKind::SymbolicLink) => {
            return Err(InitializationPlanError::DisallowedLink {
                path: removed_directory,
            });
        }
        Some(actual) => {
            return Err(InitializationPlanError::DestinationCollision {
                path: removed_directory,
                actual,
            });
        }
    }

    let metadata = root.join(".spielorders");
    let config = config_bytes(options);
    let version = VersionDocument {
        metadata_version: Version1,
    }
    .to_json_bytes()
    .expect("version 1 marker serialization is infallible");
    let vendors = VendorsDocument {
        schema_version: Version1,
        vendors: Vec::new(),
    }
    .to_json_bytes()
    .expect("empty version 1 Vendors serialization is infallible");
    let index = IndexDocument {
        schema_version: Version1,
        orders: Vec::new(),
        vendors: Vec::new(),
    }
    .to_json_bytes()
    .expect("empty version 1 Package Index serialization is infallible");
    operations.extend([
        create_file(metadata.join("config.toml"), config),
        create_file(metadata.join("version.json"), version),
        create_file(metadata.join("vendors.json"), vendors),
        create_file(metadata.join("index.json"), index),
    ]);

    let readme = root.join("README.md");
    match observe(&readme, filesystem)? {
        None => operations.push(create_file(readme, Vec::new())),
        Some(FileSystemEntryKind::File) => operations.push(preserve_file(readme)),
        Some(FileSystemEntryKind::SymbolicLink) => {
            return Err(InitializationPlanError::DisallowedLink { path: readme });
        }
        Some(actual) => {
            return Err(InitializationPlanError::DestinationCollision {
                path: readme,
                actual,
            });
        }
    }

    for operation in &operations {
        if operation.kind == InitializationOperationKind::Create
            && operation.destination_kind == InitializationDestinationKind::File
        {
            let stage =
                staging_path(&operation.path).expect("planned initialization files have basenames");
            if let Some(actual) = observe(&stage, filesystem)? {
                return Err(InitializationPlanError::DestinationCollision {
                    path: stage,
                    actual,
                });
            }
        }
    }

    Ok(PackageInitializationPlan {
        preconditions: vec![
            InitializationPrecondition {
                path: root.clone(),
                expected: Some(FileSystemEntryKind::Directory),
            },
            InitializationPrecondition {
                path: removed_directory.clone(),
                expected: removed_kind,
            },
        ],
        root,
        options: options.clone(),
        orders_directory,
        removed_directory,
        operations,
    })
}

/// Whether the two configured managed paths are one directory on the filesystem, or
/// one holds the other, as it resolves them right now.
///
/// Package loading asks the same question of the same resolution, so a configuration
/// this accepts is one that loading accepts too. Keeping the two answers in one place
/// is what stops initialization minting a Package its own loader then refuses.
///
/// A failure carries the path it was observing, so neither caller has to guess which
/// of the two managed paths the filesystem refused to answer for.
fn managed_paths_collide<F: FileSystem>(
    orders_directory: &Path,
    removed_directory: &Path,
    filesystem: &F,
) -> Result<bool, (PathBuf, F::Error)> {
    let orders = resolve_managed_directory(orders_directory, filesystem)
        .map_err(|error| (orders_directory.to_path_buf(), error))?;
    let removed = resolve_managed_directory(removed_directory, filesystem)
        .map_err(|error| (removed_directory.to_path_buf(), error))?;
    let (ManagedDirectory::Resolved(orders), ManagedDirectory::Resolved(removed)) =
        (orders, removed)
    else {
        return Ok(false);
    };
    Ok(resolved_paths_overlap(&orders, &removed))
}

pub fn commit_package_initialization<F: DirectoryFileSystem>(
    plan: PackageInitializationPlan,
    filesystem: &F,
) -> PackageInitializationReport<F::Error> {
    match changed_precondition(&plan, filesystem) {
        Ok(Some(path)) => {
            return MutationReport::Rejected {
                plan,
                rejection: InitializationRejection::PreconditionsChanged { path },
            };
        }
        Err(failure) => {
            return MutationReport::Rejected {
                plan,
                rejection: InitializationRejection::CommitFailed {
                    failure,
                    cleanup_failures: Vec::new(),
                },
            };
        }
        Ok(None) => {}
    }

    let mut created_directories = Vec::new();
    let mut created_files = Vec::new();
    for operation in plan.operations.clone() {
        if operation.kind != InitializationOperationKind::Create {
            continue;
        }
        match operation.destination_kind {
            InitializationDestinationKind::Directory => {
                if let Err(error) = filesystem.create_directory(&operation.path) {
                    return rejected_after_cleanup(
                        plan,
                        InitializationCommitFailure {
                            phase: InitializationCommitPhase::CreateDirectory,
                            path: operation.path.clone(),
                            cause: InitializationCommitFailureCause::FileSystem(error),
                        },
                        StageCleanup::None,
                        created_files,
                        created_directories,
                        filesystem,
                    );
                }
                let directory_path = operation.path;
                created_directories.push(track_created(
                    directory_path.clone(),
                    FileSystemEntryKind::Directory,
                    filesystem,
                ));
                refresh_containing_directories(
                    &directory_path,
                    &mut created_directories,
                    filesystem,
                );
                // Creating the Orders Directory is what first lets the filesystem say
                // whether the Removed Orders Directory is a second directory or
                // another name for this one. Asking here refuses before anything
                // durable exists, so a configuration this application would then
                // refuse to load never becomes a Package.
                if directory_path == plan.orders_directory {
                    let failure = match managed_paths_collide(
                        &plan.orders_directory,
                        &plan.removed_directory,
                        filesystem,
                    ) {
                        Ok(false) => None,
                        Ok(true) => Some((
                            plan.removed_directory.clone(),
                            InitializationCommitFailureCause::ManagedPathsOverlap,
                        )),
                        Err((path, error)) => {
                            Some((path, InitializationCommitFailureCause::FileSystem(error)))
                        }
                    };
                    if let Some((path, cause)) = failure {
                        return rejected_after_cleanup(
                            plan,
                            InitializationCommitFailure {
                                phase: InitializationCommitPhase::VerifyManagedPaths,
                                path,
                                cause,
                            },
                            StageCleanup::None,
                            created_files,
                            created_directories,
                            filesystem,
                        );
                    }
                }
            }
            InitializationDestinationKind::File => {
                let candidate = operation
                    .contents
                    .as_deref()
                    .expect("file creation carries candidate bytes");
                let stage = staging_path(&operation.path)
                    .expect("planned initialization files have basenames");
                if let Err(error) = filesystem.write_new(&stage, candidate) {
                    return rejected_after_cleanup(
                        plan,
                        InitializationCommitFailure {
                            phase: InitializationCommitPhase::WriteStage,
                            path: stage.clone(),
                            cause: InitializationCommitFailureCause::FileSystem(error),
                        },
                        StageCleanup::Untracked(stage),
                        created_files,
                        created_directories,
                        filesystem,
                    );
                }
                let tracked_stage =
                    track_created(stage.clone(), FileSystemEntryKind::File, filesystem);
                refresh_containing_directories(&stage, &mut created_directories, filesystem);
                let staged = match filesystem.read(&stage) {
                    Ok(staged) => staged,
                    Err(error) => {
                        return rejected_after_cleanup(
                            plan,
                            InitializationCommitFailure {
                                phase: InitializationCommitPhase::ReadStage,
                                path: stage.clone(),
                                cause: InitializationCommitFailureCause::FileSystem(error),
                            },
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        );
                    }
                };
                if staged != candidate {
                    return rejected_after_cleanup(
                        plan,
                        InitializationCommitFailure {
                            phase: InitializationCommitPhase::ValidateCandidate,
                            path: stage.clone(),
                            cause: InitializationCommitFailureCause::CandidateChanged,
                        },
                        StageCleanup::Tracked(tracked_stage),
                        created_files,
                        created_directories,
                        filesystem,
                    );
                }
                if let Err(error) = filesystem.sync_file(&stage) {
                    return rejected_after_cleanup(
                        plan,
                        InitializationCommitFailure {
                            phase: InitializationCommitPhase::SyncStage,
                            path: stage.clone(),
                            cause: InitializationCommitFailureCause::FileSystem(error),
                        },
                        StageCleanup::Tracked(tracked_stage),
                        created_files,
                        created_directories,
                        filesystem,
                    );
                }
                match filesystem.link_new_file(&stage, &operation.path) {
                    Ok(NewFileLinkOutcome::Linked) => {}
                    Ok(NewFileLinkOutcome::DestinationExists) => {
                        return rejected_after_cleanup(
                            plan,
                            InitializationCommitFailure {
                                phase: InitializationCommitPhase::InstallDestination,
                                path: operation.path.clone(),
                                cause: InitializationCommitFailureCause::DestinationChanged,
                            },
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        );
                    }
                    Err(error) => {
                        return rejected_after_cleanup(
                            plan,
                            InitializationCommitFailure {
                                phase: InitializationCommitPhase::InstallDestination,
                                path: operation.path.clone(),
                                cause: InitializationCommitFailureCause::FileSystem(error),
                            },
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        );
                    }
                }
                created_files.push(track_created(
                    operation.path.clone(),
                    FileSystemEntryKind::File,
                    filesystem,
                ));
                refresh_containing_directories(
                    &operation.path,
                    &mut created_directories,
                    filesystem,
                );
                if let Err(error) = filesystem.remove_file(&stage) {
                    return rejected_after_cleanup(
                        plan,
                        InitializationCommitFailure {
                            phase: InitializationCommitPhase::RemoveInstalledStage,
                            path: stage.clone(),
                            cause: InitializationCommitFailureCause::FileSystem(error),
                        },
                        StageCleanup::Tracked(tracked_stage),
                        created_files,
                        created_directories,
                        filesystem,
                    );
                }
                refresh_containing_directories(&stage, &mut created_directories, filesystem);
            }
        }
    }

    let root = plan.root.clone();
    MutationReport::Committed {
        plan,
        result: InitializedPackage { root },
    }
}

fn changed_precondition<F: FileSystem>(
    plan: &PackageInitializationPlan,
    filesystem: &F,
) -> Result<Option<PathBuf>, InitializationCommitFailure<F::Error>> {
    for precondition in &plan.preconditions {
        match filesystem.symlink_metadata(&precondition.path) {
            Ok(actual) if actual == precondition.expected => {}
            Ok(_) => return Ok(Some(precondition.path.clone())),
            Err(error) => {
                return Err(observation_failure(precondition.path.clone(), error));
            }
        }
    }
    for operation in &plan.operations {
        let expected = match operation.kind {
            InitializationOperationKind::Create => None,
            InitializationOperationKind::Preserve => Some(match operation.destination_kind {
                InitializationDestinationKind::Directory => FileSystemEntryKind::Directory,
                InitializationDestinationKind::File => FileSystemEntryKind::File,
            }),
        };
        match filesystem.symlink_metadata(&operation.path) {
            Ok(actual) if actual == expected => {}
            Ok(_) => return Ok(Some(operation.path.clone())),
            Err(error) => return Err(observation_failure(operation.path.clone(), error)),
        }
        if operation.kind == InitializationOperationKind::Create
            && operation.destination_kind == InitializationDestinationKind::File
        {
            let stage =
                staging_path(&operation.path).expect("planned initialization files have basenames");
            match filesystem.symlink_metadata(&stage) {
                Ok(None) => {}
                Ok(Some(_)) => return Ok(Some(stage)),
                Err(error) => return Err(observation_failure(stage, error)),
            }
        }
    }
    Ok(None)
}

fn observation_failure<E>(path: PathBuf, error: E) -> InitializationCommitFailure<E> {
    InitializationCommitFailure {
        phase: InitializationCommitPhase::ObserveDestination,
        path,
        cause: InitializationCommitFailureCause::FileSystem(error),
    }
}

struct TrackedArtifact<E> {
    path: PathBuf,
    kind: FileSystemEntryKind,
    observation: Result<FileChangeObservation, E>,
}

enum StageCleanup<E> {
    None,
    Untracked(PathBuf),
    Tracked(TrackedArtifact<E>),
}

fn track_created<F: FileSystem>(
    path: PathBuf,
    kind: FileSystemEntryKind,
    filesystem: &F,
) -> TrackedArtifact<F::Error> {
    let observation = filesystem.observe_file_change(&path);
    TrackedArtifact {
        path,
        kind,
        observation,
    }
}

fn refresh_containing_directories<F: FileSystem>(
    changed_path: &Path,
    directories: &mut [TrackedArtifact<F::Error>],
    filesystem: &F,
) {
    for directory in directories {
        if changed_path.starts_with(&directory.path) {
            directory.observation = filesystem.observe_file_change(&directory.path);
        }
    }
}

fn rejected_after_cleanup<F: DirectoryFileSystem>(
    plan: PackageInitializationPlan,
    failure: InitializationCommitFailure<F::Error>,
    stage: StageCleanup<F::Error>,
    mut created_files: Vec<TrackedArtifact<F::Error>>,
    mut created_directories: Vec<TrackedArtifact<F::Error>>,
    filesystem: &F,
) -> PackageInitializationReport<F::Error> {
    let mut cleanup_failures = Vec::new();
    match stage {
        StageCleanup::None => {}
        StageCleanup::Untracked(path) => match filesystem.observe_file_change(&path) {
            Ok(observation) if observation.kind().is_none() => {}
            Ok(_) => cleanup_failures.push(ownership_uncertain(
                InitializationCleanupPhase::RemoveStage,
                path,
            )),
            Err(error) => cleanup_failures.push(cleanup_filesystem_failure(
                InitializationCleanupPhase::ObserveStage,
                path,
                error,
            )),
        },
        StageCleanup::Tracked(stage) => {
            let stage_path = stage.path.clone();
            if remove_tracked(
                stage,
                InitializationCleanupPhase::ObserveStage,
                InitializationCleanupPhase::RemoveStage,
                filesystem,
                &mut cleanup_failures,
            ) {
                refresh_containing_directories(&stage_path, &mut created_directories, filesystem);
            }
        }
    }
    while let Some(file) = created_files.pop() {
        let path = file.path.clone();
        if remove_tracked(
            file,
            InitializationCleanupPhase::ObserveCreatedFile,
            InitializationCleanupPhase::RemoveCreatedFile,
            filesystem,
            &mut cleanup_failures,
        ) {
            refresh_containing_directories(&path, &mut created_directories, filesystem);
        }
    }
    while let Some(directory) = created_directories.pop() {
        let path = directory.path.clone();
        if remove_tracked_directory(directory, filesystem, &mut cleanup_failures) {
            refresh_containing_directories(&path, &mut created_directories, filesystem);
        }
    }
    MutationReport::Rejected {
        plan,
        rejection: InitializationRejection::CommitFailed {
            failure,
            cleanup_failures,
        },
    }
}

fn remove_tracked<F: FileSystem>(
    artifact: TrackedArtifact<F::Error>,
    observe_phase: InitializationCleanupPhase,
    remove_phase: InitializationCleanupPhase,
    filesystem: &F,
    failures: &mut Vec<InitializationCleanupFailure<F::Error>>,
) -> bool {
    let expected = match artifact.observation {
        Ok(observation) => observation,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                observe_phase,
                artifact.path,
                error,
            ));
            return false;
        }
    };
    let current = match filesystem.observe_file_change(&artifact.path) {
        Ok(observation) => observation,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                observe_phase,
                artifact.path,
                error,
            ));
            return false;
        }
    };
    if current.kind().is_none() {
        return false;
    }
    if !same_owned_artifact(expected, current, artifact.kind) {
        failures.push(ownership_uncertain(remove_phase, artifact.path));
        return false;
    }
    match filesystem.remove_file(&artifact.path) {
        Ok(()) => true,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                remove_phase,
                artifact.path,
                error,
            ));
            false
        }
    }
}

fn remove_tracked_directory<F: DirectoryFileSystem>(
    directory: TrackedArtifact<F::Error>,
    filesystem: &F,
    failures: &mut Vec<InitializationCleanupFailure<F::Error>>,
) -> bool {
    let expected = match directory.observation {
        Ok(observation) => observation,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                InitializationCleanupPhase::ObserveCreatedDirectory,
                directory.path,
                error,
            ));
            return false;
        }
    };
    let current = match filesystem.observe_file_change(&directory.path) {
        Ok(observation) => observation,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                InitializationCleanupPhase::ObserveCreatedDirectory,
                directory.path,
                error,
            ));
            return false;
        }
    };
    if current.kind().is_none() {
        return false;
    }
    if !same_owned_artifact(expected, current, FileSystemEntryKind::Directory) {
        failures.push(ownership_uncertain(
            InitializationCleanupPhase::RemoveCreatedDirectory,
            directory.path,
        ));
        return false;
    }
    match filesystem.remove_directory(&directory.path) {
        Ok(()) => true,
        Err(error) => {
            failures.push(cleanup_filesystem_failure(
                InitializationCleanupPhase::RemoveCreatedDirectory,
                directory.path,
                error,
            ));
            false
        }
    }
}

fn same_owned_artifact(
    expected: FileChangeObservation,
    current: FileChangeObservation,
    kind: FileSystemEntryKind,
) -> bool {
    if expected.kind() != Some(kind) || current.kind() != Some(kind) {
        return false;
    }
    matches!(
        (expected.evidence(), current.evidence()),
        (
            FileChangeEvidence::Comparable {
                comparison_domain: expected_domain,
                sequence: expected_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: current_domain,
                sequence: current_sequence,
            }
        ) if expected_domain == current_domain && expected_sequence == current_sequence
    )
}

fn cleanup_filesystem_failure<E>(
    phase: InitializationCleanupPhase,
    path: PathBuf,
    error: E,
) -> InitializationCleanupFailure<E> {
    InitializationCleanupFailure {
        phase,
        path,
        cause: InitializationCleanupFailureCause::FileSystem(error),
    }
}

fn ownership_uncertain<E>(
    phase: InitializationCleanupPhase,
    path: PathBuf,
) -> InitializationCleanupFailure<E> {
    InitializationCleanupFailure {
        phase,
        path,
        cause: InitializationCleanupFailureCause::OwnershipUncertain,
    }
}

fn observe<F: FileSystem>(
    path: &Path,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, InitializationPlanError<F::Error>> {
    filesystem
        .symlink_metadata(path)
        .map_err(|error| InitializationPlanError::FileSystem {
            path: path.to_path_buf(),
            error,
        })
}

fn config_bytes(options: &InitializationOptions) -> Vec<u8> {
    let mut table = toml::Table::new();
    if options.orders_path != "Orders" {
        table.insert(
            "orders_path".into(),
            toml::Value::String(options.orders_path.clone()),
        );
    }
    if options.removed_path != "Removed" {
        table.insert(
            "removed_path".into(),
            toml::Value::String(options.removed_path.clone()),
        );
    }
    toml::to_string(&table)
        .expect("string-only Package configuration serialization is infallible")
        .into_bytes()
}

fn create_directory(path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Create,
        destination_kind: InitializationDestinationKind::Directory,
        path,
        contents: None,
    }
}

fn preserve_directory(path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Preserve,
        destination_kind: InitializationDestinationKind::Directory,
        path,
        contents: None,
    }
}

fn create_file(path: PathBuf, contents: Vec<u8>) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Create,
        destination_kind: InitializationDestinationKind::File,
        path,
        contents: Some(contents),
    }
}

fn preserve_file(path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Preserve,
        destination_kind: InitializationDestinationKind::File,
        path,
        contents: None,
    }
}
