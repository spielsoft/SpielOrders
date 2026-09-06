//! The three `order` read commands and every way one of them arrives at a Package.
//!
//! **Targeting is the subject as much as the reads are.** For an `order` command
//! `--root` names the Order Directory rather than the SpielOrders Root, the Package is
//! reached through the Order's own `package_root` locator, and no ancestor is ever
//! searched. M3 shipped a repair that counted two parents to find its Package and
//! refreshed a stranger's directory under a nested layout; the same mistake here would
//! be wrong for every `order` command at once, so the nested layout is exercised
//! directly rather than assumed to behave like the default one.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record
//! and lifecycle contract; Define durable metadata schemas and version evolution.

use crate::support::{
    Invocation, NEWEST_ORDER_ID, OFFERED_VENDOR_ID, OLDEST_ORDER_ID, WrittenOption,
    directory_that_is_not_a_root, managed_snapshot, order_directory,
    order_whose_locator_names_another_package, package_with_a_damaged_order,
    package_with_a_nested_orders_directory, package_with_an_impossible_date,
    package_with_an_unreadable_order_readme, package_with_one_locator_rewritten,
    package_with_one_order_priced, placements, populated_package, response,
};

/// Standing in an Order Directory with no options at all is the ordinary human
/// invocation, and it is the one that proves `--root` defaults to `.` *for an Order
/// command* — a default that named the SpielOrders Root would answer with the Package
/// README instead, which is a different document under a different contract.
#[test]
fn order_readme_answers_the_conventional_path_from_inside_the_order_directory() {
    let (_tree, root) = populated_package("order-readme-default-target");
    let directory = order_directory(&root, "Laser_components");

    let outcome = Invocation::new(&["order", "readme"])
        .in_directory(&directory)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", directory.join("README.md").display()),
        "the conventional Order README path is the whole of the human answer"
    );
}

/// The defect this catches is the one every `order` command would share: a `--root` that
/// fell back to treating a SpielOrders Root as "the Package, therefore some Order of it",
/// or that walked down into `Orders/` looking for one. A Package Root is a real directory
/// this application manages and it is *not* an Order Directory, so naming it here is a
/// failure rather than a search.
#[test]
fn an_order_command_refuses_a_package_root_rather_than_reinterpreting_it() {
    let (_tree, root) = populated_package("order-root-is-not-a-package-root");

    for command in [
        vec!["order", "show"],
        vec!["order", "readme"],
        vec!["order", "item", "list"],
    ] {
        let outcome = Invocation::new(&command).rooted_at(&root).run();
        assert_eq!(
            outcome.status,
            Some(1),
            "{command:?}: {}",
            outcome.stdout_text()
        );
        assert!(
            outcome.stderr_text().contains("is not an Order Directory"),
            "{command:?}: {}",
            outcome.stderr_text()
        );
    }
}

/// A plain directory that is neither a Root nor an Order Directory, sitting *inside* a
/// Package. Without the Package above it, "found nothing" and "searched nothing" would be
/// the same observation — the Package one level up is what makes this a claim that no
/// ancestor was consulted.
#[test]
fn an_order_command_refuses_a_plain_directory_inside_a_package() {
    let (_tree, root) = populated_package("order-root-plain-directory");
    let inside = directory_that_is_not_a_root(&root);

    let response = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&inside)
            .run(),
    );

    assert_eq!(response["contract"], "spielorders.order.show");
    assert_eq!(response["error"]["code"], "order_root_unmanaged");
    assert_eq!(response["error"]["scope"], "order");
    // Neither root was established: the directory holds no Order record, so it carries no
    // locator, so no Package was reached. The caller's own text is the one path there is.
    assert!(response["context"]["package_root"].is_null());
    assert!(response["context"]["order_root"].is_null());
    assert_eq!(response["error"]["paths"][0]["spelling"], "as_given");
    assert_eq!(
        response["error"]["paths"][0]["path"],
        inside.to_str().expect("the test path is UTF-8")
    );
}

/// The whole point of the technical locator, exercised rather than assumed.
///
/// Under the default layout an Order Directory is two levels below its Root, so a
/// targeting rule that counted a fixed two parents would pass every other test in this
/// file. Here the configured Orders Directory is `archive/2026/Orders`, so the Order sits
/// four levels down, and the Order's own `package_root` is the only thing that knows it.
#[test]
fn an_order_under_a_nested_orders_directory_finds_its_own_package() {
    let (_tree, root, directory) = package_with_a_nested_orders_directory("order-nested-layout");

    let response = response(
        &Invocation::new(&["order", "show", "--json"])
            .in_directory(&directory)
            .run(),
    );

    assert_eq!(
        response["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8"),
        "the locator reached the Root four levels up, not the Orders Directory above it"
    );
    assert_eq!(
        response["context"]["order_root"],
        directory.to_str().expect("the test path is UTF-8")
    );
    assert_eq!(
        response["data"]["package_root"], "../../../..",
        "four parent segments, which is what makes this layout different from the default"
    );
    assert_eq!(response["data"]["title"], "Nested order");
}

/// `order show` is the complete read of one Order: its facts, its Items numbered from
/// one, the totals the core calculated, its paths, and its resolved Vendor.
///
/// The defect this catches is an adapter that reported an Order's Items in some order of
/// its own, numbered them from zero, or omitted a fact the record carries — each of which
/// leaves a consumer unable to address an Item for the mutations that follow.
#[test]
fn order_show_reports_the_complete_order_with_one_based_items_and_calculated_totals() {
    let (_tree, root) = populated_package("order-show-complete");
    let directory = order_directory(&root, "Laser_components");

    let response = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    );

    assert_eq!(response["ok"], true);
    assert_eq!(response["contract"], "spielorders.order.show");
    assert_eq!(response["contract_version"], 1);
    assert_eq!(response["data"]["order_id"], NEWEST_ORDER_ID);
    assert_eq!(response["data"]["title"], "Laser components");
    assert_eq!(response["data"]["date_ordered"], "2026-08-22");
    assert_eq!(response["data"]["status"], "in_preparation");
    assert_eq!(response["data"]["orderer"], "Ada Lovelace");
    assert_eq!(response["data"]["ordering_institution"], "Northwind");
    assert_eq!(response["data"]["currency"], "USD");
    assert_eq!(response["data"]["project"], "Telescope");
    assert_eq!(response["data"]["order_group"], "Summer build");
    // The machine response carries Order and Vendor UUIDs whether or not a human surface
    // shows them.
    assert_eq!(response["data"]["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(response["data"]["vendor"]["name"], "Tarnwick");
    assert_eq!(response["data"]["vendor"]["hidden"], false);

    let items = response["data"]["items"]
        .as_array()
        .expect("the Items are a present collection");
    assert_eq!(items.len(), 2);
    // One-based, and in the stored array's own user-controlled order.
    assert_eq!(items[0]["position"], 1);
    assert_eq!(items[0]["description"], "Fiber collimator");
    assert_eq!(items[0]["catalog_number"], "F220APC");
    assert_eq!(items[0]["item_total"], 12.0, "4 x 3.00, calculated");
    assert_eq!(items[1]["position"], 2);
    assert_eq!(items[1]["description"], "Mirror mount");
    assert!(items[1]["catalog_number"].is_null());
    assert_eq!(items[1]["item_total"], 169.0, "2 x 84.50, calculated");
    assert_eq!(response["data"]["order_total"], 181.0);

    assert_eq!(
        response["data"]["paths"]["directory"]["package_relative_path"],
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("Orders/{name}"))
            .expect("the Order Directory has a UTF-8 name"),
        "a location inside a Package carries its Package-relative spelling too"
    );
}

/// `order item list` answers with `order show`'s document, byte for byte.
///
/// Asserted on the bytes rather than on parsed members, because the claim is identity and
/// not agreement: a member added to one and not the other would still agree everywhere a
/// member-by-member comparison thought to look.
#[test]
fn order_item_list_answers_with_order_shows_document() {
    let (_tree, root) = populated_package("order-item-list-shares-the-contract");
    let directory = order_directory(&root, "Laser_components");

    let listing = Invocation::new(&["order", "item", "list", "--json"])
        .rooted_at(&directory)
        .run();
    let show = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&directory)
        .run();

    // Both succeeded, before any claim that they agree: two identical failures would
    // satisfy every assertion below without either command having answered.
    assert_eq!(listing.status, Some(0), "{}", listing.stdout_text());
    assert_eq!(show.status, Some(0), "{}", show.stdout_text());
    assert_eq!(listing.stdout_text(), show.stdout_text());
    assert_eq!(
        response(&listing)["contract"],
        "spielorders.order.show",
        "the sub-list carries the projection's identity, not one of its own"
    );

    // The whole difference between the two commands, now that the document is one: the
    // human surface renders the Items and what they come to, and not the Order's facts.
    let printed = Invocation::new(&["order", "item", "list"])
        .rooted_at(&directory)
        .run();
    let text = printed.stdout_text();
    assert!(text.contains("Order Total: 181.00"), "{text}");
    assert!(!text.contains("Laser components"), "{text}");
    assert!(!text.contains("Status:"), "{text}");
}

/// `--uuid` is the second route in, and it resolves against the current-directory
/// Package rather than against a path the caller names.
///
/// Asserted against the same Order's answer by `--root`, because the two routes must land
/// on one Order: a `--uuid` that resolved against some other Package, or that scanned for
/// a matching record instead of asking the Package's own loaded set, would answer
/// something — and the something would differ from this.
#[test]
fn uuid_resolves_an_order_of_the_current_directory_package() {
    let (_tree, root) = populated_package("order-uuid-resolves");
    let directory = order_directory(&root, "Laser_components");

    let by_identity = response(
        &Invocation::new(&["order", "show", "--json", "--uuid", NEWEST_ORDER_ID])
            .in_directory(&root)
            .run(),
    );
    let by_directory = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    );

    assert_eq!(by_identity["ok"], true);
    assert_eq!(by_identity["data"], by_directory["data"]);
    assert_eq!(by_identity["context"], by_directory["context"]);
}

/// The two routes are two ways of naming one Order, so a command line supplying both is
/// asking about two and is refused before anything is read.
///
/// **Every placement the command line allows.** `--root` is the executable's option and
/// `--uuid` this family's, so each stands in any gap after whatever declares it, and the
/// refusal held only where both landed in one subcommand's own matches. Written the way
/// the top-level help teaches — `spielorders --root … order show` — the command answered
/// instead, having quietly read `--root` as the Package to resolve the identity against.
///
/// A usage error carries the one shared `spielorders.usage` identity and never the
/// command's own: exit `2` exists because nothing about the subject was attempted, so no
/// command shape ran to be identified.
#[test]
fn uuid_together_with_root_is_a_usage_error_in_every_placement() {
    let (_tree, root) = populated_package("order-uuid-and-root");

    // The SpielOrders Root rather than an Order Directory, because that is the value the
    // unenforced placement quietly gave a second meaning: `--root` names the Order
    // Directory for every `order` command, and there it was consumed as the Package the
    // identity resolves against, so the command answered about a real Order.
    for command in [vec!["order", "show"], vec!["order", "item", "list"]] {
        for line in placements(
            &command,
            &WrittenOption::anywhere(&["--root", &root.display().to_string()]),
            &WrittenOption::after_the_family(&["--uuid", NEWEST_ORDER_ID]),
        ) {
            let written: Vec<&str> = line.iter().map(String::as_str).collect();
            let outcome = Invocation::new(&written).argument("--json").run();
            let document = response(&outcome);

            assert_eq!(outcome.status, Some(2), "{line:?}");
            assert_eq!(document["contract"], "spielorders.usage", "{line:?}");
            assert_eq!(document["error"]["code"], "usage_invalid", "{line:?}");
            assert_eq!(document["error"]["scope"], "usage", "{line:?}");
            // Named rather than classified: these two are targeting routes, and a sentence
            // that called them output modes was the defect this pins shut.
            assert_eq!(
                document["error"]["message"],
                "`--root <PATH>` and `--uuid <ORDER_ID>` are mutually exclusive",
                "{line:?}"
            );
        }
    }
}

/// No placement lets a mutation through that another placement refuses.
///
/// The read above is the visible half of the defect and this is the costly one: the
/// position where the exclusion went unnoticed also resolved the Order and **committed**
/// the edit, so which Order a caller changed depended on where they had written `--root`.
#[test]
fn no_placement_of_uuid_beside_root_commits_an_edit() {
    let (_tree, root) = populated_package("order-uuid-and-root-edit");
    let before = managed_snapshot(&root);

    for line in placements(
        &["order", "edit"],
        &WrittenOption::anywhere(&["--root", &root.display().to_string()]),
        &WrittenOption::after_the_family(&["--uuid", NEWEST_ORDER_ID]),
    ) {
        let written: Vec<&str> = line.iter().map(String::as_str).collect();
        let outcome = Invocation::new(&written)
            .argument("project=Telescope")
            .argument("--json")
            .run();

        assert_eq!(outcome.status, Some(2), "{line:?}");
        assert_eq!(
            response(&outcome)["contract"],
            "spielorders.usage",
            "{line:?}"
        );
    }

    assert_eq!(
        managed_snapshot(&root),
        before,
        "a refused command line changes nothing, wherever its options were written"
    );
}

/// An Order ID that names no Order of the Package is a failure about an *identity* and
/// about no path, which is the case the contract's "relevant paths or identities" exists
/// for. This is the first command in the executable to reach it, so it is also where the
/// `identities` shape gets its first caller.
#[test]
fn uuid_naming_no_order_of_this_package_fails_about_the_identity() {
    let (_tree, root) = populated_package("order-uuid-unknown");
    let absent = "00000000-0000-4000-8000-000000000000";

    let outcome = Invocation::new(&["order", "show", "--json", "--uuid", absent])
        .in_directory(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "order_not_found");
    assert_eq!(document["error"]["identities"][0]["kind"], "order");
    assert_eq!(document["error"]["identities"][0]["value"], absent);
    assert_eq!(
        document["error"]["paths"].as_array().map(Vec::len),
        Some(0),
        "an identity that resolves to nothing has no Order Directory to name"
    );
    // The Package it was resolved against is what this answer was about.
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
    assert!(document["context"]["order_root"].is_null());
}

/// `--uuid` against a directory that is not a SpielOrders Root reports the same code a
/// Package-level command does, and says which route the caller actually took.
///
/// The defect this catches is a sentence naming `--root` to a caller who supplied none:
/// the code is shared because the answer is the same, and the wording is not, because the
/// two routes reach it by different means.
#[test]
fn uuid_against_a_directory_that_is_not_a_root_says_which_route_it_took() {
    let (_tree, root) = populated_package("order-uuid-unmanaged");
    let inside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["order", "show", "--json", "--uuid", NEWEST_ORDER_ID])
        .in_directory(&inside)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_unmanaged");
    let message = document["error"]["message"]
        .as_str()
        .expect("a failure carries a human message");
    assert!(message.contains("--uuid"), "{message}");
    assert!(
        !message.contains("--root"),
        "no `--root` was supplied, so naming it would send a reader to the wrong option: \
         {message}"
    );
}

/// A locator that is missing or malformed leaves this Order with no route to a Package at
/// all, and the failure says the locator is why.
///
/// The defect this catches is the one the locator route is otherwise silent about: before
/// the core projection this ticket adds, that route yielded a bare damaged-record failure
/// carrying a path and nothing about the locator, so a user whose Orders Directory had
/// been reconfigured was told only "the record could not be read". Naming the locator is
/// what points them at the one repair that fixes it.
#[test]
fn a_missing_or_malformed_locator_names_the_locator_and_spares_the_sibling() {
    for (case, locator) in [
        ("missing", None),
        ("malformed: absolute", Some("/packages/optics")),
        ("malformed: not parent-only", Some("../Orders")),
        ("malformed: one segment", Some("..")),
    ] {
        let (_tree, _root, broken, sibling) =
            package_with_one_locator_rewritten(&format!("order-locator-unusable-{case}"), locator);

        let outcome = Invocation::new(&["order", "show", "--json"])
            .rooted_at(&broken)
            .run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(1), "{case}");
        assert_eq!(
            document["error"]["code"], "order_locator_unusable",
            "{case}"
        );
        assert_eq!(document["error"]["scope"], "order", "{case}");
        assert!(
            document["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("package_root")),
            "{case}: the locator is named: {}",
            document["error"]["message"]
        );

        // The damage is one Order's. A targeting layer that failed the Package would take
        // every Order in it down with one bad record.
        let unaffected = Invocation::new(&["order", "show", "--json"])
            .rooted_at(&sibling)
            .run();
        assert_eq!(
            unaffected.status,
            Some(0),
            "{case}: {}",
            unaffected.stderr_text()
        );
        assert_eq!(response(&unaffected)["data"]["title"], "Laser components");
    }
}

/// A locator that parses and leads somewhere holding no such Order is stale, and the
/// failure carries the locator's stored value so a reader can see what it says.
#[test]
fn a_stale_locator_carries_its_stored_value_and_spares_the_sibling() {
    let (_tree, _root, broken, sibling) =
        package_with_one_locator_rewritten("order-locator-stale", Some("../../.."));

    let outcome = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&broken)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "order_locator_stale");
    assert!(
        document["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("`../../..`")),
        "the stored locator is quoted back: {}",
        document["error"]["message"]
    );
    // The Order Directory did resolve — resolving it is how the locator could be read at
    // all — and no Package was reached, so the context names one root and not the other.
    assert_eq!(
        document["context"]["order_root"],
        broken.to_str().expect("the test path is UTF-8")
    );
    assert!(document["context"]["package_root"].is_null());
    assert_eq!(document["error"]["paths"][0]["spelling"], "resolved");

    let unaffected = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&sibling)
        .run();
    assert_eq!(unaffected.status, Some(0), "{}", unaffected.stderr_text());
}

/// A locator naming a *different* real Package is the mismatched case, and it answers the
/// same way a stale one does: the place it names holds no such Order.
///
/// The defect this catches is the worst one a targeting layer can have — reading, or
/// worse mutating, a stranger's Package because a locator pointed at it. M3 shipped
/// exactly that: a repair that counted two parents to find its Package and refreshed a
/// stranger's directory under a nested layout.
#[test]
fn a_locator_naming_another_package_is_refused_rather_than_followed() {
    let (_tree, outer, _inner, directory) =
        order_whose_locator_names_another_package("order-locator-mismatched");

    let outcome = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&directory)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "order_locator_stale");
    assert!(
        document["context"]["package_root"].is_null(),
        "the Package the locator named is not this Order's, so it is not reported as one"
    );
    // And the stranger is untouched: it still holds no Orders of its own.
    let stranger = response(
        &Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(&outer)
            .run(),
    );
    assert_eq!(stranger["data"]["orders"].as_array().map(Vec::len), Some(0));
}

/// An Order with no Items answers with a present, empty collection and a present total.
///
/// The defect this catches is an omitted key: a consumer written against every other
/// Order would have to null-check `items` and `order_total` for this one, and once one
/// command omits an empty collection the rule is gone. The total's bytes are `-0.0`
/// rather than `0.0` because the core sums Item Totals with `Iterator::sum` and `f64`'s
/// `Sum` folds from `-0.0` — the number the core calculated, reported as it calculated
/// it. Displayed money is a separate question and reads `0.00`.
#[test]
fn an_order_with_no_items_reports_an_empty_collection_and_a_present_total() {
    let (_tree, root) = package_with_one_order_priced("order-no-items", &[]);
    let directory = order_directory(&root, "Priced_order");

    for command in [
        vec!["order", "show", "--json"],
        vec!["order", "item", "list", "--json"],
    ] {
        let contract = command.join(" ");
        let outcome = Invocation::new(&command).rooted_at(&directory).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{contract}");
        assert_eq!(document["contract"], "spielorders.order.show");
        assert_eq!(
            document["data"]["items"].as_array().map(Vec::len),
            Some(0),
            "{contract}: present and empty, not omitted"
        );
        assert!(
            document["data"].get("order_total").is_some(),
            "{contract}: the total is present"
        );
        // Asserted on the bytes, because that is the claim: `-0.0` reads back through a
        // JSON parse as a float equal to `0.0`, so comparing values would pass either way.
        assert!(
            outcome.stdout_text().contains("\"order_total\": -0.0"),
            "{contract}: {}",
            outcome.stdout_text()
        );
    }

    // And the human surface renders the same number as `0.00`: zero has no sign, whichever
    // zero it is.
    let human = Invocation::new(&["order", "show"])
        .rooted_at(&directory)
        .run();
    assert!(
        human.stdout_text().contains("Order Total: 0.00"),
        "{}",
        human.stdout_text()
    );
}

/// A Damaged Order answers with the decided failure status and its typed error rather
/// than a partially invented projection.
///
/// `data` is `null`: an Order whose record did not validate has no facts, and reporting a
/// projection with some of them filled in would be inventing the rest. Its diagnostics
/// are the load's own, selected by the core from the groups the core made — nothing in
/// the adapter constructs a grouping, and there is no `diagnostics` member inside a
/// placeholder because there is no placeholder.
#[test]
fn a_damaged_order_answers_with_its_typed_error_and_the_loads_own_diagnostics() {
    let (_tree, root, damaged) = package_with_an_impossible_date("order-damaged-order");

    let outcome = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&damaged)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["ok"], false);
    assert!(document["data"].is_null());
    assert_eq!(document["error"]["code"], "order_record_damaged");
    assert_eq!(document["error"]["scope"], "order");
    assert_eq!(document["error"]["identities"][0]["kind"], "order");
    assert_eq!(document["error"]["identities"][0]["value"], OLDEST_ORDER_ID);
    // Both roots: the Order Directory was located, and the Package that quarantined it is
    // the Package this answer came from.
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
    assert_eq!(
        document["context"]["order_root"],
        damaged.to_str().expect("the test path is UTF-8")
    );
    // This Order's own diagnosis, in the only channel a failed read has.
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a present collection");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "order_semantic_invalid");
    assert_eq!(diagnostics[0]["scope"], "order");
    assert_eq!(diagnostics[0]["evidence"]["field"], "date_ordered");
    assert_eq!(diagnostics[0]["evidence"]["order_id"], OLDEST_ORDER_ID);
}

/// An Order record that is not JSON at all never reaches a Package, so it has no
/// diagnostics to report — and says so with an empty collection rather than omitting one.
#[test]
fn an_unreadable_order_record_answers_without_inventing_a_diagnosis() {
    let (_tree, _root, damaged) = package_with_a_damaged_order("order-unreadable-record");

    let outcome = Invocation::new(&["order", "item", "list", "--json"])
        .rooted_at(&damaged)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "order_record_damaged");
    assert_eq!(
        document["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "the locator lives in the record that would not read, so no load happened"
    );
    assert!(
        document["context"]["package_root"].is_null(),
        "no Package was reached"
    );
}

/// The one warning an `order` read can produce: the Order is healthy and its README is a
/// document that cannot be read.
///
/// The answer stands and the observation stands beside it, which is what makes this a
/// success carrying a diagnostic rather than a failure. All three commands report it,
/// because all three read one Order and the envelope is where a response says what its
/// read observed.
#[test]
fn an_unreadable_order_readme_is_a_warning_beside_the_answer() {
    let (_tree, _root, directory) =
        package_with_an_unreadable_order_readme("order-readme-unreadable");

    for command in [
        vec!["order", "show", "--json"],
        vec!["order", "readme", "--json"],
        vec!["order", "item", "list", "--json"],
    ] {
        let outcome = Invocation::new(&command).rooted_at(&directory).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{command:?}");
        assert_eq!(document["ok"], true, "{command:?}");
        let diagnostics = document["diagnostics"]
            .as_array()
            .expect("diagnostics is a present collection");
        assert_eq!(diagnostics.len(), 1, "{command:?}: {diagnostics:?}");
        assert_eq!(diagnostics[0]["code"], "readme_unavailable", "{command:?}");
        assert_eq!(diagnostics[0]["severity"], "warning", "{command:?}");
        assert_eq!(
            diagnostics[0]["evidence"]["document_kind"], "order_readme",
            "{command:?}"
        );
    }

    // `order readme` still answers the conventional path, and reports that something
    // stands there: the path is the question, and the document's readability is not.
    let document = response(
        &Invocation::new(&["order", "readme", "--json"])
            .rooted_at(&directory)
            .run(),
    );
    assert_eq!(document["data"]["exists"], true);

    // A warning goes to standard error while the result goes to standard output, in both
    // human modes.
    let human = Invocation::new(&["order", "show"])
        .rooted_at(&directory)
        .run();
    assert!(
        human.stderr_text().contains("is not a readable document"),
        "{}",
        human.stderr_text()
    );
}

/// The Items and the totals appear in the human surface too, numbered from one and
/// rendered to two decimal places.
///
/// The defect this catches is a human surface that numbered from zero, or that showed a
/// total of `12` where the machine response carries `12.0` — a column of totals a reader
/// cannot align. The money goes through the crate's one renderer; the numbers themselves
/// are untouched, which the machine assertions above pin.
#[test]
fn the_human_surface_numbers_the_items_and_renders_their_totals() {
    let (_tree, root) = populated_package("order-human-items");
    let directory = order_directory(&root, "Laser_components");

    for command in [vec!["order", "show"], vec!["order", "item", "list"]] {
        let outcome = Invocation::new(&command).rooted_at(&directory).run();
        let printed = outcome.stdout_text();
        assert_eq!(outcome.status, Some(0), "{command:?}");
        assert!(
            printed.contains("Item: 1. Fiber collimator  4.00 x 3.00  12.00"),
            "{command:?}: {printed}"
        );
        assert!(
            printed.contains("Item: 2. Mirror mount  2.00 x 84.50  169.00"),
            "{command:?}: {printed}"
        );
        assert!(
            printed.contains("Order Total: 181.00"),
            "{command:?}: {printed}"
        );
        // UUIDs are held back to `--verbose` in both, so a human reading an Order sees
        // its titles and names.
        assert!(!printed.contains(NEWEST_ORDER_ID), "{command:?}: {printed}");
    }
}

/// `--quiet` says nothing on success and `--verbose` adds the detail, for all three.
///
/// The four output modes are one set, so a command that answered under `--quiet` or that
/// leaked its result into the wrong stream would be a hole in a rule fifteen other
/// commands keep.
#[test]
fn the_four_output_modes_hold_for_every_order_read() {
    let (_tree, root) = populated_package("order-output-modes");
    let directory = order_directory(&root, "Laser_components");

    // `order readme`'s payload is the three facts the decision enumerates for it — the
    // absolute path, the relevant relative path, and existence — so it carries no Order
    // identity to hold back, and `context.order_root` is what attributes its answer.
    for (command, names_the_order) in [
        (vec!["order", "show"], true),
        (vec!["order", "readme"], false),
        (vec!["order", "item", "list"], true),
    ] {
        let quiet = Invocation::new(&command)
            .argument("--quiet")
            .rooted_at(&directory)
            .run();
        assert_eq!(quiet.status, Some(0), "{command:?}");
        assert_eq!(quiet.stdout, b"", "{command:?}: {}", quiet.stdout_text());
        assert_eq!(quiet.stderr, b"", "{command:?}: {}", quiet.stderr_text());

        let verbose = Invocation::new(&command)
            .argument("--verbose")
            .rooted_at(&directory)
            .run();
        assert_eq!(verbose.status, Some(0), "{command:?}");
        // Both roots are `--verbose` detail, because which Order and which Package a read
        // is of is the same detail for all three.
        assert!(
            verbose
                .stdout_text()
                .contains(root.to_str().expect("the test root is UTF-8")),
            "{command:?}: {}",
            verbose.stdout_text()
        );
        assert_eq!(
            verbose.stdout_text().contains(NEWEST_ORDER_ID),
            names_the_order,
            "{command:?}: an Order's UUID is `--verbose` detail wherever the response \
             carries one: {}",
            verbose.stdout_text()
        );
    }
}
