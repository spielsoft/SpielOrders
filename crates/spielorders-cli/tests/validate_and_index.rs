//! `validate`, `order validate`, `index status`, and `index rebuild`, from real
//! processes against real Packages.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    Invocation, Outcome, damage_index, managed_snapshot, modification_time,
    package_with_a_staging_artifact, package_with_an_impossible_date, populated_package,
    remove_index, response, reversion_index, touch_an_order_record,
};
use serde_json::Value;
use std::path::Path;

/// Every diagnostic one response carries, wherever it carries it: beside the answer, in
/// the core's value, or in the core's failure — in document order.
fn diagnostics(document: &Value) -> Vec<Value> {
    fn collect(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(members) => {
                for (key, member) in members {
                    match (key.as_str(), member) {
                        ("diagnostics", Value::Array(entries)) => found.extend(entries.clone()),
                        _ => collect(member, found),
                    }
                }
            }
            Value::Array(entries) => entries.iter().for_each(|entry| collect(entry, found)),
            _ => {}
        }
    }
    let mut found = document["diagnostics"]
        .as_array()
        .expect("every response carries a present diagnostics collection")
        .clone();
    collect(&document["data"], &mut found);
    collect(&document["error"], &mut found);
    found.extend(
        document["data"]["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned(),
    );
    found.extend(
        document["error"]["detail"]["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned(),
    );
    found
}

/// Every diagnostic in one response, as `(scope, code)` pairs in the order it carries
/// them.
fn findings(document: &Value) -> Vec<(String, String)> {
    diagnostics(document)
        .iter()
        .map(|diagnostic| {
            (
                diagnostic["path"].as_str().unwrap_or_default().to_owned(),
                diagnostic["code"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn validate(root: &Path) -> Outcome {
    Invocation::new(&["validate", "--json"])
        .rooted_at(root)
        .run()
}

fn index_status(root: &Path) -> Outcome {
    Invocation::new(&["index", "status", "--json"])
        .rooted_at(root)
        .run()
}

/// A healthy Package is valid at exit `0` with a present, empty diagnostics collection;
/// an error-severity finding is invalid at exit `1` and the diagnostics are the answer.
#[test]
fn validity_is_decided_by_severity_and_the_diagnostics_are_the_answer() {
    let (_healthy_tree, healthy) = populated_package("validate-healthy");
    let outcome = validate(&healthy);
    assert_eq!(outcome.status, Some(0));
    let document = response(&outcome);
    assert_eq!(document["ok"], true);
    assert_eq!(findings(&document), Vec::new());

    let (_invalid_tree, invalid, _order) = package_with_an_impossible_date("validate-invalid");
    let outcome = validate(&invalid);
    assert_eq!(outcome.status, Some(1));
    let document = response(&outcome);
    assert_eq!(document["ok"], false);
    assert!(document["data"].is_null(), "a failed response carries none");
    assert_eq!(document["error"]["code"], "package_invalid");
    let found = findings(&document);
    assert_eq!(found.len(), 1, "the diagnostics are the answer: {document}");
    assert!(found[0].0.starts_with("Orders/"), "{found:?}");
    assert_eq!(found[0].1, "order_semantic_invalid");
}

/// A Package whose only finding is a warning is valid, exits `0`, and reports the warning
/// and the repair it offers in full.
///
/// The owner ruling this pins: severity alone decides the status, and the availability of
/// a repair never does. A leftover staging artifact beside a valid destination is exactly
/// the case that used to have to exit both `0` and `1`.
#[test]
fn a_warning_leaves_the_package_valid_however_the_repair_it_offers_is_reported() {
    let (_tree, root) = package_with_a_staging_artifact("validate-warning");

    let outcome = validate(&root);

    assert_eq!(outcome.status, Some(0), "a warning does not invalidate");
    let document = response(&outcome);
    assert_eq!(document["ok"], true);
    let [diagnostic] = diagnostics(&document)
        .try_into()
        .unwrap_or_else(|found| panic!("one finding: {found:?}"));
    assert_eq!(diagnostic["code"], "staging_artifact_present");
    assert_eq!(diagnostic["severity"], "warning");
    assert_eq!(
        diagnostic["path"], ".spielorders/.vendors.json.spielorders-stage",
        "the candidate is identified, so a confirmation to promote it names one"
    );

    let [case] = document["data"]["cases"].as_array().unwrap().as_slice() else {
        panic!("one case: {document}")
    };
    assert_eq!(case["code"], "staging_artifact_present");
    assert_eq!(
        case["actions"],
        serde_json::json!([{"action": "promote_staging_candidate", "required_values": []}])
    );

    // The human surface reports the finding and the repair offered for it.
    let human = Invocation::new(&["validate"]).rooted_at(&root).run();
    assert_eq!(human.status, Some(0));
    let reported = human.stderr_text();
    assert!(
        reported.contains("warning: staging artifact present"),
        "the warning names its code: {reported}"
    );
    assert!(
        human
            .stdout_text()
            .contains("- action: promote_staging_candidate"),
        "and the repair offered for it: {}",
        human.stdout_text()
    );
}

/// The command line one diagnosis suggests, taken out of its `to apply:` line.
///
/// The suggestion is what a reader copies, so a test that means to run it has to take the
/// same characters they would rather than compose its own from what it knows.
fn suggested_command(reported: &str) -> String {
    reported
        .lines()
        .find_map(|line| line.strip_prefix("to apply: "))
        .unwrap_or_else(|| {
            panic!("a finding with an offered repair suggests a command: {reported}")
        })
        .to_owned()
}

/// One command line run exactly as it was printed, from a chosen working directory.
///
/// Through a POSIX shell, with `PATH` holding nothing but the directory this build's own
/// executable stands in, so the printed string is executed as written and resolves to the
/// binary under test rather than to anything the developer's shell happens to offer.
fn run_verbatim(command: &str, from: &Path) -> Outcome {
    let executable = Path::new(env!("CARGO_BIN_EXE_spielorders"));
    let directory = executable
        .parent()
        .expect("the executable is in a directory");

    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(from)
        .env("PATH", directory)
        .output()
        .expect("the suggested command runs");
    Outcome {
        status: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

/// A Package-scoped finding's suggested command acts on the Package it was printed about,
/// from whatever directory the reader happens to be standing in.
///
/// **A suggestion that only works where it was printed is not a suggestion.** An
/// assertion that the line merely *contains* the command name is satisfied by a bare
/// `spielorders repair`. So this prints the diagnosis from somewhere else, takes the
/// line's own characters, and runs them through a shell from somewhere else again.
#[test]
fn a_package_scoped_suggestion_acts_on_the_package_it_was_printed_about() {
    let (_tree, root) = package_with_a_staging_artifact("validate-suggestion-from-elsewhere");
    let elsewhere = std::env::temp_dir();

    let printed = Invocation::new(&["repair"])
        .rooted_at(&root)
        .in_directory(&elsewhere)
        .run();
    assert_eq!(printed.status, Some(0));
    let suggested = suggested_command(printed.stdout_text());

    let outcome = run_verbatim(&suggested, &elsewhere);

    assert_eq!(
        outcome.status,
        Some(0),
        "`{suggested}` run from {}: {}",
        elsewhere.display(),
        outcome.stderr_text()
    );
    assert!(
        outcome.stdout_text().contains("outcome: committed"),
        "`{suggested}` applies: {}",
        outcome.stdout_text()
    );
    let after = findings(&response(&validate(&root)));
    assert!(
        !after.contains(&("package".to_owned(), "staging_artifact_present".to_owned())),
        "and it acted on the Package the finding was about: {after:?}"
    );
}

/// A `--root` naming nothing is answered the way every other `order` command answers one.
///
/// The validating command reaches its Package by the Order Directory's own ancestors,
/// which is a different route from the locator the reads follow — and a route that
/// resolves the path first would report a path that does not exist as a Package it could
/// not read. The answer is the same code and the caller's own text.
#[test]
fn an_order_root_naming_nothing_is_answered_identically_by_every_order_command() {
    let (_tree, root) = populated_package("order-validate-missing-root");
    let missing = root.join("Orders/no-such-order");

    for command in [vec!["order", "show"], vec!["order", "validate"]] {
        let mut arguments = command.clone();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments).rooted_at(&missing).run();

        assert_eq!(outcome.status, Some(1), "{command:?}");
        let document = response(&outcome);
        assert_eq!(
            document["error"]["code"], "not_an_order_directory",
            "{command:?}: {document}"
        );
        assert_eq!(
            document["error"]["detail"]["path"].as_str(),
            missing.to_str(),
            "{command:?}: nothing resolved it, so it is quoted back as written: {document}"
        );
    }
}

/// `index status` distinguishes every state with its typed reasons, and writes nothing —
/// not a byte, and not a modification time.
#[test]
fn index_status_reports_every_state_and_writes_nothing() {
    for (label, prepare, state, reasons) in [
        (
            "current",
            None as Option<fn(&Path)>,
            "current",
            Vec::<&str>::new(),
        ),
        (
            "stale",
            Some(touch_an_order_record as fn(&Path)),
            "stale",
            vec!["order_metadata_changed"],
        ),
        (
            "missing",
            Some(remove_index as fn(&Path)),
            "missing",
            Vec::new(),
        ),
        (
            "malformed",
            Some(damage_index as fn(&Path)),
            "malformed",
            vec!["invalid_syntax"],
        ),
        (
            "wrong-version",
            Some(reversion_index as fn(&Path)),
            "wrong_version",
            Vec::new(),
        ),
    ] {
        let (_tree, root) = populated_package(&format!("index-status-{label}"));
        if let Some(prepare) = prepare {
            prepare(&root);
        }
        let before = managed_snapshot(&root);
        let index = root.join(".spielorders/index.json");
        let modified = modification_time(&index);

        let outcome = index_status(&root);

        assert_eq!(
            outcome.status,
            Some(0),
            "{label}: reporting a state succeeds"
        );
        let document = response(&outcome);
        assert_eq!(document["data"]["state"], state, "{label}: {document}");
        assert_eq!(
            document["data"]["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|reason| reason.as_str().unwrap())
                .collect::<Vec<_>>(),
            reasons,
            "{label}: the typed reasons are the answer: {document}"
        );
        assert_eq!(managed_snapshot(&root), before, "{label}: nothing written");
        assert_eq!(
            modification_time(&index),
            modified,
            "{label}: and the index was not even touched"
        );
    }
}
