//! One Package's whole working lifetime, composed from commands no single ticket owns.
//!
//! **Every command ticket proved its own slice; this proves they compose.** A Package is
//! initialized here, populated, drifted, aligned, adopted into, read, validated, damaged,
//! diagnosed, repaired, upgraded, reindexed and emptied — through the `spielorders`
//! executable alone, as real processes, over a layout no other test in this suite uses.
//!
//! **The layout is the point.** [`ORDERS_PATH`] puts the Orders Directory four levels below
//! the Root instead of the default one, and [`REMOVED_PATH`] renames the Removed Orders
//! Directory. Under the default layout an Order Directory is two levels down, so a rule
//! that counted a fixed depth would be right by accident everywhere else in this suite.
//! Here nothing but each Order's own technical locator knows the way back.
//!
//! **Nothing here parses human output.** Every step reads the `--json` document, which is
//! the contract a client has; the human surface is asserted as a surface by
//! `tests/output_modes_and_streams.rs` and is never a parsing contract.
//!
//! Decision sources: Define the Rust CLI capability contract; Define legacy Order adoption
//! and metadata bootstrapping; Effect and adapter boundaries.

use crate::support::{
    Invocation, Outcome, SUPPORTING_A, SUPPORTING_B, TemporaryTree, entry_names, managed_snapshot,
    response,
};
use std::path::{Path, PathBuf};

/// The Orders Directory this composition configures: nested, and not the default.
const ORDERS_PATH: &str = "archive/2026/Orders";

/// The Removed Orders Directory this composition configures: renamed, and not the default.
const REMOVED_PATH: &str = "Withdrawn";

/// The Package README a user wrote before SpielOrders ever saw the directory.
///
/// Compared byte for byte after initialization, because `init` preserves an existing one
/// and creates an empty conventional README only where there is none.
const USER_README: &[u8] = b"Telescope purchasing. Kept by hand since 2005.\n";

/// The legacy directory this composition adopts, at the name a real archive has.
///
/// `2009_04_31` is not a date — April has thirty days — which is exactly why adoption reads
/// no part of a directory name as an Order fact and why the name is left alone afterwards.
const LEGACY_BASENAME: &str = "2009_04_31_Lcom_Fiber";

// ---------------------------------------------------------------------------
// Running the executable.
// ---------------------------------------------------------------------------

/// One `--json` run of the real executable, as a caller writes it.
///
/// `response` refuses a second document, trailing text, or anything at all on standard
/// error, so every step of the composition makes the one-document claim as well as its own.
fn run(line: &[&str], root: &Path, arguments: &[&str]) -> (Outcome, serde_json::Value) {
    let mut invocation = Invocation::new(line);
    for argument in arguments {
        invocation = invocation.argument(argument);
    }
    let outcome = invocation.rooted_at(root).argument("--json").run();
    let document = response(&outcome);
    (outcome, document)
}

/// The document a command answered with, at exit `0`.
fn answered(line: &[&str], root: &Path, arguments: &[&str]) -> serde_json::Value {
    let (outcome, document) = run(line, root, arguments);
    assert_eq!(
        outcome.status,
        Some(0),
        "{line:?} {arguments:?}: {}",
        outcome.stdout_text()
    );
    assert_eq!(
        document["ok"], true,
        "{line:?} {arguments:?}: `ok` and the exit status are one fact"
    );
    document
}

/// The document a command refused with, at exit `1`, and the stable code it carries.
fn refused(line: &[&str], root: &Path, arguments: &[&str]) -> serde_json::Value {
    let (outcome, document) = run(line, root, arguments);
    assert_eq!(
        outcome.status,
        Some(1),
        "{line:?} {arguments:?}: this run is refused: {}",
        outcome.stdout_text()
    );
    assert_eq!(
        document["ok"], false,
        "{line:?} {arguments:?}: `ok` and the exit status are one fact"
    );
    assert!(
        document["data"].is_null(),
        "{line:?} {arguments:?}: a failure carries no data"
    );
    document
}

/// A committed mutation that changed something, with its report.
fn committed<'a>(document: &'a serde_json::Value, label: &str) -> &'a serde_json::Value {
    let report = &document["data"]["report"];
    assert_eq!(report["committed"], true, "{label}");
    assert_eq!(report["changed"], true, "{label}");
    report
}

// ---------------------------------------------------------------------------
// The composition.
// ---------------------------------------------------------------------------

/// A directory holding one hand-kept README, which is what a Package starts as.
fn unmanaged_tree(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let root = std::fs::canonicalize(tree.path()).expect("the temporary directory resolves");
    std::fs::write(root.join("README.md"), USER_README).expect("the user's README is written");
    (tree, root)
}

/// A Package initialized over the non-default layout, through the executable.
///
/// The failure-surface tests below each start here, so every one of them is asserted over
/// the nested Orders Directory and the renamed Removed Orders Directory rather than over a
/// default fixture.
fn initialized(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root) = unmanaged_tree(label);
    let document = answered(
        &["init"],
        &root,
        &[
            &format!("orders_path={ORDERS_PATH}"),
            &format!("removed_path={REMOVED_PATH}"),
        ],
    );
    assert_eq!(document["data"]["package"]["orders_path"], ORDERS_PATH);
    assert_eq!(document["data"]["package"]["removed_path"], REMOVED_PATH);
    assert_eq!(
        document["data"]["package"]["metadata_version"], 1,
        "nothing has run outside a test tree, so version 1 is what a Package is created at"
    );
    (tree, root)
}

/// A Vendor and one Order created in an initialized Package, and that Order's Directory.
fn populated(root: &Path) -> (String, PathBuf) {
    let vendor = answered(&["add-vendor"], root, &["Tarnwick"]);
    let vendor_id = vendor["data"]["vendor"]["vendor_id"]
        .as_str()
        .expect("a created Vendor carries the identity the core minted")
        .to_owned();

    let order = answered(
        &["add-order"],
        root,
        &[
            "title=Laser components",
            "date_ordered=2026-08-22",
            "vendor=Tarnwick",
            "orderer=Ada Lovelace",
            "ordering_institution=Northwind",
            "project=Telescope",
            "order_group=Summer build",
            "currency=USD",
            "status=ordered",
        ],
    );
    let directory = absolute(&order["data"]["order"]["paths"]["directory"]);
    (vendor_id, directory)
}

/// The absolute path one reported location names.
fn absolute(location: &serde_json::Value) -> PathBuf {
    PathBuf::from(
        location["absolute_path"]
            .as_str()
            .expect("a reported location names an absolute path"),
    )
}

/// Every byte of one Order Directory except its managed metadata.
///
/// **What a whole-directory relocation has to carry**: the opaque tree and the conventional
/// README. The managed record is deliberately outside it, because a mutation is supposed to
/// change that — comparing it would make the claim about the wrong thing, and a relocation
/// that quietly rewrote a sentinel would still be caught here.
fn opaque_snapshot(directory: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    managed_snapshot(directory)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".spielorders"))
        .collect()
}

/// Both opaque sentinels placed in one Order Directory, one of them nested.
///
/// A whole-directory relocation that enumerated children could plausibly carry the first
/// and lose the second, so one stands at the top and one inside a subdirectory. Neither is
/// ever read, hashed, or rewritten by anything the composition runs.
fn supporting_material(directory: &Path) {
    let nested = directory.join("Quotes");
    std::fs::create_dir(&nested).expect("the Supporting Material subdirectory is created");
    std::fs::write(directory.join("supporting-a.bin"), SUPPORTING_A)
        .expect("the first sentinel is written");
    std::fs::write(nested.join("supporting-b.bin"), SUPPORTING_B)
        .expect("the second sentinel is written");
}

/// The legacy candidate a real archive offers: its own README, its own opaque tree, and a
/// name that encodes nothing trustworthy.
fn legacy_candidate(root: &Path) -> PathBuf {
    let candidate = root.join(ORDERS_PATH).join(LEGACY_BASENAME);
    std::fs::create_dir(&candidate).expect("the candidate directory is created");
    std::fs::write(
        candidate.join("README.md"),
        b"Ordered by telephone. Invoice in the quotes folder.\n",
    )
    .expect("the candidate's own README is written");
    supporting_material(&candidate);
    candidate
}

/// One Package initialized, populated, adopted into, read, validated, damaged, diagnosed,
/// repaired, upgraded, reindexed and emptied — through the executable alone.
///
/// **One test rather than twenty, because the subject is the composition.** Each step here
/// has its own suite proving it in isolation; what no other test can observe is a Package
/// carried through all of them in order, over a layout no ticket's fixtures use, with the
/// opaque sentinels compared byte for byte at the end.
#[test]
fn one_package_is_initialized_populated_repaired_and_emptied_through_the_executable_alone() {
    let (_tree, root) = unmanaged_tree("m4-composition");

    // A dry run of the initialization leaves the directory exactly as it found it.
    let before = managed_snapshot(&root);
    let planned = answered(
        &["init"],
        &root,
        &[
            &format!("orders_path={ORDERS_PATH}"),
            &format!("removed_path={REMOVED_PATH}"),
            "--dry-run",
        ],
    );
    assert_eq!(planned["data"]["report"]["committed"], false);
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run creates no directory, managed file, or staging artifact"
    );

    let (_tree, root) = initialized("m4-composition");
    assert_eq!(
        std::fs::read(root.join("README.md")).expect("the README is readable"),
        USER_README,
        "an existing Package README is preserved byte for byte"
    );

    // ---- Vendors and Contacts -------------------------------------------------------
    let (vendor_id, directory) = populated(&root);

    let edited = answered(
        &["vendor", "edit"],
        &root,
        &[
            "Tarnwick",
            "phone=+1 555 0123",
            "email=sales@example.com",
        ],
    );
    committed(&edited, "a Vendor's shared facts are edited");
    assert_eq!(edited["data"]["vendor"]["vendor_id"], vendor_id);

    for contact in [
        ("name=Katherine Johnson", "role=Accounts"),
        ("name=Grace Hopper", "role=Engineering"),
    ] {
        let added = answered(
            &["vendor", "contact", "add"],
            &root,
            &["Tarnwick", contact.0, contact.1],
        );
        committed(&added, "a Contact is appended");
    }
    let moved = answered(
        &["vendor", "contact", "move"],
        &root,
        &["Tarnwick", "1", "position=1"],
    );
    assert_eq!(
        moved["data"]["report"]["changed"], false,
        "moving a Contact to the position it already occupies is a no-op, not an error"
    );
    assert_eq!(moved["data"]["report"]["committed"], true);
    committed(
        &answered(
            &["vendor", "contact", "edit"],
            &root,
            &["Tarnwick", "2", "email=grace@example.com"],
        ),
        "a Contact's facts are edited by position",
    );
    committed(
        &answered(&["vendor", "contact", "remove"], &root, &["Tarnwick", "2"]),
        "a Contact is removed by position",
    );
    let listed = answered(&["vendor", "contact", "list"], &root, &["Tarnwick"]);
    assert_eq!(
        listed["contract"], "spielorders.vendor.show",
        "a sub-list renders the show it is part of, under that one contract"
    );
    assert_eq!(listed["data"]["contacts"].as_array().map(Vec::len), Some(1));

    // A Vendor is never deleted. Hiding governs what this Package offers for new use and
    // never what an Order already referencing it resolves, and either command undoes the
    // other.
    committed(
        &answered(&["vendor", "hide"], &root, &["Tarnwick"]),
        "the Vendor stops being offered for new use",
    );
    assert_eq!(
        answered(&["package", "list", "vendors"], &root, &[])["data"]["vendors"],
        serde_json::json!([]),
        "a Hidden Vendor is not among the Vendors offered for new use"
    );
    assert_eq!(
        answered(&["package", "list", "vendors"], &root, &["--all"])["data"]["vendors"][0]["hidden"],
        true,
        "and with --all it is reported carrying its visibility"
    );
    assert_eq!(
        answered(&["order", "show"], &directory, &[])["data"]["vendor_id"],
        vendor_id,
        "the Order that references it still resolves its facts"
    );
    committed(
        &answered(&["vendor", "unhide"], &root, &["Tarnwick"]),
        "and it is offered for new use again",
    );

    // ---- Order facts, Items, and alignment ------------------------------------------
    supporting_material(&directory);
    let opaque = opaque_snapshot(&directory);

    for item in [
        &[
            "description=Fiber collimator",
            "quantity=4",
            "unit_price=3.0",
            "catalog_number=F220APC",
        ][..],
        &["description=Mirror mount", "quantity=2", "unit_price=84.5"][..],
    ] {
        committed(
            &answered(&["order", "item", "add"], &directory, item),
            "an Item is appended",
        );
    }
    committed(
        &answered(&["order", "item", "move"], &directory, &["1", "position=2"]),
        "an Item is moved within the collection",
    );
    committed(
        &answered(&["order", "item", "edit"], &directory, &["1", "quantity=5"]),
        "an Item's facts are edited by position",
    );
    committed(
        &answered(&["order", "item", "remove"], &directory, &["2"]),
        "an Item is removed by position",
    );
    let items = answered(&["order", "item", "list"], &directory, &[]);
    assert_eq!(
        items["contract"], "spielorders.order.show",
        "a sub-list renders the show it is part of, under that one contract"
    );
    assert_eq!(items["data"]["items"].as_array().map(Vec::len), Some(1));

    let retitled = answered(&["order", "edit"], &directory, &["title=Laser heads"]);
    committed(&retitled, "an Order's facts are edited");
    assert!(
        directory.is_dir(),
        "a fact edit never renames the Order Directory; alignment is asked for explicitly"
    );

    let planned = answered(&["order", "align"], &directory, &["--dry-run"]);
    let report = &planned["data"]["report"];
    assert_eq!(report["committed"], false);
    assert_eq!(report["changed"], true);
    let projected = absolute(&report["new_path"]);
    assert_ne!(
        projected, directory,
        "the Order has drifted from the name its facts project"
    );

    let aligned = answered(&["order", "align"], &directory, &[]);
    let report = committed(&aligned, "the drifted Order Directory is renamed");
    let directory = absolute(&report["new_path"]);
    assert_eq!(directory, projected, "the dry run named the destination");
    assert_eq!(
        report["old_path"]["package_relative_path"]
            .as_str()
            .map(|path| path.starts_with(ORDERS_PATH)),
        Some(true),
        "the destination it changed is reported on both sides, in the Package's spelling"
    );
    assert_eq!(
        opaque_snapshot(&directory),
        opaque,
        "a whole-directory relocation carries the opaque tree along, byte for byte"
    );

    // ---- Adoption ------------------------------------------------------------------
    let candidate = legacy_candidate(&root);
    let untouched = managed_snapshot(&candidate);
    let adopted = answered(
        &["adopt-order"],
        &root,
        &[
            candidate.to_str().expect("the test path is UTF-8"),
            "title=Fiber patch cable",
            "date_ordered=2009-04-30",
            "status=complete",
            "item",
            "description=Fiber patch cable, 3 m",
            "quantity=3",
            "unit_price=12.5",
        ],
    );
    let order = &adopted["data"]["order"];
    assert_eq!(absolute(&order["paths"]["directory"]), candidate);
    assert_eq!(
        candidate.file_name().and_then(|name| name.to_str()),
        Some(LEGACY_BASENAME),
        "a candidate is adopted where it stands and is never renamed automatically"
    );
    assert_eq!(order["date_ordered"], "2009-04-30");
    assert_eq!(order["items"].as_array().map(Vec::len), Some(1));
    let mut expected = untouched.clone();
    expected.push((PathBuf::from(".spielorders"), None));
    expected.push((
        PathBuf::from(".spielorders/order.json"),
        Some(
            std::fs::read(candidate.join(".spielorders/order.json"))
                .expect("the adopted record is readable"),
        ),
    ));
    expected.sort();
    assert_eq!(
        managed_snapshot(&candidate),
        expected,
        "adoption writes one record and leaves every other file exactly as it was"
    );

    // ---- Every read, over the nested layout ------------------------------------------
    let shown = answered(&["package", "show"], &root, &[]);
    assert_eq!(shown["data"]["orders"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        absolute(&shown["data"]["paths"]["orders_directory"]),
        root.join(ORDERS_PATH)
    );
    assert_eq!(
        absolute(&shown["data"]["paths"]["removed_orders_directory"]),
        root.join(REMOVED_PATH)
    );
    assert_eq!(shown["data"]["metadata"]["metadata_version"], 1);

    let orders = answered(&["package", "list", "orders"], &root, &[]);
    assert_eq!(orders["data"]["orders"].as_array().map(Vec::len), Some(2));
    assert_eq!(orders["data"]["damaged_orders"], serde_json::json!([]));
    let vendors = answered(&["package", "list", "vendors"], &root, &[]);
    assert_eq!(vendors["data"]["vendors"].as_array().map(Vec::len), Some(1));
    for kind in ["ordering_institution", "orderer", "project", "order_group"] {
        let values = answered(&["package", "values", kind], &root, &[]);
        assert!(
            values["data"]["values"].is_array(),
            "{kind}: an empty collection is present and empty"
        );
    }
    assert_eq!(
        answered(&["package", "readme"], &root, &[])["data"]["exists"],
        true
    );
    assert_eq!(
        answered(&["order", "readme"], &candidate, &[])["data"]["exists"],
        true
    );
    assert_eq!(
        answered(&["order", "show"], &candidate, &[])["data"]["title"],
        "Fiber patch cable"
    );
    let vendor = answered(&["vendor", "show"], &root, &["Tarnwick"]);
    assert_eq!(vendor["data"]["vendor_id"], vendor_id);
    assert_eq!(
        vendor["data"]["referencing_orders"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    // ---- Validation and the derived index --------------------------------------------
    answered(&["validate"], &root, &[]);
    assert_eq!(
        answered(&["index", "status"], &root, &[])["data"]["state"],
        "current"
    );
    assert_eq!(
        answered(&["package", "show"], &root, &[])["data"]["index"]["state"],
        "current",
        "the bootstrap read answers the index's state itself, agreeing with the command \
         dedicated to it"
    );
    let index = root.join(".spielorders/index.json");
    let projected_index = std::fs::read(&index).expect("the derived index is readable");
    let rebuilt = answered(&["index", "rebuild"], &root, &[]);
    assert_eq!(rebuilt["data"]["report"]["committed"], true);
    assert_eq!(
        std::fs::read(&index).expect("the rebuilt index is readable"),
        projected_index,
        "the same healthy Package rebuilds to identical bytes"
    );

    // ---- Damage, diagnosis, and one applied repair ------------------------------------
    damage_the_locator(&candidate);
    let invalid = refused(&["order", "validate"], &candidate, &[]);
    assert_eq!(invalid["error"]["code"], "order_invalid");

    let diagnosed = answered(&["order", "repair"], &candidate, &[]);
    assert_eq!(
        diagnosed["data"]["valid"], false,
        "a bare repair answers with the verdict in its payload and exits `0`"
    );
    let case = &diagnosed["data"]["cases"][0];
    assert_eq!(case["code"], "order_package_locator_mismatch");
    let handle = case["case"]
        .as_str()
        .expect("a diagnosed case carries its ephemeral handle")
        .to_owned();
    let action = case["actions"][0]["action"]
        .as_str()
        .expect("an actionable case offers an action")
        .to_owned();
    assert_eq!(action, "repair_package_locator");
    assert!(
        case["actions"][0]["command"]
            .as_str()
            .is_some_and(|command| command.contains(&handle)),
        "a diagnosis states the exact command that applies it"
    );

    let arguments = [format!("case={handle}"), format!("action={action}")];
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let before = managed_snapshot(&root);
    let planned = answered(
        &["order", "repair"],
        &candidate,
        &[arguments[0], arguments[1], "--dry-run"],
    );
    assert_eq!(planned["data"]["report"]["committed"], false);
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run of a repair creates no directory, staging file, or index rewrite"
    );

    let repaired = answered(&["order", "repair"], &candidate, &arguments);
    committed(&repaired, "the technical locator is recomputed");
    answered(&["order", "validate"], &candidate, &[]);
    answered(&["validate"], &root, &[]);

    // ---- Upgrade, in both moods --------------------------------------------------------
    for mood in [&["--dry-run"][..], &[][..]] {
        let document = answered(&["upgrade"], &root, mood);
        let data = &document["data"];
        assert_eq!(data["source_version"], 1, "{mood:?}");
        assert_eq!(data["target_version"], 1, "{mood:?}");
        assert_eq!(data["complete"], true, "{mood:?}");
        assert_eq!(data["blockers"], serde_json::json!([]), "{mood:?}");
    }

    // ---- Disposition, both ways ---------------------------------------------------------
    let recovered = answered(&["order", "remove"], &directory, &[]);
    let report = committed(&recovered, "the whole Order Directory leaves the Package");
    assert_eq!(report["disposition"], "recoverable_directory");
    let removed = absolute(&report["new_path"]);
    assert_eq!(
        removed.parent(),
        Some(root.join(REMOVED_PATH).as_path()),
        "the renamed Removed Orders Directory is created at that moment and is the destination"
    );
    assert_eq!(
        opaque_snapshot(&removed),
        opaque,
        "the complete tree travelled, opaque sentinels included, byte for byte"
    );

    trash_the_adopted_order(&candidate);

    let emptied = answered(&["package", "show"], &root, &[]);
    assert_eq!(emptied["data"]["orders"], serde_json::json!([]));
    answered(&["validate"], &root, &[]);
}

/// One Order's technical locator rewritten to name the wrong ancestor.
///
/// **No core capability can produce this**, because `package_root` is a managed technical
/// field every mutation refuses and the core computes the correct value wherever it creates
/// or moves an Order Directory. It is written into the record the core produced, which is
/// the shape a Package has after an Order Directory is moved by hand or its Orders
/// Directory is reconfigured to another depth.
fn damage_the_locator(directory: &Path) {
    let record = directory.join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the adopted record is readable");
    // Four levels for this layout, so three names the wrong ancestor while staying a
    // well-formed relative path — which is the stale locator rather than a malformed one.
    let rewritten = text.replace(
        "\"package_root\": \"../../../..\"",
        "\"package_root\": \"../../..\"",
    );
    assert_ne!(
        text, rewritten,
        "the locator this layout writes is four levels of parent"
    );
    std::fs::write(&record, rewritten).expect("the rewritten record is written");
}

/// The adopted Order submitted whole to the platform Trash, and taken back out again.
///
/// **Asserted against the real effect**, which is the only way to tell a submission from a
/// delete: both leave the original path empty. Every observation is taken before the
/// cleanup, so a failing assertion cannot skip it — this machine's Trash belongs to its
/// owner. A platform with no supported Trash reports that specifically instead, and the
/// Order survives it.
fn trash_the_adopted_order(candidate: &Path) {
    let submitted = managed_snapshot(candidate);
    let (outcome, document) = run(&["order", "remove"], candidate, &["--delete"]);

    if outcome.status == Some(1) {
        assert_eq!(
            document["error"]["code"],
            "order_trash_unsupported",
            "a platform with no supported Trash says so specifically: {}",
            outcome.stdout_text()
        );
        return assert_eq!(
            managed_snapshot(candidate),
            submitted,
            "and nothing was erased to discover it"
        );
    }

    let landed = document["data"]["report"]["new_path"]["absolute_path"]
        .as_str()
        .map(PathBuf::from);
    let arrived = landed.as_ref().map(|path| managed_snapshot(path));
    let cleaned = landed
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| std::fs::remove_dir_all(path).is_ok());

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["report"]["disposition"], "platform_trash");
    assert!(!candidate.exists(), "the directory left the Package");
    assert_eq!(
        arrived,
        Some(submitted),
        "the complete tree was submitted and nothing was erased"
    );
    assert_eq!(
        cleaned,
        Some(true),
        "and this suite took its own directory back out of the owner's Trash"
    );
}

// ---------------------------------------------------------------------------
// The failure surface, across that same composition.
// ---------------------------------------------------------------------------

/// A refused mutation names the fact that refused it and changes nothing.
#[test]
fn a_rejected_mutation_names_the_fact_and_leaves_the_package_byte_identical() {
    let (_tree, root) = initialized("m4-rejection");
    populated(&root);
    let before = managed_snapshot(&root);

    // The trimmed, case-insensitive comparison that makes a Vendor Name package-unique.
    // There is no Vendor Merge, so this is refused rather than reconciled.
    let refusal = refused(&["add-vendor"], &root, &["  tarnwick  "]);
    assert_eq!(refusal["error"]["code"], "vendor_name_in_use");
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a refused mutation leaves every managed byte where it was"
    );
}

/// A mutation with nothing to do reports that it changed nothing, and rewrites no bytes.
#[test]
fn a_no_op_mutation_reports_unchanged_without_replacing_canonical_bytes() {
    let (_tree, root) = initialized("m4-no-op");
    let (_vendor_id, directory) = populated(&root);
    let before = managed_snapshot(&root);

    let aligned = answered(&["order", "align"], &directory, &[]);
    let report = &aligned["data"]["report"];
    assert_eq!(
        report["changed"], false,
        "an Order already at the name its facts project has nothing to rename"
    );
    assert_eq!(report["committed"], true, "and it is not an error");
    assert_eq!(
        report["old_path"]["absolute_path"], report["new_path"]["absolute_path"],
        "the destination it kept is reported on both sides"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a no-op replaces no canonical bytes"
    );
}

/// A destination that is already taken stops the move rather than earning a suffix.
///
/// Both operations that relocate a whole Order Directory are checked, because the refusal
/// is one rule and an invented suffix would be one Order quietly filed under a name nobody
/// chose.
#[test]
fn an_occupied_destination_is_refused_rather_than_given_an_invented_suffix() {
    let (_tree, root) = initialized("m4-collision-align");
    let (_vendor_id, directory) = populated(&root);
    committed(
        &answered(&["order", "edit"], &directory, &["title=Laser heads"]),
        "the Order drifts from the name its facts project",
    );
    let projected = absolute(
        &answered(&["order", "align"], &directory, &["--dry-run"])["data"]["report"]["new_path"],
    );
    std::fs::create_dir(&projected).expect("the aligned name is occupied");
    let before = managed_snapshot(&root);
    let refusal = refused(&["order", "align"], &directory, &[]);
    assert_eq!(refusal["error"]["code"], "order_directory_occupied");
    assert_eq!(managed_snapshot(&root), before, "nothing was moved");

    let (_tree, root) = initialized("m4-collision-remove");
    let (_vendor_id, directory) = populated(&root);
    let occupied = root.join(REMOVED_PATH).join(
        directory
            .file_name()
            .expect("an Order Directory has a basename"),
    );
    std::fs::create_dir_all(&occupied).expect("the disposition's destination is occupied");
    let before = managed_snapshot(&root);
    let refusal = refused(&["order", "remove"], &directory, &[]);
    assert_eq!(refusal["error"]["code"], "order_directory_occupied");
    assert_eq!(managed_snapshot(&root), before, "nothing was moved");
}

/// A canonical write that cannot stage leaves the record holding exactly what it held.
///
/// **The precommit failure a real filesystem can produce.** A managed replacement stages
/// beside its destination and refuses an occupied name, which is what an interrupted
/// earlier write leaves behind — so a directory standing at the staging name makes the
/// replacement fail before anything is committed, in both the Package-level and the
/// Order-level record.
#[test]
fn a_precommit_failure_leaves_the_canonical_record_holding_what_it_held() {
    let (_tree, root) = initialized("m4-precommit-package");
    populated(&root);
    std::fs::create_dir(root.join(".spielorders/.vendors.json.spielorders-stage"))
        .expect("the Vendor record's staging name is occupied");
    let before = managed_snapshot(&root);
    let refusal = refused(
        &["vendor", "edit"],
        &root,
        &["Tarnwick", "phone=+1 555 0100"],
    );
    assert_eq!(refusal["error"]["code"], "vendor_mutation_failed");
    assert_eq!(
        refusal["error"]["paths"][0]["package_relative_path"], ".spielorders/vendors.json",
        "a refusal names the record in this Package's own spelling for it, as a success does"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Vendor record holds what it held, and no stage of this attempt is left behind"
    );

    let (_tree, root) = initialized("m4-precommit-order");
    let (_vendor_id, directory) = populated(&root);
    std::fs::create_dir(directory.join(".spielorders/.order.json.spielorders-stage"))
        .expect("the Order record's staging name is occupied");
    let before = managed_snapshot(&root);
    let refusal = refused(&["order", "edit"], &directory, &["title=Laser heads"]);
    assert_eq!(refusal["error"]["code"], "order_mutation_failed");
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Order record holds what it held"
    );
}

/// A committed canonical mutation whose derived-index refresh then failed stays committed.
///
/// **Nothing about the report may look like something to retry.** The canonical half
/// happened; re-running the command would ask for a change that has already been made. The
/// warning says the derived index is behind, and `index rebuild` is what answers it.
#[test]
fn a_committed_mutation_whose_index_refresh_failed_is_not_retry_shaped() {
    let (_tree, root) = initialized("m4-stale-index");
    let (_vendor_id, directory) = populated(&root);
    let staging = root.join(".spielorders/.index.json.spielorders-stage");
    std::fs::create_dir(&staging).expect("the derived index's staging name is occupied");

    let edited = answered(&["order", "edit"], &directory, &["title=Laser heads"]);
    let report = committed(&edited, "the canonical write succeeded");
    assert_eq!(
        report["warnings"],
        serde_json::json!(["index_refresh_failed"]),
        "the refresh that failed is a warning beside the commit, not a refusal"
    );
    assert_eq!(report["index"]["state"], "stale");
    assert_eq!(
        answered(&["order", "show"], &directory, &[])["data"]["title"],
        "Laser heads",
        "the canonical record carries the change the report said it committed"
    );

    // Two commands, one fact, in the state that matters: the bootstrap read a client opens
    // a Package with says the index is behind without being asked a second question, and it
    // says it while the refresh is still failing rather than by refreshing it.
    let before = managed_snapshot(&root);
    assert_eq!(
        answered(&["package", "show"], &root, &[])["data"]["index"]["state"],
        "stale"
    );
    assert_eq!(
        answered(&["index", "status"], &root, &[])["data"]["state"],
        "stale"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "asking what state the index is in is not what changes it"
    );

    std::fs::remove_dir(&staging).expect("the staging name is freed");
    committed(
        &answered(&["index", "rebuild"], &root, &[]),
        "the derived index is what recovers, and it is a rebuild rather than a repair",
    );
    assert_eq!(
        answered(&["index", "status"], &root, &[])["data"]["state"],
        "current"
    );
}

/// An upgrade of a Package this build cannot interpret is blocked, in both moods.
///
/// The one blocked upgrade this build can reach: its transformation registry is empty, so
/// there is no sequence to interrupt or resume, and a marker declaring a version this build
/// does not understand is what stops it.
#[test]
fn an_upgrade_is_blocked_by_a_version_this_build_cannot_interpret() {
    let (_tree, root) = initialized("m4-blocked-upgrade");
    populated(&root);
    let marker = root.join(".spielorders/version.json");
    std::fs::write(&marker, b"{\n  \"metadata_version\": 999\n}\n")
        .expect("the marker declares a version this build does not interpret");
    let before = managed_snapshot(&root);

    for mood in [&["--dry-run"][..], &[][..]] {
        let refusal = refused(&["upgrade"], &root, mood);
        assert_eq!(
            refusal["error"]["code"], "package_metadata_unsupported",
            "{mood:?}"
        );
    }
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a blocked upgrade interprets nothing and writes nothing"
    );
}

/// The conventional READMEs keep their three distinctions, over the nested layout.
///
/// Absent, readable, and unreadable are three answers rather than two: the conventional
/// path is reported whether or not a document stands at it, and a document that is not a
/// readable one is a warning beside the answer rather than a failure.
#[test]
fn conventional_readmes_keep_their_absent_readable_and_unreadable_distinctions() {
    let (_tree, root) = initialized("m4-readmes");
    let (_vendor_id, directory) = populated(&root);

    let readme = directory.join("README.md");
    let readable = answered(&["order", "readme"], &directory, &[]);
    assert_eq!(readable["data"]["exists"], true);
    assert_eq!(readable["diagnostics"], serde_json::json!([]));

    std::fs::remove_file(&readme).expect("the plain README is removed");
    let absent = answered(&["order", "readme"], &directory, &[]);
    assert_eq!(absent["data"]["exists"], false);
    assert_eq!(
        absolute(&absent["data"]["location"]),
        readme,
        "the conventional path is the answer whether or not a document stands at it"
    );
    assert_eq!(absent["diagnostics"], serde_json::json!([]));

    // A shape rather than a permission: `chmod 000` does not stop a suite running as root.
    std::fs::create_dir(&readme).expect("a directory takes the README's place");
    let unreadable = answered(&["order", "readme"], &directory, &[]);
    assert_eq!(unreadable["data"]["exists"], true);
    assert!(
        !unreadable["diagnostics"]
            .as_array()
            .expect("diagnostics are present")
            .is_empty(),
        "a document that is not a readable one is a warning beside the answer"
    );
}

/// Nothing but an explicit whole-Order removal disposes of Supporting Material, and the two
/// operations that relocate a whole Order Directory carry it along untouched.
///
/// **The claim is over the composition rather than over one command**: every mutation this
/// milestone delivers runs against an Order holding both opaque sentinels, and the tree is
/// compared byte for byte afterwards.
#[test]
fn no_operation_but_an_explicit_removal_disposes_of_supporting_material() {
    let (_tree, root) = initialized("m4-supporting-material");
    let (_vendor_id, directory) = populated(&root);
    supporting_material(&directory);
    let opaque = opaque_snapshot(&directory.join("Quotes"));
    let sentinel = directory.join("supporting-a.bin");
    let top = std::fs::read(&sentinel).expect("the first sentinel is readable");

    for (line, arguments) in [
        (&["order", "edit"][..], &["orderer=Grace Hopper"][..]),
        (
            &["order", "item", "add"][..],
            &["description=Post holder", "quantity=6", "unit_price=12.25"][..],
        ),
        (&["order", "item", "edit"][..], &["1", "quantity=7"][..]),
        (&["order", "item", "remove"][..], &["1"][..]),
        (&["order", "validate"][..], &[][..]),
        (&["order", "repair"][..], &[][..]),
        (&["order", "show"][..], &[][..]),
    ] {
        answered(line, &directory, arguments);
    }
    for (line, arguments) in [
        (&["validate"][..], &[][..]),
        (&["repair"][..], &[][..]),
        (&["index", "rebuild"][..], &[][..]),
        (&["upgrade"][..], &[][..]),
        (&["package", "show"][..], &[][..]),
    ] {
        answered(line, &root, arguments);
    }
    assert_eq!(
        std::fs::read(&sentinel).expect("the first sentinel is readable"),
        top,
        "no metadata mutation, validation, repair, or index work touched the opaque tree"
    );
    assert_eq!(opaque_snapshot(&directory.join("Quotes")), opaque);

    // The first of the two relocations: a rename of the containing directory.
    committed(
        &answered(&["order", "edit"], &directory, &["title=Laser heads"]),
        "the Order drifts",
    );
    let whole = opaque_snapshot(&directory);
    let aligned = answered(&["order", "align"], &directory, &[]);
    let directory = absolute(&committed(&aligned, "the directory is renamed")["new_path"]);
    assert_eq!(
        opaque_snapshot(&directory),
        whole,
        "alignment relocates the container intact and enumerates nothing inside it"
    );

    // The second: whole-Order disposition, which is the only act that may dispose of it.
    let removed = answered(&["order", "remove"], &directory, &[]);
    let landed = absolute(&committed(&removed, "the whole Order Directory leaves")["new_path"]);
    assert_eq!(
        opaque_snapshot(&landed),
        whole,
        "the disposition carries the opaque tree along without enumerating it"
    );
    assert_eq!(
        entry_names(&root.join(REMOVED_PATH)),
        vec![
            landed
                .file_name()
                .and_then(|name| name.to_str())
                .expect("the removed directory has a basename")
                .to_owned()
        ],
        "and it landed under the renamed Removed Orders Directory"
    );
}
