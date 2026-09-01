//! Decision sources: Define Vendor identity and historical semantics; Define the
//! Rust CLI capability contract; Define durable metadata schemas and version evolution.

use crate::support::{ObservingFileSystem, RealFileSystem, TemporaryTree};
use spielorders_core::canonical::{PackageIndexRefresh, PackageIndexState};
use spielorders_core::creation::{OrderCreationInput, commit_order_creation, plan_order_creation};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, VendorsDocument};
use spielorders_core::mutation::{MutationReport, RejectedAssignmentReason};
use spielorders_core::operations::OperationFailureCode;
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, LocalCalendar,
    LocalDate, UuidGenerator,
};
use spielorders_core::vendors::{
    ContactAssignments, ContactPositionRole, VendorCapabilityError, VendorContactMutationKind,
    VendorMutationRejection, VendorMutationWarning, commit_vendor_contact_mutation,
    list_vendor_contacts, plan_vendor_contact_add, plan_vendor_contact_edit,
    plan_vendor_contact_move, plan_vendor_contact_remove,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const VENDOR_STAGE: &str = ".spielorders/.vendors.json.spielorders-stage";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContactBoundary {
    /// Not an injection point: the label a cleanup failure carries.
    Cleanup,
    WriteStage,
    SyncStage,
    Replace,
    IndexReplacement,
}

struct FailingContacts {
    boundary: ContactBoundary,
    fired: Cell<bool>,
    fail_cleanup: bool,
}

impl FailingContacts {
    fn at(boundary: ContactBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: false,
        }
    }

    fn with_cleanup_failure(boundary: ContactBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: true,
        }
    }

    fn fail_once(&self, boundary: ContactBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: ContactBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingContacts {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE) && self.fail_once(ContactBoundary::WriteStage) {
            return Err(Self::error(ContactBoundary::WriteStage));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE) && self.fail_once(ContactBoundary::SyncStage) {
            return Err(Self::error(ContactBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/vendors.json")
            && self.fail_once(ContactBoundary::Replace)
        {
            return Err(Self::error(ContactBoundary::Replace));
        }
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(ContactBoundary::IndexReplacement)
        {
            return Err(Self::error(ContactBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(VENDOR_STAGE) && self.fail_cleanup {
            return Err(Self::error(ContactBoundary::Cleanup));
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

struct FixedUuid(&'static str);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.to_owned()
    }
}

struct FixedCalendar;

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 8,
            day: 28,
        }
    }
}

const TWO_CONTACT_VENDORS: &[u8] = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [
        { "name": "Grace Hopper", "role": "Sales", "email": "grace@example.com" },
        { "name": "Alan Turing", "role": "Support" }
      ]
    }
  ]
}
"#;

// Three Contacts are the smallest collection in which a forward move separates a
// destination resolved against the pre-mutation order from one resolved against
// the shorter collection left after the source Contact is taken out. Two Contacts
// cannot: every move is the same swap under either reading.
const THREE_CONTACT_VENDORS: &[u8] = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [
        { "name": "Grace Hopper", "role": "Sales", "email": "grace@example.com" },
        { "name": "Alan Turing", "role": "Support" },
        { "name": "Katherine Johnson", "role": "Accounts" }
      ]
    }
  ]
}
"#;

fn package_with_contact_document(label: &str, vendors: &[u8]) -> TemporaryTree {
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
    fs::write(tree.path().join(".spielorders/vendors.json"), vendors).unwrap();
    tree
}

fn package_with_contacts(label: &str) -> TemporaryTree {
    package_with_contact_document(label, TWO_CONTACT_VENDORS)
}

fn package_with_three_contacts(label: &str) -> TemporaryTree {
    package_with_contact_document(label, THREE_CONTACT_VENDORS)
}

#[test]
fn list_returns_ordered_contacts_with_the_complete_vendor_and_live_references() {
    let tree = package_with_contacts("vendor-contact-list");
    let order = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Laser components").vendor_id(VENDOR_ID),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the referencing Order can be planned");
    assert!(matches!(
        commit_order_creation(
            order,
            &mut FixedUuid("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
            &RealFileSystem,
        ),
        MutationReport::Committed { .. }
    ));

    let projection = list_vendor_contacts(tree.path(), VENDOR_ID, &RealFileSystem)
        .expect("an existing Vendor's Contacts can be listed");

    assert_eq!(projection.vendor().vendor_id.as_str(), VENDOR_ID);
    assert_eq!(projection.vendor().name.as_str(), "Tarnwick");
    assert_eq!(
        projection
            .vendor()
            .contacts
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing"]
    );
    assert_eq!(projection.referencing_orders().len(), 1);
    assert_eq!(
        projection.referencing_orders()[0].order_id(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
}

#[test]
fn add_without_a_position_plans_a_complete_side_effect_free_append() {
    let tree = package_with_contacts("vendor-contact-append-plan");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let index_path = tree.path().join(".spielorders/index.json");
    let vendors_before = fs::read(&vendors_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let plan = plan_vendor_contact_add(
        tree.path(),
        VENDOR_ID,
        &ContactAssignments::new()
            .assign("name", "  Katherine Johnson  ")
            .assign("role", "  Accounts  ")
            .assign("phone", "   "),
        None,
        &RealFileSystem,
    )
    .expect("a valid Contact append can be planned");

    assert_eq!(plan.kind(), VendorContactMutationKind::Add);
    assert_eq!(plan.source_position(), None);
    assert_eq!(plan.destination_position(), Some(3));
    assert_eq!(plan.subject_contact().name.as_str(), "Katherine Johnson");
    assert_eq!(plan.subject_contact().role.as_deref(), Some("Accounts"));
    assert_eq!(plan.subject_contact().phone, None);
    let canonical_root = fs::canonicalize(tree.path()).unwrap();
    assert_eq!(plan.root(), canonical_root);
    assert_eq!(
        plan.vendors_path(),
        canonical_root.join(".spielorders/vendors.json")
    );
    assert_eq!(
        plan.vendor()
            .contacts()
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing", "Katherine Johnson"]
    );
    assert_eq!(plan.operations().len(), 2);
    assert_eq!(fs::read(&vendors_path).unwrap(), vendors_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

#[test]
fn appended_contact_commits_the_complete_vendor_and_refreshes_the_index() {
    let tree = package_with_contacts("vendor-contact-append-commit");
    let plan = plan_vendor_contact_add(
        tree.path(),
        VENDOR_ID,
        &ContactAssignments::new()
            .assign("name", "Katherine Johnson")
            .assign("email", "katherine@example.com"),
        None,
        &RealFileSystem,
    )
    .expect("a valid Contact append can be planned");

    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("the Contact append commits: {report:?}");
    };
    assert_eq!(
        result
            .vendor()
            .contacts
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing", "Katherine Johnson"]
    );
    assert_eq!(
        result.vendor().contacts[2].email.as_deref(),
        Some("katherine@example.com")
    );
    assert!(result.referencing_orders().is_empty());
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);

    let persisted = VendorsDocument::from_json_bytes(
        &fs::read(tree.path().join(".spielorders/vendors.json")).unwrap(),
    )
    .expect("canonical Vendors parse");
    let indexed = IndexDocument::from_json_bytes(
        &fs::read(tree.path().join(".spielorders/index.json")).unwrap(),
    )
    .expect("the refreshed index parses");
    assert_eq!(persisted.vendors[0], *result.vendor());
    assert_eq!(indexed.vendors, persisted.vendors);
}

#[test]
fn explicit_insert_accepts_the_first_and_after_last_one_based_boundaries() {
    for (position, expected) in [
        (1, ["Katherine Johnson", "Grace Hopper", "Alan Turing"]),
        (3, ["Grace Hopper", "Alan Turing", "Katherine Johnson"]),
    ] {
        let tree = package_with_contacts("vendor-contact-insert-boundary");
        let plan = plan_vendor_contact_add(
            tree.path(),
            VENDOR_ID,
            &ContactAssignments::new().assign("name", "Katherine Johnson"),
            Some(position),
            &RealFileSystem,
        )
        .expect("a boundary Contact insertion can be planned");

        assert_eq!(plan.destination_position(), Some(position));
        let report = commit_vendor_contact_mutation(plan, &RealFileSystem);
        let MutationReport::Committed { result, .. } = report else {
            panic!("the boundary insertion commits: {report:?}");
        };
        assert_eq!(
            result
                .vendor()
                .contacts
                .iter()
                .map(|contact| contact.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn insert_rejects_zero_and_positions_beyond_the_after_last_boundary() {
    let tree = package_with_contacts("vendor-contact-insert-range");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    for position in [0, 4] {
        let error = plan_vendor_contact_add(
            tree.path(),
            VENDOR_ID,
            &ContactAssignments::new().assign("name", "Katherine Johnson"),
            Some(position),
            &RealFileSystem,
        )
        .expect_err("an invalid insertion position is rejected");

        let VendorCapabilityError::InvalidContactPosition {
            role,
            position: rejected,
            minimum,
            maximum,
        } = error
        else {
            panic!("an invalid position is reported as such: {error:?}");
        };
        assert_eq!(
            (role, rejected, minimum, maximum),
            (ContactPositionRole::Destination, position, 1, 3)
        );
    }
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn edit_updates_only_assigned_contact_facts_and_preserves_collection_order() {
    let tree = package_with_contacts("vendor-contact-edit");
    let plan = plan_vendor_contact_edit(
        tree.path(),
        VENDOR_ID,
        1,
        &ContactAssignments::new()
            .assign("role", "  Regional Sales  ")
            .assign("email", ""),
        &RealFileSystem,
    )
    .expect("a partial Contact edit can be planned");

    assert_eq!(plan.kind(), VendorContactMutationKind::Edit);
    assert_eq!(plan.source_position(), Some(1));
    assert_eq!(plan.destination_position(), Some(1));
    assert_eq!(plan.subject_contact().name.as_str(), "Grace Hopper");
    assert_eq!(
        plan.subject_contact().role.as_deref(),
        Some("Regional Sales")
    );
    assert_eq!(plan.subject_contact().email, None);

    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the Contact edit commits: {report:?}");
    };
    assert_eq!(
        result
            .vendor()
            .contacts
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing"]
    );
    assert_eq!(
        result.vendor().contacts[0].role.as_deref(),
        Some("Regional Sales")
    );
    assert_eq!(result.vendor().contacts[0].email, None);
    assert_eq!(result.vendor().contacts[1].role.as_deref(), Some("Support"));
}

#[test]
fn semantically_identical_edit_reports_unchanged_without_replacing_bytes() {
    let tree = package_with_contacts("vendor-contact-edit-no-op");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let modified_before = fs::metadata(&vendors_path).unwrap().modified().unwrap();
    let plan = plan_vendor_contact_edit(
        tree.path(),
        VENDOR_ID,
        1,
        &ContactAssignments::new()
            .assign("name", "  Grace Hopper  ")
            .assign("role", "Sales")
            .assign("email", "grace@example.com"),
        &RealFileSystem,
    )
    .expect("a semantically identical edit is a valid plan");

    assert!(!plan.changes_canonical_vendors());
    assert!(plan.operations().is_empty());
    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a no-op Contact edit reports unchanged: {report:?}");
    };
    assert_eq!(result.index().state(), PackageIndexState::NotAssessed);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&vendors_path).unwrap().modified().unwrap(),
        modified_before
    );
}

#[test]
fn move_resolves_one_based_positions_against_the_pre_mutation_order() {
    // The first two cases are the first and last boundaries. The third moves one
    // position forward, which is the case that fails if a destination is resolved
    // against the collection left after the source Contact is removed: that
    // reading leaves the order untouched instead of advancing the Contact.
    for (source, destination, subject, expected) in [
        (
            1,
            3,
            "Grace Hopper",
            ["Alan Turing", "Katherine Johnson", "Grace Hopper"],
        ),
        (
            3,
            1,
            "Katherine Johnson",
            ["Katherine Johnson", "Grace Hopper", "Alan Turing"],
        ),
        (
            1,
            2,
            "Grace Hopper",
            ["Alan Turing", "Grace Hopper", "Katherine Johnson"],
        ),
    ] {
        let tree = package_with_three_contacts("vendor-contact-move-order");
        let plan =
            plan_vendor_contact_move(tree.path(), VENDOR_ID, source, destination, &RealFileSystem)
                .expect("a boundary Contact move can be planned");

        assert_eq!(plan.kind(), VendorContactMutationKind::Move);
        assert_eq!(plan.source_position(), Some(source));
        assert_eq!(plan.destination_position(), Some(destination));
        assert_eq!(plan.subject_contact().name.as_str(), subject);
        let report = commit_vendor_contact_mutation(plan, &RealFileSystem);
        let MutationReport::Committed { result, .. } = report else {
            panic!("the boundary Contact move commits: {report:?}");
        };
        assert_eq!(
            result
                .vendor()
                .contacts
                .iter()
                .map(|contact| contact.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn remove_uses_one_based_position_and_returns_the_complete_refreshed_vendor() {
    let tree = package_with_contacts("vendor-contact-remove");
    let plan = plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem)
        .expect("an existing Contact can be removed");

    assert_eq!(plan.kind(), VendorContactMutationKind::Remove);
    assert_eq!(plan.source_position(), Some(1));
    assert_eq!(plan.destination_position(), None);
    assert_eq!(plan.subject_contact().name.as_str(), "Grace Hopper");
    assert_eq!(
        plan.vendor()
            .contacts()
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>(),
        ["Alan Turing"]
    );

    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the Contact removal commits: {report:?}");
    };
    assert_eq!(result.vendor().vendor_id.as_str(), VENDOR_ID);
    assert_eq!(result.vendor().name.as_str(), "Tarnwick");
    assert_eq!(result.vendor().contacts.len(), 1);
    assert_eq!(result.vendor().contacts[0].name.as_str(), "Alan Turing");
}

#[test]
fn move_to_the_same_pre_mutation_position_is_unchanged() {
    let tree = package_with_contacts("vendor-contact-move-no-op");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let plan = plan_vendor_contact_move(tree.path(), VENDOR_ID, 2, 2, &RealFileSystem)
        .expect("moving to the same position is valid");

    assert_eq!(plan.subject_contact().name.as_str(), "Alan Turing");
    assert!(!plan.changes_canonical_vendors());
    assert!(plan.operations().is_empty());
    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);

    assert!(matches!(report, MutationReport::Unchanged { .. }));
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn edit_move_and_remove_reject_invalid_pre_mutation_positions() {
    let tree = package_with_contacts("vendor-contact-position-rejections");
    let assignments = ContactAssignments::new().assign("role", "Accounts");
    let cases = [
        (
            plan_vendor_contact_edit(tree.path(), VENDOR_ID, 0, &assignments, &RealFileSystem)
                .unwrap_err(),
            ContactPositionRole::Source,
            0,
        ),
        (
            plan_vendor_contact_remove(tree.path(), VENDOR_ID, 3, &RealFileSystem).unwrap_err(),
            ContactPositionRole::Source,
            3,
        ),
        (
            plan_vendor_contact_move(tree.path(), VENDOR_ID, 0, 1, &RealFileSystem).unwrap_err(),
            ContactPositionRole::Source,
            0,
        ),
        (
            plan_vendor_contact_move(tree.path(), VENDOR_ID, 1, 3, &RealFileSystem).unwrap_err(),
            ContactPositionRole::Destination,
            3,
        ),
    ];

    for (error, expected_role, expected_position) in cases {
        let VendorCapabilityError::InvalidContactPosition {
            role,
            position,
            minimum,
            maximum,
        } = error
        else {
            panic!("an invalid Contact position is typed: {error:?}");
        };
        assert_eq!(
            (role, position, minimum, maximum),
            (expected_role, expected_position, 1, 2)
        );
    }
}

#[test]
fn blank_names_and_invalid_contact_assignments_reject_before_any_write() {
    let tree = package_with_contacts("vendor-contact-invalid-assignments");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let absent = plan_vendor_contact_add(
        tree.path(),
        VENDOR_ID,
        &ContactAssignments::new().assign("role", "Sales"),
        None,
        &RealFileSystem,
    )
    .unwrap_err();
    let blank = plan_vendor_contact_edit(
        tree.path(),
        VENDOR_ID,
        1,
        &ContactAssignments::new().assign("name", "   "),
        &RealFileSystem,
    )
    .unwrap_err();
    let invalid = plan_vendor_contact_add(
        tree.path(),
        VENDOR_ID,
        &ContactAssignments::new()
            .assign("name", "Katherine Johnson")
            .assign("vendor_id", "forbidden")
            .assign("role", "Sales")
            .assign("role", "Accounts"),
        None,
        &RealFileSystem,
    )
    .unwrap_err();

    assert!(matches!(absent, VendorCapabilityError::MissingRequiredName));
    let VendorCapabilityError::InvalidAssignments(blank) = blank else {
        panic!("a blank Contact Name is a rejected assignment: {blank:?}");
    };
    assert_eq!(blank[0].field(), "name");
    assert_eq!(
        blank[0].reason(),
        RejectedAssignmentReason::BlankRequiredValue
    );
    let VendorCapabilityError::InvalidAssignments(invalid) = invalid else {
        panic!("invalid Contact assignments are typed: {invalid:?}");
    };
    assert_eq!(
        invalid
            .iter()
            .map(|entry| (entry.field(), entry.reason()))
            .collect::<Vec<_>>(),
        [
            ("vendor_id", RejectedAssignmentReason::UnknownField),
            ("role", RejectedAssignmentReason::Duplicate),
        ]
    );
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn unknown_vendor_and_damaged_package_reject_every_contact_capability() {
    let tree = package_with_contacts("vendor-contact-capability-rejections");
    let unknown = "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91";
    let assignments = ContactAssignments::new().assign("name", "Katherine Johnson");

    assert!(matches!(
        list_vendor_contacts(tree.path(), unknown, &RealFileSystem),
        Err(VendorCapabilityError::UnknownVendor { .. })
    ));
    assert!(matches!(
        plan_vendor_contact_add(tree.path(), unknown, &assignments, None, &RealFileSystem),
        Err(VendorCapabilityError::UnknownVendor { .. })
    ));

    fs::write(
        tree.path().join(".spielorders/version.json"),
        b"damaged version marker",
    )
    .unwrap();
    assert!(matches!(
        list_vendor_contacts(tree.path(), VENDOR_ID, &RealFileSystem),
        Err(VendorCapabilityError::DamagedPackage)
    ));
    assert!(matches!(
        plan_vendor_contact_add(tree.path(), VENDOR_ID, &assignments, None, &RealFileSystem),
        Err(VendorCapabilityError::DamagedPackage)
    ));
    assert!(matches!(
        plan_vendor_contact_edit(tree.path(), VENDOR_ID, 1, &assignments, &RealFileSystem),
        Err(VendorCapabilityError::DamagedPackage)
    ));
    assert!(matches!(
        plan_vendor_contact_move(tree.path(), VENDOR_ID, 1, 2, &RealFileSystem),
        Err(VendorCapabilityError::DamagedPackage)
    ));
    assert!(matches!(
        plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem),
        Err(VendorCapabilityError::DamagedPackage)
    ));
}

#[test]
fn concurrent_vendor_aggregate_change_rejects_before_contact_commit() {
    let tree = package_with_contacts("vendor-contact-concurrent-change");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let plan = plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem)
        .expect("the Contact removal can be planned");
    let concurrent = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [{ "name": "Concurrent Contact" }]
    }
  ]
}
"#;
    fs::write(&vendors_path, concurrent).unwrap();

    let report = commit_vendor_contact_mutation(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a concurrent aggregate change rejects: {report:?}");
    };
    let VendorMutationRejection::PreconditionsChanged { path } = rejection else {
        panic!("the changed Vendor record is identified: {rejection:?}");
    };
    assert_eq!(path, fs::canonicalize(&vendors_path).unwrap());
    assert_eq!(fs::read(&vendors_path).unwrap(), concurrent);
}

#[test]
fn injected_precommit_failures_preserve_the_prior_contact_aggregate_exactly() {
    for boundary in [
        ContactBoundary::WriteStage,
        ContactBoundary::SyncStage,
        ContactBoundary::Replace,
    ] {
        let tree = package_with_contacts("vendor-contact-precommit-failure");
        let vendors_path = tree.path().join(".spielorders/vendors.json");
        let before = fs::read(&vendors_path).unwrap();
        let plan = plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem)
            .expect("the Contact removal can be planned");

        let report = commit_vendor_contact_mutation(plan, &FailingContacts::at(boundary));

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let VendorMutationRejection::ReplacementFailed { error } = rejection else {
            panic!("an injected failure is typed as replacement failure: {rejection:?}");
        };
        assert!(matches!(
            (boundary, error.failure()),
            (
                ContactBoundary::WriteStage,
                ValidatedReplacementFailure::StagingWrite(_)
            ) | (
                ContactBoundary::SyncStage,
                ValidatedReplacementFailure::Durability(_)
            ) | (
                ContactBoundary::Replace,
                ValidatedReplacementFailure::Replacement(_)
            )
        ));
        assert!(error.cleanup_failure().is_none());
        assert_eq!(fs::read(&vendors_path).unwrap(), before);
        assert!(!tree.path().join(VENDOR_STAGE).exists());
    }
}

#[test]
fn failed_staging_cleanup_is_retained_with_the_contact_rejection() {
    let tree = package_with_contacts("vendor-contact-cleanup-failure");
    let vendors_path = tree.path().join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();
    let plan = plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem)
        .expect("the Contact removal can be planned");

    let report = commit_vendor_contact_mutation(
        plan,
        &FailingContacts::with_cleanup_failure(ContactBoundary::SyncStage),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    let VendorMutationRejection::ReplacementFailed { error } = rejection else {
        panic!("the staging failure is retained: {rejection:?}");
    };
    assert!(matches!(
        error.failure(),
        ValidatedReplacementFailure::Durability(_)
    ));
    assert!(error.cleanup_failure().is_some());
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
    assert!(tree.path().join(VENDOR_STAGE).exists());
    fs::remove_file(tree.path().join(VENDOR_STAGE)).unwrap();
}

#[test]
fn failed_index_refresh_keeps_the_canonical_contact_mutation_committed() {
    let tree = package_with_contacts("vendor-contact-index-failure");
    let plan = plan_vendor_contact_remove(tree.path(), VENDOR_ID, 1, &RealFileSystem)
        .expect("the Contact removal can be planned");

    let report = commit_vendor_contact_mutation(
        plan,
        &FailingContacts::at(ContactBoundary::IndexReplacement),
    );

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("the canonical Contact removal remains committed: {report:?}");
    };
    assert_eq!(result.vendor().contacts.len(), 1);
    assert_eq!(result.vendor().contacts[0].name.as_str(), "Alan Turing");
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    let [VendorMutationWarning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
        panic!("the index replacement warning is retained: {warnings:?}");
    };
    assert_eq!(
        failure.code(),
        OperationFailureCode::PackageIndexReplacementFailed
    );
    let persisted = VendorsDocument::from_json_bytes(
        &fs::read(tree.path().join(".spielorders/vendors.json")).unwrap(),
    )
    .expect("the committed Vendor aggregate remains canonical");
    assert_eq!(persisted.vendors[0].contacts.len(), 1);
    assert_eq!(
        persisted.vendors[0].contacts[0].name.as_str(),
        "Alan Turing"
    );
}

#[test]
fn contact_mutation_preserves_vendor_identity_orders_readmes_and_supporting_material() {
    let tree = package_with_contacts("vendor-contact-preservation");
    let order_plan = plan_order_creation(
        tree.path(),
        &OrderCreationInput::new("Laser components").vendor_id(VENDOR_ID),
        &FixedCalendar,
        &RealFileSystem,
    )
    .expect("the referencing Order can be planned");
    let order_report = commit_order_creation(
        order_plan,
        &mut FixedUuid("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        &RealFileSystem,
    );
    let MutationReport::Committed { result, .. } = order_report else {
        panic!("the referencing Order commits: {order_report:?}");
    };
    let order_directory = result
        .order()
        .paths()
        .directory()
        .absolute_path()
        .to_path_buf();
    let order_path = order_directory.join(".spielorders/order.json");
    let order_before = fs::read(&order_path).unwrap();
    fs::write(order_directory.join("README.md"), b"Order notes\n").unwrap();
    let package_readme = tree.path().join("README.md");
    fs::write(&package_readme, b"Package notes\n").unwrap();
    let material = order_directory.join("Quotes/nested");
    fs::create_dir_all(&material).unwrap();
    let sentinels = ["supporting-a.bin", "supporting-b.bin"].map(|name| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/assets")
            .join(name);
        let destination = material.join(name);
        fs::copy(source, &destination).unwrap();
        (destination.clone(), fs::read(&destination).unwrap())
    });
    let plan = plan_vendor_contact_add(
        tree.path(),
        VENDOR_ID,
        &ContactAssignments::new().assign("name", "Katherine Johnson"),
        None,
        &RealFileSystem,
    )
    .expect("the Contact append can be planned");
    let filesystem = ObservingFileSystem::default();

    let report = commit_vendor_contact_mutation(plan, &filesystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("the Contact append commits: {report:?}");
    };
    assert_eq!(result.vendor().vendor_id.as_str(), VENDOR_ID);
    assert_eq!(result.vendor().name.as_str(), "Tarnwick");
    assert_eq!(result.referencing_orders().len(), 1);
    assert_eq!(result.referencing_orders()[0].directory(), order_directory);
    assert!(order_directory.is_dir());
    assert_eq!(fs::read(&order_path).unwrap(), order_before);
    assert_eq!(
        fs::read(order_directory.join("README.md")).unwrap(),
        b"Order notes\n"
    );
    assert_eq!(fs::read(&package_readme).unwrap(), b"Package notes\n");
    for (path, bytes) in sentinels {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let touched = filesystem
        .reads()
        .into_iter()
        .chain(filesystem.listings())
        .filter(|path| path.starts_with(&material) || path.ends_with("Quotes"))
        .collect::<Vec<_>>();
    assert!(
        touched.is_empty(),
        "Supporting Material stays recursively opaque: {touched:?}"
    );
}
