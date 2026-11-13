//! Exclusive creation batches shared by Package initialization and agent provisioning.
//! Creation never replaces a destination, and a replacement replaces a file only while it
//! still holds exactly the bytes planned against; rollback removes only unchanged owned
//! artifacts.

use crate::{
    Answer, Failure,
    filesystem::{FileReplacement, replace_validated_files, staging_path},
    mutation::{CleanupFailure, CommitFailure, MutationReport, Never},
    ports::{
        FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
        NewFileLinkOutcome,
    },
    readme::{Location, package_relative_path_text},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreationOperationKind {
    Create,
    Preserve,
    Replace,
}

impl crate::code::Coded for CreationOperationKind {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreationDestinationKind {
    Directory,
    File,
}

impl crate::code::Coded for CreationDestinationKind {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreationOperation {
    pub(crate) kind: CreationOperationKind,
    pub(crate) destination_kind: CreationDestinationKind,
    pub(crate) location: Location,
    contents: Option<Vec<u8>>,
    /// The bytes a replacement was planned against.
    prior: Option<Vec<u8>>,
}

impl CreationOperation {
    pub fn path(&self) -> &Path {
        &self.location.absolute_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreationBatch {
    pub(crate) root: PathBuf,
    pub(crate) operations: Vec<CreationOperation>,
    pub(crate) preconditions: Vec<CreationPrecondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreationPrecondition {
    pub(crate) path: PathBuf,
    pub(crate) expected: Option<FileSystemEntryKind>,
}

pub(crate) fn commit_creation<F: FileSystem>(
    plan: CreationBatch,
    filesystem: &F,
    validate_directory: impl Fn(&Path, &F) -> Option<CommitFailure>,
) -> Answer<MutationReport<()>> {
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
            if operation.kind != CreationOperationKind::Create {
                continue;
            }
            match operation.destination_kind {
                CreationDestinationKind::Directory => {
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
                    if let Some(failure) = validate_directory(&directory_path, filesystem) {
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
                CreationDestinationKind::File => {
                    let candidate = operation
                        .contents
                        .as_deref()
                        .expect("file creation carries candidate bytes");
                    let stage = staging_path(operation.path())
                        .expect("planned creation files have basenames");
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

        // Create missing entries first: a creation failure has no replaced content to
        // restore. Replacements then share the guarded reverse rollback used by other
        // multi-file mutations.
        let replacements: Vec<_> = plan
            .operations
            .iter()
            .filter(|operation| operation.kind == CreationOperationKind::Replace)
            .map(|operation| {
                FileReplacement::observed(
                    operation.path(),
                    operation.contents.as_deref().expect("replacement bytes"),
                    operation.prior.as_deref().expect("planned prior bytes"),
                )
            })
            .collect();
        if let Err(error) =
            replace_validated_files(&replacements, filesystem, |_, _| Ok::<(), Never>(()))
        {
            let destinations: Vec<_> = replacements
                .iter()
                .map(FileReplacement::destination)
                .collect();
            let failure = Failure::of_replacements(&root, &destinations, error);
            let mut cleanup = cleanup_creation(
                &plan,
                StageCleanup::None,
                created_files,
                created_directories,
                filesystem,
            );
            return Err(match failure {
                Failure::CommitFailed { failure } => Failure::commit(*failure, cleanup),
                Failure::CommitFailedCleanupIncomplete {
                    failure,
                    cleanup_failures,
                } => {
                    cleanup.extend(cleanup_failures);
                    Failure::commit(*failure, cleanup)
                }
                other => other,
            });
        }

        if plan
            .operations
            .iter()
            .any(|operation| operation.kind != CreationOperationKind::Preserve)
        {
            Ok(MutationReport::Committed { result: () })
        } else {
            Ok(MutationReport::Unchanged { result: () })
        }
    })
}

fn changed_precondition<F: FileSystem>(
    plan: &CreationBatch,
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
            CreationOperationKind::Create => None,
            CreationOperationKind::Preserve | CreationOperationKind::Replace => {
                Some(match operation.destination_kind {
                    CreationDestinationKind::Directory => FileSystemEntryKind::Directory,
                    CreationDestinationKind::File => FileSystemEntryKind::File,
                })
            }
        };
        match filesystem.symlink_metadata(operation.path()) {
            Ok(actual) if actual == expected => {}
            Ok(_) => return Ok(Some(operation.path().to_path_buf())),
            Err(_) => {
                return Err(observation_failure(operation.path().to_path_buf()));
            }
        }
        if operation.kind == CreationOperationKind::Create
            && operation.destination_kind == CreationDestinationKind::File
        {
            let stage =
                staging_path(operation.path()).expect("planned creation files have basenames");
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
    plan: &CreationBatch,
    failure: CommitFailure,
    stage: StageCleanup<F::Error>,
    created_files: Vec<TrackedArtifact<F::Error>>,
    created_directories: Vec<TrackedArtifact<F::Error>>,
    filesystem: &F,
) -> Failure {
    Failure::commit(
        failure,
        cleanup_creation(plan, stage, created_files, created_directories, filesystem),
    )
}

fn cleanup_creation<F: FileSystem>(
    plan: &CreationBatch,
    stage: StageCleanup<F::Error>,
    mut created_files: Vec<TrackedArtifact<F::Error>>,
    mut created_directories: Vec<TrackedArtifact<F::Error>>,
    filesystem: &F,
) -> Vec<CleanupFailure> {
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
    cleanup_failures
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

/// A `Location` for one planned creation artifact.
///
/// The expectation holds by construction rather than by assertion. Every planned path is
/// the canonical root joined with one of this module's own literal managed names or with
/// the components of a configured value `validate_configured_path` has already accepted —
/// and that validator enforces the same grammar `package_relative_path_text` answers
/// against, so a value that reached here has a Package-relative spelling.
pub(crate) fn planned_location(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned creation path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned creation path is normalized")
}

pub(crate) fn create_directory(root: &Path, path: PathBuf) -> CreationOperation {
    CreationOperation {
        kind: CreationOperationKind::Create,
        destination_kind: CreationDestinationKind::Directory,
        location: planned_location(root, path),
        contents: None,
        prior: None,
    }
}

pub(crate) fn preserve_directory(root: &Path, path: PathBuf) -> CreationOperation {
    CreationOperation {
        kind: CreationOperationKind::Preserve,
        destination_kind: CreationDestinationKind::Directory,
        location: planned_location(root, path),
        contents: None,
        prior: None,
    }
}

pub(crate) fn create_file(root: &Path, path: PathBuf, contents: Vec<u8>) -> CreationOperation {
    CreationOperation {
        kind: CreationOperationKind::Create,
        destination_kind: CreationDestinationKind::File,
        location: planned_location(root, path),
        contents: Some(contents),
        prior: None,
    }
}

pub(crate) fn preserve_file(root: &Path, path: PathBuf) -> CreationOperation {
    CreationOperation {
        kind: CreationOperationKind::Preserve,
        destination_kind: CreationDestinationKind::File,
        location: planned_location(root, path),
        contents: None,
        prior: None,
    }
}

/// Replaces the file at `path`, which held `prior` when planned, with `contents`.
pub(crate) fn replace_file(
    root: &Path,
    path: PathBuf,
    prior: Vec<u8>,
    contents: Vec<u8>,
) -> CreationOperation {
    CreationOperation {
        kind: CreationOperationKind::Replace,
        destination_kind: CreationDestinationKind::File,
        location: planned_location(root, path),
        contents: Some(contents),
        prior: Some(prior),
    }
}
