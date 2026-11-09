//! Planning and persistence for initializing one unmanaged SpielOrders Package.

use crate::{
    Answer, Failure,
    filesystem::staging_path,
    metadata::{IndexDocument, VendorsDocument, Version1, VersionDocument},
    mutation::{CleanupFailure, CommitFailure, MutationReport},
    operations::{FailedOperation, FailedPath, refuse_linked_root},
    paths::{
        ConfiguredPath, ManagedDirectory, paths_overlap, resolve_managed_directory,
        resolved_paths_overlap, validate_configured_path,
    },
    ports::{
        FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
        NewFileLinkOutcome,
    },
    readme::{Location, package_relative_path_text},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InitializationOptions {
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

    /// The configured Orders Directory, exactly as supplied.
    ///
    /// Exposed so that a caller which overrides one of the two paths can take the other
    /// from [`Self::default`] rather than repeating the conventional name. The
    /// conventional names are this type's own default and are spelled nowhere else.
    pub fn orders_path(&self) -> &str {
        &self.orders_path
    }

    /// The configured Removed Orders Directory, exactly as supplied.
    pub fn removed_path(&self) -> &str {
        &self.removed_path
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::new("Orders", "Removed")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationOperationKind {
    Create,
    Preserve,
}

impl crate::code::Coded for InitializationOperationKind {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationDestinationKind {
    Directory,
    File,
}

impl crate::code::Coded for InitializationDestinationKind {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationOperation {
    pub(crate) kind: InitializationOperationKind,
    pub(crate) destination_kind: InitializationDestinationKind,
    pub(crate) location: Location,
    contents: Option<Vec<u8>>,
}

impl InitializationOperation {
    pub fn path(&self) -> &Path {
        &self.location.absolute_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInitializationPlan {
    root: PathBuf,
    orders_directory: PathBuf,
    removed_directory: PathBuf,
    pub(crate) operations: Vec<InitializationOperation>,
    preconditions: Vec<InitializationPrecondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InitializationPrecondition {
    path: PathBuf,
    expected: Option<FileSystemEntryKind>,
}

pub(crate) type PackageInitializationReport = MutationReport<()>;

pub(crate) fn plan_package_initialization<F: FileSystem>(
    selected_root: &Path,
    options: &InitializationOptions,
    filesystem: &F,
) -> Answer<PackageInitializationPlan> {
    Answer::of(|| {
        refuse_linked_root(selected_root, filesystem)?;
        let selected_kind = observe(selected_root, None, filesystem)?;
        if selected_kind != Some(FileSystemEntryKind::Directory) {
            return Err(Failure::RootNotOrdinaryDirectory);
        }
        let root = filesystem.canonicalize(selected_root).map_err(|error| {
            Failure::from(FailedOperation::filesystem(
                FailedPath::of(selected_root),
                &error,
                filesystem,
            ))
        })?;
        // The root the refusals below name. They refuse before any Package exists here, so
        // there is nothing for the root to be relative to and it carries no spelling.
        let root_location = Location::absolute(root.clone())
            .expect("a canonicalized selected root is an absolute path");
        if observe(&root.join(".spielorders"), Some(&root_location), filesystem)?.is_some() {
            // The one refusal above that is about a Package which *does* stand here, so the
            // root has a Package to be relative to and spells itself the way a loaded one
            // does. One question, one answer.
            return Err(Failure::AlreadyAPackage {
                root: Location::within_package(root.clone(), ".")
                    .expect("a canonicalized selected root is an absolute path"),
            });
        }

        let orders_components =
            validate_configured_path(&options.orders_path, false).map_err(|failure| {
                Failure::InvalidConfiguredPath {
                    root: root_location.clone(),
                    path: ConfiguredPath::Orders,
                    failure,
                }
            })?;
        let removed_components =
            validate_configured_path(&options.removed_path, true).map_err(|failure| {
                Failure::InvalidConfiguredPath {
                    root: root_location.clone(),
                    path: ConfiguredPath::Removed,
                    failure,
                }
            })?;
        if paths_overlap(&orders_components, &removed_components) {
            return Err(Failure::ManagedPathsOverlap {
                root: Box::new(root_location),
            });
        }
        for (configured, component) in [
            (ConfiguredPath::Orders, orders_components[0]),
            (ConfiguredPath::Removed, removed_components[0]),
        ] {
            if component.eq_ignore_ascii_case("README.md")
                || component.eq_ignore_ascii_case(".README.md.spielorders-stage")
            {
                return Err(Failure::PlannedPathCollision {
                    root: root_location,
                    configured,
                    path: planned_location(&root, root.join(component)),
                });
            }
        }

        let mut operations = vec![create_directory(&root, root.join(".spielorders"))];
        let mut orders_directory = root.clone();
        for component in orders_components {
            orders_directory.push(component);
            match observe(&orders_directory, Some(&root_location), filesystem)? {
                None => operations.push(create_directory(&root, orders_directory.clone())),
                Some(FileSystemEntryKind::Directory) => {
                    let resolved = filesystem
                        .canonicalize(&orders_directory)
                        .map_err(|error| {
                            Failure::from(
                                FailedOperation::filesystem(
                                    FailedPath::of(&orders_directory),
                                    &error,
                                    filesystem,
                                )
                                .under(&root_location),
                            )
                        })?;
                    if !resolved.starts_with(&root) {
                        return Err(Failure::DisallowedLink {
                            root: root_location,
                            path: planned_location(&root, orders_directory),
                        });
                    }
                    operations.push(preserve_directory(&root, orders_directory.clone()));
                }
                Some(FileSystemEntryKind::SymbolicLink) => {
                    return Err(Failure::DisallowedLink {
                        root: root_location,
                        path: planned_location(&root, orders_directory),
                    });
                }
                Some(actual) => {
                    return Err(Failure::DestinationCollision {
                        root: root_location,
                        location: planned_location(&root, orders_directory),
                        found: Some(actual),
                    });
                }
            }
        }

        let removed_directory = root.join(removed_components[0]);
        let removed_kind = observe(&removed_directory, Some(&root_location), filesystem)?;
        // Two configured paths are two directories or one, and the filesystem is the only
        // thing that knows which. Both must already be there for it to answer: a path
        // with nothing at it is not the directory the other one resolves to, and where
        // neither exists yet the question has no answer until initialization creates the
        // Orders Directory, which is where the commit asks it again.
        match managed_paths_collide(&orders_directory, &removed_directory, filesystem) {
            Ok(false) => {}
            Ok(true) => {
                return Err(Failure::ManagedPathsOverlap {
                    root: Box::new(root_location),
                });
            }
            Err((path, error)) => {
                return Err(Failure::from(
                    FailedOperation::filesystem(FailedPath::of(&path), &error, filesystem)
                        .under(&root_location),
                ));
            }
        }
        match removed_kind {
            None => {}
            Some(FileSystemEntryKind::Directory) => {
                operations.push(preserve_directory(&root, removed_directory.clone()));
            }
            Some(FileSystemEntryKind::SymbolicLink) => {
                return Err(Failure::DisallowedLink {
                    root: root_location,
                    path: planned_location(&root, removed_directory),
                });
            }
            Some(actual) => {
                return Err(Failure::DestinationCollision {
                    root: root_location,
                    location: planned_location(&root, removed_directory),
                    found: Some(actual),
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
            create_file(&root, metadata.join("config.toml"), config),
            create_file(&root, metadata.join("version.json"), version),
            create_file(&root, metadata.join("vendors.json"), vendors),
            create_file(&root, metadata.join("index.json"), index),
        ]);

        let readme = root.join("README.md");
        match observe(&readme, Some(&root_location), filesystem)? {
            None => operations.push(create_file(&root, readme, Vec::new())),
            Some(FileSystemEntryKind::File) => operations.push(preserve_file(&root, readme)),
            Some(FileSystemEntryKind::SymbolicLink) => {
                return Err(Failure::DisallowedLink {
                    root: root_location,
                    path: planned_location(&root, readme),
                });
            }
            Some(actual) => {
                return Err(Failure::DestinationCollision {
                    root: root_location,
                    location: planned_location(&root, readme),
                    found: Some(actual),
                });
            }
        }

        for operation in &operations {
            if operation.kind == InitializationOperationKind::Create
                && operation.destination_kind == InitializationDestinationKind::File
            {
                let stage = staging_path(operation.path())
                    .expect("planned initialization files have basenames");
                if let Some(actual) = observe(&stage, Some(&root_location), filesystem)? {
                    return Err(Failure::DestinationCollision {
                        root: root_location,
                        location: planned_location(&root, stage),
                        found: Some(actual),
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
            orders_directory,
            removed_directory,
            operations,
        })
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
        .map_err(|(_, error)| (orders_directory.to_path_buf(), error))?;
    let removed = resolve_managed_directory(removed_directory, filesystem)
        .map_err(|(_, error)| (removed_directory.to_path_buf(), error))?;
    let (ManagedDirectory::Resolved(orders), ManagedDirectory::Resolved(removed)) =
        (orders, removed)
    else {
        return Ok(false);
    };
    Ok(resolved_paths_overlap(&orders, &removed))
}

pub(crate) fn commit_package_initialization<F: FileSystem>(
    plan: PackageInitializationPlan,
    filesystem: &F,
) -> Answer<PackageInitializationReport> {
    Answer::of(|| {
        match changed_precondition(&plan, filesystem) {
            Ok(Some(path)) => {
                let location = Location::absolute(path).expect("a precondition path is absolute");
                return Err(Failure::PreconditionsChanged { location });
            }
            Err(failure) => return Err(failure),
            Ok(None) => {}
        }

        let root = plan.root.clone();
        let mut created_directories = Vec::new();
        let mut created_files = Vec::new();
        for operation in plan.operations.clone() {
            if operation.kind != InitializationOperationKind::Create {
                continue;
            }
            match operation.destination_kind {
                InitializationDestinationKind::Directory => {
                    if filesystem.create_directory(operation.path()).is_err() {
                        return Err(rejected_after_cleanup(
                            &plan,
                            CommitFailure::new(planned_location(
                                &root,
                                operation.path().to_path_buf(),
                            )),
                            StageCleanup::None,
                            created_files,
                            created_directories,
                            filesystem,
                        ));
                    }
                    let directory_path = operation.path().to_path_buf();
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
                            Ok(true) => Some(CommitFailure::new(planned_location(
                                &root,
                                plan.removed_directory.clone(),
                            ))),
                            Err((path, _)) => {
                                Some(CommitFailure::new(planned_location(&root, path)))
                            }
                        };
                        if let Some(failure) = failure {
                            return Err(rejected_after_cleanup(
                                &plan,
                                failure,
                                StageCleanup::None,
                                created_files,
                                created_directories,
                                filesystem,
                            ));
                        }
                    }
                }
                InitializationDestinationKind::File => {
                    let candidate = operation
                        .contents
                        .as_deref()
                        .expect("file creation carries candidate bytes");
                    let stage = staging_path(operation.path())
                        .expect("planned initialization files have basenames");
                    if filesystem.write_new(&stage, candidate).is_err() {
                        return Err(rejected_after_cleanup(
                            &plan,
                            CommitFailure::new(planned_location(&root, stage.clone())),
                            StageCleanup::Untracked(stage),
                            created_files,
                            created_directories,
                            filesystem,
                        ));
                    }
                    let tracked_stage =
                        track_created(stage.clone(), FileSystemEntryKind::File, filesystem);
                    refresh_containing_directories(&stage, &mut created_directories, filesystem);
                    let staged = match filesystem.read(&stage) {
                        Ok(staged) => staged,
                        Err(_) => {
                            return Err(rejected_after_cleanup(
                                &plan,
                                CommitFailure::new(planned_location(&root, stage.clone())),
                                StageCleanup::Tracked(tracked_stage),
                                created_files,
                                created_directories,
                                filesystem,
                            ));
                        }
                    };
                    if staged != candidate {
                        return Err(rejected_after_cleanup(
                            &plan,
                            CommitFailure::new(planned_location(&root, stage.clone())),
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        ));
                    }
                    if filesystem.sync_file(&stage).is_err() {
                        return Err(rejected_after_cleanup(
                            &plan,
                            CommitFailure::new(planned_location(&root, stage.clone())),
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        ));
                    }
                    match filesystem.link_new_file(&stage, operation.path()) {
                        Ok(NewFileLinkOutcome::Linked) => {}
                        Ok(NewFileLinkOutcome::DestinationExists) => {
                            return Err(rejected_after_cleanup(
                                &plan,
                                CommitFailure::new(planned_location(
                                    &root,
                                    operation.path().to_path_buf(),
                                )),
                                StageCleanup::Tracked(tracked_stage),
                                created_files,
                                created_directories,
                                filesystem,
                            ));
                        }
                        Err(_) => {
                            return Err(rejected_after_cleanup(
                                &plan,
                                CommitFailure::new(planned_location(
                                    &root,
                                    operation.path().to_path_buf(),
                                )),
                                StageCleanup::Tracked(tracked_stage),
                                created_files,
                                created_directories,
                                filesystem,
                            ));
                        }
                    }
                    created_files.push(track_created(
                        operation.path().to_path_buf(),
                        FileSystemEntryKind::File,
                        filesystem,
                    ));
                    refresh_containing_directories(
                        operation.path(),
                        &mut created_directories,
                        filesystem,
                    );
                    if filesystem.remove_file(&stage).is_err() {
                        return Err(rejected_after_cleanup(
                            &plan,
                            CommitFailure::new(planned_location(&root, stage.clone())),
                            StageCleanup::Tracked(tracked_stage),
                            created_files,
                            created_directories,
                            filesystem,
                        ));
                    }
                    refresh_containing_directories(&stage, &mut created_directories, filesystem);
                }
            }
        }

        Ok(MutationReport::Committed { result: () })
    })
}

fn changed_precondition<F: FileSystem>(
    plan: &PackageInitializationPlan,
    filesystem: &F,
) -> Result<Option<PathBuf>, Failure> {
    let observation_failure = |path: PathBuf| {
        Failure::commit(
            CommitFailure::new(planned_location(&plan.root, path)),
            Vec::new(),
        )
    };
    for precondition in &plan.preconditions {
        match filesystem.symlink_metadata(&precondition.path) {
            Ok(actual) if actual == precondition.expected => {}
            Ok(_) => return Ok(Some(precondition.path.clone())),
            Err(_) => {
                return Err(observation_failure(precondition.path.clone()));
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
        match filesystem.symlink_metadata(operation.path()) {
            Ok(actual) if actual == expected => {}
            Ok(_) => return Ok(Some(operation.path().to_path_buf())),
            Err(_) => {
                return Err(observation_failure(operation.path().to_path_buf()));
            }
        }
        if operation.kind == InitializationOperationKind::Create
            && operation.destination_kind == InitializationDestinationKind::File
        {
            let stage = staging_path(operation.path())
                .expect("planned initialization files have basenames");
            match filesystem.symlink_metadata(&stage) {
                Ok(None) => {}
                Ok(Some(_)) => return Ok(Some(stage)),
                Err(_) => return Err(observation_failure(stage)),
            }
        }
    }
    Ok(None)
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

fn rejected_after_cleanup<F: FileSystem>(
    plan: &PackageInitializationPlan,
    failure: CommitFailure,
    stage: StageCleanup<F::Error>,
    mut created_files: Vec<TrackedArtifact<F::Error>>,
    mut created_directories: Vec<TrackedArtifact<F::Error>>,
    filesystem: &F,
) -> Failure {
    let mut cleanup_failures = Vec::new();
    let root = plan.root.as_path();
    match stage {
        StageCleanup::None => {}
        StageCleanup::Untracked(path) => match filesystem.observe_file_change(&path) {
            Ok(observation) if observation.kind.is_none() => {}
            Ok(_) => cleanup_failures.push(left_behind(root, path)),
            Err(_) => cleanup_failures.push(left_behind(root, path)),
        },
        StageCleanup::Tracked(stage) => {
            let stage_path = stage.path.clone();
            if remove_tracked(root, stage, filesystem, &mut cleanup_failures) {
                refresh_containing_directories(&stage_path, &mut created_directories, filesystem);
            }
        }
    }
    while let Some(file) = created_files.pop() {
        let path = file.path.clone();
        if remove_tracked(root, file, filesystem, &mut cleanup_failures) {
            refresh_containing_directories(&path, &mut created_directories, filesystem);
        }
    }
    while let Some(directory) = created_directories.pop() {
        let path = directory.path.clone();
        if remove_tracked_directory(root, directory, filesystem, &mut cleanup_failures) {
            refresh_containing_directories(&path, &mut created_directories, filesystem);
        }
    }
    Failure::commit(failure, cleanup_failures)
}

fn remove_tracked<F: FileSystem>(
    root: &Path,
    artifact: TrackedArtifact<F::Error>,
    filesystem: &F,
    failures: &mut Vec<CleanupFailure>,
) -> bool {
    let expected = match artifact.observation {
        Ok(observation) => observation,
        Err(_) => {
            failures.push(left_behind(root, artifact.path));
            return false;
        }
    };
    let current = match filesystem.observe_file_change(&artifact.path) {
        Ok(observation) => observation,
        Err(_) => {
            failures.push(left_behind(root, artifact.path));
            return false;
        }
    };
    if current.kind.is_none() {
        return false;
    }
    if !same_owned_artifact(expected, current, artifact.kind) {
        failures.push(left_behind(root, artifact.path));
        return false;
    }
    match filesystem.remove_file(&artifact.path) {
        Ok(()) => true,
        Err(_) => {
            failures.push(left_behind(root, artifact.path));
            false
        }
    }
}

fn remove_tracked_directory<F: FileSystem>(
    root: &Path,
    directory: TrackedArtifact<F::Error>,
    filesystem: &F,
    failures: &mut Vec<CleanupFailure>,
) -> bool {
    let expected = match directory.observation {
        Ok(observation) => observation,
        Err(_) => {
            failures.push(left_behind(root, directory.path));
            return false;
        }
    };
    let current = match filesystem.observe_file_change(&directory.path) {
        Ok(observation) => observation,
        Err(_) => {
            failures.push(left_behind(root, directory.path));
            return false;
        }
    };
    if current.kind.is_none() {
        return false;
    }
    if !same_owned_artifact(expected, current, FileSystemEntryKind::Directory) {
        failures.push(left_behind(root, directory.path));
        return false;
    }
    match filesystem.remove_directory(&directory.path) {
        Ok(()) => true,
        Err(_) => {
            failures.push(left_behind(root, directory.path));
            false
        }
    }
}

/// What a failed commit's cleanup could not remove or put back at `path`.
fn left_behind(root: &Path, path: PathBuf) -> CleanupFailure {
    CleanupFailure::new(planned_location(root, path))
}

fn same_owned_artifact(
    expected: FileChangeObservation,
    current: FileChangeObservation,
    kind: FileSystemEntryKind,
) -> bool {
    if expected.kind != Some(kind) || current.kind != Some(kind) {
        return false;
    }
    matches!(
        (expected.evidence, current.evidence),
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

/// What stands at `path`, or the typed refusal the filesystem gave instead.
///
/// `root` is the resolved selected root where the caller already has one, so a refusal
/// arriving after the resolution names it.
fn observe<F: FileSystem>(
    path: &Path,
    root: Option<&Location>,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, Failure> {
    filesystem.symlink_metadata(path).map_err(|error| {
        let failed = FailedOperation::filesystem(FailedPath::of(path), &error, filesystem);
        Failure::from(match root {
            Some(root) => failed.under(root),
            None => failed,
        })
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

/// A `Location` for one planned initialization artifact.
///
/// The expectation holds by construction rather than by assertion. Every planned path is
/// the canonical root joined with one of this module's own literal managed names or with
/// the components of a configured value `validate_configured_path` has already accepted —
/// and that validator enforces the same grammar `package_relative_path_text` answers
/// against, so a value that reached here has a Package-relative spelling.
fn planned_location(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned initialization path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned initialization path is normalized")
}

fn create_directory(root: &Path, path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Create,
        destination_kind: InitializationDestinationKind::Directory,
        location: planned_location(root, path),
        contents: None,
    }
}

fn preserve_directory(root: &Path, path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Preserve,
        destination_kind: InitializationDestinationKind::Directory,
        location: planned_location(root, path),
        contents: None,
    }
}

fn create_file(root: &Path, path: PathBuf, contents: Vec<u8>) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Create,
        destination_kind: InitializationDestinationKind::File,
        location: planned_location(root, path),
        contents: Some(contents),
    }
}

fn preserve_file(root: &Path, path: PathBuf) -> InitializationOperation {
    InitializationOperation {
        kind: InitializationOperationKind::Preserve,
        destination_kind: InitializationDestinationKind::File,
        location: planned_location(root, path),
        contents: None,
    }
}
