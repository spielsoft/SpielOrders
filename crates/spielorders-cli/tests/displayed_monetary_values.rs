//! Every path by which a monetary value reaches a human stream, and the unrounded value
//! the machine response carries beside it.
//!
//! Define the Order record and lifecycle contract leaves display precision to each front
//! end. This executable chooses two decimal places: it rounds Order Totals, Item totals, and Unit Prices for its
//! human reader and passes the library's values through `--json` unchanged.
//!
//! Decision sources: Define the Order record and lifecycle contract; Effect and adapter
//! boundaries; Define the Rust CLI capability contract.

use super::support::{
    Invocation, order_directory, package_with_one_order_priced, populated_package, response,
};

/// The Order Total lines of a human read.
///
/// Every claim below is about a monetary value inside one of these lines. Asserted
/// against the lines rather than against the whole of standard output because a fixture
/// Package Root carries a nanosecond timestamp, and digits matched against all of stdout
/// would be a rare flake rather than a claim about money.
fn order_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("total: "))
        .collect();
    assert!(!lines.is_empty(), "the read shows an Order Total:\n{text}");
    lines
}

/// The trimmed lines that follow the Item whose description line is `description`.
fn item_lines<'a>(text: &'a str, description: &str) -> Vec<&'a str> {
    let opening = format!("description: {description}");
    let numbered = |line: &str| {
        line.split_once(". ")
            .is_some_and(|(number, _)| number.parse::<usize>().is_ok())
    };
    let lines: Vec<&str> = text.lines().map(str::trim_start).collect();
    let start = lines
        .iter()
        .position(|line| numbered(line) && line.ends_with(&opening))
        .unwrap_or_else(|| panic!("the read shows the Item {description}:\n{text}"));
    lines[start + 1..]
        .iter()
        .take_while(|line| !numbered(line))
        .copied()
        .collect()
}

/// The human `order show` of the one priced Order in `root`.
fn shown_order(root: &std::path::Path) -> String {
    let outcome = Invocation::new(&["order", "show"])
        .rooted_at(&order_directory(root, "Priced_order"))
        .run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    outcome.stdout_text().to_owned()
}

/// A whole-number Order Total is shown with two decimal places, in both commands that
/// show one.
///
/// The defect this catches is a bare number on a human's screen: `total 181` for one
/// Order and `total 73.5` for another, so a Package's money is displayed in as many
/// shapes as its totals happen to have.
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
            orders.contains(&"total: 181.00"),
            "`{}` displays the whole-number total 181 as `181.00`: {orders:?}",
            command.join(" ")
        );
        assert!(
            orders.contains(&"total: 73.50"),
            "`{}` displays the one-decimal total 73.5 as `73.50`: {orders:?}",
            command.join(" ")
        );
    }
}

/// More precision than two decimal places is rounded for the reader and untouched for
/// the machine: the Unit Price, the Item totals, and the Order Total alike.
///
/// Two defects, and the test would catch either alone. Displaying `12.3456` gives a
/// reader a column of ragged numbers that is not money. Rounding the *value* — in the
/// library, or on the way into the response — silently discards precision a consumer
/// asked for by parsing JSON. `3 × 0.1` is the second Item because its total is not the
/// `0.3` anyone typed, so only the calculated value itself reads back as it.
///
/// `12.35` rather than `12.34` is also the difference between rounding and truncating.
#[test]
fn precision_beyond_two_places_is_rounded_only_for_the_reader() {
    let (_tree, root) =
        package_with_one_order_priced("money-precision", &[("1", "12.3456"), ("3", "0.1")]);
    let directory = order_directory(&root, "Priced_order");
    let item_total = 3.0 * 0.1;
    let order_total = 0.0 + 12.3456 + item_total;

    let shown = shown_order(&root);
    assert_eq!(
        item_lines(&shown, "Item 1")[..3],
        ["quantity: 1", "unit price: 12.35", "total: 12.35"],
        "{shown}"
    );
    assert_eq!(
        item_lines(&shown, "Item 2")[..3],
        ["quantity: 3", "unit price: 0.10", "total: 0.30"],
        "{shown}"
    );
    assert!(shown.contains("\ntotal: 12.65\n"), "{shown}");

    let read = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    );
    let number = |value: &serde_json::Value| value.as_str().and_then(|text| text.parse().ok());
    let items = &read["data"]["items"];
    assert_eq!(items[0]["unit_price"], "12.3456", "the stored Unit Price");
    assert_eq!(items[0]["total"], "12.3456", "an unrounded Item total");
    assert_eq!(number(&items[1]["total"]), Some(item_total));
    assert_eq!(number(&read["data"]["total"]), Some(order_total));
    let listed = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(
        number(&listed["data"]["orders"][0]["total"]),
        Some(order_total),
        "a listed Order"
    );

    // A changed Unit Price is money too, where `--verbose` shows a change.
    let edited = Invocation::new(&["order", "item", "edit", "1", "unit_price=7.126"])
        .rooted_at(&directory)
        .argument("--verbose")
        .run();
    assert_eq!(edited.status, Some(0), "{}", edited.stderr_text());
    let lines: Vec<&str> = edited.stdout_text().lines().map(str::trim).collect();
    assert!(
        lines.contains(&"before: 12.35") && lines.contains(&"after: 7.13"),
        "{}",
        edited.stdout_text()
    );
}

/// An Order whose money adds to nothing displays `0.00`, and never a signed zero.
///
/// Two ways of adding to nothing, because they are different values: an Order with no
/// Items at all, and one whose single Item is zero of something priced as a credit —
/// `0 * -5.00`, a negative zero the library answers as calculated.
///
/// The defect this catches is a signed zero on a human's screen: `-0.00` is what a
/// two-decimal formatter alone leaves, and it reports a sign the value does not carry, so
/// an Item that costs nothing reads as though it were a credit.
#[test]
fn money_that_adds_to_nothing_displays_an_unsigned_zero() {
    for (label, items) in [
        ("money-zero-no-items", Vec::new()),
        ("money-zero-credit", vec![("0", "-5.00")]),
    ] {
        let (_tree, root) = package_with_one_order_priced(label, &items);

        let outcome = Invocation::new(&["package", "show"]).rooted_at(&root).run();
        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
        let orders = order_lines(outcome.stdout_text());
        let shown = shown_order(&root);
        let lines = order_lines(&shown);

        assert!(
            orders.contains(&"total: 0.00") && lines.iter().all(|line| *line == "total: 0.00"),
            "{label}: every total of nothing displays as `0.00`: {orders:?} {lines:?}"
        );
    }
}

/// A credit keeps its sign, even where it rounds to zero.
///
/// The defect this catches is the over-correction of the rule above: deciding the sign
/// from the rounded text rather than from the value would show a real credit of a tenth of
/// a cent as `0.00`, so an Order carrying a discount would read as costing nothing.
#[test]
fn a_credit_keeps_its_sign_even_where_it_rounds_to_zero() {
    let (_tree, root) =
        package_with_one_order_priced("money-credit", &[("1", "-84.5"), ("1", "-0.001")]);

    let shown = shown_order(&root);

    assert_eq!(
        item_lines(&shown, "Item 1")[..3],
        ["quantity: 1", "unit price: -84.50", "total: -84.50"],
        "{shown}"
    );
    assert_eq!(
        item_lines(&shown, "Item 2")[..3],
        ["quantity: 1", "unit price: -0.00", "total: -0.00"],
        "{shown}"
    );
    assert!(shown.contains("\ntotal: -84.50\n"), "{shown}");
}

/// A Quantity is displayed as the value stored, because a Quantity is not money.
///
/// The two-decimal rule is about money, and an Order Item's Quantity is a count of things
/// — `0.005` of a kilogram of powder and `1.005` metres of cable are both ordinary. Sent
/// through the monetary renderer, the first reads `0.01` and the second reads `1.00`: one
/// is two hundred times the stored value and the other is short by a thousandth, and
/// neither reader is told anything was rounded.
///
/// The Item Total on the same line is money and stays rounded, which is what makes this a
/// claim about which values are money rather than about rounding.
#[test]
fn a_quantity_is_displayed_as_the_value_the_record_stores() {
    let (_tree, root) =
        package_with_one_order_priced("quantity-not-money", &[("0.005", "1000"), ("1.005", "2")]);
    let directory = order_directory(&root, "Priced_order");

    let shown = shown_order(&root);
    let added = Invocation::new(&["order", "item", "add"])
        .rooted_at(&directory)
        .argument("description=Cable")
        .argument("quantity=1.005")
        .argument("unit_price=2")
        .run();

    let first = item_lines(&shown, "Item 1");
    assert!(
        first.contains(&"quantity: 0.005") && first.contains(&"total: 5.00"),
        "the stored Quantity, beside a total that is money: {first:?}"
    );
    let second = item_lines(&shown, "Item 2");
    assert!(
        second.contains(&"quantity: 1.005") && second.contains(&"total: 2.01"),
        "and one whose third decimal place a monetary rendering drops: {second:?}"
    );

    // The result of a mutation is the other place an Item's Quantity is printed.
    assert_eq!(added.status, Some(0), "{}", added.stderr_text());
    let cable = item_lines(added.stdout_text(), "Cable");
    assert!(
        cable.contains(&"quantity: 1.005"),
        "the Item a mutation adds carries the same Quantity: {cable:?}"
    );
}
