//! Every path by which a monetary value reaches a human stream.
//!
//! Define the Order record and lifecycle contract decides it in one sentence:
//! *"Calculations are not rounded, while displayed monetary values use two decimal
//! places."* The core supplies the unrounded number and this crate renders it, so the
//! rule is an adapter rule and these are the tests that hold the adapter to it.
//!
//! The claims are about *human* output, which is never a parsing contract — so they are
//! about the value being legible as money, not about the sentence it sits in. The `--json`
//! half of each claim is the opposite one: the number the machine response carries is the
//! core's own, unrounded, and a display rule that moved it would be the defect rather than
//! the fix.
//!
//! Decision sources: Define the Order record and lifecycle contract; Define the Rust CLI
//! capability contract.

use super::support::{Invocation, package_with_one_order_priced, populated_package, response};

/// The Order summary lines of a human read.
///
/// Every claim below is about a monetary value inside one of these lines. Asserted
/// against the lines rather than against the whole of standard output because a fixture
/// Package Root carries a nanosecond timestamp, and digits matched against all of stdout
/// would be a rare flake rather than a claim about money.
fn order_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("Order: "))
        .collect();
    assert!(!lines.is_empty(), "the summary names an Order:\n{text}");
    lines
}

/// A whole-number Order Total is shown with two decimal places, in both commands that
/// show one.
///
/// The defect this catches is the shipped one: a bare `{}` over an `f64` prints
/// `total 181` for one Order and `total 73.5` for another, so a Package's money is
/// displayed in as many shapes as its totals happen to have. A reader scanning a column
/// of totals cannot align them, and two Orders differing by fifty cents read as though
/// one of them has no cents at all.
///
/// Asserted against both commands because they share one summary line. A renderer applied
/// in `package show` and missed in `package list orders` is exactly the drift that a
/// per-command test would pass over.
#[test]
fn a_whole_number_total_is_displayed_with_two_decimal_places() {
    let (_tree, root) = populated_package("money-whole-number");

    for command in [vec!["package", "show"], vec!["package", "list", "orders"]] {
        let outcome = Invocation::new(&command).rooted_at(&root).run();
        let text = outcome.stdout_text();
        let orders = order_lines(text);

        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
        assert!(
            orders.iter().any(|line| line.contains("total 181.00")),
            "`{}` displays the whole-number total 181 as `181.00`: {orders:?}",
            command.join(" ")
        );
        assert!(
            orders.iter().any(|line| line.contains("total 73.50")),
            "`{}` displays the one-decimal total 73.5 as `73.50`: {orders:?}",
            command.join(" ")
        );
    }
}

/// An Order whose money adds to nothing displays `total 0.00`, and never a signed zero.
///
/// Two ways of adding to nothing, because they are different values: an Order with no
/// Items at all, and one whose single Item is zero of something priced as a credit —
/// `0 * -5.00`, which is a negative zero.
///
/// The defect this catches is a signed zero on a human's screen. `total -0` was the
/// shipped spelling and `total -0.00` is the one a two-decimal formatter alone would
/// leave; either reports a sign the value does not carry, so an Order that costs nothing
/// reads as though it were a credit.
#[test]
fn an_order_that_adds_to_nothing_displays_an_unsigned_zero() {
    for (label, items) in [
        ("money-zero-no-items", Vec::new()),
        ("money-zero-credit", vec![("0", "-5.00")]),
    ] {
        let (_tree, root) = package_with_one_order_priced(label, &items);

        let outcome = Invocation::new(&["package", "show"]).rooted_at(&root).run();
        let orders = order_lines(outcome.stdout_text());
        let json = Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run();
        // The envelope through the shared reader, so this test contributes the same
        // "exactly one UTF-8 JSON document and nothing else" check every other `--json`
        // test does; the raw text below is then read for the one thing a parsed value
        // cannot tell, since `-0.0` and `0.0` compare equal.
        let document = response(&json);
        let reported = json.stdout_text();

        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
        assert_eq!(json.status, Some(0), "{}", json.stderr_text());
        assert!(
            orders.iter().any(|line| line.contains("total 0.00")),
            "{label}: a total of nothing displays as `0.00`: {orders:?}"
        );
        assert!(
            !orders.iter().any(|line| line.contains("total -0")),
            "{label}: and never carries a sign, in any spelling: {orders:?}"
        );
        // The machine half, which is the number itself rather than a rendering of it: a
        // total of nothing is zero, and it carries no sign in the response either.
        //
        // Read from the bytes because a parsed value cannot answer it — `-0.0` and `0.0`
        // compare equal — and this is exactly where the two spellings had to be told
        // apart: the human `0.00` was already right while the recorded response said
        // `-0.0`, which is what a float sum seeded at negative zero leaves behind and not
        // what a total looks like.
        assert_eq!(
            document["data"]["orders"][0]["order_total"], 0.0,
            "{label}: the response reports a total of nothing"
        );
        assert!(
            reported.contains("\"order_total\": 0.0"),
            "{label}: and spells it without a sign:\n{reported}"
        );
    }
}

/// More precision than two decimal places is rounded for the reader and untouched for
/// the machine.
///
/// Both halves of the decided sentence in one test, because they are one rule:
/// *"Calculations are not rounded, while displayed monetary values use two decimal
/// places."* The Order totals `12.3456`, so a human reads `12.35` and the `--json`
/// response still carries `12.3456`.
///
/// Two defects, and the test would catch either alone. Displaying `12.3456` gives a
/// reader a column of ragged numbers that is not money. Rounding the *value* — in the
/// core, in the projection, or on the way into the response — silently discards
/// precision a consumer asked for by parsing JSON, and the fourth decimal place is
/// exactly where a per-unit price lives.
///
/// `12.35` rather than `12.34` is also the difference between rounding and truncating,
/// which is why this value and not a shorter one.
#[test]
fn precision_beyond_two_places_is_rounded_only_for_the_reader() {
    let (_tree, root) = package_with_one_order_priced("money-precision", &[("1", "12.3456")]);

    let human = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    let json = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&json);
    let order = &document["data"]["orders"][0];

    assert_eq!(human.status, Some(0), "{}", human.stderr_text());
    let orders = order_lines(human.stdout_text());
    let summary = orders[0];
    assert!(
        summary.contains("total 12.35"),
        "the human summary rounds the total to two places: {summary}"
    );
    assert!(
        !summary.contains("12.3456"),
        "and shows no more precision than that: {summary}"
    );

    assert_eq!(json.status, Some(0), "{}", json.stderr_text());
    assert_eq!(
        order["order_total"], 12.3456,
        "the machine response carries the core's own unrounded total"
    );
    assert_eq!(
        order["items"][0]["item_total"], 12.3456,
        "and so does the Item Total it was calculated from"
    );
    assert_eq!(
        order["items"][0]["unit_price"], 12.3456,
        "and the Unit Price the record stores"
    );
}

/// A Quantity is displayed as the value stored, because a Quantity is not money.
///
/// The two-decimal rule is about money, and an Order Item's Quantity is a count of things
/// — `0.005` of a kilogram of powder and `1.005` metres of cable are both ordinary. Sent
/// through the monetary renderer, the first reads `0.01` and the second reads `1.00`: one
/// is two hundred times the stored value and the other is short by a thousandth, and
/// neither reader is told anything was rounded.
///
/// The Unit Price and the Item Total on the same line are money and stay rounded, which is
/// what makes this a claim about which values are money rather than about rounding.
#[test]
fn a_quantity_is_displayed_as_the_value_the_record_stores() {
    let (_tree, root) =
        package_with_one_order_priced("quantity-not-money", &[("0.005", "1000"), ("1.005", "2")]);
    let directory = crate::support::order_directory(&root, "Priced_order");

    let shown = Invocation::new(&["order", "show"])
        .rooted_at(&directory)
        .run();
    let added = Invocation::new(&["order", "item", "add"])
        .rooted_at(&directory)
        .argument("description=Cable")
        .argument("quantity=1.005")
        .argument("unit_price=2")
        .run();

    assert_eq!(shown.status, Some(0), "{}", shown.stderr_text());
    let items = shown.stdout_text();
    assert!(
        items.contains("0.005 x 1000.00"),
        "the stored Quantity, beside a Unit Price that is money:\n{items}"
    );
    assert!(
        items.contains("1.005 x 2.00"),
        "and one whose third decimal place a monetary rendering drops:\n{items}"
    );

    // The subject line of a mutation is the other place an Item's Quantity is printed.
    assert_eq!(added.status, Some(0), "{}", added.stderr_text());
    assert!(
        added.stdout_text().contains("Cable  1.005 x 2.00"),
        "the Item a mutation names carries the same Quantity:\n{}",
        added.stdout_text()
    );
}
