//! Agent guidance written into any directory: each guidance file this application ships is
//! created where missing and replaced where it differs, so a requested refresh always leaves
//! the current guidance. Nothing else is read or written, and Package metadata plays no part.

use crate::{
    Answer, Failure,
    creation_batch::{
        CreationBatch, CreationPrecondition, commit_creation, create_directory, create_file,
        planned_location, preserve_directory, preserve_file, replace_file,
    },
    filesystem::staging_path,
    mutation::MutationReport,
    operations::{FailedOperation, FailedPath},
    ports::{FileSystem, FileSystemEntryKind},
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

const DIRECTORIES: [&str; 4] = [
    ".agents",
    ".agents/skills",
    ".agents/skills/spielorders",
    ".agents/skills/spielorders/references",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentSetupPlan {
    pub(crate) batch: CreationBatch,
}

/// The caller has already refused a linked `selected` root.
pub(crate) fn plan_agent_setup<F: FileSystem>(selected: &Path, fs: &F) -> Answer<AgentSetupPlan> {
    Answer::of(|| {
        let failed = |path: &Path, error: &F::Error| -> Failure {
            FailedOperation::filesystem(FailedPath::of(path), error, fs).into()
        };
        if fs
            .symlink_metadata(selected)
            .map_err(|error| failed(selected, &error))?
            != Some(FileSystemEntryKind::Directory)
        {
            return Err(Failure::RootNotOrdinaryDirectory);
        }
        let root = fs
            .canonicalize(selected)
            .map_err(|error| failed(selected, &error))?;
        let observe = |path: &Path| {
            fs.symlink_metadata(path)
                .map_err(|error| failed(path, &error))
        };
        let collision = |path: PathBuf, found| Failure::DestinationCollision {
            root: planned_location(&root, root.clone()),
            location: planned_location(&root, path),
            found,
        };
        let mut operations = Vec::new();
        for relative in DIRECTORIES {
            let path = root.join(relative);
            match observe(&path)? {
                None => operations.push(create_directory(&root, path)),
                Some(FileSystemEntryKind::Directory) => {
                    operations.push(preserve_directory(&root, path))
                }
                found => return Err(collision(path, found)),
            }
        }
        for (relative, bytes) in GUIDANCE {
            let path = root.join(".agents/skills/spielorders").join(relative);
            match observe(&path)? {
                None => {
                    let stage = staging_path(&path).expect("guidance files have basenames");
                    if let Some(found) = observe(&stage)? {
                        return Err(collision(stage, Some(found)));
                    }
                    operations.push(create_file(&root, path, bytes.to_vec()));
                }
                Some(FileSystemEntryKind::File) => {
                    let current = fs.read(&path).map_err(|error| failed(&path, &error))?;
                    operations.push(if current == *bytes {
                        preserve_file(&root, path)
                    } else {
                        replace_file(&root, path, current, bytes.to_vec())
                    });
                }
                found => return Err(collision(path, found)),
            }
        }
        Ok(AgentSetupPlan {
            batch: CreationBatch {
                preconditions: vec![CreationPrecondition {
                    path: root.clone(),
                    expected: Some(FileSystemEntryKind::Directory),
                }],
                root,
                operations,
            },
        })
    })
}

pub(crate) fn commit_agent_setup<F: FileSystem>(
    plan: AgentSetupPlan,
    fs: &F,
) -> Answer<MutationReport<()>> {
    commit_creation(plan.batch, fs, |_, _| None)
}
