//! Installed resources: relative to a macOS helper bundle, or a Unix installation prefix.

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
    let executable = std::env::current_exe()?;
    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::other("The executable has no installation directory"))?;
    if cfg!(target_os = "macos") {
        // The Rust executable is Contents/Helpers/spielorders-backend in the CLI bundle.
        Ok(parent
            .parent()
            .ok_or_else(|| io::Error::other("The helper has no bundle directory"))?
            .join("Resources/agent-guidance"))
    } else {
        // A bin/ executable and share/ resources share their installation prefix.
        Ok(parent
            .parent()
            .ok_or_else(|| io::Error::other("The executable has no installation prefix"))?
            .join("share/spielorders/agent-guidance"))
    }
}
