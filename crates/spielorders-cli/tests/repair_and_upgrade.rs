//! `repair`, `order repair`, and `upgrade`: acting on one diagnosed case, and reaching
//! the newest metadata version this build understands.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    Invocation, managed_snapshot, package_at_an_uninterpretable_version,
    package_with_a_damaged_order, populated_package, response,
};

// ---------------------------------------------------------------------------
// `upgrade`
// ---------------------------------------------------------------------------

/// A dry run names where the Package is, where this build would take it, and every
/// artifact it would visit in the order it would visit them — and writes nothing.
#[test]
fn an_upgrade_dry_run_reports_the_deterministic_artifact_order_and_changes_nothing() {
    let (_tree, root) = populated_package("upgrade-dry-run");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["upgrade", "--dry-run", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["contract"], "spielorders.upgrade");
    assert_eq!(document["data"]["source_version"], 1);
    assert_eq!(document["data"]["target_version"], 1);
    assert_eq!(document["data"]["complete"], true);

    let artifacts = document["data"]["artifacts"]
        .as_array()
        .expect("the plan reports every artifact it would visit")
        .clone();
    let (aggregate, records) = artifacts
        .split_last()
        .expect("a Package with Orders reports its records and its Vendor aggregate");
    assert_eq!(
        aggregate["artifact"], "vendor_aggregate",
        "the Vendor aggregate is visited last"
    );
    assert!(
        records
            .iter()
            .all(|artifact| artifact["artifact"] == "order_record"),
        "every artifact before it is an Order record: {records:?}"
    );
    assert!(
        artifacts
            .iter()
            .all(|artifact| artifact["state"] == "already_upgraded"),
        "a healthy Package at this build's version has nothing outstanding: {artifacts:?}"
    );
    assert!(
        document["data"]["blockers"]
            .as_array()
            .expect("blockers are present and possibly empty")
            .is_empty()
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run creates no directory, staging file, or index rewrite"
    );
}

/// The ordinary run over a Package that is already at the newest version this build
/// understands leaves every managed file, the marker included, exactly as it was.
///
/// This build knows one durable metadata version, so its registered chain is empty and
/// every Package it can read is already complete. The run still goes through the core's
/// own orchestration; what it must not do is rewrite anything to discover that.
#[test]
fn an_ordinary_upgrade_of_a_complete_package_leaves_it_and_its_marker_as_they_were() {
    let (_tree, root) = populated_package("upgrade-complete");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["upgrade", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["complete"], true);
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(
        document["data"]["report"]["changed"], false,
        "a Package with nothing outstanding is not moved by running the command"
    );
    assert_eq!(document["data"]["report"]["marker_advanced"], false);
    assert_eq!(document["data"]["report"]["reached_version"], 1);
    assert!(
        document["data"]["report"]["warnings"]
            .as_array()
            .expect("warnings are present and possibly empty")
            .is_empty()
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the Package and its marker are byte-identical"
    );
}

/// A marker naming a version this build does not interpret is reported as exactly that,
/// and the Package is left untouched.
#[test]
fn a_marker_naming_another_version_is_reported_rather_than_interpreted() {
    let (_tree, root) = package_at_an_uninterpretable_version("upgrade-other-version");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["upgrade", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "package_metadata_unsupported");
    assert!(
        document["error"]["message"]
            .as_str()
            .expect("a failure carries a human message")
            .contains("999"),
        "the version the Package declares is named: {}",
        document["error"]["message"]
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a Package this build cannot interpret is left exactly as it is"
    );
}

/// A record this build cannot read at the version the Package declares is reported among
/// the blockers rather than passed over as finished work.
#[test]
fn a_damaged_record_appears_among_the_blockers_rather_than_being_skipped() {
    let (_tree, root, damaged) = package_with_a_damaged_order("upgrade-blocked");

    let outcome = Invocation::new(&["upgrade", "--dry-run", "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    let blockers = document["data"]["blockers"]
        .as_array()
        .expect("blockers are present and possibly empty");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert_eq!(blockers[0]["state"], "blocked");
    assert_eq!(
        blockers[0]["location"]["absolute_path"],
        damaged
            .join(".spielorders/order.json")
            .to_str()
            .expect("the test path is UTF-8"),
        "the blocker names the record this build cannot read"
    );
    assert_eq!(
        blockers[0]["location"]["package_relative_path"],
        format!(
            "Orders/{}/.spielorders/order.json",
            damaged
                .file_name()
                .and_then(|name| name.to_str())
                .expect("the test directory is UTF-8")
        ),
        "and carries the spelling the Package has for it"
    );
    assert!(
        document["data"]["artifacts"]
            .as_array()
            .expect("every artifact is reported")
            .iter()
            .any(|artifact| artifact["state"] == "already_upgraded"),
        "one blocked record does not blank the rest of the plan"
    );
}

// ---------------------------------------------------------------------------
// `repair` and `order repair`
// ---------------------------------------------------------------------------

/// One diagnosed case, by the diagnostic code it answers.
fn case<'a>(document: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    document["data"]["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases are present and possibly empty: {document}"))
        .iter()
        .find(|case| case["code"] == code)
        .unwrap_or_else(|| panic!("{code} is diagnosed: {}", document["data"]["cases"]))
}

/// The exact command one case offers for one action.
fn suggested(document: &serde_json::Value, code: &str, action: &str) -> String {
    case(document, code)["actions"]
        .as_array()
        .expect("a case names the actions it offers")
        .iter()
        .find(|offered| offered["action"] == action)
        .unwrap_or_else(|| panic!("{code} offers {action}"))["command"]
        .as_str()
        .expect("a repair's offer names the exact command that applies it")
        .to_owned()
}

/// Splits a suggested command into arguments the way a POSIX shell would.
///
/// **A shell, not `split(' ')`.** The suggestion carries a Package Root, an Order
/// Directory, and a case handle derived from a Package-relative path, and every one of
/// those is a name somebody chose — so the producer quotes any that a shell would read as
/// more than one word. A test that split on spaces would make the same assumption the
/// producer used to make and could not catch it being wrong.
fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut characters = command.chars();
    while let Some(character) = characters.next() {
        match character {
            '\'' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if !quoted => {
                if let Some(escaped) = characters.next() {
                    current.push(escaped);
                    started = true;
                }
            }
            ' ' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            other => {
                current.push(other);
                started = true;
            }
        }
    }
    assert!(
        !quoted,
        "a suggested command closes every quote it opens: {command}"
    );
    if started {
        words.push(current);
    }
    words
}

/// Runs a suggested command exactly as it was reported, plus `--json`.
///
/// **The string itself is the subject**, so it is split into arguments and executed rather
/// than reassembled from the facts that composed it: a suggestion a caller cannot run is
/// not a suggestion.
fn run_suggested(command: &str) -> crate::support::Outcome {
    run_suggested_with(command, &[])
}

/// The same, with each owed value filled into the assignment the suggestion named empty.
///
/// **Filling in is what a caller does with a suggestion that owes a value**, and it is the
/// only way the reported string reaches the actions that need one — so those are asserted
/// against the same string as the rest rather than against a command line the test built.
/// The value is put into its own argument, because a complete Order record carries spaces
/// and newlines that splitting the string would shred.
fn run_suggested_with(command: &str, resolutions: &[String]) -> crate::support::Outcome {
    let mut tokens = shell_words(command).into_iter();
    assert_eq!(
        tokens.next().as_deref(),
        Some("spielorders"),
        "a suggested command names this executable"
    );
    let mut arguments: Vec<String> = tokens.collect();
    for resolution in resolutions {
        let (field, _) = resolution
            .split_once('=')
            .unwrap_or_else(|| panic!("a resolution is `field=value`: {resolution}"));
        let unfilled = format!("{field}=");
        let named = arguments
            .iter_mut()
            .find(|argument| **argument == unfilled)
            .unwrap_or_else(|| panic!("the suggestion names `{unfilled}` unfilled: {command}"));
        *named = resolution.clone();
    }
    let mut run = Invocation::new(&[]);
    for argument in arguments {
        run = run.argument(argument);
    }
    run.argument("--json").run()
}

fn diagnose(arguments: &[&str], root: &std::path::Path) -> serde_json::Value {
    response(&Invocation::new(arguments).rooted_at(root).run())
}

/// A response names a repair command only where running it answers with a case.
///
/// **The proof is the run.** Each message is read out of the response and the command it
/// names — or the one it declines to name — is executed, so the claim is checked against
/// the executable rather than against the code that composed the sentence. The message
/// travels in `error.message` under `--json`, which is what makes a command that answers
/// nothing a false statement in a machine response rather than awkward wording.
#[test]
fn a_response_names_a_repair_command_only_where_running_it_answers_with_a_case() {
    let package_scope = "`spielorders repair`";
    let order_scope = "`spielorders order repair`";

    // A Package whose Vendor aggregate this build cannot read. Nothing here salvages
    // malformed JSON, so neither scope has a case for it.
    let (_tree, root, order) =
        crate::support::package_with_an_impossible_date("repair-suggestion-run");
    std::fs::write(root.join(".spielorders/vendors.json"), b"{ not a record")
        .expect("the Vendor aggregate is damaged");

    for message in [
        // The Package's own read, refused because its contents are what it could not read.
        failure_message(&diagnose(&["package", "show", "--json"], &root)),
        // And one Order, which cannot be judged while the Package around it cannot be.
        failure_message(&diagnose(&["order", "validate", "--json"], &order)),
    ] {
        assert!(
            !message.contains(package_scope) && !message.contains(order_scope),
            "this build offers nothing for a record it cannot read: {message}"
        );
    }

    let refused = diagnose(&["repair", "--json"], &root);
    assert!(
        refused["data"]["cases"]
            .as_array()
            .expect("cases are present and possibly empty")
            .is_empty(),
        "and a run of it answers with nothing, which is what the messages promised: {refused}"
    );

    // The same Package read again with its Vendor aggregate whole, where the Order's own
    // finding does authorize an action.
    let (_tree, root, order) =
        crate::support::package_with_an_impossible_date("repair-suggestion-run-order");
    let message = failure_message(&diagnose(&["order", "show", "--json"], &order));
    assert!(
        message.contains(order_scope) && !message.contains(package_scope),
        "the finding is inside the Order and a Package-scoped `repair` answers no cases \
         for it: {message}"
    );

    let diagnosed = diagnose(&["order", "repair", "--json"], &order);
    assert!(
        !case(&diagnosed, "order_semantic_invalid")["actions"]
            .as_array()
            .expect("the case names its actions")
            .is_empty(),
        "and the command the message named answers with the case: {diagnosed}"
    );
    assert!(
        diagnose(&["repair", "--json"], &root)["data"]["cases"]
            .as_array()
            .expect("cases are present and possibly empty")
            .is_empty(),
        "while the command it did not name has nothing to do"
    );
}

/// The message a refusal carries, which is the member that travels under `--json`.
fn failure_message(document: &serde_json::Value) -> String {
    document["error"]["message"]
        .as_str()
        .expect("a failure carries a human message")
        .to_owned()
}

/// Each scope diagnoses only its own. A Package-level answer that reached into an Order
/// would act on a scope nobody named.
#[test]
fn neither_scope_reaches_into_the_others_cases() {
    let (_tree, root, damaged) =
        crate::support::package_with_an_unresolved_vendor_reference("repair-scopes");

    let package = diagnose(&["repair", "--json"], &root);
    assert_eq!(package["contract"], "spielorders.repair");
    assert_eq!(package["data"]["scope"], "package");
    assert!(
        package["data"]["cases"]
            .as_array()
            .expect("cases are present and possibly empty")
            .is_empty(),
        "the actionable case is inside an Order: {}",
        package["data"]["cases"]
    );
    assert_eq!(
        package["data"]["valid"], true,
        "`valid` is this scope's own: the Package's artifacts carry no error-severity \
         finding, while `validate` calls the same Package invalid for the Damaged Order \
         inside it"
    );
    assert_eq!(
        Invocation::new(&["validate", "--json"])
            .rooted_at(&root)
            .run()
            .status,
        Some(1),
        "the verdict on the whole Package is the validating command's, and it differs"
    );

    let order = diagnose(&["order", "repair", "--json"], &damaged);
    assert_eq!(order["contract"], "spielorders.order.repair");
    assert_eq!(order["data"]["scope"], "order");
    assert_eq!(
        case(&order, "vendor_reference_unresolved")["actions"]
            .as_array()
            .expect("the case names its actions")
            .iter()
            .map(|action| action["action"].clone())
            .collect::<Vec<_>>(),
        [
            "assign_existing_vendor",
            "create_and_assign_vendor",
            "clear_vendor_reference"
        ],
        "and the Order's own scope offers every action this core allows for it"
    );
}

/// A suggestion runs verbatim from a Package Root with a space in it.
///
/// **`~/Google Drive/…` is the ordinary case, not the exotic one.** An unquoted `--root`
/// bound to the first half of such a Root and left the second half as a stray argument, so
/// the one member this contract calls runnable verbatim was not — and the Order-scope form
/// carried the space twice, because the case handle is derived from a Package-relative
/// path.
#[test]
fn a_suggestion_runs_verbatim_from_a_root_whose_name_carries_a_space() {
    let (_tree, root, broken, _sibling) = crate::support::package_with_one_locator_rewritten(
        "repair locator spaced",
        Some("../../.."),
    );
    assert!(
        root.display().to_string().contains(' '),
        "this Package Root is the subject: {}",
        root.display()
    );

    let diagnosed = diagnose(&["order", "repair", "--json"], &broken);
    let command = suggested(
        &diagnosed,
        "order_package_locator_mismatch",
        "repair_package_locator",
    );
    let words = shell_words(&command);
    let rooted = words
        .iter()
        .position(|word| word == "--root")
        .expect("the suggestion names the root explicitly");
    assert_eq!(
        words[rooted + 1],
        broken.display().to_string(),
        "the Order Directory survives as one argument: {command}"
    );

    let outcome = run_suggested(&command);
    let applied = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(applied["data"]["report"]["committed"], true);
    assert_eq!(applied["data"]["valid"], true);
}

/// Every locator damage the integrity decision calls repairable by recalculation reaches
/// that repair.
///
/// The decision names four — missing, malformed, stale, and resolving to something that is
/// not this Package. Only the last two arrived: a record with no locator, or one whose
/// locator was not a relative parent path, failed the *whole record's* schema, so the Order
/// was diagnosed as an invalid managed artifact whose one offered action was authoring a
/// complete replacement by hand, and it sat at error severity until someone did. The
/// promise is one repair for four damages, so all four are asserted together through the
/// executable.
#[test]
fn every_decided_locator_damage_reaches_the_recalculation_repair() {
    for (damage, locator) in [
        ("missing", None),
        ("malformed: absolute", Some("/packages/optics")),
        ("malformed: not parent-only", Some("../Orders")),
        ("malformed: one segment", Some("..")),
        ("malformed: blank", Some("")),
        ("stale", Some("../../..")),
    ] {
        let (_tree, _root, broken, _sibling) = crate::support::package_with_one_locator_rewritten(
            &format!("repair-locator-{damage}"),
            locator,
        );
        assert_repaired_by_recalculation(damage, &broken);
    }

    // The fourth damage the decision names is its own fixture: the place the locator names
    // is a real Root, and it holds no such Order.
    let (_tree, _outer, _inner, directory) =
        crate::support::order_whose_locator_names_another_package("repair-locator-mismatched");
    assert_repaired_by_recalculation("resolving to another Package", &directory);
}

/// A record with no usable locator that is *also* invalid for another reason still fails
/// for that other reason.
///
/// The hazard in delivering the repair above is delivering it by making the record
/// permissive, which would be a worse defect than the one it fixes: every rule the record
/// already had still applies, and the locator is the only field whose damage the record
/// now carries instead of failing for.
#[test]
fn a_record_that_is_invalid_for_another_reason_still_fails_for_that_reason() {
    for (fault, from, to, expected) in [
        (
            "unknown field",
            "\"title\":",
            "\"invented\": \"x\",\n  \"title\":",
            "managed_artifact_invalid_shape",
        ),
        (
            "blank required text",
            "\"title\": \"Mirror mounts\"",
            "\"title\": \"   \"",
            "managed_artifact_invalid_shape",
        ),
        (
            "unreal calendar date",
            "\"date_ordered\": \"2026-08-21\"",
            "\"date_ordered\": \"2026-02-30\"",
            "order_semantic_invalid",
        ),
    ] {
        let (_tree, _root, broken, _sibling) = crate::support::package_with_one_locator_rewritten(
            &format!("locator-and-{fault}"),
            None,
        );
        let record = broken.join(".spielorders/order.json");
        let text = std::fs::read_to_string(&record).expect("the fixture record can be read");
        let spoiled = text.replace(from, to);
        assert_ne!(text, spoiled, "{fault}: the fixture record was spoiled");
        std::fs::write(&record, spoiled).expect("the spoiled record is written");

        let diagnosed = diagnose(&["order", "validate", "--json"], &broken);
        let codes = diagnosed["diagnostics"]
            .as_array()
            .expect("an Order's findings are the answer")
            .iter()
            .map(|diagnostic| diagnostic["code"].clone())
            .collect::<Vec<_>>();

        assert!(
            codes.iter().any(|code| code == expected),
            "{fault}: the other fault is what this record fails for: {codes:?}"
        );
        assert!(
            !codes
                .iter()
                .any(|code| code == "order_package_locator_mismatch"),
            "{fault}: and a record that never reached the locator check is not judged on it: \
             {codes:?}"
        );
    }
}

/// One damaged locator, diagnosed as a locator case and recalculated by the suggestion the
/// diagnosis printed.
fn assert_repaired_by_recalculation(damage: &str, directory: &std::path::Path) {
    let diagnosed = diagnose(&["order", "repair", "--json"], directory);
    assert_eq!(
        diagnosed["data"]["valid"], false,
        "{damage}: a damaged locator damages its Order"
    );
    let command = suggested(
        &diagnosed,
        "order_package_locator_mismatch",
        "repair_package_locator",
    );

    let outcome = run_suggested(&command);
    let applied = response(&outcome);
    assert_eq!(
        outcome.status,
        Some(0),
        "{damage}: {}",
        outcome.stdout_text()
    );
    assert_eq!(
        applied["data"]["report"]["action"], "repair_package_locator",
        "{damage}"
    );
    assert_eq!(applied["data"]["report"]["committed"], true, "{damage}");
    assert_eq!(
        applied["data"]["valid"], true,
        "{damage}: and the Order is valid afterwards"
    );

    // The recalculated locator is the route the Order is read by, so an ordinary read is
    // what proves the repair wrote a usable one.
    let read = Invocation::new(&["order", "show", "--json"])
        .rooted_at(directory)
        .run();
    assert_eq!(read.status, Some(0), "{damage}: {}", read.stdout_text());
}

/// The suggested command for an action that needs no supplied value runs verbatim and
/// applies its case, leaving the scope valid.
#[test]
fn a_suggested_command_runs_verbatim_and_leaves_the_scope_valid() {
    let (_tree, root, broken, _sibling) =
        crate::support::package_with_one_locator_rewritten("repair-locator", Some("../../.."));

    let diagnosed = diagnose(&["order", "repair", "--json"], &broken);
    assert_eq!(
        diagnosed["data"]["valid"], false,
        "a broken locator damages its Order, and the answer says so"
    );
    assert_eq!(
        diagnosed["ok"], true,
        "and a repair asked about a damaged Order answers rather than failing"
    );
    let command = suggested(
        &diagnosed,
        "order_package_locator_mismatch",
        "repair_package_locator",
    );

    let outcome = run_suggested(&command);
    let applied = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(applied["data"]["report"]["committed"], true);
    assert_eq!(
        applied["data"]["report"]["action"],
        "repair_package_locator"
    );
    assert_eq!(
        applied["data"]["valid"], true,
        "and the Order is valid afterwards"
    );
    assert!(
        applied["data"]["cases"]
            .as_array()
            .expect("the remaining cases are reported")
            .is_empty(),
        "the finding is gone: {}",
        applied["data"]["cases"]
    );
    assert!(
        applied["diagnostics"]
            .as_array()
            .expect("the remaining diagnostics are the answer")
            .is_empty()
    );

    // The Order loads ordinarily afterwards, which is the whole point of the locator.
    let read = Invocation::new(&["order", "show", "--json"])
        .rooted_at(&broken)
        .run();
    assert_eq!(read.status, Some(0), "{}", read.stdout_text());
    assert_eq!(
        response(&read)["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
}

/// The suggested command for an action that needs a value names the assignment unfilled,
/// so running it verbatim is refused with the typed error naming what is still owed —
/// rather than half-applying anything.
#[test]
fn a_suggested_command_that_still_needs_a_value_is_refused_rather_than_half_applied() {
    let (_tree, root, damaged) =
        crate::support::package_with_an_unresolved_vendor_reference("repair-missing-value");
    let before = managed_snapshot(&root);

    let diagnosed = diagnose(&["order", "repair", "--json"], &damaged);
    let command = suggested(
        &diagnosed,
        "vendor_reference_unresolved",
        "assign_existing_vendor",
    );
    assert!(
        command.ends_with(" vendor_id="),
        "the offer names the assignment the caller must fill in: {command}"
    );

    let outcome = run_suggested(&command);
    let refused = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(refused["error"]["code"], "repair_resolution_missing");
    assert_eq!(
        refused["error"]["identities"][0]["value"], "vendor_id",
        "and names the resolution still owed"
    );
    assert_eq!(managed_snapshot(&root), before, "nothing was half-applied");
}

/// An action the diagnosis did not offer is rejected.
#[test]
fn an_action_the_diagnosis_did_not_offer_is_rejected() {
    let (_tree, root, damaged) =
        crate::support::package_with_an_unresolved_vendor_reference("repair-unauthorized");
    let before = managed_snapshot(&root);

    let diagnosed = diagnose(&["order", "repair", "--json"], &damaged);
    let handle = case(&diagnosed, "vendor_reference_unresolved")["case"]
        .as_str()
        .expect("a case names its handle")
        .to_owned();

    let outcome = Invocation::new(&["order", "repair", "--json"])
        .argument(format!("case={handle}"))
        .argument("action=repair_package_locator")
        .rooted_at(&damaged)
        .run();
    let refused = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(refused["error"]["code"], "repair_action_unauthorized");
    assert_eq!(managed_snapshot(&root), before, "nothing was written");
}

/// A handle from a diagnosis taken before the Package changed no longer names a case, and
/// is refused rather than resolved against whatever stands there now.
#[test]
fn a_case_handle_from_a_stale_diagnosis_is_refused() {
    let (_tree, root, damaged) =
        crate::support::package_with_an_unresolved_vendor_reference("repair-stale-handle");

    let diagnosed = diagnose(&["order", "repair", "--json"], &damaged);
    let handle = case(&diagnosed, "vendor_reference_unresolved")["case"]
        .as_str()
        .expect("a case names its handle")
        .to_owned();

    // The condition is corrected another way, so the case the handle named is gone.
    let cleared = Invocation::new(&["order", "repair", "--json"])
        .argument(format!("case={handle}"))
        .argument("action=clear_vendor_reference")
        .rooted_at(&damaged)
        .run();
    assert_eq!(cleared.status, Some(0), "{}", cleared.stdout_text());
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["order", "repair", "--json"])
        .argument(format!("case={handle}"))
        .argument("action=clear_vendor_reference")
        .rooted_at(&damaged)
        .run();
    let refused = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(refused["error"]["code"], "repair_case_unknown");
    assert_eq!(managed_snapshot(&root), before, "nothing was written");
}

/// The complete Order record a confirmed replacement supplies for the fixture's damaged
/// Order: the record it has, with the day that does not exist corrected.
fn corrected_record(directory: &std::path::Path) -> String {
    let text = std::fs::read_to_string(directory.join(".spielorders/order.json"))
        .expect("the damaged Order record can be read");
    let corrected = text.replace(
        "\"date_ordered\": \"2026-02-30\"",
        "\"date_ordered\": \"2026-02-28\"",
    );
    assert_ne!(text, corrected, "the impossible date was found");
    corrected
}

/// Every repair this core is authorized to apply, exercised end to end.
///
/// One test because they are one claim: each names one case and one action, applies
/// exactly that, and leaves its scope valid with the remaining diagnostics reported. The
/// failure names the action.
#[test]
fn every_allowed_repair_is_applied_and_leaves_its_scope_valid() {
    // A confirmed replacement for a record that parses and does not validate.
    let (_tree, _root, damaged) =
        crate::support::package_with_an_impossible_date("repair-replace-record");
    let record = corrected_record(&damaged);
    apply_and_expect_valid(
        &damaged,
        "order_semantic_invalid",
        "replace_managed_record",
        &[format!("record={record}")],
    );

    // Promotion of the staging candidate an interrupted write left behind.
    let (_tree, _root, staged) =
        crate::support::package_with_an_order_staging_artifact("repair-promote");
    apply_and_expect_valid(
        &staged,
        "staging_artifact_present",
        "promote_staging_candidate",
        &[],
    );

    // The three explicit corrections of an Unresolved Vendor Reference.
    for (action, resolution) in [
        (
            "assign_existing_vendor",
            vec![format!("vendor_id={}", crate::support::OFFERED_VENDOR_ID)],
        ),
        ("create_and_assign_vendor", vec!["name=Copperwick".to_owned()]),
        ("clear_vendor_reference", Vec::new()),
    ] {
        let (_tree, _root, unresolved) =
            crate::support::package_with_an_unresolved_vendor_reference(&format!(
                "repair-{action}"
            ));
        apply_and_expect_valid(
            &unresolved,
            "vendor_reference_unresolved",
            action,
            &resolution,
        );
    }

    // A fresh identity for the chosen side of a duplicate-ID conflict.
    let (_tree, _root, conflicting) =
        crate::support::package_with_a_duplicate_order_id("repair-adopt-identity");
    apply_and_expect_valid(
        &conflicting,
        "order_id_duplicate",
        "adopt_new_order_identity",
        &[],
    );
}

/// Applies one action of the case answering `code`, and asserts the scope it leaves.
///
/// Through the command the diagnosis itself reported, so every allowed repair is exercised
/// against the string a caller is handed rather than one this test composed.
fn apply_and_expect_valid(
    directory: &std::path::Path,
    code: &str,
    action: &str,
    resolutions: &[String],
) {
    let diagnosed = diagnose(&["order", "repair", "--json"], directory);
    let handle = case(&diagnosed, code)["case"]
        .as_str()
        .unwrap_or_else(|| panic!("{code} names its handle"))
        .to_owned();

    let outcome = run_suggested_with(&suggested(&diagnosed, code, action), resolutions);
    let applied = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "{action}: {}",
        outcome.stdout_text()
    );
    assert_eq!(applied["data"]["report"]["committed"], true, "{action}");
    assert_eq!(applied["data"]["report"]["action"], action, "{action}");
    assert_eq!(applied["data"]["report"]["case"], handle, "{action}");
    assert_eq!(
        applied["data"]["valid"], true,
        "{action} leaves this scope valid: {}",
        applied["diagnostics"]
    );
    assert!(
        applied["diagnostics"]
            .as_array()
            .expect("the remaining diagnostics are reported")
            .is_empty(),
        "{action}: {}",
        applied["diagnostics"]
    );
}

/// The Package-level case the integrity decision makes actionable, end to end.
#[test]
fn a_package_level_case_is_applied_and_the_leftover_stage_is_consumed() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("repair-package-level");

    let diagnosed = diagnose(&["repair", "--json"], &root);
    assert_eq!(
        diagnosed["data"]["valid"], true,
        "a stage beside a valid destination is cleanup, not damage"
    );
    let command = suggested(
        &diagnosed,
        "staging_artifact_present",
        "promote_staging_candidate",
    );

    let outcome = run_suggested(&command);
    let applied = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(applied["data"]["scope"], "package");
    assert_eq!(applied["data"]["report"]["committed"], true);
    assert!(
        !root
            .join(".spielorders/.vendors.json.spielorders-stage")
            .exists(),
        "the leftover stage is consumed by completing the write it belonged to"
    );
    assert!(
        applied["data"]["cases"]
            .as_array()
            .expect("the remaining cases are reported")
            .is_empty()
    );
}

/// A dry run reports the whole plan and writes nothing.
#[test]
fn a_repair_dry_run_reports_the_plan_and_changes_nothing() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("repair-dry-run");
    let before = managed_snapshot(&root);

    let diagnosed = diagnose(&["repair", "--json"], &root);
    let handle = case(&diagnosed, "staging_artifact_present")["case"]
        .as_str()
        .expect("the case names its handle")
        .to_owned();

    let outcome = Invocation::new(&["repair", "--json", "--dry-run"])
        .argument(format!("case={handle}"))
        .argument("action=promote_staging_candidate")
        .rooted_at(&root)
        .run();
    let planned = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(planned["data"]["report"]["committed"], false);
    assert_eq!(
        planned["data"]["report"]["operations"]
            .as_array()
            .expect("a plan names its operations")
            .len(),
        2,
        "the promotion and the derived refresh that follows it"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run creates no directory, staging file, or index rewrite"
    );
}

// ---------------------------------------------------------------------------
// What the validating commands now carry beside their findings
// ---------------------------------------------------------------------------

/// A validating command names the repair offered for each finding that has one, in the
/// same action vocabulary the repair commands take — so a caller sees what may be done
/// without a second command, including for the valid Package whose warnings offer cleanup
/// and whose exit status deliberately says nothing about them.
#[test]
fn a_validating_command_carries_the_offered_repair_in_the_repair_vocabulary() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("validate-offers");

    let validated = diagnose(&["validate", "--json"], &root);
    assert_eq!(
        validated["ok"], true,
        "a stage beside a valid destination is a warning, and severity alone decides"
    );
    let offer = &validated["data"]["offered_repairs"][0];
    assert_eq!(offer["code"], "staging_artifact_present");
    assert_eq!(offer["scope"], "package");
    assert_eq!(
        offer["location"]["package_relative_path"], ".spielorders/.vendors.json.spielorders-stage",
        "the offer names its diagnostic by the place the envelope already carries"
    );
    assert_eq!(offer["actions"][0]["action"], "promote_staging_candidate");
    assert_eq!(
        offer["actions"][0]["required_values"]
            .as_array()
            .expect("required values are present and possibly empty")
            .len(),
        0
    );
    assert!(
        offer["actions"][0]["command"].is_null(),
        "a validation mints no ephemeral handle, so it names no command that applies one"
    );

    // The same collection, present and empty, for a Package with nothing to offer.
    let (_tree, healthy) = populated_package("validate-offers-none");
    let clean = diagnose(&["validate", "--json"], &healthy);
    assert_eq!(
        clean["data"]["offered_repairs"]
            .as_array()
            .map(Vec::len)
            .expect("the collection is present"),
        0
    );
}

/// The Order-scoped validation carries the same, for its own scope's findings.
#[test]
fn an_order_validation_carries_the_offers_for_its_own_scope() {
    let (_tree, _root, warned) =
        crate::support::package_with_an_order_staging_artifact("order-validate-offers");

    let validated = diagnose(&["order", "validate", "--json"], &warned);

    assert!(
        outcome_status(&validated),
        "a stage beside a valid record is a warning, and severity alone decides"
    );
    let offer = &validated["data"]["offered_repairs"][0];
    assert_eq!(offer["code"], "staging_artifact_present");
    assert_eq!(offer["scope"], "order");
    assert_eq!(offer["actions"][0]["action"], "promote_staging_candidate");
    assert!(
        offer["actions"][0]["command"].is_null(),
        "a validation mints no ephemeral handle, so it names no command that applies one"
    );

    let (_tree, _root, damaged) =
        crate::support::package_with_an_unresolved_vendor_reference("order-validate-invalid");
    let invalid = diagnose(&["order", "validate", "--json"], &damaged);
    assert!(!outcome_status(&invalid), "the Order is invalid");
    // An invalid scope is a failed response, so the offers travel with `repair`'s answer
    // rather than this one — which is exactly what the repair command is for.
    let offered = diagnose(&["order", "repair", "--json"], &damaged);
    assert_eq!(
        case(&offered, "vendor_reference_unresolved")["actions"][0]["action"],
        "assign_existing_vendor"
    );

    let (_tree, root) = populated_package("order-validate-offers-none");
    let healthy = crate::support::order_directory(&root, "Laser_components");
    let clean = diagnose(&["order", "validate", "--json"], &healthy);
    assert_eq!(
        clean["data"]["offered_repairs"]
            .as_array()
            .map(Vec::len)
            .expect("the collection is present"),
        0
    );
}

fn outcome_status(document: &serde_json::Value) -> bool {
    document["ok"].as_bool().expect("every response carries ok")
}

/// The human answer names the case, the action, and the command that applies it.
///
/// Human output is not a parsing contract, so this asserts what a reader is owed rather
/// than a format: the ephemeral handle, the action, and a command line to run.
#[test]
fn the_human_answer_names_the_case_and_the_command_that_applies_it() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("repair-human");

    let outcome = Invocation::new(&["repair"]).rooted_at(&root).run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    let text = outcome.stdout_text();
    assert!(text.contains("staging_artifact_present"), "{text}");
    assert!(text.contains("promote_staging_candidate"), "{text}");
    assert!(text.contains("spielorders repair --root"), "{text}");
}
