//! The three `order` read commands and every way one of them arrives at a Package.
//!
//! **Targeting is the subject as much as the reads are.** For an `order` command
//! `--root` names the Order Directory rather than the SpielOrders Root, the Package is
//! reached through the Order's own `package_root` locator, and no ancestor is ever
//! searched. Counting parents to find the Package would refresh a stranger's directory
//! under a nested layout, and would be wrong for every `order` command at once, so the
//! nested layout is exercised directly rather than assumed to behave like the default
//! one.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record
//! and lifecycle contract; Define durable metadata schemas and version evolution.

use crate::support::{
    Invocation, directory_that_is_not_a_root, newest_order_id, order_directory, populated_package,
    response,
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
            outcome
                .stderr_text()
                .contains("error: not an order directory"),
            "{command:?}: {}",
            outcome.stderr_text()
        );
    }
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
    // human surface renders the Items, and not the Order's facts.
    let printed = Invocation::new(&["order", "item", "list"])
        .rooted_at(&directory)
        .run();
    let text = printed.stdout_text();
    assert!(text.contains("1. description: Fiber collimator"), "{text}");
    assert!(text.contains("2. description: Mirror mount"), "{text}");
    assert!(!text.contains("Laser components"), "{text}");
    assert!(!text.contains("status:"), "{text}");
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
        &Invocation::new(&[
            "order",
            "show",
            "--json",
            "--uuid",
            &*newest_order_id(&root),
        ])
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

    let outcome = Invocation::new(&[
        "order",
        "show",
        "--json",
        "--uuid",
        &*newest_order_id(&root),
    ])
    .in_directory(&inside)
    .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "unmanaged_package");
    assert_eq!(
        document["error"]["detail"]["path"],
        inside.to_str().expect("the test path is UTF-8")
    );
    let human = Invocation::new(&["order", "show", "--uuid", &*newest_order_id(&root)])
        .in_directory(&inside)
        .run();
    assert_eq!(human.status, Some(1));
    assert!(
        !human.stderr_text().contains("--root"),
        "no `--root` was supplied, so naming it would send a reader to the wrong option: \
         {}",
        human.stderr_text()
    );
}

/// The Items and the totals appear in the human surface too, numbered from one and with
/// every monetary value rendered to two decimal places.
///
/// The defect this catches is a human surface that numbered from zero, or that showed a
/// total of `12` — a column of money a reader cannot align. The Quantity beside it is not
/// money and reads as the record stores it.
#[test]
fn the_human_surface_numbers_the_items_and_renders_their_totals() {
    let (_tree, root) = populated_package("order-human-items");
    let directory = order_directory(&root, "Laser_components");

    for command in [vec!["order", "show"], vec!["order", "item", "list"]] {
        let outcome = Invocation::new(&command).rooted_at(&directory).run();
        let printed = outcome.stdout_text();
        assert_eq!(outcome.status, Some(0), "{command:?}");
        let lines: Vec<&str> = printed.lines().map(str::trim_start).collect();
        let first = lines
            .iter()
            .position(|line| *line == "1. description: Fiber collimator")
            .unwrap_or_else(|| panic!("{command:?}: the first Item is listed: {printed}"));
        let second = lines
            .iter()
            .position(|line| *line == "2. description: Mirror mount")
            .unwrap_or_else(|| panic!("{command:?}: the second Item is listed: {printed}"));
        assert!(first < second, "{command:?}: in stored order: {printed}");
        assert_eq!(
            lines[first + 1..first + 5],
            [
                "quantity: 4",
                "unit price: 3.00",
                "catalog number: F220APC",
                "total: 12.00"
            ],
            "{command:?}: {printed}"
        );
        assert_eq!(
            lines[second + 1..second + 4],
            ["quantity: 2", "unit price: 84.50", "total: 169.00"],
            "{command:?}: {printed}"
        );
        // UUIDs are held back to `--verbose` in both, so a human reading an Order sees
        // its titles and names.
        assert!(
            !printed.contains(&*newest_order_id(&root)),
            "{command:?}: {printed}"
        );
    }

    // The Order Total is rendered as money where the Order is shown.
    let shown = Invocation::new(&["order", "show"])
        .rooted_at(&directory)
        .run();
    let printed = shown.stdout_text();
    assert!(printed.contains("\ntotal: 181.00\n"), "{printed}");
}

/// `--verbose` adds which Order and which Package a read is of: the Package Root wherever a
/// read renders the Order, and the Order's UUID wherever the response carries one.
///
/// `order readme`'s payload is the three facts the decision enumerates for it — the
/// absolute path, the relevant relative path, and existence — so it carries no Order
/// identity to hold back, and its absolute path is what attributes its answer.
#[test]
fn verbose_adds_the_roots_and_the_order_identity_to_the_order_reads() {
    let (_tree, root) = populated_package("order-output-modes");
    let directory = order_directory(&root, "Laser_components");

    for (command, names_the_order) in [
        (vec!["order", "show"], true),
        (vec!["order", "readme"], false),
    ] {
        let verbose = Invocation::new(&command)
            .argument("--verbose")
            .rooted_at(&directory)
            .run();
        assert_eq!(verbose.status, Some(0), "{command:?}");
        assert!(
            verbose
                .stdout_text()
                .contains(root.to_str().expect("the test root is UTF-8")),
            "{command:?}: {}",
            verbose.stdout_text()
        );
        assert_eq!(
            verbose.stdout_text().contains(&*newest_order_id(&root)),
            names_the_order,
            "{command:?}: {}",
            verbose.stdout_text()
        );
    }
}

/// `--uuid` finds a Damaged Order by its identity, so the read says what damaged it.
#[test]
fn uuid_naming_a_damaged_order_reports_the_damage() {
    let (_tree, root, damaged) = crate::support::package_with_an_impossible_date("uuid-damaged");
    let identity = std::fs::read_to_string(damaged.join(".spielorders/order.json"))
        .expect("the record can be read")
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("\"order_id\": \"")
                .and_then(|rest| rest.split('"').next())
                .map(str::to_owned)
        })
        .expect("the record names its identity");

    let outcome = Invocation::new(&["order", "show", "--json", "--uuid", &identity])
        .in_directory(&root)
        .run();

    assert_eq!(outcome.status, Some(1));
    assert_eq!(response(&outcome)["error"]["code"], "damaged_order");
}
