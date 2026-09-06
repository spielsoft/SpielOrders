//! Creating, editing, hiding, and unhiding one Vendor through canonical assignments.
//!
//! **The mutation contract every later mutating command follows** is exercised here: the
//! typed report beside the refreshed Vendor, the four outcomes told apart, `--dry-run` as
//! the same answer with nothing done, and a committed mutation that stays committed when
//! the derived index refresh fails.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics; Durable managed-file writes.

use crate::support::{
    HIDDEN_VENDOR_ID, Invocation, OFFERED_VENDOR_ID, initialized_package,
    initialized_package_with_a_blocked_index_refresh, managed_snapshot, minted_uuid_v4,
    package_with_a_blocked_index_refresh, populated_package, response,
};

/// The two `add-vendor` forms are one command line spelled two ways, and combining them
/// is a usage error.
///
/// The Name shorthand is exactly `name=…`, so the two runs differ in nothing but the
/// identity each minted. The defect this catches is a shorthand handled by its own code
/// path — which is how `add-vendor "Tarnwick"` comes to accept a Name that
/// `add-vendor name=Tarnwick` refuses, or to record it untrimmed.
#[test]
fn both_add_vendor_forms_produce_the_same_vendor_and_cannot_be_combined() {
    let (_shorthand_tree, shorthand_root) = initialized_package("add-vendor-shorthand");
    let (_assigned_tree, assigned_root) = initialized_package("add-vendor-assigned");

    let shorthand = Invocation::new(&["add-vendor", "Tarnwick", "--json"])
        .rooted_at(&shorthand_root)
        .run();
    let assigned = Invocation::new(&["add-vendor", "name=Tarnwick", "--json"])
        .rooted_at(&assigned_root)
        .run();

    assert_eq!(shorthand.status, Some(0), "{}", shorthand.stderr_text());
    assert_eq!(assigned.status, Some(0), "{}", assigned.stderr_text());
    let one = response(&shorthand);
    let other = response(&assigned);
    for document in [&one, &other] {
        assert_eq!(document["data"]["vendor"]["name"], "Tarnwick");
        assert_eq!(document["data"]["report"]["outcome"], "created");
    }
    // Every byte but the two roots and the two minted identities.
    assert_eq!(
        normalized_vendor(&shorthand, &shorthand_root),
        normalized_vendor(&assigned, &assigned_root),
        "the shorthand is the assignment it stands for"
    );

    let combined = Invocation::new(&["add-vendor", "Tarnwick", "name=Tarnwick", "--json"])
        .rooted_at(&shorthand_root)
        .run();
    assert_eq!(combined.status, Some(2));
    assert_eq!(response(&combined)["contract"], "spielorders.usage");
}

/// One run's response with its Root and its minted identity substituted, so two runs
/// against two Packages can be compared byte for byte.
fn normalized_vendor(outcome: &crate::support::Outcome, root: &std::path::Path) -> String {
    let document = response(outcome);
    let minted = document["data"]["vendor"]["vendor_id"]
        .as_str()
        .expect("a committed creation carries the identity it minted")
        .to_owned();
    crate::support::normalized(outcome, root).replace(minted_uuid_v4(&minted), "<VENDOR_ID>")
}

/// A value containing `=` survives intact, because only the first one splits.
///
/// The defect this catches is a parser that split on every `=`, which would silently
/// truncate a Vendor Name, a website, or a purchasing note that contains one — and the
/// truncation would be recorded rather than refused.
#[test]
fn only_the_first_equals_sign_splits_an_assignment() {
    let (_tree, root) = initialized_package("assignment-first-equals");

    let outcome = Invocation::new(&["add-vendor", "--json"])
        .argument("name=Tarnwick")
        .argument("website=https://example.com/q?a=1&b=2")
        .argument("purchasing_notes=quote = required")
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    let vendor = &response(&outcome)["data"]["vendor"];
    assert_eq!(vendor["website"], "https://example.com/q?a=1&b=2");
    assert_eq!(vendor["purchasing_notes"], "quote = required");
}

/// An unknown field, an immutable one, and a blank required one are each refused with the
/// field named, at exit `1`, having written nothing.
///
/// **One refusal for the whole set**, because an assignment set is one atomic mutation:
/// every refused field is named at once, so a caller correcting a command line sees all of
/// them rather than one per run. Each field's identity carries the rule it broke in the
/// core's own spelling, which is what a caller branches on — the message may be reworded.
#[test]
fn every_refused_assignment_names_its_field_and_writes_nothing() {
    let (_tree, root) = populated_package("assignment-refusals");
    let before = managed_snapshot(&root);

    for (assignments, expected) in [
        (vec!["colour=red"], vec![("unknown_field", "colour")]),
        (
            vec![
                "name=Nethermoor",
                "vendor_id=00000000-0000-4000-8000-000000000001",
            ],
            vec![("immutable_identity", "vendor_id")],
        ),
        (vec!["name="], vec![("blank_required_value", "name")]),
        (
            vec!["colour=red", "contacts=none"],
            vec![
                ("unknown_field", "colour"),
                ("ordered_collection", "contacts"),
            ],
        ),
    ] {
        let mut run = Invocation::new(&["add-vendor", "--json"]);
        for assignment in &assignments {
            run = run.argument(assignment);
        }
        let outcome = run.rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignments:?}");
        assert_eq!(document["ok"], false, "{assignments:?}");
        assert!(document["data"].is_null(), "{assignments:?}");
        assert_eq!(
            document["error"]["code"], "vendor_assignment_rejected",
            "{assignments:?}"
        );
        let named: Vec<(&str, &str)> = document["error"]["identities"]
            .as_array()
            .expect("the identities are a list")
            .iter()
            .map(|identity| {
                (
                    identity["kind"].as_str().expect("a kind"),
                    identity["value"].as_str().expect("a value"),
                )
            })
            .collect();
        assert_eq!(named, expected, "{assignments:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{assignments:?}: a refused assignment set writes nothing"
        );
    }

    // The same refusal reaches `vendor edit`, which is the command that has an existing
    // record for a wrong assignment to damage.
    let outcome = Invocation::new(&["vendor", "edit", "Tarnwick", "colour=red", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(outcome.status, Some(1));
    assert_eq!(
        response(&outcome)["error"]["code"],
        "vendor_assignment_rejected"
    );
    assert_eq!(managed_snapshot(&root), before);
}

/// `field=` clears an optional shared fact, which is then absent rather than empty.
///
/// The defect this catches is a cleared fact stored as `""`: the refreshed Vendor would
/// report an empty string, every later read would render one, and a consumer checking for
/// absence would be told the fact is present.
#[test]
fn an_empty_value_clears_an_optional_fact_into_absence() {
    let (_tree, root) = populated_package("assignment-clearing");

    let outcome = Invocation::new(&["vendor", "edit", "Tarnwick", "phone=", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    let document = response(&outcome);
    assert_eq!(document["data"]["report"]["changed"], true);
    assert!(
        document["data"]["vendor"]["phone"].is_null(),
        "the cleared fact is absent: {}",
        document["data"]["vendor"]
    );
    // Read back through the command whose contract is about reading, because a fact that
    // is absent in the mutation's answer and empty on disk is the defect this is about.
    let read = response(
        &Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert!(read["data"]["phone"].is_null());
    assert_eq!(
        read["data"]["email"], "sales@example.com",
        "and no other fact was cleared"
    );
}

/// A Name another Vendor already carries is refused, with the existing Vendor named and no
/// merge of any kind.
///
/// The comparison is the Package's own — trimmed and case-insensitive — so a Name that
/// resolves for `vendor show` cannot be created a second time. The defect this catches is
/// a creation that admitted a near-duplicate, which is precisely the state SpielOrders has
/// no operation to undo: there is no Vendor Merge.
#[test]
fn a_name_already_in_use_is_refused_and_nothing_is_merged() {
    let (_tree, root) = populated_package("vendor-duplicate-name");
    let before = managed_snapshot(&root);

    for name in ["Tarnwick", "tarnwick", "  TARNWICK  "] {
        let outcome = Invocation::new(&["add-vendor", "--json"])
            .argument(name)
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{name}");
        assert_eq!(document["error"]["code"], "vendor_name_in_use", "{name}");
        assert_eq!(
            document["error"]["identities"][0]["kind"], "vendor",
            "{name}: the Vendor already carrying the Name is named by its identity"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], OFFERED_VENDOR_ID,
            "{name}"
        );
        assert_eq!(managed_snapshot(&root), before, "{name}");
    }

    // Renaming one Vendor onto another's Name is the same refusal, and the same absence
    // of a merge.
    let outcome = Invocation::new(&["vendor", "edit", "Nethermoor", "name=tarnwick", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(outcome.status, Some(1));
    assert_eq!(response(&outcome)["error"]["code"], "vendor_name_in_use");
    assert_eq!(managed_snapshot(&root), before);
}

/// Hiding a referenced Vendor succeeds, every Order still resolves its facts, unhiding
/// restores it, and the Vendor ID never changes.
///
/// **Hiding governs what is offered, never what is resolved**, and that is the whole of
/// why a Vendor is never deleted. The defect this catches is a hide refused on reference
/// grounds, or one that hid a Vendor from the Orders that reference it — either of which
/// would leave a user unable to stop offering a Vendor without damaging old Orders.
#[test]
fn hiding_a_referenced_vendor_is_reversible_and_never_orphans_an_order() {
    let (_tree, root) = populated_package("vendor-visibility");

    let hidden = response(
        &Invocation::new(&["vendor", "hide", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(hidden["data"]["report"]["outcome"], "hidden");
    assert_eq!(hidden["data"]["report"]["changed"], true);
    assert_eq!(hidden["data"]["vendor"]["hidden"], true);
    assert_eq!(hidden["data"]["vendor"]["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(
        hidden["data"]["vendor"]["referencing_orders"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the Orders that reference it are reported by the mutation that hid it"
    );

    // The referencing Order still resolves the Vendor's facts, which is the claim hiding
    // is about — and it is asked of the command whose subject is that Order.
    let orders = response(
        &Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let referenced = orders["data"]["orders"]
        .as_array()
        .expect("the Orders are a list")
        .iter()
        .filter(|order| order["title"] == "Laser components")
        .count();
    assert_eq!(referenced, 1);
    assert_eq!(orders["data"]["orders"][0]["vendor"]["name"], "Tarnwick");
    assert_eq!(orders["data"]["orders"][0]["vendor"]["hidden"], true);

    // Hiding an already Hidden Vendor is the idempotent no-op, not an error.
    let again = Invocation::new(&["vendor", "hide", "Tarnwick", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(again.status, Some(0), "{}", again.stderr_text());
    let again = response(&again);
    assert_eq!(again["data"]["report"]["committed"], true);
    assert_eq!(
        again["data"]["report"]["changed"], false,
        "nothing was written, and that is a success"
    );
    assert_eq!(
        again["data"]["report"]["index"]["refresh"], "not_attempted",
        "a commit that wrote nothing refreshed nothing either"
    );
    assert_eq!(
        again["data"]["report"]["index"]["state"], "not_assessed",
        "and it never opened the stored index, so it has no state to report"
    );
    assert_eq!(
        again["data"]["report"]["operations"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "and it announced no managed operation"
    );

    let unhidden = response(
        &Invocation::new(&["vendor", "unhide", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(unhidden["data"]["report"]["outcome"], "unhidden");
    assert_eq!(unhidden["data"]["vendor"]["hidden"], false);
    assert_eq!(
        unhidden["data"]["vendor"]["vendor_id"], OFFERED_VENDOR_ID,
        "the Vendor ID is immutable through both halves of the pair"
    );
}

/// A rename resolves live on every Order that references the Vendor, and the Vendor ID is
/// unchanged.
///
/// An Order stores a Vendor Reference and no Vendor snapshot, so this is the claim that
/// makes renaming an ordinary edit. The defect it catches is a rename that minted a new
/// identity, or one an Order kept reading under its old Name.
#[test]
fn a_renamed_vendor_resolves_its_new_name_on_every_referencing_order() {
    let (_tree, root) = populated_package("vendor-rename");

    let renamed = response(
        &Invocation::new(&["vendor", "edit", "Nethermoor", "name=Nethermoor Optics", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(renamed["data"]["vendor"]["name"], "Nethermoor Optics");
    assert_eq!(
        renamed["data"]["vendor"]["vendor_id"], HIDDEN_VENDOR_ID,
        "a renamed Vendor keeps its identity"
    );

    let orders = response(
        &Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(&root)
            .run(),
    );
    let resolved: Vec<&serde_json::Value> = orders["data"]["orders"]
        .as_array()
        .expect("the Orders are a list")
        .iter()
        .filter(|order| order["vendor"]["vendor_id"] == HIDDEN_VENDOR_ID)
        .collect();
    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0]["vendor"]["name"], "Nethermoor Optics",
        "the Order resolves the current Name rather than a snapshot"
    );
}

/// A committed mutation whose derived index refresh then fails stays committed, reports
/// the warning, and exits `0`.
///
/// **Driven by a real refresh failure**: a managed replacement stages beside its
/// destination and refuses an occupied staging name, so the index rebuild that follows the
/// canonical write cannot happen while the canonical write itself is untouched. The defect
/// this catches is a report that made a committed mutation look retry-shaped — a caller
/// that re-ran it would be creating a second Vendor, or editing one twice.
#[test]
fn a_failed_index_refresh_leaves_the_mutation_committed_at_exit_zero() {
    let (_tree, root) = initialized_package_with_a_blocked_index_refresh("vendor-index-warning");

    let outcome = Invocation::new(&["add-vendor", "Tarnwick", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["ok"], true);
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(document["data"]["report"]["changed"], true);
    assert_eq!(
        document["data"]["report"]["warnings"]
            .as_array()
            .map(Vec::as_slice)
            .map(|warnings| warnings.to_vec()),
        Some(vec![serde_json::Value::from("index_refresh_failed")])
    );
    assert_eq!(
        document["data"]["report"]["index"]["refresh"], "replacement_failed",
        "the derived half of the outcome says what did not happen"
    );
    assert_eq!(document["data"]["report"]["index"]["state"], "stale");

    // The canonical half happened, which is what "stays committed" means. Asked of a
    // different command, because a Vendor only this mutation can see is not committed.
    let read = response(
        &Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(read["ok"], true);
    assert_eq!(
        read["data"]["vendor_id"], document["data"]["vendor"]["vendor_id"],
        "and it is the Vendor the mutation reported"
    );
}

/// Every `--dry-run` reports the real run's outcome with `committed: false`, and leaves
/// the Package byte-identical.
///
/// **The same Package, planned and then mutated**, which is what makes the comparison mean
/// something: the plan is not a description of a hypothetical Vendor but of this one, and
/// the real run that follows proves the plan changed nothing. The refreshed Vendor is
/// compared member by member against the committed run's, so a fact the planned projection
/// forgot is a failure rather than a silence.
///
/// The byte-identical snapshot covers the staging file too: a managed write stages beside
/// its destination, so a dry run that had begun writing would leave one there.
#[test]
fn every_dry_run_reports_the_real_runs_outcome_and_changes_nothing() {
    for (label, command, outcome_code) in [
        (
            "dry-run-edit",
            vec!["vendor", "edit", "Tarnwick", "phone=+1 555 0100"],
            "edited",
        ),
        ("dry-run-hide", vec!["vendor", "hide", "Tarnwick"], "hidden"),
        (
            "dry-run-unhide",
            vec!["vendor", "unhide", "Nethermoor"],
            "unhidden",
        ),
    ] {
        let (_tree, root) = populated_package(label);
        let before = managed_snapshot(&root);

        let mut planned_run = Invocation::new(&command);
        planned_run = planned_run.argument("--json").argument("--dry-run");
        let planned = planned_run.rooted_at(&root).run();
        assert_eq!(
            planned.status,
            Some(0),
            "{label}: {}",
            planned.stderr_text()
        );
        let planned = response(&planned);

        assert_eq!(planned["data"]["report"]["committed"], false, "{label}");
        assert_eq!(
            planned["data"]["report"]["outcome"], outcome_code,
            "{label}"
        );
        assert_eq!(planned["data"]["report"]["changed"], true, "{label}");
        assert_eq!(
            planned["data"]["report"]["index"]["refresh"], "not_attempted",
            "{label}: a run that never committed refreshed nothing"
        );
        assert_eq!(
            planned["data"]["report"]["index"]["state"], "not_assessed",
            "{label}: and never opened the stored index, so it has no state to report"
        );
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{label}: no managed file, no staging artifact"
        );

        let committed = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        assert_eq!(
            committed.status,
            Some(0),
            "{label}: {}",
            committed.stderr_text()
        );
        let committed = response(&committed);

        assert_eq!(committed["data"]["report"]["committed"], true, "{label}");
        assert_eq!(
            planned["data"]["vendor"], committed["data"]["vendor"],
            "{label}: a plan reports the Vendor the commit produces"
        );
        for member in ["changed", "outcome", "operations", "warnings"] {
            assert_eq!(
                planned["data"]["report"][member], committed["data"]["report"][member],
                "{label}: `{member}` is the same question of a plan and of a commit"
            );
        }
    }
}

/// A `--dry-run` creation reports no Vendor ID, because none has been minted.
///
/// The one member a planned creation cannot fill. Reporting the planning placeholder would
/// hand a caller a Vendor ID that will never exist, which is worse than `null` in exactly
/// the way a plausible wrong answer is worse than no answer.
#[test]
fn a_planned_creation_reports_no_identity_and_creates_nothing() {
    let (_tree, root) = initialized_package("dry-run-create");
    let before = managed_snapshot(&root);

    let planned = Invocation::new(&["add-vendor", "Tarnwick", "--json", "--dry-run"])
        .rooted_at(&root)
        .run();
    assert_eq!(planned.status, Some(0), "{}", planned.stderr_text());
    let planned = response(&planned);

    assert_eq!(planned["data"]["report"]["committed"], false);
    assert_eq!(planned["data"]["report"]["outcome"], "created");
    assert_eq!(planned["data"]["vendor"]["name"], "Tarnwick");
    assert!(
        planned["data"]["vendor"]["vendor_id"].is_null(),
        "an identity is minted at the commit, and this run did not reach one"
    );
    assert_eq!(managed_snapshot(&root), before);

    let committed = response(
        &Invocation::new(&["add-vendor", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(committed["data"]["report"]["committed"], true);
    // The commit is what mints an identity, and this is the assertion that what it minted
    // is one — the same shape check the golden fixture's placeholder stands for.
    minted_uuid_v4(
        committed["data"]["vendor"]["vendor_id"]
            .as_str()
            .expect("the commit minted an identity"),
    );
}

/// A `--dry-run` over a refused mutation produces that refusal, byte for byte.
///
/// A plan is refused before the commit either way, so `--dry-run` is never a way to be
/// told a mutation would succeed when it would not.
#[test]
fn a_dry_run_over_a_refused_mutation_produces_that_refusal() {
    let (_tree, root) = populated_package("dry-run-refused");
    let before = managed_snapshot(&root);

    for command in [
        vec!["add-vendor", "tarnwick"],
        vec!["add-vendor", "colour=red"],
        vec!["vendor", "edit", "Copperwick", "phone=none"],
        vec!["vendor", "hide", "Copperwick"],
    ] {
        let planned = Invocation::new(&command)
            .argument("--json")
            .argument("--dry-run")
            .rooted_at(&root)
            .run();
        let refused = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();

        assert_eq!(planned.status, Some(1), "{command:?}");
        assert_eq!(refused.status, Some(1), "{command:?}");
        assert_eq!(
            planned.stdout_text(),
            refused.stdout_text(),
            "{command:?}: the refusal is the same document either way"
        );
        assert_eq!(managed_snapshot(&root), before, "{command:?}");
    }
}

/// A mutation of a Package the load cannot trust is refused with that Package's diagnosis.
///
/// The same code, message, and diagnostics a read of the same Package gives, because the
/// load states a Package can be in are one set. The defect this catches is a mutation that
/// reported "unreadable" for a Package `repair` could act on, or one that named no Package
/// at all.
#[test]
fn a_mutation_of_a_damaged_package_carries_its_diagnosis() {
    let (_tree, root) =
        crate::support::package_with_duplicate_vendor_names("vendor-mutate-damaged");

    for command in [
        vec!["add-vendor", "Nethermoor"],
        vec!["vendor", "edit", "Tarnwick", "phone=none"],
        vec!["vendor", "hide", "Tarnwick"],
        vec!["vendor", "unhide", "Tarnwick"],
    ] {
        let outcome = Invocation::new(&command)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{command:?}");
        assert_eq!(
            document["error"]["code"], "package_root_damaged",
            "{command:?}"
        );
        assert_eq!(
            document["context"]["package_root"],
            root.to_str().expect("the test root is UTF-8"),
            "{command:?}: the Root resolved, so the response names it"
        );
        assert_eq!(
            document["diagnostics"][0]["code"], "vendor_name_duplicate",
            "{command:?}: the diagnosis travels with the refusal"
        );
    }
}

/// A `--quiet` mutation says nothing and still does the whole thing.
#[test]
fn a_quiet_mutation_says_nothing_and_still_commits() {
    let (_tree, root) = initialized_package("mutation-quiet");

    let outcome = Invocation::new(&["add-vendor", "Tarnwick", "--quiet"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(outcome.stdout, b"", "no output on success");
    assert_eq!(outcome.stderr, b"");
    let read = response(
        &Invocation::new(&["vendor", "show", "Tarnwick", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(read["ok"], true);
}

/// The human result names the Vendor and the index outcome; `--verbose` adds the managed
/// operations, and a warning goes to standard error.
///
/// The decided split between what a mutation reports and what only `--verbose` shows.
/// Human output is not a parsing contract, so this is about which facts each mode carries
/// rather than about their wording.
#[test]
fn the_human_result_names_the_outcome_and_verbose_adds_the_operations() {
    let (_tree, root) = populated_package("mutation-human");
    let human = Invocation::new(&["vendor", "hide", "Tarnwick"])
        .rooted_at(&root)
        .run();

    assert_eq!(human.status, Some(0), "{}", human.stderr_text());
    let text = human.stdout_text();
    assert!(text.starts_with("Hidden: Tarnwick"), "{text}");
    assert!(text.contains("Index: "), "{text}");
    assert!(
        !text.contains("vendors.json"),
        "an individual managed-file operation is --verbose detail: {text}"
    );
    assert!(
        !text.contains(OFFERED_VENDOR_ID),
        "a UUID is --verbose detail: {text}"
    );

    let verbose = Invocation::new(&["vendor", "unhide", "Tarnwick", "--verbose"])
        .rooted_at(&root)
        .run();
    assert_eq!(verbose.status, Some(0), "{}", verbose.stderr_text());
    let text = verbose.stdout_text();
    assert!(text.contains("replace_vendor_record: "), "{text}");
    assert!(text.contains("refresh_package_index: "), "{text}");
    assert!(text.contains(OFFERED_VENDOR_ID), "{text}");

    // A committed mutation with a warning reports it on standard error and still succeeds.
    let (_warned_tree, warned_root) = package_with_a_blocked_index_refresh("mutation-human-warned");
    let warned = Invocation::new(&["vendor", "hide", "Tarnwick"])
        .rooted_at(&warned_root)
        .run();
    assert_eq!(warned.status, Some(0));
    assert!(warned.stdout_text().starts_with("Hidden: Tarnwick"));
    let reported = warned.stderr_text();
    assert!(reported.starts_with("warning: "), "{reported}");
    assert!(
        reported.contains("index rebuild"),
        "the warning points at the command that fixes it, not at running this one again: \
         {reported}"
    );
}

/// A dry run of a mutation whose plan changes nothing says so, and is not an error.
#[test]
fn a_planned_no_op_reports_that_it_would_change_nothing() {
    let (_tree, root) = populated_package("dry-run-no-op");

    let outcome = Invocation::new(&["vendor", "unhide", "Tarnwick", "--json", "--dry-run"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(document["data"]["report"]["committed"], false);
    assert_eq!(document["data"]["report"]["changed"], false);
    assert_eq!(
        document["data"]["report"]["operations"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "a plan that changes nothing announces no managed operation"
    );
}
