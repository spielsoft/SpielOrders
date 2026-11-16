//! Decision sources: Define the SpielOrders package topology; Define the Order
//! record and lifecycle contract; Define the Rust CLI capability contract;
//! Define integrity, validation, and repair boundaries; Define durable metadata
//! schemas and version evolution.

use crate::ChangeResult;
use crate::metadata::OrderStatus;
use crate::tests::support::committed_order;
use crate::tests::support::index_is_current;
use crate::tests::support::{
    Answer, FailedCommit, FixedUuid, Operation, PlatformFileSystem, ScriptedFileSystem,
    create_order, effects, json, opened, operation_kinds, package_tree, reusable,
};
use crate::tests::support::{operation_path, planned_order, planned_reuse};
use crate::{Assignments, Report};
use crate::{Failure, Warning};
use crate::{evidence::EvidenceOutcome, evidence::FactChange};
use std::fs;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;

/// Where a creation is made to fail. Every other effect is real.
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
    /// Something outside SpielOrders takes the name after planning found it free.
    DestinationAppearsBeforeCommit,
    ObserveIndexAfterCommit,
}

const ORDER_RECORD: &str = ".spielorders/order.json";

fn failing_at(boundary: CreationBoundary) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    let script = match boundary {
        CreationBoundary::CreateStagingDirectory => filesystem.on(Operation::CreateDirectory),
        CreationBoundary::CreateMetadataDirectory => {
            filesystem.on(Operation::CreateDirectory).skipping(1)
        }
        CreationBoundary::WriteOrderRecord => {
            filesystem.on(Operation::WriteNew).ending_in(ORDER_RECORD)
        }
        CreationBoundary::ReadOrderRecord => filesystem.on(Operation::Read).ending_in(ORDER_RECORD),
        CreationBoundary::SyncOrderRecord => {
            filesystem.on(Operation::SyncFile).ending_in(ORDER_RECORD)
        }
        CreationBoundary::WriteReadme => filesystem.on(Operation::WriteNew).ending_in("README.md"),
        CreationBoundary::SyncReadme => filesystem.on(Operation::SyncFile).ending_in("README.md"),
        CreationBoundary::CommitDirectory => filesystem.on(Operation::RenameNew),
        CreationBoundary::CorruptOrderRecord => {
            filesystem
                .on(Operation::Read)
                .ending_in(ORDER_RECORD)
                .once()
                .answers(Answer::Bytes(b"corrupted candidate".to_vec()));
            return filesystem;
        }
        CreationBoundary::DestinationAppearsBeforeCommit => {
            // Delegating after the outside directory lands leaves the collision for
            // the adapter to detect, so no answer here is one it would not give.
            filesystem
                .on(Operation::RenameNew)
                .once()
                .preceded_by(|outside| outside.create_directory(&outside.call().path))
                .proceeds();
            return filesystem;
        }
        CreationBoundary::ObserveIndexAfterCommit => {
            filesystem
                .on(Operation::SymlinkMetadata)
                .ending_in(".spielorders/index.json")
                .after_any(Operation::RenameNew)
                .fails();
            return filesystem;
        }
    };
    script.once().fails();
    filesystem
}

/// [`failing_at`], with the cleanup of the staged file ending in `suffix` failing too.
fn with_cleanup_failure(boundary: CreationBoundary, suffix: &str) -> ScriptedFileSystem {
    let filesystem = failing_at(boundary);
    filesystem
        .on(Operation::RemoveFile)
        .ending_in(suffix)
        .once()
        .fails();
    filesystem
}

fn failing_index_replacement() -> ScriptedFileSystem {
    ScriptedFileSystem::new().failing_ending_in(Operation::ReplaceFile, ".spielorders/index.json")
}

/// Something outside SpielOrders removes a created Order's record the moment the
/// Order is published, before the refresh runs, so the refreshed Package reports it
/// damaged rather than healthy.
fn vanishing_after_publish(filesystem: ScriptedFileSystem) -> ScriptedFileSystem {
    filesystem
        .on(Operation::RenameNew)
        .once()
        .followed_by(|outside| outside.remove_file(&outside.call().path.join(ORDER_RECORD)))
        .proceeds();
    filesystem
}

#[test]
fn title_only_plan_is_complete_and_side_effect_free_without_generating_identity() {
    let tree = package_tree("order-creation-default-plan");
    let root = tree.path();
    let before = fs::read_dir(root.join("Orders")).unwrap().count();

    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "  Laser components  "),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("a title-only Order can be planned");

    assert_eq!(json(planned_order(&plan))["title"], "Laser components");
    assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-08-22");
    assert_eq!(
        json(planned_order(&plan))["status"],
        json(&OrderStatus::InPreparation),
    );
    assert_eq!(
        operation_kinds(&plan),
        [
            "create_staging_directory",
            "create_metadata_directory",
            "write_order_record",
            "write_readme",
            "commit_directory",
            "refresh_package_index",
        ]
    );
    assert_eq!(fs::read_dir(root.join("Orders")).unwrap().count(), before);
}

#[test]
fn fully_supplied_plan_resolves_vendor_and_reusable_values_into_normalized_facts() {
    let tree = package_tree("order-creation-full-plan");
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
    let input = Assignments::new()
        .assign("title", " Laser / components ")
        .assign("date_ordered", "2026-07-19")
        .assign("vendor", "Tarnwick")
        .introduce("orderer", " Ada Lovelace ")
        .introduce("ordering_institution", " Example Institute ")
        .assign("currency", "USD")
        .introduce("project", " Telescope ")
        .introduce("order_group", " Summer build ");

    let plan = crate::plan(
        &create_order(PathBuf::from(root), input.clone()),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("all supplied facts are valid");

    assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-07-19");
    assert_eq!(
        json(planned_order(&plan))["vendor_id"],
        json(&Some("e0990d31-06aa-42e9-97c8-088e3e894822")),
    );
    assert_eq!(
        json(planned_order(&plan))["orderer"],
        json(&Some("Ada Lovelace"))
    );
    assert_eq!(
        json(planned_order(&plan))["ordering_institution"],
        json(&Some("Example Institute")),
    );
    assert_eq!(json(planned_order(&plan))["currency"], json(&Some("USD")));
    assert_eq!(
        json(planned_order(&plan))["project"],
        json(&Some("Telescope")),
    );
    assert_eq!(
        json(planned_order(&plan))["order_group"],
        json(&Some("Summer build")),
    );
    assert_eq!(
        planned_reuse(&plan, "orderer"),
        reusable("new", "Ada Lovelace")
    );
    assert_eq!(
        operation_path(&plan, "commit_directory")
            .file_name()
            .unwrap(),
        "2026_07_19_Tarnwick_Laser_components_Ada_Lovelace"
    );
}

#[test]
fn valid_commit_creates_one_complete_order_and_returns_the_refreshed_projection() {
    let tree = package_tree("order-creation-commit");
    let root = tree.path();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid Order creation must commit")
    };
    assert_eq!(
        committed_order(&result).document.order_id.deref(),
        uuids.value()
    );
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser components"
    );
    assert_eq!(
        committed_order(&result).document.date_ordered.deref(),
        "2026-08-22"
    );
    assert_eq!(
        committed_order(&result).document.status,
        OrderStatus::InPreparation
    );
    assert!(committed_order(&result).document.items.is_empty());
    assert!(committed_order(&result).item_totals.is_empty());
    assert_eq!(committed_order(&result).order_total, 0.0);
    // Creation writes the conventional README itself, empty, so the created Order's
    // text is known to be empty rather than merely unknown.
    assert_eq!(committed_order(&result).readme_text.as_deref(), Some(""));
    assert_eq!(
        committed_order(&result)
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert!(index_is_current(root));

    let directory = root.join("Orders/2026_08_22_Laser_components");
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), b"");
    let order_bytes = fs::read(directory.join(".spielorders/order.json")).unwrap();
    assert!(std::str::from_utf8(&order_bytes).unwrap().ends_with('\n'));
    assert!(
        !plan_staging_entries(root)
            .iter()
            .any(|name| name.starts_with('.'))
    );
    let Ok(package) = opened(root, &PlatformFileSystem).result else {
        panic!("the committed Package must load as available")
    };
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert!(package.contents.damaged_orders.is_empty());
}

#[test]
fn invalid_facts_and_unknown_vendor_reject_during_planning_without_creation() {
    let tree = package_tree("order-creation-invalid-plan");
    let root = tree.path();
    let inputs = [
        Assignments::new().assign("title", "   "),
        Assignments::new()
            .assign("title", "Bad date")
            .assign("date_ordered", "2026-02-30"),
        Assignments::new()
            .assign("title", "Bad currency")
            .assign("currency", "usd"),
        Assignments::new()
            .assign("title", "Unknown vendor")
            .assign("vendor", "Tarnwick"),
    ];

    for input in inputs {
        assert!(
            crate::plan(
                &create_order(PathBuf::from(root), input.clone()),
                &effects(&PlatformFileSystem).on(2026, 8, 22)
            )
            .result
            .is_err(),
            "invalid or unresolved facts must reject"
        );
        assert!(plan_staging_entries(root).is_empty());
    }
}

#[test]
fn failed_index_refresh_keeps_the_order_committed_and_makes_retry_collide() {
    let tree = package_tree("order-creation-index-warning");
    let root = tree.path();
    let prior_index = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&failing_index_replacement()).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(
        plan,
        &effects(&failing_index_replacement()).minting(&mut uuids),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("index replacement failure must not reject canonical creation")
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    // The canonical scan behind the failed replacement did see the Order, so the
    // report carries the complete Order that scan established.
    assert_eq!(
        committed_order(&result).document.order_id.deref(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser components"
    );
    assert_eq!(
        committed_order(&result)
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(committed_order(&result).readme_text.as_deref(), Some(""));
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
        crate::plan(
            &create_order(
                PathBuf::from(root),
                Assignments::new().assign("title", "Laser components")
            ),
            &effects(&failing_index_replacement()).on(2026, 8, 22)
        )
        .result
        .is_err(),
        "retry is rejected by the committed basename"
    );
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
        let tree = package_tree("order-creation-precommit-failure");
        let root = tree.path();
        let filesystem = failing_at(boundary);
        let plan = crate::plan(
            &create_order(
                PathBuf::from(root),
                Assignments::new().assign("title", "Laser components"),
            ),
            &effects(&filesystem).on(2026, 8, 22),
        )
        .result
        .unwrap();
        let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

        let report = crate::commit(plan, &effects(&filesystem).minting(&mut uuids)).result;

        assert!(
            report.is_err(),
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
    let tree = package_tree("order-creation-collisions");
    let root = tree.path();
    let first = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "First order"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut first_uuid = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");
    assert!(matches!(
        crate::commit(
            first,
            &effects(&PlatformFileSystem).minting(&mut first_uuid)
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    assert!(
        crate::plan(
            &create_order(
                PathBuf::from(root),
                Assignments::new().assign("title", "First order")
            ),
            &effects(&PlatformFileSystem).on(2026, 8, 22)
        )
        .result
        .is_err(),
        "an existing projected basename is never suffixed"
    );

    let second = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Second order"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut duplicate_uuid = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");
    let report = crate::commit(
        second,
        &effects(&PlatformFileSystem).minting(&mut duplicate_uuid),
    )
    .result;
    assert!(matches!(
        report,
        Err(Failure::GeneratedIdentityCollision { .. })
    ));
    assert_eq!(plan_staging_entries(root).len(), 1);
    assert!(!root.join("Orders/2026_08_22_Second_order").exists());
}

#[test]
fn damaged_package_rejects_creation_without_consuming_identity_or_creating_paths() {
    let tree = package_tree("order-creation-damaged-package");
    let root = tree.path();
    fs::write(root.join(".spielorders/vendors.json"), b"{ malformed").unwrap();

    assert!(
        crate::plan(
            &create_order(
                PathBuf::from(root),
                Assignments::new().assign("title", "Never created")
            ),
            &effects(&PlatformFileSystem).on(2026, 8, 22)
        )
        .result
        .is_err()
    );
    assert!(plan_staging_entries(root).is_empty());
}

#[test]
fn existing_leading_and_trailing_underscores_survive_projection_normalization() {
    let tree = package_tree("order-creation-underscore-projection");
    let plan = crate::plan(
        &create_order(tree.path(), Assignments::new().assign("title", "__")),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("underscores are valid title content");

    assert_eq!(
        operation_path(&plan, "commit_directory")
            .file_name()
            .unwrap(),
        "2026_08_22__"
    );
}

#[test]
fn candidate_corruption_retains_any_failed_cleanup_evidence() {
    let tree = package_tree("order-creation-corruption-cleanup");
    let filesystem = with_cleanup_failure(
        CreationBoundary::CorruptOrderRecord,
        ".spielorders/order.json",
    );
    let plan = crate::plan(
        &create_order(
            tree.path(),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&filesystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&filesystem).minting(&mut uuids)).result;

    let Err(failed) = report else {
        panic!("candidate corruption is a typed commit failure")
    };
    let cleanup_failures = failed.cleanup_failures();
    assert!(!cleanup_failures.is_empty());
}

#[test]
fn late_destination_collision_preserves_concurrent_evidence_and_cleanup_failures() {
    let tree = package_tree("order-creation-late-destination");
    let filesystem = with_cleanup_failure(
        CreationBoundary::DestinationAppearsBeforeCommit,
        "README.md",
    );
    let plan = crate::plan(
        &create_order(
            tree.path(),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&filesystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let destination = operation_path(&plan, "commit_directory");
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&filesystem).minting(&mut uuids)).result;

    let Err(failed) = report else {
        panic!("late destination collision is a typed commit rejection")
    };
    let cleanup_failures = failed.cleanup_failures();
    assert!(!cleanup_failures.is_empty());
    assert!(destination.is_dir(), "concurrent destination is preserved");
}

#[test]
fn postcommit_index_assessment_failure_retains_typed_warning_and_stale_evidence() {
    let tree = package_tree("order-creation-index-assessment-warning");
    let filesystem = failing_at(CreationBoundary::ObserveIndexAfterCommit);
    let plan = crate::plan(
        &create_order(
            tree.path(),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&filesystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&filesystem).minting(&mut uuids)).result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("postcommit index assessment failure keeps canonical success")
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    assert_eq!(
        committed_order(&result).document.order_id.deref(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser components"
    );
    assert_eq!(
        committed_order(&result)
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/2026_08_22_Laser_components")
    );
    assert_eq!(committed_order(&result).readme_text.as_deref(), Some(""));
}

#[test]
fn incomplete_precommit_cleanup_is_explicit_and_never_exposes_a_canonical_order() {
    let tree = package_tree("order-creation-incomplete-cleanup");
    let root = tree.path();
    let filesystem = with_cleanup_failure(CreationBoundary::CommitDirectory, "README.md");
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&filesystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    let staging = operation_path(&plan, "create_staging_directory");
    let final_directory = operation_path(&plan, "commit_directory");
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&filesystem).minting(&mut uuids)).result;

    let Err(failed) = report else {
        panic!("the commit and cleanup failures remain distinguishable")
    };
    let cleanup_failures = failed.cleanup_failures();
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
fn unconfirmed_facts(report: &Result<Report, Failure>) -> Vec<(&str, FactChange)> {
    let Err(Failure::PlanEvidenceUnconfirmed { outcome }) = report else {
        panic!("a plan whose evidence moved is refused before any effect: {report:?}");
    };
    let EvidenceOutcome::Changed(stale) = outcome else {
        panic!("the refusal names the evidence that changed: {outcome:?}");
    };
    stale
        .changed
        .iter()
        .map(|fact| (fact.name.as_str(), fact.change))
        .collect()
}

#[test]
fn a_commit_is_refused_when_the_package_no_longer_configures_the_orders_directory_it_planned_for() {
    let tree = package_tree("order-creation-reconfigured-orders");
    let root = tree.path();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Order can be planned against the configured Orders Directory");
    let planned_directory = operation_path(&plan, "commit_directory");

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

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

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
}

#[test]
fn a_commit_is_refused_when_the_vendor_its_directory_name_spells_was_renamed() {
    let tree = package_tree("order-creation-renamed-vendor");
    let root = tree.path();
    write_one_vendor(root, "Tarnwick");
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new()
                .assign("title", "Laser components")
                .assign("vendor", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Vendor resolves while planning");
    let planned_directory = operation_path(&plan, "commit_directory");
    assert_eq!(
        planned_directory.file_name().unwrap(),
        "2026_08_22_Tarnwick_Laser_components",
        "the projected directory spells the Vendor's name, which is what makes it stale"
    );

    write_one_vendor(root, "Nethermoor");
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [("vendor_name", FactChange::Differs)],
        "the refusal names the Vendor whose name the directory spells"
    );
    assert!(
        !planned_directory.exists(),
        "no Order is published under the Vendor's former name"
    );
}

#[test]
fn a_commit_is_refused_when_the_vendor_it_resolved_was_deleted() {
    let tree = package_tree("order-creation-deleted-vendor");
    let root = tree.path();
    write_one_vendor(root, "Tarnwick");
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new()
                .assign("title", "Laser components")
                .assign("vendor", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Vendor resolves while planning");
    let planned_directory = operation_path(&plan, "commit_directory");

    fs::write(
        root.join(".spielorders/vendors.json"),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": []\n}\n",
    )
    .unwrap();
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [("vendor_name", FactChange::Vanished)],
        "a Vendor that is gone is reported as vanished rather than merely different"
    );
    assert!(!planned_directory.exists());
}

#[test]
fn a_commit_is_refused_when_a_value_it_reported_as_new_became_reusable() {
    let tree = package_tree("order-creation-reusable-appeared");
    let root = tree.path();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new()
                .assign("title", "Laser components")
                .introduce("orderer", "Ada Lovelace"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Order can be planned");
    assert_eq!(
        planned_reuse(&plan, "orderer"),
        reusable("new", "Ada Lovelace"),
        "the plan told the caller this Orderer was new to the Package"
    );
    let planned_directory = operation_path(&plan, "commit_directory");

    // Another Order introduces the same Orderer under the Package's own spelling, so
    // the reusable-value outcome this plan would report is no longer true of the
    // Package it is about to be committed into.
    let other = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new()
                .assign("title", "Mounts")
                .introduce("orderer", "ada lovelace"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(
            other,
            &effects(&PlatformFileSystem)
                .minting(&mut FixedUuid::new("1f2e3d4c-5b6a-4798-8877-665544332211"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let mut uuids = FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    assert_eq!(
        unconfirmed_facts(&report),
        [("orderer", FactChange::Differs)],
        "the refusal names the reusable value whose canonical spelling moved"
    );
    assert!(!planned_directory.exists());
}

#[test]
fn an_order_that_vanishes_before_the_refresh_still_reports_its_commit() {
    let tree = package_tree("creation-vanished-before-refresh");
    let root = tree.path();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Order can be planned");
    let directory = operation_path(&plan, "commit_directory");

    // The directory rename that publishes the Order succeeds, so the creation really
    // happened. Reporting it as anything but committed would tell the caller no Order
    // was created when one was, and panicking would report nothing at all.
    let report = crate::commit(
        plan,
        &effects(&vanishing_after_publish(ScriptedFileSystem::new()))
            .minting(&mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16")),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a published Order that vanished before the refresh is committed: {report:?}");
    };
    assert!(
        warnings.contains(&Warning::RefreshUnavailable),
        "and says the refreshed projection could not confirm it: {warnings:?}"
    );
    assert_eq!(
        variant!(&result, ChangeResult::CreateOrder)
            .directory
            .absolute_path,
        directory,
        "the report names the directory this creation published"
    );
    // The directory really is on disk, which is why the commit must be reported.
    assert!(directory.is_dir());
}

#[test]
fn a_vanished_order_and_a_failed_index_replacement_keep_the_assessed_evidence() {
    let tree = package_tree("creation-vanished-and-replacement-failed");
    let root = tree.path();
    let prior_index = fs::read(root.join(".spielorders/index.json")).unwrap();
    let plan = crate::plan(
        &create_order(
            PathBuf::from(root),
            Assignments::new().assign("title", "Laser components"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("the Order can be planned");

    // Both post-commit misfortunes at once: the published Order is gone before the
    // refresh reads it, and the rebuilt index cannot be written either.
    let report = crate::commit(
        plan,
        &effects(&vanishing_after_publish(failing_index_replacement()))
            .minting(&mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16")),
    )
    .result;

    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the published Order is still committed: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        prior_index,
        "and the failed replacement left the stored index untouched"
    );
}
