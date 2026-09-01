//! Decision sources: Define the SpielOrders package topology; Define the Order
//! record and lifecycle contract; Define Vendor identity and historical semantics;
//! Define the Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::alignment::{
    OrderAlignmentOperationKind, OrderAlignmentPlanError, OrderAlignmentRejection,
    OrderAlignmentReport, OrderAlignmentWarning, commit_order_alignment, plan_order_alignment,
};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
};
use spielorders_core::evidence::{EvidenceOutcome, FactChange};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderStatus};
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_edits::{OrderAssignments, commit_order_edit, plan_order_edit};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeObservation, FileSystem, FileSystemEntryKind,
    LocalCalendar, LocalDate, NewDirectoryRenameOutcome, NewFileLinkOutcome, UuidGenerator,
};
use spielorders_core::vendors::{
    VendorAssignments, commit_vendor_mutation, plan_vendor_creation, plan_vendor_edit,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
/// A second Order identity, for the Order left standing where another Order was
/// planned. Everything but the identity matches, so only the identity can refuse.
const SUBSTITUTE_ORDER_ID: &str = "3f5c1d84-2b90-4a6e-8c17-6d0b95e4a731";
/// A deliberately drifted directory name, so alignment always has work to do.
const DRIFTED: &str = "Legacy_laser_purchase";

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

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

fn initialized_package(label: &str) -> TemporaryTree {
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
    tree
}

fn created_vendor(root: &Path, name: &str) -> String {
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", name),
        &RealFileSystem,
    )
    .expect("the fixture Vendor can be planned");
    let report = commit_vendor_mutation(plan, &mut FixedUuid(VENDOR_ID.into()), &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    result.vendor().vendor_id.as_str().to_owned()
}

/// Adopts one Order at a drifted directory name, optionally associating a Vendor and
/// an Orderer, so alignment has to project from exactly those facts.
fn drifted_order(
    root: &Path,
    basename: &str,
    vendor: Option<&str>,
    orderer: Option<&str>,
) -> PathBuf {
    let directory = root.join("Orders").join(basename);
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    let mut input = OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold);
    if let Some(vendor) = vendor {
        input = input.vendor_id(vendor);
    }
    if let Some(orderer) = orderer {
        input = input.orderer(orderer);
    }
    let plan = plan_order_adoption(root, &directory, &input, &RealFileSystem)
        .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the fixture Order commits: {report:?}"
    );
    directory
}

#[test]
fn the_projection_enumerates_every_optional_component_combination() {
    // The decided basename is `YYYY_MM_DD[_Vendor]_Title[_Orderer]`, with the Vendor
    // and Orderer components omitted when absent. That is four presence cases, and a
    // fixture carrying both could not separate a correct projection from one that
    // mishandles an absent component or emits an empty one.
    for (label, with_vendor, orderer, expected) in [
        ("neither", false, None, "2026_08_22_Laser_components"),
        (
            "vendor only",
            true,
            None,
            "2026_08_22_Tarnwick_Laser_components",
        ),
        (
            "orderer only",
            false,
            Some("Ada Lovelace"),
            "2026_08_22_Laser_components_Ada_Lovelace",
        ),
        (
            "both",
            true,
            Some("Ada Lovelace"),
            "2026_08_22_Tarnwick_Laser_components_Ada_Lovelace",
        ),
    ] {
        let tree = initialized_package("alignment-projection");
        let root = &fs::canonicalize(tree.path()).unwrap();
        let vendor = with_vendor.then(|| created_vendor(root, "Tarnwick"));
        let directory = drifted_order(root, DRIFTED, vendor.as_deref(), orderer);

        let plan = plan_order_alignment(&directory, &RealFileSystem)
            .unwrap_or_else(|error| panic!("{label} can be aligned: {error:?}"));

        assert_eq!(plan.basename(), expected, "{label}");
        assert_eq!(
            plan.destination(),
            root.join("Orders").join(expected),
            "{label}"
        );
        assert_eq!(plan.source().file_name().unwrap(), DRIFTED, "{label}");
        assert!(plan.changes_directory_name(), "{label}");
        assert_eq!(
            plan.operations()
                .iter()
                .map(|operation| operation.kind())
                .collect::<Vec<_>>(),
            [
                OrderAlignmentOperationKind::RenameOrderDirectory,
                OrderAlignmentOperationKind::RefreshPackageIndex,
            ],
            "{label}"
        );
        // Planning renames nothing.
        assert!(directory.is_dir(), "{label}");
        assert!(!root.join("Orders").join(expected).exists(), "{label}");
    }
}

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const ALIGNED: &str = "2026_08_22_Tarnwick_Laser_components_Ada_Lovelace";

/// A drifted Order carrying a README and two opaque Supporting Material sentinels,
/// one nested, so a rename can be shown to move the whole tree intact.
fn surrounded_drifted_order(label: &str) -> (TemporaryTree, PathBuf, String) {
    let tree = initialized_package(label);
    let root = fs::canonicalize(tree.path()).unwrap();
    let vendor = created_vendor(&root, "Tarnwick");
    let directory = root.join("Orders").join(DRIFTED);
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();

    let plan = plan_order_adoption(
        &root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold)
            .vendor_id(&vendor)
            .orderer("Ada Lovelace"),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem);
    assert!(matches!(report, MutationReport::Committed { .. }));
    (tree, directory, vendor)
}

fn indexed_directories(root: &Path) -> Vec<String> {
    let index =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    index
        .orders
        .iter()
        .map(|order| order.order_directory.as_str().to_owned())
        .collect()
}

#[test]
fn committing_renames_the_whole_tree_and_leaves_the_order_resolvable_at_its_new_name() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-commit");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    assert_eq!(
        indexed_directories(&root),
        [format!("Orders/{DRIFTED}")],
        "the index starts at the drifted name"
    );

    let plan = plan_order_alignment(&directory, &RealFileSystem).expect("the Order can be aligned");
    let report = commit_order_alignment(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("an explicit alignment commits: {report:?}");
    };
    assert!(!directory.exists(), "the drifted name is gone");
    assert!(aligned.is_dir());
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        aligned,
        "the result reports the Order at its new path"
    );

    // Identity and every domain fact survive a rename; the path is location, not
    // identity.
    let document = result.order().document();
    assert_eq!(document.order_id.as_str(), ORDER_ID);
    assert_eq!(document.title.as_str(), "Laser components");
    assert_eq!(document.date_ordered.as_str(), "2026-08-22");
    assert_eq!(document.status, OrderStatus::Hold);
    assert_eq!(
        document.vendor_id.as_ref().map(|id| id.as_str()),
        Some(vendor.as_str())
    );
    assert_eq!(document.orderer.as_deref(), Some("Ada Lovelace"));

    // The technical locator still resolves from the new location, and the derived
    // index names the new directory. A rename that left either stale would produce
    // exactly the Damaged Order the integrity decision describes.
    assert_eq!(indexed_directories(&root), [format!("Orders/{ALIGNED}")]);
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    let reloaded = plan_order_alignment(&aligned, &RealFileSystem)
        .expect("the Order still resolves through its own locator at the new path");
    assert!(
        !reloaded.changes_directory_name(),
        "the Order is now aligned"
    );
    assert_eq!(reloaded.order_id(), ORDER_ID);

    // The whole directory moved as one opaque tree.
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(aligned.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(aligned.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );
}

#[test]
fn an_already_aligned_order_is_unchanged_and_renames_nothing() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-no-op");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let plan = plan_order_alignment(&aligned, &RealFileSystem)
        .expect("an aligned Order is planned like any other");

    assert!(!plan.changes_directory_name());
    assert_eq!(plan.source(), plan.destination());
    assert!(
        plan.operations().is_empty(),
        "an alignment that renames nothing announces no managed operation"
    );

    let report = commit_order_alignment(plan, &RealFileSystem);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("an already aligned Order is an accepted no-op: {report:?}");
    };
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    assert!(aligned.is_dir());
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a no-op alignment rebuilds no index"
    );
}

#[test]
fn a_collision_stops_alignment_without_inventing_a_suffix() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-collision");
    let root = fs::canonicalize(tree.path()).unwrap();
    let occupied = root.join("Orders").join(ALIGNED);
    // Something already stands at the projected name.
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("sibling.txt"), b"someone else's directory\n").unwrap();
    let record_before = fs::read(directory.join(".spielorders/order.json")).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let error =
        plan_order_alignment(&directory, &RealFileSystem).expect_err("a collision stops alignment");

    let OrderAlignmentPlanError::DestinationCollision { path } = &error else {
        panic!("a collision is reported as such: {error:?}");
    };
    assert_eq!(path, &occupied);

    // Nothing moved, nothing was replaced, and no suffixed variant was invented.
    assert!(directory.is_dir(), "the drifted name is retained");
    assert_eq!(
        fs::read(directory.join(".spielorders/order.json")).unwrap(),
        record_before
    );
    assert_eq!(
        fs::read(occupied.join("sibling.txt")).unwrap(),
        b"someone else's directory\n",
        "the colliding sibling is untouched"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
    let names = fs::read_dir(root.join("Orders"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        [ALIGNED.to_owned(), DRIFTED.to_owned()]
            .into_iter()
            .collect(),
        "no suffixed variant such as _2 was created"
    );
}

/// Injects one directory-rename outcome. Every other effect is real.
struct FailingRename {
    outcome: Option<NewDirectoryRenameOutcome>,
}

impl FailingRename {
    fn erroring() -> Self {
        Self { outcome: None }
    }

    fn reporting(outcome: NewDirectoryRenameOutcome) -> Self {
        Self {
            outcome: Some(outcome),
        }
    }
}

impl FileSystem for FailingRename {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
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
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for FailingRename {
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
        _source: &Path,
        _destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        match self.outcome {
            Some(outcome) => Ok(outcome),
            None => Err(io::Error::other("injected directory rename failure")),
        }
    }
}

#[test]
fn an_injected_rename_failure_leaves_the_whole_source_untouched() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-rename-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    let report = commit_order_alignment(plan, &FailingRename::erroring());

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an injected rename failure rejects: {report:?}");
    };
    assert!(
        matches!(rejection, OrderAlignmentRejection::RenameFailed { .. }),
        "the rejection names the failed rename: {rejection:?}"
    );
    assert!(directory.is_dir(), "the source keeps its drifted name");
    assert!(!aligned.exists(), "no partial destination is left behind");
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(directory.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_destination_taken_between_planning_and_commit_stops_the_rename() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-collision-race");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The rename itself is the collision check, so a destination that appears after
    // planning is refused by the rename rather than overwritten.
    let report = commit_order_alignment(
        plan,
        &FailingRename::reporting(NewDirectoryRenameOutcome::DestinationExists),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a taken destination rejects: {report:?}");
    };
    let OrderAlignmentRejection::DestinationCollision { path } = &rejection else {
        panic!("a taken destination is a collision: {rejection:?}");
    };
    assert_eq!(path, &root.join("Orders").join(ALIGNED));
    assert!(directory.is_dir());
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
}

#[test]
fn a_source_that_disappeared_or_became_a_link_rejects_before_renaming() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-source-vanished");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    fs::remove_dir_all(&directory).unwrap();

    let report = commit_order_alignment(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a vanished source rejects: {report:?}");
    };
    assert!(
        matches!(
            rejection,
            OrderAlignmentRejection::PreconditionsChanged { .. }
        ),
        "a vanished source is a precondition change: {rejection:?}"
    );
    assert!(!root.join("Orders").join(ALIGNED).exists());

    // A source replaced by a link is not the Order Directory this plan validated.
    #[cfg(unix)]
    {
        let (tree, directory, _vendor) = surrounded_drifted_order("alignment-source-link");
        let root = fs::canonicalize(tree.path()).unwrap();
        let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
        let elsewhere = root.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::remove_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &directory).unwrap();

        let report = commit_order_alignment(plan, &RealFileSystem);

        assert!(
            matches!(
                report,
                MutationReport::Rejected {
                    rejection: OrderAlignmentRejection::PreconditionsChanged { .. },
                    ..
                }
            ),
            "a linked source is a precondition change: {report:?}"
        );
        assert!(!root.join("Orders").join(ALIGNED).exists());
        assert!(fs::symlink_metadata(&directory).unwrap().is_symlink());
    }
}

#[test]
fn the_projection_follows_the_live_vendor_name() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-live-vendor");
    let root = fs::canonicalize(tree.path()).unwrap();

    // An Order keeps no historical Vendor snapshot, so renaming the Vendor changes
    // the name this Order projects.
    let plan = plan_vendor_edit(
        &root,
        &vendor,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("the Vendor can be renamed");
    assert!(matches!(
        commit_vendor_mutation(plan, &mut FixedUuid(VENDOR_ID.into()), &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    assert_eq!(
        plan.basename(),
        "2026_08_22_Nethermoor_Laser_components_Ada_Lovelace"
    );
    // A Vendor rename is not itself an alignment: the directory still drifts.
    assert!(directory.is_dir());
    assert_eq!(directory.file_name().unwrap(), DRIFTED);
}

#[test]
fn editing_order_facts_never_renames_the_directory_implicitly() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-not-implicit");
    let root = fs::canonicalize(tree.path()).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    // Changing a projected fact makes the drift larger, and that is allowed state.
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the edit can be planned");
    assert!(matches!(
        commit_order_edit(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    assert!(
        directory.is_dir(),
        "a fact edit renames no directory on its own"
    );
    assert_eq!(directory.file_name().unwrap(), DRIFTED);
    assert_ne!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "the edit itself did refresh the index"
    );
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);

    // Only the explicit action renames, and it projects from the edited facts.
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        plan.basename(),
        "2026_08_22_Tarnwick_Laser_mounts_Ada_Lovelace"
    );
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert!(
        root.join("Orders")
            .join("2026_08_22_Tarnwick_Laser_mounts_Ada_Lovelace")
            .is_dir()
    );
}

/// Renames for real, then fails the derived-index replacement that follows.
struct FailingIndexRefresh;

impl FileSystem for FailingIndexRefresh {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
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
        if destination.ends_with(".spielorders/index.json") {
            return Err(io::Error::other("injected index replacement failure"));
        }
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

impl DirectoryFileSystem for FailingIndexRefresh {
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

#[test]
fn a_post_commit_index_failure_is_a_warning_rather_than_a_rename_retry_signal() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-index-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    let report = commit_order_alignment(plan, &FailingIndexRefresh);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderAlignmentWarning::IndexRefreshFailed(_)]
    ));
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    // This arm reports real canonical evidence, so these reasons are the detector's
    // own verdict on the renamed Package rather than anything this capability chose.
    // It must name the same change the without-refresh path names, or one rename
    // would hand a caller two different repair instructions.
    assert_eq!(
        result.index().stale_reasons(),
        [
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain
        ],
        "the detector independently names membership, not a path mismatch"
    );
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        aligned,
        "the result reports the Order where the rename actually left it"
    );
    // A projection built without a refreshed Package still owes the caller the
    // complete Order, README included, rather than an empty document.
    assert_eq!(
        result.order().readme_text().map(str::as_bytes),
        Some(README),
        "the committed report carries the Order's real README"
    );

    // The rename already happened. Retrying the alignment would find the Order
    // aligned and do nothing, so the warning must not read as a rename retry signal.
    assert!(aligned.is_dir());
    assert!(!directory.exists());
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
    let retried =
        plan_order_alignment(&aligned, &RealFileSystem).expect("the renamed Order still resolves");
    assert!(!retried.changes_directory_name());
    assert!(retried.operations().is_empty());
}

/// What goes wrong after the rename has already succeeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AfterRename {
    FailRefreshLoad,
    /// The Package still loads, but this Order's record no longer parses, so it
    /// reloads as a Damaged Order rather than a healthy one.
    DamageTheRecord,
    /// The Package's own version marker stops parsing, so the refresh returns a
    /// projection that carries no current Package at all.
    DamageThePackage,
}

/// Renames for real, then breaks the refresh that follows, so the report has to be
/// built from the plan rather than from a refreshed Package.
struct FailingRefreshLoad {
    after: AfterRename,
    renamed: Cell<bool>,
}

impl FailingRefreshLoad {
    fn after(after: AfterRename) -> Self {
        Self {
            after,
            renamed: Cell::new(false),
        }
    }
}

impl FileSystem for FailingRefreshLoad {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if self.renamed.get()
            && self.after == AfterRename::DamageTheRecord
            && path.ends_with(".spielorders/order.json")
        {
            return Ok(b"{ not json".to_vec());
        }
        // Only the post-rename refresh load sees the damaged marker.
        if self.renamed.get()
            && self.after == AfterRename::DamageThePackage
            && path.ends_with("version.json")
        {
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
        // Only the post-rename refresh load is failed, never the planning reads.
        if self.renamed.get() && self.after == AfterRename::FailRefreshLoad {
            return Err(io::Error::other("injected refresh listing failure"));
        }
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for FailingRefreshLoad {
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
        let outcome = RealFileSystem.rename_new_directory(source, destination)?;
        if outcome == NewDirectoryRenameOutcome::Renamed {
            self.renamed.set(true);
        }
        Ok(outcome)
    }
}

#[test]
fn a_report_built_without_a_refreshed_package_still_carries_the_orders_readme() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-refresh-load-failure");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The rename succeeds and the Package then fails to reload, so this report is
    // projected locally. It is the one path that cannot borrow a refreshed Order.
    let report = commit_order_alignment(
        plan,
        &FailingRefreshLoad::after(AfterRename::FailRefreshLoad),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderAlignmentWarning::IndexRefreshFailed(_)]
    ));
    assert_eq!(
        result.order().readme_text().map(str::as_bytes),
        Some(README),
        "a locally projected report still carries the Order's real README"
    );
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        aligned,
        "and reports it at the path the rename produced"
    );
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    // A rename moves the Order to a new name inside the same Orders Directory, so
    // what the index no longer matches is the set of Order paths. It is not
    // `OrderMetadataChanged` — this alignment wrote no Order record — and not
    // `OrderPathMismatch`, which means an index entry points outside the Orders
    // Directory entirely. A caller repairs the index by acting on this reason.
    assert_eq!(
        result.index().stale_reasons(),
        [PackageIndexStaleReason::OrderMembershipChanged]
    );
    assert!(aligned.is_dir());
    assert_eq!(fs::read(aligned.join("README.md")).unwrap(), README);
}

#[test]
fn a_completed_rename_is_never_reported_as_work_that_did_not_happen() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-undiscoverable");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The rename succeeds, the Package still loads, but this Order's record stops
    // parsing before the refresh, so it comes back as a Damaged Order and is absent
    // from the healthy set the report would otherwise borrow from.
    let report = commit_order_alignment(
        plan,
        &FailingRefreshLoad::after(AfterRename::DamageTheRecord),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the directory was renamed on disk, so the report is committed: {report:?}");
    };
    assert_eq!(warnings, [OrderAlignmentWarning::RefreshUnavailable]);
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        aligned,
        "the report names the path the rename actually produced"
    );
    assert_eq!(
        result.order().readme_text().map(str::as_bytes),
        Some(README)
    );
    // The rename is real on disk, which is exactly why it must not be reported as a
    // rejection: a caller told "nothing happened" would be misled.
    assert!(aligned.is_dir());
    assert!(!directory.exists());
}

#[test]
fn a_rename_the_refresh_cannot_assess_at_all_still_names_what_changed() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-unassessable-package");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The rename succeeds and only then does the Package's own version marker stop
    // parsing, so the refresh comes back with no current Package to inspect. This is
    // the one arm no other test reaches, and it is where a stale reason is easiest to
    // get wrong precisely because nothing was observing it.
    let report = commit_order_alignment(
        plan,
        &FailingRefreshLoad::after(AfterRename::DamageThePackage),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the directory was renamed on disk, so the report is committed: {report:?}");
    };
    assert_eq!(warnings, [OrderAlignmentWarning::RefreshUnavailable]);
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        aligned,
        "the report names the path the rename actually produced"
    );
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    // The same reason the other unrefreshed arm gives, and the same change the
    // detector names when it can look: a rename moves an Order between paths.
    assert_eq!(
        result.index().stale_reasons(),
        [PackageIndexStaleReason::OrderMembershipChanged]
    );
    assert!(aligned.is_dir());
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// A refusal is only useful if it names *which* fact moved. The rejection variant
/// alone would be the same unexplained precondition failure for a substituted Order,
/// an edited title, and a renamed Vendor.
fn unconfirmed_facts<E: std::fmt::Debug>(
    report: &OrderAlignmentReport<E>,
) -> Vec<(&str, FactChange)> {
    let MutationReport::Rejected {
        rejection: OrderAlignmentRejection::PlanEvidenceUnconfirmed { outcome },
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

#[test]
fn a_different_order_left_at_the_source_path_is_refused_though_it_is_still_a_directory() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-substituted-order");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The planned Order is gone and a different Order stands at exactly the same
    // path, carrying the same projecting facts. Every check the commit could make
    // about the *path* still passes: it is present, it is an ordinary directory, and
    // it is a healthy Order of this Package. Only its identity differs, and renaming
    // it would apply one Order's authorized alignment to another Order's directory.
    fs::remove_dir_all(&directory).unwrap();
    fs::create_dir(&directory).unwrap();
    let substituted = plan_order_adoption(
        &root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold)
            .vendor_id(&vendor)
            .orderer("Ada Lovelace"),
        &RealFileSystem,
    )
    .expect("the substituted directory can be adopted");
    assert!(matches!(
        commit_order_adoption(
            substituted,
            &mut FixedUuid(SUBSTITUTE_ORDER_ID.into()),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));
    assert!(
        fs::symlink_metadata(&directory).unwrap().is_dir(),
        "the source is still an ordinary directory, so existence alone cannot refuse"
    );

    let report = commit_order_alignment(plan, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [("order_id", FactChange::Differs)],
        "the refusal names the identity that moved, not the path"
    );
    // Nothing was renamed, and the Order that happens to occupy the path keeps its
    // own name and its own record.
    assert!(!aligned.exists(), "no rename occurred");
    assert!(directory.is_dir());
    let resident = plan_order_alignment(&directory, &RealFileSystem)
        .expect("the substituted Order is untouched and still plans on its own terms");
    assert_eq!(resident.order_id(), SUBSTITUTE_ORDER_ID);
}

#[test]
fn editing_a_basename_governing_fact_after_planning_is_refused() {
    // Every fact the aligned basename projects from, and the fact name the refusal is
    // expected to give for it. A test that only moved the title would leave the
    // others free to change unnoticed between planning and the rename. Orderer is a
    // Reusable Free-Entry Value, so a genuinely new one is introduced rather than
    // assigned; that is the confirmation, not a different kind of edit.
    for (label, field, value, reusable, expected) in [
        (
            "alignment-stale-title",
            "title",
            "Laser mounts",
            false,
            [
                ("destination", FactChange::Differs),
                ("title", FactChange::Differs),
            ],
        ),
        (
            "alignment-stale-date",
            "date_ordered",
            "2026-08-23",
            false,
            [
                ("date_ordered", FactChange::Differs),
                ("destination", FactChange::Differs),
            ],
        ),
        (
            "alignment-stale-orderer",
            "orderer",
            "Grace Hopper",
            true,
            [
                ("destination", FactChange::Differs),
                ("orderer", FactChange::Differs),
            ],
        ),
    ] {
        let (tree, directory, _vendor) = surrounded_drifted_order(label);
        let root = fs::canonicalize(tree.path()).unwrap();
        let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
        let planned_destination = plan.destination().to_path_buf();

        // An ordinary fact edit. It deliberately does not rename the directory, so
        // the source the plan recorded is still exactly where it was — and the name
        // the plan would rename it to is no longer the one these facts project.
        let assignments = if reusable {
            OrderAssignments::new().introduce(field, value)
        } else {
            OrderAssignments::new().assign(field, value)
        };
        let edit = plan_order_edit(&directory, &assignments, &FixedCalendar, &RealFileSystem)
            .expect("the fact can be edited");
        assert!(matches!(
            commit_order_edit(edit, &RealFileSystem),
            MutationReport::Committed { .. }
        ));

        let report = commit_order_alignment(plan, &RealFileSystem);

        assert_eq!(
            unconfirmed_facts(&report),
            expected,
            "editing {field} after planning is refused and named"
        );
        assert!(
            !planned_destination.exists(),
            "{field}: the stale projected name was never created"
        );
        assert!(directory.is_dir(), "{field}: the source is untouched");
        assert_eq!(
            indexed_directories(&root),
            [format!("Orders/{DRIFTED}")],
            "{field}: the derived index still names the drifted directory"
        );
    }
}

#[test]
fn clearing_the_vendor_reference_after_planning_is_refused() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-cleared-vendor");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    let planned_destination = plan.destination().to_path_buf();

    // Clearing the association drops the Vendor component from the projection
    // entirely, so the plan's name is wrong in a way no Vendor *value* comparison
    // would notice.
    let edit = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("vendor_id", ""),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Vendor Reference can be cleared");
    assert!(matches!(
        commit_order_edit(edit, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let report = commit_order_alignment(plan, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_id", FactChange::Vanished),
            ("vendor_name", FactChange::Vanished),
        ],
        "a cleared association is named as a fact that vanished, not one that differs"
    );
    assert!(!planned_destination.exists());
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);
}

#[test]
fn associating_a_vendor_after_planning_is_refused() {
    let tree = initialized_package("alignment-appeared-vendor");
    let root = fs::canonicalize(tree.path()).unwrap();
    // An Order with no Vendor at all, so its projection omits the Vendor component.
    let directory = drifted_order(&root, DRIFTED, None, Some("Ada Lovelace"));
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    assert_eq!(plan.basename(), "2026_08_22_Laser_components_Ada_Lovelace");
    let planned_destination = plan.destination().to_path_buf();

    // The mirror of clearing a reference: a plan authorized against "this Order has no
    // Vendor" must not proceed once it has one, and the projection it would rename to
    // gains a whole component.
    let vendor = created_vendor(&root, "Tarnwick");
    let edit = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("vendor_id", &vendor),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("a Vendor can be associated");
    assert!(matches!(
        commit_order_edit(edit, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let report = commit_order_alignment(plan, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_id", FactChange::Appeared),
            ("vendor_name", FactChange::Appeared),
        ],
        "an association that appeared is named as such, not as one that differs"
    );
    assert!(!planned_destination.exists());
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);
}

#[test]
fn renaming_the_vendor_after_planning_is_refused() {
    let (tree, directory, vendor) = surrounded_drifted_order("alignment-stale-vendor-name");
    let root = fs::canonicalize(tree.path()).unwrap();
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    let planned_destination = plan.destination().to_path_buf();
    assert_eq!(planned_destination, root.join("Orders").join(ALIGNED));

    // An Order keeps no Vendor snapshot, so an ordinary Vendor rename changes the
    // name this Order projects while every Order fact, the Vendor ID, and the source
    // path all stand unchanged. This is the case a fact-only comparison cannot see.
    let rename = plan_vendor_edit(
        &root,
        &vendor,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("the Vendor can be renamed");
    assert!(matches!(
        commit_vendor_mutation(rename, &mut FixedUuid(VENDOR_ID.into()), &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let report = commit_order_alignment(plan, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("vendor_name", FactChange::Differs),
        ],
        "the refusal names the live Vendor Name the projection resolved"
    );
    assert!(
        !planned_destination.exists(),
        "the name projected from the old Vendor Name was never created"
    );
    assert!(directory.is_dir());
    assert_eq!(indexed_directories(&root), [format!("Orders/{DRIFTED}")]);

    // Planning again against the current Vendor Name succeeds: the refusal is about
    // this plan being stale, not about the Order having become unalignable.
    let replanned =
        plan_order_alignment(&directory, &RealFileSystem).expect("a fresh plan is authorized");
    assert_eq!(
        replanned.basename(),
        "2026_08_22_Nethermoor_Laser_components_Ada_Lovelace"
    );
    assert!(matches!(
        commit_order_alignment(replanned, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
}

#[test]
fn an_already_aligned_order_whose_facts_moved_is_refused_rather_than_reported_unchanged() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-stale-no-op");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    assert!(matches!(
        commit_order_alignment(
            plan_order_alignment(&directory, &RealFileSystem).unwrap(),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    // This plan renames nothing, so it has no rename to refuse. It still carries a
    // claim: that this Order's facts project the name it already has. Reporting
    // `Unchanged` after a title edit would tell the caller the Order is aligned at
    // the moment it stopped being aligned.
    let plan = plan_order_alignment(&aligned, &RealFileSystem).unwrap();
    assert!(!plan.changes_directory_name());
    let edit = plan_order_edit(
        &aligned,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the title can be edited");
    assert!(matches!(
        commit_order_edit(edit, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let report = commit_order_alignment(plan, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [
            ("destination", FactChange::Differs),
            ("title", FactChange::Differs)
        ]
    );
    assert!(aligned.is_dir(), "a refused no-op renames nothing either");
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a refused no-op rebuilds no index"
    );
}

#[test]
fn a_source_that_stopped_being_a_healthy_order_is_refused_before_the_rename() {
    let (tree, directory, _vendor) = surrounded_drifted_order("alignment-damaged-source");
    let root = fs::canonicalize(tree.path()).unwrap();
    let aligned = root.join("Orders").join(ALIGNED);
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();

    // The path is still an ordinary directory holding the user's Supporting Material,
    // but its canonical record no longer parses, so nothing there confirms which
    // Order this is. A rename here would move a Damaged Order to the aligned name of
    // an Order nobody can now identify.
    fs::write(directory.join(".spielorders/order.json"), b"{ not json").unwrap();

    let report = commit_order_alignment(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an unidentifiable source rejects: {report:?}");
    };
    assert!(
        matches!(
            rejection,
            OrderAlignmentRejection::PreconditionsChanged { .. }
        ),
        "a source that no longer resolves is a precondition change: {rejection:?}"
    );
    assert!(!aligned.exists(), "no rename occurred");
    assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
}
