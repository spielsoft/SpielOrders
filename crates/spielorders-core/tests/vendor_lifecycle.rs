//! Decision sources: Define Vendor identity and historical semantics; Define the
//! Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::support::{
    ConcurrentWriter, EditPoint, ObservingFileSystem, RealFileSystem, TemporaryTree,
};
use spielorders_core::canonical::{
    CanonicalPackageLoad, PackageIndexRefresh, PackageIndexState, inspect_canonical_package,
};
use spielorders_core::creation::{OrderCreationInput, commit_order_creation, plan_order_creation};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, Vendor, VendorsDocument};
use spielorders_core::mutation::{MutationReport, RejectedAssignmentReason};
use spielorders_core::operations::OperationFailureCode;
use spielorders_core::order_edits::{OrderAssignments, commit_order_edit, plan_order_edit};
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, LocalCalendar,
    LocalDate, UuidGenerator,
};
use spielorders_core::vendors::{
    CanonicalVendorOutcome, PlannedVendorIdentity, VendorAssignments, VendorCapabilityError,
    VendorMutationKind, VendorMutationOperationKind, VendorMutationRejection,
    VendorMutationWarning, commit_vendor_mutation, plan_vendor_creation, plan_vendor_edit,
    plan_vendor_hide, plan_vendor_unhide, show_vendor,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

    fn calls(&self) -> usize {
        self.calls
    }
}

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.calls += 1;
        self.value.clone()
    }
}

fn retained_id(identity: &PlannedVendorIdentity) -> Option<&str> {
    match identity {
        PlannedVendorIdentity::GeneratedAtCommit => None,
        PlannedVendorIdentity::Retained(vendor_id) => Some(vendor_id.as_str()),
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
fn creation_plan_is_complete_and_side_effect_free_without_generating_identity() {
    let tree = initialized_package("vendor-creation-plan");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let index_path = root.join(".spielorders/index.json");
    let vendors_before = fs::read(&vendors_path).unwrap();
    let index_before = fs::read(&index_path).ok();

    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new()
            .assign("name", "  Tarnwick  ")
            .assign("phone", " +1 555 0123 ")
            .assign("address", "100 Example Avenue\r\nAnytown, ST 00000\n"),
        &RealFileSystem,
    )
    .expect("a named Vendor can be planned");

    assert_eq!(
        plan.subject().identity(),
        &PlannedVendorIdentity::GeneratedAtCommit
    );
    assert_eq!(plan.subject().name(), "Tarnwick");
    assert_eq!(plan.subject().phone(), Some("+1 555 0123"));
    assert_eq!(
        plan.subject().address(),
        Some("100 Example Avenue\nAnytown, ST 00000")
    );
    assert_eq!(plan.subject().email(), None);
    assert_eq!(plan.subject().website(), None);
    assert_eq!(plan.subject().purchasing_notes(), None);
    assert!(plan.subject().contacts().is_empty());
    assert_eq!(plan.planned_vendors().len(), 1);
    assert_eq!(plan.planned_vendors()[0].name(), "Tarnwick");
    assert!(plan.referencing_orders().is_empty());
    assert_eq!(
        plan.operations()
            .iter()
            .map(|operation| (
                operation.kind(),
                operation.path().strip_prefix(plan.root()).unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            (
                VendorMutationOperationKind::ReplaceVendorRecord,
                Path::new(".spielorders/vendors.json")
            ),
            (
                VendorMutationOperationKind::RefreshPackageIndex,
                Path::new(".spielorders/index.json")
            ),
        ]
    );
    assert_eq!(
        plan.vendors_path().strip_prefix(plan.root()).unwrap(),
        Path::new(".spielorders/vendors.json")
    );
    assert_eq!(fs::read(&vendors_path).unwrap(), vendors_before);
    assert_eq!(fs::read(&index_path).ok(), index_before);
}

#[test]
fn creation_commit_generates_identity_at_commit_and_refreshes_the_derived_index() {
    let tree = initialized_package("vendor-creation-commit");
    let root = tree.path();
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new()
            .assign("name", "Tarnwick")
            .assign("email", "sales@example.com"),
        &RealFileSystem,
    )
    .expect("a named Vendor can be planned");
    let mut uuid = FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822");

    let report = commit_vendor_mutation(plan, &mut uuid, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid Vendor creation commits: {report:?}");
    };
    assert_eq!(uuid.calls(), 1);
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Created);
    assert_eq!(
        result.vendor().vendor_id.as_str(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    assert_eq!(result.vendor().name.as_str(), "Tarnwick");
    assert_eq!(result.vendor().email.as_deref(), Some("sales@example.com"));
    assert!(result.vendor().contacts.is_empty());
    assert!(result.referencing_orders().is_empty());
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);

    let persisted = fs::read(root.join(".spielorders/vendors.json")).unwrap();
    let document = VendorsDocument::from_json_bytes(&persisted).expect("canonical Vendors parse");
    assert_eq!(document.vendors.len(), 1);
    assert_eq!(
        document.vendors[0].vendor_id.as_str(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    let indexed =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the refreshed derived index parses");
    assert_eq!(indexed.vendors, document.vendors);
}

fn created_vendor(root: &Path, uuid: &str, assignments: VendorAssignments) -> Vendor {
    let plan = plan_vendor_creation(root, &assignments, &RealFileSystem)
        .expect("the Vendor can be planned");
    let report = commit_vendor_mutation(plan, &mut FixedUuid::new(uuid), &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    result.vendor().clone()
}

#[test]
fn trimmed_case_insensitive_duplicate_name_is_rejected_and_preserves_prior_bytes() {
    let tree = initialized_package("vendor-duplicate-name");
    let root = tree.path();
    let existing = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "  tarNWICK  "),
        &RealFileSystem,
    )
    .expect_err("a case-variant duplicate Vendor Name is rejected");

    let VendorCapabilityError::DuplicateName { vendor_id } = &error else {
        panic!("a duplicate Vendor Name is rejected as such: {error:?}");
    };
    assert_eq!(vendor_id, existing.vendor_id.as_str());
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn deliberately_distinct_vendor_is_created_without_merging_the_similar_name() {
    let tree = initialized_package("vendor-distinct-creation");
    let root = tree.path();
    created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );

    let distinct = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        VendorAssignments::new().assign("name", "Tarnwick Inc"),
    );

    assert_eq!(distinct.name.as_str(), "Tarnwick Inc");
    let document = VendorsDocument::from_json_bytes(
        &fs::read(root.join(".spielorders/vendors.json")).unwrap(),
    )
    .expect("canonical Vendors parse");
    assert_eq!(
        document
            .vendors
            .iter()
            .map(|vendor| vendor.name.as_str())
            .collect::<Vec<_>>(),
        ["Tarnwick", "Tarnwick Inc"]
    );
}

#[test]
fn blank_and_absent_names_are_rejected_before_any_write() {
    let tree = initialized_package("vendor-blank-name");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let blank = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "   "),
        &RealFileSystem,
    )
    .expect_err("a blank Vendor Name is rejected");
    let absent = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("phone", "+1 555 0100"),
        &RealFileSystem,
    )
    .expect_err("an absent Vendor Name is rejected");

    let VendorCapabilityError::InvalidAssignments(rejected) = blank else {
        panic!("a blank Name is a rejected assignment: {blank:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field(), entry.reason()))
            .collect::<Vec<_>>(),
        [("name", RejectedAssignmentReason::BlankRequiredValue)]
    );
    assert!(matches!(absent, VendorCapabilityError::MissingRequiredName));
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

struct FixedCalendar;

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 8,
            day: 22,
        }
    }
}

fn created_order(root: &Path, uuid: &str, input: &OrderCreationInput) -> PathBuf {
    let plan = plan_order_creation(root, input, &FixedCalendar, &RealFileSystem)
        .expect("the fixture Order can be planned");
    let report = commit_order_creation(plan, &mut FixedUuid::new(uuid), &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Order commits: {report:?}");
    };
    result
        .order()
        .paths()
        .directory()
        .absolute_path()
        .to_path_buf()
}

#[test]
fn rename_retains_identity_updates_live_resolution_and_never_renames_the_order_directory() {
    let tree = initialized_package("vendor-rename");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new()
            .assign("name", "Tarnwick")
            .assign("phone", "+1 555 0123"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").vendor_id(vendor.vendor_id.as_str()),
    );
    let order_path = directory.join(".spielorders/order.json");
    let order_bytes_before = fs::read(&order_path).unwrap();

    let plan = plan_vendor_edit(
        root,
        vendor.vendor_id.as_str(),
        &VendorAssignments::new().assign("name", "  Tarnwick Photonics  "),
        &RealFileSystem,
    )
    .expect("an existing Vendor can be renamed");

    assert_eq!(
        plan.subject().identity(),
        &PlannedVendorIdentity::Retained(vendor.vendor_id.clone())
    );
    assert_eq!(plan.subject().name(), "Tarnwick Photonics");
    assert_eq!(plan.subject().phone(), Some("+1 555 0123"));
    assert_eq!(
        plan.referencing_orders()
            .iter()
            .map(|order| (order.order_id(), order.title(), order.directory()))
            .collect::<Vec<_>>(),
        [(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "Laser components",
            directory.as_path()
        )]
    );

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid rename commits: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Edited);
    assert_eq!(result.vendor().vendor_id, vendor.vendor_id);
    assert_eq!(result.vendor().name.as_str(), "Tarnwick Photonics");
    assert_eq!(result.vendor().phone.as_deref(), Some("+1 555 0123"));
    assert_eq!(result.referencing_orders().len(), 1);
    assert_eq!(result.referencing_orders()[0].directory(), directory);
    assert_eq!(result.index().state(), PackageIndexState::Current);

    assert!(directory.is_dir(), "the Order Directory keeps its name");
    assert_eq!(
        directory.file_name().unwrap(),
        std::ffi::OsStr::new("2026_08_22_Tarnwick_Laser_components")
    );
    assert_eq!(fs::read(&order_path).unwrap(), order_bytes_before);
    let indexed =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the refreshed derived index parses");
    assert_eq!(indexed.vendors[0].name.as_str(), "Tarnwick Photonics");
    assert_eq!(
        indexed.orders[0].order_directory.as_str(),
        "Orders/2026_08_22_Tarnwick_Laser_components"
    );
}

const VENDOR_WITH_CONTACTS: &[u8] = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "phone": "+1 555 0123",
      "email": "sales@example.com",
      "website": "https://example.com",
      "address": "100 Example Avenue\nAnytown, ST 00000",
      "contacts": [
        {
          "name": "Grace Hopper",
          "role": "Sales",
          "phone": "+1 555 0100",
          "email": "grace@example.com"
        },
        {
          "name": "Alan Turing",
          "role": "Support"
        }
      ],
      "purchasing_notes": "Use institutional account."
    }
  ]
}
"#;

const SEEDED_VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

fn package_with_seeded_vendor(label: &str) -> TemporaryTree {
    let tree = initialized_package(label);
    fs::write(
        tree.path().join(".spielorders/vendors.json"),
        VENDOR_WITH_CONTACTS,
    )
    .unwrap();
    tree
}

#[test]
fn edit_clears_explicitly_and_preserves_unspecified_facts_and_contact_order() {
    let tree = package_with_seeded_vendor("vendor-edit-clearing");
    let root = tree.path();

    let plan = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new()
            .assign("phone", "")
            .assign("website", "  ")
            .assign("email", " orders@example.com "),
        &RealFileSystem,
    )
    .expect("optional Vendor facts can be cleared and replaced");

    assert_eq!(plan.subject().phone(), None);
    assert_eq!(plan.subject().website(), None);
    assert_eq!(plan.subject().email(), Some("orders@example.com"));
    assert_eq!(
        plan.subject().address(),
        Some("100 Example Avenue\nAnytown, ST 00000")
    );
    assert_eq!(
        plan.subject().purchasing_notes(),
        Some("Use institutional account.")
    );
    assert_eq!(
        plan.subject()
            .contacts()
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing"]
    );

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );

    let MutationReport::Committed { result, .. } = report else {
        panic!("explicit clearing commits: {report:?}");
    };
    assert_eq!(result.vendor().phone, None);
    assert_eq!(result.vendor().website, None);
    assert_eq!(result.vendor().email.as_deref(), Some("orders@example.com"));
    assert_eq!(result.vendor().contacts.len(), 2);
    assert_eq!(result.vendor().contacts[0].name.as_str(), "Grace Hopper");
    assert_eq!(result.vendor().contacts[0].role.as_deref(), Some("Sales"));
    assert_eq!(result.vendor().contacts[1].name.as_str(), "Alan Turing");
    assert_eq!(result.vendor().contacts[1].email, None);
}

#[test]
fn edit_rejects_unknown_and_immutable_assignments_before_replacement() {
    let tree = package_with_seeded_vendor("vendor-edit-invalid-assignments");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new()
            .assign("vendor_id", "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")
            .assign("schema_version", "2")
            .assign("contacts", "[]")
            .assign("legal_name", "Tarnwick LLC")
            .assign("email", "first@example.com")
            .assign("email", "second@example.com"),
        &RealFileSystem,
    )
    .expect_err("unknown and immutable assignments are rejected");

    let VendorCapabilityError::InvalidAssignments(rejected) = &error else {
        panic!("invalid assignments are reported as such: {error:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field(), entry.reason()))
            .collect::<Vec<_>>(),
        [
            ("vendor_id", RejectedAssignmentReason::ImmutableIdentity),
            (
                "schema_version",
                RejectedAssignmentReason::ManagedTechnicalField
            ),
            ("contacts", RejectedAssignmentReason::OrderedCollection),
            ("legal_name", RejectedAssignmentReason::UnknownField),
            ("email", RejectedAssignmentReason::Duplicate),
        ]
    );
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn editing_an_absent_vendor_id_is_rejected_without_inferring_a_vendor() {
    let tree = package_with_seeded_vendor("vendor-edit-unknown-target");
    let root = tree.path();

    let error = plan_vendor_edit(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect_err("an absent Vendor ID is rejected");

    let VendorCapabilityError::UnknownVendor { vendor_id } = &error else {
        panic!("an absent Vendor ID is reported as unknown: {error:?}");
    };
    assert_eq!(vendor_id, "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91");
}

#[test]
fn no_op_edit_reports_unchanged_without_replacing_canonical_bytes() {
    let tree = package_with_seeded_vendor("vendor-edit-no-op");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let observed_before = fs::metadata(&vendors_path).unwrap().modified().unwrap();

    let plan = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new()
            .assign("name", "  Tarnwick  ")
            .assign("email", "sales@example.com"),
        &RealFileSystem,
    )
    .expect("a no-op edit is a valid plan");

    assert!(!plan.changes_canonical_vendors());
    assert!(
        plan.operations().is_empty(),
        "a plan that changes nothing announces no managed operation"
    );

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a no-op edit reports unchanged: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Edited);
    assert_eq!(result.vendor().name.as_str(), "Tarnwick");
    // The empty operation list above is what proves no refresh was claimed. These
    // pin the canonical inspection's own index shape, which asserts no freshness.
    assert_eq!(result.index().state(), PackageIndexState::NotAssessed);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&vendors_path).unwrap().modified().unwrap(),
        observed_before
    );
}

/// The whole point of hiding rather than deleting: the Vendor Reference on a committed
/// Order still resolves to the Hidden Vendor's complete current facts.
#[test]
fn hiding_a_vendor_referenced_by_a_committed_order_keeps_its_facts_resolvable() {
    let tree = initialized_package("vendor-hide-referenced");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new()
            .assign("name", "Tarnwick")
            .assign("email", "sales@example.com"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").vendor_id(vendor.vendor_id.as_str()),
    );
    let order_path = directory.join(".spielorders/order.json");
    let order_before = fs::read(&order_path).unwrap();

    let plan = plan_vendor_hide(root, vendor.vendor_id.as_str(), &RealFileSystem)
        .expect("a referenced Vendor can be hidden");

    assert_eq!(plan.kind(), VendorMutationKind::Hide);
    assert!(plan.subject().hidden());
    assert_eq!(plan.subject().name(), "Tarnwick");
    assert_eq!(
        plan.referencing_orders()
            .iter()
            .map(|order| (order.order_id(), order.title(), order.directory()))
            .collect::<Vec<_>>(),
        [(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "Laser components",
            directory.as_path()
        )],
        "hiding names the Orders that keep referencing the Vendor rather than refusing"
    );

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );

    let MutationReport::Committed { result, .. } = report else {
        panic!("hiding a referenced Vendor commits: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Hidden);
    assert!(result.vendor().is_hidden());
    assert_eq!(result.vendor().name.as_str(), "Tarnwick");
    assert_eq!(result.vendor().email.as_deref(), Some("sales@example.com"));
    assert_eq!(result.referencing_orders().len(), 1);
    assert_eq!(result.referencing_orders()[0].directory(), directory);
    assert_eq!(result.index().state(), PackageIndexState::Current);

    // Resolution is proven against the committed Order reloaded from disk, not against
    // the mutation's own report: the Order record is untouched, and the Package the
    // next reader loads still resolves the Hidden Vendor's complete facts for it.
    assert_eq!(fs::read(&order_path).unwrap(), order_before);
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(root, &RealFileSystem)
    else {
        panic!("the Package still loads as a current candidate");
    };
    let resolved = package
        .vendor_for_order(0)
        .expect("the committed Order still resolves its Vendor");
    assert!(resolved.is_hidden());
    assert_eq!(resolved.name.as_str(), "Tarnwick");
    assert_eq!(resolved.email.as_deref(), Some("sales@example.com"));
    let indexed =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the refreshed derived index parses");
    assert_eq!(indexed.vendors[0].hidden, Some(true));
}

#[test]
fn hide_and_unhide_round_trip_to_the_exact_prior_canonical_bytes() {
    let tree = package_with_seeded_vendor("vendor-visibility-round-trip");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let visible_bytes = fs::read(&vendors_path).unwrap();
    let mut uuid = FixedUuid::new("00000000-0000-4000-8000-00000000ffff");

    let plan = plan_vendor_hide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("the seeded Vendor can be hidden");
    assert!(plan.changes_canonical_vendors());
    let report = commit_vendor_mutation(plan, &mut uuid, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("hiding commits: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Hidden);
    assert!(result.vendor().is_hidden());
    let hidden_bytes = fs::read(&vendors_path).unwrap();
    assert_ne!(hidden_bytes, visible_bytes);
    let hidden_document =
        VendorsDocument::from_json_bytes(&hidden_bytes).expect("canonical Vendors parse");
    assert_eq!(hidden_document.vendors[0].hidden, Some(true));
    assert_eq!(
        hidden_document.vendors[0].contacts.len(),
        2,
        "hiding changes visibility and nothing else"
    );
    assert_eq!(
        hidden_document.vendors[0].purchasing_notes.as_deref(),
        Some("Use institutional account.")
    );

    let plan = plan_vendor_unhide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("a Hidden Vendor can be unhidden");
    assert_eq!(plan.kind(), VendorMutationKind::Unhide);
    assert!(!plan.subject().hidden());
    assert!(plan.changes_canonical_vendors());
    let report = commit_vendor_mutation(plan, &mut uuid, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("unhiding commits: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Unhidden);
    assert!(!result.vendor().is_hidden());
    assert_eq!(
        fs::read(&vendors_path).unwrap(),
        visible_bytes,
        "unhiding restores the exact bytes hiding replaced"
    );
    assert_eq!(
        uuid.calls(),
        0,
        "a visibility change retains the Vendor's identity and generates none"
    );
}

#[test]
fn redundant_visibility_changes_report_unchanged_without_replacing_canonical_bytes() {
    let tree = package_with_seeded_vendor("vendor-visibility-no-op");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");

    // Unhiding an already-visible Vendor changes nothing.
    let plan = plan_vendor_unhide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("an already-visible Vendor can be unhidden");
    assert!(!plan.changes_canonical_vendors());
    assert!(
        plan.operations().is_empty(),
        "a plan that changes nothing announces no managed operation"
    );
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );
    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a redundant unhide reports unchanged: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Unhidden);
    assert!(!result.vendor().is_hidden());

    let plan = plan_vendor_hide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("the seeded Vendor can be hidden");
    assert!(matches!(
        commit_vendor_mutation(
            plan,
            &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));
    let hidden_bytes = fs::read(&vendors_path).unwrap();
    let observed_before = fs::metadata(&vendors_path).unwrap().modified().unwrap();

    // Hiding an already-hidden Vendor changes nothing either.
    let plan = plan_vendor_hide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("an already-hidden Vendor can be hidden again");
    assert!(!plan.changes_canonical_vendors());
    assert!(plan.operations().is_empty());
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &RealFileSystem,
    );

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a redundant hide reports unchanged: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Hidden);
    assert!(result.vendor().is_hidden());
    assert_eq!(fs::read(&vendors_path).unwrap(), hidden_bytes);
    assert_eq!(
        fs::metadata(&vendors_path).unwrap().modified().unwrap(),
        observed_before
    );
}

#[test]
fn a_hidden_vendor_is_listed_with_its_visibility_but_is_not_offered_for_new_use() {
    let tree = initialized_package("vendor-offered-list");
    let root = tree.path();
    let hidden = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );
    let visible = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        VendorAssignments::new().assign("name", "Nethermoor"),
    );
    let plan = plan_vendor_hide(root, hidden.vendor_id.as_str(), &RealFileSystem)
        .expect("the Vendor can be hidden");
    assert!(matches!(
        commit_vendor_mutation(
            plan,
            &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(root, &RealFileSystem)
    else {
        panic!("the Package still loads as a current candidate");
    };

    // The complete list keeps both and carries visibility, so a caller that needs the
    // Hidden Vendor either way — `package show`, the Vendors table's own control — can
    // still reach it. Only the offer is narrowed.
    assert_eq!(
        package
            .vendors()
            .iter()
            .map(|vendor| (vendor.name.as_str(), vendor.is_hidden()))
            .collect::<Vec<_>>(),
        [("Tarnwick", true), ("Nethermoor", false)]
    );
    assert_eq!(
        package
            .offered_vendors()
            .iter()
            .map(|vendor| vendor.vendor_id.as_str())
            .collect::<Vec<_>>(),
        [visible.vendor_id.as_str()]
    );
}

#[test]
fn an_order_referencing_a_hidden_vendor_is_still_editable() {
    let tree = initialized_package("vendor-hidden-order-edit");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").vendor_id(vendor.vendor_id.as_str()),
    );
    let plan = plan_vendor_hide(root, vendor.vendor_id.as_str(), &RealFileSystem)
        .expect("the Vendor can be hidden");
    assert!(matches!(
        commit_vendor_mutation(
            plan,
            &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser components and mounts"),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("an Order keeps its Hidden Vendor Reference and stays editable");

    let report = commit_order_edit(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("editing an Order that references a Hidden Vendor commits: {report:?}");
    };
    assert_eq!(
        result.order().document().title.as_str(),
        "Laser components and mounts"
    );
    assert_eq!(
        result
            .order()
            .document()
            .vendor_id
            .as_ref()
            .map(|id| id.as_str()),
        Some(vendor.vendor_id.as_str()),
        "hiding never clears the Vendor Reference it is referenced by"
    );
}

/// A Vendor is never deleted, so the deletion capability is gone rather than merely
/// unreachable. This pins the whole set of Vendor mutations the core offers, which is
/// why it names all four rather than only the two this ticket added.
#[test]
fn every_vendor_mutation_the_core_offers_is_creation_editing_or_visibility() {
    let tree = package_with_seeded_vendor("vendor-mutation-surface");
    let root = tree.path();
    let creation = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("a Vendor can be created");
    let edit = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
        &RealFileSystem,
    )
    .expect("a Vendor can be edited");
    let hide =
        plan_vendor_hide(root, SEEDED_VENDOR_ID, &RealFileSystem).expect("a Vendor can be hidden");
    let unhide = plan_vendor_unhide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect("a Vendor can be unhidden");

    let offered = [creation.kind(), edit.kind(), hide.kind(), unhide.kind()];

    assert_eq!(
        offered,
        [
            VendorMutationKind::Create,
            VendorMutationKind::Edit,
            VendorMutationKind::Hide,
            VendorMutationKind::Unhide,
        ],
        "these four are the complete set of Vendor mutations; a Vendor is never deleted"
    );
    for kind in offered {
        // Deliberately exhaustive, and the reason it is worth a match that does nothing:
        // reintroducing a deletion variant stops this test compiling.
        match kind {
            VendorMutationKind::Create
            | VendorMutationKind::Edit
            | VendorMutationKind::Hide
            | VendorMutationKind::Unhide => {}
        }
    }
}

#[test]
fn show_resolves_live_vendor_facts_and_referencing_orders_after_a_rename() {
    let tree = initialized_package("vendor-show");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );
    let other = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        VendorAssignments::new().assign("name", "Nethermoor"),
    );
    let referencing = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").vendor_id(vendor.vendor_id.as_str()),
    );
    created_order(
        root,
        "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f",
        &OrderCreationInput::new("Optical table").vendor_id(other.vendor_id.as_str()),
    );
    let plan = plan_vendor_edit(
        root,
        vendor.vendor_id.as_str(),
        &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
        &RealFileSystem,
    )
    .expect("the Vendor can be renamed");
    assert!(matches!(
        commit_vendor_mutation(
            plan,
            &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
            &RealFileSystem
        ),
        MutationReport::Committed { .. }
    ));

    let projection = show_vendor(root, vendor.vendor_id.as_str(), &RealFileSystem)
        .expect("an existing Vendor can be shown");

    assert_eq!(projection.vendor().vendor_id, vendor.vendor_id);
    assert_eq!(projection.vendor().name.as_str(), "Tarnwick Photonics");
    assert_eq!(
        projection
            .referencing_orders()
            .iter()
            .map(|order| (order.order_id(), order.title(), order.directory()))
            .collect::<Vec<_>>(),
        [(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "Laser components",
            referencing.as_path()
        )]
    );

    let error = show_vendor(
        root,
        "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        &RealFileSystem,
    )
    .expect_err("an absent Vendor ID cannot be shown");
    let VendorCapabilityError::UnknownVendor { vendor_id } = &error else {
        panic!("an absent Vendor ID is reported as unknown: {error:?}");
    };
    assert_eq!(vendor_id, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VendorBoundary {
    WriteStage,
    SyncStage,
    Replace,
    CleanupStage,
    IndexReplacement,
}

const VENDOR_STAGE: &str = ".spielorders/.vendors.json.spielorders-stage";

struct FailingVendors {
    boundary: VendorBoundary,
    fired: Cell<bool>,
    fail_cleanup: bool,
}

impl FailingVendors {
    fn at(boundary: VendorBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: false,
        }
    }

    fn with_cleanup_failure(boundary: VendorBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: true,
        }
    }

    fn fail_once(&self, boundary: VendorBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: VendorBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingVendors {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE) && self.fail_once(VendorBoundary::WriteStage) {
            return Err(Self::error(VendorBoundary::WriteStage));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE) && self.fail_once(VendorBoundary::SyncStage) {
            return Err(Self::error(VendorBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/vendors.json")
            && self.fail_once(VendorBoundary::Replace)
        {
            return Err(Self::error(VendorBoundary::Replace));
        }
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(VendorBoundary::IndexReplacement)
        {
            return Err(Self::error(VendorBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE)
            && (self.fail_cleanup || self.fail_once(VendorBoundary::CleanupStage))
        {
            return Err(Self::error(VendorBoundary::CleanupStage));
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

#[test]
fn injected_precommit_failures_preserve_the_prior_vendor_aggregate_exactly() {
    for boundary in [
        VendorBoundary::WriteStage,
        VendorBoundary::SyncStage,
        VendorBoundary::Replace,
    ] {
        let tree = package_with_seeded_vendor("vendor-precommit-failure");
        let root = tree.path();
        let vendors_path = root.join(".spielorders/vendors.json");
        let before = fs::read(&vendors_path).unwrap();
        let plan = plan_vendor_edit(
            root,
            SEEDED_VENDOR_ID,
            &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
            &RealFileSystem,
        )
        .expect("the rename can be planned");

        let report = commit_vendor_mutation(
            plan,
            &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
            &FailingVendors::at(boundary),
        );

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let VendorMutationRejection::ReplacementFailed { error } = &rejection else {
            panic!("an injected {boundary:?} failure is a replacement failure: {rejection:?}");
        };
        let matched = matches!(
            (boundary, error.failure()),
            (
                VendorBoundary::WriteStage,
                ValidatedReplacementFailure::StagingWrite(_)
            ) | (
                VendorBoundary::SyncStage,
                ValidatedReplacementFailure::Durability(_)
            ) | (
                VendorBoundary::Replace,
                ValidatedReplacementFailure::Replacement(_)
            )
        );
        assert!(
            matched,
            "an injected {boundary:?} failure reports its own phase: {:?}",
            error.failure()
        );
        assert!(
            error.cleanup_failure().is_none(),
            "an injected {boundary:?} failure cleans up its own staging artifact"
        );
        assert_eq!(
            fs::read(&vendors_path).unwrap(),
            before,
            "the prior Vendor aggregate survives an injected {boundary:?} failure"
        );
        assert!(
            !root.join(VENDOR_STAGE).exists(),
            "an injected {boundary:?} failure leaves no staging artifact"
        );
    }
}

#[test]
fn staging_cleanup_failure_is_retained_beside_the_rejection() {
    let tree = package_with_seeded_vendor("vendor-cleanup-failure");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let plan = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
        &RealFileSystem,
    )
    .expect("the rename can be planned");

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &FailingVendors::with_cleanup_failure(VendorBoundary::SyncStage),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    let VendorMutationRejection::ReplacementFailed { error } = &rejection else {
        panic!("a durability failure is a replacement failure: {rejection:?}");
    };
    assert!(matches!(
        error.failure(),
        ValidatedReplacementFailure::Durability(_)
    ));
    assert!(
        error.cleanup_failure().is_some(),
        "the failed staging cleanup is retained in the rejection"
    );
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
    assert!(
        root.join(VENDOR_STAGE).exists(),
        "the uncleaned staging artifact remains as reported evidence"
    );
    fs::remove_file(root.join(VENDOR_STAGE)).unwrap();
}

#[test]
fn post_commit_index_refresh_failure_keeps_the_canonical_vendor_mutation() {
    let tree = initialized_package("vendor-index-refresh-failure");
    let root = tree.path();
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Tarnwick"),
        &RealFileSystem,
    )
    .expect("the Vendor can be planned");

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822"),
        &FailingVendors::at(VendorBoundary::IndexReplacement),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert_eq!(result.outcome(), CanonicalVendorOutcome::Created);
    assert_eq!(
        result.vendor().vendor_id.as_str(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    let [VendorMutationWarning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
        panic!("the refresh failure is retained: {warnings:?}");
    };
    assert_eq!(
        failure.code(),
        OperationFailureCode::PackageIndexReplacementFailed
    );

    let document = VendorsDocument::from_json_bytes(
        &fs::read(root.join(".spielorders/vendors.json")).unwrap(),
    )
    .expect("canonical Vendors parse");
    assert_eq!(document.vendors.len(), 1);

    // The committed canonical mutation makes retrying the same creation unsafe.
    let retry = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Tarnwick"),
        &RealFileSystem,
    )
    .expect_err("retrying the committed creation is rejected");
    assert!(matches!(retry, VendorCapabilityError::DuplicateName { .. }));
}

#[test]
fn a_concurrent_vendor_aggregate_change_rejects_before_any_write() {
    let tree = package_with_seeded_vendor("vendor-concurrent-change");
    let root = tree.path();
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("the Vendor can be planned");
    let vendors_path = root.join(".spielorders/vendors.json");
    fs::write(
        &vendors_path,
        br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
      "name": "Nethermoor",
      "contacts": []
    }
  ]
}
"#,
    )
    .unwrap();
    let before = fs::read(&vendors_path).unwrap();

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822"),
        &RealFileSystem,
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a concurrent aggregate change rejects: {report:?}");
    };
    let VendorMutationRejection::PreconditionsChanged { path } = &rejection else {
        panic!("a concurrent aggregate change is a precondition change: {rejection:?}");
    };
    assert!(path.ends_with(".spielorders/vendors.json"));
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

/// A Package whose Vendor aggregate this mutation is about to replace, with the plan
/// authorized against exactly the aggregate now on disk.
fn seeded_package_ready_for_a_contended_rename(label: &str) -> (TemporaryTree, PathBuf, Vec<u8>) {
    let tree = package_with_seeded_vendor(label);
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    (tree, vendors_path, before)
}

fn planned_rename(root: &Path) -> spielorders_core::vendors::VendorMutationPlan {
    plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", "Tarnwick Optics"),
        &RealFileSystem,
    )
    .expect("the rename can be planned")
}

/// An aggregate rewritten between this mutation's revalidation and its rename belongs
/// to whoever wrote it. The window the capability's own precondition check cannot see
/// is the one the destination expectation closes.
#[test]
fn a_vendor_aggregate_given_a_different_meaning_after_revalidation_is_refused() {
    let (tree, vendors_path, before) =
        seeded_package_ready_for_a_contended_rename("vendor-destination-meaning-changed");
    // The same aggregate, with someone else's phone number on the same Vendor.
    let theirs = String::from_utf8(before.clone())
        .unwrap()
        .replace("+1 555 0123", "+1 555 9999")
        .into_bytes();
    assert_ne!(theirs, before);

    let plan = planned_rename(tree.path());
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91"),
        &ConcurrentWriter::replacing(&vendors_path, &theirs, EditPoint::StagingWrite),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an aggregate rewritten underneath this mutation is refused: {report:?}");
    };
    // Refused because someone else changed it — distinct from a failed write, and
    // distinct from the precondition check that ran before any of this.
    let VendorMutationRejection::ReplacementFailed { error } = rejection else {
        panic!("the refusal comes from the replacement, not a precondition: {rejection:?}");
    };
    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "the refusal names a changed destination rather than a write failure: {:?}",
        error.failure()
    );
    assert!(
        error.cleanup_failure().is_none(),
        "the staged candidate is cleaned up: {:?}",
        error.cleanup_failure()
    );
    assert_eq!(
        fs::read(&vendors_path).unwrap(),
        theirs,
        "the other writer's aggregate survives byte for byte"
    );
    let persisted = VendorsDocument::from_json_bytes(&fs::read(&vendors_path).unwrap()).unwrap();
    assert_eq!(
        persisted.vendors[0].phone.as_deref(),
        Some("+1 555 9999"),
        "their phone number survives, not this mutation's rename"
    );
    assert_eq!(
        persisted.vendors[0].name.as_str(),
        "Tarnwick",
        "the planned rename did not land"
    );
}

/// The cost of a semantic expectation, stated as a test rather than left to be
/// discovered: an aggregate reformatted by someone else still means what this mutation
/// was authorized against, so the mutation proceeds and their formatting is
/// overwritten. Under "Canonical means loaded, not stored" that is not a loss of
/// meaning.
#[test]
fn a_vendor_aggregate_reformatted_after_revalidation_is_still_replaced() {
    let (tree, vendors_path, before) =
        seeded_package_ready_for_a_contended_rename("vendor-destination-reformatted");
    // The same aggregate, re-emitted compactly: no indentation, no trailing newline.
    let value: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let theirs = serde_json::to_vec(&value).unwrap();
    assert_ne!(theirs, before, "their formatting differs from ours");
    assert_eq!(
        VendorsDocument::from_json_bytes(&theirs).unwrap(),
        VendorsDocument::from_json_bytes(&before).unwrap(),
        "their aggregate still means exactly what ours did"
    );

    let plan = planned_rename(tree.path());
    let other_writer = ConcurrentWriter::replacing(&vendors_path, &theirs, EditPoint::StagingWrite);
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91"),
        &other_writer,
    );

    assert!(
        other_writer.fired(),
        "the reformatting really did land under this mutation"
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("an aggregate that still means what was planned against is replaced: {report:?}");
    };
    assert_eq!(result.vendor().name.as_str(), "Tarnwick Optics");
    let persisted = fs::read(&vendors_path).unwrap();
    let document = VendorsDocument::from_json_bytes(&persisted).unwrap();
    assert_eq!(document.vendors, [result.vendor().clone()]);
    assert_eq!(
        persisted,
        document.to_json_bytes().unwrap(),
        "the aggregate on disk is this mutation's canonical bytes"
    );
    assert_ne!(
        persisted, theirs,
        "their formatting is what was given up, and it was given up"
    );
}

/// The destination expectation must ask the question the capability asked, and the
/// capability revalidates against the *projection* of the aggregate — which trims
/// every Vendor and Contact field. A stored aggregate carrying untrimmed text projects
/// to exactly what this mutation was planned against, so an uncontended mutation of it
/// must still commit. A raw byte-for-byte parse would refuse it, which would mean this
/// application refusing to edit a file a person had reformatted by hand.
#[test]
fn a_vendor_aggregate_whose_stored_facts_are_untrimmed_still_commits_uncontended() {
    let (tree, vendors_path, before) =
        seeded_package_ready_for_a_contended_rename("vendor-untrimmed-stored-facts");
    let stored = String::from_utf8(before)
        .unwrap()
        .replace("\"+1 555 0123\"", "\"  +1 555 0123  \"");
    fs::write(&vendors_path, &stored).unwrap();
    assert_eq!(
        VendorsDocument::from_json_bytes(stored.as_bytes())
            .unwrap()
            .vendors[0]
            .phone
            .as_deref(),
        Some("  +1 555 0123  "),
        "the stored aggregate really does carry untrimmed text"
    );

    // No concurrent writer at all: nothing changed under this mutation.
    let plan = planned_rename(tree.path());
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91"),
        &RealFileSystem,
    );

    let MutationReport::Committed { result, .. } = report else {
        panic!("an untrimmed stored aggregate is still this aggregate, and edits: {report:?}");
    };
    assert_eq!(result.vendor().name.as_str(), "Tarnwick Optics");
    let document = VendorsDocument::from_json_bytes(&fs::read(&vendors_path).unwrap()).unwrap();
    assert_eq!(document.vendors, [result.vendor().clone()]);
    assert_eq!(
        document.vendors[0].phone.as_deref(),
        Some("+1 555 0123"),
        "the rename landed and the aggregate was written canonically"
    );
}

/// An aggregate replaced with something that no longer parses is not the aggregate
/// this mutation was authorized against, so it is refused rather than overwritten. It
/// is reported as a changed destination, not an unreadable one: the read succeeded,
/// the meaning went.
#[test]
fn a_vendor_aggregate_made_unparseable_after_revalidation_is_refused() {
    let (tree, vendors_path, _before) =
        seeded_package_ready_for_a_contended_rename("vendor-destination-unparseable");
    const THEIRS: &[u8] = b"{ half an aggregate";

    let plan = planned_rename(tree.path());
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91"),
        &ConcurrentWriter::replacing(&vendors_path, THEIRS, EditPoint::StagingWrite),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an aggregate that stopped parsing underneath this mutation is refused: {report:?}");
    };
    let VendorMutationRejection::ReplacementFailed { error } = rejection else {
        panic!("the refusal comes from the replacement: {rejection:?}");
    };
    assert!(
        matches!(
            error.failure(),
            ValidatedReplacementFailure::DestinationChanged
        ),
        "an uninterpretable destination is a changed one, not an unreadable one: {:?}",
        error.failure()
    );
    assert_eq!(
        fs::read(&vendors_path).unwrap(),
        THEIRS,
        "whatever they wrote survives for the repair capability to diagnose"
    );
}

#[test]
fn a_generated_identity_collision_rejects_before_any_write() {
    let tree = package_with_seeded_vendor("vendor-identity-collision");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("the Vendor can be planned");

    let report =
        commit_vendor_mutation(plan, &mut FixedUuid::new(SEEDED_VENDOR_ID), &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a colliding generated identity rejects: {report:?}");
    };
    let VendorMutationRejection::GeneratedIdentityCollision { vendor_id } = &rejection else {
        panic!("a colliding identity is reported as such: {rejection:?}");
    };
    assert_eq!(vendor_id, SEEDED_VENDOR_ID);
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn an_invalid_generated_identity_rejects_before_any_write() {
    let tree = package_with_seeded_vendor("vendor-invalid-identity");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect("the Vendor can be planned");

    let report = commit_vendor_mutation(plan, &mut FixedUuid::new("NOT-A-UUID"), &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("an invalid generated identity rejects: {report:?}");
    };
    let VendorMutationRejection::InvalidGeneratedIdentity { generated } = &rejection else {
        panic!("an invalid identity is reported as such: {rejection:?}");
    };
    assert_eq!(generated, "NOT-A-UUID");
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn renaming_onto_another_vendors_name_is_rejected_and_preserves_prior_bytes() {
    let tree = package_with_seeded_vendor("vendor-rename-collision");
    let root = tree.path();
    let other = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        VendorAssignments::new().assign("name", "Nethermoor"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", " NETHERMOOR "),
        &RealFileSystem,
    )
    .expect_err("renaming onto another Vendor Name is rejected");

    let VendorCapabilityError::DuplicateName { vendor_id } = &error else {
        panic!("a colliding rename is reported as a duplicate: {error:?}");
    };
    assert_eq!(vendor_id, other.vendor_id.as_str());
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn damaged_package_state_rejects_every_vendor_capability() {
    let tree = package_with_seeded_vendor("vendor-damaged-package");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    fs::write(&vendors_path, b"{ not json").unwrap();
    let before = fs::read(&vendors_path).unwrap();

    let creation = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect_err("a damaged Package rejects Vendor creation");
    let edit = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", "Nethermoor"),
        &RealFileSystem,
    )
    .expect_err("a damaged Package rejects Vendor editing");
    let hide = plan_vendor_hide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect_err("a damaged Package rejects hiding a Vendor");
    let unhide = plan_vendor_unhide(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect_err("a damaged Package rejects unhiding a Vendor");
    let show = show_vendor(root, SEEDED_VENDOR_ID, &RealFileSystem)
        .expect_err("a damaged Package rejects Vendor show");

    for error in [creation, edit, hide, unhide, show] {
        assert!(
            matches!(error, VendorCapabilityError::DamagedPackage),
            "a damaged Package is reported as such: {error:?}"
        );
    }
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn edit_and_hide_dry_runs_are_side_effect_free_with_complete_plans() {
    let tree = package_with_seeded_vendor("vendor-dry-runs");
    let root = tree.path();
    let other = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        VendorAssignments::new().assign("name", "Nethermoor"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let index_path = root.join(".spielorders/index.json");
    let vendors_before = fs::read(&vendors_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let edit = plan_vendor_edit(
        root,
        SEEDED_VENDOR_ID,
        &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
        &RealFileSystem,
    )
    .expect("the rename can be planned");
    let hide = plan_vendor_hide(root, other.vendor_id.as_str(), &RealFileSystem)
        .expect("the Vendor can be planned for hiding");

    assert_eq!(edit.kind(), VendorMutationKind::Edit);
    assert!(edit.changes_canonical_vendors());
    assert_eq!(
        edit.planned_vendors()
            .iter()
            .map(|vendor| (vendor.name(), retained_id(vendor.identity())))
            .collect::<Vec<_>>(),
        [
            ("Tarnwick Photonics", Some(SEEDED_VENDOR_ID)),
            ("Nethermoor", Some(other.vendor_id.as_str())),
        ]
    );
    assert_eq!(
        retained_id(edit.subject().identity()),
        Some(SEEDED_VENDOR_ID)
    );
    assert_eq!(hide.kind(), VendorMutationKind::Hide);
    assert!(hide.changes_canonical_vendors());
    assert_eq!(
        hide.planned_vendors()
            .iter()
            .map(|vendor| (vendor.name(), vendor.hidden()))
            .collect::<Vec<_>>(),
        [("Tarnwick", false), ("Nethermoor", true)],
        "the complete post-mutation aggregate carries every Vendor's visibility"
    );
    assert_eq!(
        retained_id(hide.subject().identity()),
        Some(other.vendor_id.as_str())
    );
    for plan in [&edit, &hide] {
        assert_eq!(
            plan.operations()
                .iter()
                .map(|operation| operation.kind())
                .collect::<Vec<_>>(),
            [
                VendorMutationOperationKind::ReplaceVendorRecord,
                VendorMutationOperationKind::RefreshPackageIndex,
            ]
        );
    }
    assert_eq!(fs::read(&vendors_path).unwrap(), vendors_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

#[test]
fn vendor_mutations_never_inspect_or_change_supporting_material() {
    let tree = initialized_package("vendor-supporting-material");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        VendorAssignments::new().assign("name", "Tarnwick"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").vendor_id(vendor.vendor_id.as_str()),
    );
    let material = directory.join("Quotes/nested");
    fs::create_dir_all(&material).unwrap();
    let sentinels = ["supporting-a.bin", "supporting-b.bin"].map(|name| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/assets")
            .join(name);
        let destination = material.join(name);
        fs::copy(&source, &destination).expect("the opaque sentinel is copied");
        (destination.clone(), fs::read(&destination).unwrap())
    });
    let plan = plan_vendor_edit(
        root,
        vendor.vendor_id.as_str(),
        &VendorAssignments::new().assign("name", "Tarnwick Photonics"),
        &RealFileSystem,
    )
    .expect("the rename can be planned");
    let filesystem = ObservingFileSystem::default();

    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"),
        &filesystem,
    );

    assert!(matches!(report, MutationReport::Committed { .. }));
    for (path, bytes) in &sentinels {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{path:?} is unchanged");
    }
    let touched = filesystem
        .reads()
        .into_iter()
        .chain(filesystem.listings())
        .filter(|path| path.starts_with(&material) || path.ends_with("Quotes"))
        .collect::<Vec<_>>();
    assert!(
        touched.is_empty(),
        "Supporting Material stays opaque: {touched:?}"
    );
}

#[test]
fn rejected_assignment_reasons_keep_their_stable_spellings() {
    for (reason, spelling) in [
        (RejectedAssignmentReason::UnknownField, "unknown_field"),
        (
            RejectedAssignmentReason::ImmutableIdentity,
            "immutable_identity",
        ),
        (
            RejectedAssignmentReason::ManagedTechnicalField,
            "managed_technical_field",
        ),
        (
            RejectedAssignmentReason::OrderedCollection,
            "ordered_collection",
        ),
        (RejectedAssignmentReason::Duplicate, "duplicate"),
        (
            RejectedAssignmentReason::BlankRequiredValue,
            "blank_required_value",
        ),
    ] {
        assert_eq!(reason.as_str(), spelling);
    }
}
