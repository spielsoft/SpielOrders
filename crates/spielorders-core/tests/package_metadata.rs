//! Decision sources: Define the Order record and lifecycle contract; Define Vendor
//! identity and historical semantics; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution.

use spielorders_core::metadata::{IndexDocument, OrderDocument, VendorsDocument, VersionDocument};
use spielorders_core::package::{
    PackageMetadata, PackageMetadataValidationFailure, ReusableValue, canonicalize_reusable_value,
};

const VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
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
fn order_with_an_existing_vendor_reference_validates_and_resolves_live_facts() {
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("Order parses");
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");

    let validated = PackageMetadata::new(&version, &[order], &vendors)
        .validate()
        .expect("live Vendor ID match is valid");

    assert_eq!(
        validated.vendor_for_order(0).unwrap().name.as_str(),
        "Tarnwick"
    );

    let renamed_source = String::from_utf8(FULL_VENDORS.to_vec())
        .expect("fixture is UTF-8")
        .replacen("Tarnwick", "Renamed Vendor", 1);
    let renamed =
        VendorsDocument::from_json_bytes(renamed_source.as_bytes()).expect("renamed Vendor parses");
    let orders = validated
        .orders()
        .iter()
        .map(|order| order.order().clone())
        .collect::<Vec<_>>();
    let resolved_after_rename = PackageMetadata::new(&version, &orders, &renamed)
        .validate()
        .expect("the unchanged Vendor ID still resolves");
    assert_eq!(
        resolved_after_rename
            .vendor_for_order(0)
            .unwrap()
            .name
            .as_str(),
        "Renamed Vendor"
    );
}

#[test]
fn absent_vendor_references_are_valid_and_unresolved_ids_fail() {
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
    let no_vendors = VendorsDocument::from_json_bytes(MINIMAL_VENDORS).expect("Vendors parse");
    let absent = OrderDocument::from_json_bytes(MINIMAL_ORDER).expect("minimal Order parses");
    let unresolved = OrderDocument::from_json_bytes(FULL_ORDER).expect("full Order parses");
    let unresolved_vendor_id = unresolved
        .vendor_id
        .clone()
        .expect("fixture has a Vendor ID");

    PackageMetadata::new(&version, &[absent], &no_vendors)
        .validate()
        .expect("an absent Vendor Reference is valid");
    let failures = PackageMetadata::new(&version, &[unresolved], &no_vendors)
        .validate()
        .expect_err("an unresolved Vendor ID is invalid");

    assert_eq!(
        failures,
        vec![
            PackageMetadataValidationFailure::UnresolvedVendorReference {
                order_index: 0,
                vendor_id: unresolved_vendor_id,
            }
        ]
    );
}

#[test]
fn duplicate_order_ids_report_every_conflicting_order() {
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("Order parses");
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");

    let failures = PackageMetadata::new(&version, &[order.clone(), order], &vendors)
        .validate()
        .expect_err("duplicate Order IDs invalidate every conflicting Order");

    assert!(
        failures.contains(&PackageMetadataValidationFailure::DuplicateOrderId {
            order_indices: vec![0, 1],
        })
    );
}

#[test]
fn vendor_names_are_unique_after_trimming_and_full_unicode_case_folding() {
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
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

    let failures = PackageMetadata::new(&version, &[], &vendors)
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
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
    let source = String::from_utf8(FULL_VENDORS.to_vec())
        .expect("fixture is UTF-8")
        .replacen(
            "    }\n  ]",
            "    },\n    {\n      \"vendor_id\": \"e0990d31-06aa-42e9-97c8-088e3e894822\",\n      \"name\": \"Second Vendor\",\n      \"contacts\": []\n    }\n  ]",
            1,
        );
    let vendors = VendorsDocument::from_json_bytes(source.as_bytes()).expect("Vendors parse");

    let failures = PackageMetadata::new(&version, &[], &vendors)
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
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
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
    "purchasing_notes": "\r\nUse account.\rSecond line.\r\n"
  }]
}"#
        .as_bytes(),
    )
    .expect("Vendor facts parse");

    let validated = PackageMetadata::new(&version, &[], &vendors)
        .validate()
        .expect("normalized Vendor facts are valid");
    let vendor = &validated.vendors().vendors[0];

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
    let version = VersionDocument::from_json_bytes(VERSION).expect("version parses");
    let vendors = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("Vendors parse");
    let source = String::from_utf8(FULL_INDEX.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let index = IndexDocument::from_json_bytes(source.as_bytes()).expect("index shape parses");

    let failures = PackageMetadata::new(&version, &[], &vendors)
        .with_index(&index)
        .validate()
        .expect_err("index-embedded impossible dates are invalid");

    assert!(
        failures.contains(&PackageMetadataValidationFailure::InvalidIndexOrder {
            index_order_index: 0,
            failures: vec![spielorders_core::order::OrderValidationFailure::InvalidDateOrdered],
        })
    );
}
