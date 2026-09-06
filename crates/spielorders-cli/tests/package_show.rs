//! `spielorders package show` — the complete GUI-bootstrap read.
//!
//! One invocation returns everything a client needs to open a Package. The interesting
//! half is that a Package which is damaged, unmanaged, or awaiting an upgrade answers
//! this command too, because saying so is the answer — so the load states are tested as
//! a set, each with its own typed code and exit status, rather than as one generic
//! failure.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    HIDDEN_VENDOR_ID, Invocation, NEWEST_ORDER_ID, OFFERED_VENDOR_ID, OLDEST_ORDER_ID,
    damage_index, directory_that_is_not_a_root, initialized_package, managed_snapshot,
    modification_time, package_at_an_uninterpretable_version, package_root_that_cannot_be_read,
    package_with_a_damaged_order, package_with_duplicate_vendor_names,
    package_with_unreadable_readme, populated_package, remove_index, response, reversion_index,
    touch_an_order_record,
};
use std::path::Path;

/// An empty Package still answers, and every collection it has none of is present and
/// empty rather than omitted.
///
/// The defect this catches: a payload that omits `orders` when there are none, so a
/// consumer written against a populated Package reads `undefined` instead of `[]`. The
/// contract's own rule is that absent collections are present and empty, and one command
/// breaking it is how fifteen consumers written against the other fifteen break.
#[test]
fn an_empty_package_answers_with_every_collection_present_and_empty() {
    let (_tree, root) = initialized_package("show-empty");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0));
    assert_eq!(document["contract"], "spielorders.package.show");
    assert_eq!(document["contract_version"], 1);
    assert_eq!(document["ok"], true);
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
    for collection in ["orders", "damaged_orders", "vendors", "diagnostic_groups"] {
        assert_eq!(
            document["data"][collection].as_array().map(Vec::len),
            Some(0),
            "an empty Package reports an empty `{collection}`, not a missing key"
        );
    }
    for kind in [
        "ordering_institutions",
        "orderers",
        "projects",
        "order_groups",
    ] {
        assert_eq!(
            document["data"]["values"][kind].as_array().map(Vec::len),
            Some(0),
            "an empty Package reports an empty `{kind}` value list"
        );
    }
    assert_eq!(document["diagnostics"].as_array().map(Vec::len), Some(0));
    assert!(document["error"].is_null());
}

/// One invocation returns everything a client needs to open a Package.
///
/// The defect this catches: a payload that reports the Package's shape but not its
/// contents, so a GUI bootstrapping from it still has to walk the Orders Directory
/// itself — which is the whole thing this command exists to avoid.
#[test]
fn one_read_returns_the_paths_metadata_readme_orders_and_totals() {
    let (_tree, root) = populated_package("show-complete");
    let absolute = root.to_str().expect("the test root is UTF-8");

    let document = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let data = &document["data"];

    assert_eq!(
        data["paths"]["metadata_directory"]["package_relative_path"],
        ".spielorders"
    );
    assert_eq!(
        data["paths"]["config"]["package_relative_path"],
        ".spielorders/config.toml"
    );
    assert_eq!(
        data["paths"]["version"]["package_relative_path"],
        ".spielorders/version.json"
    );
    assert_eq!(
        data["paths"]["vendors"]["package_relative_path"],
        ".spielorders/vendors.json"
    );
    assert_eq!(
        data["paths"]["orders_directory"]["package_relative_path"],
        "Orders"
    );
    assert_eq!(
        data["paths"]["metadata_directory"]["absolute_path"],
        format!("{absolute}/.spielorders")
    );

    assert_eq!(data["metadata"]["metadata_version"], 1);
    assert_eq!(data["metadata"]["state"], "current");

    assert_eq!(data["readme"]["exists"], true);
    assert_eq!(data["readme"]["text"], "Telescope purchasing.\n");
    assert_eq!(
        data["readme"]["location"]["package_relative_path"],
        "README.md"
    );

    let orders = data["orders"].as_array().expect("orders is a list");
    assert_eq!(orders.len(), 2, "both healthy Orders are reported");

    let newest = &orders[0];
    assert_eq!(
        newest["order_id"], NEWEST_ORDER_ID,
        "machine responses retain UUIDs"
    );
    assert_eq!(newest["title"], "Laser components");
    assert_eq!(newest["date_ordered"], "2026-08-22");
    assert_eq!(newest["status"], "in_preparation");
    assert_eq!(newest["schema_version"], 1);
    assert_eq!(newest["currency"], "USD");
    assert_eq!(newest["orderer"], "Ada Lovelace");
    assert_eq!(newest["ordering_institution"], "Northwind");
    assert_eq!(newest["project"], "Telescope");
    assert_eq!(newest["order_group"], "Summer build");
    assert_eq!(newest["package_root"], "../..");
    assert_eq!(
        newest["paths"]["order"]["package_relative_path"],
        "Orders/2026_08_22_Tarnwick_Laser_components_Ada_Lovelace/.spielorders/order.json"
    );
    assert_eq!(
        newest["paths"]["directory"]["package_relative_path"],
        "Orders/2026_08_22_Tarnwick_Laser_components_Ada_Lovelace"
    );
    assert_eq!(
        newest["paths"]["readme"]["package_relative_path"],
        "Orders/2026_08_22_Tarnwick_Laser_components_Ada_Lovelace/README.md"
    );

    let items = newest["items"].as_array().expect("items is a list");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["description"], "Fiber collimator");
    assert_eq!(items[0]["quantity"], 4.0);
    assert_eq!(items[0]["unit_price"], 3.0);
    assert_eq!(items[0]["catalog_number"], "F220APC");
    assert_eq!(
        items[0]["item_total"], 12.0,
        "the Item Total is the core's, not one the adapter multiplied"
    );
    assert!(
        items[1]["catalog_number"].is_null(),
        "an absent optional fact is null"
    );
    assert_eq!(newest["order_total"], 181.0, "4 * 3.0 + 2 * 84.5");

    assert_eq!(orders[1]["title"], "Mirror mounts");
    assert_eq!(orders[1]["order_id"], OLDEST_ORDER_ID);
    assert_eq!(orders[1]["order_total"], 73.5);

    assert_eq!(
        data["index"]["location"]["package_relative_path"],
        ".spielorders/index.json"
    );
    assert!(data["index"]["state"].is_string());
    assert_eq!(
        data["index"]["malformed_reasons"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(
        data["index"]["stale_reasons"].as_array().map(Vec::len),
        Some(0)
    );
}

/// One state the derived Package Index can be in, and what puts a Package into it.
struct IndexState {
    label: &'static str,
    /// The drift or damage this state needs, and nothing at all for a current index.
    prepare: Option<fn(&Path)>,
    /// The state a response reports, which is not always the label: `wrong-version` reads
    /// on a command line and `wrong_version` in a contract.
    reported: &'static str,
}

/// The five states, and the current one is what a Package left alone is already in.
const INDEX_STATES: [IndexState; 5] = [
    IndexState {
        label: "current",
        prepare: None,
        reported: "current",
    },
    IndexState {
        label: "stale",
        prepare: Some(touch_an_order_record),
        reported: "stale",
    },
    IndexState {
        label: "missing",
        prepare: Some(remove_index),
        reported: "missing",
    },
    IndexState {
        label: "malformed",
        prepare: Some(damage_index),
        reported: "malformed",
    },
    IndexState {
        label: "wrong-version",
        prepare: Some(reversion_index),
        reported: "wrong_version",
    },
];

/// The bootstrap read answers what state the derived index is in, in every state
/// `index status` distinguishes.
///
/// The defect this catches: the complete GUI-bootstrap read answering `not_assessed` over
/// a Package whose index is malformed, so a client that opened the Package would have to
/// run a second command to learn what this command's contract says it already carries.
///
/// **Asserted as whole projections rather than by the state word.** Two commands that
/// agree on `malformed` and disagree about why are still two answers to one question, and
/// the reasons are what a client acts on.
#[test]
fn the_bootstrap_read_reports_every_index_state_index_status_reports() {
    for IndexState {
        label,
        prepare,
        reported,
    } in INDEX_STATES
    {
        let (_tree, root) = populated_package(&format!("show-index-{label}"));
        if let Some(prepare) = prepare {
            prepare(&root);
        }

        let shown = response(
            &Invocation::new(&["package", "show", "--json"])
                .rooted_at(&root)
                .run(),
        );
        let status = response(
            &Invocation::new(&["index", "status", "--json"])
                .rooted_at(&root)
                .run(),
        );

        assert_eq!(
            shown["data"]["index"]["state"], reported,
            "{label}: {shown}"
        );
        assert_eq!(
            shown["data"]["index"], status["data"],
            "{label}: one core assessment, so the two cannot come to disagree"
        );
    }
}

/// Assessing the derived index is not allowed to be the thing that changes it.
///
/// The defect this catches: answering the state by taking the load that assesses *and*
/// durably rebuilds, so the bootstrap read reports `current` because asking made it
/// current — a durable write from a command whose whole subject is reading, and a state
/// nobody can ever observe as wrong. Said from the Package's own bytes rather than from a
/// claim, and from the index's modification time too, because a file rewritten with the
/// bytes it already held is byte-identical and was still written.
#[test]
fn the_bootstrap_read_writes_nothing_in_any_index_state() {
    for IndexState { label, prepare, .. } in INDEX_STATES {
        let (_tree, root) = populated_package(&format!("show-index-read-only-{label}"));
        if let Some(prepare) = prepare {
            prepare(&root);
        }
        let index = root.join(".spielorders/index.json");
        let before = managed_snapshot(&root);
        let modified = modification_time(&index);

        let outcome = Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run();

        assert_eq!(
            outcome.status,
            Some(0),
            "{label}: {}",
            outcome.stderr_text()
        );
        assert_eq!(managed_snapshot(&root), before, "{label}: nothing written");
        assert_eq!(
            modification_time(&index),
            modified,
            "{label}: and the index was not even touched"
        );
    }
}

/// The four value lists carry canonical spellings and usage counts that match what the
/// Package's Orders actually contain.
///
/// The defect this catches: a count that is the list's length rather than the value's
/// usage, which is invisible until one value is used twice. `Telescope` is used by
/// both Orders here and `Summer build` by one, so the two cannot be confused.
#[test]
fn the_four_value_lists_carry_canonical_spellings_and_usage_counts() {
    let (_tree, root) = populated_package("show-values");

    let document = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let values = &document["data"]["values"];

    for (kind, value, usages) in [
        ("ordering_institutions", "Northwind", 1),
        ("orderers", "Ada Lovelace", 2),
        ("projects", "Telescope", 2),
        ("order_groups", "Summer build", 1),
    ] {
        let list = values[kind].as_array().expect("the value list is a list");
        assert_eq!(list.len(), 1, "{kind}");
        assert_eq!(list[0]["value"], value, "{kind}");
        assert_eq!(list[0]["usage_count"], usages, "{kind}");
    }
}

/// Hidden Vendors are absent by default and present with `--all`, and an Order
/// referencing a Hidden Vendor resolves that Vendor's facts either way.
///
/// The defect this catches is the one the visibility default is most likely to cause:
/// resolving an Order's Vendor against the *presented* list, so the second Order's
/// Vendor vanishes whenever `--all` is omitted. Hiding governs what is offered for new
/// use and never what is resolved.
#[test]
fn hidden_vendors_are_omitted_by_default_and_still_resolve_for_the_orders_that_name_them() {
    let (_tree, root) = populated_package("show-hidden-vendors");

    for (arguments, expected_names, included) in [
        (vec!["package", "show", "--json"], vec!["Tarnwick"], false),
        (
            vec!["package", "show", "--json", "--all"],
            vec!["Nethermoor", "Tarnwick"],
            true,
        ),
    ] {
        let document = response(&Invocation::new(&arguments).rooted_at(&root).run());
        let data = &document["data"];

        let mut names: Vec<&str> = data["vendors"]
            .as_array()
            .expect("vendors is a list")
            .iter()
            .map(|vendor| vendor["name"].as_str().expect("a Vendor Name is a string"))
            .collect();
        names.sort_unstable();
        assert_eq!(names, expected_names, "{arguments:?}");
        assert_eq!(data["hidden_vendors_included"], included, "{arguments:?}");

        let orders = data["orders"].as_array().expect("orders is a list");
        assert_eq!(
            orders[1]["vendor"]["vendor_id"], HIDDEN_VENDOR_ID,
            "{arguments:?}: the Order still names the Hidden Vendor"
        );
        assert_eq!(orders[1]["vendor"]["name"], "Nethermoor", "{arguments:?}");
        assert_eq!(orders[1]["vendor"]["hidden"], true, "{arguments:?}");
        assert_eq!(
            orders[0]["vendor"]["vendor_id"], OFFERED_VENDOR_ID,
            "{arguments:?}"
        );
        assert_eq!(orders[0]["vendor"]["hidden"], false, "{arguments:?}");
    }
}

/// Every Vendor fact and its Contacts, so a client can render the Vendors table without
/// a second read.
#[test]
fn a_vendor_carries_its_facts_its_contacts_and_its_visibility() {
    let (_tree, root) = populated_package("show-vendor-facts");

    let document = response(
        &Invocation::new(&["package", "show", "--json", "--all"])
            .rooted_at(&root)
            .run(),
    );
    let vendors = document["data"]["vendors"]
        .as_array()
        .expect("vendors is a list")
        .clone();
    let offered = vendors
        .iter()
        .find(|vendor| vendor["vendor_id"] == OFFERED_VENDOR_ID)
        .expect("the offered Vendor is reported");

    assert_eq!(offered["name"], "Tarnwick");
    assert_eq!(offered["phone"], "+1 555 0123");
    assert_eq!(offered["email"], "sales@example.com");
    assert_eq!(offered["website"], "https://example.com");
    assert_eq!(offered["address"], "100 Example Avenue\nAnytown, ST 00000");
    assert_eq!(offered["purchasing_notes"], "Quote required above $5000.");
    assert_eq!(offered["hidden"], false);

    let contacts = offered["contacts"].as_array().expect("contacts is a list");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["name"], "Katherine Johnson");
    assert_eq!(contacts[0]["role"], "Accounts");
    assert_eq!(contacts[0]["email"], "accounts@example.com");
    assert!(contacts[0]["phone"].is_null());

    let hidden = vendors
        .iter()
        .find(|vendor| vendor["vendor_id"] == HIDDEN_VENDOR_ID)
        .expect("the Hidden Vendor is reported under --all");
    assert_eq!(hidden["hidden"], true);
    assert_eq!(hidden["contacts"].as_array().map(Vec::len), Some(0));
}

/// A Damaged Order is a placeholder with its diagnostics, never a healthy Order, and its
/// damage does not stop the rest of the Package from being reported.
///
/// The defect this catches: an unreadable Order record that either takes the whole
/// Package down with it or, worse, appears among the healthy Orders with invented facts.
/// Order-local damage is Order-local, and the placeholder is how the Package says so.
#[test]
fn a_damaged_order_is_a_placeholder_and_the_rest_of_the_package_still_answers() {
    let (_tree, root, damaged) = package_with_a_damaged_order("show-damaged-order");
    let relative = damaged
        .strip_prefix(&root)
        .expect("the damaged Order is inside the Package")
        .to_str()
        .expect("the test path is UTF-8");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);
    let data = &document["data"];

    assert_eq!(
        outcome.status,
        Some(0),
        "one damaged Order does not damage the Package"
    );

    let healthy = data["orders"].as_array().expect("orders is a list");
    assert_eq!(
        healthy.len(),
        1,
        "the damaged Order is not among the healthy"
    );
    assert_eq!(healthy[0]["title"], "Laser components");

    let placeholders = data["damaged_orders"]
        .as_array()
        .expect("damaged_orders is a list");
    assert_eq!(placeholders.len(), 1);
    assert_eq!(
        placeholders[0]["paths"]["directory"]["package_relative_path"], relative,
        "the placeholder names the directory rather than facts it could not read"
    );
    assert!(
        placeholders[0]["order_id"].is_null(),
        "an unreadable record yields no identity, and the core does not guess one"
    );

    // The placeholder's diagnostics are the group the core filed at the same directory,
    // which is what "grouped at the narrowest scope" means.
    let groups = data["diagnostic_groups"]
        .as_array()
        .expect("diagnostic_groups is a list");
    let group = groups
        .iter()
        .find(|group| group["location"]["package_relative_path"] == relative)
        .expect("the damaged Order has a diagnostic group at its own directory");
    assert_eq!(group["scope"], "order");
    let diagnostics = group["diagnostics"]
        .as_array()
        .expect("a group has diagnostics");
    assert!(
        !diagnostics.is_empty(),
        "the placeholder carries its diagnosis"
    );
    assert_eq!(diagnostics[0]["severity"], "error");
    assert_eq!(diagnostics[0]["evidence"]["artifact"], "order");

    // And the envelope's flat list contains every diagnostic the groups hold — it is a
    // superset of them, never a subset, because it is the member a consumer reads
    // without knowing which command produced the response. Asserted as containment
    // rather than by matching one code, because matching one code passes on an envelope
    // that dropped every other group.
    let flat = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    let grouped: Vec<&serde_json::Value> = groups
        .iter()
        .flat_map(|group| {
            group["diagnostics"]
                .as_array()
                .expect("a group has diagnostics")
        })
        .collect();
    assert!(
        !grouped.is_empty(),
        "the Package grouped something to contain"
    );
    for diagnostic in &grouped {
        assert!(
            flat.contains(diagnostic),
            "the envelope carries every grouped diagnostic: {diagnostic}"
        );
    }
    assert!(
        flat.len() >= grouped.len(),
        "and is a superset of them: {} against {}",
        flat.len(),
        grouped.len()
    );

    // The rest of the Package is intact: the Vendors, the README, and the value lists
    // the surviving Order contributes.
    assert_eq!(data["readme"]["text"], "Telescope purchasing.\n");
    assert_eq!(data["vendors"].as_array().map(Vec::len), Some(1));
    assert_eq!(data["values"]["orderers"][0]["usage_count"], 1);
}

/// Every load state a Package can be in produces its own typed answer.
///
/// The defect this catches is one generic failure for five different situations: a user
/// told only that "the Package could not be read" has no idea whether to run `init`,
/// `repair`, `upgrade`, or fix a permission. The codes come from the core's typed
/// result, not from anything this adapter inferred, and the exit status distinguishes
/// success from failure as decided.
#[test]
fn each_load_state_answers_with_its_own_code_and_exit_status() {
    let (_healthy_tree, healthy_root) = populated_package("show-state-healthy");
    let unmanaged = directory_that_is_not_a_root(&healthy_root);
    let (_damaged_tree, damaged_root) = package_with_duplicate_vendor_names("show-state-damaged");
    let (_version_tree, version_root) = package_at_an_uninterpretable_version("show-state-version");
    let (_unreadable_tree, unreadable_root) =
        package_root_that_cannot_be_read("show-state-unreadable");

    let mut codes = Vec::new();
    for (label, target, status, expected) in [
        ("healthy", healthy_root.as_path(), 0, None),
        (
            "unmanaged directory",
            unmanaged.as_path(),
            1,
            Some("package_root_unmanaged"),
        ),
        (
            "damaged Package",
            damaged_root.as_path(),
            1,
            Some("package_root_damaged"),
        ),
        (
            "uninterpretable metadata version",
            version_root.as_path(),
            1,
            Some("package_metadata_unsupported"),
        ),
        (
            "unreadable marker",
            unreadable_root.as_path(),
            1,
            Some("package_unreadable"),
        ),
    ] {
        let outcome = Invocation::new(&["package", "show", "--json"])
            .rooted_at(target)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(status), "{label}");
        assert_eq!(document["ok"], status == 0, "{label}");
        assert_eq!(document["data"].is_null(), status != 0, "{label}");
        assert_eq!(
            document["contract"], "spielorders.package.show",
            "{label}: a load state is this command's answer, not another contract's"
        );
        match expected {
            Some(code) => {
                assert_eq!(document["error"]["code"], code, "{label}");
                assert!(
                    document["error"]["scope"].as_str().is_some(),
                    "{label}: a failure names the scope it is about"
                );
                codes.push(code);
            }
            None => assert!(document["error"].is_null(), "{label}"),
        }
    }

    let distinct = {
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(
        distinct,
        codes.len(),
        "each load state has its own code rather than one generic failure: {codes:?}"
    );
}

/// A damaged Package's diagnostics *are* its answer, so they travel in the envelope of a
/// failed response.
///
/// The defect this catches: a failure that drops what the load observed, leaving a user
/// at exit 1 with a sentence and no diagnosis. `data` is `null` on every failure, so the
/// envelope's `diagnostics` is the only channel a failed read has.
#[test]
fn a_damaged_package_reports_its_diagnosis_in_the_failed_response() {
    let (_tree, root) = package_with_duplicate_vendor_names("show-damaged-package");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_damaged");
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8"),
        "the Root resolved, so the context names what the command was about"
    );
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    assert_eq!(
        diagnostics.len(),
        1,
        "the diagnosis reaches the caller: {diagnostics:?}"
    );
    assert_eq!(diagnostics[0]["code"], "vendor_name_duplicate");
    assert_eq!(diagnostics[0]["severity"], "error");
    assert_eq!(
        diagnostics[0]["evidence"]["observed_names"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "the evidence names both spellings rather than being a null nobody contracted"
    );
}

/// A Package declaring a metadata version this build cannot interpret is a state with
/// its own command, not a repair.
///
/// The defect this catches: folding it into `package_root_damaged`, which would send a
/// user to `repair` for something only `upgrade` or a newer build can settle.
#[test]
fn an_uninterpretable_metadata_version_is_its_own_answer() {
    let (_tree, root) = package_at_an_uninterpretable_version("show-unsupported-version");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "package_metadata_unsupported");
    assert!(
        document["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("upgrade")),
        "the message names the command that settles it: {}",
        document["error"]["message"]
    );
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "managed_artifact_wrong_version"),
        "the observed version is reported rather than only summarized: {diagnostics:?}"
    );
    let wrong = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "managed_artifact_wrong_version")
        .expect("the diagnostic was just found");
    assert_eq!(wrong["evidence"]["artifact"], "version");
    assert_eq!(wrong["evidence"]["observed_version"], 999);
    assert_eq!(wrong["evidence"]["expected_version"], 1);
}

/// The human summary names the same entities the machine response carries.
///
/// Human output is never a parsing contract, so this asserts that the entities are
/// *named* rather than pinning the sentences. The defect it catches is a human mode that
/// reports counts and shapes while leaving a reader unable to see which Orders and
/// Vendors the Package holds — the summary would agree with the JSON and still be
/// useless.
#[test]
fn the_human_summary_names_the_same_entities_as_the_machine_response() {
    let (_tree, root) = populated_package("show-human");

    let outcome = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    let text = outcome.stdout_text();

    assert_eq!(outcome.status, Some(0));
    for named in [
        "Laser components",
        "Mirror mounts",
        "Tarnwick",
        "Northwind",
        "Ada Lovelace",
        "Telescope",
        "Summer build",
    ] {
        assert!(text.contains(named), "the summary names {named}:\n{text}");
    }
    assert!(
        text.contains(root.to_str().expect("the test root is UTF-8")),
        "the summary names the Package Root:\n{text}"
    );
    // The Hidden Vendor is omitted from the Vendor list and still named by the Order
    // that references it, which is the same rule the machine response follows: hiding
    // governs what is offered for new use and never what is resolved.
    assert!(
        !text.contains("Vendor: Nethermoor"),
        "the Hidden Vendor is not offered in the Vendor list:\n{text}"
    );
    assert!(
        text.contains("Mirror mounts") && text.contains("Nethermoor"),
        "and the Order that references it still names it:\n{text}"
    );
    assert!(
        text.contains("Hidden Vendors: omitted"),
        "the summary says which list this is, so an absent Vendor is not a missing one:\n{text}"
    );
    assert!(
        outcome.stderr.is_empty(),
        "a clean read warns about nothing: {}",
        outcome.stderr_text()
    );
}

/// UUIDs appear only under `--verbose`, and the machine response always carries them.
///
/// The defect this catches: identities in the default human summary, which is the
/// decision's one explicit statement about what `--verbose` is for. A human reading a
/// Package wants titles and names.
#[test]
fn uuids_are_verbose_only_in_human_output_and_always_present_in_json() {
    let (_tree, root) = populated_package("show-uuids");

    let plain = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    assert!(
        !plain.stdout_text().contains(NEWEST_ORDER_ID),
        "the default summary carries no UUID:\n{}",
        plain.stdout_text()
    );
    assert!(!plain.stdout_text().contains(OFFERED_VENDOR_ID));

    let verbose = Invocation::new(&["package", "show", "--verbose"])
        .rooted_at(&root)
        .run();
    assert_eq!(verbose.status, Some(0));
    assert!(
        verbose.stdout_text().contains(NEWEST_ORDER_ID),
        "--verbose names the Order identities:\n{}",
        verbose.stdout_text()
    );
    assert!(
        verbose.stdout_text().contains(OFFERED_VENDOR_ID),
        "--verbose names the Vendor identities:\n{}",
        verbose.stdout_text()
    );

    let document = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(document["data"]["orders"][0]["order_id"], NEWEST_ORDER_ID);
    assert_eq!(
        document["data"]["vendors"][0]["vendor_id"],
        OFFERED_VENDOR_ID
    );
}

/// `--quiet` says nothing about a Package it could read, and still reports one it could
/// not.
#[test]
fn quiet_says_nothing_on_success_and_still_reports_a_failure() {
    let (_tree, root) = populated_package("show-quiet");
    let unmanaged = directory_that_is_not_a_root(&root);

    let success = Invocation::new(&["package", "show", "--quiet"])
        .rooted_at(&root)
        .run();
    assert_eq!(success.status, Some(0));
    assert_eq!(success.stdout, b"", "{}", success.stdout_text());
    assert_eq!(success.stderr, b"", "{}", success.stderr_text());

    let failure = Invocation::new(&["package", "show", "--quiet"])
        .rooted_at(&unmanaged)
        .run();
    assert_eq!(failure.status, Some(1));
    assert_eq!(failure.stdout, b"");
    assert!(
        failure.stderr_text().contains("not a SpielOrders Root"),
        "a failure is still reported: {}",
        failure.stderr_text()
    );
}

/// A Damaged Order is an error-severity diagnostic about a Package that read fine, so it
/// reaches standard error even under `--quiet`.
///
/// The defect this catches: routing every diagnostic through the warning path, which
/// `--quiet` silences — leaving a `--quiet` run at exit 0 having said nothing about an
/// Order it could not read. `--quiet` says nothing on success and still reports errors.
#[test]
fn an_error_severity_diagnostic_is_reported_even_under_quiet() {
    let (_tree, root, _damaged) = package_with_a_damaged_order("show-quiet-diagnostic");

    let outcome = Invocation::new(&["package", "show", "--quiet"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "the Package itself read fine");
    assert_eq!(outcome.stdout, b"", "still nothing on success");
    assert!(
        outcome
            .stderr_text()
            .contains("managed_artifact_invalid_syntax"),
        "the unreadable Order's own diagnostic is named on standard error: {}",
        outcome.stderr_text()
    );
}

/// `--all` is a presentation option and nothing else.
///
/// The defect this catches: `--all` leaking into the Package's own facts — a value list
/// or an Order set that changes with a Vendor-visibility flag. Only the Vendor list and
/// the flag that describes it may differ between the two runs.
#[test]
fn the_visibility_flag_changes_only_the_vendor_list() {
    let (_tree, root) = populated_package("show-all-scope");

    let default = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let all = response(
        &Invocation::new(&["package", "show", "--json", "--all"])
            .rooted_at(&root)
            .run(),
    );

    for member in [
        "paths",
        "metadata",
        "readme",
        "orders",
        "damaged_orders",
        "values",
        "index",
        "diagnostic_groups",
    ] {
        assert_eq!(
            default["data"][member], all["data"][member],
            "`--all` is a presentation choice and does not touch `{member}`"
        );
    }
    assert_ne!(default["data"]["vendors"], all["data"]["vendors"]);
    assert_ne!(
        default["data"]["hidden_vendors_included"],
        all["data"]["hidden_vendors_included"]
    );
}

/// Every location this payload carries appears exactly once.
///
/// The defect this catches: the same managed path under two keys, so a later change
/// moves one and leaves the other — and a consumer that read the stale one is wrong
/// about where the file is. `paths` holds the locations with no state of their own; the
/// README and the derived index are named beside the state that is the reason to ask.
#[test]
fn no_managed_location_is_reported_under_two_keys() {
    let (_tree, root) = populated_package("show-one-place-per-path");

    let document = response(
        &Invocation::new(&["package", "show", "--json", "--all"])
            .rooted_at(&root)
            .run(),
    );

    let mut absolute = Vec::new();
    collect_absolute_paths(&document["data"], &mut absolute);
    let mut sorted = absolute.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        absolute.len(),
        "one path is reported twice in `data`: {absolute:?}"
    );
}

/// Every `absolute_path` under `value`, at any depth.
fn collect_absolute_paths(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(members) => {
            for (key, member) in members {
                if key == "absolute_path" {
                    if let Some(path) = member.as_str() {
                        found.push(path.to_owned());
                    }
                } else {
                    collect_absolute_paths(member, found);
                }
            }
        }
        serde_json::Value::Array(entries) => {
            for entry in entries {
                collect_absolute_paths(entry, found);
            }
        }
        _ => {}
    }
}

/// A Package README that exists and cannot be read is a warning, not damage, and its
/// diagnostic reaches the envelope.
///
/// The defect this catches: the README read's own diagnostics being dropped, so a
/// `readme.text` of `null` arrives with nothing saying why. The Package's own load did
/// not perform this read and therefore did not group it, which is exactly why the
/// envelope's `diagnostics` is a superset of `data.diagnostic_groups` rather than a copy
/// of it — and why both modes have to report it.
#[test]
fn an_unreadable_package_readme_is_a_warning_the_envelope_carries() {
    let (_tree, root) = package_with_unreadable_readme("show-unreadable-readme");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "an optional document is not damage"
    );
    assert_eq!(document["data"]["readme"]["exists"], true);
    assert!(
        document["data"]["readme"]["text"].is_null(),
        "the read does not know what stands there, which is not the same as blank"
    );
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "readme_unavailable");
    assert_eq!(
        diagnostics[0]["evidence"]["document_kind"],
        "package_readme"
    );
    assert_eq!(diagnostics[0]["evidence"]["reason"], "wrong_kind");
    assert_eq!(
        document["data"]["diagnostic_groups"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "the load grouped nothing, because the load did not perform this read"
    );

    let human = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    assert!(
        human.stderr_text().contains("not a readable document"),
        "the human mode reports it too: {}",
        human.stderr_text()
    );
}

/// An entry the Orders Directory does not allow is reported with the kind that was
/// observed there.
///
/// The defect this catches: an `observed_kind` this response contract never contracted,
/// arriving as `null` beside a code that means "something of the wrong kind is here" —
/// which leaves the one fact the code is about out of the machine contract.
#[test]
fn a_disallowed_orders_directory_entry_names_the_kind_observed_there() {
    let (_tree, root) = populated_package("show-disallowed-entry");
    std::fs::write(root.join("Orders").join("stray.txt"), b"not an Order")
        .expect("a plain file is placed among the Order Directories");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "a stray entry is not damage");
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    let disallowed = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "orders_directory_entry_disallowed")
        .unwrap_or_else(|| panic!("the stray entry is reported: {diagnostics:?}"));
    assert_eq!(disallowed["severity"], "warning");
    assert_eq!(
        disallowed["evidence"]["observed_kind"], "file",
        "the kind is the core's own spelling, not one the adapter invented"
    );
    assert_eq!(
        disallowed["primary_location"]["package_relative_path"],
        "Orders/stray.txt"
    );
}
