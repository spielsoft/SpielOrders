//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping; Define durable metadata schemas and version evolution.

use crate::support::{ObservingFileSystem, RealFileSystem, TemporaryTree, diagnosed_handle};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::canonical::{ExhaustivePackageValidation, validate_package_exhaustively};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_diagnosis::{OrderDiagnosis, OrderDiagnosisError, diagnose_order};
use spielorders_core::ports::UuidGenerator;
use spielorders_core::readme::{DiagnosticCode, EvidenceScope};
use spielorders_core::repair_cases::RepairAction;
use spielorders_core::vendors::{VendorAssignments, commit_vendor_mutation, plan_vendor_creation};
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

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

fn initialized_package(label: &str) -> (TemporaryTree, PathBuf) {
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
    let root = fs::canonicalize(tree.path()).unwrap();
    (tree, root)
}

fn created_vendor(root: &Path, name: &str) -> String {
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", name),
        &RealFileSystem,
    )
    .expect("the fixture Vendor can be planned");
    let report = commit_vendor_mutation(plan, &mut FixedUuid(VENDOR_ID.into()), &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    result.vendor().vendor_id.as_str().to_owned()
}

/// An adopted Order carrying a README and one opaque sentinel, so every diagnosis can
/// also be shown never to read Supporting Material.
fn adopted_order(root: &Path, basename: &str, uuid: &str, vendor: Option<&str>) -> PathBuf {
    let directory = root.join("Orders").join(basename);
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).unwrap();
    fs::write(directory.join("quote.bin"), SUPPORTING_A).unwrap();
    let mut input = OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold);
    if let Some(vendor) = vendor {
        input = input.vendor_name(vendor);
    }
    let plan = plan_order_adoption(root, &directory, &input, &RealFileSystem)
        .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(plan, &mut FixedUuid(uuid.into()), &RealFileSystem);
    assert!(
        matches!(report, MutationReport::Committed { .. }),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn record(directory: &Path) -> PathBuf {
    directory.join(".spielorders/order.json")
}

fn codes(diagnosis: &OrderDiagnosis) -> Vec<DiagnosticCode> {
    diagnosis
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn actions(diagnosis: &OrderDiagnosis, code: DiagnosticCode) -> Vec<RepairAction> {
    diagnosis
        .cases()
        .iter()
        .find(|case| case.code() == code)
        .unwrap_or_else(|| panic!("{code:?} has a repair case: {:?}", diagnosis.cases()))
        .actions()
        .to_vec()
}

/// Damages an Order in one of the shapes the integrity decision enumerates.
fn damage(directory: &Path, shape: &str, root: &Path) {
    match shape {
        "missing" => fs::remove_file(record(directory)).unwrap(),
        "malformed" => fs::write(record(directory), b"{ not json").unwrap(),
        "invalid shape" => {
            fs::write(record(directory), br#"{"schema_version":1,"nope":true}"#).unwrap()
        }
        "unsupported version" => {
            let text = fs::read_to_string(record(directory))
                .unwrap()
                .replace("\"schema_version\": 1", "\"schema_version\": 2");
            fs::write(record(directory), text).unwrap();
        }
        "semantically invalid" => {
            let text = fs::read_to_string(record(directory))
                .unwrap()
                .replace("2026-08-22", "2026-02-30");
            fs::write(record(directory), text).unwrap();
        }
        "unresolved vendor" => {
            let text = fs::read_to_string(record(directory))
                .unwrap()
                .replace(VENDOR_ID, "00000000-0000-4000-8000-000000000000");
            fs::write(record(directory), text).unwrap();
        }
        "stale locator" => {
            let text = fs::read_to_string(record(directory))
                .unwrap()
                .replace("\"../..\"", "\"../../..\"");
            fs::write(record(directory), text).unwrap();
        }
        "staging" => {
            fs::write(
                directory.join(".spielorders/.order.json.spielorders-stage"),
                fs::read(record(directory)).unwrap(),
            )
            .unwrap();
        }
        "duplicate id" => {
            // A second physical Order carrying this Order's identity.
            let twin = adopted_order(root, "2026_08_22_Twin", OTHER_ORDER_ID, None);
            let text = fs::read_to_string(record(&twin))
                .unwrap()
                .replace(OTHER_ORDER_ID, ORDER_ID);
            fs::write(record(&twin), text).unwrap();
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
    // read model already publishes for that condition; this ticket invents no second
    // vocabulary for the same evidence.
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
        created_vendor(&root, "Tarnwick");
        let directory = adopted_order(
            &root,
            "2026_08_22_Laser_components",
            ORDER_ID,
            Some("Tarnwick"),
        );
        damage(&directory, shape, &root);

        let diagnosis = diagnose_order(&directory, &RealFileSystem)
            .unwrap_or_else(|error| panic!("{shape} is diagnosable: {error:?}"));

        assert!(
            codes(&diagnosis).contains(&expected),
            "{shape} reports {expected:?}, got {:?}",
            codes(&diagnosis)
        );
        assert!(!diagnosis.is_valid(), "{shape} is not a valid Order");
        assert_eq!(diagnosis.directory(), directory, "{shape}");
    }
}

#[test]
fn an_unreadable_record_is_diagnosed_as_the_decision_names_it() {
    // The seventh enumerated cause needs an injected read failure rather than a
    // filesystem state, and it is the one an external audit found omitted from a
    // four-item list earlier in this milestone.
    let (_tree, root) = initialized_package("diagnosis-unreadable");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let before = fs::read(record(&directory)).unwrap();

    let diagnosis =
        diagnose_order(&directory, &UnreadableRecord).expect("an unreadable record is diagnosable");

    assert!(
        codes(&diagnosis).contains(&DiagnosticCode::ManagedArtifactUnreadable),
        "an unreadable record reports its own code, got {:?}",
        codes(&diagnosis)
    );
    assert!(!diagnosis.is_valid());
    // No safe core action exists for a record this core cannot even read.
    assert!(
        diagnosis
            .unactionable()
            .iter()
            .any(|finding| finding.code() == DiagnosticCode::ManagedArtifactUnreadable)
    );
    assert!(diagnosis.cases().is_empty());
    assert_eq!(fs::read(record(&directory)).unwrap(), before);
}

#[test]
fn a_healthy_order_reports_no_diagnostic_and_no_case() {
    let (_tree, root) = initialized_package("diagnosis-healthy");
    created_vendor(&root, "Tarnwick");
    let directory = adopted_order(
        &root,
        "2026_08_22_Laser_components",
        ORDER_ID,
        Some("Tarnwick"),
    );

    let diagnosis =
        diagnose_order(&directory, &RealFileSystem).expect("a healthy Order is diagnosable");

    assert!(diagnosis.is_valid());
    assert_eq!(codes(&diagnosis), Vec::<DiagnosticCode>::new());
    assert!(diagnosis.cases().is_empty());
    assert!(diagnosis.unactionable().is_empty());
}

/// Fails only the read of an Order record, leaving every other effect real.
struct UnreadableRecord;

impl spielorders_core::ports::FileSystem for UnreadableRecord {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with(".spielorders/order.json") {
            return Err(std::io::Error::other("injected unreadable record"));
        }
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
fn every_actionable_family_offers_only_the_actions_a_repair_ticket_owns() {
    // Diagnosis is the input to the repair capabilities, so each case must name only
    // actions one of them can actually execute. The mapping below is the handoff:
    // ReplaceManagedRecord and PromoteStagingCandidate are owned by the confirmed
    // managed-metadata/staging repair; the Vendor, duplicate-identity, and locator
    // resolutions are owned by the explicit semantic repair.
    let (_tree, root) = initialized_package("diagnosis-actionable");
    created_vendor(&root, "Tarnwick");

    // Parseable but schema-invalid metadata: one confirmed complete replacement.
    let directory = adopted_order(&root, "2026_08_22_Shape", ORDER_ID, Some("Tarnwick"));
    damage(&directory, "invalid shape", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::ManagedArtifactInvalidShape),
        [RepairAction::ReplaceManagedRecord]
    );
    assert_eq!(
        RepairAction::ReplaceManagedRecord.required_values(),
        ["record"],
        "a replacement is supplied by the user, never guessed"
    );

    // Semantically invalid but parseable: the same single safe action.
    let (_tree, root) = initialized_package("diagnosis-actionable-semantic");
    let directory = adopted_order(&root, "2026_08_22_Semantic", ORDER_ID, None);
    damage(&directory, "semantically invalid", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::OrderSemanticInvalid),
        [RepairAction::ReplaceManagedRecord]
    );

    // An abandoned staging artifact: promote exactly that candidate.
    let (_tree, root) = initialized_package("diagnosis-actionable-staging");
    let directory = adopted_order(&root, "2026_08_22_Staging", ORDER_ID, None);
    damage(&directory, "staging", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::StagingArtifactPresent),
        [RepairAction::PromoteStagingCandidate]
    );
    assert!(
        RepairAction::PromoteStagingCandidate
            .required_values()
            .is_empty(),
        "the diagnosed candidate is the only one that may be promoted"
    );

    // An unresolved Vendor Reference: exactly the three authorized resolutions, and
    // never an inferred one.
    let (_tree, root) = initialized_package("diagnosis-actionable-vendor");
    created_vendor(&root, "Tarnwick");
    let directory = adopted_order(&root, "2026_08_22_Vendor", ORDER_ID, Some("Tarnwick"));
    damage(&directory, "unresolved vendor", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::VendorReferenceUnresolved),
        [
            RepairAction::AssignExistingVendor,
            RepairAction::CreateAndAssignVendor,
            RepairAction::ClearVendorReference,
        ]
    );
    assert_eq!(
        RepairAction::AssignExistingVendor.required_values(),
        ["vendor"]
    );
    assert_eq!(
        RepairAction::CreateAndAssignVendor.required_values(),
        ["name"]
    );

    // A duplicate Order ID: adopt a fresh identity for the chosen physical Order.
    let (_tree, root) = initialized_package("diagnosis-actionable-duplicate");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    damage(&directory, "duplicate id", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert_eq!(
        actions(&diagnosis, DiagnosticCode::OrderIdDuplicate),
        [RepairAction::AdoptNewOrderIdentity]
    );

    // A stale technical locator: recompute it from the physical relationship.
    let (_tree, root) = initialized_package("diagnosis-actionable-locator");
    let directory = adopted_order(&root, "2026_08_22_Locator", ORDER_ID, None);
    damage(&directory, "stale locator", &root);
    let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();
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

        let diagnosis = diagnose_order(&directory, &RealFileSystem).unwrap();

        assert!(
            diagnosis.cases().is_empty(),
            "{shape} exposes no repair case, got {:?}",
            diagnosis.cases()
        );
        assert!(
            diagnosis
                .unactionable()
                .iter()
                .any(|finding| finding.code() == expected),
            "{shape} is reported as unactionable"
        );
        assert!(!diagnosis.is_valid(), "{shape}");
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
        created_vendor(&root, "Tarnwick");
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
        let record_before = fs::read(record(&directory)).ok();
        let readme_before = fs::read(directory.join("README.md")).unwrap();
        let sentinel_before = fs::read(directory.join("quote.bin")).unwrap();

        let effects = ObservingFileSystem::default();
        let diagnosis = diagnose_order(&directory, &effects);

        assert!(diagnosis.is_ok(), "{shape} is diagnosable");
        // Diagnosis is read-only: it stages nothing, promotes nothing, replaces no
        // index, and rewrites no record.
        assert_eq!(
            fs::read(root.join(".spielorders/index.json")).unwrap(),
            index_before,
            "{shape} replaces no derived index"
        );
        assert_eq!(fs::read(record(&directory)).ok(), record_before, "{shape}");
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
        for read in effects.reads() {
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
    created_vendor(&root, "Tarnwick");
    let healthy = adopted_order(
        &root,
        "2026_08_22_Healthy",
        OTHER_ORDER_ID,
        Some("Tarnwick"),
    );
    let damaged = adopted_order(&root, "2026_08_22_Damaged", ORDER_ID, Some("Tarnwick"));
    damage(&damaged, "semantically invalid", &root);
    let damaged_before = fs::read(record(&damaged)).unwrap();

    let diagnosis = diagnose_order(&damaged, &RealFileSystem).expect("the damaged Order diagnoses");

    assert!(!diagnosis.is_valid());
    assert!(
        codes(&diagnosis).contains(&DiagnosticCode::OrderSemanticInvalid),
        "got {:?}",
        codes(&diagnosis)
    );
    // Its own bytes are evidence, not something diagnosis repairs.
    assert_eq!(fs::read(record(&damaged)).unwrap(), damaged_before);

    // The healthy sibling is unaffected, and its own scoped diagnosis excludes the
    // damaged Order's findings entirely.
    let sibling = diagnose_order(&healthy, &RealFileSystem).expect("the sibling diagnoses");
    assert!(
        sibling.is_valid(),
        "a damaged Order does not block a healthy one"
    );
    assert_eq!(codes(&sibling), Vec::<DiagnosticCode>::new());
    assert!(sibling.cases().is_empty());
    assert_eq!(sibling.directory(), healthy);

    // And the damaged Order's diagnosis names only its own scope.
    for diagnostic in diagnosis.diagnostics() {
        assert!(
            diagnostic
                .primary_location()
                .absolute_path()
                .starts_with(&damaged),
            "a scoped diagnosis reports only its own Order, but named {:?}",
            diagnostic.primary_location().absolute_path()
        );
    }
}

#[test]
fn case_handles_are_deterministic_for_a_snapshot_and_disappear_once_corrected() {
    let (_tree, root) = initialized_package("diagnosis-handles");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let healthy_record = fs::read(record(&directory)).unwrap();
    damage(&directory, "stale locator", &root);

    let first = diagnose_order(&directory, &RealFileSystem).unwrap();
    let second = diagnose_order(&directory, &RealFileSystem).unwrap();

    let handles = |diagnosis: &OrderDiagnosis| {
        diagnosis
            .cases()
            .iter()
            .map(|case| case.handle().to_owned())
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
    assert!(first.case(&handle).is_some());
    assert_eq!(
        first.case(&handle).unwrap().code(),
        DiagnosticCode::OrderPackageLocatorMismatch
    );

    // Handles are not persisted identities: once the condition is gone, so is the
    // case, which is what lets a repair capability reject a stale handle.
    fs::write(record(&directory), &healthy_record).unwrap();
    let corrected = diagnose_order(&directory, &RealFileSystem).unwrap();
    assert!(corrected.is_valid());
    assert!(
        corrected.case(&handle).is_none(),
        "a corrected condition no longer answers to its old handle"
    );
}

#[test]
fn a_scoped_diagnosis_matches_the_package_wide_evidence_for_the_same_order() {
    let (_tree, root) = initialized_package("diagnosis-parity");
    created_vendor(&root, "Tarnwick");
    let subject = adopted_order(&root, "2026_08_22_Subject", ORDER_ID, Some("Tarnwick"));
    let other = adopted_order(&root, "2026_08_22_Other", OTHER_ORDER_ID, Some("Tarnwick"));
    damage(&subject, "semantically invalid", &root);
    damage(&other, "stale locator", &root);

    let diagnosis = diagnose_order(&subject, &RealFileSystem).unwrap();

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(&root, &RealFileSystem)
    else {
        panic!("the Package validates exhaustively");
    };
    let package_wide = report
        .diagnostic_groups()
        .iter()
        .filter(|group| group.location().absolute_path().starts_with(&subject))
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect::<Vec<_>>();

    assert_eq!(
        diagnosis.diagnostics(),
        package_wide.as_slice(),
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
    let diagnosis = diagnose_order(&stranger, &RealFileSystem)
        .expect("an ordinary Orders Directory entry has a diagnosis");
    assert!(!diagnosis.is_valid());
    assert!(!diagnosis.diagnostics().is_empty());

    // A directory outside any Package has no Package to validate against.
    let outside = TemporaryTree::new("diagnosis-unmanaged");
    let error = diagnose_order(outside.path(), &RealFileSystem)
        .expect_err("a directory outside a Package is refused");
    assert!(
        matches!(error, OrderDiagnosisError::UnmanagedPackage { .. }),
        "an unmanaged location is reported as such: {error:?}"
    );
}

#[test]
fn a_package_wide_failure_is_never_downgraded_to_an_order_finding() {
    let (_tree, root) = initialized_package("diagnosis-damaged-package");
    let directory = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID, None);
    let before = fs::read(record(&directory)).unwrap();
    // The Vendor aggregate is package-wide canonical state, and an Order-scoped
    // answer depends on it to resolve Vendor References. With it untrustworthy,
    // reporting this Order as healthy would be a downgrade, not a diagnosis.
    fs::write(root.join(".spielorders/vendors.json"), b"{ not json").unwrap();

    let error = diagnose_order(&directory, &RealFileSystem)
        .expect_err("an Order inside a Damaged Package is not diagnosed in isolation");

    // The Package-scope findings travel with the refusal, so a caller can report the
    // Package-level problem as a Package-level problem rather than only that something
    // somewhere was wrong.
    let OrderDiagnosisError::DamagedPackage {
        root: package_root,
        diagnostics,
    } = &error
    else {
        panic!("a Package-wide failure is reported as such: {error:?}")
    };
    assert_eq!(
        package_root.absolute_path(),
        fs::canonicalize(&root).expect("the Package Root resolves")
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.scope() == EvidenceScope::Package),
        "and only those: a sibling Order's damage is not this scope's to report"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ManagedArtifactInvalidSyntax),
        "the evidence for the refusal travels with it: {diagnostics:?}"
    );
    assert_eq!(
        fs::read(record(&directory)).unwrap(),
        before,
        "the refusal reads evidence and changes nothing"
    );
}

#[test]
fn a_changed_unresolved_vendor_produces_a_different_handle_at_the_same_path() {
    let (_tree, root) = initialized_package("diagnosis-handle-vendor");
    let vendor = created_vendor(&root, "Tarnwick");
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
        let text = fs::read_to_string(record(&directory))
            .unwrap()
            .replace(&current, next);
        fs::write(record(&directory), text).unwrap();
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
    let text = fs::read_to_string(record(&third))
        .unwrap()
        .replace(THIRD_ORDER_ID, ORDER_ID);
    fs::write(record(&third), text).unwrap();
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
        fs::write(record(&directory), text).unwrap();
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
