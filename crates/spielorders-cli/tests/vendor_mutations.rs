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
    Invocation, initialized_package, managed_snapshot, minted_uuid_v4, offered_vendor_id,
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
        assert_eq!(document["data"]["changes"][0]["kind"], "new_identity");
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
    assert_eq!(document["data"]["outcome"], "committed");
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

/// A `--dry-run` reports the plan the real run then commits, with `committed: false`, and
/// leaves the Package byte-identical.
///
/// **The same Package, planned and then mutated**, which is what makes the comparison mean
/// something: the plan is not a description of a hypothetical Vendor but of this one, and
/// the real run that follows proves the plan changed nothing.
///
/// The byte-identical snapshot covers the staging file too: a managed write stages beside
/// its destination, so a dry run that had begun writing would leave one there.
#[test]
fn a_dry_run_reports_the_plan_the_real_run_commits_and_changes_nothing() {
    let (_tree, root) = populated_package("dry-run-edit");
    let command = ["vendor", "edit", "Tarnwick", "phone=+1 555 0100"];
    let before = managed_snapshot(&root);

    let planned = Invocation::new(&command)
        .argument("--json")
        .argument("--dry-run")
        .rooted_at(&root)
        .run();
    assert_eq!(planned.status, Some(0), "{}", planned.stderr_text());
    let planned = response(&planned);

    assert_eq!(planned["data"]["committed"], false);
    assert_eq!(planned["data"]["outcome"], "planned");
    assert!(
        planned["data"]["changes"]
            .as_array()
            .is_some_and(|changes| !changes.is_empty()),
        "the plan changes something: {}",
        planned["data"]
    );
    assert!(
        planned["data"].get("vendor").is_none(),
        "a run that never committed refreshed nothing"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "no managed file, no staging artifact"
    );

    let committed = Invocation::new(&command)
        .argument("--json")
        .rooted_at(&root)
        .run();
    assert_eq!(committed.status, Some(0), "{}", committed.stderr_text());
    let committed = response(&committed);

    assert_eq!(committed["data"]["committed"], true);
    assert_eq!(committed["data"]["outcome"], "committed");
    for member in ["operations", "changes"] {
        assert_eq!(
            planned["data"][member], committed["data"][member],
            "the plan is the same question of a dry run and of a commit"
        );
    }
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
    assert!(text.contains("outcome: committed\n"), "{text}");
    assert!(text.contains("name: Tarnwick\n"), "{text}");
    assert!(
        !text.contains("vendors.json"),
        "an individual managed-file operation is --verbose detail: {text}"
    );
    assert!(
        !text.contains(&*offered_vendor_id(&root)),
        "a UUID is --verbose detail: {text}"
    );

    let verbose = Invocation::new(&["vendor", "unhide", "Tarnwick", "--verbose"])
        .rooted_at(&root)
        .run();
    assert_eq!(verbose.status, Some(0), "{}", verbose.stderr_text());
    let text = verbose.stdout_text();
    assert!(text.contains("kind: replace_vendor_record\n"), "{text}");
    assert!(text.contains("kind: refresh_package_index\n"), "{text}");
    assert!(text.contains(&*offered_vendor_id(&root)), "{text}");

    // A committed mutation with a warning reports it on standard error and still succeeds.
    let (_warned_tree, warned_root) = package_with_a_blocked_index_refresh("mutation-human-warned");
    let warned = Invocation::new(&["vendor", "hide", "Tarnwick"])
        .rooted_at(&warned_root)
        .run();
    assert_eq!(warned.status, Some(0));
    assert!(
        warned
            .stdout_text()
            .contains("outcome: committed_with_warning\n"),
        "{}",
        warned.stdout_text()
    );
    let reported = warned.stderr_text();
    assert!(
        reported.starts_with("warning: index refresh failed"),
        "{reported}"
    );
}

/// A Vendor deletion refuses while an entry of the Removed Orders Directory cannot be read,
/// naming that entry and saying it must be fixed or moved out, in JSON and in human output.
#[test]
fn an_unreadable_removed_entry_is_named_with_what_to_do() {
    let (_tree, root) = crate::support::populated_package("vendor-delete-junk-removed");
    let junk = root.join("Removed/Junk/.spielorders/order.json");
    std::fs::create_dir_all(&junk).unwrap();
    let delete = |mode: &[&str]| {
        let mut arguments = vec!["vendor", "delete", "Nethermoor", "--clear"];
        arguments.extend(mode);
        Invocation::new(&arguments).rooted_at(&root).run()
    };

    let refused = crate::support::response(&delete(&["--json"]));
    assert_eq!(refused["error"]["code"], "package_operation_failed");
    assert_eq!(
        refused["error"]["detail"]["path"],
        "Removed/Junk/.spielorders/order.json"
    );
    let human = delete(&[]);
    let human = human.stderr_text();
    assert!(
        human.contains("move it out of the Removed Orders Directory"),
        "{human}"
    );
    assert!(
        human.contains("Removed/Junk/.spielorders/order.json"),
        "{human}"
    );
}
