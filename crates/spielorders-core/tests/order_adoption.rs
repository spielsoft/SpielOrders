//! Public behavior checks for adopting one existing Order Directory in place.
//!
//! Decision sources: Define the SpielOrders package topology; Define the Order record
//! and lifecycle contract; Define legacy Order adoption and metadata bootstrapping;
//! Define integrity, validation, and repair boundaries; Define durable metadata
//! schemas and version evolution.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{
    CanonicalAdoptionOutcome, OrderAdoptionInput, OrderAdoptionOperationKind,
    OrderAdoptionPlanError, OrderAdoptionRejection, OrderAdoptionWarning, OrderItemInput,
    PlannedAdoptionIdentity, commit_order_adoption, plan_order_adoption,
};
use spielorders_core::canonical::{PackageIndexRefresh, PackageIndexState};
use spielorders_core::creation::{OrderCreationInput, commit_order_creation, plan_order_creation};
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
use std::cell::{Cell, RefCell};
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

struct FixedUuid {
    value: String,
    calls: usize,
}

struct FixedCalendar;

#[derive(Default)]
struct ObservingFileSystem {
    reads: RefCell<Vec<PathBuf>>,
    listings: RefCell<Vec<PathBuf>>,
}

impl FileSystem for ObservingFileSystem {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.reads.borrow_mut().push(path.to_path_buf());
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
        self.listings.borrow_mut().push(path.to_path_buf());
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for ObservingFileSystem {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdoptionBoundary {
    CreateMetadata,
    WriteOrder,
    ReadOrder,
    CorruptOrder,
    SyncOrder,
    WriteReadme,
    SyncReadme,
    CommitReadme,
    CommitOrder,
    CleanupOrderStage,
    IndexReplacement,
}

struct FailingAdoption {
    boundary: AdoptionBoundary,
    fired: Cell<bool>,
    fail_rollback_order_stage: bool,
}

impl FailingAdoption {
    fn at(boundary: AdoptionBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_rollback_order_stage: false,
        }
    }

    fn with_rollback_failure(boundary: AdoptionBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_rollback_order_stage: true,
        }
    }

    fn fail_once(&self, boundary: AdoptionBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: AdoptionBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingAdoption {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with(".spielorders/.order.json.spielorders-stage")
            && self.fail_once(AdoptionBoundary::ReadOrder)
        {
            return Err(Self::error(AdoptionBoundary::ReadOrder));
        }
        if path.ends_with(".spielorders/.order.json.spielorders-stage")
            && self.fail_once(AdoptionBoundary::CorruptOrder)
        {
            return Ok(b"corrupted staged Order".to_vec());
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders/.order.json.spielorders-stage")
            && self.fail_once(AdoptionBoundary::WriteOrder)
        {
            return Err(Self::error(AdoptionBoundary::WriteOrder));
        }
        if path.ends_with(".README.md.spielorders-stage")
            && self.fail_once(AdoptionBoundary::WriteReadme)
        {
            return Err(Self::error(AdoptionBoundary::WriteReadme));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders/.order.json.spielorders-stage")
            && self.fail_once(AdoptionBoundary::SyncOrder)
        {
            return Err(Self::error(AdoptionBoundary::SyncOrder));
        }
        if path.ends_with(".README.md.spielorders-stage")
            && self.fail_once(AdoptionBoundary::SyncReadme)
        {
            return Err(Self::error(AdoptionBoundary::SyncReadme));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(AdoptionBoundary::IndexReplacement)
        {
            return Err(Self::error(AdoptionBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders/.order.json.spielorders-stage")
            && (self.fail_once(AdoptionBoundary::CleanupOrderStage)
                || (self.fail_rollback_order_stage && self.fired.get()))
        {
            return Err(io::Error::other("injected Order-stage cleanup failure"));
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

impl DirectoryFileSystem for FailingAdoption {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(".spielorders") && self.fail_once(AdoptionBoundary::CreateMetadata) {
            return Err(Self::error(AdoptionBoundary::CreateMetadata));
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
        if destination.ends_with("README.md") && self.fail_once(AdoptionBoundary::CommitReadme) {
            return Err(Self::error(AdoptionBoundary::CommitReadme));
        }
        if destination.ends_with(".spielorders/order.json")
            && self.fail_once(AdoptionBoundary::CommitOrder)
        {
            return Err(Self::error(AdoptionBoundary::CommitOrder));
        }
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

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 8,
            day: 26,
        }
    }
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
fn valid_adoption_preserves_the_directory_and_returns_a_complete_projection() {
    let tree = initialized_package("order-adoption-valid");
    let candidate = tree.path().join("Orders/Legacy microscope order");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("README.md"), b"legacy notes\n").unwrap();
    fs::write(candidate.join("quote.pdf"), b"opaque supporting bytes").unwrap();
    let input = OrderAdoptionInput::new(
        "  Microscope objective  ",
        "2024-02-29",
        OrderStatus::Received,
    )
    .item(OrderItemInput::new(" Objective ", 2.0, 125.5).catalog_number(" OBJ-10 "));
    let mut uuids = FixedUuid::new("29dcfef8-cfaa-43f1-bcff-fd54962e3d8d");

    let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem)
        .expect("the physical direct child and explicit facts are eligible");

    assert_eq!(uuids.calls, 0);
    assert_eq!(plan.directory(), fs::canonicalize(&candidate).unwrap());
    assert_eq!(plan.package_root_locator(), "../..");
    assert_eq!(
        fs::read(candidate.join("README.md")).unwrap(),
        b"legacy notes\n"
    );
    assert_eq!(
        fs::read(candidate.join("quote.pdf")).unwrap(),
        b"opaque supporting bytes"
    );

    let report = commit_order_adoption(plan, &mut uuids, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid adoption must commit")
    };
    assert_eq!(uuids.calls, 1);
    assert_eq!(
        result.canonical_outcome(),
        CanonicalAdoptionOutcome::Adopted
    );
    assert_eq!(result.order().document().order_id.as_str(), uuids.value);
    assert_eq!(
        result.order().document().title.as_str(),
        "Microscope objective"
    );
    assert_eq!(result.order().document().status, OrderStatus::Received);
    assert_eq!(result.order().item_totals(), &[251.0]);
    assert_eq!(result.order().order_total(), 251.0);
    assert_eq!(result.order().readme_text(), Some("legacy notes\n"));
    assert_eq!(
        result.order().paths().directory().absolute_path(),
        fs::canonicalize(&candidate).unwrap()
    );
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    assert_eq!(
        fs::read(candidate.join("README.md")).unwrap(),
        b"legacy notes\n"
    );
    assert_eq!(
        fs::read(candidate.join("quote.pdf")).unwrap(),
        b"opaque supporting bytes"
    );
}

#[test]
fn dry_run_is_complete_and_has_no_filesystem_or_identity_effect() {
    let tree = initialized_package("order-adoption-plan");
    let candidate = tree.path().join("Orders/Unparsed_legacy_name");
    fs::create_dir(&candidate).unwrap();
    fs::write(
        tree.path().join(".spielorders/vendors.json"),
        br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "02b97a0f-d93c-405f-96a4-1e8768cc9d88",
      "name": "Nethermoor",
      "contacts": []
    }
  ]
}
"#,
    )
    .unwrap();
    let input = OrderAdoptionInput::new(" Vacuum parts ", "2020-01-02", OrderStatus::Hold)
        .vendor_name("Nethermoor")
        .orderer(" Ada ")
        .ordering_institution(" Northwind ")
        .currency("USD")
        .project(" Clock ")
        .order_group(" Legacy ")
        .item(OrderItemInput::new(" Flange ", -2.0, 3.5));

    let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem)
        .expect("explicit valid facts can be planned");

    assert_eq!(plan.identity(), PlannedAdoptionIdentity::GeneratedAtCommit);
    assert_eq!(plan.document().title.as_str(), "Vacuum parts");
    assert_eq!(plan.document().date_ordered.as_str(), "2020-01-02");
    assert_eq!(plan.document().status, OrderStatus::Hold);
    assert_eq!(plan.document().items.len(), 1);
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::New("Ada".into())
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            OrderAdoptionOperationKind::CreateMetadataDirectory,
            OrderAdoptionOperationKind::StageOrderRecord,
            OrderAdoptionOperationKind::StageReadme,
            OrderAdoptionOperationKind::CommitReadme,
            OrderAdoptionOperationKind::CommitOrderRecord,
            OrderAdoptionOperationKind::RemoveStaging,
            OrderAdoptionOperationKind::RemoveStaging,
            OrderAdoptionOperationKind::RefreshPackageIndex,
        ]
    );
    assert!(!candidate.join(".spielorders").exists());
    assert!(!candidate.join("README.md").exists());
}

#[test]
fn only_a_non_hidden_physical_ordinary_direct_child_is_eligible() {
    let tree = initialized_package("order-adoption-eligibility");
    let orders = tree.path().join("Orders");
    let physical = orders.join("Physical");
    let hidden = orders.join(".Hidden");
    let nested_parent = physical.join("Nested");
    let nested = nested_parent.join("Candidate");
    let link = orders.join("Alias");
    let file = orders.join("receipt.txt");
    fs::create_dir(&physical).unwrap();
    fs::create_dir(&hidden).unwrap();
    fs::create_dir(&nested_parent).unwrap();
    fs::create_dir(&nested).unwrap();
    symlink(&physical, &link).unwrap();
    fs::write(&file, b"not a directory").unwrap();
    let input = OrderAdoptionInput::new("Explicit", "2026-08-26", OrderStatus::Complete);

    assert!(matches!(
        plan_order_adoption(tree.path(), &hidden, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::HiddenCandidate { .. })
    ));
    assert!(matches!(
        plan_order_adoption(tree.path(), &nested, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::NotDirectChild { .. })
    ));
    assert!(matches!(
        plan_order_adoption(tree.path(), &link, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::IneligibleCandidate {
            kind: Some(FileSystemEntryKind::SymbolicLink),
            ..
        })
    ));
    assert!(matches!(
        plan_order_adoption(tree.path(), &file, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::IneligibleCandidate {
            kind: Some(FileSystemEntryKind::File),
            ..
        })
    ));
}

#[test]
fn managed_partial_invalid_and_damaged_candidates_reject_before_identity_or_write() {
    let tree = initialized_package("order-adoption-rejections");
    let orders = tree.path().join("Orders");
    let managed = orders.join("Managed");
    fs::create_dir(&managed).unwrap();
    let input = OrderAdoptionInput::new("Managed", "2024-01-01", OrderStatus::Ordered);
    let plan = plan_order_adoption(tree.path(), &managed, &input, &RealFileSystem).unwrap();
    let mut committed_uuid = FixedUuid::new("78ee964a-f03c-43f1-acbc-149ece5cfc09");
    assert!(matches!(
        commit_order_adoption(plan, &mut committed_uuid, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert!(matches!(
        plan_order_adoption(tree.path(), &managed, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::AlreadyManaged { .. })
    ));

    let partial = orders.join("Partial");
    fs::create_dir(&partial).unwrap();
    fs::create_dir(partial.join(".spielorders")).unwrap();
    fs::write(partial.join(".spielorders/unknown"), b"reserved").unwrap();
    assert!(matches!(
        plan_order_adoption(tree.path(), &partial, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::ConflictingReservedState { .. })
    ));

    let invalid = orders.join("Invalid");
    fs::create_dir(&invalid).unwrap();
    assert!(matches!(
        plan_order_adoption(
            tree.path(),
            &invalid,
            &OrderAdoptionInput::new(" ", "2024-02-30", OrderStatus::Hold)
                .item(OrderItemInput::new("", f64::NAN, 1.0)),
            &RealFileSystem,
        ),
        Err(OrderAdoptionPlanError::InvalidFacts)
    ));
    assert!(matches!(
        plan_order_adoption(
            tree.path(),
            &invalid,
            &input.clone().vendor_name("Nonesuch Optics"),
            &RealFileSystem,
        ),
        Err(OrderAdoptionPlanError::UnknownVendor { .. })
    ));

    fs::write(tree.path().join(".spielorders/vendors.json"), b"not json").unwrap();
    assert!(matches!(
        plan_order_adoption(tree.path(), &invalid, &input, &RealFileSystem),
        Err(OrderAdoptionPlanError::DamagedPackage)
    ));
    assert!(!invalid.join(".spielorders").exists());
}

#[test]
fn replacing_the_selected_directory_after_planning_rejects_without_adopting_the_replacement() {
    let tree = initialized_package("order-adoption-replaced-candidate");
    let candidate = tree.path().join("Orders/Selected");
    fs::create_dir(&candidate).unwrap();
    let input = OrderAdoptionInput::new("Selected", "2024-01-01", OrderStatus::Complete);
    let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
    fs::remove_dir(&candidate).unwrap();
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("replacement.txt"), b"different directory").unwrap();
    let mut uuids = FixedUuid::new("236794d8-7897-489f-9206-247c7d3f2892");

    let report = commit_order_adoption(plan, &mut uuids, &RealFileSystem);

    assert!(matches!(
        report,
        MutationReport::Rejected {
            rejection: OrderAdoptionRejection::PreconditionsChanged { .. },
            ..
        }
    ));
    assert_eq!(uuids.calls, 0);
    assert!(!candidate.join(".spielorders").exists());
    assert_eq!(
        fs::read(candidate.join("replacement.txt")).unwrap(),
        b"different directory"
    );
}

#[test]
fn absent_readme_is_created_empty_and_generated_identity_collisions_do_not_mutate_candidate() {
    let tree = initialized_package("order-adoption-readme-id-collision");
    let existing_plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Existing"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    let colliding_id = "86ec2b61-c96a-4fca-9275-39479923f56b";
    let mut existing_uuid = FixedUuid::new(colliding_id);
    assert!(matches!(
        commit_order_creation(existing_plan, &mut existing_uuid, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let candidate = tree.path().join("Orders/Legacy without readme");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("support.bin"), b"keep me").unwrap();
    let input = OrderAdoptionInput::new("Legacy", "2020-05-06", OrderStatus::Complete);
    let collision_plan =
        plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
    let mut collision_uuid = FixedUuid::new(colliding_id);

    assert!(matches!(
        commit_order_adoption(collision_plan, &mut collision_uuid, &RealFileSystem),
        MutationReport::Rejected {
            rejection: OrderAdoptionRejection::GeneratedIdentityCollision { .. },
            ..
        }
    ));
    assert!(!candidate.join("README.md").exists());
    assert!(!candidate.join(".spielorders").exists());
    assert_eq!(fs::read(candidate.join("support.bin")).unwrap(), b"keep me");

    let commit_plan =
        plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
    let mut unique_uuid = FixedUuid::new("edcf2a2d-4e78-44da-aab8-19399c382da0");
    assert!(matches!(
        commit_order_adoption(commit_plan, &mut unique_uuid, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    assert_eq!(fs::read(candidate.join("README.md")).unwrap(), b"");
    assert_eq!(fs::read(candidate.join("support.bin")).unwrap(), b"keep me");
    assert!(!candidate.join(".README.md.spielorders-stage").exists());
    assert!(
        !candidate
            .join(".spielorders/.order.json.spielorders-stage")
            .exists()
    );
}

#[test]
fn adoption_never_lists_candidate_contents_or_reads_supporting_material() {
    let tree = initialized_package("order-adoption-opaque-evidence");
    let candidate = tree.path().join("Orders/Opaque candidate");
    fs::create_dir(&candidate).unwrap();
    fs::create_dir(candidate.join("documents")).unwrap();
    fs::write(candidate.join("documents/quote.bin"), b"opaque sentinel").unwrap();
    symlink("documents/quote.bin", candidate.join("quote-link")).unwrap();
    fs::write(
        candidate.join("README.md"),
        b"readme is the only user document",
    )
    .unwrap();
    let effects = ObservingFileSystem::default();
    let input = OrderAdoptionInput::new("Opaque", "2019-08-22", OrderStatus::Ordered);
    let plan = plan_order_adoption(tree.path(), &candidate, &input, &effects).unwrap();
    let canonical_candidate = plan.directory().to_path_buf();
    let mut uuids = FixedUuid::new("b86004b3-b753-47bf-88fd-5afe70596a12");

    assert!(matches!(
        commit_order_adoption(plan, &mut uuids, &effects),
        MutationReport::Committed { .. }
    ));

    assert!(
        effects
            .listings
            .borrow()
            .iter()
            .all(|path| path != &canonical_candidate)
    );
    let allowed_reads = [
        canonical_candidate.join("README.md"),
        canonical_candidate.join(".spielorders/order.json"),
        canonical_candidate.join(".spielorders/.order.json.spielorders-stage"),
    ];
    assert!(
        effects.reads.borrow().iter().all(|path| {
            !path.starts_with(&canonical_candidate) || allowed_reads.contains(path)
        })
    );
    assert_eq!(
        fs::read(candidate.join("documents/quote.bin")).unwrap(),
        b"opaque sentinel"
    );
    assert_eq!(
        fs::read_link(candidate.join("quote-link")).unwrap(),
        Path::new("documents/quote.bin")
    );
}

#[test]
fn every_precommit_effect_failure_rejects_and_restores_the_unmanaged_candidate() {
    for boundary in [
        AdoptionBoundary::CreateMetadata,
        AdoptionBoundary::WriteOrder,
        AdoptionBoundary::ReadOrder,
        AdoptionBoundary::CorruptOrder,
        AdoptionBoundary::SyncOrder,
        AdoptionBoundary::WriteReadme,
        AdoptionBoundary::SyncReadme,
        AdoptionBoundary::CommitReadme,
        AdoptionBoundary::CommitOrder,
    ] {
        let tree = initialized_package(&format!("order-adoption-fault-{boundary:?}"));
        let candidate = tree.path().join("Orders/Candidate");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("support.bin"), b"unchanged").unwrap();
        let input = OrderAdoptionInput::new("Candidate", "2025-06-07", OrderStatus::Received);
        let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
        let mut uuids = FixedUuid::new("58fc4a94-b3f7-41b2-b2a0-f81f552305f1");

        let report = commit_order_adoption(plan, &mut uuids, &FailingAdoption::at(boundary));

        assert!(
            matches!(
                report,
                MutationReport::Rejected {
                    rejection: OrderAdoptionRejection::CommitFailed { .. },
                    ..
                }
            ),
            "{boundary:?} must reject before canonical commit"
        );
        assert_eq!(
            fs::read(candidate.join("support.bin")).unwrap(),
            b"unchanged"
        );
        assert!(!candidate.join("README.md").exists(), "{boundary:?}");
        assert!(!candidate.join(".spielorders").exists(), "{boundary:?}");
    }
}

#[test]
fn rollback_failure_is_explicit_without_overwriting_preexisting_candidate_bytes() {
    let tree = initialized_package("order-adoption-rollback-failure");
    let candidate = tree.path().join("Orders/Candidate");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("support.bin"), b"preexisting").unwrap();
    let input = OrderAdoptionInput::new("Candidate", "2025-06-07", OrderStatus::Received);
    let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
    let mut uuids = FixedUuid::new("15d847c1-4f27-4765-9d31-46c8d59c79f2");

    let report = commit_order_adoption(
        plan,
        &mut uuids,
        &FailingAdoption::with_rollback_failure(AdoptionBoundary::CommitOrder),
    );

    let MutationReport::Rejected {
        rejection:
            OrderAdoptionRejection::CommitFailed {
                rollback_failures, ..
            },
        ..
    } = report
    else {
        panic!("failed rollback must remain a typed rejection")
    };
    assert!(!rollback_failures.is_empty());
    assert_eq!(
        fs::read(candidate.join("support.bin")).unwrap(),
        b"preexisting"
    );
    assert!(!candidate.join("README.md").exists());
    assert!(!candidate.join(".spielorders/order.json").exists());
    assert!(
        candidate
            .join(".spielorders/.order.json.spielorders-stage")
            .exists()
    );
}

#[test]
fn postcommit_cleanup_and_index_failures_warn_without_undoing_adoption() {
    for boundary in [
        AdoptionBoundary::CleanupOrderStage,
        AdoptionBoundary::IndexReplacement,
    ] {
        let tree = initialized_package(&format!("order-adoption-warning-{boundary:?}"));
        let candidate = tree.path().join("Orders/Candidate");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("README.md"), b"preserved").unwrap();
        let input = OrderAdoptionInput::new("Candidate", "2025-06-07", OrderStatus::Received);
        let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
        let mut uuids = FixedUuid::new("791f8208-a7a3-498b-a4fa-3608650270bd");

        let report = commit_order_adoption(plan, &mut uuids, &FailingAdoption::at(boundary));

        let MutationReport::CommittedWithWarning {
            result, warnings, ..
        } = report
        else {
            panic!("{boundary:?} follows canonical commit and must warn")
        };
        assert_eq!(
            result.canonical_outcome(),
            CanonicalAdoptionOutcome::Adopted
        );
        assert!(candidate.join(".spielorders/order.json").exists());
        assert_eq!(fs::read(candidate.join("README.md")).unwrap(), b"preserved");
        match boundary {
            AdoptionBoundary::CleanupOrderStage => assert!(warnings.iter().any(|warning| {
                matches!(warning, OrderAdoptionWarning::StagingCleanupFailed(_))
            })),
            AdoptionBoundary::IndexReplacement => {
                assert!(warnings.iter().any(|warning| {
                    matches!(warning, OrderAdoptionWarning::IndexRefreshFailed(_))
                }))
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem),
            Err(OrderAdoptionPlanError::AlreadyManaged { .. })
        ));
    }
}

#[test]
fn reusable_value_outcome_change_after_planning_rejects_before_identity_or_write() {
    let tree = initialized_package("order-adoption-reusable-race");
    let candidate = tree.path().join("Orders/Legacy candidate");
    fs::create_dir(&candidate).unwrap();
    let input =
        OrderAdoptionInput::new("Legacy", "2020-05-06", OrderStatus::Complete).orderer("Ada");
    let plan = plan_order_adoption(tree.path(), &candidate, &input, &RealFileSystem).unwrap();
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::New("Ada".into())
    );
    let intervening_plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Intervening").orderer("ADA"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .unwrap();
    let mut intervening_uuid = FixedUuid::new("3fb13ecf-7e22-4105-ac21-bf547a42e0cf");
    assert!(matches!(
        commit_order_creation(intervening_plan, &mut intervening_uuid, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let mut adoption_uuid = FixedUuid::new("90cb9604-9826-4c8d-b213-224b28117eac");

    let report = commit_order_adoption(plan, &mut adoption_uuid, &RealFileSystem);

    assert!(matches!(
        report,
        MutationReport::Rejected {
            rejection: OrderAdoptionRejection::PreconditionsChanged { .. },
            ..
        }
    ));
    assert_eq!(adoption_uuid.calls, 0);
    assert!(!candidate.join(".spielorders").exists());
    assert!(!candidate.join("README.md").exists());
}

/// Adopts the directory for real, then removes the record it just published before
/// the post-commit refresh reads it — the concurrent external change that leaves an
/// adopted Order absent from the refreshed healthy set.
struct VanishAfterAdopt {
    published: Cell<bool>,
}

impl VanishAfterAdopt {
    fn new() -> Self {
        Self {
            published: Cell::new(false),
        }
    }
}

impl FileSystem for VanishAfterAdopt {
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

impl DirectoryFileSystem for VanishAfterAdopt {
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
        let outcome = RealFileSystem.link_new_file(source, destination)?;
        // The Order record is now published in place, so the adoption really
        // happened. Something outside SpielOrders removes it before the refresh runs.
        if outcome == NewFileLinkOutcome::Linked
            && destination.ends_with(".spielorders/order.json")
            && !self.published.replace(true)
        {
            let _ = std::fs::remove_file(destination);
        }
        Ok(outcome)
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
fn an_adopted_order_that_vanishes_before_the_refresh_still_reports_its_commit() {
    let tree = initialized_package("adoption-vanished-before-refresh");
    let root = tree.path();
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir_all(&directory).unwrap();
    let plan = plan_order_adoption(
        root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the directory can be adopted");

    // The record link that publishes the Order succeeds, so the adoption really
    // happened. Reporting anything but a commit would tell the caller no Order was
    // adopted when one was, and panicking would report nothing at all.
    let report = commit_order_adoption(
        plan,
        &mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        &VanishAfterAdopt::new(),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a published Order that vanished before the refresh is committed: {report:?}");
    };
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "the report carries the Order that was adopted"
    );
    assert_eq!(
        result.canonical_outcome(),
        CanonicalAdoptionOutcome::Adopted
    );
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning, OrderAdoptionWarning::RefreshUnavailable)),
        "and says the refreshed projection could not confirm it: {warnings:?}"
    );
    // The adopted directory really is in place, which is why the commit must be
    // reported rather than the Order treated as never adopted.
    assert!(directory.is_dir());
}
