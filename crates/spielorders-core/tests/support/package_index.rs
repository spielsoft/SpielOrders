#![allow(dead_code)]

use spielorders_core::ports::{
    DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Read(PathBuf),
    WriteStage(PathBuf, Vec<u8>),
    Sync(PathBuf),
    Replace(PathBuf, PathBuf),
    Remove(PathBuf),
    Canonicalize(PathBuf),
    ObserveKind(PathBuf),
    ObserveChange(PathBuf),
    ListDirectory(PathBuf),
}

impl Effect {
    pub fn paths(&self) -> Vec<&Path> {
        match self {
            Self::Read(path)
            | Self::Sync(path)
            | Self::Remove(path)
            | Self::Canonicalize(path)
            | Self::ObserveKind(path)
            | Self::ObserveChange(path)
            | Self::ListDirectory(path) => vec![path],
            Self::WriteStage(path, _) => vec![path],
            Self::Replace(source, destination) => vec![source, destination],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementBoundary {
    WriteStage,
    SyncStage,
    Replace,
    /// Durability fails and the cleanup that follows fails too, so the retained
    /// staging evidence is reported beside the initiating failure.
    RemoveStage,
}

#[derive(Default)]
pub struct TestFileSystem {
    pub entries: RefCell<BTreeMap<PathBuf, FileSystemEntryKind>>,
    pub files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    pub effects: RefCell<Vec<Effect>>,
    fail_at: RefCell<Option<ReplacementBoundary>>,
    failed_reads: RefCell<Vec<PathBuf>>,
    failed_observations: RefCell<Vec<PathBuf>>,
    failed_canonicalizations: RefCell<Vec<PathBuf>>,
    failed_lists: RefCell<Vec<PathBuf>>,
    order_enumeration: RefCell<Option<Vec<String>>>,
    allowed_reads: RefCell<Option<BTreeSet<PathBuf>>>,
    change_evidence: RefCell<BTreeMap<PathBuf, (u64, u128)>>,
    change_kind_overrides: RefCell<BTreeMap<PathBuf, Option<FileSystemEntryKind>>>,
    failed_change_observations: RefCell<Vec<PathBuf>>,
    /// A writer outside this load, replacing a file partway through the rebuild. It
    /// fires at the staging write, which is after the load assessed the index and
    /// before the rebuild renames over it.
    edit_at_staging: RefCell<Option<(PathBuf, Vec<u8>)>>,
}

impl TestFileSystem {
    pub fn minimal_package() -> Self {
        let filesystem = Self::default();
        for path in ["/package", "/package/.spielorders", "/package/Orders"] {
            filesystem
                .entries
                .borrow_mut()
                .insert(path.into(), FileSystemEntryKind::Directory);
        }
        for (path, bytes) in [
            ("/package/.spielorders/config.toml", b"".as_slice()),
            ("/package/.spielorders/version.json", VERSION),
            ("/package/.spielorders/vendors.json", VENDORS),
        ] {
            filesystem
                .entries
                .borrow_mut()
                .insert(path.into(), FileSystemEntryKind::File);
            filesystem
                .files
                .borrow_mut()
                .insert(path.into(), bytes.to_vec());
        }
        filesystem
    }

    /// Schedules an outside writer's edit for the next staging write.
    pub fn editing_at_staging(self, path: &str, bytes: &[u8]) -> Self {
        *self.edit_at_staging.borrow_mut() = Some((path.into(), bytes.to_vec()));
        self
    }

    pub fn put_file(&self, path: &str, bytes: &[u8]) {
        self.entries
            .borrow_mut()
            .insert(path.into(), FileSystemEntryKind::File);
        self.files.borrow_mut().insert(path.into(), bytes.to_vec());
    }

    pub fn put_entry(&self, path: &str, kind: FileSystemEntryKind) {
        self.entries.borrow_mut().insert(path.into(), kind);
    }

    pub fn put_entry_path(&self, path: &Path, kind: FileSystemEntryKind) {
        self.entries.borrow_mut().insert(path.into(), kind);
    }

    pub fn remove_path(&self, path: &str) {
        self.entries.borrow_mut().remove(Path::new(path));
        self.files.borrow_mut().remove(Path::new(path));
        self.change_evidence.borrow_mut().remove(Path::new(path));
    }

    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(Path::new(path)).cloned()
    }

    pub fn fail_at(&self, boundary: ReplacementBoundary) {
        *self.fail_at.borrow_mut() = Some(boundary);
    }

    pub fn fail_read(&self, path: &str) {
        self.failed_reads.borrow_mut().push(path.into());
    }

    pub fn fail_observation(&self, path: &str) {
        self.failed_observations.borrow_mut().push(path.into());
    }

    pub fn fail_canonicalize(&self, path: &str) {
        self.failed_canonicalizations.borrow_mut().push(path.into());
    }

    pub fn fail_list(&self, path: &str) {
        self.failed_lists.borrow_mut().push(path.into());
    }

    pub fn enumerate_orders_as(&self, names: &[&str]) {
        *self.order_enumeration.borrow_mut() =
            Some(names.iter().map(|name| (*name).to_owned()).collect());
    }

    pub fn allow_reads(&self, paths: &[&str]) {
        *self.allowed_reads.borrow_mut() = Some(paths.iter().map(PathBuf::from).collect());
    }

    pub fn allow_all_reads(&self) {
        *self.allowed_reads.borrow_mut() = None;
    }

    pub fn put_change_evidence(&self, path: &str, comparison_domain: u64, sequence: u128) {
        self.change_evidence
            .borrow_mut()
            .insert(path.into(), (comparison_domain, sequence));
    }

    pub fn clear_change_evidence(&self, path: &str) {
        self.change_evidence.borrow_mut().remove(Path::new(path));
    }

    pub fn override_change_kind(&self, path: &str, kind: Option<FileSystemEntryKind>) {
        self.change_kind_overrides
            .borrow_mut()
            .insert(path.into(), kind);
    }

    pub fn fail_change_observation(&self, path: &str) {
        self.failed_change_observations
            .borrow_mut()
            .push(path.into());
    }
}

impl FileSystem for TestFileSystem {
    type Error = &'static str;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.effects.borrow_mut().push(Effect::Read(path.into()));
        if self
            .allowed_reads
            .borrow()
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(path))
        {
            return Err("read exceeded strict budget");
        }
        if self
            .failed_reads
            .borrow()
            .iter()
            .any(|failed| failed == path)
        {
            return Err("injected read failure");
        }
        self.files.borrow().get(path).cloned().ok_or("missing file")
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.files
            .borrow_mut()
            .insert(path.into(), contents.to_vec());
        Ok(())
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::WriteStage(path.into(), contents.to_vec()));
        if self.fail_at.borrow().as_ref() == Some(&ReplacementBoundary::WriteStage) {
            return Err("injected staging write failure");
        }
        // The outside writer lands here: after this load read and assessed the index,
        // and before the rebuild renames its candidate over it.
        if let Some((edited, bytes)) = self.edit_at_staging.borrow_mut().take() {
            self.entries
                .borrow_mut()
                .insert(edited.clone(), FileSystemEntryKind::File);
            self.files.borrow_mut().insert(edited, bytes);
        }
        if self.files.borrow().contains_key(path) {
            return Err("staging file exists");
        }
        self.entries
            .borrow_mut()
            .insert(path.into(), FileSystemEntryKind::File);
        let staged = contents.to_vec();
        self.files.borrow_mut().insert(path.into(), staged);
        Ok(())
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.effects.borrow_mut().push(Effect::Sync(path.into()));
        if matches!(
            self.fail_at.borrow().as_ref(),
            Some(ReplacementBoundary::SyncStage | ReplacementBoundary::RemoveStage)
        ) {
            Err("injected sync failure")
        } else {
            Ok(())
        }
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Replace(source.into(), destination.into()));
        if self.fail_at.borrow().as_ref() == Some(&ReplacementBoundary::Replace) {
            return Err("injected replace failure");
        }
        let bytes = self
            .files
            .borrow_mut()
            .remove(source)
            .ok_or("missing stage")?;
        self.entries.borrow_mut().remove(source);
        self.entries
            .borrow_mut()
            .insert(destination.into(), FileSystemEntryKind::File);
        self.files.borrow_mut().insert(destination.into(), bytes);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.effects.borrow_mut().push(Effect::Remove(path.into()));
        if self.fail_at.borrow().as_ref() == Some(&ReplacementBoundary::RemoveStage) {
            return Err("injected cleanup failure");
        }
        self.entries.borrow_mut().remove(path);
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or("missing")
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Canonicalize(path.into()));
        if self
            .failed_canonicalizations
            .borrow()
            .iter()
            .any(|failed| failed == path)
        {
            Err("injected canonicalize failure")
        } else {
            Ok(path.into())
        }
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveKind(path.into()));
        if self
            .failed_observations
            .borrow()
            .iter()
            .any(|failed| failed == path)
        {
            return Err("injected kind observation failure");
        }
        Ok(self.entries.borrow().get(path).copied())
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveChange(path.into()));
        if self
            .failed_change_observations
            .borrow()
            .iter()
            .any(|failed| failed == path)
        {
            return Err("injected change observation failure");
        }
        let kind = self
            .change_kind_overrides
            .borrow()
            .get(path)
            .copied()
            .unwrap_or_else(|| self.entries.borrow().get(path).copied());
        let evidence = self
            .change_evidence
            .borrow()
            .get(path)
            .map(
                |(comparison_domain, sequence)| FileChangeEvidence::Comparable {
                    comparison_domain: *comparison_domain,
                    sequence: *sequence,
                },
            )
            .unwrap_or(FileChangeEvidence::Uncertain);
        Ok(FileChangeObservation::new(kind, evidence))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ListDirectory(path.into()));
        if self
            .failed_lists
            .borrow()
            .iter()
            .any(|failed| failed == path)
        {
            return Err("injected list failure");
        }
        if path == Path::new("/package/Orders")
            && let Some(names) = self.order_enumeration.borrow().as_ref()
        {
            return Ok(names
                .iter()
                .map(|name| DirectoryEntry::new(name, FileSystemEntryKind::Directory))
                .collect());
        }
        Ok(self
            .entries
            .borrow()
            .iter()
            .filter(|(candidate, _)| candidate.parent() == Some(path))
            .map(|(candidate, kind)| {
                DirectoryEntry::new(candidate.file_name().expect("entry has a name"), *kind)
            })
            .collect())
    }
}
