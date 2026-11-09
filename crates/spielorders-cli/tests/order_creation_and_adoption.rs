//! `add-order` and `adopt-order`: the two ways an Order Directory comes under
//! management, through the real executable.
//!
//! Decision sources: Define the Rust CLI capability contract; Define legacy Order
//! adoption and metadata bootstrapping; Define the Order record and lifecycle contract

use crate::support::{Invocation, initialized_package, managed_snapshot, response};

/// The Title shorthand and the assignment it stands for are one command.
#[test]
fn both_add_order_forms_produce_the_same_order() {
    let mut produced = Vec::new();
    for form in [
        vec!["add-order", "Laser components"],
        vec!["add-order", "title=Laser components"],
    ] {
        let (_tree, root) = initialized_package("add-order-forms");
        let outcome = Invocation::new(&form)
            .argument("--json")
            .rooted_at(&root)
            .run();
        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
        produced.push(response(&outcome)["data"]["order"].clone());
    }

    let [shorthand, assigned] =
        <[serde_json::Value; 2]>::try_from(produced).expect("both forms were run");
    // The Order ID and the paths differ by construction: each run minted its own identity
    // in its own Package. Every domain fact is the same Order.
    for fact in [
        "title",
        "date_ordered",
        "status",
        "vendor_id",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
        "items",
    ] {
        assert_eq!(
            shorthand["document"][fact], assigned["document"][fact],
            "{fact}"
        );
    }
    assert_eq!(shorthand["order_total"], assigned["order_total"]);
}

/// A Title beside an assignment names the Order two ways at once, and a second Title is
/// not a second subject. Neither is a refused mutation: parsing failed, so nothing about
/// the Package was attempted.
#[test]
fn combining_the_two_add_order_forms_is_a_usage_error() {
    for wrong in [
        vec!["add-order", "Laser components", "status=ordered"],
        vec!["add-order", "Laser components", "Mirror mounts"],
    ] {
        let (_tree, root) = initialized_package("add-order-combined");
        let before = managed_snapshot(&root);

        let outcome = Invocation::new(&wrong)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(2), "{wrong:?}");
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Package was attempted"
        );
    }
}

/// Every route either command has to exit `2` carries the one shared usage identity.
#[test]
fn every_usage_error_these_commands_reach_carries_the_shared_identity() {
    for wrong in [
        // A Title beside an assignment, and two Titles.
        vec!["add-order", "--json", "Laser components", "status=ordered"],
        vec!["add-order", "--json", "Laser components", "Mirror mounts"],
        // A misspelled option, and one another family has.
        vec!["add-order", "--json", "--dry-runn"],
        vec!["add-order", "--json", "--all"],
        // Two output modes at once.
        vec!["add-order", "--json", "--quiet"],
        // A misspelling of the command itself.
        vec!["add-orders", "--json"],
        // Adoption with no candidate named at all.
        vec!["adopt-order", "--json"],
        vec!["adopt-order", "--json", "--uuid", "whatever"],
    ] {
        let (_tree, root) = initialized_package("new-order-usage-errors");
        let before = managed_snapshot(&root);

        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Package was attempted"
        );
    }
}
