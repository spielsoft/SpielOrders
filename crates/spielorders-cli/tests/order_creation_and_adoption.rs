//! `add-order` and `adopt-order`: the two ways an Order Directory comes under
//! management, through the real executable.
//!
//! Decision sources: Define the Rust CLI capability contract; Define legacy Order
//! adoption and metadata bootstrapping; Define the Order record and lifecycle contract

use crate::support::{
    ADOPTED_FACTS, Invocation, OFFERED_VENDOR_ID, initialized_package, managed_snapshot,
    package_with_an_adoption_candidate, response,
};
use spielorders_core::ports::LocalCalendar;
use std::path::{Path, PathBuf};

/// Today's local calendar date, as this build reads it.
///
/// The child process reads the real clock, so a test about "today" has to ask the same
/// question the command does. Asking the executable's own production effect is what
/// makes this a claim about the Order rather than a second calendar in the test suite;
/// the effect itself is pinned by `tests/platform_effects.rs`.
fn today() -> String {
    let today = spielorders_cli::effects::PlatformCalendar.today();
    format!("{:04}-{:02}-{:02}", today.year, today.month, today.day)
}

/// An Order created with nothing but a Title takes every established default, and
/// `status=` is the one of them a caller may choose at creation.
#[test]
fn a_created_order_takes_its_defaults_and_may_be_created_at_another_status() {
    for (assigned, expected) in [
        (Vec::new(), "in_preparation"),
        (vec!["status=ordered"], "ordered"),
    ] {
        let (_tree, root) = initialized_package("add-order-defaults");
        // The assignment form once a second fact is assigned, because the two forms cannot
        // be combined — and `both_add_order_forms_produce_the_same_order` is what makes the
        // Title reach the core identically either way.
        let mut run =
            Invocation::new(&["add-order", "title=Laser components", "--json"]).rooted_at(&root);
        for assigned in &assigned {
            run = run.argument(assigned);
        }

        let outcome = run.run();
        let document = response(&outcome);
        let order = &document["data"]["order"];

        assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
        assert_eq!(document["contract"], "spielorders.add_order");
        assert_eq!(order["title"], "Laser components");
        assert_eq!(order["date_ordered"], today(), "{assigned:?}");
        assert_eq!(order["status"], expected, "{assigned:?}");
        assert!(order["vendor_id"].is_null(), "{assigned:?}");
        assert!(order["vendor"].is_null(), "{assigned:?}");
        assert_eq!(
            order["items"].as_array().map(Vec::len),
            Some(0),
            "an Order's Items are present and empty rather than absent"
        );
        assert_eq!(document["data"]["report"]["committed"], true);
    }
}

/// The Title shorthand and the assignment it stands for are one command.
#[test]
fn both_add_order_forms_produce_the_same_order() {
    let mut produced = Vec::new();
    for form in [
        vec!["add-order", "Laser components"],
        vec!["add-order", "title=Laser components"],
    ] {
        let (_tree, root) = initialized_package("add-order-forms");
        let outcome = Invocation::new(&form)
            .argument("--json")
            .rooted_at(&root)
            .run();
        assert_eq!(outcome.status, Some(0), "{}", outcome.stderr_text());
        produced.push(response(&outcome)["data"]["order"].clone());
    }

    let [shorthand, assigned] =
        <[serde_json::Value; 2]>::try_from(produced).expect("both forms were run");
    // The Order ID and the paths differ by construction: each run minted its own identity
    // in its own Package. Every domain fact is the same Order.
    for fact in [
        "title",
        "date_ordered",
        "status",
        "vendor_id",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
        "items",
        "order_total",
    ] {
        assert_eq!(shorthand[fact], assigned[fact], "{fact}");
    }
}

/// A Title beside an assignment names the Order two ways at once, and a second Title is
/// not a second subject. Neither is a refused mutation: parsing failed, so nothing about
/// the Package was attempted.
#[test]
fn combining_the_two_add_order_forms_is_a_usage_error() {
    for wrong in [
        vec!["add-order", "Laser components", "status=ordered"],
        vec!["add-order", "Laser components", "Mirror mounts"],
    ] {
        let (_tree, root) = initialized_package("add-order-combined");
        let before = managed_snapshot(&root);

        let outcome = Invocation::new(&wrong)
            .argument("--json")
            .rooted_at(&root)
            .run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(2), "{wrong:?}");
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Package was attempted"
        );
    }
}

/// A Vendor Reference is assigned by Vendor Name, and the created Order resolves it.
///
/// **The Name is the selector and the identity is the answer.** A Vendor Name is trimmed
/// and matched without regard to letter case, which is how a Vendor is selected everywhere
/// else in this surface; the response carries the Vendor ID the Order now stores, because a
/// machine response keeps the identities a human surface holds back.
#[test]
fn a_created_order_resolves_the_vendor_it_references() {
    let (_tree, root) = crate::support::populated_package("add-order-vendor");

    let outcome = Invocation::new(&["add-order", "title=Fiber patch cables", "--json"])
        .argument("vendor=  tarNWICK ")
        .rooted_at(&root)
        .run();
    let document = response(&outcome);
    let order = &document["data"]["order"];

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(order["vendor_id"], OFFERED_VENDOR_ID);
    assert_eq!(order["vendor"]["name"], "Tarnwick");
}

/// A candidate holding Supporting Material and a user-authored README is adopted where it
/// stands, with both byte-identical and nothing else created, renamed, or removed.
///
/// Its name embeds `2009_04_31`, an impossible calendar date. The adopted Order carries the
/// Date Ordered the command line supplied and the directory keeps its name, because a
/// directory component is evidence for a human and never a field grammar.
#[test]
fn a_candidate_is_adopted_in_place_with_everything_it_holds_untouched() {
    let (_tree, root, candidate) = package_with_an_adoption_candidate("adopt-in-place");
    let before = managed_snapshot(&candidate);

    let mut run = Invocation::new(&["adopt-order"])
        .argument(&candidate)
        .rooted_at(&root)
        .argument("--json");
    for assignment in ADOPTED_FACTS {
        run = run.argument(assignment);
    }
    let outcome = run.run();
    let document = response(&outcome);
    let order = &document["data"]["order"];

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["contract"], "spielorders.adopt_order");
    assert_eq!(order["title"], "Fiber patch cable");
    assert_eq!(
        order["date_ordered"], "2009-04-30",
        "the supplied date is adopted uncorrected, and the name is not read"
    );
    assert_eq!(order["status"], "complete");
    assert_eq!(
        order["readme_text"], "Ordered by telephone. Invoice in the quotes folder.\n",
        "the candidate's own README is what the adopted Order carries"
    );
    assert_eq!(document["data"]["report"]["committed"], true);

    let after = managed_snapshot(&candidate);
    let added: Vec<&std::path::PathBuf> = after
        .iter()
        .map(|(path, _)| path)
        .filter(|path| !before.iter().any(|(existing, _)| existing == *path))
        .collect();
    assert_eq!(
        added,
        vec![
            &std::path::PathBuf::from(".spielorders"),
            &std::path::PathBuf::from(".spielorders/order.json"),
        ],
        "adoption writes the Order record and nothing else"
    );
    for (path, contents) in &before {
        assert_eq!(
            after.iter().find(|(candidate, _)| candidate == path),
            Some(&(path.clone(), contents.clone())),
            "{} is byte-identical",
            path.display()
        );
    }
}

/// An adoption that names none of the three required facts is refused naming all three,
/// having written nothing.
#[test]
fn adoption_missing_a_required_fact_is_refused_naming_it() {
    for (supplied, missing) in [
        (Vec::new(), vec!["title", "date_ordered", "status"]),
        (
            vec!["title=Fiber patch cable"],
            vec!["date_ordered", "status"],
        ),
        (
            vec!["title=Fiber patch cable", "date_ordered=2009-04-30"],
            vec!["status"],
        ),
    ] {
        let (_tree, root, candidate) = package_with_an_adoption_candidate("adopt-missing");
        let before = managed_snapshot(&candidate);

        let mut run = Invocation::new(&["adopt-order"])
            .argument(&candidate)
            .rooted_at(&root)
            .argument("--json");
        for assignment in &supplied {
            run = run.argument(assignment);
        }
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{supplied:?}");
        assert_eq!(
            document["error"]["code"], "order_facts_required",
            "{supplied:?}"
        );
        let named: Vec<String> = document["error"]["identities"]
            .as_array()
            .expect("the refusal names the facts it is about")
            .iter()
            .map(|identity| identity["value"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(named, missing, "{supplied:?}");
        assert_eq!(
            managed_snapshot(&candidate),
            before,
            "{supplied:?}: nothing was written"
        );
    }
}

/// A candidate with no README of its own is given the empty conventional one.
#[test]
fn a_candidate_without_a_readme_is_given_an_empty_one() {
    let (_tree, root) = crate::support::populated_package("adopt-no-readme");
    let candidate = crate::support::unmanaged_candidate(&root, "2011_02_09_Nethermoor_Posts");

    let outcome = adopt(&root, &candidate).run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(0), "{}", outcome.stdout_text());
    assert_eq!(document["data"]["order"]["readme_exists"], true);
    assert_eq!(document["data"]["order"]["readme_text"], "");
    assert_eq!(
        std::fs::read(candidate.join("README.md")).expect("the conventional README is there"),
        Vec::<u8>::new(),
        "the created README is empty"
    );
}

/// One `adopt-order` run over one candidate, with the three required facts and `--json`.
fn adopt(root: &std::path::Path, candidate: &std::path::Path) -> Invocation {
    let mut run = Invocation::new(&["adopt-order"])
        .argument(candidate)
        .rooted_at(root)
        .argument("--json");
    for assignment in ADOPTED_FACTS {
        run = run.argument(assignment);
    }
    run
}

/// Every refusal adoption has leaves the candidate untouched and is distinguishable from
/// the others.
///
/// One test over the set rather than one per refusal, because the claim is about the set:
/// a caller branching on a code needs each of these to be its own answer, and two of them
/// sharing one would be invisible in either refusal's own test.
#[test]
fn every_adoption_refusal_is_its_own_answer_and_writes_nothing() {
    let mut seen: Vec<String> = Vec::new();
    for (condition, expected) in adoption_refusals() {
        let (_tree, root, candidate) = condition("adopt-refusals");
        let orders = crate::support::orders_directory(&root);
        let before = managed_snapshot(&orders);

        let outcome = adopt(&root, &candidate).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(1),
            "{expected}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["error"]["code"], expected);
        assert!(document["data"].is_null(), "{expected}");
        let refused = &document["error"]["paths"][0];
        let absolute = refused["path"]
            .as_str()
            .expect("a refusal about a candidate names it");
        assert!(
            Path::new(absolute).starts_with(&candidate),
            "{expected}: the refusal is about the candidate this run named"
        );
        assert_eq!(
            refused["package_relative_path"]
                .as_str()
                .map(|relative| root.join(relative)),
            spelled(expected).then(|| PathBuf::from(absolute)),
            "{expected}: a refusal spells its path the way a success does"
        );
        assert_eq!(
            managed_snapshot(&orders),
            before,
            "{expected}: the candidate and its siblings are untouched"
        );
        seen.push(expected.to_owned());
    }
    let mut distinct = seen.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        seen.len(),
        "each refusal carries its own code: {seen:?}"
    );
}

/// Whether one adoption refusal owes the Package's own spelling for the path it names.
///
/// **Every candidate in the set above stands inside the Package**, so the answer is not a
/// property of the tree: it is whether *this* refusal was reached with the candidate
/// resolved and expressible, which is what [Decision traceability and contract-test
/// conventions] makes the spelling depend on. Each code names its own reason here rather
/// than the claim being made over a subset somebody chose.
///
/// [Decision traceability and contract-test conventions]: ../../../docs/development/traceability.md
fn spelled(code: &str) -> bool {
    match code {
        // Reached after the candidate resolved, inside the Package, with a name this
        // Package can express.
        "order_candidate_not_a_direct_child"
        | "order_candidate_already_managed"
        | "order_candidate_reserved_state_conflict"
        | "order_candidate_staging_occupied" => true,
        // Refused on the caller's own selected path, before anything resolved it.
        "order_candidate_ineligible" | "order_candidate_link" | "order_candidate_hidden" => false,
        // Resolved, and having no name this Package can express is the refusal itself.
        "order_candidate_name_unusable" => false,
        other => panic!("{other} says nothing about the spelling its path carries"),
    }
}

/// The candidates each adoption refusal is about, and the code each earns.
type Candidate = fn(
    &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
);

fn adoption_refusals() -> Vec<(Candidate, &'static str)> {
    vec![
        (candidate_that_is_a_file, "order_candidate_ineligible"),
        (candidate_that_is_a_link, "order_candidate_link"),
        (
            candidate_outside_the_orders_directory,
            "order_candidate_not_a_direct_child",
        ),
        (hidden_candidate, "order_candidate_hidden"),
        (
            candidate_with_an_unusable_name,
            "order_candidate_name_unusable",
        ),
        (already_managed_candidate, "order_candidate_already_managed"),
        (
            candidate_with_partial_reserved_state,
            "order_candidate_reserved_state_conflict",
        ),
        (
            candidate_with_an_occupied_staging_name,
            "order_candidate_staging_occupied",
        ),
    ]
}

fn candidate_that_is_a_file(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root) = crate::support::populated_package(label);
    let path = crate::support::orders_directory(&root).join("Order_Summary.xlsx");
    std::fs::write(&path, b"not a directory").expect("the file is written");
    (tree, root, path)
}

/// The institution-grouped alias view a real archive has: a link into the Orders
/// Directory, which is never followed and never adopted.
fn candidate_that_is_a_link(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root, candidate) = package_with_an_adoption_candidate(label);
    let link = crate::support::orders_directory(&root).join("Northwind");
    std::os::unix::fs::symlink(&candidate, &link).expect("the alias view is created");
    (tree, root, link)
}

fn candidate_outside_the_orders_directory(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root) = crate::support::populated_package(label);
    let outside = root.join("scripts");
    std::fs::create_dir(&outside).expect("the directory beside the Orders Directory is created");
    (tree, root, outside)
}

fn hidden_candidate(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root) = crate::support::populated_package(label);
    let candidate = crate::support::unmanaged_candidate(&root, ".tmp_import");
    (tree, root, candidate)
}

/// A directory the operating system is content with and that nothing describing a Package
/// could refer to.
///
/// A backslash is a legal character in a filename here and is not one a normalized
/// Package-relative path may carry, so this entry has no spelling a Package can express —
/// the same condition the core's `orders_directory_entry_names` suite builds, and the
/// reachable one on a filesystem that enforces valid UTF-8 names.
fn candidate_with_an_unusable_name(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root) = crate::support::populated_package(label);
    let candidate = crate::support::orders_directory(&root).join("2019_08_22_Quote\\0161172");
    std::fs::create_dir(&candidate).expect("the unusable-named directory is created");
    (tree, root, candidate)
}

/// One of the Package's own Orders, which already holds a readable Order record.
fn already_managed_candidate(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root) = crate::support::populated_package(label);
    let existing = crate::support::order_directory(&root, "Laser_components");
    (tree, root, existing)
}

/// Reserved Package metadata that is not a readable Order record — the shape a partial
/// earlier adoption leaves.
fn candidate_with_partial_reserved_state(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root, candidate) = package_with_an_adoption_candidate(label);
    std::fs::create_dir(candidate.join(".spielorders")).expect("the reserved subtree is created");
    std::fs::write(candidate.join(".spielorders/order.json"), b"{ not json")
        .expect("the partial record is written");
    (tree, root, candidate)
}

/// A staging name this adoption needs, occupied by an interrupted earlier write.
fn candidate_with_an_occupied_staging_name(
    label: &str,
) -> (
    crate::support::TemporaryTree,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (tree, root, candidate) = package_with_an_adoption_candidate(label);
    std::fs::write(candidate.join(".README.md.spielorders-stage"), b"leftover")
        .expect("the staging name is occupied");
    (tree, root, candidate)
}

/// A creation and an adoption both write a locator that resolves from the new Order
/// Directory back to this Package's Root, including under a nested `orders_path`.
///
/// Asserted by reading the Order back with `order show`, which reaches its Package through
/// that locator and nothing else — so a locator counted from the wrong depth fails here
/// rather than in a comparison against a string this test spelled.
#[test]
fn both_commands_write_a_locator_that_resolves_under_a_nested_orders_path() {
    let (_tree, root, _existing) =
        crate::support::package_with_a_nested_orders_directory("new-order-locator");

    let created = Invocation::new(&["add-order", "title=Laser components", "--json"])
        .rooted_at(&root)
        .run();
    let created_directory =
        response(&created)["data"]["order"]["paths"]["directory"]["absolute_path"]
            .as_str()
            .expect("the created Order names its Directory")
            .to_owned();

    let candidate = crate::support::unmanaged_candidate(&root, "2009_04_31_Lcom_Fiber");
    let adopted = adopt(&root, &candidate).run();
    assert_eq!(adopted.status, Some(0), "{}", adopted.stdout_text());

    for directory in [created_directory.as_str(), candidate.to_str().unwrap()] {
        let read = Invocation::new(&["order", "show", "--json"])
            .argument("--root")
            .argument(directory)
            .run();
        let document = response(&read);
        assert_eq!(read.status, Some(0), "{directory}: {}", read.stdout_text());
        assert_eq!(
            document["context"]["package_root"],
            root.to_str().expect("the test root is UTF-8"),
            "{directory}: the locator reaches this Package"
        );
    }
}

/// A committed creation or adoption whose derived index refresh then fails stays
/// committed, reports the warning, exits `0`, and shows the Order once afterwards.
#[test]
fn a_failed_index_refresh_leaves_the_new_order_committed_at_exit_zero() {
    for adopting in [false, true] {
        let (_tree, root) =
            crate::support::package_with_a_blocked_index_refresh("new-order-index-warning");
        let before = crate::support::entry_names(&crate::support::orders_directory(&root));

        let outcome = if adopting {
            let candidate = crate::support::unmanaged_candidate(&root, "2009_04_31_Lcom_Fiber");
            adopt(&root, &candidate).run()
        } else {
            Invocation::new(&["add-order", "title=Laser components", "--json"])
                .rooted_at(&root)
                .run()
        };
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(0),
            "{adopting}: {}",
            outcome.stderr_text()
        );
        assert_eq!(document["data"]["report"]["committed"], true, "{adopting}");
        assert!(
            !document["data"]["report"]["warnings"]
                .as_array()
                .expect("a committed mutation reports its warnings")
                .is_empty(),
            "{adopting}: the failed refresh is reported"
        );

        // One ordinary load afterwards shows the Order once, which is the half a warning
        // alone would not say: the canonical record is there and is not doubled.
        let listed = Invocation::new(&["package", "list", "orders", "--json"])
            .rooted_at(&root)
            .run();
        let orders = response(&listed)["data"]["orders"]
            .as_array()
            .expect("the listing carries its Orders")
            .len();
        assert_eq!(
            orders,
            before.len() + 1,
            "{adopting}: the new Order is listed exactly once"
        );
    }
}

/// A dry run of either command reports the real run's answer with `committed: false` and
/// leaves the Package and the candidate byte-identical — no directory, no staging file, no
/// index rewrite.
#[test]
fn a_dry_run_of_either_command_reports_the_real_runs_order_and_writes_nothing() {
    // Creation: the plan's Order against the same plan committed.
    let (_tree, root) = initialized_package("new-order-dry-run");
    let before = managed_snapshot(&root);
    let planned = Invocation::new(&["add-order", "title=Laser components", "--json"])
        .rooted_at(&root)
        .argument("--dry-run")
        .run();
    let planned = response(&planned);
    assert_eq!(planned["data"]["report"]["committed"], false);
    assert!(
        planned["data"]["order"]["order_id"].is_null(),
        "an identity is minted at a commit that did not happen"
    );
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run leaves the Package byte-identical, staging included"
    );

    let committed = Invocation::new(&["add-order", "title=Laser components", "--json"])
        .rooted_at(&root)
        .run();
    let committed = response(&committed);
    for fact in [
        "title",
        "date_ordered",
        "status",
        "items",
        "order_total",
        "paths",
    ] {
        assert_eq!(
            planned["data"]["order"][fact], committed["data"]["order"][fact],
            "{fact}: a dry run answers with the real run's Order"
        );
    }

    // Adoption: the same, over a candidate that must not be touched at all.
    let (_tree, root, candidate) = package_with_an_adoption_candidate("adopt-dry-run");
    let before = managed_snapshot(&root);
    let planned = adopt(&root, &candidate).argument("--dry-run").run();
    let planned = response(&planned);
    assert_eq!(planned["data"]["report"]["committed"], false);
    assert_eq!(
        managed_snapshot(&root),
        before,
        "a dry run leaves the candidate byte-identical"
    );

    let committed = adopt(&root, &candidate).run();
    let committed = response(&committed);
    for fact in ["title", "date_ordered", "status", "readme_text", "paths"] {
        assert_eq!(
            planned["data"]["order"][fact], committed["data"]["order"][fact],
            "{fact}: a dry run answers with the real run's Order"
        );
    }
}

/// A dry run over a refusal produces that refusal rather than a plan, and still writes
/// nothing.
///
/// Both commands, because both have refusals of their own: adoption's are about the
/// candidate, and creation's are about the facts it was given and the name they project.
#[test]
fn a_dry_run_over_a_refusal_produces_the_refusal() {
    for (condition, expected) in adoption_refusals() {
        let (_tree, root, candidate) = condition("adopt-dry-run-refusals");
        let orders = crate::support::orders_directory(&root);
        let before = managed_snapshot(&orders);

        let outcome = adopt(&root, &candidate).argument("--dry-run").run();
        let document = response(&outcome);

        assert_eq!(outcome.status, Some(1), "{expected}");
        assert_eq!(document["error"]["code"], expected);
        assert_eq!(managed_snapshot(&orders), before, "{expected}");
    }

    for (assignments, expected) in [
        (
            vec!["title=Laser components", "colour=red"],
            "order_assignment_rejected",
        ),
        (
            vec!["title=Laser components", "status=shipped"],
            "order_status_invalid",
        ),
        (
            vec!["title=Laser components", "date_ordered=2026-02-30"],
            "order_date_ordered_invalid",
        ),
        (
            vec!["title=Laser components", "currency=dollars"],
            "order_currency_invalid",
        ),
    ] {
        let (_tree, root) = crate::support::populated_package("add-order-dry-run-refusals");
        let before = managed_snapshot(&root);

        let mut run = Invocation::new(&["add-order", "--json"])
            .rooted_at(&root)
            .argument("--dry-run");
        for assignment in &assignments {
            run = run.argument(assignment);
        }
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(1),
            "{expected}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["error"]["code"], expected, "{assignments:?}");
        assert_eq!(managed_snapshot(&root), before, "{expected}");
    }
}

/// An assignment naming a field an Order does not have, one it cannot have assigned, and
/// one assigned twice are all refused together, and nothing is written.
#[test]
fn a_refused_assignment_set_names_every_field_it_refused() {
    let (_tree, root) = initialized_package("add-order-assignments");
    let before = managed_snapshot(&root);

    let outcome = Invocation::new(&["add-order", "--json"])
        .rooted_at(&root)
        .argument("title=Laser components")
        .argument("colour=red")
        .argument("order_id=00000000-0000-4000-8000-000000000000")
        .argument("package_root=../..")
        .argument("items=none")
        .run();
    let document = response(&outcome);
    let refused: Vec<(&str, &str)> = document["error"]["identities"]
        .as_array()
        .expect("the refusal names every field it refused")
        .iter()
        .map(|identity| {
            (
                identity["value"].as_str().unwrap_or_default(),
                identity["kind"].as_str().unwrap_or_default(),
            )
        })
        .collect();

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_assignment_rejected");
    assert_eq!(
        refused,
        vec![
            ("colour", "unknown_field"),
            ("order_id", "immutable_identity"),
            ("package_root", "managed_technical_field"),
            ("items", "ordered_collection"),
        ],
        "each field names the rule it broke, in the core's own spelling"
    );
    assert_eq!(managed_snapshot(&root), before, "nothing was written");
}

/// A value the Order record cannot hold is refused naming the fact, on both commands.
///
/// **The claim is agreement**, so the three facts are asserted over both creating commands
/// and against the codes and identity kinds `order edit` reports for the same values —
/// `tests/order_and_item_edits.rs` is where the edit's own side of it is pinned. Two
/// commands describing one rule two ways is the defect: an assembled record that failed to
/// parse can only say that some fact was wrong, which is least useful in exactly the case
/// an atomic set makes commonest.
#[test]
fn a_value_the_record_cannot_hold_is_refused_naming_the_fact_on_both_commands() {
    for (assignment, code, kind, value) in [
        (
            "date_ordered=2026-02-30",
            "order_date_ordered_invalid",
            "order_date_ordered",
            "2026-02-30",
        ),
        (
            "date_ordered=the ides of March",
            "order_date_ordered_invalid",
            "order_date_ordered",
            "the ides of March",
        ),
        (
            "currency=dollars",
            "order_currency_invalid",
            "order_currency",
            "dollars",
        ),
        (
            "currency=usd",
            "order_currency_invalid",
            "order_currency",
            "usd",
        ),
        (
            "status=shipped",
            "order_status_invalid",
            "order_status",
            "shipped",
        ),
    ] {
        let (_tree, root) = initialized_package("add-order-unholdable");
        let before = managed_snapshot(&root);
        let created = Invocation::new(&["add-order", "--json"])
            .rooted_at(&root)
            .argument("title=Laser components")
            .argument(assignment)
            .run();
        let document = response(&created);

        assert_eq!(
            created.status,
            Some(1),
            "{assignment}: {}",
            created.stdout_text()
        );
        assert_eq!(document["error"]["code"], code, "{assignment}");
        assert_eq!(
            document["error"]["identities"][0]["kind"], kind,
            "{assignment}"
        );
        assert_eq!(
            document["error"]["identities"][0]["value"], value,
            "{assignment}"
        );
        assert_eq!(managed_snapshot(&root), before, "{assignment}");

        let (_candidate_tree, candidate_root, candidate) =
            crate::support::package_with_an_adoption_candidate("adopt-order-unholdable");
        let candidate_before = managed_snapshot(&candidate);
        // The required fact under test replaces the one the shared set supplies rather
        // than joining it: an assignment set naming one field twice is a different
        // refusal, and this is about the value rather than about the set.
        let field = assignment.split_once('=').expect("an assignment").0;
        let mut adoption_run = Invocation::new(&["adopt-order"])
            .argument(&candidate)
            .rooted_at(&candidate_root)
            .argument("--json");
        for supplied in crate::support::ADOPTED_FACTS {
            if !supplied.starts_with(&format!("{field}=")) {
                adoption_run = adoption_run.argument(supplied);
            }
        }
        let adopted = adoption_run.argument(assignment).run();
        let adoption = response(&adopted);

        assert_eq!(
            adopted.status,
            Some(1),
            "{assignment}: {}",
            adopted.stdout_text()
        );
        assert_eq!(adoption["error"]["code"], code, "{assignment}");
        assert_eq!(
            adoption["error"]["identities"][0]["kind"], kind,
            "{assignment}"
        );
        assert_eq!(
            adoption["error"]["identities"][0]["value"], value,
            "{assignment}"
        );
        assert_eq!(
            managed_snapshot(&candidate),
            candidate_before,
            "{assignment}"
        );
    }
}

/// A status that is not an established Order Status is refused naming the text supplied.
#[test]
fn a_status_that_is_not_one_is_refused_naming_it() {
    let (_tree, root) = initialized_package("add-order-status");

    let outcome = Invocation::new(&["add-order", "--json"])
        .rooted_at(&root)
        .argument("title=Laser components")
        .argument("status=shipped")
        .run();
    let document = response(&outcome);

    assert_eq!(outcome.status, Some(1), "{}", outcome.stdout_text());
    assert_eq!(document["error"]["code"], "order_status_invalid");
    assert_eq!(document["error"]["identities"][0]["value"], "shipped");
    // The identity's own `kind` is part of this contract too, and no cell of it records
    // this refusal — so a later command sharing the constructor and renaming the kind
    // would change what this response says with nothing to notice.
    assert_eq!(document["error"]["identities"][0]["kind"], "order_status");
}

/// Every route either command has to exit `2` carries the one shared usage identity.
#[test]
fn every_usage_error_these_commands_reach_carries_the_shared_identity() {
    for wrong in [
        // A Title beside an assignment, and two Titles.
        vec!["add-order", "--json", "Laser components", "status=ordered"],
        vec!["add-order", "--json", "Laser components", "Mirror mounts"],
        // A misspelled option, and one another family has.
        vec!["add-order", "--json", "--dry-runn"],
        vec!["add-order", "--json", "--all"],
        // Two output modes at once.
        vec!["add-order", "--json", "--quiet"],
        // A misspelling of the command itself.
        vec!["add-orders", "--json"],
        // Adoption with no candidate named at all.
        vec!["adopt-order", "--json"],
        vec!["adopt-order", "--json", "--uuid", "whatever"],
    ] {
        let (_tree, root) = initialized_package("new-order-usage-errors");
        let before = managed_snapshot(&root);

        let outcome = Invocation::new(&wrong).rooted_at(&root).run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(2),
            "{wrong:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(document["contract"], "spielorders.usage", "{wrong:?}");
        assert_eq!(document["error"]["code"], "usage_invalid", "{wrong:?}");
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{wrong:?}: nothing about the Package was attempted"
        );
    }
}

/// A creation succeeds only when the projected Order Directory name is free, and a taken
/// one is refused rather than given an invented suffix.
///
/// Driven by creating the same Order twice, because the projected basename is composed from
/// the Order's own facts: the second run projects the name the first one published.
///
/// **The refused path carries the Package's own spelling for it**, which is the rule in
/// [Decision traceability and contract-test conventions]: a refusal about a path inside a
/// Package owes that spelling exactly as a success does. Asserted here because this is the
/// occupied-destination refusal a real process can reach — the one the commit reaches is a
/// race no test may manufacture, and its spelling is held by the type instead.
///
/// [Decision traceability and contract-test conventions]: ../../../docs/development/traceability.md
#[test]
fn a_creation_onto_an_occupied_order_directory_is_refused() {
    let (_tree, root) = initialized_package("add-order-collision");
    let first = Invocation::new(&["add-order", "title=Laser components", "--json"])
        .rooted_at(&root)
        .run();
    assert_eq!(first.status, Some(0), "{}", first.stderr_text());
    // The directory the first run published, in the spelling that run reported it in —
    // which is what the refusal below owes for the same path.
    let occupied = response(&first)["data"]["order"]["paths"]["directory"].clone();
    assert!(
        occupied["package_relative_path"].is_string(),
        "the created Order Directory is inside the Package: {occupied}"
    );
    let before = managed_snapshot(&root);

    for extra in [Vec::new(), vec!["--dry-run"]] {
        let mut run =
            Invocation::new(&["add-order", "title=Laser components", "--json"]).rooted_at(&root);
        for argument in &extra {
            run = run.argument(argument);
        }
        let outcome = run.run();
        let document = response(&outcome);

        assert_eq!(
            outcome.status,
            Some(1),
            "{extra:?}: {}",
            outcome.stdout_text()
        );
        assert_eq!(
            document["error"]["code"], "order_directory_occupied",
            "{extra:?}"
        );
        let refused = &document["error"]["paths"][0];
        assert_eq!(
            refused["path"], occupied["absolute_path"],
            "{extra:?}: the refusal names the directory that is already there"
        );
        assert_eq!(
            refused["package_relative_path"], occupied["package_relative_path"],
            "{extra:?}: a refusal spells its path the way the success beside it does"
        );
        assert_eq!(
            managed_snapshot(&root),
            before,
            "{extra:?}: no second Order Directory, and no suffix invented"
        );
    }
}
