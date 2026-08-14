//! Decision sources: Define the Order record and lifecycle contract; Define
//! integrity, validation, and repair boundaries; Define durable metadata schemas
//! and version evolution.

use spielorders_core::metadata::OrderDocument;
use spielorders_core::order::OrderValidationFailure;

const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));

#[test]
fn minimal_order_validates_with_an_empty_total() {
    let order = OrderDocument::from_json_bytes(MINIMAL_ORDER).expect("minimal fixture parses");

    let validated = order.validate().expect("minimal Order is valid");

    assert!(validated.item_totals().is_empty());
    assert_eq!(validated.order_total(), 0.0);
}

#[test]
fn impossible_gregorian_date_is_reported() {
    let source = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let order = OrderDocument::from_json_bytes(source.as_bytes())
        .expect("impossible date retains the published structural shape");

    let failures = order.validate().expect_err("impossible date is invalid");

    assert_eq!(failures, vec![OrderValidationFailure::InvalidDateOrdered]);
}

#[test]
fn full_order_validates_and_calculates_item_and_order_totals() {
    let order = OrderDocument::from_json_bytes(FULL_ORDER).expect("full fixture parses");

    let validated = order.validate().expect("full Order is valid");

    assert_eq!(validated.item_totals(), &[169.0]);
    assert_eq!(validated.order_total(), 169.0);
}

#[test]
fn validation_normalizes_order_single_line_facts() {
    let source = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "  Laser components  ",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "orderer": "  Ada Lovelace  ",
  "ordering_institution": "  ",
  "project": "  Telescope\t",
  "items": [{
    "description": "  Mirror mount  ",
    "quantity": 2,
    "unit_price": 84.5,
    "catalog_number": "  TM-100  "
  }]
}"#;
    let order = OrderDocument::from_json_bytes(source).expect("typed Order parses");

    let validated = order.validate().expect("normalized Order is valid");
    let normalized = validated.order();

    assert_eq!(normalized.title.as_str(), "Laser components");
    assert_eq!(normalized.orderer.as_deref(), Some("Ada Lovelace"));
    assert_eq!(normalized.ordering_institution, None);
    assert_eq!(normalized.project.as_deref(), Some("Telescope"));
    assert_eq!(normalized.items[0].description.as_str(), "Mirror mount");
    assert_eq!(normalized.items[0].catalog_number.as_deref(), Some("TM-100"));
}

#[test]
fn totals_keep_unrounded_floating_point_results() {
    let source = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Fractional values",
  "date_ordered": "2026-07-19",
  "status": "complete",
  "items": [
    {"description": "First", "quantity": 1.25, "unit_price": 0.333},
    {"description": "Second", "quantity": -2.5, "unit_price": 0.111}
  ]
}"#;
    let order = OrderDocument::from_json_bytes(source).expect("typed Order parses");

    let validated = order.validate().expect("finite values are valid");
    let expected_items = [1.25 * 0.333, -2.5 * 0.111];

    assert_eq!(validated.item_totals(), &expected_items);
    assert_eq!(validated.order_total(), expected_items.iter().sum::<f64>());
}

#[test]
fn every_status_and_a_future_date_remain_valid_without_completeness_rules() {
    let fixture = String::from_utf8(MINIMAL_ORDER.to_vec()).expect("fixture is UTF-8");

    for status in ["in_preparation", "hold", "ordered", "received", "complete"] {
        let source =
            fixture
                .replacen("2026-07-19", "9999-12-31", 1)
                .replacen("in_preparation", status, 1);
        let order = OrderDocument::from_json_bytes(source.as_bytes()).expect("status parses");

        order
            .validate()
            .unwrap_or_else(|failures| panic!("{status} should validate: {failures:?}"));
    }
}

#[test]
fn zero_and_negative_item_values_remain_valid() {
    let source = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Unusual values",
  "date_ordered": "2026-07-19",
  "status": "hold",
  "items": [
    {"description": "Zero", "quantity": 0, "unit_price": -5},
    {"description": "Negative", "quantity": -2, "unit_price": -3}
  ]
}"#;
    let order = OrderDocument::from_json_bytes(source).expect("typed Order parses");

    let validated = order
        .validate()
        .expect("finite signs do not affect validity");

    assert_eq!(validated.item_totals(), &[0.0, 6.0]);
    assert_eq!(validated.order_total(), 6.0);
}

#[test]
fn gregorian_leap_year_and_month_lengths_are_enforced() {
    let fixture = String::from_utf8(MINIMAL_ORDER.to_vec()).expect("fixture is UTF-8");

    for date in ["2000-02-29", "2400-02-29", "2024-04-30"] {
        let source = fixture.replacen("2026-07-19", date, 1);
        let order = OrderDocument::from_json_bytes(source.as_bytes()).expect("date shape parses");
        assert!(order.validate().is_ok(), "{date} should be a real date");
    }

    for date in ["0000-01-01", "1900-02-29", "2100-02-29", "2026-04-31"] {
        let source = fixture.replacen("2026-07-19", date, 1);
        let order = OrderDocument::from_json_bytes(source.as_bytes()).expect("date shape parses");
        assert_eq!(
            order.validate().expect_err("date should be impossible"),
            vec![OrderValidationFailure::InvalidDateOrdered],
            "{date} should be rejected"
        );
    }
}
