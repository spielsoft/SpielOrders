//! Which Orders and Vendors of a loaded Package match a query.
//!
//! Decision sources: Define lookup and search behavior.

use crate::tests::support::index_is_current;
use crate::tests::support::{
    PlatformFileSystem, QueuedUuids, TemporaryTree, committed, edit_vendor, effects, opened,
    rebuild_index,
};
use crate::{Assignments, Change, Report, VendorRef};
use crate::{
    Package, canonical::HealthyOrder, metadata::Vendor, search::CaseSensitivity,
    search::OrderListEntry, search::SearchQuery, search::VendorVisibility,
};
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// The configured Orders Directory of every fixture below.
///
/// Deliberately not the conventional name: the configured path is one of the excluded
/// sources, so a fixture whose Orders Directory is called `Orders` could not tell a
/// search that reads configuration from one that does not.
const ORDERS_DIRECTORY: &str = "Bestellungen";

/// The configured Removed Orders Directory of every fixture below, for the same reason.
const REMOVED_DIRECTORY: &str = "Entfernte";

/// A committed Package whose two managed directories are configured rather than
/// conventional.
fn package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = crate::plan(
        &Change::InitializePackage {
            root: tree.path().into(),
            orders_path: Some(String::from(ORDERS_DIRECTORY)),
            removed_path: Some(String::from(REMOVED_DIRECTORY)),
        },
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Committed { .. })
    ));
    let root = std::fs::canonicalize(tree.path()).expect("the test Package root resolves");
    (tree, root)
}

/// One Order Directory carrying the record given, and its conventional README.
fn order(root: &Path, basename: &str, record: &str, readme: &str) -> PathBuf {
    let directory = root.join(ORDERS_DIRECTORY).join(basename);
    std::fs::create_dir_all(directory.join(".spielorders")).expect("the Order Directory");
    std::fs::write(directory.join(".spielorders/order.json"), record).expect("the Order record");
    std::fs::write(directory.join("README.md"), readme).expect("the Order README");
    directory
}

/// The Package's Vendor record, replacing whatever initialization wrote.
fn vendors(root: &Path, record: &str) {
    std::fs::write(root.join(".spielorders/vendors.json"), record).expect("the Vendor record");
}

/// The opened Package a search answers about.
fn loaded(root: &Path) -> Package {
    match opened(root, &PlatformFileSystem).result {
        Ok(package) => package,
        other => panic!("the fixture Package loads as a current candidate: {other:?}"),
    }
}

/// The healthy Orders `query` retains, each at its position in the Package's list.
fn matching_orders(package: &Package, query: &SearchQuery) -> Vec<(usize, HealthyOrder)> {
    package
        .orders_matching(query)
        .result
        .unwrap()
        .into_iter()
        .filter_map(|row| match row {
            OrderListEntry::Healthy { position, order } => Some((position, *order)),
            OrderListEntry::Damaged(_) => None,
        })
        .collect()
}

/// Every Vendor `query` retains, Hidden Vendors included.
fn matching_vendors(package: &Package, query: &SearchQuery) -> Vec<Vendor> {
    package
        .vendors_matching(query, VendorVisibility::All)
        .result
        .unwrap()
}

/// The Order Titles a query retains, in the order the answer carries them.
fn titles(matched: &[(usize, HealthyOrder)]) -> Vec<String> {
    matched
        .iter()
        .map(|(_, order)| order.document.title.deref().to_owned())
        .collect()
}

/// The Vendor Names a query retains, in the order the answer carries them.
fn names(matched: &[Vendor]) -> Vec<String> {
    matched
        .iter()
        .map(|vendor| vendor.name.deref().to_owned())
        .collect()
}

const KARBORUND: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Karborund Läppscheibe",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
  "orderer": "Ada Lovelace",
  "items": []
}
"#;

const TARNWICK: &str = r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": []
    }
  ]
}
"#;

#[test]
fn ordinary_order_browsing_includes_damage_only_without_query_terms() {
    let (_tree, root) = package("ordinary-order-list");
    vendors(&root, TARNWICK);
    order(&root, "Healthy", KARBORUND, "");
    order(&root, "Damaged", MOLYBDAENIT, "");
    let package = loaded(&root);
    let unfiltered = package
        .orders_matching(&SearchQuery::new("  ", CaseSensitivity::Insensitive))
        .result
        .unwrap();
    assert_eq!(unfiltered.len(), 2);
    assert!(matches!(
        unfiltered[0],
        OrderListEntry::Healthy { position: 0, .. }
    ));
    assert!(matches!(unfiltered[1], OrderListEntry::Damaged(_)));
    let matched = package
        .orders_matching(&SearchQuery::new("Karborund", CaseSensitivity::Insensitive))
        .result
        .unwrap();
    assert_eq!(matched.len(), 1);
    assert!(matches!(matched[0], OrderListEntry::Healthy { .. }));
    assert!(
        package
            .orders_matching(&SearchQuery::new(
                "Molybdaenit",
                CaseSensitivity::Insensitive
            ))
            .result
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ordinary_vendor_browsing_combines_visibility_and_query_in_core() {
    let (_tree, root) = package("ordinary-vendor-list");
    let mut document: serde_json::Value = serde_json::from_str(TARNWICK).unwrap();
    let mut hidden = document["vendors"][0].clone();
    hidden["vendor_id"] = serde_json::json!("e0990d31-06aa-42e9-97c8-088e3e894823");
    hidden["name"] = serde_json::json!("Hidden supplier");
    hidden["hidden"] = serde_json::json!(true);
    document["vendors"].as_array_mut().unwrap().push(hidden);
    vendors(&root, &document.to_string());
    let package = loaded(&root);
    let query = SearchQuery::new("", CaseSensitivity::Insensitive);
    assert_eq!(
        names(
            &package
                .vendors_matching(&query, VendorVisibility::Offered)
                .result
                .unwrap()
        ),
        ["Tarnwick"]
    );
    assert_eq!(
        package
            .vendors_matching(&query, VendorVisibility::All)
            .result
            .unwrap()
            .len(),
        2
    );
    let query = SearchQuery::new("hidden", CaseSensitivity::Insensitive);
    assert!(
        package
            .vendors_matching(&query, VendorVisibility::Offered)
            .result
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        names(
            &package
                .vendors_matching(&query, VendorVisibility::All)
                .result
                .unwrap()
        ),
        ["Hidden supplier"]
    );
}

/// Two terms may land on two different facts of one entity, and every term must land.
///
/// The defect this catches is a search that scores or ORs its terms: `karborund lovelace`
/// names a Title and an Orderer, and no single fact holds both, so an implementation that
/// required one fact to carry the whole query would retain nothing. The second half is the
/// other direction — one term nothing carries removes the entity, which is what makes the
/// combination an AND rather than a ranking.
#[test]
fn terms_combine_with_and_across_different_facts_of_one_entity() {
    let (_tree, root) = package("entity-search-and-semantics");
    vendors(&root, TARNWICK);
    order(&root, "Karborund-Order", KARBORUND, "");
    let package = loaded(&root);

    let matched = matching_orders(
        &package,
        &SearchQuery::new("karborund lovelace", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Karborund Läppscheibe"],
        "one term names the Title and the other the Orderer"
    );

    let matched = matching_orders(
        &package,
        &SearchQuery::new("karborund nethermoor", CaseSensitivity::Insensitive),
    );
    assert!(
        matched.is_empty(),
        "a term no fact carries removes the Order: {:?}",
        titles(&matched)
    );

    let matched = matching_vendors(
        &package,
        &SearchQuery::new("tarnwick", CaseSensitivity::Insensitive),
    );
    assert_eq!(names(&matched), ["Tarnwick"]);
}

const ALPHA: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d",
  "title": "Alpha Halterung",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "items": [
    {
      "description": "Spiegel Karborund",
      "quantity": 2.0,
      "unit_price": 5.0
    }
  ]
}
"#;

const BETA: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e",
  "title": "Beta Spiegel",
  "date_ordered": "2026-07-19",
  "status": "hold",
  "items": []
}
"#;

const GAMMA: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "3c4d5e6f-7a8b-4c9d-a0e1-f2a3b4c5d6e7",
  "title": "Gamma Spiegel",
  "date_ordered": "2026-08-01",
  "status": "received",
  "items": []
}
"#;

/// Three Orders sharing a term, one of which carries it in several facts at once.
fn three_orders(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = package(label);
    vendors(&root, TARNWICK);
    order(&root, "Alpha", ALPHA, "Spiegel Spiegel Spiegel\n");
    order(&root, "Beta", BETA, "");
    order(&root, "Gamma", GAMMA, "");
    (tree, root)
}

/// An entity is retained once however many of its facts match, and the Package's own
/// ordering comes back untouched.
///
/// The defect this catches is a search that ranks. `Alpha Halterung` carries the term in
/// an Item description and three times over in its README while the other two carry it
/// once in a Title, so anything counting matches would lift it to the front; and an
/// implementation that collected one entry per matching fact would list it three times.
/// The positions come back as the load numbered them, which is what lets a caller resolve
/// each Order's Vendor for a filtered list exactly as it does for the whole one.
#[test]
fn an_entity_is_retained_once_and_the_packages_own_ordering_survives() {
    let (_tree, root) = three_orders("entity-search-ordering");
    let package = loaded(&root);
    let loaded_order: Vec<String> = package
        .contents
        .healthy_orders
        .iter()
        .map(|order| order.document.title.deref().to_owned())
        .collect();

    let matched = matching_orders(
        &package,
        &SearchQuery::new("spiegel", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        loaded_order,
        "every Order carries the term, so the answer is the loaded list itself"
    );
    assert_eq!(
        matched
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>(),
        [0, 1, 2],
        "each Order keeps the position the load gave it"
    );

    let matched = matching_orders(
        &package,
        &SearchQuery::new("karborund", CaseSensitivity::Insensitive),
    );
    assert_eq!(titles(&matched), ["Alpha Halterung"]);
    assert_eq!(
        matched
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>(),
        [1],
        "a retained Order keeps its position in the loaded list rather than being renumbered"
    );

    let matched = matching_orders(
        &package,
        &SearchQuery::new("spiegel karborund", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Alpha Halterung"],
        "two terms landing on one Order retain it once"
    );
}

/// An empty query restricts nothing, and neither does one that is only whitespace.
///
/// The defect this catches is an empty query treated as a term: one empty string compared
/// as a substring would match everything by accident rather than by rule, and one compared
/// as an equality would retain nothing at all — a caller clearing the box would be shown an
/// empty Package.
#[test]
fn an_empty_query_restricts_nothing() {
    let (_tree, root) = three_orders("entity-search-empty-query");
    let package = loaded(&root);

    for text in ["", "   \t\n "] {
        let query = SearchQuery::new(text, CaseSensitivity::Insensitive);
        assert_eq!(
            matching_orders(&package, &query).len(),
            package.contents.healthy_orders.len(),
            "{text:?} retains every Order"
        );
        assert_eq!(
            names(&matching_vendors(&package, &query)),
            ["Tarnwick"],
            "{text:?} retains every Vendor"
        );
    }
}

/// An Order whose record reads perfectly well and names a day February does not have, so
/// the load quarantines it with every one of its facts legible.
const MOLYBDAENIT: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "4d5e6f7a-8b9c-4d0e-b1f2-a3b4c5d6e7f8",
  "title": "Molybdaenit Halterung",
  "date_ordered": "2026-02-30",
  "status": "ordered",
  "items": []
}
"#;

/// A Damaged Order participates in no search.
///
/// The defect this catches is a quarantine that only looks like one. This Order's record
/// parses and every fact in it is readable — a search assembled from parsed records rather
/// than from the Orders the load trusts would match `Molybdaenit` and offer a caller an
/// Order the Package refuses to report facts for anywhere else.
#[test]
fn a_damaged_order_participates_in_no_search() {
    let (_tree, root) = package("entity-search-damaged-order");
    vendors(&root, TARNWICK);
    order(&root, "Karborund-Order", KARBORUND, "");
    order(
        &root,
        "Molybdaenit-Order",
        MOLYBDAENIT,
        "Molybdaenit Notiz\n",
    );
    let package = loaded(&root);
    assert_eq!(
        package
            .contents
            .damaged_orders
            .iter()
            .filter_map(|order| order.order_id.as_ref())
            .map(|order_id| order_id.deref().to_owned())
            .collect::<Vec<_>>(),
        ["4d5e6f7a-8b9c-4d0e-b1f2-a3b4c5d6e7f8"],
        "the Order is in the Package and the load quarantined it"
    );

    let query = SearchQuery::new("molybdaenit", CaseSensitivity::Insensitive);
    let matched = matching_orders(&package, &query);
    assert!(
        matched.is_empty(),
        "a Damaged Order is in no Order result: {:?}",
        titles(&matched)
    );
    assert!(
        matching_vendors(&package, &query).is_empty(),
        "and in no Vendor result"
    );

    let matched = matching_orders(
        &package,
        &SearchQuery::new("karborund", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Karborund Läppscheibe"],
        "the healthy Order beside it still matches"
    );
}

const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

const TARNWICK_FULL: &str = r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": [
        {
          "name": "Grace Hopper",
          "role": "Vertrieb"
        }
      ],
      "purchasing_notes": "Zahlung per Kobaltblau"
    }
  ]
}
"#;

/// A Vendor's current facts reach every Order that names it, so editing the Vendor moves
/// those Orders in and out of a result while their records stay exactly as they were.
///
/// The defect this catches is an Order that carries a copy of its Vendor. A search built
/// over the Order record alone, or over a snapshot taken when the Order was written, would
/// go on matching the old purchasing notes and would never match the new ones — and the
/// only way to correct it would be rewriting every Order that names the Vendor.
#[test]
fn editing_a_vendor_changes_which_orders_match_without_touching_those_orders() {
    let (_tree, root) = package("entity-search-vendor-reach");
    vendors(&root, TARNWICK_FULL);
    let directory = order(&root, "Karborund-Order", KARBORUND, "");
    let record = directory.join(".spielorders/order.json");
    let before = std::fs::read(&record).expect("the Order record is readable");

    let package = loaded(&root);
    let matched = matching_orders(
        &package,
        &SearchQuery::new("hopper kobaltblau", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Karborund Läppscheibe"],
        "a Vendor Contact and the purchasing notes both reach the Order that names the \
         Vendor"
    );

    let plan = crate::plan(
        &edit_vendor(
            root.clone(),
            VendorRef::Id(String::from(VENDOR_ID)),
            Assignments::new().assign("purchasing_notes", "Zahlung per Zinnober"),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Vendor's purchasing notes can be edited");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem).minting(QueuedUuids::of(&[]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    assert_eq!(
        std::fs::read(&record).expect("the Order record is still readable"),
        before,
        "editing the Vendor wrote nothing to the Order that names it"
    );

    let package = loaded(&root);
    let matched = matching_orders(
        &package,
        &SearchQuery::new("kobaltblau", CaseSensitivity::Insensitive),
    );
    assert!(
        matched.is_empty(),
        "the purchasing notes the Vendor no longer carries reach nothing: {:?}",
        titles(&matched)
    );
    let matched = matching_orders(
        &package,
        &SearchQuery::new("zinnober", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Karborund Läppscheibe"],
        "the purchasing notes the Vendor now carries reach the Order"
    );
}

const ANGSTROM: &str = r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Ångström Optik",
      "contacts": []
    }
  ]
}
"#;

/// Diacritics never distinguish, case distinguishes only when the caller asks, and asking
/// changes nothing else.
///
/// The defect this catches is a case control implemented as a second matching mode. Case
/// sensitivity is one rule of several: a case-sensitive query still folds `Ångström` and
/// `Angstrom` together, still matches a substring inside a word, and still combines its
/// terms with AND. An implementation that switched to comparing raw text would quietly
/// take the diacritic rule away with the case rule.
#[test]
fn diacritics_never_distinguish_and_only_the_case_rule_follows_the_case_control() {
    let (_tree, root) = package("entity-search-folding");
    vendors(&root, ANGSTROM);
    order(&root, "Karborund-Order", KARBORUND, "");
    let package = loaded(&root);

    for query in ["Ångström Optik", "ångström optik", "ANGSTROM", "angstrom"] {
        let matched = matching_vendors(
            &package,
            &SearchQuery::new(query, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            names(&matched),
            ["Ångström Optik"],
            "{query:?} differs from the Vendor Name only by diacritics or case"
        );
    }

    let matched = matching_orders(
        &package,
        &SearchQuery::new("angstrom", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Karborund Läppscheibe"],
        "the Vendor's Name reaches the Order that names it, folded the same way"
    );

    for query in ["Angstrom Optik", "strö", "Optik Angstrom"] {
        let matched = matching_vendors(
            &package,
            &SearchQuery::new(query, CaseSensitivity::Sensitive),
        );
        assert_eq!(
            names(&matched),
            ["Ångström Optik"],
            "{query:?}: a case-sensitive query still folds diacritics, still matches a \
             substring, and still combines terms with AND"
        );
    }

    for query in ["angstrom", "ÅNGSTRÖM", "optik"] {
        let matched = matching_vendors(
            &package,
            &SearchQuery::new(query, CaseSensitivity::Sensitive),
        );
        assert!(
            matched.is_empty(),
            "{query:?} differs from the Vendor Name by case, which is the one thing the \
             control changes: {:?}",
            names(&matched)
        );
    }
}

/// A term is a literal substring, and the query around it spells no grammar at all.
///
/// The defect this catches is an implementation that reached for a query language: an
/// anchored or word-boundary match would lose `orund` inside `Karborund`, a phrase
/// grammar would give the quotation marks a meaning they do not have and would care what
/// order the terms arrive in, a Boolean grammar would let `OR` widen the query instead of
/// narrowing it with a third term, and a wildcard or an edit distance would match text the
/// Package does not contain.
#[test]
fn a_term_is_a_literal_substring_and_the_query_has_no_grammar() {
    let (_tree, root) = package("entity-search-literal-terms");
    vendors(&root, TARNWICK);
    order(&root, "Karborund-Order", KARBORUND, "");
    let package = loaded(&root);

    for query in ["orund", "läppscheibe karborund"] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(query, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Karborund Läppscheibe"],
            "{query:?} matches inside the Title, whatever order its terms arrive in"
        );
    }

    for query in [
        "\"karborund läppscheibe\"",
        "karborund OR nethermoor",
        "karbor*nd",
        "karborond",
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(query, CaseSensitivity::Insensitive),
        );
        assert!(
            matched.is_empty(),
            "{query:?} spells no grammar, so its punctuation and its extra term are \
             ordinary characters that match nothing: {:?}",
            titles(&matched)
        );
    }
}

/// A derived Package Index naming this Package's one Order under a Title its record does
/// not carry.
///
/// A projection of exactly the shape a load writes, so the term below is in the derived
/// index the way any other term would be, rather than in a file that merely sits at the
/// path.
const PLANTED_INDEX: &str = r#"{
  "schema_version": 1,
  "orders": [
    {
      "order_directory": "Bestellungen/Molybdaen",
      "order": {
        "schema_version": 1,
        "package_root": "../..",
        "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "title": "Kryolith Schleifpaste",
        "date_ordered": "2026-07-19",
        "status": "ordered",
        "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
        "orderer": "Ada Lovelace",
        "items": []
      },
      "readme": ""
    }
  ],
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": []
    }
  ]
}
"#;

/// Nothing outside an entity's own facts is searched: not where it is kept, not what is
/// kept beside it, not the Package's own document, and not the index derived from it all.
///
/// Every term below appears in exactly one excluded source and nowhere else in the
/// Package, so each one names the source it would have come from. The defect this catches
/// is a search widened by convenience — folding in the Order Directory basename because it
/// is right there in the projection, or the Supporting Material beside the record because
/// a directory listing was already in hand — which would answer a Vendors query with an
/// Order whose only connection to the term is the folder someone happened to name.
#[test]
fn no_path_no_neighbouring_file_and_no_derived_artifact_is_searched() {
    let (_tree, root) = package("entity-search-exclusions-Wolframit");
    // Planted before the records it disagrees with, so it is not proven current and the
    // open reads the records rather than projecting the Package out of it.
    std::fs::write(root.join(".spielorders/index.json"), PLANTED_INDEX)
        .expect("the derived Package Index");
    vendors(&root, TARNWICK);
    let directory = order(&root, "Molybdaen", KARBORUND, "Wismut in der Notiz.\n");
    std::fs::write(
        directory.join("Angebot-Kobaltit.pdf"),
        "Vanadinit Preisliste\n",
    )
    .expect("the Supporting Material");
    std::fs::write(root.join("README.md"), "Paketnotiz Mikroskop\n").expect("the Package README");
    let package = loaded(&root);
    assert!(
        std::fs::read_to_string(root.join(".spielorders/index.json"))
            .expect("the derived Package Index is readable")
            .contains("Kryolith"),
        "the planted Index is still what stands on disk while the search runs"
    );

    for (term, source) in [
        ("Wolframit", "the Package Root's own directory name"),
        ("Bestellungen", "the configured Orders Directory"),
        ("Entfernte", "the configured Removed Orders Directory"),
        ("Molybdaen", "the Order Directory basename"),
        ("Kobaltit", "a Supporting Material filename"),
        ("Vanadinit", "what that Supporting Material holds"),
        ("Mikroskop", "the Package README"),
        ("Kryolith", "the derived Package Index"),
        ("README.md", "the conventional README's filename"),
        ("order.json", "the Order record's filename"),
        (".spielorders", "the reserved subtree's name"),
    ] {
        let query = SearchQuery::new(term, CaseSensitivity::Insensitive);
        let matched = matching_orders(&package, &query);
        assert!(
            matched.is_empty(),
            "{term:?} appears only in {source}, which no Order matches on: {:?}",
            titles(&matched)
        );
        assert!(
            matching_vendors(&package, &query).is_empty(),
            "{term:?} appears only in {source}, which no Vendor matches on"
        );
    }

    for (term, fact) in [
        ("karborund", "the Title"),
        ("wismut", "the Order README's text"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Karborund Läppscheibe"],
            "{term:?} is {fact}: the answers above are exclusions rather than silence, and \
             the Order README is searched where the Package README is not"
        );
    }
    assert_eq!(
        names(&matching_vendors(
            &package,
            &SearchQuery::new("tarnwick", CaseSensitivity::Insensitive)
        )),
        ["Tarnwick"],
        "and so is the Vendor"
    );
}

const NO_VENDORS: &str = r#"{
  "schema_version": 1,
  "vendors": []
}
"#;

/// One Order carrying no digit `1` in any fact it is searched on, and Items whose totals
/// are the only place `21` could come from.
const SCHLEIFPASTE: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e0-4b6652a56d06",
  "title": "Karborund Schleifpaste",
  "date_ordered": "2026-07-09",
  "status": "ordered",
  "items": [
    {
      "description": "Spiegelhalter",
      "quantity": 3.0,
      "unit_price": 7.0,
      "catalog_number": "KMX"
    }
  ]
}
"#;

/// A record's own syntax, its technical version, and the numbers calculated from it are
/// not facts.
///
/// The Order below is built so that each term proves one thing on its own. No fact it is
/// searched on carries the digit `1`, so `1` reaching nothing is the schema version and the
/// metadata version staying out; and its one Item is three at seven, so `21` is spelled by
/// the Item Total and the Order Total and by nothing else. The defect this catches is a
/// search assembled by serializing a record and matching the text: every field name would
/// become searchable content, and a query naming a field would return every Order that has
/// one.
#[test]
fn a_records_syntax_its_version_and_its_calculated_totals_are_not_facts() {
    let (_tree, root) = package("entity-search-record-internals");
    vendors(&root, NO_VENDORS);
    order(&root, "Werkzeug", SCHLEIFPASTE, "");
    let package = loaded(&root);

    for (term, source) in [
        ("1", "the schema version and the metadata version"),
        ("21", "the calculated Item Total and Order Total"),
        ("schema_version", "a raw JSON key"),
        ("package_root", "a raw JSON key"),
        ("order_id", "a raw JSON key"),
        ("date_ordered", "a raw JSON key"),
        ("catalog_number", "a raw JSON key"),
        ("unit_price", "a raw JSON key"),
        ("items", "a raw JSON key"),
        ("{\"", "raw JSON syntax"),
        ("../..", "the record's technical locator"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert!(
            matched.is_empty(),
            "{term:?} is {source}, which no Order matches on: {:?}",
            titles(&matched)
        );
    }

    for (term, fact) in [
        ("8b6e3f0a", "the Order ID"),
        ("karborund", "the Title"),
        ("2026-07-09", "the Date Ordered"),
        ("ordered", "the Order Status"),
        ("spiegelhalter", "an Item description"),
        ("kmx", "an Item catalog number"),
        ("3", "an Item Quantity"),
        ("7", "an Item Unit Price"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Karborund Schleifpaste"],
            "{term:?} is {fact}, which the Order does match on"
        );
    }
}

/// One Order whose Unit Prices are the only monetary facts in the Package, neither of
/// them stored with two decimal places.
const PREISPROBE: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
  "title": "Preisprobe",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "items": [
    {
      "description": "Kollimatorwort",
      "quantity": 4,
      "unit_price": 13.5
    }
  ]
}
"#;

/// One Order whose Quantity is the only fractional Quantity in the Package.
///
/// Its own Unit Price is whole, so this Order is also where a monetary value stored with
/// no decimal places at all is asked for the way a reader sees it.
const MENGENPROBE: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "6d5c4b3a-2e1f-4b8a-9c0d-1e2f3a4b5c6d",
  "title": "Mengenprobe",
  "date_ordered": "2026-07-18",
  "status": "ordered",
  "items": [
    {
      "description": "Halterwort",
      "quantity": 2.5,
      "unit_price": 40
    }
  ]
}
"#;

/// A Unit Price is matched by the spelling a reader is shown as well as by the one the
/// record stores; a Quantity is matched by the stored spelling alone.
///
/// The defect this catches is a reader searching what they were shown and finding
/// nothing: a Unit Price of `13.5` reads as `13.50`, so `13.50` is what gets typed. The
/// other half is the over-correction — two decimal places given to every number would
/// make `2.50` name a Quantity of `2.5`, which is not money and carries no such
/// convention, and would make the Item Total and the Order Total matchable by a second
/// spelling of numbers the decision excludes outright.
///
/// The two Orders keep the two claims apart on purpose: `13.50` holds `2.50` inside it,
/// so a Package where one Item carried both facts could not tell a Quantity's spelling
/// from a Unit Price's.
#[test]
fn a_unit_price_is_matched_as_it_is_read_and_a_quantity_is_not_money() {
    let (_tree, root) = package("entity-search-monetary-spelling");
    vendors(&root, NO_VENDORS);
    order(&root, "Preis", PREISPROBE, "");
    order(&root, "Menge", MENGENPROBE, "");
    let package = loaded(&root);

    for (term, spelling) in [
        ("13.5", "the Unit Price as the record stores it"),
        ("13.50", "the Unit Price as a reader is shown it"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Preisprobe"],
            "{term:?} is {spelling}, which the Order matches on"
        );
    }

    let matched = matching_orders(
        &package,
        &SearchQuery::new("40.00", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Mengenprobe"],
        "a Unit Price stored with no decimal places is still shown with two"
    );

    let matched = matching_orders(
        &package,
        &SearchQuery::new("2.5", CaseSensitivity::Insensitive),
    );
    assert_eq!(
        titles(&matched),
        ["Mengenprobe"],
        "a Quantity is matched on the spelling the record stores"
    );

    for (term, source) in [
        (
            "2.50",
            "a Quantity given a monetary value's two decimal places",
        ),
        ("4.00", "a whole Quantity given them"),
        (
            "54.00",
            "the calculated Item Total and Order Total of one Order",
        ),
        ("100.00", "those of the other"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert!(
            matched.is_empty(),
            "{term:?} would be {source}, which no Order matches on: {:?}",
            titles(&matched)
        );
    }
}

/// One Order carrying every canonical fact it can carry, each spelled by a word that
/// appears nowhere else in the Package.
const EVERY_ORDER_FACT: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "5e6f7a8b-9c0d-4e1f-a2b3-c4d5e6f7a8b9",
  "title": "Titelwort",
  "date_ordered": "2026-07-19",
  "status": "received",
  "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
  "orderer": "Ordererwort",
  "ordering_institution": "Institutwort",
  "currency": "XTS",
  "project": "Projektwort",
  "order_group": "Gruppenwort",
  "items": [
    {
      "description": "Beschreibungswort",
      "quantity": 4.25,
      "unit_price": 9.75,
      "catalog_number": "Katalogwort"
    }
  ]
}
"#;

/// One Vendor carrying every shared fact it can carry, and one Contact carrying every
/// fact a Contact can carry.
const EVERY_VENDOR_FACT: &str = r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Vendorwort",
      "phone": "Telefonwort",
      "email": "mailwort@example.com",
      "website": "https://webseitenwort.example.com",
      "address": "Adresswort 5\n12345 Ortwort",
      "contacts": [
        {
          "name": "Kontaktwort",
          "role": "Rollenwort",
          "phone": "Kontaktruf",
          "email": "kontaktpost@example.com"
        }
      ],
      "purchasing_notes": "Notizwort"
    }
  ]
}
"#;

/// Every canonical fact an Order and a Vendor carry is matched on, and a Vendor's shared
/// facts carry across to the Order that names it.
///
/// The defect this catches is a forgotten optional field. Each fact below is spelled by a
/// word that appears in no other fact of the Package, so a search that assembled only the
/// facts it happened to need — the Title and the Item descriptions, say — loses one
/// assertion per field it left out rather than passing on the strength of the fields it
/// remembered. One Vendor ID appears in both lists below, because it is the Vendor's
/// identity and the Vendor Reference the Order stores, and the decision excludes neither.
#[test]
fn every_canonical_fact_of_an_order_and_of_its_vendor_is_matched_on() {
    let (_tree, root) = package("entity-search-every-fact");
    vendors(&root, EVERY_VENDOR_FACT);
    order(&root, "Alles", EVERY_ORDER_FACT, "Readmewort\n");
    let package = loaded(&root);

    for (term, fact) in [
        ("5e6f7a8b-9c0d-4e1f-a2b3-c4d5e6f7a8b9", "the Order ID"),
        ("titelwort", "the Order Title"),
        ("2026-07-19", "the Date Ordered"),
        ("received", "the Order Status"),
        ("ordererwort", "the Orderer"),
        ("institutwort", "the Ordering Institution"),
        ("xts", "the Order Currency"),
        ("projektwort", "the Project"),
        ("gruppenwort", "the Order Group"),
        ("beschreibungswort", "an Item description"),
        ("katalogwort", "an Item catalog number"),
        ("4.25", "an Item Quantity"),
        ("9.75", "an Item Unit Price"),
        ("readmewort", "the Order README's text"),
        ("vendorwort", "the Vendor's Name"),
        ("telefonwort", "the Vendor's phone"),
        ("mailwort", "the Vendor's email"),
        ("webseitenwort", "the Vendor's website"),
        ("adresswort", "the Vendor's address"),
        ("ortwort", "a later line of the Vendor's address"),
        ("notizwort", "the Vendor's purchasing notes"),
        ("kontaktwort", "a Vendor Contact's name"),
        ("rollenwort", "a Vendor Contact's role"),
        ("kontaktruf", "a Vendor Contact's phone"),
        ("kontaktpost", "a Vendor Contact's email"),
        (
            "e0990d31-06aa-42e9-97c8-088e3e894822",
            "the Vendor Reference the Order stores",
        ),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Titelwort"],
            "{term:?} is {fact}, which reaches the Order"
        );
    }

    for (term, fact) in [
        ("e0990d31-06aa-42e9-97c8-088e3e894822", "the Vendor ID"),
        ("vendorwort", "the Vendor's Name"),
        ("webseitenwort", "the Vendor's website"),
        ("ortwort", "a later line of the Vendor's address"),
        ("kontaktpost", "a Vendor Contact's email"),
    ] {
        assert_eq!(
            names(&matching_vendors(
                &package,
                &SearchQuery::new(term, CaseSensitivity::Insensitive)
            )),
            ["Vendorwort"],
            "{term:?} is {fact}, which reaches the Vendor"
        );
    }
}

/// An Order README written the way a README is written: a heading, a link, an emphasis
/// inside a word, a nested list, some inline code, raw HTML, a strikethrough, and a table.
const MARKDOWN_README: &str = "\
## Notiz zur Lieferung

Siehe das [Angebot](Angebot-Kobaltit.pdf) und **Karbo**rund.

- `Katalognummer` KMX
- Wolfram
  - Molybdaen

<p align=\"center\">Zentriertwort</p>

Und <a href=\"Kobaltit>Angebot.pdf\">Linkwort</a> dazu.

~~Storniert~~ und Ersatzwort.

| Spalte | Wert |
| --- | --- |
| Tabellenwort | Zellenwort |
";

/// An Order README is searched as text and not as the Markdown it is written in.
///
/// The defect this catches is the exclusion that lets the others back in. Matching the
/// document as written makes a link's destination searchable — and a destination is a
/// filename or a filesystem path, both excluded in their own right — so `Angebot` reaching
/// the Order while `Angebot-Kobaltit.pdf` reaches nothing is what says the text was read
/// rather than the markup, and the same holds for an HTML tag's attributes beside the text
/// it wraps. The last two cases are the other direction, where the boundaries matter
/// rather than the markers: `**Karbo**rund` is one word to a reader and must not be broken
/// in two, while two list items are two words and must not be run into one.
#[test]
fn an_order_readme_is_searched_as_text_and_not_as_the_markdown_it_is_written_in() {
    let (_tree, root) = package("entity-search-readme-markdown");
    vendors(&root, NO_VENDORS);
    order(&root, "Notiz", BETA, MARKDOWN_README);
    let package = loaded(&root);

    for (term, text) in [
        ("beta", "the Order Title, which the README does not carry"),
        ("notiz zur lieferung", "a heading's own words"),
        ("angebot", "a link's text"),
        ("karborund", "a word an emphasis interrupts"),
        ("katalognummer", "inline code"),
        ("kmx", "a list item beside it"),
        (
            "wolfram molybdaen",
            "a list item and the nested one under it",
        ),
        ("zentriertwort", "what a raw HTML paragraph holds"),
        ("linkwort", "what a raw HTML anchor wraps"),
        (
            "storniert ersatzwort",
            "struck-through text and the text beside it",
        ),
        ("tabellenwort zellenwort", "two cells of a table"),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Beta Spiegel"],
            "{term:?} is {text}, which the Order matches on"
        );
    }

    for (term, markup) in [
        ("##", "a heading's marker"),
        ("**", "an emphasis marker"),
        ("[Angebot]", "a link's brackets"),
        (
            "Angebot-Kobaltit.pdf",
            "a link's destination, which is a Supporting Material filename",
        ),
        (".pdf", "the extension inside that destination"),
        ("`", "an inline code delimiter"),
        ("~~", "a strikethrough marker"),
        ("|", "a table's cell separator"),
        ("<p", "a raw HTML tag"),
        ("align", "an attribute name inside that tag"),
        ("center", "that attribute's value"),
        (
            "Kobaltit>Angebot.pdf",
            "a quoted attribute value carrying the character that ends a tag",
        ),
        (
            "wolframmolybdaen",
            "two list items with the boundary between them taken away",
        ),
    ] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert!(
            matched.is_empty(),
            "{term:?} is {markup}, which is not the README's text: {:?}",
            titles(&matched)
        );
    }
}

/// The answer is the same whichever load produced the projection, including the one that
/// reads the derived Package Index instead of the canonical records.
///
/// The defect this catches is an exclusion that only holds on one route. A current index
/// stores each Order's Directory as a Package-relative path, so a load that projects an
/// Order out of it has that path in hand — and a search reading the projection's own path
/// fields would match on a directory basename for exactly the callers that take the fast
/// path. The Order's README travels in the index too, so the other half is that it is
/// still searched here.
#[test]
fn the_index_fast_path_answers_on_the_same_facts() {
    let (_tree, root) = package("entity-search-index-fast-path");
    vendors(&root, TARNWICK);
    order(&root, "Molybdaen", KARBORUND, "Wismut in der Notiz.\n");
    // A rebuild leaves a current derived index behind; the open projects the Package out
    // of it rather than from the canonical records.
    committed(&rebuild_index(root.clone()), &effects(&PlatformFileSystem));
    assert!(
        index_is_current(&root),
        "the open takes the current-index fast path"
    );
    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("the fixture Package opens");
    };

    for (term, fact) in [("karborund", "the Title"), ("wismut", "the README's text")] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert_eq!(
            titles(&matched),
            ["Karborund Läppscheibe"],
            "{term:?} is {fact}, which the index carries and the Order matches on"
        );
    }

    for term in ["Molybdaen", "Bestellungen"] {
        let matched = matching_orders(
            &package,
            &SearchQuery::new(term, CaseSensitivity::Insensitive),
        );
        assert!(
            matched.is_empty(),
            "{term:?} is a segment of the Order Directory the index stores, which no Order \
             matches on: {:?}",
            titles(&matched)
        );
    }
}
