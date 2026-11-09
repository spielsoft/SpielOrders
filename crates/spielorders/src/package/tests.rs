//! Decision sources: Define the Order record and lifecycle contract; Define Vendor
//! identity and historical semantics; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use crate::package::{
    PackageMetadata, PackageMetadataValidationFailure, ReusableValue, canonicalize_reusable_value,
};
use crate::{
    metadata::{IndexDocument, VendorsDocument},
    model::OrderDocument,
};

const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
));

#[test]
fn an_order_referencing_an_existing_vendor_id_validates_whatever_that_vendor_is_called() {
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("Order parses");
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");

    PackageMetadata::new(std::slice::from_ref(&order), &vendors)
        .validate()
        .expect("live Vendor ID match is valid");

    let renamed_source = String::from_utf8(FULL_VENDORS.to_vec())
        .expect("fixture is UTF-8")
        .replacen("Tarnwick", "Renamed Vendor", 1);
    let renamed =
        VendorsDocument::from_json_bytes(renamed_source.as_bytes()).expect("renamed Vendor parses");
    PackageMetadata::new(&[order], &renamed)
        .validate()
        .expect("the unchanged Vendor ID still resolves");
}

#[test]
fn absent_vendor_references_are_valid_and_unresolved_ids_fail() {
    let no_vendors = VendorsDocument::from_json_bytes(MINIMAL_VENDORS).expect("Vendors parse");
    let absent = OrderDocument::from_json_bytes(MINIMAL_ORDER).expect("minimal Order parses");
    let unresolved = OrderDocument::from_json_bytes(FULL_ORDER).expect("full Order parses");

    PackageMetadata::new(&[absent], &no_vendors)
        .validate()
        .expect("an absent Vendor Reference is valid");
    let failures = PackageMetadata::new(&[unresolved], &no_vendors)
        .validate()
        .expect_err("an unresolved Vendor ID is invalid");

    assert_eq!(
        failures,
        vec![PackageMetadataValidationFailure::UnresolvedVendorReference]
    );
}

#[test]
fn duplicate_order_ids_are_invalid() {
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("Order parses");
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");

    let failures = PackageMetadata::new(&[order.clone(), order], &vendors)
        .validate()
        .expect_err("duplicate Order IDs invalidate every conflicting Order");

    assert!(failures.contains(&PackageMetadataValidationFailure::DuplicateOrderId));
}

#[test]
fn vendor_names_are_unique_after_trimming_and_full_unicode_case_folding() {
    let vendors = VendorsDocument::from_json_bytes(
        r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": " Straße Instruments ",
      "contacts": []
    },
    {
      "vendor_id": "f1990d31-06aa-42e9-97c8-088e3e894822",
      "name": "STRASSE Instruments",
      "contacts": []
    }
  ]
}"#
        .as_bytes(),
    )
    .expect("individually valid Vendors parse");

    let failures = PackageMetadata::new(&[], &vendors)
        .validate()
        .expect_err("Unicode-case-insensitive duplicate names invalidate Vendors");

    assert!(
        failures.contains(&PackageMetadataValidationFailure::DuplicateVendorName {
            vendor_indices: vec![0, 1],
        })
    );
}

#[test]
fn duplicate_vendor_ids_report_every_conflicting_vendor() {
    let source = String::from_utf8(FULL_VENDORS.to_vec())
        .expect("fixture is UTF-8")
        .replacen(
            "    }\n  ]",
            "    },\n    {\n      \"vendor_id\": \"e0990d31-06aa-42e9-97c8-088e3e894822\",\n      \"name\": \"Second Vendor\",\n      \"contacts\": []\n    }\n  ]",
            1,
        );
    let vendors = VendorsDocument::from_json_bytes(source.as_bytes()).expect("Vendors parse");

    let failures = PackageMetadata::new(&[], &vendors)
        .validate()
        .expect_err("duplicate Vendor IDs invalidate the aggregate");

    assert!(
        failures.contains(&PackageMetadataValidationFailure::DuplicateVendorId {
            vendor_indices: vec![0, 1],
        })
    );
}

#[test]
fn validation_normalizes_vendor_and_contact_facts() {
    let vendors = VendorsDocument::from_json_bytes(
        r#"{
  "schema_version": 1,
  "vendors": [{
    "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
    "name": "  Tarnwick  ",
    "phone": "   ",
    "email": "  sales@example.com  ",
    "address": " \r\n100 Example Avenue\r\n  Anytown, ST  \r\n ",
    "contacts": [{
      "name": "  Grace Hopper  ",
      "role": "  Sales  ",
      "phone": " \t "
    }],
    "purchasing_notes": "\r\nUse account.\rSecond line.\r\n",
    "hidden": false
  }, {
    "vendor_id": "3f1b8c22-5f4a-4a4c-9d0e-6d2e5a7c8b91",
    "name": "Nethermoor",
    "contacts": [],
    "hidden": true
  }]
}"#
        .as_bytes(),
    )
    .expect("Vendor facts parse");

    let validated = PackageMetadata::new(&[], &vendors)
        .validate()
        .expect("normalized Vendor facts are valid");
    let vendor = &validated.vendors().vendors[0];
    let hidden = &validated.vendors().vendors[1];

    // Visible is spelled one way: an explicit `false` normalizes to absence, so a
    // no-op hide of an already-visible Vendor stays a no-op. A Hidden Vendor keeps
    // its visibility exactly.
    assert_eq!(vendor.hidden, None);
    assert!(!vendor.is_hidden());
    assert_eq!(hidden.hidden, Some(true));
    assert!(hidden.is_hidden());
    assert_eq!(vendor.name.as_str(), "Tarnwick");
    assert_eq!(vendor.phone, None);
    assert_eq!(vendor.email.as_deref(), Some("sales@example.com"));
    assert_eq!(
        vendor.address.as_deref(),
        Some("100 Example Avenue\n  Anytown, ST")
    );
    assert_eq!(vendor.contacts[0].name.as_str(), "Grace Hopper");
    assert_eq!(vendor.contacts[0].role.as_deref(), Some("Sales"));
    assert_eq!(vendor.contacts[0].phone, None);
    assert_eq!(
        vendor.purchasing_notes.as_deref(),
        Some("Use account.\nSecond line.")
    );
}

#[test]
fn reusable_values_are_absent_reused_with_canonical_spelling_or_new() {
    let existing = ["Northwind", "Straße Team"];

    assert_eq!(
        canonicalize_reusable_value(Some("  \t "), &existing),
        ReusableValue::Absent
    );
    assert_eq!(
        canonicalize_reusable_value(Some(" STRASSE TEAM "), &existing),
        ReusableValue::Reused("Straße Team".into())
    );
    assert_eq!(
        canonicalize_reusable_value(Some("  Cedarbrook  "), &existing),
        ReusableValue::New("Cedarbrook".into())
    );
}

#[test]
fn index_embedded_orders_receive_local_semantic_validation() {
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");
    let source = String::from_utf8(FULL_INDEX.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let index = IndexDocument::from_json_bytes(source.as_bytes()).expect("index shape parses");

    let failures = PackageMetadata::new(&[], &vendors)
        .with_index(&index)
        .validate()
        .expect_err("index-embedded impossible dates are invalid");

    assert!(failures.contains(&PackageMetadataValidationFailure::InvalidIndexOrder));
}
