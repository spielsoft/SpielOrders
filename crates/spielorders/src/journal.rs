//! One commit journal for every effect a commit performs.
//!
//! A capability plans its commit as a journal: the ordered steps the commit takes, each
//! with the operation a dry run lists for it. The same journal both lists those
//! operations and performs them, so what a plan says it will do and what its commit does
//! cannot disagree. Running it performs the steps in order; where one fails, the journal
//! undoes the steps it completed, newest first, and reports what it could not undo.
//!
//! An undo touches a path only while that path still holds what this commit put there:
//! a created entry is removed only while change evidence proves it is the one the commit
//! created, a replaced record is restored only while it still holds exactly the bytes
//! the commit wrote, and a moved entry goes back only to a name nothing has taken since.
//! Where that cannot be proven, including on a volume that keeps no comparable change
//! evidence, the entry is left as it is and named instead. The rules are in
//! `docs/architecture/durable-writes.md`.

use crate::{
    Failure, Warning,
    canonical::Touched,
    change::PlannedAct,
    filesystem::staging_path,
    integrity::Aggregate,
    metadata::{OrderDocument, VendorsDocument},
    mutation::{CleanupFailure, CommitFailure, IndexRefresh, MutationReport, OperationKind},
    ports::{
        FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
        NewFileLinkOutcome, RenameOutcome,
    },
    readme::{Location, located_in_package, package_relative_path_text},
};
use std::path::{Path, PathBuf};

/// What one operation of an exclusive creation does at its destination. A directory is
/// never replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreationAct {
    CreateDirectory,
    CreateFile,
    PreserveDirectory,
    PreserveFile,
    ReplaceFile,
}

/// What a replacement expects its destination to hold immediately before it is taken
/// over. Which one a caller carries is decided in "Preserve evidence and other writers"
/// in `docs/architecture/durable-writes.md`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expect {
    /// Nothing stands there: observed absent, which is a real observation rather than a
    /// missing one, and checked by observing the path, because a read cannot tell nothing
    /// from something unreadable.
    Absent,
    /// Exactly these bytes, which are also what an undo puts back.
    Bytes(Vec<u8>),
    /// Bytes that still mean this; an undo puts back the bytes found there.
    Meaning(Box<Meaning>),
}

/// What one managed record means, as the Package load projects it.
///
/// Meaning rather than bytes, because a record a person reformatted by hand, or whose
/// stored facts carry untrimmed text, projects to the same Order or Vendors and is still
/// the record a plan was made against. `docs/architecture/durable-writes.md` decides
/// where meaning is the evidence a write carries.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Meaning {
    Order(OrderDocument),
    Vendors(VendorsDocument),
}

impl Meaning {
    /// Whether `bytes` still project to this meaning. Bytes that do not read as the record
    /// at all do not.
    pub(crate) fn held_by(&self, bytes: &[u8]) -> bool {
        match self {
            Self::Order(document) => OrderDocument::from_json_bytes(bytes)
                .ok()
                .and_then(|parsed| parsed.validate().ok())
                .is_some_and(|projected| projected.order() == document),
            // The aggregate alone is judged, as the load judges it: this asks whether one
            // file still means what was read out of it, and duplicate Order IDs or
            // unresolved Vendor References are about state that does not live in it.
            Self::Vendors(document) => VendorsDocument::from_json_bytes(bytes)
                .ok()
                .and_then(|parsed| Aggregate::judge(&parsed).ok())
                .is_some_and(|projected| projected.document() == document),
        }
    }
}

/// One managed record a commit replaces, and what it meant when the commit was planned.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ManagedRecord {
    path: PathBuf,
    meaning: Meaning,
}

impl ManagedRecord {
    /// The Order record at `path`, planned against `document`.
    pub(crate) fn order(path: PathBuf, document: OrderDocument) -> Self {
        Self {
            path,
            meaning: Meaning::Order(document),
        }
    }

    /// The Vendor record at `path`, planned against `document`.
    pub(crate) fn vendors(path: PathBuf, document: VendorsDocument) -> Self {
        Self {
            path,
            meaning: Meaning::Vendors(document),
        }
    }

    /// The check a replacement of this record makes immediately before taking it over.
    pub(crate) fn expectation(&self) -> Expect {
        Expect::Meaning(Box::new(self.meaning.clone()))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `bytes` still mean what this record was planned against.
    pub(crate) fn held_by(&self, bytes: &[u8]) -> bool {
        self.meaning.held_by(bytes)
    }

    /// The record's bytes as they stand, read once more and confirmed to still mean what
    /// it was planned against. A commit replacing several records together then expects
    /// exactly these bytes, so a writer landing after this read is refused rather than
    /// overwritten. Refused as a changed precondition, inside the Root at `root`, where the
    /// record cannot be read or means something else.
    pub(crate) fn confirm<F: FileSystem>(
        &self,
        root: &Path,
        filesystem: &F,
    ) -> Result<Vec<u8>, Failure> {
        match filesystem.read(&self.path) {
            Ok(bytes) if self.held_by(&bytes) => Ok(bytes),
            _ => Err(Failure::PreconditionsChanged {
                location: located_in_package(root, self.path.clone()),
            }),
        }
    }
}

/// Which record a step writes, so a commit knows which records the index refresh after
/// it must read again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Subject {
    /// The Order Directory at this path.
    Order(PathBuf),
    /// The Vendor record.
    Vendors,
    /// Nothing the Package Index holds.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
enum Effect {
    /// Nothing is done; the entry must still be the kind it was planned against.
    Preserve {
        path: PathBuf,
        kind: FileSystemEntryKind,
    },
    /// One new directory, never an existing one.
    CreateDirectory { path: PathBuf },
    /// One new file inside a directory this commit created and has not yet published,
    /// written in place, read back, and made durable.
    WriteUnpublished { path: PathBuf, bytes: Vec<u8> },
    /// One new file published beside existing entries: staged, read back, made durable,
    /// and linked to a name nothing holds, after which the stage is removed.
    CreateFile { path: PathBuf, bytes: Vec<u8> },
    /// One existing record replaced with these bytes. Adjacent replacements are staged and
    /// made durable together before any of them is published.
    Replace {
        path: PathBuf,
        bytes: Vec<u8>,
        expect: Expect,
    },
    /// One entry moved to a name nothing holds.
    Move { from: PathBuf, to: PathBuf },
    /// Nothing is done; the step only lists what the commit leaves to a later one.
    Announce,
    /// A stage this commit did not write, published at `destination` as it stands.
    Promote {
        stage: PathBuf,
        destination: PathBuf,
        /// What the stage was confirmed to hold.
        bytes: Vec<u8>,
        expect: Expect,
    },
    /// The stage beside `destination` written and made durable, to be linked there.
    Stage {
        destination: PathBuf,
        bytes: Vec<u8>,
    },
    /// The stage beside `destination` linked to it, a name nothing holds.
    Link { destination: PathBuf },
    /// The stage beside `destination` removed, once its contents are published.
    RemoveStage { destination: PathBuf },
}

#[derive(Clone, Debug, PartialEq)]
struct Step {
    effect: Effect,
    act: PlannedAct,
    /// Where a dry run lists the step, and where a commit that stops at it says it
    /// stopped.
    location: Location,
    /// Whether the entry the step acts on is a directory, for the steps that act on a
    /// merged Order's entries.
    directory: bool,
    subject: Subject,
}

/// The ordered steps one commit performs. See the module documentation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Journal {
    root: PathBuf,
    required: Vec<(PathBuf, Option<FileSystemEntryKind>)>,
    steps: Vec<Step>,
    /// How many steps publish the commit; every later step only tidies up.
    commit_point: Option<usize>,
    /// Order Directories the commit changes beyond the records its steps name.
    touches: Vec<PathBuf>,

    refresh: Option<Location>,
}

impl Journal {
    /// An empty journal of a commit inside the Root at `root`.
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            required: Vec::new(),
            steps: Vec::new(),
            commit_point: None,
            touches: Vec::new(),
            refresh: None,
        }
    }

    /// The Root this commit writes inside.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn push(&mut self, effect: Effect, act: PlannedAct, path: &Path, subject: Subject) {
        let location = located_in_package(&self.root, path.to_path_buf());
        self.steps.push(Step {
            effect,
            act,
            location,
            directory: false,
            subject,
        });
    }

    /// Requires the entry at `path` to still be `kind`, or absent for `None`, before the
    /// first effect; a commit that finds otherwise changes nothing.
    pub(crate) fn require(&mut self, path: PathBuf, kind: Option<FileSystemEntryKind>) {
        self.required.push((path, kind));
    }

    /// Requires every step planned so far to still find what it was planned against:
    /// nothing where it creates, the entry it preserves, a file where it replaces one, and
    /// no stage where it stages.
    pub(crate) fn require_planned(&mut self) {
        let mut required = Vec::new();
        for step in &self.steps {
            match &step.effect {
                Effect::Preserve { path, kind } => required.push((path.clone(), Some(*kind))),
                Effect::CreateDirectory { path } | Effect::WriteUnpublished { path, .. } => {
                    required.push((path.clone(), None));
                }
                Effect::CreateFile { path, .. } => {
                    required.push((path.clone(), None));
                    required.push((
                        staging_path(path).expect("a created file has a basename"),
                        None,
                    ));
                }
                Effect::Replace { path, .. } => {
                    required.push((path.clone(), Some(FileSystemEntryKind::File)));
                }
                Effect::Move { to, .. } => required.push((to.clone(), None)),
                Effect::Stage { destination, .. } => required.push((
                    staging_path(destination).expect("a staged file has a basename"),
                    None,
                )),
                Effect::Link { destination } => required.push((destination.clone(), None)),
                Effect::RemoveStage { .. } | Effect::Announce | Effect::Promote { .. } => {}
            }
        }
        self.required.extend(required);
    }

    /// Lists the entry at `path`, which the commit leaves as it is.
    pub(crate) fn preserve(&mut self, path: PathBuf, kind: FileSystemEntryKind, act: PlannedAct) {
        self.push(
            Effect::Preserve {
                path: path.clone(),
                kind,
            },
            act,
            &path,
            Subject::Other,
        );
    }

    pub(crate) fn create_directory(&mut self, path: PathBuf, act: PlannedAct) {
        self.push(
            Effect::CreateDirectory { path: path.clone() },
            act,
            &path,
            Subject::Other,
        );
    }

    pub(crate) fn write_unpublished(&mut self, path: PathBuf, bytes: Vec<u8>, act: PlannedAct) {
        self.push(
            Effect::WriteUnpublished {
                path: path.clone(),
                bytes,
            },
            act,
            &path,
            Subject::Other,
        );
    }

    pub(crate) fn create_file(&mut self, path: PathBuf, bytes: Vec<u8>, act: PlannedAct) {
        self.push(
            Effect::CreateFile {
                path: path.clone(),
                bytes,
            },
            act,
            &path,
            Subject::Other,
        );
    }

    pub(crate) fn replace(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        expect: Expect,
        act: PlannedAct,
        subject: Subject,
    ) {
        self.push(
            Effect::Replace {
                path: path.clone(),
                bytes,
                expect,
            },
            act,
            &path,
            subject,
        );
    }

    /// Moves the entry at `from` to `to`, listed at `to`.
    pub(crate) fn move_entry(
        &mut self,
        from: PathBuf,
        to: PathBuf,
        act: PlannedAct,
        subject: Subject,
    ) {
        let listed = to.clone();
        self.push(Effect::Move { from, to }, act, &listed, subject);
    }

    /// Moves the entry at `from`, a directory where `directory`, to `to`, listed at `from`.
    pub(crate) fn move_out(
        &mut self,
        from: PathBuf,
        to: PathBuf,
        directory: bool,
        act: PlannedAct,
    ) {
        let listed = from.clone();
        self.push(Effect::Move { from, to }, act, &listed, Subject::Other);
        self.mark_directory(directory);
    }

    /// Lists the entry at `path`, a directory where `directory`, which this commit leaves
    /// for a later one to act on.
    pub(crate) fn announce(&mut self, path: PathBuf, directory: bool, act: PlannedAct) {
        self.push(Effect::Announce, act, &path, Subject::Other);
        self.mark_directory(directory);
    }

    fn mark_directory(&mut self, directory: bool) {
        if let Some(step) = self.steps.last_mut() {
            step.directory = directory;
        }
    }

    /// Lists `other`'s steps after this journal's, creating each directory only once.
    /// Listing only: the combined journal is never run, because each part is confirmed
    /// and run on its own.
    pub(crate) fn listing_after(&mut self, other: &Journal) {
        for step in &other.steps {
            let created_already = matches!(&step.effect, Effect::CreateDirectory { path }
            if self.steps.iter().any(|mine| {
                matches!(&mine.effect, Effect::CreateDirectory { path: own } if own == path)
            }));
            if !created_already {
                self.steps.push(step.clone());
            }
        }
    }

    /// Records that this commit changes the Order Directories `orders` beyond the records
    /// its steps name, so the index refresh after it reads them again.
    pub(crate) fn touching(&mut self, orders: impl IntoIterator<Item = PathBuf>) {
        self.touches.extend(orders);
    }

    /// Publishes the stage at `stage`, which this commit did not write and which was
    /// confirmed to hold `bytes`, at `destination`, listed at the destination. The stage is
    /// evidence of an interrupted write rather than this commit's own, so nothing removes
    /// it, whatever happens.
    pub(crate) fn promote(
        &mut self,
        (stage, bytes): (PathBuf, Vec<u8>),
        destination: PathBuf,
        expect: Expect,
        act: PlannedAct,
        subject: Subject,
    ) {
        let listed = destination.clone();
        self.push(
            Effect::Promote {
                stage,
                destination,
                bytes,
                expect,
            },
            act,
            &listed,
            subject,
        );
    }

    /// Stages `bytes` beside `destination`, listed at the stage.
    pub(crate) fn stage(&mut self, destination: PathBuf, bytes: Vec<u8>, act: PlannedAct) {
        let stage = staging_path(&destination).expect("a staged file has a basename");
        self.push(
            Effect::Stage { destination, bytes },
            act,
            &stage,
            Subject::Other,
        );
    }

    /// Links the stage beside `destination` to it.
    pub(crate) fn link(&mut self, destination: PathBuf, act: PlannedAct, subject: Subject) {
        let listed = destination.clone();
        self.push(Effect::Link { destination }, act, &listed, subject);
    }

    /// Removes the stage beside `destination`, listed at the stage. A stage that cannot be
    /// removed is reported as a committed warning naming it.
    pub(crate) fn remove_stage(&mut self, destination: PathBuf, act: PlannedAct) {
        let stage = staging_path(&destination).expect("a staged file has a basename");
        self.push(
            Effect::RemoveStage { destination },
            act,
            &stage,
            Subject::Other,
        );
    }

    /// Marks the steps planned so far as the ones that publish the commit. Every later
    /// step only tidies up, removing a stage or rewriting a moved Order's locator: nothing
    /// undoes a published step, and a later step that fails is a warning of a commit that
    /// stands.
    pub(crate) fn commit_point(&mut self) {
        self.commit_point = Some(self.steps.len());
    }

    /// Ends the commit with a refresh of the Package Index at `index`.
    pub(crate) fn refreshing(&mut self, index: Location) {
        self.refresh = Some(index);
    }

    /// For each operation in [`Self::operations`] order, whether it acts on a directory.
    pub(crate) fn directories(&self) -> Vec<bool> {
        self.steps
            .iter()
            .map(|step| step.directory)
            .chain(self.refresh.iter().map(|_| false))
            .collect()
    }

    /// Each operation committing this journal performs, in the order it performs them.
    pub(crate) fn operations(&self) -> Vec<(PlannedAct, &Location)> {
        self.steps
            .iter()
            .map(|step| (step.act, &step.location))
            .chain(self.refresh.iter().map(|index| {
                (
                    PlannedAct::Managed(OperationKind::RefreshPackageIndex),
                    index,
                )
            }))
            .collect()
    }

    /// The records committing this journal writes.
    fn touched(&self) -> Touched {
        let orders = self
            .steps
            .iter()
            .filter_map(|step| match &step.subject {
                Subject::Order(directory) => Some(directory.clone()),
                _ => None,
            })
            .chain(self.touches.iter().cloned());
        Touched::orders(orders).with_vendors(
            self.steps
                .iter()
                .any(|step| step.subject == Subject::Vendors),
        )
    }

    /// Performs every step in order. See [`Self::run_checking`].
    pub(crate) fn run<F: FileSystem>(&self, filesystem: &F) -> Result<Committed, Failure> {
        self.run_checking(filesystem, |_, _| None)
    }

    /// Performs every step in order, asking `after_directory` about each directory just
    /// after creating it; a failure it names stops the commit as a failed step does.
    pub(crate) fn run_checking<F: FileSystem>(
        &self,
        filesystem: &F,
        after_directory: impl Fn(&Path, &F) -> Option<CommitFailure>,
    ) -> Result<Committed, Failure> {
        self.check_required(filesystem)?;
        let mut run = Run {
            root: &self.root,
            filesystem,
            done: Vec::new(),
            committed: Vec::new(),
        };
        let mut warnings = Vec::new();
        let mut index = 0;
        while index < self.steps.len() {
            let point = self.commit_point.unwrap_or(self.steps.len());
            let tidy = index >= point;
            if tidy {
                // A published commit stands; keep only the ownership its tidy-up needs.
                run.seal();
            }
            let mut next = index + 1;
            let outcome = match &self.steps[index].effect {
                Effect::Preserve { .. } => Ok(()),
                Effect::CreateDirectory { path } => run.create_directory(path).and_then(|()| {
                    match after_directory(path, filesystem) {
                        Some(failure) => Err(Failure::commit(failure, run.undo())),
                        None => Ok(()),
                    }
                }),
                Effect::WriteUnpublished { path, bytes } => run.write_unpublished(path, bytes),
                Effect::CreateFile { path, bytes } => run.create_file(path, bytes),
                Effect::Replace { .. } => {
                    let end = if tidy { self.steps.len() } else { point };
                    let group = self.steps[index..end]
                        .iter()
                        .map_while(|step| match &step.effect {
                            Effect::Replace {
                                path,
                                bytes,
                                expect,
                            } => Some((path.as_path(), bytes.as_slice(), expect)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    next = index + group.len();
                    run.replace(&group)
                }
                Effect::Move { from, to } => {
                    run.move_entry(from, to, &self.steps[index].location.absolute_path)
                }
                Effect::Announce => Ok(()),
                Effect::Promote {
                    stage,
                    destination,
                    bytes,
                    expect,
                } => run.promote(stage, destination, bytes, expect),
                Effect::Stage { destination, bytes } => run.stage(destination, bytes),
                Effect::Link { destination } => run.link(destination),
                Effect::RemoveStage { destination } => {
                    if let Err(left) = run.remove_stage(destination) {
                        warnings.push(Warning::StagingCleanupFailed(left));
                    }
                    Ok(())
                }
            };
            match outcome {
                Ok(()) => {}
                Err(failure) if tidy => warnings.push(Warning::LocatorUpdateFailed(failure)),
                Err(failure) => return Err(failure),
            }
            index = next;
        }
        Ok(Committed {
            touched: self.touched(),
            warnings,
            changed: self
                .steps
                .iter()
                .any(|step| !matches!(step.effect, Effect::Preserve { .. })),
        })
    }

    fn check_required<F: FileSystem>(&self, filesystem: &F) -> Result<(), Failure> {
        for (path, expected) in &self.required {
            match filesystem.symlink_metadata(path) {
                Ok(actual) if actual == *expected => {}
                Ok(_) => {
                    let location =
                        Location::absolute(path.clone()).expect("a required path is absolute");
                    return Err(Failure::PreconditionsChanged { location });
                }
                Err(_) => {
                    return Err(Failure::commit(
                        CommitFailure::new(located_in_package(&self.root, path.clone())),
                        Vec::new(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// What running a journal committed.
pub(crate) struct Committed {
    touched: Touched,
    /// What its tidy-up steps could not do.
    warnings: Vec<Warning>,
    changed: bool,
}

impl Committed {
    /// Refreshes the Package Index at `root` after this commit, reading again every
    /// record it wrote.
    pub(crate) fn refresh<F: FileSystem>(&self, root: &Path, filesystem: &F) -> IndexRefresh {
        IndexRefresh::after_commit(root, &self.touched, filesystem)
    }

    /// Takes in what running another journal committed, as part of this commit.
    pub(crate) fn absorb(&mut self, other: Committed) {
        self.touched.orders.extend(other.touched.orders);
        self.touched.vendors |= other.touched.vendors;
        self.warnings.extend(other.warnings);
        self.changed |= other.changed;
    }

    /// Adds `warning` about this commit.
    pub(crate) fn warn(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    /// What the commit's tidy-up steps could not do, then what `refresh` could not.
    pub(crate) fn warnings(self, refresh: IndexRefresh) -> Vec<Warning> {
        let mut warnings = self.warnings;
        warnings.extend(refresh.warning());
        warnings
    }

    /// The report of a commit that refreshes nothing: unchanged where every step only
    /// preserved what it found.
    pub(crate) fn report<R>(self, result: R) -> MutationReport<R> {
        if self.changed {
            MutationReport::committed(result, self.warnings)
        } else {
            MutationReport::Unchanged { result }
        }
    }
}

/// One entry this commit created, with the change evidence that proves it is still the
/// one it created.
struct Owned {
    path: PathBuf,
    kind: FileSystemEntryKind,
    observation: Option<FileChangeObservation>,
    /// The bytes a successful write promised. Readable different content is not owned,
    /// even if it changed before the first observation. Directories have no owned text.
    bytes: Option<Vec<u8>>,
    /// Another name of the same file, whose status changes with this one's.
    partner: Option<PathBuf>,
}

/// One completed step, as its undo needs it.
enum Done {
    Created(Owned),
    Replaced {
        path: PathBuf,
        prior: Vec<u8>,
        candidate: Vec<u8>,
    },
    Moved {
        from: PathBuf,
        to: PathBuf,
    },
}

struct Run<'a, F: FileSystem> {
    root: &'a Path,
    filesystem: &'a F,
    done: Vec<Done>,
    /// Entries a committed step created. They cannot be undone, but a later tidy-up
    /// still needs their ownership evidence to remove a staging name safely.
    committed: Vec<Owned>,
}

impl<F: FileSystem> Run<'_, F> {
    fn seal(&mut self) {
        for done in self.done.drain(..) {
            if let Done::Created(owned) = done {
                self.committed.push(owned);
            }
        }
    }

    fn located(&self, path: &Path) -> Location {
        located_in_package(self.root, path.to_path_buf())
    }

    fn left(&self, path: &Path) -> CleanupFailure {
        CleanupFailure::new(self.located(path))
    }

    fn observe(&self, path: &Path) -> Option<FileChangeObservation> {
        self.filesystem.observe_file_change(path).ok()
    }

    /// The ordinary file at `path` still holds the candidate this commit authorized.
    fn candidate_at(&self, path: &Path, bytes: &[u8]) -> bool {
        matches!(
            self.filesystem.symlink_metadata(path),
            Ok(Some(FileSystemEntryKind::File))
        ) && self.filesystem.read(path).ok().as_deref() == Some(bytes)
    }

    /// The commit stopped at `at`: every completed step is undone, and `cleanup` names
    /// what the failed step itself left.
    fn failed(&mut self, at: &Path, mut cleanup: Vec<CleanupFailure>) -> Failure {
        cleanup.extend(self.undo());
        Failure::commit(CommitFailure::new(self.located(at)), cleanup)
    }

    /// Records `path` as this commit's own.
    fn own(
        &mut self,
        path: &Path,
        kind: FileSystemEntryKind,
        bytes: Option<&[u8]>,
        partner: Option<&Path>,
    ) {
        let observation = self.observe(path);
        self.done.push(Done::Created(Owned {
            path: path.to_path_buf(),
            kind,
            observation,
            bytes: bytes.map(<[u8]>::to_vec),
            partner: partner.map(Path::to_path_buf),
        }));
        self.changed_in(path);
    }

    /// This commit changed the entry at `path`, which changes the status of every
    /// directory it created that holds it.
    fn changed_in(&mut self, path: &Path) {
        let filesystem = self.filesystem;
        for done in &mut self.done {
            if let Done::Created(owned) = done
                && owned.kind == FileSystemEntryKind::Directory
                && path.starts_with(&owned.path)
            {
                owned.observation = filesystem.observe_file_change(&owned.path).ok();
            }
        }
    }

    /// This commit changed the status of its own entry at `path`.
    fn observe_again(&mut self, path: &Path) {
        let filesystem = self.filesystem;
        for done in &mut self.done {
            if let Done::Created(owned) = done
                && owned.path == path
            {
                owned.observation = filesystem.observe_file_change(path).ok();
            }
        }
    }

    /// Forgets `path` as one of this commit's entries, once the commit removed it itself.
    fn disown(&mut self, path: &Path) {
        self.done
            .retain(|done| !matches!(done, Done::Created(owned) if owned.path == path));
    }

    fn create_directory(&mut self, path: &Path) -> Result<(), Failure> {
        if self.filesystem.create_directory(path).is_err() {
            return Err(self.failed(path, Vec::new()));
        }
        self.own(path, FileSystemEntryKind::Directory, None, None);
        Ok(())
    }

    /// The cleanup a write that failed before creating `path` owes: nothing where nothing
    /// stands there, and the entry named where something does.
    fn unwritten(&self, path: &Path) -> Vec<CleanupFailure> {
        match self.filesystem.observe_file_change(path) {
            Ok(observation) if observation.kind.is_none() => Vec::new(),
            _ => vec![self.left(path)],
        }
    }

    fn write_unpublished(&mut self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        if self.filesystem.write_new(path, bytes).is_err() {
            let cleanup = self.unwritten(path);
            return Err(self.failed(path, cleanup));
        }
        self.own(path, FileSystemEntryKind::File, Some(bytes), None);
        if self.filesystem.sync_file(path).is_err() || !self.candidate_at(path, bytes) {
            return Err(self.failed(path, Vec::new()));
        }
        self.observe_again(path);
        Ok(())
    }

    fn create_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        let stage = staging_path(path).expect("a created file has a basename");
        if self.filesystem.write_new(&stage, bytes).is_err() {
            let cleanup = self.unwritten(&stage);
            return Err(self.failed(&stage, cleanup));
        }
        self.own(&stage, FileSystemEntryKind::File, Some(bytes), Some(path));
        if self.filesystem.sync_file(&stage).is_err() || !self.candidate_at(&stage, bytes) {
            return Err(self.failed(&stage, Vec::new()));
        }
        match self.filesystem.link_new_file(&stage, path) {
            Ok(NewFileLinkOutcome::Linked) => {}
            Ok(NewFileLinkOutcome::DestinationExists) | Err(_) => {
                return Err(self.failed(path, Vec::new()));
            }
        }
        // Linking changes the status of the file this commit created, so each of its
        // names is observed again after it, and again after the stage is removed.
        self.observe_again(&stage);
        self.own(path, FileSystemEntryKind::File, Some(bytes), Some(&stage));
        if self.remove_owned_at(&stage).is_err() {
            return Err(self.failed(&stage, Vec::new()));
        }
        self.observe_again(path);
        self.changed_in(&stage);
        Ok(())
    }

    /// Replaces every destination of `group` together: every candidate is staged and
    /// handed to the device first, and one full sync then makes them all durable, before
    /// any destination is taken over. A full sync drains the device's whole cache, so one
    /// per group does what one per file would.
    fn replace(&mut self, group: &[(&Path, &[u8], &Expect)]) -> Result<(), Failure> {
        let mut stages: Vec<PathBuf> = Vec::with_capacity(group.len());
        for (index, (destination, candidate, _)) in group.iter().enumerate() {
            let Some(stage) = staging_path(destination)
                .filter(|_| in_ordinary_directory(destination, self.filesystem))
            else {
                return Err(self.failed(destination, Vec::new()));
            };
            // A stage already there is earlier evidence, not output this write may
            // leave behind. Distinguish it before attempting an exclusive creation.
            if !matches!(self.filesystem.symlink_metadata(&stage), Ok(None)) {
                return Err(self.failed(destination, Vec::new()));
            }
            if self.filesystem.write_new(&stage, candidate).is_err() {
                let cleanup = self.unwritten(&stage);
                return Err(self.failed(destination, cleanup));
            }
            self.own(&stage, FileSystemEntryKind::File, Some(candidate), None);
            stages.push(stage.clone());
            let durable = if group.len() == 1 {
                self.filesystem.sync_file(&stage)
            } else {
                self.filesystem.sync_file_contents(&stage)
            };
            if durable.is_err() {
                return Err(self.failed(group[index].0, Vec::new()));
            }
        }
        if group.len() > 1
            && let Some(last) = stages.last()
            && self.filesystem.sync_file(last).is_err()
        {
            return Err(self.failed(group[group.len() - 1].0, Vec::new()));
        }
        for (index, (destination, candidate, expect)) in group.iter().enumerate() {
            if !self.candidate_at(&stages[index], candidate) {
                return Err(self.failed(destination, Vec::new()));
            }
            // Last possible moment before the destination is taken over. This narrows the
            // window in which another writer's change goes unseen; it does not close it.
            let held = self.holds(destination, expect);
            let Some(prior) = held.filter(|_| {
                self.filesystem
                    .replace_file(&stages[index], destination)
                    .is_ok()
            }) else {
                return Err(self.failed(destination, Vec::new()));
            };
            self.disown(&stages[index]);
            self.published(destination, prior, candidate);
        }
        Ok(())
    }

    /// What `destination` holds immediately before it is taken over, where that is what
    /// `expect` names: nothing for an absent destination, or its bytes. This narrows the
    /// window in which another writer's change goes unseen; it does not close it.
    fn holds(&self, destination: &Path, expect: &Expect) -> Option<Option<Vec<u8>>> {
        match expect {
            Expect::Absent => {
                matches!(self.filesystem.symlink_metadata(destination), Ok(None)).then_some(None)
            }
            Expect::Bytes(observed) => (self.filesystem.read(destination).ok().as_deref()
                == Some(observed.as_slice()))
            .then(|| Some(observed.clone())),
            Expect::Meaning(meaning) => self
                .filesystem
                .read(destination)
                .ok()
                .filter(|current| meaning.held_by(current))
                .map(Some),
        }
    }

    /// Records `destination`, which held `prior` and now holds `candidate`, as published by
    /// this commit: a file it created where nothing stood, or a replacement its undo puts
    /// back.
    fn published(&mut self, destination: &Path, prior: Option<Vec<u8>>, candidate: &[u8]) {
        match prior {
            None => self.own(
                destination,
                FileSystemEntryKind::File,
                Some(candidate),
                None,
            ),
            Some(prior) => {
                self.done.push(Done::Replaced {
                    path: destination.to_path_buf(),
                    prior,
                    candidate: candidate.to_vec(),
                });
                self.changed_in(destination);
            }
        }
    }

    fn promote(
        &mut self,
        stage: &Path,
        destination: &Path,
        candidate: &[u8],
        expect: &Expect,
    ) -> Result<(), Failure> {
        // The stage survived long enough to be observed, but the write that abandoned it
        // may have stopped before making it durable, and this is the moment it becomes a
        // managed file.
        if self.filesystem.sync_file(stage).is_err() || !self.candidate_at(stage, candidate) {
            return Err(self.failed(destination, Vec::new()));
        }
        let Some(prior) = self.holds(destination, expect) else {
            return Err(self.failed(destination, Vec::new()));
        };
        if self.filesystem.replace_file(stage, destination).is_err() {
            return Err(self.failed(destination, Vec::new()));
        }
        self.published(destination, prior, candidate);
        Ok(())
    }

    fn stage(&mut self, destination: &Path, bytes: &[u8]) -> Result<(), Failure> {
        let stage = staging_path(destination).expect("a staged file has a basename");
        if self.filesystem.write_new(&stage, bytes).is_err() {
            let cleanup = self.unwritten(&stage);
            return Err(self.failed(&stage, cleanup));
        }
        self.own(
            &stage,
            FileSystemEntryKind::File,
            Some(bytes),
            Some(destination),
        );
        if self.filesystem.sync_file(&stage).is_err() || !self.candidate_at(&stage, bytes) {
            return Err(self.failed(&stage, Vec::new()));
        }
        Ok(())
    }

    fn link(&mut self, destination: &Path) -> Result<(), Failure> {
        let stage = staging_path(destination).expect("a staged file has a basename");
        let Some(bytes) = self.done.iter().find_map(|done| match done {
            Done::Created(owned) if owned.path == stage => owned.bytes.clone(),
            _ => None,
        }) else {
            return Err(self.failed(&stage, Vec::new()));
        };
        if !self.candidate_at(&stage, &bytes) {
            return Err(self.failed(&stage, Vec::new()));
        }
        match self.filesystem.link_new_file(&stage, destination) {
            Ok(NewFileLinkOutcome::Linked) => {}
            Ok(NewFileLinkOutcome::DestinationExists) | Err(_) => {
                return Err(self.failed(destination, Vec::new()));
            }
        }
        // Linking changes the status of the file the stage holds, under both its names.
        self.observe_again(&stage);
        self.own(
            destination,
            FileSystemEntryKind::File,
            Some(&bytes),
            Some(&stage),
        );
        Ok(())
    }

    /// Removes the stage beside `destination` where one still stands, or names it.
    fn remove_stage(&mut self, destination: &Path) -> Result<(), CleanupFailure> {
        let stage = staging_path(destination).expect("a staged file has a basename");
        self.remove_owned_at(&stage)
    }

    /// Settles a named entry from the ownership journal through the shared ownership
    /// check, which removes it or reports what remains.
    fn remove_owned_at(&mut self, path: &Path) -> Result<(), CleanupFailure> {
        let index = self
            .done
            .iter()
            .position(|done| matches!(done, Done::Created(owned) if owned.path == path));
        let owned = if let Some(index) = index {
            let Done::Created(owned) = self.done.remove(index) else {
                unreachable!("the named step created an entry")
            };
            owned
        } else if let Some(index) = self.committed.iter().position(|owned| owned.path == path) {
            self.committed.remove(index)
        } else {
            return match self.filesystem.symlink_metadata(path) {
                Ok(None) => Ok(()),
                _ => Err(self.left(path)),
            };
        };
        let mut left = Vec::new();
        self.remove_owned(&owned, &mut left);
        match left.into_iter().next() {
            None => Ok(()),
            Some(left) => {
                if let Some(index) = index {
                    self.done.insert(index, Done::Created(owned));
                } else {
                    self.committed.push(owned);
                }
                Err(left)
            }
        }
    }

    /// Moves `from` to `to`; a move that fails stops the commit at `at`, where it is
    /// listed.
    fn move_entry(&mut self, from: &Path, to: &Path, at: &Path) -> Result<(), Failure> {
        match self.filesystem.rename_new(from, to) {
            Ok(RenameOutcome::Renamed) => {}
            Ok(RenameOutcome::DestinationExists) => return Err(self.failed(at, Vec::new())),
            Err(error) => {
                let failure = self.failed(at, Vec::new());
                return Err(failure.unless_unsupported(self.root, from, &error, self.filesystem));
            }
        }
        self.done.push(Done::Moved {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
        });
        self.changed_in(from);
        self.changed_in(to);
        Ok(())
    }

    /// Undoes every completed step, newest first, naming what it could not undo.
    fn undo(&mut self) -> Vec<CleanupFailure> {
        let mut left = Vec::new();
        while let Some(done) = self.done.pop() {
            match done {
                Done::Created(owned) => self.remove_owned(&owned, &mut left),
                Done::Replaced {
                    path,
                    prior,
                    candidate,
                } => self.restore(&path, &prior, &candidate, &mut left),
                Done::Moved { from, to } => match self.filesystem.rename_new(&to, &from) {
                    Ok(RenameOutcome::Renamed) => {
                        self.changed_in(&to);
                        self.changed_in(&from);
                    }
                    _ => left.push(self.left(&to)),
                },
            }
        }
        left
    }

    /// Removes one entry this commit created, only while it is provably still that entry.
    fn remove_owned(&mut self, owned: &Owned, left: &mut Vec<CleanupFailure>) {
        let Some(expected) = owned.observation else {
            left.push(self.left(&owned.path));
            return;
        };
        let current = match self.filesystem.observe_file_change(&owned.path) {
            Ok(current) => current,
            Err(_) => {
                left.push(self.left(&owned.path));
                return;
            }
        };
        // Already gone: nothing this commit created is left there to undo.
        if current.kind.is_none() {
            return;
        }
        if !same_owned_entry(expected, current, owned.kind) {
            left.push(self.left(&owned.path));
            return;
        }
        // Comparable, unchanged evidence proves ownership even when file permissions
        // prevent a read. Where the bytes can be read, a mismatch supplies contrary
        // evidence that must also be respected.
        if let Some(bytes) = &owned.bytes
            && let Ok(current) = self.filesystem.read(&owned.path)
            && current != *bytes
        {
            left.push(self.left(&owned.path));
            return;
        }
        let removed = match owned.kind {
            FileSystemEntryKind::Directory => self.filesystem.remove_directory(&owned.path),
            _ => self.filesystem.remove_file(&owned.path),
        };
        if removed.is_err() {
            left.push(self.left(&owned.path));
            return;
        }
        // Another name of the same file shares the status removing this one just changed.
        if let Some(partner) = &owned.partner {
            let filesystem = self.filesystem;
            for done in &mut self.done {
                if let Done::Created(other) = done
                    && &other.path == partner
                    && other.observation == Some(expected)
                {
                    other.observation = filesystem.observe_file_change(partner).ok();
                }
            }
        }
        self.changed_in(&owned.path);
    }

    /// Puts `prior` back at `path` only while it still holds exactly `candidate`, which
    /// this commit wrote there.
    fn restore(
        &mut self,
        path: &Path,
        prior: &[u8],
        candidate: &[u8],
        left: &mut Vec<CleanupFailure>,
    ) {
        let Some(stage) =
            staging_path(path).filter(|_| in_ordinary_directory(path, self.filesystem))
        else {
            left.push(self.left(path));
            return;
        };
        if self.filesystem.write_new(&stage, prior).is_err() {
            left.push(self.left(path));
            left.extend(self.unwritten(&stage));
            return;
        }
        let owned = Owned {
            path: stage.clone(),
            kind: FileSystemEntryKind::File,
            observation: self.observe(&stage),
            bytes: Some(prior.to_vec()),
            partner: None,
        };
        let restored = self.filesystem.sync_file(&stage).is_ok()
            && self.candidate_at(&stage, prior)
            && self.filesystem.read(path).ok().as_deref() == Some(candidate)
            && self.filesystem.replace_file(&stage, path).is_ok();
        if !restored {
            // The restoration failed; the shared ownership check decides whether its
            // stage can be removed or must remain as evidence.
            left.push(self.left(path));
            self.remove_owned(&owned, left);
            return;
        }
        self.changed_in(path);
    }
}

/// Whether `path` sits in an ordinary directory rather than one reached through a link:
/// a record behind a linked directory is not the Package's own, and nothing is ever
/// written through one.
fn in_ordinary_directory<F: FileSystem>(path: &Path, filesystem: &F) -> bool {
    path.parent().is_some_and(|parent| {
        matches!(
            filesystem.symlink_metadata(parent),
            Ok(Some(FileSystemEntryKind::Directory))
        )
    })
}

/// Whether two observations prove one unchanged entry of `kind`. Only comparable evidence
/// proves it; a volume that keeps none proves nothing, so nothing on it is removed.
fn same_owned_entry(
    expected: FileChangeObservation,
    current: FileChangeObservation,
    kind: FileSystemEntryKind,
) -> bool {
    if expected.kind != Some(kind) || current.kind != Some(kind) {
        return false;
    }
    matches!(
        (expected.evidence, current.evidence),
        (
            FileChangeEvidence::Comparable {
                comparison_domain: expected_domain,
                sequence: expected_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: current_domain,
                sequence: current_sequence,
            }
        ) if expected_domain == current_domain && expected_sequence == current_sequence
    )
}

/// A `Location` for one path a plan names beneath the canonical Root `root`.
///
/// The Package-relative spelling holds by construction rather than by assertion. Every
/// planned path is the canonical root joined with one of:
///
/// - a capability's own literal managed names, or the components of a configured value
///   `validate_configured_path` has accepted, which enforces the grammar
///   `package_relative_path_text` answers against;
/// - an Order Directory the load discovered, whose own `Location` it already carries;
/// - the Orders or Removed Orders Directory, which the load has already confirmed has a
///   Package-relative spelling, and then a basename `normalize_filename_component` has
///   stripped of separators, a moved directory's own basename, or literal managed names;
/// - an adoption candidate below the check that refuses one this Package cannot express,
///   and then literal managed names.
///
/// The Root itself is spelled `.` by every Package.
pub(crate) fn planned_location(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned path is normalized")
}
