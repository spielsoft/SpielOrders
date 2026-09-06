//! `spielorders init`: turning an ordinary directory into a SpielOrder Package.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution.

use crate::support::{
    Invocation, entry_names, one_order_created_in, populated_package, response, unmanaged_directory,
};

/// The whole point of the command: an ordinary directory becomes a Package the read
/// commands open.
///
/// Asserted by reading the result back through a *different* command, because a
/// Package that only `init` can make sense of is not a Package. The defect this catches
/// is an initialization that writes some of the managed artifacts, or writes them at a
/// version this build does not read.
#[test]
fn an_ordinary_directory_becomes_a_package_the_read_commands_open() {
    let (_tree, root) = unmanaged_directory("init-makes-a-package");

    let outcome = Invocation::new(&["init"]).rooted_at(&root).run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());

    let document = response(
        &Invocation::new(&["package", "show", "--json"])
            .rooted_at(&root)
            .run(),
    );
    assert_eq!(document["ok"], true, "the new Package reads");
    // The configuration is unversioned by closed decision, so a default run writes an
    // empty table and every configured path is the conventional default.
    assert_eq!(document["data"]["orders"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        document["data"]["vendors"].as_array().map(Vec::len),
        Some(0)
    );
}

/// Non-default configured paths are honored, and the Package they describe is one the
/// `order` commands work in.
///
/// **The Orders Directory is several levels below the Root**, which is the layout a
/// targeting rule that counted a fixed two parents would get wrong: an Order's own
/// technical locator is the only thing that knows how far up its Package is. And the two
/// assignments are written in the opposite order to the one they are reported in, because
/// an assignment set is order-independent.
#[test]
fn non_default_configured_paths_are_honored_and_hold_orders() {
    let (_tree, root) = unmanaged_directory("init-configured-paths");

    let document = response(
        &Invocation::new(&[
            "init",
            "removed_path=Trash",
            "orders_path=archive/2026/Orders",
            "--json",
        ])
        .rooted_at(&root)
        .run(),
    );

    assert_eq!(document["ok"], true);
    assert_eq!(
        document["data"]["package"]["orders_path"],
        "archive/2026/Orders"
    );
    assert_eq!(document["data"]["package"]["removed_path"], "Trash");
    assert!(root.join("archive/2026/Orders").is_dir());

    let directory = one_order_created_in(&root, "Nested order");
    let order = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    );
    assert_eq!(order["ok"], true, "the new Package holds a readable Order");
    assert_eq!(order["data"]["title"], "Nested order");
}

/// An existing Package README survives byte for byte, and a directory without one gains
/// an empty conventional Package README.
///
/// The defect this catches is an initialization that writes its own README over a user's
/// own document — the one file in the Root that is theirs rather than this application's,
/// and the one it would be least forgivable to overwrite.
#[test]
fn an_existing_package_readme_survives_and_an_absent_one_is_created_empty() {
    let (_tree, existing) = unmanaged_directory("init-readme-existing");
    let readme = existing.join("README.md");
    let authored = "# Telescope purchasing\n\nAuthored before this was a Package.\n";
    std::fs::write(&readme, authored).expect("the existing README is written");

    let outcome = Invocation::new(&["init"]).rooted_at(&existing).run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        std::fs::read(&readme).expect("the README is still readable"),
        authored.as_bytes(),
        "the existing README is unchanged byte for byte"
    );

    let (_tree, absent) = unmanaged_directory("init-readme-absent");
    let outcome = Invocation::new(&["init"]).rooted_at(&absent).run();
    assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
    assert_eq!(
        std::fs::read(absent.join("README.md")).expect("the conventional README is there"),
        b"",
        "a Package initialized without one gains an empty conventional README"
    );
}

/// The Removed Orders Directory is not created eagerly.
///
/// It is created when something is first removed into it, which is Ticket 012's. An
/// initialization that made it anyway would leave every new Package carrying an empty
/// directory nobody asked for.
#[test]
fn the_removed_orders_directory_is_not_created_eagerly() {
    let (_tree, root) = unmanaged_directory("init-removed-lazily");

    let document = response(&Invocation::new(&["init", "--json"]).rooted_at(&root).run());

    assert_eq!(document["data"]["package"]["removed_path"], "Removed");
    assert!(
        !root.join("Removed").exists(),
        "the configured path is reported and nothing stands at it yet"
    );
}

/// Every configuration path rule is refused with its own stable code, at exit `1`, having
/// written nothing.
///
/// One test over the whole set because they are one claim: a caller correcting a
/// configured path branches on the code, and a rule folded into a neighbour's code — or
/// into one generic "invalid configuration" — is a correction they cannot make without
/// reading a sentence that may be reworded. The key that broke the rule travels beside
/// the code as an identity, because the code says which rule and not which of the two
/// paths.
#[test]
fn each_configuration_path_rule_is_refused_with_its_own_code_and_writes_nothing() {
    for (assignment, key, code) in [
        (
            "orders_path=/var/orders",
            "orders_path",
            "configuration_path_absolute",
        ),
        (
            "orders_path=.",
            "orders_path",
            "configuration_path_current_directory",
        ),
        (
            "orders_path=../outside",
            "orders_path",
            "configuration_path_parent_traversal",
        ),
        (
            "orders_path=",
            "orders_path",
            "configuration_path_empty_component",
        ),
        (
            "orders_path=archive\\2026",
            "orders_path",
            "configuration_path_platform_separator",
        ),
        (
            "orders_path=.spielorders/Orders",
            "orders_path",
            "configuration_path_reserved_metadata",
        ),
        (
            "removed_path=archive/Removed",
            "removed_path",
            "configuration_path_not_direct_child",
        ),
        (
            "orders_path=README.md",
            "orders_path",
            "configuration_path_reserved_readme",
        ),
    ] {
        let (_tree, root) = unmanaged_directory("init-path-rule");
        let outcome = Invocation::new(&["init", assignment, "--json"])
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{assignment}");
        assert_eq!(document["ok"], false, "{assignment}");
        assert!(document["data"].is_null(), "{assignment}");
        assert_eq!(document["error"]["code"], code, "{assignment}");
        assert_eq!(
            document["error"]["identities"][0]["kind"], "configuration_path",
            "{assignment}"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], key,
            "{assignment}"
        );
        // The directory resolved and this command was about it, whatever the outcome.
        assert_eq!(
            document["context"]["package_root"],
            root.to_str().expect("the test root is UTF-8"),
            "{assignment}"
        );
        assert_eq!(
            entry_names(&root),
            Vec::<String>::new(),
            "{assignment}: a refused initialization writes nothing"
        );
    }
}

/// Two configured paths that name one place are refused as one rule about the pair.
///
/// Its own code and no identity, because neither key is the one at fault: the two of them
/// together are.
#[test]
fn two_configured_paths_naming_one_place_are_refused() {
    let (_tree, root) = unmanaged_directory("init-overlapping-paths");

    let outcome = Invocation::new(&[
        "init",
        "orders_path=Shared",
        "removed_path=Shared",
        "--json",
    ])
    .rooted_at(&root)
    .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "configuration_paths_overlap");
    assert_eq!(
        document["error"]["identities"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(entry_names(&root), Vec::<String>::new());
}

/// A configured path that resolves through a link out of the Root is refused.
///
/// The rule is about where the path *ends up*, which only the filesystem knows, so it is
/// exercised against a real link rather than assumed to follow from the spelling.
#[test]
fn a_configured_path_resolving_through_a_link_is_refused() {
    let (_tree, root) = unmanaged_directory("init-linked-path");
    let (_outside, outside) = unmanaged_directory("init-link-target");
    std::os::unix::fs::symlink(&outside, root.join("Orders")).expect("the link is created");

    let outcome = Invocation::new(&["init", "--json"]).rooted_at(&root).run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "initialization_disallowed_link");
    // The core resolved both the Root and the offending path, and the response says so
    // rather than quoting the caller's own text back for a resolution that happened.
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
    assert_eq!(document["error"]["paths"][0]["spelling"], "resolved");
    assert_eq!(
        document["error"]["paths"][0]["path"],
        root.join("Orders")
            .to_str()
            .expect("the test path is UTF-8")
    );
    assert_eq!(
        document["error"]["paths"][0]["package_relative_path"], "Orders",
        "and it is a place in the Package this would have created"
    );
    assert!(
        !root.join(".spielorders").exists(),
        "a refused initialization writes nothing"
    );
}

/// Initializing a directory that is already a Package is refused, and it is unchanged.
///
/// The defect this catches is an initialization that merged into an existing Package —
/// rewriting its configuration, or replacing its Vendor aggregate with an empty one,
/// which would discard every Vendor it had.
#[test]
fn initializing_a_package_again_is_refused_and_changes_nothing() {
    let (_tree, root) = populated_package("init-already-a-package");
    let before = std::fs::read(root.join(".spielorders/vendors.json"))
        .expect("the Vendor aggregate is readable");

    let outcome = Invocation::new(&["init", "--json"]).rooted_at(&root).run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1));
    assert_eq!(document["error"]["code"], "package_root_already_managed");
    // A Root did stand there, which is exactly why this was refused.
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8")
    );
    // And so it spells itself the way a loaded Package's root does. This is the one
    // `init` refusal with a Package to be relative to; the rest refuse before one exists.
    assert_eq!(document["error"]["paths"][0]["package_relative_path"], ".");
    assert_eq!(
        std::fs::read(root.join(".spielorders/vendors.json")).expect("still readable"),
        before,
        "the existing Package is untouched"
    );
}

/// A `--root` that is not an ordinary directory is refused, naming the caller's own text.
///
/// No root resolved, so the context names none and the failure's path is what says what
/// was asked for — the same division every command makes.
#[test]
fn a_root_that_is_not_an_ordinary_directory_is_refused() {
    let (_tree, root) = unmanaged_directory("init-not-a-directory");
    let file = root.join("ordinary-file");
    std::fs::write(&file, b"not a directory").expect("the file is written");

    for target in [file, root.join("absent")] {
        let outcome = Invocation::new(&["init", "--json"])
            .rooted_at(&target)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{}", target.display());
        assert_eq!(
            document["error"]["code"],
            "package_root_not_a_directory",
            "{}",
            target.display()
        );
        assert!(document["context"]["package_root"].is_null());
        assert_eq!(
            document["error"]["paths"][0]["spelling"], "as_given",
            "nothing resolved it, so it is the caller's own text"
        );
    }
}

/// A dry run reports what the real run would, and creates nothing at all.
///
/// **The same directory, planned and then initialized**, which is what makes the
/// comparison mean something: the plan is not a description of a hypothetical directory
/// but of this one, and the real run that follows proves the plan changed nothing about
/// it. `committed` is the only difference between the two documents, which is the whole
/// of what "the same planned mutation report with `committed: false`" claims.
///
/// The directory being empty afterwards covers the staging file too: a managed write
/// stages beside its destination, so a dry run that had begun writing would leave one
/// there.
#[test]
fn a_dry_run_reports_what_the_real_run_would_and_creates_nothing() {
    let (_tree, root) = unmanaged_directory("init-dry-run");

    let planned = Invocation::new(&["init", "--json", "--dry-run"])
        .rooted_at(&root)
        .run();

    assert_eq!(planned.status, Some(0), "{}", planned.stderr_text());
    assert_eq!(
        response(&planned)["data"]["report"]["committed"],
        false,
        "the report says it was not carried out"
    );
    assert_eq!(
        entry_names(&root),
        Vec::<String>::new(),
        "no directory, no managed file, no staging artifact"
    );

    let committed = Invocation::new(&["init", "--json"]).rooted_at(&root).run();

    assert_eq!(committed.status, Some(0), "{}", committed.stderr_text());
    assert_eq!(
        planned
            .stdout_text()
            .replace("\"committed\": false", "\"committed\": true"),
        committed.stdout_text(),
        "`committed` is the only difference between a plan and the run of it"
    );
}

/// A dry run over a refused plan produces that refusal instead.
///
/// Byte for byte the same document the real run gives, because a plan is refused before
/// the commit either way — so `--dry-run` is never a way to be told a mutation would
/// succeed when it would not.
#[test]
fn a_dry_run_over_a_refused_plan_produces_that_refusal() {
    let (_tree, root) = unmanaged_directory("init-dry-run-refused");
    let arguments = ["init", "orders_path=.", "--json"];

    let planned = Invocation::new(&arguments)
        .argument("--dry-run")
        .rooted_at(&root)
        .run();
    let refused = Invocation::new(&arguments).rooted_at(&root).run();

    assert_eq!(planned.status, Some(1));
    assert_eq!(refused.status, Some(1));
    assert_eq!(planned.stdout_text(), refused.stdout_text());
    assert_eq!(entry_names(&root), Vec::<String>::new());
}

/// A `--quiet` initialization says nothing and still does the whole thing.
#[test]
fn a_quiet_initialization_says_nothing_and_still_initializes() {
    let (_tree, root) = unmanaged_directory("init-quiet");

    let outcome = Invocation::new(&["init", "--quiet"]).rooted_at(&root).run();

    assert_eq!(outcome.status, Some(0));
    assert_eq!(outcome.stdout, b"", "no output on success");
    assert_eq!(outcome.stderr, b"");
    assert!(root.join(".spielorders/version.json").is_file());
}

/// The human result names the places, and `--verbose` adds the managed documents.
///
/// The decided split between what a mutation reports and what only `--verbose` shows.
/// Human output is not a parsing contract, so this is about which facts each mode
/// carries rather than about their wording.
#[test]
fn the_human_result_names_the_directories_and_verbose_adds_the_managed_files() {
    let (_tree, plain) = unmanaged_directory("init-human-result");
    let human = Invocation::new(&["init"]).rooted_at(&plain).run();

    assert_eq!(human.status, Some(0), "{}", human.stderr_text());
    let text = human.stdout_text();
    assert!(text.contains("Metadata version: 1"), "{text}");
    assert!(text.contains("Orders Directory: Orders"), "{text}");
    assert!(
        text.contains(&format!("Created: {}/Orders\n", plain.display())),
        "{text}"
    );
    assert!(
        !text.contains("config.toml"),
        "an individual managed file is --verbose detail: {text}"
    );

    let (_tree, verbose_root) = unmanaged_directory("init-verbose-result");
    let verbose = Invocation::new(&["init", "--verbose"])
        .rooted_at(&verbose_root)
        .run();

    assert_eq!(verbose.status, Some(0), "{}", verbose.stderr_text());
    let text = verbose.stdout_text();
    assert!(text.contains(".spielorders/config.toml"), "{text}");
    assert!(text.contains(".spielorders/version.json"), "{text}");
    assert!(text.contains("README.md"), "{text}");
}

/// A filesystem that refuses to answer about a configured path is a typed refusal naming
/// the Root the resolution had already reached.
///
/// **A name longer than this platform accepts** rather than a permission, for the reason
/// every other fixture in this suite avoids one: `chmod 000` does not stop a suite running
/// as root, and a component of three hundred characters stops every filesystem in the same
/// way. The refusal arrives *after* the selected root resolved, which is the half a
/// response is most likely to get wrong — reporting no Root, and quoting a path this
/// application resolved back as the caller's own text.
#[test]
fn a_filesystem_refusal_about_a_configured_path_names_the_resolved_root() {
    let (_tree, root) = unmanaged_directory("init-unobservable-path");
    let overlong = "o".repeat(300);

    let outcome = Invocation::new(&["init", &format!("orders_path={overlong}"), "--json"])
        .rooted_at(&root)
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(
        document["error"]["code"],
        "initialization_filesystem_failed"
    );
    assert_eq!(
        document["context"]["package_root"],
        root.to_str().expect("the test root is UTF-8"),
        "the resolution had already happened, so the response names what it reached"
    );
    assert_eq!(
        document["error"]["paths"][0]["spelling"], "resolved",
        "and the path it could not examine is one this application resolved"
    );
    assert_eq!(entry_names(&root), Vec::<String>::new());
}
