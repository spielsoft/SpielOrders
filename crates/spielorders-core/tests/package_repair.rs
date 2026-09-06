//! Diagnosing and applying the repairs a Package's own managed artifacts authorize.
//!
//! Decision sources: Define integrity, validation, and repair boundaries; Define the
//! Rust CLI capability contract; Define durable metadata schemas and version evolution.

use crate::support::{
    ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree, initialized_package,
};
use spielorders_core::mutation::MutationReport;
use spielorders_core::package_diagnosis::diagnose_package;
use spielorders_core::package_repair::{
    PackageRepairError, PackageRepairOperationKind, PackageRepairRejection, commit_package_repair,
    plan_package_repair,
};
use spielorders_core::readme::DiagnosticCode;
use spielorders_core::repair_cases::RepairAction;
use spielorders_core::{
    metadata_repair::{RepairConfirmation, RepairResolution},
    readme::Severity,
};
use std::fs;
use std::path::{Path, PathBuf};

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

fn handle(root: &Path, code: DiagnosticCode) -> String {
    let diagnosis = diagnose_package(root, &RealFileSystem).expect("the Package can be diagnosed");
    diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == code)
        .unwrap_or_else(|| panic!("{code:?} is diagnosed: {:?}", diagnosis.cases()))
        .handle()
        .to_owned()
}

/// The Package-level case the integrity decision makes actionable: a leftover staging
/// artifact beside a managed Package artifact, promotable on explicit confirmation.
#[test]
fn a_leftover_stage_beside_a_package_artifact_is_a_diagnosed_case() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-diagnosis");

    let diagnosis = diagnose_package(&root, &RealFileSystem).expect("the Package can be diagnosed");

    assert!(
        diagnosis.is_valid(),
        "a stage beside a valid destination is cleanup, not damage: {:?}",
        diagnosis.diagnostics()
    );
    let case = diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == DiagnosticCode::StagingArtifactPresent)
        .expect("the leftover stage authorizes a case");
    assert_eq!(case.actions(), [RepairAction::PromoteStagingCandidate]);
    assert_eq!(
        case.location().absolute_path(),
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

    let diagnosis = diagnose_package(&root, &RealFileSystem).expect("the Package can be diagnosed");

    assert!(
        diagnosis.cases().is_empty(),
        "the actionable case is inside an Order: {:?}",
        diagnosis.cases()
    );
    assert!(
        diagnosis.diagnostics().iter().all(
            |diagnostic| diagnostic.scope() == spielorders_core::readme::EvidenceScope::Package
        ),
        "and only Package-scope findings are reported: {:?}",
        diagnosis.diagnostics()
    );
}

/// A promotion completes the interrupted write: the candidate becomes the artifact, the
/// leftover stage is consumed, and the finding is gone afterwards.
#[test]
fn promoting_a_validated_candidate_completes_the_interrupted_write() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-promote");
    let handle = handle(&root, DiagnosticCode::StagingArtifactPresent);

    let plan = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect("the diagnosed candidate can be promoted");
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>(),
        [
            PackageRepairOperationKind::PromoteStagingCandidate,
            PackageRepairOperationKind::RefreshPackageIndex,
        ]
    );

    let report = commit_package_repair(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
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
        result
            .remaining()
            .expect("the scope is diagnosed again")
            .cases()
            .is_empty(),
        "the finding is gone"
    );
}

/// A candidate this build cannot read as the artifact it would become is never promoted.
/// The adapter may not judge which candidate is safe, so the judgement is here — and it
/// is made per artifact kind.
#[test]
fn a_candidate_that_is_not_the_artifact_it_would_become_is_never_promoted() {
    let (_tree, root) = initialized_package("package-repair-invalid-candidate");
    fs::write(
        root.join(VENDORS_STAGE),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": \"nope\"\n}\n",
    )
    .unwrap();
    let before = fs::read(root.join(VENDORS)).unwrap();
    let handle = handle(&root, DiagnosticCode::StagingArtifactPresent);

    let refusal = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("a candidate that is not a Vendor aggregate is refused");

    assert!(
        matches!(refusal, PackageRepairError::InvalidStagingCandidate { .. }),
        "{refusal:?}"
    );
    assert_eq!(
        fs::read(root.join(VENDORS)).unwrap(),
        before,
        "and nothing is written"
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
    let handle = handle(&root, DiagnosticCode::ManagedArtifactInvalidShape);

    let refusal = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(b"{ still not a record".to_vec()),
        &RealFileSystem,
    )
    .expect_err("a replacement that is not a Vendor aggregate is refused");
    assert!(
        matches!(refusal, PackageRepairError::InvalidReplacement { .. }),
        "{refusal:?}"
    );

    let plan = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::ReplaceManagedRecord),
        &RepairResolution::ReplacementRecord(vendor_aggregate("Tarnwick")),
        &RealFileSystem,
    )
    .expect("a complete valid aggregate is accepted");
    let report = commit_package_repair(plan, &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "{report:?}"
    );
    let diagnosis = diagnose_package(&root, &RealFileSystem).expect("the Package is diagnosed");
    assert!(
        diagnosis.is_valid()
            && !diagnosis
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.severity() == Severity::Error),
        "the Package is valid in its own scope afterwards: {:?}",
        diagnosis.diagnostics()
    );
}

/// A destination that changed between the diagnosis and the commit is refused, the other
/// writer's bytes survive, and the refusal is not a write failure.
#[test]
fn a_destination_that_changed_after_the_diagnosis_is_refused_and_their_bytes_survive() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-race");
    let handle = handle(&root, DiagnosticCode::StagingArtifactPresent);
    let theirs = vendor_aggregate("Nethermoor");
    let filesystem = ConcurrentWriter::replacing(
        &root.join(VENDORS),
        &theirs,
        EditPoint::ReadEndingIn(VENDORS_STAGE),
    );

    // Planned against the Package as the diagnosis found it; the other writer lands
    // between that plan and the commit, which is the window a destination guard closes.
    let plan = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect("the diagnosed candidate can be promoted");
    let report = commit_package_repair(plan, &filesystem);

    assert!(filesystem.fired(), "the other writer landed");
    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("the promotion is refused: {report:?}");
    };
    assert!(
        matches!(rejection, PackageRepairRejection::DestinationChanged { .. }),
        "nothing failed — the destination stopped being the one this was authorized \
         against: {rejection:?}"
    );
    assert_eq!(
        fs::read(root.join(VENDORS)).unwrap(),
        theirs,
        "and their bytes survive exactly"
    );
}

/// A handle from a diagnosis taken before the Package changed no longer names a case.
#[test]
fn a_handle_from_a_stale_diagnosis_is_refused_rather_than_resolved() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-stale");
    let handle = handle(&root, DiagnosticCode::StagingArtifactPresent);
    fs::write(root.join(VENDORS_STAGE), vendor_aggregate("Nethermoor")).unwrap();

    let refusal = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::PromoteStagingCandidate),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("a handle naming evidence that has moved is refused");

    assert!(
        matches!(refusal, PackageRepairError::UnknownCase { .. }),
        "{refusal:?}"
    );
}

/// An action the diagnosed case did not offer is refused rather than attempted.
#[test]
fn an_action_the_case_did_not_offer_is_refused() {
    let (_tree, root) = package_with_a_leftover_vendor_stage("package-repair-unauthorized");
    let handle = handle(&root, DiagnosticCode::StagingArtifactPresent);

    let refusal = plan_package_repair(
        &root,
        &RepairConfirmation::new(&handle, RepairAction::ClearVendorReference),
        &RepairResolution::None,
        &RealFileSystem,
    )
    .expect_err("the case authorizes one action and this is not it");

    assert!(
        matches!(refusal, PackageRepairError::UnauthorizedAction { .. }),
        "{refusal:?}"
    );
}
