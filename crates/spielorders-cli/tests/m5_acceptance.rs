//! The three `search` commands over one Package, composed through the executable alone.
//!
//! **Each search ticket proved its own engine; this proves the two answer about one
//! Package.** The entity searches ask the core about records the load already read, the
//! Supporting Material search asks a platform service, and nothing is shared between them
//! but the Package. What no other test in this suite can observe is both engines put to one
//! Package that holds Orders, Vendors, Supporting Material and a Damaged Order at once,
//! over a layout no search fixture uses.
//!
//! **The layout is the point.** [`ORDERS_PATH`] moves the Orders Directory to a configured
//! path below the Root, so an Order Directory under it stands four parents from the Root
//! instead of the default two, and [`REMOVED_PATH`] renames the Removed Orders Directory. A
//! scoping rule that counted the default depth would be right by accident under every other
//! search fixture. That the layout is in force is read off the Order Directory the
//! executable reports rather than assumed.
//!
//! **Every claim here is made where the property has to hold.** These would otherwise
//! certify nothing:
//!
//! - *A term reaches an Order through its Vendor.* [`VENDOR_ONLY_TERM`] is checked absent
//!   from every byte under the Order's own Directory first, so the Vendor is the only place
//!   the match can have come from. A test that let the term stand in the Order's record
//!   would pass on an engine that never resolved a Vendor.
//! - *A Damaged Order is in neither engine.* The Order is searched for and found **while it
//!   is healthy**, and only then damaged, so [`DAMAGED_TERM`] is known to be a real
//!   searchable fact rather than a word nothing carried. That the damaged Directory is
//!   still enumerated afterwards is read off the load's own observation in the envelope,
//!   so the empty answer is an Order excluded and not an Order that was never there.
//! - *The engines never mix.* [`MATERIAL_ONLY_TERM`] is written into a file's name and its
//!   contents inside a *healthy* Order Directory, and asserted present there, so an entity
//!   engine that consulted a neighbouring file would match it.
//!
//! **What this file deliberately does not claim.** No assertion here depends on what the
//! platform's index happens to hold: Spotlight indexes a temporary directory late or never,
//! so an empty Supporting Material answer is no evidence about scoping or exclusion. What is
//! asserted instead is that the second engine answers at all about a Package the first
//! engine has just reported damage in. An assertion that no group named the damaged Order
//! would hold against an answer that carried no groups at all, which is the shape of a check
//! that certifies nothing. That the Damaged Order is neither a scope nor a group is proven
//! where a provider can be made to report a file under one: `spielorders-core`'s
//! `tests/supporting_material_search.rs` for the core's scoping, and this crate's
//! `tests/platform_index_and_opener.rs` for the macOS adapter's.
//!
//! **Four properties of that engine cannot be reached through the shipped executable, and
//! are not attempted here.** The bound is a private constant, the cancellation is wired to
//! an uninterrupted query, and neither a path reported twice nor a refused query can be
//! conjured out of a real index. Each has a home that prepares the provider it needs:
//! deduplication by path, the elapsed bound, and every state that prevents an answer are
//! `spielorders-core`'s `tests/supporting_material_search.rs`; a build with no provider is
//! this crate's own `commands::search` suite; and the adapter's half of the bound is
//! `tests/platform_index_and_opener.rs`. What a caller's interrupt does to this executable
//! is a different question from the port's cancellation, and this crate's
//! `tests/supporting_material_search.rs` asks it of a real process. Naming them here rather
//! than restating their claims, because a second copy of a claim is a second thing to keep
//! true.
//!
//! Nothing here parses human output. Every step reads the `--json` document.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{
    Invocation, Outcome, TemporaryTree, managed_snapshot, modification_time, response,
    supporting_material_provider,
};
use std::path::{Path, PathBuf};

/// The Orders Directory this composition configures: nested, and not the default.
const ORDERS_PATH: &str = "archive/2026/Orders";

/// The Removed Orders Directory this composition configures: renamed, and not the default.
const REMOVED_PATH: &str = "Withdrawn";

/// A word carried by one Vendor's Address and by no fact of any Order.
///
/// **The Vendor Name would prove nothing.** An Order's record stores the Name of the Vendor
/// it references, so a term landing on that Name lands on a fact the Order carries itself —
/// and an engine that never resolved a Vendor would retain the Order anyway. An Address is
/// the Vendor's alone.
const VENDOR_ONLY_TERM: &str = "example";

/// A word carried only by one Supporting Material file, in its name and in its contents.
const MATERIAL_ONLY_TERM: &str = "tantalum";

/// A word carried by one Order's Title, which is damaged partway through the composition.
const DAMAGED_TERM: &str = "krypton";

/// The Vendor whose Address alone carries [`VENDOR_ONLY_TERM`].
const REFERENCED_VENDOR: &str = "Tarnwick";

/// The Vendor the Order that gets damaged references, and which is hidden partway through.
const HIDDEN_VENDOR: &str = "Nethermoor";

// ---------------------------------------------------------------------------
// Running the executable.
// ---------------------------------------------------------------------------

/// One `--json` run of the real executable, as a caller writes it.
///
/// `response` refuses a second document, trailing text, or anything at all on standard
/// error, so every step makes the one-document claim as well as its own.
fn run(line: &[&str], root: &Path, arguments: &[&str]) -> (Outcome, serde_json::Value) {
    let mut invocation = Invocation::new(line);
    for argument in arguments {
        invocation = invocation.argument(argument);
    }
    let outcome = invocation.rooted_at(root).argument("--json").run();
    let document = response(&outcome);
    (outcome, document)
}

/// One run that answered: exit `0`, `ok`, and a payload.
fn answered(line: &[&str], root: &Path, arguments: &[&str]) -> serde_json::Value {
    let (outcome, document) = run(line, root, arguments);
    assert_eq!(
        outcome.status,
        Some(0),
        "{line:?} {arguments:?}: {}",
        outcome.stdout_text()
    );
    assert_eq!(document["ok"], true, "{line:?} {arguments:?}: {document}");
    document
}

/// The Order Titles one entity answer carries, in the order it carries them.
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

/// The Vendor Names one entity answer carries, in the order it carries them.
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

/// Whether the load reported an Order-scoped observation about something inside
/// `directory`.
///
/// **The path, not the code.** What has to be true for a Damaged Order's absence to mean
/// anything is that the load walked that Directory and reported what it found there — so
/// this is asserted against the location the diagnostic names, which no observation about
/// some other Order can satisfy.
fn load_observed_inside(document: &serde_json::Value, directory: &Path) -> bool {
    document["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("every response carries a diagnostics collection: {document}"))
        .iter()
        .any(|diagnostic| {
            diagnostic["scope"] == "order"
                && diagnostic["primary_location"]["absolute_path"]
                    .as_str()
                    .is_some_and(|path| Path::new(path).starts_with(directory))
        })
}

/// The absolute path one reported location names.
fn absolute(location: &serde_json::Value) -> PathBuf {
    PathBuf::from(
        location["absolute_path"]
            .as_str()
            .expect("a reported location names an absolute path"),
    )
}

/// Which of the three shapes one Supporting Material run settled into, with the claim that
/// shape has to satisfy already made. Answers whether a payload arrived, so a caller can go
/// on to ask about it.
///
/// **Three shapes rather than two, because the second engine's answer depends on the
/// machine.** A build that ships a provider still reports rather than answers when the
/// platform's index is switched off, an unindexed volume was asked, a directory could not be
/// read, or the query was refused. A composition that demanded exit `0` wherever a provider
/// exists would fail on a Mac with Spotlight switched off, for a reason that is not about
/// this code, and a gate only a particular machine can pass has certified that machine.
///
/// **The branch is a claim, not a way out.** A build that ships a provider is asserted not
/// to be in the one state whose whole meaning is having none, so a reported failure still
/// has to be a state that could have happened to a provider that exists.
fn material_answered(outcome: &Outcome, material: &serde_json::Value) -> bool {
    let named = supporting_material_provider();
    let code = material["error"]["code"].as_str();
    match (named, material["ok"].as_bool()) {
        (Some(named), Some(true)) => {
            assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
            assert_eq!(
                material["data"]["provider"], named,
                "the provider that looked, named in its own words: {material}"
            );
            // The pair, not the value: whether this run settled inside the bound is the
            // platform's business, and demanding that it did would fail this composition
            // for a slow index. What must hold either way is that the two halves agree —
            // a partial answer names which of the two reasons made it partial, and a
            // complete one names none.
            assert_eq!(
                material["data"]["complete"],
                material["data"]["partial_reason"].is_null(),
                "{material}"
            );
            true
        }
        (Some(_), _) => {
            assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
            assert!(
                material["data"].is_null(),
                "no provider answered, so there is no answer to carry: {material}"
            );
            assert!(
                code.is_some_and(|code| code.starts_with("supporting_material_")),
                "the state that prevented an answer is named rather than reported as an \
                 empty one: {material}"
            );
            assert_ne!(
                code,
                Some("supporting_material_unsupported"),
                "this build ships a provider, so the one state it cannot be in is having \
                 none at all: {material}"
            );
            false
        }
        (None, _) => {
            assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
            assert_eq!(material["ok"], false, "{material}");
            assert!(
                material["data"].is_null(),
                "nothing looked, so there is no answer to carry: {material}"
            );
            assert_eq!(code, Some("supporting_material_unsupported"), "{material}");
            false
        }
    }
}

/// Whether `term` appears in any byte of any file under `directory`, case folded.
///
/// **What makes the Vendor traversal claim a claim.** Every file is read, the managed record
/// and the conventional README included, so a term the Order carries anywhere of its own
/// cannot hide from this.
fn term_appears_under(directory: &Path, term: &str) -> bool {
    managed_snapshot(directory)
        .into_iter()
        .any(|(path, bytes)| {
            let in_name = path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains(&term.to_ascii_lowercase());
            let in_bytes = bytes.is_some_and(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .to_ascii_lowercase()
                    .contains(&term.to_ascii_lowercase())
            });
            in_name || in_bytes
        })
}

// ---------------------------------------------------------------------------
// The composition.
// ---------------------------------------------------------------------------

/// A Package initialized over the non-default layout, holding two Orders, two Vendors and
/// Supporting Material — and the two Order Directories.
///
/// Built through the executable, because the subject is what a caller can compose from the
/// command surface.
fn composed(label: &str) -> (TemporaryTree, PathBuf, PathBuf, PathBuf) {
    let tree = TemporaryTree::new(label);
    let root = std::fs::canonicalize(tree.path()).expect("the temporary directory resolves");

    let package = answered(
        &["init"],
        &root,
        &[
            &format!("orders_path={ORDERS_PATH}"),
            &format!("removed_path={REMOVED_PATH}"),
        ],
    );
    assert_eq!(package["data"]["package"]["orders_path"], ORDERS_PATH);
    assert_eq!(package["data"]["package"]["removed_path"], REMOVED_PATH);

    answered(
        &["add-vendor"],
        &root,
        &[
            &format!("name={REFERENCED_VENDOR}"),
            "address=100 Example Avenue\nAnytown, ST 00000",
            "purchasing_notes=Quote required above $5000.",
        ],
    );
    answered(&["add-vendor"], &root, &[&format!("name={HIDDEN_VENDOR}")]);

    let referencing = answered(
        &["add-order"],
        &root,
        &[
            "title=Laser components",
            "date_ordered=2026-08-22",
            &format!("vendor={REFERENCED_VENDOR}"),
            "orderer=Ada Lovelace",
            "project=Telescope",
        ],
    );
    let referencing = absolute(&referencing["data"]["order"]["paths"]["directory"]);

    let damaged = answered(
        &["add-order"],
        &root,
        &[
            &format!("title={DAMAGED_TERM} lamp"),
            "date_ordered=2026-08-21",
            &format!("vendor={HIDDEN_VENDOR}"),
            "orderer=Ada Lovelace",
        ],
    );
    let damaged = absolute(&damaged["data"]["order"]["paths"]["directory"]);

    // The layout, read off what the executable reported rather than assumed: an Order
    // Directory under this Package stands below the configured Orders Directory, which is
    // four levels down instead of the default one.
    for directory in [&referencing, &damaged] {
        assert!(
            directory.starts_with(root.join(ORDERS_PATH)),
            "the Order Directory stands under the configured Orders Directory: {}",
            directory.display()
        );
    }

    std::fs::write(
        referencing.join(format!("{MATERIAL_ONLY_TERM}-quote.txt")),
        format!("One sputtering target, {MATERIAL_ONLY_TERM}, 99.95%.\n"),
    )
    .expect("the Supporting Material file is written");
    std::fs::write(
        damaged.join(format!("{DAMAGED_TERM}-invoice.txt")),
        b"Invoice, paid by card.\n",
    )
    .expect("the Supporting Material file under the damaged Order is written");

    (tree, root, referencing, damaged)
}

/// Both engines answer about one Package, over a layout neither ticket's fixtures use.
///
/// **One test rather than a dozen, because the subject is the composition.** Every claim
/// below has a home that proves it in isolation; what only this can observe is one Package
/// carrying Orders, Vendors, Supporting Material and a Damaged Order through all three
/// commands in order, with the Package compared byte for byte at the end.
#[test]
fn both_search_engines_answer_about_one_package_through_the_executable_alone() {
    let (_tree, root, referencing, damaged) = composed("m5-acceptance-composition");

    // -----------------------------------------------------------------------
    // A term reaches an Order through the Vendor it references.
    // -----------------------------------------------------------------------

    assert!(
        !term_appears_under(&referencing, VENDOR_ONLY_TERM),
        "no byte under the Order's own Directory carries the term, so its Vendor is the \
         only place a match can come from"
    );
    assert_eq!(
        titles(&answered(&["search", "orders"], &root, &[VENDOR_ONLY_TERM])),
        ["Laser components"],
        "the Order that references the Vendor whose Address carries the term"
    );
    assert_eq!(
        names(&answered(
            &["search", "vendors"],
            &root,
            &[VENDOR_ONLY_TERM]
        )),
        [REFERENCED_VENDOR],
        "the same fact, asked of the other entity engine"
    );

    // -----------------------------------------------------------------------
    // The two engines never mix.
    // -----------------------------------------------------------------------

    assert!(
        term_appears_under(&referencing, MATERIAL_ONLY_TERM),
        "the term stands in a Supporting Material file's name and contents inside a healthy \
         Order Directory, which is what an entity engine must not reach"
    );
    for (command, collection, members) in [
        ("orders", "orders", vec!["orders", "query"]),
        (
            "vendors",
            "vendors",
            vec!["hidden_vendors_included", "query", "vendors"],
        ),
    ] {
        let document = answered(&["search", command], &root, &[MATERIAL_ONLY_TERM]);
        assert_eq!(
            document["data"][collection],
            serde_json::json!([]),
            "no entity carries a word only a Supporting Material file carries: {document}"
        );
        // The whole payload, not the absence of one member: a missing key reads as null,
        // so asserting `provider` or `files` is null would hold against a payload that
        // carried the provider's results under any other name. An entity answer has no
        // member a file could arrive in.
        let mut carried: Vec<_> = document["data"]
            .as_object()
            .unwrap_or_else(|| panic!("an entity answer carries a payload: {document}"))
            .keys()
            .map(String::as_str)
            .collect();
        carried.sort_unstable();
        assert_eq!(carried, members, "{document}");
    }

    let (outcome, material) = run(
        &["search", "supporting-material"],
        &root,
        &[MATERIAL_ONLY_TERM],
    );
    assert_eq!(
        material["contract"], "spielorders.search.supporting_material",
        "{material}"
    );
    if material_answered(&outcome, &material) {
        // Whatever the index held, every group is one Order's files — an entity that
        // merely matched by its facts is not one of them.
        for group in material["data"]["orders"]
            .as_array()
            .expect("a settled answer carries a group collection")
        {
            assert!(
                group["files"]
                    .as_array()
                    .is_some_and(|files| !files.is_empty()),
                "an Order no file was found in is not a group: {material}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Case sensitivity belongs to the entity engines, and Vendor visibility to one of them.
    // -----------------------------------------------------------------------

    assert_eq!(
        titles(&answered(&["search", "orders"], &root, &["LASER"])),
        ["Laser components"],
        "matching folds case by default"
    );
    assert_eq!(
        answered(&["search", "orders"], &root, &["LASER", "--case-sensitive"])["data"]["orders"],
        serde_json::json!([]),
        "and distinguishes it when asked to"
    );

    answered(&["vendor", "hide"], &root, &[HIDDEN_VENDOR]);
    let offered = answered(&["search", "vendors"], &root, &[HIDDEN_VENDOR]);
    assert_eq!(
        offered["data"]["vendors"],
        serde_json::json!([]),
        "a Hidden Vendor is not among the ones this Package offers: {offered}"
    );
    assert_eq!(offered["data"]["hidden_vendors_included"], false);
    let every = answered(&["search", "vendors"], &root, &[HIDDEN_VENDOR, "--all"]);
    assert_eq!(names(&every), [HIDDEN_VENDOR], "{every}");
    assert_eq!(every["data"]["hidden_vendors_included"], true);

    // -----------------------------------------------------------------------
    // A Damaged Order is in neither engine.
    // -----------------------------------------------------------------------

    // Found while it is healthy, so the term is known to be a searchable fact of it.
    assert_eq!(
        titles(&answered(&["search", "orders"], &root, &[DAMAGED_TERM])),
        [format!("{DAMAGED_TERM} lamp")],
        "the Order this term names, while its record can still be read"
    );

    std::fs::write(damaged.join(".spielorders/order.json"), b"{ not json")
        .expect("the Order record is damaged");

    let searched = answered(&["search", "orders"], &root, &[DAMAGED_TERM]);
    assert_eq!(
        searched["data"]["orders"],
        serde_json::json!([]),
        "a Damaged Order has no facts to match on: {searched}"
    );
    assert!(
        load_observed_inside(&searched, &damaged),
        "the load still walked that Directory and reported what it found there, which is \
         what makes the empty answer an Order excluded rather than an Order that was never \
         there: {searched}"
    );
    assert!(
        !load_observed_inside(&searched, &referencing),
        "and it is the damaged Directory the load reported about, not the healthy one, which \
         is what keeps the claim above from holding against any observation at all: \
         {searched}"
    );

    // The second engine reaches the same Package state the first just reported damage in,
    // and the load's observation travels beside its answer too. Whether the file under the
    // damaged Order is out of scope is not asked here — see this file's own note on what it
    // does not claim.
    let (outcome, material) = run(&["search", "supporting-material"], &root, &[DAMAGED_TERM]);
    material_answered(&outcome, &material);
    assert!(
        load_observed_inside(&material, &damaged),
        "a Damaged Order is the load's observation whether or not a provider answered: \
         {material}"
    );

    // -----------------------------------------------------------------------
    // A search writes nothing.
    // -----------------------------------------------------------------------

    // Taken here rather than at the top, because the composition deliberately hid a Vendor
    // and damaged a record on the way through. From this point on nothing but searches run,
    // so the Package has to be byte-identical afterwards — and the records' modification
    // times are the half of "wrote nothing" a byte comparison cannot make, since a file
    // rewritten with the bytes it already held is byte-identical and was still written.
    let settled = managed_snapshot(&root);
    let stamps: Vec<_> = [&referencing, &damaged]
        .iter()
        .map(|directory| modification_time(&directory.join(".spielorders/order.json")))
        .collect();

    for line in [
        vec!["search", "orders"],
        vec!["search", "vendors"],
        vec!["search", "supporting-material"],
    ] {
        run(&line, &root, &[VENDOR_ONLY_TERM]);
    }

    assert_eq!(
        managed_snapshot(&root),
        settled,
        "a search writes nothing, whichever engine answered"
    );
    assert_eq!(
        [&referencing, &damaged]
            .iter()
            .map(|directory| modification_time(&directory.join(".spielorders/order.json")))
            .collect::<Vec<_>>(),
        stamps,
        "and rewrites no record with the bytes it already held"
    );
}
