//! Decision sources: Define the SpielOrders package topology; Define the Order
//! record and lifecycle contract; Define the Rust CLI capability contract;
//! Define integrity, validation, and repair boundaries; Define durable metadata
//! schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexState, PackageLoad, open_package,
};
use spielorders_core::creation::{
    CanonicalOrderOutcome, OrderCreationInput, OrderCreationOperationKind, OrderCreationRejection,
    OrderCreationReport, OrderCreationWarning, PlannedOrderIdentity, commit_order_creation,
    plan_order_creation,
};
use spielorders_core::evidence::{EvidenceOutcome, FactChange};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::mutation::MutationReport;
use spielorders_core::package::ReusableValue;
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeObservation, FileSystem, FileSystemEntryKind,
    LocalCalendar, LocalDate, NewDirectoryRenameOutcome, NewFileLinkOutcome, UuidGenerator,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

struct FixedCalendar;

struct FixedUuid {
    value: String,
    calls: usize,
}

impl FixedUuid {
    fn new(value: &str) -> Self {
        Self {
            value: value.into(),
            calls: 0,
        }
    }
}

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.calls += 1;
        self.value.clone()
    }
}

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 8,
            day: 22,
        }
    }
}

struct FailingIndexReplacement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreationBoundary {
    CreateStagingDirectory,
    CreateMetadataDirectory,
    WriteOrderRecord,
    ReadOrderRecord,
    CorruptOrderRecord,
    SyncOrderRecord,
    WriteReadme,
    SyncReadme,
    CommitDirectory,
    DestinationAppearsBeforeCommit,
    ObserveIndexAfterCommit,
}

struct FailingCreation {
    boundary: CreationBoundary,
    directory_creations: Cell<usize>,
    fired: Cell<bool>,
    cleanup_failure_suffix: Option<&'static str>,
    cleanup_fired: Cell<bool>,
}

impl FailingCreation {
    fn at(boundary: CreationBoundary) -> Self {
        Self {
            boundary,
            directory_creations: Cell::new(0),
            fired: Cell::new(false),
            cleanup_failure_suffix: None,
            cleanup_fired: Cell::new(false),
        }
    }

    fn with_cleanup_failure(boundary: CreationBoundary, suffix: &'static str) -> Self {
        Self {
            boundary,
            directory_creations: Cell::new(0),
            fired: Cell::new(false),
            cleanup_failure_suffix: Some(suffix),
            cleanup_fired: Cell::new(false),
        }
    }

    fn fail_once(&self, boundary: CreationBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: CreationBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingCreation {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with(".spielorders/order.json")
            && self.fail_once(CreationBoundary::ReadOrderRecord)
        {
            return Err(Self::error(CreationBoundary::ReadOrderRecord));
        }
        if path.ends_with(".spielorders/order.json")
            && self.fail_once(CreationBoundary::CorruptOrderRecord)
        {
            return Ok(b"corrupted candidate".to_vec());
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders/order.json")
            && self.fail_once(CreationBoundary::WriteOrderRecord)
        {
            return Err(Self::error(CreationBoundary::WriteOrderRecord));
        }
        if path.ends_with("README.md") && self.fail_once(CreationBoundary::WriteReadme) {
            return Err(Self::error(CreationBoundary::WriteReadme));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders/order.json")
            && self.fail_once(CreationBoundary::SyncOrderRecord)
        {
            return Err(Self::error(CreationBoundary::SyncOrderRecord));
        }
        if path.ends_with("README.md") && self.fail_once(CreationBoundary::SyncReadme) {
            return Err(Self::error(CreationBoundary::SyncReadme));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if self
            .cleanup_failure_suffix
            .is_some_and(|suffix| path.ends_with(suffix))
            && self.fired.get()
            && !self.cleanup_fired.replace(true)
        {
            return Err(io::Error::other("injected README cleanup failure"));
        }
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        if self.boundary == CreationBoundary::ObserveIndexAfterCommit
            && self.fired.get()
            && path.ends_with(".spielorders/index.json")
        {
            return Err(Self::error(CreationBoundary::ObserveIndexAfterCommit));
        }
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for FailingCreation {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        let call = self.directory_creations.get();
        self.directory_creations.set(call + 1);
        let boundary = if call == 0 {
            CreationBoundary::CreateStagingDirectory
        } else {
            CreationBoundary::CreateMetadataDirectory
        };
        if self.fail_once(boundary) {
            return Err(Self::error(boundary));
        }
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
        if self.fail_once(CreationBoundary::CommitDirectory) {
            return Err(Self::error(CreationBoundary::CommitDirectory));
        }
        if self.boundary == CreationBoundary::DestinationAppearsBeforeCommit {
            // Something outside SpielOrders takes the name after planning found it
            // free. Delegating afterwards leaves the collision for the real adapter to
            // detect, so this fixture cannot assert an answer the adapter would not give.
            fs::create_dir(destination)?;
            self.fired.set(true);
        }
        if self.boundary == CreationBoundary::ObserveIndexAfterCommit {
            self.fired.set(true);
        }
        RealFileSystem.rename_new_directory(source, destination)
    }
}

impl FileSystem for FailingIndexReplacement {
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

impl DirectoryFileSystem for FailingIndexReplacement {
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

#[test]
fn title_only_plan_is_complete_and_side_effect_free_without_generating_identity() {
    let tree = initialized_package("order-creation-default-plan");
    let root = tree.path();
    let before = fs::read_dir(root.join("Orders")).unwrap().count();

    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("  Laser components  "),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("a title-only Order can be planned");

    assert_eq!(plan.identity(), PlannedOrderIdentity::GeneratedAtCommit);
    assert_eq!(plan.facts().title(), "Laser components");
    assert_eq!(plan.facts().date_ordered(), "2026-08-22");
    assert_eq!(plan.facts().status(), OrderStatus::InPreparation);
    assert_eq!(plan.package_root_locator(), "../..");
    assert_eq!(
        plan.directory().strip_prefix(plan.root()).unwrap(),
        std::path::Path::new("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderCreationOperationKind::CreateStagingDirectory,
            OrderCreationOperationKind::CreateMetadataDirectory,
            OrderCreationOperationKind::WriteOrderRecord,
            OrderCreationOperationKind::WriteReadme,
            OrderCreationOperationKind::CommitDirectory,
            OrderCreationOperationKind::RefreshPackageIndex,
        ]
    );
    assert_eq!(fs::read_dir(root.join("Orders")).unwrap().count(), before);
}

#[test]
fn fully_supplied_plan_resolves_vendor_and_reusable_values_into_normalized_facts() {
    let tree = initialized_package("order-creation-full-plan");
    let root = tree.path();
    fs::write(
        root.join(".spielorders/vendors.json"),
        br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": []
    }
  ]
}
"#,
    )
    .unwrap();
    let input = OrderCreationInput::new(" Laser / components ")
        .date_ordered("2026-07-19")
        .vendor_id("e0990d31-06aa-42e9-97c8-088e3e894822")
        .orderer(" Ada Lovelace ")
        .ordering_institution(" Northwind ")
        .currency("USD")
        .project(" Telescope ")
        .order_group(" Summer build ");

    let plan = plan_order_creation(root, &input, &FixedCalendar, &RealFileSystem)
        .expect("all supplied facts are valid");

    assert_eq!(plan.facts().date_ordered(), "2026-07-19");
    assert_eq!(
        plan.facts().vendor_id(),
        Some("e0990d31-06aa-42e9-97c8-088e3e894822")
    );
    assert_eq!(plan.facts().orderer(), Some("Ada Lovelace"));
    assert_eq!(plan.facts().ordering_institution(), Some("Northwind"));
    assert_eq!(plan.facts().currency(), Some("USD"));
    assert_eq!(plan.facts().project(), Some("Telescope"));
    assert_eq!(plan.facts().order_group(), Some("Summer build"));
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::New("Ada Lovelace".into())
    );
    assert_eq!(
        plan.directory().file_name().unwrap(),
        "2026_07_19_Tarnwick_Laser_components_Ada_Lovelace"
    );
}

#[test]
fn valid_commit_creates_one_complete_order_and_returns_the_refreshed_projection() {
    let tree = initialized_package("order-creation-commit");
    let root = tree.path();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid Order creation must commit")
    };
    assert_eq!(uuids.calls, 1);
    assert_eq!(result.canonical_outcome(), CanonicalOrderOutcome::Created);
    assert_eq!(result.order().document().order_id.as_str(), uuids.value);
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().document().date_ordered.as_str(),
        "2026-08-22"
    );
    assert_eq!(result.order().document().status, OrderStatus::InPreparation);
    assert!(result.order().document().items.is_empty());
    assert!(result.order().item_totals().is_empty());
    assert_eq!(result.order().order_total(), 0.0);
    // Creation writes the conventional README itself, empty, so the created Order's
    // text is known to be empty rather than merely unknown.
    assert_eq!(result.order().readme_text(), Some(""));
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);

    let directory = root.join("Orders/2026_08_22_Laser_components");
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), b"");
    let order_bytes = fs::read(directory.join(".spielorders/order.json")).unwrap();
    assert!(std::str::from_utf8(&order_bytes).unwrap().ends_with('\n'));
    assert!(
        !plan_staging_entries(root)
            .iter()
            .any(|name| name.starts_with('.'))
    );
    let PackageLoad::Available(package) = open_package(root, &RealFileSystem) else {
        panic!("the committed Package must load as available")
    };
    assert_eq!(package.healthy_orders().len(), 1);
    assert!(package.damaged_orders().is_empty());
}

#[test]
fn invalid_facts_and_unknown_vendor_reject_during_planning_without_creation() {
    let tree = initialized_package("order-creation-invalid-plan");
    let root = tree.path();
    let inputs = [
        OrderCreationInput::new("   "),
        OrderCreationInput::new("Bad date").date_ordered("2026-02-30"),
        OrderCreationInput::new("Bad currency").currency("usd"),
        OrderCreationInput::new("Unknown vendor").vendor_id("e0990d31-06aa-42e9-97c8-088e3e894822"),
    ];

    for input in inputs {
        assert!(
            plan_order_creation(root, &input, &FixedCalendar, &RealFileSystem).is_err(),
            "invalid or unresolved facts must reject"
        );
        assert!(plan_staging_entries(root).is_empty());
    }
}

#[test]
fn failed_index_refresh_keeps_the_order_committed_and_makes_retry_collide() {
    let tree = initialized_package("order-creation-index-warning");
    let root = tree.path();
    let prior_index = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &FailingIndexReplacement,
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &FailingIndexReplacement);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("index replacement failure must not reject canonical creation")
    };
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    assert!(matches!(
        warnings.as_slice(),
        [OrderCreationWarning::IndexRefreshFailed(_)]
    ));
    // The canonical scan behind the failed replacement did see the Order, so the
    // report carries the complete Order that scan established.
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(result.order().readme_text(), Some(""));
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        prior_index,
        "failed replacement preserves stale index bytes"
    );
    assert!(
        root.join("Orders/2026_08_22_Laser_components/.spielorders/order.json")
            .is_file()
    );

    assert!(
        plan_order_creation(
            root,
            &OrderCreationInput::new("Laser components"),
            &FixedCalendar,
            &FailingIndexReplacement,
        )
        .is_err(),
        "retry is rejected by the committed basename"
    );
    assert_eq!(uuids.calls, 1, "retry planning consumes no new identity");
}

#[test]
fn every_precommit_effect_failure_rejects_and_cleans_the_hidden_aggregate() {
    for boundary in [
        CreationBoundary::CreateStagingDirectory,
        CreationBoundary::CreateMetadataDirectory,
        CreationBoundary::WriteOrderRecord,
        CreationBoundary::ReadOrderRecord,
        CreationBoundary::CorruptOrderRecord,
        CreationBoundary::SyncOrderRecord,
        CreationBoundary::WriteReadme,
        CreationBoundary::SyncReadme,
        CreationBoundary::CommitDirectory,
    ] {
        let tree = initialized_package("order-creation-precommit-failure");
        let root = tree.path();
        let filesystem = FailingCreation::at(boundary);
        let plan = plan_order_creation(
            root,
            &OrderCreationInput::new("Laser components"),
            &FixedCalendar,
            &filesystem,
        )
        .unwrap();
        let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

        let report = commit_order_creation(plan, &mut uuids, &filesystem);

        assert!(
            matches!(report, MutationReport::Rejected { .. }),
            "{boundary:?} must reject before canonical commit"
        );
        assert!(
            plan_staging_entries(root).is_empty(),
            "{boundary:?} must remove the complete hidden staging aggregate"
        );
    }
}

#[test]
fn basename_and_generated_identity_collisions_reject_without_a_second_order() {
    let tree = initialized_package("order-creation-collisions");
    let root = tree.path();
    let first = plan_order_creation(
        root,
        &OrderCreationInput::new("First order"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    let mut first_uuid = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");
    assert!(matches!(
        commit_order_creation(first, &mut first_uuid, &RealFileSystem),
        MutationReport::Committed { .. }
    ));

    assert!(
        plan_order_creation(
            root,
            &OrderCreationInput::new("First order"),
            &FixedCalendar,
            &RealFileSystem,
        )
        .is_err(),
        "an existing projected basename is never suffixed"
    );

    let second = plan_order_creation(
        root,
        &OrderCreationInput::new("Second order"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    let mut duplicate_uuid = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");
    let report = commit_order_creation(second, &mut duplicate_uuid, &RealFileSystem);
    assert!(matches!(
        report,
        MutationReport::Rejected {
            rejection: OrderCreationRejection::GeneratedIdentityCollision { .. },
            ..
        }
    ));
    assert_eq!(plan_staging_entries(root).len(), 1);
    assert!(!root.join("Orders/2026_08_22_Second_order").exists());
}

#[test]
fn damaged_package_rejects_creation_without_consuming_identity_or_creating_paths() {
    let tree = initialized_package("order-creation-damaged-package");
    let root = tree.path();
    fs::write(root.join(".spielorders/vendors.json"), b"{ malformed").unwrap();

    assert!(
        plan_order_creation(
            root,
            &OrderCreationInput::new("Never created"),
            &FixedCalendar,
            &RealFileSystem,
        )
        .is_err()
    );
    assert!(plan_staging_entries(root).is_empty());
}

#[test]
fn existing_leading_and_trailing_underscores_survive_projection_normalization() {
    let tree = initialized_package("order-creation-underscore-projection");
    let plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("__"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("underscores are valid title content");

    assert_eq!(plan.directory().file_name().unwrap(), "2026_08_22__");
}

#[test]
fn candidate_corruption_retains_any_failed_cleanup_evidence() {
    let tree = initialized_package("order-creation-corruption-cleanup");
    let filesystem = FailingCreation::with_cleanup_failure(
        CreationBoundary::CorruptOrderRecord,
        ".spielorders/order.json",
    );
    let plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &filesystem,
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &filesystem);

    let MutationReport::Rejected {
        rejection: OrderCreationRejection::CommitFailed {
            cleanup_failures, ..
        },
        ..
    } = report
    else {
        panic!("candidate corruption is a typed commit failure")
    };
    assert!(!cleanup_failures.is_empty());
}

#[test]
fn late_destination_collision_preserves_concurrent_evidence_and_cleanup_failures() {
    let tree = initialized_package("order-creation-late-destination");
    let filesystem = FailingCreation::with_cleanup_failure(
        CreationBoundary::DestinationAppearsBeforeCommit,
        "README.md",
    );
    let plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &filesystem,
    )
    .unwrap();
    let destination = plan.directory().to_path_buf();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &filesystem);

    let MutationReport::Rejected {
        rejection: OrderCreationRejection::CommitFailed {
            cleanup_failures, ..
        },
        ..
    } = report
    else {
        panic!("late destination collision is a typed commit rejection")
    };
    assert!(!cleanup_failures.is_empty());
    assert!(destination.is_dir(), "concurrent destination is preserved");
}

#[test]
fn postcommit_index_assessment_failure_retains_typed_warning_and_stale_evidence() {
    let tree = initialized_package("order-creation-index-assessment-warning");
    let filesystem = FailingCreation::at(CreationBoundary::ObserveIndexAfterCommit);
    let plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &filesystem,
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &filesystem);

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("postcommit index assessment failure keeps canonical success")
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderCreationWarning::IndexRefreshFailed(_)]
    ));
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    // No refreshed Package exists here at all, so the Order in this report is again
    // creation's own projection and nothing else confirms its contents.
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(result.order().readme_text(), Some(""));
}

#[test]
fn incomplete_precommit_cleanup_is_explicit_and_never_exposes_a_canonical_order() {
    let tree = initialized_package("order-creation-incomplete-cleanup");
    let root = tree.path();
    let filesystem =
        FailingCreation::with_cleanup_failure(CreationBoundary::CommitDirectory, "README.md");
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &filesystem,
    )
    .unwrap();
    let staging = plan.staging_directory().to_path_buf();
    let final_directory = plan.directory().to_path_buf();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &filesystem);

    let MutationReport::Rejected {
        rejection: OrderCreationRejection::CommitFailed {
            cleanup_failures, ..
        },
        ..
    } = report
    else {
        panic!("the commit and cleanup failures remain distinguishable")
    };
    assert!(!cleanup_failures.is_empty());
    assert!(staging.is_dir(), "staging evidence remains for diagnosis");
    assert!(
        !final_directory.exists(),
        "no incomplete Order is discoverable"
    );
}

fn plan_staging_entries(root: &std::path::Path) -> Vec<String> {
    fs::read_dir(root.join("Orders"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

/// One Vendor at a fixed identity, written straight into the Package the way an edit
/// made outside this process would land between a plan and its commit.
fn write_one_vendor(root: &Path, name: &str) {
    fs::write(
        root.join(".spielorders/vendors.json"),
        format!(
            "{{\n  \"schema_version\": 1,\n  \"vendors\": [\n    {{\n      \
             \"vendor_id\": \"{VENDOR_ID}\",\n      \"name\": \"{name}\",\n      \
             \"contacts\": []\n    }}\n  ]\n}}\n"
        ),
    )
    .expect("the fixture Vendors document is written");
}

/// The facts a refused commit says no longer stand, as `(name, change)` pairs.
///
/// The refusal is only useful if it says *which* fact moved, so every test below
/// reads the names rather than the rejection's variant alone.
fn unconfirmed_facts<E: std::fmt::Debug>(
    report: &OrderCreationReport<E>,
) -> Vec<(&str, FactChange)> {
    let MutationReport::Rejected {
        rejection: OrderCreationRejection::PlanEvidenceUnconfirmed { outcome },
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
fn a_commit_is_refused_when_the_package_no_longer_configures_the_orders_directory_it_planned_for() {
    let tree = initialized_package("order-creation-reconfigured-orders");
    let root = tree.path();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be planned against the configured Orders Directory");
    let planned_directory = plan.directory().to_path_buf();

    // The Package is reconfigured to keep its Orders somewhere else. The destination
    // this plan projected is still absent, so absence on its own still reads as
    // "go ahead" — which is exactly why absence is not enough.
    fs::create_dir(root.join("Purchases")).unwrap();
    fs::write(
        root.join(".spielorders/config.toml"),
        b"orders_path = \"Purchases\"\nremoved_path = \"Removed\"\n",
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [("orders_directory", FactChange::Differs)],
        "the refusal names the configured directory that moved"
    );
    assert!(
        !planned_directory.exists(),
        "nothing is published into the Orders Directory this Package no longer configures"
    );
    assert_eq!(
        fs::read_dir(root.join("Purchases")).unwrap().count(),
        0,
        "and nothing is published into the newly configured one either"
    );
    assert_eq!(uuids.calls, 0, "a refused plan consumes no identity");
}

#[test]
fn a_commit_is_refused_when_the_vendor_its_directory_name_spells_was_renamed() {
    let tree = initialized_package("order-creation-renamed-vendor");
    let root = tree.path();
    write_one_vendor(root, "Tarnwick");
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components").vendor_id(VENDOR_ID),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Vendor resolves while planning");
    let planned_directory = plan.directory().to_path_buf();
    assert_eq!(
        planned_directory.file_name().unwrap(),
        "2026_08_22_Tarnwick_Laser_components",
        "the projected directory spells the Vendor's name, which is what makes it stale"
    );

    write_one_vendor(root, "Nethermoor");
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [("vendor_name", FactChange::Differs)],
        "the refusal names the Vendor whose name the directory spells"
    );
    assert!(
        !planned_directory.exists(),
        "no Order is published under the Vendor's former name"
    );
    assert_eq!(uuids.calls, 0, "a refused plan consumes no identity");
}

#[test]
fn a_commit_is_refused_when_the_vendor_it_resolved_was_deleted() {
    let tree = initialized_package("order-creation-deleted-vendor");
    let root = tree.path();
    write_one_vendor(root, "Tarnwick");
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components").vendor_id(VENDOR_ID),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Vendor resolves while planning");
    let planned_directory = plan.directory().to_path_buf();

    fs::write(
        root.join(".spielorders/vendors.json"),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": []\n}\n",
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [("vendor_name", FactChange::Vanished)],
        "a Vendor that is gone is reported as vanished rather than merely different"
    );
    assert!(!planned_directory.exists());
    assert_eq!(uuids.calls, 0, "a refused plan consumes no identity");
}

#[test]
fn a_commit_is_refused_when_a_value_it_reported_as_new_became_reusable() {
    let tree = initialized_package("order-creation-reusable-appeared");
    let root = tree.path();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components").orderer("Ada Lovelace"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be planned");
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::New("Ada Lovelace".into()),
        "the plan told the caller this Orderer was new to the Package"
    );
    let planned_directory = plan.directory().to_path_buf();

    // Another Order introduces the same Orderer under the Package's own spelling, so
    // the reusable-value outcome this plan would report is no longer true of the
    // Package it is about to be committed into.
    let other = plan_order_creation(
        root,
        &OrderCreationInput::new("Mounts").orderer("ada lovelace"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    assert!(matches!(
        commit_order_creation(
            other,
            &mut FixedUuid::new("1f2e3d4c-5b6a-4798-8877-665544332211"),
            &RealFileSystem,
        ),
        MutationReport::Committed { .. }
    ));
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = commit_order_creation(plan, &mut uuids, &RealFileSystem);

    assert_eq!(
        unconfirmed_facts(&report),
        [("orderer", FactChange::Differs)],
        "the refusal names the reusable value whose canonical spelling moved"
    );
    assert!(!planned_directory.exists());
    assert_eq!(uuids.calls, 0, "a refused plan consumes no identity");
}

/// Publishes the Order for real, then removes its canonical record before the
/// post-commit refresh reads it — the concurrent external change that leaves a
/// committed Order absent from the refreshed healthy set.
struct VanishAfterPublish {
    published: Cell<bool>,
    failing_index_replacement: bool,
}

impl VanishAfterPublish {
    fn new() -> Self {
        Self {
            published: Cell::new(false),
            failing_index_replacement: false,
        }
    }

    /// The same vanishing Order, but the rebuilt index cannot be written either, so
    /// the refresh reports a replacement failure over evidence that never saw the
    /// Order.
    fn with_failing_index_replacement() -> Self {
        Self {
            published: Cell::new(false),
            failing_index_replacement: true,
        }
    }
}

impl FileSystem for VanishAfterPublish {
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
        if self.failing_index_replacement && destination.ends_with(".spielorders/index.json") {
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

impl DirectoryFileSystem for VanishAfterPublish {
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
        // The Order is now published. Something outside SpielOrders removes its
        // record before the refresh runs, so the refreshed Package reports it damaged
        // rather than healthy.
        if outcome == NewDirectoryRenameOutcome::Renamed && !self.published.replace(true) {
            let _ = std::fs::remove_file(destination.join(".spielorders/order.json"));
        }
        Ok(outcome)
    }
}

#[test]
fn an_order_that_vanishes_before_the_refresh_still_reports_its_commit() {
    let tree = initialized_package("creation-vanished-before-refresh");
    let root = tree.path();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be planned");
    let directory = plan.directory().to_path_buf();

    // The directory rename that publishes the Order succeeds, so the creation really
    // happened. Reporting it as anything but committed would tell the caller no Order
    // was created when one was, and panicking would report nothing at all.
    let report = commit_order_creation(
        plan,
        &mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        &VanishAfterPublish::new(),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a published Order that vanished before the refresh is committed: {report:?}");
    };
    assert_eq!(result.canonical_outcome(), CanonicalOrderOutcome::Created);
    assert!(
        warnings.contains(&OrderCreationWarning::RefreshUnavailable),
        "and says the refreshed projection could not confirm it: {warnings:?}"
    );
    // Everything below comes from the projection this commit builds for itself: the
    // refreshed Package cannot see this Order, so nothing else can supply it. Asserting
    // only the variant, or only the identity, would pass against a projection that got
    // every other fact wrong.
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "the report carries the Order that was created"
    );
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().document().date_ordered.as_str(),
        "2026-08-22"
    );
    assert_eq!(result.order().document().status, OrderStatus::InPreparation);
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser_components"),
        "at the directory this creation published it to"
    );
    assert_eq!(
        result.order().readme_text(),
        Some(""),
        "creation wrote the README itself, empty, so its text is known rather than unread"
    );
    assert!(result.order().item_totals().is_empty());
    assert_eq!(result.order().order_total(), 0.0);
    // The refresh itself succeeded — it simply does not contain this Order — so the
    // index this report carries is the one that refresh assessed, not a stale record
    // standing in for a refresh that never ran.
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    // The directory really is on disk, which is why the commit must be reported.
    assert!(directory.is_dir());
}

#[test]
fn a_vanished_order_and_a_failed_index_replacement_keep_the_assessed_evidence() {
    let tree = initialized_package("creation-vanished-and-replacement-failed");
    let root = tree.path();
    let prior_index = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = plan_order_creation(
        root,
        &OrderCreationInput::new("Laser components"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the Order can be planned");

    // Both post-commit misfortunes at once: the published Order is gone before the
    // refresh reads it, and the rebuilt index cannot be written either.
    let report = commit_order_creation(
        plan,
        &mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        &VanishAfterPublish::with_failing_index_replacement(),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the published Order is still committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [OrderCreationWarning::IndexRefreshFailed(_)]
    ));
    // The canonical scan behind the failed replacement completed, so its assessment is
    // retained rather than replaced by a stale record standing in for no refresh.
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    // That scan could not see this Order, so the Order is creation's own projection.
    assert_eq!(result.canonical_outcome(), CanonicalOrderOutcome::Created);
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(result.order().readme_text(), Some(""));
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        prior_index,
        "and the failed replacement left the stored index untouched"
    );
}
