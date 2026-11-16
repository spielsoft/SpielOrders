//! Ports for effects owned outside the SpielOrders core.
//!
//! These interfaces establish substitution boundaries only: what each effect is asked and
//! what it answers with, never how a platform supplies it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileSystemEntryKind {
    Directory,
    File,
    SymbolicLink,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewFileLinkOutcome {
    Linked,
    DestinationExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenameOutcome {
    Renamed,
    DestinationExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileChangeEvidence {
    Comparable {
        comparison_domain: u64,
        sequence: u128,
    },
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileChangeObservation {
    pub kind: Option<FileSystemEntryKind>,
    pub evidence: FileChangeEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    pub name: OsString,
    pub kind: FileSystemEntryKind,
}

/// Filesystem effect seam for managed bytes and no-follow topology inspection.
///
/// `symlink_metadata` and the kinds returned by `read_directory` describe links
/// themselves rather than their targets.
pub trait FileSystem {
    /// An effect failure, which the core reports as its text.
    type Error: std::fmt::Display;

    /// Whether the effect reports an access denial rather than another I/O failure.
    fn is_access_denied(&self, _error: &Self::Error) -> bool {
        false
    }
    /// Whether the effect reports that this filesystem cannot perform the operation at
    /// all, such as an exclusive rename on a volume that has none.
    fn is_unsupported(&self, _error: &Self::Error) -> bool {
        false
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error>;
    /// Creates a new file without replacing any existing staging evidence.
    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error>;
    /// Makes the named file's current contents durable before replacement.
    fn sync_file(&self, path: &Path) -> Result<(), Self::Error>;
    /// Hands the named file's contents to the device without waiting for the device's
    /// own cache, which the next [`Self::sync_file`] of any file on it then makes durable.
    /// An adapter with no weaker sync makes it durable here.
    fn sync_file_contents(&self, path: &Path) -> Result<(), Self::Error> {
        self.sync_file(path)
    }
    /// The Package at the canonical `root` as this seam already loaded it, where nothing
    /// has been written through it since. A seam that keeps no such memory has none.
    fn remembered_package(
        &self,
        _root: &Path,
    ) -> Option<crate::canonical::CurrentPackageCandidate> {
        None
    }
    /// Keeps `package` for [`Self::remembered_package`] until the next write.
    fn remember_package(&self, _package: &crate::canonical::CurrentPackageCandidate) {}
    /// Atomically replaces `destination` with `source`, preserving the destination
    /// unchanged when the operation fails.
    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error>;
    fn remove_file(&self, path: &Path) -> Result<(), Self::Error>;
    /// Resolves a path to the filesystem's canonical spelling. Low-level batch
    /// replacement uses this value as destination identity before its first write.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error>;
    /// The filesystem's spelling of an existing entry's final component, without
    /// following a final symbolic link. An absent entry returns None; names of
    /// distinct hard links remain distinct even though their file identity is shared.
    fn entry_name(&self, path: &Path) -> Result<Option<OsString>, Self::Error>;
    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error>;
    /// Observes the final entry without following a symbolic link and returns
    /// comparable in-memory change evidence when the adapter can supply it.
    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.symlink_metadata(path)
            .map(|kind| FileChangeObservation {
                kind,
                evidence: FileChangeEvidence::Uncertain,
            })
    }
    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error>;
    /// Creates one new directory, failing when anything already stands at that path. A
    /// failed creation leaves no newly created directory.
    ///
    /// It never adopts an existing directory. A capability that undoes its own work
    /// tells the directory it created from one that was already there by whether this
    /// succeeded, so an adapter that quietly accepted an existing directory would hand
    /// that rollback a directory the user made.
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error>;
    /// Removes one empty directory the caller selected explicitly.
    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error>;
    /// Atomically links a staged file at an absent destination without consuming
    /// the staged name. Existing destination evidence is never replaced.
    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error>;
    /// Atomically moves one file, directory, or symbolic link to an absent name,
    /// without following links or replacing destination evidence.
    ///
    /// An occupied destination must be refused by the same operation that publishes.
    /// Checking the destination separately leaves a window in which an entry created
    /// at that name is silently replaced instead of reported.
    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Every effect a capability draws on, supplied together by the caller.
///
/// **One bundle rather than one argument per effect**, so a caller hands each capability
/// the same value and the core decides which effects it needs: today's date only where an
/// Order enters `ordered` without one, and a new identity only where a commit mints one.
pub trait Effects {
    type FileSystem: FileSystem;

    fn file_system(&self) -> &Self::FileSystem;

    /// The user's local calendar date, read when a mutation needs it.
    fn today(&self) -> LocalDate;

    /// Opaque UUIDv4 text. The core validates it before any record carries it.
    fn new_uuid(&self) -> String;
}

/// A caller's withdrawal of the question a long-running call is answering.
pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// A flag another thread sets to cancel.
impl Cancellation for std::sync::atomic::AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(std::sync::atomic::Ordering::SeqCst)
    }
}
