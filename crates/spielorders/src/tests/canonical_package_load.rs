//! Decision sources: Define the stable diagnostic vocabulary and evidence
//! contract; Define the complete Package read projection and load-state
//! contract; Decide how Package Index freshness is proven without canonical rereads.

use crate::Failure;
use crate::readme::ReadmeUnavailableReason;
use crate::tests::support::{
    Operation, PlatformFileSystem, ScriptedFileSystem, TemporaryTree, add_contact, adoption,
    create_order, create_vendor, current_readme, edit_vendor, effects, failed_location,
    failed_path, minimal_package, opened, package_scope, set_vendor_hidden, validated_package,
    write_readme,
};
use crate::{Assignments, VendorRef};
use std::ops::Deref;

use crate::Coded;
use crate::canonical::{MetadataState, PackageIndexState};

use crate::operations::FailedPath;
use crate::ports::FileSystemEntryKind;
use crate::{
    readme::DiagnosticCode, readme::DiagnosticEvidence, readme::EvidenceScope, readme::Severity,
};

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

const VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const INVALID_ORDER_ITEM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/order-invalid-item.json"
));
const INVALID_VENDOR_CONTACT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/vendor-invalid-contact.json"
));
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

#[test]
fn unmanaged_and_minimal_current_packages_have_distinct_canonical_results() {
    let unmanaged_filesystem =
        ScriptedFileSystem::private("canonical-load").with_directory("/unmanaged");

    let unmanaged = opened(Path::new("/unmanaged"), &unmanaged_filesystem);

    let Err(Failure::UnmanagedPackage(unmanaged)) = unmanaged.result else {
        panic!("a wholly absent root metadata directory is unmanaged")
    };
    assert_eq!(unmanaged.root.absolute_path, Path::new("/unmanaged"));
    assert_eq!(unmanaged.root.package_relative_path.as_deref(), None);

    let managed_filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_file("/package/.spielorders/vendors.json", VENDORS)
        .with_directory("/package/Orders");

    let managed = opened(Path::new("/package"), &managed_filesystem);

    let Ok(package) = managed.result else {
        panic!("current canonical inputs produce a pre-index candidate")
    };
    assert_eq!(
        package
            .contents
            .context
            .root
            .package_relative_path
            .as_deref(),
        Some(".")
    );
    assert_eq!(package.contents.metadata_version, 1);
    assert!(package.contents.healthy_orders.is_empty());
    assert!(package.contents.damaged_orders.is_empty());
    assert!(package.contents.vendors.is_empty());
    assert!(package.contents.diagnostic_groups.is_empty());
    assert_eq!(package.contents.index.state, PackageIndexState::Missing);
}

#[test]
fn missing_package_canonical_artifact_is_diagnostic_only_without_partial_projection() {
    let filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_directory("/package/Orders");

    let result = opened(Path::new("/package"), &filesystem).result;

    let Err(Failure::DamagedPackage(damaged)) = result else {
        panic!("missing canonical Vendors must damage the Package")
    };
    assert_eq!(damaged.context.root.absolute_path, Path::new("/package"));
    assert_eq!(damaged.index.state, PackageIndexState::Missing);
    let [group] = damaged.diagnostic_groups.as_slice() else {
        panic!("the missing artifact has exactly one Package diagnostic group")
    };
    let [diagnostic] = group.diagnostics.as_slice() else {
        panic!("the missing artifact is diagnosed exactly once")
    };
    assert_eq!(diagnostic.code, DiagnosticCode::ManagedArtifactMissing);
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.scope, EvidenceScope::Package);
    assert_eq!(
        diagnostic.primary_location.package_relative_path.as_deref(),
        Some(".spielorders/vendors.json")
    );
}

#[test]
fn healthy_order_projection_contains_complete_facts_totals_readme_vendor_and_reusable_values() {
    let filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_file("/package/.spielorders/vendors.json", FULL_VENDORS)
        .with_directory("/package/Orders")
        .with_directory("/package/Orders/Optics")
        .with_directory("/package/Orders/Optics/.spielorders")
        .with_file("/package/Orders/Optics/.spielorders/order.json", FULL_ORDER)
        .with_file(
            "/package/Orders/Optics/README.md",
            b"# Exact Order notes\r\n",
        );

    let result = opened(Path::new("/package"), &filesystem).result;

    let Ok(package) = result else {
        panic!("healthy canonical state produces a pre-index candidate")
    };
    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("the healthy Order is projected exactly once")
    };
    assert_eq!(
        order.paths.directory.package_relative_path.as_deref(),
        Some("Orders/Optics")
    );
    assert_eq!(
        order.paths.metadata.package_relative_path.as_deref(),
        Some("Orders/Optics/.spielorders")
    );
    assert_eq!(
        order.paths.order.package_relative_path.as_deref(),
        Some("Orders/Optics/.spielorders/order.json")
    );
    assert_eq!(order.document.title.deref(), "Laser components");
    assert_eq!(order.item_totals, &[169.0]);
    assert_eq!(order.order_total, 169.0);
    assert_eq!(
        order.readme_text.as_deref(),
        Some("# Exact Order notes\r\n")
    );
    assert!(order.document.vendor_id.is_some());
    assert_eq!(package.contents.vendors[0].name.deref(), "Tarnwick");
    assert_eq!(
        package.contents.vendor_for_order(0).unwrap().name.deref(),
        "Tarnwick"
    );
    assert!(package.contents.damaged_orders.is_empty());
    assert!(package.contents.diagnostic_groups.is_empty());
    assert_eq!(
        package.contents.projections.ordering_institutions[0].value,
        "Example Institute"
    );
    assert_eq!(
        package.contents.projections.orderers[0].value,
        "Ada Lovelace"
    );
    assert_eq!(
        package.contents.projections.projects[0].value,
        "Telescope"
    );
    assert_eq!(
        package.contents.projections.order_groups[0].value,
        "Summer build"
    );
    assert_eq!(package.contents.projections.order_groups[0].usage_count, 1);
}

#[test]
fn malformed_order_is_quarantined_beside_a_healthy_order_with_untrusted_identity_absent() {
    let filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_file("/package/.spielorders/vendors.json", VENDORS)
        .with_directory("/package/Orders")
        .with_directory("/package/Orders/Bad")
        .with_file(
            "/package/Orders/Bad/.spielorders/order.json",
            b"{ malformed",
        )
        .with_directory("/package/Orders/Good")
        .with_file(
            "/package/Orders/Good/.spielorders/order.json",
            MINIMAL_ORDER,
        );

    let result = opened(Path::new("/package"), &filesystem).result;

    let Ok(package) = result else {
        panic!("Order-local damage does not damage the Package")
    };
    assert_eq!(package.contents.healthy_orders.len(), 1);
    let [damaged] = package.contents.damaged_orders.as_slice() else {
        panic!("the damaged candidate has one placeholder")
    };
    assert_eq!(
        damaged.paths.directory.package_relative_path.as_deref(),
        Some("Orders/Bad")
    );
    assert_eq!(damaged.order_id.as_ref(), None);
    let group = package
        .contents
        .diagnostic_groups
        .iter()
        .find(|group| group.location == damaged.paths.directory)
        .expect("the placeholder has one matching Order diagnostic group");
    assert_eq!(group.scope, EvidenceScope::Order);
    assert_eq!(
        group.diagnostics[0].code,
        DiagnosticCode::ManagedArtifactInvalidSyntax
    );
}

#[test]
fn package_artifact_encoding_syntax_shape_and_version_failures_keep_distinct_codes() {
    let cases: [(&str, &[u8], DiagnosticCode); 7] = [
        (
            "/package/.spielorders/config.toml",
            b"orders_path = \xff",
            DiagnosticCode::ManagedArtifactInvalidEncoding,
        ),
        (
            "/package/.spielorders/config.toml",
            b"orders_path = \"",
            DiagnosticCode::ManagedArtifactInvalidSyntax,
        ),
        (
            "/package/.spielorders/config.toml",
            b"unknown = true\n",
            DiagnosticCode::ManagedArtifactInvalidShape,
        ),
        (
            "/package/.spielorders/version.json",
            b"{ malformed",
            DiagnosticCode::ManagedArtifactInvalidSyntax,
        ),
        (
            "/package/.spielorders/version.json",
            br#"{"metadata_version": 999}"#,
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
        (
            "/package/.spielorders/vendors.json",
            br#"{"schema_version":1,"vendors":{}}"#,
            DiagnosticCode::ManagedArtifactInvalidShape,
        ),
        (
            "/package/.spielorders/vendors.json",
            br#"{"schema_version":2,"vendors":[]}"#,
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
    ];

    for (path, bytes, expected_code) in cases {
        let filesystem = minimal_package().with_file(path, bytes);

        let result = opened(Path::new("/package"), &filesystem).result;

        let Err(
            ref unusable @ (Failure::DamagedPackage(ref damaged)
            | Failure::PackageMetadataUnsupported(ref damaged)),
        ) = result
        else {
            panic!("{path} failure must be Package diagnostic-only")
        };
        assert_eq!(
            matches!(unusable, Failure::PackageMetadataUnsupported(_)),
            expected_code == DiagnosticCode::ManagedArtifactWrongVersion,
            "an uninterpretable version is its own refusal for {path}"
        );
        let codes = damaged
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(
            codes.contains(&expected_code),
            "wrong code for {path}: {codes:?}"
        );
    }
}

#[test]
fn an_unavailable_orders_directory_damages_and_disallowed_direct_children_only_warn() {
    let unavailable = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_file("/package/.spielorders/vendors.json", VENDORS)
        .with_file("/package/Orders", b"not a directory");

    let Err(Failure::DamagedPackage(damaged)) = opened(Path::new("/package"), &unavailable).result
    else {
        panic!("an Orders Directory that is not a directory damages the Package")
    };
    assert!(
        damaged
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .any(|diagnostic| diagnostic.code
                == DiagnosticCode::ConfiguredOrdersDirectoryUnavailable)
    );

    let warned = minimal_package()
        .with_file("/package/Orders/loose.txt", b"opaque")
        .with_entry(
            "/package/Orders/VisibleAlias",
            FileSystemEntryKind::SymbolicLink,
        )
        .with_entry("/package/Orders/.hidden", FileSystemEntryKind::SymbolicLink)
        .with_entry("/package/Orders/device", FileSystemEntryKind::Other);
    let Ok(package) = opened(Path::new("/package"), &warned).result else {
        panic!("structural warnings do not block the canonical candidate")
    };
    let warnings = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 3);
    assert!(warnings.iter().all(|warning| {
        warning.code == DiagnosticCode::OrdersDirectoryEntryDisallowed
            && warning.severity == Severity::Warning
            && warning.scope == EvidenceScope::Package
    }));
}

#[test]
fn path_failure_retains_and_orders_all_previously_observed_package_diagnostics() {
    let filesystem = minimal_package()
        .with_file(
            "/package/.spielorders/version.json",
            br#"{"metadata_version":999}"#,
        )
        .with_file("/package/Orders", b"not a directory");

    let Err(Failure::PackageMetadataUnsupported(damaged)) =
        opened(Path::new("/package"), &filesystem).result
    else {
        panic!("the independently observed Package failures are diagnostic-only")
    };
    let diagnostics = damaged
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::ManagedArtifactWrongVersion })
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ConfiguredOrdersDirectoryUnavailable
    }));
}

#[test]
fn early_package_damage_normalizes_diagnostics_before_returning() {
    let filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/version.json", b"{ malformed")
        .with_directory("/package/Orders");

    let Err(Failure::DamagedPackage(damaged)) = opened(Path::new("/package"), &filesystem).result
    else {
        panic!("multiple required-artifact failures are diagnostic-only")
    };
    let diagnostics = damaged
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.windows(2).all(|pair| pair[0] <= pair[1]));
    let unique = diagnostics.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), diagnostics.len());
}

#[test]
fn duplicate_vendor_id_and_name_damage_the_package_with_trustworthy_evidence() {
    let vendors = r#"{
  "schema_version": 1,
  "vendors": [
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":" Straße Instruments ","contacts":[]},
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Other","contacts":[]},
    {"vendor_id":"f1990d31-06aa-42e9-97c8-088e3e894822","name":"STRASSE Instruments","contacts":[]}
  ]
}"#;
    let filesystem =
        minimal_package().with_file("/package/.spielorders/vendors.json", vendors.as_bytes());

    let result = opened(Path::new("/package"), &filesystem).result;

    let Err(Failure::DamagedPackage(damaged)) = result else {
        panic!("an untrustworthy Vendor aggregate damages the Package")
    };
    let diagnostics = damaged
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::VendorIdDuplicate)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::VendorNameDuplicate)
    );
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic.scope == EvidenceScope::Package
            && diagnostic.severity == Severity::Error
            && diagnostic.primary_location.package_relative_path
                == Some(".spielorders/vendors.json".to_string())
    }));
}

#[test]
fn semantic_locator_and_vendor_reference_failures_quarantine_only_their_orders() {
    let minimal = String::from_utf8(MINIMAL_ORDER.to_vec()).expect("fixture is UTF-8");
    let semantic = minimal.replacen("2026-07-19", "2026-02-31", 1);
    let locator = minimal
        .replacen(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "f1111111-1111-4111-8111-111111111111",
            1,
        )
        .replacen("../..", "../../..", 1);
    let unresolved = minimal
        .replacen(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "a2222222-2222-4222-8222-222222222222",
            1,
        )
        .replacen(
            "  \"status\": \"in_preparation\",",
            "  \"status\": \"in_preparation\",\n  \"vendor_id\": \"e0990d31-06aa-42e9-97c8-088e3e894822\",",
            1,
        );
    let healthy = minimal.replacen(
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "b3333333-3333-4333-8333-333333333333",
        1,
    );
    let filesystem = minimal_package()
        .with_directory("/package/Orders/A-Semantic")
        .with_file(
            "/package/Orders/A-Semantic/.spielorders/order.json",
            semantic.as_bytes(),
        )
        .with_directory("/package/Orders/B-Locator")
        .with_file(
            "/package/Orders/B-Locator/.spielorders/order.json",
            locator.as_bytes(),
        )
        .with_directory("/package/Orders/C-Unresolved")
        .with_file(
            "/package/Orders/C-Unresolved/.spielorders/order.json",
            unresolved.as_bytes(),
        )
        .with_directory("/package/Orders/D-Healthy")
        .with_file(
            "/package/Orders/D-Healthy/.spielorders/order.json",
            healthy.as_bytes(),
        );

    let result = opened(Path::new("/package"), &filesystem).result;

    let Ok(package) = result else {
        panic!("Order-local failures preserve the Package candidate")
    };
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(package.contents.damaged_orders.len(), 3);
    assert!(
        package
            .contents
            .damaged_orders
            .iter()
            .all(|order| order.order_id.is_some())
    );
    let codes = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&DiagnosticCode::OrderSemanticInvalid));
    assert!(codes.contains(&DiagnosticCode::OrderPackageLocatorMismatch));
    assert!(codes.contains(&DiagnosticCode::VendorReferenceUnresolved));
    assert_eq!(
        package
            .contents
            .damaged_orders
            .iter()
            .map(|order| order
                .paths
                .directory
                .package_relative_path
                .as_deref()
                .unwrap())
            .collect::<Vec<_>>(),
        vec![
            "Orders/A-Semantic",
            "Orders/B-Locator",
            "Orders/C-Unresolved",
        ]
    );
}

#[test]
fn every_duplicate_order_id_participant_is_damaged_with_the_complete_conflict_set() {
    let filesystem = minimal_package()
        .with_directory("/package/Orders/C")
        .with_file("/package/Orders/C/.spielorders/order.json", MINIMAL_ORDER)
        .with_directory("/package/Orders/A")
        .with_file("/package/Orders/A/.spielorders/order.json", MINIMAL_ORDER)
        .with_directory("/package/Orders/B")
        .with_file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER);

    let result = opened(Path::new("/package"), &filesystem).result;

    let Ok(package) = result else {
        panic!("duplicate identities remain Order-local damage")
    };
    assert!(package.contents.healthy_orders.is_empty());
    assert_eq!(package.contents.damaged_orders.len(), 3);
    assert!(package.contents.damaged_orders.iter().all(|order| {
        order.order_id.as_deref() == Some("8b6e3f0a-8ec5-4d47-95e1-412652a56d16")
    }));
    let duplicate_diagnostics = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::OrderIdDuplicate)
        .collect::<Vec<_>>();
    assert_eq!(duplicate_diagnostics.len(), 3);
    for diagnostic in duplicate_diagnostics {
        let DiagnosticEvidence::OrderIdDuplicate {
            conflict_locations, ..
        } = &diagnostic.evidence
        else {
            panic!("duplicate diagnostic carries typed conflict evidence")
        };
        assert_eq!(
            conflict_locations
                .iter()
                .map(|location| location.package_relative_path.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["Orders/A", "Orders/B", "Orders/C"]
        );
    }
}

#[test]
fn order_artifact_failures_are_distinct_and_preserve_only_trustworthy_identity() {
    let order_path = "/package/Orders/Bad/.spielorders/order.json";
    let missing_items = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Missing items",
  "date_ordered": "2026-07-19",
  "status": "ordered"
}"#;
    let malformed_id = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "NOT-AN-ID",
  "title": "Bad identity",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "items": []
}"#;
    let cases = [
        (
            minimal_package().with_directory("/package/Orders/Bad"),
            DiagnosticCode::ManagedArtifactMissing,
            None,
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_entry(order_path, FileSystemEntryKind::Directory),
            DiagnosticCode::ManagedArtifactWrongKind,
            None,
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(order_path, MINIMAL_ORDER)
                .failing(Operation::Read, order_path),
            DiagnosticCode::ManagedArtifactUnreadable,
            None,
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(order_path, &[0xff]),
            DiagnosticCode::ManagedArtifactInvalidEncoding,
            None,
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(order_path, missing_items),
            DiagnosticCode::ManagedArtifactInvalidShape,
            Some("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(order_path, malformed_id),
            DiagnosticCode::ManagedArtifactInvalidShape,
            None,
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(
                    order_path,
                    br#"{"schema_version":2,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Future","date_ordered":"2026-07-19","status":"ordered","items":[]}"#,
                ),
            DiagnosticCode::ManagedArtifactWrongVersion,
            None,
        ),
    ];

    for (filesystem, expected_code, expected_id) in cases {
        let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
            panic!("Order artifact failure must remain Order-local")
        };
        let [damaged] = package.contents.damaged_orders.as_slice() else {
            panic!("each failed candidate produces one placeholder")
        };
        assert_eq!(damaged.order_id.as_deref(), expected_id);
        assert!(
            package
                .contents
                .diagnostic_groups
                .iter()
                .flat_map(|group| &group.diagnostics)
                .any(|diagnostic| diagnostic.code == expected_code)
        );
    }
}

#[test]
fn duplicate_identity_detection_includes_participants_with_other_order_damage() {
    let invalid_date = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let filesystem = minimal_package()
        .with_directory("/package/Orders/A")
        .with_file(
            "/package/Orders/A/.spielorders/order.json",
            invalid_date.as_bytes(),
        )
        .with_directory("/package/Orders/B")
        .with_file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER);

    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("composed Order-local failures preserve the Package")
    };
    assert!(package.contents.healthy_orders.is_empty());
    assert_eq!(package.contents.damaged_orders.len(), 2);
    assert_eq!(
        package
            .contents
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::OrderIdDuplicate)
            .count(),
        2
    );
}

#[test]
fn healthy_order_sorting_drives_unicode_reusable_value_spelling_counts_and_order() {
    let order = |id: &str, title: &str, date: &str, value: &str| {
        String::from_utf8(MINIMAL_ORDER.to_vec())
            .expect("fixture is UTF-8")
            .replacen("8b6e3f0a-8ec5-4d47-95e1-412652a56d16", id, 1)
            .replacen("Laser components", title, 1)
            .replacen("2026-07-19", date, 1)
            .replacen(
                "  \"status\": \"in_preparation\",",
                &format!(
                    "  \"status\": \"in_preparation\",\n  \"orderer\": \"{value}\",\n  \"ordering_institution\": \"{value}\",\n  \"project\": \"{value}\",\n  \"order_group\": \"{value}\","
                ),
                1,
            )
    };
    let first = order(
        "a1111111-1111-4111-8111-111111111111",
        "  Alpha  ",
        "2026-07-19",
        " Straße Team ",
    );
    let second = order(
        "b2222222-2222-4222-8222-222222222222",
        "Zulu",
        "2026-07-19",
        "STRASSE TEAM",
    );
    let older = order(
        "c3333333-3333-4333-8333-333333333333",
        "Newest path but older date",
        "2025-01-01",
        "Sample Academy",
    );
    let filesystem = minimal_package()
        .with_directory("/package/Orders/Z-Older")
        .with_file(
            "/package/Orders/Z-Older/.spielorders/order.json",
            older.as_bytes(),
        )
        .with_directory("/package/Orders/B-First")
        .with_file(
            "/package/Orders/B-First/.spielorders/order.json",
            first.as_bytes(),
        )
        .with_directory("/package/Orders/A-Second")
        .with_file(
            "/package/Orders/A-Second/.spielorders/order.json",
            second.as_bytes(),
        );

    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("valid values produce a canonical candidate")
    };
    assert_eq!(
        package
            .contents
            .healthy_orders
            .iter()
            .map(|order| order.document.title.deref())
            .collect::<Vec<_>>(),
        vec!["Alpha", "Zulu", "Newest path but older date"]
    );
    for values in [
        package.contents.projections.ordering_institutions,
        package.contents.projections.orderers,
        package.contents.projections.projects,
        package.contents.projections.order_groups,
    ] {
        assert_eq!(
            values
                .iter()
                .map(|value| (value.value.as_str(), value.usage_count))
                .collect::<Vec<_>>(),
            vec![("Sample Academy", 1), ("Straße Team", 2)]
        );
    }
}

#[test]
fn order_readme_absence_and_unavailability_keep_orders_healthy_with_exact_cache_semantics() {
    let absent = minimal_package()
        .with_directory("/package/Orders/Order")
        .with_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
    let Ok(absent_package) = opened(Path::new("/package"), &absent).result else {
        panic!("README absence is ordinary")
    };
    assert_eq!(
        absent_package.contents.healthy_orders[0]
            .readme_text
            .as_deref(),
        Some("")
    );
    assert!(absent_package.contents.diagnostic_groups.is_empty());

    for filesystem in [
        minimal_package()
            .with_directory("/package/Orders/Order")
            .with_file(
                "/package/Orders/Order/.spielorders/order.json",
                MINIMAL_ORDER,
            )
            .with_entry(
                "/package/Orders/Order/README.md",
                FileSystemEntryKind::Directory,
            ),
        minimal_package()
            .with_directory("/package/Orders/Order")
            .with_file(
                "/package/Orders/Order/.spielorders/order.json",
                MINIMAL_ORDER,
            )
            .with_file("/package/Orders/Order/README.md", &[0xff]),
    ] {
        let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
            panic!("README unavailability does not damage an Order")
        };
        assert_eq!(package.contents.healthy_orders.len(), 1);
        assert_eq!(
            package.contents.healthy_orders[0].readme_text, None,
            "a README that exists and could not be read carries no text, \
             which the absent case above shows is not the same as an empty one"
        );
        let [group] = package.contents.diagnostic_groups.as_slice() else {
            panic!("one unavailable README produces one document group")
        };
        assert_eq!(group.scope, EvidenceScope::Document);
        assert_eq!(group.diagnostics[0].code, DiagnosticCode::ReadmeUnavailable);
        assert_eq!(group.diagnostics[0].severity, Severity::Warning);
    }
}

#[test]
fn denied_package_operations_remain_distinct_from_other_io_failures() {
    for denied in [false, true] {
        for (operation, path) in [
            (Operation::Canonicalize, "/package"),
            (Operation::SymlinkMetadata, "/package/.spielorders"),
            (
                Operation::SymlinkMetadata,
                "/package/.spielorders/config.toml",
            ),
            (Operation::ReadDirectory, "/package/Orders"),
        ] {
            let filesystem = if denied {
                minimal_package().denying(operation, path)
            } else {
                minimal_package().failing(operation, path)
            };
            let Err(opened) = opened(Path::new("/package"), &filesystem).result else {
                panic!("a failed operation refuses the load")
            };
            let Err(validated) = validated_package(Path::new("/package"), &filesystem).result
            else {
                panic!("validation must preserve a failed operation")
            };
            for observed in [&opened, &validated] {
                assert_eq!(
                    matches!(observed, Failure::PackageAccessDenied(_)),
                    denied,
                    "{observed:?}"
                );
                assert!(
                    matches!(
                        observed,
                        Failure::PackageAccessDenied(_) | Failure::PackageOperationFailed(_)
                    ),
                    "{observed:?}"
                );
                assert_eq!(observed, &opened);
            }
        }
    }
}

#[test]
fn a_denied_index_read_cannot_trigger_a_replacement() {
    let filesystem = minimal_package()
        .with_file("/package/.spielorders/index.json", b"private cache")
        .denying(Operation::Read, "/package/.spielorders/index.json");
    let Err(Failure::PackageAccessDenied(failure)) =
        opened(Path::new("/package"), &filesystem).result
    else {
        panic!("a denied index must not be replaced or reported as canonical damage")
    };
    assert_eq!(
        failed_location(&failure.path)
            .unwrap()
            .package_relative_path
            .as_deref(),
        Some(".spielorders/index.json")
    );
    assert_eq!(
        filesystem
            .bytes("/package/.spielorders/index.json")
            .unwrap(),
        b"private cache"
    );
}

#[test]
fn a_denied_optional_readme_stays_a_specific_nonfatal_warning() {
    let filesystem = minimal_package()
        .with_directory("/package/Orders/Order")
        .with_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        )
        .with_file("/package/Orders/Order/README.md", b"private notes")
        .denying(Operation::Read, "/package/Orders/Order/README.md");
    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("optional README denial must not damage the Package")
    };
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(package.contents.healthy_orders[0].readme_text, None);
    let diagnostic = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .find(|diagnostic| diagnostic.code == DiagnosticCode::ReadmeUnavailable)
        .unwrap();
    assert_eq!(diagnostic.severity, Severity::Warning);
    let DiagnosticEvidence::ReadmeUnavailable { reason, .. } = &diagnostic.evidence else {
        panic!("the warning carries its reason")
    };
    assert_eq!(*reason, ReadmeUnavailableReason::AccessDenied);
}

#[test]
fn denied_managed_reads_are_not_misreported_as_corrupt_metadata() {
    for denied in [false, true] {
        let config = "/package/.spielorders/config.toml";
        let filesystem = if denied {
            minimal_package().denying(Operation::Read, config)
        } else {
            minimal_package().failing(Operation::Read, config)
        };
        let load = opened(Path::new("/package"), &filesystem);
        if denied {
            let Err(Failure::PackageAccessDenied(failure)) = load.result else {
                panic!("access denial is not evidence of corrupt metadata");
            };
            assert_eq!(
                failed_path(&failure.path),
                Path::new("/package/.spielorders/config.toml")
            );
        } else {
            assert!(matches!(load.result, Err(Failure::DamagedPackage(_))));
        }
    }
}

#[test]
fn unclassifiable_filesystem_failures_return_exact_operation_evidence() {
    let cases = [
        (
            minimal_package().failing(Operation::Canonicalize, "/package"),
            "/package",
        ),
        (
            minimal_package().failing(Operation::SymlinkMetadata, "/package/.spielorders"),
            "/package/.spielorders",
        ),
        (
            minimal_package().failing(Operation::ReadDirectory, "/package/Orders"),
            "/package/Orders",
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Order")
                .with_file(
                    "/package/Orders/Order/.spielorders/order.json",
                    MINIMAL_ORDER,
                )
                .failing(
                    Operation::SymlinkMetadata,
                    "/package/Orders/Order/README.md",
                ),
            "/package/Orders/Order/README.md",
        ),
    ];

    for (filesystem, expected_path) in cases {
        let Err(Failure::PackageOperationFailed(failure)) =
            opened(Path::new("/package"), &filesystem).result
        else {
            panic!("unclassifiable filesystem failure must remain an operation result")
        };
        assert_eq!(
            failed_location(&failure.path).map(|location| location.absolute_path.as_path()),
            Some(Path::new(expected_path))
        );
        assert_eq!(
            failure
                .root
                .as_ref()
                .map(|root| root.absolute_path.as_path()),
            Some(Path::new("/package"))
        );
    }
}

#[test]
fn metadata_state_distinguishes_current_unsupported_and_not_assessed_without_fake_schemas() {
    let Ok(current) = opened(Path::new("/package"), &minimal_package()).result else {
        panic!("minimal version 1 is current")
    };
    assert_eq!(current.contents.metadata_state, MetadataState::Current);

    let future = minimal_package().with_file(
        "/package/.spielorders/version.json",
        br#"{"metadata_version":999}"#,
    );
    let Err(Failure::PackageMetadataUnsupported(future)) =
        opened(Path::new("/package"), &future).result
    else {
        panic!("recognizable future metadata is diagnostic-only")
    };
    assert_eq!(future.metadata_state, MetadataState::Unsupported);

    let future_vendors = minimal_package().with_file(
        "/package/.spielorders/vendors.json",
        br#"{"schema_version":2,"vendors":[]}"#,
    );
    let Err(Failure::PackageMetadataUnsupported(future_vendors)) =
        opened(Path::new("/package"), &future_vendors).result
    else {
        panic!("a recognizable future canonical artifact is diagnostic-only")
    };
    assert_eq!(future_vendors.metadata_state, MetadataState::Unsupported);

    let missing_version = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file("/package/.spielorders/config.toml", b"")
        .with_file("/package/.spielorders/vendors.json", VENDORS)
        .with_directory("/package/Orders");
    let Err(Failure::DamagedPackage(missing_version)) =
        opened(Path::new("/package"), &missing_version).result
    else {
        panic!("missing version evidence is diagnostic-only")
    };
    assert_eq!(missing_version.metadata_state, MetadataState::NotAssessed);
}

/// Supporting Material that cannot be observed at all leaves the load exactly as it is
/// without it, and is left as it was.
#[test]
fn loading_never_inspects_supporting_material_and_preserves_it() {
    let temporary = TemporaryTree::new("canonical-package-preservation");
    let root = temporary.path().join("package");
    let metadata = root.join(".spielorders");
    let order = root.join("Orders/Order");
    let order_metadata = order.join(".spielorders");
    let supporting_a = order.join("quote.bin");
    let supporting_b = order.join("nested/drawing.bin");
    std::fs::create_dir_all(&metadata).expect("Package metadata directory is created");
    std::fs::create_dir_all(&order_metadata).expect("Order metadata directory is created");
    std::fs::create_dir_all(supporting_b.parent().unwrap())
        .expect("nested Supporting Material directory is created");
    std::fs::write(metadata.join("config.toml"), b"").unwrap();
    std::fs::write(metadata.join("version.json"), VERSION).unwrap();
    std::fs::write(metadata.join("vendors.json"), VENDORS).unwrap();
    std::fs::write(order_metadata.join("order.json"), MINIMAL_ORDER).unwrap();
    std::fs::write(order.join("README.md"), b"exact README\n").unwrap();
    std::fs::write(&supporting_a, SUPPORTING_A).unwrap();
    std::fs::write(&supporting_b, SUPPORTING_B).unwrap();
    let filesystem = ScriptedFileSystem::new();
    // The Order Directory need not be listed to load the Order.
    filesystem
        .on(Operation::ReadDirectory)
        .matching(|path| path.ends_with("Orders/Order"))
        .fails();

    let Ok(package) = opened(&root, &filesystem).result else {
        panic!("a Package loads without its Supporting Material")
    };

    assert_eq!(
        package,
        opened(&root, &PlatformFileSystem).result.unwrap(),
        "the load matches one on the real filesystem"
    );
    assert!(package.contents.diagnostic_groups.is_empty());
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(filesystem.effects(), []);
    let inspected = filesystem.naming(is_supporting);
    assert!(
        inspected.is_empty(),
        "loading never inspects Supporting Material: {inspected:?}"
    );
    assert_eq!(std::fs::read(&supporting_a).unwrap(), SUPPORTING_A);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), SUPPORTING_B);
}

/// Whether `path` is the Supporting Material the test above places in its Order.
fn is_supporting(path: &Path) -> bool {
    let nested = Path::new("Orders/Order/nested");
    path.ends_with("Orders/Order/quote.bin")
        || path.ends_with(nested)
        || path.parent().is_some_and(|parent| parent.ends_with(nested))
}

#[test]
fn invalid_shape_diagnostics_retain_stable_known_field_paths() {
    let cases: Vec<(ScriptedFileSystem, &str)> = vec![
        (
            minimal_package().with_file(
                "/package/.spielorders/config.toml",
                b"unexpected = true\n",
            ),
            "unexpected",
        ),
        (
            minimal_package().with_file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor"}]}"#,
            ),
            "vendors[0].contacts",
        ),
        (
            minimal_package().with_file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor","contacts":[{"name":17}]}]}"#,
            ),
            "vendors[0].contacts[0].name",
        ),
        (
            minimal_package().with_file(
                "/package/.spielorders/vendors.json",
                INVALID_VENDOR_CONTACT,
            ),
            "vendors[0].contacts[0].name",
        ),
        (
            minimal_package().with_file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor","contacts":[],"hidden":"true"}]}"#,
            ),
            "vendors[0].hidden",
        ),
        (
            minimal_package().with_file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":17,"contacts":[],"hidden":true}]}"#,
            ),
            "vendors[0].name",
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    br#"{"schema_version":1,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Order","date_ordered":"2026-07-19","status":"ordered"}"#,
            ),
            "items",
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    br#"{"schema_version":1,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Order","date_ordered":"2026-07-19","status":"ordered","items":[{"description":"Part","quantity":"one","unit_price":1.0}]}"#,
            ),
            "items[0].quantity",
        ),
        (
            minimal_package()
                .with_directory("/package/Orders/Bad")
                .with_file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    INVALID_ORDER_ITEM,
                ),
            "items[0].unit_price",
        ),
    ];

    for (filesystem, expected_field) in cases {
        let result = opened(Path::new("/package"), &filesystem).result;
        let diagnostics = match &result {
            Err(Failure::DamagedPackage(package)) => package
                .diagnostic_groups
                .iter()
                .flat_map(|group| &group.diagnostics)
                .collect::<Vec<_>>(),
            Ok(package) => package
                .contents
                .diagnostic_groups
                .iter()
                .flat_map(|group| &group.diagnostics)
                .collect::<Vec<_>>(),
            _ => panic!("shape failure is a canonical diagnostic"),
        };
        let diagnostic = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::ManagedArtifactInvalidShape)
            .expect("shape diagnostic exists");
        let DiagnosticEvidence::ManagedArtifact { field, .. } = &diagnostic.evidence else {
            panic!("shape diagnostic uses managed-artifact evidence")
        };
        assert_eq!(field.as_deref(), Some(expected_field));
    }
}

#[test]
fn nested_configuration_projects_every_exact_managed_and_readme_location() {
    let order = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("../..", "../../..", 1);
    let filesystem = ScriptedFileSystem::private("canonical-load")
        .with_directory("/package")
        .with_directory("/package/.spielorders")
        .with_file(
            "/package/.spielorders/config.toml",
            b"orders_path = \"Purchases/Active\"\nremoved_path = \"Retired\"\n",
        )
        .with_file("/package/.spielorders/version.json", VERSION)
        .with_file("/package/.spielorders/vendors.json", VENDORS)
        .with_directory("/package/Purchases")
        .with_directory("/package/Purchases/Active")
        .with_directory("/package/Purchases/Active/Order")
        .with_file(
            "/package/Purchases/Active/Order/.spielorders/order.json",
            order.as_bytes(),
        );

    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("safe nested configuration is current")
    };
    let context = package.contents.context;
    assert_eq!(
        context.metadata_directory.package_relative_path.as_deref(),
        Some(".spielorders")
    );
    assert_eq!(
        context.config.package_relative_path.as_deref(),
        Some(".spielorders/config.toml")
    );
    assert_eq!(
        context.version.package_relative_path.as_deref(),
        Some(".spielorders/version.json")
    );
    assert_eq!(
        context.vendors.package_relative_path.as_deref(),
        Some(".spielorders/vendors.json")
    );
    assert_eq!(
        context.index.package_relative_path.as_deref(),
        Some(".spielorders/index.json")
    );
    assert_eq!(
        context
            .orders_directory
            .as_ref()
            .unwrap()
            .package_relative_path
            .as_deref(),
        Some("Purchases/Active")
    );
    assert_eq!(
        context
            .removed_orders_directory
            .as_ref()
            .unwrap()
            .package_relative_path
            .as_deref(),
        Some("Retired")
    );
    assert_eq!(
        context
            .package_readme
            .location
            .package_relative_path
            .as_deref(),
        Some("README.md")
    );
    let paths = &package.contents.healthy_orders[0].paths;
    assert_eq!(
        paths.readme.location.package_relative_path.as_deref(),
        Some("Purchases/Active/Order/README.md")
    );
}

#[test]
fn resolved_orders_directory_failures_use_only_outside_or_reserved_reasons() {
    use crate::readme::ConfiguredOrdersDirectoryReason;

    let outside = minimal_package()
        .with_file(
            "/package/.spielorders/config.toml",
            b"orders_path = \"Mount/Orders\"\n",
        )
        .with_directory("/outside/Orders")
        .with_symlink("/package/Mount", "/outside");
    let reserved = minimal_package().with_symlink("/package/Orders", "/package/.spielorders");
    let reserved_similar_prefix = minimal_package()
        .with_directory("/package/.spielorders/Orders")
        .with_symlink("/package/.spielorders-link", "/package/.spielorders")
        .with_file(
            "/package/.spielorders/config.toml",
            b"orders_path = \".spielorders-link/Orders\"\n",
        );

    for (filesystem, expected_reason) in [
        (outside, ConfiguredOrdersDirectoryReason::OutsidePackage),
        (reserved, ConfiguredOrdersDirectoryReason::ReservedMetadata),
        (
            reserved_similar_prefix,
            ConfiguredOrdersDirectoryReason::ReservedMetadata,
        ),
    ] {
        let Err(Failure::DamagedPackage(package)) =
            opened(Path::new("/package"), &filesystem).result
        else {
            panic!("unsafe resolved Orders location damages the Package")
        };
        let diagnostic = package
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .find(|diagnostic| {
                diagnostic.code == DiagnosticCode::ConfiguredOrdersDirectoryUnavailable
            })
            .expect("configured-directory diagnostic exists");
        let DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable { reason, .. } =
            &diagnostic.evidence
        else {
            panic!("configured-directory evidence is typed")
        };
        assert_eq!(*reason, expected_reason);
    }
}

/// A deliberate interface lock: a code's spelling is a compatibility promise to every
/// adapter that reads one.
#[test]
fn public_load_vocabulary_uses_every_closed_stable_spelling() {
    let codes = [
        (
            DiagnosticCode::ManagedArtifactMissing,
            "managed_artifact_missing",
        ),
        (
            DiagnosticCode::ManagedArtifactWrongKind,
            "managed_artifact_wrong_kind",
        ),
        (
            DiagnosticCode::ManagedArtifactUnreadable,
            "managed_artifact_unreadable",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidEncoding,
            "managed_artifact_invalid_encoding",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidSyntax,
            "managed_artifact_invalid_syntax",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidShape,
            "managed_artifact_invalid_shape",
        ),
        (
            DiagnosticCode::ManagedArtifactWrongVersion,
            "managed_artifact_wrong_version",
        ),
        (
            DiagnosticCode::ConfigurationPathInvalid,
            "configuration_path_invalid",
        ),
        (
            DiagnosticCode::ConfigurationPathsOverlap,
            "configuration_paths_overlap",
        ),
        (
            DiagnosticCode::ConfiguredOrdersDirectoryUnavailable,
            "configured_orders_directory_unavailable",
        ),
        (
            DiagnosticCode::OrderSemanticInvalid,
            "order_semantic_invalid",
        ),
        (
            DiagnosticCode::OrderPackageLocatorMismatch,
            "order_package_locator_mismatch",
        ),
        (DiagnosticCode::OrderIdDuplicate, "order_id_duplicate"),
        (DiagnosticCode::VendorIdDuplicate, "vendor_id_duplicate"),
        (DiagnosticCode::VendorNameDuplicate, "vendor_name_duplicate"),
        (
            DiagnosticCode::VendorReferenceUnresolved,
            "vendor_reference_unresolved",
        ),
        (
            DiagnosticCode::OrdersDirectoryEntryDisallowed,
            "orders_directory_entry_disallowed",
        ),
        (DiagnosticCode::ReadmeUnavailable, "readme_unavailable"),
        (
            DiagnosticCode::ConfiguredOrdersDirectoryNameUnusable,
            "configured_orders_directory_name_unusable",
        ),
        (
            DiagnosticCode::OrdersDirectoryEntryNameUnusable,
            "orders_directory_entry_name_unusable",
        ),
    ];
    for (code, spelling) in codes {
        assert_eq!(code.code(), spelling);
    }
}

#[test]
fn diagnostic_groups_and_composed_order_diagnostics_follow_stable_scope_path_and_code_order() {
    let invalid_date = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let filesystem = minimal_package()
        .with_file("/package/Orders/loose.txt", b"warning")
        .with_directory("/package/Orders/B")
        .with_file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER)
        .with_directory("/package/Orders/A")
        .with_file(
            "/package/Orders/A/.spielorders/order.json",
            invalid_date.as_bytes(),
        );

    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("warnings and Order damage retain a candidate")
    };
    let groups = package.contents.diagnostic_groups;
    assert_eq!(groups[0].scope, EvidenceScope::Package);
    assert_eq!(
        groups[1].location.package_relative_path.as_deref(),
        Some("Orders/A")
    );
    assert_eq!(
        groups[2].location.package_relative_path.as_deref(),
        Some("Orders/B")
    );
    assert_eq!(
        groups[1]
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![
            DiagnosticCode::OrderIdDuplicate,
            DiagnosticCode::OrderSemanticInvalid,
        ]
    );
    assert_eq!(
        groups[2].diagnostics[0].code,
        DiagnosticCode::OrderIdDuplicate
    );
}

/// A caller-supplied root the filesystem cannot canonicalize is a typed load failure
/// that names the path the caller supplied.
///
/// Naming the path back is the substance rather than a nicety: a caller has to be able
/// to say which root it asked about, and it cannot resolve the path itself to find out —
/// that is the operation that just failed. A relative root has no absolute spelling, so
/// none is invented.
#[test]
fn a_relative_root_that_cannot_be_canonicalized_fails_naming_the_path_supplied() {
    let selected_root = Path::new("./no-such-package-root");

    let Err(Failure::PackageOperationFailed(failure)) =
        opened(selected_root, &PlatformFileSystem).result
    else {
        panic!("a root that cannot be canonicalized is an operation failure")
    };
    assert_eq!(
        failure.path,
        FailedPath::Unlocated(selected_root.to_path_buf())
    );
    assert_eq!(failed_path(&failure.path), selected_root);
    // There is no absolute spelling to report and none is invented, at either level of
    // the answer.
    assert_eq!(failed_location(&failure.path), None);
    assert_eq!(failure.root, None);
}

/// Every public core entry point that takes a caller-supplied Package Root answers a
/// root it cannot resolve with its typed failure, rather than aborting the process.
///
/// Order creation, Order adoption, Package README writes, and the Vendor family reach
/// the root through their own resolution rather than through the load, so each is
/// named.
#[test]
fn no_public_entry_point_aborts_on_a_root_it_cannot_resolve() {
    let root = Path::new("./no-such-package-root");

    assert!(matches!(
        opened(root, &PlatformFileSystem).result,
        Err(Failure::PackageOperationFailed(_))
    ));
    assert!(matches!(
        validated_package(root, &PlatformFileSystem).result,
        Err(Failure::PackageOperationFailed(_))
    ));

    assert!(matches!(
        crate::plan(
            &create_order(
                PathBuf::from(root),
                Assignments::new().assign("title", "Anything")
            ),
            &effects(&PlatformFileSystem).on(2026, 8, 22)
        )
        .result,
        Err(Failure::PackageOperationFailed(_))
    ));
    assert!(matches!(
        crate::plan(
            &adoption(
                PathBuf::from(root),
                PathBuf::from(&root.join("candidate")),
                &(
                    Assignments::new()
                        .assign("title", "Anything")
                        .assign("date_ordered", "2026-08-22")
                        .assign("status", "ordered"),
                    Vec::new()
                )
            ),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::PackageOperationFailed(_))
    ));
    assert!(matches!(
        crate::plan(
            &write_readme(
                package_scope(PathBuf::from(root)),
                current_readme(root),
                "text"
            ),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::PackageOperationFailed(_))
    ));

    let contacts = Assignments::new().assign("name", "Anyone");
    let vendor_failures: [Result<_, Failure>; 5] = [
        crate::plan(
            &create_vendor(
                PathBuf::from(root),
                Assignments::new().assign("name", "Anyone"),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .map(|_| ()),
        crate::plan(
            &edit_vendor(
                PathBuf::from(root),
                VendorRef::Id(String::from("vendor")),
                Assignments::new().assign("name", "Anyone"),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .map(|_| ()),
        crate::plan(
            &set_vendor_hidden(
                PathBuf::from(root),
                VendorRef::Id(String::from("vendor")),
                true,
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .map(|_| ()),
        crate::plan(
            &set_vendor_hidden(
                PathBuf::from(root),
                VendorRef::Id(String::from("vendor")),
                false,
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .map(|_| ()),
        crate::plan(
            &add_contact(
                PathBuf::from(root),
                VendorRef::Id(String::from("vendor")),
                contacts.clone(),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .map(|_| ()),
    ];
    for failure in vendor_failures {
        assert!(matches!(failure, Err(Failure::PackageOperationFailed(_))));
    }
}

/// A relative root that resolves is unaffected, and this proves it rather than assuming
/// it.
///
/// Canonicalization normalizes a resolvable relative path before anything downstream can
/// tell how it was spelled, so the load through the relative spelling and the load
/// through the resolved absolute one are the same answer.
#[test]
fn a_relative_root_that_resolves_loads_exactly_as_its_resolved_root_does() {
    let tree = TemporaryTree::new("relative-root-that-resolves");
    let root = tree.path().join("package");
    std::fs::create_dir_all(root.join(".spielorders")).expect("the Package metadata directory");
    std::fs::write(root.join(".spielorders/config.toml"), b"").expect("the configuration");
    std::fs::write(root.join(".spielorders/version.json"), VERSION).expect("the version marker");
    std::fs::write(root.join(".spielorders/vendors.json"), VENDORS).expect("the Vendor record");
    std::fs::create_dir(root.join("Orders")).expect("the Orders Directory");

    let resolved = std::fs::canonicalize(&root).expect("the Root resolves");

    // Two roots that resolve to the same directory and are spelled differently from it:
    // a genuinely relative one, and an absolute one carrying a component the filesystem
    // has to normalize away. Both must load what the resolved spelling loads.
    for selected in [
        relative_to_working_directory(&resolved),
        root.join("Orders").join(".."),
    ] {
        let Ok(through_selected) = opened(&selected, &PlatformFileSystem).result else {
            panic!("a root that resolves loads its Package, however it was spelled")
        };
        let Ok(through_resolved) = opened(&resolved, &PlatformFileSystem).result else {
            panic!("the resolved root loads the same Package")
        };
        assert_eq!(through_selected, through_resolved);
    }
}

/// A relative spelling of `resolved`, built by ascending from the process working
/// directory to the filesystem root and descending again.
///
/// A relative path only means anything against a working directory, and this suite
/// cannot choose one: `set_current_dir` is process-global and these tests share a
/// process. Spelling the ascent out is what is left, and it makes the test's own premise
/// checkable — the assertion below is that the path really is relative, not that it looks
/// it.
fn relative_to_working_directory(resolved: &Path) -> PathBuf {
    let working_directory = std::fs::canonicalize(
        std::env::current_dir().expect("the process has a working directory"),
    )
    .expect("the process working directory resolves");
    let mut relative = PathBuf::new();
    for _ in working_directory
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
    {
        relative.push("..");
    }
    relative.push(
        resolved
            .strip_prefix(Component::RootDir.as_os_str())
            .expect("a resolved path is absolute"),
    );
    assert!(
        relative.is_relative(),
        "the relative spelling must actually be relative"
    );
    relative
}
