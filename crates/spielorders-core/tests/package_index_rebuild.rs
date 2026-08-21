//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the stable M2 diagnostic vocabulary and evidence contract; Define the complete M2
//! Package read projection and load-state contract.

mod support;

use spielorders_core::canonical::{
    PackageIndexMalformedReason, PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
    PackageLoad, open_package,
};
use spielorders_core::metadata::IndexDocument;
use spielorders_core::ports::FileSystemEntryKind;
use spielorders_core::readme::{
    OperationEvidence, OperationFailureCode, PackageIndexReplacementPhase,
};
use std::path::{Path, PathBuf};
use support::{
    RealFileSystem, TemporaryTree,
    package_index::{Effect, ReplacementBoundary, TestFileSystem},
};

const VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
));
const FULL_INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
));
const NON_DETERMINISTIC_MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/index.json"
));
const WRONG_PATH_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/wrong-order-path.json"
));

#[test]
fn missing_index_is_rebuilt_exactly_and_the_package_becomes_available() {
    let filesystem = TestFileSystem::minimal_package();

    let result = open_package(Path::new("/package"), &filesystem);

    let PackageLoad::Available(package) = result else {
        panic!("a successful required index rebuild makes the Package available")
    };
    assert_eq!(package.index().state(), PackageIndexState::Current);
    assert_eq!(package.index().refresh(), PackageIndexRefresh::Rebuilt);
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(MINIMAL_INDEX.to_vec())
    );
    let staging = PathBuf::from("/package/.spielorders/.index.json.spielorders-stage");
    assert!(
        filesystem.effects.borrow().iter().any(|effect| {
            effect == &Effect::WriteStage(staging.clone(), MINIMAL_INDEX.to_vec())
        })
    );
    let stage_effects = filesystem
        .effects
        .borrow()
        .iter()
        .filter(|effect| match effect {
            Effect::Read(path)
            | Effect::Sync(path)
            | Effect::Remove(path)
            | Effect::WriteStage(path, _) => path == &staging,
            Effect::Replace(source, destination) => {
                source == &staging || destination == Path::new("/package/.spielorders/index.json")
            }
            _ => false,
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        stage_effects,
        [
            Effect::WriteStage(staging.clone(), MINIMAL_INDEX.to_vec()),
            Effect::Read(staging.clone()),
            Effect::Sync(staging.clone()),
            Effect::Replace(
                staging.clone(),
                PathBuf::from("/package/.spielorders/index.json"),
            ),
        ]
    );
    assert_eq!(
        filesystem
            .effects
            .borrow()
            .iter()
            .filter_map(|effect| match effect {
                Effect::ListDirectory(path) => Some(path.as_path()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [Path::new("/package/Orders")]
    );
    assert!(filesystem.effects.borrow().iter().any(|effect| {
        effect
            == &Effect::Replace(
                staging.clone(),
                PathBuf::from("/package/.spielorders/index.json"),
            )
    }));
}

#[test]
fn valid_equal_index_without_freshness_evidence_is_conservatively_stale() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let result = open_package(Path::new("/package"), &filesystem);

    let PackageLoad::IndexReplacementFailed(failed) = result else {
        panic!("the injected replacement failure preserves the stale assessment")
    };
    assert_eq!(failed.index().state(), PackageIndexState::Stale);
    assert_eq!(
        failed.index().stale_reasons(),
        &[PackageIndexStaleReason::ChangeEvidenceUncertain]
    );
}

#[test]
fn index_kind_observation_failure_is_an_operation_failure_without_replacement() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.fail_observation("/package/.spielorders/index.json");

    let PackageLoad::OperationFailed(failed) = open_package(Path::new("/package"), &filesystem)
    else {
        panic!("an unknown index kind cannot safely select replacement")
    };

    assert_eq!(
        failed.failure().code(),
        OperationFailureCode::FilesystemOperationFailed
    );
    assert_eq!(
        failed.failure().evidence(),
        &OperationEvidence::FileSystem {
            operation: spielorders_core::readme::FileSystemOperation::ObserveKind,
        }
    );
    assert_eq!(
        failed.failure().primary_location().package_relative_path(),
        Some(".spielorders/index.json")
    );
    assert!(!filesystem.effects.borrow().iter().any(|effect| matches!(
        effect,
        Effect::WriteStage(..) | Effect::Sync(..) | Effect::Replace(..) | Effect::Remove(..)
    )));
}

#[test]
fn staging_write_failure_blocks_opening_and_preserves_missing_assessment() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let result = open_package(Path::new("/package"), &filesystem);

    let PackageLoad::IndexReplacementFailed(failed) = result else {
        panic!("a required replacement failure blocks ordinary Package use")
    };
    assert_eq!(failed.index().state(), PackageIndexState::Missing);
    assert_eq!(
        failed.index().refresh(),
        PackageIndexRefresh::ReplacementFailed
    );
    assert!(failed.old_index_bytes().is_none());
    assert!(failed.canonical_evidence().healthy_orders().is_empty());
    assert!(failed.canonical_evidence().damaged_orders().is_empty());
    assert_eq!(
        failed.failure().code(),
        OperationFailureCode::PackageIndexReplacementFailed
    );
    assert_eq!(
        failed.failure().evidence(),
        &OperationEvidence::PackageIndexReplacement {
            phase: PackageIndexReplacementPhase::WriteStage,
            cleanup_failure: None,
        }
    );
    assert_eq!(
        failed.failure().primary_location().package_relative_path(),
        Some(".spielorders/index.json")
    );
    assert_eq!(
        failed.failure().related_locations()[0].package_relative_path(),
        Some(".spielorders/.index.json.spielorders-stage")
    );
}

#[test]
fn wrong_kind_index_preserves_the_exact_malformed_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry(
        "/package/.spielorders/index.json",
        FileSystemEntryKind::Directory,
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.index().state(), PackageIndexState::Malformed);
    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::WrongKind]
    );
}

#[test]
fn unreadable_index_preserves_the_exact_malformed_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.fail_read("/package/.spielorders/index.json");
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.index().state(), PackageIndexState::Malformed);
    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::Unreadable]
    );
}

#[test]
fn invalid_encoding_preserves_reason_and_old_bytes_on_replacement_failure() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", &[0xff]);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidEncoding]
    );
    assert_eq!(failed.old_index_bytes(), Some([0xff].as_slice()));
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(vec![0xff])
    );
}

#[test]
fn invalid_syntax_preserves_the_exact_malformed_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", b"{ malformed");
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidSyntax]
    );
}

#[test]
fn invalid_shape_preserves_the_exact_malformed_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        b"{\"schema_version\":1,\"orders\":[]}",
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidShape]
    );
}

#[test]
fn recognizable_wrong_version_is_distinct_from_malformed() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        b"{\"schema_version\":2,\"orders\":[],\"vendors\":[]}",
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.index().state(), PackageIndexState::WrongVersion);
    assert!(failed.index().malformed_reasons().is_empty());
    assert_eq!(failed.index().observed_version(), Some(2));
    assert_eq!(failed.index().expected_version(), Some(1));
}

#[test]
fn semantically_invalid_embedded_order_is_malformed_before_freshness() {
    let filesystem = TestFileSystem::minimal_package();
    let impossible_date = FULL_INDEX.replacen("2026-07-19", "2026-02-31", 1);
    filesystem.put_file(
        "/package/.spielorders/index.json",
        impossible_date.as_bytes(),
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.index().state(), PackageIndexState::Malformed);
    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn valid_noncanonical_json_preserves_non_deterministic_serialization_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        NON_DETERMINISTIC_MINIMAL_INDEX,
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::NonDeterministicSerialization]
    );
}

#[test]
fn discovered_order_membership_change_precedes_uncertain_change_evidence() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn index_order_path_mismatch_is_distinct_and_ordered_after_membership() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/.spielorders/index.json", WRONG_PATH_INDEX);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::OrderPathMismatch,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn managed_order_input_wrong_kind_is_a_separate_stale_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/Orders/Damaged", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Damaged/.spielorders/order.json",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ManagedInputWrongKind,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
    assert_eq!(failed.canonical_evidence().damaged_orders().len(), 1);
}

#[test]
fn canonical_vendor_change_is_a_separate_stale_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::VendorsChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn canonical_order_metadata_change_is_a_separate_stale_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    let stale_index = String::from_utf8(WRONG_PATH_INDEX.to_vec())
        .unwrap()
        .replacen("Elsewhere/Order", "Orders/Order", 1)
        .replacen("Laser components", "Former title", 1);
    filesystem.put_file("/package/.spielorders/index.json", stale_index.as_bytes());
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::OrderMetadataChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn canonical_order_readme_change_is_a_separate_stale_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/Orders/Order/README.md", b"new notes\n");
    let stale_index = String::from_utf8(WRONG_PATH_INDEX.to_vec())
        .unwrap()
        .replacen("Elsewhere/Order", "Orders/Order", 1);
    filesystem.put_file("/package/.spielorders/index.json", stale_index.as_bytes());
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            PackageIndexStaleReason::OrderReadmeChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn rebuild_bytes_are_enumeration_independent_sorted_and_healthy_only() {
    fn filesystem_with_orders(enumeration: &[&str]) -> TestFileSystem {
        let filesystem = TestFileSystem::minimal_package();
        let later = String::from_utf8(MINIMAL_ORDER.to_vec())
            .unwrap()
            .replacen(
                "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
                "11111111-1111-4111-8111-111111111111",
                1,
            )
            .replacen("Laser components", "Later order", 1)
            .replacen("2026-07-19", "2027-01-01", 1);
        for name in ["Earlier", "Later", "Damaged"] {
            filesystem.put_entry(
                &format!("/package/Orders/{name}"),
                FileSystemEntryKind::Directory,
            );
        }
        filesystem.put_file(
            "/package/Orders/Earlier/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        filesystem.put_file(
            "/package/Orders/Later/.spielorders/order.json",
            later.as_bytes(),
        );
        filesystem.put_file(
            "/package/Orders/Damaged/.spielorders/order.json",
            b"{ malformed",
        );
        filesystem.put_file("/package/Orders/Earlier/README.md", b"earlier notes\n");
        filesystem.put_file("/package/Orders/Later/README.md", b"later notes\n");
        filesystem.enumerate_orders_as(enumeration);
        filesystem
    }

    let forward = filesystem_with_orders(&["Earlier", "Later", "Damaged"]);
    let reverse = filesystem_with_orders(&["Damaged", "Later", "Earlier"]);

    let PackageLoad::Available(forward_package) = open_package(Path::new("/package"), &forward)
    else {
        panic!("the forward rebuild succeeds")
    };
    let PackageLoad::Available(reverse_package) = open_package(Path::new("/package"), &reverse)
    else {
        panic!("the reverse rebuild succeeds")
    };

    let forward_bytes = forward
        .bytes("/package/.spielorders/index.json")
        .expect("the forward index was written");
    let reverse_bytes = reverse
        .bytes("/package/.spielorders/index.json")
        .expect("the reverse index was written");
    assert_eq!(forward_bytes, reverse_bytes);
    let index = IndexDocument::from_json_bytes(&forward_bytes).unwrap();
    assert_eq!(
        index
            .orders
            .iter()
            .map(|entry| entry.order_directory.as_str())
            .collect::<Vec<_>>(),
        ["Orders/Later", "Orders/Earlier"]
    );
    assert_eq!(index.orders[0].readme, "later notes\n");
    assert_eq!(index.orders[1].readme, "earlier notes\n");
    assert_eq!(forward_package.damaged_orders().len(), 1);
    assert_eq!(reverse_package.damaged_orders().len(), 1);
    assert!(
        !String::from_utf8(forward_bytes)
            .unwrap()
            .contains("Damaged")
    );
}

#[test]
fn staged_deterministic_but_semantically_invalid_index_is_rejected() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    let impossible = FULL_INDEX.replacen("2026-07-19", "2026-02-31", 1);
    let deterministic_impossible = IndexDocument::from_json_bytes(impossible.as_bytes())
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.override_staged_bytes(deterministic_impossible);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("semantic candidate validation blocks replacement")
    };

    assert_eq!(
        failed.failure().evidence(),
        &OperationEvidence::PackageIndexReplacement {
            phase: PackageIndexReplacementPhase::CandidateValidation,
            cleanup_failure: None,
        }
    );
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(MINIMAL_INDEX.to_vec())
    );
}

#[test]
fn staged_valid_deterministic_bytes_that_differ_from_the_projection_are_rejected() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.override_staged_bytes(WRONG_PATH_INDEX.to_vec());

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("candidate validation must bind staged bytes to the canonical projection")
    };

    assert_eq!(
        failed.failure().evidence(),
        &OperationEvidence::PackageIndexReplacement {
            phase: PackageIndexReplacementPhase::CandidateValidation,
            cleanup_failure: None,
        }
    );
    assert_eq!(failed.old_index_bytes(), Some(MINIMAL_INDEX));
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(MINIMAL_INDEX.to_vec())
    );
}

#[test]
fn index_semantics_require_already_normalized_embedded_metadata() {
    let filesystem = TestFileSystem::minimal_package();
    let unnormalized = FULL_INDEX.replacen("Laser components", " Laser components ", 1);
    let deterministic = IndexDocument::from_json_bytes(unnormalized.as_bytes())
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("an index containing unnormalized embedded metadata is malformed")
    };

    assert_eq!(failed.index().state(), PackageIndexState::Malformed);
    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn unsorted_index_orders_are_invalid_semantics_before_freshness() {
    fn package_with_two_orders() -> TestFileSystem {
        let filesystem = TestFileSystem::minimal_package();
        let later = String::from_utf8(MINIMAL_ORDER.to_vec())
            .unwrap()
            .replacen(
                "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
                "11111111-1111-4111-8111-111111111111",
                1,
            )
            .replacen("Laser components", "Later order", 1)
            .replacen("2026-07-19", "2027-01-01", 1);
        for name in ["Earlier", "Later"] {
            filesystem.put_entry(
                &format!("/package/Orders/{name}"),
                FileSystemEntryKind::Directory,
            );
        }
        filesystem.put_file(
            "/package/Orders/Earlier/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        filesystem.put_file(
            "/package/Orders/Later/.spielorders/order.json",
            later.as_bytes(),
        );
        filesystem
    }

    let source = package_with_two_orders();
    assert!(matches!(
        open_package(Path::new("/package"), &source),
        PackageLoad::Available(_)
    ));
    let mut unsorted = IndexDocument::from_json_bytes(
        &source
            .bytes("/package/.spielorders/index.json")
            .expect("source rebuild wrote an index"),
    )
    .unwrap();
    unsorted.orders.reverse();

    let filesystem = package_with_two_orders();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        &unsorted.to_json_bytes().unwrap(),
    );
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_package(Path::new("/package"), &filesystem)
    else {
        panic!("the injected replacement failure preserves invalid semantics")
    };
    assert_eq!(failed.index().state(), PackageIndexState::Malformed);
    assert_eq!(
        failed.index().malformed_reasons(),
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn successful_malformed_and_uncertain_stale_refreshes_return_current_rebuilt_without_diagnostic() {
    for old_index in [b"{ malformed".as_slice(), MINIMAL_INDEX] {
        let filesystem = TestFileSystem::minimal_package();
        filesystem.put_file("/package/.spielorders/index.json", old_index);

        let PackageLoad::Available(package) = open_package(Path::new("/package"), &filesystem)
        else {
            panic!("the required derived-index refresh succeeds")
        };

        assert_eq!(package.index().state(), PackageIndexState::Current);
        assert_eq!(package.index().refresh(), PackageIndexRefresh::Rebuilt);
        assert!(package.index().malformed_reasons().is_empty());
        assert!(package.index().stale_reasons().is_empty());
        assert!(package.diagnostic_groups().is_empty());
    }
}

#[test]
fn package_wide_damage_and_unsupported_version_never_attempt_replacement() {
    for filesystem in {
        let missing_vendors = TestFileSystem::minimal_package();
        missing_vendors
            .entries
            .borrow_mut()
            .remove(Path::new("/package/.spielorders/vendors.json"));
        missing_vendors
            .files
            .borrow_mut()
            .remove(Path::new("/package/.spielorders/vendors.json"));

        let unsupported = TestFileSystem::minimal_package();
        unsupported.put_file(
            "/package/.spielorders/version.json",
            b"{\"metadata_version\":999}\n",
        );
        [missing_vendors, unsupported]
    } {
        let PackageLoad::DiagnosticOnly(package) = open_package(Path::new("/package"), &filesystem)
        else {
            panic!("Package-wide canonical damage is diagnostic-only")
        };
        assert_eq!(package.index().state(), PackageIndexState::Missing);
        assert_eq!(package.index().refresh(), PackageIndexRefresh::NotAttempted);
        assert!(!filesystem.effects.borrow().iter().any(|effect| {
            matches!(
                effect,
                Effect::WriteStage(..)
                    | Effect::Sync(..)
                    | Effect::Replace(..)
                    | Effect::Remove(..)
            )
        }));
    }
}

#[test]
fn success_and_every_replacement_failure_preserve_and_never_touch_supporting_material() {
    let scenarios = [
        None,
        Some(ReplacementBoundary::WriteStage),
        Some(ReplacementBoundary::ReadStage),
        Some(ReplacementBoundary::CandidateValidation),
        Some(ReplacementBoundary::SyncStage),
        Some(ReplacementBoundary::Replace),
        Some(ReplacementBoundary::RemoveStage),
    ];
    for boundary in scenarios {
        let filesystem = TestFileSystem::minimal_package();
        filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
        filesystem.put_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        let supporting_a = "/package/Orders/Order/supporting-a.bin";
        let supporting_b = "/package/Orders/Order/nested/supporting-b.bin";
        filesystem.put_file(supporting_a, SUPPORTING_A);
        filesystem.put_file(supporting_b, SUPPORTING_B);
        if let Some(boundary) = boundary {
            filesystem.fail_at(boundary);
        }

        let result = open_package(Path::new("/package"), &filesystem);

        if boundary.is_none() {
            assert!(matches!(result, PackageLoad::Available(_)));
        } else {
            assert!(matches!(result, PackageLoad::IndexReplacementFailed(_)));
        }
        assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
        assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
        assert!(filesystem.effects.borrow().iter().all(|effect| {
            let paths: Vec<&Path> = match effect {
                Effect::Read(path)
                | Effect::Sync(path)
                | Effect::Remove(path)
                | Effect::Canonicalize(path)
                | Effect::ObserveKind(path)
                | Effect::ObserveChange(path)
                | Effect::ListDirectory(path) => vec![path],
                Effect::WriteStage(path, _) => vec![path],
                Effect::Replace(source, destination) => vec![source, destination],
            };
            paths
                .into_iter()
                .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
        }));
    }
}

#[test]
fn package_index_public_vocabulary_uses_every_closed_spelling() {
    for (state, spelling) in [
        (PackageIndexState::Current, "current"),
        (PackageIndexState::Missing, "missing"),
        (PackageIndexState::Malformed, "malformed"),
        (PackageIndexState::WrongVersion, "wrong_version"),
        (PackageIndexState::Stale, "stale"),
        (PackageIndexState::NotAssessed, "not_assessed"),
    ] {
        assert_eq!(state.as_str(), spelling);
    }
    for (refresh, spelling) in [
        (PackageIndexRefresh::NotNeeded, "not_needed"),
        (PackageIndexRefresh::Rebuilt, "rebuilt"),
        (PackageIndexRefresh::NotAttempted, "not_attempted"),
        (PackageIndexRefresh::ReplacementFailed, "replacement_failed"),
    ] {
        assert_eq!(refresh.as_str(), spelling);
    }
    for (reason, spelling) in [
        (PackageIndexMalformedReason::WrongKind, "wrong_kind"),
        (PackageIndexMalformedReason::Unreadable, "unreadable"),
        (
            PackageIndexMalformedReason::InvalidEncoding,
            "invalid_encoding",
        ),
        (PackageIndexMalformedReason::InvalidSyntax, "invalid_syntax"),
        (PackageIndexMalformedReason::InvalidShape, "invalid_shape"),
        (
            PackageIndexMalformedReason::InvalidSemantics,
            "invalid_semantics",
        ),
        (
            PackageIndexMalformedReason::NonDeterministicSerialization,
            "non_deterministic_serialization",
        ),
    ] {
        assert_eq!(reason.as_str(), spelling);
    }
    for (reason, spelling) in [
        (
            PackageIndexStaleReason::OrderMembershipChanged,
            "order_membership_changed",
        ),
        (
            PackageIndexStaleReason::OrderPathMismatch,
            "order_path_mismatch",
        ),
        (
            PackageIndexStaleReason::ManagedInputWrongKind,
            "managed_input_wrong_kind",
        ),
        (PackageIndexStaleReason::VendorsChanged, "vendors_changed"),
        (
            PackageIndexStaleReason::OrderMetadataChanged,
            "order_metadata_changed",
        ),
        (
            PackageIndexStaleReason::OrderReadmeChanged,
            "order_readme_changed",
        ),
        (
            PackageIndexStaleReason::ChangeEvidenceUncertain,
            "change_evidence_uncertain",
        ),
    ] {
        assert_eq!(reason.as_str(), spelling);
    }
    for (phase, spelling) in [
        (PackageIndexReplacementPhase::WriteStage, "write_stage"),
        (PackageIndexReplacementPhase::ReadStage, "read_stage"),
        (
            PackageIndexReplacementPhase::CandidateValidation,
            "candidate_validation",
        ),
        (PackageIndexReplacementPhase::SyncStage, "sync_stage"),
        (PackageIndexReplacementPhase::Replace, "replace"),
        (PackageIndexReplacementPhase::RemoveStage, "remove_stage"),
    ] {
        assert_eq!(phase.as_str(), spelling);
    }
}

#[test]
fn real_filesystem_rebuild_is_atomic_and_preserves_both_sentinels() {
    let temporary = TemporaryTree::new("package-index-real-rebuild");
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
    std::fs::write(package_metadata.join("version.json"), VERSION).unwrap();
    std::fs::write(package_metadata.join("vendors.json"), VENDORS).unwrap();
    std::fs::write(order_metadata.join("order.json"), MINIMAL_ORDER).unwrap();
    std::fs::write(&supporting_a, SUPPORTING_A).unwrap();
    std::fs::write(&supporting_b, SUPPORTING_B).unwrap();

    let result = open_package(&root, &RealFileSystem);

    let PackageLoad::Available(package) = result else {
        panic!("real validated replacement makes the Package available")
    };
    assert_eq!(package.index().refresh(), PackageIndexRefresh::Rebuilt);
    let index_path = package_metadata.join("index.json");
    let index = IndexDocument::from_json_bytes(&std::fs::read(&index_path).unwrap()).unwrap();
    assert_eq!(index.orders.len(), 1);
    assert!(
        !package_metadata
            .join(".index.json.spielorders-stage")
            .exists()
    );
    assert_eq!(std::fs::read(&supporting_a).unwrap(), SUPPORTING_A);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), SUPPORTING_B);
}
