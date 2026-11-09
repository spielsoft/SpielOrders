//! `search orders` and `search vendors`: which of a Package's entities match a query.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{
    Invocation, hidden_vendor_id, newest_order_id, offered_vendor_id, populated_package, response,
};

/// The Order Titles one response carries, in the order it carries them.
fn titles(document: &serde_json::Value) -> Vec<String> {
    document["data"]
        .as_array()
        .unwrap_or_else(|| panic!("a matched-Orders answer is a collection: {document}"))
        .iter()
        .map(|entry| {
            entry["title"]
                .as_str()
                .expect("a matched Order names its Title")
                .to_owned()
        })
        .collect()
}

/// The Vendor Names one response carries, in the order it carries them.
fn names(document: &serde_json::Value) -> Vec<String> {
    document["data"]
        .as_array()
        .unwrap_or_else(|| panic!("a matched-Vendors answer is a collection: {document}"))
        .iter()
        .map(|vendor| {
            vendor["name"]
                .as_str()
                .expect("a matched Vendor names its Name")
                .to_owned()
        })
        .collect()
}

/// One quoted multiword argument searches as several terms, and matches exactly what the
/// same terms match when the shell splits them.
///
/// **The two are different code paths**, so a test that passed the terms as separate
/// arguments would certify one of them. The whole response is compared rather than the
/// collection alone.
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

    for (command, term, insensitively) in [("orders", "lovelace", 2), ("vendors", "tarnwick", 1)] {
        let run = || {
            Invocation::new(&["search", command, term])
                .rooted_at(&root)
                .argument("--json")
        };
        let insensitive = response(&run().run());
        let sensitive = response(&run().argument("--case-sensitive").run());

        assert_eq!(
            insensitive["data"].as_array().map(Vec::len),
            Some(insensitively),
            "search {command}: case-insensitive by default: {insensitive}"
        );
        assert_eq!(
            sensitive["data"].as_array().map(Vec::len),
            Some(0),
            "search {command}: the Package spells it with a capital: {sensitive}"
        );

        let blanked = |document: &serde_json::Value| {
            let mut without = document.clone();
            without["data"] = serde_json::Value::Null;
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
        (
            "orders",
            "collimator",
            "Laser components",
            &*newest_order_id(&root),
        ),
        (
            "vendors",
            "tarnwick",
            "Tarnwick",
            &*offered_vendor_id(&root),
        ),
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

/// A Hidden Vendor a query retains is reported only under `--all`.
///
/// A Vendor list offers only the active Vendors until `--all` asks for every one, so a
/// query naming a Hidden Vendor retains nothing by default. Hiding still governs nothing
/// about resolution: the Order that references this Vendor reports it either way, which
/// is the second half here.
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

    let complete = response(
        &Invocation::new(&["search", "vendors", "nethermoor"])
            .rooted_at(&root)
            .argument("--all")
            .argument("--json")
            .run(),
    );
    assert_eq!(names(&complete), ["Nethermoor"], "{complete}");
    assert_eq!(complete["data"][0]["hidden"], "true");

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
    assert_eq!(orders["data"][0]["vendor_id"], &*hidden_vendor_id(&root));
}
