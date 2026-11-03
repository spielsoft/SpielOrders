//! The response envelope, the `contract` identity registry, and the golden fixtures.
//!
//! This is the artifact every later command ticket consumes and none of them may
//! redefine, so it is tested as a *set* — every registered contract, every envelope
//! member, every fixture cell — rather than as whatever `package readme` happens to
//! emit. The failure mode being guarded against is contract drift: a `contract` string
//! that collides with another command's, a version bumped for a wording change, an
//! empty collection present in every command but one.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution.

use crate::support::{
    ENVELOPE, Invocation, NEWEST_ORDER_ID, TemporaryTree, assert_golden, assert_matches_golden,
    assert_matches_golden_with_minted_vendor_id, assert_matches_golden_with_substitutions,
    directory_that_is_not_a_root, entry_names, fixture_directory, initialized_package,
    initialized_package_with_a_blocked_index_refresh, order_directory,
    package_at_an_uninterpretable_version, package_root_that_cannot_be_read,
    package_with_a_blocked_index_refresh, package_with_a_damaged_order,
    package_with_a_drifted_order, package_with_a_drifted_order_and_a_blocked_index_refresh,
    package_with_a_package_case_and_a_blocked_index_refresh, package_with_a_staging_artifact,
    package_with_an_adoption_candidate, package_with_an_empty_order,
    package_with_an_impossible_date, package_with_an_occupied_aligned_name,
    package_with_an_occupied_removed_name, package_with_an_order_staging_artifact,
    package_with_an_unreadable_order_readme, package_with_an_unreferenced_vendor,
    package_with_an_unresolved_vendor_reference, package_with_duplicate_vendor_names,
    package_with_three_contacts, package_with_three_contacts_and_a_blocked_index_refresh,
    package_with_three_items, package_with_three_items_and_a_blocked_index_refresh,
    package_with_unreadable_readme, populated_package, response, supporting_material_provider,
    unmanaged_directory,
};
use spielorders_cli::contract::ResponseContract;
use std::path::{Path, PathBuf};

/// The fixture cells `package readme` records.
///
/// `success` is a Package whose conventional README is there and `empty-result` is one
/// where it is not — the two differ in exactly one fact, which is the point of having
/// both. `warning` is a README that exists and is not a readable document, and it is the
/// only cell in which the envelope's `diagnostics` member is non-empty: without it the
/// diagnostic projection every later command inherits would have no golden evidence at
/// all. `invalid` is a `--root` that is not a SpielOrders Root.
const README_CELLS: [&str; 4] = ["success", "empty-result", "warning", "invalid"];

/// The fixture cells `package show` records.
///
/// `success` is a Package with everything this command reports; `empty-result` is an
/// empty Package, where the answer is that every collection is empty rather than that
/// there is no answer; `warning` is a readable
/// Package carrying a Damaged Order, which is the cell where the diagnostic groups and
/// the placeholder are pinned; `invalid` is a Package the load could read and not trust,
/// which is where a *failed* response carrying diagnostics is pinned.
const SHOW_CELLS: [&str; 4] = ["success", "empty-result", "warning", "invalid"];

// ---------------------------------------------------------------------------
// The registry.
// ---------------------------------------------------------------------------

/// The registry is the reason a `contract` string exists in exactly one place: two
/// commands cannot be kept distinct unless their identities are comparable somewhere.
#[test]
fn every_registered_contract_has_a_distinct_identity() {
    let mut names: Vec<&str> = ResponseContract::REGISTERED
        .iter()
        .map(|contract| contract.name())
        .collect();
    let registered = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        registered,
        "two commands share a `contract` identity: {names:?}"
    );
}

/// A contract that appeared in neither exhaustive match would be unnameable; one absent
/// from `REGISTERED` would be invisible to every check in this file, including the
/// fixture checks below. This is what makes the slice the registry rather than a list.
#[test]
fn every_registered_contract_is_named_and_versioned() {
    for contract in ResponseContract::REGISTERED {
        let name = contract.name();
        assert!(
            name.starts_with("spielorders."),
            "{name} is namespaced to this application"
        );
        assert!(
            name.split('.').skip(1).all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|character| character.is_ascii_lowercase() || character == '_')
            }),
            "{name} is `snake_case` segments naming the command tree"
        );
        assert!(
            contract.version() >= 1,
            "{name} carries a consecutive positive integer, not a zero or a semver"
        );
    }
}

/// The fixture home is derived from the identity rather than chosen per command, so a
/// contract cannot be filed under a directory that says something else and a version
/// bump lands in a new directory instead of editing the old one.
#[test]
fn every_registered_contract_has_its_own_fixture_directory() {
    let mut directories: Vec<String> = ResponseContract::REGISTERED
        .iter()
        .map(|contract| {
            let directory = contract.fixture_directory();
            assert!(
                directory.ends_with(&format!("/v{}", contract.version())),
                "{} is filed under its own contract version: {directory}",
                contract.name()
            );
            directory
        })
        .collect();
    let registered = directories.len();
    directories.sort_unstable();
    directories.dedup();
    assert_eq!(
        directories.len(),
        registered,
        "two contracts share a fixture directory: {directories:?}"
    );
}

/// A registered command whose fixtures nobody added fails here, without anyone having
/// remembered to write a check for that command. That is the whole reason the fixture
/// path is derived from the registry and read at run time.
#[test]
fn every_registered_contract_has_a_populated_fixture_directory() {
    for contract in ResponseContract::REGISTERED {
        let directory = fixture_directory(*contract);
        assert!(
            directory.is_dir(),
            "{} has no fixture directory at {}",
            contract.name(),
            directory.display()
        );
        let present = entry_names(&directory);
        assert!(
            !present.is_empty(),
            "{} has an empty fixture directory at {}",
            contract.name(),
            directory.display()
        );
        for name in &present {
            assert!(
                name.ends_with(".json"),
                "{} holds a fixture that is not a response document: {name}",
                directory.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The envelope.
// ---------------------------------------------------------------------------

/// One `--json` run per kind of response this command can produce: a clean read, a read
/// that carries a diagnostic, a domain failure, and a usage error.
///
/// The trees come back with the runs because a `TemporaryTree` deletes its Package when
/// it drops, and a run against a deleted Package would test the wrong failure.
fn envelope_runs(label: &str) -> (Vec<TemporaryTree>, Vec<Invocation>) {
    let (tree, root) = initialized_package(label);
    let outside = directory_that_is_not_a_root(&root);

    let (warning_tree, warning_root) = package_with_unreadable_readme(&format!("{label}-warning"));

    let runs = vec![
        Invocation::new(&["package", "readme", "--json"]).rooted_at(&root),
        Invocation::new(&["package", "readme", "--json"]).rooted_at(&warning_root),
        Invocation::new(&["package", "readme", "--json"]).rooted_at(&outside),
        Invocation::new(&["package", "readme", "--json", "--jsom"]),
    ];
    (vec![tree, warning_tree], runs)
}

/// Seven members, always all seven — on success and on failure. A member absent rather
/// than `null` is the drift that makes a consumer write `if "data" in response`, and
/// once one command does it the rule is gone.
#[test]
fn every_response_carries_the_whole_envelope() {
    let (_trees, runs) = envelope_runs("envelope-shape");
    for (index, run) in runs.into_iter().enumerate() {
        let document = response(&run.run());
        let mut members: Vec<&str> = document
            .as_object()
            .expect("a response is a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        members.sort_unstable();
        let mut expected = ENVELOPE;
        expected.sort_unstable();
        assert_eq!(members, expected, "run {index}");
    }
}

/// And in one order, which is a fact about the bytes rather than about the parse.
///
/// Asserted against the raw document because `serde_json::Value` holds its members
/// sorted: reading the order back through a parse would compare the parser's ordering to
/// itself and pass no matter what the executable wrote.
#[test]
fn every_response_carries_the_envelope_in_one_order() {
    let (_trees, runs) = envelope_runs("envelope-order");
    for (index, run) in runs.into_iter().enumerate() {
        let outcome = run.run();
        let text = outcome.stdout_text();
        let mut previous = 0;
        for member in ENVELOPE {
            let key = format!("\"{member}\":");
            assert_eq!(
                text.matches(&key).count(),
                1,
                "run {index}: `{member}` names exactly one member of this response, \
                 which is what makes the offset below the top-level one"
            );
            let at = text.find(&key).expect("the member was just counted");
            assert!(
                at > previous,
                "run {index}: `{member}` is out of the envelope's order:\n{text}"
            );
            previous = at;
        }
    }
}

/// `ok` and the exit status are two spellings of one fact. They are asserted together
/// everywhere so the pair cannot drift: a response saying `ok: true` at exit `1` would
/// make every caller that trusts one of them wrong about the other.
#[test]
fn ok_and_the_exit_status_never_disagree() {
    let (_tree, root) = initialized_package("envelope-ok-and-status");
    let outside = directory_that_is_not_a_root(&root);

    for (run, status) in [
        (
            Invocation::new(&["package", "readme", "--json"]).rooted_at(&root),
            0,
        ),
        (
            Invocation::new(&["package", "readme", "--json"]).rooted_at(&outside),
            1,
        ),
        (
            Invocation::new(&["package", "readme", "--json", "--jsom"]),
            2,
        ),
    ] {
        let outcome = run.run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(status));
        assert_eq!(document["ok"], status == 0, "exit {status}");
        assert_eq!(
            document["data"].is_null(),
            status != 0,
            "exit {status}: a failure carries no data"
        );
        assert_eq!(
            document["error"].is_null(),
            status == 0,
            "exit {status}: a success carries no error"
        );
    }
}

/// An absent collection is present and empty. Omitting it makes every consumer write a
/// null check, and omitting it in *one* command is how a consumer written against all the
/// others breaks.
#[test]
fn absent_collections_are_present_and_empty() {
    let (_tree, root) = initialized_package("envelope-empty-collections");

    let success = response(
        &Invocation::new(&["package", "readme", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(
        success["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "a clean read has no diagnostics, and says so with an empty array"
    );

    let failure = response(&Invocation::new(&["package", "readme", "--json", "--jsom"]).run());
    for member in ["paths", "identities"] {
        assert_eq!(
            failure["error"][member].as_array().map(Vec::len),
            Some(0),
            "a usage error is about no path and no identity: {member}"
        );
    }
}

// ---------------------------------------------------------------------------
// The golden fixtures.
// ---------------------------------------------------------------------------

/// Every recorded cell is one this file names for comparison.
///
/// **The bridge between the fixture tree and automatic comparisons.** Each golden test
/// names its cells, so a cell standing in the tree that no list
/// names would never have its bytes compared — and the check that the tree holds the right
/// set would pass, because the file is there.
///
/// Supporting Material success is compared here before its key enters the inventory; other cells use
/// process-level comparisons below. Which cells a contract supplies
/// and why it supplies no others is not this file's question: it is the matrix in
/// `tests/cross_command_conformance.rs`.
#[test]
fn every_recorded_cell_is_one_this_file_compares() {
    let reads: Vec<&str> = NARROWER_READ_CELLS.iter().map(|cell| cell.name()).collect();
    let mutations: Vec<&str> = MUTATION_CELLS.iter().map(|cell| cell.name()).collect();
    let mut supporting_cells = SUPPORTING_MATERIAL_CELLS.to_vec();
    supporting_cells.push(compare_supporting_material_success());

    let mut compared: Vec<(ResponseContract, Vec<&str>)> = vec![
        (ResponseContract::PackageReadme, README_CELLS.to_vec()),
        (ResponseContract::PackageShow, SHOW_CELLS.to_vec()),
        (ResponseContract::Init, INIT_CELLS.to_vec()),
        (ResponseContract::VendorShow, reads.clone()),
        (ResponseContract::Validate, VALIDATE_CELLS.to_vec()),
        (
            ResponseContract::OrderValidate,
            ORDER_VALIDATE_CELLS.to_vec(),
        ),
        (ResponseContract::IndexStatus, INDEX_STATUS_CELLS.to_vec()),
        (ResponseContract::IndexRebuild, INDEX_REBUILD_CELLS.to_vec()),
        (ResponseContract::Repair, REPAIR_CELLS.to_vec()),
        (ResponseContract::OrderRepair, REPAIR_CELLS.to_vec()),
        (ResponseContract::Upgrade, UPGRADE_CELLS.to_vec()),
        (ResponseContract::SearchSupportingMaterial, supporting_cells),
    ];
    for contract in [
        ResponseContract::SearchOrders,
        ResponseContract::SearchVendors,
    ] {
        compared.push((
            contract,
            ENTITY_SEARCH_CELLS
                .iter()
                .filter(|recorded| recorded.contract == contract)
                .map(|recorded| recorded.cell)
                .collect(),
        ));
    }
    for (contract, _, _) in NARROWER_READS {
        compared.push((contract, reads.clone()));
    }
    for (contract, _) in ORDER_READS {
        compared.push((contract, reads.clone()));
    }
    for contract in VENDOR_MUTATIONS
        .into_iter()
        .chain(CONTACT_MUTATIONS)
        .chain(NEW_ORDER_MUTATIONS)
        .chain(RECORD_MUTATIONS)
    {
        compared.push((contract, mutations.clone()));
    }
    for (contract, _) in RELOCATION_MUTATIONS {
        compared.push((contract, mutations.clone()));
    }

    let mut named: Vec<&str> = compared
        .iter()
        .map(|(contract, _)| contract.name())
        .collect();
    named.sort_unstable();
    named.dedup();
    let mut expected: Vec<&str> = ResponseContract::REGISTERED
        .iter()
        .filter(|contract| **contract != ResponseContract::Usage)
        .map(|contract| contract.name())
        .collect();
    expected.sort_unstable();
    assert_eq!(
        named, expected,
        "every contract with recorded bytes is compared here, and only those"
    );

    for (contract, cells) in compared {
        let mut expected: Vec<String> = cells.iter().map(|cell| format!("{cell}.json")).collect();
        expected.sort();
        let directory = fixture_directory(contract);
        assert_eq!(
            entry_names(&directory),
            expected,
            "every fixture in {} is a cell this file compares, and every cell has one",
            directory.display()
        );
    }
}

/// A Package whose conventional README is there.
#[test]
fn the_successful_response_matches_its_golden_fixture() {
    let (_tree, root) = initialized_package("golden-success");
    assert!(root.join("README.md").is_file());

    let outcome = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_matches_golden(ResponseContract::PackageReadme, "success", &outcome, &root);
}

/// This command's empty result: the conventional path is the answer, and no document
/// stands there. It is a success, because the path is what was asked for — and the two
/// responses differ in exactly one fact, which is the point of having both fixtures.
#[test]
fn the_empty_result_response_matches_its_golden_fixture() {
    let (_tree, root) = initialized_package("golden-empty-result");
    std::fs::remove_file(root.join("README.md")).expect("the optional README is removed");

    let outcome = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_matches_golden(
        ResponseContract::PackageReadme,
        "empty-result",
        &outcome,
        &root,
    );
}

/// The one cell in which `diagnostics` is non-empty, which makes it the only golden
/// evidence the shared diagnostic projection has.
#[test]
fn the_warning_response_matches_its_golden_fixture() {
    let (_tree, root) = package_with_unreadable_readme("golden-warning");

    let outcome = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(
        outcome.status,
        Some(0),
        "the conventional path is still the answer"
    );
    assert_matches_golden(ResponseContract::PackageReadme, "warning", &outcome, &root);
}

/// A `--root` that is not a SpielOrders Root. The fixture's path is the targeted
/// directory, which is how it records that no ancestor was consulted.
#[test]
fn the_invalid_response_matches_its_golden_fixture() {
    let (_tree, root) = initialized_package("golden-invalid");
    let outside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&outside)
        .run();

    assert_eq!(outcome.status, Some(1));
    assert_matches_golden(ResponseContract::PackageReadme, "invalid", &outcome, &root);
}

/// A usage error carries no Package Root and no path, so its fixture needs no
/// substitution at all — which is what makes it the strictest of them.
#[test]
fn the_usage_error_response_matches_its_golden_fixture() {
    let (_tree, root) = initialized_package("golden-usage-error");

    let outcome = Invocation::new(&["package", "readme", "--json", "--jsom"]).run();

    assert_eq!(outcome.status, Some(2));
    assert_matches_golden(ResponseContract::Usage, "usage-error", &outcome, &root);
}

/// **Every usage error is the same response contract, whichever command line produced
/// it.** The defect this catches is the one the shape started with: a usage error
/// stamped with the identity of whichever command the executable happened to have, so
/// `spielorders --json` — which names no command at all — claimed to be a
/// `package readme` response. `contract` identifies the exact command shape, and a
/// command line that named no command has none.
///
/// It is invisible while one command is registered and wrong for every command but that
/// one afterwards, which is why it is pinned here rather than left for the command that
/// would have exposed it.
#[test]
fn every_usage_error_carries_the_usage_contract() {
    let (_tree, root) = initialized_package("usage-identity");

    for arguments in [
        vec!["--json"],
        vec!["--json", "--jsom"],
        vec!["package", "--json"],
        vec!["package", "readme", "--json", "--jsom"],
        vec!["package", "reamde", "--json"],
        vec!["pakcage", "readme", "--json"],
        vec!["package", "readme", "--json", "--quiet"],
        // The command this milestone's third ticket added. A usage error carrying
        // `spielorders.package.show` is the rule a new command is most likely to break,
        // so every command line that reaches exit `2` through it is listed here too.
        vec!["package", "show", "--json", "--jsom"],
        vec!["package", "show", "--json", "--quiet"],
        vec!["package", "show", "--json", "--all", "--verbose"],
        vec!["package", "shwo", "--json"],
    ] {
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(2), "{arguments:?}");
        assert_eq!(
            document["contract"],
            ResponseContract::Usage.name(),
            "{arguments:?}: no command ran, so no command's contract is claimed"
        );
        assert_eq!(document["error"]["code"], "usage_invalid", "{arguments:?}");
    }
}

/// The usage contract's fixture directory holds exactly the one cell it can produce.
#[test]
fn the_usage_contract_supplies_only_a_usage_error_fixture() {
    let directory = fixture_directory(ResponseContract::Usage);
    assert_eq!(
        entry_names(&directory),
        vec!["usage-error.json".to_owned()],
        "a usage error has one response shape, so its contract has one fixture"
    );
}

/// The two successful fixtures differ in exactly one member.
///
/// Asserted against the fixtures themselves rather than against two live runs, because
/// this is a claim about the recorded contract: a later change that made an absent
/// README report a different path, or no path, would have to edit one of these files and
/// this is what notices.
#[test]
fn a_present_and_an_absent_readme_differ_only_in_existence() {
    let directory = fixture_directory(ResponseContract::PackageReadme);
    let present = std::fs::read_to_string(directory.join("success.json"))
        .expect("the successful fixture is present");
    let absent = std::fs::read_to_string(directory.join("empty-result.json"))
        .expect("the empty-result fixture is present");

    assert_ne!(present, absent, "the two responses are not identical");
    assert_eq!(
        present.replace("\"exists\": true", "\"exists\": false"),
        absent,
        "existence is the only difference between them, so the conventional path is the \
         answer whether or not the optional document is there"
    );
}

/// **A failure is not automatically rootless.**
///
/// The defect: `context` reported `package_root: null` for every failure, including one
/// where the Root had resolved and only the Package under it could not be read. The
/// response was dropping a fact it held — provably, because the failure's own path came
/// back carrying a Package-relative spelling, which only a resolved Root can produce.
/// A consumer reading `context` to learn what the command was about got nothing, and
/// the envelope's documented rule for the member said the opposite of what it did.
#[test]
fn a_failure_under_a_resolved_root_still_names_it() {
    let (_tree, root) = package_root_that_cannot_be_read("context-unreadable-root");

    let outcome = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["ok"], false);
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8"),
        "the Root resolved, so the context names what the command was about"
    );
    assert_eq!(
        document["error"]["paths"][0]["package_relative_path"], ".spielorders/config.toml",
        "and the failure's own path is the thing under the Root that could not be read"
    );
}

/// The converse, so the two cannot be collapsed: a directory that resolved but is not a
/// Root has no `package_root` to name, and the failure's path is what names it.
#[test]
fn a_failure_with_no_root_names_none() {
    let (_tree, root) = initialized_package("context-unmanaged");
    let outside = directory_that_is_not_a_root(&root);

    let document = response(
        &Invocation::new(&["package", "readme", "--json"])
            .rooted_at(&outside)
            .run(),
    );

    assert!(
        document["context"]["package_root"].is_null(),
        "an unmanaged directory is a resolved path, not a Root"
    );
    assert_eq!(
        document["error"]["paths"][0]["path"],
        outside.to_str().expect("the test path is UTF-8")
    );
}

// ---------------------------------------------------------------------------
// `package show`'s golden fixtures.
// ---------------------------------------------------------------------------

/// A Package with Orders, Items, Vendors, a Hidden Vendor, a README, and all four value
/// lists populated, at the **default** Vendor presentation.
///
/// Recorded without `--all` on purpose: this is where the decided default is pinned by
/// bytes — the Hidden Vendor absent from `vendors`, `hidden_vendors_included: false`, and
/// the Order that references it still resolving its facts. The complete Vendor list is
/// pinned by the `warning` cell, which is recorded with `--all`, so the two halves of the
/// visibility default are recorded rather than only tested.
#[test]
fn the_successful_show_response_matches_its_golden_fixture() {
    let (_tree, root) = populated_package("golden-show-success");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_matches_golden(ResponseContract::PackageShow, "success", &outcome, &root);
}

/// This command's empty result: a Package with nothing in it still answers, and every
/// collection is present and empty. A document missing those keys is the drift this
/// fixture exists to catch.
#[test]
fn the_empty_result_show_response_matches_its_golden_fixture() {
    let (_tree, root) = initialized_package("golden-show-empty-result");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_matches_golden(
        ResponseContract::PackageShow,
        "empty-result",
        &outcome,
        &root,
    );
}

/// A readable Package carrying a Damaged Order. The cell that pins the placeholder, the
/// diagnostic groups, and the `managed_artifact` evidence this ticket contracted.
#[test]
fn the_warning_show_response_matches_its_golden_fixture() {
    let (_tree, root, _damaged) = package_with_a_damaged_order("golden-show-warning");

    let outcome = Invocation::new(&["package", "show", "--json", "--all"])
        .rooted_at(&root)
        .run();

    assert_eq!(
        outcome.status,
        Some(0),
        "one Damaged Order does not damage the Package"
    );
    assert_matches_golden(ResponseContract::PackageShow, "warning", &outcome, &root);
}

/// A Package the load could read and could not trust. The cell that pins a *failed*
/// response carrying diagnostics — `data` is `null`, `error` names the code, and the
/// diagnosis is in the envelope because that is the only channel left.
#[test]
fn the_invalid_show_response_matches_its_golden_fixture() {
    let (_tree, root) = package_with_duplicate_vendor_names("golden-show-invalid");

    let outcome = Invocation::new(&["package", "show", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(1));
    assert_matches_golden(ResponseContract::PackageShow, "invalid", &outcome, &root);
}

// ---------------------------------------------------------------------------
// The six narrower Package reads' golden fixtures.
// ---------------------------------------------------------------------------

/// The fixture cells each of the six narrower Package reads supplies.
///
/// `success` is a Package with something in every collection; `empty-result` is an empty
/// Package, where the answer is that the collection is empty rather than that there is no
/// answer; `warning` is a readable Package carrying a Damaged Order, which is where each
/// answer carrying the observation its read made is pinned — and, for the two value lists
/// that Damaged Order carried a value for, where the smaller count is pinned beside the
/// reason for it; `invalid` is a Package the load could read and not trust, where a
/// *failed* response carrying the diagnosis is pinned.
///
/// The six carry the same four rather than each choosing, because they are one set: a cell
/// one of them supplies and another does not is a hole nobody would find until a consumer
/// hit it.
const NARROWER_READ_CELLS: [Cell; 4] = [
    Cell::Success,
    Cell::EmptyResult,
    Cell::Warning,
    Cell::Invalid,
];

#[derive(Clone, Copy, Debug)]
enum Cell {
    Success,
    EmptyResult,
    Warning,
    Invalid,
}

impl Cell {
    fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::EmptyResult => "empty-result",
            Self::Warning => "warning",
            Self::Invalid => "invalid",
        }
    }

    /// The exit status this cell records. `warning` is a success carrying observations.
    fn status(self) -> i32 {
        match self {
            Self::Success | Self::EmptyResult | Self::Warning => 0,
            Self::Invalid => 1,
        }
    }

    /// The Package this cell is recorded against.
    fn package(self, label: &str) -> (TemporaryTree, PathBuf) {
        match self {
            Self::Success => populated_package(label),
            Self::EmptyResult => initialized_package(label),
            Self::Warning => {
                let (tree, root, _damaged) = package_with_a_damaged_order(label);
                (tree, root)
            }
            Self::Invalid => package_with_duplicate_vendor_names(label),
        }
    }
}

/// Ticket 004's six commands, each with the command line that produces its response and
/// the presentation the `warning` cell is recorded at.
///
/// **`success` is recorded at each command's default presentation.** For
/// `package list vendors` that means the Hidden Vendor is absent from `success` and
/// present in `warning`, which is recorded with `--all` — so both halves of the Vendor
/// visibility default are recorded in bytes rather than only tested. It is the same
/// choice `package show` made, for the same reason, and the five commands with no
/// presentation option owe nothing here.
const NARROWER_READS: [(ResponseContract, &[&str], &[&str]); 6] = [
    (
        ResponseContract::PackageListOrders,
        &["package", "list", "orders"],
        &[],
    ),
    (
        ResponseContract::PackageListVendors,
        &["package", "list", "vendors"],
        &["--all"],
    ),
    (
        ResponseContract::PackageValuesOrderingInstitution,
        &["package", "values", "ordering_institution"],
        &[],
    ),
    (
        ResponseContract::PackageValuesOrderer,
        &["package", "values", "orderer"],
        &[],
    ),
    (
        ResponseContract::PackageValuesProject,
        &["package", "values", "project"],
        &[],
    ),
    (
        ResponseContract::PackageValuesOrderGroup,
        &["package", "values", "order_group"],
        &[],
    ),
];

/// One run of one command against one cell's Package, with the tree kept alive.
fn narrower_read(
    command: &[&str],
    extra: &[&str],
    cell: Cell,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    let (tree, root) = cell.package(label);
    let mut arguments: Vec<&str> = command.to_vec();
    arguments.push("--json");
    arguments.extend_from_slice(extra);
    let run = Invocation::new(&arguments).rooted_at(&root);
    (tree, root, run)
}

/// Each of the six, in each of its four cells, against its recorded bytes.
///
/// Twenty-four comparisons in one test because they are one claim: these six contracts
/// are a set, and a change to one of them is as much a contract change as a change to all
/// of them. The failure names the contract and the cell.
#[test]
fn every_narrower_read_matches_its_golden_fixtures() {
    for (contract, command, extra) in NARROWER_READS {
        for cell in NARROWER_READ_CELLS {
            let label = format!("golden-{}-{}", command.join("-"), cell.name());
            let (_tree, root, run) = narrower_read(
                command,
                if matches!(cell, Cell::Warning) {
                    extra
                } else {
                    &[]
                },
                cell,
                &label,
            );
            let outcome = run.run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}",
                contract.name(),
                cell.name()
            );
            assert_matches_golden(contract, cell.name(), &outcome, &root);
        }
    }
}

/// A Damaged Order makes a value list partial, and the recorded contract says so.
///
/// Asserted against the fixtures rather than against two live runs, because this is a
/// claim about what the contract records: the same Package with one Order it cannot read
/// answers with a smaller count *and* a diagnostic. A later change that dropped the
/// diagnostic would leave a consumer unable to tell a Package with one Project from a
/// Package with two and one unreadable Order — and it would have to edit this file.
#[test]
fn a_partial_value_list_is_recorded_with_the_reason_it_is_partial() {
    let directory = fixture_directory(ResponseContract::PackageValuesProject);
    let complete: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(directory.join("success.json"))
            .expect("the successful fixture is present"),
    )
    .expect("the fixture parses");
    let partial: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(directory.join("warning.json"))
            .expect("the warning fixture is present"),
    )
    .expect("the fixture parses");

    assert_eq!(complete["data"]["values"][0]["value"], "Telescope");
    assert_eq!(partial["data"]["values"][0]["value"], "Telescope");
    assert_eq!(complete["data"]["values"][0]["usage_count"], 2);
    assert_eq!(
        partial["data"]["values"][0]["usage_count"], 1,
        "a Damaged Order contributes to no value list"
    );

    assert_eq!(complete["diagnostics"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        partial["diagnostics"][0]["scope"], "order",
        "and the answer carries the load's observation about the Order it could not read"
    );
    assert_eq!(
        partial["ok"], true,
        "which is a partial answer, not a failure"
    );
}

// ---------------------------------------------------------------------------
// The `order` read contracts.
// ---------------------------------------------------------------------------

/// The fixture cells each `order` read contract supplies.
///
/// **The same cells the Package reads supply**, because a read command's matrix is the
/// same matrix wherever its subject lives — and they carry them together rather than each
/// choosing, because they are one set: a cell one of them supplies and another does not is
/// a hole nobody finds until a consumer hits it.
///
/// `success` is a healthy Order with something in every member — a resolved Vendor, an
/// Order Currency, all four Reusable Free-Entry Values, and two Items one of which carries
/// a Catalog Number. `empty-result` is an Order with no Items and no README, where every
/// answers that there is nothing there rather than that there is no
/// answer. `warning` is a healthy Order whose README exists and is not a readable
/// document, which is the only cell in which an `order` read's `diagnostics` is non-empty.
/// `invalid` is a Damaged Order, where a *failed* response carrying this Order's own
/// diagnosis is pinned.
const ORDER_READS: [(ResponseContract, &[&str]); 2] = [
    (ResponseContract::OrderShow, &["order", "show"]),
    (ResponseContract::OrderReadme, &["order", "readme"]),
];

/// Every `order` read command line, which outnumbers the contracts above.
///
/// `order item list` shares `order show`'s contract and has no fixtures of its own, so it
/// is absent from [`ORDER_READS`] and present here: what is asserted per *contract* is
/// recorded bytes, and what is asserted per *command line* is how it answers being asked
/// wrongly. Its agreement with `order show` is pinned in
/// `tests/order_reads_and_targeting.rs`, which is what carries it to those bytes.
const ORDER_READ_COMMANDS: [&[&str]; 3] = [
    &["order", "show"],
    &["order", "readme"],
    &["order", "item", "list"],
];

/// The Order each cell is recorded against, with the tree kept alive.
///
/// Both roots come back because an `order` command is targeted at the Order Directory
/// while the fixture's substitution is of the Package Root: the response carries paths
/// under the Root, and no two runs of this suite have the same temporary one.
fn order_read_package(cell: Cell, label: &str) -> (TemporaryTree, PathBuf, PathBuf) {
    match cell {
        Cell::Success => {
            let (tree, root) = populated_package(label);
            let directory = order_directory(&root, "Laser_components");
            (tree, root, directory)
        }
        Cell::EmptyResult => package_with_an_empty_order(label),
        Cell::Warning => package_with_an_unreadable_order_readme(label),
        Cell::Invalid => package_with_an_impossible_date(label),
    }
}

/// Each read contract, in each of its cells, against its recorded bytes.
///
/// Every comparison in one test because they are one claim: these contracts are a set, and
/// a change to one of them is as much a contract change as a change to all of them. The
/// failure names the contract and the cell.
#[test]
fn every_order_read_matches_its_golden_fixtures() {
    for (contract, command) in ORDER_READS {
        for cell in NARROWER_READ_CELLS {
            let label = format!("golden-{}-{}", command.join("-"), cell.name());
            let (_tree, root, directory) = order_read_package(cell, &label);
            let mut arguments: Vec<&str> = command.to_vec();
            arguments.push("--json");
            let outcome = Invocation::new(&arguments).rooted_at(&directory).run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}: {}",
                contract.name(),
                cell.name(),
                outcome.stdout_text()
            );
            assert_matches_golden(contract, cell.name(), &outcome, &root);
        }
    }
}

/// Every way each `order` read reaches exit `2` carries `spielorders.usage`.
///
/// **A usage error is not any command's cell, and this is the behavioural half of that.**
/// No command may stamp a usage error with its own identity, so there is nothing here to
/// record in bytes — and there is still something to assert, once per way each of these
/// command lines can be asked wrongly. Every command line is covered, `order item list`
/// included, because being asked wrongly is a property of the line rather than of the
/// contract it would have answered with.
#[test]
fn every_usage_error_an_order_read_reaches_carries_the_shared_identity() {
    let (_tree, root) = populated_package("order-usage-errors");
    let directory = order_directory(&root, "Laser_components");

    for command in ORDER_READ_COMMANDS {
        for wrong in [
            // Two output modes at once.
            vec!["--json", "--quiet"],
            // An option this command does not have.
            vec!["--json", "--all"],
            // A misspelling of one it does.
            vec!["--json", "--uuidd", NEWEST_ORDER_ID],
            // The two targeting routes together, which is this family's own usage error.
            vec!["--json", "--uuid", NEWEST_ORDER_ID, "--root", "."],
        ] {
            let mut arguments: Vec<&str> = command.to_vec();
            arguments.extend_from_slice(&wrong);
            let outcome = Invocation::new(&arguments).in_directory(&directory).run();
            let document = response(&outcome);
            assert_eq!(
                outcome.status,
                Some(2),
                "{} {wrong:?}: {}",
                command.join(" "),
                outcome.stdout_text()
            );
            assert_eq!(
                document["contract"],
                "spielorders.usage",
                "{} {wrong:?}",
                command.join(" ")
            );
            assert_eq!(document["contract_version"], 1);
            assert_eq!(document["error"]["code"], "usage_invalid");
        }
    }
}

/// A positional argument is not how an `order` command is targeted.
///
/// Every `order` command takes its subject from an option, so a bare path or an Order ID
/// written positionally is a usage error rather than a second targeting route nobody
/// decided to add.
#[test]
fn an_order_read_takes_no_positional_subject() {
    let (_tree, root) = populated_package("order-positional-subject");
    let directory = order_directory(&root, "Laser_components");

    for command in ORDER_READ_COMMANDS {
        let mut arguments: Vec<&str> = command.to_vec();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments)
            .argument(&directory)
            .in_directory(&directory)
            .run();
        assert_eq!(outcome.status, Some(2), "{}", command.join(" "));
        assert_eq!(response(&outcome)["contract"], "spielorders.usage");
    }
}

// ---------------------------------------------------------------------------
// The `vendor` read contract.
// ---------------------------------------------------------------------------

/// Every `vendor` read command line, which outnumbers the contract they share.
///
/// `vendor contact list` shares `vendor show`'s contract and has no fixtures of its own, so
/// what is asserted per *contract* is recorded bytes and what is asserted per *command
/// line* is how it answers being asked wrongly. Their agreement is pinned in
/// `tests/vendor_reads.rs`, which is what carries the second line to these bytes.
const VENDOR_READ_COMMANDS: [&[&str]; 2] = [&["vendor", "show"], &["vendor", "contact", "list"]];

/// The Package and the Vendor Name each cell is recorded against.
///
/// **The same cells every other read supplies**, because a read command's matrix is the
/// same matrix wherever its subject lives. `success` is a Vendor with every optional shared
/// fact, a Contact, and an Order referencing it; `empty-result` is a Vendor with only a
/// Name, where both collections are present and empty; `warning` is the Hidden Vendor of a
/// Package carrying a Damaged Order, which is where the partial referencing list is
/// recorded beside the observation that explains it; `invalid` is a Vendor Name no Vendor
/// carries, which is this command's own failure — the Package-load states it shares are
/// recorded by the Package contracts.
///
/// `rejected` is a mutation's answer and these mutate nothing, and **`usage-error` is not
/// any command's cell** — a usage error carries `spielorders.usage`, whose one fixture is
/// shared by every command.
fn vendor_read_package(cell: Cell, label: &str) -> (TemporaryTree, PathBuf, &'static str) {
    match cell {
        Cell::Success => {
            let (tree, root) = populated_package(label);
            (tree, root, "Tarnwick")
        }
        Cell::EmptyResult => {
            let (tree, root) = package_with_an_unreferenced_vendor(label);
            (tree, root, "Nethermoor")
        }
        Cell::Warning => {
            let (tree, root, _damaged) = package_with_a_damaged_order(label);
            (tree, root, "Nethermoor")
        }
        Cell::Invalid => {
            let (tree, root) = populated_package(label);
            (tree, root, "Copperwick")
        }
    }
}

/// The read contract, in each of its cells, against its recorded bytes.
#[test]
fn every_vendor_read_matches_its_golden_fixtures() {
    for cell in NARROWER_READ_CELLS {
        let label = format!("golden-vendor-show-{}", cell.name());
        let (_tree, root, name) = vendor_read_package(cell, &label);
        let outcome = Invocation::new(&["vendor", "show"])
            .argument(name)
            .argument("--json")
            .rooted_at(&root)
            .run();
        assert_eq!(
            outcome.status,
            Some(cell.status()),
            "{} {}: {}",
            ResponseContract::VendorShow.name(),
            cell.name(),
            outcome.stdout_text()
        );
        assert_matches_golden(ResponseContract::VendorShow, cell.name(), &outcome, &root);
    }
}

/// Every way each `vendor` read reaches exit `2` carries `spielorders.usage`.
///
/// **A usage error is not any command's cell, and this is the behavioural half of that.**
/// Every command line is covered, `vendor contact list` included, because being asked
/// wrongly is a property of the line rather than of the contract it would have answered
/// with. A Vendor Name is a required positional here, so a command line that names none is
/// among the ways to be asked wrongly — and `--uuid` is an `order` option, which this
/// family does not have.
#[test]
fn every_usage_error_a_vendor_read_reaches_carries_the_shared_identity() {
    let (_tree, root) = populated_package("vendor-usage-errors");

    for command in VENDOR_READ_COMMANDS {
        let mut lines: Vec<Vec<&str>> = Vec::new();
        for wrong in [
            // Two output modes at once.
            vec!["--json", "--quiet"],
            // An option this command does not have.
            vec!["--json", "--all"],
            // An option another family has.
            vec!["--json", "--uuid", NEWEST_ORDER_ID],
            // A misspelling of one this command does.
            vec!["--json", "--rooot", "."],
        ] {
            let mut arguments: Vec<&str> = command.to_vec();
            arguments.push("Tarnwick");
            arguments.extend_from_slice(&wrong);
            lines.push(arguments);
        }
        // No Vendor Name at all: the selector is required, and a command line that
        // supplies none has named no subject.
        let mut nameless: Vec<&str> = command.to_vec();
        nameless.push("--json");
        lines.push(nameless);
        // Two Vendor Names: one command reads one Vendor, so a second positional is not a
        // second subject and is not a second selector either.
        let mut two_names: Vec<&str> = command.to_vec();
        two_names.extend_from_slice(&["Tarnwick", "Nethermoor", "--json"]);
        lines.push(two_names);

        for arguments in lines {
            let outcome = Invocation::new(&arguments).rooted_at(&root).run();
            let document = response(&outcome);
            assert_eq!(
                outcome.status,
                Some(2),
                "{arguments:?}: {}",
                outcome.stdout_text()
            );
            assert_eq!(document["contract"], "spielorders.usage", "{arguments:?}");
            assert_eq!(document["contract_version"], 1, "{arguments:?}");
            assert_eq!(document["error"]["code"], "usage_invalid", "{arguments:?}");
        }
    }
}

/// A misspelled command inside the family is a usage error too, and names no command's
/// contract.
#[test]
fn a_misspelled_vendor_command_names_no_contract() {
    let (_tree, root) = populated_package("vendor-misspelled-command");

    for arguments in [
        vec!["vendor", "shwo", "Tarnwick", "--json"],
        vec!["vendor", "contact", "lsit", "Tarnwick", "--json"],
        vec!["vendor", "--json"],
        vec!["vendor", "contact", "--json"],
        vec!["vendro", "show", "Tarnwick", "--json"],
    ] {
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        let document = response(&outcome);
        assert_eq!(outcome.status, Some(2), "{arguments:?}");
        assert_eq!(document["contract"], "spielorders.usage", "{arguments:?}");
    }
}

// ---------------------------------------------------------------------------
// The `init` mutation contract.
// ---------------------------------------------------------------------------

/// The fixture cells `init` supplies.
///
/// `success` is a plain directory becoming a Package: every artifact created, the
/// conventional README among them, at this build's metadata version. `rejected` is a
/// refused mutation, which no read command has — a configured path that broke a rule,
/// recorded because it is the refusal carrying the most of this contract's shape: the
/// code that names the rule and the identity that names the key.
///
/// A dry run has no cell of its own and needs none: it is the `success` document with
/// `committed: false`, which `tests/initialization.rs` pins against the real run of the
/// same plan, and that carries it to these recorded bytes.
const INIT_CELLS: [&str; 2] = ["success", "rejected"];

/// A plain directory becoming a Package, which is the whole mutation report in bytes.
#[test]
fn the_successful_init_response_matches_its_golden_fixture() {
    let (_tree, root) = unmanaged_directory("golden-init-success");

    let outcome = Invocation::new(&["init", "--json"]).rooted_at(&root).run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_matches_golden(ResponseContract::Init, "success", &outcome, &root);
}

/// A refused mutation: `data` is `null`, the code names the rule broken, and the identity
/// names the configured key that broke it.
#[test]
fn the_rejected_init_response_matches_its_golden_fixture() {
    let (_tree, root) = unmanaged_directory("golden-init-rejected");

    let outcome = Invocation::new(&["init", "orders_path=.", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(1));
    assert_matches_golden(ResponseContract::Init, "rejected", &outcome, &root);
}

/// Every way `init` reaches exit `2` carries `spielorders.usage`.
///
/// **The assignments are the new route here.** A token that is not a `key=value`, a key
/// this command does not have, and a key assigned twice are all command lines naming
/// something `init` has no notion of — so nothing about the directory was attempted, and
/// the response is the shared usage one rather than a refused mutation. The directory is
/// left alone in every case, which is the half a status alone would not say.
#[test]
fn every_usage_error_init_reaches_carries_the_shared_identity() {
    for wrong in [
        // A token that is not an assignment at all.
        vec!["init", "--json", "Orders"],
        // A key this command does not have.
        vec!["init", "--json", "project=Telescope"],
        // A key it does have, assigned twice.
        vec!["init", "--json", "orders_path=A", "orders_path=B"],
        // An empty key, which names nothing.
        vec!["init", "--json", "=Orders"],
        // Two output modes at once.
        vec!["init", "--json", "--quiet"],
        // A misspelling of an option this command does have.
        vec!["init", "--json", "--dry-runn"],
        // An option another family has.
        vec!["init", "--json", "--all"],
        // A misspelling of the command itself.
        vec!["innit", "--json"],
    ] {
        let (_tree, root) = unmanaged_directory("init-usage-errors");
        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            entry_names(&root),
            Vec::<String>::new(),
            "{wrong:?}: nothing about the directory was attempted"
        );
    }
}

// ---------------------------------------------------------------------------
// The four Vendor mutation contracts, and the cells every mutation of a Vendor
// record records.
// ---------------------------------------------------------------------------

/// The fixture cells each Vendor mutation supplies.
///
/// `success` is a committed mutation, the whole report and the refreshed Vendor in bytes.
/// `warning` is a committed mutation whose derived index refresh then failed, which is
/// where committed-with-warnings is pinned — exit `0`, `committed: true`, the warning
/// named, and the index reported stale. `invalid` is a Package the load could read and not
/// trust, where a *failed* response carrying that Package's diagnosis is pinned.
/// `rejected` is a refused mutation, which no read command has.
///
/// **A cell names the kind of response, not the condition.** So a Vendor Name matching
/// nothing is `rejected` for `vendor hide` and `invalid` for `vendor show`: a read has no
/// mutation to refuse, and for a mutation a subject that does not resolve is a refusal
/// that wrote nothing.
const MUTATION_CELLS: [MutationCell; 4] = [
    MutationCell::Success,
    MutationCell::Warning,
    MutationCell::Invalid,
    MutationCell::Rejected,
];

#[derive(Clone, Copy, Debug)]
enum MutationCell {
    Success,
    Warning,
    Invalid,
    Rejected,
}

impl MutationCell {
    fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Invalid => "invalid",
            Self::Rejected => "rejected",
        }
    }

    /// The exit status this cell records. A committed mutation carrying a warning is a
    /// success: the canonical write happened, and only the derived refresh did not.
    fn status(self) -> i32 {
        match self {
            Self::Success | Self::Warning => 0,
            Self::Invalid | Self::Rejected => 1,
        }
    }

    /// Asserts the response is the *kind* this cell names.
    ///
    /// **A status cannot say which cell a response is, and the recorded bytes cannot
    /// either.** Two of the four exit `0` and two exit `1`, so a command line that reached
    /// a neighbouring kind records its own document happily under the wrong name — a
    /// mutation that changed nothing filed as `warning` is the case that occurred, and the
    /// golden comparison then pins a cell that no regression of committed-with-warnings can
    /// make fail. The four distinctions are the mutation report's own: a success is `ok`
    /// with no warning, committed-with-warnings is `ok` with one, an untrusted subject is
    /// `ok: false` carrying that subject's diagnosis, and a refused mutation is `ok: false`
    /// with no `data` at all.
    fn assert_is_this_kind(self, document: &serde_json::Value, label: &str) {
        let warnings = || {
            document["data"]["report"]["warnings"]
                .as_array()
                .unwrap_or_else(|| panic!("{label}: a committed mutation reports its warnings"))
        };
        match self {
            Self::Success => {
                assert_eq!(document["ok"], true, "{label}");
                assert!(
                    warnings().is_empty(),
                    "{label}: a success carries no warning"
                );
            }
            Self::Warning => {
                assert_eq!(document["ok"], true, "{label}");
                assert!(
                    !warnings().is_empty(),
                    "{label}: a committed-with-warnings mutation names the warning, and a \
                     mutation that changed nothing is not this cell"
                );
            }
            Self::Invalid => {
                assert_eq!(document["ok"], false, "{label}");
                assert!(
                    !document["diagnostics"]
                        .as_array()
                        .expect("every response carries its diagnostics")
                        .is_empty(),
                    "{label}: the diagnosis of the subject the load could not trust is what \
                     this cell pins"
                );
            }
            Self::Rejected => {
                assert_eq!(document["ok"], false, "{label}");
                assert!(document["data"].is_null(), "{label}");
            }
        }
    }
}

/// The eight mutations, each with the Package and command line every cell is recorded on.
///
/// The `success` cells carry as much of each contract's shape as the command has: a
/// creation assigns every optional shared fact, an edit both renames and clears one, and
/// the two visibility mutations are recorded on the referenced Vendors of a populated
/// Package — so a Hidden Vendor's referencing Orders are in the recorded bytes rather than
/// only in a test.
///
/// A Contact mutation is recorded against a Vendor with three Contacts, because a position
/// is only worth recording where three positions are three answers. See
/// [`crate::support::package_with_three_contacts`].
fn mutation_run(
    contract: ResponseContract,
    cell: MutationCell,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    let (tree, root) = match cell {
        MutationCell::Invalid => package_with_duplicate_vendor_names(label),
        MutationCell::Warning => match contract {
            ResponseContract::AddVendor => initialized_package_with_a_blocked_index_refresh(label),
            contract if CONTACT_MUTATIONS.contains(&contract) => {
                package_with_three_contacts_and_a_blocked_index_refresh(label)
            }
            _ => package_with_a_blocked_index_refresh(label),
        },
        MutationCell::Success | MutationCell::Rejected => match contract {
            ResponseContract::AddVendor if matches!(cell, MutationCell::Success) => {
                initialized_package(label)
            }
            contract if CONTACT_MUTATIONS.contains(&contract) => package_with_three_contacts(label),
            _ => populated_package(label),
        },
    };
    let mut run = Invocation::new(&mutation_command(contract, cell));
    run = run.argument("--json").rooted_at(&root);
    (tree, root, run)
}

fn mutation_command(contract: ResponseContract, cell: MutationCell) -> Vec<&'static str> {
    match (contract, cell) {
        (ResponseContract::AddVendor, MutationCell::Success) => vec![
            "add-vendor",
            "name=Tarnwick",
            "phone=+1 555 0123",
            "email=sales@example.com",
            "website=https://example.com",
            "address=100 Example Avenue\nAnytown, ST 00000",
            "purchasing_notes=Quote required above $5000.",
        ],
        (ResponseContract::AddVendor, MutationCell::Warning) => vec!["add-vendor", "Tarnwick"],
        (ResponseContract::AddVendor, MutationCell::Invalid) => vec!["add-vendor", "Copperwick"],
        // Two rules broken by one assignment set, because a set is adjudicated as a whole:
        // this is where a refusal naming several fields for several reasons is recorded.
        (ResponseContract::AddVendor, MutationCell::Rejected) => {
            vec!["add-vendor", "colour=red", "contacts=none"]
        }
        (ResponseContract::VendorEdit, MutationCell::Success) => {
            vec!["vendor", "edit", "Tarnwick", "name=Tarnwick Inc", "phone="]
        }
        (ResponseContract::VendorEdit, MutationCell::Warning) => {
            vec!["vendor", "edit", "Tarnwick", "website=https://example.org"]
        }
        (ResponseContract::VendorEdit, MutationCell::Invalid) => {
            vec!["vendor", "edit", "Tarnwick", "phone=none"]
        }
        // A rename onto a Name another Vendor carries: the refusal that records both the
        // code and the existing Vendor's identity, and that there is no merge.
        (ResponseContract::VendorEdit, MutationCell::Rejected) => {
            vec!["vendor", "edit", "Nethermoor", "name=tarnwick"]
        }
        (ResponseContract::VendorHide, MutationCell::Rejected) => {
            vec!["vendor", "hide", "Copperwick"]
        }
        (ResponseContract::VendorHide, _) => vec!["vendor", "hide", "Tarnwick"],
        (ResponseContract::VendorUnhide, MutationCell::Rejected) => {
            vec!["vendor", "unhide", "Copperwick"]
        }
        (ResponseContract::VendorUnhide, MutationCell::Invalid) => {
            vec!["vendor", "unhide", "Tarnwick"]
        }
        // The Hidden Vendor of a populated Package, which is the one there is anything to
        // unhide about.
        (ResponseContract::VendorUnhide, _) => vec!["vendor", "unhide", "Nethermoor"],
        // Every optional Contact fact and an insertion point, so the fullest shape this
        // command has is what is recorded — and the insertion is into the middle of a
        // collection of three, where the Contacts it shifts are in the recorded bytes.
        (ResponseContract::VendorContactAdd, MutationCell::Success) => vec![
            "vendor",
            "contact",
            "add",
            "Tarnwick",
            "name=Ada Lovelace",
            "role=Purchasing",
            "phone=+1 555 0188",
            "email=ada@example.com",
            "position=2",
        ],
        // The one refusal only `add` reaches: a command line that assigned the Contact no
        // Name at all. The Vendor's Name resolved, which is what makes this the Contact's
        // own missing-Name refusal rather than a Vendor's.
        (ResponseContract::VendorContactAdd, MutationCell::Rejected) => {
            vec!["vendor", "contact", "add", "Tarnwick", "role=Purchasing"]
        }
        (ResponseContract::VendorContactAdd, _) => {
            vec!["vendor", "contact", "add", "Tarnwick", "name=Ada Lovelace"]
        }
        // A rename and a clearing in one atomic set, as `vendor edit`'s own success is.
        (ResponseContract::VendorContactEdit, MutationCell::Success) => vec![
            "vendor",
            "contact",
            "edit",
            "Tarnwick",
            "2",
            "name=Grace B. Hopper",
            "phone=",
        ],
        // A source position past the end, where the sentence names the range this command
        // can address.
        (ResponseContract::VendorContactEdit, MutationCell::Rejected) => {
            vec![
                "vendor",
                "contact",
                "edit",
                "Tarnwick",
                "9",
                "role=Engineering",
            ]
        }
        // A role the Contact standing there does not already carry, so the cell that is
        // about a committed write records one.
        (ResponseContract::VendorContactEdit, _) => {
            vec![
                "vendor",
                "contact",
                "edit",
                "Tarnwick",
                "2",
                "role=Purchasing",
            ]
        }
        (ResponseContract::VendorContactMove, MutationCell::Success) => {
            vec!["vendor", "contact", "move", "Tarnwick", "3", "position=1"]
        }
        // The destination end refused, which is the role a `move` alone has two of.
        (ResponseContract::VendorContactMove, MutationCell::Rejected) => {
            vec!["vendor", "contact", "move", "Tarnwick", "1", "position=0"]
        }
        (ResponseContract::VendorContactMove, _) => {
            vec!["vendor", "contact", "move", "Tarnwick", "2", "position=1"]
        }
        // A position against the Hidden Vendor, which has no Contacts — the other branch
        // of the sentence a refused position carries, where there is no range to name.
        (ResponseContract::VendorContactRemove, MutationCell::Rejected) => {
            vec!["vendor", "contact", "remove", "Nethermoor", "1"]
        }
        (ResponseContract::VendorContactRemove, _) => {
            vec!["vendor", "contact", "remove", "Tarnwick", "2"]
        }
        (contract, cell) => panic!("{} has no {} run", contract.name(), cell.name()),
    }
}

const VENDOR_MUTATIONS: [ResponseContract; 4] = [
    ResponseContract::AddVendor,
    ResponseContract::VendorEdit,
    ResponseContract::VendorHide,
    ResponseContract::VendorUnhide,
];

/// Each mutation of a Vendor record, in each of its cells, against its recorded bytes.
///
/// **The Vendor mutations and the Contact mutations in one comparison, because they are one
/// claim.** All eight replace the same document through the same [`mutation_run`], each
/// records the same [`MUTATION_CELLS`], and a change to one of them is as much a contract
/// change as a change to all of them; two loops differing only in whether they looked for a
/// minted identity were two chances for one of the eight to be compared by a weaker rule
/// than its neighbour. The failure names the contract and the cell.
///
/// A creation's Vendor ID is minted in the child process, so the two cells that carry one
/// substitute `<VENDOR_ID>` after asserting its shape — a Contact mutation mints none,
/// because it retains its owning Vendor's, which is what [`minted_identity`] answers rather
/// than this loop assuming. Every other byte is compared exactly. See
/// `docs/development/traceability.md`.
#[test]
fn every_vendor_and_contact_mutation_matches_its_golden_fixtures() {
    for contract in VENDOR_MUTATIONS.into_iter().chain(CONTACT_MUTATIONS) {
        for cell in MUTATION_CELLS {
            let label = format!("golden-{}-{}", contract.name(), cell.name());
            let (_tree, root, run) = mutation_run(contract, cell, &label);
            let outcome = run.run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}: {}{}",
                contract.name(),
                cell.name(),
                outcome.stdout_text(),
                outcome.stderr_text()
            );
            cell.assert_is_this_kind(&response(&outcome), &label);
            match minted_identity(&outcome) {
                Some(minted) => assert_matches_golden_with_minted_vendor_id(
                    contract,
                    cell.name(),
                    &outcome,
                    &root,
                    &minted,
                ),
                None => assert_matches_golden(contract, cell.name(), &outcome, &root),
            }
        }
    }
}

/// The Vendor ID a run minted, where the run was a committed creation.
///
/// Read from the response rather than assumed per cell, because which cells mint one is a
/// fact about the commands rather than about this table — and a cell that started minting
/// one would otherwise fail with an unexplained diff.
fn minted_identity(outcome: &crate::support::Outcome) -> Option<String> {
    let document = response(outcome);
    if document["contract"] != ResponseContract::AddVendor.name() {
        return None;
    }
    document["data"]["vendor"]["vendor_id"]
        .as_str()
        .map(str::to_owned)
}

/// Every way each Vendor mutation reaches exit `2` carries `spielorders.usage`.
///
/// **A usage error is not any command's cell, and this is the behavioural half of that.**
/// The assignment language is the new route here: a token that is not `field=value`, and
/// for `add-vendor` a Name written beside one or a second Name, are command lines naming
/// something no command has — so nothing about the Package was attempted, and the response
/// is the shared usage one rather than a refused mutation. The Package is untouched in
/// every case, which is the half a status alone would not say.
#[test]
fn every_usage_error_a_vendor_mutation_reaches_carries_the_shared_identity() {
    let (_tree, root) = populated_package("vendor-mutation-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        // Two output modes at once.
        vec!["add-vendor", "Tarnwick", "--json", "--quiet"],
        // An option another family has.
        vec!["add-vendor", "Tarnwick", "--json", "--all"],
        // A misspelling of one this command does have.
        vec!["add-vendor", "Tarnwick", "--json", "--dry-runn"],
        // The two `add-vendor` forms at once, and two Names.
        vec!["add-vendor", "Tarnwick", "name=Tarnwick", "--json"],
        vec!["add-vendor", "Tarnwick", "Nethermoor", "--json"],
        // A token that is not an assignment, where there is no shorthand to read it as.
        vec!["vendor", "edit", "Tarnwick", "phone", "--json"],
        // No Vendor Name at all, where one is the required selector.
        vec!["vendor", "edit", "--json"],
        vec!["vendor", "hide", "--json"],
        vec!["vendor", "unhide", "--json"],
        // An assignment where the command takes none.
        vec!["vendor", "hide", "Tarnwick", "phone=none", "--json"],
        // A misspelling of the command itself.
        vec!["add-vendro", "Tarnwick", "--json"],
        vec!["vendor", "edti", "Tarnwick", "--json"],
    ] {
        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            crate::support::managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Package was attempted"
        );
    }
}

// ---------------------------------------------------------------------------
// The four Contact mutation contracts.
// ---------------------------------------------------------------------------

/// The four Contact mutations, whose cells are [`MUTATION_CELLS`] for the reasons given
/// there and whose recorded bytes are compared beside the Vendor mutations' in
/// [`every_vendor_and_contact_mutation_matches_its_golden_fixtures`].
///
/// **The same four cells, because these are mutations of the same document.** Each replaces
/// the complete owning Vendor record, so `success` is a committed rewrite, `warning` is one
/// whose derived index refresh then failed, `invalid` is a Package the load could read and
/// not trust, and `rejected` is a refused mutation — and `empty-result` is unreachable for
/// the reason it is unreachable for a Vendor mutation: an answer that there is nothing there
/// is not one a mutation can give, and each of these reports the Vendor whose collection it
/// changed.
///
/// **What is these four's own is the position**, so each `rejected` cell records a different
/// way of naming none: the Contact assigned no Name at all is `add`'s, a source past the end
/// is `edit`'s, a destination of zero is `move`'s, and a Vendor with no Contacts to address
/// is `remove`'s. One code covers all of them and the identity says which end it was for, so
/// what the four cells record between them is the whole refusal.
///
/// A dry run has no cell of its own, as no mutation's does:
/// `tests/vendor_contact_mutations.rs` pins each of the four plans against the commit of the
/// same command line, and that carries them to these recorded bytes.
const CONTACT_MUTATIONS: [ResponseContract; 4] = [
    ResponseContract::VendorContactAdd,
    ResponseContract::VendorContactEdit,
    ResponseContract::VendorContactMove,
    ResponseContract::VendorContactRemove,
];

/// Every way each Contact mutation reaches exit `2` carries `spielorders.usage`.
///
/// **The new routes are the `POSITION` positional and the `position=` key.** A command line
/// missing the position its shape has, a `move` with no destination or a destination spelled
/// as something else, and an insertion point assigned twice all name something no command
/// has — so nothing about the collection was attempted, and the response is the shared usage
/// one rather than a refused mutation. Which numbers name a Contact is the collection's own
/// business and reaches exit `1` instead, which
/// `tests/vendor_contact_mutations.rs` is what pins.
#[test]
fn every_usage_error_a_contact_mutation_reaches_carries_the_shared_identity() {
    let (_tree, root) = package_with_three_contacts("contact-mutation-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        // No Vendor Name at all, where one is the required selector.
        vec!["vendor", "contact", "add", "--json"],
        // The `POSITION` its shape has, omitted.
        vec!["vendor", "contact", "edit", "Tarnwick", "--json"],
        vec!["vendor", "contact", "remove", "Tarnwick", "--json"],
        // A `move` with no destination, one spelled as another key, and one written as a
        // bare number.
        vec!["vendor", "contact", "move", "Tarnwick", "1", "--json"],
        vec![
            "vendor",
            "contact",
            "move",
            "Tarnwick",
            "1",
            "destination=2",
            "--json",
        ],
        vec!["vendor", "contact", "move", "Tarnwick", "1", "2", "--json"],
        // An insertion point assigned twice.
        vec![
            "vendor",
            "contact",
            "add",
            "Tarnwick",
            "name=Ada Lovelace",
            "position=1",
            "position=2",
            "--json",
        ],
        // A token that is not an assignment, and an assignment where there are none.
        vec![
            "vendor", "contact", "edit", "Tarnwick", "1", "phone", "--json",
        ],
        vec![
            "vendor",
            "contact",
            "remove",
            "Tarnwick",
            "1",
            "phone=none",
            "--json",
        ],
        // Two output modes at once, an option another family has, and a misspelling of one
        // these commands do have.
        vec![
            "vendor", "contact", "edit", "Tarnwick", "1", "--json", "--quiet",
        ],
        vec![
            "vendor", "contact", "remove", "Tarnwick", "1", "--json", "--all",
        ],
        vec![
            "vendor",
            "contact",
            "move",
            "Tarnwick",
            "1",
            "position=2",
            "--json",
            "--dry-runn",
        ],
    ] {
        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            crate::support::managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the collection was attempted"
        );
    }
}

// ---------------------------------------------------------------------------
// The two contracts that give an Order Directory its first record.
// ---------------------------------------------------------------------------

/// `add-order` and `adopt-order`, whose cells are [`MUTATION_CELLS`] for the reasons given
/// there.
///
/// **The same four cells, because these are mutations.** `success` is a committed one,
/// `warning` is one whose derived index refresh then failed, `invalid` is a Package the
/// load could read and not trust, and `rejected` is a refused mutation.
///
/// **`empty-result` is unreachable** for the reason it is unreachable for every other
/// mutation: an answer that there is nothing there is not one a mutation can give. Each of
/// these reports the Order it created or adopted, so there is always something to report,
/// and a refusal to produce one is `rejected`.
///
/// **What is these two's own is the identity and the clock.** `add-order`'s cells that
/// commit carry an Order ID minted in the child process and, where the command line
/// assigned no Date Ordered, today's local date — which reaches the response twice, once as
/// the Order's fact and once inside the projected Order Directory basename. Both are
/// substituted; see [`crate::support::normalized_with_substitutions`].
///
/// A dry run has no cell of its own, as no mutation's does:
/// `tests/order_creation_and_adoption.rs` pins each plan against the commit of the same
/// command line, and that carries it to these recorded bytes.
const NEW_ORDER_MUTATIONS: [ResponseContract; 2] =
    [ResponseContract::AddOrder, ResponseContract::AdoptOrder];

/// Each of the two contracts, in each of its cells, against its recorded bytes.
#[test]
fn every_new_order_mutation_matches_its_golden_fixtures() {
    for contract in NEW_ORDER_MUTATIONS {
        for cell in MUTATION_CELLS {
            let label = format!("golden-{}-{}", contract.name(), cell.name());
            let (_tree, root, run) = new_order_run(contract, cell, &label);
            let outcome = run.run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}: {}{}",
                contract.name(),
                cell.name(),
                outcome.stdout_text(),
                outcome.stderr_text()
            );
            cell.assert_is_this_kind(&response(&outcome), &label);
            let minted = minted_order_identity(&outcome);
            assert_matches_golden_with_substitutions(
                contract,
                cell.name(),
                &outcome,
                &root,
                minted.as_deref(),
            );
        }
    }
}

/// The Order ID a run minted, where the run committed one.
///
/// Read from the response rather than assumed per cell, for the reason
/// [`minted_identity`] is: which cells mint one is a fact about the commands rather than
/// about this table.
fn minted_order_identity(outcome: &crate::support::Outcome) -> Option<String> {
    response(outcome)["data"]["order"]["order_id"]
        .as_str()
        .map(str::to_owned)
}

/// The two commands, each with the Package and command line every cell is recorded on.
///
/// The `success` cells carry as much of each contract's shape as the command has.
/// `add-order` assigns every optional Order fact except the Date Ordered, so the recorded
/// bytes are the ordinary command line *and* the stamped default; `adopt-order` supplies
/// the three facts it requires over a candidate holding a README and Supporting Material,
/// so its recorded bytes are an adoption of a real legacy directory.
fn new_order_run(
    contract: ResponseContract,
    cell: MutationCell,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    let (tree, root, candidate) = match cell {
        MutationCell::Invalid => {
            let (tree, root) = package_with_duplicate_vendor_names(label);
            // A candidate this Package would hold, named without being created: the load
            // refuses before anything observes the path, and a command line that named no
            // candidate at all would bind an assignment to the positional instead.
            let candidate = (contract == ResponseContract::AdoptOrder)
                .then(|| root.join("Orders/2011_02_09_Nethermoor_Posts"));
            (tree, root, candidate)
        }
        MutationCell::Warning => {
            let (tree, root) = crate::support::package_with_a_blocked_index_refresh(label);
            let candidate = adoption_candidate(contract, &root);
            (tree, root, candidate)
        }
        MutationCell::Success if contract == ResponseContract::AdoptOrder => {
            let (tree, root, candidate) = crate::support::package_with_an_adoption_candidate(label);
            (tree, root, Some(candidate))
        }
        // Adoption's own refusal, recorded over one of this Package's existing Orders: the
        // safety property that matters most is that a directory already carrying a record
        // is refused rather than overwritten, and it is the refusal creation has no
        // counterpart for.
        MutationCell::Rejected if contract == ResponseContract::AdoptOrder => {
            let (tree, root) = populated_package(label);
            let existing = order_directory(&root, "Laser_components");
            (tree, root, Some(existing))
        }
        MutationCell::Success | MutationCell::Rejected => {
            let (tree, root) = populated_package(label);
            let candidate = adoption_candidate(contract, &root);
            (tree, root, candidate)
        }
    };
    let mut run = Invocation::new(&[match contract {
        ResponseContract::AddOrder => "add-order",
        _ => "adopt-order",
    }]);
    if let Some(candidate) = candidate {
        run = run.argument(candidate);
    }
    for assignment in new_order_assignments(contract, cell) {
        run = run.argument(assignment);
    }
    run = run.argument("--json").rooted_at(&root);
    (tree, root, run)
}

/// One plain candidate directory, for the adoption cells that need an eligible one and do
/// not need it to hold anything.
fn adoption_candidate(contract: ResponseContract, root: &Path) -> Option<PathBuf> {
    (contract == ResponseContract::AdoptOrder)
        .then(|| crate::support::unmanaged_candidate(root, "2011_02_09_Nethermoor_Posts"))
}

fn new_order_assignments(contract: ResponseContract, cell: MutationCell) -> Vec<&'static str> {
    match (contract, cell) {
        // Every optional Order fact, and no `date_ordered`: the ordinary command line, and
        // the one whose recorded bytes carry the stamped default.
        (ResponseContract::AddOrder, MutationCell::Success) => vec![
            "title=Laser components",
            "status=ordered",
            "vendor=Tarnwick",
            "orderer=Ada Lovelace",
            "ordering_institution=northwind",
            "currency=USD",
            "project=Telescope",
            "order_group=Autumn build",
        ],
        // Two rules broken by one assignment set, because a set is adjudicated as a whole:
        // this is where a refusal naming several fields for several reasons is recorded.
        (ResponseContract::AddOrder, MutationCell::Rejected) => {
            vec!["title=Laser components", "colour=red", "items=none"]
        }
        (ResponseContract::AddOrder, _) => vec!["title=Laser components"],
        (_, MutationCell::Success) => vec![
            "title=Fiber patch cable",
            "date_ordered=2009-04-30",
            "status=complete",
            "vendor=Tarnwick",
            "orderer=Ada Lovelace",
            "ordering_institution=Northwind",
            "project=Telescope",
        ],
        (_, _) => vec![
            "title=Fiber patch cable",
            "date_ordered=2009-04-30",
            "status=complete",
        ],
    }
}

// ---------------------------------------------------------------------------
// The two contracts that move a whole Order Directory.
// ---------------------------------------------------------------------------

/// `order align` and `order remove`, whose cells are [`MUTATION_CELLS`] for the reasons
/// given there.
///
/// **The same four cells, because these are mutations.** `success` is a committed one,
/// `warning` is one whose derived index refresh then failed, `invalid` is a Damaged Order
/// — where a *failed* response carrying that Order's own diagnosis is pinned — and
/// `rejected` is a refused mutation: for both of these, a destination name that is
/// already taken.
///
/// `warning` is recorded over a *drifted* Order for `order align` on purpose: an Order
/// already carrying its aligned name is committed by performing nothing, so it refreshes
/// nothing, reaches no warning, and is not this cell.
///
/// **`order remove --delete` records no cell of its own**, and cannot: the platform owns
/// where a trashed directory goes, so the location it reports is particular to the machine
/// and the moment. The closed placeholder set has no stand-in for one, and inventing one
/// would need a ruling. The disposition is pinned behaviourally instead, against the real
/// platform effect — see `tests/order_alignment_and_removal.rs`.
///
/// A dry run has no cell of its own either, as no mutation's does: that same file pins
/// each plan against the commit of the same command line, and that carries them to these
/// recorded bytes.
const RELOCATION_MUTATIONS: [(ResponseContract, &[&str]); 2] = [
    (ResponseContract::OrderAlign, &["order", "align"]),
    (ResponseContract::OrderRemove, &["order", "remove"]),
];

/// Each of the two contracts, in each of its cells, against its recorded bytes.
///
/// Every comparison in one test because they are one claim: the two are a set, and a
/// change to one of them is as much a contract change as a change to both. Every byte but
/// the Package Root is compared exactly — neither command mints an identity or reads the
/// clock, because the Order they move already has both.
#[test]
fn every_relocation_matches_its_golden_fixtures() {
    for (contract, command) in RELOCATION_MUTATIONS {
        for cell in MUTATION_CELLS {
            let label = format!("golden-{}-{}", command.join("-"), cell.name());
            let (_tree, root, directory) = relocation_package(contract, cell, &label);
            let mut arguments: Vec<&str> = command.to_vec();
            arguments.push("--json");
            let outcome = Invocation::new(&arguments).rooted_at(&directory).run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}: {}{}",
                contract.name(),
                cell.name(),
                outcome.stdout_text(),
                outcome.stderr_text()
            );
            cell.assert_is_this_kind(&response(&outcome), &label);
            assert_matches_golden(contract, cell.name(), &outcome, &root);
        }
    }
}

/// The Package and the Order Directory each relocation cell is recorded against.
fn relocation_package(
    contract: ResponseContract,
    cell: MutationCell,
    label: &str,
) -> (TemporaryTree, PathBuf, PathBuf) {
    match (contract, cell) {
        (_, MutationCell::Invalid) => package_with_an_impossible_date(label),
        (ResponseContract::OrderAlign, MutationCell::Success) => {
            package_with_a_drifted_order(label)
        }
        (ResponseContract::OrderAlign, MutationCell::Warning) => {
            package_with_a_drifted_order_and_a_blocked_index_refresh(label)
        }
        (ResponseContract::OrderAlign, _) => {
            let (tree, root, directory, _occupied) = package_with_an_occupied_aligned_name(label);
            (tree, root, directory)
        }
        (_, MutationCell::Rejected) => {
            let (tree, root, directory, _occupied) = package_with_an_occupied_removed_name(label);
            (tree, root, directory)
        }
        (_, MutationCell::Warning) => {
            let (tree, root) = package_with_a_blocked_index_refresh(label);
            let directory = order_directory(&root, "Laser_components");
            (tree, root, directory)
        }
        _ => {
            let (tree, root) = populated_package(label);
            let directory = order_directory(&root, "Laser_components");
            (tree, root, directory)
        }
    }
}

/// Every way either relocation reaches exit `2` carries `spielorders.usage`.
///
/// **A usage error is not any command's cell, and this is the behavioural half of that.**
/// The Package is untouched in every case, which is the half a status alone would not
/// say: a command line naming something no command has means nothing about the Order was
/// attempted — and for these two commands that is the whole of the safety property, since
/// the thing not attempted is a directory being moved out of the Package.
#[test]
fn every_usage_error_a_relocation_reaches_carries_the_shared_identity() {
    let (_tree, root, directory) = package_with_a_drifted_order("relocation-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        // Two output modes at once.
        vec!["order", "align", "--json", "--quiet"],
        vec!["order", "remove", "--json", "--quiet"],
        // An option another family has.
        vec!["order", "align", "--json", "--all"],
        vec!["order", "remove", "--json", "--all"],
        // A misspelling of one these commands do have.
        vec!["order", "align", "--json", "--dry-runn"],
        vec!["order", "remove", "--json", "--delet"],
        // An option one of them has and the other does not.
        vec!["order", "align", "--json", "--delete"],
        // The two targeting routes together, which is this family's own usage error.
        vec![
            "order",
            "align",
            "--json",
            "--uuid",
            NEWEST_ORDER_ID,
            "--root",
            ".",
        ],
        vec![
            "order",
            "remove",
            "--json",
            "--uuid",
            NEWEST_ORDER_ID,
            "--root",
            ".",
        ],
        // An assignment where these commands take none, and a positional subject where
        // every `order` command is targeted by option.
        vec!["order", "align", "--json", "title=Laser heads"],
        vec!["order", "remove", "--json", "."],
        // A misspelling of the command itself.
        vec!["order", "algin", "--json"],
        vec!["order", "remvoe", "--json"],
    ] {
        let outcome = Invocation::new(&wrong).in_directory(&directory).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            crate::support::managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Order was attempted"
        );
    }
}

// ---------------------------------------------------------------------------
// The five contracts that mutate one Order's own record.
// ---------------------------------------------------------------------------

/// `order edit` and the four `order item` mutations, whose cells are [`MUTATION_CELLS`]
/// for the reasons given there.
///
/// **The same four cells, because these are mutations of one document.** Each replaces the
/// complete Order record, so `success` is a committed rewrite, `warning` is one whose
/// derived index refresh then failed, `invalid` is a Damaged Order — where a *failed*
/// response carrying that Order's own diagnosis is pinned — and `rejected` is a refused
/// mutation.
///
/// **What `order edit`'s `success` cell is its own is the clock.** It enters `ordered`
/// with no date, so the recorded bytes carry the date the automatic rule applied and the
/// source that says the rule applied it; `<TODAY>` is what stands in for the value — see
/// `docs/development/traceability.md`.
///
/// **What the four Item cells are their own is the position**, so each `rejected` cell
/// records a different way of naming none: an Item assigned none of its required facts is
/// `add`'s, a source past the end is `edit`'s, a destination of zero is `move`'s, and an
/// Order with no Items to address is `remove`'s. One code covers the positions and the
/// identity says which end it was for, so what the cells record between them is the whole
/// refusal.
///
/// A dry run has no cell of its own, as no mutation's does:
/// `tests/order_and_item_edits.rs` pins each of the five plans against the commit of the
/// same command line, and that carries them to these recorded bytes.
const RECORD_MUTATIONS: [ResponseContract; 5] = [
    ResponseContract::OrderEdit,
    ResponseContract::OrderItemAdd,
    ResponseContract::OrderItemEdit,
    ResponseContract::OrderItemMove,
    ResponseContract::OrderItemRemove,
];

/// Each of the five contracts, in each of its cells, against its recorded bytes.
///
/// Every comparison in one test because they are one claim: the five are a set, and a
/// change to one of them is as much a contract change as a change to all of them. Every
/// byte but the Package Root — and, where the automatic Date Ordered rule applied one,
/// today's local date — is compared exactly. None of these commands mints an identity: the
/// Order they change already has one.
#[test]
fn every_record_mutation_matches_its_golden_fixtures() {
    for contract in RECORD_MUTATIONS {
        for cell in MUTATION_CELLS {
            let label = format!("golden-{}-{}", contract.name(), cell.name());
            let (_tree, root, run) = record_mutation_run(contract, cell, &label);
            let outcome = run.run();
            assert_eq!(
                outcome.status,
                Some(cell.status()),
                "{} {}: {}{}",
                contract.name(),
                cell.name(),
                outcome.stdout_text(),
                outcome.stderr_text()
            );
            cell.assert_is_this_kind(&response(&outcome), &label);
            assert_matches_golden_with_substitutions(contract, cell.name(), &outcome, &root, None);
        }
    }
}

/// The five commands, each with the Order and command line every cell is recorded on.
///
/// The `success` cells carry as much of each contract's shape as the command has: the edit
/// reuses one Reusable Free-Entry Value, introduces another, clears a third, and enters
/// `ordered` without a date; the `add` assigns every optional Item fact and an insertion
/// point into the middle of a collection of three, so the Items it shifts are in the
/// recorded bytes.
///
/// An Item mutation is recorded against an Order with three Items, because a position is
/// only worth recording where three positions are three answers. See
/// [`crate::support::package_with_three_items`].
fn record_mutation_run(
    contract: ResponseContract,
    cell: MutationCell,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    let (tree, root, directory) = match (contract, cell) {
        (_, MutationCell::Invalid) => package_with_an_impossible_date(label),
        (_, MutationCell::Warning) => package_with_three_items_and_a_blocked_index_refresh(label),
        // The other branch of a refused position's sentence: an Order with no Items has no
        // range to name, and only a `remove` can be asked about one — an `add` may always
        // address the first position, and this Package's other Order is what the two
        // remaining commands are recorded against.
        (ResponseContract::OrderItemRemove, MutationCell::Rejected) => {
            package_with_an_empty_order(label)
        }
        _ => package_with_three_items(label),
    };
    let mut run = Invocation::new(&record_mutation_command(contract, cell));
    run = run.argument("--json").rooted_at(&directory);
    (tree, root, run)
}

fn record_mutation_command(contract: ResponseContract, cell: MutationCell) -> Vec<&'static str> {
    match (contract, cell) {
        // A reuse under a different spelling, an introduction, a clearing, and the entry
        // into `ordered` that applies today's date — the fullest shape this report has.
        (ResponseContract::OrderEdit, MutationCell::Success) => vec![
            "order",
            "edit",
            "status=ordered",
            "ordering_institution=northwind",
            "order_group=Autumn build",
            "orderer=",
        ],
        // A Date Ordered the record cannot hold, named. The two creating commands refuse
        // the same value under the same code, and record a different cell — a rejected
        // assignment set and an already-managed candidate carry more of their own shape.
        (ResponseContract::OrderEdit, MutationCell::Rejected) => {
            vec!["order", "edit", "date_ordered=2026-02-30"]
        }
        (ResponseContract::OrderEdit, _) => vec!["order", "edit", "title=Laser heads"],
        (ResponseContract::OrderItemAdd, MutationCell::Success) => vec![
            "order",
            "item",
            "add",
            "description=Fiber patch cable",
            "quantity=3",
            "unit_price=12.5",
            "catalog_number=SC-100",
            "position=2",
        ],
        // The one refusal only `add` reaches: a command line that assigned the Item none of
        // its required facts. The Order resolved, which is what makes this the Item's own
        // missing-facts refusal rather than the Order's.
        (ResponseContract::OrderItemAdd, MutationCell::Rejected) => {
            vec!["order", "item", "add", "catalog_number=SC-100"]
        }
        (ResponseContract::OrderItemAdd, _) => vec![
            "order",
            "item",
            "add",
            "description=Fiber patch cable",
            "quantity=3",
            "unit_price=12.5",
        ],
        // A change and a clearing in one atomic set.
        (ResponseContract::OrderItemEdit, MutationCell::Success) => {
            vec![
                "order",
                "item",
                "edit",
                "1",
                "quantity=6",
                "catalog_number=",
            ]
        }
        // A source position past the end, where the sentence names the range this command
        // can address.
        (ResponseContract::OrderItemEdit, MutationCell::Rejected) => {
            vec!["order", "item", "edit", "9", "quantity=6"]
        }
        (ResponseContract::OrderItemEdit, _) => {
            vec!["order", "item", "edit", "1", "quantity=6"]
        }
        (ResponseContract::OrderItemMove, MutationCell::Success) => {
            vec!["order", "item", "move", "3", "position=1"]
        }
        // The destination end refused, which is the role a `move` alone has two of.
        (ResponseContract::OrderItemMove, MutationCell::Rejected) => {
            vec!["order", "item", "move", "1", "position=0"]
        }
        (ResponseContract::OrderItemMove, _) => {
            vec!["order", "item", "move", "2", "position=1"]
        }
        (ResponseContract::OrderItemRemove, MutationCell::Rejected) => {
            vec!["order", "item", "remove", "1"]
        }
        (ResponseContract::OrderItemRemove, _) => vec!["order", "item", "remove", "2"],
        (contract, cell) => panic!("{} has no {} run", contract.name(), cell.name()),
    }
}

/// Every way each of the five reaches exit `2` carries `spielorders.usage`.
///
/// **A usage error is not any command's cell, and this is the behavioural half of that.**
/// The new routes are the assignment language over an Order-rooted command, the `POSITION`
/// positional, and the `position=` key. A command line missing the position its shape has,
/// a `move` with no destination or one spelled as something else, and an insertion point
/// assigned twice all name something no command has — so nothing about the Order was
/// attempted, and the response is the shared usage one rather than a refused mutation.
/// Which numbers name an Item is the collection's own business and reaches exit `1`
/// instead, which `tests/order_and_item_edits.rs` is what pins.
#[test]
fn every_usage_error_a_record_mutation_reaches_carries_the_shared_identity() {
    let (_tree, root, directory) = package_with_three_items("record-mutation-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        // A token that is not an assignment, where there is no shorthand to read it as.
        vec!["order", "edit", "title", "--json"],
        vec!["order", "item", "edit", "1", "quantity", "--json"],
        // The `POSITION` its shape has, omitted.
        vec!["order", "item", "edit", "--json"],
        vec!["order", "item", "remove", "--json"],
        // A `move` with no destination, one spelled as another key, and one written as a
        // bare number.
        vec!["order", "item", "move", "1", "--json"],
        vec!["order", "item", "move", "1", "destination=2", "--json"],
        vec!["order", "item", "move", "1", "2", "--json"],
        // An insertion point assigned twice.
        vec![
            "order",
            "item",
            "add",
            "description=Lens",
            "position=1",
            "position=2",
            "--json",
        ],
        // An assignment where a command takes none.
        vec!["order", "item", "remove", "1", "quantity=1", "--json"],
        vec!["order", "item", "list", "1", "--json"],
        // Two output modes at once, an option another family has, and a misspelling of one
        // these commands do have.
        vec!["order", "edit", "--json", "--quiet"],
        vec!["order", "item", "add", "--json", "--all"],
        vec![
            "order",
            "item",
            "move",
            "1",
            "position=2",
            "--json",
            "--dry-runn",
        ],
        // The two targeting routes together, which is this family's own usage error.
        vec![
            "order",
            "edit",
            "title=Laser heads",
            "--json",
            "--uuid",
            NEWEST_ORDER_ID,
            "--root",
            ".",
        ],
        // A misspelling of the command itself, and of the family the Items are under.
        vec!["order", "edti", "--json"],
        vec!["order", "itme", "list", "--json"],
        vec!["order", "item", "remvoe", "1", "--json"],
    ] {
        let outcome = Invocation::new(&wrong).in_directory(&directory).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            crate::support::managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Order was attempted"
        );
    }
}

/// Every way an adoption carrying Items reaches exit `2` carries `spielorders.usage`.
///
/// **`item` is the new route.** A token that is neither an assignment nor that word names
/// something this command line has no notion of, so nothing about the candidate was
/// attempted. An Item whose *assignments* break a rule is the core's refusal at exit `1`
/// instead, which `tests/order_and_item_edits.rs` is what pins.
#[test]
fn every_usage_error_an_adoption_with_items_reaches_carries_the_shared_identity() {
    let (_tree, root, candidate) = package_with_an_adoption_candidate("adopt-items-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        vec!["item", "Lens"],
        vec!["items", "description=Lens"],
        vec!["item", "description=Lens", "3"],
    ] {
        let mut run = Invocation::new(&["adopt-order"])
            .argument(&candidate)
            .rooted_at(&root);
        for assignment in crate::support::ADOPTED_FACTS {
            run = run.argument(assignment);
        }
        for argument in &wrong {
            run = run.argument(argument);
        }
        let outcome = run.argument("--json").run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            crate::support::managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the candidate was attempted"
        );
    }
}

// ---------------------------------------------------------------------------
// The two validating commands and the two index commands.
// ---------------------------------------------------------------------------

/// The fixture cells `validate` supplies.
///
/// `success` is a healthy populated Package, valid with a present, empty diagnostics collection;
/// `empty-result` is an empty Package, where the answer is that there was nothing to
/// examine rather than that there is no answer; `warning` is a valid Package whose one
/// finding is a leftover staging artifact beside a valid destination, which is where
/// "severity alone decides the status" is pinned in bytes; `invalid` is a Package whose
/// record the load *read and did not trust*, which is where a failed response carrying
/// the diagnosis as its whole answer is pinned.
const VALIDATE_CELLS: [&str; 4] = ["success", "empty-result", "warning", "invalid"];

/// The fixture cells `order validate` supplies.
///
/// `success` is a healthy Order; `warning` is a valid Order with a leftover staging
/// artifact beside its record, the only cell whose diagnostics are non-empty and whose
/// response still succeeds — and the one that records an offered repair, because that
/// finding is the Order-scope one this core has a safe action for; `invalid` is an Order
/// whose record parsed and did not validate, which is the sharp case for a validating
/// command — a subject the load could read and not trust, rather than one it could not
/// read.
const ORDER_VALIDATE_CELLS: [&str; 3] = ["success", "warning", "invalid"];

/// The fixture cells `index status` supplies.
///
/// `success` is a Package whose derived index is current; `warning` is a readable
/// Package carrying a Damaged Order, where the index reports `stale` with its typed reason
/// *and* the observation behind it; `invalid` is a Package the load could read and not
/// trust, where the index is a projection of records there is nothing to project from.
const INDEX_STATUS_CELLS: [&str; 3] = ["success", "warning", "invalid"];

/// The fixture cells `index rebuild` supplies.
///
/// `success` is a committed rebuild that replaced the index; `invalid` is a Package
/// the load could read and not trust, which has no healthy records to project; `rejected`
/// is a rebuild whose replacement failed, which preserves the stored index.
///
/// A dry run has no cell of its own: it is the `success` document with `committed: false`,
/// which `tests/validate_and_index.rs` pins against the byte-identical index it leaves.
const INDEX_REBUILD_CELLS: [&str; 3] = ["success", "invalid", "rejected"];

/// Each of the four contracts, in each of its cells, against its recorded bytes.
///
/// The four are compared together because they are one set: the two validating commands
/// answer one question over two scopes, and the two index commands report and replace one
/// artifact. A change to any of their recorded bytes is a contract change, and the ticket
/// causing it says whether it is a version bump or a formatting change.
#[test]
fn every_validating_and_index_response_matches_its_golden_fixtures() {
    for (contract, cells) in [
        (ResponseContract::Validate, VALIDATE_CELLS.as_slice()),
        (
            ResponseContract::OrderValidate,
            ORDER_VALIDATE_CELLS.as_slice(),
        ),
        (ResponseContract::IndexStatus, INDEX_STATUS_CELLS.as_slice()),
        (
            ResponseContract::IndexRebuild,
            INDEX_REBUILD_CELLS.as_slice(),
        ),
    ] {
        for cell in cells {
            let label = format!("golden-{}-{cell}", contract.name().replace('.', "-"));
            let (_tree, root, run) = judging_run(contract, cell, &label);
            let outcome = run.run();
            assert_eq!(
                outcome.status,
                Some(judging_status(cell)),
                "{} {cell}: {}",
                contract.name(),
                outcome.stdout_text()
            );
            assert_matches_golden(contract, cell, &outcome, &root);
        }
    }
}

/// The exit status each cell records. `warning` is a success carrying observations.
fn judging_status(cell: &str) -> i32 {
    match cell {
        "success" | "empty-result" | "warning" => 0,
        _ => 1,
    }
}

/// The Package each cell is recorded against, and the command line that produces it.
fn judging_run(
    contract: ResponseContract,
    cell: &str,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    match (contract, cell) {
        (ResponseContract::Validate, "success") => {
            let (tree, root) = populated_package(label);
            let run = Invocation::new(&["validate", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Validate, "empty-result") => {
            let (tree, root) = initialized_package(label);
            let run = Invocation::new(&["validate", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Validate, "warning") => {
            let (tree, root) = package_with_a_staging_artifact(label);
            let run = Invocation::new(&["validate", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Validate, _) => {
            let (tree, root, _damaged) = package_with_an_impossible_date(label);
            let run = Invocation::new(&["validate", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::OrderValidate, "success") => {
            let (tree, root) = populated_package(label);
            let directory = order_directory(&root, "Laser_components");
            let run = Invocation::new(&["order", "validate", "--json"]).rooted_at(&directory);
            (tree, root, run)
        }
        (ResponseContract::OrderValidate, "warning") => {
            let (tree, root, directory) = package_with_an_order_staging_artifact(label);
            let run = Invocation::new(&["order", "validate", "--json"]).rooted_at(&directory);
            (tree, root, run)
        }
        (ResponseContract::OrderValidate, _) => {
            let (tree, root, damaged) = package_with_an_impossible_date(label);
            let run = Invocation::new(&["order", "validate", "--json"]).rooted_at(&damaged);
            (tree, root, run)
        }
        (ResponseContract::IndexStatus, "success") => {
            let (tree, root) = populated_package(label);
            let run = Invocation::new(&["index", "status", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::IndexStatus, "warning") => {
            let (tree, root, _damaged) = package_with_a_damaged_order(label);
            let run = Invocation::new(&["index", "status", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::IndexStatus, _) => {
            let (tree, root) = package_with_duplicate_vendor_names(label);
            let run = Invocation::new(&["index", "status", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::IndexRebuild, "success") => {
            let (tree, root) = populated_package(label);
            std::fs::remove_file(root.join(".spielorders/index.json"))
                .expect("the derived index is removed");
            let run = Invocation::new(&["index", "rebuild", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::IndexRebuild, "rejected") => {
            let (tree, root) = populated_package(label);
            // A directory at the staging name makes the managed replacement's staging
            // write fail, which is a real filesystem refusal rather than a core
            // capability asked to misbehave.
            std::fs::create_dir(root.join(".spielorders/.index.json.spielorders-stage"))
                .expect("the index staging name is occupied");
            let run = Invocation::new(&["index", "rebuild", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::IndexRebuild, _) => {
            let (tree, root) = package_with_duplicate_vendor_names(label);
            let run = Invocation::new(&["index", "rebuild", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (contract, cell) => panic!("{} has no {cell} run", contract.name()),
    }
}

/// Every usage error these four commands can reach carries the one shared identity.
///
/// Each of them takes no positional subject at all — a validating command's subject is
/// its root and an index command's is the Package's own derived artifact — so a positional
/// token names something no command here has, and `index` without a subcommand names no
/// command at all.
#[test]
fn every_usage_error_a_judging_command_reaches_carries_the_shared_identity() {
    let (_tree, root) = populated_package("judging-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        vec!["validate", "Tarnwick"],
        vec!["validate", "--fix"],
        vec!["order", "validate", "2"],
        vec!["index"],
        vec!["index", "statuz"],
        vec!["index", "rebuild", "--repair"],
        vec!["index", "rebuild", "Orders"],
    ] {
        let mut arguments = wrong.clone();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert!(
            document["context"]["package_root"].is_null(),
            "{wrong:?}: no command ran, so none was about a Package"
        );
    }
    assert_eq!(
        crate::support::managed_snapshot(&root),
        before,
        "a usage error attempts nothing"
    );
}

// ---------------------------------------------------------------------------
// The two repair commands and the upgrade.
// ---------------------------------------------------------------------------

/// The fixture cells each repair command supplies.
///
/// `success` is a bare diagnosis carrying a case: the command answered, and the answer is
/// what may be done — the ephemeral handle, the actions, the values each requires, and the
/// exact command that applies it. `empty-result` is a scope with nothing actionable, where
/// the answer is that there is nothing to do rather than that there is no answer.
/// `warning` is an applied repair whose canonical write succeeded and whose derived index
/// refresh then failed, which is where committed-with-warnings is pinned. `invalid` is a
/// subject that could not be diagnosed at all. `rejected` is a refused application.
///
/// **A cleanly committed application has no cell of its own and needs none**: it is the
/// `warning` document with an empty `warnings`, which `tests/repair_and_upgrade.rs` pins
/// against the real run of every allowed repair, and that carries it to these recorded
/// bytes. It is the same reading that gives a dry run no cell of its own.
const REPAIR_CELLS: [&str; 5] = ["success", "empty-result", "warning", "invalid", "rejected"];

/// The fixture cells `upgrade` supplies.
///
/// `success` is a Package already at the newest version this build understands, which
/// reports its source, its target, every artifact in the order an upgrade would visit
/// them, and that there is nothing outstanding. `invalid` is a Package declaring a version
/// this build does not interpret, which is left exactly as it is. `rejected` is a Package
/// whose configuration does not name a usable Orders Directory, so the upgrade refuses
/// rather than assuming the conventional location.
const UPGRADE_CELLS: [&str; 3] = ["success", "invalid", "rejected"];

/// Each of the three contracts, in each of its cells, against its recorded bytes.
#[test]
fn every_repairing_response_matches_its_golden_fixtures() {
    for (contract, cells) in [
        (ResponseContract::Repair, REPAIR_CELLS.as_slice()),
        (ResponseContract::OrderRepair, REPAIR_CELLS.as_slice()),
        (ResponseContract::Upgrade, UPGRADE_CELLS.as_slice()),
    ] {
        for cell in cells {
            let label = format!("golden-{}-{cell}", contract.name().replace('.', "-"));
            let (_tree, root, run) = repairing_run(contract, cell, &label);
            let outcome = run.run();
            let document = response(&outcome);
            assert_eq!(
                outcome.status,
                Some(repairing_status(cell)),
                "{} {cell}: {}",
                contract.name(),
                outcome.stdout_text()
            );
            assert_repairing_kind(contract, cell, &document, &label);
            assert_matches_golden_with_substitutions(contract, cell, &outcome, &root, None);
        }
    }
}

/// The exit status each cell records.
fn repairing_status(cell: &str) -> i32 {
    match cell {
        "success" | "empty-result" | "warning" => 0,
        _ => 1,
    }
}

/// Asserts the response is the *kind* this cell names.
///
/// **A status cannot say which cell a response is, and the recorded bytes cannot either.**
/// Three of the five exit `0`, so a run that reached a neighbouring kind would record its
/// own document happily under the wrong name — and a `warning` cell holding a repair that
/// found nothing to do would pin a cell no regression of committed-with-warnings could
/// make fail. The distinctions are the report's own.
fn assert_repairing_kind(
    contract: ResponseContract,
    cell: &str,
    document: &serde_json::Value,
    label: &str,
) {
    if contract == ResponseContract::Upgrade {
        // The upgrade's own three: an answer about a Package this build reads, and two
        // refusals about one it will not.
        match cell {
            "success" => assert_eq!(document["data"]["complete"], true, "{label}"),
            _ => assert_refused_kind(cell, document, label),
        }
        return;
    }
    let cases = || {
        document["data"]["cases"]
            .as_array()
            .unwrap_or_else(|| panic!("{label}: a repair reports what is actionable now"))
    };
    match cell {
        "success" => {
            assert!(
                document["data"]["report"].is_null(),
                "{label}: a diagnosis applies nothing"
            );
            assert!(
                !cases().is_empty(),
                "{label}: the answer is the case it offers, so a scope with none is not this cell"
            );
        }
        "empty-result" => {
            assert!(document["data"]["report"].is_null(), "{label}");
            assert!(cases().is_empty(), "{label}: nothing here is actionable");
        }
        "warning" => {
            assert_eq!(
                document["data"]["report"]["committed"], true,
                "{label}: a repair that applied nothing is not this cell"
            );
            assert!(
                !document["data"]["report"]["warnings"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{label}: a committed repair reports its warnings"))
                    .is_empty(),
                "{label}: committed-with-warnings names the warning"
            );
        }
        _ => assert_refused_kind(cell, document, label),
    }
}

/// Tells the two failing cells apart, which neither the status nor the envelope can.
///
/// `invalid` and `rejected` are both `ok: false` with no `data`, so the check that a run
/// did not quietly record a neighbouring kind has to ask what failed. The two are
/// different questions: `invalid` is a subject this command could not diagnose at all, and
/// `rejected` is a subject it diagnosed and an application it then refused.
fn assert_refused_kind(cell: &str, document: &serde_json::Value, label: &str) {
    assert_eq!(document["ok"], false, "{label}");
    assert!(document["data"].is_null(), "{label}");
    let code = document["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("{label}: a failed response carries a stable code"));
    let diagnosed_nothing = matches!(
        code,
        "package_root_unmanaged" | "order_root_unmanaged" | "package_metadata_unsupported"
    );
    match cell {
        "invalid" => assert!(
            diagnosed_nothing,
            "{label}: this cell is a subject that could not be diagnosed, and `{code}` is a \
             refused application"
        ),
        _ => assert!(
            !diagnosed_nothing,
            "{label}: this cell is a refused application, and `{code}` is a subject that \
             could not be diagnosed"
        ),
    }
}

/// The Package each cell is recorded against, and the command line that produces it.
fn repairing_run(
    contract: ResponseContract,
    cell: &str,
    label: &str,
) -> (TemporaryTree, PathBuf, Invocation) {
    match (contract, cell) {
        (ResponseContract::Repair, "success") => {
            let (tree, root) = package_with_a_staging_artifact(label);
            let run = Invocation::new(&["repair", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Repair, "empty-result") => {
            let (tree, root) = populated_package(label);
            let run = Invocation::new(&["repair", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Repair, "warning") => {
            let (tree, root) = package_with_a_package_case_and_a_blocked_index_refresh(label);
            let run = applying_run(
                &["repair"],
                ".spielorders/.vendors.json.spielorders-stage",
                &root,
                "staging_artifact_present",
                &["action=promote_staging_candidate"],
            );
            (tree, root, run)
        }
        (ResponseContract::Repair, "rejected") => {
            let (tree, root) = package_with_a_staging_artifact(label);
            let run = Invocation::new(&["repair", "--json"])
                .argument(format!("case={STALE_HANDLE}"))
                .argument("action=promote_staging_candidate")
                .rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Repair, _) => {
            let (tree, root) = initialized_package(label);
            let outside = directory_that_is_not_a_root(&root);
            let run = Invocation::new(&["repair", "--json"]).rooted_at(&outside);
            (tree, root, run)
        }
        (ResponseContract::OrderRepair, "success") => {
            let (tree, root, damaged) = package_with_an_unresolved_vendor_reference(label);
            let run = Invocation::new(&["order", "repair", "--json"]).rooted_at(&damaged);
            (tree, root, run)
        }
        (ResponseContract::OrderRepair, "empty-result") => {
            let (tree, root) = populated_package(label);
            let directory = order_directory(&root, "Laser_components");
            let run = Invocation::new(&["order", "repair", "--json"]).rooted_at(&directory);
            (tree, root, run)
        }
        (ResponseContract::OrderRepair, "warning") => {
            let (tree, root, damaged) = package_with_an_unresolved_vendor_reference(label);
            std::fs::create_dir(root.join(".spielorders/.index.json.spielorders-stage"))
                .expect("the index staging name is occupied");
            let run = applying_run(
                &["order", "repair"],
                &format!(
                    "Orders/{}/.spielorders/order.json",
                    damaged
                        .file_name()
                        .and_then(|name| name.to_str())
                        .expect("the fixture directory is UTF-8")
                ),
                &damaged,
                "vendor_reference_unresolved",
                &["action=clear_vendor_reference"],
            );
            (tree, root, run)
        }
        (ResponseContract::OrderRepair, "rejected") => {
            let (tree, root, damaged) = package_with_an_unresolved_vendor_reference(label);
            let run = Invocation::new(&["order", "repair", "--json"])
                .argument(format!("case={STALE_HANDLE}"))
                .argument("action=clear_vendor_reference")
                .rooted_at(&damaged);
            (tree, root, run)
        }
        (ResponseContract::OrderRepair, _) => {
            let (tree, root) = initialized_package(label);
            let outside = directory_that_is_not_a_root(&root);
            let run = Invocation::new(&["order", "repair", "--json"]).rooted_at(&outside);
            (tree, root, run)
        }
        (ResponseContract::Upgrade, "success") => {
            let (tree, root) = populated_package(label);
            let run = Invocation::new(&["upgrade", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Upgrade, "rejected") => {
            let (tree, root) = populated_package(label);
            std::fs::write(
                root.join(".spielorders/config.toml"),
                b"orders_path = \"../elsewhere\"\nremoved_path = \"Removed\"\n",
            )
            .expect("the fixture configuration is written");
            let run = Invocation::new(&["upgrade", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (ResponseContract::Upgrade, _) => {
            let (tree, root) = package_at_an_uninterpretable_version(label);
            let run = Invocation::new(&["upgrade", "--json"]).rooted_at(&root);
            (tree, root, run)
        }
        (contract, cell) => panic!("{} has no {cell} run", contract.name()),
    }
}

/// A handle no diagnosis of any Package ever reported.
///
/// It is shaped like one — a code, a place, and an evidence digest — because a refusal for
/// a handle that could not have come from a diagnosis at all would pin nothing about a
/// *stale* one, which is the condition this records.
const STALE_HANDLE: &str = "staging_artifact_present:.spielorders/.vendors.json.spielorders-stage:\
     0000000000000000000000000000000000000000000000000000000000000000";

/// One application of the case answering `code` at `place`, with the handle this Package
/// reports now.
///
/// The handle is ephemeral, so it cannot be written into a fixture recipe: it is read out
/// of a diagnosis taken against this very Package a moment before the run. The case is
/// selected by its place as well as its code, because one Package can carry two findings
/// under one code and a recipe that took the first would record whichever the enumeration
/// happened to yield.
fn applying_run(
    command: &[&str],
    place: &str,
    subject: &Path,
    code: &str,
    rest: &[&str],
) -> Invocation {
    let mut arguments: Vec<&str> = command.to_vec();
    arguments.push("--json");
    let diagnosed = response(&Invocation::new(&arguments).rooted_at(subject).run());
    let handle = diagnosed["data"]["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("{subject:?}: the diagnosis reports its cases"))
        .iter()
        .find(|case| {
            case["code"] == code
                && case["location"]["package_relative_path"]
                    .as_str()
                    .is_some_and(|path| path == place)
        })
        .unwrap_or_else(|| {
            panic!(
                "{code} at {place} is diagnosed: {}",
                diagnosed["data"]["cases"]
            )
        })["case"]
        .as_str()
        .expect("a case names its handle")
        .to_owned();
    let mut run = Invocation::new(&arguments).argument(format!("case={handle}"));
    for argument in rest {
        run = run.argument(argument);
    }
    run.rooted_at(subject)
}

/// Every way these three commands reach exit `2` carries `spielorders.usage`.
///
/// **The `case` and `action` assignments are the new routes here.** They are what tell a
/// bare diagnosis from an application, so a command line that names one without the other,
/// names either twice, names an action this build has no notion of, or supplies a
/// resolution with no action to resolve has not said which of the two moods it is asking
/// for — nothing about the subject was attempted, and the response is the shared usage one
/// rather than a refused repair. An action a diagnosed case did not offer is the other
/// thing entirely: that is a perfectly formed application the core refuses on the
/// evidence, at exit `1`.
#[test]
fn every_usage_error_a_repairing_command_reaches_carries_the_shared_identity() {
    let (_tree, root) = package_with_a_staging_artifact("repairing-usage-errors");
    let before = crate::support::managed_snapshot(&root);

    for wrong in [
        // A token that is not an assignment at all.
        vec!["repair", "Tarnwick"],
        // Half an application, both ways round.
        vec!["repair", "case=staging_artifact_present:x:y"],
        vec!["repair", "action=promote_staging_candidate"],
        // One key assigned twice.
        vec![
            "repair",
            "case=a",
            "case=b",
            "action=promote_staging_candidate",
        ],
        // An action this build has no notion of.
        vec!["repair", "case=a", "action=fix_everything"],
        // A resolution with no application to resolve.
        vec!["repair", "vendor=Tarnwick"],
        // The broad fix this contract does not have.
        vec!["repair", "--fix"],
        // Two output modes at once.
        vec!["repair", "--quiet"],
        // The same routes at the Order scope.
        vec!["order", "repair", "Tarnwick"],
        vec!["order", "repair", "case=a"],
        vec!["order", "repair", "action=clear_vendor_reference"],
        vec!["order", "repair", "--all"],
        // A misspelling of the command itself.
        vec!["reapir"],
        vec!["order", "repari"],
        // The upgrade takes no subject and no target.
        vec!["upgrade", "Orders"],
        vec!["upgrade", "--to", "2"],
        vec!["upgrade", "--quiet"],
        vec!["upgarde"],
    ] {
        let mut arguments = wrong.clone();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["contract_version"], 1, "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert!(
            document["context"]["package_root"].is_null(),
            "{wrong:?}: no command ran, so none was about a Package"
        );
    }
    assert_eq!(
        crate::support::managed_snapshot(&root),
        before,
        "a usage error attempts nothing"
    );
}

// ---------------------------------------------------------------------------
// `search supporting-material`'s golden fixture.
// ---------------------------------------------------------------------------

/// The fixture cells `search supporting-material` records.
///
/// Success is registered only after its byte comparison in the fixture inventory.
const SUPPORTING_MATERIAL_CELLS: [&str; 3] = ["empty-result", "warning", "invalid"];

fn compare_supporting_material_success() -> &'static str {
    use spielorders_core::{
        canonical::{CanonicalPackageLoad, inspect_canonical_package},
        ports::{Cancellation, SupportingMaterialIndex},
        supporting_material::{
            Completeness, ProviderAnswer, ProviderHit, SupportingMaterialQuery,
            SupportingMaterialRequest, search_supporting_material,
        },
    };
    struct RecordedIndex(PathBuf);
    impl SupportingMaterialIndex for RecordedIndex {
        fn name(&self) -> &'static str {
            "spotlight"
        }
        fn search(
            &self,
            _: &SupportingMaterialRequest<'_>,
            _: &dyn Cancellation,
        ) -> ProviderAnswer {
            ProviderAnswer::Looked {
                completeness: Completeness::Complete,
                hits: vec![ProviderHit {
                    path: self.0.clone(),
                    kind: Some("txt".to_owned()),
                    modified_unix_seconds: Some(1_784_937_600),
                    rank: None,
                }],
                diagnostics: Vec::new(),
            }
        }
    }
    struct NotCancelled;
    impl Cancellation for NotCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }
    }
    let (_tree, root) = initialized_package("recorded-file-search-success");
    let directory = root.join("Orders/Laser_components");
    std::fs::create_dir_all(directory.join(".spielorders")).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/examples/v1/minimal/order.json");
    let mut order: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    order["status"] = "ordered".into();
    std::fs::write(
        directory.join(".spielorders/order.json"),
        serde_json::to_vec(&order).unwrap(),
    )
    .unwrap();
    let path = directory.join("quote-coherence.txt");
    std::fs::write(&path, b"Supporting Material contract capture: coherence.\n").unwrap();
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(&root, &spielorders_cli::effects::PlatformFileSystem)
    else {
        panic!("fixture loads")
    };
    let index = RecordedIndex(path);
    let snapshot = search_supporting_material(
        &package,
        &SupportingMaterialQuery::new("coherence", std::time::Duration::from_secs(5)),
        Some(&index),
        &NotCancelled,
    );
    let bytes =
        spielorders_cli::contract::supporting_material_document(&package, "coherence", &snapshot)
            .unwrap();
    let actual = String::from_utf8(bytes)
        .unwrap()
        .replace(root.to_str().unwrap(), "<PACKAGE_ROOT>");
    assert_golden(
        ResponseContract::SearchSupportingMaterial,
        "success",
        actual,
    );
    "success"
}

/// A word no file in the fixture Packages carries, in its name or its contents.
///
/// The recorded bytes must be the same bytes whatever the platform's index holds, and this
/// is what makes them so: an index that has never looked at a temporary directory and one
/// that has indexed all of it both report nothing for this word.
const UNMATCHABLE: &str = "zirconium";

/// Each cell `search supporting-material` records, against its recorded bytes.
///
/// Three comparisons in one test because they are one claim about one contract: what the
/// index answered, that answer beside a Package the load has something to say about, and a
/// Package it could not trust at all.
///
/// **The two cells carrying a provider's answer are the bytes a build with a provider wrote**,
/// which is the one thing in this file a platform decides. A build with none reaches the state
/// that prevented an answer for those two runs, which is asserted here rather than passed
/// over — including that what the load observed still travels beside it, since a Damaged Order
/// is a fact about the Package whether or not anything could look inside it. `invalid` is the
/// load refusing before any provider is asked, so it is the one cell whose bytes every build
/// writes.
#[test]
fn every_supporting_material_response_matches_its_golden_fixtures() {
    for (cell, package, status) in [
        (
            "empty-result",
            populated_package as fn(&str) -> (TemporaryTree, PathBuf),
            0,
        ),
        ("warning", damaged_order_package, 0),
        ("invalid", package_with_duplicate_vendor_names, 1),
    ] {
        let label = format!("golden-supporting-material-{cell}");
        let (_tree, root) = package(&label);

        let outcome = Invocation::new(&["search", "supporting-material", UNMATCHABLE])
            .rooted_at(&root)
            .argument("--json")
            .run();

        if supporting_material_provider().is_none() && cell != "invalid" {
            let document = response(&outcome);
            assert_eq!(outcome.status, Some(1), "{cell}: {}", outcome.stdout_text());
            assert_eq!(
                document["error"]["code"], "supporting_material_unsupported",
                "{cell}: {document}"
            );
            assert_eq!(
                document["diagnostics"].as_array().map(Vec::is_empty),
                Some(cell != "warning"),
                "{cell}: what the load observed reaches the caller either way: {document}"
            );
            continue;
        }
        assert_eq!(
            outcome.status,
            Some(status),
            "{cell}: {}",
            outcome.stdout_text()
        );
        assert_matches_golden(
            ResponseContract::SearchSupportingMaterial,
            cell,
            &outcome,
            &root,
        );
    }
}

// ---------------------------------------------------------------------------
// The two entity searches' golden fixtures.
// ---------------------------------------------------------------------------

/// One recorded cell of an entity search.
struct SearchCell {
    contract: ResponseContract,
    /// Which of the response kinds this cell is.
    cell: &'static str,
    /// The command line a caller writes, before `--root` and `--json`.
    line: &'static [&'static str],
    /// The Package it is recorded against.
    package: fn(&str) -> (TemporaryTree, PathBuf),
    status: i32,
}

/// [`package_with_a_damaged_order`]'s Root, without the Directory it damaged.
fn damaged_order_package(label: &str) -> (TemporaryTree, PathBuf) {
    let (tree, root, _damaged) = package_with_a_damaged_order(label);
    (tree, root)
}

/// Every cell the two entity searches record.
///
/// One table rather than two, because the two record one set of cells: a cell one of them
/// supplies and the other does not is a hole nobody finds until a consumer hits it, and
/// `tests/cross_command_conformance.rs` is where the set being the four a read has is
/// said.
///
/// **`empty-result` is a populated Package and a query nothing matches**, rather than an
/// empty Package. The answer a search can give that no other read can is that a Package
/// with entities in it holds none that match, and recording an empty Package instead would
/// record having nothing where the contract's claim is about matching nothing.
///
/// **`success` is recorded from one quoted multiword argument** whose two terms land on
/// two different facts of one Order and on no single fact of either, so the tokenization
/// this contract depends on is in the recorded bytes rather than only in a live assertion.
/// `warning` drops one of those terms, so that cell is also where the Vendor's facts
/// reaching the Order that names it is recorded.
///
/// **`warning` is recorded with `--all` for the Vendor search**, the same choice
/// `package list vendors` made and for the same reason: both halves of the Vendor
/// visibility default are then in bytes, and the Hidden Vendor a query retains is
/// reachable in a recorded response rather than only in a live one.
const ENTITY_SEARCH_CELLS: [SearchCell; 8] = [
    SearchCell {
        contract: ResponseContract::SearchOrders,
        cell: "success",
        line: &["search", "orders", "laser lovelace"],
        package: populated_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchOrders,
        cell: "empty-result",
        line: &["search", "orders", "zirconium"],
        package: populated_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchOrders,
        cell: "warning",
        line: &["search", "orders", "lovelace"],
        package: damaged_order_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchOrders,
        cell: "invalid",
        line: &["search", "orders", "lovelace"],
        package: package_with_duplicate_vendor_names,
        status: 1,
    },
    SearchCell {
        contract: ResponseContract::SearchVendors,
        cell: "success",
        line: &["search", "vendors", "tarnwick accounts"],
        package: populated_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchVendors,
        cell: "empty-result",
        line: &["search", "vendors", "zirconium"],
        package: populated_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchVendors,
        cell: "warning",
        line: &["search", "vendors", "nethermoor", "--all"],
        package: damaged_order_package,
        status: 0,
    },
    SearchCell {
        contract: ResponseContract::SearchVendors,
        cell: "invalid",
        line: &["search", "vendors", "tarnwick"],
        package: package_with_duplicate_vendor_names,
        status: 1,
    },
];

/// Each entity search, in each of its four cells, against its recorded bytes.
///
/// Eight comparisons in one test because they are one claim: these two contracts are a
/// set, and a change to one of them is as much a contract change as a change to both. The
/// failure names the contract and the cell.
#[test]
fn every_entity_search_matches_its_golden_fixtures() {
    for recorded in ENTITY_SEARCH_CELLS {
        let name = recorded.contract.name();
        let label = format!("golden-{}-{}", name.replace('.', "-"), recorded.cell);
        let (_tree, root) = (recorded.package)(&label);

        let outcome = Invocation::new(recorded.line)
            .rooted_at(&root)
            .argument("--json")
            .run();

        assert_eq!(
            outcome.status,
            Some(recorded.status),
            "{name} {}: {}",
            recorded.cell,
            outcome.stdout_text()
        );
        assert_matches_golden(recorded.contract, recorded.cell, &outcome, &root);
    }
}
