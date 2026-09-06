//! The one boundary at which this executable implements production effects.
//!
//! Everything the `spielorders` process learns from outside itself enters here: the
//! platform filesystem, the process arguments, the environment, and whether a stream is
//! an interactive terminal. No other module in this crate names `fs::` or `env::`.
//!
//! *Implements*, not *constructs*: a command hands a core capability
//! `&PlatformFileSystem` at the call site, because that is where the capability is
//! invoked. That is not a second boundary — the type is a unit struct with no state and
//! no configuration, so there is nothing about the value for two call sites to disagree
//! about. An effect with state to configure would need a single place to configure it,
//! and this is the module that place would be.
//!
//! One boundary, because an effect built inside a command module is a second boundary
//! nobody notices until the two disagree. `NativeTrash` arrived here when
//! `order remove --delete` first needed it, rather than beside the command that needed
//! it.
//!
//! **The environment half is held; the filesystem half is not.** The `std::env` reads
//! that could carry a caller's preference are named in `clippy.toml`, and the two this
//! executable takes are permitted here one at a time, by name, so a third has to be
//! argued for where it is taken — which is how the decided absences beside it stay
//! absent, the pager and `COLUMNS` and a configuration file of the CLI's own among them.
//! The filesystem has no such closed surface, so no config fences it, and a Package read
//! through a second boundary produces the same bytes as one read through this, so no
//! process can see the difference either. A reader is what holds `fs::` to this module,
//! and nothing else does.
//!
//! `spielorders-core`'s own test suite carries a `RealFileSystem` double over
//! `std::fs`. That one is a test double for core's suite and deliberately not this
//! adapter: `docs/architecture/boundaries.md` puts production behavior outside the
//! core, so the shipped boundary belongs to the executable. Where this adapter differs
//! from that double, the difference is commented.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology; Define integrity, validation, and repair boundaries.

#[cfg(target_vendor = "apple")]
use objc2_foundation::{NSFileManager, NSURL};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeEvidence, FileChangeObservation, FileSystem,
    FileSystemEntryKind, LocalCalendar, LocalDate, NativeTrash, NewDirectoryRenameOutcome,
    NewFileLinkOutcome, UuidGenerator,
};
use std::{
    ffi::{OsStr, OsString},
    fs::OpenOptions,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// The platform filesystem, as the core's `FileSystem` and `DirectoryFileSystem`.
///
/// It is a unit type because it holds no state: every operation names an absolute path
/// the core resolved, and nothing about one call conditions the next.
pub struct PlatformFileSystem;

impl FileSystem for PlatformFileSystem {
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
        let file = OpenOptions::new().read(true).open(path)?;
        full_sync(&file)
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

    /// Modification time as the comparable sequence the derived Package Index proves
    /// itself current against.
    ///
    /// Every observation this adapter makes is one wall-clock modification time of the
    /// same kind, so they all share one comparison domain and the constant `0` names
    /// it. Two files on different volumes are still both wall-clock readings; what
    /// separates comparable evidence from uncertain evidence here is whether a
    /// timestamp could be read at all, not which volume it came from.
    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                let evidence = metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .map(|since_epoch| FileChangeEvidence::Comparable {
                        comparison_domain: 0,
                        sequence: since_epoch.as_nanos(),
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

    /// `DirEntry::file_type` does not follow a symbolic link on this platform, which is
    /// what discovery requires: a link is reported as a link rather than as whatever it
    /// points at.
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

impl DirectoryFileSystem for PlatformFileSystem {
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

/// The platform Trash, as the core's [`NativeTrash`].
///
/// A unit type for the reason [`PlatformFileSystem`] is one: there is one Trash for the
/// user this process runs as, and nothing about one submission conditions the next.
///
/// **The mechanism is the platform's own request to trash an item**, which is the only
/// one that keeps both halves of the port's promise in a single operation. On Apple
/// platforms that is `NSFileManager`'s `trashItemAtURL:resultingItemURL:error:`: it moves
/// the item whole, it reports the name the platform gave it — the Trash renames a
/// colliding one rather than replacing it — and it has no notion of erasing, enumerating,
/// or emptying, so those are absent here because they are absent there. Reimplementing
/// the platform's convention by moving a directory into `~/.Trash` ourselves would be
/// this application deciding where a user's Trash is and what a "put back" is worth,
/// which is exactly the policy the platform owns.
///
/// **There is no delete fallback anywhere in it.** A platform this build has no such
/// request for refuses with [`io::ErrorKind::Unsupported`], so the removal is rejected
/// with the Order still in the Package. That is the same shape [`rename_exclusive`] takes
/// for the same reason: substituting a weaker operation would silence a promise rather
/// than report that it cannot be kept.
pub struct PlatformTrash;

#[cfg(target_vendor = "apple")]
impl NativeTrash for PlatformTrash {
    type Error = io::Error;

    /// Hands the whole directory to the platform and reports where it landed.
    ///
    /// The URL is built from the path's own **file-system representation** rather than
    /// from its text, and the reported location is read back the same way, because a path
    /// is not required to be valid UTF-8 and a lossy round trip would submit a directory
    /// this application cannot name — or report one that does not open.
    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        let path = nul_terminated(path)?;
        let source = std::ptr::NonNull::new(path.as_ptr().cast_mut()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "a managed path is not empty")
        })?;
        // SAFETY: `source` points at `path`'s nul-terminated bytes, which outlive the
        // call, and this initializer copies them without retaining the pointer.
        let url = unsafe {
            NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(source, true, None)
        };
        let mut landed = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed))
            .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        // The platform reports a location for a submission it completed, and it is free
        // not to. `None` is that answer rather than a failure: the directory has gone
        // either way, which is what the port asked about.
        Ok(landed.map(|landed| landed_path(&landed)))
    }
}

/// One reported Trash location as this platform's own path bytes.
///
/// The pointer is valid only until the enclosing autorelease pool drains, so the bytes
/// are copied here rather than borrowed out.
#[cfg(target_vendor = "apple")]
fn landed_path(landed: &NSURL) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;

    // SAFETY: the pointer is the receiver's own file-system representation, which stays
    // valid while `landed` is retained, and the bytes are copied before returning.
    let bytes = unsafe { std::ffi::CStr::from_ptr(landed.fileSystemRepresentation().as_ptr()) };
    PathBuf::from(OsStr::from_bytes(bytes.to_bytes()))
}

/// Refusing is the only honest answer where this build has no platform Trash to ask.
///
/// The exclusion is explicit: no force flag, no empty-trash operation, and **no fallback
/// that deletes**. A removal refused here leaves the Order exactly where it was, which is
/// the outcome a user can act on; erasing their Supporting Material because this build
/// could not find a Trash is not.
#[cfg(not(target_vendor = "apple"))]
impl NativeTrash for PlatformTrash {
    type Error = io::Error;

    fn submit_directory(&self, _path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform has no Trash this build can submit a directory to",
        ))
    }
}

/// The platform's random-number source, as the core's [`UuidGenerator`].
///
/// A unit type for the same reason [`PlatformFileSystem`] is one: a version-4 UUID is
/// randomness and a version nibble, so there is no seed, sequence, or namespace for two
/// call sites to disagree about. It is `&mut` only because the port is — a generator with
/// state to advance would need it, and this one does not.
///
/// **The core validates what this returns.** The port's signature is infallible, so a
/// generator that produced something that is not a Vendor ID has no channel of its own to
/// say so; the mutation that asked for one refuses instead, naming the text it was given.
/// That is why the identity is minted at the commit rather than while planning: a run that
/// never commits asks for none.
pub(crate) struct PlatformUuids;

impl UuidGenerator for PlatformUuids {
    fn generate_v4(&mut self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// The user's own calendar, as the core's [`LocalCalendar`].
///
/// A unit type for the reason [`PlatformFileSystem`] is one: there is one system clock and
/// one system time zone, so there is nothing about the value for two call sites to
/// disagree about.
///
/// **The local date is not the UTC date**, and it is not derivable from
/// `SystemTime::now()` alone: turning an instant into a calendar date needs this
/// machine's current offset, which needs the platform's time-zone database. `std` exposes
/// neither, which is why this effect has a dependency behind it where the other two do
/// not. A machine whose time zone cannot be determined at all leaves the library reading
/// the instant as UTC, which is the only calendar left to read it in.
///
/// **Read at the moment the mutation needs it, not at process start.** A creation that
/// takes its Date Ordered from today asks once, while it is planning; nothing here caches
/// a date, so a process that outlived midnight would not stamp yesterday.
pub struct PlatformCalendar;

impl LocalCalendar for PlatformCalendar {
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

/// `AT_FDCWD` is correct for both paths because this adapter is only ever given
/// absolute paths: the core canonicalizes the selected root through
/// [`FileSystem::canonicalize`] before anything else, and builds every managed path from
/// that canonical root. A relative path therefore never reaches this call — and if one
/// somehow did, resolving it against the process working directory is what the
/// non-`at` `rename` this replaces would have done anyway.
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
        "no exclusive directory rename is available on this platform",
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
const RENAME_UNSUPPORTED_MESSAGE: &str = "this filesystem has no exclusive directory rename";

#[cfg(target_os = "linux")]
const RENAME_UNSUPPORTED_MESSAGE: &str = "this filesystem has no exclusive directory rename, or the rename itself was \
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

/// The process arguments, including the program name clap expects first.
#[expect(
    clippy::disallowed_methods,
    reason = "the arguments are one of the two things this executable asks its \
              environment for, and this is the boundary that asks"
)]
pub(crate) fn arguments() -> Vec<OsString> {
    std::env::args_os().collect()
}

// The process working directory is deliberately not among these effects. A relative
// `--root` reaches the core exactly as the caller wrote it, and the core resolves it
// through `FileSystem::canonicalize` above — which the platform already interprets
// against the process working directory. Reading it here to join it on first would
// resolve the target twice, in two places, by two rules.

/// The `NO_COLOR` request exactly as the environment carries it.
///
/// The raw value rather than a decision: whether it disables color is a policy, and
/// the policy lives in `crate::output` where it can be exercised without an
/// environment.
#[expect(
    clippy::disallowed_methods,
    reason = "the color request is the other, and no third reason has been argued for \
              here — there is no pager, no `COLUMNS`, and no configuration file of the \
              CLI's own"
)]
pub(crate) fn no_color_request() -> Option<OsString> {
    std::env::var_os("NO_COLOR")
}

/// Whether this process's standard output is an interactive terminal.
pub(crate) fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}

/// Whether this process's standard error is an interactive terminal.
pub(crate) fn stderr_is_terminal() -> bool {
    io::stderr().is_terminal()
}
