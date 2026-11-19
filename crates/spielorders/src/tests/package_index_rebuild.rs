//! Decision sources: Define durable metadata schemas and version evolution;
//! Define the stable diagnostic vocabulary and evidence contract; Define the complete
//! Package read projection and load-state contract.

use crate::Failure;
use crate::ports::FileSystemEntryKind;
use crate::tests::support::{
    Index, Operation, PlatformFileSystem, ScriptedFileSystem, TemporaryTree, effects,
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

/// A missing, malformed, or stale index is rebuilt to the deterministic bytes, and no
/// staging artifact is left behind.
#[test]
fn a_missing_malformed_or_stale_index_is_rebuilt_exactly() {
    for old_index in [None, Some(b"{ malformed".as_slice()), Some(MINIMAL_INDEX)] {
        let filesystem = minimal_package();
        if let Some(old_index) = old_index {
            filesystem.put_file("/package/.spielorders/index.json", old_index);
        }

        assert!(
            rebuilt(Path::new("/package"), &filesystem).is_ok(),
            "the derived-index rebuild succeeds"
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

/// Each way an index can be unusable is assessed with its own reason, and the stored
/// bytes are left exactly as they were.
#[test]
fn every_unusable_index_is_assessed_with_its_own_reason() {
    let unnormalized = Index::from_json_bytes(
        FULL_INDEX
            .replacen("Laser components", " Laser components ", 1)
            .as_bytes(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap();
    let impossible_date = FULL_INDEX.replacen("2026-07-19", "2026-02-31", 1);
    // The deterministic index fixture, made non-canonical here rather than relied upon
    // to be non-canonical.
    let non_deterministic = [MINIMAL_INDEX, b"\n"].concat();
    /// What is stored at the index path (a directory where absent), and its assessment.
    type Case<'a> = (
        &'a str,
        Option<&'a [u8]>,
        PackageIndexState,
        &'a [PackageIndexMalformedReason],
    );
    let cases: [Case; 9] = [
        (
            "a directory",
            None,
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::WrongKind],
        ),
        (
            "unreadable",
            Some(MINIMAL_INDEX),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::Unreadable],
        ),
        (
            "not UTF-8",
            Some(&[0xff]),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::InvalidEncoding],
        ),
        (
            "not JSON",
            Some(b"{ malformed"),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::InvalidSyntax],
        ),
        (
            "missing vendors",
            Some(b"{\"schema_version\":1,\"orders\":[]}"),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::InvalidShape],
        ),
        (
            "an impossible embedded date",
            Some(impossible_date.as_bytes()),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::InvalidSemantics],
        ),
        (
            "unnormalized embedded metadata",
            Some(&unnormalized),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::InvalidSemantics],
        ),
        (
            "non-deterministic serialization",
            Some(&non_deterministic),
            PackageIndexState::Malformed,
            &[PackageIndexMalformedReason::NonDeterministicSerialization],
        ),
        (
            "a recognizable other version",
            Some(b"{\"schema_version\":2,\"orders\":[],\"vendors\":[]}"),
            PackageIndexState::WrongVersion,
            &[],
        ),
    ];

    for (case, stored, state, reasons) in cases {
        let filesystem = minimal_package();
        match stored {
            Some(bytes) => filesystem.put_file("/package/.spielorders/index.json", bytes),
            None => filesystem.put_entry(
                "/package/.spielorders/index.json",
                FileSystemEntryKind::Directory,
            ),
        }
        if case == "unreadable" {
            filesystem
                .on(Operation::Read)
                .at("/package/.spielorders/index.json")
                .fails();
        }

        let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
            panic!("{case}: a Package whose only problem is its index opens")
        };

        assert_eq!(package.contents.index.state, state, "{case}");
        assert_eq!(package.contents.index.malformed_reasons, reasons, "{case}");
        assert_eq!(
            filesystem
                .bytes("/package/.spielorders/index.json")
                .as_deref(),
            stored,
            "{case}: the stored index is left as it was"
        );
    }
}

/// Each way a well-formed index can disagree with the canonical records is reported as
/// its own stale reason, in a stable order.
#[test]
fn every_disagreement_with_canonical_records_is_its_own_stale_reason() {
    let at_order = String::from_utf8(WRONG_PATH_INDEX.to_vec())
        .unwrap()
        .replacen("Elsewhere/Order", "Orders/Order", 1);
    let former_title = at_order.replacen("Laser components", "Former title", 1);
    let with_order = |index: &[u8]| {
        let filesystem = minimal_package();
        filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
        filesystem.put_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        filesystem.put_file("/package/.spielorders/index.json", index);
        filesystem
    };

    let unindexed = with_order(MINIMAL_INDEX);
    let wrong_path = with_order(WRONG_PATH_INDEX);
    let wrong_kind = minimal_package();
    wrong_kind.put_entry("/package/Orders/Damaged", FileSystemEntryKind::Directory);
    wrong_kind.put_entry(
        "/package/Orders/Damaged/.spielorders/order.json",
        FileSystemEntryKind::Directory,
    );
    wrong_kind.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    let vendors = minimal_package();
    vendors.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    vendors.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    let record = with_order(former_title.as_bytes());
    let readme = with_order(at_order.as_bytes());
    readme.put_file("/package/Orders/Order/README.md", b"new notes\n");

    use PackageIndexStaleReason::*;
    for (case, filesystem, reasons) in [
        (
            "an unindexed Order",
            unindexed,
            &[OrderMembershipChanged][..],
        ),
        (
            "an Order indexed elsewhere",
            wrong_path,
            &[OrderMembershipChanged, OrderPathMismatch],
        ),
        (
            "a record of the wrong kind",
            wrong_kind,
            &[OrderMembershipChanged, ManagedInputWrongKind],
        ),
        ("changed Vendors", vendors, &[VendorsChanged]),
        ("a changed record", record, &[OrderMetadataChanged]),
        ("a changed README", readme, &[OrderReadmeChanged]),
    ] {
        let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
            panic!("{case}: a Package whose only problem is its index opens")
        };
        assert_eq!(
            package.contents.index.state,
            PackageIndexState::Stale,
            "{case}"
        );
        assert_eq!(package.contents.index.stale_reasons, reasons, "{case}");
    }
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
        panic!("a Package whose only problem is its index opens")
    };
    assert_eq!(failed.contents.index.state, PackageIndexState::Malformed);
    assert_eq!(
        failed.contents.index.malformed_reasons,
        &[PackageIndexMalformedReason::InvalidSemantics]
    );
}

/// Package-wide canonical damage, or a version this build does not read, refuses the
/// rebuild and writes no index.
#[test]
fn package_wide_damage_and_an_unsupported_version_refuse_the_rebuild() {
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
        assert!(matches!(
            rebuilt(Path::new("/package"), &filesystem),
            Err(Failure::DamagedPackage(_) | Failure::PackageMetadataUnsupported(_))
        ));
        assert!(!filesystem.exists("/package/.spielorders/index.json"));
        assert_eq!(filesystem.effects(), []);
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
