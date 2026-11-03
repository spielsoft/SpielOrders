//! The one boundary at which this executable implements production effects.
//!
//! Everything the `spielorders` process learns from outside itself enters here, and every
//! platform service it asks for something is asked from here: the platform filesystem, the
//! process arguments, the environment, whether a stream is an interactive terminal, the
//! platform's Trash, its own Supporting Material index, and its own opener. No other module
//! in this crate names `fs::`, `env::`, or a process it runs.
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
    Cancellation, DirectoryEntry, DirectoryFileSystem, FileChangeEvidence, FileChangeObservation,
    FileSystem, FileSystemEntryKind, LocalCalendar, LocalDate, NativeTrash,
    NewDirectoryRenameOutcome, NewFileLinkOutcome, SupportingMaterialIndex, UuidGenerator,
};
// The names only the platform services below are written in terms of. Imported under the
// same condition as the items that use them, because a build for a platform this one has no
// index query and no opener for compiles every one of those items away — and an unused
// import is what `scripts/gate.sh` fails on.
#[cfg(target_vendor = "apple")]
use spielorders_core::ports::NativeFiles;
#[cfg(target_vendor = "apple")]
use spielorders_core::supporting_material::{
    Completeness, PartialReason, ProviderAnswer, ProviderDiagnostic, ProviderHit,
    SupportingMaterialRequest, UnavailableReason,
};
#[cfg(target_vendor = "apple")]
use std::{ffi::OsStr, time::Duration};
use std::{
    ffi::OsString,
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

/// The Supporting Material index this build can ask, where it has one for this platform.
///
/// **`None` is this build having none**, which the core reports as the platform state
/// `unsupported` rather than as a failure of the command line: it is a fact about the
/// build, and the answer says so instead of falling back to walking the tree.
///
/// A `&'static dyn` rather than a type parameter, because a build with no provider has no
/// type to name in one.
///
/// Public because which provider a build has is the one fact its Supporting Material search
/// answers differ by, and this is where that fact is: a suite that asked a `cfg` of its own
/// instead would be asserting what it had assumed.
#[cfg(target_vendor = "apple")]
pub fn supporting_material_index() -> Option<&'static dyn SupportingMaterialIndex> {
    /// The one provider this build ships, over the platform's own query.
    ///
    /// A `static` because the port is asked through a `&'static dyn` and this provider
    /// holds no state: the query, the bound, and the cancellation all arrive with the
    /// request.
    static SPOTLIGHT: Spotlight<'static> = Spotlight::over(&MetadataTools::SYSTEM);

    Some(&SPOTLIGHT)
}

/// The same question on a platform this build has no index query for.
///
/// **`None`, and nothing weaker behind it**: writing a provider for such a platform is that
/// platform's release gate, which is why this is an absence rather than a walk of the Order
/// Directories.
#[cfg(not(target_vendor = "apple"))]
pub fn supporting_material_index() -> Option<&'static dyn SupportingMaterialIndex> {
    None
}

// ---------------------------------------------------------------------------
// The platform's own Supporting Material index.
// ---------------------------------------------------------------------------

/// What one run of a platform index query produced.
///
/// **The seam is the run, not the answer.** Everything above it — which reported paths are
/// this Package's, how a stream of bytes becomes paths, and what the provider states mean —
/// is decided in this module and is asserted over prepared bytes, because no test may
/// depend on what a platform index happens to hold: Spotlight indexes a temporary directory
/// late or never. The real run is proven once, by hand, by the script the milestone's
/// by-hand proof ticket builds.
#[cfg(target_vendor = "apple")]
pub enum IndexRun {
    /// What the query wrote — one NUL-terminated path per result — together with why it was
    /// stopped before it finished, where it was.
    Wrote {
        output: Vec<u8>,
        stopped: Option<PartialReason>,
    },
    /// The query could not be run at all, in the platform's own words.
    Refused(String),
}

/// What state the platform says the index covering one directory is in.
///
/// **Four answers, because the platform gives four and two of them settle the same way.** A
/// volume it says has no index and a volume it declines to report on are one answer to a
/// caller — see [`Spotlight::search`] — but they are two different things for the platform to
/// have said, and folding them together here would leave nothing able to tell a suite which
/// of them a run was given.
#[cfg(target_vendor = "apple")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexState {
    /// Indexing is on for this volume, so the index is a thing that can be asked.
    Enabled,
    /// Indexing is switched off for this volume.
    Disabled,
    /// This volume has no index.
    NotIndexed,
    /// The platform would not say what state it is in, in its own words.
    Unreported(String),
}

/// This platform's own metadata index, as the two things this build asks it.
///
/// **One seam, because there is one service.** What state the index is in and what it holds
/// are two questions, and a build that faked one while running the other for real would prove
/// a composition it does not ship.
#[cfg(target_vendor = "apple")]
pub trait PlatformIndex: Sync {
    /// What state the index covering `directory` is in, within `bound` and stopping early
    /// where `cancellation` says the caller has withdrawn the question.
    ///
    /// **The same two limits a run carries, because the bound is on the answer.** Learning
    /// the state is work the caller waits through, so a state ask outside the bound would let
    /// a settled answer take longer than the caller allowed.
    ///
    /// Asked about a directory rather than about a volume, because which volume a directory
    /// is on is the platform's own to answer — see [`MetadataTools::state`].
    fn state(
        &self,
        directory: &Path,
        bound: Duration,
        cancellation: &dyn Cancellation,
    ) -> IndexState;

    /// Asks the index for `text` within `scope`, for at most `bound`, stopping early where
    /// `cancellation` says the caller has withdrawn the question.
    fn run(
        &self,
        scope: &Path,
        text: &str,
        bound: Duration,
        cancellation: &dyn Cancellation,
    ) -> IndexRun;
}

/// The platform's own index, as the core's [`SupportingMaterialIndex`].
///
/// **One run, over the directory that encloses every scope.** The platform's query takes a
/// single directory to look in, and the request's scopes are the Package's healthy Order
/// Directories, so the run is scoped to the Orders Directory above them — which is wider
/// than the scopes themselves. This adapter narrows nothing afterwards: it reports what the
/// query printed, and whether a path is inside a scope at all is decided with everything
/// else a kept path *means*. Grouping it under an Order, leaving out this application's own
/// managed documents, telling one file reported twice from two files, and ordering what is
/// left are Package rules and are applied in `spielorders-core`, and so is leaving out a
/// file under a Damaged Order and a file loose in the Orders Directory.
///
/// `hits` attaches extension-derived kinds and filesystem modification times to the
/// reported paths. Spotlight supplies no rank, so deterministic fallback ordering applies.
///
/// **What prevented an answer is established before one is asked for.** The query itself
/// prints nothing and succeeds whether the index is switched off, the volume is unindexed, or
/// simply nothing matched, so a provider that only ran it would report an index that cannot
/// answer as a search that completed and found no file. The state of the volume's index and
/// whether this process may read the Orders Directory at all are therefore settled first, and
/// each is asked of the thing that knows it.
#[cfg(target_vendor = "apple")]
pub struct Spotlight<'a> {
    index: &'a dyn PlatformIndex,
}

#[cfg(target_vendor = "apple")]
impl<'a> Spotlight<'a> {
    pub const fn over(index: &'a dyn PlatformIndex) -> Self {
        Self { index }
    }
}

#[cfg(target_vendor = "apple")]
impl SupportingMaterialIndex for Spotlight<'_> {
    /// The service's established name, which the answer reports and a view labels itself
    /// with.
    fn name(&self) -> &'static str {
        "spotlight"
    }

    fn search(
        &self,
        request: &SupportingMaterialRequest<'_>,
        cancellation: &dyn Cancellation,
    ) -> ProviderAnswer {
        let looked = |completeness, hits| ProviderAnswer::Looked {
            completeness,
            hits,
            diagnostics: Vec::new(),
        };
        let prevented = |reason, said: Option<String>| ProviderAnswer::Unavailable {
            reason,
            diagnostics: said
                .iter()
                .map(|said| ProviderDiagnostic::reported(said))
                .collect(),
        };
        // A Package with no healthy Order Directory has nowhere for Supporting Material to
        // be, so there is nothing to ask the index about. Looking and finding nothing is
        // the honest answer: the index was available, and no file in scope matched because
        // there was no scope.
        let Some(scope) = enclosing_directory(request.scopes()) else {
            return looked(Completeness::Complete, Vec::new());
        };
        // **Asked of the filesystem, because the filesystem is what knows.** An index is not
        // the authority on whether this process may read a directory, and asking it would
        // get the answer the index has for itself rather than the one the caller is refused
        // by. Only a refusal is this state: a directory that is not there is not one this
        // process is denied.
        //
        // Denied access is settled before the index state because it is the narrower fact —
        // this Package's own Orders Directory rather than the whole volume — and the
        // narrower correction. A choice, since nothing decides which of two true answers is
        // reported when both hold.
        // **No command line reaches this.** `search supporting-material` loads the Package
        // before it searches and that load lists this same directory, so a refusal here is a
        // refusal there first and the command reports that the Package could not be read.
        // What reaches it is a caller holding a Package it loaded earlier — a desktop shell
        // searching a Package open across a session — for which the alternative is an empty
        // answer about a directory nothing looked in.
        if let Err(refusal) = std::fs::read_dir(&scope)
            && refusal.kind() == io::ErrorKind::PermissionDenied
        {
            return prevented(
                UnavailableReason::AccessDenied,
                Some(format!("{} could not be read: {refusal}", scope.display())),
            );
        }
        // **Before the query, because the query cannot report these.** It prints nothing and
        // succeeds whether the index is switched off, the volume is unindexed, or nothing
        // matched, so an answer taken from it alone reports a switched-off index as a
        // completed search that found no file.
        let asking = std::time::Instant::now();
        match self.index.state(&scope, request.timeout(), cancellation) {
            IndexState::Disabled => return prevented(UnavailableReason::IndexingDisabled, None),
            IndexState::NotIndexed => return prevented(UnavailableReason::VolumeNotIndexed, None),
            // A volume the platform will not report on is reported as the same state as one
            // it says has no index, because the difference between them is invisible to a
            // caller and there is nothing different for one to do about it. What the platform
            // said travels beside it, which is the only part of the difference that survives.
            IndexState::Unreported(said) => {
                // Except where nobody is waiting for it. A caller who withdrew the question is
                // owed no sentence about their volume, and telling them it is unindexed on the
                // strength of an ask this adapter cut short would claim what nothing learned.
                // Asked here rather than trusted from the answer, for the reason the core asks
                // it again about a run: the platform is the one thing that cannot be relied on
                // to have noticed.
                if cancellation.is_cancelled() {
                    return looked(Completeness::Partial(PartialReason::Cancelled), Vec::new());
                }
                return prevented(UnavailableReason::VolumeNotIndexed, Some(said));
            }
            IndexState::Enabled => {}
        }
        // A directory the user has privately excluded from indexing is not one of the states
        // above and cannot be made into one. The platform was asked the only two questions it
        // answers about this — whether the volume's index is on, and what it holds for this
        // query — and neither reports an exclusion: `mdutil -s` reports the volume, not a
        // path within it, and `mdfind` prints the same empty result for an excluded directory
        // as for one holding no match. So an exclusion arrives here as an empty answer, which
        // is the case the search decision's own hedge already covers.
        // What the bound has left, because it is one bound on one answer rather than one per
        // tool: learning the state was time the caller has already waited through.
        let left = request.timeout().saturating_sub(asking.elapsed());
        match self.index.run(&scope, request.text(), left, cancellation) {
            IndexRun::Refused(detail) => ProviderAnswer::Unavailable {
                // The one state the query itself can report. It does not distinguish a
                // switched-off index, an unindexed volume, or a directory this process
                // cannot read from an empty answer — it prints nothing and succeeds for all
                // of them — so none of those is guessed at from a run, and each is
                // established above by whatever does know it.
                reason: UnavailableReason::QueryRefused,
                diagnostics: vec![ProviderDiagnostic::reported(&detail)],
            },
            IndexRun::Wrote { output, stopped } => looked(
                match stopped {
                    Some(reason) => Completeness::Partial(reason),
                    None => Completeness::Complete,
                },
                hits(&output),
            ),
        }
    }
}

/// The deepest directory that holds every one of `scopes`.
///
/// **The directory each scope is in, rather than the deepest one that merely contains them
/// all**, so the run's own scope is the same directory however many healthy Orders a
/// Package has: a Package's Order Directories are direct children of its Orders Directory,
/// so this is that Orders Directory whether there is one of them or twenty. Taking the
/// deepest common ancestor instead would scope a one-Order Package's run to that Order's
/// own Directory, which covers it — and makes the directory a run is scoped to a fact about
/// how many Orders happen to be healthy. `None` where there are no scopes at all, and where
/// they share no directory to ask about — a query scoped to nowhere is not one to run.
#[cfg(target_vendor = "apple")]
fn enclosing_directory(scopes: &[&Path]) -> Option<PathBuf> {
    // A scope with no parent is a filesystem root, which no Order Directory is; holding it
    // rather than dropping it keeps this total without inventing a wider scope.
    let mut holders = scopes.iter().map(|scope| scope.parent().unwrap_or(scope));
    let mut shared: Vec<std::path::Component<'_>> = holders.next()?.components().collect();
    for holder in holders {
        let common = shared
            .iter()
            .zip(holder.components())
            .take_while(|(held, component)| **held == *component)
            .count();
        shared.truncate(common);
    }
    (!shared.is_empty()).then(|| shared.iter().collect())
}

/// What the query wrote, as one hit per path it reported.
///
/// **NUL-terminated rather than newline-separated**, which is what the query is asked for: a
/// filename may hold a newline, so a line-oriented reading would report two paths that do
/// not exist in place of the one that does. The tail after the final terminator is dropped
/// for the same reason — in a run that was stopped part way through a write, that tail is a
/// path the query never finished.
///
/// **Every path it reported, and no narrowing of its own.** The run is scoped to the
/// directory that holds the request's scopes, which is wider than the scopes themselves, so
/// a path here can be one no Order Directory holds. Keeping only what a healthy Order holds
/// is a Package rule and is applied in `spielorders-core`, where the group a file is
/// reported under is decided: a provider is free to report whatever its index holds, and the
/// scope it was given is advice.
#[cfg(target_vendor = "apple")]
fn hits(output: &[u8]) -> Vec<ProviderHit> {
    use std::os::unix::ffi::OsStrExt;

    let mut fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    fields.pop();
    fields
        .into_iter()
        .filter(|field| !field.is_empty())
        .map(|field| {
            let path = PathBuf::from(OsStr::from_bytes(field));
            ProviderHit {
                kind: extension_kind(&path),
                modified_unix_seconds: modified_unix_seconds(&path),
                rank: None,
                path,
            }
        })
        .collect()
}

/// The file's kind, taken from its name's extension.
///
/// **The extension, not a content-type query.** `mdfind` prints paths and nothing else, so
/// asking the platform what a file *is* would be a second query per result. The name is
/// what a reader sees in Finder and is what the desktop's file row shows, so it is what
/// this reports — and the snapshot's own accessor says the kind is the provider's word for
/// it, which this is. A name with no extension has no kind rather than an invented one.
#[cfg(target_vendor = "apple")]
fn extension_kind(path: &Path) -> Option<String> {
    path.extension()
        .and_then(OsStr::to_str)
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
}

/// When the file was last written, as the filesystem reports it.
///
/// Absent rather than guessed where the path cannot be read: a result that has moved or
/// vanished between the query and this call is one the core reports as unavailable, and a
/// modification time invented for it would say the file was there.
///
/// **Signed, and both directions are carried.** The snapshot's field is signed because a
/// file may be stamped before 1970 — a restored archive or a deliberately backdated
/// timestamp — and reporting such a file as absent, or as the epoch itself, would say
/// something false about it.
#[cfg(target_vendor = "apple")]
fn modified_unix_seconds(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()?;
    match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs())
            .ok()
            .map(|seconds| -seconds),
    }
}

/// The operating system's own metadata tools, `mdutil` and `mdfind`.
///
/// **Processes rather than bindings**, and deliberately: both questions this needs are ones
/// the shipped command-line tools already answer, and reaching them through the platform's
/// metadata framework would put an Objective-C binding in the dependency graph for a status
/// line and a list of paths.
///
/// Each named by absolute path, so the tool asked is the one the operating system ships
/// rather than whatever a caller's `PATH` resolves.
#[cfg(target_vendor = "apple")]
struct MetadataTools<'a> {
    state_program: &'a str,
    query_program: &'a str,
}

#[cfg(target_vendor = "apple")]
impl MetadataTools<'_> {
    const SYSTEM: Self = Self {
        state_program: "/usr/bin/mdutil",
        query_program: "/usr/bin/mdfind",
    };
}

#[cfg(target_vendor = "apple")]
impl PlatformIndex for MetadataTools<'_> {
    /// **`mdutil` reports on a mount point**, which its own manual says and which this
    /// resolves the volume for. Handing it the Orders Directory instead is answered "unknown
    /// indexing state" for every path that is not itself a mount point, which would report
    /// every ordinary Package on this machine as being on a volume the platform will not
    /// report on.
    fn state(
        &self,
        directory: &Path,
        bound: Duration,
        cancellation: &dyn Cancellation,
    ) -> IndexState {
        match enclosing_volume(directory)
            .and_then(|volume| run_mdutil(self.state_program, &volume, bound, cancellation))
        {
            Ok(state) => state,
            // A volume that could not be resolved and a tool that could not be run are both
            // the platform not having said what state its index is in, in its own words.
            Err(error) => IndexState::Unreported(error.to_string()),
        }
    }

    fn run(
        &self,
        scope: &Path,
        text: &str,
        bound: Duration,
        cancellation: &dyn Cancellation,
    ) -> IndexRun {
        // A query that could not be started or waited on is one that could not be run, and
        // the platform's own message for it is what travels.
        run_mdfind(self.query_program, scope, text, bound, cancellation)
            .unwrap_or_else(|error| IndexRun::Refused(error.to_string()))
    }
}

/// The mount point of the volume `path` is on, which is what `mdutil` is asked about.
///
/// The platform's own answer, through `statfs`, rather than a walk up the path comparing
/// device numbers: which mount point a path belongs to is a thing the kernel already knows,
/// and a walk would ask it once per ancestor to rediscover it.
#[cfg(target_vendor = "apple")]
fn enclosing_volume(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    let path = nul_terminated(path)?;
    let mut reported = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is a nul-terminated C string that outlives the call, and `reported` is
    // writable for a whole `statfs`, which the call fills before reporting success.
    let code = unsafe { libc::statfs(path.as_ptr(), reported.as_mut_ptr()) };
    if code != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call reported success, so it filled the struct.
    let reported = unsafe { reported.assume_init() };
    // SAFETY: `f_mntonname` is the nul-terminated mount point the kernel wrote into the
    // struct, which lives until the end of this function, and the bytes are copied here.
    let mount = unsafe { std::ffi::CStr::from_ptr(reported.f_mntonname.as_ptr()) };
    Ok(PathBuf::from(OsStr::from_bytes(mount.to_bytes())))
}

/// What this platform says about one volume's index, within the bound the answer has.
///
/// **The status is not the answer, so the output is.** This tool exits successfully having
/// printed that the path it was given was invalid, so nothing can be read from the status
/// alone; a status that is not success travels only where the tool also said nothing that
/// names a state.
#[cfg(target_vendor = "apple")]
fn run_mdutil(
    program: &str,
    volume: &Path,
    bound: Duration,
    cancellation: &dyn Cancellation,
) -> io::Result<IndexState> {
    let mut asking = std::process::Command::new(program);
    asking.arg("-s").arg(volume);
    Ok(match bounded(&mut asking, bound, cancellation)? {
        ToolRun::Exited { status, wrote, .. } => {
            match index_state(&String::from_utf8_lossy(&wrote)) {
                // A tool that said nothing a state can be read from has still not reported
                // one, and the status it exited with is then the only word it gave.
                IndexState::Unreported(said) if said.is_empty() => {
                    IndexState::Unreported(status.to_string())
                }
                state => state,
            }
        }
        // A tool this adapter stopped never finished saying what state the volume is in, so
        // no state was learned — and what stopped it is the whole of what there is to report.
        ToolRun::Stopped { reason, .. } => IndexState::Unreported(
            match reason {
                PartialReason::TimedOut => {
                    "the platform did not report this volume's index state within the time it \
                     was given"
                }
                PartialReason::Cancelled => {
                    "the question was withdrawn before the platform reported this volume's \
                     index state"
                }
            }
            .to_owned(),
        ),
    })
}

/// What one `mdutil -s` report says, as the state it names.
///
/// **Only the indented lines are read.** The tool prints the volume it resolved on a line of
/// its own and its answer indented beneath it, so a volume whose mount point happened to be
/// spelled with one of the words below would otherwise decide the state. A report with no
/// indented line named no state at all — an invalid path is reported that way — and the
/// whole of what it said travels instead.
///
/// **A report naming both readings is read as the disabling.** Nothing decides which half of
/// "indexing and searching disabled" governs, and an index that will not be searched answers
/// nothing whatever it holds, so this reads the half that cannot report an index that will
/// not answer as one that will. A choice, not a rule.
///
/// **A phrasing this does not know is the volume's state rather than an answer.** Every
/// reading but the two named ones settles as the same unavailable reason the platform's own
/// "no index" does, so a word the tool has that this has not still stops a run from reporting
/// a search that completed.
#[cfg(target_vendor = "apple")]
fn index_state(reported: &str) -> IndexState {
    let said = reported
        .lines()
        .filter(|line| line.starts_with(char::is_whitespace))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let named = said.to_ascii_lowercase();
    if named.contains("disabled") {
        IndexState::Disabled
    } else if named.contains("enabled") {
        IndexState::Enabled
    } else if named.contains("no index") {
        IndexState::NotIndexed
    } else if said.is_empty() {
        IndexState::Unreported(reported.trim().to_owned())
    } else {
        IndexState::Unreported(said)
    }
}

/// How long the wait between two looks at the cancellation and the bound is.
///
/// A choice this code makes rather than a rule it is held to: nothing decides how promptly
/// a withdrawn question stops the platform's work, only that it does. Short enough that a
/// caller who cancelled is not left waiting, long enough that a query that answers at once
/// is not woken for nothing.
#[cfg(target_vendor = "apple")]
const LOOK: Duration = Duration::from_millis(25);

/// The text this query is invoked as, in the words the platform's own manual gives it.
#[cfg(target_vendor = "apple")]
fn run_mdfind(
    program: &str,
    scope: &Path,
    text: &str,
    bound: Duration,
    cancellation: &dyn Cancellation,
) -> io::Result<IndexRun> {
    let mut asking = std::process::Command::new(program);
    asking
        .arg("-0")
        .arg("-onlyin")
        .arg(scope)
        // Last, and as one argument: the text is the caller's own and the query governs
        // matching, so nothing here tokenizes it, folds it, or wraps it in a syntax of this
        // application's own.
        .arg(text);
    Ok(match bounded(&mut asking, bound, cancellation)? {
        ToolRun::Exited { status, wrote, .. } if status.success() => IndexRun::Wrote {
            output: wrote,
            stopped: None,
        },
        ToolRun::Exited { status, said, .. } => IndexRun::Refused(refusal(&status, &said)),
        ToolRun::Stopped { wrote, reason } => IndexRun::Wrote {
            output: wrote,
            stopped: Some(reason),
        },
    })
}

/// How one bounded run of a platform tool ended.
#[cfg(target_vendor = "apple")]
enum ToolRun {
    /// The tool exited on its own: the status it exited with, and what it wrote to each of
    /// its two streams.
    Exited {
        status: std::process::ExitStatus,
        wrote: Vec<u8>,
        said: Vec<u8>,
    },
    /// This adapter stopped the tool before it exited, for this reason, and this is what it
    /// had written by then.
    Stopped {
        wrote: Vec<u8>,
        reason: PartialReason,
    },
}

/// One run of a platform tool, held to the bound the request carries and to the caller's
/// cancellation.
///
/// **Both tools this build asks go through it, because the bound is on the answer rather than
/// on a run.** A second tool run outside it would let a settled answer take longer than the
/// caller allowed, and a tool that never returns would hold a caller who has already withdrawn
/// the question for as long as it liked.
///
/// Both streams are piped rather than inherited, because a tool that writes a locale notice to
/// standard error on every run — successful or not — would otherwise put a platform notice on
/// this process's standard error, where a successful command writes nothing. They are
/// therefore not a failure signal either; the exit status is, and what was said is read back
/// only to carry the tool's own words into a refusal.
#[cfg(target_vendor = "apple")]
fn bounded(
    asking: &mut std::process::Command,
    bound: Duration,
    cancellation: &dyn Cancellation,
) -> io::Result<ToolRun> {
    let child = asking
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut running = RunningTool(child);
    let mut wrote = ToolPipe::new(running.0.stdout.take())?;
    let mut said = ToolPipe::new(running.0.stderr.take())?;
    let deadline = std::time::Instant::now() + bound;
    loop {
        let progress = wrote.poll()? | said.poll()?;
        let reason = if cancellation.is_cancelled() {
            Some(PartialReason::Cancelled)
        } else if std::time::Instant::now() >= deadline {
            Some(PartialReason::TimedOut)
        } else {
            None
        };
        if let Some(status) = running.0.try_wait()? {
            wrote.drain_ready()?;
            said.drain_ready()?;
            return Ok(ToolRun::Exited {
                status,
                wrote: wrote.bytes,
                said: said.bytes,
            });
        }
        if let Some(reason) = reason {
            running.0.kill()?;
            running.0.wait()?;
            wrote.drain_ready()?;
            said.drain_ready()?;
            return Ok(ToolRun::Stopped {
                wrote: wrote.bytes,
                reason,
            });
        }
        if !progress {
            std::thread::sleep(
                LOOK.min(deadline.saturating_duration_since(std::time::Instant::now())),
            );
        }
    }
}

/// Owns cleanup even if configuring or reading a pipe fails.
#[cfg(target_vendor = "apple")]
struct RunningTool(std::process::Child);

#[cfg(target_vendor = "apple")]
impl Drop for RunningTool {
    fn drop(&mut self) {
        if self.0.kill().is_ok() {
            let _ = self.0.wait();
        } else {
            let _ = self.0.try_wait();
        }
    }
}

/// A nonblocking pipe, with only the bytes actually gathered so far.
#[cfg(target_vendor = "apple")]
struct ToolPipe<R> {
    pipe: Option<R>,
    bytes: Vec<u8>,
}

#[cfg(target_vendor = "apple")]
impl<R: std::io::Read + std::os::fd::AsRawFd> ToolPipe<R> {
    fn drain_ready(&mut self) -> io::Result<()> {
        use std::io::Read;
        let Some(pipe) = &mut self.pipe else {
            return Ok(());
        };
        let mut ready: libc::c_int = 0;
        // SAFETY: the borrowed pipe owns a live descriptor and ready points to a writable
        // c_int. FIONREAD reports the bytes queued without waiting for a writer or EOF.
        if unsafe { libc::ioctl(pipe.as_raw_fd(), libc::FIONREAD, &mut ready) } == -1 {
            return Err(io::Error::last_os_error());
        }
        // Read only this snapshot. A surviving writer cannot extend the drain forever.
        match pipe.take(ready as u64).read_to_end(&mut self.bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn new(pipe: Option<R>) -> io::Result<Self> {
        if let Some(pipe) = &pipe {
            let fd = pipe.as_raw_fd();
            // SAFETY: the borrowed pipe owns a live descriptor. F_GETFL takes no third
            // argument; F_SETFL changes only its file status flags, retaining the others.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags == -1
                || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self {
            pipe,
            bytes: Vec::new(),
        })
    }

    fn poll(&mut self) -> io::Result<bool> {
        let Some(pipe) = &mut self.pipe else {
            return Ok(false);
        };
        let mut buffer = [0; 8192];
        match pipe.read(&mut buffer) {
            Ok(0) => {
                self.pipe = None;
                Ok(true)
            }
            Ok(length) => {
                self.bytes.extend_from_slice(&buffer[..length]);
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

/// What a refused query said about itself, or the status it refused with where it said
/// nothing.
#[cfg(target_vendor = "apple")]
fn refusal(status: &std::process::ExitStatus, said: &[u8]) -> String {
    let said = String::from_utf8_lossy(said).trim().to_owned();
    if said.is_empty() {
        status.to_string()
    } else {
        said
    }
}

// ---------------------------------------------------------------------------
// Opening and revealing a file the platform's own way.
// ---------------------------------------------------------------------------

/// One handover of a file to the platform, as the platform's own opener receives it.
///
/// **The seam is the handover.** Launching an application is not something an automated
/// test may do on the owner's machine, so what a test asserts is the arguments that would
/// have been handed over. The handover itself runs in one ignored test, which the gate
/// never runs and the by-hand proof script runs by name, because a real one launches an
/// application on the owner's machine.
#[cfg(target_vendor = "apple")]
pub trait Launched: Sync {
    /// Hands `arguments` to the platform's opener and reports whether it accepted them.
    fn launch(&self, arguments: &[&OsStr]) -> io::Result<()>;
}

/// Opening a file and revealing it, as the core's [`NativeFiles`].
///
/// **The platform's own opener, and nothing of this application's own.** Which application
/// opens a file is the user's setting, and where a file is shown is the file manager's, so
/// both are one request to the platform: its opener with the file, and its opener with the
/// reveal flag and the file. Neither reads the file, and neither has an alternative to fall
/// back to.
///
/// **A file that has gone is reported rather than acted on.** A result stands for a file the
/// index held when it answered, which a user may have moved or deleted since; handing that
/// path over anyway asks the platform to open something that is not there, and its
/// complaint about a launch is not the answer. The path is looked for first, so the failure
/// says the file is unavailable. It leaves the results themselves alone: a result that has
/// gone drops out when the next query is answered, because nothing reports a file the index
/// no longer holds.
///
/// This build's own is [`native_files`], and **no command line reaches it.** Nothing this
/// executable does opens or reveals a file, and the desktop, which will, does not depend on
/// this crate — so the arguments are asserted against a recording double, and the one real
/// handover is an ignored test the by-hand proof script runs while somebody watches.
#[cfg(target_vendor = "apple")]
pub struct PlatformOpener<'a> {
    launcher: &'a dyn Launched,
}

#[cfg(target_vendor = "apple")]
impl<'a> PlatformOpener<'a> {
    pub const fn over(launcher: &'a dyn Launched) -> Self {
        Self { launcher }
    }

    /// Hands one file over under `flags`, once it is established that the file is there.
    fn hand_over(&self, flags: &[&OsStr], path: &Path) -> io::Result<()> {
        if PlatformFileSystem.symlink_metadata(path)?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "this file is no longer where the index reported it",
            ));
        }
        let mut arguments: Vec<&OsStr> = flags.to_vec();
        arguments.push(path.as_os_str());
        self.launcher.launch(&arguments)
    }
}

#[cfg(target_vendor = "apple")]
impl NativeFiles for PlatformOpener<'_> {
    type Error = io::Error;

    fn open(&self, path: &Path) -> Result<(), Self::Error> {
        self.hand_over(&[], path)
    }

    /// Reveals the file itself, selected where it stands, rather than opening the directory
    /// that holds it — which is what the reveal flag is for and what the result is about.
    fn reveal(&self, path: &Path) -> Result<(), Self::Error> {
        self.hand_over(&[OsStr::new("-R")], path)
    }
}

/// The opener this build hands a Supporting Material file to, over the platform's own.
///
/// **The one composed opener, for the reason [`supporting_material_index`] is the one
/// composed provider**: what the seam under it is belongs here rather than to each caller.
#[cfg(target_vendor = "apple")]
pub const fn native_files() -> PlatformOpener<'static> {
    static OPEN: Open = Open;

    PlatformOpener::over(&OPEN)
}

/// The operating system's own opener, `open`.
///
/// A process rather than a binding, for the reason [`MetadataTools`] is one, and by absolute
/// path for the same reason too.
#[cfg(target_vendor = "apple")]
struct Open;

#[cfg(target_vendor = "apple")]
impl Launched for Open {
    fn launch(&self, arguments: &[&OsStr]) -> io::Result<()> {
        let status = std::process::Command::new("/usr/bin/open")
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "the platform's opener declined this file ({status})"
        )))
    }
}

/// The cancellation this process's query carries.
///
/// **It is never cancelled, and that follows from a choice made elsewhere**: this
/// executable installs no signal handler — see [`crate::exit`] — so an interrupt ends the
/// process by the platform's own convention rather than travelling to a running query. A
/// desktop shell calling the same core capability from a worker owns a real one.
pub(crate) struct UninterruptedQuery;

impl Cancellation for UninterruptedQuery {
    fn is_cancelled(&self) -> bool {
        false
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

/// What this platform's own tools say, read as the states an answer is built from.
///
/// **Here because this is the half a prepared index cannot reach.** Everything above the seam
/// is driven through the composition with a prepared index in
/// `tests/platform_index_and_opener.rs`, which is where each state that prevents an answer is
/// shown preventing one. What that cannot show is the two things only the shipped tools do:
/// resolve the volume a Package is on, turn the platform report into a state, and enforce
/// stopping against a real subprocess. These checks do not change the host's index state.
#[cfg(all(test, target_vendor = "apple"))]
mod tests {
    use super::{
        Cancellation, Duration, IndexRun, IndexState, MetadataTools, PartialReason, PlatformIndex,
        UninterruptedQuery, enclosing_volume, index_state,
    };
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    struct QueryProcess {
        directory: PathBuf,
        program: PathBuf,
        written: PathBuf,
        pid: PathBuf,
    }

    impl QueryProcess {
        fn new(closes_stdout: bool) -> Self {
            let directory = std::env::temp_dir()
                .join(format!("spielorders-query-stop-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
            let program = directory.join("query");
            let written = directory.join("written");
            let pid = directory.join("pid");
            let mut script = format!(
                "#!/bin/sh\nprintf '%s' $$ > '{}'\nprintf '/reported-file\\0'\ntouch '{}'\n",
                pid.display(),
                written.display()
            );
            if closes_stdout {
                script.push_str("exec 1>&-\n");
            }
            script.push_str("exec /bin/sleep 5\n");
            std::fs::write(&program, script).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                directory,
                program,
                written,
                pid,
            }
        }
    }

    impl Drop for QueryProcess {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    struct CancelWhenWritten<'a>(&'a std::path::Path);

    impl Cancellation for CancelWhenWritten<'_> {
        fn is_cancelled(&self) -> bool {
            self.0.exists()
        }
    }

    /// Never cancels, but holds the runner until the child has written, so a bound short
    /// enough to elapse mid-query cannot elapse before there is output to preserve.
    struct TimeOutOnceWritten<'a>(&'a std::path::Path);

    impl Cancellation for TimeOutOnceWritten<'_> {
        fn is_cancelled(&self) -> bool {
            let giving_up = std::time::Instant::now() + Duration::from_secs(30);
            while !self.0.exists() {
                assert!(
                    std::time::Instant::now() < giving_up,
                    "query child never wrote"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        }
    }

    struct StopAfterQueued<'a> {
        release: &'a std::path::Path,
        written: &'a std::path::Path,
        cancel: bool,
    }

    impl Cancellation for StopAfterQueued<'_> {
        fn is_cancelled(&self) -> bool {
            std::fs::write(self.release, b"go").unwrap();
            TimeOutOnceWritten(self.written).is_cancelled();
            self.cancel
        }
    }

    struct StopAfterExit<'a> {
        process: &'a QueryProcess,
        cancel: bool,
    }

    impl Cancellation for StopAfterExit<'_> {
        fn is_cancelled(&self) -> bool {
            TimeOutOnceWritten(&self.process.written).is_cancelled();
            let pid: libc::id_t = std::fs::read_to_string(&self.process.pid)
                .unwrap()
                .parse()
                .unwrap();
            // SAFETY: siginfo_t is a C data structure with a valid zero representation.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: pid belongs to this fixture and info is writable. WNOWAIT observes
            // termination without reaping, leaving the production runner to collect it.
            assert_eq!(
                unsafe { libc::waitid(libc::P_PID, pid, &mut info, libc::WEXITED | libc::WNOWAIT) },
                0
            );
            self.cancel
        }
    }

    #[test]
    fn a_finished_query_keeps_its_exit_when_a_limit_is_also_observed() {
        for status in [0, 7] {
            for cancel in [false, true] {
                let process = QueryProcess::new(false);
                let script = format!(
                    "#!/bin/sh\nprintf '%s' $$ > '{}'\nprintf '/complete-result\\0'\nprintf 'tool refused' >&2\ntouch '{}'\nexit {status}\n",
                    process.pid.display(),
                    process.written.display()
                );
                std::fs::write(&process.program, script).unwrap();
                let tools = MetadataTools {
                    state_program: "/usr/bin/mdutil",
                    query_program: process.program.to_str().unwrap(),
                };
                let token = StopAfterExit {
                    process: &process,
                    cancel,
                };
                match tools.run(
                    &process.directory,
                    "quote",
                    if cancel {
                        Duration::from_secs(10)
                    } else {
                        Duration::ZERO
                    },
                    &token,
                ) {
                    IndexRun::Wrote { output, stopped } if status == 0 => {
                        assert_eq!(stopped, None, "finished query is not stopped");
                        assert_eq!(output, b"/complete-result\0");
                    }
                    IndexRun::Refused(reason) if status != 0 => assert_eq!(reason, "tool refused"),
                    _ => panic!("exit {status} was replaced by a stop, cancel={cancel}"),
                }
            }
        }
    }

    #[test]
    fn stopping_retains_all_complete_paths_queued_after_the_last_poll() {
        let paths: String = (0..24)
            .map(|number| format!("/scope/{number:02}-{}.pdf\0", "q".repeat(470)))
            .collect();
        assert!(paths.len() > 8192 && paths.len() < 16384);
        for cancel in [false, true] {
            let process = QueryProcess::new(false);
            let release = process.directory.join("release");
            let script = format!(
                "#!/bin/sh\nwhile [ ! -e '{}' ]; do /bin/sleep 0.005; done\nprintf '{}'\ntouch '{}'\nexec /bin/sleep 5\n",
                release.display(),
                paths.replace('\0', "\\0"),
                process.written.display()
            );
            std::fs::write(&process.program, script).unwrap();
            let tools = MetadataTools {
                state_program: "/usr/bin/mdutil",
                query_program: process.program.to_str().unwrap(),
            };
            let token = StopAfterQueued {
                release: &release,
                written: &process.written,
                cancel,
            };
            match tools.run(
                &process.directory,
                "quote",
                if cancel {
                    Duration::from_secs(10)
                } else {
                    Duration::ZERO
                },
                &token,
            ) {
                IndexRun::Wrote { output, stopped } => {
                    assert_eq!(
                        stopped,
                        Some(if cancel {
                            PartialReason::Cancelled
                        } else {
                            PartialReason::TimedOut
                        })
                    );
                    assert_eq!(
                        output.len(),
                        paths.len(),
                        "all queued paths survive, cancel={cancel}"
                    );
                    assert_eq!(output, paths.as_bytes());
                }
                IndexRun::Refused(reason) => panic!("started query was refused: {reason}"),
            }
        }
    }

    #[test]
    fn active_queries_stop_with_partial_results_even_after_stdout_closes() {
        for closes_stdout in [false, true] {
            for cancels in [false, true] {
                let process = QueryProcess::new(closes_stdout);
                let tools = MetadataTools {
                    state_program: "/usr/bin/mdutil",
                    query_program: process.program.to_str().unwrap(),
                };
                let cancelling = CancelWhenWritten(&process.written);
                let timing_out = TimeOutOnceWritten(&process.written);
                let cancellation: &dyn Cancellation =
                    if cancels { &cancelling } else { &timing_out };
                let answer = tools.run(
                    &process.directory,
                    "file",
                    if cancels {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_millis(100)
                    },
                    cancellation,
                );
                let expected = if cancels {
                    PartialReason::Cancelled
                } else {
                    PartialReason::TimedOut
                };
                match answer {
                    IndexRun::Wrote { output, stopped } => {
                        assert_eq!(stopped, Some(expected), "closed stdout: {closes_stdout}");
                        assert_eq!(output, b"/reported-file\0");
                    }
                    IndexRun::Refused(reason) => panic!("started query was refused: {reason}"),
                }
                let pid: libc::pid_t = std::fs::read_to_string(&process.pid)
                    .unwrap()
                    .parse()
                    .unwrap();
                // SAFETY: waitpid receives a PID this fixture's child wrote, a valid status
                // pointer, and WNOHANG. A reaped child must no longer belong to this process.
                let mut status = 0;
                assert_eq!(
                    unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                    -1
                );
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
            }
        }
    }

    /// The report the tool prints for a state is read as that state.
    ///
    /// The enabled report and the one for a path that is not a mount point are what this
    /// platform's own tool prints, byte for byte. The rest are its remaining vocabulary — and
    /// a phrasing this reading does not know is not a hazard, which is the last case: it
    /// settles as the volume state, so a word the tool has that this has not still stops a run
    /// from reporting a search that completed.
    #[test]
    fn what_the_tool_prints_is_read_as_the_state_it_names() {
        let read = |reported| index_state(reported);

        assert_eq!(read("/:\n\tIndexing enabled. \n"), IndexState::Enabled);
        assert_eq!(
            read("/Volumes/Archive:\n\tIndexing disabled.\n"),
            IndexState::Disabled
        );
        assert_eq!(
            read("/Volumes/Archive:\n\tIndexing and searching disabled.\n"),
            IndexState::Disabled,
            "an index that will not be searched answers nothing whatever it holds"
        );
        assert_eq!(
            read("/Volumes/Archive:\n\tNo index.\n"),
            IndexState::NotIndexed
        );
        assert_eq!(
            read("/System/Volumes/Data/Users/nobody:\n\tError: unknown indexing state.\n"),
            IndexState::Unreported("Error: unknown indexing state.".to_owned())
        );
        assert_eq!(
            read("Error: invalid path `/nowhere'.\n"),
            IndexState::Unreported("Error: invalid path `/nowhere'.".to_owned()),
            "a report naming no volume said nothing about one, and all of it travels"
        );
        assert_eq!(
            read("/Volumes/Archive:\n\tSomething this reading has never seen.\n"),
            IndexState::Unreported("Something this reading has never seen.".to_owned())
        );
    }

    /// The volume the tool names in its own report does not decide the state.
    ///
    /// **The defect this catches is reading the whole report.** The tool prints the volume it
    /// resolved on a line of its own, so a volume mounted at a name spelled with one of the
    /// words would otherwise report a state nobody said.
    #[test]
    fn the_volume_the_tool_names_cannot_decide_the_state() {
        assert_eq!(
            index_state("/Volumes/disabled:\n\tIndexing enabled. \n"),
            IndexState::Enabled
        );
    }

    /// The shipped tools resolve the volume an ordinary directory is on and get a state for
    /// it.
    ///
    /// **The one claim about the shipped tools an automated suite can make on this machine.**
    /// Which state this volume is in is a fact about the owner's laptop and is not asserted;
    /// that the platform reported one at all is the property, and it is the property volume
    /// resolution decides — the tool takes a mount point and answers "unknown indexing state"
    /// for every path that is not one, which is the answer this fails on. It therefore holds
    /// whether this machine's index is switched on or off, and it needs neither.
    #[test]
    fn the_shipped_tools_get_a_state_for_the_volume_a_directory_is_on() {
        let directory = std::env::temp_dir();
        let volume = enclosing_volume(&directory)
            .expect("the platform resolves the volume a temporary directory is on");
        assert!(
            volume.is_dir(),
            "the resolved mount point is a directory that is there: {}",
            volume.display()
        );

        if let IndexState::Unreported(said) =
            MetadataTools::SYSTEM.state(&directory, Duration::from_secs(10), &UninterruptedQuery)
        {
            panic!(
                "the platform reported no state for {}, the volume {} is on: {said}",
                volume.display(),
                directory.display()
            );
        }
    }
}
