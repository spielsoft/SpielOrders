//! Decision sources: Define the Order record and lifecycle contract; Define Vendor
//! identity and historical semantics; Define the Rust CLI capability contract;
//! Define integrity, validation, and repair boundaries.

use crate::tests::support::assert_reported_beside;
use crate::tests::support::committed_order;
use crate::tests::support::index_is_current;
use crate::tests::support::{
    EditPoint, FailedCommit, FixedCalendar, FixedUuid, Index, ManagedFileBoundary,
    PlatformFileSystem, TemporaryTree, adoption, another_writer_replacing, canonical_bytes,
    created_order, created_vendor, edit_order, effects, failing_replacement, json, operations,
    order_at, package_tree, reusable, with_stage_cleanup_failure,
};
use crate::tests::support::{planned_order, planned_order_vendor, planned_reuse, reported};
use crate::{Assignments, Change, OrderRef, Plan, Report};
use std::ops::Deref;

use crate::Coded;

use crate::mutation::RejectedAssignmentReason;
use crate::order_edits::OrderFactKind;
use crate::{Failure, Warning};
use crate::{metadata::OrderDocument, metadata::OrderStatus};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn an_editor_original_is_enforced_by_the_core_commit() {
    let tree = package_tree("order-editor-original");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Original"),
    );
    let original = order_at(&directory, &PlatformFileSystem)
        .result
        .unwrap()
        .order
        .document
        .clone();
    let calendar = FixedCalendar::on(2026, 11, 4);
    let external = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "External"),
        ),
        &effects(&PlatformFileSystem).dated(&calendar),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(external, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let stale = crate::plan(
        &Change::SaveOrder {
            order: OrderRef::Directory(directory.clone()),
            assignments: Assignments::new().assign("title", "Stale"),
            items: Vec::new(),
            original,
        },
        &effects(&PlatformFileSystem).dated(&calendar),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(stale, &effects(&PlatformFileSystem)).result,
        Err(Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(
        order_at(&directory, &PlatformFileSystem)
            .result
            .unwrap()
            .order
            .document
            .title
            .deref(),
        "External"
    );
}

#[test]
fn title_edit_plan_reports_complete_facts_and_writes_nothing() {
    let tree = package_tree("order-edit-title-plan");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("orderer", "Ada Lovelace"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let index_path = root.join(".spielorders/index.json");
    let order_before = fs::read(&order_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "  Laser mounts  "),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("an existing Order's title can be edited");

    assert_eq!(
        json(planned_order(&plan))["order_id"],
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "an edit retains the immutable Order ID"
    );
    assert_eq!(json(planned_order(&plan))["title"], "Laser mounts");
    assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-08-22");
    assert_eq!(
        json(planned_order(&plan))["status"],
        json(&OrderStatus::InPreparation),
    );
    assert_eq!(
        json(planned_order(&plan))["orderer"],
        json(&Some("Ada Lovelace"))
    );
    assert_eq!(
        json(planned_order(&plan))["vendor_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(planned_order(&plan))["currency"],
        serde_json::Value::Null
    );
    assert_eq!(
        planned_reuse(&plan, "orderer"),
        reusable("reused", "Ada Lovelace")
    );
    assert_eq!(
        operations(&plan),
        [
            "replace_order_record Orders/2026_08_22_Laser_components_Ada_Lovelace/.spielorders/order.json",
            "refresh_package_index .spielorders/index.json",
        ]
    );
    assert_eq!(fs::read(&order_path).unwrap(), order_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

#[test]
fn commit_persists_facts_refreshes_index_and_retains_the_drifted_directory_name() {
    let tree = package_tree("order-edit-commit");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let basename_before = directory.file_name().unwrap().to_owned();

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new()
                .assign("title", "Laser mounts")
                .assign("date_ordered", "2026-07-19")
                .assign("currency", "USD"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the Order can be edited");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid Order edit commits: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser mounts"
    );
    assert_eq!(
        committed_order(&result).document.date_ordered.deref(),
        "2026-07-19"
    );
    assert_eq!(
        committed_order(&result).document.currency.as_deref(),
        Some("USD")
    );
    assert_eq!(
        committed_order(&result).document.order_id.deref(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(committed_order(&result).order_total, 0.0);
    assert!(index_is_current(root));

    // Directory drift is ordinary allowed state; only explicit alignment renames.
    assert!(directory.is_dir());
    assert_eq!(directory.file_name().unwrap(), basename_before);
    assert_eq!(
        basename_before,
        std::ffi::OsStr::new("2026_08_22_Laser_components")
    );

    let persisted = serde_json::from_slice::<OrderDocument>(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.deref(), "Laser mounts");
    assert_eq!(json(&persisted.package_root), "../..");
    let indexed = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the refreshed derived index parses");
    assert_eq!(indexed.orders.len(), 1);
    assert_eq!(indexed.orders[0].order.title.deref(), "Laser mounts");
    assert_eq!(
        indexed.orders[0].order_directory.as_str(),
        "Orders/2026_08_22_Laser_components"
    );
}

fn committed_edit(
    directory: &Path,
    assignments: Assignments,
    calendar: &FixedCalendar,
) -> crate::canonical::HealthyOrder {
    let plan = crate::plan(
        &edit_order(PathBuf::from(directory), assignments.clone()),
        &effects(&PlatformFileSystem).dated(calendar),
    )
    .result
    .expect("the edit can be planned");
    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;
    let Ok(Report::Committed { result, .. }) = report else {
        panic!("the edit commits: {report:?}");
    };
    committed_order(&result)
}

/// An Order already at `ordered`, whose Date Ordered is deliberately not the
/// calendar date used by the edits under test.
fn ordered_order(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = package_tree(label);
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let result = committed_edit(
        &directory,
        Assignments::new()
            .assign("status", "ordered")
            .assign("date_ordered", "2026-08-22"),
        &FixedCalendar::on(2026, 8, 22),
    );
    assert_eq!(result.document.status, OrderStatus::Ordered);
    assert_eq!(result.document.date_ordered.deref(), "2026-08-22");
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
        let tree = package_tree(&format!("order-edit-entering-ordered-{label}"));
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Laser components"),
        );
        if let Some(prior_spelling) = prior_spelling {
            let staged = committed_edit(
                &directory,
                Assignments::new().assign("status", prior_spelling),
                &FixedCalendar::on(2026, 8, 22),
            );
            assert_eq!(staged.document.status, prior);
            assert_eq!(
                staged.document.date_ordered.deref(),
                "2026-08-22",
                "reaching {label} leaves the archival date alone"
            );
        }

        let plan = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().assign("status", "ordered"),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .expect("entering ordered can be planned");

        assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-09-01");
        assert_ne!(json(planned_order(&plan))["date_ordered"], "2026-08-22");
        let result = committed_edit(
            &directory,
            Assignments::new().assign("status", "ordered"),
            &FixedCalendar::on(2026, 9, 1),
        );
        assert_eq!(
            result.document.date_ordered.deref(),
            "2026-09-01",
            "entering ordered from {label} commits the entry date"
        );
        assert_eq!(result.document.status, OrderStatus::Ordered);
    }
}

#[test]
fn an_order_already_ordered_keeps_its_date_when_no_date_is_assigned() {
    let (_tree, directory) = ordered_order("order-edit-already-ordered");

    // Re-asserting the same status is not entry, and neither is editing another
    // fact, so a later calendar date must not silently overwrite the archival date.
    for assignments in [
        Assignments::new().assign("status", "ordered"),
        Assignments::new().assign("title", "Laser mounts"),
        Assignments::new()
            .assign("status", "ordered")
            .assign("title", "Laser adapters"),
    ] {
        let plan = crate::plan(
            &edit_order(directory.clone(), assignments.clone()),
            &effects(&PlatformFileSystem).on(2026, 12, 25),
        )
        .result
        .expect("the edit can be planned");

        assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-08-22");
        assert_eq!(
            json(planned_order(&plan))["status"],
            json(&OrderStatus::Ordered)
        );
    }
}

#[test]
fn leaving_ordered_for_another_status_keeps_the_date() {
    let (_tree, directory) = ordered_order("order-edit-leaving-ordered");

    for status in ["received", "complete", "hold", "in_preparation"] {
        let plan = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().assign("status", status),
            ),
            &effects(&PlatformFileSystem).on(2026, 12, 25),
        )
        .result
        .expect("every direct transition can be planned");

        assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-08-22");
    }
}

#[test]
fn an_explicit_same_mutation_date_wins_regardless_of_assignment_order() {
    for assignments in [
        Assignments::new()
            .assign("status", "ordered")
            .assign("date_ordered", "2026-07-19"),
        Assignments::new()
            .assign("date_ordered", "2026-07-19")
            .assign("status", "ordered"),
    ] {
        let tree = package_tree("order-edit-explicit-date");
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Laser components"),
        );

        let plan = crate::plan(
            &edit_order(directory.clone(), assignments.clone()),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .expect("an explicit historical date can be planned");

        assert_eq!(json(planned_order(&plan))["date_ordered"], "2026-07-19");
        assert_ne!(json(planned_order(&plan))["date_ordered"], "2026-09-01");
        assert_eq!(
            json(planned_order(&plan))["status"],
            json(&OrderStatus::Ordered)
        );
    }
}

#[test]
fn every_direct_status_transition_commits() {
    let tree = package_tree("order-edit-transitions");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
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
            Assignments::new().assign("status", spelling),
            &calendar,
        );
        assert_eq!(result.document.status, status);
    }
}

/// Every rejected assignment set below also carries a valid change that would
/// otherwise alter canonical bytes, so byte-identity proves atomicity rather than
/// merely proving the invalid field was refused.
#[test]
fn one_rejected_assignment_leaves_the_whole_order_untouched() {
    let tree = package_tree("order-edit-atomic");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("orderer", "Ada Lovelace"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let index_path = root.join(".spielorders/index.json");
    let order_before = fs::read(&order_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();
    let basename_before = directory.file_name().unwrap().to_owned();
    let valid = ("title", "Laser mounts");

    let cases: Vec<(&str, Assignments)> = vec![
        (
            "unknown field",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("received_on", "2026-09-01"),
        ),
        (
            "immutable identity",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("order_id", "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f"),
        ),
        (
            "managed technical locator",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("package_root", "../../.."),
        ),
        (
            "managed technical version",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("schema_version", "2"),
        ),
        (
            "ordered collection",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("items", "[]"),
        ),
        (
            "duplicate assignment",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("currency", "USD")
                .assign("currency", "EUR"),
        ),
        (
            "introduce on a non-reusable field",
            Assignments::new()
                .assign("currency", "USD")
                .introduce(valid.0, valid.1),
        ),
        (
            "blank required title",
            Assignments::new()
                .assign("currency", "USD")
                .assign("title", "   "),
        ),
        (
            "blank required status",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("status", ""),
        ),
        (
            "invalid calendar date",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("date_ordered", "2026-02-30"),
        ),
        (
            "unknown status",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("status", "shipped"),
        ),
        (
            "malformed currency",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("currency", "usd"),
        ),
        (
            "unresolved Vendor",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("vendor", "Nonesuch Optics"),
        ),
        (
            "unconfirmed new reusable value",
            Assignments::new()
                .assign(valid.0, valid.1)
                .assign("project", "Telescope"),
        ),
    ];

    for (label, assignments) in cases {
        let error = crate::plan(
            &edit_order(directory.clone(), assignments.clone()),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .map_err(crate::tests::support::first_fault)
        .err()
        .unwrap_or_else(|| panic!("{label} is rejected"));
        assert!(
            !matches!(
                error,
                Failure::PackageAccessDenied(_) | Failure::PackageOperationFailed(_)
            ),
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
    let tree = package_tree("order-edit-assignment-reasons");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );

    let error = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new()
                .assign("order_id", "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f")
                .assign("schema_version", "2")
                .assign("package_root", "../..")
                .assign("items", "[]")
                .assign("received_on", "2026-09-01")
                .assign("currency", "USD")
                .assign("currency", "EUR")
                .introduce("title", "Laser mounts")
                .assign("status", " "),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("every invalid assignment is rejected at once");

    let Failure::InvalidAssignments(rejected) = &error else {
        panic!("invalid assignments are reported as such: {error:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field.as_str(), entry.reason))
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
    let tree = package_tree(label);
    let root = tree.path();
    let vendor = created_vendor(root, "Tarnwick", "e0990d31-06aa-42e9-97c8-088e3e894822");
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).unwrap();
    let plan = crate::plan(
        &adoption(
            PathBuf::from(root),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold")
                    .assign("vendor", "Tarnwick")
                    .assign("orderer", "Ada Lovelace")
                    .assign("ordering_institution", "Northwind")
                    .assign("currency", "USD")
                    .assign("project", "Telescope")
                    .assign("order_group", "Summer build"),
                vec![
                    Assignments::new()
                        .assign("description", "Mirror mount")
                        .assign("quantity", "2.0")
                        .assign("unit_price", "84.5")
                        .assign("catalog_number", "TM-100"),
                    Assignments::new()
                        .assign("description", "Lens")
                        .assign("quantity", "3.0")
                        .assign("unit_price", "10.0"),
                ],
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(FixedUuid("8b6e3f0a-8ec5-4d47-95e1-412652a56d16".into())),
    )
    .result;
    assert!(matches!(report, Ok(Report::Committed { .. })));
    (tree, directory, vendor)
}

#[test]
fn every_optional_scalar_clears_explicitly_without_disturbing_the_rest() {
    let (_tree, directory, _vendor) = fully_populated_order("order-edit-clearing");

    for field in [
        "vendor",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
    ] {
        let plan = crate::plan(
            &edit_order(directory.clone(), Assignments::new().assign(field, "")),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .unwrap_or_else(|error| panic!("clearing {field} can be planned: {error:?}"));

        let facts = json(planned_order(&plan)).clone();
        let cleared = if field == "vendor" {
            "vendor_id"
        } else {
            field
        };
        assert!(facts[cleared].is_null(), "{field} is cleared");
        // Required facts, unnamed optional facts, and the ordered Items all survive.
        assert_eq!(facts["title"], "Laser components");
        assert_eq!(facts["date_ordered"], "2026-08-22");
        assert_eq!(facts["status"], json(&OrderStatus::Hold));
        for other in [
            "vendor_id",
            "orderer",
            "ordering_institution",
            "currency",
            "project",
            "order_group",
        ] {
            if other != cleared {
                assert!(
                    !facts[other].is_null(),
                    "clearing {field} must not also clear {other}"
                );
            }
        }
    }

    // Clearing every optional scalar in one atomic set leaves only required facts.
    let result = committed_edit(
        &directory,
        Assignments::new()
            .assign("vendor", "")
            .assign("orderer", "")
            .assign("ordering_institution", "")
            .assign("currency", "")
            .assign("project", "")
            .assign("order_group", ""),
        &FixedCalendar::on(2026, 9, 1),
    );
    let document = result.document;
    assert_eq!(document.vendor_id, None);
    assert_eq!(document.orderer, None);
    assert_eq!(document.ordering_institution, None);
    assert_eq!(document.currency, None);
    assert_eq!(document.project, None);
    assert_eq!(document.order_group, None);
    assert_eq!(document.title.deref(), "Laser components");
    assert_eq!(document.items.len(), 2);
    assert_eq!(result.order_total, 199.0);
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
        Assignments::new().assign("title", "Optical table"),
    );
    (tree, second)
}

#[test]
fn a_case_variant_reusable_value_reuses_the_existing_canonical_spelling() {
    let (_tree, directory) = order_beside_populated_order("order-edit-reusable-reuse");

    for (field, canonical, variant) in REUSABLE {
        let plan = crate::plan(
            &edit_order(directory.clone(), Assignments::new().assign(field, variant)),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .unwrap_or_else(|error| panic!("reusing {field} can be planned: {error:?}"));

        assert_eq!(
            planned_reuse(&plan, field),
            reusable("reused", canonical),
            "{field} reports reuse rather than introduction"
        );
        assert_eq!(
            json(planned_order(&plan))[field],
            canonical,
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
        let error = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new()
                    .assign("title", "Optical bench")
                    .assign(field, "  Weather station  "),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .map_err(crate::tests::support::first_fault)
        .err()
        .unwrap_or_else(|| panic!("an unconfirmed new {field} is rejected"));

        let Failure::UnconfirmedReusableValue {
            field: named,
            proposed,
        } = &error
        else {
            panic!("an unconfirmed new {field} is reported as such: {error:?}");
        };
        assert_eq!(named, field);
        assert_eq!(proposed, "Weather station");
        assert_eq!(fs::read(&order_path).unwrap(), before);

        let plan = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new()
                    .assign("title", "Optical bench")
                    .introduce(field, "  Weather station  "),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .unwrap_or_else(|error| panic!("introducing {field} can be planned: {error:?}"));

        assert_eq!(
            planned_reuse(&plan, field),
            reusable("new", "Weather station"),
            "{field} reports introduction rather than reuse"
        );
        assert_eq!(json(planned_order(&plan))[field], "Weather station");
    }
}

#[test]
fn introduction_intent_still_reuses_an_existing_canonical_spelling() {
    let (_tree, directory) = order_beside_populated_order("order-edit-reusable-introduce-match");

    for (field, canonical, variant) in REUSABLE {
        let plan = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().introduce(field, variant),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .unwrap_or_else(|error| panic!("introducing a matching {field} can be planned: {error:?}"));

        // Introduction intent permits creating a value; it never duplicates one.
        assert_eq!(planned_reuse(&plan, field), reusable("reused", canonical));
        assert_eq!(json(planned_order(&plan))[field], canonical);
    }
}

/// **The Vendor an edit leaves is resolved by the plan and by the result**, because the
/// projection a mutation answers with resolves one and an edit is what can change which
/// Vendor that is. A run asked to plan only has no committed record to resolve it from, so
/// the plan carries it; a run that committed reports the Vendor the refreshed load found.
#[test]
fn a_live_vendor_reference_can_be_assigned_and_cleared() {
    let tree = package_tree("order-edit-vendor");
    let root = tree.path();
    let vendor = created_vendor(root, "Tarnwick", "e0990d31-06aa-42e9-97c8-088e3e894822");
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("vendor", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("a live Vendor can be referenced");
    assert_eq!(
        json(&planned_order_vendor(&plan))["name"],
        "Tarnwick",
        "the Vendor the plan would leave is resolved against what it would store"
    );

    let assigned = committed_edit(
        &directory,
        Assignments::new().assign("vendor", "Tarnwick"),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(
        order_at(&directory, &PlatformFileSystem)
            .result
            .expect("the edited Order reads")
            .vendor
            .as_ref()
            .map(|vendor| vendor.name.deref()),
        Some("Tarnwick"),
        "and the committed Order resolves it too"
    );
    assert_eq!(
        assigned.document.vendor_id.as_deref(),
        Some(vendor.as_str())
    );

    let cleared = committed_edit(
        &directory,
        Assignments::new().assign("vendor", ""),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(cleared.document.vendor_id, None);
    assert!(
        order_at(&directory, &PlatformFileSystem)
            .result
            .expect("the edited Order reads")
            .vendor
            .is_none(),
        "an Order that references no Vendor resolves none"
    );
}

#[test]
fn a_no_op_assignment_set_reports_unchanged_without_replacing_canonical_bytes() {
    let (_tree, directory, _vendor) = fully_populated_order("order-edit-no-op");
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    let modified_before = fs::metadata(&order_path).unwrap().modified().unwrap();

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new()
                .assign("title", "  Laser components  ")
                .assign("status", "hold")
                .assign("project", "teleSCOPE"),
        ),
        &effects(&PlatformFileSystem).on(2026, 12, 25),
    )
    .result
    .expect("a no-op assignment set is a valid plan");

    assert!(
        operations(&plan).is_empty(),
        "a plan that changes nothing announces no managed operation"
    );

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Unchanged { result, .. }) = report else {
        panic!("a no-op assignment set reports unchanged: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser components"
    );
    assert_eq!(committed_order(&result).order_total, 199.0);
    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&order_path).unwrap().modified().unwrap(),
        modified_before
    );
}

#[test]
fn a_damaged_order_is_rejected_without_touching_its_record() {
    let tree = package_tree("order-edit-damaged");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    // A parseable record whose Date Ordered is not a real calendar date damages
    // only this Order, so the edit must refuse rather than repair it.
    let damaged = fs::read_to_string(&order_path)
        .unwrap()
        .replace("2026-08-22", "2026-02-30");
    fs::write(&order_path, &damaged).unwrap();

    let error = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Order cannot be edited");

    let Failure::DamagedOrder {
        paths, diagnostics, ..
    } = &error
    else {
        panic!("a damaged Order is reported as such: {error:?}");
    };
    assert!(
        paths
            .directory
            .absolute_path
            .ends_with(directory.file_name().unwrap())
    );
    assert!(
        !diagnostics.is_empty(),
        "with the diagnosis that quarantined it"
    );
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
        let tree = package_tree("order-edit-unusable-record");
        let directory = created_order(
            tree.path(),
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Laser components"),
        );
        let order_path = directory.join(".spielorders/order.json");
        match &corrupt {
            Some(bytes) => fs::write(&order_path, bytes).unwrap(),
            None => fs::remove_file(&order_path).unwrap(),
        }

        let error = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().assign("title", "Laser mounts"),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .map_err(crate::tests::support::first_fault)
        .err()
        .unwrap_or_else(|| panic!("{label} is rejected"));

        assert!(
            matches!(
                error,
                Failure::DamagedRecord { .. }
                    | Failure::DamagedOrder { .. }
                    | Failure::NotAnOrderDirectory { .. }
            ),
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
    let tree = package_tree("order-edit-unreadable-record");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();

    let error = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&failing_replacement(
            ".spielorders/order.json",
            ManagedFileBoundary::ReadRecord,
        ))
        .on(2026, 9, 1),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("an unreadable Order record is rejected");

    assert!(
        matches!(
            error,
            Failure::DamagedRecord { .. } | Failure::DamagedOrder { .. }
        ),
        "an unreadable record is one Damaged Order: {error:?}"
    );
    assert_eq!(fs::read(&order_path).unwrap(), before);
}

#[test]
fn a_concurrent_order_record_change_rejects_before_any_write() {
    let tree = package_tree("order-edit-concurrent");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");
    let intervening = committed_edit(
        &directory,
        Assignments::new().assign("currency", "USD"),
        &FixedCalendar::on(2026, 9, 1),
    );
    assert_eq!(intervening.document.currency.as_deref(), Some("USD"));
    let before = fs::read(&order_path).unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Err(rejection) = report else {
        panic!("a concurrent record change rejects: {report:?}");
    };
    let Failure::PreconditionsChanged { location } = &rejection else {
        panic!("a concurrent record change is a precondition change: {rejection:?}");
    };
    assert!(location.absolute_path.ends_with(".spielorders/order.json"));
    assert!(
        location
            .package_relative_path
            .as_ref()
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
/// built from facts that parsed and validated, so nothing reaches a commit holding facts
/// that were never validated.
///
/// **Each of the three facts whose value the record constrains is refused by name.** A
/// Date Ordered with the shape of a date but no such day parses and then fails calendar
/// validation, and one with no date shape at all fails to parse: two different internal
/// paths, one answer, because a caller correcting an assignment needs to be told which
/// assignment. A refusal that said only "these facts are invalid" is what an atomic set
/// makes least useful.
#[test]
fn a_value_the_record_cannot_hold_is_refused_by_name_while_planning_and_writes_nothing() {
    let tree = package_tree("order-edit-invalid-facts-refused-at-plan");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
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
        let error = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().assign(field, proposed),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .map_err(crate::tests::support::first_fault)
        .expect_err("an Order cannot be edited into an invalid record");
        assert!(
            matches!(
                &error,
                Failure::InvalidFactValue { fact, value }
                    if *fact == expected && value == proposed
            ),
            "{field}={proposed:?} is refused naming the fact, not as something else: {error:?}"
        );
        assert_eq!(
            expected.code(),
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
    let tree = package_tree(label);
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    (tree, directory, order_path)
}

fn planned_title_edit(directory: &Path) -> Plan {
    crate::plan(
        &edit_order(
            PathBuf::from(directory),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
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
    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &order_path,
            &theirs,
            EditPoint::StagingWrite,
        )),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a record rewritten underneath this edit is refused: {report:?}");
    };
    // Refused because someone else changed it — distinct from a failed write, and
    // distinct from the precondition check that ran before any of this.
    let error = rejection;
    assert!(
        error.cleanup_failures().is_empty(),
        "the staged candidate is cleaned up: {:?}",
        error.cleanup_failures()
    );
    assert_eq!(
        fs::read(&order_path).unwrap(),
        theirs,
        "the other writer's record survives byte for byte"
    );
    assert_eq!(
        serde_json::from_slice::<OrderDocument>(&fs::read(&order_path).unwrap())
            .unwrap()
            .title
            .deref(),
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
        serde_json::from_slice::<OrderDocument>(&theirs).unwrap(),
        serde_json::from_slice::<OrderDocument>(&before).unwrap(),
        "their record still means exactly what ours did"
    );

    let plan = planned_title_edit(&directory);
    let other_writer = another_writer_replacing(&order_path, &theirs, EditPoint::StagingWrite);
    let report = crate::commit(plan, &effects(&other_writer)).result;

    assert!(
        other_writer.disturbed(),
        "the reformatting really did land under this edit"
    );
    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a record that still means what was planned against is replaced: {report:?}");
    };
    let result = committed_order(&result);
    assert_eq!(result.document.title.deref(), "Laser mounts");
    let persisted = fs::read(&order_path).unwrap();
    assert_eq!(
        persisted,
        canonical_bytes(&result.document),
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
        serde_json::from_slice::<OrderDocument>(stored.as_bytes())
            .unwrap()
            .title
            .deref(),
        "  Laser components  ",
        "the stored record really does carry untrimmed text"
    );

    // No concurrent writer at all: nothing changed under this edit.
    let plan = planned_title_edit(&directory);
    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("an untrimmed stored record is still this Order, and edits: {report:?}");
    };
    let result = committed_order(&result);
    assert_eq!(result.document.title.deref(), "Laser mounts");
    assert_eq!(
        fs::read(&order_path).unwrap(),
        canonical_bytes(&result.document),
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
    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &order_path,
            THEIRS,
            EditPoint::StagingWrite,
        )),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a record that stopped parsing underneath this edit is refused: {report:?}");
    };
    let _error = rejection;
    assert_eq!(
        fs::read(&order_path).unwrap(),
        THEIRS,
        "whatever they wrote survives for the repair capability to diagnose"
    );
}

const ORDER_STAGE: &str = ".spielorders/.order.json.spielorders-stage";

#[test]
fn injected_precommit_failures_preserve_the_prior_order_record_exactly() {
    for boundary in [
        ManagedFileBoundary::WriteStage,
        ManagedFileBoundary::SyncStage,
        ManagedFileBoundary::Replace,
    ] {
        let (_tree, directory, _vendor) = fully_populated_order("order-edit-precommit-failure");
        let order_path = directory.join(".spielorders/order.json");
        let before = fs::read(&order_path).unwrap();
        let basename_before = directory.file_name().unwrap().to_owned();
        let plan = crate::plan(
            &edit_order(
                directory.clone(),
                Assignments::new().assign("title", "Laser mounts"),
            ),
            &effects(&PlatformFileSystem).on(2026, 9, 1),
        )
        .result
        .expect("the edit can be planned");

        let report = crate::commit(
            plan,
            &effects(&failing_replacement(".spielorders/order.json", boundary)),
        )
        .result;

        let Err(rejection) = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let error = &rejection;
        assert_reported_beside(error);
        assert!(error.cleanup_failures().is_empty());
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
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");

    let report = crate::commit(
        plan,
        &effects(&with_stage_cleanup_failure(
            ".spielorders/order.json",
            ManagedFileBoundary::SyncStage,
        )),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    // The interface reports the stopped commit at the record it was replacing, and the
    // stage it could not remove as its own entry beside it.
    assert_eq!(
        reported(&rejection),
        [
            (
                crate::SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE,
                String::from("Orders/Legacy_laser_purchase/.spielorders/order.json"),
            ),
            (
                crate::SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE,
                String::from(
                    "Orders/Legacy_laser_purchase/.spielorders/.order.json.spielorders-stage"
                ),
            ),
        ]
    );
    assert_eq!(fs::read(&order_path).unwrap(), before);
    assert!(directory.join(ORDER_STAGE).exists());
    fs::remove_file(directory.join(ORDER_STAGE)).unwrap();
}

#[test]
fn a_post_commit_index_refresh_failure_keeps_the_canonical_edit() {
    let tree = package_tree("order-edit-index-failure");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");

    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            ".spielorders/order.json",
            ManagedFileBoundary::IndexReplacement,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser mounts"
    );
    let [Warning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
        panic!("the refresh failure is retained: {warnings:?}");
    };
    assert!(matches!(failure, Failure::IndexReplacementFailed(_)));

    // The canonical edit stands, so re-running the same plan is not what a caller
    // should retry; the committed title is already the new one.
    let persisted = serde_json::from_slice::<OrderDocument>(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.deref(), "Laser mounts");
}

#[test]
fn a_refresh_load_failure_keeps_the_edit_and_reports_a_stale_index() {
    let tree = package_tree("order-edit-refresh-unavailable");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");

    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            ".spielorders/order.json",
            ManagedFileBoundary::RefreshListing,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser mounts"
    );
    assert!(matches!(
        warnings.as_slice(),
        [Warning::IndexRefreshFailed(_)]
    ));

    let persisted = serde_json::from_slice::<OrderDocument>(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the canonical Order parses");
    assert_eq!(persisted.title.deref(), "Laser mounts");
}

#[test]
fn order_edit_code_vocabularies_keep_their_stable_spellings() {
    assert_eq!(
        RejectedAssignmentReason::NotAReusableValue.code(),
        "not_a_reusable_value"
    );
}

#[test]
fn a_directory_that_is_not_an_order_of_its_package_is_rejected() {
    let tree = package_tree("order-edit-not-an-order");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    // Discovery ignores hidden direct children, so a hidden directory carrying a
    // valid record is not an Order and must not be editable as one.
    let hidden = root.join("Orders/.staged_laser_components");
    fs::rename(&directory, &hidden).unwrap();

    let error = crate::plan(
        &edit_order(
            hidden.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a hidden directory is not an editable Order");

    assert!(
        matches!(
            error,
            Failure::LocatorReachesNoPackageHoldingThisOrder { .. }
        ),
        "a non-Order directory is reported as such: {error:?}"
    );
    let persisted = serde_json::from_slice::<OrderDocument>(
        &fs::read(hidden.join(".spielorders/order.json")).unwrap(),
    )
    .expect("the record parses");
    assert_eq!(persisted.title.deref(), "Laser components");
}

#[test]
fn damaged_package_state_rejects_the_edit_without_touching_the_order() {
    let tree = package_tree("order-edit-damaged-package");
    let root = tree.path();
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    let order_path = directory.join(".spielorders/order.json");
    let before = fs::read(&order_path).unwrap();
    // An untrustworthy package-wide Vendor aggregate blocks ordinary mutation.
    fs::write(root.join(".spielorders/vendors.json"), b"{ not json").unwrap();

    let error = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Package rejects Order edits");

    assert!(
        matches!(error, Failure::DamagedPackage(_)),
        "a damaged Package is reported as such: {error:?}"
    );
    assert_eq!(fs::read(&order_path).unwrap(), before);
}

#[test]
fn a_committed_report_without_a_refresh_carries_the_orders_real_readme() {
    let tree = package_tree("order-edit-refresh-readme");
    let directory = created_order(
        tree.path(),
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
    );
    // An Order README is an exact user-authored document. The committed projection
    // is the complete refreshed Order, so it must carry that text even when the
    // derived-index refresh could not run and the projection is built locally.
    let readme = "# Laser components\n\nQuote pending from the vendor.\n";
    fs::write(directory.join("README.md"), readme).unwrap();
    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 9, 1),
    )
    .result
    .expect("the edit can be planned");

    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            ".spielorders/order.json",
            ManagedFileBoundary::RefreshListing,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning { result, .. }) = report else {
        panic!("a failed refresh load remains committed: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser mounts"
    );
    assert_eq!(
        committed_order(&result).readme_text.as_deref(),
        Some(readme),
        "the committed projection reports the Order README rather than an empty one"
    );
    // An Order edit never writes the README it reports.
    assert_eq!(
        fs::read_to_string(directory.join("README.md")).unwrap(),
        readme
    );
}
