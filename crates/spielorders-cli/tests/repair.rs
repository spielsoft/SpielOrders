//! `repair` and `order repair`: acting on one diagnosed case.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries.

use crate::support::{Invocation, managed_snapshot, populated_package, response};

/// One diagnosed case, by the diagnostic code it answers.
fn case<'a>(document: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    document["data"]["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases are present and possibly empty: {document}"))
        .iter()
        .find(|case| case["code"] == code)
        .unwrap_or_else(|| panic!("{code} is diagnosed: {}", document["data"]["cases"]))
}

/// The exact command one case offers for one action: the `to apply:` line the human
/// diagnosis prints for the case `document` diagnosed, run as `arguments` at `root`.
fn suggested(
    document: &serde_json::Value,
    arguments: &[&str],
    root: &std::path::Path,
    code: &str,
    action: &str,
) -> String {
    let handle = case(document, code)["handle"]
        .as_str()
        .expect("a case names its handle")
        .to_owned();
    assert!(
        case(document, code)["actions"]
            .as_array()
            .expect("a case names the actions it offers")
            .iter()
            .any(|offered| offered["action"] == action),
        "{code} offers {action}"
    );
    let human = Invocation::new(arguments).rooted_at(root).run();
    human
        .stdout_text()
        .lines()
        .filter_map(|line| line.strip_prefix("to apply: "))
        .find(|command| {
            let words = shell_words(command);
            words.contains(&format!("case={handle}")) && words.contains(&format!("action={action}"))
        })
        .unwrap_or_else(|| {
            panic!(
                "a repair's offer names the exact command that applies it: {}",
                human.stdout_text()
            )
        })
        .to_owned()
}

/// Whether a diagnosis leaves its scope valid: no error-severity finding remains.
fn valid(diagnosis: &serde_json::Value) -> bool {
    !diagnosis["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("a diagnosis carries its findings: {diagnosis}"))
        .iter()
        .any(|finding| finding["severity"] == "error")
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
/// **The proof is the run.** The repair scopes each refusal names under `--json` are read
/// out of the response and the command each names — or the one it declines to name — is
/// executed, so the claim is checked against the executable rather than against the code
/// that composed the answer. A scope that answers nothing would be a false statement in a
/// machine response.
#[test]
fn a_response_names_a_repair_command_only_where_running_it_answers_with_a_case() {
    let package_scope = "package";
    let order_scope = "order";

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
            !message.contains(&package_scope) && !message.contains(&order_scope),
            "this build offers nothing for a record it cannot read: {message:?}"
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

/// The scope of every repair case a refusal carries under `--json`: `order` for a case
/// inside an Order Directory, `package` otherwise.
fn failure_message(document: &serde_json::Value) -> Vec<&'static str> {
    fn cases(value: &serde_json::Value, found: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(members) => {
                for (key, member) in members {
                    match (key.as_str(), member) {
                        ("cases", serde_json::Value::Array(entries)) => {
                            found.extend(entries.clone());
                        }
                        _ => cases(member, found),
                    }
                }
            }
            serde_json::Value::Array(entries) => {
                entries.iter().for_each(|entry| cases(entry, found));
            }
            _ => {}
        }
    }
    assert_eq!(document["ok"], false, "a refusal: {document}");
    let mut found = Vec::new();
    cases(&document["error"], &mut found);
    found
        .iter()
        .map(|case| {
            let location = case["path"].as_str().unwrap_or_default();
            if location.starts_with("Orders/") {
                "order"
            } else {
                "package"
            }
        })
        .collect()
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
    assert_eq!(
        diagnosed["ok"], true,
        "a repair asked about a damaged Order answers rather than failing"
    );
    assert!(
        !valid(&diagnosed["data"]),
        "a broken locator damages its Order, and the answer says so"
    );
    let command = suggested(
        &diagnosed,
        &["order", "repair"],
        &broken,
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
    assert_eq!(applied["data"]["committed"], true);
    let remaining = &applied["data"]["remaining"];
    assert!(valid(remaining), "{applied}");
    assert!(
        remaining["findings"]
            .as_array()
            .expect("the remaining diagnostics are the answer")
            .is_empty(),
        "the finding is gone: {applied}"
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
        &["order", "repair"],
        &damaged,
        "vendor_reference_unresolved",
        "assign_existing_vendor",
    );
    assert!(
        command.ends_with(" vendor="),
        "the offer names the assignment the caller must fill in: {command}"
    );

    let outcome = run_suggested(&command);
    let refused = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(refused["error"]["code"], "repair_resolution_missing");
    assert_eq!(managed_snapshot(&root), before, "nothing was half-applied");
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
        ("assign_existing_vendor", vec!["vendor=Tarnwick".to_owned()]),
        (
            "create_and_assign_vendor",
            vec!["name=Copperwick".to_owned()],
        ),
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
    let outcome = run_suggested_with(
        &suggested(&diagnosed, &["order", "repair"], directory, code, action),
        resolutions,
    );
    let applied = response(&outcome);

    assert_eq!(
        outcome.status,
        Some(0),
        "{action}: {}",
        outcome.stdout_text()
    );
    let remaining = &applied["data"]["remaining"];
    assert_eq!(applied["data"]["committed"], true, "{action}");
    assert!(
        valid(remaining),
        "{action} leaves this scope valid: {}",
        remaining["findings"]
    );
    assert!(
        remaining["findings"]
            .as_array()
            .expect("the remaining diagnostics are reported")
            .is_empty(),
        "{action}: {}",
        remaining["findings"]
    );
}

/// A dry run reports the whole plan and writes nothing.
#[test]
fn a_repair_dry_run_reports_the_plan_and_changes_nothing() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("repair-dry-run");
    let before = managed_snapshot(&root);

    let diagnosed = diagnose(&["repair", "--json"], &root);
    let handle = case(&diagnosed, "staging_artifact_present")["handle"]
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
    assert_eq!(planned["data"]["committed"], false);
    assert!(
        planned["data"]["operations"]
            .as_array()
            .is_some_and(|operations| !operations.is_empty()),
        "a plan names the operations it would perform: {planned}"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run creates no directory, staging file, or index rewrite"
    );
}

/// `order validate` names the repair offered for each finding in its own scope, in the
/// action vocabulary the repair commands take, and a present empty collection when there
/// is nothing to offer.
#[test]
fn an_order_validation_carries_the_offers_for_its_own_scope() {
    let (_tree, _root, warned) =
        crate::support::package_with_an_order_staging_artifact("order-validate-offers");

    let validated = diagnose(&["order", "validate", "--json"], &warned);

    assert!(
        outcome_status(&validated),
        "a stage beside a valid record is a warning, and severity alone decides"
    );
    let offer = &validated["data"]["cases"][0];
    assert_eq!(offer["code"], "staging_artifact_present");
    assert_eq!(offer["actions"][0]["action"], "promote_staging_candidate");

    let (_tree, root) = populated_package("order-validate-offers-none");
    let healthy = crate::support::order_directory(&root, "Laser_components");
    let clean = diagnose(&["order", "validate", "--json"], &healthy);
    assert_eq!(
        clean["data"]["cases"]
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

/// A case handle names the evidence it was diagnosed from, so a confirmation given before
/// that evidence changed applies to nothing and is refused.
#[test]
fn a_handle_given_before_its_evidence_changed_is_refused() {
    let (_tree, root) = crate::support::package_with_a_staging_artifact("repair-stale-handle");
    let diagnosed = diagnose(&["repair", "--json"], &root);
    let handle = case(&diagnosed, "staging_artifact_present")["handle"]
        .as_str()
        .expect("the case names its handle")
        .to_owned();
    std::fs::write(
        root.join(".spielorders/.vendors.json.spielorders-stage"),
        b"{\n  \"schema_version\": 1,\n  \"vendors\": [ ]\n}\n",
    )
    .expect("the staged candidate changes");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["repair", "--json"])
        .argument(format!("case={handle}"))
        .argument("action=promote_staging_candidate")
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(response(&outcome)["error"]["code"], "unknown_case");
    assert_eq!(managed_snapshot(&root), before, "nothing was promoted");
}

/// Validating a Package offers each case's repair as a command that runs: a case inside an
/// Order Directory is applied by `order repair` on that Order, not by the Package repair.
#[test]
fn a_validation_offers_a_runnable_repair_for_a_case_inside_an_order() {
    let (_tree, root) = crate::support::populated_package("validate-suggests-order-repair");
    let directory = crate::support::order_directory(&root, "Laser_components");
    std::fs::copy(
        directory.join(".spielorders/order.json"),
        directory.join(".spielorders/.order.json.spielorders-stage"),
    )
    .unwrap();
    let validated = Invocation::new(&["validate", "--json"])
        .rooted_at(&root)
        .run();
    let validated = response(&validated);
    let command = case(&validated, "staging_artifact_present")["actions"][0]["command"]
        .as_str()
        .expect("the action carries its command")
        .to_owned();

    let applied = run_suggested(&command);
    assert_eq!(
        applied.status,
        Some(0),
        "{command}: {}",
        applied.stdout_text()
    );
    assert!(
        !directory
            .join(".spielorders/.order.json.spielorders-stage")
            .exists()
    );
}
