//! The production filesystem, clock, and identity effects of the SpielOrders library.
//!
//! Implements the core filesystem and effects-bundle ports with ordinary filesystem
//! system calls and nothing else: no OS-native service, so the library ports to another
//! OS unchanged. The per-OS code here is the durable-write calls the
//! [durable writes](../../../docs/architecture/durable-writes.md) contract needs: a
//! full flush and an exclusive rename.
//!
//! Decision sources: Effect and adapter boundaries; Durable managed-file writes.

#![cfg_attr(
    not(test),
    deny(
        clippy::dbg_macro,
        clippy::exit,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stderr,
        clippy::print_stdout,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

use crate::ports::{
    DirectoryEntry, Effects, FileChangeEvidence, FileChangeObservation, FileSystem,
    FileSystemEntryKind, LocalDate, NewFileLinkOutcome, RenameOutcome,
};
use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
};

/// The platform filesystem, as the core's `FileSystem`.
///
/// It is a unit type because it holds no state: every operation names an absolute path
/// the core resolved, and nothing about one call conditions the next.
pub struct PlatformFileSystem;

impl FileSystem for PlatformFileSystem {
    type Error = io::Error;

    fn is_access_denied(&self, error: &Self::Error) -> bool {
        error.kind() == io::ErrorKind::PermissionDenied
    }

    fn is_unsupported(&self, error: &Self::Error) -> bool {
        error.kind() == io::ErrorKind::Unsupported
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        std::fs::read(path)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        let file = OpenOptions::new().read(true).open(path)?;
        full_sync(&file)
    }

    /// `fsync`, which on Apple platforms leaves the drive's cache to the next
    /// `F_FULLFSYNC`; elsewhere it is the full sync itself.
    fn sync_file_contents(&self, path: &Path) -> Result<(), Self::Error> {
        let file = OpenOptions::new().read(true).open(path)?;
        contents_sync(&file)
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

    fn entry_name(&self, path: &Path) -> Result<Option<OsString>, Self::Error> {
        entry_name(path)
    }

    /// An absent entry is an answer; every other failure is a failure.
    ///
    /// The distinction is load-bearing and narrower than it looks. The core decides a
    /// directory is unmanaged from `None` here, so widening this to swallow, say, a
    /// path whose parent is a plain file would report an ordinary file as an
    /// uninitialized Package and offer to initialize it.
    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(entry_kind(&metadata.file_type()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Status-change time as the comparable sequence the derived Package Index proves
    /// itself current against, within the device that holds the entry.
    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                let evidence = status_change(&metadata);
                Ok(FileChangeObservation {
                    kind: Some(entry_kind(&metadata.file_type())),
                    evidence,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FileChangeObservation {
                kind: None,
                evidence: FileChangeEvidence::Uncertain,
            }),
            Err(error) => Err(error),
        }
    }

    /// `DirEntry::file_type` does not follow a symbolic link on this platform, which is
    /// what discovery requires: a link is reported as a link rather than as whatever it
    /// points at.
    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        std::fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                Ok(DirectoryEntry {
                    name: entry.file_name(),
                    kind: entry_kind(&entry.file_type()?),
                })
            })
            .collect()
    }

    /// `create_dir` rather than `create_dir_all`: this must fail when anything already
    /// stands at the path, because a capability that undoes its own work tells the
    /// directory it created from one that was already there by whether this succeeded.
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::create_dir(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::remove_dir(path)
    }

    /// A hard link keeps both halves of the port's promise in one operation: it refuses
    /// an occupied destination in the kernel, and it does not consume the staged name.
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

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error> {
        match rename_exclusive(source, destination) {
            Ok(()) => Ok(RenameOutcome::Renamed),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Ok(RenameOutcome::DestinationExists)
            }
            Err(error) => Err(error),
        }
    }
}

/// Every production effect a core mutation draws on, as the core's [`Effects`] bundle.
///
/// A unit type for the reason [`PlatformFileSystem`] is one: there is one filesystem, one
/// system clock and time zone, and one random-number source, so nothing about one call
/// conditions the next.
///
/// **The local date is not the UTC date**, and it is not derivable from
/// `SystemTime::now()` alone: turning an instant into a calendar date needs this
/// machine's current offset, which needs the platform's time-zone database. `std` exposes
/// neither, which is why the date has a dependency behind it. A machine whose time zone
/// cannot be determined at all leaves the library reading the instant as UTC, which is
/// the only calendar left to read it in. It is read at the moment a mutation needs it, so
/// a process that outlived midnight would not stamp yesterday.
///
/// **The core validates the identities this mints.** A version-4 UUID is randomness and a
/// version nibble; a generator that produced something that is not an identity has no
/// channel of its own to say so, so the mutation that asked for one refuses instead.
pub struct PlatformEffects;

impl Effects for PlatformEffects {
    type FileSystem = PlatformFileSystem;

    fn file_system(&self) -> &PlatformFileSystem {
        &PlatformFileSystem
    }

    fn today(&self) -> LocalDate {
        let today = jiff::Zoned::now().date();
        LocalDate {
            year: i32::from(today.year()),
            // A month is `1..=12` and a day of the month `1..=31`, which the library's own
            // types guarantee, so a total conversion loses nothing here. `unsigned_abs`
            // rather than `try_from`: this boundary has no channel to report a failure and
            // no date to fall back to, so a conversion that cannot fail is the honest one.
            month: today.month().unsigned_abs(),
            day: today.day().unsigned_abs(),
        }
    }

    fn new_uuid(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Makes a file's current contents durable as far as this platform can be asked to.
///
/// On Apple platforms `fsync` is documented to return once the data has reached the
/// drive, without waiting for the drive to commit it to the medium; `F_FULLFSYNC` is
/// the request that waits. The port's word is "durable", and a managed-metadata
/// replacement is exactly the write a user expects to survive losing power, so the
/// stronger request is the honest one to make. A volume that has no such request
/// rejects the call, and the ordinary flush is then all this platform offers.
///
/// `EINVAL` counts as such a rejection here, which is the opposite reading from the one
/// [`RENAME_UNSUPPORTED`] takes for `renamex_np`, and deliberately so: `fcntl` is passed
/// no argument that could be invalid, so an invalid *argument* is not a thing this call
/// can have. The only thing left for the kernel to be objecting to is the command on
/// this descriptor's volume. `renamex_np` takes two paths, which is why `EINVAL` there
/// can mean the caller's rename rather than the volume's capability.
///
/// Falling back is safe in a way the rename case is not: `sync_all` is a weaker flush,
/// not a different operation, and it is what this platform would have done had the
/// stronger request never been made.
#[cfg(target_vendor = "apple")]
fn full_sync(file: &std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // SAFETY: the descriptor is owned by `file`, which outlives the call, and
    // `F_FULLFSYNC` takes no argument.
    let code = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) };
    if code != -1 {
        return Ok(());
    }
    // Bound once. `last_os_error` reads a thread-local the standard library is free to
    // set on its own, so reading it twice risks reporting an error other than the one
    // that was just matched on.
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOTSUP | libc::ENOTTY | libc::EINVAL) => file.sync_all(),
        _ => Err(error),
    }
}

#[cfg(not(target_vendor = "apple"))]
fn full_sync(file: &std::fs::File) -> io::Result<()> {
    file.sync_all()
}

/// Plain `fsync`. The standard library's `sync_all` is `F_FULLFSYNC` on Apple platforms,
/// which is the full sync this is the weaker sibling of.
#[cfg(target_vendor = "apple")]
fn contents_sync(file: &std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // SAFETY: the descriptor is owned by `file`, which outlives the call.
    if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_vendor = "apple"))]
fn contents_sync(file: &std::fs::File) -> io::Result<()> {
    file.sync_all()
}

/// Renames `source` onto an unoccupied `destination` in one filesystem operation,
/// reporting [`io::ErrorKind::AlreadyExists`] when anything already holds the name.
///
/// `std::fs::rename` cannot implement this. POSIX `rename` replaces an existing empty
/// destination directory, so checking the destination first and renaming afterwards
/// destroys a directory created inside that window — which is what a user making a
/// folder at the same name does. An exclusive rename refuses an occupied destination
/// in the kernel, which is the only form that keeps
/// [`FileSystem::rename_new`]'s promise.
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

/// `AT_FDCWD` is correct for both paths because this adapter is only ever given
/// absolute paths: the core canonicalizes the selected root through
/// [`FileSystem::canonicalize`] before anything else, and builds every managed path from
/// that canonical root. A relative path therefore never reaches this call — and if one
/// somehow did, resolving it against the process working directory is what a plain
/// `rename` would do.
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
/// rename. Substituting a replacing rename would silence the contract
/// `docs/architecture/durable-writes.md` says every shipped adapter owes.
#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
fn rename_exclusive(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no exclusive rename is available on this platform",
    ))
}

/// The errno values that mean this kernel or volume refused the exclusive flag itself.
///
/// `renamex_np` documents `ENOTSUP` for a filesystem that does not support the
/// operation, so `EINVAL` there is what it says it is: an invalid rename, such as one
/// that would move a directory inside itself. Reporting that as "unsupported" would
/// misattribute a caller's error to the volume.
#[cfg(target_vendor = "apple")]
const RENAME_UNSUPPORTED: &[std::ffi::c_int] = &[libc::ENOTSUP, libc::ENOSYS];

/// `renameat2` overloads `EINVAL`: the kernel uses it both for a filesystem that does
/// not support one of the flags and for a genuinely invalid rename. The two are
/// indistinguishable from the errno alone, so `EINVAL` is read here as the unsupported
/// case, and the message below says the reading is uncertain rather than asserting a
/// fact about the volume.
///
/// The safety property does not depend on which it was: neither case is a collision, so
/// nothing can mistake it for one and tell creation to preserve evidence that was never
/// there. What is at stake is only the accuracy of the sentence.
#[cfg(target_os = "linux")]
const RENAME_UNSUPPORTED: &[std::ffi::c_int] = &[libc::ENOTSUP, libc::ENOSYS, libc::EINVAL];

#[cfg(target_vendor = "apple")]
const RENAME_UNSUPPORTED_MESSAGE: &str = "this filesystem has no exclusive rename";

#[cfg(target_os = "linux")]
const RENAME_UNSUPPORTED_MESSAGE: &str = "this filesystem has no exclusive rename, or the rename itself was \
     invalid — this kernel reports both the same way";

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
        Some(errno) if RENAME_UNSUPPORTED.contains(&errno) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            RENAME_UNSUPPORTED_MESSAGE,
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

/// The entry's status-change time (ctime), comparable only within its device.
///
/// Every write, rename, copy, restore, sync, permission change, and timestamp change sets
/// it to the present, and no ordinary tool sets it back, so a record changed after the
/// index was written cannot look older than the index. Modification time is not this
/// evidence: restores, copies, and syncs preserve it.
#[cfg(unix)]
fn status_change(metadata: &std::fs::Metadata) -> FileChangeEvidence {
    use std::os::unix::fs::MetadataExt;
    match (
        u128::try_from(metadata.ctime()),
        u128::try_from(metadata.ctime_nsec()),
    ) {
        (Ok(seconds), Ok(nanoseconds)) => FileChangeEvidence::Comparable {
            comparison_domain: metadata.dev(),
            sequence: seconds * 1_000_000_000 + nanoseconds,
        },
        _ => FileChangeEvidence::Uncertain,
    }
}

#[cfg(not(unix))]
fn status_change(_metadata: &std::fs::Metadata) -> FileChangeEvidence {
    FileChangeEvidence::Uncertain
}

#[cfg(test)]
mod tests;

// The name belongs to the directory entry, not its inode or a symlink's target.
// ATTR_CMN_FULLPATH is deliberately not requested: Apple documents ambiguous
// results for hard links there. ATTR_CMN_NAME retains the selected link's name.
#[cfg(target_vendor = "apple")]
fn entry_name(path: &Path) -> io::Result<Option<OsString>> {
    use std::os::unix::ffi::OsStringExt;
    let path = nul_terminated(path)?;
    let mut request = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_NAME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buffer = [0u8; 4096];
    // SAFETY: the request and buffer are writable and live through the call; path
    // is NUL terminated. NOFOLLOW requests the entry, including a dangling link.
    let status = unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&raw mut request).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            libc::FSOPT_NOFOLLOW,
        )
    };
    if status != 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid directory-entry name");
    let size = u32::from_ne_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    let offset = i32::from_ne_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
    let length = u32::from_ne_bytes([buffer[8], buffer[9], buffer[10], buffer[11]]) as usize;
    let start = usize::try_from(offset)
        .ok()
        .and_then(|offset| offset.checked_add(4))
        .ok_or_else(invalid)?;
    let end = start.checked_add(length).ok_or_else(invalid)?;
    if size > buffer.len() || start < 12 || end > size || length < 2 || buffer[end - 1] != 0 {
        return Err(invalid());
    }
    Ok(Some(OsString::from_vec(buffer[start..end - 1].to_vec())))
}

// On a case-folding volume (ext4 casefold, or a case-insensitive mount) the dentry,
// and so procfs, may carry the spelling of the first lookup rather than the name on
// disk. A merge then plans a differently spelled name as a new entry; its exclusive
// move into the existing one refuses, and the merge rolls back with nothing lost.
// This is untested in CI.
#[cfg(target_os = "linux")]
fn entry_name(path: &Path) -> io::Result<Option<OsString>> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let entry = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(entry) => entry,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    // O_PATH holds the directory entry without reading a file or following its final
    // symlink. procfs reports this descriptor's entry path, keeping hard-link names.
    let resolved = std::fs::read_link(format!("/proc/self/fd/{}", entry.as_raw_fd()))?;
    resolved
        .file_name()
        .map(|name| Some(name.to_owned()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing directory-entry name"))
}

#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
fn entry_name(_path: &Path) -> io::Result<Option<OsString>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory-entry name lookup is unavailable",
    ))
}
