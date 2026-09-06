//! The one place a monetary value is rendered for a human. Rounding happens here and
//! nowhere else, so nothing rounded is stored, summed, compared, or returned to a caller.
//!
//! The unit tests below hold the rendering itself, and the commands that show money are
//! held to it through the running executable: the Package summary lines in
//! `tests/displayed_monetary_values.rs`, the Order and Item lines in
//! `tests/order_reads_and_targeting.rs`. A command written later that arrived at a second
//! spelling would be caught the moment it has such a test, and not before.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the Rust CLI
//! capability contract.

/// One monetary value, as a human reads it: two decimal places, always.
///
/// Three behaviors. The value handed in is the core's unrounded calculation and is not
/// modified — this returns text, so `12.3456` reads as `12.35` while `--json` still carries
/// `12.3456`. A value numerically equal to zero renders `0.00` whichever zero it is, since
/// `-0.0 == 0.0` and a leading sign would report one the value does not carry. A nonzero
/// credit that merely rounds to zero keeps its sign as `-0.00`, because only the magnitude
/// was rounded away.
pub(crate) fn displayed_amount(value: f64) -> String {
    // Decided on the value rather than on the rendered text, which cannot tell a true
    // zero from a small credit: `{:.2}` spells both `-0.0` and `-0.001` as `-0.00`, and
    // only one of them is reporting a sign it does not have.
    if value == 0.0 {
        return "0.00".to_owned();
    }
    format!("{value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Order record and lifecycle contract.

    /// The rule itself, over the shapes an `f64` total arrives in.
    #[test]
    fn every_value_is_rendered_with_exactly_two_decimal_places() {
        assert_eq!(displayed_amount(181.0), "181.00", "a whole number");
        assert_eq!(displayed_amount(73.5), "73.50", "one decimal place");
        assert_eq!(displayed_amount(12.25), "12.25", "already two");
        assert_eq!(displayed_amount(-84.5), "-84.50", "a credit");
    }

    /// More precision than two decimal places is rounded for the reader.
    ///
    /// The caller's value is untouched, which is what the `--json` half of
    /// `tests/displayed_monetary_values.rs` asserts against the running executable.
    #[test]
    fn more_precision_than_two_places_is_rounded_for_display() {
        assert_eq!(displayed_amount(12.3456), "12.35");
        assert_eq!(displayed_amount(12.3444), "12.34");
        assert_eq!(displayed_amount(-12.3456), "-12.35");
    }

    /// A zero is displayed without a sign, whichever zero it is.
    ///
    /// `-0.0` is the value an Order with no Items actually carries, which
    /// `tests/displayed_monetary_values.rs` observes through the executable. Stated here
    /// over the value itself, alongside the negative zero one Item produces: `0` of
    /// something priced as a credit.
    #[test]
    fn a_zero_is_displayed_without_a_sign_whichever_zero_it_is() {
        assert_eq!(displayed_amount(-0.0), "0.00");
        assert_eq!(displayed_amount(0.0), "0.00");
        assert_eq!(displayed_amount(0.0 * -5.0), "0.00", "as it is calculated");
        assert_eq!(
            displayed_amount([0.0_f64; 0].iter().sum::<f64>()),
            "0.00",
            "the empty sum, whose seed is `-0.0`"
        );
    }

    /// A nonzero credit that rounds to zero keeps its sign.
    ///
    /// The defect this catches is the over-correction: deciding the sign from the
    /// rendered text rather than from the value would display a real credit as `0.00`, so
    /// an Order carrying a discount would read as costing nothing.
    #[test]
    fn a_small_credit_keeps_the_sign_the_value_actually_carries() {
        assert_eq!(displayed_amount(-0.001), "-0.00");
        assert_eq!(displayed_amount(0.001), "0.00");
    }
}
