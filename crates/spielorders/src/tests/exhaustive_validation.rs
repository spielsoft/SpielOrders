//! Decision sources: Define the stable diagnostic vocabulary and evidence contract;
//! Define the complete Package read projection and load-state contract; Decide how Package
//! Index freshness is proven without canonical rereads.

use crate::Coded;
use crate::Failure;
use crate::readme::OrderSemanticReason;
use crate::tests::support::{
    Operation, ScriptedFileSystem, enumerate_orders_as, failed_location, findings, minimal_package,
    opened, proven_current_minimal_package, sha256, staging_evidence, validated_package,
};
use crate::tests::support::{path_set, touched_by};
use crate::{
    ports::{DirectoryEntry, FileSystemEntryKind},
    readme::DiagnosticCode,
    readme::DiagnosticEvidence,
    readme::EvidenceScope,
    readme::Severity,
};
use std::path::Path;

const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const STAGED_ORDER: &[u8] = b"{\"schema_version\": 1, \"abandoned\": \"order candidate\"}\n";
const STAGED_VENDORS: &[u8] = b"{\"schema_version\": 1, \"abandoned\": \"vendors candidate\"}\n";
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

#[test]
fn exhaustive_validation_finds_reserved_content_that_fast_loading_deliberately_does_not_inspect() {
    let filesystem = proven_current_minimal_package();
    filesystem.put_entry(
        "/package/.spielorders/unexpected.txt",
        FileSystemEntryKind::File,
    );
    let supporting_a = "/package/Orders/.opaque/supporting-a.bin";
    let supporting_b = "/package/Orders/.opaque/nested/supporting-b.bin";
    filesystem.put_file(supporting_a, SUPPORTING_A);
    filesystem.put_file(supporting_b, SUPPORTING_B);

    let Ok(before) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the deterministic index is proven current")
    };
    assert!(before.contents.diagnostic_groups.is_empty());
    let fast_listings = filesystem.paths(Operation::ReadDirectory);

    let Ok(report) = validated_package(Path::new("/package"), &filesystem).result else {
        panic!("a reserved warning does not make the Package invalid")
    };

    let warning = report
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .find(|diagnostic| diagnostic.code == DiagnosticCode::ReservedContentUnexpected)
        .expect("exhaustive validation reports unexpected reserved content");
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.scope, EvidenceScope::Package);
    assert_eq!(
        warning.primary_location.package_relative_path.as_deref(),
        Some(".spielorders/unexpected.txt")
    );
    assert_eq!(
        warning.evidence,
        DiagnosticEvidence::ReservedContentUnexpected {
            observed_kind: FileSystemEntryKind::File,
        }
    );
    assert_eq!(before.contents.diagnostic_groups, &[]);
    assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
    assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
    assert!(filesystem.calls().iter().all(|call| {
        call.paths()
            .into_iter()
            .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
    }));
    let metadata = Path::new("/package/.spielorders");
    assert!(!fast_listings.iter().any(|path| path == metadata));
    assert!(
        filesystem
            .paths(Operation::ReadDirectory)
            .iter()
            .any(|path| path == metadata)
    );
}

#[test]
fn exhaustive_validation_reads_exact_documents_and_reports_staging_with_known_destination_validity()
{
    let filesystem = minimal_package();
    filesystem.put_file("/package/README.md", b"# Package notes\n");
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/Orders/Order/README.md", b"# Order notes\n");
    filesystem.put_file(
        "/package/.spielorders/.vendors.json.spielorders-stage",
        STAGED_VENDORS,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/.order.json.spielorders-stage",
        STAGED_ORDER,
    );
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders/unexpected.txt",
        FileSystemEntryKind::SymbolicLink,
    );

    let Ok(report) = validated_package(Path::new("/package"), &filesystem).result else {
        panic!("healthy managed evidence is completely validated and valid")
    };

    let [order] = report.examined_orders.as_slice() else {
        panic!("every discovered Order Directory is examined")
    };
    assert_eq!(
        order.directory.package_relative_path.as_deref(),
        Some("Orders/Order")
    );
    let reads = filesystem.paths(Operation::Read);
    for readme in ["/package/README.md", "/package/Orders/Order/README.md"] {
        assert!(
            reads.iter().any(|path| path == Path::new(readme)),
            "every README is read exactly: {readme}"
        );
    }
    let diagnostics = report
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    let staging = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::StagingArtifactPresent)
        .collect::<Vec<_>>();
    assert_eq!(staging.len(), 2);
    assert_eq!(
        DiagnosticCode::ReservedContentUnexpected.code(),
        "reserved_content_unexpected"
    );
    assert_eq!(
        DiagnosticCode::StagingArtifactPresent.code(),
        "staging_artifact_present"
    );
    assert!(staging.iter().all(|diagnostic| {
        diagnostic.severity == Severity::Warning && diagnostic.related_locations.len() == 1
    }));
    // Each staging diagnostic names the candidate actually sitting at that name, not
    // merely that some candidate is there: promotion writes those exact bytes.
    let staged_evidence = |name: &str| {
        staging
            .iter()
            .find(|diagnostic| diagnostic.primary_location.absolute_path.ends_with(name))
            .unwrap_or_else(|| panic!("a staging diagnostic at {name}"))
            .evidence
            .clone()
    };
    assert_eq!(
        staging_evidence(&staged_evidence(".vendors.json.spielorders-stage")),
        (Some(true), Some(sha256(STAGED_VENDORS)))
    );
    assert_eq!(
        staging_evidence(&staged_evidence(".order.json.spielorders-stage")),
        (Some(true), Some(sha256(STAGED_ORDER)))
    );
    let unexpected = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::ReservedContentUnexpected)
        .expect("unexpected Order-reserved content is reported");
    assert_eq!(unexpected.scope, EvidenceScope::Order);
    assert_eq!(
        unexpected.evidence,
        DiagnosticEvidence::ReservedContentUnexpected {
            observed_kind: FileSystemEntryKind::SymbolicLink,
        }
    );
}

#[test]
fn missing_and_unavailable_readmes_keep_distinct_empty_evidence_without_duplicate_warnings() {
    let filesystem = minimal_package();
    for name in ["A", "B"] {
        filesystem.put_entry(
            format!("/package/Orders/{name}"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_entry(
            format!("/package/Orders/{name}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
    }
    filesystem.put_file("/package/Orders/A/.spielorders/order.json", MINIMAL_ORDER);
    let second_order = String::from_utf8(MINIMAL_ORDER.to_vec()).unwrap().replacen(
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "11111111-1111-4111-8111-111111111111",
        1,
    );
    filesystem.put_file(
        "/package/Orders/B/.spielorders/order.json",
        second_order.as_bytes(),
    );
    filesystem.put_file("/package/Orders/B/README.md", &[0xff, 0xfe]);

    let Ok(report) = validated_package(Path::new("/package"), &filesystem).result else {
        panic!("README warnings do not make the Package invalid")
    };

    assert_eq!(report.examined_orders.len(), 2);
    let readme_groups = report
        .diagnostic_groups
        .iter()
        .filter(|group| group.scope == EvidenceScope::Document)
        .collect::<Vec<_>>();
    assert_eq!(readme_groups.len(), 1, "exact duplicate warnings collapse");
    assert_eq!(
        readme_groups[0].location.package_relative_path.as_deref(),
        Some("Orders/B/README.md"),
        "a missing README is ordinary and reports nothing"
    );
    assert_eq!(readme_groups[0].diagnostics.len(), 1);
    assert_eq!(
        readme_groups[0].diagnostics[0].code,
        DiagnosticCode::ReadmeUnavailable
    );
}

#[test]
fn exhaustive_validation_uses_exact_authorized_effect_budgets_and_never_mutates_or_observes_supporting_material()
 {
    let filesystem = minimal_package();
    filesystem.put_file("/package/README.md", b"Package\n");
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/Orders/Order/README.md", b"Order\n");
    let supporting_a = "/package/Orders/Order/supporting-a.bin";
    let supporting_b = "/package/Orders/Order/nested/supporting-b.bin";
    filesystem.put_file(supporting_a, SUPPORTING_A);
    filesystem.put_file(supporting_b, SUPPORTING_B);
    let before_a = filesystem.bytes(supporting_a);
    let before_b = filesystem.bytes(supporting_b);

    let Ok(_) = validated_package(Path::new("/package"), &filesystem).result else {
        panic!("the complete authorized evidence produces a validation result")
    };

    let reads = filesystem.paths(Operation::Read);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        "/package/Orders/Order/.spielorders/order.json",
        "/package/README.md",
        "/package/Orders/Order/README.md",
    ] {
        assert_eq!(
            reads
                .iter()
                .filter(|read| **read == Path::new(path))
                .count(),
            1,
            "{path} is read exactly once"
        );
    }
    assert_eq!(
        touched_by(&filesystem, Operation::ReadDirectory),
        path_set(&[
            "/package/Orders",
            "/package/.spielorders",
            "/package/Orders/Order/.spielorders",
        ])
    );
    assert_eq!(filesystem.effects(), []);
    assert!(filesystem.calls().iter().all(|call| {
        call.paths()
            .into_iter()
            .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
    }));
    assert_eq!(filesystem.bytes(supporting_a), before_a);
    assert_eq!(filesystem.bytes(supporting_b), before_b);
}

#[test]
fn combined_damage_documents_reserved_content_and_staging_have_complete_deterministic_groups() {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/README.md", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/.spielorders/unexpected-root",
        FileSystemEntryKind::Other,
    );
    for name in ["A", "B"] {
        filesystem.put_entry(
            format!("/package/Orders/{name}"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_entry(
            format!("/package/Orders/{name}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
    }
    let invalid_date =
        String::from_utf8(MINIMAL_ORDER.to_vec())
            .unwrap()
            .replacen("2026-07-19", "2026-02-31", 1);
    filesystem.put_file(
        "/package/Orders/A/.spielorders/order.json",
        invalid_date.as_bytes(),
    );
    filesystem.put_file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER);
    filesystem.put_file(
        "/package/Orders/A/.spielorders/.order.json.spielorders-stage",
        STAGED_ORDER,
    );
    filesystem.put_entry(
        "/package/Orders/A/.spielorders/unexpected",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file("/package/Orders/B/README.md", &[0xff]);

    let validation = validated_package(Path::new("/package"), &filesystem);
    let Err(Failure::PackageInvalid(diagnosis)) = validation.result else {
        panic!("every Damaged Order invalidates exhaustive Package validation")
    };
    assert_eq!(
        validation.diagnostics,
        [],
        "the verdict carries its own findings"
    );
    // Every finding is grouped at its narrowest scope, in the scopes' own order: the
    // Package, its README, each Order, and the README of the one Order whose README is
    // unreadable.
    let groups = diagnosis
        .diagnostic_groups
        .iter()
        .map(|group| {
            (
                group.scope,
                group.location.package_relative_path.as_deref().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        groups,
        [
            (EvidenceScope::Package, "."),
            (EvidenceScope::Document, "README.md"),
            (EvidenceScope::Order, "Orders/A"),
            (EvidenceScope::Order, "Orders/B"),
            (EvidenceScope::Document, "Orders/B/README.md"),
        ]
    );
    for group in &diagnosis.diagnostic_groups {
        assert!(group.diagnostics.iter().all(|diagnostic| {
            diagnostic.scope == group.scope
                && diagnostic
                    .primary_location
                    .absolute_path
                    .starts_with(&group.location.absolute_path)
        }));
    }
    let findings = findings(&diagnosis).collect::<Vec<_>>();
    let in_order = |order: &str| {
        findings
            .iter()
            .copied()
            .filter(|diagnostic| {
                diagnostic.scope == EvidenceScope::Order
                    && diagnostic
                        .primary_location
                        .package_relative_path
                        .as_deref()
                        .is_some_and(|path| path.starts_with(order))
            })
            .collect::<Vec<_>>()
    };
    for order in ["Orders/A/", "Orders/B/"] {
        let duplicate = in_order(order)
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::OrderIdDuplicate)
            .expect("every duplicate-ID participant explains itself");
        let DiagnosticEvidence::OrderIdDuplicate {
            conflict_locations, ..
        } = &duplicate.evidence
        else {
            panic!("duplicate evidence is typed")
        };
        assert_eq!(
            conflict_locations
                .iter()
                .map(|location| location.package_relative_path.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["Orders/A", "Orders/B"]
        );
    }
    let a_diagnostics = in_order("Orders/A/");
    let first_warning = a_diagnostics
        .iter()
        .position(|diagnostic| diagnostic.severity == Severity::Warning)
        .unwrap();
    assert!(
        a_diagnostics[..first_warning]
            .iter()
            .all(|diagnostic| diagnostic.severity == Severity::Error)
    );
    let staging = a_diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::StagingArtifactPresent)
        .unwrap();
    assert_eq!(
        staging_evidence(&staging.evidence),
        (Some(false), Some(sha256(STAGED_ORDER)))
    );
    assert_eq!(
        findings
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::ReadmeUnavailable)
            .count(),
        2,
        "the Package README and Order B's README are each reported once"
    );
    assert_eq!(
        filesystem
            .paths(Operation::Read)
            .iter()
            .filter(|path| *path == Path::new("/package/Orders/B/README.md"))
            .count(),
        1,
        "a healthy duplicate-ID participant's exact README is read once"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_reserved_content_conservatively_prevents_an_incomplete_validation_report() {
    use std::os::unix::ffi::OsStringExt;

    let filesystem = one_order_filesystem();
    // Not every volume can hold this name, so the listing reports it.
    filesystem
        .on(Operation::ReadDirectory)
        .at("/package/.spielorders")
        .lists_also(vec![DirectoryEntry {
            name: std::ffi::OsString::from_vec(b"unexpected-\xff".to_vec()),
            kind: FileSystemEntryKind::File,
        }]);

    let Err(Failure::PackageOperationFailed(failed)) =
        validated_package(Path::new("/package"), &filesystem).result
    else {
        panic!("unrepresentable reserved evidence cannot produce an incomplete report")
    };

    assert_eq!(
        failed_location(&failed.path).map(|location| location.absolute_path.as_path()),
        Some(Path::new("/package/.spielorders"))
    );
    assert_supporting_material_preserved(&filesystem);
}

#[test]
fn every_authorized_canonicalize_kind_and_list_failure_returns_exact_operation_evidence() {
    enum Fault {
        Canonicalize(&'static str),
        Observe(&'static str),
        List(&'static str),
    }
    let cases = [
        Fault::Canonicalize("/package"),
        Fault::Canonicalize("/package/Orders"),
        Fault::Canonicalize("/package/Orders/Order"),
        Fault::Observe("/package/.spielorders"),
        Fault::Observe("/package/.spielorders/config.toml"),
        Fault::Observe("/package/.spielorders/version.json"),
        Fault::Observe("/package/.spielorders/vendors.json"),
        Fault::Observe("/package"),
        Fault::Observe("/package/Removed"),
        Fault::Observe("/package/Orders"),
        Fault::Observe("/package/Orders/Order/.spielorders/order.json"),
        Fault::Observe("/package/README.md"),
        Fault::Observe("/package/Orders/Order/README.md"),
        Fault::Observe("/package/Orders/Order/.spielorders"),
        Fault::List("/package/Orders"),
        Fault::List("/package/.spielorders"),
        Fault::List("/package/Orders/Order/.spielorders"),
    ];

    for failure in cases {
        let filesystem = one_order_filesystem();
        let expected_path = match failure {
            Fault::Canonicalize(path) => {
                filesystem.on(Operation::Canonicalize).at(path).fails();
                path
            }
            Fault::Observe(path) => {
                filesystem.on(Operation::SymlinkMetadata).at(path).fails();
                path
            }
            Fault::List(path) => {
                filesystem.on(Operation::ReadDirectory).at(path).fails();
                path
            }
        };

        let Err(Failure::PackageOperationFailed(failed)) =
            validated_package(Path::new("/package"), &filesystem).result
        else {
            panic!("a failure at {expected_path} cannot establish validation")
        };
        assert_eq!(
            failed_location(&failed.path).map(|location| location.absolute_path.as_path()),
            Some(Path::new(expected_path))
        );
        assert_supporting_material_preserved(&filesystem);
    }
}

#[test]
fn persistent_required_artifact_and_document_read_failures_keep_their_distinct_contracts() {
    for (path, expected_scope) in [
        ("/package/.spielorders/config.toml", EvidenceScope::Package),
        ("/package/.spielorders/version.json", EvidenceScope::Package),
        ("/package/.spielorders/vendors.json", EvidenceScope::Package),
        (
            "/package/Orders/Order/.spielorders/order.json",
            EvidenceScope::Order,
        ),
    ] {
        let filesystem = one_order_filesystem();
        filesystem.on(Operation::Read).at(path).fails();
        let validation = validated_package(Path::new("/package"), &filesystem);
        let Err(Failure::PackageInvalid(diagnosis)) = validation.result else {
            panic!("a persistent required-artifact read failure is canonical evidence")
        };
        let matches = findings(&diagnosis)
            .filter(|diagnostic| {
                diagnostic.code == DiagnosticCode::ManagedArtifactUnreadable
                    && diagnostic.scope == expected_scope
                    && diagnostic.primary_location.absolute_path.as_path() == Path::new(path)
            })
            .count();
        assert_eq!(matches, 1, "{path} remains one canonical diagnostic");
        assert_eq!(
            filesystem
                .paths(Operation::Read)
                .iter()
                .filter(|observed| *observed == Path::new(path))
                .count(),
            1,
            "{path} is attempted exactly once"
        );
        assert_supporting_material_preserved(&filesystem);
    }

    for path in ["/package/README.md", "/package/Orders/Order/README.md"] {
        let filesystem = one_order_filesystem();
        filesystem.put_file(path, b"readable before injection");
        filesystem.on(Operation::Read).at(path).fails();
        let Ok(report) = validated_package(Path::new("/package"), &filesystem).result else {
            panic!("README warnings never make the Package invalid")
        };
        let matches = report
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .filter(|diagnostic| {
                diagnostic.code == DiagnosticCode::ReadmeUnavailable
                    && diagnostic.primary_location.absolute_path == Path::new(path)
            })
            .count();
        assert_eq!(matches, 1, "{path} warning is deduplicated");
        assert_eq!(
            filesystem
                .paths(Operation::Read)
                .iter()
                .filter(|observed| *observed == Path::new(path))
                .count(),
            1,
            "{path} is attempted exactly once"
        );
        assert_supporting_material_preserved(&filesystem);
    }
}

#[test]
fn exhaustive_validation_never_retrofits_or_changes_proven_current_fast_load_evidence() {
    let filesystem = proven_current_minimal_package();
    filesystem.put_entry(
        "/package/.spielorders/unexpected.txt",
        FileSystemEntryKind::File,
    );

    let Ok(before) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the index starts proven current")
    };
    let Ok(validation) = validated_package(Path::new("/package"), &filesystem).result else {
        panic!("the separate exhaustive call completes")
    };
    let Ok(after) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the index remains proven current")
    };

    assert_eq!(before, after);
    assert!(before.contents.diagnostic_groups.is_empty());
    assert!(after.contents.diagnostic_groups.is_empty());
    assert!(validation.diagnostic_groups.iter().any(|group| {
        group
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ReservedContentUnexpected)
    }));

    let Ok(canonical) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the same canonical evidence remains current")
    };
    assert_eq!(
        before.contents.healthy_orders, canonical.contents.healthy_orders,
        "ordinary public Order equality is independent of the load route"
    );
}

#[test]
fn ordinary_public_order_equality_ignores_unexposed_readme_observation_state() {
    let filesystem = one_order_filesystem();
    filesystem.remove("/package/Orders/Order/README.md");
    let Ok(absent) = opened(Path::new("/package"), &filesystem).result else {
        panic!("an absent README does not damage the Order")
    };

    filesystem.put_file("/package/Orders/Order/README.md", b"");
    let Ok(empty) = opened(Path::new("/package"), &filesystem).result else {
        panic!("an empty README does not damage the Order")
    };

    assert_eq!(
        absent.contents.healthy_orders, empty.contents.healthy_orders,
        "unexposed validation evidence cannot alter ordinary public equality"
    );
}

#[test]
fn exhaustive_reports_are_independent_of_directory_enumeration_and_opaque_source_errors() {
    let forward = two_order_filesystem(&["A", "B"]);
    let reverse = two_order_filesystem(&["B", "A"]);

    let forward_report = validated_package(Path::new("/package"), &forward);
    let reverse_report = validated_package(Path::new("/package"), &reverse);

    assert_eq!(forward_report, reverse_report);

    let forward_failure = two_order_filesystem(&["A", "B"]);
    let reverse_failure = two_order_filesystem(&["B", "A"]);
    forward_failure
        .on(Operation::ReadDirectory)
        .at("/package/.spielorders")
        .fails();
    reverse_failure
        .on(Operation::ReadDirectory)
        .at("/package/.spielorders")
        .fails();
    assert_eq!(
        validated_package(Path::new("/package"), &forward_failure),
        validated_package(Path::new("/package"), &reverse_failure),
    );
}

#[test]
fn package_damage_does_not_hide_independently_observable_order_document_or_reserved_evidence() {
    let filesystem = one_order_filesystem();
    filesystem.put_file(
        "/package/.spielorders/vendors.json",
        br#"{
  "schema_version": 1,
  "vendors": [
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"First","contacts":[]},
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Second","contacts":[]}
  ]
}
"#,
    );
    let invalid_date =
        String::from_utf8(MINIMAL_ORDER.to_vec())
            .unwrap()
            .replacen("2026-07-19", "2026-02-31", 1);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        invalid_date.as_bytes(),
    );
    filesystem.put_file("/package/Orders/Order/README.md", &[0xff]);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders/unexpected",
        FileSystemEntryKind::File,
    );

    let validation = validated_package(Path::new("/package"), &filesystem);
    let Err(Failure::PackageInvalid(diagnosis)) = validation.result else {
        panic!("exhaustive validation retains all independently knowable evidence")
    };
    let codes = findings(&diagnosis)
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    for code in [
        DiagnosticCode::VendorIdDuplicate,
        DiagnosticCode::OrderSemanticInvalid,
        DiagnosticCode::ReadmeUnavailable,
        DiagnosticCode::ReservedContentUnexpected,
    ] {
        assert!(codes.contains(&code), "{code:?} remains visible");
    }
}

/// A calculated total that is not finite makes its Order semantically invalid, and the
/// evidence names the total rather than a stored value.
///
/// **Overflow is the only route to one.** `FiniteNumber`'s deserializer refuses a
/// non-finite stored Quantity or Unit Price, so a record that loads at all carries finite
/// numbers and there is nothing stored to blame — which is why the reason is its own and
/// the `field` is the calculated total. An Item Total and an Order Total are told apart
/// there: a product that overflows and a sum of finite products that overflows are
/// different arithmetic, and a caller correcting one is not looking at the other.
#[test]
fn a_calculated_total_that_is_not_finite_invalidates_its_order_naming_the_total() {
    let filesystem = minimal_package();
    for (directory, order_id, items) in [
        (
            "Item",
            "c1111111-1111-4111-8111-111111111111",
            r#"{"description":"Mirror mount","quantity":1e308,"unit_price":10.0}"#,
        ),
        (
            "Order",
            "c2222222-2222-4222-8222-222222222222",
            r#"{"description":"First","quantity":1e308,"unit_price":1.0},
      {"description":"Second","quantity":1e308,"unit_price":1.0}"#,
        ),
    ] {
        let path = format!("/package/Orders/{directory}");
        filesystem.put_entry(&path, FileSystemEntryKind::Directory);
        filesystem.put_entry(
            format!("{path}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_file(
            format!("{path}/.spielorders/order.json"),
            format!(
                r#"{{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "{order_id}",
  "title": "Overflowing order",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "items": [
    {items}
  ]
}}
"#
            )
            .as_bytes(),
        );
    }

    let validation = validated_package(Path::new("/package"), &filesystem);
    let Err(Failure::PackageInvalid(diagnosis)) = validation.result else {
        panic!("an error-severity finding is invalid")
    };
    let semantic = findings(&diagnosis)
        .filter_map(|diagnostic| match &diagnostic.evidence {
            DiagnosticEvidence::OrderSemanticInvalid { field, reason, .. } => {
                assert_eq!(diagnostic.code, DiagnosticCode::OrderSemanticInvalid);
                assert_eq!(diagnostic.severity, Severity::Error);
                assert_eq!(diagnostic.scope, EvidenceScope::Order);
                Some((field.as_str(), *reason))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        semantic,
        [
            ("item_total", OrderSemanticReason::NonFiniteTotal),
            ("order_total", OrderSemanticReason::NonFiniteTotal),
        ]
    );
}

fn one_order_filesystem() -> ScriptedFileSystem {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/Orders/Order/README.md", b"Order\n");
    filesystem.put_file("/package/Orders/Order/supporting-a.bin", SUPPORTING_A);
    filesystem.put_file(
        "/package/Orders/Order/nested/supporting-b.bin",
        SUPPORTING_B,
    );
    filesystem
}

fn assert_supporting_material_preserved(filesystem: &ScriptedFileSystem) {
    let supporting_a = Path::new("/package/Orders/Order/supporting-a.bin");
    let supporting_b = Path::new("/package/Orders/Order/nested/supporting-b.bin");
    assert_eq!(
        filesystem.bytes(supporting_a.to_str().unwrap()),
        Some(SUPPORTING_A.to_vec())
    );
    assert_eq!(
        filesystem.bytes(supporting_b.to_str().unwrap()),
        Some(SUPPORTING_B.to_vec())
    );
    assert!(filesystem.calls().iter().all(|call| {
        call.paths()
            .into_iter()
            .all(|path| path != supporting_a && path != supporting_b)
    }));
}

fn two_order_filesystem(enumeration: &[&str]) -> ScriptedFileSystem {
    let filesystem = one_order_filesystem();
    filesystem.put_entry("/package/Orders/A", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/A/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file("/package/Orders/A/.spielorders/order.json", MINIMAL_ORDER);
    filesystem.remove("/package/Orders/Order");
    let second = String::from_utf8(MINIMAL_ORDER.to_vec()).unwrap().replacen(
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "11111111-1111-4111-8111-111111111111",
        1,
    );
    filesystem.put_entry("/package/Orders/B", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/B/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/B/.spielorders/order.json",
        second.as_bytes(),
    );
    enumerate_orders_as(&filesystem, enumeration);
    filesystem
}
