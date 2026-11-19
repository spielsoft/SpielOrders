//! What every exported function shares: panic catching, out-parameters, the outcome, the
//! cancellation table, and the effects a call performs.

use crate::interface::{
    SPIELORDERS_RESULT_CANCELLED, SPIELORDERS_RESULT_INTERNAL, SPIELORDERS_RESULT_OK,
    SPIELORDERS_RESULT_REFUSED, SpielordersEntries, SpielordersResult,
    entries::{Entry, Subject},
};
use crate::platform::{PlatformEffects, PlatformFileSystem};
use crate::{
    Answer, Failure,
    ports::{
        Cancellation, DirectoryEntry, Effects, FileChangeObservation, FileSystem,
        FileSystemEntryKind, LocalDate, NewFileLinkOutcome, RenameOutcome,
    },
};
use std::{
    collections::BTreeMap,
    fmt, io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Writes `value` into a caller's out-parameter without reading or freeing what it held.
pub(crate) fn put<T>(slot: &mut Option<Box<T>>, value: Option<Box<T>>) {
    // SAFETY: `slot` is a valid, aligned, writable location for this type; the header
    // says its previous contents are overwritten and never freed, so they are not read.
    unsafe { std::ptr::write(slot, value) }
}

/// Runs a fallible function's `body`, catching any panic as INTERNAL, and hands the
/// entries it produced to `outcome`.
pub(crate) fn fallible(
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
    body: impl FnOnce(&mut Vec<Entry>) -> SpielordersResult,
) -> SpielordersResult {
    let mut entries = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| body(&mut entries)))
        .unwrap_or(SPIELORDERS_RESULT_INTERNAL);
    if let Some(slot) = outcome {
        put(slot, SpielordersEntries::of(entries));
    }
    result
}

/// An accessor's answer, or `misuse` where computing it panicked.
pub(crate) fn quietly<T>(misuse: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(misuse)
}

/// The outcome of a call that settled to `result`: OK with its value, or the status the
/// failure and the cancellation flag make of it. Findings beside the answer become
/// entries without changing its status.
pub(crate) fn settle<T>(
    answer: Answer<T>,
    session: &Session,
    subject: Subject,
    entries: &mut Vec<Entry>,
) -> Result<T, SpielordersResult> {
    entries.extend(answer.diagnostics.iter().map(Entry::finding));
    if session.is_cancelled() {
        return Err(SPIELORDERS_RESULT_CANCELLED);
    }
    match answer.result {
        Ok(value) => Ok(value),
        Err(Failure::Cancelled) => Err(SPIELORDERS_RESULT_CANCELLED),
        Err(failure) => {
            Entry::failures(&failure, subject, entries);
            Err(SPIELORDERS_RESULT_REFUSED)
        }
    }
}

/// `result` as a status, writing its value into `slot` when it is one.
pub(crate) fn deliver<T>(
    result: Result<T, SpielordersResult>,
    slot: &mut Option<Box<T>>,
) -> SpielordersResult {
    match result {
        Ok(value) => {
            put(slot, Some(Box::new(value)));
            SPIELORDERS_RESULT_OK
        }
        Err(status) => {
            put(slot, None);
            status
        }
    }
}

static TOKENS: Mutex<BTreeMap<u64, Arc<AtomicBool>>> = Mutex::new(BTreeMap::new());
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

fn tokens() -> std::sync::MutexGuard<'static, BTreeMap<u64, Arc<AtomicBool>>> {
    TOKENS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A new cancellation token: never 0 and never reused. Pass it to calls that take a
/// token, request it from any thread to cancel them, and release it when done.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_cancel_new() -> u64 {
    quietly(0, || {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::SeqCst);
        tokens().insert(token, Arc::new(AtomicBool::new(false)));
        token
    })
}

/// Cancels every call running or later started with `token`: each returns
/// SPIELORDERS_RESULT_CANCELLED at its next effect, having written nothing. An unknown
/// or released token does nothing. Safe from any thread.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_cancel_request(token: u64) {
    quietly((), || {
        if let Some(flag) = tokens().get(&token) {
            flag.store(true, Ordering::SeqCst);
        }
    });
}

/// Releases `token`. Calls already running with it are unaffected; later requests of it
/// do nothing. An unknown or released token does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_cancel_release(token: u64) {
    quietly((), || {
        tokens().remove(&token);
    });
}

/// The effects one call performs: the platform's, each refused once the call's token is
/// cancelled, with a record of whether any of them wrote.
pub(crate) struct Session {
    cancel: Arc<AtomicBool>,
    wrote: AtomicBool,
    /// The Package this call last loaded, forgotten at its next write.
    remembered: std::sync::Mutex<Option<crate::canonical::CurrentPackageCandidate>>,
}

impl Session {
    /// A session cancelled by `token`; 0 or an unknown token is never cancelled.
    pub(crate) fn new(token: u64) -> Self {
        let cancel = tokens()
            .get(&token)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        Self {
            cancel,
            wrote: AtomicBool::new(false),
            remembered: std::sync::Mutex::new(None),
        }
    }

    pub(crate) fn uncancellable() -> Self {
        Self::new(0)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub(crate) fn cancellation(&self) -> &dyn Cancellation {
        &*self.cancel
    }

    /// The root of the Package this call can reuse as a current-index open, with nothing
    /// written since. A rebuilt index omits damaged Orders, so a fresh open would call
    /// that index stale rather than the current state the rebuild itself reported.
    pub(crate) fn current_root(&self) -> Option<PathBuf> {
        let remembered = self.remembered.lock().ok()?;
        remembered
            .as_ref()
            .filter(|package| {
                package.index.state == crate::canonical::PackageIndexState::Current
                    && package.damaged_orders.is_empty()
            })
            .map(|package| package.context.root.absolute_path.clone())
    }

    /// Whether any effect of this session wrote, renamed, or removed anything.
    pub(crate) fn wrote(&self) -> bool {
        self.wrote.load(Ordering::SeqCst)
    }

    fn effect<T>(
        &self,
        writes: bool,
        effect: impl FnOnce(&PlatformFileSystem) -> io::Result<T>,
    ) -> Result<T, Interrupted> {
        if self.is_cancelled() {
            return Err(Interrupted::Cancelled);
        }
        if writes && let Ok(mut remembered) = self.remembered.lock() {
            *remembered = None;
        }
        let result = effect(&PlatformFileSystem).map_err(Interrupted::Failed);
        if writes && result.is_ok() {
            self.wrote.store(true, Ordering::SeqCst);
        }
        result
    }
}

#[derive(Debug)]
pub(crate) enum Interrupted {
    Cancelled,
    Failed(io::Error),
}

impl fmt::Display for Interrupted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::Failed(error) => error.fmt(formatter),
        }
    }
}

impl Effects for Session {
    type FileSystem = Self;

    fn file_system(&self) -> &Self {
        self
    }

    fn today(&self) -> LocalDate {
        PlatformEffects.today()
    }

    fn new_uuid(&self) -> String {
        PlatformEffects.new_uuid()
    }
}

impl FileSystem for Session {
    type Error = Interrupted;

    fn is_access_denied(&self, error: &Interrupted) -> bool {
        matches!(error, Interrupted::Failed(error) if PlatformFileSystem.is_access_denied(error))
    }

    fn is_unsupported(&self, error: &Interrupted) -> bool {
        matches!(error, Interrupted::Failed(error) if PlatformFileSystem.is_unsupported(error))
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Interrupted> {
        self.effect(false, |fs| fs.read(path))
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Interrupted> {
        self.effect(true, |fs| fs.write_new(path, contents))
    }

    fn sync_file(&self, path: &Path) -> Result<(), Interrupted> {
        self.effect(false, |fs| fs.sync_file(path))
    }

    fn sync_file_contents(&self, path: &Path) -> Result<(), Interrupted> {
        self.effect(false, |fs| fs.sync_file_contents(path))
    }

    fn remembered_package(&self, root: &Path) -> Option<crate::canonical::CurrentPackageCandidate> {
        let remembered = self.remembered.lock().ok()?;
        remembered
            .as_ref()
            .filter(|package| package.context.root.absolute_path == root)
            .cloned()
    }

    fn remember_package(&self, package: &crate::canonical::CurrentPackageCandidate) {
        if let Ok(mut remembered) = self.remembered.lock() {
            *remembered = Some(package.clone());
        }
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Interrupted> {
        self.effect(true, |fs| fs.replace_file(source, destination))
    }

    fn remove_file(&self, path: &Path) -> Result<(), Interrupted> {
        self.effect(true, |fs| fs.remove_file(path))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Interrupted> {
        self.effect(false, |fs| fs.canonicalize(path))
    }

    fn entry_name(&self, path: &Path) -> Result<Option<std::ffi::OsString>, Interrupted> {
        self.effect(false, |fs| fs.entry_name(path))
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Interrupted> {
        self.effect(false, |fs| fs.symlink_metadata(path))
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Interrupted> {
        self.effect(false, |fs| fs.observe_file_change(path))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Interrupted> {
        self.effect(false, |fs| fs.read_directory(path))
    }

    fn create_directory(&self, path: &Path) -> Result<(), Interrupted> {
        self.effect(true, |fs| fs.create_directory(path))
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Interrupted> {
        self.effect(true, |fs| fs.remove_directory(path))
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Interrupted> {
        self.effect(true, |fs| fs.link_new_file(source, destination))
    }

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Interrupted> {
        self.effect(true, |fs| fs.rename_new(source, destination))
    }
}
