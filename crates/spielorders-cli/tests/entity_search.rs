//! `search orders` and `search vendors`: which of a Package's entities match a query.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{
    Invocation, NEWEST_ORDER_ID, OFFERED_VENDOR_ID, initialized_package, populated_package,
    response,
};

#[test]
fn unicode_caseless_substrings_match_orders_and_vendors_without_changing_case_control() {
    for (name, query, sensitive_matches) in [
        ("ΟΣ", "Σ", true),
        ("Straße", "STRASSE", false),
        ("ﬃ optics", "FI", false),
    ] {
        let (_tree, root) = initialized_package("unicode-caseless-search");
        let vendor = Invocation::new(&["add-vendor", name, "--json"])
            .rooted_at(&root)
            .run();
        assert_eq!(vendor.status, Some(0), "{}", vendor.stdout_text());
        let order = Invocation::new(&["add-order", "--json"])
            .argument(format!("title={name}"))
            .argument(format!("vendor={name}"))
            .rooted_at(&root)
            .run();
        assert_eq!(order.status, Some(0), "{}", order.stdout_text());
        for entity in ["orders", "vendors"] {
            for sensitive in [false, true] {
                let mut run =
                    Invocation::new(&["search", entity, query, "--json"]).rooted_at(&root);
                if sensitive {
                    run = run.argument("--case-sensitive");
                }
                let outcome = run.run();
                assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
                let document = response(&outcome);
                let expected = if !sensitive || sensitive_matches {
                    vec![name.to_owned()]
                } else {
                    Vec::new()
                };
                let retained = if entity == "orders" {
                    titles(&document)
                } else {
                    names(&document)
                };
                assert_eq!(
                    retained, expected,
                    "{entity}: {name:?} for {query:?}, sensitive={sensitive}"
                );
            }
        }
    }
}

/// The Order Titles one response carries, in the order it carries them.
fn titles(document: &serde_json::Value) -> Vec<String> {
    document["data"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("a matched-Orders answer carries a collection: {document}"))
        .iter()
        .map(|order| {
            order["title"]
                .as_str()
                .expect("a matched Order names its Title")
                .to_owned()
        })
        .collect()
}

/// The Vendor Names one response carries, in the order it carries them.
fn names(document: &serde_json::Value) -> Vec<String> {
    document["data"]["vendors"]
        .as_array()
        .unwrap_or_else(|| panic!("a matched-Vendors answer carries a collection: {document}"))
        .iter()
        .map(|vendor| {
            vendor["name"]
                .as_str()
                .expect("a matched Vendor names its Name")
                .to_owned()
        })
        .collect()
}

/// Each command answers with the entities that match, in the Package's own ordering.
///
/// The defect this catches is a search that answers about the wrong Package or reorders
/// what it found: `populated_package` holds two Orders whose default ordering is newest
/// Date Ordered first, and one term lands on a fact of each.
#[test]
fn each_command_answers_with_the_entities_that_match() {
    let (_tree, root) = populated_package("search-answers");

    let document = response(
        &Invocation::new(&["search", "orders", "lovelace"])
            .rooted_at(&root)
            .argument("--json")
            .run(),
    );
    assert_eq!(
        titles(&document),
        ["Laser components", "Mirror mounts"],
        "the Orderer both Orders carry, in the Package's own ordering: {document}"
    );

    let document = response(
        &Invocation::new(&["search", "orders", "collimator"])
            .rooted_at(&root)
            .argument("--json")
            .run(),
    );
    assert_eq!(
        titles(&document),
        ["Laser components"],
        "an Item description only one Order carries: {document}"
    );

    let document = response(
        &Invocation::new(&["search", "vendors", "tarnwick"])
            .rooted_at(&root)
            .argument("--json")
            .run(),
    );
    assert_eq!(names(&document), ["Tarnwick"], "{document}");
}

/// One quoted multiword argument searches as several terms, and matches exactly what the
/// same terms match when the shell splits them.
///
/// **The two are different code paths**, so a test that passed the terms as separate
/// arguments would certify one of them. The whole response is compared rather than the
/// collection alone, because the query the answer reports back is where a command line
/// that dropped a word shows up.
///
/// The terms are chosen so that the claim is not vacuous either way. No single searchable
/// fact of the Package carries `laser lovelace` — one term names an Order Title and the
/// other its Orderer — so a query read as a phrase retains nothing, and `tarnwick
/// accounts` is the same arrangement over a Vendor Name and one of its Contacts' roles.
/// Each answer is non-empty, so an implementation that had stopped separating terms fails
/// here rather than passing on two empty collections being equal.
#[test]
fn one_quoted_multiword_argument_searches_as_the_terms_a_shell_would_split() {
    let (_tree, root) = populated_package("search-quoted-arguments");

    for (command, terms, retained, entities) in [
        (
            "orders",
            ["laser", "lovelace"],
            titles as fn(&serde_json::Value) -> Vec<String>,
            ["Laser components"],
        ),
        ("vendors", ["tarnwick", "accounts"], names, ["Tarnwick"]),
    ] {
        let quoted = response(
            &Invocation::new(&["search", command, &terms.join(" ")])
                .rooted_at(&root)
                .argument("--json")
                .run(),
        );
        let split = response(
            &Invocation::new(&["search", command])
                .argument(terms[0])
                .argument(terms[1])
                .rooted_at(&root)
                .argument("--json")
                .run(),
        );

        assert_eq!(
            quoted, split,
            "search {command}: one quoted argument and two arguments are one query"
        );
        assert_eq!(
            retained(&quoted),
            entities,
            "search {command}: the terms land on two different facts of one entity, and \
             no fact carries both: {quoted}"
        );
        assert_eq!(
            quoted["data"]["query"],
            terms.join(" "),
            "search {command}: the answer reports the query it was asked"
        );
    }
}

/// `--case-sensitive` changes which entities match and nothing else about the response.
///
/// **Both halves are the claim.** Without the first, the option could do nothing at all;
/// without the second, it could quietly change the contract identity, the ordering, or
/// what the answer reports it was asked. The comparison blanks the one collection the
/// option is allowed to change and requires every other byte of the document to be equal,
/// so a member that moved with the flag fails here.
///
/// The Package spells `Ada Lovelace` and `Tarnwick` with capitals, so the lowercase query
/// retains both entities without the option and neither with it.
#[test]
fn the_case_control_changes_which_entities_match_and_nothing_else() {
    let (_tree, root) = populated_package("search-case-control");

    for (command, collection, term, insensitively) in [
        ("orders", "orders", "lovelace", 2),
        ("vendors", "vendors", "tarnwick", 1),
    ] {
        let run = || {
            Invocation::new(&["search", command, term])
                .rooted_at(&root)
                .argument("--json")
        };
        let insensitive = response(&run().run());
        let sensitive = response(&run().argument("--case-sensitive").run());

        assert_eq!(
            insensitive["data"][collection].as_array().map(Vec::len),
            Some(insensitively),
            "search {command}: case-insensitive by default: {insensitive}"
        );
        assert_eq!(
            sensitive["data"][collection].as_array().map(Vec::len),
            Some(0),
            "search {command}: the Package spells it with a capital: {sensitive}"
        );

        let blanked = |document: &serde_json::Value| {
            let mut without = document.clone();
            without["data"][collection] = serde_json::Value::Null;
            without
        };
        assert_eq!(
            blanked(&insensitive),
            blanked(&sensitive),
            "search {command}: the option changes which entities match and nothing else"
        );

        // The option is declared on the command it modifies, so it stands anywhere after
        // the command path — before the words as readily as after them. Both placements
        // are asked for, because one of them passing certifies one of them.
        let before_the_words = response(
            &Invocation::new(&["search", command])
                .argument("--case-sensitive")
                .argument(term)
                .rooted_at(&root)
                .argument("--json")
                .run(),
        );
        assert_eq!(
            before_the_words, sensitive,
            "search {command}: the option is the same option wherever it follows the \
             command path"
        );
    }
}

/// A query nothing matches is an answer, and no query at all restricts nothing.
///
/// **The two ends of the same rule.** A collection that is present and empty says the
/// Package holds nothing matching; a failure would say the question could not be answered,
/// and a caller told the second when the first is true goes looking for a broken Package.
/// The other end is the decided empty query: with no words the answer is every entity, so
/// a command line that demanded a word would put that rule out of reach.
#[test]
fn a_query_nothing_matches_answers_and_no_query_restricts_nothing() {
    let (_tree, root) = populated_package("search-empty-answers");

    for (command, collection, everything) in [("orders", "orders", 2), ("vendors", "vendors", 1)] {
        let outcome = Invocation::new(&["search", command, "zirconium"])
            .rooted_at(&root)
            .argument("--json")
            .run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(0),
            "search {command}: nothing matching is an answer: {document}"
        );
        assert_eq!(document["ok"], true, "search {command}");
        assert_eq!(
            document["data"][collection].as_array().map(Vec::len),
            Some(0),
            "search {command}: the collection is present and empty: {document}"
        );

        let document = response(
            &Invocation::new(&["search", command])
                .rooted_at(&root)
                .argument("--json")
                .run(),
        );
        assert_eq!(
            document["data"][collection].as_array().map(Vec::len),
            Some(everything),
            "search {command}: no words is no restriction: {document}"
        );
        assert_eq!(document["data"]["query"], "", "search {command}");
    }
}

/// The human surface holds each entity's identity back to `--verbose`, and the machine
/// response carries it either way.
///
/// The defect this catches is a UUID on every line of an ordinary human answer, or an
/// identity absent from the response a script reads. Both commands are swept, because a
/// surface that held one back and not the other would be two conventions.
#[test]
fn the_human_surface_holds_each_identity_back_to_verbose() {
    let (_tree, root) = populated_package("search-verbose-identities");

    for (command, term, named, identity) in [
        ("orders", "collimator", "Laser components", NEWEST_ORDER_ID),
        ("vendors", "tarnwick", "Tarnwick", OFFERED_VENDOR_ID),
    ] {
        let run = || {
            Invocation::new(&["search", command, term])
                .rooted_at(&root)
                .run()
        };
        let ordinary = run();
        assert_eq!(ordinary.status, Some(0), "{}", ordinary.stderr_text());
        let text = ordinary.stdout_text();
        assert!(
            text.contains(named),
            "search {command}: the answer names the entity: {text}"
        );
        assert!(
            !text.contains(identity),
            "search {command}: and holds its identity back: {text}"
        );

        let verbose = Invocation::new(&["search", command, term])
            .rooted_at(&root)
            .argument("--verbose")
            .run();
        assert!(
            verbose.stdout_text().contains(identity),
            "search {command}: `--verbose` adds it: {}",
            verbose.stdout_text()
        );

        let document = response(
            &Invocation::new(&["search", command, term])
                .rooted_at(&root)
                .argument("--json")
                .run(),
        );
        assert!(
            document.to_string().contains(identity),
            "search {command}: the machine response carries it: {document}"
        );
    }
}

/// A Hidden Vendor a query retains is reported only under `--all`, and the answer says
/// which list it is.
///
/// **The empty answer is the reason the member beside it exists.** A Vendor list offers
/// only the active Vendors until `--all` asks for every one, so a query naming a Hidden
/// Vendor retains nothing by default — and without `hidden_vendors_included` a caller
/// could not tell that from there being no such Vendor. Hiding still governs nothing about
/// resolution: the Order that references this Vendor reports it either way, which is the
/// second half here.
#[test]
fn a_hidden_vendor_a_query_retains_is_reported_only_under_all() {
    let (_tree, root) = populated_package("search-hidden-vendors");

    let offered = response(
        &Invocation::new(&["search", "vendors", "nethermoor"])
            .rooted_at(&root)
            .argument("--json")
            .run(),
    );
    assert_eq!(names(&offered), Vec::<String>::new(), "{offered}");
    assert_eq!(offered["data"]["hidden_vendors_included"], false);

    let complete = response(
        &Invocation::new(&["search", "vendors", "nethermoor"])
            .rooted_at(&root)
            .argument("--all")
            .argument("--json")
            .run(),
    );
    assert_eq!(names(&complete), ["Nethermoor"], "{complete}");
    assert_eq!(complete["data"]["vendors"][0]["hidden"], true);
    assert_eq!(complete["data"]["hidden_vendors_included"], true);

    let orders = response(
        &Invocation::new(&["search", "orders", "nethermoor"])
            .rooted_at(&root)
            .argument("--json")
            .run(),
    );
    assert_eq!(
        titles(&orders),
        ["Mirror mounts"],
        "an Order resolves the Hidden Vendor it references, and matches on its facts, \
         with no option asked for: {orders}"
    );
    assert_eq!(orders["data"]["orders"][0]["vendor"]["hidden"], true);
}
