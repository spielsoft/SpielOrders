//! The one call site every command that shows money reaches its spelling through.
//!
//! **The spelling itself is the core's**, in [`spielorders_core::money`], because a
//! monetary fact is matched by it as well as by the one its record stores: a reader who
//! searches what they were shown finds it only while there is one spelling. Rounding
//! therefore happens there and nowhere else, and no rounded number exists anywhere — the
//! machine responses carry the core's unrounded values.
//!
//! The core's own unit tests hold the spelling. The commands that show money are held to
//! it through the running executable: the Package summary lines in
//! `tests/displayed_monetary_values.rs`, the Order and Item lines in
//! `tests/order_reads_and_targeting.rs`. A command written later that arrived at a second
//! spelling would be caught the moment it has such a test, and not before.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the Rust CLI
//! capability contract.

/// One monetary value, as a human reads it: two decimal places, always.
pub(crate) fn displayed_amount(value: f64) -> String {
    spielorders_core::money::two_decimal_spelling(value)
}
