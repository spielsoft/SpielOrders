//! The four output modes, the two streams, the three exit statuses, and color.
//!
//! These are the contract every later command inherits unchanged, so they are asserted
//! here from real processes rather than inside any one command's tests. A stream rule
//! that held for `package readme` because of how `package readme` happens to be written
//! is not a rule; these tests are about the executable.
//!
//! Decision sources: Define the Rust CLI capability contract.

use crate::support::{
    Invocation, MISSING_VENDOR_ID, directory_that_is_not_a_root, initialized_package,
    package_with_an_unresolved_vendor_reference, package_with_unreadable_readme, response,
};

/// Any escape sequence at all. Color is decoration, so its presence anywhere in a
/// redirected stream is the defect — which sequence it was does not matter.
fn has_escapes(bytes: &[u8]) -> bool {
    bytes.contains(&0x1b)
}

// ---------------------------------------------------------------------------
// The three exit statuses, each from a real process.
// ---------------------------------------------------------------------------

/// One test for all three, because the contract is that they are *distinct* and a
/// per-status test cannot see a collision. An internal return value cannot stand in
/// here: the status is what a shell reads.
#[test]
fn success_domain_failure_and_usage_error_carry_the_three_decided_statuses() {
    let (_tree, root) = initialized_package("exit-statuses");
    let outside = directory_that_is_not_a_root(&root);

    assert_eq!(
        Invocation::new(&["package", "readme"])
            .rooted_at(&root)
            .run()
            .status,
        Some(0),
        "the command did what was asked"
    );
    assert_eq!(
        Invocation::new(&["package", "readme"])
            .rooted_at(&outside)
            .run()
            .status,
        Some(1),
        "a domain failure"
    );
    assert_eq!(
        Invocation::new(&["package", "readme", "--jsom"])
            .rooted_at(&root)
            .run()
            .status,
        Some(2),
        "the command line itself was wrong"
    );
}

/// Reference text is an answer, not a failure. A `--help` that exited `2` would make
/// every script that offers help to its own user fail.
#[test]
fn generated_reference_text_answers_on_standard_output_at_success() {
    for flag in ["--help", "--version"] {
        let outcome = Invocation::new(&[flag]).run();

        assert_eq!(outcome.status, Some(0), "{flag}");
        assert!(!outcome.stdout.is_empty(), "{flag} writes the reference");
        assert_eq!(
            outcome.stderr, b"",
            "{flag} is an answer, so nothing goes to standard error"
        );
    }
}

/// An unknown subcommand is a usage error like any other, so a family that exists is
/// not a licence for a command that does not.
#[test]
fn an_unknown_command_in_a_known_family_is_a_usage_error() {
    let outcome = Invocation::new(&["package", "reamde"]).run();

    assert_eq!(outcome.status, Some(2));
    assert_eq!(
        outcome.stdout, b"",
        "a usage error writes nothing to stdout"
    );
    assert!(!outcome.stderr.is_empty(), "and reports on standard error");
}

// ---------------------------------------------------------------------------
// The command path comes first and its modifiers follow it.
// ---------------------------------------------------------------------------

/// **Nothing precedes the command path except `--help` and `--version`.**
///
/// This is the grammar rule the exclusions below depend on. While an option stood in any
/// gap between the command path's segments, a pair written on either side of a segment
/// reached the parser as two sets of matches and no declared conflict ever saw them
/// together — so an exclusion had to be decided after parsing, over the whole command
/// line, and three defects were that one cause. Each option is now declared on the
/// command it modifies, which puts the whole command line in one parse level and leaves
/// the grammar to the parser.
///
/// Each line runs inside the prepared Package rather than naming it, so a grammar that
/// regressed would answer about that Package rather than about the repository this test
/// is checked out in.
#[test]
fn an_option_written_before_the_command_path_is_a_usage_error() {
    let (_tree, root) = initialized_package("modifiers-follow-the-command");

    for line in [
        vec!["--json", "package", "readme"],
        vec!["package", "--json", "readme"],
        vec!["--verbose", "package", "list", "orders"],
        vec!["--uuid", MISSING_VENDOR_ID, "order", "show"],
        vec!["order", "--uuid", MISSING_VENDOR_ID, "show"],
        vec!["--dry-run", "order", "align"],
        vec!["--case-sensitive", "search", "orders", "laser"],
        vec!["search", "--case-sensitive", "orders", "laser"],
        vec!["--all", "search", "vendors", "tarnwick"],
    ] {
        // The one document a usage error carries is how its identity is read, and it is
        // asked for where the grammar now allows it: after the command path.
        let mut written = line.clone();
        written.push("--json");
        let outcome = Invocation::new(&written).in_directory(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{line:?}: the command line never named a command: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["contract"], "spielorders.usage",
            "{line:?}: no command ran, so no command's contract is claimed"
        );
        assert_eq!(document["error"]["code"], "usage_invalid", "{line:?}");
    }
}

// ---------------------------------------------------------------------------
// The four modes, and the audience a usage error is reported to.
// ---------------------------------------------------------------------------

/// Silence is what a caller asked for.
///
/// The mutual exclusion between the modes is the parser's, asserted pair by pair beside
/// the group that declares it. This is the half of it that cost a caller something: while
/// two modes could both be named, one of them won, and the caller who had asked for
/// silence was the one who lost.
#[test]
fn quiet_is_silent_on_success() {
    let (_tree, root) = initialized_package("quiet-on-success");

    let outcome = Invocation::new(&["package", "readme", "--quiet"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(outcome.stdout, b"", "nothing is said on success");
    assert_eq!(outcome.stderr, b"");
}

/// **A usage error is reported to whichever audience the command line named.** Parsing
/// failed, so no mode was legitimately selected and the executable has to choose. It
/// errs toward the machine: a caller that wrote `--json` anywhere gets the one response
/// document, and a caller that did not gets generated human help on standard error.
///
/// This is the rule for *every* usage error, including a conflict between `--json` and
/// another mode — which is the case where "err toward the machine" is a real decision
/// rather than an obvious one, because `--quiet` was also named. It is stated here
/// because a script is worse served by help text it cannot parse than a human is by a
/// JSON document they can still read.
#[test]
fn a_usage_error_is_reported_to_the_audience_the_command_line_named() {
    let (_tree, root) = initialized_package("modes-exclusive-audience");

    let machine = Invocation::new(&["package", "readme", "--json", "--quiet"])
        .rooted_at(&root)
        .run();
    let document = response(&machine);
    assert_eq!(machine.status, Some(2));
    assert_eq!(document["error"]["code"], "usage_invalid");
    let message = document["error"]["message"]
        .as_str()
        .expect("a usage error carries its message");
    assert!(
        message.contains("--json") && message.contains("--quiet"),
        "the document names both modes: {message}"
    );

    let human = Invocation::new(&["package", "readme", "--quiet", "--verbose"])
        .rooted_at(&root)
        .run();
    assert_eq!(human.status, Some(2));
    assert_eq!(
        human.stdout, b"",
        "no `--json` was named, so standard output carries nothing at all"
    );
    let reported = human.stderr_text();
    assert!(
        reported.contains("--quiet") && reported.contains("--verbose"),
        "the generated help names both modes: {reported}"
    );
}

// ---------------------------------------------------------------------------
// Stream discipline, mode by mode.
// ---------------------------------------------------------------------------

#[test]
fn the_result_goes_to_standard_output_and_nothing_else_does() {
    let (_tree, root) = initialized_package("streams-human-success");

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&root)
        .run();

    assert_eq!(
        outcome.stdout_text(),
        format!("{}\n", root.join("README.md").display())
    );
    assert_eq!(outcome.stderr, b"");
}

#[test]
fn a_failure_goes_to_standard_error_and_leaves_standard_output_empty() {
    let (_tree, root) = initialized_package("streams-human-failure");
    let outside = directory_that_is_not_a_root(&root);

    let outcome = Invocation::new(&["package", "readme"])
        .rooted_at(&outside)
        .run();

    assert_eq!(
        outcome.stdout, b"",
        "a caller composing `$(...)` must not receive an error as the result"
    );
    assert!(outcome.stderr_text().starts_with("error: "));
}

/// `--quiet` says nothing on success. Its whole use is a status check, so a line of
/// output would defeat it — and a *silenced failure* would defeat the caller.
#[test]
fn quiet_says_nothing_on_success_and_still_reports_a_failure() {
    let (_tree, root) = initialized_package("streams-quiet");
    let outside = directory_that_is_not_a_root(&root);

    let success = Invocation::new(&["package", "readme", "--quiet"])
        .rooted_at(&root)
        .run();
    assert_eq!(success.status, Some(0));
    assert_eq!(success.stdout, b"", "no output on success");
    assert_eq!(success.stderr, b"", "and no warning either");

    let failure = Invocation::new(&["package", "readme", "--quiet"])
        .rooted_at(&outside)
        .run();
    assert_eq!(failure.status, Some(1));
    assert_eq!(failure.stdout, b"");
    assert!(
        failure.stderr_text().contains("is not a SpielOrders Root"),
        "a failure is reported even under --quiet: {}",
        failure.stderr_text()
    );
}

/// `--verbose` adds detail to the result on standard output. The first line stays the
/// result itself, so `--verbose` remains composable with `head -1`.
#[test]
fn verbose_adds_detail_to_the_result_on_standard_output() {
    let (_tree, root) = initialized_package("streams-verbose");

    let outcome = Invocation::new(&["package", "readme", "--verbose"])
        .rooted_at(&root)
        .run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(outcome.stderr, b"");
    let mut lines = outcome.stdout_text().lines();
    assert_eq!(
        lines.next(),
        Some(root.join("README.md").display().to_string().as_str()),
        "the result is still the first line"
    );
    let detail: Vec<&str> = lines.collect();
    assert!(
        detail.iter().any(|line| line.starts_with("Package Root: ")),
        "{detail:?}"
    );
    assert!(
        detail.iter().any(|line| line == &"Exists: yes"),
        "{detail:?}"
    );
}

// ---------------------------------------------------------------------------
// `--json`: exactly one document, on success and on failure.
// ---------------------------------------------------------------------------

/// The `response` helper in `support` is what enforces "exactly one UTF-8 document and
/// nothing else" — it refuses a second document, trailing text, or anything at all on
/// standard error. This test is here so the claim is made on both sides of `ok`.
#[test]
fn json_writes_exactly_one_document_on_success_and_on_failure() {
    let (_tree, root) = initialized_package("json-one-document");
    let outside = directory_that_is_not_a_root(&root);

    let success = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(success.status, Some(0));
    assert_eq!(response(&success)["ok"], true);

    let failure = Invocation::new(&["package", "readme", "--json"])
        .rooted_at(&outside)
        .run();
    assert_eq!(failure.status, Some(1));
    assert_eq!(response(&failure)["ok"], false);
}

/// A Package whose README exists and one whose README does not are the same answer
/// except for one fact, and that fact is only in the document. A command that printed
/// nothing, or a different path, for an absent README would break the way the document
/// gets created.
#[test]
fn existence_is_the_only_difference_an_absent_readme_makes() {
    let (_tree, present_root) = initialized_package("json-readme-present");
    let (_absent_tree, absent_root) = initialized_package("json-readme-absent");
    std::fs::remove_file(absent_root.join("README.md")).expect("the optional README is removed");

    let present = response(
        &Invocation::new(&["package", "readme", "--json"])
            .rooted_at(&present_root)
            .run(),
    );
    let absent = response(
        &Invocation::new(&["package", "readme", "--json"])
            .rooted_at(&absent_root)
            .run(),
    );

    assert_eq!(present["data"]["exists"], true);
    assert_eq!(absent["data"]["exists"], false);
    assert_eq!(
        present["data"]["location"]["package_relative_path"],
        absent["data"]["location"]["package_relative_path"],
        "the conventional path does not depend on the document being there"
    );
    assert_eq!(
        present["data"]["location"]["package_relative_path"],
        "README.md"
    );
}

/// A usage error under `--json` is a response document. Generated human help on a
/// stream promised to carry one JSON document is exactly the contamination the contract
/// forbids, and it is what a naive implementation does: the parser fails before any
/// mode is known, so the help text goes out by default.
#[test]
fn a_usage_error_under_json_is_a_document_rather_than_generated_help() {
    let outcome = Invocation::new(&["package", "readme", "--json", "--jsom"]).run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(2));
    assert_eq!(document["ok"], false);
    assert!(document["data"].is_null());
    assert_eq!(document["error"]["code"], "usage_invalid");
    assert_eq!(document["error"]["scope"], "usage");
    assert_eq!(document["error"]["message"], "unrecognized option `--jsom`");
    assert!(
        document["context"]["package_root"].is_null(),
        "nothing about the subject was attempted"
    );

    let message = document["error"]["message"]
        .as_str()
        .expect("the message is a string");
    assert!(!message.contains("Usage:"), "{message}");
    assert!(!message.contains("--help"), "{message}");
}

/// `--help` is reference text in every mode. Answering a request for the reference with
/// a JSON document would be answering a different question.
#[test]
fn help_stays_reference_text_even_when_json_was_asked_for() {
    let outcome = Invocation::new(&["package", "readme", "--json", "--help"]).run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(outcome.stderr, b"");
    assert!(
        outcome.stdout_text().contains("Usage:"),
        "the reference is the generated help: {}",
        outcome.stdout_text()
    );
}

// ---------------------------------------------------------------------------
// Color.
// ---------------------------------------------------------------------------

/// Both streams are pipes for every test in this suite, so this is the redirected case
/// and it covers every mode at once. The terminal branch of the policy is exercised as
/// a unit test in `src/output.rs`, where it needs no terminal — a process test cannot
/// reach it without allocating one.
///
/// The parser is deliberately told to style its own text unconditionally, so this also
/// catches the real defect: reference text and usage errors escaping this crate's color
/// policy because the parser has its own.
#[test]
fn a_redirected_stream_receives_no_color_in_any_mode() {
    let (_tree, root) = initialized_package("color-redirected");
    let outside = directory_that_is_not_a_root(&root);

    let runs = [
        Invocation::new(&["package", "readme"]).rooted_at(&root),
        Invocation::new(&["package", "readme", "--verbose"]).rooted_at(&root),
        Invocation::new(&["package", "readme", "--json"]).rooted_at(&root),
        Invocation::new(&["package", "readme"]).rooted_at(&outside),
        Invocation::new(&["package", "readme", "--json"]).rooted_at(&outside),
        Invocation::new(&["package", "readme", "--jsom"]),
        Invocation::new(&["package", "readme", "--json", "--jsom"]),
        Invocation::new(&["--help"]),
        Invocation::new(&["--version"]),
    ];
    for (index, run) in runs.into_iter().enumerate() {
        let outcome = run.run();
        assert!(!has_escapes(&outcome.stdout), "run {index}: stdout");
        assert!(!has_escapes(&outcome.stderr), "run {index}: stderr");
    }
}

/// `NO_COLOR` withdraws color independently of the stream. Both are pipes here, so this
/// asserts what a process test can: setting it changes nothing that was already plain,
/// and in particular does not make the executable fail or fall silent.
#[test]
fn a_no_color_request_is_honored_without_changing_the_message() {
    let (_tree, root) = initialized_package("color-no-color");

    let plain = Invocation::new(&["package", "readme"])
        .rooted_at(&root)
        .run();
    let requested = Invocation::new(&["package", "readme"])
        .rooted_at(&root)
        .with_environment("NO_COLOR", "1")
        .run();

    assert_eq!(requested.status, Some(0));
    assert!(!has_escapes(&requested.stdout));
    assert_eq!(
        requested.stdout, plain.stdout,
        "color is decoration, so withdrawing it changes no byte of the result"
    );
}

/// No message needs color to be understood: the label carries the meaning. A message
/// whose severity was only in its color would be unreadable in a log file, which is
/// where these messages mostly end up.
#[test]
fn every_message_states_its_severity_in_words() {
    let (_tree, root) = initialized_package("color-carries-no-meaning");
    let outside = directory_that_is_not_a_root(&root);

    let failure = Invocation::new(&["package", "readme"])
        .rooted_at(&outside)
        .run();
    assert!(
        failure.stderr_text().starts_with("error: "),
        "the severity is a word: {}",
        failure.stderr_text()
    );

    // A README that exists but cannot be read as a document is the warning this
    // command can produce, and it is a warning rather than a failure: the conventional
    // path is still the answer.
    let (_warning_tree, warning_root) = package_with_unreadable_readme("color-warning-severity");

    let warned = Invocation::new(&["package", "readme"])
        .rooted_at(&warning_root)
        .run();
    assert_eq!(warned.status, Some(0), "the path is still the answer");
    assert!(
        warned.stderr_text().starts_with("warning: "),
        "the severity is a word: {}",
        warned.stderr_text()
    );
}

/// A finding's domain UUID reaches the human surface only under `--verbose`.
///
/// The decided human output reserves UUIDs for `--verbose`, and the same decision requires
/// every diagnostic to carry an explanation — so the default sentence says what is wrong
/// and names the record it read, and `--verbose` is what adds which identity. The machine
/// surface carries the identity in every mode, which is where a consumer branches on one.
#[test]
fn a_findings_domain_identity_reaches_a_human_only_under_verbose() {
    let (_tree, root, _damaged) = package_with_an_unresolved_vendor_reference("verbose-identity");

    for mode in [vec!["validate"], vec!["validate", "--quiet"]] {
        let outcome = Invocation::new(&mode).rooted_at(&root).run();
        let reported = outcome.stderr_text();
        assert!(
            reported.contains("vendor_reference_unresolved"),
            "{mode:?}: the finding is still reported: {reported}"
        );
        assert!(
            !reported.contains(MISSING_VENDOR_ID),
            "{mode:?}: a UUID is `--verbose`'s to show: {reported}"
        );
    }

    let verbose = Invocation::new(&["validate", "--verbose"])
        .rooted_at(&root)
        .run();
    assert!(
        verbose.stderr_text().contains(MISSING_VENDOR_ID),
        "the identity is what `--verbose` adds: {}",
        verbose.stderr_text()
    );
}
