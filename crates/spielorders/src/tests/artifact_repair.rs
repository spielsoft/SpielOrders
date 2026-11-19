//! Diagnosing and applying the repairs of one managed artifact, at Package scope and at
//! Order scope.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::tests::support::operation_path;
use crate::tests::support::{
    Answer, EditPoint, FixedUuid, Operation, PlatformFileSystem, QueuedUuids, ScriptedFileSystem,
    TemporaryTree, adopted_order, adoption, another_writer_replacing, diagnosed_handle,
    diagnosed_order, diagnosed_package, effects, findings, initialized_package, operation_kinds,
    order_scope, package_scope, record_path, repair, valid,
};
use crate::{Assignments, Change, Plan, Report};
use std::ops::Deref;

use crate::repair::PackageConstraint;
use crate::repair_cases::RepairAction;
use crate::{Failure, Warning};
use crate::{readme::DiagnosticCode, readme::Severity};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
/// The identity of a second Order Directory in the same Package.
const OTHER_ORDER_ID: &str = "3f1b0c52-4d8a-4f2e-9b77-6c1e5d0a9f34";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const STAGE: &str = ".spielorders/.order.json.spielorders-stage";

fn package_with_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    package_with_order_at(label, "Orders")
}

/// The fixture Package with its Orders at the configured path, so a repair can be
/// exercised under a layout its Package root cannot be guessed from.
fn package_with_order_at(label: &str, orders_path: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = crate::plan(
        &Change::InitializePackage {
            root: tree.path().into(),
            orders_path: Some(String::from(orders_path)),
            removed_path: Some(String::from("Removed")),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = root.join(orders_path).join("2026_08_22_Laser_components");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("README.md"), README).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    let plan = crate::plan(
        &adoption(
            root.clone(),
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
    .expect("the fixture directory can be adopted");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(FixedUuid(ORDER_ID.into()))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    (tree, root, directory)
}

/// A complete, fully valid replacement record for the fixture Order.
fn valid_replacement() -> Vec<u8> {
    valid_replacement_with("../..", ORDER_ID)
}

/// The same replacement carrying a chosen technical locator and Order identity, for
/// the Package-scoped constraints a standalone-valid record can still break.
fn valid_replacement_with(package_root: &str, order_id: &str) -> Vec<u8> {
    format!(
        "{{\n  \"schema_version\": 1,\n  \"package_root\": \"{package_root}\",\n  \"order_id\": \"{order_id}\",\n  \"title\": \"Repaired components\",\n  \"date_ordered\": \"2026-08-22\",\n  \"status\": \"hold\",\n  \"items\": []\n}}\n"
    )
    .into_bytes()
}

fn damage_shape(directory: &Path) {
    fs::write(
        record_path(directory),
        br#"{"schema_version":1,"nope":true}"#,
    )
    .unwrap();
}

#[test]
fn a_case_authorizes_only_the_actions_its_diagnosis_offers() {
    // The invalid record's case offers only its replacement.
    let (_tree, _root, directory) = package_with_order("repair-mapping");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    for resolution in [
        (
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        (RepairAction::PromoteStagingCandidate, Assignments::new()),
        (
            RepairAction::AssignExistingVendor,
            Assignments::new().assign("vendor", "Tarnwick"),
        ),
        (
            RepairAction::CreateAndAssignVendor,
            Assignments::new().assign("name", "Nethermoor"),
        ),
        (RepairAction::ClearVendorReference, Assignments::new()),
        (RepairAction::AdoptNewOrderIdentity, Assignments::new()),
        (RepairAction::RepairPackageLocator, Assignments::new()),
    ] {
        let outcome = crate::plan(
            &repair(
                order_scope(&directory),
                &handle,
                resolution.0,
                resolution.1.clone(),
            ),
            &effects(&PlatformFileSystem),
        );
        match resolution.0 {
            RepairAction::ReplaceManagedRecord => assert!(outcome.result.is_ok(), "{outcome:?}"),
            action => assert!(
                matches!(
                    &outcome.result,
                    Err(Failure::UnauthorizedAction { action: refused, .. }) if *refused == action
                ),
                "{action:?} is not offered by this case: {outcome:?}"
            ),
        }
    }
}

#[test]
fn a_confirmed_valid_replacement_restores_the_scope_and_touches_nothing_else() {
    let (_tree, _root, directory) = package_with_order("repair-replacement");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let readme_before = fs::read(directory.join("README.md")).unwrap();
    let sentinel_before = fs::read(directory.join("quote.bin")).unwrap();
    let basename_before = directory.file_name().unwrap().to_owned();

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a confirmed valid replacement is planned");

    assert_eq!(
        operation_path(&plan, "replace_managed_artifact"),
        record_path(&directory)
    );
    assert_eq!(
        operation_kinds(&plan),
        ["replace_managed_artifact", "refresh_package_index",]
    );
    // A dry run performs none of it.
    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        br#"{"schema_version":1,"nope":true}"#
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("a confirmed valid replacement commits: {report:?}");
    };
    let remaining = &diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("the scope rediagnoses");
    assert!(valid(remaining), "the repaired scope is healthy");
    assert!(findings(remaining).next().is_none());

    let repaired = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.deref(), "Repaired components");
    assert_eq!(repaired.order_id.deref(), ORDER_ID);

    // Only the named artifact and the derived index were touched.
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        readme_before
    );
    assert_eq!(
        fs::read(directory.join("quote.bin")).unwrap(),
        sentinel_before
    );
    assert_eq!(directory.file_name().unwrap(), basename_before);
}

/// The index operation this plan proposes, which names the Package the repair will
/// refresh.
fn refreshed_index(plan: &Plan) -> PathBuf {
    operation_path(plan, "refresh_package_index")
}

#[test]
fn a_repair_under_a_nested_orders_path_refreshes_the_real_package() {
    // The Order Directory sits two levels below the configured Orders path, so the
    // Package cannot be reached by counting parents: the directory two levels up is
    // an ordinary folder of the user's, not a Package.
    for (label, action, repaired_title) in [
        (
            "a confirmed replacement",
            RepairAction::ReplaceManagedRecord,
            "Repaired components",
        ),
        (
            "a confirmed promotion",
            RepairAction::PromoteStagingCandidate,
            "Promoted components",
        ),
    ] {
        let (_tree, root, directory) = package_with_order_at("repair-nested", "Purchases/Active");
        let (code, resolution) = if action == RepairAction::ReplaceManagedRecord {
            damage_shape(&directory);
            (
                DiagnosticCode::ManagedArtifactInvalidShape,
                (
                    RepairAction::ReplaceManagedRecord,
                    Assignments::new().assign(
                        "record",
                        String::from_utf8(valid_replacement_with("../../..", ORDER_ID)).unwrap(),
                    ),
                ),
            )
        } else {
            stage_candidate(&directory, promoted_candidate("../../.."));
            (
                DiagnosticCode::StagingArtifactPresent,
                (RepairAction::PromoteStagingCandidate, Assignments::new()),
            )
        };
        let handle = diagnosed_handle(&directory, code);

        let plan = crate::plan(
            &repair(
                order_scope(&directory),
                &handle,
                resolution.0,
                resolution.1.clone(),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap_or_else(|error| panic!("{label} is planned under a nested Orders path: {error:?}"));

        assert_eq!(
            refreshed_index(&plan),
            root.join(".spielorders/index.json"),
            "{label} names the real Package's index rather than an inferred ancestor"
        );

        let report = crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
        )
        .result;

        let Ok(Report::Committed { .. }) = report else {
            panic!("{label} commits without warning under a nested Orders path: {report:?}");
        };
        assert!(
            valid(
                &diagnosed_order(&directory, &PlatformFileSystem)
                    .result
                    .expect("the scope rediagnoses")
            ),
            "{label} leaves the repaired scope healthy"
        );
        assert!(
            String::from_utf8_lossy(&fs::read(root.join(".spielorders/index.json")).unwrap())
                .contains(repaired_title),
            "{label} reaches the real Package's index"
        );
        assert!(
            !root.join("Purchases/.spielorders").exists(),
            "{label} writes nothing to the ancestor a parent count would have inferred"
        );
    }
}

#[test]
fn a_replacement_that_is_still_invalid_is_refused_before_any_write() {
    let (_tree, root, directory) = package_with_order("repair-invalid-replacement");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let before = fs::read(record_path(&directory)).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    // Each candidate falls short of a fully valid record in a different way. A repair
    // never makes up the difference: unknown or invalid facts disappear only because
    // the user confirmed a record without them.
    let semantically_invalid = String::from_utf8(valid_replacement())
        .unwrap()
        .replace("2026-08-22", "2026-02-30")
        .into_bytes();
    for (label, candidate) in [
        ("still malformed", b"{ not json".to_vec()),
        (
            "still carrying an unknown field",
            br#"{"schema_version":1,"nope":true}"#.to_vec(),
        ),
        ("semantically invalid", semantically_invalid),
        ("empty", Vec::new()),
    ] {
        let error = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::ReplaceManagedRecord,
                Assignments::new().assign("record", String::from_utf8(candidate).unwrap()),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .err()
        .unwrap_or_else(|| panic!("{label} must be refused rather than salvaged"));
        assert!(
            matches!(
                error,
                Failure::InvalidCandidate { .. } | Failure::RepairResolutionMissing { .. }
            ),
            "{label} is refused as an invalid replacement: {error:?}"
        );
    }

    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        before,
        "a refused replacement writes nothing"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

/// The same replacement carrying a Vendor Reference.
fn valid_replacement_referencing(vendor_id: &str) -> Vec<u8> {
    String::from_utf8(valid_replacement())
        .unwrap()
        .replace(
            "\"status\": \"hold\"",
            &format!("\"status\": \"hold\",\n  \"vendor_id\": \"{vendor_id}\""),
        )
        .into_bytes()
}

#[test]
fn a_replacement_that_would_leave_the_package_damaged_is_refused_before_any_write() {
    // Each record below is a perfectly valid Order read on its own. Only the Package
    // can tell that writing it answers this diagnosed condition by creating another,
    // so each is refused before anything is written rather than committed and then
    // rediagnosed as damage.
    for (label, candidate, constraint) in [
        (
            "an identity another Order Directory already holds",
            valid_replacement_with("../..", OTHER_ORDER_ID),
            PackageConstraint::OrderIdentityTaken,
        ),
        (
            "a Vendor Reference this Package cannot resolve",
            valid_replacement_referencing("6b0d9e77-2a41-4c8e-9f10-0d3b5a7c2e18"),
            PackageConstraint::VendorReferenceUnresolved,
        ),
        (
            "a locator that leads somewhere other than this Package",
            valid_replacement_with("../../..", ORDER_ID),
            PackageConstraint::PackageRootMismatch,
        ),
    ] {
        let (_tree, root, directory) = package_with_order("repair-package-invalid");
        adopted_order(&root, "2026_08_23_Vacuum_fittings", OTHER_ORDER_ID);
        damage_shape(&directory);
        let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
        let before = fs::read(record_path(&directory)).unwrap();
        let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

        let error = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::ReplaceManagedRecord,
                Assignments::new().assign("record", String::from_utf8(candidate).unwrap()),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .err()
        .unwrap_or_else(|| panic!("{label} must be refused"));

        assert!(
            matches!(
                error,
                Failure::CandidateDamagesPackage { constraint: broken, .. }
                    if broken == constraint
            ),
            "{label} is refused by the name of the constraint it broke: {error:?}"
        );
        assert_eq!(
            fs::read(record_path(&directory)).unwrap(),
            before,
            "{label} writes nothing"
        );
        assert_eq!(
            fs::read(root.join(".spielorders/index.json")).unwrap(),
            index_before,
            "{label} leaves the derived index alone"
        );
    }
}

/// A complete valid record someone outside this repair would have written.
fn someone_elses_record() -> Vec<u8> {
    String::from_utf8(valid_replacement())
        .unwrap()
        .replace("Repaired components", "Corrected by someone else")
        .into_bytes()
}

#[test]
fn a_destination_changed_after_diagnosis_is_preserved_rather_than_replaced() {
    let (_tree, _root, directory) = package_with_order("repair-destination-changed");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // Someone else corrects the record between the plan and its write. The confirmed
    // replacement was authorized against the damaged bytes, not against theirs.
    let theirs = someone_elses_record();
    let filesystem =
        another_writer_replacing(&record_path(&directory), &theirs, EditPoint::StagingWrite);

    let report = crate::commit(plan, &effects(&filesystem).minting(QueuedUuids::of(&[]))).result;

    assert!(filesystem.disturbed(), "the outside write really landed");
    let Err(_rejection) = report else {
        panic!("a changed destination is refused: {report:?}");
    };
    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        theirs,
        "the other writer's record survives"
    );
    assert!(
        !directory.join(STAGE).exists(),
        "the refused replacement leaves no staging file of its own behind"
    );
}

/// A valid record that differs from the committed one, for a Package whose Orders sit
/// at the given locator depth.
fn promoted_candidate(package_root: &str) -> Vec<u8> {
    String::from_utf8(valid_replacement_with(package_root, ORDER_ID))
        .unwrap()
        .replace("Repaired components", "Promoted components")
        .into_bytes()
}

/// Puts a candidate beside its destination, as an interrupted replacement would leave
/// it.
fn stage_candidate(directory: &Path, candidate: Vec<u8>) -> Vec<u8> {
    fs::write(directory.join(STAGE), &candidate).unwrap();
    candidate
}

/// The same, for the default Package layout.
fn stage_valid_candidate(directory: &Path) -> Vec<u8> {
    stage_candidate(directory, promoted_candidate("../.."))
}

#[test]
fn promoting_a_valid_stage_preserves_its_json_spelling() {
    let (_tree, _root, directory) = package_with_order("repair-compact-promotion");
    let canonical = promoted_candidate("../..");
    let document: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
    let compact = serde_json::to_vec(&document).unwrap();
    assert_ne!(compact, canonical);
    stage_candidate(&directory, compact.clone());
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("valid JSON formatting does not change the candidate's meaning");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");
    assert_eq!(fs::read(record_path(&directory)).unwrap(), compact);
    assert!(!directory.join(STAGE).exists());
}

#[test]
fn a_promotion_never_publishes_a_candidate_changed_while_becoming_durable() {
    let (_tree, _root, directory) = package_with_order("repair-stage-before-publication");
    stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let before = fs::read(record_path(&directory)).unwrap();
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::SyncFile)
        .ending_in(STAGE)
        .once()
        .preceded_by(|outside| outside.write(&outside.call().path, b"unconfirmed stage"))
        .proceeds();

    crate::commit(plan, &effects(&filesystem))
        .result
        .expect_err("the promoted bytes must be the confirmed candidate");

    assert_eq!(fs::read(record_path(&directory)).unwrap(), before);
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        b"unconfirmed stage"
    );
}

#[test]
fn promoting_the_diagnosed_candidate_completes_its_interrupted_replacement() {
    let (_tree, _root, directory) = package_with_order("repair-promotion");
    stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let readme_before = fs::read(directory.join("README.md")).unwrap();

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the diagnosed candidate can be promoted");

    assert_eq!(
        operation_path(&plan, "promote_staging_candidate"),
        record_path(&directory),
        "and the destination it belongs to, which it never has to choose"
    );
    assert_eq!(
        operation_kinds(&plan),
        ["promote_staging_candidate", "refresh_package_index",]
    );
    // A dry run promotes nothing.
    assert!(directory.join(STAGE).exists());

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("an exact validated promotion commits: {report:?}");
    };
    let promoted = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.deref(), "Promoted components");
    assert!(
        !directory.join(STAGE).exists(),
        "the candidate became the record rather than being cleaned up separately"
    );
    let remaining = &diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("the scope rediagnoses");
    assert!(valid(remaining));
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        readme_before
    );
}

#[test]
fn an_invalid_staging_candidate_is_never_promoted() {
    for (label, candidate) in [
        ("malformed", b"{ not json".to_vec()),
        (
            "unknown field",
            br#"{"schema_version":1,"nope":true}"#.to_vec(),
        ),
        (
            "semantically invalid",
            String::from_utf8(valid_replacement())
                .unwrap()
                .replace("2026-08-22", "2026-02-30")
                .into_bytes(),
        ),
    ] {
        let (_tree, _root, directory) = package_with_order("repair-invalid-stage");
        fs::write(directory.join(STAGE), &candidate).unwrap();
        let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
        let record_before = fs::read(record_path(&directory)).unwrap();

        let error = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::PromoteStagingCandidate,
                Assignments::new(),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .err()
        .unwrap_or_else(|| panic!("a {label} candidate must not be promoted"));

        assert!(
            matches!(error, Failure::InvalidCandidate { .. }),
            "a {label} candidate is refused: {error:?}"
        );
        // Both the canonical record and the abandoned evidence survive untouched.
        assert_eq!(fs::read(record_path(&directory)).unwrap(), record_before);
        assert_eq!(fs::read(directory.join(STAGE)).unwrap(), candidate);
    }
}

#[test]
fn a_staged_candidate_that_would_leave_the_package_damaged_is_never_promoted() {
    // A staged candidate is judged against the Package exactly as a confirmed
    // replacement is. Completing an interrupted write is not licence to write a
    // record that quarantines two Order Directories.
    let (_tree, root, directory) = package_with_order("repair-stage-package-invalid");
    adopted_order(&root, "2026_08_23_Vacuum_fittings", OTHER_ORDER_ID);
    let candidate = valid_replacement_with("../..", OTHER_ORDER_ID);
    fs::write(directory.join(STAGE), &candidate).unwrap();
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let before = fs::read(record_path(&directory)).unwrap();

    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a candidate that would damage the Package must not be promoted");

    assert!(
        matches!(
            error,
            Failure::CandidateDamagesPackage {
                constraint: PackageConstraint::OrderIdentityTaken,
                ..
            }
        ),
        "the refusal names the Package constraint it broke: {error:?}"
    );
    assert_eq!(fs::read(record_path(&directory)).unwrap(), before);
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        candidate,
        "the abandoned evidence survives the refusal"
    );
}

#[test]
fn a_candidate_that_changed_after_planning_is_not_promoted() {
    let (_tree, _root, directory) = package_with_order("repair-stage-changed");
    stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let record_before = fs::read(record_path(&directory)).unwrap();

    // Someone rewrites the stage between planning and commit. It is no longer the
    // candidate that was diagnosed and validated.
    fs::write(directory.join(STAGE), b"{ not json").unwrap();

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a changed stage is not promoted: {report:?}");
    };
    assert!(
        matches!(rejection, Failure::EvidenceChanged { .. }),
        "a changed stage is a changed-evidence rejection: {rejection:?}"
    );
    assert_eq!(fs::read(record_path(&directory)).unwrap(), record_before);
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), b"{ not json");
}

#[test]
fn a_promotion_whose_destination_changed_after_diagnosis_preserves_it() {
    let (_tree, _root, directory) = package_with_order("repair-promote-destination");
    let staged = stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // Someone else writes the record between the plan and the promotion. Completing
    // an interrupted write is not licence to discard work that arrived since.
    let theirs = someone_elses_record();
    let filesystem = another_writer_replacing(
        &record_path(&directory),
        &theirs,
        EditPoint::ReadEndingIn(STAGE),
    );

    let report = crate::commit(plan, &effects(&filesystem).minting(QueuedUuids::of(&[]))).result;

    assert!(filesystem.disturbed(), "the outside write really landed");
    let Err(_rejection) = report else {
        panic!("a changed destination is refused: {report:?}");
    };
    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        theirs,
        "the other writer's record survives"
    );
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        staged,
        "the diagnosed candidate is still there to be looked at again"
    );
}

/// The state an interrupted replacement leaves when it never reached the rename and
/// the canonical record was not there to begin with: a durable candidate beside a
/// destination that does not exist.
fn stage_beside_a_missing_record(directory: &Path) -> Vec<u8> {
    let staged = stage_valid_candidate(directory);
    fs::remove_file(record_path(directory)).unwrap();
    staged
}

#[test]
fn a_candidate_beside_a_missing_record_is_promoted_into_place() {
    let (_tree, _root, directory) = package_with_order("repair-promote-absent");
    stage_beside_a_missing_record(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);

    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a candidate beside a missing record can be promoted");

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("promoting into a destination nothing occupies commits: {report:?}");
    };
    assert!(valid(
        &diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .expect("the scope rediagnoses")
    ));
    let promoted = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.deref(), "Promoted components");
    assert!(!directory.join(STAGE).exists());
}

#[test]
fn a_record_that_appeared_where_the_diagnosis_found_none_is_not_overwritten() {
    let (_tree, _root, directory) = package_with_order("repair-promote-appeared");
    let staged = stage_beside_a_missing_record(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // Someone else restores the record while this promotion is under way. An empty
    // destination is an observation like any other, and theirs is not the emptiness
    // this promotion was authorized against.
    let theirs = someone_elses_record();
    let filesystem = another_writer_replacing(
        &record_path(&directory),
        &theirs,
        EditPoint::ReadEndingIn(STAGE),
    );

    let report = crate::commit(plan, &effects(&filesystem).minting(QueuedUuids::of(&[]))).result;

    assert!(filesystem.disturbed(), "the outside write really landed");
    let Err(_rejection) = report else {
        panic!("a destination that stopped being empty is refused: {report:?}");
    };
    assert_eq!(fs::read(record_path(&directory)).unwrap(), theirs);
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), staged);
}

#[test]
fn a_handle_whose_condition_changed_since_diagnosis_is_rejected() {
    let (_tree, root, directory) = package_with_order("repair-stale-handle");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);

    // The condition is corrected by other means after the diagnosis was taken, and a
    // different condition appears in its place. The handle now names a case that no
    // longer exists, so acting on it would be acting on outdated evidence — and a
    // capability that quietly fell back to whichever case is currently first would
    // repair something the user never confirmed.
    fs::write(record_path(&directory), valid_replacement()).unwrap();
    stage_valid_candidate(&directory);
    let standing = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert!(
        standing
            .cases
            .iter()
            .any(|case| case.code == DiagnosticCode::StagingArtifactPresent),
        "a different case is standing when the stale handle is presented"
    );
    let before = fs::read(record_path(&directory)).unwrap();
    let stage_before = fs::read(directory.join(STAGE)).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a stale handle is rejected");

    assert!(
        matches!(error, Failure::UnknownCase { .. }),
        "a stale handle names no current case: {error:?}"
    );
    assert_eq!(fs::read(record_path(&directory)).unwrap(), before);
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        stage_before,
        "the case that is standing was never repaired in the stale one's place"
    );
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );

    // An invented handle is refused the same way; handles are not a namespace a
    // caller may compose.
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            String::from("managed_artifact_invalid_shape:Orders/elsewhere/.spielorders/order.json"),
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("an invented handle is rejected");
    assert!(matches!(error, Failure::UnknownCase { .. }));
}

#[test]
fn a_confirmation_naming_a_different_case_applies_nothing() {
    // Two conditions stand at once. A confirmation names exactly one, so confirming
    // the staging case can never carry out the record replacement, and vice versa.
    let (_tree, _root, directory) = package_with_order("repair-one-case");
    damage_shape(&directory);
    let staged = stage_valid_candidate(&directory);
    let shape_handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let stage_handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    assert_ne!(shape_handle, stage_handle);

    // The staging handle does not authorize a record replacement.
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &stage_handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a confirmation must match the case it names");
    assert!(
        matches!(error, Failure::UnauthorizedAction { .. }),
        "got {error:?}"
    );

    // And the replacement handle does not authorize a promotion.
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &shape_handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a confirmation must match the case it names");
    assert!(
        matches!(error, Failure::UnauthorizedAction { .. }),
        "got {error:?}"
    );

    // A replacement stages beside its destination, and the abandoned candidate is
    // sitting exactly there. Carrying it out would destroy evidence this core may
    // neither overwrite nor silently clean, so it is refused by name rather than
    // failing opaquely at the staging write.
    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &shape_handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a replacement never writes over abandoned staging evidence");
    assert!(
        matches!(error, Failure::StagingEvidencePresent { .. }),
        "got {error:?}"
    );
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), staged);

    // Confirming the staging case carries out only that case.
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &stage_handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "the confirmed case commits: {report:?}"
    );
    assert!(!directory.join(STAGE).exists());
    // The promoted record is the candidate's, not the replacement's: one call carried
    // out one case and never touched the other's resolution.
    let promoted = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.deref(), "Promoted components");
}

/// Where a repair is made to fail. Every other effect is real.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepairBoundary {
    WriteStage,
    ReplaceRecord,
    IndexReplacement,
    /// The repair commits, and the Package is damaged before its refresh loads it.
    DamagePackageAfterRepair,
}

fn failing_at(boundary: RepairBoundary) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    match boundary {
        RepairBoundary::WriteStage => filesystem.on(Operation::WriteNew).ending_in(STAGE).fails(),
        RepairBoundary::ReplaceRecord => filesystem
            .on(Operation::ReplaceFile)
            .ending_in(".spielorders/order.json")
            .fails(),
        RepairBoundary::IndexReplacement => filesystem
            .on(Operation::ReplaceFile)
            .ending_in(".spielorders/index.json")
            .fails(),
        RepairBoundary::DamagePackageAfterRepair => filesystem
            .on(Operation::Read)
            .ending_in("vendors.json")
            .after(Operation::ReplaceFile, ".spielorders/order.json")
            .answers(Answer::Bytes(b"{ not json".to_vec())),
    }
    filesystem
}

#[test]
fn an_injected_replacement_failure_preserves_the_prior_canonical_bytes() {
    for boundary in [RepairBoundary::WriteStage, RepairBoundary::ReplaceRecord] {
        let (_tree, _root, directory) = package_with_order("repair-replace-failure");
        damage_shape(&directory);
        let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
        let before = fs::read(record_path(&directory)).unwrap();
        let plan = crate::plan(
            &repair(
                order_scope(directory.clone()),
                &handle,
                RepairAction::ReplaceManagedRecord,
                Assignments::new()
                    .assign("record", String::from_utf8(valid_replacement()).unwrap()),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();

        let report = crate::commit(
            plan,
            &effects(&failing_at(boundary)).minting(QueuedUuids::of(&[])),
        )
        .result;

        let Err(_rejection) = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        assert_eq!(
            fs::read(record_path(&directory)).unwrap(),
            before,
            "the prior canonical bytes survive an injected {boundary:?} failure"
        );
        assert!(!directory.join(STAGE).exists());
    }
}

#[test]
fn an_injected_promotion_failure_preserves_both_the_record_and_the_candidate() {
    let (_tree, _root, directory) = package_with_order("repair-promote-failure");
    let staged = stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let before = fs::read(record_path(&directory)).unwrap();
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&failing_at(RepairBoundary::ReplaceRecord)).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Err(_rejection) = report else {
        panic!("an injected promotion failure rejects: {report:?}");
    };
    assert_eq!(fs::read(record_path(&directory)).unwrap(), before);
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        staged,
        "the abandoned evidence survives a failed promotion"
    );
}

#[test]
fn a_post_repair_index_failure_stays_committed_with_a_warning() {
    let (_tree, _root, directory) = package_with_order("repair-index-failure");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&failing_at(RepairBoundary::IndexReplacement)).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("a failed index refresh remains a committed repair: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));
    // The canonical repair stands and the scope is healthy again.
    let remaining = &diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("the scope rediagnoses");
    assert!(valid(remaining));
    let repaired = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.deref(), "Repaired components");
}

#[test]
fn a_repair_that_cannot_be_rediagnosed_still_reports_itself_committed() {
    let (_tree, _root, directory) = package_with_order("repair-rediagnosis-failure");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", String::from_utf8(valid_replacement()).unwrap()),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The record is replaced, and only then does package-wide state stop loading, so
    // no remaining-diagnostic answer can be produced.
    let report = crate::commit(
        plan,
        &effects(&failing_at(RepairBoundary::DamagePackageAfterRepair))
            .minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the repair happened, so the report is committed: {report:?}");
    };
    assert!(
        warnings.contains(&Warning::RediagnosisUnavailable),
        "got {warnings:?}"
    );
    let repaired = serde_json::from_slice::<crate::metadata::OrderDocument>(
        &fs::read(record_path(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.deref(), "Repaired components");
}

#[test]
fn a_confirmation_is_rejected_once_its_evidence_moved_though_the_condition_still_stands() {
    // Nothing is corrected here. The same code stands at the same path from start to
    // finish; only the candidate a promotion would write changes. That is precisely
    // what a handle naming just a code and a path cannot see: it would still name a
    // standing case, authorize the action, and promote bytes the user never confirmed.
    let (_tree, _root, directory) = package_with_order("repair-moved-evidence");
    let confirmed = stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);

    let replaced = String::from_utf8(confirmed)
        .unwrap()
        .replace("Promoted components", "Never confirmed")
        .into_bytes();
    fs::write(directory.join(STAGE), &replaced).unwrap();

    let standing = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert!(
        standing
            .cases
            .iter()
            .any(|case| case.code == DiagnosticCode::StagingArtifactPresent),
        "the same condition is still standing at the same path: {:?}",
        standing.cases
    );
    let record_before = fs::read(record_path(&directory)).unwrap();

    let error = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a confirmation whose evidence moved is rejected");

    assert!(
        matches!(error, Failure::UnknownCase { .. }),
        "a confirmation naming evidence that has moved names no current case: {error:?}"
    );
    assert_eq!(
        fs::read(record_path(&directory)).unwrap(),
        record_before,
        "the candidate the user never confirmed was not promoted"
    );
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        replaced,
        "and the standing candidate was left exactly as it was found"
    );
}

// ---------------------------------------------------------------------------
// Package scope
// ---------------------------------------------------------------------------

/// A removed Order record that cannot be read might name any Vendor, so a Vendor aggregate
/// repair is refused rather than judged as though it named none.
#[test]
fn a_vendor_aggregate_repair_is_refused_while_a_removed_order_is_unreadable() {
    let (_tree, root) = initialized_package("package-repair-unreadable-removed");
    crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    let order = crate::tests::support::created_order(
        &root,
        ORDER_ID,
        Assignments::new()
            .assign("title", "Lenses")
            .assign("vendor", "Tarnwick"),
    );
    crate::tests::support::committed(
        &Change::RemoveOrder {
            order: crate::OrderRef::Directory(order.clone()),
        },
        &effects(&PlatformFileSystem),
    );
    let record = record_path(&root.join("Removed").join(order.file_name().unwrap()));
    fs::write(root.join(VENDORS_STAGE), vendor_aggregate("Tarnwick")).unwrap();
    let handle = package_handle(&root, DiagnosticCode::StagingArtifactPresent);
    let filesystem = ScriptedFileSystem::new().failing(Operation::Read, &record);

    let refusal = crate::plan(
        &repair(
            package_scope(&root),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&filesystem),
    )
    .result
    .expect_err("an unreadable removed Order refuses the repair");
    let Failure::RemovedOrderUnreadable(read) = &refusal else {
        panic!("refused as an unreadable removed Order: {refusal:?}")
    };
    let Failure::PackageOperationFailed(failure) = read.as_ref() else {
        panic!("the repair refuses as a failed operation: {refusal:?}")
    };
    assert_eq!(
        crate::tests::support::failed_path(&failure.path),
        record.as_path()
    );
}

const VENDORS: &str = ".spielorders/vendors.json";
const VENDORS_STAGE: &str = ".spielorders/.vendors.json.spielorders-stage";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

/// One Vendor aggregate a Package will accept.
fn vendor_aggregate(name: &str) -> Vec<u8> {
    format!(
        "{{\n  \"schema_version\": 1,\n  \"vendors\": [\n    {{\n      \
         \"vendor_id\": \"{VENDOR_ID}\",\n      \"name\": \"{name}\",\n      \
         \"contacts\": []\n    }}\n  ]\n}}\n"
    )
    .into_bytes()
}

/// A Package with a leftover staging artifact beside its Vendor aggregate.
///
/// An interrupted managed write leaves exactly this: a durable stage beside the
/// destination it was to become. No capability produces one on purpose — every
/// replacement that succeeds removes its own stage — so the fixture writes it.
fn package_with_a_leftover_vendor_stage(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    fs::write(root.join(VENDORS_STAGE), vendor_aggregate("Tarnwick")).unwrap();
    (tree, root)
}

fn package_handle(root: &Path, code: DiagnosticCode) -> String {
    let diagnosis = diagnosed_package(root, &PlatformFileSystem)
        .result
        .expect("the Package can be diagnosed");
    diagnosis
        .cases
        .iter()
        .find(|case| case.code == code)
        .unwrap_or_else(|| panic!("{code:?} is diagnosed: {:?}", diagnosis.cases))
        .handle
        .to_owned()
}

/// The Package-level case the integrity decision makes actionable: a leftover staging
/// artifact beside a managed Package artifact, promotable on explicit confirmation.
#[test]
fn a_leftover_stage_beside_a_package_artifact_is_a_diagnosed_case() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-diagnosis");

    let diagnosis = diagnosed_package(&root, &PlatformFileSystem)
        .result
        .expect("the Package can be diagnosed");

    assert!(
        valid(&diagnosis),
        "a stage beside a valid destination is cleanup, not damage: {:?}",
        diagnosis.diagnostic_groups
    );
    let case = diagnosis
        .cases
        .iter()
        .find(|case| case.code == DiagnosticCode::StagingArtifactPresent)
        .expect("the leftover stage authorizes a case");
    assert_eq!(case.actions, [RepairAction::PromoteStagingCandidate]);
    assert_eq!(
        case.location.absolute_path,
        root.join(VENDORS_STAGE),
        "the case names the exact evidence it was diagnosed from"
    );
}

/// Only this Package's own scope. A case inside an Order belongs to that Order's
/// diagnosis, and a Package-level answer that reached into it would repair a scope
/// nobody named.
#[test]
fn a_package_diagnosis_reports_no_order_scoped_case() {
    let (_tree, root) = initialized_package("package-repair-scope");
    let directory = root.join("Orders/2026_08_22_Laser_components/.spielorders");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("order.json"), b"{ not a record").unwrap();
    fs::write(
        directory.join(".order.json.spielorders-stage"),
        b"{ still not a record",
    )
    .unwrap();

    let diagnosis = diagnosed_package(&root, &PlatformFileSystem)
        .result
        .expect("the Package can be diagnosed");

    assert!(
        diagnosis.cases.is_empty(),
        "the actionable case is inside an Order: {:?}",
        diagnosis.cases
    );
    assert!(
        findings(&diagnosis)
            .all(|diagnostic| diagnostic.scope == crate::readme::EvidenceScope::Package),
        "and only Package-scope findings are reported: {:?}",
        diagnosis.diagnostic_groups
    );
}

/// A promotion completes the interrupted write: the candidate becomes the artifact, the
/// leftover stage is consumed, and the finding is gone afterwards.
#[test]
fn promoting_a_validated_candidate_completes_the_interrupted_write() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-promote");
    let handle = package_handle(&root, DiagnosticCode::StagingArtifactPresent);

    let plan = crate::plan(
        &repair(
            package_scope(root.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the diagnosed candidate can be promoted");
    assert_eq!(
        operation_kinds(&plan),
        ["promote_staging_candidate", "refresh_package_index",]
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("the promotion commits: {report:?}");
    };
    assert_eq!(
        fs::read(root.join(VENDORS)).unwrap(),
        vendor_aggregate("Tarnwick"),
        "the candidate is now the artifact"
    );
    assert!(
        !root.join(VENDORS_STAGE).exists(),
        "and the leftover stage is consumed rather than cleaned up separately"
    );
    assert!(
        &diagnosed_package(&root, &PlatformFileSystem)
            .result
            .expect("the scope is diagnosed again")
            .cases
            .is_empty(),
        "the finding is gone"
    );
}

/// A confirmed replacement is accepted only as a complete, valid artifact, and one that
/// would leave this Package's Vendor set untrustworthy is refused before anything is
/// written.
#[test]
fn a_confirmed_replacement_for_a_package_artifact_is_judged_before_it_is_written() {
    let (_tree, root) = initialized_package("package-repair-replace");
    fs::write(
        root.join(VENDORS),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": {}\n}\n",
    )
    .unwrap();
    let handle = package_handle(&root, DiagnosticCode::ManagedArtifactInvalidShape);

    let refusal = crate::plan(
        &repair(
            package_scope(&root),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign("record", "{ still not a record"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a replacement that is not a Vendor aggregate is refused");
    assert!(
        matches!(refusal, Failure::InvalidCandidate { .. }),
        "{refusal:?}"
    );

    let plan = crate::plan(
        &repair(
            package_scope(root.clone()),
            &handle,
            RepairAction::ReplaceManagedRecord,
            Assignments::new().assign(
                "record",
                String::from_utf8(vendor_aggregate("Tarnwick")).unwrap(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a complete valid aggregate is accepted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
    )
    .result;
    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");
    let diagnosis = diagnosed_package(&root, &PlatformFileSystem)
        .result
        .expect("the Package is diagnosed");
    assert!(
        valid(&diagnosis)
            && !findings(&diagnosis).any(|diagnostic| diagnostic.severity == Severity::Error),
        "the Package is valid in its own scope afterwards: {:?}",
        diagnosis.diagnostic_groups
    );
}

/// A destination that changed between the diagnosis and the commit is refused, the other
/// writer's bytes survive, and the refusal is not a write failure.
#[test]
fn a_destination_that_changed_after_the_diagnosis_is_refused_and_their_bytes_survive() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-race");
    let handle = package_handle(&root, DiagnosticCode::StagingArtifactPresent);
    let theirs = vendor_aggregate("Nethermoor");
    let filesystem = another_writer_replacing(
        &root.join(VENDORS),
        &theirs,
        EditPoint::ReadEndingIn(VENDORS_STAGE),
    );

    // Planned against the Package as the diagnosis found it; the other writer lands
    // between that plan and the commit, which is the window a destination guard closes.
    let plan = crate::plan(
        &repair(
            package_scope(root.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the diagnosed candidate can be promoted");
    let report = crate::commit(plan, &effects(&filesystem).minting(QueuedUuids::of(&[]))).result;

    assert!(filesystem.disturbed(), "the other writer landed");
    let Err(_rejection) = report else {
        panic!("the promotion is refused: {report:?}");
    };
    assert_eq!(
        fs::read(root.join(VENDORS)).unwrap(),
        theirs,
        "and their bytes survive exactly"
    );
}

/// A promotion of the one staging candidate diagnosed in this Package's own scope.
fn promote_package_stage(root: &Path) -> crate::Answer<Plan> {
    crate::plan(
        &repair(
            package_scope(PathBuf::from(root)),
            String::from(&package_handle(
                root,
                DiagnosticCode::StagingArtifactPresent,
            )),
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
}

/// Each of a Package's managed artifacts is held to its own kind's semantics before a
/// staged candidate is promoted over it: a candidate that fails them is refused and left
/// where it is, and one that meets them is promoted.
#[test]
fn each_package_artifact_is_judged_as_its_own_kind_before_promotion() {
    let duplicate_names = format!(
        "{{\n  \"schema_version\": 1,\n  \"vendors\": [\n    {{\n      \
         \"vendor_id\": \"{VENDOR_ID}\",\n      \"name\": \"Tarnwick\",\n      \
         \"contacts\": []\n    }},\n    {{\n      \
         \"vendor_id\": \"6b0d9e77-2a41-4c8e-9f10-0d3b5a7c2e18\",\n      \
         \"name\": \"tarnwick\",\n      \"contacts\": []\n    }}\n  ]\n}}\n"
    )
    .into_bytes();
    for (artifact, refused) in [
        ("version.json", br#"{"metadata_version":2}"#.to_vec()),
        (
            "config.toml",
            b"orders_path = \"../Elsewhere\"\nremoved_path = \"Removed\"\n".to_vec(),
        ),
        ("vendors.json", duplicate_names),
        ("index.json", vendor_aggregate("Tarnwick")),
    ] {
        let (_tree, root) = initialized_package(&format!("package-repair-kind-{artifact}"));
        let destination = root.join(".spielorders").join(artifact);
        let stage = root
            .join(".spielorders")
            .join(format!(".{artifact}.spielorders-stage"));
        let before = fs::read(&destination).unwrap();

        fs::write(&stage, &refused).unwrap();
        let error = promote_package_stage(&root)
            .result
            .expect_err("a candidate failing its kind's semantics is refused");
        assert!(
            matches!(error, Failure::InvalidCandidate { .. }),
            "{artifact}: {error:?}"
        );
        assert_eq!(fs::read(&destination).unwrap(), before, "{artifact}");
        assert_eq!(fs::read(&stage).unwrap(), refused, "{artifact}");

        fs::write(&stage, &before).unwrap();
        let plan = promote_package_stage(&root)
            .result
            .unwrap_or_else(|error| panic!("{artifact}: a valid candidate plans: {error:?}"));
        let report = crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[])),
        )
        .result;
        assert!(
            matches!(report, Ok(Report::Committed { .. })),
            "{artifact}: {report:?}"
        );
        assert!(!stage.exists(), "{artifact}: the stage became the artifact");
    }
}

/// A case belongs to the scope whose diagnosis offered it. Presented to the other scope it
/// names no case there, so neither scope's repair acts on the other's artifacts.
#[test]
fn a_case_from_one_scope_is_unknown_to_the_other() {
    let (_tree, root, directory) = package_with_order("repair-cross-scope");
    let order_stage = stage_valid_candidate(&directory);
    fs::write(root.join(VENDORS_STAGE), vendor_aggregate("Tarnwick")).unwrap();
    let order_case = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let package_case = package_handle(&root, DiagnosticCode::StagingArtifactPresent);
    let record_before = fs::read(record_path(&directory)).unwrap();
    let vendors_before = fs::read(root.join(VENDORS)).unwrap();

    for (scope, handle) in [
        (package_scope(&root), &order_case),
        (order_scope(&directory), &package_case),
    ] {
        let error = crate::plan(
            &repair(
                scope.clone(),
                handle,
                RepairAction::PromoteStagingCandidate,
                Assignments::new(),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("the other scope's case is refused");
        assert!(
            matches!(error, Failure::UnknownCase { .. }),
            "{scope:?}: {error:?}"
        );
    }
    assert_eq!(fs::read(record_path(&directory)).unwrap(), record_before);
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), order_stage);
    assert_eq!(fs::read(root.join(VENDORS)).unwrap(), vendors_before);
    assert_eq!(
        fs::read(root.join(VENDORS_STAGE)).unwrap(),
        vendor_aggregate("Tarnwick")
    );
}

/// A Vendor aggregate is judged against the Vendor References the Package's Orders carry,
/// removed ones and ones this build cannot read included: a repair never takes away a
/// Vendor an Order resolves.
#[test]
fn a_vendor_aggregate_repair_never_unresolves_an_orders_vendor() {
    for (removed, unsupported) in [(false, false), (true, false), (false, true)] {
        let (_tree, root) = initialized_package("package-repair-vendor-reference");
        crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
        let order = crate::tests::support::created_order(
            &root,
            ORDER_ID,
            Assignments::new().assign("title", "Lenses"),
        );
        let effects = effects(&PlatformFileSystem);
        crate::tests::support::committed(
            &crate::tests::support::edit_order(
                order.clone(),
                Assignments::new().assign("vendor", "Tarnwick"),
            ),
            &effects,
        );
        if removed {
            crate::tests::support::committed(
                &Change::RemoveOrder {
                    order: crate::OrderRef::Directory(order.clone()),
                },
                &effects,
            );
        }
        if unsupported {
            let record = fs::read_to_string(record_path(&order)).unwrap();
            let future = record.replacen("\"schema_version\": 1", "\"schema_version\": 2", 1);
            assert_ne!(future, record);
            fs::write(record_path(&order), future).unwrap();
        }
        let empty = b"{\n  \"schema_version\": 1,\n  \"vendors\": []\n}\n";
        fs::write(root.join(VENDORS_STAGE), empty).unwrap();
        let handle = package_handle(&root, DiagnosticCode::StagingArtifactPresent);

        let refusal = crate::plan(
            &repair(
                package_scope(&root),
                &handle,
                RepairAction::PromoteStagingCandidate,
                Assignments::new(),
            ),
            &effects,
        )
        .result
        .expect_err("a stage dropping a referenced Vendor is refused");
        assert!(
            matches!(
                refusal,
                Failure::CandidateDamagesPackage {
                    constraint: PackageConstraint::VendorReferenceUnresolved,
                    ..
                }
            ),
            "removed {removed}, unsupported {unsupported}: {refusal:?}"
        );
    }
}

#[test]
fn a_promoted_candidate_is_made_durable_before_it_becomes_the_record() {
    use crate::tests::support::{Sync, Syncs};
    let (_tree, _root, directory) = package_with_order("repair-promotion-durable");
    stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = crate::plan(
        &repair(
            order_scope(directory.clone()),
            &handle,
            RepairAction::PromoteStagingCandidate,
            Assignments::new(),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let filesystem = Syncs::default();

    let report = crate::commit(plan, &effects(&filesystem).minting(QueuedUuids::of(&[]))).result;

    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");
    let stage = directory.join(STAGE);
    assert_eq!(filesystem.of(|path| path == stage), [(Sync::Full, stage)]);
}
