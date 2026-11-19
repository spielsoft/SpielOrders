//! Decision sources: Define Vendor identity and historical semantics; Define integrity,
//! validation, and repair boundaries.

use crate::{
    integrity::{Aggregate, AggregateFault},
    metadata::VendorsDocument,
};

const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));

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

    let faults = Aggregate::judge(&vendors)
        .expect_err("Unicode-case-insensitive duplicate names invalidate Vendors");

    assert!(faults.contains(&AggregateFault::DuplicateVendorName {
        vendor_indices: vec![0, 1],
    }));
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

    let faults =
        Aggregate::judge(&vendors).expect_err("duplicate Vendor IDs invalidate the aggregate");

    assert!(faults.contains(&AggregateFault::DuplicateVendorId {
        vendor_indices: vec![0, 1],
    }));
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

    let aggregate = Aggregate::judge(&vendors).expect("normalized Vendor facts are valid");
    let vendor = &aggregate.document().vendors[0];
    let hidden = &aggregate.document().vendors[1];

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
