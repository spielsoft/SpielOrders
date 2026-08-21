#![allow(dead_code)]

use spielorders_core::ports::{
    DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
};
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub mod package_index;

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        std::fs::read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        std::fs::write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        OpenOptions::new().read(true).open(path)?.sync_all()
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        std::fs::rename(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        std::fs::canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(entry_kind(&metadata.file_type()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                let evidence = metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| FileChangeEvidence::Comparable {
                        comparison_domain: 0,
                        sequence: duration.as_nanos(),
                    })
                    .unwrap_or(FileChangeEvidence::Uncertain);
                Ok(FileChangeObservation::new(
                    Some(entry_kind(&metadata.file_type())),
                    evidence,
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(
                FileChangeObservation::new(None, FileChangeEvidence::Uncertain),
            ),
            Err(error) => Err(error),
        }
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        std::fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                Ok(DirectoryEntry::new(
                    entry.file_name(),
                    entry_kind(&entry.file_type()?),
                ))
            })
            .collect()
    }
}

fn entry_kind(file_type: &std::fs::FileType) -> FileSystemEntryKind {
    if file_type.is_symlink() {
        FileSystemEntryKind::SymbolicLink
    } else if file_type.is_dir() {
        FileSystemEntryKind::Directory
    } else if file_type.is_file() {
        FileSystemEntryKind::File
    } else {
        FileSystemEntryKind::Other
    }
}

pub struct TemporaryTree(PathBuf);

impl TemporaryTree {
    pub fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "spielorders-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temporary test root is created");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryTree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("temporary test tree is removed");
    }
}
