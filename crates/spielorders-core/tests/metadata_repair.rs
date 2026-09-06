//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree, adopted_order, diagnosed_handle,
};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::metadata_repair::{
    MetadataRepairError, MetadataRepairOperationKind, MetadataRepairRejection,
    MetadataRepairWarning, PackageConstraint, RepairConfirmation, RepairResolution,
    commit_metadata_repair, plan_metadata_repair,
};
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_diagnosis::diagnose_order;
use spielorders_core::ports::UuidGenerator;
use spielorders_core::readme::DiagnosticCode;
use spielorders_core::repair_cases::RepairAction;
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

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

fn package_with_order(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    package_with_order_at(label, "Orders")
}

/// The fixture Package with its Orders at the configured path, so a repair can be
/// exercised under a layout its Package root cannot be guessed from.
fn package_with_order_at(label: &str, orders_path: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::new(orders_path, "Removed"),
        &RealFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = fs::canonicalize(tree.path()).unwrap();
    let directory = root.join(orders_path).join("2026_08_22_Laser_components");
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
    assert!(matches!(
        commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    (tree, root, directory)
}

fn record(directory: &Path) -> PathBuf {
    directory.join(".spielorders/order.json")
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
    fs::write(record(directory), br#"{"schema_version":1,"nope":true}"#).unwrap();
}

#[test]
fn every_action_the_diagnosis_can_emit_is_either_executed_here_or_deliberately_refused() {
    // The exhaustive match is the point: adding a variant to the published
    // RepairAction contract will not compile until this ticket decides whether it
    // executes it. The mapping is asserted against the real enum, never a local copy.
    for action in [
        RepairAction::ReplaceManagedRecord,
        RepairAction::PromoteStagingCandidate,
        RepairAction::AssignExistingVendor,
        RepairAction::CreateAndAssignVendor,
        RepairAction::ClearVendorReference,
        RepairAction::AdoptNewOrderIdentity,
        RepairAction::RepairPackageLocator,
    ] {
        let owned_here = match action {
            // This ticket owns the managed-metadata and staging repairs.
            RepairAction::ReplaceManagedRecord | RepairAction::PromoteStagingCandidate => true,
            // The semantic resolutions belong to the explicit Order-reference,
            // identity, and locator repair capability.
            RepairAction::AssignExistingVendor
            | RepairAction::CreateAndAssignVendor
            | RepairAction::ClearVendorReference
            | RepairAction::AdoptNewOrderIdentity
            | RepairAction::RepairPackageLocator => false,
        };

        let (_tree, _root, directory) = package_with_order("repair-mapping");
        damage_shape(&directory);
        let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
        let outcome = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, action),
            &RepairResolution::ReplacementRecord(valid_replacement()),
            &RealFileSystem,
        );

        if owned_here {
            // The one case diagnosed here authorizes only the replacement, so the
            // promotion is refused as unauthorized rather than unsupported.
            match action {
                RepairAction::ReplaceManagedRecord => {
                    assert!(outcome.is_ok(), "{action:?} is executed by this ticket");
                }
                _ => assert!(
                    matches!(outcome, Err(MetadataRepairError::UnauthorizedAction { .. })),
                    "{action:?} is owned here but this case does not authorize it: {outcome:?}"
                ),
            }
        } else {
            assert!(
                matches!(
                    outcome,
                    Err(MetadataRepairError::UnauthorizedAction { .. })
                        | Err(MetadataRepairError::UnsupportedAction { .. })
                ),
                "{action:?} belongs to another capability and is refused here: {outcome:?}"
            );
        }
    }
}

#[test]
fn an_action_this_capability_does_not_own_is_refused_even_when_the_case_authorizes_it() {
    // A Vendor case genuinely authorizes three semantic resolutions. None of them may
    // be executed here, so the refusal must be "unsupported", not "unauthorized".
    let (_tree, root, directory) = package_with_order("repair-unsupported");
    let text = fs::read_to_string(record(&directory)).unwrap().replace(
        "\"items\": []",
        "\"vendor_id\": \"00000000-0000-4000-8000-000000000000\",\n  \"items\": []",
    );
    fs::write(record(&directory), text).unwrap();
    let handle = diagnosed_handle(&directory, DiagnosticCode::VendorReferenceUnresolved);
    let before = fs::read(record(&directory)).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    for action in [
        RepairAction::AssignExistingVendor,
        RepairAction::CreateAndAssignVendor,
        RepairAction::ClearVendorReference,
    ] {
        let error = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, action),
            &RepairResolution::None,
            &RealFileSystem,
        )
        .expect_err("a semantic resolution is not applied by this capability");
        assert!(
            matches!(error, MetadataRepairError::UnsupportedAction { .. }),
            "{action:?} is refused as unsupported here: {error:?}"
        );
    }
    assert_eq!(fs::read(record(&directory)).unwrap(), before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_confirmed_valid_replacement_restores_the_scope_and_touches_nothing_else() {
    let (_tree, _root, directory) = package_with_order("repair-replacement");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let readme_before = fs::read(directory.join("README.md")).unwrap();
    let sentinel_before = fs::read(directory.join("quote.bin")).unwrap();
    let basename_before = directory.file_name().unwrap().to_owned();

    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .expect("a confirmed valid replacement is planned");

    assert_eq!(plan.handle(), handle);
    assert_eq!(plan.action(), RepairAction::ReplaceManagedRecord);
    assert_eq!(plan.artifact(), record(&directory));
    assert_eq!(plan.source(), None);
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            MetadataRepairOperationKind::ReplaceManagedArtifact,
            MetadataRepairOperationKind::RefreshPackageIndex,
        ]
    );
    // A dry run performs none of it.
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
        br#"{"schema_version":1,"nope":true}"#
    );

    let report = commit_metadata_repair(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a confirmed valid replacement commits: {report:?}");
    };
    let remaining = result.remaining().expect("the scope rediagnoses");
    assert!(remaining.is_valid(), "the repaired scope is healthy");
    assert!(remaining.diagnostics().is_empty());
    assert!(result.index().is_some());

    let repaired = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.as_str(), "Repaired components");
    assert_eq!(repaired.order_id.as_str(), ORDER_ID);

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
fn refreshed_index(plan: &spielorders_core::metadata_repair::MetadataRepairPlan) -> PathBuf {
    plan.operations()
        .iter()
        .find(|operation| operation.kind() == MetadataRepairOperationKind::RefreshPackageIndex)
        .expect("a repair refreshes the derived index")
        .path()
        .to_path_buf()
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
                RepairResolution::ReplacementRecord(valid_replacement_with("../../..", ORDER_ID)),
            )
        } else {
            stage_candidate(&directory, promoted_candidate("../../.."));
            (
                DiagnosticCode::StagingArtifactPresent,
                RepairResolution::None,
            )
        };
        let handle = diagnosed_handle(&directory, code);

        let plan = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, action),
            &resolution,
            &RealFileSystem,
        )
        .unwrap_or_else(|error| panic!("{label} is planned under a nested Orders path: {error:?}"));

        assert_eq!(
            refreshed_index(&plan),
            root.join(".spielorders/index.json"),
            "{label} names the real Package's index rather than an inferred ancestor"
        );

        let report = commit_metadata_repair(plan, &RealFileSystem);

        let MutationReport::Committed { result, .. } = report else {
            panic!("{label} commits without warning under a nested Orders path: {report:?}");
        };
        assert!(
            result
                .remaining()
                .expect("the scope rediagnoses")
                .is_valid(),
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
    let before = fs::read(record(&directory)).unwrap();
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
        let error = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
            &RepairResolution::ReplacementRecord(candidate),
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("{label} must be refused rather than salvaged"));
        assert!(
            matches!(error, MetadataRepairError::InvalidReplacement { .. }),
            "{label} is refused as an invalid replacement: {error:?}"
        );
    }

    assert_eq!(
        fs::read(record(&directory)).unwrap(),
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
        let before = fs::read(record(&directory)).unwrap();
        let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

        let error = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
            &RepairResolution::ReplacementRecord(candidate),
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("{label} must be refused"));

        assert!(
            matches!(
                error,
                MetadataRepairError::ReplacementDamagesPackage { constraint: broken, .. }
                    if broken == constraint
            ),
            "{label} is refused by the name of the constraint it broke: {error:?}"
        );
        assert_eq!(
            fs::read(record(&directory)).unwrap(),
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
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .unwrap();

    // Someone else corrects the record between the plan and its write. The confirmed
    // replacement was authorized against the damaged bytes, not against theirs.
    let theirs = someone_elses_record();
    let filesystem =
        ConcurrentWriter::replacing(&record(&directory), &theirs, EditPoint::StagingWrite);

    let report = commit_metadata_repair(plan, &filesystem);

    assert!(filesystem.fired(), "the outside write really landed");
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a changed destination is refused: {report:?}");
    };
    assert!(
        matches!(
            rejection,
            MetadataRepairRejection::DestinationChanged { .. }
        ),
        "a changed destination is reported apart from a write failure: {rejection:?}"
    );
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
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
fn promoting_the_diagnosed_candidate_completes_its_interrupted_replacement() {
    let (_tree, _root, directory) = package_with_order("repair-promotion");
    stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let readme_before = fs::read(directory.join("README.md")).unwrap();

    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect("the diagnosed candidate can be promoted");

    assert_eq!(plan.action(), RepairAction::PromoteStagingCandidate);
    assert_eq!(
        plan.source(),
        Some(directory.join(STAGE).as_path()),
        "the plan names the exact diagnosed candidate"
    );
    assert_eq!(
        plan.artifact(),
        record(&directory),
        "and the destination it belongs to, which it never has to choose"
    );
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            MetadataRepairOperationKind::PromoteStagingCandidate,
            MetadataRepairOperationKind::RefreshPackageIndex,
        ]
    );
    // A dry run promotes nothing.
    assert!(directory.join(STAGE).exists());

    let report = commit_metadata_repair(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("an exact validated promotion commits: {report:?}");
    };
    let promoted = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.as_str(), "Promoted components");
    assert!(
        !directory.join(STAGE).exists(),
        "the candidate became the record rather than being cleaned up separately"
    );
    let remaining = result.remaining().expect("the scope rediagnoses");
    assert!(remaining.is_valid());
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
        let record_before = fs::read(record(&directory)).unwrap();

        let error = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
            &RepairResolution::None,
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("a {label} candidate must not be promoted"));

        assert!(
            matches!(error, MetadataRepairError::InvalidStagingCandidate { .. }),
            "a {label} candidate is refused: {error:?}"
        );
        // Both the canonical record and the abandoned evidence survive untouched.
        assert_eq!(fs::read(record(&directory)).unwrap(), record_before);
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
    let before = fs::read(record(&directory)).unwrap();

    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("a candidate that would damage the Package must not be promoted");

    assert!(
        matches!(
            error,
            MetadataRepairError::StagingCandidateDamagesPackage {
                constraint: PackageConstraint::OrderIdentityTaken,
                ..
            }
        ),
        "the refusal names the Package constraint it broke: {error:?}"
    );
    assert_eq!(fs::read(record(&directory)).unwrap(), before);
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
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .unwrap();
    let record_before = fs::read(record(&directory)).unwrap();

    // Someone rewrites the stage between planning and commit. It is no longer the
    // candidate that was diagnosed and validated.
    fs::write(directory.join(STAGE), b"{ not json").unwrap();

    let report = commit_metadata_repair(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a changed stage is not promoted: {report:?}");
    };
    assert!(
        matches!(rejection, MetadataRepairRejection::EvidenceChanged { .. }),
        "a changed stage is a changed-evidence rejection: {rejection:?}"
    );
    assert_eq!(fs::read(record(&directory)).unwrap(), record_before);
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), b"{ not json");
}

#[test]
fn a_promotion_whose_destination_changed_after_diagnosis_preserves_it() {
    let (_tree, _root, directory) = package_with_order("repair-promote-destination");
    let staged = stage_valid_candidate(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .unwrap();

    // Someone else writes the record between the plan and the promotion. Completing
    // an interrupted write is not licence to discard work that arrived since.
    let theirs = someone_elses_record();
    let filesystem =
        ConcurrentWriter::replacing(&record(&directory), &theirs, EditPoint::ReadEndingIn(STAGE));

    let report = commit_metadata_repair(plan, &filesystem);

    assert!(filesystem.fired(), "the outside write really landed");
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a changed destination is refused: {report:?}");
    };
    assert!(
        matches!(
            rejection,
            MetadataRepairRejection::DestinationChanged { .. }
        ),
        "a changed destination is reported apart from a write failure: {rejection:?}"
    );
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
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
    fs::remove_file(record(directory)).unwrap();
    staged
}

#[test]
fn a_candidate_beside_a_missing_record_is_promoted_into_place() {
    let (_tree, _root, directory) = package_with_order("repair-promote-absent");
    stage_beside_a_missing_record(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);

    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect("a candidate beside a missing record can be promoted");

    let report = commit_metadata_repair(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("promoting into a destination nothing occupies commits: {report:?}");
    };
    assert!(
        result
            .remaining()
            .expect("the scope rediagnoses")
            .is_valid()
    );
    let promoted = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.as_str(), "Promoted components");
    assert!(!directory.join(STAGE).exists());
}

#[test]
fn a_record_that_appeared_where_the_diagnosis_found_none_is_not_overwritten() {
    let (_tree, _root, directory) = package_with_order("repair-promote-appeared");
    let staged = stage_beside_a_missing_record(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent);
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .unwrap();

    // Someone else restores the record while this promotion is under way. An empty
    // destination is an observation like any other, and theirs is not the emptiness
    // this promotion was authorized against.
    let theirs = someone_elses_record();
    let filesystem =
        ConcurrentWriter::replacing(&record(&directory), &theirs, EditPoint::ReadEndingIn(STAGE));

    let report = commit_metadata_repair(plan, &filesystem);

    assert!(filesystem.fired(), "the outside write really landed");
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a destination that stopped being empty is refused: {report:?}");
    };
    assert!(
        matches!(
            rejection,
            MetadataRepairRejection::DestinationChanged { .. }
        ),
        "an occupied destination is reported apart from a write failure: {rejection:?}"
    );
    assert_eq!(fs::read(record(&directory)).unwrap(), theirs);
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
    fs::write(record(&directory), valid_replacement()).unwrap();
    stage_valid_candidate(&directory);
    let standing = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert!(
        standing
            .cases()
            .iter()
            .any(|case| case.code() == DiagnosticCode::StagingArtifactPresent),
        "a different case is standing when the stale handle is presented"
    );
    let before = fs::read(record(&directory)).unwrap();
    let stage_before = fs::read(directory.join(STAGE)).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .expect_err("a stale handle is rejected");

    assert!(
        matches!(error, MetadataRepairError::UnknownCase { .. }),
        "a stale handle names no current case: {error:?}"
    );
    assert_eq!(fs::read(record(&directory)).unwrap(), before);
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
    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(
            "managed_artifact_invalid_shape:Orders/elsewhere/.spielorders/order.json",
            RepairAction::ReplaceManagedRecord,
        ),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .expect_err("an invented handle is rejected");
    assert!(matches!(error, MetadataRepairError::UnknownCase { .. }));
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
    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&stage_handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .expect_err("a confirmation must match the case it names");
    assert!(
        matches!(error, MetadataRepairError::UnauthorizedAction { .. }),
        "got {error:?}"
    );

    // And the replacement handle does not authorize a promotion.
    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&shape_handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("a confirmation must match the case it names");
    assert!(
        matches!(error, MetadataRepairError::UnauthorizedAction { .. }),
        "got {error:?}"
    );

    // A replacement stages beside its destination, and the abandoned candidate is
    // sitting exactly there. Carrying it out would destroy evidence this core may
    // neither overwrite nor silently clean, so it is refused by name rather than
    // failing opaquely at the staging write.
    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&shape_handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .expect_err("a replacement never writes over abandoned staging evidence");
    assert!(
        matches!(error, MetadataRepairError::StagingEvidencePresent { .. }),
        "got {error:?}"
    );
    assert_eq!(fs::read(directory.join(STAGE)).unwrap(), staged);

    // Confirming the staging case carries out only that case.
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&stage_handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .unwrap();
    let report = commit_metadata_repair(plan, &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the confirmed case commits: {report:?}"
    );
    assert!(!directory.join(STAGE).exists());
    // The promoted record is the candidate's, not the replacement's: one call carried
    // out one case and never touched the other's resolution.
    let promoted = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the promoted record parses");
    assert_eq!(promoted.title.as_str(), "Promoted components");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepairBoundary {
    WriteStage,
    Replace,
    Promote,
    IndexReplacement,
    DamagePackageAfterRepair,
}

/// Injects one failure into a repair. Every other effect is real.
struct FailingRepair {
    boundary: RepairBoundary,
    repaired: std::cell::Cell<bool>,
}

impl FailingRepair {
    fn at(boundary: RepairBoundary) -> Self {
        Self {
            boundary,
            repaired: std::cell::Cell::new(false),
        }
    }
}

impl spielorders_core::ports::FileSystem for FailingRepair {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if self.repaired.get()
            && self.boundary == RepairBoundary::DamagePackageAfterRepair
            && path.ends_with("vendors.json")
        {
            return Ok(b"{ not json".to_vec());
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if self.boundary == RepairBoundary::WriteStage && path.ends_with(STAGE) {
            return Err(std::io::Error::other("injected staging write failure"));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/order.json") {
            if self.boundary == RepairBoundary::Replace && source.ends_with(STAGE) {
                return Err(std::io::Error::other("injected replacement failure"));
            }
            if self.boundary == RepairBoundary::Promote {
                return Err(std::io::Error::other("injected promotion failure"));
            }
            self.repaired.set(true);
        }
        if self.boundary == RepairBoundary::IndexReplacement
            && destination.ends_with(".spielorders/index.json")
        {
            return Err(std::io::Error::other("injected index replacement failure"));
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
fn an_injected_replacement_failure_preserves_the_prior_canonical_bytes() {
    for boundary in [RepairBoundary::WriteStage, RepairBoundary::Replace] {
        let (_tree, _root, directory) = package_with_order("repair-replace-failure");
        damage_shape(&directory);
        let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
        let before = fs::read(record(&directory)).unwrap();
        let plan = plan_metadata_repair(
            &directory,
            &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
            &RepairResolution::ReplacementRecord(valid_replacement()),
            &RealFileSystem,
        )
        .unwrap();

        let report = commit_metadata_repair(plan, &FailingRepair::at(boundary));

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        assert!(
            matches!(rejection, MetadataRepairRejection::ReplacementFailed { .. }),
            "an injected {boundary:?} failure is a replacement failure: {rejection:?}"
        );
        assert_eq!(
            fs::read(record(&directory)).unwrap(),
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
    let before = fs::read(record(&directory)).unwrap();
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .unwrap();

    let report = commit_metadata_repair(plan, &FailingRepair::at(RepairBoundary::Promote));

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an injected promotion failure rejects: {report:?}");
    };
    assert!(
        matches!(rejection, MetadataRepairRejection::PromotionFailed { .. }),
        "an injected promotion failure names itself: {rejection:?}"
    );
    assert_eq!(fs::read(record(&directory)).unwrap(), before);
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
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .unwrap();

    let report = commit_metadata_repair(plan, &FailingRepair::at(RepairBoundary::IndexReplacement));

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed index refresh remains a committed repair: {report:?}");
    };
    assert!(matches!(
        warnings.as_slice(),
        [MetadataRepairWarning::IndexRefreshFailed(_)]
    ));
    // The canonical repair stands and the scope is healthy again.
    let remaining = result.remaining().expect("the scope rediagnoses");
    assert!(remaining.is_valid());
    let repaired = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.as_str(), "Repaired components");
}

#[test]
fn a_repair_that_cannot_be_rediagnosed_still_reports_itself_committed() {
    let (_tree, _root, directory) = package_with_order("repair-rediagnosis-failure");
    damage_shape(&directory);
    let handle = diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape);
    let plan = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(valid_replacement()),
        &RealFileSystem,
    )
    .unwrap();

    // The record is replaced, and only then does package-wide state stop loading, so
    // no remaining-diagnostic answer can be produced.
    let report = commit_metadata_repair(
        plan,
        &FailingRepair::at(RepairBoundary::DamagePackageAfterRepair),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the repair happened, so the report is committed: {report:?}");
    };
    assert!(
        warnings.contains(&MetadataRepairWarning::RediagnosisUnavailable),
        "got {warnings:?}"
    );
    assert_eq!(
        result.remaining(),
        None,
        "no remaining-diagnostic answer is invented when the scope cannot be reread"
    );
    let repaired = spielorders_core::metadata::OrderDocument::from_json_bytes(
        &fs::read(record(&directory)).unwrap(),
    )
    .expect("the repaired record parses");
    assert_eq!(repaired.title.as_str(), "Repaired components");
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

    let standing = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert!(
        standing
            .cases()
            .iter()
            .any(|case| case.code() == DiagnosticCode::StagingArtifactPresent),
        "the same condition is still standing at the same path: {:?}",
        standing.cases()
    );
    let record_before = fs::read(record(&directory)).unwrap();

    let error = plan_metadata_repair(
        &directory,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("a confirmation whose evidence moved is rejected");

    assert!(
        matches!(error, MetadataRepairError::UnknownCase { .. }),
        "a confirmation naming evidence that has moved names no current case: {error:?}"
    );
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
        record_before,
        "the candidate the user never confirmed was not promoted"
    );
    assert_eq!(
        fs::read(directory.join(STAGE)).unwrap(),
        replaced,
        "and the standing candidate was left exactly as it was found"
    );
}
