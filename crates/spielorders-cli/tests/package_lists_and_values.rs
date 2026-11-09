//! `package list orders`, `package list vendors`, and the four `package values`
//! commands, tested as one set.
//!
//! **The six are narrower views of the projection `package show` already presents**, so
//! the defect this file exists to catch is a disagreement *between* them, or between one
//! of them and `package show`, on the four things each could answer differently: Vendor
//! visibility, Damaged Order representation, ordering, and empty-collection shape. Such a
//! disagreement appears in no single command's diff, which is why they are inventoried
//! here together rather than in six files.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics; Define the Order record and lifecycle contract.

use crate::support::{
    Invocation, directory_that_is_not_a_root, hidden_vendor_id, initialized_package,
    newest_order_id, offered_vendor_id, package_at_an_uninterpretable_version,
    package_root_that_cannot_be_read, package_with_a_damaged_order,
    package_with_duplicate_vendor_names, populated_package, response,
};

// ---------------------------------------------------------------------------
// `package list orders`
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// `package list vendors`
// ---------------------------------------------------------------------------

/// The decided default: the Vendors this Package offers for new use.
///
/// **Asserted against `package show` at the same presentation**, because the criterion is
/// that the two agree exactly. A default that omitted nothing, or that reported a Hidden
/// Vendor without saying so, is a listing a user would act on to place a new Order.
#[test]
fn the_vendor_listing_omits_hidden_vendors_by_default_exactly_as_package_show_does() {
    let (_tree, root) = populated_package("list-vendors-default");

    let outcome = Invocation::new(&["package", "list", "vendors", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);
    let vendors = document["data"]
        .as_array()
        .expect("the listing is a list of Vendors");

    assert_eq!(outcome.status, Some(0));
    assert_eq!(vendors.len(), 1, "the Hidden Vendor is not offered");
    assert_eq!(vendors[0]["vendor_id"], &*offered_vendor_id(&root));
    assert_eq!(vendors[0]["name"], "Tarnwick");
    assert!(
        vendors[0]
            .get("hidden")
            .is_none_or(|hidden| hidden == "false"),
        "an offered Vendor is not hidden: {}",
        vendors[0]
    );

    // `package show` carries every Vendor with its visibility; the ones it offers are
    // exactly the ones this listing reports, and its human Vendor list offers the same.
    let shown = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let offered: Vec<&serde_json::Value> = shown["data"]["vendors"]
        .as_array()
        .expect("package show carries a Vendors collection")
        .iter()
        .filter(|vendor| vendor["hidden"] != "true")
        .collect();
    assert_eq!(offered, vendors.iter().collect::<Vec<_>>());
    let summary = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    assert!(
        !summary.stdout_text().contains("name: Nethermoor"),
        "{}",
        summary.stdout_text()
    );
}

/// `--all` reports every Vendor, each carrying its visibility.
#[test]
fn the_vendor_listing_includes_hidden_vendors_with_their_visibility_under_all() {
    let (_tree, root) = populated_package("list-vendors-all");

    let document = response(
        &Invocation::new(&["package", "list", "vendors", "--json", "--all"])
            .rooted_at(&root)
            .run(),
    );
    let vendors = document["data"]
        .as_array()
        .expect("the listing is a list of Vendors");

    assert_eq!(vendors.len(), 2, "the complete list");
    let hidden: Vec<&serde_json::Value> = vendors
        .iter()
        .filter(|vendor| vendor["vendor_id"] == *hidden_vendor_id(&root))
        .collect();
    assert_eq!(hidden.len(), 1, "the Hidden Vendor is present");
    assert_eq!(
        hidden[0]["hidden"], "true",
        "and it says so, rather than being reported as any other Vendor"
    );
}

// ---------------------------------------------------------------------------
// `package values <kind>`
// ---------------------------------------------------------------------------

/// The four kinds, each reporting its own list under its own contract.
///
/// One test over all four rather than four tests, because the claim is that the four
/// agree: same payload shape, same collection key, and each about the list its identity
/// names. A kind wired to the wrong core list is the defect, and it is invisible in any
/// one of them.
#[test]
fn each_value_kind_reports_its_own_list_under_its_own_contract() {
    let (_tree, root) = populated_package("values-kinds");

    for (kind, expected) in [
        ("project", vec![("Telescope", 2)]),
        ("orderer", vec![("Ada Lovelace", 2)]),
        ("ordering_institution", vec![("Northwind", 1)]),
        ("order_group", vec![("Summer build", 1)]),
    ] {
        let outcome = Invocation::new(&["package", "values", kind, "--json"])
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(0), "{kind}");
        assert_eq!(
            document["contract"],
            format!("spielorders.package.values.{kind}"),
            "{kind} answers under its own identity"
        );
        let reported: Vec<(&str, u64)> = document["data"]
            .as_array()
            .unwrap_or_else(|| panic!("{kind} carries a values collection"))
            .iter()
            .map(|value| {
                (
                    value["value"].as_str().expect("a canonical spelling"),
                    value["usage_count"]
                        .as_str()
                        .and_then(|count| count.parse().ok())
                        .expect("a usage count"),
                )
            })
            .collect();
        let expected: Vec<(&str, u64)> = expected
            .into_iter()
            .map(|(value, count)| (value, count as u64))
            .collect();
        assert_eq!(reported, expected, "{kind}");
    }
}

/// Every one of the six reports the same lists `package show` does.
///
/// The whole review boundary in one assertion: six narrower views of one projection, each
/// compared against the complete read of the same Package. A command that recomputed a
/// value list, re-sorted the Orders, or resolved a Vendor of its own would disagree here
/// and nowhere else.
///
/// **Over two Packages, because neither alone exercises everything the six could differ
/// about.** The populated one holds two healthy Orders, the second referencing a Hidden
/// Vendor, so a listing that resolved against the presented list disagrees here. The
/// damaged one holds a Damaged Order, so the placeholder, the partial value lists, and
/// the diagnostics are exercised. With only the second, the one healthy Order left names
/// an offered Vendor and the resolution rule would be invisible.
#[test]
fn the_six_narrower_reads_agree_with_the_complete_one() {
    let (_tree, root) = populated_package("six-agree-with-show-populated");
    the_six_agree_with_package_show(&root);

    let (_tree, root, _damaged) = package_with_a_damaged_order("six-agree-with-show-damaged");
    the_six_agree_with_package_show(&root);
}

fn the_six_agree_with_package_show(root: &std::path::Path) {
    let shown = response(
        &Invocation::new(&["package", "show", "--json", "--all"])
            .rooted_at(root)
            .run(),
    );

    let listed_orders = response(
        &Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(root)
            .run(),
    );
    assert_eq!(
        listed_orders["data"], shown["data"]["orders"],
        "every Order, healthy or damaged, reported identically"
    );

    let listed_vendors = response(
        &Invocation::new(&["package", "list", "vendors", "--json", "--all"])
            .rooted_at(root)
            .run(),
    );
    let shown_vendors = shown["data"]["vendors"]
        .as_array()
        .expect("package show carries a Vendors collection");
    let vendors = listed_vendors["data"]
        .as_array()
        .expect("the listing is a list of Vendors");
    assert_eq!(vendors.len(), shown_vendors.len());
    for (listed, shown) in vendors.iter().zip(shown_vendors) {
        assert_eq!(listed, shown, "one Vendor, reported identically");
    }

    for kind in ["project", "orderer", "ordering_institution", "order_group"] {
        let document = response(
            &Invocation::new(&["package", "values", kind, "--json"])
                .rooted_at(root)
                .run(),
        );
        assert_eq!(
            document["data"], shown["data"]["values"][kind],
            "{kind} reports the list `package show` reports under `{kind}`"
        );
    }
}

// ---------------------------------------------------------------------------
// The six as one set: load states, diagnostics, human output, usage errors.
// ---------------------------------------------------------------------------

/// Every command line this ticket delivers, as arguments after `package`.
///
/// The six are inventoried from this one list, so a command added to the family without a
/// thought for the rules below is not quietly exempt from them.
const SIX: [&[&str]; 6] = [
    &["list", "orders"],
    &["list", "vendors"],
    &["values", "project"],
    &["values", "orderer"],
    &["values", "ordering_institution"],
    &["values", "order_group"],
];

fn run_six(root: &std::path::Path, extra: &[&str]) -> Vec<(String, crate::support::Outcome)> {
    SIX.iter()
        .map(|command| {
            let mut arguments = vec!["package"];
            arguments.extend_from_slice(command);
            arguments.extend_from_slice(extra);
            (
                command.join(" "),
                Invocation::new(&arguments).rooted_at(root).run(),
            )
        })
        .collect()
}

/// Each load state produces one answer, and all six commands give the same one.
///
/// **Asserted against `package show`'s answer for the same Package**, because these are
/// narrower views of what that command reports: a Package whose contents cannot be
/// trusted has no contents for any of the seven to report, and a command that answered
/// `0` where its siblings answer `1` would be a consumer's inconsistency rather than a
/// Package's.
#[test]
fn every_load_state_answers_the_six_exactly_as_it_answers_package_show() {
    let states: [(&str, (crate::support::TemporaryTree, std::path::PathBuf)); 3] = [
        (
            "damaged_package",
            package_with_duplicate_vendor_names("six-load-damaged"),
        ),
        (
            "package_metadata_unsupported",
            package_at_an_uninterpretable_version("six-load-unsupported"),
        ),
        (
            "package_operation_failed",
            package_root_that_cannot_be_read("six-load-unreadable"),
        ),
    ];

    for (code, (_tree, root)) in &states {
        let shown = response(
            &Invocation::new(&["package", "show", "--json"])
                .rooted_at(root)
                .run(),
        );
        assert_eq!(shown["error"]["code"], *code, "the fixture reaches {code}");

        for (name, outcome) in run_six(root, &["--json"]) {
            let document = response(&outcome);
            assert_eq!(outcome.status, Some(1), "{name}: {code}");
            assert_eq!(document["error"]["code"], *code, "{name}");
            assert_eq!(
                document["error"]["detail"], shown["error"]["detail"],
                "{name}"
            );
            assert_eq!(
                document["context"]["root"], shown["context"]["root"],
                "{name}: the Root the command was about"
            );
            assert!(document["data"].is_null(), "{name}");
        }
    }

    // The one state whose *directory* resolves without being a Root, kept separate
    // because it is built from a Package rather than being one.
    let (_tree, root) = initialized_package("six-load-unmanaged");
    let outside = directory_that_is_not_a_root(&root);
    let shown = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&outside)
            .run(),
    );
    for (name, outcome) in run_six(&outside, &["--json"]) {
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(1), "{name}");
        assert_eq!(document["error"]["code"], "unmanaged_package", "{name}");
        assert_eq!(
            document["error"], shown["error"],
            "{name}: the unmanaged directory is named exactly as `package show` names it"
        );
    }
}

/// The human summary names the same entities the machine response carries.
///
/// Names rather than sentences: human output is not a parsing contract, so what is pinned
/// is that the summary is *about* the same Package. The defect it catches is a human mode
/// that quietly reports a different set — Vendors filtered one way in JSON and another in
/// the summary would agree on nothing and still print plausibly.
#[test]
fn the_human_listings_name_the_same_entities_as_the_machine_responses() {
    let (_tree, root) = populated_package("six-human");

    let orders = Invocation::new(&["package", "list", "orders"])
        .rooted_at(&root)
        .run();
    for named in ["Laser components", "Mirror mounts"] {
        assert!(
            orders.stdout_text().contains(named),
            "the Order listing names {named}:\n{}",
            orders.stdout_text()
        );
    }

    let vendors = Invocation::new(&["package", "list", "vendors"])
        .rooted_at(&root)
        .run();
    assert!(vendors.stdout_text().contains("Tarnwick"));
    assert!(
        !vendors.stdout_text().contains("Nethermoor"),
        "the default omits the Hidden Vendor here exactly as it does in JSON:\n{}",
        vendors.stdout_text()
    );

    let values = Invocation::new(&["package", "values", "project"])
        .rooted_at(&root)
        .run();
    assert!(
        values
            .stdout_text()
            .contains("- value: Telescope\n  usage count: 2\n"),
        "the value list names the canonical spelling and its count:\n{}",
        values.stdout_text()
    );
}

/// Identities are `--verbose` detail in human output and always present in JSON.
#[test]
fn uuids_are_verbose_only_in_the_human_listings() {
    let (_tree, root) = populated_package("six-human-uuids");

    for (name, outcome) in run_six(&root, &[]) {
        for identity in [&*newest_order_id(&root), &*offered_vendor_id(&root)] {
            assert!(
                !outcome.stdout_text().contains(identity),
                "{name}: the default summary carries no UUID:\n{}",
                outcome.stdout_text()
            );
        }
    }

    let verbose = Invocation::new(&["package", "list", "orders", "--verbose"])
        .rooted_at(&root)
        .run();
    assert!(
        verbose.stdout_text().contains(&*newest_order_id(&root)),
        "`--verbose` is where they appear:\n{}",
        verbose.stdout_text()
    );
    let verbose = Invocation::new(&["package", "list", "vendors", "--verbose"])
        .rooted_at(&root)
        .run();
    assert!(
        verbose.stdout_text().contains(&*offered_vendor_id(&root)),
        "for Vendors too:\n{}",
        verbose.stdout_text()
    );
}

/// **A usage error carries `spielorders.usage`, never one of these six identities.**
///
/// The rule a new command is most likely to break, and this ticket registers six
/// contracts to the previous one's one. Every way each of them can reach exit `2` is
/// listed, including the value kind that does not exist.
#[test]
fn every_usage_error_from_the_six_carries_the_usage_contract() {
    let (_tree, root) = initialized_package("six-usage-identity");

    let mut lines: Vec<Vec<&str>> = Vec::new();
    for command in SIX {
        let mut misspelled = vec!["package"];
        misspelled.extend_from_slice(command);
        misspelled.extend_from_slice(&["--json", "--jsom"]);
        lines.push(misspelled);

        let mut two_modes = vec!["package"];
        two_modes.extend_from_slice(command);
        two_modes.extend_from_slice(&["--json", "--quiet"]);
        lines.push(two_modes);
    }
    lines.extend([
        vec!["package", "list", "--json"],
        vec!["package", "list", "vendros", "--json"],
        vec!["package", "lsit", "orders", "--json"],
        vec!["package", "values", "--json"],
        vec!["package", "values", "vendor", "--json"],
        vec!["package", "values", "project", "--json", "--all"],
        vec!["package", "list", "orders", "--json", "--all"],
    ]);

    for arguments in lines {
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(2), "{arguments:?}");
        assert_eq!(
            document["contract"], "spielorders.usage",
            "{arguments:?}: no command ran, so no command's contract is claimed"
        );
        assert_eq!(document["error"]["code"], "usage_invalid", "{arguments:?}");
    }
}

/// **There is no `order list`.**
///
/// `package list` owns Package collections, which is what lets every `order` command keep
/// Order-root semantics. A decided absence has no behavior to test except that asking for
/// it fails, so that is what is asserted.
#[test]
fn there_is_no_order_list() {
    let (_tree, root) = populated_package("no-order-list");

    for arguments in [
        vec!["order", "list", "--json"],
        vec!["order", "list", "orders", "--json"],
    ] {
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        assert_eq!(outcome.status, Some(2), "{arguments:?}");
        assert_eq!(
            response(&outcome)["contract"],
            "spielorders.usage",
            "{arguments:?}"
        );
    }
}

/// Every value kind is iterated, not just every one that has a contract.
///
/// The four categories, in `package show`'s published `values` payload order, which is
/// also the order the parser offers them in — held by the parser's own unit tests. Each
/// has its own command and contract, and `package show`'s human summary prints a line
/// for each: a kind missing from the summary while its own `package values` command
/// worked is the quiet half of the pair, and the half nothing else binds.
#[test]
fn every_value_kind_is_iterated() {
    let declared = ["ordering_institution", "orderer", "project", "order_group"];

    let (_tree, root) = populated_package("value-kind-coverage");
    for kind in declared {
        let document = response(
            &Invocation::new(&["package", "values", kind, "--json"])
                .rooted_at(&root)
                .run(),
        );
        assert_eq!(
            document["contract"],
            format!("spielorders.package.values.{kind}"),
            "{kind} has its own contract"
        );
    }

    // What "iterated" means from outside the executable is the sequence of labels
    // `package show`'s summary prints — one line per kind, whether or not that kind has
    // any value, so a kind missing from the array is a missing line rather than a line
    // that happens to be empty. This is also the assertion the first
    // review pass's own regression needed and did not have: reordering `VALUE_KINDS`
    // reordered a closed command's summary with the whole suite green.
    let summary = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    let printed: Vec<&str> = summary
        .stdout_text()
        .lines()
        .skip_while(|line| *line != "values:")
        .skip(1)
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.trim_start().split(':').next())
        .collect();
    assert_eq!(
        printed,
        vec!["ordering institution", "orderer", "project", "order group"],
        "every kind is printed, once, in the published order:\n{}",
        summary.stdout_text()
    );
}
