//! Decision sources: Define Vendor identity and historical semantics; Define the
//! Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::Coded;
use crate::tests::support::assert_reported_beside;
use crate::tests::support::planned_vendor;
use crate::tests::support::{
    EditPoint, FailedCommit, FixedUuid, Index, ManagedFileBoundary, Operation, PlatformFileSystem,
    ScriptedFileSystem, TemporaryTree, Vendors, another_writer_replacing, create_vendor,
    created_order, edit_order, edit_vendor, effects, failing_replacement, json, opened,
    operation_kinds, operations, package_tree, set_vendor_hidden, text, vendor_named,
    with_stage_cleanup_failure,
};
use crate::tests::support::{committed_order, committed_vendor};
use crate::tests::support::{index_is_current, order_at};
use crate::{Assignments, Change, Plan, Report, VendorRef};
use crate::{search::CaseSensitivity, search::SearchQuery, search::VendorVisibility};
use std::ops::Deref;

use crate::{Failure, Warning};
use crate::{metadata::Vendor, mutation::RejectedAssignmentReason};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn creation_plan_is_complete_and_side_effect_free_without_generating_identity() {
    let tree = package_tree("vendor-creation-plan");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let index_path = root.join(".spielorders/index.json");
    let vendors_before = fs::read(&vendors_path).unwrap();
    let index_before = fs::read(&index_path).ok();

    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new()
                .assign("name", "  Tarnwick  ")
                .assign("phone", " +1 555 0123 ")
                .assign("address", "100 Example Avenue\r\nAnytown, ST 00000\n"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a named Vendor can be planned");

    assert_eq!(json(planned_vendor(&plan))["name"], "Tarnwick");
    assert_eq!(
        json(planned_vendor(&plan))["phone"],
        json(&Some("+1 555 0123")),
    );
    assert_eq!(
        json(planned_vendor(&plan))["address"],
        json(&Some("100 Example Avenue\nAnytown, ST 00000")),
    );
    assert_eq!(
        json(planned_vendor(&plan))["email"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(planned_vendor(&plan))["website"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(planned_vendor(&plan))["purchasing_notes"],
        serde_json::Value::Null,
    );
    assert!(
        json(planned_vendor(&plan))["contacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        operations(&plan),
        [
            "replace_vendor_record .spielorders/vendors.json",
            "refresh_package_index .spielorders/index.json",
        ]
    );
    assert_eq!(fs::read(&vendors_path).unwrap(), vendors_before);
    assert_eq!(fs::read(&index_path).ok(), index_before);
}

#[test]
fn creation_commit_generates_identity_at_commit_and_refreshes_the_derived_index() {
    let tree = package_tree("vendor-creation-commit");
    let root = tree.path();
    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new()
                .assign("name", "Tarnwick")
                .assign("email", "sales@example.com"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a named Vendor can be planned");
    let mut uuid = FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822");

    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuid)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid Vendor creation commits: {report:?}");
    };
    assert_eq!(
        committed_vendor(root, &result).vendor_id.deref(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    assert_eq!(committed_vendor(root, &result).name.deref(), "Tarnwick");
    assert_eq!(
        committed_vendor(root, &result).email.as_deref(),
        Some("sales@example.com")
    );
    assert!(committed_vendor(root, &result).contacts.is_empty());
    assert!(index_is_current(root));

    let persisted = fs::read(root.join(".spielorders/vendors.json")).unwrap();
    let document = Vendors::from_json_bytes(&persisted).expect("canonical Vendors parse");
    assert_eq!(document.vendors.len(), 1);
    assert_eq!(
        document.vendors[0].vendor_id.deref(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    let indexed = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the refreshed derived index parses");
    assert_eq!(indexed.vendors, document.vendors);
}

fn created_vendor(root: &Path, uuid: &str, assignments: Assignments) -> Vendor {
    let plan = crate::plan(
        &create_vendor(PathBuf::from(root), assignments.clone()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be planned");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(&mut FixedUuid::new(uuid)),
    )
    .result;
    let Ok(Report::Committed { result, .. }) = report else {
        panic!("the fixture Vendor commits: {report:?}");
    };
    committed_vendor(root, &result).clone()
}

#[test]
fn an_editor_original_prevents_overwriting_shared_vendor_changes() {
    let tree = package_tree("vendor-editor-original");
    let root = tree.path();
    let original = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Supplier"),
    );
    let id = original.vendor_id.deref();
    let external = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(id)),
            Assignments::new().assign("phone", "External phone"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(
            external,
            &effects(&PlatformFileSystem).minting(&mut FixedUuid::new("unused"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let before = fs::read(root.join(".spielorders/vendors.json")).unwrap();
    let stale = crate::plan(
        &Change::SaveVendor {
            root: PathBuf::from(root),
            assignments: Assignments::new().assign("name", "Draft name"),
            contacts: Vec::new(),
            original: original.clone(),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(matches!(
        crate::commit(
            stale,
            &effects(&PlatformFileSystem).minting(&mut FixedUuid::new("unused"))
        )
        .result,
        Err(Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(
        fs::read(root.join(".spielorders/vendors.json")).unwrap(),
        before
    );
}

#[test]
fn trimmed_case_insensitive_duplicate_name_is_rejected_and_preserves_prior_bytes() {
    let tree = package_tree("vendor-duplicate-name");
    let root = tree.path();
    let existing = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "  tarNWICK  "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a case-variant duplicate Vendor Name is rejected");

    let Failure::VendorNameInUse { vendor_id, .. } = &error else {
        panic!("a duplicate Vendor Name is rejected as such: {error:?}");
    };
    assert_eq!(vendor_id, existing.vendor_id.deref());
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn deliberately_distinct_vendor_is_created_without_merging_the_similar_name() {
    let tree = package_tree("vendor-distinct-creation");
    let root = tree.path();
    created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );

    let distinct = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        Assignments::new().assign("name", "Tarnwick Inc"),
    );

    assert_eq!(distinct.name.deref(), "Tarnwick Inc");
    let document =
        Vendors::from_json_bytes(&fs::read(root.join(".spielorders/vendors.json")).unwrap())
            .expect("canonical Vendors parse");
    assert_eq!(
        document
            .vendors
            .iter()
            .map(|vendor| vendor.name.deref())
            .collect::<Vec<_>>(),
        ["Tarnwick", "Tarnwick Inc"]
    );
}

#[test]
fn blank_and_absent_names_are_rejected_before_any_write() {
    let tree = package_tree("vendor-blank-name");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let blank = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "   "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a blank Vendor Name is rejected");
    let absent = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("phone", "+1 555 0100"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("an absent Vendor Name is rejected");

    let Failure::InvalidAssignments(rejected) = blank else {
        panic!("a blank Name is a rejected assignment: {blank:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field.as_str(), entry.reason))
            .collect::<Vec<_>>(),
        [("name", RejectedAssignmentReason::BlankRequiredValue)]
    );
    assert!(matches!(absent, Failure::MissingRequiredName { .. }));
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn rename_retains_identity_updates_live_resolution_and_never_renames_the_order_directory() {
    let tree = package_tree("vendor-rename");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new()
            .assign("name", "Tarnwick")
            .assign("phone", "+1 555 0123"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("vendor", vendor.name.deref()),
    );
    let order_path = directory.join(".spielorders/order.json");
    let order_bytes_before = fs::read(&order_path).unwrap();

    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(vendor.vendor_id.deref())),
            Assignments::new().assign("name", "  Tarnwick Photonics  "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an existing Vendor can be renamed");

    assert_eq!(json(planned_vendor(&plan))["name"], "Tarnwick Photonics");
    assert_eq!(
        json(planned_vendor(&plan))["phone"],
        json(&Some("+1 555 0123")),
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("a valid rename commits: {report:?}");
    };
    assert_eq!(committed_vendor(root, &result).vendor_id, vendor.vendor_id);
    assert_eq!(
        committed_vendor(root, &result).name.deref(),
        "Tarnwick Photonics"
    );
    assert_eq!(
        committed_vendor(root, &result).phone.as_deref(),
        Some("+1 555 0123")
    );
    assert!(index_is_current(root));

    assert!(directory.is_dir(), "the Order Directory keeps its name");
    assert_eq!(
        directory.file_name().unwrap(),
        std::ffi::OsStr::new("2026_08_22_Tarnwick_Laser_components")
    );
    assert_eq!(fs::read(&order_path).unwrap(), order_bytes_before);
    let indexed = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the refreshed derived index parses");
    assert_eq!(indexed.vendors[0].name.deref(), "Tarnwick Photonics");
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
    let tree = package_tree(label);
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

    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new()
                .assign("phone", "")
                .assign("website", "  ")
                .assign("email", " orders@example.com "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("optional Vendor facts can be cleared and replaced");

    assert_eq!(
        json(planned_vendor(&plan))["phone"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(planned_vendor(&plan))["website"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(planned_vendor(&plan))["email"],
        json(&Some("orders@example.com")),
    );
    assert_eq!(
        json(planned_vendor(&plan))["address"],
        json(&Some("100 Example Avenue\nAnytown, ST 00000")),
    );
    assert_eq!(
        json(planned_vendor(&plan))["purchasing_notes"],
        json(&Some("Use institutional account.")),
    );
    assert_eq!(
        json(planned_vendor(&plan))["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|contact| text(&contact["name"]))
            .collect::<Vec<_>>(),
        ["Grace Hopper", "Alan Turing"]
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("explicit clearing commits: {report:?}");
    };
    assert_eq!(committed_vendor(root, &result).phone, None);
    assert_eq!(committed_vendor(root, &result).website, None);
    assert_eq!(
        committed_vendor(root, &result).email.as_deref(),
        Some("orders@example.com")
    );
    assert_eq!(committed_vendor(root, &result).contacts.len(), 2);
    assert_eq!(
        committed_vendor(root, &result).contacts[0].name.deref(),
        "Grace Hopper"
    );
    assert_eq!(
        committed_vendor(root, &result).contacts[0].role.as_deref(),
        Some("Sales")
    );
    assert_eq!(
        committed_vendor(root, &result).contacts[1].name.deref(),
        "Alan Turing"
    );
    assert_eq!(committed_vendor(root, &result).contacts[1].email, None);
}

#[test]
fn edit_rejects_unknown_and_immutable_assignments_before_replacement() {
    let tree = package_with_seeded_vendor("vendor-edit-invalid-assignments");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new()
                .assign("vendor_id", "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")
                .assign("schema_version", "2")
                .assign("contacts", "[]")
                .assign("legal_name", "Tarnwick LLC")
                .assign("email", "first@example.com")
                .assign("email", "second@example.com"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("unknown and immutable assignments are rejected");

    let Failure::InvalidAssignments(rejected) = &error else {
        panic!("invalid assignments are reported as such: {error:?}");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|entry| (entry.field.as_str(), entry.reason))
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

    let error = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("an absent Vendor ID is rejected");

    let Failure::UnknownVendorId { vendor_id } = &error else {
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

    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new()
                .assign("name", "  Tarnwick  ")
                .assign("email", "sales@example.com"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a no-op edit is a valid plan");

    assert!(operations(&plan).is_empty());
    assert!(
        plan.operations().is_empty(),
        "a plan that changes nothing announces no managed operation"
    );

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    let Ok(Report::Unchanged { result, .. }) = report else {
        panic!("a no-op edit reports unchanged: {report:?}");
    };
    assert_eq!(committed_vendor(root, &result).name.deref(), "Tarnwick");
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&vendors_path).unwrap().modified().unwrap(),
        observed_before
    );
}

#[test]
fn hide_and_unhide_round_trip_to_the_exact_prior_canonical_bytes() {
    let tree = package_with_seeded_vendor("vendor-visibility-round-trip");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    let visible_bytes = fs::read(&vendors_path).unwrap();
    let mut uuid = FixedUuid::new("00000000-0000-4000-8000-00000000ffff");

    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the seeded Vendor can be hidden");
    assert!(!operations(&plan).is_empty());
    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuid)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("hiding commits: {report:?}");
    };
    assert!(committed_vendor(root, &result).is_hidden());
    let hidden_bytes = fs::read(&vendors_path).unwrap();
    assert_ne!(hidden_bytes, visible_bytes);
    let hidden_document = Vendors::from_json_bytes(&hidden_bytes).expect("canonical Vendors parse");
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

    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            false,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a Hidden Vendor can be unhidden");
    assert_ne!(json(planned_vendor(&plan))["hidden"], true);
    assert!(!operations(&plan).is_empty());
    let report = crate::commit(plan, &effects(&PlatformFileSystem).minting(&mut uuid)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("unhiding commits: {report:?}");
    };
    assert!(!committed_vendor(root, &result).is_hidden());
    assert_eq!(
        fs::read(&vendors_path).unwrap(),
        visible_bytes,
        "unhiding restores the exact bytes hiding replaced"
    );
}

#[test]
fn redundant_visibility_changes_report_unchanged_without_replacing_canonical_bytes() {
    let tree = package_with_seeded_vendor("vendor-visibility-no-op");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");

    // Unhiding an already-visible Vendor changes nothing.
    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            false,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an already-visible Vendor can be unhidden");
    assert!(operations(&plan).is_empty());
    assert!(
        plan.operations().is_empty(),
        "a plan that changes nothing announces no managed operation"
    );
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;
    let Ok(Report::Unchanged { result, .. }) = report else {
        panic!("a redundant unhide reports unchanged: {report:?}");
    };
    assert!(!committed_vendor(root, &result).is_hidden());

    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the seeded Vendor can be hidden");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let hidden_bytes = fs::read(&vendors_path).unwrap();
    let observed_before = fs::metadata(&vendors_path).unwrap().modified().unwrap();

    // Hiding an already-hidden Vendor changes nothing either.
    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an already-hidden Vendor can be hidden again");
    assert!(operations(&plan).is_empty());
    assert!(plan.operations().is_empty());
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    let Ok(Report::Unchanged { result, .. }) = report else {
        panic!("a redundant hide reports unchanged: {report:?}");
    };
    assert!(committed_vendor(root, &result).is_hidden());
    assert_eq!(fs::read(&vendors_path).unwrap(), hidden_bytes);
    assert_eq!(
        fs::metadata(&vendors_path).unwrap().modified().unwrap(),
        observed_before
    );
}

#[test]
fn a_hidden_vendor_is_listed_with_its_visibility_but_is_not_offered_for_new_use() {
    let tree = package_tree("vendor-offered-list");
    let root = tree.path();
    let hidden = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );
    let visible = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        Assignments::new().assign("name", "Nethermoor"),
    );
    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(hidden.vendor_id.deref())),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be hidden");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let Ok(package) = opened(root, &PlatformFileSystem).result else {
        panic!("the Package still loads as a current candidate");
    };

    // The complete list keeps both and carries visibility, so a caller that needs the
    // Hidden Vendor either way — `package show`, the Vendors table's own control — can
    // still reach it. Only the offer is narrowed.
    assert_eq!(
        package
            .contents
            .vendors
            .iter()
            .map(|vendor| (vendor.name.deref(), vendor.is_hidden()))
            .collect::<Vec<_>>(),
        [("Tarnwick", true), ("Nethermoor", false)]
    );
    assert_eq!(
        package
            .vendors_matching(
                &SearchQuery::new("", CaseSensitivity::Insensitive),
                VendorVisibility::Offered
            )
            .result
            .unwrap()
            .iter()
            .map(|vendor| vendor.vendor_id.deref())
            .collect::<Vec<_>>(),
        [visible.vendor_id.deref()]
    );
}

#[test]
fn an_order_referencing_a_hidden_vendor_is_still_editable() {
    let tree = package_tree("vendor-hidden-order-edit");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("vendor", vendor.name.deref()),
    );
    let plan = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(vendor.vendor_id.deref())),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be hidden");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let plan = crate::plan(
        &edit_order(
            directory.clone(),
            Assignments::new().assign("title", "Laser components and mounts"),
        ),
        &effects(&PlatformFileSystem).on(2026, 8, 22),
    )
    .result
    .expect("an Order keeps its Hidden Vendor Reference and stays editable");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("editing an Order that references a Hidden Vendor commits: {report:?}");
    };
    assert_eq!(
        committed_order(&result).document.title.deref(),
        "Laser components and mounts"
    );
    assert_eq!(
        committed_order(&result).document.vendor_id.as_deref(),
        Some(vendor.vendor_id.deref()),
        "hiding never clears the Vendor Reference it is referenced by"
    );
}

#[test]
fn show_resolves_live_vendor_facts_and_the_referencing_order_resolves_the_rename() {
    let tree = package_tree("vendor-show");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );
    let other = created_vendor(
        root,
        "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
        Assignments::new().assign("name", "Nethermoor"),
    );
    let referencing = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("vendor", vendor.name.deref()),
    );
    created_order(
        root,
        "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f",
        Assignments::new()
            .assign("title", "Optical table")
            .assign("vendor", other.name.deref()),
    );
    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(vendor.vendor_id.deref())),
            Assignments::new().assign("name", "Tarnwick Photonics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be renamed");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff"))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let projection = vendor_named(root, "Tarnwick Photonics", &PlatformFileSystem)
        .result
        .expect("an existing Vendor can be read");

    assert_eq!(projection.vendor.vendor_id, vendor.vendor_id);
    assert_eq!(projection.vendor.name.deref(), "Tarnwick Photonics");
    assert_eq!(
        order_at(&referencing, &PlatformFileSystem)
            .result
            .expect("the referencing Order reads")
            .vendor
            .map(|vendor| vendor.name.to_string()),
        Some(String::from("Tarnwick Photonics")),
        "the referencing Order resolves the renamed Vendor"
    );

    let error = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("an absent Vendor ID resolves no Vendor");
    let Failure::UnknownVendorId { vendor_id } = &error else {
        panic!("an absent Vendor ID is reported as unknown: {error:?}");
    };
    assert_eq!(vendor_id, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
}

const VENDOR_STAGE: &str = ".spielorders/.vendors.json.spielorders-stage";

#[test]
fn injected_precommit_failures_preserve_the_prior_vendor_aggregate_exactly() {
    for boundary in [
        ManagedFileBoundary::WriteStage,
        ManagedFileBoundary::SyncStage,
        ManagedFileBoundary::Replace,
    ] {
        let tree = package_with_seeded_vendor("vendor-precommit-failure");
        let root = tree.path();
        let vendors_path = root.join(".spielorders/vendors.json");
        let before = fs::read(&vendors_path).unwrap();
        let plan = crate::plan(
            &edit_vendor(
                PathBuf::from(root),
                VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
                Assignments::new().assign("name", "Tarnwick Photonics"),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect("the rename can be planned");

        let report = crate::commit(
            plan,
            &effects(&failing_replacement(".spielorders/vendors.json", boundary))
                .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
        )
        .result;

        let Err(rejection) = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let error = &rejection;
        assert_reported_beside(error);
        assert!(
            error.cleanup_failures().is_empty(),
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
    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new().assign("name", "Tarnwick Photonics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the rename can be planned");

    let report = crate::commit(
        plan,
        &effects(&with_stage_cleanup_failure(
            ".spielorders/vendors.json",
            ManagedFileBoundary::SyncStage,
        ))
        .minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a durability failure rejects: {report:?}");
    };
    let error = &rejection;
    assert_reported_beside(error);
    assert!(
        !error.cleanup_failures().is_empty(),
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
    let tree = package_tree("vendor-index-refresh-failure");
    let root = tree.path();
    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be planned");

    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            ".spielorders/vendors.json",
            ManagedFileBoundary::IndexReplacement,
        ))
        .minting(&mut FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822")),
    )
    .result;

    let Ok(Report::CommittedWithWarning {
        result, warnings, ..
    }) = report
    else {
        panic!("a failed index refresh remains committed: {report:?}");
    };
    assert_eq!(
        committed_vendor(root, &result).vendor_id.deref(),
        "e0990d31-06aa-42e9-97c8-088e3e894822"
    );
    let [Warning::IndexRefreshFailed(failure)] = warnings.as_slice() else {
        panic!("the refresh failure is retained: {warnings:?}");
    };
    assert!(matches!(failure, Failure::IndexReplacementFailed(_)));

    let document =
        Vendors::from_json_bytes(&fs::read(root.join(".spielorders/vendors.json")).unwrap())
            .expect("canonical Vendors parse");
    assert_eq!(document.vendors.len(), 1);

    // The committed canonical mutation makes retrying the same creation unsafe.
    let retry = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Tarnwick"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("retrying the committed creation is rejected");
    assert!(matches!(retry, Failure::VendorNameInUse { .. }));
}

#[test]
fn a_concurrent_vendor_aggregate_change_rejects_before_any_write() {
    let tree = package_with_seeded_vendor("vendor-concurrent-change");
    let root = tree.path();
    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
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

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("e0990d31-06aa-42e9-97c8-088e3e894822")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a concurrent aggregate change rejects: {report:?}");
    };
    let Failure::PreconditionsChanged { location } = &rejection else {
        panic!("a concurrent aggregate change is a precondition change: {rejection:?}");
    };
    assert_eq!(
        location.package_relative_path.as_deref(),
        Some(".spielorders/vendors.json"),
        "a refusal carries this Package's spelling of the record it names"
    );
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

fn planned_rename(root: &Path) -> Plan {
    crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new().assign("name", "Tarnwick Optics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
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
    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &vendors_path,
            &theirs,
            EditPoint::StagingWrite,
        ))
        .minting(&mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("an aggregate rewritten underneath this mutation is refused: {report:?}");
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
        fs::read(&vendors_path).unwrap(),
        theirs,
        "the other writer's aggregate survives byte for byte"
    );
    let persisted = Vendors::from_json_bytes(&fs::read(&vendors_path).unwrap()).unwrap();
    assert_eq!(
        persisted.vendors[0].phone.as_deref(),
        Some("+1 555 9999"),
        "their phone number survives, not this mutation's rename"
    );
    assert_eq!(
        persisted.vendors[0].name.deref(),
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
        Vendors::from_json_bytes(&theirs).unwrap(),
        Vendors::from_json_bytes(&before).unwrap(),
        "their aggregate still means exactly what ours did"
    );

    let plan = planned_rename(tree.path());
    let other_writer = another_writer_replacing(&vendors_path, &theirs, EditPoint::StagingWrite);
    let report = crate::commit(
        plan,
        &effects(&other_writer)
            .minting(&mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")),
    )
    .result;

    assert!(
        other_writer.disturbed(),
        "the reformatting really did land under this mutation"
    );
    let Ok(Report::Committed { result, .. }) = report else {
        panic!("an aggregate that still means what was planned against is replaced: {report:?}");
    };
    let result = committed_vendor(tree.path(), &result);
    assert_eq!(result.name.deref(), "Tarnwick Optics");
    let persisted = fs::read(&vendors_path).unwrap();
    let document = Vendors::from_json_bytes(&persisted).unwrap();
    assert_eq!(document.vendors, [result]);
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
        Vendors::from_json_bytes(stored.as_bytes()).unwrap().vendors[0]
            .phone
            .as_deref(),
        Some("  +1 555 0123  "),
        "the stored aggregate really does carry untrimmed text"
    );

    // No concurrent writer at all: nothing changed under this mutation.
    let plan = planned_rename(tree.path());
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(&mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")),
    )
    .result;

    let Ok(Report::Committed { result, .. }) = report else {
        panic!("an untrimmed stored aggregate is still this aggregate, and edits: {report:?}");
    };
    let result = committed_vendor(tree.path(), &result);
    assert_eq!(result.name.deref(), "Tarnwick Optics");
    let document = Vendors::from_json_bytes(&fs::read(&vendors_path).unwrap()).unwrap();
    assert_eq!(document.vendors, [result]);
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
    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &vendors_path,
            THEIRS,
            EditPoint::StagingWrite,
        ))
        .minting(&mut FixedUuid::new("3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("an aggregate that stopped parsing underneath this mutation is refused: {report:?}");
    };
    let _error = rejection;
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
    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be planned");

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(&mut FixedUuid::new(SEEDED_VENDOR_ID)),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a colliding generated identity rejects: {report:?}");
    };
    let Failure::GeneratedVendorIdentityCollision { vendor_id } = &rejection else {
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
    let plan = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be planned");

    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(&mut FixedUuid::new("NOT-A-UUID")),
    )
    .result;

    let Err(rejection) = report else {
        panic!("an invalid generated identity rejects: {report:?}");
    };
    let Failure::InvalidGeneratedIdentity { generated } = &rejection else {
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
        Assignments::new().assign("name", "Nethermoor"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let before = fs::read(&vendors_path).unwrap();

    let error = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new().assign("name", " NETHERMOOR "),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("renaming onto another Vendor Name is rejected");

    let Failure::VendorNameInUse { vendor_id, .. } = &error else {
        panic!("a colliding rename is reported as a duplicate: {error:?}");
    };
    assert_eq!(vendor_id, other.vendor_id.deref());
    assert_eq!(fs::read(&vendors_path).unwrap(), before);
}

#[test]
fn damaged_package_state_rejects_every_vendor_capability() {
    let tree = package_with_seeded_vendor("vendor-damaged-package");
    let root = tree.path();
    let vendors_path = root.join(".spielorders/vendors.json");
    fs::write(&vendors_path, b"{ not json").unwrap();
    let before = fs::read(&vendors_path).unwrap();

    let creation = crate::plan(
        &create_vendor(
            PathBuf::from(root),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Package rejects Vendor creation");
    let edit = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new().assign("name", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Package rejects Vendor editing");
    let hide = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Package rejects hiding a Vendor");
    let unhide = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            false,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .map_err(crate::tests::support::first_fault)
    .expect_err("a damaged Package rejects unhiding a Vendor");

    for error in [creation, edit, hide, unhide] {
        assert!(
            matches!(error, Failure::DamagedPackage(_)),
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
        Assignments::new().assign("name", "Nethermoor"),
    );
    let vendors_path = root.join(".spielorders/vendors.json");
    let index_path = root.join(".spielorders/index.json");
    let vendors_before = fs::read(&vendors_path).unwrap();
    let index_before = fs::read(&index_path).unwrap();

    let edit = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(SEEDED_VENDOR_ID)),
            Assignments::new().assign("name", "Tarnwick Photonics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the rename can be planned");
    let hide = crate::plan(
        &set_vendor_hidden(
            PathBuf::from(root),
            VendorRef::Id(String::from(other.vendor_id.deref())),
            true,
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor can be planned for hiding");

    assert!(!operations(&edit).is_empty());
    assert!(!operations(&hide).is_empty());
    for plan in [&edit, &hide] {
        assert_eq!(
            operation_kinds(plan),
            ["replace_vendor_record", "refresh_package_index",]
        );
    }
    assert_eq!(fs::read(&vendors_path).unwrap(), vendors_before);
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
}

#[test]
fn vendor_mutations_never_inspect_or_change_supporting_material() {
    let tree = package_tree("vendor-supporting-material");
    let root = tree.path();
    let vendor = created_vendor(
        root,
        "e0990d31-06aa-42e9-97c8-088e3e894822",
        Assignments::new().assign("name", "Tarnwick"),
    );
    let directory = created_order(
        root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("vendor", vendor.name.deref()),
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
    let plan = crate::plan(
        &edit_vendor(
            PathBuf::from(root),
            VendorRef::Id(String::from(vendor.vendor_id.deref())),
            Assignments::new().assign("name", "Tarnwick Photonics"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the rename can be planned");
    let filesystem = ScriptedFileSystem::new();

    let report = crate::commit(
        plan,
        &effects(&filesystem).minting(&mut FixedUuid::new("00000000-0000-4000-8000-00000000ffff")),
    )
    .result;

    assert!(matches!(report, Ok(Report::Committed { .. })));
    for (path, bytes) in &sentinels {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{path:?} is unchanged");
    }
    let touched = filesystem
        .paths(Operation::Read)
        .into_iter()
        .chain(filesystem.paths(Operation::ReadDirectory))
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
        assert_eq!(reason.code(), spelling);
    }
}
