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
    ADOPTED_FACTS, Invocation, NEWEST_ORDER_ID, managed_snapshot, order_directory,
    package_with_a_blocked_index_refresh, package_with_an_adoption_candidate,
    package_with_three_items, populated_package, response, supporting_material_in,
    unmanaged_candidate,
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

/// The Order as an ordinary read of it finds it, which is what "the change landed" means.
fn read(directory: &Path) -> serde_json::Value {
    let outcome = Invocation::new(&["order", "show", "--json"])
        .rooted_at(directory)
        .run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    response(&outcome)["data"].clone()
}

/// Every mutable fact can be set, changed, and where optional cleared — and the refreshed
/// Order a mutation answers with is the Order an ordinary read then finds.
///
/// One test over all nine, because the claim is about the set: a fact that could be
/// assigned and did not reach the record, or reached it and was not reported, is the same
/// defect wherever it is. The two required facts are changed rather than cleared, and the
/// seven optional ones are cleared afterwards.
#[test]
fn every_mutable_fact_can_be_set_changed_and_where_optional_cleared() {
    let (_tree, directory) = one_order("order-edit-every-fact");

    let outcome = edit(
        &directory,
        &[
            "title=Laser heads",
            "date_ordered=2026-07-20",
            "status=received",
            "orderer=Grace Hopper",
            "ordering_institution=Uplands",
            "currency=EUR",
            "project=Frequency comb",
            "order_group=Autumn build",
        ],
    );
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    let document = response(&outcome);
    assert_eq!(document["data"]["report"]["changed"], true);
    assert_eq!(document["data"]["report"]["committed"], true);
    for (member, value) in [
        ("title", "Laser heads"),
        ("date_ordered", "2026-07-20"),
        ("status", "received"),
        ("orderer", "Grace Hopper"),
        ("ordering_institution", "Uplands"),
        ("currency", "EUR"),
        ("project", "Frequency comb"),
        ("order_group", "Autumn build"),
    ] {
        assert_eq!(document["data"]["order"][member], value, "{member}");
        assert_eq!(read(&directory)[member], value, "{member}, read back");
    }

    // An empty value clears an optional scalar, and a cleared fact is absent from the
    // refreshed Order rather than reported as an empty string.
    let cleared = edit(
        &directory,
        &[
            "orderer=",
            "ordering_institution=",
            "currency=",
            "project=",
            "order_group=",
            "vendor_id=",
        ],
    );
    assert_eq!(cleared.status, Some(0), "{}", cleared.stderr_text());
    let document = response(&cleared);
    for member in [
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
        "vendor_id",
        "vendor",
    ] {
        assert!(
            document["data"]["order"][member].is_null(),
            "{member} is absent from the refreshed Order"
        );
        assert!(
            read(&directory)[member].is_null(),
            "{member} is absent when the Order is read back"
        );
    }
}

/// The Order ID and the technical locator are refused as what they are, each named.
///
/// Two different reasons, because they are two different rules: the Order ID is the
/// immutable identity, and `package_root` and `schema_version` are technical fields the
/// core maintains. A caller is told which of theirs broke which, and nothing is written.
#[test]
fn the_identity_and_the_technical_fields_are_refused_by_name() {
    let (_tree, directory) = one_order("order-edit-immutable");
    let before = managed_snapshot(&directory);

    for (assignment, field, reason) in [
        (
            "order_id=00000000-0000-4000-8000-000000000000",
            "order_id",
            "immutable_identity",
        ),
        (
            "package_root=../..",
            "package_root",
            "managed_technical_field",
        ),
        (
            "schema_version=1",
            "schema_version",
            "managed_technical_field",
        ),
        ("items=none", "items", "ordered_collection"),
        ("colour=red", "colour", "unknown_field"),
    ] {
        let outcome = edit(&directory, &[assignment]);
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignment}");
        assert_eq!(
            document["error"]["code"], "order_assignment_rejected",
            "{assignment}"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], field,
            "{assignment} names the field"
        );
        assert_eq!(
            document["error"]["identities"][0]["kind"], reason,
            "{assignment} names the rule it broke"
        );
        assert_eq!(managed_snapshot(&directory), before, "{assignment}");
    }
}

/// Entering `ordered` with no date applies today's; a date in the same edit overrides it;
/// and an Order already `ordered` keeps the date it has.
///
/// **The response says which of the three happened**, which is the point: the stored date
/// alone cannot distinguish a date the caller supplied from one the clock supplied, and a
/// historical correction is one edit rather than two.
#[test]
fn entering_ordered_applies_todays_date_unless_the_same_edit_supplies_one() {
    let today = {
        use spielorders_core::ports::LocalCalendar;
        let today = spielorders_cli::effects::PlatformCalendar.today();
        format!("{:04}-{:02}-{:02}", today.year, today.month, today.day)
    };

    let (_tree, directory) = one_order("order-edit-ordered-applies-today");
    let applied = response(&edit(&directory, &["status=ordered"]));
    assert_eq!(applied["data"]["order"]["date_ordered"], today);
    assert_eq!(
        applied["data"]["report"]["date_ordered_source"],
        "applied_on_entering_ordered"
    );

    // Already `ordered`: entering the status it is at applies nothing, so the date stands.
    let again = response(&edit(&directory, &["status=ordered"]));
    assert_eq!(again["data"]["order"]["date_ordered"], today);
    assert_eq!(again["data"]["report"]["date_ordered_source"], "retained");
    assert_eq!(
        again["data"]["report"]["changed"], false,
        "an Order already at this status with this date is unchanged"
    );

    // The same edit carrying a date stores that one, which is the historical correction.
    let (_tree, directory) = one_order("order-edit-ordered-explicit-date");
    let assigned = response(&edit(
        &directory,
        &["status=ordered", "date_ordered=2019-08-22"],
    ));
    assert_eq!(assigned["data"]["order"]["date_ordered"], "2019-08-22");
    assert_eq!(
        assigned["data"]["report"]["date_ordered_source"],
        "assigned"
    );
    assert_eq!(read(&directory)["date_ordered"], "2019-08-22");
}

/// Every one of the five established statuses can be entered from any other.
#[test]
fn every_status_can_be_entered_and_nothing_else_can() {
    let (_tree, directory) = one_order("order-edit-statuses");

    for status in [
        "hold",
        "ordered",
        "in_preparation",
        "complete",
        "received",
        "hold",
    ] {
        let outcome = edit(&directory, &[&format!("status={status}")]);
        assert_eq!(
            outcome.status,
            Some(0),
            "{status}: {}",
            outcome.stderr_text()
        );
        assert_eq!(response(&outcome)["data"]["order"]["status"], status);
    }
}

/// A Vendor Reference can be set to an existing Vendor and cleared, and one that resolves
/// to nothing is refused naming the Vendor ID it asked for.
///
/// The refreshed Order resolves the Vendor it now references, so a caller needs no second
/// read to display it — which is the half a stored `vendor_id` alone would not give.
#[test]
fn a_vendor_reference_can_be_set_cleared_and_is_refused_when_it_resolves_to_nothing() {
    let (_tree, directory) = one_order("order-edit-vendor");

    let hidden = response(&edit(
        &directory,
        &["vendor_id=1c9a5b7e-3d44-4f21-8a6c-9b0e2d7f4a13"],
    ));
    assert_eq!(
        hidden["data"]["order"]["vendor"]["name"], "Nethermoor",
        "a Hidden Vendor resolves like any other: hiding governs what is offered"
    );
    assert_eq!(hidden["data"]["order"]["vendor"]["hidden"], true);

    let cleared = response(&edit(&directory, &["vendor_id="]));
    assert!(cleared["data"]["order"]["vendor_id"].is_null());
    assert!(cleared["data"]["order"]["vendor"].is_null());

    let before = managed_snapshot(&directory);
    let outcome = edit(
        &directory,
        &["vendor_id=8b6e3f0a-8ec5-4d47-95e1-412652a56d16"],
    );
    let document = response(&outcome);
    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_vendor_unknown");
    assert_eq!(
        document["error"]["identities"][0]["value"],
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16"
    );
    assert_eq!(managed_snapshot(&directory), before);
}

/// A value the record cannot hold is refused by name, and writes nothing.
///
/// **Naming the field is the whole point.** An assignment set is one atomic mutation, so a
/// refusal that said only that the assembled record was invalid would leave a caller to
/// find which of their assignments it was about. A blank required fact is a refused
/// assignment and travels under its own code, which is the other half of the same claim.
#[test]
fn a_status_date_currency_or_blank_title_is_refused_naming_the_field() {
    let (_tree, directory) = one_order("order-edit-invalid-values");
    let before = managed_snapshot(&directory);

    for (assignment, code, kind, value) in [
        (
            "status=shipped",
            "order_status_invalid",
            "order_status",
            "shipped",
        ),
        (
            "date_ordered=2026-02-30",
            "order_date_ordered_invalid",
            "order_date_ordered",
            "2026-02-30",
        ),
        (
            "date_ordered=the ides of March",
            "order_date_ordered_invalid",
            "order_date_ordered",
            "the ides of March",
        ),
        (
            "currency=dollars",
            "order_currency_invalid",
            "order_currency",
            "dollars",
        ),
        (
            "currency=usd",
            "order_currency_invalid",
            "order_currency",
            "usd",
        ),
    ] {
        let outcome = edit(&directory, &[assignment]);
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignment}");
        assert_eq!(document["error"]["code"], code, "{assignment}");
        assert_eq!(
            document["error"]["identities"][0]["kind"], kind,
            "{assignment} names the fact"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], value,
            "{assignment} quotes the value back"
        );
        assert_eq!(managed_snapshot(&directory), before, "{assignment}");
    }

    // A blank required fact is a refused assignment rather than an unacceptable value:
    // there is nothing to adjudicate, and the field is what the caller has to fill in.
    for assignment in ["title=", "title=   ", "date_ordered=", "status="] {
        let outcome = edit(&directory, &[assignment]);
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignment}");
        assert_eq!(
            document["error"]["code"], "order_assignment_rejected",
            "{assignment}"
        );
        assert_eq!(
            document["error"]["identities"][0]["kind"], "blank_required_value",
            "{assignment}"
        );
        assert_eq!(managed_snapshot(&directory), before, "{assignment}");
    }
}

/// Several assignments are one atomic mutation: an invalid fifth writes none of the first
/// four.
///
/// Asserted against the Package's own bytes rather than against the response, because the
/// claim is that nothing was written — a report saying so and a record carrying four new
/// facts would both be true of a response and only one of them is the contract.
#[test]
fn an_invalid_assignment_in_a_set_writes_none_of_it() {
    let (_tree, directory) = one_order("order-edit-atomic");
    let before = managed_snapshot(&directory);

    let outcome = edit(
        &directory,
        &[
            "title=Laser heads",
            "orderer=Grace Hopper",
            "project=Frequency comb",
            "order_group=Autumn build",
            "status=shipped",
        ],
    );

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(response(&outcome)["error"]["code"], "order_status_invalid");
    assert_eq!(
        managed_snapshot(&directory),
        before,
        "not one of the four valid assignments was applied"
    );
    assert_eq!(
        read(&directory)["title"],
        "Laser components",
        "and an ordinary read finds the Order exactly as it was"
    );
}

/// Reuse versus introduction is reported for each of the four categories.
///
/// A CLI assignment is itself the explicit confirmation a genuinely new value requires, so
/// a new Project is introduced rather than refused — and a value this Package already uses
/// under another spelling reuses that canonical spelling, which is what makes "reused"
/// worth reporting at all.
#[test]
fn an_edit_reports_reuse_and_introduction_for_each_category() {
    let (_tree, directory) = one_order("order-edit-reusable-values");

    let document = response(&edit(
        &directory,
        &[
            // A spelling this Package already uses, written differently.
            "ordering_institution=northwind",
            // A value nothing in this Package uses yet.
            "order_group=Autumn build",
            // Cleared, so the category is absent.
            "orderer=",
        ],
    ));
    let values = &document["data"]["report"]["reusable_values"];

    assert_eq!(values["ordering_institution"]["outcome"], "reused");
    assert_eq!(
        values["ordering_institution"]["value"], "Northwind",
        "a reused value names the canonical spelling it matched"
    );
    assert_eq!(values["order_group"]["outcome"], "introduced");
    assert_eq!(values["order_group"]["value"], "Autumn build");
    assert_eq!(values["orderer"]["outcome"], "absent");
    assert!(values["orderer"]["value"].is_null());
    assert_eq!(
        values["project"]["outcome"], "reused",
        "a category this command line said nothing about keeps the value the Order carries"
    );
    assert_eq!(values["project"]["value"], "Telescope");
}

/// An edit that changes a fact the directory name is composed from leaves the directory
/// where it is.
///
/// Drift is ordinary allowed state and `order align` is the explicit action that resolves
/// it, so an edit that renamed the directory would be doing a second command's work.
#[test]
fn an_edit_never_renames_the_order_directory() {
    let (_tree, directory) = one_order("order-edit-no-rename");

    let document = response(&edit(
        &directory,
        &["title=Laser heads", "date_ordered=2019-08-22"],
    ));

    assert_eq!(
        document["context"]["order_root"],
        directory.to_str().expect("the test path is UTF-8"),
        "the Order is still the directory the command targeted"
    );
    assert!(
        directory.is_dir(),
        "and that directory is still where it was"
    );
}

/// An `order edit` that assigns nothing changes nothing and is not an error.
#[test]
fn an_edit_assigning_the_facts_the_order_carries_reports_unchanged() {
    let (_tree, directory) = one_order("order-edit-no-op");
    let before = managed_snapshot(&directory);

    let outcome = Invocation::new(&["order", "edit", "--json"])
        .rooted_at(&directory)
        .argument("title=Laser components")
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(document["data"]["report"]["changed"], false);
    assert_eq!(
        document["data"]["report"]["operations"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "an unchanged record is committed by performing nothing"
    );
    assert_eq!(managed_snapshot(&directory), before);
}

/// A committed edit whose derived index refresh then fails stays committed, exits `0`,
/// reports the warning, and landed exactly once.
///
/// **The change is canonical and the failure is derived**, so re-running the command is
/// not the action: the report says the index is stale and names the command that replaces
/// it, and one ordinary load shows one edit rather than two.
#[test]
fn a_committed_edit_whose_index_refresh_fails_stays_committed_and_reports_it() {
    let (_tree, root) = package_with_a_blocked_index_refresh("order-edit-stale-index");
    let directory = order_directory(&root, "Laser_components");

    let outcome = edit(&directory, &["title=Laser heads"]);
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "a committed canonical mutation exits 0: {}",
        outcome.stdout_text()
    );
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(document["data"]["report"]["changed"], true);
    assert_eq!(
        document["data"]["report"]["warnings"],
        serde_json::json!(["index_refresh_failed"])
    );
    assert_eq!(document["data"]["report"]["index"]["state"], "stale");

    // The same run for a human says what to do about it, on the stream a warning earns —
    // and it names the command that replaces a stale index rather than this one, because
    // re-running a committed mutation is the retry a report must not encourage.
    let human = Invocation::new(&["order", "edit"])
        .rooted_at(&directory)
        .argument("title=Laser mounts")
        .run();
    assert_eq!(human.status, Some(0), "{}", human.stderr_text());
    assert!(
        human.stderr_text().contains("index rebuild"),
        "the warning points at the command that replaces a stale index: {}",
        human.stderr_text()
    );
    assert!(
        !human.stderr_text().contains("order edit"),
        "and never at running this one again: {}",
        human.stderr_text()
    );

    assert_eq!(
        read(&directory)["title"],
        "Laser mounts",
        "one ordinary load shows both changes landed"
    );
    assert_eq!(
        read(&directory)["items"].as_array().map(Vec::len),
        Some(2),
        "and landed over the Order that was there, once each"
    );
}

// ---------------------------------------------------------------------------
// The four Item commands.
// ---------------------------------------------------------------------------

/// The Descriptions of one Order's Items, in the order the response reports them, each
/// with the position it is reported at and its calculated total.
fn items(directory: &Path) -> Vec<(u64, String, f64)> {
    read(directory)["items"]
        .as_array()
        .expect("the Items are a list")
        .iter()
        .map(|item| {
            (
                item["position"].as_u64().expect("a one-based position"),
                item["description"]
                    .as_str()
                    .expect("a Description")
                    .to_owned(),
                item["item_total"].as_f64().expect("a calculated total"),
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
    assert_eq!(document["data"]["report"]["outcome"], "added");
    assert_eq!(document["data"]["report"]["destination_position"], 4);
    assert!(
        document["data"]["report"]["source_position"].is_null(),
        "an appended Item came from no position"
    );
    assert_eq!(document["data"]["report"]["item"]["quantity"], 3.0);
    assert!(
        document["data"]["report"]["item"]["catalog_number"].is_null(),
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
        response(&inserted)["data"]["report"]["destination_position"],
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

/// `edit`, `move`, and `remove` address the position the collection had before the
/// mutation, and the refreshed totals recalculate from the Items that result.
#[test]
fn item_positions_address_the_pre_mutation_collection() {
    let (_tree, _root, directory) = package_with_three_items("item-positions");
    assert_eq!(
        items(&directory),
        vec![
            (1, "Fiber collimator".to_owned(), 12.0),
            (2, "Mirror mount".to_owned(), 169.0),
            (3, "Optical breadboard".to_owned(), 220.0),
        ]
    );

    let edited = item(
        &directory,
        &["edit", "2", "quantity=3", "catalog_number=TM-100"],
    );
    assert_eq!(edited.status, Some(0), "{}", edited.stderr_text());
    let document = response(&edited);
    assert_eq!(document["data"]["report"]["outcome"], "edited");
    assert_eq!(document["data"]["report"]["source_position"], 2);
    assert_eq!(document["data"]["report"]["destination_position"], 2);
    assert_eq!(
        document["data"]["order"]["order_total"], 485.5,
        "the Order Total is the sum of the recalculated Item Totals"
    );
    assert_eq!(
        items(&directory)[1],
        (2, "Mirror mount".to_owned(), 253.5),
        "only the addressed Item changed, and its total recalculated"
    );

    let moved = item(&directory, &["move", "3", "position=1"]);
    assert_eq!(moved.status, Some(0), "{}", moved.stderr_text());
    let document = response(&moved);
    assert_eq!(document["data"]["report"]["outcome"], "moved");
    assert_eq!(document["data"]["report"]["source_position"], 3);
    assert_eq!(document["data"]["report"]["destination_position"], 1);
    assert_eq!(
        items(&directory)
            .into_iter()
            .map(|(_, description, _)| description)
            .collect::<Vec<_>>(),
        vec!["Optical breadboard", "Fiber collimator", "Mirror mount"]
    );

    let removed = item(&directory, &["remove", "2"]);
    assert_eq!(removed.status, Some(0), "{}", removed.stderr_text());
    let document = response(&removed);
    assert_eq!(document["data"]["report"]["outcome"], "removed");
    assert_eq!(document["data"]["report"]["source_position"], 2);
    assert!(
        document["data"]["report"]["destination_position"].is_null(),
        "a removed Item was left at no position"
    );
    assert_eq!(
        document["data"]["report"]["item"]["description"], "Fiber collimator",
        "the report names the Item it removed, which is in no collection afterwards"
    );
    assert_eq!(
        items(&directory)
            .into_iter()
            .map(|(position, description, _)| (position, description))
            .collect::<Vec<_>>(),
        vec![
            (1, "Optical breadboard".to_owned()),
            (2, "Mirror mount".to_owned()),
        ],
        "and the Items after it moved up"
    );
}

/// Moving an Item to the position it already occupies changes nothing and is not an error.
#[test]
fn moving_an_item_to_its_own_position_reports_unchanged() {
    let (_tree, _root, directory) = package_with_three_items("item-move-no-op");
    let before = managed_snapshot(&directory);

    let outcome = item(&directory, &["move", "2", "position=2"]);
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(document["data"]["report"]["changed"], false);
    assert_eq!(managed_snapshot(&directory), before);
}

/// Every way of naming no position is one refusal, with the range this command can
/// address in the sentence and the position in an identity.
///
/// Zero, a negative, a token that is not a number at all, and one past the end: all four
/// are the collection's own business rather than the command line's, so all four exit `1`
/// under one code rather than two of them being usage errors.
#[test]
fn every_way_of_naming_no_item_position_is_one_refusal() {
    let (_tree, _root, directory) = package_with_three_items("item-invalid-positions");
    let before = managed_snapshot(&directory);

    let lines: Vec<(Vec<&str>, &str, &str)> = vec![
        (vec!["edit", "0", "quantity=1"], "0", "source_position"),
        (vec!["edit", "-1", "quantity=1"], "-1", "source_position"),
        (vec!["edit", "two", "quantity=1"], "two", "source_position"),
        (vec!["edit", "4", "quantity=1"], "4", "source_position"),
        (vec!["remove", "0"], "0", "source_position"),
        (vec!["remove", "9"], "9", "source_position"),
        (vec!["move", "4", "position=1"], "4", "source_position"),
        (vec!["move", "1", "position=0"], "0", "destination_position"),
        (vec!["move", "1", "position=4"], "4", "destination_position"),
        (
            vec![
                "add",
                "description=Lens",
                "quantity=1",
                "unit_price=2",
                "position=0",
            ],
            "0",
            "destination_position",
        ),
        (
            vec![
                "add",
                "description=Lens",
                "quantity=1",
                "unit_price=2",
                "position=5",
            ],
            "5",
            "destination_position",
        ),
    ];

    for (arguments, position, role) in lines {
        let outcome = item(&directory, &arguments);
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{arguments:?}");
        assert_eq!(
            document["error"]["code"], "order_item_position_invalid",
            "{arguments:?}"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], position,
            "{arguments:?} quotes the position back"
        );
        assert_eq!(
            document["error"]["identities"][0]["kind"], role,
            "{arguments:?} says which end it was for"
        );
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
        response(&appended)["data"]["report"]["destination_position"],
        4
    );
}

/// An Item requires a Description, a Quantity, and a Unit Price, and a Quantity the record
/// cannot hold as a finite number is refused.
///
/// Every missing fact is reported at once rather than one round trip at a time, and a
/// refused numeric value names the field and the rule under the assignment-set code.
#[test]
fn an_item_requires_three_facts_and_refuses_a_value_it_cannot_hold() {
    let (_tree, _root, directory) = package_with_three_items("item-required-facts");
    let before = managed_snapshot(&directory);

    let outcome = item(&directory, &["add", "catalog_number=F220APC"]);
    let document = response(&outcome);
    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_item_facts_required");
    assert_eq!(
        document["error"]["identities"]
            .as_array()
            .expect("the missing facts are identities")
            .iter()
            .map(|identity| identity["value"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["description", "quantity", "unit_price"]
    );
    assert_eq!(managed_snapshot(&directory), before);

    for (assignment, field) in [
        ("quantity=quite a few", "quantity"),
        ("quantity=inf", "quantity"),
        ("quantity=NaN", "quantity"),
        ("unit_price=1e400", "unit_price"),
    ] {
        let outcome = item(
            &directory,
            &[
                "add",
                "description=Lens",
                "unit_price=2",
                "quantity=1",
                assignment,
            ],
        );
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignment}");
        assert_eq!(
            document["error"]["code"], "order_assignment_rejected",
            "{assignment}"
        );
        let refused = document["error"]["identities"]
            .as_array()
            .expect("the refused assignments are identities")
            .iter()
            .map(|identity| {
                (
                    identity["value"].as_str().unwrap_or_default().to_owned(),
                    identity["kind"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            refused.contains(&(field.to_owned(), "not_a_finite_number".to_owned()))
                || refused.contains(&(field.to_owned(), "duplicate".to_owned())),
            "{assignment}: {refused:?}"
        );
        assert_eq!(managed_snapshot(&directory), before, "{assignment}");
    }

    // Zero and negative numbers carry no sign restriction, so they are ordinary values.
    let outcome = item(
        &directory,
        &[
            "add",
            "description=Discount",
            "quantity=1",
            "unit_price=-40.5",
        ],
    );
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        response(&outcome)["data"]["report"]["item"]["unit_price"],
        -40.5
    );
}

/// Removing the last Item leaves a collection that is present and empty.
#[test]
fn removing_the_last_item_leaves_a_present_empty_collection() {
    let (_tree, _root, directory) = package_with_three_items("item-remove-last");

    for _ in 0..3 {
        let outcome = item(&directory, &["remove", "1"]);
        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    }

    let order = read(&directory);
    assert_eq!(
        order["items"].as_array().map(Vec::len),
        Some(0),
        "the Items are present and empty rather than absent"
    );
    assert_eq!(order["order_total"], 0.0);

    // And the empty collection is addressable by an append, which is what it is for.
    let outcome = item(
        &directory,
        &["add", "description=Lens", "quantity=2", "unit_price=10"],
    );
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        response(&outcome)["data"]["report"]["destination_position"],
        1
    );
}

/// `order item list` reports what an Item mutation left, with the same numbers.
///
/// The two command lines share one contract, so this is what carries a mutation's refreshed
/// Order to the same bytes the reads are recorded at.
#[test]
fn the_item_list_reports_what_a_mutation_left() {
    let (_tree, _root, directory) = package_with_three_items("item-list-after-mutation");

    let mutated = response(&item(&directory, &["move", "1", "position=3"]));
    let listed = Invocation::new(&["order", "item", "list", "--json"])
        .rooted_at(&directory)
        .run();

    assert_eq!(listed.status, Some(0), "{}", listed.stderr_text());
    assert_eq!(
        response(&listed)["data"]["items"],
        mutated["data"]["order"]["items"],
        "the mutation's refreshed Order and the list are one projection"
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

/// A dry run of each command reports the mutation and performs none of it, and the Order
/// Directory — Supporting Material included — is byte-identical afterwards.
///
/// **Compared as bytes rather than asserted as a claim**: a staging file beside the record,
/// a rewritten record with the same length, or a touched Supporting Material file would
/// all pass a report that said `committed: false`.
#[test]
fn a_dry_run_of_every_command_leaves_the_order_byte_identical() {
    for arguments in every_mutation() {
        let (_tree, _root, directory) =
            package_with_three_items(&format!("item-dry-run-{}", arguments.join("-")));
        supporting_material_in(&directory);
        let before = managed_snapshot(&directory);

        let mut run = Invocation::new(&arguments).rooted_at(&directory);
        run = run.argument("--json").argument("--dry-run");
        let planned = run.run();
        let plan = response(&planned);

        assert_eq!(
            planned.status,
            Some(0),
            "{arguments:?}: {}",
            planned.stderr_text()
        );
        assert_eq!(plan["data"]["report"]["committed"], false, "{arguments:?}");
        assert_eq!(
            managed_snapshot(&directory),
            before,
            "{arguments:?}: no record replaced, no staging file, nothing in the directory \
             touched"
        );

        let mut run = Invocation::new(&arguments).rooted_at(&directory);
        run = run.argument("--json");
        let committed = run.run();
        let real = response(&committed);

        assert_eq!(
            committed.status,
            Some(0),
            "{arguments:?}: {}",
            committed.stderr_text()
        );
        assert_eq!(real["data"]["report"]["committed"], true, "{arguments:?}");
        assert_eq!(
            plan["data"]["order"], real["data"]["order"],
            "{arguments:?}: the plan reported the Order the commit left"
        );
        assert_eq!(
            plan["data"]["report"]["operations"], real["data"]["report"]["operations"],
            "{arguments:?}: and the same managed operations"
        );
        assert_eq!(
            plan["data"]["report"]["changed"], real["data"]["report"]["changed"],
            "{arguments:?}"
        );
    }
}

/// A dry run of a refused mutation refuses identically and still writes nothing.
///
/// A refusal is a precondition, so `--dry-run` does not soften it into a report: the same
/// code, and the Package untouched either way.
#[test]
fn a_dry_run_of_every_refusal_refuses_rather_than_reporting() {
    for (arguments, code) in [
        (
            vec!["order", "edit", "status=shipped"],
            "order_status_invalid",
        ),
        (
            vec!["order", "edit", "date_ordered=2026-02-30"],
            "order_date_ordered_invalid",
        ),
        (
            vec!["order", "edit", "colour=red"],
            "order_assignment_rejected",
        ),
        (
            vec!["order", "item", "add", "catalog_number=F220APC"],
            "order_item_facts_required",
        ),
        (
            vec!["order", "item", "edit", "9", "quantity=1"],
            "order_item_position_invalid",
        ),
        (
            vec!["order", "item", "move", "1", "position=0"],
            "order_item_position_invalid",
        ),
        (
            vec!["order", "item", "remove", "9"],
            "order_item_position_invalid",
        ),
    ] {
        let (_tree, _root, directory) =
            package_with_three_items(&format!("item-dry-refusal-{code}"));
        let before = managed_snapshot(&directory);

        let mut run = Invocation::new(&arguments).rooted_at(&directory);
        run = run.argument("--json").argument("--dry-run");
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{arguments:?}");
        assert_eq!(document["error"]["code"], code, "{arguments:?}");
        assert!(document["data"].is_null(), "{arguments:?}");
        assert_eq!(managed_snapshot(&directory), before, "{arguments:?}");
    }
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
            .argument(NEWEST_ORDER_ID);
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(0),
            "{arguments:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["data"]["order"]["order_id"], NEWEST_ORDER_ID,
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
    assert_eq!(
        document["data"]["order"]["items"]
            .as_array()
            .expect("the Items are a list")
            .iter()
            .map(|item| {
                (
                    item["position"].as_u64().expect("a position"),
                    item["description"].as_str().expect("a Description"),
                    item["item_total"].as_f64().expect("a calculated total"),
                )
            })
            .collect::<Vec<_>>(),
        vec![(1, "Fiber patch cable", 37.5), (2, "Connector", 8.5),],
        "the Items are written in the order they were given, with totals the core \
         calculated"
    );
    assert_eq!(document["data"]["order"]["order_total"], 46.0);
    assert_eq!(
        document["data"]["order"]["items"][0]["catalog_number"], "SC-100",
        "and the optional Catalog Number where one was given"
    );
    assert_eq!(
        read(&candidate)["items"][1]["description"],
        "Connector",
        "an ordinary read of the adopted Order finds them"
    );
}

/// An adoption that carries no Item writes a collection that is present and empty.
#[test]
fn an_adoption_with_no_items_writes_a_present_empty_collection() {
    let (_tree, root) = populated_package("adopt-without-items");
    let candidate = unmanaged_candidate(&root, "2011_02_09_Nethermoor_Posts");

    let mut run = Invocation::new(&["adopt-order"])
        .argument(&candidate)
        .rooted_at(&root);
    for assignment in ADOPTED_FACTS {
        run = run.argument(assignment);
    }
    let outcome = run.argument("--json").run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(
        document["data"]["order"]["items"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(document["data"]["order"]["order_total"], 0.0);
}

/// An Item an adoption cannot accept is refused about the Item, and nothing is adopted.
///
/// The same rules the four Item commands refuse a set by, reported as the Item's own: a
/// caller told an *Order* fact was missing would look at the wrong half of their command
/// line.
#[test]
fn an_adoption_refuses_an_item_it_cannot_accept_and_adopts_nothing() {
    for (items, code) in [
        (
            vec!["item", "description=Lens"],
            "order_item_facts_required",
        ),
        (
            vec![
                "item",
                "description=Lens",
                "quantity=quite a few",
                "unit_price=2",
            ],
            "order_assignment_rejected",
        ),
        (
            vec![
                "item",
                "description=Lens",
                "quantity=1",
                "unit_price=2",
                "colour=red",
            ],
            "order_assignment_rejected",
        ),
    ] {
        let (_tree, root) = populated_package(&format!("adopt-refused-item-{code}"));
        let candidate = unmanaged_candidate(&root, "2011_02_09_Nethermoor_Posts");
        let before = managed_snapshot(&root);

        let mut run = Invocation::new(&["adopt-order"])
            .argument(&candidate)
            .rooted_at(&root);
        for assignment in ADOPTED_FACTS {
            run = run.argument(assignment);
        }
        for argument in &items {
            run = run.argument(argument);
        }
        let outcome = run.argument("--json").run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(1),
            "{items:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["error"]["code"], code, "{items:?}");
        assert_eq!(managed_snapshot(&root), before, "{items:?}");
        assert!(
            !candidate.join(".spielorders").exists(),
            "{items:?}: the candidate is left exactly as it was"
        );
    }
}
