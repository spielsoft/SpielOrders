#![allow(dead_code)]

use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_diagnosis::diagnose_order;
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeEvidence, FileChangeObservation, FileSystem,
    FileSystemEntryKind, NewDirectoryRenameOutcome, NewFileLinkOutcome,
};
use spielorders_core::readme::DiagnosticCode;
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    fs::OpenOptions,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
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

impl DirectoryFileSystem for RealFileSystem {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::create_dir(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::remove_dir(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        match std::fs::hard_link(source, destination) {
            Ok(()) => Ok(NewFileLinkOutcome::Linked),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Ok(NewFileLinkOutcome::DestinationExists)
            }
            Err(error) => Err(error),
        }
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        match rename_exclusive(source, destination) {
            Ok(()) => Ok(NewDirectoryRenameOutcome::Renamed),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Ok(NewDirectoryRenameOutcome::DestinationExists)
            }
            Err(error) => Err(error),
        }
    }
}

/// Renames `source` onto an unoccupied `destination` in one filesystem operation,
/// reporting [`io::ErrorKind::AlreadyExists`] when anything already holds the name.
///
/// `std::fs::rename` cannot implement this. POSIX `rename` replaces an existing empty
/// destination directory, so checking the destination first and renaming afterwards
/// destroys a directory created inside that window — which is what a user making a
/// folder at the same name does. An exclusive rename refuses an occupied destination
/// in the kernel, which is the only form that keeps
/// [`DirectoryFileSystem::rename_new_directory`]'s promise.
///
/// A filesystem that has no exclusive rename reports [`io::ErrorKind::Unsupported`]
/// rather than an opaque errno, so it cannot be mistaken for an ordinary publication
/// failure — and never for a collision, which would tell creation to preserve evidence
/// that was never there.
#[cfg(target_vendor = "apple")]
fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    let (source, destination) = (nul_terminated(source)?, nul_terminated(destination)?);
    // SAFETY: both arguments are nul-terminated C strings that outlive the call, and
    // `renamex_np` reads them without retaining either pointer.
    let code =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    exclusive_rename_result(code)
}

/// `AT_FDCWD` is correct for both paths because this adapter is only ever given paths
/// resolved against the process working directory.
#[cfg(target_os = "linux")]
fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    let (source, destination) = (nul_terminated(source)?, nul_terminated(destination)?);
    // SAFETY: both arguments are nul-terminated C strings that outlive the call, and
    // `renameat2` reads them without retaining either pointer.
    let code = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    exclusive_rename_result(code)
}

/// Refusing is the only honest answer where this adapter cannot name an exclusive
/// rename. The adapters the shipped application uses are owned by their own milestone;
/// substituting a replacing rename here would silence the very contract these tests
/// exist to prove.
#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
fn rename_exclusive(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no exclusive directory rename is available on this platform",
    ))
}

#[cfg(any(target_vendor = "apple", target_os = "linux"))]
fn exclusive_rename_result(code: std::ffi::c_int) -> io::Result<()> {
    if code == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    // A kernel or volume without the exclusive flag rejects the call itself. Reported
    // as one honest refusal rather than an errno a caller would read as an ordinary
    // publication failure.
    match error.raw_os_error() {
        Some(libc::ENOTSUP | libc::ENOSYS | libc::EINVAL) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this filesystem has no exclusive directory rename",
        )),
        _ => Err(error),
    }
}

#[cfg(any(target_vendor = "apple", target_os = "linux"))]
fn nul_terminated(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a managed path cannot contain an interior nul byte",
        )
    })
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

/// Records every read and directory listing a capability performs so a test can
/// prove which paths it did and did not inspect. All effects are otherwise real.
#[derive(Default)]
pub struct ObservingFileSystem {
    reads: RefCell<Vec<PathBuf>>,
    listings: RefCell<Vec<PathBuf>>,
}

impl ObservingFileSystem {
    pub fn reads(&self) -> Vec<PathBuf> {
        self.reads.borrow().clone()
    }

    pub fn listings(&self) -> Vec<PathBuf> {
        self.listings.borrow().clone()
    }
}

impl FileSystem for ObservingFileSystem {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.reads.borrow_mut().push(path.to_path_buf());
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.listings.borrow_mut().push(path.to_path_buf());
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for ObservingFileSystem {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        RealFileSystem.link_new_file(source, destination)
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        RealFileSystem.rename_new_directory(source, destination)
    }
}

/// A real tree seen through a volume that resolves names without regard to ASCII
/// case, the way APFS does in its default macOS configuration.
///
/// Two spellings that differ only in case name one entry here, so a refusal that
/// depends on that equivalence is exercised wherever the suite runs rather than only
/// on a volume that happens to merge them. On ext4 the same two spellings really are
/// two directories, and a test driven through `RealFileSystem` there would pass
/// without ever reaching the condition it claims to cover.
///
/// Every effect is otherwise real. Each path below the tree is rewritten to the
/// on-disk spelling of the components that already exist; a component that does not
/// exist keeps the spelling the caller wrote, which is what such a volume records
/// when it creates one. Paths outside the tree are passed through untouched.
pub struct CaseInsensitiveVolume {
    roots: Vec<PathBuf>,
}

impl CaseInsensitiveVolume {
    pub fn over(root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        match std::fs::canonicalize(root) {
            Ok(canonical) if canonical != root => roots.push(canonical),
            _ => {}
        }
        Self { roots }
    }

    fn resolve(&self, path: &Path) -> PathBuf {
        let Some((root, relative)) = self
            .roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok().map(|rest| (root, rest)))
        else {
            return path.to_path_buf();
        };
        let mut resolved = root.clone();
        for component in relative.components() {
            match component {
                Component::Normal(name) => {
                    resolved.push(existing_spelling(&resolved, name).unwrap_or(name.to_owned()));
                }
                other => resolved.push(other.as_os_str()),
            }
        }
        resolved
    }
}

/// The spelling `directory` already holds for a name that differs only in ASCII case.
fn existing_spelling(directory: &Path, name: &OsStr) -> Option<OsString> {
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|candidate| {
            candidate
                .as_encoded_bytes()
                .eq_ignore_ascii_case(name.as_encoded_bytes())
        })
}

impl FileSystem for CaseInsensitiveVolume {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(&self.resolve(path))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(&self.resolve(path), contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write_new(&self.resolve(path), contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(&self.resolve(path))
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(&self.resolve(source), &self.resolve(destination))
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(&self.resolve(path))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(&self.resolve(path))
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(&self.resolve(path))
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(&self.resolve(path))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(&self.resolve(path))
    }
}

impl DirectoryFileSystem for CaseInsensitiveVolume {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.create_directory(&self.resolve(path))
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_directory(&self.resolve(path))
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        RealFileSystem.link_new_file(&self.resolve(source), &self.resolve(destination))
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        RealFileSystem.rename_new_directory(&self.resolve(source), &self.resolve(destination))
    }
}

/// Whether the volume holding `directory` itself resolves two spellings that differ
/// only in case to one entry.
///
/// Measured rather than assumed: case sensitivity belongs to the volume the test
/// writes on, not to the platform it runs on.
pub fn resolves_case_insensitively(directory: &Path) -> bool {
    let probe = directory.join("CaseEquivalenceProbe");
    std::fs::create_dir(&probe).expect("the case probe directory is created");
    let merged = std::fs::symlink_metadata(directory.join("caseequivalenceprobe")).is_ok();
    std::fs::remove_dir(&probe).expect("the case probe directory is removed");
    merged
}

pub struct TemporaryTree(PathBuf);

/// Distinguishes trees claimed within one process. The whole suite is one test
/// binary, so two tests that pass the same label now share a process id, and a
/// clock read is not guaranteed to separate two threads that reach it together.
static TREE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TemporaryTree {
    pub fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows the Unix epoch")
            .as_nanos();
        let sequence = TREE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "spielorders-{label}-{}-{nonce}-{sequence}",
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

/// A UUID port that hands out prepared identities in order, so a fixture's Order and
/// Vendor identifiers are the ones the test names.
pub struct QueuedUuids(RefCell<Vec<String>>);

impl QueuedUuids {
    pub fn of(values: &[&str]) -> Self {
        Self(RefCell::new(
            values.iter().rev().map(|value| value.to_string()).collect(),
        ))
    }
}

impl spielorders_core::ports::UuidGenerator for QueuedUuids {
    fn generate_v4(&mut self) -> String {
        self.0.borrow_mut().pop().expect("a prepared identity")
    }
}

/// A committed Package at its default layout, and its canonical root.
pub fn initialized_package(label: &str) -> (TemporaryTree, std::path::PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::default(),
        &RealFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = std::fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

/// One adopted Order at a known identity, for tests that need a real canonical Order
/// rather than a hand-written record.
pub fn adopted_order(root: &Path, basename: &str, uuid: &str) -> std::path::PathBuf {
    let directory = root.join("Orders").join(basename);
    std::fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    let plan = plan_order_adoption(
        root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    assert!(matches!(
        commit_order_adoption(plan, &mut QueuedUuids::of(&[uuid]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    directory
}

/// An Order Directory's canonical record.
pub fn order_record(directory: &Path) -> std::path::PathBuf {
    directory.join(".spielorders/order.json")
}

/// The handle the current diagnosis gives the case carrying `code`.
///
/// Taken from diagnosis rather than reconstructed, so a test consumes the published
/// contract instead of restating how a handle is built.
pub fn diagnosed_handle(directory: &Path, code: DiagnosticCode) -> String {
    let diagnosis = diagnose_order(directory, &RealFileSystem).expect("the Order diagnoses");
    diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == code)
        .unwrap_or_else(|| panic!("{code:?} has a repair case: {:?}", diagnosis.cases()))
        .handle()
        .to_owned()
}

/// Where an outside writer's edit lands during an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditPoint {
    /// At the next staging write — after the operation observed its destination and
    /// before it renames over it. This is the window a destination guard closes.
    StagingWrite,
    /// At the next read of a path ending in this suffix, for operations whose own
    /// reads are the only port events between observing and writing.
    ReadEndingIn(&'static str),
}

/// A filesystem that behaves normally except that one writer outside the operation
/// under test replaces a file partway through it.
///
/// Every capability suite that needed this had grown its own near-identical copy. The
/// interesting part is one line; the rest was delegation.
pub struct ConcurrentWriter {
    path: PathBuf,
    replacement: Vec<u8>,
    at: EditPoint,
    armed: RefCell<bool>,
}

impl ConcurrentWriter {
    pub fn replacing(path: &Path, replacement: &[u8], at: EditPoint) -> Self {
        Self {
            path: path.to_path_buf(),
            replacement: replacement.to_vec(),
            at,
            armed: RefCell::new(true),
        }
    }

    fn fire_if(&self, point: EditPoint) -> std::io::Result<()> {
        if self.at == point && self.armed.replace(false) {
            std::fs::write(&self.path, &self.replacement)?;
        }
        Ok(())
    }

    /// Whether the outside write actually landed. A test whose operation *succeeds*
    /// under contention cannot see the other writer's bytes afterwards — it overwrote
    /// them — so without this the contention it claims to exercise has no observer.
    pub fn fired(&self) -> bool {
        !*self.armed.borrow()
    }
}

impl FileSystem for ConcurrentWriter {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        let bytes = RealFileSystem.read(path)?;
        if let EditPoint::ReadEndingIn(suffix) = self.at
            && path.ends_with(suffix)
        {
            self.fire_if(EditPoint::ReadEndingIn(suffix))?;
        }
        Ok(bytes)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.fire_if(EditPoint::StagingWrite)?;
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
}

/// Contention is not confined to capabilities that only write files. A whole-directory
/// disposition replaces a managed record too, and its destination is reached through
/// the same staging write.
impl DirectoryFileSystem for ConcurrentWriter {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        RealFileSystem.link_new_file(source, destination)
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        RealFileSystem.rename_new_directory(source, destination)
    }
}
