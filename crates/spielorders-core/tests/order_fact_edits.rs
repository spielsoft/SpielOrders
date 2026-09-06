//! Decision sources: Define the Order record and lifecycle contract; Define Vendor
//! identity and historical semantics; Define the Rust CLI capability contract;
//! Define integrity, validation, and repair boundaries.

use crate::support::{ConcurrentWriter, EditPoint, RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{
    OrderAdoptionInput, OrderItemInput, commit_order_adoption, plan_order_adoption,
};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
};
use spielorders_core::creation::{OrderCreationInput, commit_order_creation, plan_order_creation};
use spielorders_core::filesystem::ValidatedReplacementFailure;
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderDocument, OrderStatus};
use spielorders_core::mutation::{MutationReport, RejectedAssignmentReason};
use spielorders_core::operations::OperationFailureCode;
use spielorders_core::order_edits::{
    DateOrderedSource, OrderAssignments, OrderEditOperationKind, OrderEditPlanError,
    OrderEditRejection, OrderEditResult, OrderEditReusableValues, OrderEditWarning, OrderFactKind,
    PlannedOrderFacts, commit_order_edit, plan_order_edit,
};
use spielorders_core::package::ReusableValue;
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind, LocalCalendar,
    LocalDate, UuidGenerator,
};
use spielorders_core::vendors::{
    VendorAssignments, commit_vendor_mutation, plan_vendor_creation, show_vendor,
};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

struct FixedCalendar {
    date: LocalDate,
}

impl FixedCalendar {
    fn on(year: i32, month: u8, day: u8) -> Self {
        Self {
            date: LocalDate { year, month, day },
        }
    }
}

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        self.date
    }
}

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
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

fn created_order(root: &Path, uuid: &str, input: &OrderCreationInput) -> PathBuf {
    let plan = plan_order_creation(
        root,
        input,
        &FixedCalendar::on(2026, 8, 22),
        &RealFileSystem,
    )
    .expect("the fixture Order can be planned");
    let report = commit_order_creation(plan, &mut FixedUuid(uuid.into()), &RealFileSystem);
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
fn title_edit_plan_reports_complete_facts_and_writes_nothing() {
    let tree = initialized_package("order-edit-title-plan");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").orderer("Ada Lovelace"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let index_path = root.join(".spielorders/index.json");
    let order_before = fs::read(&order_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "  Laser mounts  "),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("an existing Order's title can be edited");

    assert_eq!(
        plan.facts().order_id(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "an edit retains the immutable Order ID"
    );
    assert_eq!(plan.facts().title(), "Laser mounts");
    assert_eq!(plan.facts().date_ordered(), "2026-08-22");
    assert_eq!(plan.facts().status(), OrderStatus::InPreparation);
    assert_eq!(plan.facts().orderer(), Some("Ada Lovelace"));
    assert_eq!(plan.facts().vendor_id(), None);
    assert_eq!(plan.facts().currency(), None);
    assert_eq!(plan.date_ordered_source(), DateOrderedSource::Retained);
    assert_eq!(
        plan.reusable_values().orderer(),
        &ReusableValue::Reused("Ada Lovelace".into())
    );
    assert!(plan.changes_canonical_order());
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
                OrderEditOperationKind::ReplaceOrderRecord,
                Path::new(
                    "Orders/2026_08_22_Laser_components_Ada_Lovelace/.spielorders/order.json"
                )
            ),
            (
                OrderEditOperationKind::RefreshPackageIndex,
                Path::new(".spielorders/index.json")
            ),
        ]
    );
    assert_eq!(fs::read(&order_path).unwrap(), order_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

#[test]
fn commit_persists_facts_refreshes_index_and_retains_the_drifted_directory_name() {
    let tree = initialized_package("order-edit-commit");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let basename_before = directory.file_name().unwrap().to_owned();

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new()
            .assign("title", "Laser mounts")
            .assign("date_ordered", "2026-07-19")
            .assign("currency", "USD"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the Order can be edited");

    let report = commit_order_edit(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("a valid Order edit commits: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(
        result.order().document().date_ordered.as_str(),
        "2026-07-19"
    );
    assert_eq!(
        result
            .order()
            .document()
            .currency
            .as_ref()
            .map(|c| c.as_str()),
        Some("USD")
    );
    assert_eq!(
        result.order().document().order_id.as_str(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(result.order().order_total(), 0.0);
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);

    // Directory drift is ordinary allowed state; only explicit alignment renames.
    assert!(directory.is_dir());
    assert_eq!(directory.file_name().unwrap(), basename_before);
    assert_eq!(
        basename_before,
        std::ffi::OsStr::new("2026_08_22_Laser_components")
    );

    let persisted = OrderDocument::from_json_bytes(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.as_str(), "Laser mounts");
    assert_eq!(persisted.package_root.stored_text(), Some("../.."));
    let indexed =
        IndexDocument::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
            .expect("the refreshed derived index parses");
    assert_eq!(indexed.orders.len(), 1);
    assert_eq!(indexed.orders[0].order.title.as_str(), "Laser mounts");
    assert_eq!(
        indexed.orders[0].order_directory.as_str(),
        "Orders/2026_08_22_Laser_components"
    );
}

fn committed_edit(
    directory: &Path,
    assignments: OrderAssignments,
    calendar: &FixedCalendar,
) -> OrderEditResult {
    let plan = plan_order_edit(directory, &assignments, calendar, &RealFileSystem)
        .expect("the edit can be planned");
    let report = commit_order_edit(plan, &RealFileSystem);
    let MutationReport::Committed { result, .. } = report else {
        panic!("the edit commits: {report:?}");
    };
    result
}

/// An Order already at `ordered`, whose Date Ordered is deliberately not the
/// calendar date used by the edits under test.
fn ordered_order(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = initialized_package(label);
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let result = committed_edit(
        &directory,
        OrderAssignments::new()
            .assign("status", "ordered")
            .assign("date_ordered", "2026-08-22"),
        &FixedCalendar::on(2026, 8, 22),
    );
    assert_eq!(result.order().document().status, OrderStatus::Ordered);
    assert_eq!(
        result.order().document().date_ordered.as_str(),
        "2026-08-22"
    );
    (tree, directory)
}

#[test]
fn entering_ordered_from_any_other_status_applies_todays_local_date() {
    // The rule keys on arriving at `ordered` from any other status. Exercising only
    // the created `in_preparation` default cannot separate that rule from one that
    // keys on `in_preparation` alone, which would silently keep the stale archival
    // date on the Hold, Received, and Complete corrections users actually make.
    for (label, prior, prior_spelling) in [
        ("in-preparation", OrderStatus::InPreparation, None),
        ("hold", OrderStatus::Hold, Some("hold")),
        ("received", OrderStatus::Received, Some("received")),
        ("complete", OrderStatus::Complete, Some("complete")),
    ] {
        let tree = initialized_package(&format!("order-edit-entering-ordered-{label}"));
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            &OrderCreationInput::new("Laser components"),
        );
        if let Some(prior_spelling) = prior_spelling {
            let staged = committed_edit(
                &directory,
                OrderAssignments::new().assign("status", prior_spelling),
                &FixedCalendar::on(2026, 8, 22),
            );
            assert_eq!(staged.order().document().status, prior);
            assert_eq!(
                staged.order().document().date_ordered.as_str(),
                "2026-08-22",
                "reaching {label} leaves the archival date alone"
            );
        }

        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign("status", "ordered"),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .expect("entering ordered can be planned");

        assert_eq!(
            plan.date_ordered_source(),
            DateOrderedSource::AppliedOnEnteringOrdered,
            "entering ordered from {label} is an entry"
        );
        assert_eq!(plan.facts().date_ordered(), "2026-09-01");
        assert_ne!(plan.facts().date_ordered(), "2026-08-22");
        let result = committed_edit(
            &directory,
            OrderAssignments::new().assign("status", "ordered"),
            &FixedCalendar::on(2026, 9, 1),
        );
        assert_eq!(
            result.order().document().date_ordered.as_str(),
            "2026-09-01",
            "entering ordered from {label} commits the entry date"
        );
        assert_eq!(result.order().document().status, OrderStatus::Ordered);
    }
}

#[test]
fn an_order_already_ordered_keeps_its_date_when_no_date_is_assigned() {
    let (_tree, directory) = ordered_order("order-edit-already-ordered");

    // Re-asserting the same status is not entry, and neither is editing another
    // fact, so a later calendar date must not silently overwrite the archival date.
    for assignments in [
        OrderAssignments::new().assign("status", "ordered"),
        OrderAssignments::new().assign("title", "Laser mounts"),
        OrderAssignments::new()
            .assign("status", "ordered")
            .assign("title", "Laser adapters"),
    ] {
        let plan = plan_order_edit(
            &directory,
            &assignments,
            &FixedCalendar::on(2026, 12, 25),
            &RealFileSystem,
        )
        .expect("the edit can be planned");

        assert_eq!(
            plan.date_ordered_source(),
            DateOrderedSource::Retained,
            "an already-ordered Order does not re-enter ordered"
        );
        assert_eq!(plan.facts().date_ordered(), "2026-08-22");
        assert_eq!(plan.facts().status(), OrderStatus::Ordered);
    }
}

#[test]
fn leaving_ordered_for_another_status_keeps_the_date() {
    let (_tree, directory) = ordered_order("order-edit-leaving-ordered");

    for status in ["received", "complete", "hold", "in_preparation"] {
        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign("status", status),
            &FixedCalendar::on(2026, 12, 25),
            &RealFileSystem,
        )
        .expect("every direct transition can be planned");

        assert_eq!(
            plan.date_ordered_source(),
            DateOrderedSource::Retained,
            "only entry into ordered triggers the date rule, not {status}"
        );
        assert_eq!(plan.facts().date_ordered(), "2026-08-22");
    }
}

#[test]
fn an_explicit_same_mutation_date_wins_regardless_of_assignment_order() {
    for assignments in [
        OrderAssignments::new()
            .assign("status", "ordered")
            .assign("date_ordered", "2026-07-19"),
        OrderAssignments::new()
            .assign("date_ordered", "2026-07-19")
            .assign("status", "ordered"),
    ] {
        let tree = initialized_package("order-edit-explicit-date");
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            &OrderCreationInput::new("Laser components"),
        );

        let plan = plan_order_edit(
            &directory,
            &assignments,
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .expect("an explicit historical date can be planned");

        assert_eq!(plan.date_ordered_source(), DateOrderedSource::Assigned);
        assert_eq!(plan.facts().date_ordered(), "2026-07-19");
        assert_ne!(plan.facts().date_ordered(), "2026-09-01");
        assert_eq!(plan.facts().status(), OrderStatus::Ordered);
    }
}

#[test]
fn every_direct_status_transition_commits() {
    let tree = initialized_package("order-edit-transitions");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let calendar = FixedCalendar::on(2026, 8, 22);

    for status in [
        OrderStatus::Complete,
        OrderStatus::Hold,
        OrderStatus::Received,
        OrderStatus::InPreparation,
        OrderStatus::Ordered,
        OrderStatus::Hold,
    ] {
        let spelling = match status {
            OrderStatus::InPreparation => "in_preparation",
            OrderStatus::Hold => "hold",
            OrderStatus::Ordered => "ordered",
            OrderStatus::Received => "received",
            OrderStatus::Complete => "complete",
        };
        let result = committed_edit(
            &directory,
            OrderAssignments::new().assign("status", spelling),
            &calendar,
        );
        assert_eq!(result.order().document().status, status);
    }
}

/// Every rejected assignment set below also carries a valid change that would
/// otherwise alter canonical bytes, so byte-identity proves atomicity rather than
/// merely proving the invalid field was refused.
#[test]
fn one_rejected_assignment_leaves_the_whole_order_untouched() {
    let tree = initialized_package("order-edit-atomic");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components").orderer("Ada Lovelace"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let index_path = root.join(".spielorders/index.json");
    let order_before = fs::read(&order_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();
    let basename_before = directory.file_name().unwrap().to_owned();
    let valid = ("title", "Laser mounts");

    let cases: Vec<(&str, OrderAssignments)> = vec![
        (
            "unknown field",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("received_on", "2026-09-01"),
        ),
        (
            "immutable identity",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("order_id", "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f"),
        ),
        (
            "managed technical locator",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("package_root", "../../.."),
        ),
        (
            "managed technical version",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("schema_version", "2"),
        ),
        (
            "ordered collection",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("items", "[]"),
        ),
        (
            "duplicate assignment",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("currency", "USD")
                .assign("currency", "EUR"),
        ),
        (
            "introduce on a non-reusable field",
            OrderAssignments::new()
                .assign("currency", "USD")
                .introduce(valid.0, valid.1),
        ),
        (
            "blank required title",
            OrderAssignments::new()
                .assign("currency", "USD")
                .assign("title", "   "),
        ),
        (
            "blank required status",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("status", ""),
        ),
        (
            "invalid calendar date",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("date_ordered", "2026-02-30"),
        ),
        (
            "unknown status",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("status", "shipped"),
        ),
        (
            "malformed currency",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("currency", "usd"),
        ),
        (
            "unresolved Vendor",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("vendor_id", "e0990d31-06aa-42e9-97c8-088e3e894822"),
        ),
        (
            "unconfirmed new reusable value",
            OrderAssignments::new()
                .assign(valid.0, valid.1)
                .assign("project", "Telescope"),
        ),
    ];

    for (label, assignments) in cases {
        let error = plan_order_edit(
            &directory,
            &assignments,
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("{label} is rejected"));
        assert!(
            !matches!(error, OrderEditPlanError::FileSystem { .. }),
            "{label} is a domain rejection, not an effect failure: {error:?}"
        );
        assert_eq!(
            fs::read(&order_path).unwrap(),
            order_before,
            "{label} leaves the canonical Order byte-identical"
        );
        assert_eq!(
            fs::read(&index_path).unwrap(),
            index_before,
            "{label} leaves the derived index byte-identical"
        );
        assert_eq!(
            directory.file_name().unwrap(),
            basename_before,
            "{label} leaves the Order Directory name unchanged"
        );
    }
}

#[test]
fn rejected_assignments_name_every_offending_field_and_reason() {
    let tree = initialized_package("order-edit-assignment-reasons");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );

    let error = plan_order_edit(
        &directory,
        &OrderAssignments::new()
            .assign("order_id", "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f")
            .assign("schema_version", "2")
            .assign("package_root", "../..")
            .assign("items", "[]")
            .assign("received_on", "2026-09-01")
            .assign("currency", "USD")
            .assign("currency", "EUR")
            .introduce("title", "Laser mounts")
            .assign("status", " "),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect_err("every invalid assignment is rejected at once");

    let OrderEditPlanError::InvalidAssignments(rejected) = &error else {
        panic!("invalid assignments are reported as such: {error:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field(), entry.reason()))
            .collect::<Vec<_>>(),
        [
            ("order_id", RejectedAssignmentReason::ImmutableIdentity),
            (
                "schema_version",
                RejectedAssignmentReason::ManagedTechnicalField
            ),
            (
                "package_root",
                RejectedAssignmentReason::ManagedTechnicalField
            ),
            ("items", RejectedAssignmentReason::OrderedCollection),
            ("received_on", RejectedAssignmentReason::UnknownField),
            ("currency", RejectedAssignmentReason::Duplicate),
            ("title", RejectedAssignmentReason::NotAReusableValue),
            ("status", RejectedAssignmentReason::BlankRequiredValue),
        ]
    );
}

/// Adopts a directory carrying every optional scalar plus two Items, so clearing
/// one fact can be shown not to disturb the others or the ordered Items.
fn fully_populated_order(label: &str) -> (TemporaryTree, PathBuf, String) {
    let tree = initialized_package(label);
    let root = tree.path();
    let vendor = created_vendor(root, "Tarnwick");
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).unwrap();
    let plan = plan_order_adoption(
        root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold)
            .vendor_id(&vendor)
            .orderer("Ada Lovelace")
            .ordering_institution("Northwind")
            .currency("USD")
            .project("Telescope")
            .order_group("Summer build")
            .item(OrderItemInput::new("Mirror mount", 2.0, 84.5).catalog_number("TM-100"))
            .item(OrderItemInput::new("Lens", 3.0, 10.0)),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    let report = commit_order_adoption(
        plan,
        &mut FixedUuid("8b6e3f0a-8ec5-4d47-95e1-412652a56d16".into()),
        &RealFileSystem,
    );
    assert!(matches!(report, MutationReport::Committed { .. }));
    (tree, directory, vendor)
}

fn created_vendor(root: &Path, name: &str) -> String {
    let plan = plan_vendor_creation(
        root,
        &VendorAssignments::new().assign("name", name),
        &RealFileSystem,
    )
    .expect("the fixture Vendor can be planned");
    let report = commit_vendor_mutation(
        plan,
        &mut FixedUuid("e0990d31-06aa-42e9-97c8-088e3e894822".into()),
        &RealFileSystem,
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    result.vendor().vendor_id.as_str().to_owned()
}

#[test]
fn every_optional_scalar_clears_explicitly_without_disturbing_the_rest() {
    let (_tree, directory, vendor) = fully_populated_order("order-edit-clearing");

    for field in [
        "vendor_id",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
    ] {
        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign(field, ""),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .unwrap_or_else(|error| panic!("clearing {field} can be planned: {error:?}"));

        let facts = plan.facts();
        let cleared = match field {
            "vendor_id" => facts.vendor_id(),
            "orderer" => facts.orderer(),
            "ordering_institution" => facts.ordering_institution(),
            "currency" => facts.currency(),
            "project" => facts.project(),
            _ => facts.order_group(),
        };
        assert_eq!(cleared, None, "{field} is cleared");
        // Required facts, unnamed optional facts, and the ordered Items all survive.
        assert_eq!(facts.title(), "Laser components");
        assert_eq!(facts.date_ordered(), "2026-08-22");
        assert_eq!(facts.status(), OrderStatus::Hold);
        assert_eq!(facts.item_totals(), [169.0, 30.0]);
        assert_eq!(facts.order_total(), 199.0);
        for (other, value) in [
            ("vendor_id", facts.vendor_id().map(|_| vendor.as_str())),
            ("orderer", facts.orderer()),
            ("ordering_institution", facts.ordering_institution()),
            ("currency", facts.currency()),
            ("project", facts.project()),
            ("order_group", facts.order_group()),
        ] {
            if other != field {
                assert!(
                    value.is_some(),
                    "clearing {field} must not also clear {other}"
                );
            }
        }
    }

    // Clearing every optional scalar in one atomic set leaves only required facts.
    let result = committed_edit(
        &directory,
        OrderAssignments::new()
            .assign("vendor_id", "")
            .assign("orderer", "")
            .assign("ordering_institution", "")
            .assign("currency", "")
            .assign("project", "")
            .assign("order_group", ""),
        &FixedCalendar::on(2026, 9, 1),
    );
    let document = result.order().document();
    assert_eq!(document.vendor_id, None);
    assert_eq!(document.orderer, None);
    assert_eq!(document.ordering_institution, None);
    assert_eq!(document.currency, None);
    assert_eq!(document.project, None);
    assert_eq!(document.order_group, None);
    assert_eq!(document.title.as_str(), "Laser components");
    assert_eq!(document.items.len(), 2);
    assert_eq!(result.order().order_total(), 199.0);
    let persisted = fs::read_to_string(directory.join(".spielorders/order.json")).unwrap();
    assert!(
        !persisted.contains("\"orderer\""),
        "a cleared optional fact is omitted, never written as empty text: {persisted}"
    );
}

const REUSABLE: [(&str, &str, &str); 4] = [
    ("orderer", "Ada Lovelace", "  ada LOVELACE  "),
    ("ordering_institution", "Northwind", " northwind "),
    ("project", "Telescope", "teleSCOPE"),
    ("order_group", "Summer build", " SUMMER build "),
];

/// A second Order in the Package whose reusable values are all absent, alongside a
/// first Order that already contributes each category's canonical spelling.
fn order_beside_populated_order(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, _populated, _vendor) = fully_populated_order(label);
    let second = created_order(
        tree.path(),
        "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f",
        &OrderCreationInput::new("Optical table"),
    );
    (tree, second)
}

#[test]
fn a_case_variant_reusable_value_reuses_the_existing_canonical_spelling() {
    let (_tree, directory) = order_beside_populated_order("order-edit-reusable-reuse");

    for (field, canonical, variant) in REUSABLE {
        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign(field, variant),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .unwrap_or_else(|error| panic!("reusing {field} can be planned: {error:?}"));

        assert_eq!(
            outcome(plan.reusable_values(), field),
            &ReusableValue::Reused(canonical.into()),
            "{field} reports reuse rather than introduction"
        );
        assert_eq!(
            planned_value(plan.facts(), field),
            Some(canonical),
            "{field} stores the existing canonical spelling, not the submitted text"
        );
    }
}

#[test]
fn a_genuinely_new_reusable_value_requires_explicit_introduction() {
    let (_tree, directory) = order_beside_populated_order("order-edit-reusable-new");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();

    for (field, _canonical, _variant) in REUSABLE {
        let error = plan_order_edit(
            &directory,
            &OrderAssignments::new()
                .assign("title", "Optical bench")
                .assign(field, "  Weather station  "),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("an unconfirmed new {field} is rejected"));

        let OrderEditPlanError::UnconfirmedReusableValue {
            field: named,
            proposed,
        } = &error
        else {
            panic!("an unconfirmed new {field} is reported as such: {error:?}");
        };
        assert_eq!(named, field);
        assert_eq!(proposed, "Weather station");
        assert_eq!(fs::read(&order_path).unwrap(), before);

        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new()
                .assign("title", "Optical bench")
                .introduce(field, "  Weather station  "),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .unwrap_or_else(|error| panic!("introducing {field} can be planned: {error:?}"));

        assert_eq!(
            outcome(plan.reusable_values(), field),
            &ReusableValue::New("Weather station".into()),
            "{field} reports introduction rather than reuse"
        );
        assert_eq!(planned_value(plan.facts(), field), Some("Weather station"));
    }
}

#[test]
fn introduction_intent_still_reuses_an_existing_canonical_spelling() {
    let (_tree, directory) = order_beside_populated_order("order-edit-reusable-introduce-match");

    for (field, canonical, variant) in REUSABLE {
        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().introduce(field, variant),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .unwrap_or_else(|error| panic!("introducing a matching {field} can be planned: {error:?}"));

        // Introduction intent permits creating a value; it never duplicates one.
        assert_eq!(
            outcome(plan.reusable_values(), field),
            &ReusableValue::Reused(canonical.into())
        );
        assert_eq!(planned_value(plan.facts(), field), Some(canonical));
    }
}

fn outcome<'a>(values: &'a OrderEditReusableValues, field: &str) -> &'a ReusableValue {
    match field {
        "orderer" => values.orderer(),
        "ordering_institution" => values.ordering_institution(),
        "project" => values.project(),
        _ => values.order_group(),
    }
}

fn planned_value<'a>(facts: &'a PlannedOrderFacts, field: &str) -> Option<&'a str> {
    match field {
        "orderer" => facts.orderer(),
        "ordering_institution" => facts.ordering_institution(),
        "project" => facts.project(),
        _ => facts.order_group(),
    }
}

/// **The Vendor an edit leaves is resolved by the plan and by the result**, because the
/// projection a mutation answers with resolves one and an edit is what can change which
/// Vendor that is. A run asked to plan only has no committed record to resolve it from, so
/// the plan carries it; a run that committed reports the Vendor the refreshed load found.
#[test]
fn a_live_vendor_reference_can_be_assigned_and_cleared() {
    let tree = initialized_package("order-edit-vendor");
    let root = tree.path();
    let vendor = created_vendor(root, "Tarnwick");
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("vendor_id", &vendor),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("a live Vendor can be referenced");
    assert_eq!(
        plan.vendor().map(|vendor| vendor.name.as_str()),
        Some("Tarnwick"),
        "the Vendor the plan would leave is resolved against what it would store"
    );

    let assigned = committed_edit(
        &directory,
        OrderAssignments::new().assign("vendor_id", &vendor),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(
        assigned.vendor().map(|vendor| vendor.name.as_str()),
        Some("Tarnwick"),
        "and the committed Order resolves it too, so the answer needs no second read"
    );
    assert_eq!(
        assigned
            .order()
            .document()
            .vendor_id
            .as_ref()
            .map(|v| v.as_str()),
        Some(vendor.as_str())
    );

    // The reference resolves live through the Package rather than a stored snapshot.
    let projection = show_vendor(root, &vendor, &RealFileSystem).expect("the Vendor resolves");
    assert_eq!(
        projection
            .referencing_orders()
            .iter()
            .map(|order| order.order_id())
            .collect::<Vec<_>>(),
        ["8b6e3f0a-8ec5-4d47-95e1-412652a56d16"]
    );

    let cleared = committed_edit(
        &directory,
        OrderAssignments::new().assign("vendor_id", ""),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(cleared.order().document().vendor_id, None);
    assert!(
        cleared.vendor().is_none(),
        "an Order that references no Vendor resolves none"
    );
    let projection = show_vendor(root, &vendor, &RealFileSystem).expect("the Vendor resolves");
    assert!(projection.referencing_orders().is_empty());
}

/// **Assigning is itself the confirmation, and the core is what knows for which fields.**
///
/// A genuinely new Reusable Free-Entry Value needs explicit confirmation, and a caller for
/// whom supplying the value *is* that confirmation — a CLI command line is the decided one
/// — hands over the whole set the same way. So the same method carries a new Project into
/// the record and an ordinary Title beside it, and neither has to know which of the two it
/// was: a confirmation on a field that is not a Reusable Free-Entry Value would otherwise
/// be refused as one.
#[test]
fn a_confirmed_assignment_introduces_a_new_value_and_still_assigns_an_ordinary_fact() {
    let tree = initialized_package("order-edit-confirmed-assignment");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );

    let committed = committed_edit(
        &directory,
        OrderAssignments::new()
            .confirm("project", "Telescope")
            .confirm("title", "Laser heads"),
        &FixedCalendar::on(2026, 9, 1),
    );

    assert_eq!(
        committed.order().document().project.as_deref(),
        Some("Telescope"),
        "a Package that uses no Project yet takes the confirmed new value"
    );
    assert_eq!(
        committed.order().document().title.as_str(),
        "Laser heads",
        "and a field that is no Reusable Free-Entry Value is assigned rather than refused"
    );
}

#[test]
fn a_no_op_assignment_set_reports_unchanged_without_replacing_canonical_bytes() {
    let (_tree, directory, _vendor) = fully_populated_order("order-edit-no-op");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    let modified_before = fs::metadata(&order_path).unwrap().modified().unwrap();

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new()
            .assign("title", "  Laser components  ")
            .assign("status", "hold")
            .assign("project", "teleSCOPE"),
        &FixedCalendar::on(2026, 12, 25),
        &RealFileSystem,
    )
    .expect("a no-op assignment set is a valid plan");

    assert!(!plan.changes_canonical_order());
    assert!(
        plan.operations().is_empty(),
        "a plan that changes nothing announces no managed operation"
    );
    assert_eq!(plan.date_ordered_source(), DateOrderedSource::Retained);

    let report = commit_order_edit(plan, &RealFileSystem);

    let MutationReport::Unchanged { result, .. } = report else {
        panic!("a no-op assignment set reports unchanged: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser components");
    assert_eq!(result.order().order_total(), 199.0);
    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&order_path).unwrap().modified().unwrap(),
        modified_before
    );
}

#[test]
fn a_damaged_order_is_rejected_without_touching_its_record() {
    let tree = initialized_package("order-edit-damaged");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    // A parseable record whose Date Ordered is not a real calendar date damages
    // only this Order, so the edit must refuse rather than repair it.
    let damaged = fs::read_to_string(&order_path)
        .unwrap()
        .replace("2026-08-22", "2026-02-30");
    fs::write(&order_path, &damaged).unwrap();

    let error = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect_err("a damaged Order cannot be edited");

    let OrderEditPlanError::DamagedOrder { path } = &error else {
        panic!("a damaged Order is reported as such: {error:?}");
    };
    assert!(path.ends_with(directory.file_name().unwrap()));
    assert_eq!(fs::read_to_string(&order_path).unwrap(), damaged);
}

#[test]
fn every_unusable_order_record_shape_reports_one_damaged_order() {
    for (label, corrupt) in [
        ("malformed syntax", Some(b"{ not json".to_vec())),
        (
            "unknown field",
            Some(br#"{"schema_version":1,"nope":true}"#.to_vec()),
        ),
        ("absent record", None),
    ] {
        let tree = initialized_package("order-edit-unusable-record");
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            &OrderCreationInput::new("Laser components"),
        );
        let order_path = directory.join(".spielorders/order.json");
        match &corrupt {
            Some(bytes) => fs::write(&order_path, bytes).unwrap(),
            None => fs::remove_file(&order_path).unwrap(),
        }

        let error = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign("title", "Laser mounts"),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .err()
        .unwrap_or_else(|| panic!("{label} is rejected"));

        assert!(
            matches!(error, OrderEditPlanError::DamagedOrder { .. }),
            "{label} is one Damaged Order: {error:?}"
        );
        match &corrupt {
            Some(bytes) => assert_eq!(&fs::read(&order_path).unwrap(), bytes),
            None => assert!(!order_path.exists()),
        }
    }

    // A record that is present and well formed but cannot be read is the fourth
    // unusable shape. The integrity decision names `unreadable` beside missing,
    // malformed, and wrongly typed, so it damages only this Order rather than
    // surfacing as an incidental filesystem error the caller must interpret.
    let tree = initialized_package("order-edit-unreadable-record");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();

    let error = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &FailingOrder::at(OrderBoundary::ReadRecord),
    )
    .expect_err("an unreadable Order record is rejected");

    assert!(
        matches!(error, OrderEditPlanError::DamagedOrder { .. }),
        "an unreadable record is one Damaged Order: {error:?}"
    );
    assert_eq!(fs::read(&order_path).unwrap(), before);
}

#[test]
fn a_concurrent_order_record_change_rejects_before_any_write() {
    let tree = initialized_package("order-edit-concurrent");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned");
    let intervening = committed_edit(
        &directory,
        OrderAssignments::new().assign("currency", "USD"),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(
        intervening
            .order()
            .document()
            .currency
            .as_ref()
            .map(|c| c.as_str()),
        Some("USD")
    );
    let before = fs::read(&order_path).unwrap();

    let report = commit_order_edit(plan, &RealFileSystem);

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a concurrent record change rejects: {report:?}");
    };
    let OrderEditRejection::PreconditionsChanged { location } = &rejection else {
        panic!("a concurrent record change is a precondition change: {rejection:?}");
    };
    assert!(
        location
            .absolute_path()
            .ends_with(".spielorders/order.json")
    );
    assert!(
        location
            .package_relative_path()
            .is_some_and(|relative| relative.ends_with(".spielorders/order.json")),
        "a refusal names the record in the Package's own spelling: {location:?}"
    );
    assert_eq!(
        fs::read(&order_path).unwrap(),
        before,
        "the intervening edit is not silently overwritten"
    );
}

/// The refusal that makes commit-time candidate validation redundant here, pinned at
/// the site that actually performs it — and it names the fact it refused.
///
/// An Order edit's candidate is the planned document serialized, and the plan is only
/// built from facts that parsed and validated. Nothing reaches a commit holding facts
/// that were never validated, which is why the commit-time guard was removed rather than
/// kept as a net.
///
/// **Each of the three facts whose value the record constrains is refused by name.** A
/// Date Ordered with the shape of a date but no such day parses and then fails calendar
/// validation, and one with no date shape at all fails to parse: two different internal
/// paths, one answer, because a caller correcting an assignment needs to be told which
/// assignment. A refusal that said only "these facts are invalid" is what an atomic set
/// makes least useful.
#[test]
fn a_value_the_record_cannot_hold_is_refused_by_name_while_planning_and_writes_nothing() {
    let tree = initialized_package("order-edit-invalid-facts-refused-at-plan");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();

    for (field, proposed, expected) in [
        ("date_ordered", "2026-02-30", OrderFactKind::DateOrdered),
        (
            "date_ordered",
            "the ides of March",
            OrderFactKind::DateOrdered,
        ),
        ("status", "shipped", OrderFactKind::Status),
        ("currency", "dollars", OrderFactKind::Currency),
        ("currency", "usd", OrderFactKind::Currency),
    ] {
        let error = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign(field, proposed),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .expect_err("an Order cannot be edited into an invalid record");
        assert!(
            matches!(
                &error,
                OrderEditPlanError::InvalidFactValue { fact, value }
                    if *fact == expected && value == proposed
            ),
            "{field}={proposed:?} is refused naming the fact, not as something else: {error:?}"
        );
        assert_eq!(
            expected.as_str(),
            field,
            "the fact a refusal names is the field a caller assigned"
        );
    }

    assert_eq!(
        fs::read(&order_path).unwrap(),
        before,
        "planning is side-effect free, so the record is untouched"
    );
    assert!(
        !order_path
            .with_file_name(".order.json.spielorders-stage")
            .exists(),
        "a refusal while planning never reaches staging"
    );
}

/// An Order whose record this edit is about to replace, and the plan authorized
/// against exactly the record now on disk.
fn order_ready_for_a_contended_edit(label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    let tree = initialized_package(label);
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    (tree, directory, order_path)
}

fn planned_title_edit(directory: &Path) -> spielorders_core::order_edits::OrderEditPlan {
    plan_order_edit(
        directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned")
}

/// A record rewritten between this edit's revalidation and its rename belongs to
/// whoever wrote it. The window the capability's own precondition check cannot see is
/// the one the destination expectation closes.
#[test]
fn an_order_record_given_a_different_meaning_after_revalidation_is_refused() {
    let (_tree, directory, order_path) =
        order_ready_for_a_contended_edit("order-edit-destination-meaning-changed");
    let before = fs::read(&order_path).unwrap();
    // The same Order, retitled by someone else: valid, parseable, different meaning.
    let theirs = String::from_utf8(before.clone())
        .unwrap()
        .replace("Laser components", "Retitled by someone else")
        .into_bytes();
    assert_ne!(theirs, before);

    let plan = planned_title_edit(&directory);
    let report = commit_order_edit(
        plan,
        &ConcurrentWriter::replacing(&order_path, &theirs, EditPoint::StagingWrite),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a record rewritten underneath this edit is refused: {report:?}");
    };
    // Refused because someone else changed it — distinct from a failed write, and
    // distinct from the precondition check that ran before any of this.
    let OrderEditRejection::ReplacementFailed { error } = rejection else {
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
        fs::read(&order_path).unwrap(),
        theirs,
        "the other writer's record survives byte for byte"
    );
    assert_eq!(
        OrderDocument::from_json_bytes(&fs::read(&order_path).unwrap())
            .unwrap()
            .title
            .as_str(),
        "Retitled by someone else",
        "their retitling survives, not this edit's"
    );
}

/// The cost of a semantic expectation, stated as a test rather than left to be
/// discovered: a record reformatted by someone else still means what this edit was
/// authorized against, so the edit proceeds and their formatting is overwritten. Under
/// "Canonical means loaded, not stored" that is not a loss of meaning.
#[test]
fn an_order_record_reformatted_after_revalidation_is_still_replaced() {
    let (_tree, directory, order_path) =
        order_ready_for_a_contended_edit("order-edit-destination-reformatted");
    let before = fs::read(&order_path).unwrap();
    // The same Order, re-emitted compactly: no indentation, no trailing newline.
    let value: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let theirs = serde_json::to_vec(&value).unwrap();
    assert_ne!(theirs, before, "their formatting differs from ours");
    assert_eq!(
        OrderDocument::from_json_bytes(&theirs).unwrap(),
        OrderDocument::from_json_bytes(&before).unwrap(),
        "their record still means exactly what ours did"
    );

    let plan = planned_title_edit(&directory);
    let other_writer = ConcurrentWriter::replacing(&order_path, &theirs, EditPoint::StagingWrite);
    let report = commit_order_edit(plan, &other_writer);

    assert!(
        other_writer.fired(),
        "the reformatting really did land under this edit"
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("a record that still means what was planned against is replaced: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    let persisted = fs::read(&order_path).unwrap();
    assert_eq!(
        persisted,
        result.order().document().to_json_bytes().unwrap(),
        "the record on disk is this edit's canonical bytes"
    );
    assert_ne!(
        persisted, theirs,
        "their formatting is what was given up, and it was given up"
    );
}

/// The destination expectation must ask the question the capability asked, and the
/// capability revalidates against the *projection* of the record — which trims the
/// single-line facts. A stored record carrying untrimmed text projects to exactly what
/// this edit was planned against, so an uncontended edit of it must still commit. A
/// raw byte-for-byte parse would refuse it, which would mean this application refusing
/// to edit a file a person had reformatted by hand.
#[test]
fn an_order_whose_stored_facts_are_untrimmed_still_commits_uncontended() {
    let (_tree, directory, order_path) =
        order_ready_for_a_contended_edit("order-edit-untrimmed-stored-facts");
    let stored = String::from_utf8(fs::read(&order_path).unwrap())
        .unwrap()
        .replace("\"Laser components\"", "\"  Laser components  \"");
    fs::write(&order_path, &stored).unwrap();
    assert_eq!(
        OrderDocument::from_json_bytes(stored.as_bytes())
            .unwrap()
            .title
            .as_str(),
        "  Laser components  ",
        "the stored record really does carry untrimmed text"
    );

    // No concurrent writer at all: nothing changed under this edit.
    let plan = planned_title_edit(&directory);
    let report = commit_order_edit(plan, &RealFileSystem);

    let MutationReport::Committed { result, .. } = report else {
        panic!("an untrimmed stored record is still this Order, and edits: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(
        fs::read(&order_path).unwrap(),
        result.order().document().to_json_bytes().unwrap(),
        "the edit landed as this application's canonical bytes"
    );
}

/// A record replaced with something that no longer parses is not a record this edit
/// was authorized against, so it is refused rather than overwritten. It is reported as
/// a changed destination, not an unreadable one: the read succeeded, the meaning went.
#[test]
fn an_order_record_made_unparseable_after_revalidation_is_refused() {
    let (_tree, directory, order_path) =
        order_ready_for_a_contended_edit("order-edit-destination-unparseable");
    const THEIRS: &[u8] = b"{ half a record";

    let plan = planned_title_edit(&directory);
    let report = commit_order_edit(
        plan,
        &ConcurrentWriter::replacing(&order_path, THEIRS, EditPoint::StagingWrite),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a record that stopped parsing underneath this edit is refused: {report:?}");
    };
    let OrderEditRejection::ReplacementFailed { error } = rejection else {
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
        fs::read(&order_path).unwrap(),
        THEIRS,
        "whatever they wrote survives for the repair capability to diagnose"
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrderBoundary {
    ReadRecord,
    WriteStage,
    SyncStage,
    Replace,
    IndexReplacement,
    RefreshListing,
}

const ORDER_STAGE: &str = ".spielorders/.order.json.spielorders-stage";

struct FailingOrder {
    boundary: OrderBoundary,
    fired: Cell<bool>,
    fail_cleanup: bool,
    replaced: Cell<bool>,
}

impl FailingOrder {
    fn at(boundary: OrderBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: false,
            replaced: Cell::new(false),
        }
    }

    fn with_cleanup_failure(boundary: OrderBoundary) -> Self {
        Self {
            boundary,
            fired: Cell::new(false),
            fail_cleanup: true,
            replaced: Cell::new(false),
        }
    }

    fn fail_once(&self, boundary: OrderBoundary) -> bool {
        self.boundary == boundary && !self.fired.replace(true)
    }

    fn error(boundary: OrderBoundary) -> io::Error {
        io::Error::other(format!("injected {boundary:?} failure"))
    }
}

impl FileSystem for FailingOrder {
    type Error = io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with(".spielorders/order.json") && self.fail_once(OrderBoundary::ReadRecord) {
            return Err(Self::error(OrderBoundary::ReadRecord));
        }
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_once(OrderBoundary::WriteStage) {
            return Err(Self::error(OrderBoundary::WriteStage));
        }
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_once(OrderBoundary::SyncStage) {
            return Err(Self::error(OrderBoundary::SyncStage));
        }
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        if destination.ends_with(".spielorders/order.json")
            && self.fail_once(OrderBoundary::Replace)
        {
            return Err(Self::error(OrderBoundary::Replace));
        }
        if destination.ends_with(".spielorders/order.json") {
            self.replaced.set(true);
        }
        if destination.ends_with(".spielorders/index.json")
            && self.fail_once(OrderBoundary::IndexReplacement)
        {
            return Err(Self::error(OrderBoundary::IndexReplacement));
        }
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        if path.ends_with(ORDER_STAGE) && self.fail_cleanup {
            return Err(io::Error::other("injected staging cleanup failure"));
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
        // Only the post-commit refresh load is failed, never the precommit checks.
        if self.replaced.get() && self.boundary == OrderBoundary::RefreshListing {
            return Err(Self::error(OrderBoundary::RefreshListing));
        }
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn injected_precommit_failures_preserve_the_prior_order_record_exactly() {
    for boundary in [
        OrderBoundary::WriteStage,
        OrderBoundary::SyncStage,
        OrderBoundary::Replace,
    ] {
        let (_tree, directory, _vendor) = fully_populated_order("order-edit-precommit-failure");
        let order_path = directory.join(".spielorders/order.json");
        let before = fs::read(&order_path).unwrap();
        let basename_before = directory.file_name().unwrap().to_owned();
        let plan = plan_order_edit(
            &directory,
            &OrderAssignments::new().assign("title", "Laser mounts"),
            &FixedCalendar::on(2026, 9, 1),
            &RealFileSystem,
        )
        .expect("the edit can be planned");

        let report = commit_order_edit(plan, &FailingOrder::at(boundary));

        let MutationReport::Rejected { rejection, .. } = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let OrderEditRejection::ReplacementFailed { error } = &rejection else {
            panic!("an injected {boundary:?} failure is a replacement failure: {rejection:?}");
        };
        assert!(
            matches!(
                (boundary, error.failure()),
                (
                    OrderBoundary::WriteStage,
                    ValidatedReplacementFailure::StagingWrite(_)
                ) | (
                    OrderBoundary::SyncStage,
                    ValidatedReplacementFailure::Durability(_)
                ) | (
                    OrderBoundary::Replace,
                    ValidatedReplacementFailure::Replacement(_)
                )
            ),
            "an injected {boundary:?} failure reports its own phase: {:?}",
            error.failure()
        );
        assert!(error.cleanup_failure().is_none());
        assert_eq!(
            fs::read(&order_path).unwrap(),
            before,
            "the prior Order record survives an injected {boundary:?} failure"
        );
        assert_eq!(directory.file_name().unwrap(), basename_before);
        assert!(!directory.join(ORDER_STAGE).exists());
    }
}

#[test]
fn a_staging_cleanup_failure_is_retained_beside_the_rejection() {
    let (_tree, directory, _vendor) = fully_populated_order("order-edit-cleanup-failure");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned");

    let report = commit_order_edit(
        plan,
        &FailingOrder::with_cleanup_failure(OrderBoundary::SyncStage),
    );

    let MutationReport::Rejected { rejection, .. } = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    let OrderEditRejection::ReplacementFailed { error } = &rejection else {
        panic!("a durability failure is a replacement failure: {rejection:?}");
    };
    assert!(error.cleanup_failure().is_some());
    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert!(directory.join(ORDER_STAGE).exists());
    fs::remove_file(directory.join(ORDER_STAGE)).unwrap();
}

#[test]
fn a_post_commit_index_refresh_failure_keeps_the_canonical_edit() {
    let tree = initialized_package("order-edit-index-failure");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned");

    let report = commit_order_edit(plan, &FailingOrder::at(OrderBoundary::IndexReplacement));

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(
        result.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    let [OrderEditWarning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
        panic!("the refresh failure is retained: {warnings:?}");
    };
    assert_eq!(
        failure.code(),
        OperationFailureCode::PackageIndexReplacementFailed
    );

    // The canonical edit stands, so re-running the same plan is not what a caller
    // should retry; the committed title is already the new one.
    let persisted = OrderDocument::from_json_bytes(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.as_str(), "Laser mounts");
}

#[test]
fn a_refresh_load_failure_keeps_the_edit_and_reports_a_stale_index() {
    let tree = initialized_package("order-edit-refresh-unavailable");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned");

    let report = commit_order_edit(plan, &FailingOrder::at(OrderBoundary::RefreshListing));

    let MutationReport::CommittedWithWarning {
        result, warnings, ..
    } = report
    else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(result.index().state(), PackageIndexState::Stale);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::NotAttempted);
    assert_eq!(
        result.index().stale_reasons(),
        [PackageIndexStaleReason::OrderMetadataChanged]
    );
    assert!(matches!(
        warnings.as_slice(),
        [OrderEditWarning::IndexRefreshFailed(_)]
    ));

    let persisted = OrderDocument::from_json_bytes(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.as_str(), "Laser mounts");
}

#[test]
fn order_edit_code_vocabularies_keep_their_stable_spellings() {
    for (source, spelling) in [
        (DateOrderedSource::Retained, "retained"),
        (DateOrderedSource::Assigned, "assigned"),
        (
            DateOrderedSource::AppliedOnEnteringOrdered,
            "applied_on_entering_ordered",
        ),
    ] {
        assert_eq!(source.as_str(), spelling);
    }
    assert_eq!(
        RejectedAssignmentReason::NotAReusableValue.as_str(),
        "not_a_reusable_value"
    );
}

#[test]
fn a_directory_that_is_not_an_order_of_its_package_is_rejected() {
    let tree = initialized_package("order-edit-not-an-order");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    // Discovery ignores hidden direct children, so a hidden directory carrying a
    // valid record is not an Order and must not be editable as one.
    let hidden = root.join("Orders/.staged_laser_components");
    fs::rename(&directory, &hidden).unwrap();

    let error = plan_order_edit(
        &hidden,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect_err("a hidden directory is not an editable Order");

    assert!(
        matches!(error, OrderEditPlanError::OrderNotFound { .. }),
        "a non-Order directory is reported as such: {error:?}"
    );
    let persisted =
        OrderDocument::from_json_bytes(&fs::read(hidden.join(".spielorders/order.json")).unwrap())
            .expect("the record parses");
    assert_eq!(persisted.title.as_str(), "Laser components");
}

#[test]
fn damaged_package_state_rejects_the_edit_without_touching_the_order() {
    let tree = initialized_package("order-edit-damaged-package");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    // An untrustworthy package-wide Vendor aggregate blocks ordinary mutation.
    fs::write(root.join(".spielorders/vendors.json"), b"{ not json").unwrap();

    let error = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect_err("a damaged Package rejects Order edits");

    assert!(
        matches!(error, OrderEditPlanError::DamagedPackage),
        "a damaged Package is reported as such: {error:?}"
    );
    assert_eq!(fs::read(&order_path).unwrap(), before);
}

#[test]
fn a_committed_report_without_a_refresh_carries_the_orders_real_readme() {
    let tree = initialized_package("order-edit-refresh-readme");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        &OrderCreationInput::new("Laser components"),
    );
    // An Order README is an exact user-authored document. The committed projection
    // is the complete refreshed Order, so it must carry that text even when the
    // derived-index refresh could not run and the projection is built locally.
    let readme = "# Laser components\n\nQuote pending from the vendor.\n";
    fs::write(directory.join("README.md"), readme).unwrap();
    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar::on(2026, 9, 1),
        &RealFileSystem,
    )
    .expect("the edit can be planned");

    let report = commit_order_edit(plan, &FailingOrder::at(OrderBoundary::RefreshListing));

    let MutationReport::CommittedWithWarning { result, .. } = report else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(
        result.order().readme_text(),
        Some(readme),
        "the committed projection reports the Order README rather than an empty one"
    );
    // An Order edit never writes the README it reports.
    assert_eq!(
        fs::read_to_string(directory.join("README.md")).unwrap(),
        readme
    );
}
