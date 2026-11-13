//! `spielorders package show` — the complete GUI-bootstrap read.
//!
//! One invocation returns everything a client needs to open a Package. The interesting
//! half is that a Package which is damaged, unmanaged, or at a metadata version this
//! build does not interpret answers this command too, because saying so is the answer — so the load states are tested as
//! a set, each with its own typed code and exit status, rather than as one generic
//! failure.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    Invocation, damage_index, directory_that_is_not_a_root, hidden_vendor_id, managed_snapshot,
    modification_time, newest_order_id, offered_vendor_id, package_at_an_uninterpretable_version,
    package_root_that_cannot_be_read, package_with_a_damaged_order,
    package_with_duplicate_vendor_names, package_with_unreadable_readme, populated_package,
    remove_index, response, reversion_index, touch_an_order_record,
};
use std::path::Path;

/// One state the derived Package Index can be in, and what puts a Package into it.
struct IndexState {
    label: &'static str,
    /// The drift or damage this state needs, and nothing at all for a current index.
    prepare: Option<fn(&Path)>,
}

/// The five states, and the current one is what a Package left alone is already in.
const INDEX_STATES: [IndexState; 5] = [
    IndexState {
        label: "current",
        prepare: None,
    },
    IndexState {
        label: "stale",
        prepare: Some(touch_an_order_record),
    },
    IndexState {
        label: "missing",
        prepare: Some(remove_index),
    },
    IndexState {
        label: "malformed",
        prepare: Some(damage_index),
    },
    IndexState {
        label: "wrong-version",
        prepare: Some(reversion_index),
    },
];

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

/// The complete read carries every Vendor with its visibility, the human output offers
/// Hidden Vendors only under `--all`, and an Order referencing a Hidden Vendor resolves
/// that Vendor's facts either way.
///
/// The defect this catches is the one the visibility default is most likely to cause:
/// resolving an Order's Vendor against the *presented* list, so the second Order's
/// Vendor vanishes whenever `--all` is omitted. Hiding governs what is offered for new
/// use and never what is resolved.
#[test]
fn hidden_vendors_are_omitted_by_default_and_still_resolve_for_the_orders_that_name_them() {
    let (_tree, root) = populated_package("show-hidden-vendors");

    // The machine response carries every Vendor with its visibility, so an Order naming
    // a Hidden Vendor resolves against it whatever the flag says.
    for arguments in [
        vec!["package", "show", "--json"],
        vec!["package", "show", "--json", "--all"],
    ] {
        let document = response(&Invocation::new(&arguments).rooted_at(&root).run());
        let package = &document["data"];
        let vendors = package["vendors"].as_array().expect("vendors is a list");
        let resolved = |vendor_id: &serde_json::Value| {
            vendors
                .iter()
                .find(|vendor| &vendor["vendor_id"] == vendor_id)
                .unwrap_or_else(|| panic!("{arguments:?}: {vendor_id} resolves"))
        };

        let orders = package["orders"].as_array().expect("orders is a list");
        assert_eq!(
            orders[1]["vendor_id"],
            &*hidden_vendor_id(&root),
            "{arguments:?}: the Order still names the Hidden Vendor"
        );
        let hidden = resolved(&orders[1]["vendor_id"]);
        assert_eq!(hidden["name"], "Nethermoor", "{arguments:?}");
        assert_eq!(hidden["hidden"], "true", "{arguments:?}");
        assert_eq!(
            orders[0]["vendor_id"],
            &*offered_vendor_id(&root),
            "{arguments:?}"
        );
        assert_ne!(
            resolved(&orders[0]["vendor_id"])["hidden"],
            "true",
            "{arguments:?}"
        );
    }

    // The Vendor list a reader is offered omits the Hidden Vendor unless asked for all.
    let offered = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    assert!(
        offered.stdout_text().contains("name: Tarnwick"),
        "{}",
        offered.stdout_text()
    );
    assert!(
        !offered.stdout_text().contains("name: Nethermoor"),
        "the Hidden Vendor is omitted by default:\n{}",
        offered.stdout_text()
    );
    let all = Invocation::new(&["package", "show", "--all"])
        .rooted_at(&root)
        .run();
    assert!(
        all.stdout_text().contains("name: Nethermoor"),
        "`--all` lists the Hidden Vendor:\n{}",
        all.stdout_text()
    );
}

/// Every load state a Package can be in produces its own typed answer.
///
/// The defect this catches is one generic failure for five different situations: a user
/// told only that "the Package could not be read" has no idea whether to run `init` or
/// `repair`, to fix a permission, or that nothing here reads it. The codes come from the
/// core's typed result, not from anything this adapter inferred, and the exit status distinguishes
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
            Some("unmanaged_package"),
        ),
        (
            "damaged Package",
            damaged_root.as_path(),
            1,
            Some("damaged_package"),
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
            Some("package_operation_failed"),
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
                assert_eq!(
                    document["context"]["root"].as_str(),
                    target.to_str(),
                    "{label}: a failure names the Root it is about: {document}"
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
        "Example Institute",
        "Ada Lovelace",
        "Telescope",
        "Summer build",
    ] {
        assert!(text.contains(named), "the summary names {named}:\n{text}");
    }
    // The Hidden Vendor is omitted from the Vendor list and still named by the Order
    // that references it: hiding governs what is offered for new use and never what is
    // resolved.
    assert!(
        !text.contains("name: Nethermoor"),
        "the Hidden Vendor is not offered in the Vendor list:\n{text}"
    );
    assert!(
        outcome.stderr.is_empty(),
        "a clean read warns about nothing: {}",
        outcome.stderr_text()
    );

    let verbose = Invocation::new(&["package", "show", "--verbose"])
        .rooted_at(&root)
        .run();
    let detailed = verbose.stdout_text();
    assert!(
        detailed.contains(root.to_str().expect("the test root is UTF-8")),
        "the detailed summary names the Package Root:\n{detailed}"
    );
    assert!(
        detailed.contains(&format!("vendor id: {}", hidden_vendor_id(&root))),
        "and the Order that references the Hidden Vendor still names it:\n{detailed}"
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
        !plain.stdout_text().contains(&*newest_order_id(&root)),
        "the default summary carries no UUID:\n{}",
        plain.stdout_text()
    );
    assert!(!plain.stdout_text().contains(&*offered_vendor_id(&root)));

    let verbose = Invocation::new(&["package", "show", "--verbose"])
        .rooted_at(&root)
        .run();
    assert_eq!(verbose.status, Some(0));
    assert!(
        verbose.stdout_text().contains(&*newest_order_id(&root)),
        "--verbose names the Order identities:\n{}",
        verbose.stdout_text()
    );
    assert!(
        verbose.stdout_text().contains(&*offered_vendor_id(&root)),
        "--verbose names the Vendor identities:\n{}",
        verbose.stdout_text()
    );

    let document = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let package = &document["data"];
    assert_eq!(package["orders"][0]["order_id"], &*newest_order_id(&root));
    assert_eq!(
        package["vendors"][0]["vendor_id"],
        &*offered_vendor_id(&root)
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
            .contains("error: managed artifact invalid syntax"),
        "the unreadable Order's own diagnostic is named on standard error: {}",
        outcome.stderr_text()
    );
}

/// `--all` is a presentation option and nothing else.
///
/// The defect this catches: `--all` leaking into the Package's own facts — a value list
/// or an Order set that changes with a Vendor-visibility flag. The machine response is
/// the same either way, and the human Vendor list is the only thing the flag adds to.
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

    assert_eq!(
        default["data"], all["data"],
        "`--all` is a presentation choice and does not touch the Package's facts"
    );

    let offered = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    let whole = Invocation::new(&["package", "show", "--all"])
        .rooted_at(&root)
        .run();
    let mut remaining = whole.stdout_text().lines();
    for line in offered.stdout_text().lines() {
        assert!(
            remaining.any(|candidate| candidate == line),
            "`--all` only adds to the default rendering; `{line}` is missing:\n{}",
            whole.stdout_text()
        );
    }
    let added: Vec<&str> = whole
        .stdout_text()
        .lines()
        .filter(|line| !offered.stdout_text().lines().any(|kept| kept == *line))
        .collect();
    assert!(
        added.contains(&"  - name: Nethermoor"),
        "what `--all` adds is the Hidden Vendor: {added:?}"
    );
}

/// A Package README that exists and cannot be read is a warning, not damage, and its
/// diagnostic reaches the response.
///
/// The defect this catches: the README read's own diagnostics being dropped, so a
/// `readme.text` of `null` arrives with nothing saying why. The Package's own load did
/// not perform this read and therefore did not group it; the README read carries it
/// beside its text — and both modes have to report it.
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
    assert_eq!(
        document["data"]["readme"]["state"], "unreadable",
        "the read does not know what stands there, which is not the same as missing"
    );
    let diagnostics = document["diagnostics"]
        .as_array()
        .expect("diagnostics is a list");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "readme_unavailable");
    assert_eq!(diagnostics[0]["severity"], "warning");

    let human = Invocation::new(&["package", "show"]).rooted_at(&root).run();
    assert!(
        human.stderr_text().contains("warning: readme unavailable"),
        "the human mode reports it too: {}",
        human.stderr_text()
    );
}
