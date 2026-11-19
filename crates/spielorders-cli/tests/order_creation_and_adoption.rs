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

/// Every route either command has to exit `2` carries the one shared usage identity, and
/// nothing about the Package is attempted. A Title beside an assignment names the Order two
/// ways at once, and a second Title is not a second subject.
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

/// A value the Package lacks is refused by name and writes nothing; `--add-values` adds it
/// in the same change, after which an ordinary command reuses it.
#[test]
fn a_value_the_package_lacks_is_refused_unless_the_command_adds_it() {
    let (_tree, root) = initialized_package("add-order-new-values");
    let before = managed_snapshot(&root);
    let refused = Invocation::new(&["add-order", "title=Ruby cell", "orderer=Emmy Noether"])
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_ne!(refused.status, Some(0));
    let document = response(&refused);
    assert_eq!(document["error"]["code"], "unconfirmed_reusable_value");
    assert_eq!(document["error"]["detail"]["field"], "orderer");
    assert_eq!(document["error"]["detail"]["after"], "Emmy Noether");
    assert_eq!(managed_snapshot(&root), before);

    let added = Invocation::new(&["add-order", "title=Ruby cell", "orderer=Emmy Noether"])
        .argument("--add-values")
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(added.status, Some(0), "{}", added.stdout_text());

    let reused = Invocation::new(&["add-order", "title=Cesium cell", "orderer=emmy noether"])
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(reused.status, Some(0), "{}", reused.stdout_text());
    assert_eq!(
        response(&reused)["data"]["order"]["orderer"],
        "Emmy Noether"
    );
}

/// A creation names every fault of its facts, its Vendor, and its values in one refusal,
/// as an edit does, rather than the first alone.
#[test]
fn a_creation_names_every_fault_together() {
    let (_tree, root) = crate::support::initialized_package("creation-every-fault");
    let refused = crate::support::response(
        &crate::support::Invocation::new(&[
            "add-order",
            "title=Bad",
            "vendor=Nobody",
            "orderer=Zed",
            "project=Unknown",
            "status=nope",
            "date_ordered=bad",
            "--json",
        ])
        .rooted_at(&root)
        .run(),
    );
    let mut named: Vec<(String, String)> = std::iter::once(&refused["error"]["detail"])
        .chain(refused["diagnostics"].as_array().unwrap())
        .map(|entry| {
            let code = entry["code"].as_str().unwrap_or("invalid_fact_value");
            (
                code.to_owned(),
                entry["field"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    named.sort();
    assert_eq!(
        named,
        [
            ("invalid_fact_value", "date_ordered"),
            ("invalid_fact_value", "status"),
            ("unconfirmed_reusable_value", "orderer"),
            ("unconfirmed_reusable_value", "project"),
            ("unknown_vendor", "vendor"),
        ]
        .map(|(code, field)| (code.to_owned(), field.to_owned()))
    );
    assert_eq!(refused["error"]["code"], "invalid_fact_value");
}
