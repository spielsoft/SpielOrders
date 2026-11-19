//! Agent guidance written into any directory: each guidance file this application ships is
//! created where missing and replaced where it differs, so a requested refresh always leaves
//! the current guidance. Nothing else is read or written, and Package metadata plays no part.

use crate::{
    Answer, Failure,
    change::PlannedAct,
    filesystem::staging_path,
    journal::{CreationAct, Expect, Journal, Subject, planned_location},
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentSetupPlan {
    pub(crate) journal: Journal,
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
        let act = PlannedAct::Creation;
        let mut journal = Journal::new(&root);
        journal.require(root.clone(), Some(FileSystemEntryKind::Directory));
        for relative in DIRECTORIES {
            let path = root.join(relative);
            match observe(&path)? {
                None => journal.create_directory(path, act(CreationAct::CreateDirectory)),
                Some(FileSystemEntryKind::Directory) => journal.preserve(
                    path,
                    FileSystemEntryKind::Directory,
                    act(CreationAct::PreserveDirectory),
                ),
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
                    journal.create_file(path, bytes.to_vec(), act(CreationAct::CreateFile));
                }
                Some(FileSystemEntryKind::File) => {
                    let current = fs.read(&path).map_err(|error| failed(&path, &error))?;
                    if current == *bytes {
                        journal.preserve(
                            path,
                            FileSystemEntryKind::File,
                            act(CreationAct::PreserveFile),
                        );
                    } else {
                        journal.replace(
                            path,
                            bytes.to_vec(),
                            Expect::Bytes(current),
                            act(CreationAct::ReplaceFile),
                            Subject::Other,
                        );
                    }
                }
                found => return Err(collision(path, found)),
            }
        }
        journal.require_planned();
        Ok(AgentSetupPlan { journal })
    })
}

pub(crate) fn commit_agent_setup<F: FileSystem>(
    plan: AgentSetupPlan,
    fs: &F,
) -> Answer<MutationReport<()>> {
    Answer::of(|| Ok(plan.journal.run(fs)?.report(())))
}
