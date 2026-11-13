//! Changing one Order's own facts, and maintaining its ordered Item collection.
//!
//! **The number a reader sees is the number a mutation takes**, which the four Item
//! commands share with the four Contact ones: `order show` and `order item list` number an
//! Order's Items from one, and `add`, `edit`, `move`, and `remove` address exactly those
//! numbers against the collection as it stood before the mutation.
//!
//! **A fact edit deliberately leaves the directory alone.** Drift between an Order's facts
//! and its Directory name is ordinary allowed state; `order align` is the explicit action
//! that resolves it, and the tests here assert the drift rather than a rename.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record and
//! lifecycle contract; Durable managed-file writes; Define legacy Order adoption and
//! metadata bootstrapping.

use crate::support::{
    ADOPTED_FACTS, Invocation, managed_snapshot, newest_order_id, order_directory,
    package_with_an_adoption_candidate, package_with_three_items, populated_package, response,
};
use std::path::Path;

/// The Order `populated_package` holds first, with its two Items and every optional fact.
fn one_order(label: &str) -> (crate::support::TemporaryTree, std::path::PathBuf) {
    let (tree, root) = populated_package(label);
    let directory = order_directory(&root, "Laser_components");
    (tree, directory)
}

/// One `order edit` over that Order, as JSON.
fn edit(directory: &Path, assignments: &[&str]) -> crate::support::Outcome {
    let mut run = Invocation::new(&["order", "edit"]).rooted_at(directory);
    for assignment in assignments {
        run = run.argument(assignment);
    }
    run.argument("--json").run()
}

/// The one change a mutation's plan reports of `kind` to `field` (or to no field).
fn change<'a>(
    document: &'a serde_json::Value,
    kind: &str,
    field: Option<&str>,
) -> Option<&'a serde_json::Value> {
    document["data"]["changes"]
        .as_array()
        .expect("a mutation reports its changes")
        .iter()
        .find(|change| change["kind"] == kind && change["field"].as_str() == field)
}

/// The Order as an ordinary read of it finds it, which is what "the change landed" means.
fn read(directory: &Path) -> serde_json::Value {
    let outcome = Invocation::new(&["order", "show", "--json"])
        .rooted_at(directory)
        .run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    response(&outcome)["data"].clone()
}

/// Entering `ordered` with no date applies this machine's local date, and a date in the
/// same edit overrides it.
///
/// A historical correction is one edit rather than two.
#[test]
fn entering_ordered_applies_todays_date_unless_the_same_edit_supplies_one() {
    let today = jiff::Zoned::now().date().strftime("%Y-%m-%d").to_string();

    let (_tree, directory) = one_order("order-edit-ordered-applies-today");
    let applied = response(&edit(&directory, &["status=ordered"]));
    assert_eq!(applied["data"]["order"]["date_ordered"], today);
    assert_eq!(
        change(&applied, "fact", Some("date_ordered")).expect("the date changed")["after"],
        today
    );

    // The same edit carrying a date stores that one, which is the historical correction.
    let (_tree, directory) = one_order("order-edit-ordered-explicit-date");
    let assigned = response(&edit(
        &directory,
        &["status=ordered", "date_ordered=2019-08-22"],
    ));
    assert_eq!(assigned["data"]["order"]["date_ordered"], "2019-08-22");
    assert_eq!(read(&directory)["date_ordered"], "2019-08-22");
}

/// Reuse versus introduction is reported for each of the four categories.
///
/// A CLI assignment is itself the explicit confirmation a genuinely new value requires, so
/// a new Project is introduced rather than refused — and a value this Package already uses
/// under another spelling reuses that canonical spelling, which is what makes "reused"
/// worth reporting at all.
#[test]
fn an_edit_reports_reuse_and_introduction_for_each_category() {
    let (_tree, root) = populated_package("order-edit-reusable-values");
    // The Order that has no Ordering Institution yet, and one this Package already uses.
    let directory = order_directory(&root, "Mirror_mounts");

    let document = response(&edit(
        &directory,
        &[
            // A spelling this Package already uses, written differently.
            "ordering_institution=example institute",
            // A value nothing in this Package uses yet.
            "order_group=Autumn build",
            // Cleared, so the category is absent.
            "orderer=",
        ],
    ));
    let institution = change(&document, "fact", Some("ordering_institution"));
    assert_eq!(
        institution.map(|change| &change["reuse"]),
        Some(&"reused".into())
    );
    assert_eq!(
        institution.map(|change| &change["after"]),
        Some(&"Example Institute".into()),
        "a reused value names the canonical spelling it matched"
    );
    let group = change(&document, "fact", Some("order_group"));
    assert_eq!(group.map(|change| &change["reuse"]), Some(&"new".into()));
    assert_eq!(
        group.map(|change| &change["after"]),
        Some(&"Autumn build".into())
    );
    let orderer = change(&document, "fact", Some("orderer")).expect("the Orderer is cleared");
    assert!(orderer["after"].is_null() && orderer["reuse"].is_null());
    assert!(
        change(&document, "fact", Some("project")).is_none(),
        "a category this command line said nothing about keeps the value the Order carries"
    );
    assert_eq!(document["data"]["order"]["project"], "Telescope");
}

// ---------------------------------------------------------------------------
// The four Item commands.
// ---------------------------------------------------------------------------

/// The Descriptions of one Order's Items, in the order the response reports them, each
/// with the one-based position it stands at and its calculated total.
fn items(directory: &Path) -> Vec<(u64, String, f64)> {
    let order = read(directory);
    order["items"]
        .as_array()
        .expect("the Items are a list")
        .iter()
        .zip(1..)
        .map(|(item, position)| {
            (
                position,
                item["description"]
                    .as_str()
                    .expect("a Description")
                    .to_owned(),
                item["total"]
                    .as_str()
                    .and_then(|total| total.parse().ok())
                    .expect("a calculated total"),
            )
        })
        .collect()
}

/// One `order item` command over that Order, as JSON.
fn item(directory: &Path, arguments: &[&str]) -> crate::support::Outcome {
    let mut run = Invocation::new(&["order", "item"]);
    for argument in arguments {
        run = run.argument(argument);
    }
    run.argument("--json").rooted_at(directory).run()
}

/// `add` appends unless `position=` is supplied, and `position=1` inserts first.
///
/// **`position=` is not an Item fact**, which is the defect this catches: an adapter that
/// passed it through with the Item's own assignments would have the core refuse it as an
/// unknown field, and one that read it and forgot to shift would overwrite the Item
/// already standing there.
#[test]
fn an_item_is_appended_unless_a_position_is_supplied() {
    let (_tree, _root, directory) = package_with_three_items("item-add");

    let appended = item(
        &directory,
        &[
            "add",
            "description=Fiber patch cable",
            "quantity=3",
            "unit_price=12.5",
        ],
    );
    assert_eq!(appended.status, Some(0), "{}", appended.stderr_text());
    let document = response(&appended);
    assert_eq!(document["data"]["outcome"], "committed");
    assert_eq!(
        change(&document, "row_added", None).expect("an Item is added")["position"],
        4
    );
    assert_eq!(document["data"]["order"]["items"][3]["quantity"], "3");
    assert!(
        document["data"]["order"]["items"][3]["catalog_number"].is_null(),
        "the optional Catalog Number is absent rather than empty"
    );

    let inserted = item(
        &directory,
        &[
            "add",
            "description=Optical post",
            "quantity=5",
            "unit_price=1.5",
            "catalog_number=TR3",
            "position=1",
        ],
    );
    assert_eq!(inserted.status, Some(0), "{}", inserted.stderr_text());
    assert_eq!(
        change(&response(&inserted), "row_added", None).expect("an Item is added")["position"],
        1
    );
    assert_eq!(
        items(&directory)
            .into_iter()
            .map(|(position, description, _)| (position, description))
            .collect::<Vec<_>>(),
        vec![
            (1, "Optical post".to_owned()),
            (2, "Fiber collimator".to_owned()),
            (3, "Mirror mount".to_owned()),
            (4, "Optical breadboard".to_owned()),
            (5, "Fiber patch cable".to_owned()),
        ],
        "the insertion shifted the Items from that position on"
    );
}

/// Every way of naming no position is a refusal.
///
/// Zero, a negative, a token that is not a number at all, and one past the end: all four
/// are the collection's own business rather than the command line's, so all four exit `1`
/// rather than two of them being usage errors. A token that is not a position number is
/// refused under its own code, before any range applies.
#[test]
fn every_way_of_naming_no_item_position_is_one_refusal() {
    let (_tree, _root, directory) = package_with_three_items("item-invalid-positions");
    let before = managed_snapshot(&directory);

    // Whether the line names a position at all, or one past what the collection holds.
    let lines: Vec<(Vec<&str>, &str)> = vec![
        (vec!["edit", "0", "quantity=1"], "invalid_item_position"),
        (vec!["edit", "-1", "quantity=1"], "position_not_a_number"),
        (vec!["edit", "two", "quantity=1"], "position_not_a_number"),
        (vec!["edit", "4", "quantity=1"], "invalid_item_position"),
        (vec!["remove", "0"], "invalid_item_position"),
        (vec!["remove", "9"], "invalid_item_position"),
        (vec!["move", "4", "position=1"], "invalid_item_position"),
        (vec!["move", "1", "position=0"], "invalid_item_position"),
        (vec!["move", "1", "position=4"], "invalid_item_position"),
        (
            vec![
                "add",
                "description=Lens",
                "quantity=1",
                "unit_price=2",
                "position=0",
            ],
            "invalid_item_position",
        ),
        (
            vec![
                "add",
                "description=Lens",
                "quantity=1",
                "unit_price=2",
                "position=5",
            ],
            "invalid_item_position",
        ),
    ];

    for (arguments, code) in lines {
        let outcome = item(&directory, &arguments);
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{arguments:?}");
        assert_eq!(document["error"]["code"], code, "{arguments:?}");
        assert_eq!(managed_snapshot(&directory), before, "{arguments:?}");
    }

    // An `add` may name the one position past the last, which is what appending means.
    let appended = item(
        &directory,
        &[
            "add",
            "description=Lens",
            "quantity=1",
            "unit_price=2",
            "position=4",
        ],
    );
    assert_eq!(appended.status, Some(0), "{}", appended.stderr_text());
    assert_eq!(
        change(&response(&appended), "row_added", None).expect("an Item is added")["position"],
        4
    );
}

// ---------------------------------------------------------------------------
// `--dry-run`, over every command and every refusal.
// ---------------------------------------------------------------------------

/// Every command line these commands have, as a dry run and then as the real run.
fn every_mutation() -> Vec<Vec<&'static str>> {
    vec![
        vec!["order", "edit", "title=Laser heads", "status=ordered"],
        vec![
            "order",
            "item",
            "add",
            "description=Fiber patch cable",
            "quantity=3",
            "unit_price=12.5",
        ],
        vec!["order", "item", "edit", "2", "quantity=9"],
        vec!["order", "item", "move", "3", "position=1"],
        vec!["order", "item", "remove", "2"],
    ]
}

/// `--uuid` reaches every one of these commands, as it reaches the reads.
#[test]
fn every_command_can_be_targeted_by_order_id() {
    for arguments in every_mutation() {
        let (_tree, root, _directory) =
            package_with_three_items(&format!("item-uuid-{}", arguments.join("-")));

        let mut run = Invocation::new(&arguments).in_directory(&root);
        run = run
            .argument("--json")
            .argument("--uuid")
            .argument(&*newest_order_id(&root));
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(0),
            "{arguments:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["data"]["order"]["order_id"],
            &*newest_order_id(&root),
            "{arguments:?}: the identity named the Order this mutation changed"
        );
    }
}

// ---------------------------------------------------------------------------
// Items at adoption.
// ---------------------------------------------------------------------------

/// An adoption carrying Items writes them in the order the command line gave, with
/// calculated totals — and one carrying none writes a present empty collection.
///
/// **Adoption is the only mutation that may carry Items at creation**, because an Order
/// created from nothing starts with none. `item` opens one and the assignments after it are
/// that Item's, which is the `order item add` command line with its verb left off.
#[test]
fn an_adoption_writes_the_items_it_was_given_in_order() {
    let (_tree, root, candidate) = package_with_an_adoption_candidate("adopt-with-items");

    let mut run = Invocation::new(&["adopt-order"])
        .argument(&candidate)
        .rooted_at(&root);
    for assignment in ADOPTED_FACTS {
        run = run.argument(assignment);
    }
    for argument in [
        "item",
        "description=Fiber patch cable",
        "quantity=3",
        "unit_price=12.5",
        "catalog_number=SC-100",
        "notes=Polished to APC\nSubstitution acceptable",
        "item",
        "description=Connector",
        "quantity=10",
        "unit_price=0.85",
    ] {
        run = run.argument(argument);
    }
    let outcome = run.argument("--json").run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    let adopted = &document["data"]["order"];
    assert_eq!(
        adopted["items"]
            .as_array()
            .expect("the Items are a list")
            .iter()
            .map(|item| {
                (
                    item["description"].as_str().expect("a Description"),
                    item["total"].as_str().expect("a calculated total"),
                )
            })
            .collect::<Vec<_>>(),
        vec![("Fiber patch cable", "37.5"), ("Connector", "8.5")],
        "the Items are written in the order they were given, with totals the core \
         calculated"
    );
    assert_eq!(adopted["total"], "46");
    assert_eq!(
        adopted["items"][0]["catalog_number"], "SC-100",
        "and the optional Catalog Number where one was given"
    );
    assert_eq!(
        adopted["items"][0]["notes"], "Polished to APC\nSubstitution acceptable",
        "and multiline Notes where they were given"
    );
    assert!(adopted["items"][1]["notes"].is_null());
    assert_eq!(
        read(&candidate)["items"][1]["description"],
        "Connector",
        "an ordinary read of the adopted Order finds them"
    );
}

/// An Item's Notes keep their lines, clear on an empty value, and are found by search.
#[test]
fn item_notes_keep_their_lines_clear_when_empty_and_are_searchable() {
    let (_tree, root, directory) = package_with_three_items("item-notes");
    let notes = "Production notes: insertion loss under 2 dB\r\nJacket 3 mm, 20 m long";
    let added = item(
        &directory,
        &[
            "add",
            "description=Patch cable",
            "quantity=2",
            "unit_price=40",
            &format!("notes=  {notes}  "),
        ],
    );
    assert_eq!(added.status, Some(0), "{}", added.stdout_text());
    let last = |order: &serde_json::Value| {
        order["items"]
            .as_array()
            .expect("Items")
            .last()
            .cloned()
            .expect("an Item")
    };
    assert_eq!(
        last(&read(&directory))["notes"],
        "Production notes: insertion loss under 2 dB\nJacket 3 mm, 20 m long",
        "lines are kept, line endings and outer whitespace normalized"
    );
    assert_eq!(
        last(&read(&directory))["total"],
        "80",
        "Notes never change a total"
    );

    let found = Invocation::new(&["search", "orders", "insertion", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(found.status, Some(0), "{}", found.stdout_text());
    assert_eq!(
        response(&found)["data"].as_array().expect("Orders").len(),
        1
    );

    let position = read(&directory)["items"]
        .as_array()
        .expect("Items")
        .len()
        .to_string();
    let edited = item(
        &directory,
        &["edit", &position, "notes=Lead time four weeks"],
    );
    assert_eq!(edited.status, Some(0), "{}", edited.stdout_text());
    assert_eq!(last(&read(&directory))["notes"], "Lead time four weeks");
    let cleared = item(&directory, &["edit", &position, "notes="]);
    assert_eq!(cleared.status, Some(0), "{}", cleared.stdout_text());
    assert!(last(&read(&directory))["notes"].is_null());
    assert_eq!(last(&read(&directory))["description"], "Patch cable");
}

/// Moving an Order to another Project keeps an Order Group the same command names, even
/// unchanged, and clears one it does not name that the new Project lacks.
#[test]
fn a_project_change_keeps_only_an_order_group_the_command_names() {
    let (_tree, directory) = one_order("project-move-group");
    let group = read(&directory)["order_group"].clone();
    assert!(group.is_string(), "the populated Order has a group");

    let kept = edit(
        &directory,
        &[
            "project=Weather station",
            &format!("order_group={}", group.as_str().unwrap()),
        ],
    );
    assert_eq!(kept.status, Some(0), "{}", kept.stdout_text());
    assert_eq!(read(&directory)["project"], "Weather station");
    assert_eq!(
        read(&directory)["order_group"],
        group,
        "a named group survives the move"
    );

    let cleared = edit(&directory, &["project=Shipping"]);
    assert_eq!(cleared.status, Some(0), "{}", cleared.stdout_text());
    assert_eq!(read(&directory)["project"], "Shipping");
    assert!(
        read(&directory)["order_group"].is_null(),
        "an unnamed group the new Project lacks is cleared"
    );
}
