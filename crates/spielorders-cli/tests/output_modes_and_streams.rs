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
    let message = document["error"]["detail"]
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

/// A failure goes to standard error and leaves standard output empty, in the default mode
/// and under `--quiet` alike. A caller composing `$(...)` must not receive an error as the
/// result, and `--quiet`'s whole use is a status check, so a *silenced failure* would
/// defeat the caller.
#[test]
fn a_failure_goes_to_standard_error_even_under_quiet() {
    let (_tree, root) = initialized_package("streams-human-failure");
    let outside = directory_that_is_not_a_root(&root);

    for mode in [&[][..], &["--quiet"][..]] {
        let mut arguments = vec!["package", "readme"];
        arguments.extend_from_slice(mode);
        let outcome = Invocation::new(&arguments).rooted_at(&outside).run();

        assert_eq!(outcome.status, Some(1), "{mode:?}");
        assert_eq!(outcome.stdout, b"", "{mode:?}");
        assert!(
            outcome
                .stderr_text()
                .starts_with("error: unmanaged package"),
            "{mode:?}: the failure is reported, its severity in words: {}",
            outcome.stderr_text()
        );
    }
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
        detail.iter().any(|line| line == &"state: present"),
        "{detail:?}"
    );
}

// ---------------------------------------------------------------------------
// Reference text under `--json`.
// ---------------------------------------------------------------------------

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
/// where these messages mostly end up. A failure's `error: ` label is asserted with the
/// failure streams above.
#[test]
fn a_warning_states_its_severity_in_words() {
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

/// A dry run is how a reader reviews a change before making it, so its human output
/// names what would change, and the entries an Order merge would discard, without
/// `--verbose`.
#[test]
fn a_human_dry_run_shows_what_it_would_change_and_discard() {
    let (_tree, root) = crate::support::populated_package("human-dry-run");
    let kept = crate::support::order_directory(&root, "Laser_components");
    let edit = Invocation::new(&["order", "edit", "title=Better lenses", "--dry-run"])
        .rooted_at(&kept)
        .run();
    assert_eq!(edit.status, Some(0));
    let shown = edit.stdout_text();
    assert!(shown.contains("title"), "{shown}");
    assert!(shown.contains("Better lenses"), "{shown}");

    let other = crate::support::new_order(
        &root,
        &[("title", "Duplicate order"), ("currency", "USD")],
        &[&[
            ("description", "Spare mount"),
            ("quantity", "1"),
            ("unit_price", "5"),
        ]],
    );
    std::fs::write(other.join("quote.pdf"), b"Quote").unwrap();
    std::fs::write(kept.join("quote.pdf"), b"Kept quote").unwrap();
    let merge = Invocation::new(&["order", "merge", "--dry-run"])
        .argument(&other)
        .rooted_at(&kept)
        .run();
    assert_eq!(merge.status, Some(0));
    let shown = merge.stdout_text();
    assert!(shown.contains("discards"), "{shown}");
    assert!(shown.contains("quote.pdf"), "{shown}");
    assert!(shown.contains("row_added"), "the Item it adds: {shown}");
    assert!(shown.contains("Spare mount"), "{shown}");

    // A plan that changes no record's facts names no changes at all.
    let removal = Invocation::new(&["order", "remove", "--dry-run"])
        .rooted_at(&other)
        .run();
    assert_eq!(removal.status, Some(0));
    assert!(
        !removal.stdout_text().contains("changes"),
        "{}",
        removal.stdout_text()
    );
}

/// `order merge` asks before a discard only a person reading human output can answer: under
/// `--json` or `--quiet` on a terminal it refuses with `discard_unconfirmed` and writes no
/// question, even where an answer is waiting on standard input.
#[test]
fn a_merge_asks_nothing_under_json_or_quiet_on_a_terminal() {
    for mode in ["--json", "--quiet"] {
        let (_tree, root) = crate::support::populated_package("merge-no-prompt");
        let kept = crate::support::order_directory(&root, "Laser_components");
        let other = crate::support::new_order(&root, &[("title", "Duplicate order")], &[]);
        std::fs::write(other.join("quote.pdf"), b"Quote").unwrap();
        std::fs::write(kept.join("quote.pdf"), b"Kept quote").unwrap();
        let arguments = [
            env!("CARGO_BIN_EXE_spielorders").to_owned(),
            "order".to_owned(),
            "merge".to_owned(),
            other.display().to_string(),
            "--root".to_owned(),
            kept.display().to_string(),
            mode.to_owned(),
        ];
        // `script` gives the command a terminal on its standard streams.
        let mut script = std::process::Command::new("script");
        if cfg!(target_os = "macos") {
            script.arg("-q").arg("/dev/null").args(&arguments);
        } else {
            let quoted: Vec<String> = arguments.iter().map(|word| format!("'{word}'")).collect();
            script.args(["-qec", &quoted.join(" "), "/dev/null"]);
        }
        let mut child = script
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("script runs");
        std::io::Write::write_all(child.stdin.as_mut().unwrap(), b"y\n").unwrap();
        let output = child.wait_with_output().expect("script finishes");
        let shown = String::from_utf8_lossy(&output.stdout);

        assert!(!shown.contains("[y/N]"), "{mode} asks: {shown}");
        assert!(
            std::fs::read(other.join("quote.pdf")).is_ok(),
            "{mode} discards nothing without --yes"
        );
        if mode == "--json" {
            assert!(shown.contains("discard_unconfirmed"), "{shown}");
        }
    }
}
