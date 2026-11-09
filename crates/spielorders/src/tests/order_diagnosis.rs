//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping; Define durable metadata schemas and version evolution.

use crate::Failure;
use crate::tests::support::{
    FixedUuid, Operation, PlatformFileSystem, ScriptedFileSystem, TemporaryTree, adoption, case,
    created_vendor, diagnosed_handle, diagnosed_order, effects, findings, initialized_package,
    json, order_record, valid, validated_order, validated_package,
};
use crate::{Assignments, Diagnosis, Report};

use crate::repair_cases::RepairAction;
use crate::{readme::DiagnosticCode, readme::EvidenceScope};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const OTHER_ORDER_ID: &str = "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));

/// An adopted Order carrying a README and one opaque sentinel, so every diagnosis can
/// also be shown never to read Supporting Material.
fn adopted_order(root: &Path, basename: &str, uuid: &str, vendor: Option<&str>) -> PathBuf {
    let directory = root.join("Orders").join(basename);
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    let mut input = (
        Assignments::new()
            .assign("title", "Laser components")
            .assign("date_ordered", "2026-08-22")
            .assign("status", "hold"),
        Vec::new(),
    );
    if let Some(vendor) = vendor {
        input.0 = input.0.assign("vendor", vendor);
    }
    let plan = crate::plan(
        &adoption(root, &directory, &input),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(uuid.into())),
    )
    .result;
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "the fixture Order commits: {report:?}"
    );
    directory
}

/// The values the case in `diagnosis` offering `action` requires for it.
fn required(diagnosis: &Diagnosis, action: RepairAction) -> serde_json::Value {
    assert!(
        diagnosis
            .cases
            .iter()
            .any(|case| case.actions.contains(&action)),
        "a case offers {action:?}"
    );
    json(&action.required_values())
}

fn codes(diagnosis: &Diagnosis) -> Vec<DiagnosticCode> {
    findings(diagnosis)
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn examined(diagnosis: &Diagnosis) -> &Path {
    &diagnosis.examined_orders[0].directory.absolute_path
}

fn actions(diagnosis: &Diagnosis, code: DiagnosticCode) -> Vec<RepairAction> {
    diagnosis
        .cases
        .iter()
        .find(|case| case.code == code)
        .unwrap_or_else(|| panic!("{code:?} has a repair case: {:?}", diagnosis.cases))
        .actions
        .to_vec()
}

/// Damages an Order in one of the shapes the integrity decision enumerates.
fn damage(directory: &Path, shape: &str, root: &Path) {
    match shape {
        "missing" => fs::remove_file(order_record(directory)).unwrap(),
        "malformed" => fs::write(order_record(directory), b"{ not json").unwrap(),
        "invalid shape" => fs::write(
            order_record(directory),
            br#"{"schema_version":1,"nope":true}"#,
        )
        .unwrap(),
        "unsupported version" => {
            let text = fs::read_to_string(order_record(directory))
                .unwrap()
                .replace("\"schema_version\": 1", "\"schema_version\": 2");
            fs::write(order_record(directory), text).unwrap();
        }
        "semantically invalid" => {
            let text = fs::read_to_string(order_record(directory))
                .unwrap()
                .replace("2026-08-22", "2026-02-30");
            fs::write(order_record(directory), text).unwrap();
        }
        "unresolved vendor" => {
            let text = fs::read_to_string(order_record(directory))
                .unwrap()
                .replace(VENDOR_ID, "00000000-0000-4000-8000-000000000000");
            fs::write(order_record(directory), text).unwrap();
        }
        "stale locator" => {
            let text = fs::read_to_string(order_record(directory))
                .unwrap()
                .replace("\"../..\"", "\"../../..\"");
            fs::write(order_record(directory), text).unwrap();
        }
        "staging" => {
            fs::write(
                directory.join(".spielorders/.order.json.spielorders-stage"),
                fs::read(order_record(directory)).unwrap(),
            )
            .unwrap();
        }
        "duplicate id" => {
            // A second physical Order carrying this Order's identity.
            let twin = adopted_order(root, "2026_08_22_Twin", OTHER_ORDER_ID, None);
            let text = fs::read_to_string(order_record(&twin))
                .unwrap()
                .replace(OTHER_ORDER_ID, ORDER_ID);
            fs::write(order_record(&twin), text).unwrap();
        }
        other => panic!("unknown damage shape {other}"),
    }
}

#[test]
fn every_damaged_order_cause_the_integrity_decision_enumerates_is_diagnosed() {
    // The closed integrity decision enumerates the Damaged Order causes: a missing,
    // unreadable, malformed, unsupported-version, or semantically invalid
    // `order.json`; an unresolved Vendor Reference; or a duplicate Order ID. An
    // enumeration in a decision is a checklist, not a menu, so every one is exercised
    // here rather than a representative sample. Each reuses the code the Package-wide
    // read model already publishes for that condition, never a second vocabulary for
    // the same evidence.
    for (shape, expected) in [
        ("missing", DiagnosticCode::ManagedArtifactMissing),
        ("malformed", DiagnosticCode::ManagedArtifactInvalidSyntax),
        ("invalid shape", DiagnosticCode::ManagedArtifactInvalidShape),
        (
            "unsupported version",
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
        ("semantically invalid", DiagnosticCode::OrderSemanticInvalid),
        (
            "unresolved vendor",
            DiagnosticCode::VendorReferenceUnresolved,
        ),
        ("duplicate id", DiagnosticCode::OrderIdDuplicate),
    ] {
        let (_tree, root) = initialized_package("diagnosis-enumeration");
        created_vendor(&root, "Tarnwick", VENDOR_ID);
        let directory = adopted_order(
            &root,
            "2026_08_22_Laser_components",
            ORDER_ID,
            Some("Tarnwick"),
        );
        damage(&directory, shape, &root);

        let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .unwrap_or_else(|error| panic!("{shape} is diagnosable: {error:?}"));

        assert!(
            codes(&diagnosis).contains(&expected),
            "{shape} reports {expected:?}, got {:?}",
            codes(&diagnosis)
        );
        assert!(!valid(&diagnosis), "{shape} is not a valid Order");
        assert_eq!(examined(&diagnosis), directory, "{shape}");
    }
}

#[test]
fn an_unreadable_record_is_diagnosed_as_the_decision_names_it() {
    // This cause needs an injected read failure rather than a filesystem state.
    let (_tree, root) = initialized_package("diagnosis-unreadable");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let before = fs::read(order_record(&directory)).unwrap();

    let unreadable = ScriptedFileSystem::new().failing(Operation::Read, order_record(&directory));
    let diagnosis = diagnosed_order(&directory, &unreadable)
        .result
        .expect("an unreadable record is diagnosable");

    assert!(
        codes(&diagnosis).contains(&DiagnosticCode::ManagedArtifactUnreadable),
        "an unreadable record reports its own code, got {:?}",
        codes(&diagnosis)
    );
    assert!(!valid(&diagnosis));
    assert!(diagnosis.cases.is_empty());
    assert_eq!(fs::read(order_record(&directory)).unwrap(), before);
}

#[test]
fn a_healthy_order_reports_no_diagnostic_and_no_case() {
    let (_tree, root) = initialized_package("diagnosis-healthy");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let directory = adopted_order(
        &root,
        "2026_08_22_Laser_components",
        ORDER_ID,
        Some("Tarnwick"),
    );

    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("a healthy Order is diagnosable");

    assert!(valid(&diagnosis));
    assert_eq!(codes(&diagnosis), Vec::<DiagnosticCode>::new());
    assert!(diagnosis.cases.is_empty());
}

/// Validating an Order scope with an error refuses as `order_invalid`, carrying the repairs
/// it offers, with the findings beside the refusal; diagnosing the same scope answers.
#[test]
fn an_invalid_order_fails_validation_and_answers_diagnosis() {
    let (_tree, root) = initialized_package("diagnosis-validation-verdict");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let directory = adopted_order(
        &root,
        "2026_08_22_Laser_components",
        ORDER_ID,
        Some("Tarnwick"),
    );
    damage(&directory, "unresolved vendor", &root);

    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("an invalid Order is diagnosable");
    let validation = validated_order(&directory, &PlatformFileSystem);

    assert_eq!(
        validation.result,
        Err(Failure::OrderInvalid(Box::new(diagnosis))),
        "the verdict carries the diagnosis that found it"
    );
    assert!(validation.diagnostics.is_empty());

    let healthy = adopted_order(&root, "2026_08_23_Optics", OTHER_ORDER_ID, Some("Tarnwick"));
    assert!(
        validated_order(&healthy, &PlatformFileSystem)
            .result
            .is_ok()
    );
}

#[test]
fn every_actionable_family_offers_only_actions_a_repair_capability_executes() {
    // Diagnosis is the input to the repair capabilities, so each case must name only
    // actions one of them can actually execute. The mapping below is the handoff:
    // ReplaceManagedRecord and PromoteStagingCandidate are owned by the confirmed
    // managed-metadata/staging repair; the Vendor, duplicate-identity, and locator
    // resolutions are owned by the explicit semantic repair.
    let (_tree, root) = initialized_package("diagnosis-actionable");
    created_vendor(&root, "Tarnwick", VENDOR_ID);

    // Parseable but schema-invalid metadata: one confirmed complete replacement.
    let directory = adopted_order(&root, "2026_08_22_Shape", ORDER_ID, Some("Tarnwick"));
    damage(&directory, "invalid shape", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::ManagedArtifactInvalidShape),
        [RepairAction::ReplaceManagedRecord]
    );
    assert_eq!(
        required(&diagnosis, RepairAction::ReplaceManagedRecord),
        json(&["record"]),
        "a replacement is supplied by the user, never guessed"
    );

    // Semantically invalid but parseable: the same single safe action.
    let (_tree, root) = initialized_package("diagnosis-actionable-semantic");
    let directory = adopted_order(&root, "2026_08_22_Semantic", ORDER_ID, None);
    damage(&directory, "semantically invalid", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::OrderSemanticInvalid),
        [RepairAction::ReplaceManagedRecord]
    );

    // An abandoned staging artifact: promote exactly that candidate.
    let (_tree, root) = initialized_package("diagnosis-actionable-staging");
    let directory = adopted_order(&root, "2026_08_22_Staging", ORDER_ID, None);
    damage(&directory, "staging", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::StagingArtifactPresent),
        [RepairAction::PromoteStagingCandidate]
    );
    assert!(
        required(&diagnosis, RepairAction::PromoteStagingCandidate) == serde_json::json!([]),
        "the diagnosed candidate is the only one that may be promoted"
    );

    // An unresolved Vendor Reference: exactly the three authorized resolutions, and
    // never an inferred one.
    let (_tree, root) = initialized_package("diagnosis-actionable-vendor");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let directory = adopted_order(&root, "2026_08_22_Vendor", ORDER_ID, Some("Tarnwick"));
    damage(&directory, "unresolved vendor", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::VendorReferenceUnresolved),
        [
            RepairAction::AssignExistingVendor,
            RepairAction::CreateAndAssignVendor,
            RepairAction::ClearVendorReference,
        ]
    );
    assert_eq!(
        required(&diagnosis, RepairAction::AssignExistingVendor),
        json(&["vendor"])
    );
    assert_eq!(
        required(&diagnosis, RepairAction::CreateAndAssignVendor),
        json(&["name"])
    );

    // A duplicate Order ID: adopt a fresh identity for the chosen physical Order.
    let (_tree, root) = initialized_package("diagnosis-actionable-duplicate");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    damage(&directory, "duplicate id", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::OrderIdDuplicate),
        [RepairAction::AdoptNewOrderIdentity]
    );

    // A stale technical locator: recompute it from the physical relationship.
    let (_tree, root) = initialized_package("diagnosis-actionable-locator");
    let directory = adopted_order(&root, "2026_08_22_Locator", ORDER_ID, None);
    damage(&directory, "stale locator", &root);
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::OrderPackageLocatorMismatch),
        [RepairAction::RepairPackageLocator]
    );
}

#[test]
fn no_unsafe_family_exposes_an_invented_repair() {
    // Malformed JSON is never partially salvaged, an unsupported version is never
    // interpreted, and missing metadata is never adopted. Each is reported as a
    // finding with no action rather than omitted, so a caller can tell "healthy" from
    // "nothing safe can be done here".
    for (shape, expected) in [
        ("malformed", DiagnosticCode::ManagedArtifactInvalidSyntax),
        (
            "unsupported version",
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
        ("missing", DiagnosticCode::ManagedArtifactMissing),
    ] {
        let (_tree, root) = initialized_package("diagnosis-unsafe");
        let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
        damage(&directory, shape, &root);

        let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
            .result
            .unwrap();

        assert!(
            diagnosis.cases.is_empty(),
            "{shape} exposes no repair case, got {:?}",
            diagnosis.cases
        );
        assert!(
            codes(&diagnosis).contains(&expected),
            "{shape} is reported by its own code"
        );
        assert!(!valid(&diagnosis), "{shape}");
    }
}

#[test]
fn diagnosis_writes_nothing_for_any_shape_and_never_reads_supporting_material() {
    for shape in [
        "healthy",
        "malformed",
        "invalid shape",
        "unsupported version",
        "semantically invalid",
        "unresolved vendor",
        "stale locator",
        "staging",
        "duplicate id",
    ] {
        let (_tree, root) = initialized_package("diagnosis-read-only");
        created_vendor(&root, "Tarnwick", VENDOR_ID);
        let directory = adopted_order(
            &root,
            "2026_08_22_Laser_components",
            ORDER_ID,
            Some("Tarnwick"),
        );
        if shape != "healthy" {
            damage(&directory, shape, &root);
        }
        let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();
        let record_before = fs::read(order_record(&directory)).ok();
        let readme_before = fs::read(directory.join("README.md")).unwrap();
        let sentinel_before = fs::read(directory.join("quote.bin")).unwrap();

        let scripted = ScriptedFileSystem::new();
        let diagnosis = diagnosed_order(&directory, &scripted);

        assert!(diagnosis.result.is_ok(), "{shape} is diagnosable");
        // Diagnosis is read-only: it stages nothing, promotes nothing, replaces no
        // index, and rewrites no record.
        assert_eq!(
            fs::read(root.join(".spielorders/index.json")).unwrap(),
            index_before,
            "{shape} replaces no derived index"
        );
        assert_eq!(
            fs::read(order_record(&directory)).ok(),
            record_before,
            "{shape}"
        );
        assert_eq!(
            fs::read(directory.join("README.md")).unwrap(),
            readme_before,
            "{shape}"
        );
        assert_eq!(
            fs::read(directory.join("quote.bin")).unwrap(),
            sentinel_before,
            "{shape}"
        );
        assert!(
            !directory
                .join(".spielorders/.order.json.spielorders-stage.spielorders-stage")
                .exists(),
            "{shape} stages nothing of its own"
        );
        for read in scripted.paths(Operation::Read) {
            let read = read.to_string_lossy().to_string();
            assert!(
                !read.contains("quote.bin"),
                "{shape} never reads Supporting Material, but read {read}"
            );
        }
    }
}

#[test]
fn diagnosing_one_order_leaves_its_siblings_usable_and_its_own_bytes_untouched() {
    let (_tree, root) = initialized_package("diagnosis-isolation");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let healthy = adopted_order(
        &root,
        "2026_08_22_Healthy",
        OTHER_ORDER_ID,
        Some("Tarnwick"),
    );
    let damaged = adopted_order(&root, "2026_08_22_Damaged", ORDER_ID, Some("Tarnwick"));
    damage(&damaged, "semantically invalid", &root);
    let damaged_before = fs::read(order_record(&damaged)).unwrap();

    let diagnosis = diagnosed_order(&damaged, &PlatformFileSystem)
        .result
        .expect("the damaged Order diagnoses");

    assert!(!valid(&diagnosis));
    assert!(
        codes(&diagnosis).contains(&DiagnosticCode::OrderSemanticInvalid),
        "got {:?}",
        codes(&diagnosis)
    );
    // Its own bytes are evidence, not something diagnosis repairs.
    assert_eq!(fs::read(order_record(&damaged)).unwrap(), damaged_before);

    // The healthy sibling is unaffected, and its own scoped diagnosis excludes the
    // damaged Order's findings entirely.
    let sibling = diagnosed_order(&healthy, &PlatformFileSystem)
        .result
        .expect("the sibling diagnoses");
    assert!(
        valid(&sibling),
        "a damaged Order does not block a healthy one"
    );
    assert_eq!(codes(&sibling), Vec::<DiagnosticCode>::new());
    assert!(sibling.cases.is_empty());
    assert_eq!(examined(&sibling), healthy);

    // And the damaged Order's diagnosis names only its own scope.
    for diagnostic in findings(&diagnosis) {
        assert!(
            diagnostic
                .primary_location
                .absolute_path
                .as_path()
                .starts_with(&damaged),
            "a scoped diagnosis reports only its own Order, but named {:?}",
            diagnostic.primary_location.absolute_path.as_path()
        );
    }
}

#[test]
fn case_handles_are_deterministic_for_a_snapshot_and_disappear_once_corrected() {
    let (_tree, root) = initialized_package("diagnosis-handles");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let healthy_record = fs::read(order_record(&directory)).unwrap();
    damage(&directory, "stale locator", &root);

    let first = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    let second = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();

    let handles = |diagnosis: &Diagnosis| {
        diagnosis
            .cases
            .iter()
            .map(|case| case.handle.to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        handles(&first),
        handles(&second),
        "the same snapshot yields the same handles in the same order"
    );
    let handle = handles(&first).first().cloned().expect("one case");
    let named = "order_package_locator_mismatch:Orders/2026_08_22_Laser_components/.spielorders/order.json:";
    assert!(
        handle.starts_with(named),
        "a handle names the condition and where it is, so a reader can recognize it: \
         {handle}"
    );
    assert!(
        handle.len() > named.len(),
        "and carries the evidence that tells it apart from another condition at that \
         same code and path: {handle}"
    );
    assert!(case(&first, &handle).is_some());
    assert_eq!(
        case(&first, &handle).unwrap().code,
        DiagnosticCode::OrderPackageLocatorMismatch
    );

    // Handles are not persisted identities: once the condition is gone, so is the
    // case, which is what lets a repair capability reject a stale handle.
    fs::write(order_record(&directory), &healthy_record).unwrap();
    let corrected = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .unwrap();
    assert!(valid(&corrected));
    assert!(
        case(&corrected, &handle).is_none(),
        "a corrected condition no longer answers to its old handle"
    );
}

#[test]
fn a_scoped_diagnosis_matches_the_package_wide_evidence_for_the_same_order() {
    let (_tree, root) = initialized_package("diagnosis-parity");
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let subject = adopted_order(&root, "2026_08_22_Subject", ORDER_ID, Some("Tarnwick"));
    let other = adopted_order(&root, "2026_08_22_Other", OTHER_ORDER_ID, Some("Tarnwick"));
    damage(&subject, "semantically invalid", &root);
    damage(&other, "stale locator", &root);

    let diagnosis = diagnosed_order(&subject, &PlatformFileSystem)
        .result
        .unwrap();

    let validation = validated_package(&root, &PlatformFileSystem);
    let Err(Failure::PackageInvalid(validation)) = validation.result else {
        panic!("the damaged Orders make the Package invalid");
    };
    let package_wide = findings(&validation)
        .filter(|diagnostic| {
            diagnostic
                .primary_location
                .absolute_path
                .as_path()
                .starts_with(&subject)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        findings(&diagnosis).collect::<Vec<_>>(),
        package_wide,
        "the scoped result is the same evidence with the same codes, not a parallel set"
    );
    // And it excludes the unrelated Order's findings even though the same pass saw
    // them.
    assert!(
        !codes(&diagnosis).contains(&DiagnosticCode::OrderPackageLocatorMismatch),
        "another Order's condition never appears in this Order's scope"
    );
    assert!(codes(&diagnosis).contains(&DiagnosticCode::OrderSemanticInvalid));
}

#[test]
fn a_directory_that_is_not_an_order_and_a_damaged_package_are_refused() {
    let (_tree, root) = initialized_package("diagnosis-not-an-order");
    let _order = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let stranger = root.join("Orders").join("plain_directory");
    fs::create_dir(&stranger).unwrap();

    // A directory with no Order metadata is still an entry of the Orders Directory,
    // so it is diagnosed rather than refused: the decision gives every ordinary entry
    // a presence as either healthy or damaged.
    let diagnosis = diagnosed_order(&stranger, &PlatformFileSystem)
        .result
        .expect("an ordinary Orders Directory entry has a diagnosis");
    assert!(!valid(&diagnosis));
    assert!(findings(&diagnosis).next().is_some());

    // A directory outside any Package has no Package to validate against.
    let outside = TemporaryTree::new("diagnosis-unmanaged");
    let error = diagnosed_order(outside.path(), &PlatformFileSystem)
        .result
        .expect_err("a directory outside a Package is refused");
    assert!(
        matches!(error, Failure::UnmanagedPackage(_)),
        "an unmanaged location is reported as such: {error:?}"
    );
}

#[test]
fn a_package_wide_failure_is_never_downgraded_to_an_order_finding() {
    let (_tree, root) = initialized_package("diagnosis-damaged-package");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let before = fs::read(order_record(&directory)).unwrap();
    // The Vendor aggregate is package-wide canonical state, and an Order-scoped
    // answer depends on it to resolve Vendor References. With it untrustworthy,
    // reporting this Order as healthy would be a downgrade, not a diagnosis.
    fs::write(root.join(".spielorders/vendors.json"), b"{ not json").unwrap();

    let error = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect_err("an Order inside a Damaged Package is not diagnosed in isolation");

    // The Package-scope findings travel with the refusal, so a caller can report the
    // Package-level problem as a Package-level problem rather than only that something
    // somewhere was wrong.
    let Failure::DamagedPackage(package) = &error else {
        panic!("a Package-wide failure is reported as such: {error:?}")
    };
    assert_eq!(
        package.context.root.absolute_path,
        fs::canonicalize(&root).expect("the Package Root resolves")
    );
    let diagnostics = package
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.scope == EvidenceScope::Package),
        "and only those: a sibling Order's damage is not this scope's to report"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ManagedArtifactInvalidSyntax),
        "the evidence for the refusal travels with it: {diagnostics:?}"
    );
    assert_eq!(
        fs::read(order_record(&directory)).unwrap(),
        before,
        "the refusal reads evidence and changes nothing"
    );
}

#[test]
fn a_changed_unresolved_vendor_produces_a_different_handle_at_the_same_path() {
    let (_tree, root) = initialized_package("diagnosis-handle-vendor");
    let vendor = created_vendor(&root, "Tarnwick", VENDOR_ID);
    let directory = adopted_order(
        &root,
        "2026_08_22_Laser_components",
        ORDER_ID,
        Some("Tarnwick"),
    );

    // The same Order, at the same path, referencing two different Vendors that both
    // fail to resolve. The condition and its location are identical; only the fact a
    // repair would act on differs.
    let mut current = vendor;
    let mut repoint = |next: &str| {
        let text = fs::read_to_string(order_record(&directory))
            .unwrap()
            .replace(&current, next);
        fs::write(order_record(&directory), text).unwrap();
        current = next.to_owned();
        diagnosed_handle(&directory, DiagnosticCode::VendorReferenceUnresolved)
    };

    let first = repoint("00000000-0000-4000-8000-000000000000");
    let second = repoint("11111111-1111-4111-8111-111111111111");

    assert_ne!(
        first, second,
        "a confirmation naming one unresolved Vendor must not authorize a repair of a \
         different one"
    );
}

#[test]
fn a_changed_staged_candidate_produces_a_different_handle_at_the_same_path() {
    let (_tree, root) = initialized_package("diagnosis-handle-staging");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let staged = directory.join(".spielorders/.order.json.spielorders-stage");

    // Two different abandoned candidates at one staging name. Promotion writes the
    // candidate's bytes, so which candidate is there is the whole fact being repaired.
    let stage = |bytes: &[u8]| {
        fs::write(&staged, bytes).unwrap();
        diagnosed_handle(&directory, DiagnosticCode::StagingArtifactPresent)
    };

    let first = stage(b"{\"schema_version\": 1, \"title\": \"one\"}\n");
    let second = stage(b"{\"schema_version\": 1, \"title\": \"two\"}\n");

    assert_ne!(
        first, second,
        "a confirmation to promote one staged candidate must not promote a different \
         one that replaced it"
    );
}

#[test]
fn a_changed_duplicate_participant_set_produces_a_different_handle() {
    const THIRD_ORDER_ID: &str = "2b7c8d9e-0f1a-4b2c-8d3e-4f5a6b7c8d9e";
    let (_tree, root) = initialized_package("diagnosis-handle-duplicate");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    damage(&directory, "duplicate id", &root);
    let against_one = diagnosed_handle(&directory, DiagnosticCode::OrderIdDuplicate);

    // A third directory claims the same Order ID. This Order, its code, and its path
    // are unchanged; who it conflicts with is not, and resolving the conflict means
    // designating a survivor among a different set of directories.
    let third = adopted_order(&root, "2026_08_22_Third", THIRD_ORDER_ID, None);
    let text = fs::read_to_string(order_record(&third))
        .unwrap()
        .replace(THIRD_ORDER_ID, ORDER_ID);
    fs::write(order_record(&third), text).unwrap();
    let against_two = diagnosed_handle(&directory, DiagnosticCode::OrderIdDuplicate);

    assert_ne!(
        against_one, against_two,
        "a confirmation resolving a two-way conflict must not resolve a three-way one"
    );
}

#[test]
fn a_changed_invalid_record_produces_a_different_handle_at_the_same_path() {
    let (_tree, root) = initialized_package("diagnosis-handle-invalid");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);

    // One record at one path, invalid in two different ways. A replacement is confirmed
    // against what the diagnosis said was wrong, so the two must not share a handle.
    let invalidate = |text: &str| {
        fs::write(order_record(&directory), text).unwrap();
        diagnosed_handle(&directory, DiagnosticCode::ManagedArtifactInvalidShape)
    };

    let missing_title = invalidate(&format!(
        r#"{{"schema_version":1,"package_root":"../..","order_id":"{ORDER_ID}","date_ordered":"2026-08-22","status":"hold","items":[]}}"#
    ));
    let missing_status = invalidate(&format!(
        r#"{{"schema_version":1,"package_root":"../..","order_id":"{ORDER_ID}","title":"Laser components","date_ordered":"2026-08-22","items":[]}}"#
    ));

    assert_ne!(
        missing_title, missing_status,
        "a confirmation naming one diagnosed invalidity must not authorize replacing a \
         record that is now invalid for a different reason"
    );
}
