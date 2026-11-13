//! Installed resources beside the executable under one installation prefix:
//! `<prefix>/bin/spielorders` and `<prefix>/share/spielorders/agent-guidance`.

use crate::response::Answer;
use serde_json::json;
use std::{io, path::PathBuf};

pub(crate) fn defaults() -> Answer {
    match directory().and_then(|directory| {
        // Query the actual installed tree, not the build machine's source directory.
        let skill = directory.join("SKILL.md");
        if !skill.is_file() || !directory.join("references").is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Default guidance is not installed at {}",
                    directory.display()
                ),
            ));
        }
        let canonical = directory.canonicalize()?;
        canonical.to_str().map(str::to_owned).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "The resource location cannot be represented as UTF-8",
            )
        })
    }) {
        Ok(directory) => Answer::Answered {
            data: json!({ "skill_directory": directory }),
            findings: Vec::new(),
        },
        Err(error) => Answer::Failed {
            error: json!({ "code": "resources_unavailable", "detail": error.to_string() }),
            findings: Vec::new(),
        },
    }
}

fn directory() -> io::Result<PathBuf> {
    // A command on PATH may be a link to the installed executable; its prefix is the target's.
    let executable = std::env::current_exe()?.canonicalize()?;
    let prefix = executable
        .parent()
        .and_then(|bin| bin.parent())
        .ok_or_else(|| io::Error::other("The executable has no installation prefix"))?;
    Ok(prefix.join("share/spielorders/agent-guidance"))
}
