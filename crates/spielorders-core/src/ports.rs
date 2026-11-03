//! Ports for effects owned outside the SpielOrders core.
//!
//! These interfaces establish substitution boundaries only: what each effect is asked and
//! what it answers with, never how a platform supplies it.

use crate::supporting_material::{ProviderAnswer, SupportingMaterialRequest};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileSystemEntryKind {
    Directory,
    File,
    SymbolicLink,
    Other,
}

impl FileSystemEntryKind {
    /// This kind's one stable spelling.
    ///
    /// Diagnostic evidence carries an observed entry kind, so a reporting adapter needs
    /// a name for it. Supplying it here rather than there is what keeps one spelling:
    /// every other enum a diagnostic carries already has an `as_str`, and this one's
    /// absence would make an adapter the second place deciding what a symbolic link is
    /// called.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
            Self::SymbolicLink => "symbolic_link",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewFileLinkOutcome {
    Linked,
    DestinationExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewDirectoryRenameOutcome {
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
    kind: Option<FileSystemEntryKind>,
    evidence: FileChangeEvidence,
}

impl FileChangeObservation {
    pub fn new(kind: Option<FileSystemEntryKind>, evidence: FileChangeEvidence) -> Self {
        Self { kind, evidence }
    }

    pub fn kind(self) -> Option<FileSystemEntryKind> {
        self.kind
    }

    pub fn evidence(self) -> FileChangeEvidence {
        self.evidence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    name: OsString,
    kind: FileSystemEntryKind,
}

impl DirectoryEntry {
    pub fn new(name: impl AsRef<OsStr>, kind: FileSystemEntryKind) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            kind,
        }
    }

    pub fn name(&self) -> &OsStr {
        &self.name
    }

    pub fn kind(&self) -> FileSystemEntryKind {
        self.kind
    }
}

/// Filesystem effect seam for managed bytes and no-follow topology inspection.
///
/// `symlink_metadata` and the kinds returned by `read_directory` describe links
/// themselves rather than their targets.
pub trait FileSystem {
    type Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error>;
    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error>;
    /// Creates a new file without replacing any existing staging evidence.
    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error>;
    /// Makes the named file's current contents durable before replacement.
    fn sync_file(&self, path: &Path) -> Result<(), Self::Error>;
    /// Atomically replaces `destination` with `source`, preserving the destination
    /// unchanged when the operation fails.
    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error>;
    fn remove_file(&self, path: &Path) -> Result<(), Self::Error>;
    /// Resolves a path to the filesystem's canonical spelling. Low-level batch
    /// replacement uses this value as destination identity before its first write.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error>;
    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error>;
    /// Observes the final entry without following a symbolic link and returns
    /// comparable in-memory change evidence when the adapter can supply it.
    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.symlink_metadata(path)
            .map(|kind| FileChangeObservation::new(kind, FileChangeEvidence::Uncertain))
    }
    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error>;
}

/// Minimal creation effects used by Package and Order mutations.
///
/// A failed creation must not leave a newly created directory. Removal applies
/// only to an empty directory selected explicitly by the caller.
pub trait DirectoryFileSystem: FileSystem {
    /// Creates one new directory, failing when anything already stands at that path.
    ///
    /// It never adopts an existing directory. A capability that undoes its own work
    /// tells the directory it created from one that was already there by whether this
    /// succeeded, so an adapter that quietly accepted an existing directory would hand
    /// that rollback a directory the user made.
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error>;
    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error>;
    /// Atomically links a staged file at an absent destination without consuming
    /// the staged name. Existing destination evidence is never replaced.
    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error>;
    /// Atomically renames a complete staging directory into discovery without
    /// replacing destination evidence that appeared concurrently.
    ///
    /// An occupied destination must be refused by the same operation that publishes.
    /// Checking the destination separately leaves a window in which a directory created
    /// at that name is silently replaced instead of reported.
    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

pub trait LocalCalendar {
    fn today(&self) -> LocalDate;
}

/// Supplies opaque UUIDv4 text; semantic identity validation begins in M1.
pub trait UuidGenerator {
    fn generate_v4(&mut self) -> String;
}

pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// The platform index Supporting Material is looked for in.
///
/// **One synchronous call that returns one settled answer**, taking the cancellation the
/// caller owns and the bound the request carries. Not a stream and not a future: running
/// the work off a user interface thread is the caller's arrangement, so no asynchronous
/// runtime enters this core or the adapters over it.
///
/// **A provider that cannot look says so rather than failing**, which is why nothing here
/// returns a `Result`: a switched-off index, an unindexed volume, refused access and a
/// refused query are states of the platform a caller can act on, and
/// [`ProviderAnswer::Unavailable`] is where each of them travels.
///
/// What a provider does *not* decide is what its hits mean. Grouping them under Orders,
/// leaving out this application's own managed documents, telling one file reported twice
/// from two files, and the order they come back in are Package rules, and
/// [`crate::supporting_material::search_supporting_material`] is where they are applied.
pub trait SupportingMaterialIndex {
    /// This provider's own name, which the answer reports.
    fn name(&self) -> &'static str;

    fn search(
        &self,
        request: &SupportingMaterialRequest<'_>,
        cancellation: &dyn Cancellation,
    ) -> ProviderAnswer;
}

/// The narrow platform Trash effect one explicit whole-Order removal may use. It
/// submits an intact directory to the platform Trash or Recycle Bin and never erases
/// content; permanent disposal remains ordinary platform trash management, and this
/// port offers no enumeration or emptying.
pub trait NativeTrash {
    type Error;

    /// Submits one complete directory, returning where the platform put it when the
    /// platform reports that at all.
    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error>;
}

pub trait NativeFiles {
    type Error;

    fn open(&self, path: &Path) -> Result<(), Self::Error>;
    fn reveal(&self, path: &Path) -> Result<(), Self::Error>;
}

// Decision sources: Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation;
// Define lookup and search behavior; Define the Rust CLI capability contract;
// Prototype the SpielOrders desktop experience; Define durable metadata schemas and version evolution.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::supporting_material::Completeness;
    use std::{convert::Infallible, path::PathBuf};

    struct FakeEffects;

    impl FileSystem for FakeEffects {
        type Error = Infallible;

        fn read(&self, _path: &Path) -> Result<Vec<u8>, Self::Error> {
            Ok(Vec::new())
        }

        fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
            Ok(())
        }

        fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
            Ok(())
        }

        fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
            Ok(())
        }

        fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
            Ok(path.to_path_buf())
        }

        fn symlink_metadata(
            &self,
            _path: &Path,
        ) -> Result<Option<FileSystemEntryKind>, Self::Error> {
            Ok(Some(FileSystemEntryKind::Directory))
        }

        fn read_directory(&self, _path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
            Ok(Vec::new())
        }
    }

    impl LocalCalendar for FakeEffects {
        fn today(&self) -> LocalDate {
            LocalDate {
                year: 2026,
                month: 7,
                day: 21,
            }
        }
    }

    impl UuidGenerator for FakeEffects {
        fn generate_v4(&mut self) -> String {
            "00000000-0000-4000-8000-000000000000".into()
        }
    }

    impl Cancellation for FakeEffects {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    impl SupportingMaterialIndex for FakeEffects {
        fn name(&self) -> &'static str {
            "fixture"
        }

        fn search(
            &self,
            _request: &SupportingMaterialRequest<'_>,
            _cancellation: &dyn Cancellation,
        ) -> ProviderAnswer {
            ProviderAnswer::Looked {
                completeness: Completeness::Complete,
                hits: Vec::new(),
                diagnostics: Vec::new(),
            }
        }
    }

    impl NativeFiles for FakeEffects {
        type Error = Infallible;

        fn open(&self, _path: &Path) -> Result<(), Self::Error> {
            Ok(())
        }

        fn reveal(&self, _path: &Path) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn accepts_all_ports(
        effects: &mut (
                 impl FileSystem<Error = Infallible>
                 + LocalCalendar
                 + UuidGenerator
                 + SupportingMaterialIndex
                 + NativeFiles<Error = Infallible>
             ),
    ) {
        let path = PathBuf::from("fixture");
        assert_eq!(effects.read(&path).unwrap(), Vec::<u8>::new());
        effects.write(&path, b"fixture").unwrap();
        assert_eq!(effects.today().year, 2026);
        assert_eq!(effects.generate_v4().len(), 36);
        effects.open(&path).unwrap();
        effects.reveal(&path).unwrap();
        let request = SupportingMaterialRequest::new(
            "fixture",
            std::time::Duration::from_secs(1),
            Vec::new(),
        );
        assert!(matches!(
            effects.search(&request, &FakeEffects),
            ProviderAnswer::Looked { .. }
        ));
    }

    #[test]
    fn one_test_double_can_substitute_for_every_core_effect() {
        accepts_all_ports(&mut FakeEffects);
    }
}
