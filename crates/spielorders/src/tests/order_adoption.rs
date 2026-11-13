//! Public behavior checks for adopting one existing Order Directory in place.
//!
//! Decision sources: Define the SpielOrders package topology; Define the Order record
//! and lifecycle contract; Define legacy Order adoption and metadata bootstrapping;
//! Define integrity, validation, and repair boundaries; Define durable metadata
//! schemas and version evolution.

use crate::tests::support::committed_order;
use crate::tests::support::index_is_current;
use crate::tests::support::{
    Answer, FailedCommit, FixedUuid, Operation, PlatformFileSystem, ScriptedFileSystem, adoption,
    create_order, effects, operation_kinds, package_tree, record_vanishing_after_adoption,
    reusable,
};
use crate::tests::support::{adopted_directory, planned_reuse};
use crate::{Assignments, Report};
use std::ops::Deref;
use std::path::PathBuf;

use crate::metadata::OrderStatus;
use crate::ports::FileSystemEntryKind;
use crate::{Failure, Warning};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

/// Where an adoption is made to fail. Every other effect is real.
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

const ORDER_STAGE: &str = ".spielorders/.order.json.spielorders-stage";
const README_STAGE: &str = ".README.md.spielorders-stage";

fn failing_at(boundary: AdoptionBoundary) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    let (operation, suffix) = match boundary {
        AdoptionBoundary::CreateMetadata => (Operation::CreateDirectory, ".spielorders"),
        AdoptionBoundary::WriteOrder => (Operation::WriteNew, ORDER_STAGE),
        AdoptionBoundary::ReadOrder => (Operation::Read, ORDER_STAGE),
        AdoptionBoundary::SyncOrder => (Operation::SyncFile, ORDER_STAGE),
        AdoptionBoundary::WriteReadme => (Operation::WriteNew, README_STAGE),
        AdoptionBoundary::SyncReadme => (Operation::SyncFile, README_STAGE),
        AdoptionBoundary::CommitReadme => (Operation::LinkNewFile, "README.md"),
        AdoptionBoundary::CommitOrder => (Operation::LinkNewFile, ".spielorders/order.json"),
        AdoptionBoundary::CleanupOrderStage => (Operation::RemoveFile, ORDER_STAGE),
        AdoptionBoundary::IndexReplacement => (Operation::ReplaceFile, ".spielorders/index.json"),
        AdoptionBoundary::CorruptOrder => {
            filesystem
                .on(Operation::Read)
                .ending_in(ORDER_STAGE)
                .once()
                .answers(Answer::Bytes(b"corrupted staged Order".to_vec()));
            return filesystem;
        }
    };
    filesystem.on(operation).ending_in(suffix).once().fails();
    filesystem
}

/// [`failing_at`], with every removal of the Order stage after the failure failing too.
fn with_rollback_failure(boundary: AdoptionBoundary) -> ScriptedFileSystem {
    let filesystem = failing_at(boundary);
    filesystem
        .on(Operation::RemoveFile)
        .ending_in(ORDER_STAGE)
        .after(Operation::LinkNewFile, ".spielorders/order.json")
        .fails();
    filesystem
}

#[test]
fn valid_adoption_preserves_the_directory_and_returns_a_complete_projection() {
    let tree = package_tree("order-adoption-valid");
    let candidate = tree.path().join("Orders/Legacy microscope order");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("README.md"), b"legacy notes\n").unwrap();
    fs::write(candidate.join("quote.pdf"), b"opaque supporting bytes").unwrap();
    let input = (
        Assignments::new()
            .assign("title", "  Microscope objective  ")
            .assign("date_ordered", "2024-02-29")
            .assign("status", "received"),
        vec![
            Assignments::new()
                .assign("description", " Objective ")
                .assign("quantity", "2.0")
                .assign("unit_price", "125.5")
                .assign("catalog_number", " OBJ-10 "),
        ],
    );
    let mut uuids = FixedUuid::new("29dcfef8-cfaa-43f1-bcff-fd54962e3d8d");

    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the physical direct child and explicit facts are eligible");

    assert_eq!(
        adopted_directory(&plan),
        fs::canonicalize(&candidate).unwrap()
    );
    assert_eq!(
        fs::read(candidate.join("README.md")).unwrap(),
        b"legacy notes\n"
    );
    assert_eq!(
        fs::read(candidate.join("quote.pdf")).unwrap(),
        b"opaque supporting bytes"
    );

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid adoption must commit")
    };
    assert_eq!(
        committed_order(&result).document.order_id.deref(),
        uuids.value()
    );
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Microscope objective"
    );
    assert_eq!(
        committed_order(&result).document.status,
        OrderStatus::Received
    );
    assert_eq!(committed_order(&result).item_totals, &[251.0]);
    assert_eq!(committed_order(&result).order_total, 251.0);
    assert_eq!(
        committed_order(&result).readme_text.as_deref(),
        Some("legacy notes\n")
    );
    assert_eq!(
        committed_order(&result).paths.directory.absolute_path,
        fs::canonicalize(&candidate).unwrap()
    );
    assert!(index_is_current(tree.path()));
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
    let tree = package_tree("order-adoption-plan");
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
    let input = (
        Assignments::new()
            .assign("title", " Vacuum parts ")
            .assign("date_ordered", "2020-01-02")
            .assign("status", "hold")
            .assign("vendor", "Nethermoor")
            .assign("orderer", " Ada ")
            .assign("ordering_institution", " Example Institute ")
            .assign("currency", "USD")
            .assign("project", " Clock ")
            .assign("order_group", " Legacy "),
        vec![
            Assignments::new()
                .assign("description", " Flange ")
                .assign("quantity", "-2.0")
                .assign("unit_price", "3.5"),
        ],
    );

    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("explicit valid facts can be planned");

    assert_eq!(planned_reuse(&plan, "orderer"), reusable("new", "Ada"));
    assert_eq!(
        operation_kinds(&plan),
        [
            "create_metadata_directory",
            "stage_order_record",
            "stage_readme",
            "commit_readme",
            "commit_order_record",
            "remove_staging",
            "remove_staging",
            "refresh_package_index",
        ]
    );
    assert!(!candidate.join(".spielorders").exists());
    assert!(!candidate.join("README.md").exists());
}

#[test]
fn only_a_non_hidden_physical_ordinary_direct_child_is_eligible() {
    let tree = package_tree("order-adoption-eligibility");
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
    let input = (
        Assignments::new()
            .assign("title", "Explicit")
            .assign("date_ordered", "2026-08-26")
            .assign("status", "complete"),
        Vec::new(),
    );

    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &hidden, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::HiddenCandidate { .. })
    ));
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &nested, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::NotDirectChild { .. })
    ));
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &link, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::IneligibleCandidate {
            kind: Some(FileSystemEntryKind::SymbolicLink),
            ..
        })
    ));
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &file, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::IneligibleCandidate {
            kind: Some(FileSystemEntryKind::File),
            ..
        })
    ));
}

#[test]
fn managed_partial_invalid_and_damaged_candidates_reject_before_identity_or_write() {
    let tree = package_tree("order-adoption-rejections");
    let orders = tree.path().join("Orders");
    let managed = orders.join("Managed");
    fs::create_dir(&managed).unwrap();
    let input = (
        Assignments::new()
            .assign("title", "Managed")
            .assign("date_ordered", "2024-01-01")
            .assign("status", "ordered"),
        Vec::new(),
    );
    let plan = crate::plan(
        &adoption(tree.path(), &managed, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let mut committed_uuid = FixedUuid::new("78ee964a-f03c-43f1-acbc-149ece5cfc09");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(&mut committed_uuid)
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &managed, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::AlreadyAnOrder { .. })
    ));

    let partial = orders.join("Partial");
    fs::create_dir(&partial).unwrap();
    fs::create_dir(partial.join(".spielorders")).unwrap();
    fs::write(partial.join(".spielorders/unknown"), b"reserved").unwrap();
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &partial, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::ConflictingReservedState { .. })
    ));

    let invalid = orders.join("Invalid");
    fs::create_dir(&invalid).unwrap();
    assert!(matches!(
        crate::plan(
            &adoption(
                tree.path(),
                invalid.clone(),
                &(
                    Assignments::new()
                        .assign("title", " ")
                        .assign("date_ordered", "2024-02-30")
                        .assign("status", "hold"),
                    vec![
                        Assignments::new()
                            .assign("description", "")
                            .assign("quantity", f64::NAN.to_string())
                            .assign("unit_price", "1.0")
                    ]
                )
            ),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::InvalidAssignments(_))
    ));
    assert!(matches!(
        crate::plan(
            &adoption(
                tree.path(),
                &invalid,
                &(
                    input.0.clone().assign("vendor", "Nonesuch Optics"),
                    input.1.clone()
                )
            ),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::UnknownVendor { .. })
    ));

    fs::write(tree.path().join(".spielorders/vendors.json"), b"not json").unwrap();
    assert!(matches!(
        crate::plan(
            &adoption(tree.path(), &invalid, &input),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::DamagedPackage(_))
    ));
    assert!(!invalid.join(".spielorders").exists());
}

#[test]
fn replacing_the_selected_directory_after_planning_rejects_without_adopting_the_replacement() {
    let tree = package_tree("order-adoption-replaced-candidate");
    let candidate = tree.path().join("Orders/Selected");
    fs::create_dir(&candidate).unwrap();
    let input = (
        Assignments::new()
            .assign("title", "Selected")
            .assign("date_ordered", "2024-01-01")
            .assign("status", "complete"),
        Vec::new(),
    );
    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    fs::remove_dir(&candidate).unwrap();
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("replacement.txt"), b"different directory").unwrap();
    let mut uuids = FixedUuid::new("236794d8-7897-489f-9206-247c7d3f2892");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuids)).result;

    assert!(matches!(report, Err(Failure::PreconditionsChanged { .. })));
    assert!(!candidate.join(".spielorders").exists());
    assert_eq!(
        fs::read(candidate.join("replacement.txt")).unwrap(),
        b"different directory"
    );
}

#[test]
fn absent_readme_is_created_empty_and_generated_identity_collisions_do_not_mutate_candidate() {
    let tree = package_tree("order-adoption-readme-id-collision");
    let existing_plan = crate::plan(
        &create_order(tree.path(), Assignments::new().assign("title", "Existing")),
        &effects(&PlatformFileSystem).on(2026, 8, 26),
    )
    .result
    .unwrap();
    let colliding_id = "86ec2b61-c96a-4fca-9275-39479923f56b";
    let mut existing_uuid = FixedUuid::new(colliding_id);
    assert!(matches!(
        crate::commit(
            existing_plan,
            &effects(&PlatformFileSystem).minting(&mut existing_uuid)
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let candidate = tree.path().join("Orders/Legacy without readme");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("support.bin"), b"keep me").unwrap();
    let input = (
        Assignments::new()
            .assign("title", "Legacy")
            .assign("date_ordered", "2020-05-06")
            .assign("status", "complete"),
        Vec::new(),
    );
    let collision_plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let mut collision_uuid = FixedUuid::new(colliding_id);

    assert!(matches!(
        crate::commit(
            collision_plan,
            &effects(&PlatformFileSystem).minting(&mut collision_uuid)
        )
        .result,
        Err(Failure::GeneratedIdentityCollision { .. })
    ));
    assert!(!candidate.join("README.md").exists());
    assert!(!candidate.join(".spielorders").exists());
    assert_eq!(fs::read(candidate.join("support.bin")).unwrap(), b"keep me");

    let commit_plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let mut unique_uuid = FixedUuid::new("edcf2a2d-4e78-44da-aab8-19399c382da0");
    assert!(matches!(
        crate::commit(
            commit_plan,
            &effects(&PlatformFileSystem).minting(&mut unique_uuid)
        )
        .result,
        Ok(Report::Committed { .. })
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
    let tree = package_tree("order-adoption-opaque-evidence");
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
    let scripted = ScriptedFileSystem::new();
    let input = (
        Assignments::new()
            .assign("title", "Opaque")
            .assign("date_ordered", "2019-08-22")
            .assign("status", "ordered"),
        Vec::new(),
    );
    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&scripted),
    )
    .result
    .unwrap();
    let canonical_candidate = adopted_directory(&plan);
    let mut uuids = FixedUuid::new("b86004b3-b753-47bf-88fd-5afe70596a12");

    assert!(matches!(
        crate::commit(plan, &effects(&scripted).minting(&mut uuids)).result,
        Ok(Report::Committed { .. })
    ));

    assert!(
        scripted
            .paths(Operation::ReadDirectory)
            .iter()
            .all(|path| path != &canonical_candidate)
    );
    let allowed_reads = [
        canonical_candidate.join("README.md"),
        canonical_candidate.join(".spielorders/order.json"),
        canonical_candidate.join(".spielorders/.order.json.spielorders-stage"),
    ];
    assert!(
        scripted.paths(Operation::Read).iter().all(|path| {
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
        let tree = package_tree(&format!("order-adoption-fault-{boundary:?}"));
        let candidate = tree.path().join("Orders/Candidate");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("support.bin"), b"unchanged").unwrap();
        let input = (
            Assignments::new()
                .assign("title", "Candidate")
                .assign("date_ordered", "2025-06-07")
                .assign("status", "received"),
            Vec::new(),
        );
        let plan = crate::plan(
            &adoption(tree.path(), &candidate, &input),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();
        let mut uuids = FixedUuid::new("58fc4a94-b3f7-41b2-b2a0-f81f552305f1");

        let report =
            crate::commit(plan, &effects(&failing_at(boundary)).minting(&mut uuids)).result;

        assert!(
            report.is_err(),
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
    let tree = package_tree("order-adoption-rollback-failure");
    let candidate = tree.path().join("Orders/Candidate");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("support.bin"), b"preexisting").unwrap();
    let input = (
        Assignments::new()
            .assign("title", "Candidate")
            .assign("date_ordered", "2025-06-07")
            .assign("status", "received"),
        Vec::new(),
    );
    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let mut uuids = FixedUuid::new("15d847c1-4f27-4765-9d31-46c8d59c79f2");

    let report = crate::commit(
        plan,
        &effects(&with_rollback_failure(AdoptionBoundary::CommitOrder)).minting(&mut uuids),
    )
    .result;

    let Err(failed) = report else {
        panic!("failed rollback must remain a typed rejection")
    };
    let rollback_failures = failed.cleanup_failures();
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
        let tree = package_tree(&format!("order-adoption-warning-{boundary:?}"));
        let candidate = tree.path().join("Orders/Candidate");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("README.md"), b"preserved").unwrap();
        let input = (
            Assignments::new()
                .assign("title", "Candidate")
                .assign("date_ordered", "2025-06-07")
                .assign("status", "received"),
            Vec::new(),
        );
        let plan = crate::plan(
            &adoption(tree.path(), &candidate, &input),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();
        let mut uuids = FixedUuid::new("791f8208-a7a3-498b-a4fa-3608650270bd");

        let report =
            crate::commit(plan, &effects(&failing_at(boundary)).minting(&mut uuids)).result;

        let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
            panic!("{boundary:?} follows canonical commit and must warn")
        };
        assert!(candidate.join(".spielorders/order.json").exists());
        assert_eq!(fs::read(candidate.join("README.md")).unwrap(), b"preserved");
        match boundary {
            AdoptionBoundary::CleanupOrderStage => assert!(
                warnings
                    .iter()
                    .any(|warning| { matches!(warning, Warning::StagingCleanupFailed(_)) })
            ),
            AdoptionBoundary::IndexReplacement => {
                assert!(
                    warnings
                        .iter()
                        .any(|warning| { matches!(warning, Warning::IndexRefreshFailed(_)) })
                )
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            crate::plan(
                &adoption(tree.path(), &candidate, &input),
                &effects(&PlatformFileSystem)
            )
            .result,
            Err(Failure::AlreadyAnOrder { .. })
        ));
    }
}

#[test]
fn reusable_value_outcome_change_after_planning_rejects_before_identity_or_write() {
    let tree = package_tree("order-adoption-reusable-race");
    let candidate = tree.path().join("Orders/Legacy candidate");
    fs::create_dir(&candidate).unwrap();
    let input = (
        Assignments::new()
            .assign("title", "Legacy")
            .assign("date_ordered", "2020-05-06")
            .assign("status", "complete")
            .assign("orderer", "Ada"),
        Vec::new(),
    );
    let plan = crate::plan(
        &adoption(tree.path(), &candidate, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert_eq!(planned_reuse(&plan, "orderer"), reusable("new", "Ada"));
    let intervening_plan = crate::plan(
        &create_order(
            tree.path(),
            Assignments::new()
                .assign("title", "Intervening")
                .assign("orderer", "ADA"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 26),
    )
    .result
    .unwrap();
    let mut intervening_uuid = FixedUuid::new("3fb13ecf-7e22-4105-ac21-bf547a42e0cf");
    assert!(matches!(
        crate::commit(
            intervening_plan,
            &effects(&PlatformFileSystem).minting(&mut intervening_uuid)
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let mut adoption_uuid = FixedUuid::new("90cb9604-9826-4c8d-b213-224b28117eac");

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(&mut adoption_uuid),
    )
    .result;

    assert!(matches!(report, Err(Failure::PreconditionsChanged { .. })));
    assert!(!candidate.join(".spielorders").exists());
    assert!(!candidate.join("README.md").exists());
}

#[test]
fn an_adopted_order_that_vanishes_before_the_refresh_still_reports_its_commit() {
    let tree = package_tree("adoption-vanished-before-refresh");
    let root = tree.path();
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir_all(&directory).unwrap();
    let plan = crate::plan(
        &adoption(
            PathBuf::from(root),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the directory can be adopted");

    // The record link that publishes the Order succeeds, so the adoption really
    // happened. Reporting anything but a commit would tell the caller no Order was
    // adopted when one was, and panicking would report nothing at all.
    let report = crate::commit(
        plan,
        &effects(&{
            let filesystem = ScriptedFileSystem::new();
            record_vanishing_after_adoption(&filesystem);
            filesystem
        })
        .minting(&mut FixedUuid::new("8b6e3f0a-8ec5-4d47-95e1-412652a56d16")),
    )
    .result;

    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("a published Order that vanished before the refresh is committed: {report:?}");
    };
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning, Warning::RefreshUnavailable)),
        "and says the refreshed projection could not confirm it: {warnings:?}"
    );
    // The adopted directory really is in place, which is why the commit must be
    // reported rather than the Order treated as never adopted.
    assert!(directory.is_dir());
}
