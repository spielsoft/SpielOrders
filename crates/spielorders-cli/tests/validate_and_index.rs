//! `validate`, `order validate`, `index status`, and `index rebuild`, from real
//! processes against real Packages.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::{
    Invocation, Outcome, damage_index, directory_that_is_not_a_root, initialized_package,
    managed_snapshot, modification_time, order_directory, package_with_a_damaged_order,
    package_with_a_staging_artifact, package_with_an_impossible_date,
    package_with_an_overflowing_total, package_with_an_unreadable_order_readme,
    package_with_duplicate_vendor_names, populated_package, remove_index, response,
    reversion_index, touch_an_order_record, unmanaged_directory,
};
use serde_json::Value;
use std::path::Path;

/// Every diagnostic in one response, as `(scope, code)` pairs in the order it carries
/// them.
fn findings(document: &Value) -> Vec<(String, String)> {
    document["diagnostics"]
        .as_array()
        .expect("every response carries a present diagnostics collection")
        .iter()
        .map(|diagnostic| {
            (
                diagnostic["scope"].as_str().unwrap_or_default().to_owned(),
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

fn validate_order(directory: &Path) -> Outcome {
    Invocation::new(&["order", "validate", "--json"])
        .rooted_at(directory)
        .run()
}

fn index_status(root: &Path) -> Outcome {
    Invocation::new(&["index", "status", "--json"])
        .rooted_at(root)
        .run()
}

/// One run reports every problem it found, each under the narrowest scope that owns it.
///
/// **Not one problem at a time.** A Package with a Damaged Order, a loose file in its
/// Orders Directory, and an unreadable Order README has three unrelated findings at three
/// scopes, and a validation that stopped at the first would leave a user running it once
/// per problem.
#[test]
fn a_package_with_several_unrelated_problems_reports_all_of_them_in_one_run() {
    let (_tree, root, damaged) = package_with_a_damaged_order("validate-several");
    std::fs::write(root.join("Orders/loose.txt"), b"not an Order\n")
        .expect("a loose file stands in the Orders Directory");
    let healthy = order_directory(&root, "Laser_components");
    std::fs::remove_file(healthy.join("README.md")).expect("the plain Order README is removed");
    std::fs::create_dir(healthy.join("README.md")).expect("a directory takes its place");

    let outcome = validate(&root);

    assert_eq!(
        outcome.status,
        Some(1),
        "an error-severity finding is invalid"
    );
    let document = response(&outcome);
    let found = findings(&document);
    assert!(
        found.contains(&(
            "order".to_owned(),
            "managed_artifact_invalid_syntax".to_owned()
        )),
        "the Damaged Order is an Order-scope finding: {found:?}"
    );
    assert!(
        found.contains(&(
            "package".to_owned(),
            "orders_directory_entry_disallowed".to_owned()
        )),
        "the loose file is a Package-scope finding: {found:?}"
    );
    assert!(
        found.contains(&("document".to_owned(), "readme_unavailable".to_owned())),
        "the unreadable README is a Document-scope finding: {found:?}"
    );
    // Each finding names the place it is about, which is what makes the scope actionable.
    let damaged_record = damaged.join(".spielorders/order.json");
    assert!(
        document["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["primary_location"]["absolute_path"]
                == damaged_record.to_str().unwrap()),
        "the Order-scope finding names the record it read: {document}"
    );
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
    assert_eq!(document["data"]["scope"], "package");
    assert_eq!(
        document["data"]["orders"].as_array().map(Vec::len),
        Some(2),
        "and says what it examined, so a run with no findings is not a run over nothing"
    );

    let (_invalid_tree, invalid, _order) = package_with_an_impossible_date("validate-invalid");
    let outcome = validate(&invalid);
    assert_eq!(outcome.status, Some(1));
    let document = response(&outcome);
    assert_eq!(document["ok"], false);
    assert!(document["data"].is_null(), "a failed response carries none");
    assert_eq!(document["error"]["code"], "package_invalid");
    assert_eq!(
        findings(&document),
        [("order".to_owned(), "order_semantic_invalid".to_owned())],
        "the diagnostics are the answer: {document}"
    );
}

/// An invalid Package names `repair` only where `repair` would answer with a case.
///
/// The defect this catches is a false statement in a machine response: the message travels
/// in `error.message` under `--json`, and it told every reader to run `spielorders repair`
/// — which diagnoses a Package's own artifacts. A Package whose only actionable finding is
/// inside an Order is one that command answers no cases for, and a Package whose findings
/// this build offers nothing for has no repair to run at all.
#[test]
fn an_invalid_package_names_the_repair_scope_that_would_answer_with_a_case() {
    let package_scope = "`spielorders repair`";
    let order_scope = "`spielorders order repair`";

    // One Order-scoped actionable finding and nothing else.
    let (_tree, root, _order) = package_with_an_impossible_date("validate-suggest-order");
    let message = invalid_message(&root);
    assert!(
        message.contains(order_scope) && !message.contains(package_scope),
        "the actionable finding is inside an Order, and a Package-scoped `repair` would \
         answer no cases: {message}"
    );

    // The same Package with an actionable finding of its own beside it.
    let (_tree, root) = package_with_a_staging_artifact("validate-suggest-both");
    let record = order_directory(&root, "Mirror_mounts").join(".spielorders/order.json");
    let text = std::fs::read_to_string(&record).expect("the fixture record can be read");
    let spoiled = text.replace(
        "\"date_ordered\": \"2026-08-21\"",
        "\"date_ordered\": \"2026-02-30\"",
    );
    assert_ne!(text, spoiled, "the fixture record was spoiled");
    std::fs::write(&record, spoiled).expect("the spoiled record is written");
    let message = invalid_message(&root);
    assert!(
        message.contains(package_scope) && message.contains(order_scope),
        "both scopes hold a case, so both commands are named: {message}"
    );

    // A Package whose findings this build offers no action for names neither.
    let (_tree, root, _damaged) = package_with_a_damaged_order("validate-suggest-none");
    let message = invalid_message(&root);
    assert!(
        !message.contains(package_scope) && !message.contains(order_scope),
        "this build salvages no malformed record, and a suggestion to try is a false one: \
         {message}"
    );
}

/// The message an invalid Package is reported with, which travels under `--json`.
fn invalid_message(root: &Path) -> String {
    let document = response(&validate(root));
    assert_eq!(document["error"]["code"], "package_invalid", "{document}");
    document["error"]["message"]
        .as_str()
        .expect("a failure carries a human message")
        .to_owned()
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
    let [diagnostic] = document["diagnostics"].as_array().unwrap().as_slice() else {
        panic!("one finding: {document}")
    };
    assert_eq!(diagnostic["code"], "staging_artifact_present");
    assert_eq!(diagnostic["severity"], "warning");
    assert_eq!(
        diagnostic["evidence"]["destination_valid"], true,
        "the destination beside it is valid, which is what makes this cleanup"
    );
    assert!(
        diagnostic["evidence"]["candidate_digest"].is_string(),
        "and the candidate is identified, so a confirmation to promote it names one"
    );

    // The human surface reports the finding and the command that acts on it, which is
    // where the repair a warning offers is named.
    let human = Invocation::new(&["validate"]).rooted_at(&root).run();
    assert_eq!(human.status, Some(0));
    let reported = human.stderr_text();
    assert!(
        reported.contains("staging_artifact_present")
            && reported.contains("promote_staging_candidate"),
        "the warning names its code and the repair offered for it: {reported}"
    );
}

/// The command line one sentence suggests, taken out of the sentence.
///
/// The suggestion is what a reader copies, so a test that means to run it has to take the
/// same characters they would rather than compose its own from what it knows.
fn suggested_command(reported: &str) -> String {
    let (_, after) = reported.split_once("Run `").unwrap_or_else(|| {
        panic!("a finding with an offered repair suggests a command: {reported}")
    });
    let (command, _) = after
        .split_once('`')
        .expect("the suggested command is closed");
    command.to_owned()
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
/// **A suggestion that only works where it was printed is not a suggestion.** The
/// Order-scoped one beside it names its Order Directory and the machine surface names the
/// Root, so a bare `spielorders repair` was the odd one out — and an assertion that the
/// line merely *contains* the command name is satisfied by exactly that. So this prints
/// the finding from somewhere else, takes the sentence's own characters, and runs them
/// through a shell from somewhere else again.
#[test]
fn a_package_scoped_suggestion_acts_on_the_package_it_was_printed_about() {
    let (_tree, root) = package_with_a_staging_artifact("validate-suggestion-from-elsewhere");
    let elsewhere = std::env::temp_dir();

    let printed = Invocation::new(&["validate"])
        .rooted_at(&root)
        .in_directory(&elsewhere)
        .run();
    assert_eq!(printed.status, Some(0));
    let suggested = suggested_command(printed.stderr_text());

    let outcome = run_verbatim(&suggested, &elsewhere);

    assert_eq!(
        outcome.status,
        Some(0),
        "`{suggested}` run from {}: {}",
        elsewhere.display(),
        outcome.stderr_text()
    );
    assert!(
        outcome
            .stdout_text()
            .contains(&format!("Package Root: {}", root.display())),
        "`{suggested}` diagnoses the Package the finding was about: {}",
        outcome.stdout_text()
    );
}

/// A `--root` naming nothing is answered the way every other `order` command answers one.
///
/// The validating command reaches its Package by the Order Directory's own ancestors,
/// which is a different route from the locator the reads follow — and a route that
/// resolves the path first would report a path that does not exist as a Package it could
/// not read. The answer is the same code, the same scope, and the caller's own text.
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
            document["error"]["code"], "order_root_unmanaged",
            "{command:?}: {document}"
        );
        assert_eq!(document["error"]["scope"], "order", "{command:?}");
        assert_eq!(
            document["error"]["paths"][0]["spelling"], "as_given",
            "{command:?}: nothing resolved it, so it is quoted back as written"
        );
    }
}

/// A directory inside a Package that is none of its Orders is refused under the Root the
/// refusal spelled its path against.
///
/// **The spelling is the proof.** These two commands reach the Package by walking the
/// selected directory's own ancestors and then validating it, so by the time the
/// directory is judged against the Orders that load discovered, the Root is established —
/// and the refusal reports the Package's own relative spelling for the path. A `context`
/// naming no Root beside it hands a consumer a relative path and tells it there is
/// nothing to resolve it against.
#[test]
fn a_directory_a_package_holds_but_does_not_order_is_refused_under_that_package() {
    let (_tree, root) = populated_package("order-diagnosis-inside-a-package");
    let inside = directory_that_is_not_a_root(&root);

    for command in [vec!["order", "validate"], vec!["order", "repair"]] {
        let mut arguments = command.clone();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments).rooted_at(&inside).run();

        assert_eq!(outcome.status, Some(1), "{command:?}");
        let document = response(&outcome);
        assert_eq!(
            document["error"]["code"], "order_root_unmanaged",
            "{command:?}: {document}"
        );
        assert_eq!(
            document["error"]["paths"][0]["package_relative_path"], "not-a-root",
            "{command:?}: the Package that holds it is what spells it"
        );
        assert_eq!(
            document["context"]["package_root"].as_str(),
            root.to_str(),
            "{command:?}: the Root that spelling is relative to is named"
        );
        assert!(
            document["context"]["order_root"].is_null(),
            "{command:?}: an Order Directory is precisely what is absent"
        );
    }
}

/// `order validate` reports only the selected Order's scope, and never a sibling's
/// damage.
#[test]
fn an_order_scoped_validation_reports_no_siblings_damage() {
    let (_tree, root, damaged) = package_with_a_damaged_order("order-validate-sibling");
    let healthy = order_directory(&root, "Laser_components");

    let healthy_outcome = validate_order(&healthy);
    assert_eq!(healthy_outcome.status, Some(0));
    let document = response(&healthy_outcome);
    assert_eq!(document["ok"], true);
    assert_eq!(document["data"]["scope"], "order");
    assert_eq!(
        findings(&document),
        Vec::new(),
        "the sibling's damage is not this Order's: {document}"
    );
    assert_eq!(
        document["context"]["order_root"],
        healthy.to_str().unwrap(),
        "and the context names the Order the run was about"
    );

    let damaged_outcome = validate_order(&damaged);
    assert_eq!(damaged_outcome.status, Some(1));
    let document = response(&damaged_outcome);
    assert_eq!(document["error"]["code"], "order_invalid");
    assert_eq!(
        findings(&document),
        [(
            "order".to_owned(),
            "managed_artifact_invalid_syntax".to_owned()
        )],
        "and the damaged Order's own scope is its answer: {document}"
    );
}

/// A Package-level problem is reported at Package scope rather than as the selected
/// Order's.
///
/// An Order-scoped answer depends on Package-wide evidence — the Vendor aggregate is what
/// a Vendor Reference resolves against — so reporting "this Order looks fine" while the
/// Package cannot be trusted would be a downgrade rather than a diagnosis.
#[test]
fn an_order_scoped_validation_reports_a_package_level_problem_at_package_scope() {
    let (_tree, root) = package_with_duplicate_vendor_names("order-validate-package-damage");
    let directory = root.join("Orders/2026_08_22_Untargeted");
    std::fs::create_dir_all(directory.join(".spielorders"))
        .expect("an Order Directory stands in the Orders Directory");
    std::fs::write(
        directory.join(".spielorders/order.json"),
        b"{ not an Order record",
    )
    .expect("its record is written");

    let outcome = validate_order(&directory);

    assert_eq!(outcome.status, Some(1));
    let document = response(&outcome);
    assert_eq!(document["error"]["code"], "package_root_damaged");
    assert_eq!(document["error"]["scope"], "package");
    let found = findings(&document);
    assert!(
        found.iter().all(|(scope, _)| scope == "package")
            && found.contains(&("package".to_owned(), "vendor_name_duplicate".to_owned())),
        "the Package's own findings, and only those: {found:?}"
    );
}

/// One condition carries one code across `validate`, `order validate`, and
/// `package show`.
///
/// The three read the same core diagnostics, so this is a property of the core rather
/// than of an agreement three commands maintain — which is exactly what makes it worth
/// pinning: a surface that started composing its own codes would break here first.
#[test]
fn one_condition_carries_one_code_across_every_command_that_reports_it() {
    let (_tree, root, damaged) = package_with_an_impossible_date("shared-codes");

    let from_validate = findings(&response(&validate(&root)));
    let from_order = findings(&response(&validate_order(&damaged)));
    let from_show = findings(&response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    ));

    let expected = [("order".to_owned(), "order_semantic_invalid".to_owned())];
    assert_eq!(from_validate, expected);
    assert_eq!(from_order, expected);
    assert_eq!(from_show, expected);
}

/// A calculated total that is not finite makes its Order invalid under its own reason,
/// and names the total rather than a stored value.
#[test]
fn a_non_finite_calculated_total_invalidates_its_order_naming_the_total() {
    let (_tree, root, overflowing) = package_with_an_overflowing_total("validate-overflow");

    let outcome = validate(&root);

    assert_eq!(outcome.status, Some(1));
    let document = response(&outcome);
    let [diagnostic] = document["diagnostics"].as_array().unwrap().as_slice() else {
        panic!("one finding: {document}")
    };
    assert_eq!(diagnostic["code"], "order_semantic_invalid");
    assert_eq!(diagnostic["evidence"]["reason"], "non_finite_total");
    assert_eq!(
        diagnostic["evidence"]["field"], "item_total",
        "the calculated total is what is not finite; no stored value is at fault"
    );

    // And the Order is a Damaged Order rather than one with an unreportable figure.
    let scoped = validate_order(&overflowing);
    assert_eq!(scoped.status, Some(1));
    assert_eq!(
        response(&scoped)["diagnostics"][0]["evidence"]["reason"],
        "non_finite_total"
    );
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
            Vec::new(),
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
            document["data"]["stale_reasons"]
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

/// A malformed index reports why it is malformed, and a wrong-version one reports both
/// versions — the typed reasons behind the state rather than the state alone.
#[test]
fn a_malformed_or_wrong_version_index_reports_its_own_typed_reason() {
    let (_tree, malformed) = populated_package("index-status-malformed-reason");
    damage_index(&malformed);
    let document = response(&index_status(&malformed));
    assert_eq!(
        document["data"]["malformed_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reason| reason.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["invalid_syntax"]
    );

    let (_tree, wrong_version) = populated_package("index-status-wrong-version-reason");
    reversion_index(&wrong_version);
    let document = response(&index_status(&wrong_version));
    assert_eq!(document["data"]["observed_version"], 999);
    assert_eq!(document["data"]["expected_version"], 1);
}

/// A stale index is not reported as damage to canonical metadata.
#[test]
fn a_stale_index_is_not_reported_as_damage() {
    let (_tree, root) = populated_package("index-status-stale-only");
    touch_an_order_record(&root);

    let outcome = index_status(&root);

    assert_eq!(outcome.status, Some(0));
    let document = response(&outcome);
    assert_eq!(document["data"]["state"], "stale");
    assert_eq!(
        findings(&document),
        Vec::new(),
        "the derived cache being out of date says nothing about the records: {document}"
    );
    // And the Package itself still validates.
    assert_eq!(validate(&root).status, Some(0));
}

/// `index rebuild` is deterministic, omits a Damaged Order, and keeps it visible.
#[test]
fn a_rebuild_is_deterministic_and_omits_a_damaged_order_while_reporting_it() {
    let (_tree, root, _damaged) = package_with_a_damaged_order("index-rebuild-damaged");
    let index = root.join(".spielorders/index.json");

    let first = Invocation::new(&["index", "rebuild", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(first.status, Some(0));
    let document = response(&first);
    assert_eq!(document["data"]["report"]["committed"], true);
    assert_eq!(document["data"]["index"]["state"], "current");
    assert_eq!(
        document["data"]["report"]["indexed_orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|order| order["package_relative_path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Orders/2026_08_22_Tarnwick_Laser_components_Ada_Lovelace"],
        "the Damaged Order has no healthy record to project: {document}"
    );
    assert!(
        findings(&document)
            .iter()
            .any(|(scope, code)| scope == "order" && code == "managed_artifact_invalid_syntax"),
        "and stays visible diagnostically: {document}"
    );
    let rebuilt = std::fs::read(&index).expect("the rebuilt index is stored");

    let second = Invocation::new(&["index", "rebuild", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(second.status, Some(0));
    assert_eq!(
        response(&second)["data"]["report"]["changed"],
        false,
        "the same healthy Package rebuilds to the same bytes"
    );
    assert_eq!(std::fs::read(&index).unwrap(), rebuilt);
}

/// A dry run reports the same plan and leaves the index byte-identical.
#[test]
fn a_planned_rebuild_leaves_the_index_byte_identical() {
    let (_tree, root) = populated_package("index-rebuild-dry-run");
    touch_an_order_record(&root);
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["index", "rebuild", "--json", "--dry-run"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    let document = response(&outcome);
    assert_eq!(document["data"]["report"]["committed"], false);
    assert_eq!(
        document["data"]["index"]["state"], "stale",
        "a dry run leaves the index it found, so what it found is what it reports"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "and no managed byte moved: {document}"
    );
}

/// A rebuild whose replacement fails preserves the existing index and reports the
/// failure.
#[test]
fn a_rebuild_whose_replacement_fails_preserves_the_existing_index() {
    let (_tree, root) = populated_package("index-rebuild-blocked");
    touch_an_order_record(&root);
    // The managed replacement stages beside its destination, and a directory standing at
    // the staging name makes the staging write fail — a real filesystem refusal rather
    // than a core capability asked to misbehave.
    std::fs::create_dir(root.join(".spielorders/.index.json.spielorders-stage"))
        .expect("the index staging name is occupied");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["index", "rebuild", "--json"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(1));
    let document = response(&outcome);
    assert_eq!(document["error"]["code"], "index_replacement_failed");
    assert!(document["data"].is_null());
    assert_eq!(
        managed_snapshot(&root),
        before,
        "the stored index is preserved: {document}"
    );
}

/// Neither index command repairs canonical metadata or inspects Supporting Material.
#[test]
fn neither_index_command_touches_canonical_metadata_or_supporting_material() {
    let (_tree, root) = package_with_duplicate_vendor_names("index-canonical-damage");
    let before = managed_snapshot(&root);

    // A Package whose canonical records cannot be trusted has nothing to project, and
    // the refusal points at the diagnosis rather than at a rebuild.
    let status = index_status(&root);
    assert_eq!(status.status, Some(1));
    assert_eq!(response(&status)["error"]["code"], "package_root_damaged");
    let rebuild = Invocation::new(&["index", "rebuild", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(rebuild.status, Some(1));
    assert_eq!(response(&rebuild)["error"]["code"], "package_root_damaged");
    assert_eq!(
        managed_snapshot(&root),
        before,
        "and neither wrote a canonical byte"
    );

    // Supporting Material travels through a rebuild untouched. That it is not so much as
    // *observed* is pinned where the filesystem is substitutable and every effect is
    // recorded — `spielorders-core`'s `package_index_maintenance`; here the claim is
    // about the real bytes under a real Package.
    let (_healthy_tree, healthy) = populated_package("index-supporting-material");
    let order = order_directory(&healthy, "Laser_components");
    let material = order.join("Quotes/quote.txt");
    std::fs::create_dir_all(material.parent().unwrap()).expect("the subtree is created");
    std::fs::write(&material, b"Fiber, 3 metres.\n").expect("the opaque file is written");
    let before = managed_snapshot(&healthy);

    assert_eq!(
        Invocation::new(&["index", "rebuild"])
            .rooted_at(&healthy)
            .run()
            .status,
        Some(0)
    );

    let after = managed_snapshot(&healthy);
    assert_eq!(
        after
            .iter()
            .filter(|(path, _)| path != Path::new(".spielorders/index.json"))
            .collect::<Vec<_>>(),
        before
            .iter()
            .filter(|(path, _)| path != Path::new(".spielorders/index.json"))
            .collect::<Vec<_>>(),
        "the derived index is the only thing a rebuild replaces"
    );
}

/// An unmanaged directory is answered the way every other command answers one.
#[test]
fn every_validating_and_index_command_answers_an_unmanaged_directory_identically() {
    let (_tree, outside) = unmanaged_directory("validate-unmanaged");

    for command in [
        vec!["validate"],
        vec!["order", "validate"],
        vec!["index", "status"],
        vec!["index", "rebuild"],
    ] {
        let mut arguments = command.clone();
        arguments.push("--json");
        let outcome = Invocation::new(&arguments).rooted_at(&outside).run();
        assert_eq!(outcome.status, Some(1), "{command:?}");
        let document = response(&outcome);
        assert_eq!(
            document["error"]["code"], "package_root_unmanaged",
            "{command:?}: {document}"
        );
        assert!(
            document["context"]["package_root"].is_null(),
            "{command:?}: a resolved directory that is not a Root names no Package"
        );
    }
}

/// An empty Package validates, and says that it examined nothing.
#[test]
fn an_empty_package_validates_and_reports_examining_nothing() {
    let (_tree, root) = initialized_package("validate-empty");

    let outcome = validate(&root);

    assert_eq!(outcome.status, Some(0));
    let document = response(&outcome);
    assert_eq!(document["data"]["orders"].as_array().map(Vec::len), Some(0));
    assert_eq!(findings(&document), Vec::new());
}

/// An Order whose README exists and is not readable is a warning, not damage.
#[test]
fn an_unreadable_order_readme_leaves_its_order_valid() {
    let (_tree, _root, directory) =
        package_with_an_unreadable_order_readme("order-validate-readme");

    let outcome = validate_order(&directory);

    assert_eq!(outcome.status, Some(0));
    let document = response(&outcome);
    assert_eq!(document["ok"], true);
    assert_eq!(
        findings(&document),
        [("document".to_owned(), "readme_unavailable".to_owned())]
    );
}
