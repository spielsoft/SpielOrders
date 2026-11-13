//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the stable diagnostic vocabulary and evidence contract; Define the complete
//! Package read projection and load-state contract.

use crate::Coded;
use crate::Failure;
use crate::ports::FileSystemEntryKind;
use crate::tests::support::assert_reported_beside;
use crate::tests::support::{
    FailedCommit, Index, Operation, PlatformFileSystem, ScriptedFileSystem, TemporaryTree, effects,
    enumerate_orders_as, failed_location, minimal_package, opened, rebuild_index,
};
use crate::{ChangeResult, Report};
use crate::{
    canonical::PackageIndexMalformedReason, canonical::PackageIndexStaleReason,
    canonical::PackageIndexState,
};
use std::path::Path;

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
const WRONG_PATH_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/wrong-order-path.json"
));

/// A deliberate rebuild of the derived index of the Package at `root`, planned and
/// committed over `filesystem`.
fn rebuilt<F: crate::ports::FileSystem>(root: &Path, filesystem: &F) -> Result<(), Failure> {
    let effects = effects(filesystem);
    let plan = crate::plan(&rebuild_index(root), &effects).result?;
    match crate::commit(plan, &effects).result? {
        Report::Committed {
            result: ChangeResult::RebuildIndex,
        } => Ok(()),
        other => panic!("a rebuild commits or refuses: {other:?}"),
    }
}

#[test]
fn missing_index_is_rebuilt_exactly_and_the_package_becomes_available() {
    let filesystem = minimal_package();

    assert!(
        rebuilt(Path::new("/package"), &filesystem).is_ok(),
        "a successful index rebuild makes the Package available"
    );
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(MINIMAL_INDEX.to_vec())
    );
    assert!(
        !filesystem.exists("/package/.spielorders/.index.json.spielorders-stage"),
        "no staging artifact is left behind"
    );
}

#[test]
fn valid_equal_index_without_freshness_evidence_is_conservatively_stale() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);

    let result = opened(Path::new("/package"), &filesystem);

    let Ok(failed) = result.result else {
        panic!("the injected replacement failure preserves the stale assessment")
    };
    assert_eq!(failed.contents.index.state, PackageIndexState::Stale);
    assert_eq!(
        failed.contents.index.stale_reasons,
        &[PackageIndexStaleReason::ChangeEvidenceUncertain]
    );
}

#[test]
fn index_kind_observation_failure_is_an_operation_failure_without_replacement() {
    let filesystem = minimal_package();
    filesystem
        .on(Operation::SymlinkMetadata)
        .at("/package/.spielorders/index.json")
        .fails();

    let Err(Failure::PackageOperationFailed(failed)) =
        opened(Path::new("/package"), &filesystem).result
    else {
        panic!("an unknown index kind cannot safely select replacement")
    };

    assert_eq!(
        failed_location(&failed.path)
            .and_then(|location| location.package_relative_path.as_deref()),
        Some(".spielorders/index.json")
    );
    assert_eq!(filesystem.effects(), []);
}

#[test]
fn staging_write_failure_refuses_the_rebuild_and_leaves_the_index_missing() {
    let filesystem = minimal_package();
    filesystem.on(Operation::WriteNew).fails();

    let Err(failed) = rebuilt(Path::new("/package"), &filesystem) else {
        panic!("a replacement failure refuses the rebuild")
    };
    assert_reported_beside(&failed);
    assert!(failed.cleanup_failures().is_empty());
    assert_eq!(
        failed.failure().location.package_relative_path.as_deref(),
        Some(".spielorders/index.json")
    );
    assert!(!filesystem.exists("/package/.spielorders/index.json"));
}

#[test]
fn wrong_kind_index_preserves_the_exact_malformed_reason() {
    let filesystem = minimal_package();
    filesystem.put_entry(
        "/package/.spielorders/index.json",
        FileSystemEntryKind::Directory,
    );

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::WrongKind]
    );
}

#[test]
fn unreadable_index_preserves_the_exact_malformed_reason() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem
        .on(Operation::Read)
        .at("/package/.spielorders/index.json")
        .fails();

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::Unreadable]
    );
}

#[test]
fn invalid_encoding_preserves_reason_and_old_bytes_on_replacement_failure() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", &[0xff]);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidEncoding]
    );
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(vec![0xff])
    );
}

#[test]
fn invalid_syntax_preserves_the_exact_malformed_reason() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", b"{ malformed");

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidSyntax]
    );
}

#[test]
fn invalid_shape_preserves_the_exact_malformed_reason() {
    let filesystem = minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        b"{\"schema_version\":1,\"orders\":[]}",
    );

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidShape]
    );
}

#[test]
fn recognizable_wrong_version_is_distinct_from_malformed() {
    let filesystem = minimal_package();
    filesystem.put_file(
        "/package/.spielorders/index.json",
        b"{\"schema_version\":2,\"orders\":[],\"vendors\":[]}",
    );

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.contents.index.state, PackageIndexState::WrongVersion);
    assert!(failed.contents.index.malformed_reasons.is_empty());
}

#[test]
fn semantically_invalid_embedded_order_is_malformed_before_freshness() {
    let filesystem = minimal_package();
    let impossible_date = FULL_INDEX.replacen("2026-07-19", "2026-02-31", 1);
    filesystem.put_file(
        "/package/.spielorders/index.json",
        impossible_date.as_bytes(),
    );

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn valid_noncanonical_json_preserves_non_deterministic_serialization_reason() {
    let filesystem = minimal_package();
    // The deterministic index fixture, made non-canonical here rather than relied
    // upon to be non-canonical.
    let non_deterministic = [MINIMAL_INDEX, b"\n"].concat();
    filesystem.put_file("/package/.spielorders/index.json", &non_deterministic);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the original assessment")
    };

    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::NonDeterministicSerialization]
    );
}

#[test]
fn discovered_order_membership_change_precedes_uncertain_change_evidence() {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn index_order_path_mismatch_is_distinct_and_ordered_after_membership() {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/.spielorders/index.json", WRONG_PATH_INDEX);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::OrderPathMismatch,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn managed_order_input_wrong_kind_is_a_separate_stale_reason() {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/Orders/Damaged", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Damaged/.spielorders/order.json",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::OrderMembershipChanged,
            PackageIndexStaleReason::ManagedInputWrongKind,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
    assert_eq!(failed.contents.damaged_orders.len(), 1);
}

#[test]
fn canonical_vendor_change_is_a_separate_stale_reason() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::VendorsChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn canonical_order_metadata_change_is_a_separate_stale_reason() {
    let filesystem = minimal_package();
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

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::OrderMetadataChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn canonical_order_readme_change_is_a_separate_stale_reason() {
    let filesystem = minimal_package();
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

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves the stale reasons")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            PackageIndexStaleReason::OrderReadmeChanged,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ]
    );
}

#[test]
fn rebuild_bytes_are_enumeration_independent_sorted_and_healthy_only() {
    fn filesystem_with_orders(enumeration: &[&str]) -> ScriptedFileSystem {
        let filesystem = minimal_package();
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
                format!("/package/Orders/{name}"),
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
        enumerate_orders_as(&filesystem, enumeration);
        filesystem
    }

    let forward = filesystem_with_orders(&["Earlier", "Later", "Damaged"]);
    let reverse = filesystem_with_orders(&["Damaged", "Later", "Earlier"]);

    assert!(
        rebuilt(Path::new("/package"), &forward).is_ok(),
        "the forward rebuild succeeds"
    );
    assert!(
        rebuilt(Path::new("/package"), &reverse).is_ok(),
        "the reverse rebuild succeeds"
    );

    let forward_bytes = forward
        .bytes("/package/.spielorders/index.json")
        .expect("the forward index was written");
    let reverse_bytes = reverse
        .bytes("/package/.spielorders/index.json")
        .expect("the reverse index was written");
    assert_eq!(forward_bytes, reverse_bytes);
    let index = Index::from_json_bytes(&forward_bytes).unwrap();
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
    assert!(
        !String::from_utf8(forward_bytes)
            .unwrap()
            .contains("Damaged")
    );
}

#[test]
fn index_semantics_require_already_normalized_embedded_metadata() {
    let filesystem = minimal_package();
    let unnormalized = FULL_INDEX.replacen("Laser components", " Laser components ", 1);
    let deterministic = Index::from_json_bytes(unnormalized.as_bytes())
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic);

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("an index containing unnormalized embedded metadata is malformed")
    };

    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn unsorted_index_orders_are_invalid_semantics_before_freshness() {
    fn package_with_two_orders() -> ScriptedFileSystem {
        let filesystem = minimal_package();
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
                format!("/package/Orders/{name}"),
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
    assert!(rebuilt(Path::new("/package"), &source).is_ok());
    let mut unsorted = Index::from_json_bytes(
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

    let Ok(failed) = opened(Path::new("/package"), &filesystem).result else {
        panic!("the injected replacement failure preserves invalid semantics")
    };
    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

#[test]
fn malformed_and_uncertain_stale_indexes_are_rebuilt_exactly() {
    for old_index in [b"{ malformed".as_slice(), MINIMAL_INDEX] {
        let filesystem = minimal_package();
        filesystem.put_file("/package/.spielorders/index.json", old_index);

        assert!(
            rebuilt(Path::new("/package"), &filesystem).is_ok(),
            "the derived-index rebuild succeeds"
        );
        assert_eq!(
            filesystem.bytes("/package/.spielorders/index.json"),
            Some(MINIMAL_INDEX.to_vec())
        );
    }
}

#[test]
fn package_wide_damage_and_unsupported_version_never_attempt_replacement() {
    for filesystem in {
        let missing_vendors = minimal_package();
        missing_vendors.remove("/package/.spielorders/vendors.json");

        let unsupported = minimal_package();
        unsupported.put_file(
            "/package/.spielorders/version.json",
            b"{\"metadata_version\":999}\n",
        );
        [missing_vendors, unsupported]
    } {
        let Err(Failure::DamagedPackage(package) | Failure::PackageMetadataUnsupported(package)) =
            opened(Path::new("/package"), &filesystem).result
        else {
            panic!("Package-wide canonical damage is diagnostic-only")
        };
        assert_eq!(package.index.state, PackageIndexState::Missing);
        assert_eq!(filesystem.effects(), []);
    }
}

#[test]
fn success_and_every_replacement_failure_preserve_and_never_touch_supporting_material() {
    let scenarios = [
        &[][..],
        &[Operation::WriteNew],
        &[Operation::SyncFile],
        &[Operation::ReplaceFile],
        &[Operation::SyncFile, Operation::RemoveFile],
    ];
    for failing in scenarios {
        let filesystem = minimal_package();
        filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
        filesystem.put_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        let supporting_a = "/package/Orders/Order/supporting-a.bin";
        let supporting_b = "/package/Orders/Order/nested/supporting-b.bin";
        filesystem.put_file(supporting_a, SUPPORTING_A);
        filesystem.put_file(supporting_b, SUPPORTING_B);
        for operation in failing {
            filesystem.on(*operation).fails();
        }

        let result = rebuilt(Path::new("/package"), &filesystem);

        assert_eq!(result.is_ok(), failing.is_empty(), "{failing:?}");
        assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
        assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
        assert!(filesystem.calls().iter().all(|call| {
            [Some(call.path.as_path()), call.source.as_deref()]
                .into_iter()
                .flatten()
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
        assert_eq!(state.code(), spelling);
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
        assert_eq!(reason.code(), spelling);
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
        assert_eq!(reason.code(), spelling);
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

    assert!(
        rebuilt(&root, &PlatformFileSystem).is_ok(),
        "real validated replacement makes the Package available"
    );
    let index_path = package_metadata.join("index.json");
    let index = Index::from_json_bytes(&std::fs::read(&index_path).unwrap()).unwrap();
    assert_eq!(index.orders.len(), 1);
    assert!(
        !package_metadata
            .join(".index.json.spielorders-stage")
            .exists()
    );
    assert_eq!(std::fs::read(&supporting_a).unwrap(), SUPPORTING_A);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), SUPPORTING_B);
}

/// A rebuild replaces a stale index. If someone else writes that index between the
/// plan assessing it and the commit renaming over it, the rebuild must not discard
/// their work — and the caller must be able to tell that from a failed rename,
/// because retrying is right for one and wrong for the other.
#[test]
fn a_rebuild_refuses_an_index_another_writer_changed_and_says_which() {
    const THEIRS: &[u8] = b"{\n  \"schema_version\": 1,\n  \"orders\": [],\n  \"vendors\": []\n}\n";
    let filesystem = minimal_package();
    filesystem
        .on(Operation::WriteNew)
        .once()
        .preceded_by(|outside| outside.write(Path::new("/package/.spielorders/index.json"), THEIRS))
        .proceeds();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file("/package/.spielorders/index.json", WRONG_PATH_INDEX);

    let Err(_failed) = rebuilt(Path::new("/package"), &filesystem) else {
        panic!("an index changed underneath the rebuild does not replace it");
    };

    // Their index is intact.
    assert_eq!(
        filesystem
            .bytes("/package/.spielorders/index.json")
            .unwrap(),
        THEIRS
    );
}

/// The same protection where the plan observed no index at all. Absence is what the
/// rebuild is authorized against, so an index appearing at the path afterwards — a
/// concurrent rebuild, or a restore — is a changed destination like any other, and
/// renaming over it would discard work this rebuild never saw.
#[test]
fn a_rebuild_refuses_an_index_that_appeared_where_the_load_observed_none() {
    const THEIRS: &[u8] = b"{\n  \"schema_version\": 1,\n  \"orders\": [],\n  \"vendors\": []\n}\n";
    let filesystem = minimal_package();
    filesystem
        .on(Operation::WriteNew)
        .once()
        .preceded_by(|outside| outside.write(Path::new("/package/.spielorders/index.json"), THEIRS))
        .proceeds();
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    // No index is put at the destination, so the plan observes nothing there.

    let Err(_failed) = rebuilt(Path::new("/package"), &filesystem) else {
        panic!("an index that appeared underneath the rebuild does not replace it");
    };

    assert_eq!(
        filesystem
            .bytes("/package/.spielorders/index.json")
            .unwrap(),
        THEIRS,
        "their index survives"
    );
}

/// An index that is there but could not be read is not an index observed absent, and
/// the difference is the whole reason absence is checked by observing the path. The
/// rebuild heals an unreadable index — a rename replaces a file it never has to read —
/// and calling that a changed destination would strand the Package in a state whose
/// only repair is the rebuild being refused.
///
/// The two malformed-reason cases next door cannot see this: both inject a staging
/// write failure, which fires before the destination is ever compared.
#[test]
fn an_unreadable_index_is_healed_rather_than_refused_as_a_changed_destination() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem
        .on(Operation::Read)
        .at("/package/.spielorders/index.json")
        .fails();

    assert!(
        rebuilt(Path::new("/package"), &filesystem).is_ok(),
        "an unreadable index is rebuilt rather than refused"
    );
    assert_eq!(
        filesystem.bytes("/package/.spielorders/index.json"),
        Some(MINIMAL_INDEX.to_vec()),
        "the rebuild reaches the destination it could not read"
    );
}
