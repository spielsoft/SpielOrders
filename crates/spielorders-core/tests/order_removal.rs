//! Decision sources: Define the SpielOrders package topology; Define the Order record
//! and lifecycle contract; Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping.

use crate::support::{
    ConcurrentWriter, EditPoint, ObservingFileSystem, RealFileSystem, TemporaryTree,
};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::canonical::{
    CanonicalPackageLoad, PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
    inspect_canonical_package,
};
use spielorders_core::evidence::{EvidenceOutcome, FactChange};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderDocument, OrderStatus};
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_edits::{OrderAssignments, commit_order_edit, plan_order_edit};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeObservation, FileSystem, FileSystemEntryKind,
    LocalCalendar, LocalDate, NativeTrash, NewDirectoryRenameOutcome, NewFileLinkOutcome,
    UuidGenerator,
};
use spielorders_core::removal::{
    LocatorOutcome, OrderRemovalOperationKind, OrderRemovalPlanError, OrderRemovalRejection,
    OrderRemovalReport, OrderRemovalWarning, RemovalDisposition, RemovedDirectoryCleanup,
    commit_order_removal, plan_order_removal,
};
use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SUBSTITUTE_ORDER_ID: &str = "1f0c9a52-4d3b-4a71-8e0c-6b2f5d9a7c31";
const BASENAME: &str = "2026_08_22_Laser_components";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

struct FixedCalendar;

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 9,
            day: 1,
        }
    }
}

/// Records every directory submitted to the platform Trash. It moves nothing, so a
/// test can also prove the source is left exactly where the platform took it from.
#[derive(Default)]
struct RecordingTrash {
    submitted: RefCell<Vec<PathBuf>>,
    location: Option<PathBuf>,
    fail: bool,
}

impl RecordingTrash {
    fn reporting(location: PathBuf) -> Self {
        Self {
            location: Some(location),
            ..Self::default()
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    fn submitted(&self) -> Vec<PathBuf> {
        self.submitted.borrow().clone()
    }
}

impl NativeTrash for RecordingTrash {
    type Error = io::Error;

    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        self.submitted.borrow_mut().push(path.to_path_buf());
        if self.fail {
            return Err(io::Error::other("injected trash submission failure"));
        }
        // A real platform Trash takes the directory away. The fake leaves it in place
        // so a test can prove the core performed no delete of its own.
        Ok(self.location.clone())
    }
}

/// A Trash port that behaves the way a real platform Trash does: the directory it
/// accepts genuinely leaves the Package. `RecordingTrash` deliberately leaves it in
/// place to prove this core deletes nothing itself, which makes it unable to model
/// what the Package looks like afterwards.
struct RemovingTrash {
    location: PathBuf,
}

impl RemovingTrash {
    fn to(location: PathBuf) -> Self {
        Self { location }
    }
}

impl NativeTrash for RemovingTrash {
    type Error = io::Error;

    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        fs::create_dir_all(self.location.parent().unwrap())?;
        fs::rename(path, &self.location)?;
        Ok(Some(self.location.clone()))
    }
}

/// A Trash port that must never be called.
struct UnreachableTrash;

impl NativeTrash for UnreachableTrash {
    type Error = io::Error;

    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        panic!("a recoverable removal never reaches the platform Trash: {path:?}");
    }
}

fn initialized_package(label: &str, options: &InitializationOptions) -> TemporaryTree {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(tree.path(), options, &RealFileSystem)
        .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    tree
}

/// An Order carrying a README, managed metadata, and both opaque sentinels, one
/// nested, so a disposition can be shown to move the complete tree intact.
fn surrounded_order(root: &Path, orders_path: &str) -> PathBuf {
    surrounded_order_with_id(root, orders_path, ORDER_ID)
}

fn surrounded_order_with_id(root: &Path, orders_path: &str, order_id: &str) -> PathBuf {
    let directory = root.join(orders_path).join(BASENAME);
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();

    let plan = plan_order_adoption(
        root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(order_id.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn default_package(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = initialized_package(label, &InitializationOptions::default());
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Orders");
    (tree, root, directory)
}

/// A Package whose Orders Directory sits one level deeper than its Removed Orders
/// Directory, so a recoverable disposition genuinely changes the Order's locator and
/// the record rewrite is reached at all. Every shallower layout skips it.
fn package_with_a_deeper_orders_directory(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = initialized_package(
        label,
        &InitializationOptions::new("Archive/Orders", "Removed"),
    );
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");
    (tree, root, directory)
}

/// The record the moved Order carries, and the staging file a rewrite of it stages
/// beside it.
fn moved_record(removed: &Path) -> (PathBuf, PathBuf) {
    let metadata = removed.join(".spielorders");
    (
        metadata.join("order.json"),
        metadata.join(".order.json.spielorders-stage"),
    )
}

fn indexed_order_ids(root: &Path) -> Vec<String> {
    let index =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    index
        .orders
        .iter()
        .map(|order| order.order.order_id.as_str().to_owned())
        .collect()
}

/// Every byte inside the Order Directory that must survive a disposition intact.
fn contents(directory: &Path) -> Vec<(String, Vec<u8>)> {
    [
        "README.md",
        "quote.bin",
        "Quotes/drawing.bin",
        ".spielorders/order.json",
    ]
    .into_iter()
    .map(|relative| {
        (
            relative.to_owned(),
            fs::read(directory.join(relative))
                .unwrap_or_else(|error| panic!("{relative} is readable: {error}")),
        )
    })
    .collect()
}

#[test]
fn a_recoverable_removal_moves_the_intact_directory_and_never_reaches_the_trash() {
    let (_tree, root, directory) = default_package("removal-recoverable");
    let removed = root.join("Removed").join(BASENAME);
    let before = contents(&directory);
    assert!(!root.join("Removed").exists(), "Removed starts absent");

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("an active Order can be removed recoverably");

    assert_eq!(plan.disposition(), RemovalDisposition::RecoverableDirectory);
    assert_eq!(plan.destination(), Some(removed.as_path()));
    assert!(plan.creates_removed_orders_directory());
    assert_eq!(plan.order_id(), ORDER_ID);
    assert_eq!(plan.title(), "Laser components");
    assert_eq!(plan.source(), directory);
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderRemovalOperationKind::CreateRemovedOrdersDirectory,
            OrderRemovalOperationKind::MoveOrderDirectory,
            OrderRemovalOperationKind::RefreshPackageIndex,
        ],
        "a same-depth disposition rewrites no record"
    );
    assert!(directory.is_dir(), "planning moves nothing");

    // The Trash port would panic if this path reached it.
    let report = commit_order_removal(plan, &RealFileSystem, &UnreachableTrash);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid recoverable removal commits: {report:?}");
    };
    assert_eq!(
        result.disposition(),
        RemovalDisposition::RecoverableDirectory
    );
    assert_eq!(result.final_location(), Some(removed.as_path()));
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().readme_text().map(str::as_bytes),
        Some(README)
    );
    // The summary describes the Order as it stood, at the path it was removed from;
    // where it went is reported separately as the disposition's final location.
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        directory,
        "the summary reports the original path"
    );

    assert!(!directory.exists(), "the Order left the Orders Directory");
    assert!(removed.is_dir());
    assert_eq!(
        contents(&removed),
        before,
        "every byte inside the directory survives the move"
    );

    // The removed Order leaves active scope entirely.
    assert_eq!(indexed_order_ids(&root), Vec::<String>::new());
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(&root, &RealFileSystem)
    else {
        panic!("the Package still loads after the removal");
    };
    assert!(
        package.healthy_orders().is_empty(),
        "a later load discovers no Order"
    );
    assert!(package.damaged_orders().is_empty(), "and diagnoses none");
}

#[test]
fn a_trash_removal_submits_the_exact_directory_once_and_deletes_nothing_itself() {
    let (_tree, root, directory) = default_package("removal-trash");
    let before = contents(&directory);
    let reported = root.join("PlatformTrash").join(BASENAME);

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::PlatformTrash,
        &RealFileSystem,
    )
    .expect("an active Order can be sent to the platform Trash");

    assert_eq!(plan.disposition(), RemovalDisposition::PlatformTrash);
    assert_eq!(
        plan.destination(),
        None,
        "the platform owns where a trashed directory goes"
    );
    assert!(!plan.creates_removed_orders_directory());
    assert_eq!(
        plan.locator(),
        &LocatorOutcome::NotApplicable,
        "a directory leaving the Package has no Package-relative locator"
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderRemovalOperationKind::SubmitDirectoryToTrash,
            OrderRemovalOperationKind::RefreshPackageIndex,
        ]
    );

    let trash = RecordingTrash::reporting(reported.clone());
    let report = commit_order_removal(plan, &RealFileSystem, &trash);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid Trash removal commits: {report:?}");
    };
    // Exactly once, with the exact directory.
    assert_eq!(trash.submitted(), std::slice::from_ref(&directory));
    assert_eq!(result.disposition(), RemovalDisposition::PlatformTrash);
    assert_eq!(result.final_location(), Some(reported.as_path()));
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);

    // This fake Trash deliberately leaves the directory in place. A core that also
    // deleted it as a fallback would have emptied or removed it here.
    assert!(
        directory.is_dir(),
        "the core performs no delete of its own; the platform owns disposal"
    );
    assert_eq!(contents(&directory), before);
    assert!(!root.join("Removed").exists(), "Trash creates no Removed");
}

#[test]
fn an_existing_removed_orders_directory_is_reused_rather_than_clobbered() {
    let (_tree, root, directory) = default_package("removal-removed-exists");
    let removed_root = root.join("Removed");
    fs::create_dir(&removed_root).unwrap();
    // An earlier removal already left something here; reuse must not disturb it.
    fs::create_dir(removed_root.join("2020_01_01_Older_order")).unwrap();
    fs::write(
        removed_root.join("2020_01_01_Older_order/keep.txt"),
        b"an earlier removal\n",
    )
    .unwrap();

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed into an existing Removed directory");

    assert!(
        !plan.creates_removed_orders_directory(),
        "an existing Removed Orders Directory is reused, not created"
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderRemovalOperationKind::MoveOrderDirectory,
            OrderRemovalOperationKind::RefreshPackageIndex,
        ]
    );

    assert!(matches!(
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash),
        MutationReport::Committed { .. }
    ));

    assert!(removed_root.join(BASENAME).is_dir());
    assert_eq!(
        fs::read(removed_root.join("2020_01_01_Older_order/keep.txt")).unwrap(),
        b"an earlier removal\n",
        "the earlier removal is untouched"
    );
}

#[test]
fn the_locator_outcome_covers_unchanged_updated_and_not_applicable() {
    // Unchanged: Orders and Removed are both direct children, so the relative path
    // back to the Package root keeps its depth.
    let (_tree, root, directory) = default_package("removal-locator-unchanged");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert_eq!(
        plan.locator(),
        &LocatorOutcome::Unchanged {
            locator: "../..".into()
        }
    );
    let removed = root.join("Removed").join(BASENAME);
    assert!(matches!(
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash),
        MutationReport::Committed { .. }
    ));
    assert_eq!(
        stored_locator(&removed),
        "../..",
        "an unchanged locator is left exactly as stored"
    );

    // Updated: a nested Orders Directory sits one level deeper than Removed, so the
    // moved Order's relative path back to the root genuinely changes.
    let tree = initialized_package(
        "removal-locator-updated",
        &InitializationOptions::new("Archive/Orders", "Removed"),
    );
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");
    assert_eq!(stored_locator(&directory), "../../..");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert_eq!(
        plan.locator(),
        &LocatorOutcome::Updated {
            from: "../../..".into(),
            to: "../..".into()
        }
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderRemovalOperationKind::CreateRemovedOrdersDirectory,
            OrderRemovalOperationKind::MoveOrderDirectory,
            OrderRemovalOperationKind::ReplaceOrderRecord,
            OrderRemovalOperationKind::RefreshPackageIndex,
        ],
        "a depth change adds the record rewrite the shallower cases do not need"
    );
    assert!(matches!(
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash),
        MutationReport::Committed { .. }
    ));
    let removed = root.join("Removed").join(BASENAME);
    assert_eq!(
        stored_locator(&removed),
        "../..",
        "the locator is rewritten to match where the directory now sits"
    );

    // Not applicable: the directory leaves the Package altogether.
    let (_tree, _root, directory) = default_package("removal-locator-not-applicable");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::PlatformTrash,
        &RealFileSystem,
    )
    .unwrap();
    assert_eq!(plan.locator(), &LocatorOutcome::NotApplicable);
    assert_eq!(plan.locator().as_str(), "not_applicable");
}

fn stored_locator(directory: &Path) -> String {
    OrderDocument::from_json_bytes(&fs::read(directory.join(".spielorders/order.json")).unwrap())
        .expect("the canonical Order parses")
        .package_root
        .as_str()
        .to_owned()
}

#[test]
fn no_disposition_enumerates_the_directory_it_moves() {
    let (_tree, root, directory) = default_package("removal-no-enumeration");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();

    let observing = ObservingFileSystem::default();
    let report = commit_order_removal(plan, &observing, &UnreachableTrash);

    assert!(matches!(report, MutationReport::Committed { .. }));
    // The refresh that follows lists the Orders Directory, which is its declared
    // work. Nothing lists inside the Order that moved, and nothing reads its opaque
    // Supporting Material.
    let removed = root.join("Removed").join(BASENAME);
    for listed in observing.listings() {
        assert!(
            !listed.starts_with(&directory) && !listed.starts_with(&removed),
            "no disposition enumerates the Order Directory, but listed {listed:?}"
        );
    }
    for read in observing.reads() {
        let read = read.to_string_lossy().to_string();
        assert!(
            !read.contains("quote.bin") && !read.contains("drawing.bin"),
            "Supporting Material is never inspected, but read {read}"
        );
    }
}

/// Injects one whole-directory move outcome, or fails the derived-index refresh that
/// follows a successful move. Every other effect is real.
struct FailingRemoval {
    move_outcome: Option<Option<NewDirectoryRenameOutcome>>,
    fail_index: bool,
    fail_create: bool,
    /// Failures that must be visible only to the refresh that runs after the Order
    /// has already left, never to the planning reads. `source` is what they watch:
    /// once it is gone, the disposition has happened.
    source: Option<PathBuf>,
    fail_refresh_listing: bool,
    damage_package: bool,
    /// Planted inside the Removed Orders Directory at the moment the move fails, which
    /// is the one window in which something can arrive in a directory this operation
    /// created before its rollback looks at it.
    arrival: Option<&'static str>,
    fail_remove_directory: bool,
    /// The rename that publishes the rewritten Order record fails, so the locator
    /// rewrite leaves a durable stage beside a record it never replaced.
    fail_record_replacement: bool,
    /// That stage cannot be removed either, so the replacement's own cleanup fails and
    /// the orphan is real.
    fail_stage_removal: bool,
}

impl FailingRemoval {
    /// The post-disposition refresh cannot list the Package at all.
    fn refresh_listing_failing(source: PathBuf) -> Self {
        Self {
            source: Some(source),
            fail_refresh_listing: true,
            ..Self::moving()
        }
    }

    /// The Package's own version marker stops parsing after the disposition, so the
    /// refresh comes back with no current Package to inspect.
    fn package_damaged_after(source: PathBuf) -> Self {
        Self {
            source: Some(source),
            damage_package: true,
            ..Self::moving()
        }
    }

    /// The disposition has happened once the Order is no longer where it was.
    fn disposed(&self) -> bool {
        self.source.as_ref().is_some_and(|source| !source.exists())
    }
}

impl FailingRemoval {
    fn moving() -> Self {
        Self {
            move_outcome: None,
            fail_index: false,
            fail_create: false,
            source: None,
            fail_refresh_listing: false,
            damage_package: false,
            arrival: None,
            fail_remove_directory: false,
            fail_record_replacement: false,
            fail_stage_removal: false,
        }
    }

    /// The locator rewrite fails at the rename, and the staging file it wrote cannot be
    /// removed afterwards.
    fn locator_rewrite_failing_with_cleanup_failing() -> Self {
        Self {
            fail_record_replacement: true,
            fail_stage_removal: true,
            ..Self::moving()
        }
    }

    fn move_erroring_after_arrival(arrival: &'static str) -> Self {
        Self {
            arrival: Some(arrival),
            ..Self::move_erroring()
        }
    }

    fn move_erroring_with_undo_failing() -> Self {
        Self {
            fail_remove_directory: true,
            ..Self::move_erroring()
        }
    }

    fn move_erroring() -> Self {
        Self {
            move_outcome: Some(None),
            ..Self::moving()
        }
    }

    fn move_reporting(outcome: NewDirectoryRenameOutcome) -> Self {
        Self {
            move_outcome: Some(Some(outcome)),
            ..Self::moving()
        }
    }

    fn index_failing() -> Self {
        Self {
            fail_index: true,
            ..Self::moving()
        }
    }

    fn creation_failing() -> Self {
        Self {
            fail_create: true,
            ..Self::moving()
        }
    }
}

impl FileSystem for FailingRemoval {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if self.damage_package && self.disposed() && path.ends_with("version.json") {
            return Ok(b"{ not json".to_vec());
        }
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
        if self.fail_index && destination.ends_with(".spielorders/index.json") {
            return Err(io::Error::other("injected index replacement failure"));
        }
        if self.fail_record_replacement && destination.ends_with(".spielorders/order.json") {
            return Err(io::Error::other(
                "injected Order record replacement failure",
            ));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_stage_removal && path.ends_with(".order.json.spielorders-stage") {
            return Err(io::Error::other("injected staging removal failure"));
        }
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
        if self.fail_refresh_listing && self.disposed() {
            return Err(io::Error::other("injected refresh listing failure"));
        }
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for FailingRemoval {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_create {
            return Err(io::Error::other(
                "injected Removed directory creation failure",
            ));
        }
        RealFileSystem.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if self.fail_remove_directory {
            return Err(io::Error::other("injected directory removal failure"));
        }
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
        match self.move_outcome {
            Some(Some(outcome)) => Ok(outcome),
            Some(None) => {
                if let Some(arrival) = self.arrival {
                    fs::write(destination.parent().unwrap().join(arrival), b"not ours\n").unwrap();
                }
                Err(io::Error::other("injected directory move failure"))
            }
            None => RealFileSystem.rename_new_directory(source, destination),
        }
    }
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// A refusal is only useful if it names *which* fact moved. The rejection variant
/// alone would be the same unexplained precondition failure for a substituted Order
/// and for a source that simply vanished.
fn unconfirmed_facts<E: std::fmt::Debug>(
    report: &OrderRemovalReport<E>,
) -> Vec<(&str, FactChange)> {
    let MutationReport::Rejected {
        rejection: OrderRemovalRejection::PlanEvidenceUnconfirmed { outcome },
        ..
    } = report
    else {
        panic!("a plan whose evidence moved is refused before any effect: {report:?}");
    };
    let EvidenceOutcome::Changed(stale) = outcome else {
        panic!("the refusal names the evidence that changed: {outcome:?}");
    };
    stale
        .changed()
        .iter()
        .map(|fact| (fact.name(), fact.change()))
        .collect()
}

/// Both dispositions, because one moves the directory and the other hands it to the
/// platform Trash: authorizing either against the wrong Order disposes of an Order the
/// user never selected, and the Trash arm is the one a user cannot simply move back.
#[test]
fn a_different_order_left_at_the_planned_source_is_never_disposed_of() {
    // Each disposition names every fact it relies on. Only the recoverable one rewrites
    // the Order record, so only it is bound to what that record says.
    for (disposition, expected) in [
        (
            RemovalDisposition::RecoverableDirectory,
            vec![
                ("order_id", FactChange::Differs),
                ("order_record", FactChange::Differs),
            ],
        ),
        (
            RemovalDisposition::PlatformTrash,
            vec![("order_id", FactChange::Differs)],
        ),
    ] {
        let (_tree, root, directory) = default_package("removal-substituted-order");
        let plan = plan_order_removal(&directory, disposition, &RealFileSystem).unwrap();

        // The planned Order is gone and a different Order stands at exactly the same
        // path. Every check the commit could make about the *path* still passes: it is
        // present, it is an ordinary directory, and it is a healthy Order of this
        // Package. Only its identity differs, and disposing of it would apply one
        // Order's authorized removal to another Order's directory.
        fs::remove_dir_all(&directory).unwrap();
        let substituted = surrounded_order_with_id(&root, "Orders", SUBSTITUTE_ORDER_ID);
        assert_eq!(substituted, directory);
        assert!(
            fs::symlink_metadata(&directory).unwrap().is_dir(),
            "the source is still an ordinary directory, so existence alone cannot refuse"
        );
        let before = contents(&directory);

        // The Trash port panics if this path reaches it.
        let report = commit_order_removal(plan, &RealFileSystem, &UnreachableTrash);

        assert_eq!(
            unconfirmed_facts(&report),
            expected,
            "{disposition:?}: the refusal names the identity that moved, not the path"
        );
        // The Order that happens to occupy the path is left exactly as it stands.
        assert!(directory.is_dir(), "{disposition:?}");
        assert_eq!(contents(&directory), before, "{disposition:?}");
        assert!(
            !root.join("Removed").exists(),
            "{disposition:?}: a refused removal creates no Removed Orders Directory"
        );
        let resident = plan_order_removal(&directory, disposition, &RealFileSystem)
            .expect("the substituted Order is untouched and still plans on its own terms");
        assert_eq!(resident.order_id(), SUBSTITUTE_ORDER_ID);
    }
}

/// A depth-changing recoverable disposition rewrites the Order record at the
/// destination from the plan's own copy of it, and that copy is the only one left once
/// the directory has moved. A plan bound to identity alone would carry an edit made
/// after planning back out and destroy it.
#[test]
fn an_order_edited_after_planning_is_never_carried_back_out_stale() {
    let tree = initialized_package(
        "removal-stale-record",
        &InitializationOptions::new("Archive/Orders", "Removed"),
    );
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = surrounded_order(&root, "Archive/Orders");

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert!(
        matches!(plan.locator(), LocatorOutcome::Updated { .. }),
        "this depth is the one that rewrites the record: {:?}",
        plan.locator()
    );

    // An ordinary fact edit. It deliberately leaves the directory where it is, so the
    // source, the identity, and the projected destination the plan recorded all still
    // stand; only the record the removal would rewrite has moved.
    let edit = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the title can be edited");
    assert!(matches!(
        commit_order_edit(edit, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let report = commit_order_removal(plan, &RealFileSystem, &UnreachableTrash);

    assert_eq!(
        unconfirmed_facts(&report),
        [("order_record", FactChange::Differs)],
        "the refusal names the record that moved, not the path it still occupies"
    );
    assert!(directory.is_dir(), "nothing moved");
    assert!(!root.join("Removed").exists());
    assert_eq!(
        OrderDocument::from_json_bytes(
            &fs::read(directory.join(".spielorders/order.json")).unwrap()
        )
        .expect("the canonical Order parses")
        .title
        .as_str(),
        "Laser mounts",
        "the edit survives the refused removal"
    );
}

#[test]
fn planning_either_disposition_moves_nothing() {
    let (_tree, root, directory) = default_package("removal-dry-run");
    let before = contents(&directory);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    for disposition in [
        RemovalDisposition::RecoverableDirectory,
        RemovalDisposition::PlatformTrash,
    ] {
        let plan = plan_order_removal(&directory, disposition, &RealFileSystem)
            .unwrap_or_else(|error| panic!("{disposition:?} can be planned: {error:?}"));
        assert!(!plan.operations().is_empty(), "{disposition:?}");
    }

    // A dry run creates no Removed Orders Directory, moves nothing, submits nothing,
    // and rebuilds no index.
    assert!(!root.join("Removed").exists());
    assert!(directory.is_dir());
    assert_eq!(contents(&directory), before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_destination_collision_stops_removal_without_inventing_a_suffix() {
    let (_tree, root, directory) = default_package("removal-collision");
    let removed_root = root.join("Removed");
    let occupied = removed_root.join(BASENAME);
    fs::create_dir_all(&occupied).unwrap();
    fs::write(occupied.join("sibling.txt"), b"an earlier removal\n").unwrap();
    let before = contents(&directory);

    let error = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect_err("a taken destination stops removal");

    let OrderRemovalPlanError::DestinationCollision { path } = &error else {
        panic!("a collision is reported as such: {error:?}");
    };
    assert_eq!(path, &occupied);
    assert!(directory.is_dir(), "the Order stays active");
    assert_eq!(contents(&directory), before);
    assert_eq!(
        fs::read(occupied.join("sibling.txt")).unwrap(),
        b"an earlier removal\n"
    );
    let names = fs::read_dir(&removed_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, [BASENAME], "no suffixed variant was created");
}

#[test]
fn a_removed_path_that_is_not_an_ordinary_directory_damages_the_package() {
    // Configured-path validity belongs to the Package load, which refuses anything
    // other than an absent or ordinary root-child directory. An unusable Removed path
    // therefore damages the Package before a removal can be planned at all, which is
    // a stronger refusal than a removal-local one.
    let (_tree, root, directory) = default_package("removal-removed-is-file");
    fs::write(root.join("Removed"), b"not a directory\n").unwrap();
    let error = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect_err("a file at the Removed path is not usable");
    assert!(
        matches!(error, OrderRemovalPlanError::DamagedPackage),
        "a file at the Removed path damages the Package: {error:?}"
    );
    assert!(directory.is_dir());
    assert_eq!(
        fs::read(root.join("Removed")).unwrap(),
        b"not a directory\n"
    );

    // A link is likewise not an ordinary directory this core will move into.
    #[cfg(unix)]
    {
        let (_tree, root, directory) = default_package("removal-removed-is-link");
        let elsewhere = root.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("Removed")).unwrap();
        let error = plan_order_removal(
            &directory,
            RemovalDisposition::RecoverableDirectory,
            &RealFileSystem,
        )
        .expect_err("a link at the Removed path is not usable");
        assert!(
            matches!(error, OrderRemovalPlanError::DamagedPackage),
            "a link at the Removed path damages the Package: {error:?}"
        );
        assert!(directory.is_dir());
        assert_eq!(
            fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "the link target is never written through"
        );
    }
}

#[test]
fn a_wrong_scope_or_vanished_source_never_removes_anything() {
    // A directory that is not an Order of its Package.
    let (_tree, root, _directory) = default_package("removal-wrong-scope");
    let stranger = root.join("Orders").join("not_an_order");
    fs::create_dir(&stranger).unwrap();
    let error = plan_order_removal(
        &stranger,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect_err("a directory without an Order record is not removable");
    assert!(
        matches!(error, OrderRemovalPlanError::DamagedOrder { .. }),
        "a non-Order directory is one Damaged Order: {error:?}"
    );
    assert!(stranger.is_dir());
    assert!(!root.join("Removed").exists());

    // A source that vanished between planning and commit.
    let (_tree, root, directory) = default_package("removal-vanished-source");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    fs::remove_dir_all(&directory).unwrap();

    let report = commit_order_removal(plan, &RealFileSystem, &UnreachableTrash);

    assert!(
        matches!(
            report,
            MutationReport::Rejected {
                rejection: OrderRemovalRejection::PreconditionsChanged { .. },
                ..
            }
        ),
        "a vanished source is a precondition change: {report:?}"
    );
    assert!(
        !root.join("Removed").exists(),
        "a refused removal creates no Removed Orders Directory"
    );
}

#[test]
fn injected_disposition_failures_leave_the_order_active_and_whole() {
    // The move itself fails.
    let (_tree, root, directory) = default_package("removal-move-failure");
    let before = contents(&directory);
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let report = commit_order_removal(plan, &FailingRemoval::move_erroring(), &UnreachableTrash);
    assert!(
        matches!(
            report,
            MutationReport::Rejected {
                rejection: OrderRemovalRejection::MoveFailed { .. },
                ..
            }
        ),
        "an injected move failure rejects: {report:?}"
    );
    assert!(directory.is_dir(), "the Order stays where it was");
    assert_eq!(contents(&directory), before, "no child moved on its own");
    assert!(!root.join("Removed").join(BASENAME).exists());
    assert_eq!(indexed_order_ids(&root), [ORDER_ID.to_owned()]);

    // The destination is taken between planning and the move.
    let (_tree, _root, directory) = default_package("removal-move-collision-race");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let report = commit_order_removal(
        plan,
        &FailingRemoval::move_reporting(NewDirectoryRenameOutcome::DestinationExists),
        &UnreachableTrash,
    );
    assert!(
        matches!(
            report,
            MutationReport::Rejected {
                rejection: OrderRemovalRejection::DestinationCollision { .. },
                ..
            }
        ),
        "a taken destination at commit is a collision: {report:?}"
    );
    assert!(directory.is_dir());

    // The lazy Removed Orders Directory cannot be created.
    let (_tree, root, directory) = default_package("removal-create-failure");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let report = commit_order_removal(plan, &FailingRemoval::creation_failing(), &UnreachableTrash);
    assert!(
        matches!(
            report,
            MutationReport::Rejected {
                rejection: OrderRemovalRejection::RemovedDirectoryCreationFailed { .. },
                ..
            }
        ),
        "a failed Removed creation rejects: {report:?}"
    );
    assert!(directory.is_dir());
    assert!(!root.join("Removed").exists());

    // The platform refuses the submission.
    let (_tree, root, directory) = default_package("removal-trash-failure");
    let before = contents(&directory);
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::PlatformTrash,
        &RealFileSystem,
    )
    .unwrap();
    let trash = RecordingTrash::failing();
    let report = commit_order_removal(plan, &RealFileSystem, &trash);
    assert!(
        matches!(
            report,
            MutationReport::Rejected {
                rejection: OrderRemovalRejection::TrashSubmissionFailed { .. },
                ..
            }
        ),
        "a refused submission rejects: {report:?}"
    );
    assert_eq!(trash.submitted(), std::slice::from_ref(&directory));
    assert!(
        directory.is_dir(),
        "a refused submission never falls back to deleting the directory"
    );
    assert_eq!(contents(&directory), before);
    assert_eq!(indexed_order_ids(&root), [ORDER_ID.to_owned()]);
}

/// What a rejected removal says it left of the Removed Orders Directory it created.
fn cleanup<E: std::fmt::Debug>(report: &OrderRemovalReport<E>) -> &RemovedDirectoryCleanup<E> {
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a disposition that could not happen is a rejection: {report:?}");
    };
    match rejection {
        OrderRemovalRejection::DestinationCollision { cleanup, .. }
        | OrderRemovalRejection::MoveFailed { cleanup, .. } => cleanup,
        other => panic!("this rejection follows the move that could create it: {other:?}"),
    }
}

/// A recoverable removal creates the Removed Orders Directory lazily, so a rejection
/// after that creation would otherwise leave a directory in the user's Package that no
/// removal ever put anything in — and the user has no way to tell it from one they
/// created themselves.
#[test]
fn a_rejected_move_leaves_no_removed_orders_directory_it_created() {
    // The move errors outright, and the destination is taken between this operation's
    // own creation and its move — the only window in which a genuine collision can
    // follow a creation this operation is responsible for.
    for (label, injected) in [
        (
            "removal-rollback-move-error",
            FailingRemoval::move_erroring(),
        ),
        (
            "removal-rollback-move-collision",
            FailingRemoval::move_reporting(NewDirectoryRenameOutcome::DestinationExists),
        ),
    ] {
        let (_tree, root, directory) = default_package(label);
        let removed_root = root.join("Removed");
        assert!(!removed_root.exists(), "{label}: Removed starts absent");
        let plan = plan_order_removal(
            &directory,
            RemovalDisposition::RecoverableDirectory,
            &RealFileSystem,
        )
        .unwrap();
        assert!(plan.creates_removed_orders_directory(), "{label}");

        let report = commit_order_removal(plan, &injected, &UnreachableTrash);

        let RemovedDirectoryCleanup::Removed { path } = cleanup(&report) else {
            panic!("{label}: the rejection reports the directory it undid: {report:?}");
        };
        assert_eq!(path, &removed_root, "{label}");
        assert!(
            !removed_root.exists(),
            "{label}: the rejected operation left no directory behind"
        );
        assert!(directory.is_dir(), "{label}: the Order stays active");
        assert_eq!(indexed_order_ids(&root), [ORDER_ID.to_owned()], "{label}");
    }
}

#[test]
fn a_rejected_removal_never_undoes_a_removed_orders_directory_it_did_not_create() {
    let (_tree, root, directory) = default_package("removal-rollback-preexisting");
    let removed_root = root.join("Removed");
    // Empty and already there. Emptiness alone would not protect it, so this is the
    // case where only knowing it created nothing keeps the rollback off a directory the
    // user made themselves.
    fs::create_dir(&removed_root).unwrap();

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert!(!plan.creates_removed_orders_directory());

    let report = commit_order_removal(plan, &FailingRemoval::move_erroring(), &UnreachableTrash);

    assert!(
        matches!(cleanup(&report), RemovedDirectoryCleanup::NothingCreated),
        "this operation created nothing to undo: {report:?}"
    );
    assert!(
        removed_root.is_dir(),
        "a directory this operation did not create survives its rejection"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

#[test]
fn a_rejected_removal_retains_a_directory_it_created_that_evidence_arrived_in() {
    let (_tree, root, directory) = default_package("removal-rollback-arrival");
    let removed_root = root.join("Removed");
    let arrived = removed_root.join("arrived.txt");

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert!(plan.creates_removed_orders_directory());

    let report = commit_order_removal(
        plan,
        &FailingRemoval::move_erroring_after_arrival("arrived.txt"),
        &UnreachableTrash,
    );

    let RemovedDirectoryCleanup::Retained { path } = cleanup(&report) else {
        panic!("evidence this operation did not create retains the directory: {report:?}");
    };
    assert_eq!(path, &removed_root);
    assert_eq!(
        fs::read(&arrived).unwrap(),
        b"not ours\n",
        "the rollback never discards what it did not create"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

#[test]
fn a_cleanup_that_fails_is_reported_beside_the_failure_that_initiated_it() {
    let (_tree, root, directory) = default_package("removal-rollback-undo-failure");
    let removed_root = root.join("Removed");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();

    let report = commit_order_removal(
        plan,
        &FailingRemoval::move_erroring_with_undo_failing(),
        &UnreachableTrash,
    );

    // Beside, not instead of: the removal was refused because the move failed, and the
    // caller separately learns that the directory that refusal created is still there.
    let MutationReport::Rejected {
        rejection:
            OrderRemovalRejection::MoveFailed {
                cleanup: RemovedDirectoryCleanup::Failed { path, .. },
                ..
            },
        ..
    } = &report
    else {
        panic!("the initiating failure and the cleanup failure are both reported: {report:?}");
    };
    assert_eq!(path, &removed_root);
    assert!(
        removed_root.is_dir(),
        "the report names a directory that really is still there"
    );
    assert!(directory.is_dir(), "the Order stays active");
}

/// The record the plan observed is the one the rewrite is authorized to replace, and
/// the copy it replaces is that record's own, carried along by the move. A writer that
/// lands on it between the move and the rename — a restore, an adoption, a repair —
/// meets no other guard: the Order has left the caller's view and nothing looks at that
/// record again.
#[test]
fn a_record_given_a_different_meaning_after_the_move_is_not_overwritten() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-destination-changed");
    let (record, stage) = moved_record(&root.join("Removed").join(BASENAME));
    // The same Order, retitled by someone else: valid, parseable, different meaning.
    let theirs = String::from_utf8(fs::read(directory.join(".spielorders/order.json")).unwrap())
        .unwrap()
        .replace("Laser components", "Retitled by someone else")
        .into_bytes();

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let other_writer = ConcurrentWriter::replacing(&record, &theirs, EditPoint::StagingWrite);
    let report = commit_order_removal(plan, &other_writer, &UnreachableTrash);

    assert!(
        other_writer.fired(),
        "their record really did land under this rewrite"
    );
    assert_eq!(
        fs::read(&record).unwrap(),
        theirs,
        "their record survives byte for byte, stale locator and all"
    );
    // The directory moved, so the removal committed; what it declined to finish
    // afterwards is the warning.
    let MutationReport::CommittedWithWarning { warnings, .. } = &report else {
        panic!("the disposition happened, so the removal commits: {report:?}");
    };
    let [OrderRemovalWarning::LocatorUpdateFailed(failure)] = warnings.as_slice() else {
        panic!("the refused rewrite is the one warning: {warnings:?}");
    };
    assert!(
        matches!(
            failure.failure(),
            Some(ValidatedReplacementFailure::DestinationChanged)
        ),
        "refused because someone else changed the record, not because a write failed: {:?}",
        failure.failure()
    );
    assert!(
        !stage.exists(),
        "a refused replacement takes its own staging file back"
    );
    assert!(
        failure.orphaned_stage().is_none(),
        "nothing is reported as left behind, because nothing was: {failure:?}"
    );
}

/// The cost of a semantic expectation, stated rather than left to be discovered: a
/// record someone reformatted, or one whose stored facts were never trimmed, still
/// means what this removal planned against, so the locator rewrite proceeds and their
/// formatting is what is given up. A byte expectation, or a raw parse compared against
/// the load's normalized projection, would refuse to relocate a record a person had
/// tidied by hand.
#[test]
fn a_record_reformatted_after_the_move_is_still_relocated() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-destination-reformatted");
    let (record, _) = moved_record(&root.join("Removed").join(BASENAME));
    let ours = fs::read(directory.join(".spielorders/order.json")).unwrap();
    // Re-emitted with indentation, and with the title left untrimmed the way a hand
    // edit leaves it.
    let untrimmed = String::from_utf8(ours.clone())
        .unwrap()
        .replace("\"Laser components\"", "\"  Laser components  \"");
    let value: serde_json::Value = serde_json::from_str(&untrimmed).unwrap();
    let theirs = serde_json::to_vec_pretty(&value).unwrap();
    assert_ne!(theirs, ours, "their bytes differ from ours");
    assert_eq!(
        OrderDocument::from_json_bytes(&theirs)
            .unwrap()
            .validate()
            .unwrap()
            .order(),
        OrderDocument::from_json_bytes(&ours)
            .unwrap()
            .validate()
            .unwrap()
            .order(),
        "their record still means exactly what ours did"
    );

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let other_writer = ConcurrentWriter::replacing(&record, &theirs, EditPoint::StagingWrite);
    let report = commit_order_removal(plan, &other_writer, &UnreachableTrash);

    assert!(
        other_writer.fired(),
        "their reformatting really did land under this rewrite"
    );
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "a record that still means what was planned against is relocated: {report:?}"
    );
    assert_eq!(
        stored_locator(&root.join("Removed").join(BASENAME)),
        "../..",
        "the locator is rewritten to match where the directory now sits"
    );
}

/// The locator rewrite is a durable replacement like any other, so a failure of it
/// leaves a staging file beside the record. That file sits inside a directory the user
/// has just removed from active scope, where no diagnosis reaches it, so the warning is
/// the only thing that can name it. A warning that names nothing leaves an orphan
/// nobody removes.
#[test]
fn a_locator_rewrite_that_cannot_clean_up_names_the_stage_it_left() {
    let (_tree, root, directory) =
        package_with_a_deeper_orders_directory("removal-locator-cleanup-failure");
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    assert!(
        matches!(plan.locator(), LocatorOutcome::Updated { .. }),
        "this layout reaches the record rewrite: {:?}",
        plan.locator()
    );

    let report = commit_order_removal(
        plan,
        &FailingRemoval::locator_rewrite_failing_with_cleanup_failing(),
        &UnreachableTrash,
    );

    let removed = root.join("Removed").join(BASENAME);
    let (record, stage) = moved_record(&removed);
    assert!(
        stage.is_file(),
        "the failed replacement really did leave a staging file behind"
    );

    // The directory moved, so the removal committed. What could not be finished
    // afterwards is a warning, and it is the only one: the refresh still succeeds.
    let MutationReport::CommittedWithWarning { warnings, .. } = &report else {
        panic!("the disposition happened, so the removal commits: {report:?}");
    };
    let [OrderRemovalWarning::LocatorUpdateFailed(failure)] = warnings.as_slice() else {
        panic!("the rewrite that failed is the one warning: {warnings:?}");
    };
    assert_eq!(
        failure.record(),
        record,
        "the warning names the record that still carries the locator from before the move"
    );
    assert!(
        matches!(
            failure.failure(),
            Some(ValidatedReplacementFailure::Replacement(_))
        ),
        "the initiating failure is the rename, not a refusal: {:?}",
        failure.failure()
    );
    // Beside, not instead of: the caller learns why the rewrite did not happen and,
    // separately, that the attempt left a file it could not take back.
    assert_eq!(
        failure.orphaned_stage(),
        Some(stage.as_path()),
        "the report names a file that really is there: {failure:?}"
    );
    assert!(
        failure.cleanup_failure().is_some(),
        "and carries the error that stopped it being taken back: {failure:?}"
    );
    assert_eq!(
        stored_locator(&removed),
        "../../..",
        "the record is left exactly as the move found it"
    );
}

#[test]
fn a_post_move_index_failure_stays_committed_and_is_not_a_retry_signal() {
    let (_tree, root, directory) = default_package("removal-index-failure");
    let removed = root.join("Removed").join(BASENAME);
    let before = contents(&directory);
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();

    let report = commit_order_removal(plan, &FailingRemoval::index_failing(), &UnreachableTrash);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed index refresh remains a committed removal: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderRemovalWarning::IndexRefreshFailed(_)]
    ));
    // The disposition facts are complete even though the index could not be rebuilt.
    assert_eq!(
        result.disposition(),
        RemovalDisposition::RecoverableDirectory
    );
    assert_eq!(result.final_location(), Some(removed.as_path()));
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    // This arm reports real canonical evidence, so these reasons are the detector's
    // own verdict on the Package the removal left behind, not anything this
    // capability chose. The unrefreshed arms below must name the same change, or one
    // removal would hand a caller two different repair instructions.
    assert_eq!(
        result.index().stale_reasons(),
        [
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain
        ],
        "the detector names membership when an Order leaves"
    );

    // The move already happened, so this warning is never a reason to retry the
    // removal: the Order is no longer where a retry would look for it.
    assert!(!directory.exists());
    assert_eq!(contents(&removed), before);
    let retried = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    );
    assert!(
        retried.is_err(),
        "retrying the removal finds no Order to remove"
    );
}

/// Neither committed-without-refresh arm was reached by any test in this suite,
/// which is why a stale reason naming the wrong change survived in both. Each
/// disposition is driven through each arm, because the reason describes what left
/// the Orders Directory and both dispositions take the whole Order out of it.
#[test]
fn a_disposition_the_refresh_cannot_confirm_still_names_what_changed() {
    for disposition in [
        RemovalDisposition::RecoverableDirectory,
        RemovalDisposition::PlatformTrash,
    ] {
        let (_tree, root, directory) = default_package("removal-unconfirmed-refresh");
        let trashed = root.join("trash-store").join(BASENAME);

        // The Package cannot be listed at all after the Order has gone.
        let plan = plan_order_removal(&directory, disposition, &RealFileSystem).unwrap();
        let report = commit_order_removal(
            plan,
            &FailingRemoval::refresh_listing_failing(directory.clone()),
            &RemovingTrash::to(trashed.clone()),
        );
        let MutationReport::CommittedWithWarning {
            result, warnings, ..
        } = report
        else {
            panic!("the Order has already gone, so {disposition:?} is committed: {report:?}");
        };
        assert!(
            matches!(
                warnings.as_slice(),
                [OrderRemovalWarning::IndexRefreshFailed(_)]
            ),
            "{disposition:?}: {warnings:?}"
        );
        assert_eq!(result.disposition(), disposition);
        assert_eq!(result.index().state(), PackageIndexState::Stale);
        assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
        // The whole Order left the Orders Directory. It is not `OrderMetadataChanged`
        // — this removal wrote no Order record — and it matches what the detector
        // says above when it can look.
        assert_eq!(
            result.index().stale_reasons(),
            [PackageIndexStaleReason::OrderMembershipChanged],
            "{disposition:?} names the Order that left"
        );
        assert!(!directory.exists(), "{disposition:?} really disposed of it");
    }
}

#[test]
fn a_disposition_the_refresh_cannot_assess_at_all_still_names_what_changed() {
    for disposition in [
        RemovalDisposition::RecoverableDirectory,
        RemovalDisposition::PlatformTrash,
    ] {
        let (_tree, root, directory) = default_package("removal-unassessable-package");
        let trashed = root.join("trash-store").join(BASENAME);

        // The Package's own version marker stops parsing once the Order has gone, so
        // the refresh comes back with no current Package to inspect at all.
        let plan = plan_order_removal(&directory, disposition, &RealFileSystem).unwrap();
        let report = commit_order_removal(
            plan,
            &FailingRemoval::package_damaged_after(directory.clone()),
            &RemovingTrash::to(trashed.clone()),
        );
        let MutationReport::CommittedWithWarning {
            result, warnings, ..
        } = report
        else {
            panic!("the Order has already gone, so {disposition:?} is committed: {report:?}");
        };
        assert!(
            matches!(
                warnings.as_slice(),
                [OrderRemovalWarning::RefreshUnavailable]
            ),
            "{disposition:?}: {warnings:?}"
        );
        assert_eq!(result.disposition(), disposition);
        assert_eq!(result.index().state(), PackageIndexState::Stale);
        assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
        assert_eq!(
            result.index().stale_reasons(),
            [PackageIndexStaleReason::OrderMembershipChanged],
            "{disposition:?} names the Order that left"
        );
        assert!(!directory.exists(), "{disposition:?} really disposed of it");
    }
}
