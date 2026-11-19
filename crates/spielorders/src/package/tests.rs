//! Decision sources: Define the Order record and lifecycle contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::values::{ReusableValue, canonicalize_reusable_value};

#[test]
fn reusable_values_are_absent_reused_with_canonical_spelling_or_new() {
    let existing = ["Example Institute", "Straße Team"];

    assert_eq!(
        canonicalize_reusable_value(Some("  \t "), &existing),
        ReusableValue::Absent
    );
    assert_eq!(
        canonicalize_reusable_value(Some(" STRASSE TEAM "), &existing),
        ReusableValue::Reused("Straße Team".into())
    );
    assert_eq!(
        canonicalize_reusable_value(Some("  Sample Academy  "), &existing),
        ReusableValue::New("Sample Academy".into())
    );
}
