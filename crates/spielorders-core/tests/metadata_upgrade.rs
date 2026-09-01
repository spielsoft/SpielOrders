//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_edits::{OrderAssignments, plan_order_edit};
use spielorders_core::ports::{LocalCalendar, LocalDate, UuidGenerator};
use spielorders_core::readme_writes::plan_package_readme_write;
use spielorders_core::upgrade::{
    MetadataTransformation, MetadataUpgradeError, MetadataUpgradeOperationKind,
    MetadataUpgradePlan, MetadataUpgradeRejection, MetadataUpgradeWarning, TransformationRefused,
    TransformationRegistry, UpgradeArtifact, UpgradeArtifactFailure, UpgradeArtifactState,
    commit_metadata_upgrade, plan_metadata_upgrade,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
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

/// A bounded transformation double. It exists only to drive the orchestration: it
/// stamps a `schema_version` and records what it was asked to convert. It publishes
/// no durable schema and makes no version part of the product contract.
struct VersionStamp {
    source: u64,
    seen: RefCell<Vec<UpgradeArtifact>>,
    refuse_containing: Option<String>,
}

impl VersionStamp {
    fn from(source: u64) -> Self {
        Self {
            source,
            seen: RefCell::new(Vec::new()),
            refuse_containing: None,
        }
    }

    fn refusing(source: u64, marker: &str) -> Self {
        Self {
            refuse_containing: Some(marker.to_owned()),
            ..Self::from(source)
        }
    }

    /// Every artifact this step was actually asked to convert, in order.
    fn applied(&self) -> Vec<UpgradeArtifact> {
        self.seen.borrow().clone()
    }
}

impl MetadataTransformation for VersionStamp {
    fn source_version(&self) -> u64 {
        self.source
    }

    fn transform(
        &self,
        artifact: UpgradeArtifact,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransformationRefused> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| TransformationRefused)?;
        if let Some(marker) = &self.refuse_containing
            && text.contains(marker)
        {
            return Err(TransformationRefused);
        }
        self.seen.borrow_mut().push(artifact);
        Ok(text
            .replace(
                &format!("\"schema_version\": {}", self.source),
                &format!("\"schema_version\": {}", self.source + 1),
            )
            .into_bytes())
    }
}

fn package_with_orders(label: &str, basenames: &[&str]) -> (TemporaryTree, PathBuf) {
    package_with_orders_at(label, "Orders", basenames)
}

/// The same fixture at a configured Orders Directory. `orders_path` is persisted by
/// initialization and every capability resolves it, so a Package that keeps its
/// Orders somewhere other than `Orders/` is an ordinary Package, not an edge case.
fn package_with_orders_at(
    label: &str,
    orders_path: &str,
    basenames: &[&str],
) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let options = InitializationOptions::new(orders_path, "Removed");
    let plan = plan_package_initialization(tree.path(), &options, &RealFileSystem)
        .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = fs::canonicalize(tree.path()).unwrap();
    for (index, basename) in basenames.iter().enumerate() {
        let directory = root.join(orders_path).join(basename);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("README.md"), README).unwrap();
        fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
        let plan = plan_order_adoption(
            &root,
            &directory,
            &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
            &RealFileSystem,
        )
        .expect("the fixture directory can be adopted");
        let uuid = format!("8b6e3f0a-8ec5-4d47-95e1-41265{:07}", index);
        assert!(matches!(
            commit_order_adoption(plan, &mut FixedUuid(uuid), &RealFileSystem),
            MutationReport::Committed { .. }
        ));
    }
    (tree, root)
}

fn marker_version(root: &Path) -> u64 {
    serde_json::from_slice::<serde_json::Value>(
        &fs::read(root.join(".spielorders/version.json")).unwrap(),
    )
    .unwrap()["metadata_version"]
        .as_u64()
        .unwrap()
}

fn artifact_version(path: &Path) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(&fs::read(path).ok()?)
        .ok()?
        .get("schema_version")?
        .as_u64()
}

fn order_record(root: &Path, basename: &str) -> PathBuf {
    order_record_at(root, "Orders", basename)
}

fn order_record_at(root: &Path, orders_path: &str, basename: &str) -> PathBuf {
    root.join(orders_path)
        .join(basename)
        .join(".spielorders/order.json")
}

fn vendors_record(root: &Path) -> PathBuf {
    root.join(".spielorders/vendors.json")
}

#[test]
fn a_package_at_the_newest_version_is_unchanged_by_the_registered_chain() {
    // The running build knows one durable version, so its own registered chain has
    // nothing to do. This is the production path.
    let (_tree, root) = package_with_orders("upgrade-current", &["2026_08_22_A"]);
    let before = fs::read(order_record(&root, "2026_08_22_A")).unwrap();

    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered(),
        &RealFileSystem,
    )
    .expect("a current Package plans an upgrade");

    assert_eq!(plan.source_version(), 1);
    assert_eq!(plan.target_version(), 1);
    assert!(plan.is_complete());
    assert!(plan.blockers().is_empty());
    assert!(
        plan.operations().is_empty(),
        "a Package with nothing to carry anywhere advertises no write: {:?}",
        plan.operations()
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("an already current Package is a no-op: {report:?}");
    };
    assert_eq!(result.reached_version(), 1);
    assert!(!result.marker_advanced());
    assert_eq!(marker_version(&root), 1);
    assert_eq!(
        fs::read(order_record(&root, "2026_08_22_A")).unwrap(),
        before
    );
}

/// A Package whose marker already declares the newest version has no upgrade to run,
/// whatever else is wrong inside it. A record this build cannot read there is a Damaged
/// Order, which integrity repair answers — an upgrade has no chain to run on it and
/// says so. Calling it outstanding upgrade work would leave a caller driving on
/// `is_complete()` waiting forever for a run that reports nothing to do.
#[test]
fn a_current_package_holding_an_unreadable_record_still_has_no_upgrade_to_run() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-current-damaged", &basenames);
    // Well-formed JSON at the current version that is not a record at that version.
    let record = order_record(&root, "2026_08_22_A");
    let text = fs::read_to_string(&record)
        .unwrap()
        .replace("\"title\": \"Laser components\"", "\"title\": \"   \"");
    fs::write(&record, &text).unwrap();

    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered(),
        &RealFileSystem,
    )
    .expect("a current Package plans an upgrade");

    assert_eq!(plan.source_version(), plan.target_version());
    assert!(
        plan.is_complete(),
        "the marker already declares the newest version"
    );
    assert!(
        plan.operations().is_empty(),
        "so there is no write to enumerate: {:?}",
        plan.operations()
    );
    // Naming what it could not read is still honest; it is simply not upgrade work.
    assert_eq!(
        plan.blockers()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&record)
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    assert!(
        matches!(report, MutationReport::Unchanged { .. }),
        "the plan and the commit agree that there is nothing to do: {report:?}"
    );
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        text,
        "and the damaged record is left for repair, untouched"
    );
    assert_eq!(marker_version(&root), 1);
}

/// Puts the fixture Package at a source version below the build's, so a bounded
/// transformation double has something to convert.
fn regress_to(root: &Path, version: u64, basenames: &[&str]) {
    regress_to_at(root, "Orders", version, basenames)
}

fn regress_to_at(root: &Path, orders_path: &str, version: u64, basenames: &[&str]) {
    fs::write(
        root.join(".spielorders/version.json"),
        format!("{{\n  \"metadata_version\": {version}\n}}\n"),
    )
    .unwrap();
    for basename in basenames {
        let path = order_record_at(root, orders_path, basename);
        let text = fs::read_to_string(&path).unwrap().replace(
            "\"schema_version\": 1",
            &format!("\"schema_version\": {version}"),
        );
        fs::write(path, text).unwrap();
    }
    let text = fs::read_to_string(vendors_record(root)).unwrap().replace(
        "\"schema_version\": 1",
        &format!("\"schema_version\": {version}"),
    );
    fs::write(vendors_record(root), text).unwrap();
}

#[test]
fn the_upgrade_visits_orders_in_order_then_vendors_and_advances_the_marker_last() {
    let basenames = ["2026_08_22_A", "2026_08_22_B", "2026_08_22_C"];
    let (_tree, root) = package_with_orders("upgrade-order", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an older Package plans an upgrade");

    assert_eq!(plan.source_version(), 0);
    assert_eq!(plan.target_version(), 1);
    assert!(!plan.is_complete());
    // Each Order Directory in deterministic order, then the Vendor aggregate. The
    // marker is not an artifact in this list: it advances only after all of them.
    assert_eq!(
        plan.artifacts()
            .iter()
            .map(|artifact| (
                artifact.artifact(),
                artifact
                    .path()
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                artifact.state()
            ))
            .collect::<Vec<_>>(),
        [
            (
                UpgradeArtifact::OrderRecord,
                "Orders/2026_08_22_A/.spielorders/order.json".to_owned(),
                UpgradeArtifactState::Pending
            ),
            (
                UpgradeArtifact::OrderRecord,
                "Orders/2026_08_22_B/.spielorders/order.json".to_owned(),
                UpgradeArtifactState::Pending
            ),
            (
                UpgradeArtifact::OrderRecord,
                "Orders/2026_08_22_C/.spielorders/order.json".to_owned(),
                UpgradeArtifactState::Pending
            ),
            (
                UpgradeArtifact::VendorAggregate,
                ".spielorders/vendors.json".to_owned(),
                UpgradeArtifactState::Pending
            ),
        ]
    );
    // A dry run advances nothing.
    assert_eq!(marker_version(&root), 0);
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(0)
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a complete chain commits: {report:?}");
    };
    // The upgraded list is the order the run actually visited them in.
    assert_eq!(
        result
            .upgraded()
            .iter()
            .map(|path| path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string())
            .collect::<Vec<_>>(),
        [
            "Orders/2026_08_22_A/.spielorders/order.json",
            "Orders/2026_08_22_B/.spielorders/order.json",
            "Orders/2026_08_22_C/.spielorders/order.json",
            ".spielorders/vendors.json",
        ]
    );
    assert!(result.marker_advanced());
    assert_eq!(result.reached_version(), 1);
    assert_eq!(marker_version(&root), 1);
    for basename in basenames {
        assert_eq!(artifact_version(&order_record(&root, basename)), Some(1));
    }
    assert_eq!(artifact_version(&vendors_record(&root)), Some(1));
    assert!(
        result.index().is_some(),
        "the derived index is rebuilt last"
    );
    // The upgrade reads and rewrites no README or Supporting Material.
    for basename in basenames {
        let directory = root.join("Orders").join(basename);
        assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
        assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
    }
}

/// A dry run that under-reports its own effects is not a dry run. Advancing the
/// Package marker and rebuilding the derived index are the two package-wide writes an
/// upgrade performs, and a caller inspecting the plan has to be able to see them
/// alongside the record replacements rather than discover them afterwards.
#[test]
fn the_plan_enumerates_the_marker_advance_and_the_index_rebuild() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-operations", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an older Package plans an upgrade");

    assert_eq!(
        operations(&plan, &root),
        [
            (
                MetadataUpgradeOperationKind::ReplaceArtifact,
                "Orders/2026_08_22_A/.spielorders/order.json".to_owned()
            ),
            (
                MetadataUpgradeOperationKind::ReplaceArtifact,
                "Orders/2026_08_22_B/.spielorders/order.json".to_owned()
            ),
            (
                MetadataUpgradeOperationKind::ReplaceArtifact,
                ".spielorders/vendors.json".to_owned()
            ),
            (
                MetadataUpgradeOperationKind::AdvanceMarker,
                ".spielorders/version.json".to_owned()
            ),
            (
                MetadataUpgradeOperationKind::RefreshPackageIndex,
                ".spielorders/index.json".to_owned()
            ),
        ]
    );

    assert!(matches!(
        commit_metadata_upgrade(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    // An artifact already at the target is not work, so it is not an operation. The
    // marker and the index remain, which is what a resumed run still has to do.
    regress_marker_only(&root, 0);
    let resumed = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("a Package whose records are done can be replanned");
    assert_eq!(
        operations(&resumed, &root),
        [
            (
                MetadataUpgradeOperationKind::AdvanceMarker,
                ".spielorders/version.json".to_owned()
            ),
            (
                MetadataUpgradeOperationKind::RefreshPackageIndex,
                ".spielorders/index.json".to_owned()
            ),
        ]
    );
}

fn operations(
    plan: &MetadataUpgradePlan<'_>,
    root: &Path,
) -> Vec<(MetadataUpgradeOperationKind, String)> {
    plan.operations()
        .iter()
        .map(|operation| {
            (
                operation.kind(),
                operation
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            )
        })
        .collect()
}

/// Moves only the Package marker back, leaving every record at the target. This is
/// the state an interrupted run leaves behind once its records are finished.
fn regress_marker_only(root: &Path, version: u64) {
    fs::write(
        root.join(".spielorders/version.json"),
        format!("{{\n  \"metadata_version\": {version}\n}}\n"),
    )
    .unwrap();
}

/// A nested `orders_path` is an ordinary configured Package, not an edge case. A run
/// that resolved only its first segment, or fell back to `Orders`, would enumerate the
/// wrong directory, find nothing pending, and advance the marker over every record it
/// never migrated. Supporting Material and Order READMEs sit in the same directories
/// the run walks and are outside its authority entirely.
#[test]
fn a_nested_orders_directory_is_upgraded_and_its_supporting_material_left_alone() {
    const ORDERS: &str = "Admin/Purchase Orders";
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders_at("upgrade-nested-orders", ORDERS, &basenames);
    regress_to_at(&root, ORDERS, 0, &basenames);
    // A directory that merely happens to be named `Orders`. Nothing points at it.
    fs::create_dir_all(root.join("Orders")).unwrap();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("a nested Package plans an upgrade");

    assert_eq!(
        plan.artifacts()
            .iter()
            .map(|artifact| artifact
                .path()
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string())
            .collect::<Vec<_>>(),
        [
            "Admin/Purchase Orders/2026_08_22_A/.spielorders/order.json",
            "Admin/Purchase Orders/2026_08_22_B/.spielorders/order.json",
            ".spielorders/vendors.json",
        ]
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a nested Package completes: {report:?}");
    };
    assert_eq!(result.upgraded().len(), 3, "{:?}", result.upgraded());
    assert_eq!(marker_version(&root), 1);
    for basename in basenames {
        assert_eq!(
            artifact_version(&order_record_at(&root, ORDERS, basename)),
            Some(1),
            "{basename} was actually upgraded, not merely declared upgraded"
        );
        // The upgrade owns the reserved record and nothing else in the directory.
        let directory = root.join(ORDERS).join(basename);
        assert_eq!(fs::read(directory.join("README.md")).unwrap(), README);
        assert_eq!(fs::read(directory.join("quote.bin")).unwrap(), SUPPORTING_A);
    }
}

/// The plan read the Package marker to learn which version everything else is being
/// carried from. Advancing it unconditionally at the end would discard a marker
/// somebody else had since written and replace it with one derived from bytes that are
/// no longer there — and the marker is the single fact that says whether this Package
/// may be used at all.
#[test]
fn a_marker_that_changed_mid_upgrade_is_refused_rather_than_overwritten() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-concurrent-marker", &basenames);
    regress_to(&root, 0, &basenames);
    let marker = root.join(".spielorders/version.json");
    let theirs = b"{\n  \"metadata_version\": 5\n}\n".to_vec();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let outsider = ConcurrentWriter::replacing(&marker, &theirs, EditPoint::StagingWrite);
    let report = commit_metadata_upgrade(plan, &outsider);

    assert!(outsider.fired(), "the outside write really landed");
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the records were rewritten, so this is not a rejection: {report:?}");
    };
    let [MetadataUpgradeWarning::MarkerAdvanceFailed { error, .. }] = warnings.as_slice() else {
        panic!("the marker is what could not advance: {warnings:?}");
    };
    assert!(
        matches!(
            error.failure(),
            spielorders_core::filesystem::ValidatedReplacementFailure::DestinationChanged
        ),
        "refused because the marker changed, not because the write failed: {:?}",
        error.failure()
    );
    assert_eq!(
        fs::read(&marker).unwrap(),
        theirs,
        "the other writer's marker survives"
    );

    // The guard is on what the marker means, not on how it was spelled. A managed JSON
    // record is canonical by parsed meaning, so a marker still declaring the source
    // version is the marker this run observed however it was reformatted.
    let (_tree, root) = package_with_orders("upgrade-reformatted-marker", &basenames);
    regress_to(&root, 0, &basenames);
    let marker = root.join(".spielorders/version.json");
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let outsider = ConcurrentWriter::replacing(
        &marker,
        b"{\"metadata_version\":0}",
        EditPoint::StagingWrite,
    );
    let report = commit_metadata_upgrade(plan, &outsider);

    assert!(outsider.fired());
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "a reformatted marker still declaring the source version does not refuse: {report:?}"
    );
    assert_eq!(marker_version(&root), 1);
}

/// A transformation double that edits the record without ever moving it off the
/// version it came in at.
struct UnstampedEdit {
    source: u64,
    find: &'static str,
    replace: &'static str,
}

impl MetadataTransformation for UnstampedEdit {
    fn source_version(&self) -> u64 {
        self.source
    }

    fn transform(
        &self,
        _artifact: UpgradeArtifact,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransformationRefused> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| TransformationRefused)?;
        Ok(text.replace(self.find, self.replace).into_bytes())
    }
}

/// Two registered steps reading the same version are two different answers to one
/// question. Taking whichever was registered first would migrate the Package with a
/// step nobody chose, and the one that lost would leave no trace of having been
/// registered at all, so an ambiguous chain is diagnosed before anything is read.
#[test]
fn two_steps_reading_the_same_version_are_refused_rather_than_silently_resolved() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-repeated-step", &basenames);
    regress_to(&root, 0, &basenames);
    let first = VersionStamp::from(0);
    let second = VersionStamp::from(0);

    let error = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered()
            .with(&first)
            .with(&second),
        &RealFileSystem,
    )
    .expect_err("an ambiguous chain is not run");

    assert!(
        matches!(error, MetadataUpgradeError::IncoherentChain { version: 0 }),
        "got {error:?}"
    );
    assert_eq!(marker_version(&root), 0);
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(0)
    );
}

/// Without a schema for the version a step is producing, the version that step's
/// output declares is the whole of what can be checked — and it is what separates a
/// step that migrated from one that quietly did nothing, downgraded, or skipped ahead.
/// Carrying such an output forward would end with the marker advancing over an
/// artifact still at the version it started from.
#[test]
fn a_step_whose_output_does_not_declare_the_next_version_pauses_the_run() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-unstamped-output", &basenames);
    regress_to(&root, 0, &basenames);
    let before = fs::read(order_record(&root, "2026_08_22_A")).unwrap();
    let step = UnstampedEdit {
        source: 0,
        find: "Laser components",
        replace: "Laser parts",
    };
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("an output still at the old version pauses the run: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtUnusableOutput {
                failure: UpgradeArtifactFailure::WrongDeclaredVersion,
                ..
            }]
        ),
        "{warnings:?}"
    );
    assert_eq!(
        fs::read(order_record(&root, "2026_08_22_A")).unwrap(),
        before,
        "the record is left exactly as it was"
    );
    assert_eq!(marker_version(&root), 0);
}

/// A transformation double that stamps the version and then edits the record it
/// produced. It exists to drive outputs that reach the target version's number without
/// being the artifact that version accepts.
struct StampThenEdit {
    source: u64,
    find: &'static str,
    replace: &'static str,
}

impl MetadataTransformation for StampThenEdit {
    fn source_version(&self) -> u64 {
        self.source
    }

    fn transform(
        &self,
        _artifact: UpgradeArtifact,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransformationRefused> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| TransformationRefused)?;
        Ok(text
            .replace(
                &format!("\"schema_version\": {}", self.source),
                &format!("\"schema_version\": {}", self.source + 1),
            )
            .replace(self.find, self.replace)
            .into_bytes())
    }
}

/// Version 1 is the one durable contract this repository publishes a schema for, so it
/// is the one an upgrade can check what it produced against. A step that stamps the
/// target version onto something that is not a record at that version would otherwise
/// have its output written and the marker advanced over it, and the Package would
/// declare a version whose own rules its records break.
#[test]
fn an_output_that_is_not_a_record_at_the_target_version_pauses_the_run() {
    // Not parseable as an Order at version 1: a blank required Title is not a Title.
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-output-unparsable", &basenames);
    regress_to(&root, 0, &basenames);
    let before = fs::read(order_record(&root, "2026_08_22_A")).unwrap();
    let step = StampThenEdit {
        source: 0,
        find: "\"title\": \"Laser components\"",
        replace: "\"title\": \"   \"",
    };
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("an unusable output pauses the run: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtUnusableOutput {
                path,
                failure: UpgradeArtifactFailure::NotTheDeclaredSchema,
            }] if path == &order_record(&root, "2026_08_22_A")
        ),
        "{warnings:?}"
    );
    assert_eq!(
        fs::read(order_record(&root, "2026_08_22_A")).unwrap(),
        before,
        "the record this build would not write is left exactly as it was"
    );
    assert_eq!(marker_version(&root), 0, "and the marker did not advance");

    // Parseable but not valid at version 1: February has no thirty-first day.
    let (_tree, root) = package_with_orders("upgrade-output-invalid", &basenames);
    regress_to(&root, 0, &basenames);
    let step = StampThenEdit {
        source: 0,
        find: "\"date_ordered\": \"2026-08-22\"",
        replace: "\"date_ordered\": \"2026-02-31\"",
    };
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("an invalid output pauses the run: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtUnusableOutput {
                failure: UpgradeArtifactFailure::InvalidAtDeclaredVersion,
                ..
            }]
        ),
        "{warnings:?}"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(0)
    );
    assert_eq!(marker_version(&root), 0);
}

/// The same schema is the standard for what this run reads, not only for what it
/// produces. An artifact declaring the target version while breaking that version's
/// rules is not finished work, and skipping it as finished would let the marker advance
/// over a record the very version it declares would reject.
#[test]
fn an_artifact_declaring_the_target_version_but_breaking_it_blocks_the_run() {
    // An Order record that claims to be finished and is not a record at version 1.
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-source-shape", &basenames);
    regress_to(&root, 0, &basenames);
    let record = order_record(&root, "2026_08_22_A");
    let text = fs::read_to_string(&record)
        .unwrap()
        .replace("\"schema_version\": 0", "\"schema_version\": 1")
        .replace("\"title\": \"Laser components\"", "\"title\": \"   \"");
    fs::write(&record, &text).unwrap();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    assert_eq!(
        plan.blockers()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&record),
        "the dry run names it before anything runs"
    );
    let report = commit_metadata_upgrade(plan, &RealFileSystem);
    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the run pauses rather than skipping it: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtBlockedArtifact { path }] if path == &record
        ),
        "{warnings:?}"
    );
    assert!(
        result.skipped().is_empty(),
        "it was not treated as finished work: {:?}",
        result.skipped()
    );
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        text,
        "and it is left exactly as it was"
    );
    assert_eq!(marker_version(&root), 0);

    // The Vendor aggregate is held to the Package's own rules at that version, not
    // only to its shape: two Vendors cannot share one Vendor ID.
    let (_tree, root) = package_with_orders("upgrade-source-semantics", &basenames);
    regress_to(&root, 0, &basenames);
    fs::write(
        vendors_record(&root),
        br#"{
  "schema_version": 1,
  "vendors": [
    { "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822", "name": "Tarnwick", "contacts": [] },
    { "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822", "name": "Nethermoor", "contacts": [] }
  ]
}
"#,
    )
    .unwrap();

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    assert_eq!(
        plan.blockers()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>(),
        [vendors_record(&root)]
    );
    let report = commit_metadata_upgrade(plan, &RealFileSystem);
    let MutationReport::CommittedWithWarning { warnings, .. } = report else {
        panic!("the run pauses at the Vendor aggregate: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtBlockedArtifact { path }]
                if path == &vendors_record(&root)
        ),
        "{warnings:?}"
    );
    // The Orders ahead of it are upgraded, and the marker is not.
    for basename in basenames {
        assert_eq!(artifact_version(&order_record(&root, basename)), Some(1));
    }
    assert_eq!(marker_version(&root), 0);
}

/// A plan's classification is evidence, read before the run started. This record was
/// pending then and had reached the target by the time the commit got to it — another
/// resume, or the user's own second run. Acting on the stale classification would hand
/// a version 0 to 1 migration a record that is already at version 1, applying it twice,
/// which is the one thing a resumable upgrade exists to avoid.
#[test]
fn a_pending_artifact_that_reached_the_target_first_is_not_transformed_twice() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-reclassify-pending", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert_eq!(plan.artifacts()[0].state(), UpgradeArtifactState::Pending);

    // Someone else finishes this record between the plan and the commit.
    let record = order_record(&root, "2026_08_22_A");
    let theirs = fs::read_to_string(&record)
        .unwrap()
        .replace("\"schema_version\": 0", "\"schema_version\": 1")
        .into_bytes();
    fs::write(&record, &theirs).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("the rest of the Package still completes: {report:?}");
    };
    assert_eq!(
        result.skipped(),
        std::slice::from_ref(&record),
        "the finished record is skipped, not migrated a second time"
    );
    assert!(!result.upgraded().contains(&record));
    assert_eq!(
        fs::read(&record).unwrap(),
        theirs,
        "and their bytes survive exactly"
    );
    assert_eq!(
        step.applied(),
        [
            UpgradeArtifact::OrderRecord,
            UpgradeArtifact::VendorAggregate
        ],
        "only the artifacts still at the source version were converted"
    );
    assert_eq!(marker_version(&root), 1);
}

/// The mirror case. The plan saw this record already at the target, and by the time
/// the commit reached it the record was back at the source version — restored from a
/// backup, or reverted by hand. Skipping it on the strength of the stale plan would
/// advance the marker over a record that never reached the target, leaving the Package
/// declaring a version one of its records does not have.
#[test]
fn an_already_upgraded_artifact_that_went_back_is_not_silently_skipped() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-reclassify-finished", &basenames);
    regress_to(&root, 0, &basenames);
    let record = order_record(&root, "2026_08_22_A");
    let source = fs::read(&record).unwrap();
    fs::write(
        &record,
        String::from_utf8(source.clone())
            .unwrap()
            .replace("\"schema_version\": 0", "\"schema_version\": 1"),
    )
    .unwrap();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert_eq!(
        plan.artifacts()[0].state(),
        UpgradeArtifactState::AlreadyUpgraded
    );

    // The record goes back to the source version between the plan and the commit.
    fs::write(&record, &source).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("the run completes by doing the work again: {report:?}");
    };
    assert!(
        result.upgraded().contains(&record),
        "the record that went back is upgraded rather than skipped: {:?}",
        result.upgraded()
    );
    assert_eq!(
        artifact_version(&record),
        Some(1),
        "it really reached the version the marker is about to claim"
    );
    assert_eq!(marker_version(&root), 1);
}

/// The chain is chosen when the plan is made. That chain decided the target version,
/// the classification of every artifact, and the version the marker will be advanced
/// to. A commit driven by some other chain would rewrite records with steps this plan
/// never assessed and then advance the marker to a version it never verified, so
/// committing takes no registry at all: there is no second chain to disagree with.
#[test]
fn the_chain_bound_when_planning_is_the_chain_that_runs() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-bound-chain", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);

    let plan = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered().with(&step),
        &RealFileSystem,
    )
    .expect("an older Package plans an upgrade");

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { .. } = report else {
        panic!("the bound chain commits: {report:?}");
    };
    assert_eq!(
        step.applied(),
        [
            UpgradeArtifact::OrderRecord,
            UpgradeArtifact::VendorAggregate
        ],
        "every artifact was converted by the step the plan bound"
    );
    assert_eq!(marker_version(&root), 1);
}

/// Fails the atomic replacement of one named artifact, so a run stops partway with
/// everything before it already durably upgraded.
struct FailAt(&'static str);

impl spielorders_core::ports::FileSystem for FailAt {
    type Error = std::io::Error;

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
        if destination.to_string_lossy().contains(self.0) {
            return Err(std::io::Error::other("injected interruption"));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(
        &self,
        path: &Path,
    ) -> Result<Option<spielorders_core::ports::FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(
        &self,
        path: &Path,
    ) -> Result<spielorders_core::ports::FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(
        &self,
        path: &Path,
    ) -> Result<Vec<spielorders_core::ports::DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn an_interruption_leaves_finished_artifacts_upgraded_and_the_marker_unadvanced() {
    let basenames = ["2026_08_22_A", "2026_08_22_B", "2026_08_22_C"];
    let (_tree, root) = package_with_orders("upgrade-interrupted", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    // The run stops while replacing the second Order.
    let report = commit_metadata_upgrade(plan, &FailAt("2026_08_22_B"));

    // The first Order is already upgraded on disk by the time the second one fails.
    // Reporting a rejection would tell the caller nothing happened, and would discard
    // the list of what did — which is the one thing a resumable run owes them.
    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a run that already upgraded an artifact is committed: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::StoppedAtFailedArtifact { path, .. }]
                if path == &order_record(&root, "2026_08_22_B")
        ),
        "the report names the artifact it stopped at, with its error: {warnings:?}"
    );
    assert_eq!(
        result.upgraded(),
        [order_record(&root, "2026_08_22_A")],
        "and names exactly what it did upgrade"
    );
    assert!(!result.marker_advanced());
    assert_eq!(result.reached_version(), 0);

    // This is the deliberate exception to package-wide rollback: the Order that
    // finished stays upgraded rather than being restored.
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(1),
        "the finished Order keeps its new form"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_B")),
        Some(0),
        "the interrupted Order keeps its old form, atomically"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_C")),
        Some(0)
    );
    assert_eq!(artifact_version(&vendors_record(&root)), Some(0));
    assert_eq!(
        marker_version(&root),
        0,
        "the marker did not advance, so the upgrade does not look complete"
    );
}

#[test]
fn resumption_reads_current_state_and_neither_redoes_nor_skips_work() {
    let basenames = ["2026_08_22_A", "2026_08_22_B", "2026_08_22_C"];
    let (_tree, root) = package_with_orders("upgrade-resume", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert!(matches!(
        commit_metadata_upgrade(plan, &FailAt("2026_08_22_B")),
        MutationReport::CommittedWithWarning { .. }
    ));
    let finished_before = fs::read(order_record(&root, "2026_08_22_A")).unwrap();

    // Resuming replans from what is actually on disk: no stored progress marker is
    // consulted, and the mixed state is read directly.
    let resumed = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an interrupted Package can be replanned");
    assert_eq!(
        resumed
            .artifacts()
            .iter()
            .map(|artifact| artifact.state())
            .collect::<Vec<_>>(),
        [
            UpgradeArtifactState::AlreadyUpgraded,
            UpgradeArtifactState::Pending,
            UpgradeArtifactState::Pending,
            UpgradeArtifactState::Pending,
        ],
        "the finished Order is recognized as already upgraded"
    );

    let report = commit_metadata_upgrade(resumed, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a resumed upgrade completes: {report:?}");
    };
    assert_eq!(
        result
            .skipped()
            .iter()
            .map(|path| path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string())
            .collect::<Vec<_>>(),
        ["Orders/2026_08_22_A/.spielorders/order.json"],
        "finished work is skipped, not replayed"
    );
    assert_eq!(
        result
            .upgraded()
            .iter()
            .map(|path| path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string())
            .collect::<Vec<_>>(),
        [
            "Orders/2026_08_22_B/.spielorders/order.json",
            "Orders/2026_08_22_C/.spielorders/order.json",
            ".spielorders/vendors.json",
        ],
        "and no unfinished work is skipped"
    );
    assert_eq!(
        fs::read(order_record(&root, "2026_08_22_A")).unwrap(),
        finished_before,
        "the already upgraded Order was not transformed a second time"
    );
    assert!(result.marker_advanced());
    assert_eq!(marker_version(&root), 1);

    // Running again once complete is an ordinary no-op.
    let again = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert!(again.is_complete());
    assert!(matches!(
        commit_metadata_upgrade(again, &RealFileSystem),
        MutationReport::Unchanged { .. }
    ));
    assert_eq!(marker_version(&root), 1);
}

#[test]
fn a_blocked_artifact_pauses_the_run_at_its_position_without_dropping_anything() {
    let basenames = ["2026_08_22_A", "2026_08_22_B", "2026_08_22_C"];
    let (_tree, root) = package_with_orders("upgrade-blocked", &basenames);
    regress_to(&root, 0, &basenames);
    // The middle Order declares a version this build recognizes as neither the source
    // nor the target, so it is never interpreted.
    let path = order_record(&root, "2026_08_22_B");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("\"schema_version\": 0", "\"schema_version\": 47");
    fs::write(&path, &text).unwrap();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert_eq!(
        plan.blockers()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&path),
        "the dry run names the blocker before anything runs"
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a blocked artifact pauses a partially committed run: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtBlockedArtifact { path: stopped }]
                if stopped == &path
        ),
        "the report says where it stopped: {warnings:?}"
    );
    // Everything before the blocker is upgraded; nothing after it was touched, and
    // nothing was discarded or repaired.
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(1)
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        text,
        "the blocker is untouched"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_C")),
        Some(0),
        "the upgrade did not skip past the blocker"
    );
    assert_eq!(artifact_version(&vendors_record(&root)), Some(0));
    assert!(!result.marker_advanced());
    assert_eq!(
        result.reached_version(),
        0,
        "a paused run reports the version the marker still declares, not the one it was heading for"
    );
    assert_eq!(result.source_version(), 0);
    assert_eq!(marker_version(&root), 0);

    // The user resolves it, and rerunning resumes without replaying the finished one.
    fs::write(
        &path,
        text.replace("\"schema_version\": 47", "\"schema_version\": 0"),
    )
    .unwrap();
    let resumed = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    assert!(resumed.blockers().is_empty());
    let report = commit_metadata_upgrade(resumed, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the repaired Package completes: {report:?}");
    };
    assert_eq!(result.skipped().len(), 1);
    assert_eq!(marker_version(&root), 1);
}

#[test]
fn ordinary_package_use_stays_blocked_while_the_marker_is_old() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-blocks-use", &basenames);
    let directory = root.join("Orders").join("2026_08_22_A");
    // Only the marker is moved off the newest version, so the records themselves are
    // still readable. The Package is nonetheless not usable: this build does not
    // interpret a Package whose declared version it is not at.
    fs::write(
        root.join(".spielorders/version.json"),
        b"{\n  \"metadata_version\": 0\n}\n",
    )
    .unwrap();
    let record_before = fs::read(order_record(&root, "2026_08_22_A")).unwrap();

    // Two capabilities that ordinarily succeed on this Package now refuse.
    let error = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Renamed"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect_err("an Order edit refuses while the marker is not the newest version");
    assert!(
        matches!(
            error,
            spielorders_core::order_edits::OrderEditPlanError::DamagedPackage
                | spielorders_core::order_edits::OrderEditPlanError::UpgradePending
        ),
        "an ordinary mutation is blocked rather than proceeding: {error:?}"
    );

    let error = plan_package_readme_write(&root, "new prose\n", &RealFileSystem)
        .expect_err("a README write refuses while the marker is not the newest version");
    assert!(
        matches!(
            error,
            spielorders_core::readme_writes::ReadmeWritePlanError::DamagedPackage
                | spielorders_core::readme_writes::ReadmeWritePlanError::UpgradePending
        ),
        "an ordinary mutation is blocked rather than proceeding: {error:?}"
    );
    assert_eq!(
        fs::read(order_record(&root, "2026_08_22_A")).unwrap(),
        record_before,
        "a blocked mutation changed nothing"
    );

    // Restoring the marker unblocks ordinary use again, which is what completing an
    // upgrade does at its last step.
    fs::write(
        root.join(".spielorders/version.json"),
        b"{\n  \"metadata_version\": 1\n}\n",
    )
    .unwrap();
    assert!(
        plan_order_edit(
            &directory,
            &OrderAssignments::new().assign("title", "Renamed"),
            &FixedCalendar,
            &RealFileSystem,
        )
        .is_ok(),
        "ordinary use resumes once the marker declares the newest version"
    );
}

#[test]
fn only_a_complete_consecutive_chain_runs() {
    let basenames = ["2026_08_22_A"];

    // A version newer than this build knows is left untouched.
    let (_tree, root) = package_with_orders("upgrade-newer", &basenames);
    regress_to(&root, 9, &basenames);
    let error = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered(),
        &RealFileSystem,
    )
    .expect_err("a newer Package is not upgraded");
    assert!(
        matches!(error, MetadataUpgradeError::NewerThanBuild { .. }),
        "got {error:?}"
    );
    assert_eq!(marker_version(&root), 9);

    // An older version with no registered step is diagnostic-only, never partially
    // migrated.
    let (_tree, root) = package_with_orders("upgrade-no-chain", &basenames);
    regress_to(&root, 0, &basenames);
    let error = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered(),
        &RealFileSystem,
    )
    .expect_err("an older Package with no chain is not upgraded");
    assert!(
        matches!(error, MetadataUpgradeError::NoCompleteChain { .. }),
        "got {error:?}"
    );
    assert_eq!(marker_version(&root), 0);
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(0)
    );

    // A gap in the chain truncates it rather than being bridged: a step from 0 alone
    // cannot carry a Package at version -1's stand-in, and a step registered only for
    // a later version leaves the earlier one unreachable.
    let (_tree, root) = package_with_orders("upgrade-gap", &basenames);
    regress_to(&root, 0, &basenames);
    let distant = VersionStamp::from(5);
    let error = plan_metadata_upgrade(
        &root,
        &TransformationRegistry::registered().with(&distant),
        &RealFileSystem,
    )
    .expect_err("a non-consecutive step does not bridge the gap");
    assert!(
        matches!(error, MetadataUpgradeError::NoCompleteChain { .. }),
        "got {error:?}"
    );
    assert_eq!(marker_version(&root), 0);
}

#[test]
fn a_refused_transformation_pauses_the_run_like_a_blocked_artifact() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-refused", &basenames);
    regress_to(&root, 0, &basenames);
    // The double refuses the second Order by content rather than by version.
    let text = fs::read_to_string(order_record(&root, "2026_08_22_B"))
        .unwrap()
        .replace("Laser components", "Refuse me");
    fs::write(order_record(&root, "2026_08_22_B"), text).unwrap();
    let step = VersionStamp::refusing(0, "Refuse me");
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a refused transformation pauses the run: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::PausedAtRefusedTransformation { path }]
                if path == &order_record(&root, "2026_08_22_B")
        ),
        "{warnings:?}"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(1)
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_B")),
        Some(0)
    );
    assert!(!result.marker_advanced());
    assert_eq!(
        result.reached_version(),
        0,
        "a paused run reports the version the marker still declares"
    );
    assert_eq!(marker_version(&root), 0);
}

/// The Orders Directory is configurable and persisted by initialization. An upgrade
/// that looked in the wrong place would enumerate nothing, conclude nothing is
/// pending, and then advance the marker — declaring every Order record migrated while
/// all of them stayed at the source version. That is the failure this pins.
#[test]
fn a_configured_orders_directory_is_upgraded_rather_than_declared_complete() {
    const ORDERS: &str = "Purchase Orders";
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders_at("upgrade-configured-orders", ORDERS, &basenames);
    regress_to_at(&root, ORDERS, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an older Package plans an upgrade");

    // The records really are where the configuration says, and the plan finds them.
    assert_eq!(
        plan.artifacts()
            .iter()
            .map(|artifact| artifact
                .path()
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string())
            .collect::<Vec<_>>(),
        [
            "Purchase Orders/2026_08_22_A/.spielorders/order.json",
            "Purchase Orders/2026_08_22_B/.spielorders/order.json",
            ".spielorders/vendors.json",
        ],
        "the plan enumerates the configured Orders Directory"
    );
    assert!(
        !plan.is_complete(),
        "records at the source version are pending work, not a finished upgrade"
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a complete chain commits: {report:?}");
    };
    assert_eq!(result.upgraded().len(), 3, "{:?}", result.upgraded());
    // The marker may only claim what the records can back up.
    assert_eq!(marker_version(&root), 1);
    for basename in basenames {
        assert_eq!(
            artifact_version(&order_record_at(&root, ORDERS, basename)),
            Some(1),
            "{basename} was actually upgraded, not merely declared upgraded"
        );
    }
}

/// The severe form of the same defect. When a directory merely named `Orders` also
/// exists beside the configured one, looking in the wrong place does not fail — it
/// finds nothing pending and advances the marker, leaving the Package declaring a
/// version its Order records never reached. A later run reads the advanced marker,
/// sees the newest version, and never revisits them.
#[test]
fn an_unrelated_orders_directory_cannot_make_the_upgrade_declare_itself_complete() {
    const ORDERS: &str = "Purchase Orders";
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders_at("upgrade-decoy-orders", ORDERS, &basenames);
    regress_to_at(&root, ORDERS, 0, &basenames);
    // An ordinary directory that merely happens to be named `Orders`. Nothing about
    // this Package's configuration points at it.
    fs::create_dir_all(root.join("Orders")).unwrap();
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an older Package plans an upgrade");
    assert!(
        plan.artifacts()
            .iter()
            .any(|artifact| artifact.path().starts_with(root.join(ORDERS))),
        "the decoy directory does not displace the configured one: {:?}",
        plan.artifacts()
            .iter()
            .map(|artifact| artifact.path().to_path_buf())
            .collect::<Vec<_>>()
    );

    let report = commit_metadata_upgrade(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a complete chain commits: {report:?}");
    };
    assert_eq!(result.upgraded().len(), 2, "{:?}", result.upgraded());
    // The marker and the records agree. Advancing one without the other is the
    // failure this exists to catch.
    assert_eq!(marker_version(&root), 1);
    assert_eq!(
        artifact_version(&order_record_at(&root, ORDERS, "2026_08_22_A")),
        Some(1),
        "the record reached the version the marker now claims"
    );
}

/// Every artifact reached the target and only the marker did not. The rewrites are
/// durable, so this cannot be a rejection either — but the Package still declares the
/// source version, which is what keeps rerunning safe.
#[test]
fn a_marker_that_cannot_advance_after_every_artifact_upgraded_still_reports_them() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-marker-failure", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &FailAt("version.json"));

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("artifacts that were really rewritten are committed: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::MarkerAdvanceFailed { path, .. }]
                if path == &root.join(".spielorders/version.json")
        ),
        "the report names the marker it could not advance: {warnings:?}"
    );
    assert_eq!(result.upgraded().len(), 3, "{:?}", result.upgraded());
    assert!(!result.marker_advanced());
    assert_eq!(
        result.reached_version(),
        0,
        "the Package still declares the source version, so rerunning is safe"
    );

    // The artifacts really were rewritten, which is why discarding them would be a
    // lie, and the marker really did not move.
    assert_eq!(marker_version(&root), 0);
    for basename in basenames {
        assert_eq!(artifact_version(&order_record(&root, basename)), Some(1));
    }

    // Rerunning finds only the marker outstanding and completes.
    let resumed = plan_metadata_upgrade(&root, &registry, &RealFileSystem)
        .expect("an interrupted Package can be replanned");
    assert_eq!(
        resumed
            .artifacts()
            .iter()
            .map(|artifact| artifact.state())
            .collect::<Vec<_>>(),
        [UpgradeArtifactState::AlreadyUpgraded; 3]
    );
    // Completion is the marker as well as the artifacts. Answering "complete" here
    // would tell a caller the Package may be used ordinarily while it still declares
    // the source version, which is the one thing keeping ordinary use blocked.
    assert!(
        !resumed.is_complete(),
        "the marker is still old, so the upgrade is not complete"
    );
    assert!(matches!(
        commit_metadata_upgrade(resumed, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert_eq!(marker_version(&root), 1);
}

/// The derived Package Index is a cache this application owns and can rebuild at any
/// time, and it is refreshed only after the marker has advanced. Failing to write it
/// therefore cannot undo an upgrade that already completed: it is reported beside a
/// committed run rather than turning one into a failure.
#[test]
fn an_index_that_cannot_be_rebuilt_is_a_warning_beside_a_completed_upgrade() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-index-failure", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &FailAt("index.json"));

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the upgrade completed and only its cache did not: {report:?}");
    };
    assert!(
        matches!(
            warnings.as_slice(),
            [MetadataUpgradeWarning::IndexRefreshFailed(_)]
        ),
        "{warnings:?}"
    );
    assert!(result.marker_advanced());
    assert_eq!(result.reached_version(), 1);
    assert_eq!(
        marker_version(&root),
        1,
        "the upgrade itself is durable; the index is a cache"
    );
    assert_eq!(
        artifact_version(&order_record(&root, "2026_08_22_A")),
        Some(1)
    );
}

/// The public vocabulary of an upgrade plan and report is what an adapter presents, so
/// its spellings are part of the contract rather than an implementation detail.
#[test]
fn the_upgrade_vocabulary_has_stable_spellings() {
    assert_eq!(UpgradeArtifact::OrderRecord.as_str(), "order_record");
    assert_eq!(
        UpgradeArtifact::VendorAggregate.as_str(),
        "vendor_aggregate"
    );
    assert_eq!(UpgradeArtifactState::Pending.as_str(), "pending");
    assert_eq!(
        UpgradeArtifactState::AlreadyUpgraded.as_str(),
        "already_upgraded"
    );
    assert_eq!(UpgradeArtifactState::Blocked.as_str(), "blocked");
    assert_eq!(
        MetadataUpgradeOperationKind::ReplaceArtifact.as_str(),
        "replace_artifact"
    );
    assert_eq!(
        MetadataUpgradeOperationKind::AdvanceMarker.as_str(),
        "advance_marker"
    );
    assert_eq!(
        MetadataUpgradeOperationKind::RefreshPackageIndex.as_str(),
        "refresh_package_index"
    );
    assert_eq!(
        UpgradeArtifactFailure::WrongDeclaredVersion.as_str(),
        "wrong_declared_version"
    );
    assert_eq!(
        UpgradeArtifactFailure::NotTheDeclaredSchema.as_str(),
        "not_the_declared_schema"
    );
    assert_eq!(
        UpgradeArtifactFailure::InvalidAtDeclaredVersion.as_str(),
        "invalid_at_declared_version"
    );
}

/// The other half of the same contract. When the very first artifact fails, nothing
/// durable has happened, so a rejection is the honest report and prior canonical bytes
/// are untouched. Reporting every failure as committed would be as wrong as reporting
/// every failure as rejected.
#[test]
fn a_failure_before_any_durable_progress_is_still_a_rejection() {
    let basenames = ["2026_08_22_A", "2026_08_22_B"];
    let (_tree, root) = package_with_orders("upgrade-no-progress", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();

    let report = commit_metadata_upgrade(plan, &FailAt("2026_08_22_A"));

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a run that upgraded nothing is a rejection: {report:?}");
    };
    assert!(matches!(
        rejection,
        MetadataUpgradeRejection::ArtifactReplacementFailed { .. }
    ));
    // Prior canonical bytes, exactly as the rejection promises.
    assert_eq!(marker_version(&root), 0);
    for basename in basenames {
        assert_eq!(artifact_version(&order_record(&root, basename)), Some(0));
    }
    assert_eq!(artifact_version(&vendors_record(&root)), Some(0));
}

/// An upgrade transforms bytes it read. If the record is rewritten in between,
/// writing the transformation back would discard the other writer's record and
/// replace it with a migration of bytes that are no longer there.
#[test]
fn an_artifact_rewritten_mid_upgrade_is_refused_rather_than_overwritten() {
    let basenames = ["2026_08_22_A"];
    let (_tree, root) = package_with_orders("upgrade-concurrent-artifact", &basenames);
    regress_to(&root, 0, &basenames);
    let step = VersionStamp::from(0);
    let registry = TransformationRegistry::registered().with(&step);
    let artifact = order_record(&root, "2026_08_22_A");
    let theirs = fs::read_to_string(&artifact)
        .unwrap()
        .replace("Laser components", "Rewritten by someone else")
        .into_bytes();

    let plan = plan_metadata_upgrade(&root, &registry, &RealFileSystem).unwrap();
    let report = commit_metadata_upgrade(
        plan,
        &ConcurrentWriter::replacing(&artifact, &theirs, EditPoint::StagingWrite),
    );

    // Nothing durable had happened yet, so this is a rejection rather than a
    // committed run — and the marker must not have advanced.
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an artifact rewritten underneath the run is refused: {report:?}");
    };
    let MetadataUpgradeRejection::ArtifactReplacementFailed { error, .. } = rejection else {
        panic!("the refusal comes from the artifact replacement: {rejection:?}");
    };
    assert!(
        matches!(
            error.failure(),
            spielorders_core::filesystem::ValidatedReplacementFailure::DestinationChanged
        ),
        "refused because the destination changed, not because the write failed: {:?}",
        error.failure()
    );
    assert_eq!(
        fs::read(&artifact).unwrap(),
        theirs,
        "the other writer's record survives"
    );
    assert_eq!(marker_version(&root), 0, "the marker did not advance");
}
