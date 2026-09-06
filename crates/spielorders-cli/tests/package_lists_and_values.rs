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
    HIDDEN_VENDOR_ID, Invocation, NEWEST_ORDER_ID, OFFERED_VENDOR_ID, directory_that_is_not_a_root,
    initialized_package, package_at_an_uninterpretable_version, package_root_that_cannot_be_read,
    package_with_a_damaged_order, package_with_a_differently_spelled_value,
    package_with_duplicate_vendor_names, package_with_every_order_damaged, populated_package,
    response,
};

// ---------------------------------------------------------------------------
// `package list orders`
// ---------------------------------------------------------------------------

/// Every Order the Package holds, with its identity and its Order Directory root.
#[test]
fn the_order_listing_names_every_order_with_its_identity_and_root() {
    let (_tree, root) = populated_package("list-orders-populated");

    let outcome = Invocation::new(&["package", "list", "orders", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);
    let orders = document["data"]["orders"]
        .as_array()
        .expect("the listing carries an Orders collection");

    assert_eq!(outcome.status, Some(0));
    assert_eq!(orders.len(), 2, "both Orders are listed");
    assert_eq!(orders[0]["order_id"], NEWEST_ORDER_ID);
    assert_eq!(orders[0]["title"], "Laser components");
    assert_eq!(orders[0]["date_ordered"], "2026-08-22");
    assert_eq!(orders[0]["status"], "in_preparation");
    assert_eq!(
        orders[0]["directory"]["package_relative_path"],
        "Orders/2026_08_22_Tarnwick_Laser_components_Ada_Lovelace",
        "the Order Directory root, in the Package-relative spelling every location carries"
    );
    assert!(
        orders[0]["directory"]["absolute_path"]
            .as_str()
            .expect("a location carries an absolute path")
            .starts_with(root.to_str().expect("the test root is UTF-8")),
        "and the absolute one beside it"
    );
}

/// The Vendor an Order names, resolved by the core against the complete Vendor list.
///
/// The defect: resolving against the *presented* list, which would make the second
/// Order's Vendor vanish because it is hidden. `package list orders` has no `--all` at
/// all, because hiding governs what is offered for new use and never what is resolved —
/// so this listing is where that rule is either kept or silently broken.
#[test]
fn a_listed_order_resolves_its_vendor_whether_or_not_the_vendor_is_hidden() {
    let (_tree, root) = populated_package("list-orders-hidden-vendor");

    let document = response(
        &Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let orders = &document["data"]["orders"];

    assert_eq!(orders[0]["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(orders[0]["vendor"]["name"], "Tarnwick");
    assert_eq!(orders[0]["vendor"]["hidden"], false);

    assert_eq!(orders[1]["vendor_id"], HIDDEN_VENDOR_ID);
    assert_eq!(
        orders[1]["vendor"]["name"], "Nethermoor",
        "the Hidden Vendor an Order references still resolves its facts"
    );
    assert_eq!(orders[1]["vendor"]["hidden"], true);
}

/// The decided default ordering, which is the core's: newest Date Ordered first.
///
/// Asserted against the *core's own* sequence rather than against a literal, so the test
/// states the real claim — the adapter presents the core's ordering and computes none of
/// its own. A literal list would pass just as well for an adapter that happened to sort
/// the same way today and would say nothing about which of them decided it.
///
/// **Two Packages, because one Package cannot make both halves mean anything.** The
/// healthy half needs at least two healthy Orders whose core sequence differs from their
/// directory names, which is [`populated_package`]: it holds them newest first while
/// their directory names sort oldest first. The Damaged half needs two Damaged Orders,
/// and a Package with two of those has no healthy ones left.
#[test]
fn the_order_listing_presents_the_orderings_the_core_decided() {
    use spielorders_core::canonical::{CanonicalPackageLoad, inspect_canonical_package};

    fn loaded(root: &std::path::Path) -> spielorders_core::canonical::CurrentPackageCandidate {
        let CanonicalPackageLoad::CurrentCandidate(package) =
            inspect_canonical_package(root, &spielorders_cli::effects::PlatformFileSystem)
        else {
            panic!("the fixture Package is a current candidate");
        };
        package
    }

    fn listed(root: &std::path::Path, collection: &str, member: &str) -> Vec<String> {
        response(
            &Invocation::new(&["package", "list", "orders", "--json"])
                .rooted_at(root)
                .run(),
        )["data"][collection]
            .as_array()
            .unwrap_or_else(|| panic!("the listing carries a {collection} collection"))
            .iter()
            .map(|order| {
                let value = if member == "order_id" {
                    &order["order_id"]
                } else {
                    &order["directory"]["package_relative_path"]
                };
                value.as_str().expect("the member is a string").to_owned()
            })
            .collect()
    }

    let (_tree, root) = populated_package("list-orders-core-ordering");
    let package = loaded(&root);
    let core: Vec<String> = package
        .healthy_orders()
        .iter()
        .map(|order| order.document().order_id.as_str().to_owned())
        .collect();
    assert_eq!(core.len(), 2, "two healthy Orders, so a sequence exists");
    let mut by_directory: Vec<String> = package
        .healthy_orders()
        .iter()
        .map(|order| {
            order
                .paths()
                .directory()
                .package_relative_path()
                .expect("the fixture directory has a Package-relative spelling")
                .to_owned()
        })
        .collect();
    by_directory.sort();
    assert_ne!(
        by_directory.first().map(|name| name.contains("2026_08_22")),
        Some(true),
        "the decided ordering is not the directory ordering, so an adapter that sorted \
         by name would be visible here"
    );
    assert_eq!(
        listed(&root, "orders", "order_id"),
        core,
        "the adapter reorders nothing"
    );

    let (_tree, root) = package_with_every_order_damaged("list-damaged-core-ordering");
    let package = loaded(&root);
    let core: Vec<String> = package
        .damaged_orders()
        .iter()
        .map(|order| {
            order
                .paths()
                .directory()
                .package_relative_path()
                .expect("the fixture directory has a Package-relative spelling")
                .to_owned()
        })
        .collect();
    assert_eq!(core.len(), 2, "two Damaged Orders, so a sequence exists");
    assert_eq!(
        listed(&root, "damaged_orders", "directory"),
        core,
        "and it reorders the placeholders no more than it reorders the Orders"
    );
}

/// The same ordering on the next run over the same Package.
#[test]
fn the_order_listing_is_stable_across_runs_over_one_package() {
    let (_tree, root) = populated_package("list-orders-stable");

    let first = Invocation::new(&["package", "list", "orders", "--json"])
        .rooted_at(&root)
        .run();
    let second = Invocation::new(&["package", "list", "orders", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(
        first.stdout_text(),
        second.stdout_text(),
        "two reads of one Package are the same answer, byte for byte"
    );
}

/// A Damaged Order is a placeholder, never a healthy Order, and never dropped.
///
/// Both halves are the defect: a listing that silently omits an Order Directory it could
/// not read tells a user the Order is gone, and one that lists it among the healthy
/// invents facts it does not have.
///
/// **The placeholder is asserted against `package show`'s** rather than against literals,
/// because the criterion is agreement between the two commands. The listing's placeholder
/// carries the Order Directory and the identity the load could still trust — `null` here,
/// because this fixture's record is unreadable and the core guesses no identity from a
/// directory name.
#[test]
fn a_damaged_order_is_listed_as_a_placeholder_rather_than_as_an_order() {
    let (_tree, root, damaged) = package_with_a_damaged_order("list-orders-damaged");

    let outcome = Invocation::new(&["package", "list", "orders", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "one Damaged Order does not damage the Package"
    );
    let orders = document["data"]["orders"]
        .as_array()
        .expect("the listing carries an Orders collection");
    assert_eq!(orders.len(), 1, "the damaged one is not among the healthy");
    assert_eq!(orders[0]["order_id"], NEWEST_ORDER_ID);

    let placeholders = document["data"]["damaged_orders"]
        .as_array()
        .expect("the listing carries a Damaged Orders collection");
    assert_eq!(placeholders.len(), 1, "and it is not dropped either");
    assert_eq!(
        placeholders[0]["directory"]["absolute_path"],
        damaged.to_str().expect("the test path is UTF-8")
    );

    let shown = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let shown = &shown["data"]["damaged_orders"][0];
    assert_eq!(
        placeholders[0]["order_id"], shown["order_id"],
        "the two commands report one Damaged Order's identity identically"
    );
    assert_eq!(
        placeholders[0]["directory"], shown["paths"]["directory"],
        "and one Damaged Order's directory identically"
    );
}

/// An empty Package is a real answer: present, empty collections at exit `0`.
#[test]
fn an_empty_package_answers_the_order_listing_with_present_empty_collections() {
    let (_tree, root) = initialized_package("list-orders-empty");

    let outcome = Invocation::new(&["package", "list", "orders", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "an empty Package is not a failure");
    for collection in ["orders", "damaged_orders"] {
        assert_eq!(
            document["data"][collection].as_array().map(Vec::len),
            Some(0),
            "{collection} is present and empty rather than absent"
        );
    }
}

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
    let vendors = document["data"]["vendors"]
        .as_array()
        .expect("the listing carries a Vendors collection");

    assert_eq!(outcome.status, Some(0));
    assert_eq!(vendors.len(), 1, "the Hidden Vendor is not offered");
    assert_eq!(vendors[0]["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(vendors[0]["name"], "Tarnwick");
    assert_eq!(
        vendors[0]["hidden"], false,
        "every Vendor carries its visibility, so the shape does not depend on the flag"
    );
    assert_eq!(
        document["data"]["hidden_vendors_included"], false,
        "and the answer says which list this is, so an absent Vendor is not a missing one"
    );

    let shown = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let offered: Vec<&serde_json::Value> = shown["data"]["vendors"]
        .as_array()
        .expect("package show carries a Vendors collection")
        .iter()
        .collect();
    assert_eq!(offered.len(), vendors.len());
    assert_eq!(offered[0]["vendor_id"], vendors[0]["vendor_id"]);
    assert_eq!(
        shown["data"]["hidden_vendors_included"],
        document["data"]["hidden_vendors_included"]
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
    let vendors = document["data"]["vendors"]
        .as_array()
        .expect("the listing carries a Vendors collection");

    assert_eq!(vendors.len(), 2, "the complete list");
    assert_eq!(document["data"]["hidden_vendors_included"], true);
    let hidden: Vec<&serde_json::Value> = vendors
        .iter()
        .filter(|vendor| vendor["vendor_id"] == HIDDEN_VENDOR_ID)
        .collect();
    assert_eq!(hidden.len(), 1, "the Hidden Vendor is present");
    assert_eq!(
        hidden[0]["hidden"], true,
        "and it says so, rather than being reported as any other Vendor"
    );
}

/// An empty Package answers with a present, empty Vendors collection.
#[test]
fn an_empty_package_answers_the_vendor_listing_with_a_present_empty_collection() {
    let (_tree, root) = initialized_package("list-vendors-empty");

    let outcome = Invocation::new(&["package", "list", "vendors", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0));
    assert_eq!(
        document["data"]["vendors"].as_array().map(Vec::len),
        Some(0),
        "`vendors` is present and empty rather than absent"
    );
    assert_eq!(
        document["data"]["hidden_vendors_included"], false,
        "and the member that says which list this is stays present too"
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
        let reported: Vec<(&str, u64)> = document["data"]["values"]
            .as_array()
            .unwrap_or_else(|| panic!("{kind} carries a values collection"))
            .iter()
            .map(|value| {
                (
                    value["value"].as_str().expect("a canonical spelling"),
                    value["usage_count"].as_u64().expect("a usage count"),
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

/// Two Orders whose value differs only in case and surrounding whitespace are one value.
///
/// The canonical spelling appears once with an accurate count. The defect: reporting two
/// entries, or one with a count of `1`, either of which would offer a user a second
/// spelling of a value the Package already has.
#[test]
fn two_orders_differing_only_in_case_or_whitespace_are_one_value() {
    let (_tree, root) = package_with_a_differently_spelled_value("values-canonicalization");

    let outcome = Invocation::new(&["package", "values", "project", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);
    let values = document["data"]["values"]
        .as_array()
        .expect("the answer carries a values collection");

    assert_eq!(outcome.status, Some(0));
    assert_eq!(values.len(), 1, "one value, not two spellings of it");
    assert_eq!(
        values[0]["value"], "Telescope",
        "reported in its canonical trimmed spelling"
    );
    assert_eq!(values[0]["usage_count"], 2, "and counting both Orders");
}

/// An empty Package answers every value kind with a present, empty collection.
#[test]
fn an_empty_package_answers_every_value_kind_with_a_present_empty_collection() {
    let (_tree, root) = initialized_package("values-empty");

    for kind in ["project", "orderer", "ordering_institution", "order_group"] {
        let outcome = Invocation::new(&["package", "values", kind, "--json"])
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(0), "{kind}: not a failure");
        assert_eq!(
            document["data"]["values"].as_array().map(Vec::len),
            Some(0),
            "{kind}: `values` is present and empty rather than absent"
        );
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
    let shown_orders = shown["data"]["orders"]
        .as_array()
        .expect("package show carries an Orders collection");
    let listed = listed_orders["data"]["orders"]
        .as_array()
        .expect("the listing carries an Orders collection");
    assert_eq!(listed.len(), shown_orders.len());
    for (listed, shown) in listed.iter().zip(shown_orders) {
        for member in ["order_id", "title", "date_ordered", "status", "vendor_id"] {
            assert_eq!(listed[member], shown[member], "{member}");
        }
        assert_eq!(listed["vendor"], shown["vendor"], "the resolved Vendor");
        assert_eq!(listed["directory"], shown["paths"]["directory"]);
    }
    assert_eq!(
        listed_orders["data"]["damaged_orders"]
            .as_array()
            .map(Vec::len),
        shown["data"]["damaged_orders"].as_array().map(Vec::len)
    );

    let listed_vendors = response(
        &Invocation::new(&["package", "list", "vendors", "--json", "--all"])
            .rooted_at(root)
            .run(),
    );
    let shown_vendors = shown["data"]["vendors"]
        .as_array()
        .expect("package show carries a Vendors collection");
    let vendors = listed_vendors["data"]["vendors"]
        .as_array()
        .expect("the listing carries a Vendors collection");
    assert_eq!(vendors.len(), shown_vendors.len());
    for (listed, shown) in vendors.iter().zip(shown_vendors) {
        for member in ["vendor_id", "name", "hidden"] {
            assert_eq!(listed[member], shown[member], "{member}");
        }
    }

    for (kind, member) in [
        ("project", "projects"),
        ("orderer", "orderers"),
        ("ordering_institution", "ordering_institutions"),
        ("order_group", "order_groups"),
    ] {
        let document = response(
            &Invocation::new(&["package", "values", kind, "--json"])
                .rooted_at(root)
                .run(),
        );
        assert_eq!(
            document["data"]["values"], shown["data"]["values"][member],
            "{kind} reports the list `package show` reports under `{member}`"
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
            "package_root_damaged",
            package_with_duplicate_vendor_names("six-load-damaged"),
        ),
        (
            "package_metadata_unsupported",
            package_at_an_uninterpretable_version("six-load-unsupported"),
        ),
        (
            "package_unreadable",
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
                document["error"]["scope"], shown["error"]["scope"],
                "{name}"
            );
            assert_eq!(
                document["error"]["paths"], shown["error"]["paths"],
                "{name}"
            );
            assert_eq!(
                document["context"]["package_root"], shown["context"]["package_root"],
                "{name}: the Root the command was about"
            );
            assert!(document["data"].is_null(), "{name}");
        }
    }

    // The one state whose *directory* resolves without being a Root, kept separate
    // because it is built from a Package rather than being one.
    let (_tree, root) = initialized_package("six-load-unmanaged");
    let outside = directory_that_is_not_a_root(&root);
    for (name, outcome) in run_six(&outside, &["--json"]) {
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(1), "{name}");
        assert_eq!(
            document["error"]["code"], "package_root_unmanaged",
            "{name}"
        );
        assert!(
            document["context"]["package_root"].is_null(),
            "{name}: an unmanaged directory is a resolved path, not a Root"
        );
    }
}

/// A damaged Package's diagnostics are the answer, and reach all six.
#[test]
fn a_damaged_package_reports_its_diagnosis_through_every_one_of_the_six() {
    let (_tree, root) = package_with_duplicate_vendor_names("six-damaged-diagnosis");

    for (name, outcome) in run_six(&root, &["--json"]) {
        let document = response(&outcome);
        let diagnostics = document["diagnostics"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} carries a diagnostics collection"));
        assert!(
            !diagnostics.is_empty(),
            "{name}: `data` is null on a failure, so the envelope is the only channel \
             the diagnosis has"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic["code"] == "vendor_name_duplicate"),
            "{name}: and it is the load's own diagnosis"
        );
    }
}

/// A Damaged Order is reported by every one of the six, whether or not it narrowed the
/// answer.
///
/// The envelope is where a response says what its read observed, and one load serves all
/// six — so the observation travels in all six rather than in the ones it happened to
/// change. **It narrows two of them.** A Damaged Order contributes to no value list, so
/// `package values project` over this Package answers `Telescope (1)` where the same
/// Package undamaged answers `(2)`; without the observation beside it, that partial answer
/// would be indistinguishable from a complete one. `package list orders` reports the
/// Damaged Order itself, `package list vendors` is untouched by one, and the two value
/// lists the damaged Order carried no value for are untouched too — for those, this is a
/// read reporting what it saw rather than a caveat on the answer.
#[test]
fn a_damaged_order_is_reported_by_every_one_of_the_six() {
    let (_tree, root, _damaged) = package_with_a_damaged_order("six-damaged-order-diagnostic");

    let shown = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let shown_diagnostics = shown["diagnostics"]
        .as_array()
        .expect("package show carries a diagnostics collection");

    for (name, outcome) in run_six(&root, &["--json"]) {
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(0), "{name}: the Package still reads");
        let diagnostics = document["diagnostics"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} carries a diagnostics collection"));
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic["scope"] == "order"
                    && diagnostic["primary_location"]["package_relative_path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with("/.spielorders/order.json"))
            }),
            "{name}: the answer says the Package holds an Order it could not read:\n\
             {diagnostics:#?}"
        );
        // Every one of them is the load's own observation, never one this adapter
        // invented: `package show` reports the same load plus the Package README read it
        // alone performs, so these are a subset of what it carries.
        for diagnostic in diagnostics {
            assert!(
                shown_diagnostics.contains(diagnostic),
                "{name}: {diagnostic:#?} is not among the load's own observations"
            );
        }
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
    for named in ["Laser components", "Mirror mounts", "Tarnwick", "Nethermoor"] {
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
    assert!(
        vendors.stdout_text().contains("Hidden Vendors: omitted"),
        "and says which list this is, so an absent Vendor is not a missing one:\n{}",
        vendors.stdout_text()
    );

    let values = Invocation::new(&["package", "values", "project"])
        .rooted_at(&root)
        .run();
    assert!(
        values.stdout_text().contains("Projects: Telescope (2)"),
        "the value list names the canonical spelling and its count:\n{}",
        values.stdout_text()
    );
}

/// Identities are `--verbose` detail in human output and always present in JSON.
#[test]
fn uuids_are_verbose_only_in_the_human_listings() {
    let (_tree, root) = populated_package("six-human-uuids");

    for (name, outcome) in run_six(&root, &[]) {
        for identity in [NEWEST_ORDER_ID, OFFERED_VENDOR_ID] {
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
        verbose.stdout_text().contains(NEWEST_ORDER_ID),
        "`--verbose` is where they appear:\n{}",
        verbose.stdout_text()
    );
    let verbose = Invocation::new(&["package", "list", "vendors", "--verbose"])
        .rooted_at(&root)
        .run();
    assert!(
        verbose.stdout_text().contains(OFFERED_VENDOR_ID),
        "for Vendors too:\n{}",
        verbose.stdout_text()
    );
}

/// `--quiet` says nothing on success, from every one of the six.
#[test]
fn quiet_withholds_the_result_of_every_one_of_the_six() {
    let (_tree, root) = populated_package("six-quiet");

    for (name, outcome) in run_six(&root, &["--quiet"]) {
        assert_eq!(outcome.status, Some(0), "{name}");
        assert_eq!(outcome.stdout, b"", "{name}: {}", outcome.stdout_text());
        assert_eq!(outcome.stderr, b"", "{name}: {}", outcome.stderr_text());
    }
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
///
/// **Today this is weaker than it reads, and will strengthen on its own.** The `order`
/// family is not registered yet — Ticket 005 delivers it — so both lines currently fail
/// because `order` is not a subcommand at all, rather than because `list` is not one of
/// its commands. Nothing in production code can be broken to make this test wrong until
/// that family exists; it is a guard placed ahead of what it guards, which is the point
/// of writing a decided absence down.
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
/// `value_kind`'s exhaustive match makes a fifth Reusable Free-Entry Value category a
/// compile error until it has a contract, a label, and a core list. `VALUE_KINDS` is a
/// fixed-length literal, so the same fifth category would compile against it untouched
/// and go missing from `package show`'s human summary while its own `package values`
/// command worked — which is the quiet half of the pair, and the half nothing else binds.
#[test]
fn every_value_kind_is_iterated() {
    use clap::ValueEnum;

    let declared: Vec<String> = spielorders_cli::cli::ValueKind::value_variants()
        .iter()
        .map(|kind| {
            kind.to_possible_value()
                .expect("every kind is selectable on the command line")
                .get_name()
                .to_owned()
        })
        .collect();
    assert_eq!(
        declared,
        vec![
            "ordering_institution".to_owned(),
            "orderer".to_owned(),
            "project".to_owned(),
            "order_group".to_owned(),
        ],
        "the four categories, in `package show`'s published `values` payload order"
    );

    let (_tree, root) = populated_package("value-kind-coverage");
    for kind in &declared {
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

    // `VALUE_KINDS` is private, so what "iterated" means from outside the crate is the
    // sequence of labels `package show`'s summary prints — one line per kind, whether or
    // not that kind has any value, so a kind missing from the array is a missing line
    // rather than a line that happens to be empty. This is also the assertion the first
    // review pass's own regression needed and did not have: reordering `VALUE_KINDS`
    // reordered a closed command's summary with the whole suite green.
    let summary = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    let printed: Vec<&str> = summary
        .stdout_text()
        .lines()
        .filter_map(|line| line.split(':').next())
        .filter(|label| {
            [
                "Ordering institutions",
                "Orderers",
                "Projects",
                "Order groups",
            ]
            .contains(label)
        })
        .collect();
    assert_eq!(
        printed,
        vec![
            "Ordering institutions",
            "Orderers",
            "Projects",
            "Order groups"
        ],
        "every kind is printed, once, in the order `ValueKind` declares them:\n{}",
        summary.stdout_text()
    );
}
