//! The one filesystem double the core suites script.
//!
//! Every operation reaches a real directory tree through [`PlatformFileSystem`], so the
//! core runs its real code path against real bytes. A test scripts what happens around
//! that: a fault at one operation, an answer that replaces an effect, or an outside
//! writer acting just before or after one. Every call is recorded, so a test can prove
//! what an operation did and did not touch.

use super::{PlatformFileSystem, TemporaryTree};
use crate::ports::{
    DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
    NewFileLinkOutcome, RenameOutcome,
};
use std::{
    cell::{Cell, RefCell},
    ffi::{OsStr, OsString},
    io,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

/// One operation of the core's filesystem port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Read,
    WriteNew,
    SyncFile,
    ReplaceFile,
    RemoveFile,
    Canonicalize,
    EntryName,
    SymlinkMetadata,
    ObserveFileChange,
    ReadDirectory,
    CreateDirectory,
    RemoveDirectory,
    LinkNewFile,
    RenameNew,
}

impl Operation {
    /// Whether the operation changes the tree or makes a change durable.
    pub fn is_effect(self) -> bool {
        matches!(
            self,
            Self::WriteNew
                | Self::SyncFile
                | Self::ReplaceFile
                | Self::RemoveFile
                | Self::CreateDirectory
                | Self::RemoveDirectory
                | Self::LinkNewFile
                | Self::RenameNew
        )
    }
}

/// One recorded call. A two-path operation records its destination as `path` and
/// its source as `source`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub operation: Operation,
    pub path: PathBuf,
    pub source: Option<PathBuf>,
}

impl Call {
    /// Every path the call names.
    pub fn paths(&self) -> Vec<&Path> {
        [Some(self.path.as_path()), self.source.as_deref()]
            .into_iter()
            .flatten()
            .collect()
    }
}

/// What a scripted filesystem call fails with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FsError {
    /// The script injected a failure at this operation.
    Injected(Operation),
    /// The script refused access at this operation.
    Denied(Operation),
    /// The real filesystem failed.
    Io(io::ErrorKind),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Injected(operation) => write!(formatter, "injected failure at {operation:?}"),
            Self::Denied(operation) => write!(formatter, "access denied at {operation:?}"),
            Self::Io(kind) => write!(formatter, "{kind}"),
        }
    }
}

impl From<io::Error> for FsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// A scripted answer that replaces the real effect.
#[derive(Clone, Debug)]
pub enum Answer {
    Bytes(Vec<u8>),
    /// A directory listing in exactly this order.
    Listing(Vec<DirectoryEntry>),
    Renamed(RenameOutcome),
}

#[derive(Clone)]
enum PathMatch {
    Any,
    Exactly(PathBuf),
    EndingIn(PathBuf),
    Where(fn(&Path) -> bool),
}

impl PathMatch {
    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::Any => true,
            Self::Exactly(expected) => path == expected,
            Self::EndingIn(suffix) => path.ends_with(suffix),
            Self::Where(predicate) => predicate(path),
        }
    }
}

#[derive(Clone)]
enum Outcome {
    Real,
    Fail,
    FailWith(io::ErrorKind),
    Deny,
    Answer(Answer),
    Evidence(FileChangeEvidence),
    AlsoListing(Vec<DirectoryEntry>),
}

type Disturbance = Rc<dyn Fn(&Outside<'_>)>;

#[derive(Clone)]
struct Rule {
    operation: Operation,
    path: PathMatch,
    after: Option<(Operation, PathMatch)>,
    condition: Option<Rc<dyn Fn() -> bool>>,
    after_injected_failure: bool,
    skip: usize,
    remaining: Option<usize>,
    before_effect: Option<Disturbance>,
    after_effect: Option<Disturbance>,
    outcome: Outcome,
}

/// Where scripted paths land on disk.
enum Volume {
    /// Paths are real paths.
    Real,
    /// Absolute paths are rooted in a private tree, so `/package` names the same
    /// Package wherever the suite runs. Canonical answers are given in that spelling.
    Private { _tree: TemporaryTree, base: PathBuf },
    /// Real paths below these roots resolve without regard to ASCII case, the way APFS
    /// does in its default macOS configuration, wherever the suite runs.
    CaseInsensitive {
        roots: Vec<PathBuf>,
        trimmed_names: bool,
    },
    /// Real paths below these roots keep every two byte spellings of a name apart, the
    /// way a case- and normalization-sensitive volume does, wherever the suite runs.
    /// Each name is stored escaped to bytes no host volume folds together.
    ByteDistinct { roots: Vec<PathBuf> },
}

impl Volume {
    fn resolve(&self, path: &Path) -> PathBuf {
        match self {
            Self::Real => path.to_path_buf(),
            Self::Private { base, .. } => match path.strip_prefix("/") {
                Ok(relative) => base.join(relative),
                Err(_) => path.to_path_buf(),
            },
            Self::CaseInsensitive {
                roots,
                trimmed_names,
            } => {
                let Some((root, relative)) = roots
                    .iter()
                    .find_map(|root| path.strip_prefix(root).ok().map(|rest| (root, rest)))
                else {
                    return path.to_path_buf();
                };
                let mut resolved = root.clone();
                for component in relative.components() {
                    match component {
                        Component::Normal(name) => resolved.push(
                            existing_spelling(&resolved, name, *trimmed_names)
                                .unwrap_or(name.to_owned()),
                        ),
                        other => resolved.push(other.as_os_str()),
                    }
                }
                resolved
            }
            Self::ByteDistinct { roots } => respelled(roots, path, escaped),
        }
    }

    fn present(&self, real: PathBuf) -> PathBuf {
        match self {
            Self::Private { base, .. } => match real.strip_prefix(base) {
                Ok(relative) => Path::new("/").join(relative),
                Err(_) => real,
            },
            Self::ByteDistinct { roots } => respelled(roots, &real, unescaped),
            _ => real,
        }
    }

    /// The name an entry listed in `directory` has in this volume's spelling.
    fn present_name(&self, directory: &Path, name: OsString) -> OsString {
        match self {
            Self::ByteDistinct { roots }
                if roots.iter().any(|root| directory.starts_with(root)) =>
            {
                unescaped(&name)
            }
            _ => name,
        }
    }
}

/// `path` with every name below one of `roots` passed through `respell`.
fn respelled(roots: &[PathBuf], path: &Path, respell: fn(&OsStr) -> OsString) -> PathBuf {
    let Some((root, relative)) = roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok().map(|rest| (root, rest)))
    else {
        return path.to_path_buf();
    };
    let mut respelled = root.clone();
    for component in relative.components() {
        match component {
            Component::Normal(name) => respelled.push(respell(name)),
            other => respelled.push(other.as_os_str()),
        }
    }
    respelled
}

/// `name` with every byte but a lowercase ASCII letter, a digit, `.`, `-`, and `_` written
/// as `%` and two uppercase hexadecimal digits, so no two names escape alike and no host
/// volume can fold two escapes together.
fn escaped(name: &OsStr) -> OsString {
    let mut escaped = String::new();
    for &byte in name.as_encoded_bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte) {
            escaped.push(char::from(byte));
        } else {
            escaped.push_str(&format!("%{byte:02X}"));
        }
    }
    OsString::from(escaped)
}

/// The name `escaped` stored as `name`.
fn unescaped(name: &OsStr) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    let bytes = name.as_encoded_bytes();
    let mut unescaped = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).expect("an escape");
            unescaped.push(u8::from_str_radix(hex, 16).expect("an escape"));
            index += 3;
        } else {
            unescaped.push(bytes[index]);
            index += 1;
        }
    }
    OsString::from_vec(unescaped)
}

/// The spelling `directory` already holds under this volume's alias rules.
fn existing_spelling(directory: &Path, name: &OsStr, trimmed_names: bool) -> Option<OsString> {
    fn comparable(name: &OsStr, trimmed_names: bool) -> &[u8] {
        let bytes = name.as_encoded_bytes();
        let start = if trimmed_names {
            bytes
                .iter()
                .position(|byte| *byte != b' ')
                .unwrap_or(bytes.len())
        } else {
            0
        };
        let end = if trimmed_names {
            bytes
                .iter()
                .rposition(|byte| !matches!(byte, b'.' | b' '))
                .map_or(0, |last| last + 1)
        } else {
            bytes.len()
        };
        &bytes[start.min(end)..end]
    }
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|candidate| {
            comparable(candidate, trimmed_names)
                .eq_ignore_ascii_case(comparable(name, trimmed_names))
        })
}

/// A writer outside the operation under test, acting at a scripted call.
pub struct Outside<'a> {
    volume: &'a Volume,
    call: &'a Call,
}

impl Outside<'_> {
    /// The call this writer acts beside.
    pub fn call(&self) -> &Call {
        self.call
    }

    pub fn write(&self, path: &Path, bytes: &[u8]) {
        std::fs::write(self.volume.resolve(path), bytes).expect("the outside write lands");
    }

    pub fn remove_file(&self, path: &Path) {
        std::fs::remove_file(self.volume.resolve(path)).expect("the outside removal lands");
    }

    pub fn create_directory(&self, path: &Path) {
        std::fs::create_dir(self.volume.resolve(path)).expect("the outside directory lands");
    }
}

/// The scriptable filesystem. See the module documentation.
pub struct ScriptedFileSystem {
    volume: Volume,
    rules: RefCell<Vec<Rule>>,
    calls: RefCell<Vec<Call>>,
    disturbances: Cell<usize>,
    injected_failures: Cell<usize>,
}

impl Default for ScriptedFileSystem {
    fn default() -> Self {
        Self::over(Volume::Real)
    }
}

impl ScriptedFileSystem {
    /// Real paths, real effects, nothing scripted yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh private tree in which `/` is the tree's root.
    pub fn private(label: &str) -> Self {
        let tree = TemporaryTree::new(label);
        let base = std::fs::canonicalize(tree.path()).expect("the private tree resolves");
        Self::over(Volume::Private { _tree: tree, base })
    }

    /// A real tree seen through a volume that resolves names without regard to ASCII
    /// case. Two spellings that differ only in case name one entry here, so a refusal
    /// that depends on that equivalence is exercised on any volume.
    pub fn case_insensitive_over(root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        match std::fs::canonicalize(root) {
            Ok(canonical) if canonical != root => roots.push(canonical),
            _ => {}
        }
        Self::over(Volume::CaseInsensitive {
            roots,
            trimmed_names: false,
        })
    }

    /// A case-insensitive volume whose lookups also ignore leading ASCII spaces and
    /// trailing periods and ASCII spaces.
    pub fn trimmed_names_over(root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        match std::fs::canonicalize(root) {
            Ok(canonical) if canonical != root => roots.push(canonical),
            _ => {}
        }
        Self::over(Volume::CaseInsensitive {
            roots,
            trimmed_names: true,
        })
    }

    /// A real tree seen through a volume that keeps every two byte spellings of a name
    /// apart. Two spellings that differ only in case or Unicode normalization name two
    /// entries here, so a rule that must not merge them is exercised on any volume.
    pub fn byte_distinct_over(root: &Path) -> Self {
        let mut roots = vec![root.to_path_buf()];
        match std::fs::canonicalize(root) {
            Ok(canonical) if canonical != root => roots.push(canonical),
            _ => {}
        }
        Self::over(Volume::ByteDistinct { roots })
    }

    fn over(volume: Volume) -> Self {
        Self {
            volume,
            rules: RefCell::new(Vec::new()),
            calls: RefCell::new(Vec::new()),
            disturbances: Cell::new(0),
            injected_failures: Cell::new(0),
        }
    }

    /// Scripts `operation`. The most recently scripted matching rule applies.
    pub fn on(&self, operation: Operation) -> Script<'_> {
        Script {
            filesystem: self,
            rule: Rule {
                operation,
                path: PathMatch::Any,
                after: None,
                condition: None,
                after_injected_failure: false,
                skip: 0,
                remaining: None,
                before_effect: None,
                after_effect: None,
                outcome: Outcome::Real,
            },
        }
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// Every call that named a path `named` holds for, whatever the call did.
    pub fn naming(&self, named: impl Fn(&Path) -> bool) -> Vec<Call> {
        self.calls
            .borrow()
            .iter()
            .filter(|call| call.paths().into_iter().any(&named))
            .cloned()
            .collect()
    }

    /// Every call that changed the tree or made a change durable.
    pub fn effects(&self) -> Vec<Call> {
        self.calls
            .borrow()
            .iter()
            .filter(|call| call.operation.is_effect())
            .cloned()
            .collect()
    }

    /// Whether a scripted outside writer has acted. A test whose operation succeeds
    /// under contention cannot see the other writer's bytes afterwards, so without
    /// this the contention it claims to exercise has no observer.
    pub fn disturbed(&self) -> bool {
        self.disturbances.get() > 0
    }

    /// Writes a file outside any operation, creating its parents and replacing
    /// whatever entry stood there.
    pub fn put_file(&self, path: impl AsRef<Path>, bytes: &[u8]) {
        let real = self.vacate(path.as_ref());
        std::fs::write(real, bytes).expect("the file is written");
    }

    /// Creates a directory and its parents outside any operation.
    fn put_directory(&self, path: impl AsRef<Path>) {
        let real = self.volume.resolve(path.as_ref());
        if !real.is_dir() {
            self.vacate(path.as_ref());
        }
        std::fs::create_dir_all(real).expect("the directory is created");
    }

    /// Creates a symbolic link to `target`, in this volume's spelling, replacing
    /// whatever entry stood there.
    fn put_symlink(&self, path: impl AsRef<Path>, target: impl AsRef<Path>) {
        let real = self.vacate(path.as_ref());
        std::os::unix::fs::symlink(self.volume.resolve(target.as_ref()), real)
            .expect("the symbolic link is created");
    }

    /// Clears `path` and creates its parents, returning where it is on disk.
    fn vacate(&self, path: &Path) -> PathBuf {
        let real = self.volume.resolve(path);
        if self.exists(path) {
            self.remove(path);
        }
        std::fs::create_dir_all(real.parent().expect("an entry has a parent"))
            .expect("the entry's parents are created");
        real
    }

    /// Creates an entry of `kind` outside any operation: an empty file, a directory, a
    /// symbolic link to nothing, or a named pipe as the kind that is neither.
    pub fn put_entry(&self, path: impl AsRef<Path>, kind: FileSystemEntryKind) {
        let path = path.as_ref();
        match kind {
            FileSystemEntryKind::File => self.put_file(path, b""),
            FileSystemEntryKind::Directory => self.put_directory(path),
            FileSystemEntryKind::SymbolicLink => self.put_symlink(path, "/nothing-here"),
            FileSystemEntryKind::Other => {
                use std::os::unix::ffi::OsStrExt;
                let real = self.vacate(path);
                let name = std::ffi::CString::new(real.as_os_str().as_bytes())
                    .expect("a fixture path has no nul byte");
                // SAFETY: `name` is a nul-terminated C string that outlives the call.
                let code = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
                assert_eq!(code, 0, "the named pipe is created");
            }
        }
    }

    pub fn with_file(self, path: impl AsRef<Path>, bytes: &[u8]) -> Self {
        self.put_file(path, bytes);
        self
    }

    /// The same tree, with a symbolic link at `path` to `target`.
    pub fn with_link(self, path: impl AsRef<Path>, target: impl AsRef<Path>) -> Self {
        self.put_symlink(path, target);
        self
    }

    pub fn with_directory(self, path: impl AsRef<Path>) -> Self {
        self.put_directory(path);
        self
    }

    pub fn with_entry(self, path: impl AsRef<Path>, kind: FileSystemEntryKind) -> Self {
        self.put_entry(path, kind);
        self
    }

    pub fn with_symlink(self, path: impl AsRef<Path>, target: impl AsRef<Path>) -> Self {
        self.put_symlink(path, target);
        self
    }

    /// Change observations of `path` report this comparable evidence.
    pub fn put_change_evidence(
        &self,
        path: impl AsRef<Path>,
        comparison_domain: u64,
        sequence: u128,
    ) {
        self.on(Operation::ObserveFileChange)
            .at(path)
            .reports_evidence(FileChangeEvidence::Comparable {
                comparison_domain,
                sequence,
            });
    }

    /// Every call to `operation` at exactly `path` fails.
    pub fn failing(self, operation: Operation, path: impl AsRef<Path>) -> Self {
        self.on(operation).at(path).fails();
        self
    }

    /// Every call to `operation` whose path ends in `suffix` fails.
    pub fn failing_ending_in(self, operation: Operation, suffix: impl AsRef<Path>) -> Self {
        self.on(operation).ending_in(suffix).fails();
        self
    }

    /// Every call to `operation` at exactly `path` is refused access.
    pub fn denying(self, operation: Operation, path: impl AsRef<Path>) -> Self {
        self.on(operation).at(path).denies_access();
        self
    }

    /// Removes a file or a whole directory outside any operation.
    pub fn remove(&self, path: impl AsRef<Path>) {
        let real = self.volume.resolve(path.as_ref());
        match std::fs::symlink_metadata(&real) {
            Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(real),
            Ok(_) => std::fs::remove_file(real),
            Err(error) => Err(error),
        }
        .expect("the entry is removed");
    }

    /// The bytes at `path`, read outside any operation.
    pub fn bytes(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        std::fs::read(self.volume.resolve(path.as_ref())).ok()
    }

    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        std::fs::symlink_metadata(self.volume.resolve(path.as_ref())).is_ok()
    }

    fn applicable(&self, call: &Call) -> Option<Rule> {
        let calls = self.calls.borrow();
        let mut rules = self.rules.borrow_mut();
        for rule in rules.iter_mut().rev() {
            if rule.operation != call.operation || !rule.path.matches(&call.path) {
                continue;
            }
            if let Some((operation, path)) = &rule.after
                && !calls
                    .iter()
                    .any(|earlier| earlier.operation == *operation && path.matches(&earlier.path))
            {
                continue;
            }
            if rule.condition.as_ref().is_some_and(|holds| !holds())
                || (rule.after_injected_failure && self.injected_failures.get() == 0)
            {
                continue;
            }
            if rule.skip > 0 {
                rule.skip -= 1;
                continue;
            }
            match &mut rule.remaining {
                Some(0) => continue,
                Some(remaining) => *remaining -= 1,
                None => {}
            }
            return Some(rule.clone());
        }
        None
    }

    fn perform<T>(
        &self,
        call: Call,
        real: impl FnOnce(&Volume) -> io::Result<T>,
        answered: impl FnOnce(Answer) -> Option<T>,
        adjust: impl FnOnce(T, &Outcome) -> T,
    ) -> Result<T, FsError> {
        let rule = self.applicable(&call);
        self.calls.borrow_mut().push(call.clone());
        let outside = Outside {
            volume: &self.volume,
            call: &call,
        };
        if let Some(disturbance) = rule.as_ref().and_then(|rule| rule.before_effect.as_ref()) {
            disturbance(&outside);
            self.disturbances.set(self.disturbances.get() + 1);
        }
        let outcome = rule
            .as_ref()
            .map_or(Outcome::Real, |rule| rule.outcome.clone());
        let failure = match &outcome {
            Outcome::Fail => Some(FsError::Injected(call.operation)),
            Outcome::FailWith(kind) => Some(FsError::Io(*kind)),
            Outcome::Deny => Some(FsError::Denied(call.operation)),
            _ => None,
        };
        if let Some(failure) = failure {
            self.injected_failures.set(self.injected_failures.get() + 1);
            return Err(failure);
        }
        let value = match outcome {
            Outcome::Answer(answer) => answered(answer.clone()).unwrap_or_else(|| {
                panic!("{answer:?} cannot answer {:?}", call.operation);
            }),
            other => adjust(real(&self.volume)?, &other),
        };
        if let Some(disturbance) = rule.as_ref().and_then(|rule| rule.after_effect.as_ref()) {
            disturbance(&outside);
            self.disturbances.set(self.disturbances.get() + 1);
        }
        Ok(value)
    }

    fn one(&self, operation: Operation, path: &Path) -> Call {
        Call {
            operation,
            path: path.to_path_buf(),
            source: None,
        }
    }

    fn two(&self, operation: Operation, source: &Path, destination: &Path) -> Call {
        Call {
            operation,
            path: destination.to_path_buf(),
            source: Some(source.to_path_buf()),
        }
    }
}

fn unadjusted<T>(value: T, outcome: &Outcome) -> T {
    assert!(
        matches!(outcome, Outcome::Real),
        "this outcome does not apply to this operation"
    );
    value
}

fn no_answer<T>(_: Answer) -> Option<T> {
    None
}

/// One rule being scripted. It takes effect when a terminal method names its outcome.
#[must_use = "a script takes effect only when its outcome is named"]
pub struct Script<'a> {
    filesystem: &'a ScriptedFileSystem,
    rule: Rule,
}

impl Script<'_> {
    /// Only calls at exactly this path (a two-path operation's destination).
    pub fn at(mut self, path: impl AsRef<Path>) -> Self {
        self.rule.path = PathMatch::Exactly(path.as_ref().to_path_buf());
        self
    }

    /// Only calls whose path (a two-path operation's destination) ends in `suffix`.
    pub fn ending_in(mut self, suffix: impl AsRef<Path>) -> Self {
        self.rule.path = PathMatch::EndingIn(suffix.as_ref().to_path_buf());
        self
    }

    /// Only calls whose path (a two-path operation's destination) satisfies `predicate`.
    pub fn matching(mut self, predicate: fn(&Path) -> bool) -> Self {
        self.rule.path = PathMatch::Where(predicate);
        self
    }

    /// Only once some earlier call to `operation` ended in `suffix`.
    pub fn after(mut self, operation: Operation, suffix: impl AsRef<Path>) -> Self {
        self.rule.after = Some((
            operation,
            PathMatch::EndingIn(suffix.as_ref().to_path_buf()),
        ));
        self
    }

    /// Only once some earlier call to `operation` was made, at any path.
    pub fn after_any(mut self, operation: Operation) -> Self {
        self.rule.after = Some((operation, PathMatch::Any));
        self
    }

    /// Only once the script has failed some earlier call, so a cleanup fault answers
    /// the failure that asked for the cleanup.
    pub fn after_an_injected_failure(mut self) -> Self {
        self.rule.after_injected_failure = true;
        self
    }

    /// Only while `condition` holds of the world outside the filesystem's record.
    pub fn when(mut self, condition: impl Fn() -> bool + 'static) -> Self {
        self.rule.condition = Some(Rc::new(condition));
        self
    }

    /// Passes over the first `count` matching calls, leaving them to older rules or
    /// to the real effect.
    pub fn skipping(mut self, count: usize) -> Self {
        self.rule.skip = count;
        self
    }

    /// Applies to the next matching call only.
    pub fn once(mut self) -> Self {
        self.rule.remaining = Some(1);
        self
    }

    /// An outside writer acts just before the call.
    pub fn preceded_by(mut self, disturbance: impl Fn(&Outside<'_>) + 'static) -> Self {
        self.rule.before_effect = Some(Rc::new(disturbance));
        self
    }

    /// An outside writer acts just after the call succeeds.
    pub fn followed_by(mut self, disturbance: impl Fn(&Outside<'_>) + 'static) -> Self {
        self.rule.after_effect = Some(Rc::new(disturbance));
        self
    }

    fn outcome(mut self, outcome: Outcome) {
        self.rule.outcome = outcome;
        self.filesystem.rules.borrow_mut().push(self.rule);
    }

    /// The real effect runs; only the disturbances apply.
    pub fn proceeds(self) {
        self.outcome(Outcome::Real);
    }

    /// The call fails with [`FsError::Injected`] and has no effect.
    pub fn fails(self) {
        self.outcome(Outcome::Fail);
    }

    /// The call fails as the real filesystem would with `kind`, and has no effect.
    pub fn fails_with(self, kind: io::ErrorKind) {
        self.outcome(Outcome::FailWith(kind));
    }

    /// The call is refused with [`FsError::Denied`] and has no effect.
    pub fn denies_access(self) {
        self.outcome(Outcome::Deny);
    }

    /// The call answers this instead of performing its effect.
    pub fn answers(self, answer: Answer) {
        self.outcome(Outcome::Answer(answer));
    }

    /// A change observation reports this evidence beside the real entry kind.
    pub fn reports_evidence(self, evidence: FileChangeEvidence) {
        self.outcome(Outcome::Evidence(evidence));
    }

    /// A directory listing reports these entries beside the real ones.
    pub fn lists_also(self, entries: Vec<DirectoryEntry>) {
        self.outcome(Outcome::AlsoListing(entries));
    }
}

impl FileSystem for ScriptedFileSystem {
    type Error = FsError;

    fn is_access_denied(&self, error: &Self::Error) -> bool {
        matches!(
            error,
            FsError::Denied(_) | FsError::Io(io::ErrorKind::PermissionDenied)
        )
    }

    fn is_unsupported(&self, error: &Self::Error) -> bool {
        matches!(error, FsError::Io(io::ErrorKind::Unsupported))
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.perform(
            self.one(Operation::Read, path),
            |volume| PlatformFileSystem.read(&volume.resolve(path)),
            |answer| match answer {
                Answer::Bytes(bytes) => Some(bytes),
                _ => None,
            },
            unadjusted,
        )
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.perform(
            self.one(Operation::WriteNew, path),
            |volume| PlatformFileSystem.write_new(&volume.resolve(path), contents),
            no_answer,
            unadjusted,
        )
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.perform(
            self.one(Operation::SyncFile, path),
            |volume| PlatformFileSystem.sync_file(&volume.resolve(path)),
            no_answer,
            unadjusted,
        )
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.perform(
            self.two(Operation::ReplaceFile, source, destination),
            |volume| {
                PlatformFileSystem
                    .replace_file(&volume.resolve(source), &volume.resolve(destination))
            },
            no_answer,
            unadjusted,
        )
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.perform(
            self.one(Operation::RemoveFile, path),
            |volume| PlatformFileSystem.remove_file(&volume.resolve(path)),
            no_answer,
            unadjusted,
        )
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.perform(
            self.one(Operation::Canonicalize, path),
            |volume| {
                PlatformFileSystem
                    .canonicalize(&volume.resolve(path))
                    .map(|real| volume.present(real))
            },
            no_answer,
            unadjusted,
        )
    }

    fn entry_name(&self, path: &Path) -> Result<Option<OsString>, Self::Error> {
        self.perform(
            self.one(Operation::EntryName, path),
            |volume| {
                PlatformFileSystem
                    .entry_name(&volume.resolve(path))
                    .map(|name| name.map(|name| volume.present_name(path, name)))
            },
            no_answer,
            unadjusted,
        )
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.perform(
            self.one(Operation::SymlinkMetadata, path),
            |volume| PlatformFileSystem.symlink_metadata(&volume.resolve(path)),
            no_answer,
            unadjusted,
        )
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.perform(
            self.one(Operation::ObserveFileChange, path),
            |volume| PlatformFileSystem.observe_file_change(&volume.resolve(path)),
            no_answer,
            |observation, outcome| match outcome {
                // An entry that is not there has no evidence to report.
                Outcome::Evidence(evidence) if observation.kind.is_some() => {
                    FileChangeObservation {
                        evidence: *evidence,
                        ..observation
                    }
                }
                Outcome::Evidence(_) => observation,
                other => unadjusted(observation, other),
            },
        )
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.perform(
            self.one(Operation::ReadDirectory, path),
            |volume| {
                PlatformFileSystem
                    .read_directory(&volume.resolve(path))
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|entry| DirectoryEntry {
                                name: volume.present_name(path, entry.name),
                                kind: entry.kind,
                            })
                            .collect()
                    })
            },
            |answer| match answer {
                Answer::Listing(entries) => Some(entries),
                _ => None,
            },
            |mut entries, outcome| match outcome {
                Outcome::AlsoListing(extra) => {
                    entries.extend(extra.iter().cloned());
                    entries
                }
                other => unadjusted(entries, other),
            },
        )
    }

    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.perform(
            self.one(Operation::CreateDirectory, path),
            |volume| PlatformFileSystem.create_directory(&volume.resolve(path)),
            no_answer,
            unadjusted,
        )
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.perform(
            self.one(Operation::RemoveDirectory, path),
            |volume| PlatformFileSystem.remove_directory(&volume.resolve(path)),
            no_answer,
            unadjusted,
        )
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        self.perform(
            self.two(Operation::LinkNewFile, source, destination),
            |volume| {
                PlatformFileSystem
                    .link_new_file(&volume.resolve(source), &volume.resolve(destination))
            },
            no_answer,
            unadjusted,
        )
    }

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error> {
        self.perform(
            self.two(Operation::RenameNew, source, destination),
            |volume| {
                PlatformFileSystem.rename_new(&volume.resolve(source), &volume.resolve(destination))
            },
            |answer| match answer {
                Answer::Renamed(outcome) => Some(outcome),
                _ => None,
            },
            unadjusted,
        )
    }
}
