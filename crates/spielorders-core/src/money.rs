//! The two-decimal spelling of a monetary value: what a reader is shown, and what a
//! search matches.
//!
//! **One spelling, here, because two would come to disagree.** A monetary fact is matched
//! by the spelling its record stores and by this one, so a reader who sees `12.50` and
//! searches it finds the Item stored as `12.5`. That holds only while the text a search
//! compares is the text a reader was shown — an adapter spelling its own would make the
//! rule true of whichever adapter the reader happened to be looking at, and the command
//! line and the desktop would answer differently about one Package.
//!
//! No rounded *number* exists anywhere: this returns text, and the values themselves are
//! untouched wherever they travel — nothing rounded is stored, summed, or returned to a
//! caller. The one thing done with the text besides showing it is looking for a term
//! inside it, which is the rule above.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define lookup and
//! search behavior.

/// One monetary value, as a human reads it: two decimal places, always.
///
/// Three behaviors. The value handed in is not modified — `12.3456` reads as `12.35`
/// while every response carrying the number carries `12.3456`. A value numerically equal
/// to zero spells `0.00` whichever zero it is, since `-0.0 == 0.0` and a leading sign
/// would report one the value does not carry. A nonzero credit that merely rounds to zero
/// keeps its sign as `-0.00`, because only the magnitude was rounded away.
pub fn two_decimal_spelling(value: f64) -> String {
    // Decided on the value rather than on the spelled text, which cannot tell a true zero
    // from a small credit: `{:.2}` spells both `-0.0` and `-0.001` as `-0.00`, and only
    // one of them is reporting a sign it does not have.
    if value == 0.0 {
        return "0.00".to_owned();
    }
    format!("{value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Order record and lifecycle contract.

    /// The rule itself, over the shapes an `f64` monetary value arrives in.
    #[test]
    fn every_value_is_spelled_with_exactly_two_decimal_places() {
        assert_eq!(two_decimal_spelling(181.0), "181.00", "a whole number");
        assert_eq!(two_decimal_spelling(73.5), "73.50", "one decimal place");
        assert_eq!(two_decimal_spelling(12.25), "12.25", "already two");
        assert_eq!(two_decimal_spelling(-84.5), "-84.50", "a credit");
    }

    /// More precision than two decimal places is rounded for the reader.
    ///
    /// The caller's value is untouched, which is what the `--json` half of
    /// `spielorders-cli`'s `tests/displayed_monetary_values.rs` asserts against the
    /// running executable.
    #[test]
    fn more_precision_than_two_places_is_rounded_for_the_reader() {
        assert_eq!(two_decimal_spelling(12.3456), "12.35");
        assert_eq!(two_decimal_spelling(12.3444), "12.34");
        assert_eq!(two_decimal_spelling(-12.3456), "-12.35");
    }

    /// A zero is spelled without a sign, whichever zero it is.
    ///
    /// `-0.0` is what an Item Total of `0` of something priced as a credit calculates to.
    /// Stated over the value itself rather than over one Order, because a spelling that
    /// met a negative zero from anywhere else owes the same answer.
    #[test]
    fn a_zero_is_spelled_without_a_sign_whichever_zero_it_is() {
        assert_eq!(two_decimal_spelling(-0.0), "0.00");
        assert_eq!(two_decimal_spelling(0.0), "0.00");
        assert_eq!(
            two_decimal_spelling(0.0 * -5.0),
            "0.00",
            "as it is calculated"
        );
    }

    /// A nonzero credit that rounds to zero keeps its sign.
    ///
    /// The defect this catches is the over-correction: deciding the sign from the spelled
    /// text rather than from the value would spell a real credit as `0.00`, so an Order
    /// carrying a discount would read as costing nothing.
    #[test]
    fn a_small_credit_keeps_the_sign_the_value_actually_carries() {
        assert_eq!(two_decimal_spelling(-0.001), "-0.00");
        assert_eq!(two_decimal_spelling(0.001), "0.00");
    }
}
