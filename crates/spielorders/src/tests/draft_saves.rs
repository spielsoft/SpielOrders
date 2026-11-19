//! Decision sources: Prototype the SpielOrders desktop experience; Define the Rust CLI
//! capability contract; Effect and adapter boundaries; Durable managed-file writes.
//!
//! An editor's Save is one change request for its whole draft: a record's facts and its
//! complete ordered Item or Contact list, validated as a whole and committed as one
//! guarded rewrite of that one record.

use crate::tests::support::{
    PlatformFileSystem, add_contact, add_item, committed, created_order, created_vendor,
    edit_order, edit_vendor, effects, initialized_package, json, operation_kinds, order_at,
    planned, save_order, save_vendor, snapshot, vendor_named,
};
use crate::{Assignments, ChangeResult, Failure, Report, VendorRef, commit, plan};
use crate::{
    answer::DraftFailure, metadata::Vendor, mutation::RejectedAssignment,
    order_facts::OrderFactKind,
};
use crate::{metadata::OrderDocument, metadata::OrderStatus, mutation::RejectedAssignmentReason};
use std::ops::Deref;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const TARNWICK: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";
const NETHERMOOR: &str = "6b0d9e77-2a41-4c8e-9f10-0d3b5a7c2e18";

/// One Item row as an editor drafts it.
fn item(description: &str, quantity: &str, unit_price: &str, catalog_number: &str) -> Assignments {
    Assignments::new()
        .assign("description", description)
        .assign("quantity", quantity)
        .assign("unit_price", unit_price)
        .assign("catalog_number", catalog_number)
}

/// One Contact row as an editor drafts it.
fn contact(name: &str, role: &str) -> Assignments {
    Assignments::new().assign("name", name).assign("role", role)
}

/// A Package with two Vendors, Tarnwick carrying three Contacts, and one Order
/// referencing Tarnwick and carrying three Items.
fn package(label: &str) -> (crate::tests::support::TemporaryTree, PathBuf, PathBuf) {
    let (tree, root) = initialized_package(label);
    created_vendor(&root, "Tarnwick", TARNWICK);
    created_vendor(&root, "Nethermoor", NETHERMOOR);
    for name in ["Grace Hopper", "Alan Turing", "Katherine Johnson"] {
        committed(
            &add_contact(
                root.clone(),
                VendorRef::Id(TARNWICK.into()),
                contact(name, "Sales"),
            ),
            &effects(&PlatformFileSystem),
        );
    }
    let directory = created_order(
        &root,
        ORDER_ID,
        Assignments::new()
            .assign("title", "Laser components")
            .assign("vendor", "Tarnwick"),
    );
    for row in [
        item("Mirror mount", "2", "84.5", "TM-100"),
        item("Post", "5", "6.75", "TR2"),
        item("Lens", "3", "12.25", "LA1131"),
    ] {
        committed(
            &add_item(directory.clone(), row),
            &effects(&PlatformFileSystem),
        );
    }
    (tree, root, directory)
}

fn stored_order(directory: &Path) -> OrderDocument {
    order_at(directory, &PlatformFileSystem)
        .result
        .expect("the Order reads")
        .order
        .document
}

fn stored_vendor(root: &Path, name: &str) -> Vendor {
    vendor_named(root, name, &PlatformFileSystem)
        .result
        .expect("the Vendor reads")
        .vendor
}

#[test]
fn one_save_commits_an_orders_facts_and_every_item_change_at_once() {
    let (_tree, _root, directory) = package("draft-save-order");
    let original = stored_order(&directory);
    let effects = effects(&PlatformFileSystem).on(2031, 1, 15);

    // The Post is removed, the Lens moves first, the Mirror mount's Quantity changes, and
    // an Iris is added; the title, Vendor, and Status change beside them.
    let save = save_order(
        &directory,
        original,
        Assignments::new()
            .assign("title", "Laser optics")
            .assign("vendor", "nethermoor")
            .assign("status", "ordered"),
        vec![
            item("Lens", "3", "12.25", "LA1131"),
            item("Mirror mount", "4", "84.5", "TM-100"),
            item("Iris", "1", "55", ""),
        ],
    );
    let plan = planned(&save, &effects);
    assert_eq!(
        operation_kinds(&plan),
        ["replace_order_record", "refresh_package_index"],
        "one rewrite of the one Order record"
    );
    let report = commit(plan, &effects).result;
    let Ok(Report::Committed {
        result: ChangeResult::EditOrder(saved),
        ..
    }) = report
    else {
        panic!("the Save commits: {report:?}");
    };

    let stored = stored_order(&directory);
    assert_eq!(saved.directory.absolute_path, directory);
    assert_eq!(stored.title.deref(), "Laser optics");
    assert_eq!(stored.vendor_id.as_deref(), Some(NETHERMOOR));
    assert_eq!(stored.status, OrderStatus::Ordered);
    assert_eq!(
        stored.date_ordered.deref(),
        "2031-01-15",
        "entering Ordered without a date takes today's"
    );
    assert_eq!(
        json(&stored.items),
        serde_json::json!([
            { "description": "Lens", "quantity": 3.0, "unit_price": 12.25, "catalog_number": "LA1131" },
            { "description": "Mirror mount", "quantity": 4.0, "unit_price": 84.5, "catalog_number": "TM-100" },
            { "description": "Iris", "quantity": 1.0, "unit_price": 55.0 },
        ])
    );
}

#[test]
fn one_save_commits_a_vendors_facts_and_every_contact_change_at_once() {
    let (_tree, root, _directory) = package("draft-save-vendor");
    let original = stored_vendor(&root, "Tarnwick");

    // Alan Turing is removed, Katherine Johnson moves first, Grace Hopper's role changes,
    // and Ada Lovelace is added; the Name and phone change beside them.
    let save = save_vendor(
        &root,
        original,
        Assignments::new()
            .assign("name", "Tarnwick Inc")
            .assign("phone", "+1 555 0123"),
        vec![
            contact("Katherine Johnson", "Sales"),
            contact("Grace Hopper", "Support"),
            Assignments::new().assign("name", "Ada Lovelace"),
        ],
    );
    let effects = effects(&PlatformFileSystem);
    let plan = planned(&save, &effects);
    assert_eq!(
        operation_kinds(&plan),
        ["replace_vendor_record", "refresh_package_index"]
    );
    let report = commit(plan, &effects).result;
    let Ok(Report::Committed {
        result: ChangeResult::Vendor(saved),
        ..
    }) = report
    else {
        panic!("the Save commits: {report:?}");
    };

    let stored = stored_vendor(&root, "Tarnwick Inc");
    assert_eq!(saved.vendor_id, stored.vendor_id);
    assert_eq!(stored.vendor_id.deref(), TARNWICK);
    assert_eq!(stored.phone.as_deref(), Some("+1 555 0123"));
    assert_eq!(
        json(&stored.contacts),
        serde_json::json!([
            { "name": "Katherine Johnson", "role": "Sales" },
            { "name": "Grace Hopper", "role": "Support" },
            { "name": "Ada Lovelace" },
        ])
    );
    assert_eq!(
        stored_vendor(&root, "Nethermoor").contacts,
        [],
        "no other Vendor changes"
    );
}

#[test]
fn a_draft_equal_to_the_stored_record_plans_no_operation() {
    let (_tree, root, directory) = package("draft-save-unchanged");
    let order = stored_order(&directory);
    let vendor = stored_vendor(&root, "Tarnwick");
    let effects = effects(&PlatformFileSystem);

    for save in [
        save_order(
            &directory,
            order.clone(),
            Assignments::new()
                .assign("title", "Laser components")
                .assign("vendor", "Tarnwick"),
            vec![
                item("Mirror mount", "2", "84.5", "TM-100"),
                item("Post", "5.0", "6.75", "TR2"),
                item("Lens", "3", "12.25", "LA1131"),
            ],
        ),
        save_vendor(
            &root,
            vendor.clone(),
            Assignments::new().assign("name", "Tarnwick"),
            ["Grace Hopper", "Alan Turing", "Katherine Johnson"]
                .map(|name| contact(name, "Sales"))
                .to_vec(),
        ),
    ] {
        let plan = planned(&save, &effects);
        assert_eq!(operation_kinds(&plan), Vec::<String>::new(), "{save:?}");
        let report = commit(plan, &effects).result;
        assert!(
            matches!(report, Ok(Report::Unchanged { .. })),
            "{save:?}: {report:?}"
        );
    }
    assert_eq!(stored_order(&directory), order);
    assert_eq!(stored_vendor(&root, "Tarnwick"), vendor);
}

#[test]
fn an_order_draft_with_faults_in_several_fields_and_rows_refuses_with_all_of_them() {
    let (_tree, root, directory) = package("draft-save-order-faults");
    let before = snapshot(&root);

    let refused = plan(
        &save_order(
            &directory,
            stored_order(&directory),
            Assignments::new()
                .assign("title", "Laser optics")
                .assign("colour", "red")
                .assign("status", "lost")
                .assign("currency", "dollars")
                .assign("vendor", "Nonesuch Optics")
                .assign("project", "Clock"),
            vec![
                item("Lens", "3", "12.25", "LA1131"),
                item("Mirror mount", "many", "84.5", "TM-100"),
                item("Iris", "1", "55", ""),
                Assignments::new()
                    .assign("description", "Post")
                    .assign("quantity", "5"),
            ],
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("the draft is refused");

    let Failure::InvalidDraft(faults) = &refused else {
        panic!("every fault is reported together: {refused:?}");
    };
    let located = faults
        .iter()
        .map(|DraftFailure { row, failure }| (*row, failure.clone()))
        .collect::<Vec<_>>();
    assert!(
        matches!(
            located.as_slice(),
            [
                (None, Failure::InvalidAssignments(rejected)),
                (None, Failure::InvalidFactValue { fact: OrderFactKind::Status, value: status }),
                (None, Failure::InvalidFactValue { fact: OrderFactKind::Currency, value: currency }),
                (None, Failure::UnknownVendor { name, .. }),
                (None, Failure::UnconfirmedReusableValue { field, proposed }),
                (Some(2), Failure::InvalidAssignments(quantity)),
                (Some(4), Failure::MissingRequiredItemFacts { fields }),
            ] if rejected == &[RejectedAssignment { field: "colour".into(), reason: RejectedAssignmentReason::UnknownField }]
                && status == "lost"
                && currency == "dollars"
                && name == "Nonesuch Optics"
                && field == "project" && proposed == "Clock"
                && quantity == &[RejectedAssignment { field: "quantity".into(), reason: RejectedAssignmentReason::NotAFiniteNumber }]
                && fields == &["unit_price"]
        ),
        "{located:#?}"
    );
    assert_eq!(snapshot(&root), before, "a refused Save changes no file");
}

#[test]
fn a_vendor_draft_with_faults_in_several_fields_and_rows_refuses_with_all_of_them() {
    let (_tree, root, _directory) = package("draft-save-vendor-faults");
    let before = snapshot(&root);

    let refused = plan(
        &save_vendor(
            &root,
            stored_vendor(&root, "Tarnwick"),
            Assignments::new()
                .assign("name", " nethermoor ")
                .assign("fax", "+1 555 0124"),
            vec![
                contact("Grace Hopper", "Sales"),
                contact(" ", "Support"),
                Assignments::new().assign("role", "Accounts"),
                contact("Ada Lovelace", "Sales").assign("fax", "1"),
            ],
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("the draft is refused");

    let Failure::InvalidDraft(faults) = &refused else {
        panic!("every fault is reported together: {refused:?}");
    };
    let rejected = |field: &str, reason| {
        vec![RejectedAssignment {
            field: field.into(),
            reason,
        }]
    };
    let located = faults
        .iter()
        .map(|DraftFailure { row, failure }| (*row, failure.clone()))
        .collect::<Vec<_>>();
    assert!(
        matches!(
            located.as_slice(),
            [
                (None, Failure::InvalidAssignments(fax)),
                (None, Failure::VendorNameInUse { vendor_id, .. }),
                (Some(2), Failure::InvalidAssignments(blank)),
                (Some(3), Failure::MissingRequiredName { .. }),
                (Some(4), Failure::InvalidAssignments(unknown)),
            ] if fax == &rejected("fax", RejectedAssignmentReason::UnknownField)
                && vendor_id == NETHERMOOR
                && blank == &rejected("name", RejectedAssignmentReason::BlankRequiredValue)
                && unknown == &rejected("fax", RejectedAssignmentReason::UnknownField)
        ),
        "{located:#?}"
    );
    assert_eq!(snapshot(&root), before, "a refused Save changes no file");
}

#[test]
fn a_draft_row_reports_a_missing_required_fact_beside_a_value_it_rejects() {
    let (_tree, root, directory) = package("draft-save-row-faults");

    let rows = |refused: Failure| match refused {
        Failure::InvalidDraft(faults) => faults
            .into_iter()
            .map(|DraftFailure { row, failure }| (row, failure))
            .collect::<Vec<_>>(),
        other => panic!("every fault is reported together: {other:?}"),
    };
    let item_faults = rows(
        plan(
            &save_order(
                &directory,
                stored_order(&directory),
                Assignments::new(),
                vec![
                    Assignments::new()
                        .assign("description", "Clamp")
                        .assign("quantity", "lots"),
                ],
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("the draft is refused"),
    );
    assert!(
        matches!(
            item_faults.as_slice(),
            [
                (Some(1), Failure::InvalidAssignments(quantity)),
                (Some(1), Failure::MissingRequiredItemFacts { fields }),
            ] if quantity == &[RejectedAssignment { field: "quantity".into(), reason: RejectedAssignmentReason::NotAFiniteNumber }]
                && fields == &["unit_price"]
        ),
        "{item_faults:#?}"
    );

    let contact_faults = rows(
        plan(
            &save_vendor(
                &root,
                stored_vendor(&root, "Tarnwick"),
                Assignments::new(),
                vec![
                    Assignments::new()
                        .assign("role", "Accounts")
                        .assign("fax", "1"),
                ],
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("the draft is refused"),
    );
    assert!(
        matches!(
            contact_faults.as_slice(),
            [
                (Some(1), Failure::InvalidAssignments(fax)),
                (Some(1), Failure::MissingRequiredName { .. }),
            ] if fax == &[RejectedAssignment { field: "fax".into(), reason: RejectedAssignmentReason::UnknownField }]
        ),
        "{contact_faults:#?}"
    );
}

#[test]
fn a_save_against_a_record_changed_since_its_draft_was_read_refuses_and_writes_nothing() {
    let (_tree, root, directory) = package("draft-save-stale");
    let order = stored_order(&directory);
    let vendor = stored_vendor(&root, "Tarnwick");

    // Another editor saves both records after this editor read them.
    committed(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Retitled elsewhere"),
        ),
        &effects(&PlatformFileSystem),
    );
    committed(
        &edit_vendor(
            root.clone(),
            VendorRef::Id(TARNWICK.into()),
            Assignments::new().assign("phone", "+1 555 0100"),
        ),
        &effects(&PlatformFileSystem),
    );
    let before = snapshot(&root);

    for save in [
        save_order(
            &directory,
            order,
            Assignments::new().assign("title", "Laser optics"),
            vec![item("Lens", "3", "12.25", "LA1131")],
        ),
        save_vendor(
            &root,
            vendor,
            Assignments::new().assign("website", "https://www.tarnwick.example"),
            vec![contact("Grace Hopper", "Sales")],
        ),
    ] {
        let effects = effects(&PlatformFileSystem);
        let refused = commit(planned(&save, &effects), &effects).result;
        assert!(
            matches!(refused, Err(Failure::PreconditionsChanged { .. })),
            "{save:?}: {refused:?}"
        );
    }
    assert_eq!(snapshot(&root), before, "a refused Save changes no file");
}
