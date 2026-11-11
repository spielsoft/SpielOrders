//! Package-local guidance, created only where missing and never merged into user files.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, load_package},
    creation_batch::{
        CreationBatch, CreationPrecondition, commit_creation, create_directory, create_file,
        planned_location, preserve_directory, preserve_file,
    },
    filesystem::staging_path,
    mutation::{CommitFailure, MutationReport},
    operations::{FailedOperation, FailedPath},
    paths::{ManagedDirectory, resolve_managed_directory, resolved_paths_overlap},
    ports::{FileSystem, FileSystemEntryKind},
    readme::Location,
};
use std::path::{Path, PathBuf};

/// These files are also copied verbatim into the installed application's defaults.
const GUIDANCE: &[(&str, &[u8])] = &[
    ("SKILL.md", include_bytes!("../agent-guidance/SKILL.md")),
    (
        "references/packages-and-adoption.md",
        include_bytes!("../agent-guidance/references/packages-and-adoption.md"),
    ),
    (
        "references/orders-and-items.md",
        include_bytes!("../agent-guidance/references/orders-and-items.md"),
    ),
    (
        "references/vendors-and-contacts.md",
        include_bytes!("../agent-guidance/references/vendors-and-contacts.md"),
    ),
    (
        "references/search-and-inspection.md",
        include_bytes!("../agent-guidance/references/search-and-inspection.md"),
    ),
    (
        "references/diagnostics-and-repair.md",
        include_bytes!("../agent-guidance/references/diagnostics-and-repair.md"),
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentSetupPlan {
    pub(crate) batch: CreationBatch,
    inputs: Vec<(PathBuf, Vec<u8>)>,
    managed: Vec<PathBuf>,
}

fn failed<F: FileSystem>(root: &Location, path: &Path, error: &F::Error, fs: &F) -> Failure {
    FailedOperation::filesystem(FailedPath::of(path), error, fs)
        .under(root)
        .into()
}

fn inputs<F: FileSystem>(
    package: &CurrentPackageCandidate,
    fs: &F,
) -> Result<Vec<(PathBuf, Vec<u8>)>, Failure> {
    [&package.context.config, &package.context.version]
        .into_iter()
        .map(|location| {
            let bytes = fs.read(&location.absolute_path).map_err(|error| {
                failed(&package.context.root, &location.absolute_path, &error, fs)
            })?;
            Ok((location.absolute_path.clone(), bytes))
        })
        .collect()
}

fn managed(package: &CurrentPackageCandidate) -> Vec<PathBuf> {
    [
        &package.context.orders_directory,
        &package.context.removed_orders_directory,
    ]
    .into_iter()
    .flatten()
    .map(|location| location.absolute_path.clone())
    .collect()
}

/// Refuse both literal overlap and aliases the actual filesystem resolves. Missing
/// directories are checked again immediately after creation, before publishing files.
fn check_overlap<F: FileSystem>(
    root: &Path,
    path: &Path,
    managed: &[PathBuf],
    fs: &F,
) -> Result<(), Failure> {
    let root_location = planned_location(root, root.to_path_buf());
    for directory in managed {
        let resolved = |path| {
            resolve_managed_directory(path, fs)
                .map_err(|(_, error)| failed(&root_location, path, &error, fs))
        };
        let overlaps = resolved_paths_overlap(path, directory)
            || matches!((resolved(path)?, resolved(directory)?),
                (ManagedDirectory::Resolved(left), ManagedDirectory::Resolved(right))
                    if resolved_paths_overlap(&left, &right));
        if overlaps {
            return Err(Failure::DestinationCollision {
                root: root_location,
                location: planned_location(root, path.to_path_buf()),
                found: fs.symlink_metadata(path).map_err(|error| {
                    failed(
                        &planned_location(root, root.to_path_buf()),
                        path,
                        &error,
                        fs,
                    )
                })?,
            });
        }
    }
    Ok(())
}

pub(crate) fn plan_agent_setup<F: FileSystem>(selected: &Path, fs: &F) -> Answer<AgentSetupPlan> {
    Answer::of(|| {
        let first = load_package(selected, fs)?;
        let before = inputs(&first, fs)?;
        let package = load_package(selected, fs)?;
        let after = inputs(&package, fs)?;
        if before != after {
            return Err(Failure::PreconditionsChanged {
                location: package.context.config.clone(),
            });
        }
        let root = package.context.root.absolute_path.clone();
        let managed = managed(&package);
        let mut operations = Vec::new();
        for relative in [
            ".agents",
            ".agents/skills",
            ".agents/skills/spielorders",
            ".agents/skills/spielorders/references",
        ] {
            let path = root.join(relative);
            check_overlap(&root, &path, &managed, fs)?;
            match fs
                .symlink_metadata(&path)
                .map_err(|error| failed(&package.context.root, &path, &error, fs))?
            {
                None => operations.push(create_directory(&root, path)),
                Some(FileSystemEntryKind::Directory) => {
                    operations.push(preserve_directory(&root, path))
                }
                Some(actual) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: planned_location(&root, path),
                        found: Some(actual),
                    });
                }
            }
        }
        for (relative, bytes) in GUIDANCE {
            let path = root.join(".agents/skills/spielorders").join(relative);
            match fs
                .symlink_metadata(&path)
                .map_err(|error| failed(&package.context.root, &path, &error, fs))?
            {
                None => {
                    let stage = staging_path(&path).expect("guidance files have basenames");
                    if let Some(actual) = fs
                        .symlink_metadata(&stage)
                        .map_err(|error| failed(&package.context.root, &stage, &error, fs))?
                    {
                        return Err(Failure::DestinationCollision {
                            root: package.context.root.clone(),
                            location: planned_location(&root, stage),
                            found: Some(actual),
                        });
                    }
                    operations.push(create_file(&root, path, bytes.to_vec()));
                }
                Some(FileSystemEntryKind::File) => operations.push(preserve_file(&root, path)),
                Some(actual) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: planned_location(&root, path),
                        found: Some(actual),
                    });
                }
            }
        }
        Ok(AgentSetupPlan {
            batch: CreationBatch {
                root: root.clone(),
                operations,
                preconditions: vec![
                    CreationPrecondition {
                        path: root,
                        expected: Some(FileSystemEntryKind::Directory),
                    },
                    CreationPrecondition {
                        path: package.context.metadata_directory.absolute_path,
                        expected: Some(FileSystemEntryKind::Directory),
                    },
                    CreationPrecondition {
                        path: package.context.config.absolute_path,
                        expected: Some(FileSystemEntryKind::File),
                    },
                    CreationPrecondition {
                        path: package.context.version.absolute_path,
                        expected: Some(FileSystemEntryKind::File),
                    },
                ],
            },
            inputs: after,
            managed,
        })
    })
}

pub(crate) fn commit_agent_setup<F: FileSystem>(
    plan: AgentSetupPlan,
    fs: &F,
) -> Answer<MutationReport<()>> {
    Answer::of(|| {
        let package = load_package(&plan.batch.root, fs)?;
        if inputs(&package, fs)? != plan.inputs || managed(&package) != plan.managed {
            return Err(Failure::PreconditionsChanged {
                location: package.context.config,
            });
        }
        for operation in &plan.batch.operations {
            check_overlap(&plan.batch.root, operation.path(), &plan.managed, fs)?;
        }
        let root = plan.batch.root.clone();
        commit_creation(plan.batch, fs, |directory, fs| {
            check_overlap(&root, directory, &plan.managed, fs)
                .err()
                .map(|_| CommitFailure::new(planned_location(&root, directory.to_path_buf())))
        })
        .result
    })
}
