//! Decision sources: Define the stable M2 diagnostic vocabulary and evidence contract;
//! Define the complete M2 Package read projection and load-state contract; Decide how Package
//! Index freshness is proven without canonical rereads.

use crate::support::{
    RealFileSystem, TemporaryTree,
    package_index::{Effect, TestFileSystem},
};
use spielorders_core::{
    canonical::{
        CanonicalPackageLoad, ExhaustivePackageValidation, PackageIndexRefresh, PackageLoad,
        inspect_canonical_package, open_package, validate_package_exhaustively,
    },
    digest::ContentDigest,
    ports::{DirectoryEntry, FileSystem, FileSystemEntryKind},
    readme::{DiagnosticCode, DiagnosticEvidence, EvidenceScope, Severity},
};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};

const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
));
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
    let filesystem = minimal_current_filesystem();
    filesystem.put_entry(
        "/package/.spielorders/unexpected.txt",
        FileSystemEntryKind::File,
    );
    let supporting_a = "/package/Orders/.opaque/supporting-a.bin";
    let supporting_b = "/package/Orders/.opaque/nested/supporting-b.bin";
    filesystem.put_file(supporting_a, SUPPORTING_A);
    filesystem.put_file(supporting_b, SUPPORTING_B);

    let PackageLoad::Available(before) = open_package(Path::new("/package"), &filesystem) else {
        panic!("the deterministic index is proven current")
    };
    assert_eq!(before.index().refresh(), PackageIndexRefresh::NotNeeded);
    assert!(before.diagnostic_groups().is_empty());
    let fast_effects = filesystem.effects.borrow().clone();

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("managed evidence can be exhaustively validated")
    };

    assert!(
        report.is_valid(),
        "a reserved warning does not damage the Package"
    );
    let warning = report
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::ReservedContentUnexpected)
        .expect("exhaustive validation reports unexpected reserved content");
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.scope(), EvidenceScope::Package);
    assert_eq!(
        warning.primary_location().package_relative_path(),
        Some(".spielorders/unexpected.txt")
    );
    assert_eq!(
        warning.evidence(),
        &DiagnosticEvidence::ReservedContentUnexpected {
            observed_kind: FileSystemEntryKind::File,
        }
    );
    assert_eq!(before.diagnostic_groups(), &[]);
    assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
    assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
    assert!(filesystem.effects.borrow().iter().all(|effect| {
        effect
            .paths()
            .into_iter()
            .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
    }));
    assert!(!fast_effects.iter().any(|effect| matches!(
        effect,
        Effect::ListDirectory(path) if path == Path::new("/package/.spielorders")
    )));
    assert!(filesystem.effects.borrow().iter().any(|effect| matches!(
        effect,
        Effect::ListDirectory(path) if path == Path::new("/package/.spielorders")
    )));
}

#[test]
fn exhaustive_validation_reads_exact_documents_and_reports_staging_with_known_destination_validity()
{
    let filesystem = TestFileSystem::minimal_package();
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

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("healthy managed evidence is completely validated")
    };

    assert!(report.is_valid());
    assert!(report.package_readme().exists());
    assert_eq!(report.package_readme().text(), Some("# Package notes\n"));
    let [order_readme] = report.order_readmes() else {
        panic!("every discovered Order README has exact evidence")
    };
    assert!(order_readme.exists());
    assert_eq!(order_readme.text(), Some("# Order notes\n"));
    let diagnostics = report
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    let staging = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::StagingArtifactPresent)
        .collect::<Vec<_>>();
    assert_eq!(staging.len(), 2);
    assert_eq!(
        DiagnosticCode::ReservedContentUnexpected.as_str(),
        "reserved_content_unexpected"
    );
    assert_eq!(
        DiagnosticCode::StagingArtifactPresent.as_str(),
        "staging_artifact_present"
    );
    assert!(staging.iter().all(|diagnostic| {
        diagnostic.severity() == Severity::Warning && diagnostic.related_locations().len() == 1
    }));
    // Each staging diagnostic names the candidate actually sitting at that name, not
    // merely that some candidate is there: promotion writes those exact bytes.
    let staged_evidence = |name: &str| {
        staging
            .iter()
            .find(|diagnostic| {
                diagnostic
                    .primary_location()
                    .absolute_path()
                    .ends_with(name)
            })
            .unwrap_or_else(|| panic!("a staging diagnostic at {name}"))
            .evidence()
            .clone()
    };
    assert_eq!(
        staged_evidence(".vendors.json.spielorders-stage"),
        DiagnosticEvidence::StagingArtifactPresent {
            destination_valid: Some(true),
            candidate_digest: Some(ContentDigest::of(STAGED_VENDORS)),
        }
    );
    assert_eq!(
        staged_evidence(".order.json.spielorders-stage"),
        DiagnosticEvidence::StagingArtifactPresent {
            destination_valid: Some(true),
            candidate_digest: Some(ContentDigest::of(STAGED_ORDER)),
        }
    );
    let unexpected = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::ReservedContentUnexpected)
        .expect("unexpected Order-reserved content is reported");
    assert_eq!(unexpected.scope(), EvidenceScope::Order);
    assert_eq!(
        unexpected.evidence(),
        &DiagnosticEvidence::ReservedContentUnexpected {
            observed_kind: FileSystemEntryKind::SymbolicLink,
        }
    );
}

#[test]
fn missing_and_unavailable_readmes_keep_distinct_empty_evidence_without_duplicate_warnings() {
    let filesystem = TestFileSystem::minimal_package();
    for name in ["A", "B"] {
        filesystem.put_entry(
            &format!("/package/Orders/{name}"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_entry(
            &format!("/package/Orders/{name}/.spielorders"),
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

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("README warnings do not prevent a validation result")
    };

    assert!(report.is_valid());
    assert_eq!(report.order_readmes().len(), 2);
    let missing = report
        .order_readmes()
        .iter()
        .find(|readme| {
            readme.reference().location().package_relative_path() == Some("Orders/A/README.md")
        })
        .unwrap();
    let unavailable = report
        .order_readmes()
        .iter()
        .find(|readme| {
            readme.reference().location().package_relative_path() == Some("Orders/B/README.md")
        })
        .unwrap();
    assert!(!missing.exists());
    assert_eq!(missing.text(), Some(""));
    assert!(missing.diagnostics().is_empty());
    assert!(unavailable.exists());
    assert_eq!(unavailable.text(), None);
    assert_eq!(unavailable.diagnostics().len(), 1);
    let readme_groups = report
        .diagnostic_groups()
        .iter()
        .filter(|group| group.scope() == EvidenceScope::Document)
        .collect::<Vec<_>>();
    assert_eq!(readme_groups.len(), 1, "exact duplicate warnings collapse");
    assert_eq!(readme_groups[0].diagnostics().len(), 1);
    assert_eq!(
        readme_groups[0].diagnostics()[0].code(),
        DiagnosticCode::ReadmeUnavailable
    );
}

#[test]
fn exhaustive_validation_uses_exact_authorized_effect_budgets_and_never_mutates_or_observes_supporting_material()
 {
    let filesystem = TestFileSystem::minimal_package();
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

    let ExhaustivePackageValidation::Complete(_) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("the complete authorized evidence produces a validation result")
    };

    let effects = filesystem.effects.borrow();
    let reads = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Read(path) => Some(path.as_path()),
            _ => None,
        })
        .collect::<Vec<_>>();
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
    let listings = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ListDirectory(path) => Some(path.as_path()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        listings,
        [
            Path::new("/package/Orders"),
            Path::new("/package/.spielorders"),
            Path::new("/package/Orders/Order/.spielorders"),
        ]
    );
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        Effect::WriteStage(..) | Effect::Sync(..) | Effect::Replace(..) | Effect::Remove(..)
    )));
    assert!(effects.iter().all(|effect| {
        effect
            .paths()
            .into_iter()
            .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
    }));
    assert_eq!(filesystem.bytes(supporting_a), before_a);
    assert_eq!(filesystem.bytes(supporting_b), before_b);
}

#[test]
fn combined_damage_documents_reserved_content_and_staging_have_complete_deterministic_groups() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/README.md", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/.spielorders/unexpected-root",
        FileSystemEntryKind::Other,
    );
    for name in ["A", "B"] {
        filesystem.put_entry(
            &format!("/package/Orders/{name}"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_entry(
            &format!("/package/Orders/{name}/.spielorders"),
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

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("persistent canonical damage still produces exhaustive diagnostics")
    };

    assert!(
        !report.is_valid(),
        "every Damaged Order invalidates exhaustive Package validation"
    );
    let group_keys = report
        .diagnostic_groups()
        .iter()
        .map(|group| {
            (
                group.scope(),
                group.location().package_relative_path().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        group_keys,
        [
            (EvidenceScope::Package, "."),
            (EvidenceScope::Document, "README.md"),
            (EvidenceScope::Order, "Orders/A"),
            (EvidenceScope::Order, "Orders/B"),
            (EvidenceScope::Document, "Orders/B/README.md"),
        ]
    );
    let order_groups = report
        .diagnostic_groups()
        .iter()
        .filter(|group| group.scope() == EvidenceScope::Order)
        .collect::<Vec<_>>();
    assert_eq!(order_groups.len(), 2);
    for group in &order_groups {
        let duplicate = group
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::OrderIdDuplicate)
            .expect("every duplicate-ID participant explains itself");
        let DiagnosticEvidence::OrderIdDuplicate {
            conflict_locations, ..
        } = duplicate.evidence()
        else {
            panic!("duplicate evidence is typed")
        };
        assert_eq!(
            conflict_locations
                .iter()
                .map(|location| location.package_relative_path().unwrap())
                .collect::<Vec<_>>(),
            ["Orders/A", "Orders/B"]
        );
    }
    let a_diagnostics = order_groups[0].diagnostics();
    let first_warning = a_diagnostics
        .iter()
        .position(|diagnostic| diagnostic.severity() == Severity::Warning)
        .unwrap();
    assert!(
        a_diagnostics[..first_warning]
            .iter()
            .all(|diagnostic| diagnostic.severity() == Severity::Error)
    );
    let staging = a_diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::StagingArtifactPresent)
        .unwrap();
    assert_eq!(
        staging.evidence(),
        &DiagnosticEvidence::StagingArtifactPresent {
            destination_valid: Some(false),
            candidate_digest: Some(ContentDigest::of(STAGED_ORDER)),
        }
    );
    assert!(report.package_readme().diagnostics().len() == 1);
    assert_eq!(report.order_readmes().len(), 2);
    assert_eq!(
        filesystem
            .effects
            .borrow()
            .iter()
            .filter(|effect| matches!(
                effect,
                Effect::Read(path) if path == Path::new("/package/Orders/B/README.md")
            ))
            .count(),
        1,
        "a healthy duplicate-ID participant's exact README is read once"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_reserved_content_conservatively_prevents_an_incomplete_validation_report() {
    use spielorders_core::readme::{FileSystemOperation, OperationEvidence};
    use std::os::unix::ffi::OsStringExt;

    let filesystem = one_order_filesystem();
    let path = Path::new("/package/.spielorders")
        .join(std::ffi::OsString::from_vec(b"unexpected-\xff".to_vec()));
    filesystem.put_entry_path(&path, FileSystemEntryKind::File);

    let ExhaustivePackageValidation::OperationFailed(failed) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("unrepresentable reserved evidence cannot produce an incomplete report")
    };

    assert_eq!(
        failed
            .failure()
            .primary_location()
            .map(|location| location.absolute_path()),
        Some(Path::new("/package/.spielorders"))
    );
    assert_eq!(
        failed.failure().evidence(),
        &OperationEvidence::FileSystem {
            operation: FileSystemOperation::ListDirectory,
        }
    );
    assert_supporting_material_preserved(&filesystem);
}

#[test]
fn every_authorized_canonicalize_kind_and_list_failure_returns_exact_operation_evidence() {
    use spielorders_core::readme::{FileSystemOperation, OperationEvidence};

    enum Failure {
        Canonicalize(&'static str),
        Observe(&'static str),
        List(&'static str),
    }
    let cases = [
        Failure::Canonicalize("/package"),
        Failure::Canonicalize("/package/Orders"),
        Failure::Canonicalize("/package/Orders/Order"),
        Failure::Observe("/package/.spielorders"),
        Failure::Observe("/package/.spielorders/config.toml"),
        Failure::Observe("/package/.spielorders/version.json"),
        Failure::Observe("/package/.spielorders/vendors.json"),
        Failure::Observe("/package"),
        Failure::Observe("/package/Removed"),
        Failure::Observe("/package/Orders"),
        Failure::Observe("/package/Orders/Order/.spielorders/order.json"),
        Failure::Observe("/package/README.md"),
        Failure::Observe("/package/Orders/Order/README.md"),
        Failure::Observe("/package/Orders/Order/.spielorders"),
        Failure::List("/package/Orders"),
        Failure::List("/package/.spielorders"),
        Failure::List("/package/Orders/Order/.spielorders"),
    ];

    for failure in cases {
        let filesystem = one_order_filesystem();
        let (expected_operation, expected_path) = match failure {
            Failure::Canonicalize(path) => {
                filesystem.fail_canonicalize(path);
                (FileSystemOperation::Canonicalize, path)
            }
            Failure::Observe(path) => {
                filesystem.fail_observation(path);
                (FileSystemOperation::ObserveKind, path)
            }
            Failure::List(path) => {
                filesystem.fail_list(path);
                (FileSystemOperation::ListDirectory, path)
            }
        };

        let ExhaustivePackageValidation::OperationFailed(failed) =
            validate_package_exhaustively(Path::new("/package"), &filesystem)
        else {
            panic!("{expected_operation:?} failure at {expected_path} cannot establish validation")
        };
        assert_eq!(
            failed
                .failure()
                .primary_location()
                .map(|location| location.absolute_path()),
            Some(Path::new(expected_path))
        );
        assert_eq!(
            failed.failure().evidence(),
            &OperationEvidence::FileSystem {
                operation: expected_operation,
            }
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
        filesystem.fail_read(path);
        let ExhaustivePackageValidation::Complete(report) =
            validate_package_exhaustively(Path::new("/package"), &filesystem)
        else {
            panic!("a persistent required-artifact read failure is canonical evidence")
        };
        assert!(!report.is_valid());
        let matches = report
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .filter(|diagnostic| {
                diagnostic.code() == DiagnosticCode::ManagedArtifactUnreadable
                    && diagnostic.scope() == expected_scope
                    && diagnostic.primary_location().absolute_path() == Path::new(path)
            })
            .count();
        assert_eq!(matches, 1, "{path} remains one canonical diagnostic");
        assert_eq!(
            filesystem
                .effects
                .borrow()
                .iter()
                .filter(
                    |effect| matches!(effect, Effect::Read(observed) if observed == Path::new(path))
                )
                .count(),
            1,
            "{path} is attempted exactly once"
        );
        assert_supporting_material_preserved(&filesystem);
    }

    for path in ["/package/README.md", "/package/Orders/Order/README.md"] {
        let filesystem = one_order_filesystem();
        filesystem.put_file(path, b"readable before injection");
        filesystem.fail_read(path);
        let ExhaustivePackageValidation::Complete(report) =
            validate_package_exhaustively(Path::new("/package"), &filesystem)
        else {
            panic!("document unavailability still produces a validation result")
        };
        assert!(report.is_valid(), "README warnings never create damage");
        let matches = report
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .filter(|diagnostic| {
                diagnostic.code() == DiagnosticCode::ReadmeUnavailable
                    && diagnostic.primary_location().absolute_path() == Path::new(path)
            })
            .count();
        assert_eq!(matches, 1, "{path} warning is deduplicated");
        assert_eq!(
            filesystem
                .effects
                .borrow()
                .iter()
                .filter(
                    |effect| matches!(effect, Effect::Read(observed) if observed == Path::new(path))
                )
                .count(),
            1,
            "{path} is attempted exactly once"
        );
        assert_supporting_material_preserved(&filesystem);
    }
}

#[test]
fn exhaustive_validation_never_retrofits_or_changes_proven_current_fast_load_evidence() {
    let filesystem = minimal_current_filesystem();
    filesystem.put_entry(
        "/package/.spielorders/unexpected.txt",
        FileSystemEntryKind::File,
    );

    let PackageLoad::Available(before) = open_package(Path::new("/package"), &filesystem) else {
        panic!("the index starts proven current")
    };
    let ExhaustivePackageValidation::Complete(validation) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("the separate exhaustive call completes")
    };
    let PackageLoad::Available(after) = open_package(Path::new("/package"), &filesystem) else {
        panic!("the index remains proven current")
    };

    assert_eq!(before, after);
    assert!(before.diagnostic_groups().is_empty());
    assert!(after.diagnostic_groups().is_empty());
    assert!(validation.diagnostic_groups().iter().any(|group| {
        group
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ReservedContentUnexpected)
    }));

    let CanonicalPackageLoad::CurrentCandidate(canonical) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("the same canonical evidence remains current")
    };
    assert_eq!(
        before.healthy_orders(),
        canonical.healthy_orders(),
        "ordinary public Order equality is independent of the load route"
    );
}

#[test]
fn ordinary_public_order_equality_ignores_unexposed_readme_observation_state() {
    let filesystem = one_order_filesystem();
    filesystem.remove_path("/package/Orders/Order/README.md");
    let CanonicalPackageLoad::CurrentCandidate(absent) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("an absent README does not damage the Order")
    };

    filesystem.put_file("/package/Orders/Order/README.md", b"");
    let CanonicalPackageLoad::CurrentCandidate(empty) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("an empty README does not damage the Order")
    };

    assert_eq!(
        absent.healthy_orders(),
        empty.healthy_orders(),
        "unexposed validation evidence cannot alter ordinary public equality"
    );
}

#[test]
fn exhaustive_reports_are_independent_of_directory_enumeration_and_opaque_source_errors() {
    let forward = two_order_filesystem(&["A", "B"]);
    let reverse = two_order_filesystem(&["B", "A"]);

    let forward_report = validate_package_exhaustively(Path::new("/package"), &forward);
    let reverse_report = validate_package_exhaustively(Path::new("/package"), &reverse);

    assert_eq!(forward_report, reverse_report);

    let forward_failure = two_order_filesystem(&["A", "B"]);
    let reverse_failure = two_order_filesystem(&["B", "A"]);
    forward_failure.fail_list("/package/.spielorders");
    reverse_failure.fail_list("/package/.spielorders");
    assert_eq!(
        validate_package_exhaustively(Path::new("/package"), &forward_failure),
        validate_package_exhaustively(Path::new("/package"), &reverse_failure),
    );
}

#[derive(Default)]
struct RecordingRealFileSystem {
    effects: RefCell<Vec<Effect>>,
}

impl FileSystem for RecordingRealFileSystem {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.effects.borrow_mut().push(Effect::Read(path.into()));
        RealFileSystem.read(path)
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        panic!("exhaustive validation never writes")
    }

    fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        panic!("exhaustive validation never stages")
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        panic!("exhaustive validation never syncs")
    }

    fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
        panic!("exhaustive validation never replaces")
    }

    fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
        panic!("exhaustive validation never removes")
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Canonicalize(path.into()));
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveKind(path.into()));
        RealFileSystem.symlink_metadata(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ListDirectory(path.into()));
        RealFileSystem.read_directory(path)
    }
}

#[test]
fn real_exhaustive_validation_preserves_both_opaque_sentinels_and_touches_only_owned_evidence() {
    let temporary = TemporaryTree::new("exhaustive-validation-preservation");
    let root = temporary.path().join("package");
    let package_metadata = root.join(".spielorders");
    let order = root.join("Orders/Order");
    let order_metadata = order.join(".spielorders");
    let supporting_a = order.join("supporting-a.bin");
    let supporting_b = order.join("nested/supporting-b.bin");
    std::fs::create_dir_all(&package_metadata).unwrap();
    std::fs::create_dir_all(&order_metadata).unwrap();
    std::fs::create_dir_all(supporting_b.parent().unwrap()).unwrap();
    std::fs::write(package_metadata.join("config.toml"), b"").unwrap();
    std::fs::write(
        package_metadata.join("version.json"),
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/examples/v1/minimal/version.json"
        )),
    )
    .unwrap();
    std::fs::write(
        package_metadata.join("vendors.json"),
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/examples/v1/minimal/vendors.json"
        )),
    )
    .unwrap();
    std::fs::write(package_metadata.join("unexpected"), b"preserve").unwrap();
    std::fs::write(order_metadata.join("order.json"), MINIMAL_ORDER).unwrap();
    std::fs::write(order_metadata.join("unexpected"), b"preserve").unwrap();
    std::fs::write(order.join("README.md"), b"Order notes\n").unwrap();
    std::fs::write(&supporting_a, SUPPORTING_A).unwrap();
    std::fs::write(&supporting_b, SUPPORTING_B).unwrap();
    let before_a = std::fs::read(&supporting_a).unwrap();
    let before_b = std::fs::read(&supporting_b).unwrap();
    let canonical_root = std::fs::canonicalize(&root).unwrap();
    let canonical_supporting_a = std::fs::canonicalize(&supporting_a).unwrap();
    let canonical_supporting_b = std::fs::canonicalize(&supporting_b).unwrap();
    let filesystem = RecordingRealFileSystem::default();

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(&root, &filesystem)
    else {
        panic!("the real managed tree can be validated")
    };

    assert!(report.is_valid());
    assert_eq!(std::fs::read(&supporting_a).unwrap(), before_a);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), before_b);
    let effects = filesystem.effects.into_inner();
    assert!(effects.iter().all(|effect| {
        effect.paths().into_iter().all(|path| {
            path != canonical_supporting_a
                && path != canonical_supporting_b
                && !path.starts_with(canonical_root.join("Orders/Order/nested"))
        })
    }));
    assert_eq!(
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::ListDirectory(path) => Some(path.as_path()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            canonical_root.join("Orders"),
            canonical_root.join(".spielorders"),
            canonical_root.join("Orders/Order/.spielorders"),
        ]
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

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("exhaustive validation retains all independently knowable evidence")
    };

    assert!(!report.is_valid());
    let codes = report
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .map(|diagnostic| diagnostic.code())
        .collect::<Vec<_>>();
    for code in [
        DiagnosticCode::VendorIdDuplicate,
        DiagnosticCode::OrderSemanticInvalid,
        DiagnosticCode::ReadmeUnavailable,
        DiagnosticCode::ReservedContentUnexpected,
    ] {
        assert!(codes.contains(&code), "{code:?} remains visible");
    }
    assert_eq!(report.order_readmes().len(), 1);
    assert_eq!(report.order_readmes()[0].text(), None);
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
    let filesystem = TestFileSystem::minimal_package();
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
            &format!("{path}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_file(
            &format!("{path}/.spielorders/order.json"),
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

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(Path::new("/package"), &filesystem)
    else {
        panic!("a Package holding only Order-local damage validates completely")
    };

    assert!(!report.is_valid(), "an error-severity finding is invalid");
    let semantic = report
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .filter_map(|diagnostic| match diagnostic.evidence() {
            DiagnosticEvidence::OrderSemanticInvalid { field, reason, .. } => {
                assert_eq!(diagnostic.code(), DiagnosticCode::OrderSemanticInvalid);
                assert_eq!(diagnostic.severity(), Severity::Error);
                assert_eq!(diagnostic.scope(), EvidenceScope::Order);
                Some((field.as_str(), reason.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        semantic,
        [
            ("item_total", "non_finite_total"),
            ("order_total", "non_finite_total"),
        ]
    );
}

fn minimal_current_filesystem() -> TestFileSystem {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    filesystem
}

fn one_order_filesystem() -> TestFileSystem {
    let filesystem = TestFileSystem::minimal_package();
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

fn assert_supporting_material_preserved(filesystem: &TestFileSystem) {
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
    assert!(filesystem.effects.borrow().iter().all(|effect| {
        effect
            .paths()
            .into_iter()
            .all(|path| path != supporting_a && path != supporting_b)
    }));
}

fn two_order_filesystem(enumeration: &[&str]) -> TestFileSystem {
    let filesystem = one_order_filesystem();
    filesystem.put_entry("/package/Orders/A", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/A/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file("/package/Orders/A/.spielorders/order.json", MINIMAL_ORDER);
    filesystem.remove_path("/package/Orders/Order");
    filesystem.remove_path("/package/Orders/Order/.spielorders");
    filesystem.remove_path("/package/Orders/Order/.spielorders/order.json");
    filesystem.remove_path("/package/Orders/Order/README.md");
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
    filesystem.enumerate_orders_as(enumeration);
    filesystem
}
