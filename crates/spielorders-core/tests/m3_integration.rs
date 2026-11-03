//! Integrated M3 acceptance evidence: capabilities composed through public Rust-core
//! seams, proving claims no single capability ticket owns.
//!
//! Decision sources: Define the SpielOrders package topology; Define Vendor identity
//! and historical semantics; Define the Rust CLI capability contract; Define
//! integrity, validation, and repair boundaries.

use crate::support::{ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{
    OrderAdoptionInput, OrderAdoptionWarning, commit_order_adoption, plan_order_adoption,
};
use spielorders_core::alignment::{
    OrderAlignmentPlanError, OrderAlignmentWarning, commit_order_alignment, plan_order_alignment,
};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexState, PackageLoad, open_package,
};
use spielorders_core::creation::{
    OrderCreationInput, OrderCreationWarning, commit_order_creation, plan_order_creation,
};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{OrderDocument, OrderStatus};
use spielorders_core::metadata_repair::{
    RepairConfirmation, RepairResolution, commit_metadata_repair, plan_metadata_repair,
};
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_diagnosis::diagnose_order;
use spielorders_core::order_edits::{
    ItemAssignments, OrderAssignments, OrderEditPlanError, OrderEditWarning, commit_order_edit,
    commit_order_item_mutation, plan_order_edit, plan_order_item_add,
};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeObservation, FileSystem, FileSystemEntryKind,
    LocalCalendar, LocalDate, NewDirectoryRenameOutcome, NewFileLinkOutcome, UuidGenerator,
};
use spielorders_core::readme::DiagnosticCode;
use spielorders_core::readme_writes::{
    ReadmeWriteRejection, ReadmeWriteWarning, commit_readme_write, plan_order_readme_write,
    plan_package_readme_write,
};
use spielorders_core::removal::{
    OrderRemovalRejection, OrderRemovalWarning, RemovalDisposition, RemovedDirectoryCleanup,
    commit_order_removal, plan_order_removal,
};
use spielorders_core::repair_cases::RepairAction;
use spielorders_core::semantic_repair::{
    SemanticRepairWarning, SemanticResolution, commit_semantic_repair, plan_semantic_repair,
};
use spielorders_core::upgrade::{
    MetadataTransformation, MetadataUpgradeWarning, TransformationRefused, TransformationRegistry,
    UpgradeArtifact, UpgradeArtifactState, commit_metadata_upgrade, plan_metadata_upgrade,
};
use spielorders_core::vendors::{
    CanonicalVendorOutcome, ContactAssignments, VendorAssignments, VendorMutationWarning,
    commit_vendor_contact_mutation, commit_vendor_mutation, plan_vendor_contact_add,
    plan_vendor_creation, plan_vendor_hide,
};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const ORDER_README: &str = "# Optical bench\n\nWritten through the core.\n";
/// A well-formed Vendor ID that no Vendor in the Package carries.
const ABSENT_VENDOR: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const FIRST_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const SECOND_ID: &str = "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f";
const THIRD_ID: &str = "5d7a9b03-2e14-4c86-b0f5-73a8c9e1d240";
const VENDOR_ID: &str = "5a2f1c88-9b30-4c17-a4de-2f6b7c8d9e01";
/// A configured Orders Directory that no caller could guess from the default spelling,
/// and a Removed Directory renamed alongside it.
const NESTED_ORDERS: &str = "Admin/Purchase Orders";
const RETIRED: &str = "Retired";

struct QueuedUuids(RefCell<Vec<String>>);

impl QueuedUuids {
    fn of(values: &[&str]) -> Self {
        Self(RefCell::new(
            values.iter().rev().map(|v| (*v).to_owned()).collect(),
        ))
    }
}

impl UuidGenerator for QueuedUuids {
    fn generate_v4(&mut self) -> String {
        self.0.borrow_mut().pop().expect("a prepared identity")
    }
}

struct UnreachableTrash;

impl spielorders_core::ports::NativeTrash for UnreachableTrash {
    type Error = io::Error;

    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        panic!("a recoverable removal never reaches the platform Trash: {path:?}");
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

/// A bounded transformation double. It drives the upgrade orchestration and nothing
/// else: it stamps a `schema_version`, optionally refusing the one artifact whose text
/// carries `refuse_containing`. It publishes no durable schema and makes no version
/// part of the product contract.
struct StampVersion {
    source: u64,
    refuse_containing: Option<String>,
}

impl StampVersion {
    fn from(source: u64) -> Self {
        Self {
            source,
            refuse_containing: None,
        }
    }

    fn refusing(source: u64, marker: &str) -> Self {
        Self {
            source,
            refuse_containing: Some(marker.to_owned()),
        }
    }
}

impl MetadataTransformation for StampVersion {
    fn source_version(&self) -> u64 {
        self.source
    }

    fn transform(
        &self,
        _artifact: UpgradeArtifact,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransformationRefused> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| TransformationRefused)?;
        if self
            .refuse_containing
            .as_deref()
            .is_some_and(|marker| text.contains(marker))
        {
            return Err(TransformationRefused);
        }
        Ok(text
            .replace(
                &format!("\"schema_version\": {}", self.source),
                &format!("\"schema_version\": {}", self.source + 1),
            )
            .into_bytes())
    }
}

/// Where a capability's single injected failure lands. Every capability suite grows
/// its own double for the boundaries that capability owns; the gate composes failures
/// across capabilities, so it names only the boundaries composition crosses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    /// The staging write fails, so the capability refuses before any canonical byte
    /// is written and has nothing to undo.
    StagingWrite,
    /// Staging durability fails and the teardown that answers it fails too, so a
    /// retained staging file is reported beside the refusal.
    StagingDurabilityAndCleanup,
    /// The canonical write succeeds and the derived index refresh after it fails.
    IndexRefresh,
    /// A whole-directory move fails, and undoing the directory that attempt created
    /// fails as well.
    DirectoryMoveAndUndo,
}

const INDEX: &str = ".spielorders/index.json";
const INDEX_STAGE: &str = ".spielorders/.index.json.spielorders-stage";

/// A capability's own staging file, as distinct from the derived index's. The two are
/// separated so a staging boundary and an index boundary can never fire for each other.
fn is_capability_stage(path: &Path) -> bool {
    !path.ends_with(INDEX_STAGE)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".spielorders-stage"))
}

/// A real filesystem with exactly one boundary failing.
struct Injected {
    boundary: Boundary,
    fired: Cell<bool>,
}

impl Injected {
    fn at(boundary: Boundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
        }
    }

    fn fail_once(&self, boundary: Boundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: Boundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for Injected {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if is_capability_stage(path) && self.fail_once(Boundary::StagingWrite) {
            return Err(Self::error(Boundary::StagingWrite));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if is_capability_stage(path) && self.fail_once(Boundary::StagingDurabilityAndCleanup) {
            return Err(Self::error(Boundary::StagingDurabilityAndCleanup));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(INDEX) && self.boundary == Boundary::IndexRefresh {
            return Err(Self::error(Boundary::IndexRefresh));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        // Only after the durability failure it answers, so the teardown this reports
        // is the one that failure asked for.
        if is_capability_stage(path)
            && self.boundary == Boundary::StagingDurabilityAndCleanup
            && self.fired.get()
        {
            return Err(Self::error(Boundary::StagingDurabilityAndCleanup));
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
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for Injected {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if self.boundary == Boundary::DirectoryMoveAndUndo {
            return Err(Self::error(Boundary::DirectoryMoveAndUndo));
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
        if self.boundary == Boundary::DirectoryMoveAndUndo {
            return Err(Self::error(Boundary::DirectoryMoveAndUndo));
        }
        RealFileSystem.rename_new_directory(source, destination)
    }
}

fn initialized_package(label: &str) -> (TemporaryTree, PathBuf) {
    package_with_layout(label, &InitializationOptions::default())
}

fn package_with_layout(label: &str, options: &InitializationOptions) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(tree.path(), options, &RealFileSystem)
        .expect("the Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

fn adopted_order(root: &Path, basename: &str, uuid: &str, vendor: Option<&str>) -> PathBuf {
    adopted_order_at(root, "Orders", basename, uuid, vendor)
}

fn adopted_order_at(
    root: &Path,
    orders_path: &str,
    basename: &str,
    uuid: &str,
    vendor: Option<&str>,
) -> PathBuf {
    let directory = legacy_directory(root, orders_path, basename);
    let plan = plan_order_adoption(root, &directory, &legacy_facts(vendor), &RealFileSystem)
        .expect("the fixture directory can be adopted");
    assert!(matches!(
        commit_order_adoption(plan, &mut QueuedUuids::of(&[uuid]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    directory
}

/// An unmanaged directory carrying a user-authored README and two opaque sentinels,
/// ready for adoption.
fn legacy_directory(root: &Path, orders_path: &str, basename: &str) -> PathBuf {
    let directory = root.join(orders_path).join(basename);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("README.md"), README).unwrap();
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    fs::write(directory.join("Quotes/drawing.bin"), SUPPORTING_B).unwrap();
    directory
}

fn legacy_facts(vendor: Option<&str>) -> OrderAdoptionInput {
    let input = OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold);
    match vendor {
        Some(vendor) => input.vendor_name(vendor),
        None => input,
    }
}

/// Puts a Package at a source version below the build's, so the transformation double
/// has something to convert.
fn regress_to_zero(root: &Path, orders_path: &str, basenames: &[&str]) {
    fs::write(
        root.join(".spielorders/version.json"),
        b"{\n  \"metadata_version\": 0\n}\n",
    )
    .unwrap();
    let mut artifacts = basenames
        .iter()
        .map(|basename| record(&root.join(orders_path).join(basename)))
        .collect::<Vec<_>>();
    artifacts.push(vendors_record(root));
    for path in artifacts {
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("\"schema_version\": 1", "\"schema_version\": 0");
        fs::write(path, text).unwrap();
    }
}

fn vendors_record(root: &Path) -> PathBuf {
    root.join(".spielorders/vendors.json")
}

fn marker_version(root: &Path) -> u64 {
    declared_version(&root.join(".spielorders/version.json"), "metadata_version")
        .expect("the marker declares a version")
}

fn artifact_version(path: &Path) -> Option<u64> {
    declared_version(path, "schema_version")
}

fn declared_version(path: &Path, field: &str) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(&fs::read(path).ok()?)
        .ok()?
        .get(field)?
        .as_u64()
}

/// A complete, valid Order record for a Package whose Orders sit two directories deep.
fn nested_replacement_record(order_id: &str) -> Vec<u8> {
    format!(
        "{{\n  \"schema_version\": 1,\n  \"package_root\": \"../../..\",\n  \"order_id\": \"{order_id}\",\n  \"title\": \"Laser components\",\n  \"date_ordered\": \"2026-08-22\",\n  \"status\": \"hold\",\n  \"items\": []\n}}\n"
    )
    .into_bytes()
}

fn record(directory: &Path) -> PathBuf {
    directory.join(".spielorders/order.json")
}

/// Both opaque sentinels plus the user-authored README, wherever the Order Directory
/// currently sits.
fn opaque_contents(directory: &Path) -> Vec<Vec<u8>> {
    vec![
        fs::read(directory.join("README.md")).expect("README"),
        fs::read(directory.join("quote.bin")).expect("sentinel a"),
        fs::read(directory.join("Quotes/drawing.bin")).expect("nested sentinel b"),
    ]
}

#[test]
fn an_unresolved_vendor_reference_is_diagnosed_and_repairable_beside_another_case() {
    // SpielOrders itself can no longer create an Unresolved Vendor Reference — a Vendor
    // is hidden, never deleted, so its ID stays resolvable forever. One can still arrive
    // by hand-editing, external corruption, or a legacy adoption carrying a bogus ID,
    // which is exactly how this manufactures it. This gate confirms the resulting state
    // is diagnosed and repairable rather than stranded, alongside a second condition, so
    // neither masks the other.
    let (_tree, root) = initialized_package("m3-unresolved-vendor");
    let first = adopted_order(&root, "2026_08_22_First", FIRST_ID, None);
    let second = adopted_order(&root, "2026_08_22_Second", SECOND_ID, None);
    let text = fs::read_to_string(record(&first)).unwrap().replace(
        "\"items\": []",
        &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
    );
    fs::write(record(&first), text).unwrap();
    // A second, independent condition on the same Order: a duplicate Order identity.
    let text = fs::read_to_string(record(&second))
        .unwrap()
        .replace(SECOND_ID, FIRST_ID);
    fs::write(record(&second), text).unwrap();

    // The reference is unresolved, and diagnosis says so with the same code the
    // Package-wide read model uses.
    let diagnosis = diagnose_order(&first, &RealFileSystem).expect("the Order diagnoses");
    let case = diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == DiagnosticCode::VendorReferenceUnresolved)
        .expect("the orphaned reference is diagnosed");
    assert_eq!(
        case.actions(),
        [
            RepairAction::AssignExistingVendor,
            RepairAction::CreateAndAssignVendor,
            RepairAction::ClearVendorReference,
        ],
        "all three authorized resolutions are offered"
    );
    // Both conditions are diagnosed together rather than one masking the other, so
    // recovery is not two-step: scoped diagnosis narrows the exhaustive Package
    // validation, which reports every diagnostic at once.
    assert_eq!(
        diagnosis
            .cases()
            .iter()
            .map(|case| case.code())
            .collect::<Vec<_>>(),
        [
            DiagnosticCode::OrderIdDuplicate,
            DiagnosticCode::VendorReferenceUnresolved,
        ],
        "the orphaned reference and the identity conflict are both visible at once"
    );

    let handle = case.handle().to_owned();
    let plan = plan_semantic_repair(
        &first,
        &handle,
        &SemanticResolution::ClearVendorReference,
        &RealFileSystem,
    )
    .expect("the orphaned reference can be repaired");
    assert!(matches!(
        commit_semantic_repair(plan, &mut QueuedUuids::of(&[]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    // Resolving one case leaves the other standing and repairable, so no canonical
    // byte was lost and no state is stranded.
    let second_before = fs::read(record(&second)).unwrap();
    let reloaded = diagnose_order(&first, &RealFileSystem).expect("the Order diagnoses again");
    assert_eq!(
        reloaded
            .cases()
            .iter()
            .map(|case| case.code())
            .collect::<Vec<_>>(),
        [DiagnosticCode::OrderIdDuplicate],
        "the reference is resolved and only the identity conflict remains"
    );
    assert!(
        reloaded
            .cases()
            .iter()
            .any(|case| case.actions() == [RepairAction::AdoptNewOrderIdentity]),
        "and it is itself repairable"
    );
    assert_eq!(
        fs::read(record(&second)).unwrap(),
        second_before,
        "the other conflict participant is untouched"
    );
}

#[test]
fn every_whole_directory_relocation_carries_supporting_material_untouched() {
    // Two operations relocate an Order Directory as a whole: explicit alignment,
    // which renames it inside the Orders Directory and leaves the Order active, and
    // explicit whole-Order disposition, which takes it out of active scope. Neither
    // inspects, enumerates, or rewrites the opaque tree it carries.
    let (_tree, root) = initialized_package("m3-relocation");
    let directory = adopted_order(&root, "Legacy_drifted_name", FIRST_ID, None);
    let before = opaque_contents(&directory);
    assert_eq!(before[1], SUPPORTING_A);
    assert_eq!(before[2], SUPPORTING_B);

    // Alignment: renamed in place, still active, contents identical.
    let plan = plan_order_alignment(&directory, &RealFileSystem).expect("the Order aligns");
    let aligned = plan.destination().absolute_path().to_path_buf();
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert!(!directory.exists());
    assert_eq!(
        opaque_contents(&aligned),
        before,
        "alignment carries the opaque tree byte for byte"
    );
    assert!(
        diagnose_order(&aligned, &RealFileSystem)
            .expect("the aligned Order diagnoses")
            .is_valid(),
        "and the Order is still active and healthy at its new name"
    );

    // Disposition: moved out of active scope, contents still identical.
    let plan = plan_order_removal(
        &aligned,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed");
    let removed = plan
        .destination()
        .expect("a recoverable destination")
        .absolute_path()
        .to_path_buf();
    assert!(matches!(
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash),
        MutationReport::Committed { .. }
    ));
    assert!(!aligned.exists());
    assert_eq!(
        opaque_contents(&removed),
        before,
        "disposition carries the same opaque tree byte for byte"
    );
    // And the removed Order has left active scope.
    let index: spielorders_core::metadata::IndexDocument =
        serde_json::from_slice(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the derived index parses");
    assert!(index.orders.is_empty());
}

#[test]
fn a_full_lifecycle_composes_through_public_core_seams_only() {
    // Adopt, align, diagnose, remove — each capability handing its result to the next
    // through the public Rust core, with no adapter owning a Package rule.
    let (_tree, root) = initialized_package("m3-lifecycle");
    let directory = adopted_order(&root, "Legacy_name", FIRST_ID, None);

    let document = OrderDocument::from_json_bytes(&fs::read(record(&directory)).unwrap())
        .expect("the adopted record parses");
    assert_eq!(document.order_id.as_str(), FIRST_ID);
    assert_eq!(document.package_root.stored_text(), Some("../.."));

    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    let aligned = plan.destination().absolute_path().to_path_buf();
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    // The locator still resolves from the new location, which is what keeps the Order
    // discoverable rather than damaged.
    let diagnosis = diagnose_order(&aligned, &RealFileSystem).expect("the Order diagnoses");
    assert!(diagnosis.is_valid());
    assert!(diagnosis.cases().is_empty());
    assert!(diagnosis.unactionable().is_empty());

    let plan = plan_order_removal(
        &aligned,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .unwrap();
    let MutationReport::Committed { result, .. } =
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash)
    else {
        panic!("the removal commits");
    };
    assert_eq!(result.order().document().order_id.as_str(), FIRST_ID);
    assert_eq!(result.index().state(), PackageIndexState::Current);
}

#[test]
fn every_write_capability_composes_over_a_configured_nested_layout() {
    // A configured Package is the ordinary case, not an edge case. Each capability here
    // resolves its own managed locations from the Package it was handed, so a seam that
    // fell back to the default spelling would write outside the layout the user chose.
    let (_tree, root) = package_with_layout(
        "m3-nested-composition",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    // A directory that merely happens to be named `Orders`. Nothing configures it.
    fs::create_dir(root.join("Orders")).unwrap();

    let plan = plan_vendor_creation(
        &root,
        &VendorAssignments::new().assign("name", "Precision Optics"),
        &RealFileSystem,
    )
    .expect("a Vendor can be planned");
    let MutationReport::Committed { result, .. } =
        commit_vendor_mutation(plan, &mut QueuedUuids::of(&[VENDOR_ID]), &RealFileSystem)
    else {
        panic!("the Vendor commits");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Created);

    let plan = plan_vendor_contact_add(
        &root,
        VENDOR_ID,
        &ContactAssignments::new()
            .assign("name", "Dana Reyes")
            .assign("role", "Sales"),
        None,
        &RealFileSystem,
    )
    .expect("a Contact can be planned");
    let MutationReport::Committed { result, .. } =
        commit_vendor_contact_mutation(plan, &RealFileSystem)
    else {
        panic!("the Contact commits");
    };
    assert_eq!(result.vendor().contacts.len(), 1);

    let plan = plan_order_creation(
        &root,
        &OrderCreationInput::new("Optical table")
            .date_ordered("2026-08-22")
            .vendor_name("Precision Optics"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("an Order can be planned");
    let created = plan.directory().to_path_buf();
    assert!(
        created.starts_with(root.join(NESTED_ORDERS)),
        "creation places the Order under the configured Orders Directory: {created:?}"
    );
    assert!(matches!(
        commit_order_creation(plan, &mut QueuedUuids::of(&[FIRST_ID]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let plan = plan_order_edit(
        &created,
        &OrderAssignments::new().assign("title", "Optical bench"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be edited");
    assert!(matches!(
        commit_order_edit(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let plan = plan_order_item_add(
        &created,
        &ItemAssignments::new()
            .assign("description", "Mirror mount")
            .assign("quantity", "2")
            .assign("unit_price", "41.50"),
        None,
        &RealFileSystem,
    )
    .expect("an Item can be planned");
    let MutationReport::Committed { result, .. } =
        commit_order_item_mutation(plan, &RealFileSystem)
    else {
        panic!("the Item commits");
    };
    assert_eq!(result.order().order_total(), 83.0);

    let plan = plan_order_readme_write(&created, ORDER_README, &RealFileSystem)
        .expect("the Order README can be planned");
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let plan = plan_package_readme_write(&root, "# Purchasing\n", &RealFileSystem)
        .expect("the Package README can be planned");
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let drifted = adopted_order_at(&root, NESTED_ORDERS, "Legacy_drifted", SECOND_ID, None);
    let plan = plan_order_alignment(&drifted, &RealFileSystem).expect("the Order aligns");
    let aligned = plan.destination().absolute_path().to_path_buf();
    assert!(
        aligned.starts_with(root.join(NESTED_ORDERS)),
        "alignment stays inside the configured Orders Directory: {aligned:?}"
    );
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let carried = opaque_contents(&aligned);

    let third = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Third", THIRD_ID, None);

    let plan = plan_vendor_hide(&root, VENDOR_ID, &RealFileSystem).expect("the Vendor can hide");
    let MutationReport::Committed { result, .. } =
        commit_vendor_mutation(plan, &mut QueuedUuids::of(&[]), &RealFileSystem)
    else {
        panic!("hiding commits");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Hidden);

    let PackageLoad::Available(candidate) = open_package(&root, &RealFileSystem) else {
        panic!("the composed Package loads");
    };
    assert_eq!(
        candidate
            .context()
            .orders_directory()
            .and_then(|location| location.package_relative_path()),
        Some(NESTED_ORDERS)
    );
    assert_eq!(
        candidate
            .context()
            .removed_orders_directory()
            .and_then(|location| location.package_relative_path()),
        Some(RETIRED)
    );
    assert_eq!(candidate.healthy_orders().len(), 3);
    assert!(candidate.damaged_orders().is_empty());
    assert_eq!(candidate.index().state(), PackageIndexState::Current);
    // Hidden is not deleted: the Vendor the created Order references still resolves,
    // and is simply not offered for new use.
    let position = |directory: &Path| {
        candidate
            .healthy_orders()
            .iter()
            .position(|order| order.paths().directory().absolute_path() == directory)
            .expect("the Order is projected")
    };
    assert_eq!(
        candidate
            .vendor_for_order(position(&created))
            .map(|vendor| vendor.name.as_str()),
        Some("Precision Optics")
    );
    assert!(candidate.offered_vendors().is_empty());
    // Nothing wrote into the decoy, and no capability disturbed Supporting Material.
    assert!(fs::read_dir(root.join("Orders")).unwrap().next().is_none());
    assert_eq!(fs::read(third.join("quote.bin")).unwrap(), SUPPORTING_A);
    assert_eq!(
        fs::read(third.join("Quotes/drawing.bin")).unwrap(),
        SUPPORTING_B
    );

    // Only explicit whole-Order removal moves the tree, and it lands under the
    // configured Removed Directory carrying the same opaque bytes.
    let plan = plan_order_removal(
        &aligned,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed");
    let removed = plan
        .destination()
        .expect("a recoverable destination")
        .absolute_path()
        .to_path_buf();
    assert!(
        removed.starts_with(root.join(RETIRED)),
        "removal uses the configured Removed Directory: {removed:?}"
    );
    let MutationReport::Committed { result, .. } =
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash)
    else {
        panic!("the removal commits");
    };
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(fs::read(removed.join("quote.bin")).unwrap(), carried[1]);
    assert_eq!(
        fs::read(removed.join("Quotes/drawing.bin")).unwrap(),
        carried[2]
    );
}

#[test]
fn the_three_conventional_readme_states_stand_side_by_side_in_one_package() {
    // Absent, readable, and unreadable are three answers, not two. The derived index
    // is the pressure that would collapse them: recording `""` for text it could not
    // read turns an unknown README into an authoritative empty document, and every
    // later reader believes it. All three stand in one projection here so a rule that
    // held for one Order in isolation cannot hold by accident.
    let (_tree, root) = package_with_layout(
        "m3-readme-trichotomy",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let readable = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let absent = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Second", SECOND_ID, None);
    let unreadable = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Third", THIRD_ID, None);
    fs::remove_file(absent.join("README.md")).unwrap();
    fs::write(unreadable.join("README.md"), [0x66, 0xff, 0x0a]).unwrap();

    let PackageLoad::Available(candidate) = open_package(&root, &RealFileSystem) else {
        panic!("an unreadable README leaves the Package usable");
    };
    let readme = |directory: &Path| {
        candidate
            .healthy_orders()
            .iter()
            .find(|order| order.paths().directory().absolute_path() == directory)
            .expect("the Order is projected")
            .readme_text()
            .map(str::to_owned)
    };
    assert_eq!(
        readme(&readable).as_deref(),
        Some(std::str::from_utf8(README).unwrap()),
        "a readable README is exactly the text the user wrote"
    );
    assert_eq!(
        readme(&absent).as_deref(),
        Some(""),
        "an absent README is an empty document"
    );
    assert_eq!(
        readme(&unreadable),
        None,
        "an unreadable README is unknown rather than empty"
    );
    // None of the three costs the Package its usability, and the derived cache
    // declines rather than inventing the one answer it does not have.
    assert_eq!(candidate.healthy_orders().len(), 3);
    assert!(candidate.damaged_orders().is_empty());
    assert_eq!(
        candidate.index().refresh(),
        PackageIndexRefresh::NotAttempted,
        "the derived index declines to record text it could not read"
    );

    // Nothing needs repairing for the cache to recover: the moment the text reads, an
    // ordinary load rebuilds it.
    fs::write(unreadable.join("README.md"), README).unwrap();
    let PackageLoad::Available(candidate) = open_package(&root, &RealFileSystem) else {
        panic!("the Package loads once every README reads");
    };
    assert_eq!(candidate.index().refresh(), PackageIndexRefresh::Rebuilt);
    assert_eq!(candidate.index().state(), PackageIndexState::Current);
}

#[test]
fn a_refused_write_leaves_the_package_as_it_was_and_costs_no_other_capability() {
    // A refusal is inert. Each capability suite proves its own refusal preserves its
    // own destination; what no one of them can show is that a Package carrying four
    // refusals from four capabilities is still whole, still holds its opaque tree, and
    // still commits every one of those operations afterwards.
    let (_tree, root) = package_with_layout(
        "m3-refusals-are-inert",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let directory = adopted_order_at(&root, NESTED_ORDERS, "Legacy_drifted", FIRST_ID, None);
    let vendors_before = fs::read(vendors_record(&root)).unwrap();
    let readme_before = fs::read(directory.join("README.md")).unwrap();
    let opaque_before = opaque_contents(&directory);

    // Precommit: the Vendor aggregate is never written, and the staging file the
    // attempt made is gone.
    let plan = plan_vendor_creation(
        &root,
        &VendorAssignments::new().assign("name", "Precision Optics"),
        &RealFileSystem,
    )
    .expect("a Vendor can be planned");
    let report = commit_vendor_mutation(
        plan,
        &mut QueuedUuids::of(&[VENDOR_ID]),
        &Injected::at(Boundary::StagingWrite),
    );
    assert!(
        matches!(report, MutationReport::Rejected { .. }),
        "a failure before the canonical write is a rejection: {report:?}"
    );
    assert_eq!(fs::read(vendors_record(&root)).unwrap(), vendors_before);
    assert!(
        !root
            .join(".spielorders/.vendors.json.spielorders-stage")
            .exists()
    );

    // Changed evidence: someone else's record arrives after the plan was authorized,
    // and their work stands rather than being overwritten by a stale candidate.
    let theirs = String::from_utf8(fs::read(record(&directory)).unwrap())
        .unwrap()
        .replace("Laser components", "Someone else's title")
        .into_bytes();
    assert_ne!(theirs, fs::read(record(&directory)).unwrap());
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be edited");
    let other_writer =
        ConcurrentWriter::replacing(&record(&directory), &theirs, EditPoint::StagingWrite);
    let report = commit_order_edit(plan, &other_writer);
    assert!(other_writer.fired(), "the outside write really landed");
    assert!(
        matches!(report, MutationReport::Rejected { .. }),
        "a destination that changed under the plan is refused: {report:?}"
    );
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
        theirs,
        "the other writer's record survives"
    );

    // Cleanup failure: reported beside the refusal that asked for it, never instead
    // of it, and still without a canonical byte written.
    let plan = plan_order_readme_write(&directory, "# Replaced\n", &RealFileSystem)
        .expect("the README can be planned");
    let report = commit_readme_write(plan, &Injected::at(Boundary::StagingDurabilityAndCleanup));
    let MutationReport::Rejected {
        rejection: ReadmeWriteRejection::ReplacementFailed { error },
        ..
    } = &report
    else {
        panic!("a durability failure is a rejection: {report:?}");
    };
    assert!(
        error.cleanup_failure().is_some(),
        "the staging file it could not remove is reported too"
    );
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        readme_before
    );
    let retained = directory.join(".README.md.spielorders-stage");
    assert!(
        retained.exists(),
        "the report names a file really left behind"
    );
    fs::remove_file(&retained).unwrap();

    // Collision: alignment stops at an occupied name rather than inventing a suffix.
    let projected = plan_order_alignment(&directory, &RealFileSystem)
        .expect("the Order aligns while its name is free")
        .destination()
        .absolute_path()
        .to_path_buf();
    fs::create_dir(&projected).unwrap();
    fs::write(projected.join("sibling.txt"), b"someone else's directory\n").unwrap();
    let error = plan_order_alignment(&directory, &RealFileSystem)
        .expect_err("an occupied destination stops alignment");
    assert!(
        matches!(&error, OrderAlignmentPlanError::DestinationCollision { location } if location.absolute_path() == projected),
        "the collision names the occupied path: {error:?}"
    );
    fs::remove_dir_all(&projected).unwrap();

    // Four refusals later the Package is exactly what it was, opaque tree included.
    assert_eq!(
        opaque_contents(&directory),
        opaque_before,
        "no refusal read, moved, or rewrote Supporting Material"
    );
    let PackageLoad::Available(candidate) = open_package(&root, &RealFileSystem) else {
        panic!("a Package that only refused writes is still usable");
    };
    assert_eq!(candidate.healthy_orders().len(), 1);
    assert!(candidate.damaged_orders().is_empty());
    assert!(candidate.vendors().is_empty());

    // And each refused operation is a retry, not a dead end.
    let plan = plan_vendor_creation(
        &root,
        &VendorAssignments::new().assign("name", "Precision Optics"),
        &RealFileSystem,
    )
    .unwrap();
    assert!(matches!(
        commit_vendor_mutation(plan, &mut QueuedUuids::of(&[VENDOR_ID]), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    assert!(matches!(
        commit_order_edit(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let plan = plan_order_readme_write(&directory, "# Replaced\n", &RealFileSystem).unwrap();
    assert!(matches!(
        commit_readme_write(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let plan = plan_order_alignment(&directory, &RealFileSystem).unwrap();
    let aligned = plan.destination().absolute_path().to_path_buf();
    assert!(matches!(
        commit_order_alignment(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert_eq!(
        fs::read(aligned.join("quote.bin")).unwrap(),
        opaque_before[1]
    );
}

#[test]
fn a_derived_index_that_cannot_be_refreshed_never_makes_a_commit_look_retryable() {
    // The Package Index is a cache SpielOrders owns. A caller that reads a failed
    // refresh as a rejection reruns the operation, and every one of these run twice is
    // a second canonical change: two Orders, two Items, two Vendors. So the rule has to
    // hold for every write capability at once, which is not something any one of them
    // can establish alone. Each plans against a healthy read and commits through a
    // filesystem whose index replacement fails. Metadata repair answers the same
    // question over its own diagnosed subject, in `metadata_repair.rs`.
    let (_tree, root) = package_with_layout(
        "m3-index-refresh-failure",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let failing = || Injected::at(Boundary::IndexRefresh);

    let plan = plan_vendor_creation(
        &root,
        &VendorAssignments::new().assign("name", "Precision Optics"),
        &RealFileSystem,
    )
    .expect("a Vendor can be planned");
    let report = commit_vendor_mutation(plan, &mut QueuedUuids::of(&[VENDOR_ID]), &failing());
    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the Vendor is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [VendorMutationWarning::IndexRefreshFailed(_)]
    ));
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );

    let plan = plan_order_creation(
        &root,
        &OrderCreationInput::new("Optical table")
            .date_ordered("2026-08-22")
            .vendor_name("Precision Optics"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("an Order can be planned");
    let created = plan.directory().to_path_buf();
    let report = commit_order_creation(plan, &mut QueuedUuids::of(&[FIRST_ID]), &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the Order is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderCreationWarning::IndexRefreshFailed(_)]
    ));

    let plan = plan_order_edit(
        &created,
        &OrderAssignments::new().assign("title", "Optical bench"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be edited");
    let report = commit_order_edit(plan, &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the edit is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderEditWarning::IndexRefreshFailed(_)]
    ));

    let plan = plan_order_item_add(
        &created,
        &ItemAssignments::new()
            .assign("description", "Mirror mount")
            .assign("quantity", "2")
            .assign("unit_price", "41.50"),
        None,
        &RealFileSystem,
    )
    .expect("an Item can be planned");
    let report = commit_order_item_mutation(plan, &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the Item is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderEditWarning::IndexRefreshFailed(_)]
    ));

    let plan = plan_order_readme_write(&created, ORDER_README, &RealFileSystem)
        .expect("the README can be planned");
    let report = commit_readme_write(plan, &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the README is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [ReadmeWriteWarning::IndexRefreshFailed(_)]
    ));

    let candidate = legacy_directory(&root, NESTED_ORDERS, "Legacy_drifted");
    let plan = plan_order_adoption(&root, &candidate, &legacy_facts(None), &RealFileSystem)
        .expect("the directory can be adopted");
    let report = commit_order_adoption(plan, &mut QueuedUuids::of(&[SECOND_ID]), &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the adoption is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderAdoptionWarning::IndexRefreshFailed(_)]
    ));

    // Semantic repair is the one capability here that writes two managed records
    // together — the Order and the Package's Vendor aggregate — so a refresh failure
    // read as a rejection costs a rerun that creates a second Vendor for a reference
    // already resolved. The reference is manufactured by hand because SpielOrders can
    // no longer produce one: a Vendor is hidden, never deleted.
    let text = fs::read_to_string(record(&candidate)).unwrap().replace(
        "\"items\": []",
        &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
    );
    fs::write(record(&candidate), text).unwrap();
    let diagnosis = diagnose_order(&candidate, &RealFileSystem).expect("the Order diagnoses");
    let handle = diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == DiagnosticCode::VendorReferenceUnresolved)
        .expect("the orphaned reference is diagnosed")
        .handle()
        .to_owned();
    let plan = plan_semantic_repair(
        &candidate,
        &handle,
        &SemanticResolution::CreateAndAssignVendor {
            name: "Vacuum Systems".to_owned(),
        },
        &RealFileSystem,
    )
    .expect("the orphaned reference can be repaired");
    let report = commit_semantic_repair(plan, &mut QueuedUuids::of(&[THIRD_ID]), &failing());
    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a two-record repair is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [SemanticRepairWarning::IndexRefreshFailed(_)]
    ));
    // Both halves of the repair are on disk, so the caller that was told "warning"
    // rather than "rejected" was told the truth about every record it changed.
    assert_eq!(
        result
            .created_vendor()
            .expect("the repair created the Vendor it was asked for")
            .vendor_id
            .as_str(),
        THIRD_ID
    );
    let repaired = OrderDocument::from_json_bytes(&fs::read(record(&candidate)).unwrap())
        .expect("the repaired record parses");
    assert_eq!(
        repaired.vendor_id.as_ref().map(|id| id.as_str()),
        Some(THIRD_ID),
        "the Order half of the repair really landed"
    );
    assert!(
        String::from_utf8(fs::read(vendors_record(&root)).unwrap())
            .unwrap()
            .contains(THIRD_ID),
        "and so did the Vendor half"
    );

    let plan = plan_order_alignment(&candidate, &RealFileSystem).expect("the Order aligns");
    let aligned = plan.destination().absolute_path().to_path_buf();
    let report = commit_order_alignment(plan, &failing());
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the rename is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderAlignmentWarning::IndexRefreshFailed(_)]
    ));
    assert!(aligned.is_dir(), "the rename really happened");
    assert!(!candidate.exists());

    let plan = plan_order_removal(
        &aligned,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed");
    let report = commit_order_removal(plan, &failing(), &UnreachableTrash);
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the disposition is committed with a warning: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderRemovalWarning::IndexRefreshFailed(_)]
    ));

    // Nine canonical mutations survived nine failed cache writes, each exactly once,
    // and one ordinary load is all the cache needs to catch up.
    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("a Package whose cache kept failing is still usable");
    };
    assert_eq!(package.index().state(), PackageIndexState::Current);
    assert_eq!(package.healthy_orders().len(), 1);
    assert_eq!(
        package
            .vendors()
            .iter()
            .map(|vendor| vendor.name.as_str())
            .collect::<Vec<_>>(),
        ["Precision Optics", "Vacuum Systems"],
        "the created Vendor and the repair's Vendor each landed once"
    );
    let order = &package.healthy_orders()[0];
    assert_eq!(order.paths().directory().absolute_path(), created);
    assert_eq!(order.document().title.as_str(), "Optical bench");
    assert_eq!(
        order.document().items.len(),
        1,
        "the Item was added once, not once per retry"
    );
    assert_eq!(order.readme_text(), Some(ORDER_README));
    assert!(
        root.join(RETIRED)
            .join(aligned.file_name().unwrap())
            .is_dir()
    );
}

#[test]
fn a_rollback_that_cannot_finish_leaves_the_order_active_and_still_disposable() {
    // Whole-Order disposition is the one operation allowed to move Supporting Material,
    // so the case that matters most is the one where it fails and cannot fully undo
    // itself. The Order must stay active, its opaque tree must be untouched, and the
    // disposition must still be available — an Order stranded half-out of scope is the
    // failure this forbids.
    let (_tree, root) = package_with_layout(
        "m3-rollback-failure",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let directory = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let opaque_before = opaque_contents(&directory);
    let removed_root = root.join(RETIRED);
    assert!(
        !removed_root.exists(),
        "the Removed Directory is created by the first disposition, not by initialization"
    );

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed");
    let report = commit_order_removal(
        plan,
        &Injected::at(Boundary::DirectoryMoveAndUndo),
        &UnreachableTrash,
    );

    // Beside, not instead of: the move failed, and the caller separately learns that
    // undoing what the attempt created failed too.
    let MutationReport::Rejected {
        rejection:
            OrderRemovalRejection::MoveFailed {
                cleanup: RemovedDirectoryCleanup::Failed { location, .. },
                ..
            },
        ..
    } = &report
    else {
        panic!("both failures are reported: {report:?}");
    };
    assert_eq!(location.absolute_path(), removed_root);
    assert!(
        removed_root.is_dir(),
        "the report names a directory really still there"
    );

    // The Order never left active scope and nothing of it moved.
    assert!(directory.is_dir());
    assert_eq!(opaque_contents(&directory), opaque_before);
    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("the Package is still usable after a failed disposition");
    };
    assert_eq!(package.healthy_orders().len(), 1);
    assert!(package.damaged_orders().is_empty());

    // The same disposition succeeds once the filesystem does, and the tree it carries
    // is byte for byte the one the failed attempt left alone.
    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &RealFileSystem,
    )
    .expect("the Order can be removed again");
    let removed = plan
        .destination()
        .expect("a recoverable destination")
        .absolute_path()
        .to_path_buf();
    let MutationReport::Committed { result, .. } =
        commit_order_removal(plan, &RealFileSystem, &UnreachableTrash)
    else {
        panic!("the retried disposition commits");
    };
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(opaque_contents(&removed), opaque_before);
}

#[test]
fn an_interrupted_upgrade_resumes_and_hands_the_package_back_usable() {
    // The marker is what gates ordinary use, so an upgrade that stops partway leaves
    // the whole Package unusable until a later run finishes it. What matters across
    // tickets is that the second run picks up exactly where the first stopped and that
    // every other capability works again the moment the marker is current.
    let basenames = ["2026_08_22_First", "2026_08_22_Second"];
    let (_tree, root) = package_with_layout(
        "m3-upgrade-resume",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let first = adopted_order_at(&root, NESTED_ORDERS, basenames[0], FIRST_ID, None);
    let second = adopted_order_at(&root, NESTED_ORDERS, basenames[1], SECOND_ID, None);
    let opaque_before = opaque_contents(&second);
    regress_to_zero(&root, NESTED_ORDERS, &basenames);

    let edit = OrderAssignments::new().assign("title", "Laser mounts");
    let refusal = plan_order_edit(&first, &edit, &FixedCalendar, &RealFileSystem)
        .expect_err("an ordinary edit refuses while the Package is mid-upgrade");
    assert!(
        matches!(
            refusal,
            OrderEditPlanError::UpgradePending | OrderEditPlanError::DamagedOrder { .. }
        ),
        "the edit is blocked rather than acting on records this build cannot read: {refusal:?}"
    );

    let refusing = StampVersion::refusing(0, SECOND_ID);
    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered().with(&refusing),
        &RealFileSystem,
    )
    .expect("the configured Package plans an upgrade");
    assert!(!plan.is_complete());
    let report = commit_metadata_upgrade(plan, &RealFileSystem);
    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a run that made durable progress is not a rejection: {report:?}");
    };
    let [MetadataUpgradeWarning::PausedAtRefusedTransformation { path }] = warnings.as_slice()
    else {
        panic!("the run names where it stopped: {warnings:?}");
    };
    assert_eq!(path, &record(&second));
    assert!(!result.marker_advanced());
    assert_eq!(artifact_version(&record(&first)), Some(1));
    assert_eq!(artifact_version(&record(&second)), Some(0));
    assert_eq!(marker_version(&root), 0);

    // The resumed run reclassifies from the bytes on disk rather than from the first
    // run's report, so finished work is skipped and unfinished work is carried.
    let step = StampVersion::from(0);
    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered().with(&step),
        &RealFileSystem,
    )
    .expect("the interrupted Package replans");
    assert_eq!(
        plan.artifacts()
            .iter()
            .map(|artifact| (artifact.path().to_path_buf(), artifact.state()))
            .collect::<Vec<_>>(),
        [
            (record(&first), UpgradeArtifactState::AlreadyUpgraded),
            (record(&second), UpgradeArtifactState::Pending),
            (vendors_record(&root), UpgradeArtifactState::Pending),
        ]
    );
    let report = commit_metadata_upgrade(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the resumed run completes: {report:?}");
    };
    assert_eq!(
        result
            .skipped()
            .iter()
            .map(|location| location.absolute_path().to_path_buf())
            .collect::<Vec<_>>(),
        [record(&first)]
    );
    assert_eq!(
        result
            .upgraded()
            .iter()
            .map(|location| location.absolute_path().to_path_buf())
            .collect::<Vec<_>>(),
        [record(&second), vendors_record(&root)],
        "nothing already finished is rewritten a second time"
    );
    assert!(result.marker_advanced());
    assert_eq!(marker_version(&root), 1);
    assert_eq!(
        opaque_contents(&second),
        opaque_before,
        "the README and Supporting Material are outside the upgrade's authority"
    );

    let plan = plan_order_edit(&first, &edit, &FixedCalendar, &RealFileSystem)
        .expect("ordinary use resumes once the marker is current");
    let MutationReport::Committed { result, .. } = commit_order_edit(plan, &RealFileSystem) else {
        panic!("the previously gated edit commits");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(result.index().state(), PackageIndexState::Current);
}

#[test]
fn a_record_the_build_cannot_read_is_repaired_rather_than_counted_as_unfinished_upgrade() {
    // A Package already at the newest version can still hold a record this build cannot
    // interpret. Counting that as outstanding upgrade work would deadlock a caller
    // driving on completion, because the upgrade has nothing to carry and would never
    // report itself done. It is a Damaged Order, and integrity repair is what answers it.
    let (_tree, root) = package_with_layout(
        "m3-current-damaged",
        &InitializationOptions::new(NESTED_ORDERS, RETIRED),
    );
    let healthy = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_First", FIRST_ID, None);
    let damaged = adopted_order_at(&root, NESTED_ORDERS, "2026_08_22_Second", SECOND_ID, None);
    let healthy_before = fs::read(record(&healthy)).unwrap();
    fs::write(record(&damaged), br#"{"schema_version": 1, "nope": true}"#).unwrap();

    let step = StampVersion::from(0);
    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered().with(&step),
        &RealFileSystem,
    )
    .expect("a current Package still plans");
    assert!(plan.is_complete(), "the marker is already at the target");
    assert_eq!(
        plan.blockers()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>(),
        [record(&damaged)],
        "the record is named, as a blocker rather than as upgrade work"
    );
    assert!(
        plan.operations().is_empty(),
        "a run with nothing to carry advertises no writes"
    );
    let report = commit_metadata_upgrade(plan, &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Unchanged { .. }),
        "and the commit agrees there is nothing to do: {report:?}"
    );

    let diagnosis = diagnose_order(&damaged, &RealFileSystem).expect("the Order diagnoses");
    let case = diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == DiagnosticCode::ManagedArtifactInvalidShape)
        .expect("the unreadable record is diagnosed");
    assert_eq!(case.actions(), [RepairAction::ReplaceManagedRecord]);
    let plan = plan_metadata_repair(
        &damaged,
        &RepairConfirmation::new(case.handle(), RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(nested_replacement_record(SECOND_ID)),
        &RealFileSystem,
    )
    .expect("a confirmed replacement is planned");
    assert!(matches!(
        commit_metadata_repair(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    let PackageLoad::Available(candidate) = open_package(&root, &RealFileSystem) else {
        panic!("the repaired Package loads");
    };
    assert_eq!(candidate.healthy_orders().len(), 2);
    assert!(candidate.damaged_orders().is_empty());
    assert_eq!(candidate.index().state(), PackageIndexState::Current);
    assert_eq!(
        fs::read(record(&healthy)).unwrap(),
        healthy_before,
        "the Order that was never damaged is untouched"
    );
}
