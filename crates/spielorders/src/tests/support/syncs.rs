//! A filesystem that tells the two kinds of sync apart.
//!
//! [`ScriptedFileSystem`](super::ScriptedFileSystem) records a weaker sync as the full
//! sync it defaults to, so a test about which stages are handed to the device and which
//! are made durable needs a filesystem that records them separately. Every effect is the
//! real one.

use super::PlatformFileSystem;
use crate::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, NewFileLinkOutcome,
    RenameOutcome,
};
use std::{
    cell::RefCell,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

/// One sync a commit asked for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sync {
    /// The file's contents handed to the device.
    Contents,
    /// The file made durable, draining the device's whole cache.
    Full,
}

/// The real filesystem, recording each sync in order.
#[derive(Default)]
pub struct Syncs {
    calls: RefCell<Vec<(Sync, PathBuf)>>,
}

impl Syncs {
    /// Each sync of a path `matching` accepts, in order.
    pub fn of(&self, matching: impl Fn(&Path) -> bool) -> Vec<(Sync, PathBuf)> {
        self.calls
            .borrow()
            .iter()
            .filter(|(_, path)| matching(path))
            .cloned()
            .collect()
    }
}

impl FileSystem for Syncs {
    type Error = io::Error;

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        PlatformFileSystem.read(path)
    }
    fn write_new(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        PlatformFileSystem.write_new(path, contents)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.calls
            .borrow_mut()
            .push((Sync::Full, path.to_path_buf()));
        PlatformFileSystem.sync_file(path)
    }
    fn sync_file_contents(&self, path: &Path) -> io::Result<()> {
        self.calls
            .borrow_mut()
            .push((Sync::Contents, path.to_path_buf()));
        PlatformFileSystem.sync_file_contents(path)
    }
    fn replace_file(&self, source: &Path, destination: &Path) -> io::Result<()> {
        PlatformFileSystem.replace_file(source, destination)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        PlatformFileSystem.remove_file(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        PlatformFileSystem.canonicalize(path)
    }
    fn entry_name(&self, path: &Path) -> io::Result<Option<OsString>> {
        PlatformFileSystem.entry_name(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<Option<FileSystemEntryKind>> {
        PlatformFileSystem.symlink_metadata(path)
    }
    fn observe_file_change(&self, path: &Path) -> io::Result<FileChangeObservation> {
        PlatformFileSystem.observe_file_change(path)
    }
    fn read_directory(&self, path: &Path) -> io::Result<Vec<DirectoryEntry>> {
        PlatformFileSystem.read_directory(path)
    }
    fn create_directory(&self, path: &Path) -> io::Result<()> {
        PlatformFileSystem.create_directory(path)
    }
    fn remove_directory(&self, path: &Path) -> io::Result<()> {
        PlatformFileSystem.remove_directory(path)
    }
    fn link_new_file(&self, source: &Path, destination: &Path) -> io::Result<NewFileLinkOutcome> {
        PlatformFileSystem.link_new_file(source, destination)
    }
    fn rename_new(&self, source: &Path, destination: &Path) -> io::Result<RenameOutcome> {
        PlatformFileSystem.rename_new(source, destination)
    }
}
