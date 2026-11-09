//! Decision sources: Define the SpielOrders package topology; Define the Rust CLI
//! capability contract; Define integrity, validation, and repair boundaries; Define
//! the stable M2 diagnostic vocabulary and evidence contract.
//!
//! The Orders Directory and the Removed Orders Directory must be two directories.
//! Whether they are is a question about the filesystem, not about the configured
//! text: a volume that resolves names without regard to case makes `Orders` and
//! `ORDERS` one directory, and a comparison of the two spellings sees two.
//!
//! Both places a configuration is judged ask it here — Package loading, which every
//! capability enters through, and Package initialization, which is where a
//! configuration becomes durable.
//!
//! The refusals are driven through a double that models the equivalence, so they are
//! exercised wherever this suite runs. On a case-sensitive volume the same two
//! spellings really are two directories and the configuration is legitimate, which
//! one test measures and asserts directly.

use crate::Coded;
use crate::Failure;
use crate::tests::support::{
    FailedCommit, PlatformFileSystem, ScriptedFileSystem, TemporaryTree, adopted_order, effects,
    initialization, initialized_package, opened, remove_order, resolves_case_insensitively,
};

use crate::{
    readme::ConfigurationPathKind, readme::ConfigurationPathReason,
    readme::ConfiguredOrdersDirectoryReason, readme::Diagnostic, readme::DiagnosticCode,
    readme::DiagnosticEvidence, readme::EvidenceScope, readme::Severity,
};
use std::fs;
use std::path::Path;

/// A Package whose configuration names the two managed paths by the given text.
///
/// Written into the durable configuration rather than passed to initialization,
/// because initialization refuses this pair too and the load path has to be reachable
/// on its own.
fn package_configured(
    label: &str,
    orders_path: &str,
    removed_path: &str,
) -> (TemporaryTree, std::path::PathBuf) {
    let (tree, root) = initialized_package(label);
    fs::write(
        root.join(".spielorders/config.toml"),
        format!("orders_path = \"{orders_path}\"\nremoved_path = \"{removed_path}\"\n"),
    )
    .expect("the configuration under test is written");
    (tree, root)
}

/// The one diagnostic a Package carries, so a test that means to assert its contents
/// cannot pass on some other diagnostic that happened to be emitted beside it.
fn sole_diagnostic(load: &crate::Answer<crate::Package>) -> &Diagnostic {
    let Err(Failure::DamagedPackage(damaged)) = &load.result else {
        panic!("the Package is damaged by its configuration, not {load:?}")
    };
    let diagnostics = damaged
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostics: {diagnostics:?}"
    );
    diagnostics[0]
}

#[track_caller]
fn assert_paths_overlap(
    diagnostic: &Diagnostic,
    root: &Path,
    orders_path: &str,
    removed_path: &str,
) {
    assert_eq!(diagnostic.code, DiagnosticCode::ConfigurationPathsOverlap);
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.scope, EvidenceScope::Package);
    assert_eq!(
        diagnostic.primary_location.absolute_path,
        root.join(".spielorders/config.toml")
    );
    assert_eq!(
        diagnostic.primary_location.package_relative_path.as_deref(),
        Some(".spielorders/config.toml")
    );
    assert!(diagnostic.related_locations.is_empty());
    assert_eq!(
        diagnostic.evidence,
        DiagnosticEvidence::ConfigurationPathsOverlap {
            orders_path: orders_path.to_owned(),
            removed_path: removed_path.to_owned(),
        }
    );
}

#[test]
fn two_managed_paths_that_name_one_directory_are_refused_at_load() {
    let (tree, root) = package_configured("one-directory-load", "Orders", "ORDERS");
    let volume = ScriptedFileSystem::case_insensitive_over(tree.path());

    let load = opened(&root, &volume);

    assert_paths_overlap(sole_diagnostic(&load), &root, "Orders", "ORDERS");
}

#[test]
fn a_removed_orders_directory_that_contains_the_orders_directory_is_refused() {
    let (tree, root) = initialized_package("one-directory-nested");
    fs::create_dir_all(root.join("purchases/Active")).expect("the nested Orders Directory exists");
    fs::write(
        root.join(".spielorders/config.toml"),
        b"orders_path = \"purchases/Active\"\nremoved_path = \"PURCHASES\"\n",
    )
    .expect("the nested configuration is written");

    let load = opened(
        &root,
        &ScriptedFileSystem::case_insensitive_over(tree.path()),
    );

    assert_paths_overlap(
        sole_diagnostic(&load),
        &root,
        "purchases/Active",
        "PURCHASES",
    );
}

#[test]
fn a_case_differing_pair_is_refused_exactly_where_the_volume_merges_it() {
    let (_tree, root) = package_configured("one-directory-real", "Orders", "ORDERS");

    let load = opened(&root, &PlatformFileSystem);

    if resolves_case_insensitively(&root) {
        assert_paths_overlap(sole_diagnostic(&load), &root, "Orders", "ORDERS");
    } else {
        let Ok(package) = load.result else {
            panic!("two genuinely different directories are a usable configuration")
        };
        assert_eq!(
            package
                .contents
                .context
                .removed_orders_directory
                .as_ref()
                .map(|location| location.absolute_path.as_path()),
            Some(root.join("ORDERS").as_path())
        );
    }
}

#[test]
fn a_removed_orders_directory_that_does_not_exist_yet_leaves_the_configuration_usable() {
    let (tree, root) = package_configured("absent-removed", "Orders", "Retired");
    assert!(!root.join("Retired").exists());

    let Ok(package) = opened(
        &root,
        &ScriptedFileSystem::case_insensitive_over(tree.path()),
    )
    .result
    else {
        panic!("a Removed Orders Directory is created lazily and its absence is ordinary")
    };

    assert_eq!(
        package
            .contents
            .context
            .removed_orders_directory
            .as_ref()
            .map(|location| location.absolute_path.as_path()),
        Some(root.join("Retired").as_path())
    );
}

#[test]
fn an_order_in_a_package_damaged_by_its_configuration_cannot_be_removed() {
    let (tree, root) = initialized_package("one-directory-removal");
    let order = adopted_order(
        &root,
        "2026_08_22_Laser",
        "33333333-3333-4333-8333-333333333333",
    );
    fs::write(
        root.join(".spielorders/config.toml"),
        b"orders_path = \"Orders\"\nremoved_path = \"ORDERS\"\n",
    )
    .expect("the collided configuration is written");

    let error = crate::plan(
        &remove_order(order.clone()),
        &effects(&ScriptedFileSystem::case_insensitive_over(tree.path())),
    )
    .result
    .expect_err("a Package whose managed paths are one directory cannot remove an Order");

    assert!(
        matches!(error, Failure::DamagedPackage(_)),
        "removal must name the damaged configuration, not {error:?}"
    );
}

#[test]
fn an_orders_directory_name_this_application_cannot_express_is_reported_as_a_package_error() {
    let (_tree, root) = initialized_package("unnameable-orders");
    fs::remove_dir(root.join("Orders")).expect("the ordinary Orders Directory is removed");
    let target = root.join("C:elsewhere");
    fs::create_dir(&target).expect("the link target is created");
    std::os::unix::fs::symlink(&target, root.join("Orders")).expect("the Orders link is created");

    let load = opened(&root, &PlatformFileSystem);

    let diagnostic = sole_diagnostic(&load);
    assert_eq!(
        diagnostic.code,
        DiagnosticCode::ConfiguredOrdersDirectoryNameUnusable
    );
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.scope, EvidenceScope::Package);
    assert_eq!(
        diagnostic.primary_location.absolute_path,
        root.join("Orders")
    );
    assert_eq!(
        diagnostic.primary_location.package_relative_path.as_deref(),
        Some("Orders")
    );
    assert!(diagnostic.related_locations.is_empty());
    assert_eq!(
        diagnostic.evidence,
        DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable {
            configured_value: "Orders".to_owned(),
        }
    );
}

#[test]
fn an_orders_link_to_something_that_is_not_a_directory_reports_that_it_is_not_one() {
    // The unusable-name refusal is asked last, so the condition an existing refusal
    // already named keeps naming it. Both answers are true of this link; the one that
    // tells the user their Orders Directory is a file is the more actionable.
    let (_tree, root) = initialized_package("unnameable-and-not-a-directory");
    fs::remove_dir(root.join("Orders")).expect("the ordinary Orders Directory is removed");
    let target = root.join("C:elsewhere");
    fs::write(&target, b"not a directory").expect("the link target is written");
    std::os::unix::fs::symlink(&target, root.join("Orders")).expect("the Orders link is created");

    let load = opened(&root, &PlatformFileSystem);

    let diagnostic = sole_diagnostic(&load);
    assert_eq!(
        diagnostic.code,
        DiagnosticCode::ConfiguredOrdersDirectoryUnavailable
    );
    assert_eq!(
        diagnostic.evidence,
        DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable {
            configured_value: "Orders".to_owned(),
            reason: ConfiguredOrdersDirectoryReason::NotDirectory,
        }
    );
}

#[test]
fn a_removed_orders_directory_reached_through_a_link_is_refused_as_not_ordinary() {
    // The Removed Orders Directory is a direct child, so the only component a link
    // could redirect is the child itself, and that is refused before the resolution
    // gets a chance to end at a name no configured value was checked against. This is
    // what answers the Orders side's question for the Removed side.
    let (_tree, root) = package_configured("linked-removed", "Orders", "Retired");
    let target = root.join("C:elsewhere");
    fs::create_dir(&target).expect("the link target is created");
    std::os::unix::fs::symlink(&target, root.join("Retired")).expect("the link is created");

    let load = opened(&root, &PlatformFileSystem);

    let diagnostic = sole_diagnostic(&load);
    assert_eq!(diagnostic.code, DiagnosticCode::ConfigurationPathInvalid);
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.scope, EvidenceScope::Package);
    assert_eq!(
        diagnostic.evidence,
        DiagnosticEvidence::ConfigurationPathInvalid {
            configuration_path: ConfigurationPathKind::RemovedPath,
            configured_value: "Retired".to_owned(),
            reason: ConfigurationPathReason::NotOrdinary,
        }
    );
}

#[test]
fn initialization_refuses_two_paths_that_already_name_one_directory() {
    let tree = TemporaryTree::new("initialization-one-directory-existing");
    fs::create_dir(tree.path().join("Orders")).expect("the existing directory is created");

    let error = crate::plan(
        &initialization(tree.path(), &("Orders", "ORDERS")),
        &effects(&ScriptedFileSystem::case_insensitive_over(tree.path())),
    )
    .result
    .expect_err("two configured paths that already name one directory are refused");

    assert!(
        matches!(error, Failure::ManagedPathsOverlap { .. }),
        "unexpected refusal: {error:?}"
    );
    assert!(!tree.path().join(".spielorders").exists());
}

#[test]
fn initialization_refuses_two_paths_that_would_name_one_directory_once_created() {
    let tree = TemporaryTree::new("initialization-one-directory-created");
    let root = fs::canonicalize(tree.path()).expect("the selected root canonicalizes");
    let volume = ScriptedFileSystem::case_insensitive_over(tree.path());
    let plan = crate::plan(
        &initialization(tree.path(), &("Orders", "ORDERS")),
        &effects(&volume),
    )
    .result
    .expect("neither directory exists yet, so planning cannot see the collision");

    let report = crate::commit(plan, &effects(&volume)).result;

    let Err(failed) = report else {
        panic!("initialization must not mint a Package its own loader refuses: {report:?}")
    };
    let (failure, cleanup_failures) = (failed.failure(), failed.cleanup_failures());
    assert_eq!(failure.location.absolute_path, root.join("ORDERS"));
    assert!(cleanup_failures.is_empty(), "{cleanup_failures:?}");
    assert_eq!(
        fs::read_dir(tree.path())
            .expect("the selected root is still listable")
            .count(),
        0,
        "a refused initialization leaves nothing behind"
    );
}

#[test]
fn the_configured_orders_directory_name_code_keeps_its_wire_spelling() {
    // A deliberate interface lock. The closed M2 diagnostic contract makes a code's
    // spelling a compatibility promise to every adapter and test that reads one, so
    // this spelling is public and renaming it is an incompatible change.
    assert_eq!(
        DiagnosticCode::ConfiguredOrdersDirectoryNameUnusable.code(),
        "configured_orders_directory_name_unusable",
        "the code's spelling is a stable contract, not an implementation detail"
    );
}
