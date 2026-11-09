//! Decision sources: Define the SpielOrders package topology; Define the Order
//! record and lifecycle contract; Define Vendor identity and historical semantics;
//! Define durable metadata schemas and version evolution; Require two parent
//! segments in the Order package locator.

use crate::metadata::{
    Config, FiniteNumber, IndexDocument, MetadataErrorKind, OrderDocument, VendorsDocument,
    VersionDocument,
};
use serde::Deserialize;
use std::path::PathBuf;

const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const MINIMAL_VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const FULL_VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/version.json"
));
const MINIMAL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/index.json"
));
const FULL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
));
const MALFORMED_ORDER_ID: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/order-malformed-identity.json"
));
const ONE_PARENT_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/order-package-root-one-parent.json"
));
const INVALID_CURRENCY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/order-invalid-currency.json"
));

#[test]
fn minimal_order_round_trips_through_canonical_json() {
    let order = OrderDocument::from_json_bytes(MINIMAL_ORDER).expect("minimal fixture parses");

    let canonical = order.to_json_bytes().expect("typed order serializes");

    assert_eq!(canonical, MINIMAL_ORDER);
    assert!(canonical.ends_with(b"}\n"));
    assert!(!canonical.ends_with(b"}\n\n"));
    assert_eq!(
        OrderDocument::from_json_bytes(&canonical).expect("canonical bytes reparse"),
        order
    );
}

#[test]
fn index_documents_preserve_complete_order_readme_and_vendor_projections() {
    for source in [MINIMAL_INDEX, FULL_INDEX] {
        let index = IndexDocument::from_json_bytes(source).expect("index fixture parses");

        let canonical = index.to_json_bytes().expect("index serializes");

        assert_eq!(canonical, source);
        assert!(canonical.ends_with(b"}\n"));
        assert!(!canonical.ends_with(b"}\n\n"));
        assert_eq!(
            IndexDocument::from_json_bytes(&canonical).expect("canonical bytes reparse"),
            index
        );
    }
}

#[test]
fn config_parsing_applies_defaults_and_accepts_both_path_keys() {
    let defaults = Config::from_toml_bytes(b"").expect("empty config uses defaults");
    let configured = Config::from_toml_bytes(
        b"orders_path = \"Purchases/Active\"\nremoved_path = \"Retired\"\n",
    )
    .expect("both governed keys parse");

    assert_eq!(defaults.orders_path, "Orders");
    assert_eq!(defaults.removed_path, "Removed");
    assert_eq!(configured.orders_path, "Purchases/Active");
    assert_eq!(configured.removed_path, "Retired");
}

#[test]
fn unsupported_metadata_version_is_a_shape_failure() {
    let error = VersionDocument::from_json_bytes(br#"{"metadata_version":2}"#)
        .expect_err("unsupported metadata version must fail");

    assert_eq!(error.kind(), MetadataErrorKind::Shape);
}

#[test]
fn json_rejects_a_bom_and_non_utf8_as_encoding_failures() {
    for source in [
        b"\xef\xbb\xbf{\"metadata_version\":1}".as_slice(),
        b"{\"metadata_version\":\xff}".as_slice(),
    ] {
        let error = VersionDocument::from_json_bytes(source)
            .expect_err("forbidden JSON encoding must fail");

        assert_eq!(error.kind(), MetadataErrorKind::Encoding);
    }
}

#[test]
fn present_optional_json_scalars_reject_null() {
    let source = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Laser components",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "orderer": null,
  "items": []
}"#;

    let error = OrderDocument::from_json_bytes(source)
        .expect_err("explicit null is not a schema-valid optional string");

    assert_eq!(error.kind(), MetadataErrorKind::Shape);
}

#[test]
fn order_and_vendor_identities_require_lowercase_uuid_v4_text() {
    let invalid_vendor = br#"{
  "schema_version": 1,
  "vendors": [{
    "vendor_id": "E0990D31-06AA-42E9-97C8-088E3E894822",
    "name": "Tarnwick",
    "contacts": []
  }]
}"#;

    assert!(OrderDocument::from_json_bytes(MALFORMED_ORDER_ID).is_err());
    assert!(VendorsDocument::from_json_bytes(invalid_vendor).is_err());
}

/// A locator is two or more parent-only segments, and a record carrying anything else
/// carries no locator at all.
///
/// **The verdict is the field's, not the record's.** A locator that is absent or
/// malformed is the one Order-record damage the integrity decision calls repairable by
/// recalculation, so the record parses and the field says it spells nothing — which is
/// what lets a Package load diagnose the case that offers that repair instead of an
/// invalid managed artifact whose only action is a replacement written by hand.
#[test]
fn package_root_requires_two_or_more_parent_segments() {
    let deeper = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("\"../..\"", "\"../../..\"", 1);

    for (case, source) in [
        ("one parent segment", ONE_PARENT_ORDER.to_vec()),
        ("no locator at all", without_package_root(MINIMAL_ORDER)),
        (
            "not a relative parent path",
            String::from_utf8(MINIMAL_ORDER.to_vec())
                .expect("fixture is UTF-8")
                .replacen("\"../..\"", "\"/packages/optics\"", 1)
                .into_bytes(),
        ),
        (
            "not text at all",
            String::from_utf8(MINIMAL_ORDER.to_vec())
                .expect("fixture is UTF-8")
                .replacen("\"../..\"", "7", 1)
                .into_bytes(),
        ),
    ] {
        let document = OrderDocument::from_json_bytes(&source)
            .unwrap_or_else(|error| panic!("{case}: the record itself parses: {error}"));
        assert!(
            document.package_root.spelled().is_none(),
            "{case}: and spells no locator"
        );
    }

    for source in [MINIMAL_ORDER.to_vec(), deeper.into_bytes()] {
        assert!(
            OrderDocument::from_json_bytes(&source)
                .expect("a record with a locator parses")
                .package_root
                .spelled()
                .is_some()
        );
    }
}

/// One Order record with its `package_root` member removed outright.
fn without_package_root(source: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(source.to_vec()).expect("fixture is UTF-8");
    let stripped = text
        .lines()
        .filter(|line| !line.contains("\"package_root\""))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(text, stripped, "the locator member was found and removed");
    stripped.into_bytes()
}

#[test]
fn required_domain_text_must_be_nonblank() {
    let blank_title = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "   ",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "items": []
}"#;
    let blank_item = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Order",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "items": [{"description":"\t", "quantity":1, "unit_price":1}]
}"#;
    let blank_vendor_and_contact = br#"{
  "schema_version": 1,
  "vendors": [{
    "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
    "name": " ",
    "contacts": [{"name":"\n"}]
  }]
}"#;

    assert!(OrderDocument::from_json_bytes(blank_title).is_err());
    assert!(OrderDocument::from_json_bytes(blank_item).is_err());
    assert!(VendorsDocument::from_json_bytes(blank_vendor_and_contact).is_err());
}

#[test]
fn date_ordered_enforces_schema_shape_without_semantic_calendar_validation() {
    let source = String::from_utf8(MINIMAL_ORDER.to_vec()).expect("fixture is UTF-8");

    for invalid in ["2026-7-19", "2026-13-19", "2026-07-00"] {
        let candidate = source.replacen("2026-07-19", invalid, 1);
        assert!(OrderDocument::from_json_bytes(candidate.as_bytes()).is_err());
    }
    let semantically_impossible = source.replacen("2026-07-19", "2026-02-31", 1);
    assert!(OrderDocument::from_json_bytes(semantically_impossible.as_bytes()).is_ok());
}

#[test]
fn present_currency_requires_three_uppercase_ascii_letters() {
    let error = OrderDocument::from_json_bytes(INVALID_CURRENCY)
        .expect_err("lowercase currency is not a supported durable shape");

    assert_eq!(error.kind(), MetadataErrorKind::Shape);
}

#[test]
fn index_order_directory_requires_a_normalized_relative_shape() {
    let source = String::from_utf8(FULL_INDEX.to_vec()).expect("fixture is UTF-8");
    let original = "Orders/2026_07_19_Tarnwick_Laser_components";

    for invalid in [
        "/Orders/Order",
        "C:/outside",
        "C:outside",
        "Orders/../Order",
        "Orders/./Order",
        "Orders//Order",
        "Orders\\Order",
    ] {
        let candidate = source.replacen(original, invalid, 1);
        assert!(IndexDocument::from_json_bytes(candidate.as_bytes()).is_err());
    }
}

#[test]
fn typed_item_numbers_cannot_represent_non_finite_values() {
    assert!(FiniteNumber::new(f64::NAN).is_none());
    assert!(FiniteNumber::new(f64::INFINITY).is_none());
    assert_eq!(FiniteNumber::new(-2.5).expect("finite value").get(), -2.5);
}

#[test]
fn vendor_documents_preserve_ordered_vendors_contacts_and_optional_facts() {
    for source in [MINIMAL_VENDORS, FULL_VENDORS] {
        let vendors = VendorsDocument::from_json_bytes(source).expect("vendor fixture parses");

        let canonical = vendors.to_json_bytes().expect("vendors serialize");

        assert_eq!(canonical, source);
        assert!(canonical.ends_with(b"}\n"));
        assert!(!canonical.ends_with(b"}\n\n"));
        assert_eq!(
            VendorsDocument::from_json_bytes(&canonical).expect("canonical bytes reparse"),
            vendors
        );
    }
}

#[test]
fn vendor_visibility_is_an_optional_present_boolean_that_round_trips() {
    let hidden = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [],
      "hidden": true
    }
  ]
}
"#;

    let document = VendorsDocument::from_json_bytes(hidden).expect("a Hidden Vendor parses");

    assert_eq!(document.vendors[0].hidden, Some(true));
    assert!(document.vendors[0].is_hidden());
    assert_eq!(
        document
            .to_json_bytes()
            .expect("a Hidden Vendor serializes"),
        hidden
    );
}

#[test]
fn absent_vendor_visibility_means_visible_and_explicit_null_is_a_shape_failure() {
    let visible = VendorsDocument::from_json_bytes(FULL_VENDORS).expect("full fixture parses");
    let null_visibility = br#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [],
      "hidden": null
    }
  ]
}"#;

    assert_eq!(visible.vendors[0].hidden, None);
    assert!(!visible.vendors[0].is_hidden());
    let error = VendorsDocument::from_json_bytes(null_visibility)
        .expect_err("explicit null is not a schema-valid optional visibility");
    assert_eq!(error.kind(), MetadataErrorKind::Shape);
}

#[test]
fn version_marker_round_trips_through_canonical_json() {
    for source in [MINIMAL_VERSION, FULL_VERSION] {
        let version = VersionDocument::from_json_bytes(source).expect("version fixture parses");

        let canonical = version.to_json_bytes().expect("version serializes");

        assert_eq!(canonical, source);
        assert!(canonical.ends_with(b"}\n"));
        assert!(!canonical.ends_with(b"}\n\n"));
        assert_eq!(
            VersionDocument::from_json_bytes(&canonical).expect("canonical bytes reparse"),
            version
        );
    }
}

#[test]
fn full_order_preserves_every_version_one_fact_and_item() {
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("full fixture parses");

    let canonical = order.to_json_bytes().expect("typed order serializes");

    assert_eq!(canonical, FULL_ORDER);
    assert!(canonical.ends_with(b"}\n"));
    assert!(!canonical.ends_with(b"}\n\n"));
    assert_eq!(
        OrderDocument::from_json_bytes(&canonical).expect("canonical bytes reparse"),
        order
    );
}

#[test]
fn config_rejects_unknown_keys_wrong_types_and_malformed_toml() {
    let unknown = Config::from_toml_bytes(b"orders_path = \"Orders\"\nextra = true\n")
        .expect_err("unknown config key must fail");
    let wrong_type = Config::from_toml_bytes(b"orders_path = 42\n")
        .expect_err("wrong config value type must fail");
    let malformed =
        Config::from_toml_bytes(b"orders_path = \"").expect_err("malformed TOML must fail");

    assert_eq!(unknown.kind(), MetadataErrorKind::Shape);
    assert_eq!(wrong_type.kind(), MetadataErrorKind::Shape);
    assert_eq!(malformed.kind(), MetadataErrorKind::Syntax);
}

#[derive(Deserialize)]
struct InvalidFixture {
    artifact: String,
    file: String,
}

#[test]
fn every_schema_invalid_fixture_is_rejected_by_its_public_codec() {
    let fixture_root = contracts_root().join("fixtures/schema-invalid");
    let manifest_bytes = std::fs::read(fixture_root.join("manifest.json"))
        .expect("schema-invalid manifest is readable");
    let manifest: Vec<InvalidFixture> =
        serde_json::from_slice(&manifest_bytes).expect("schema-invalid manifest parses");

    for fixture in manifest {
        let source = std::fs::read(fixture_root.join(&fixture.file))
            .unwrap_or_else(|error| panic!("failed reading {}: {error}", fixture.file));
        let rejected = match fixture.artifact.as_str() {
            "version" => VersionDocument::from_json_bytes(&source).is_err(),
            // The locator is the one version-1 rule the codec carries as an outcome
            // rather than a parse failure, so a record refused for it is refused by the
            // field rather than by `from_json_bytes` — see `PackageRootLocator`.
            "order" => match OrderDocument::from_json_bytes(&source) {
                Ok(document) => document.package_root.spelled().is_none(),
                Err(_) => true,
            },
            "vendors" => VendorsDocument::from_json_bytes(&source).is_err(),
            "index" => IndexDocument::from_json_bytes(&source).is_err(),
            artifact => panic!("unknown fixture artifact {artifact}"),
        };
        assert!(rejected, "{} should be rejected", fixture.file);
    }
}

fn contracts_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts")
}
