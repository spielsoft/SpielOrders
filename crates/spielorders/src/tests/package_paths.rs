//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define the SpielOrders package topology; Define
//! the Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution; Require two
//! parent segments in the Order package locator.

use crate::Failure;
use crate::tests::support::{
    PlatformFileSystem, QueuedUuids, ScriptedFileSystem, adoption, effects, minimal_package,
    opened, order_record, package_tree_with,
};
use crate::{Assignments, Report};

use crate::ports::FileSystemEntryKind;
use crate::{
    readme::ConfigurationPathKind, readme::ConfigurationPathReason, readme::Diagnostic,
    readme::DiagnosticCode, readme::DiagnosticEvidence,
};
use std::path::Path;

/// The one error diagnostic a Package load refuses this volume with.
fn damage(filesystem: &ScriptedFileSystem) -> Diagnostic {
    let Err(Failure::DamagedPackage(damaged)) = opened(Path::new("/package"), filesystem).result
    else {
        panic!("an unsafe configured path damages the Package")
    };
    let mut diagnostics = damaged
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .cloned();
    let diagnostic = diagnostics.next().expect("the damage is diagnosed");
    assert_eq!(diagnostics.next(), None, "one condition, one diagnostic");
    diagnostic
}

fn configured(config: &str) -> ScriptedFileSystem {
    minimal_package().with_file("/package/.spielorders/config.toml", config.as_bytes())
}

#[test]
fn every_unsafe_configured_path_shape_damages_the_package() {
    use ConfigurationPathKind::{OrdersPath, RemovedPath};
    use ConfigurationPathReason::*;
    let cases = [
        ("orders_path = \".\"", OrdersPath, ".", CurrentDirectory),
        (
            "orders_path = \"/outside\"",
            OrdersPath,
            "/outside",
            Absolute,
        ),
        (
            "orders_path = \"C:/outside\"",
            OrdersPath,
            "C:/outside",
            Absolute,
        ),
        (
            "removed_path = \"C:outside\"",
            RemovedPath,
            "C:outside",
            Absolute,
        ),
        (
            "orders_path = \"../outside\"",
            OrdersPath,
            "../outside",
            ParentTraversal,
        ),
        (
            "orders_path = \"Orders//Active\"",
            OrdersPath,
            "Orders//Active",
            EmptyComponent,
        ),
        (
            "orders_path = 'Orders\\Active'",
            OrdersPath,
            "Orders\\Active",
            PlatformSeparator,
        ),
        (
            "orders_path = \".spielorders/Orders\"",
            OrdersPath,
            ".spielorders/Orders",
            ReservedMetadata,
        ),
        (
            "removed_path = \"Removed/Old\"",
            RemovedPath,
            "Removed/Old",
            NotDirectChild,
        ),
    ];

    for (config, configuration_path, configured_value, reason) in cases {
        let diagnostic = damage(&configured(config));
        assert_eq!(
            diagnostic.evidence,
            DiagnosticEvidence::ConfigurationPathInvalid {
                configuration_path,
                configured_value: configured_value.to_owned(),
                reason,
            },
            "{config}"
        );
    }
}

#[test]
fn orders_and_removed_paths_cannot_overlap_lexically_or_after_link_resolution() {
    let linked = minimal_package()
        .with_directory("/package/Removed")
        .with_symlink("/package/Orders", "/package/Removed");
    for filesystem in [
        configured("orders_path = \"Removed/Active\"\nremoved_path = \"Removed\"\n"),
        configured("orders_path = \"Orders\"\nremoved_path = \"Orders\"\n"),
        linked,
    ] {
        assert_eq!(
            damage(&filesystem).code,
            DiagnosticCode::ConfigurationPathsOverlap
        );
    }
}

#[test]
fn nested_orders_resolve_but_an_existing_removed_path_must_be_an_ordinary_directory() {
    let config = "orders_path = \"Purchases/Active\"\nremoved_path = \"Retired\"\n";
    let valid = configured(config)
        .with_directory("/package/Purchases/Active")
        .with_directory("/package/Retired");
    assert!(
        opened(Path::new("/package"), &valid).result.is_ok(),
        "safe nested Orders and ordinary Removed paths resolve"
    );

    for kind in [
        FileSystemEntryKind::File,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        let invalid = configured(config)
            .with_directory("/package/Purchases/Active")
            .with_entry("/package/Retired", kind);
        assert_eq!(
            damage(&invalid).evidence,
            DiagnosticEvidence::ConfigurationPathInvalid {
                configuration_path: ConfigurationPathKind::RemovedPath,
                configured_value: "Retired".to_owned(),
                reason: ConfigurationPathReason::NotOrdinary,
            },
            "{kind:?}"
        );
    }
}

#[test]
fn package_locator_must_match_the_explicit_root_and_active_nested_placement() {
    let tree = package_tree_with("package-paths-locator", &("Purchases/Active", "Removed"));
    let root = std::fs::canonicalize(tree.path()).unwrap();
    let directory = root.join("Purchases/Active/Order");
    std::fs::create_dir_all(&directory).unwrap();
    let plan = crate::plan(
        &adoption(
            root.clone(),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Locator fixture")
                    .assign("date_ordered", "2026-07-19")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the nested directory can be adopted");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(QueuedUuids::of(&["8b6e3f0a-8ec5-4d47-95e1-412652a56d16"]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));
    let record = std::fs::read_to_string(order_record(&directory)).unwrap();
    assert!(
        record.contains("\"package_root\": \"../../..\""),
        "the nested locator reaches the explicit Package root: {record}"
    );

    for locator in ["../..", "../../../.."] {
        std::fs::write(
            order_record(&directory),
            record.replace("\"../../..\"", &format!("\"{locator}\"")),
        )
        .unwrap();
        let Ok(package) = opened(&root, &PlatformFileSystem).result else {
            panic!("{locator}: a mismatched locator damages only its Order");
        };
        assert!(package.contents.healthy_orders.is_empty(), "{locator}");
        assert!(
            package
                .contents
                .diagnostic_groups
                .iter()
                .flat_map(|group| &group.diagnostics)
                .any(|diagnostic| diagnostic.code == DiagnosticCode::OrderPackageLocatorMismatch),
            "{locator}: the Order is diagnosed with the mismatch"
        );
    }
}
