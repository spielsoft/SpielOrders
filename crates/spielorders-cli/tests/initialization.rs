//! `spielorders init`: turning an ordinary directory into a SpielOrder Package.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution.

use crate::support::{Invocation, one_order_created_in, response, unmanaged_directory};

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
    let created: Vec<&str> = document["data"]["operations"]
        .as_array()
        .expect("a mutation names its operations")
        .iter()
        .filter(|operation| operation["kind"] == "create_directory")
        .filter_map(|operation| operation["path"].as_str())
        .collect();
    let orders = root.join("archive/2026/Orders");
    assert!(
        created.contains(&orders.to_str().expect("the test path is UTF-8")),
        "{document}"
    );
    assert!(orders.is_dir());

    let directory = one_order_created_in(&root, "Nested order");
    let order = response(
        &Invocation::new(&["order", "show", "--json"])
            .rooted_at(&directory)
            .run(),
    );
    assert_eq!(order["ok"], true, "the new Package holds a readable Order");
    assert_eq!(order["data"]["title"], "Nested order");
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
    assert!(text.contains("outcome: committed\n"), "{text}");
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
    assert!(
        text.contains(&format!("path: {}/Orders\n", verbose_root.display())),
        "{text}"
    );
    assert!(text.contains(".spielorders/config.toml"), "{text}");
    assert!(text.contains(".spielorders/version.json"), "{text}");
    assert!(text.contains("README.md"), "{text}");
}
