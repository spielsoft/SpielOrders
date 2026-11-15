//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the stable diagnostic vocabulary and evidence contract; Define the complete
//! Package read projection and load-state contract.

use crate::Failure;
use crate::Package;
use crate::canonical::PackageIndexState;
use crate::metadata::OrderDocument;
use crate::ports::{FileChangeEvidence, FileChangeObservation, FileSystemEntryKind};
use crate::tests::support::{
    Answer, Index, IndexedOrder, Operation, ScriptedFileSystem, enumerate_orders_as,
    minimal_package, opened, proven_current_minimal_package,
};
use crate::tests::support::{path_set, touched_by};
use std::ops::Deref;
use std::path::Path;

const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
));
const FULL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
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
const WRONG_PATH_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/wrong-order-path.json"
));
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));
const SUPPORTING_A_PATH: &str =
    "/package/Orders/2026_07_19_Tarnwick_Laser_components/supporting-a.bin";
const SUPPORTING_B_PATH: &str =
    "/package/Orders/2026_07_19_Tarnwick_Laser_components/nested/supporting-b.bin";

#[test]
fn proven_current_index_opens_without_reading_canonical_records_or_documents() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    budget_reads_to(
        &filesystem,
        &[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ],
    );
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);

    let result = open_preserving_supporting_material(&filesystem);

    assert_eq!(
        touched_by(&filesystem, Operation::Read),
        path_set(&[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ]),
    );
    assert_eq!(
        touched_by(&filesystem, Operation::ObserveFileChange),
        path_set(&[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
            "/package/.spielorders/vendors.json",
        ]),
    );
    assert_eq!(
        filesystem.paths(Operation::ReadDirectory),
        [Path::new("/package/Orders")]
    );
    assert_eq!(filesystem.effects(), []);
    let Ok(package) = result else {
        panic!("a proven-current deterministic index must make the Package available")
    };
    assert_eq!(package.contents.index.state, PackageIndexState::Current);
    assert!(package.contents.healthy_orders.is_empty());
    assert!(package.contents.vendors.is_empty());
    assert!(package.contents.diagnostic_groups.is_empty());
}

#[test]
fn current_full_index_supplies_complete_order_vendor_paths_and_cached_readme() {
    let filesystem = minimal_package();
    let order_directory = "/package/Orders/2026_07_19_Tarnwick_Laser_components";
    let order_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json";
    let readme_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md";
    filesystem.put_entry(order_directory, FileSystemEntryKind::Directory);
    filesystem.put_entry(order_path, FileSystemEntryKind::File);
    filesystem.put_entry(readme_path, FileSystemEntryKind::File);
    let deterministic_index = Index::from_json_bytes(FULL_INDEX)
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic_index);
    budget_reads_to(
        &filesystem,
        &[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ],
    );
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        order_path,
        readme_path,
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);

    let result = open_preserving_supporting_material(&filesystem);
    let Ok(package) = result else {
        panic!("the full proven-current index must make the Package available: {result:?}")
    };

    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("the indexed healthy Order is projected")
    };
    assert_eq!(order.document.title.deref(), "Laser components");
    assert_eq!(
        order.paths.directory.package_relative_path.as_deref(),
        Some("Orders/2026_07_19_Tarnwick_Laser_components")
    );
    assert_eq!(
        order.readme_text.as_deref(),
        Some("User-authored Markdown text")
    );
    let [vendor] = package.contents.vendors.as_slice() else {
        panic!("the complete indexed Vendor is projected")
    };
    assert_eq!(vendor.name.deref(), "Tarnwick");
    assert_eq!(vendor.contacts[0].name.deref(), "Grace Hopper");
    assert_eq!(
        touched_by(&filesystem, Operation::ObserveFileChange),
        path_set(&[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
            "/package/.spielorders/vendors.json",
            order_path,
            readme_path,
        ]),
    );
    assert!(
        !filesystem
            .paths(Operation::ReadDirectory)
            .iter()
            .any(|path| path == Path::new(order_directory))
    );
}

#[test]
fn current_index_derives_canonical_equivalent_totals_vendor_association_and_reusable_values() {
    let (filesystem, _) = full_index_filesystem();

    let Ok(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("the full proven-current index must make the Package available")
    };

    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("one healthy Order is projected")
    };
    assert_eq!(order.item_totals, &[169.0]);
    assert_eq!(order.order_total, 169.0);
    assert_eq!(
        package
            .contents
            .vendor_for_order(0)
            .map(|vendor| vendor.name.deref()),
        Some("Tarnwick")
    );
    assert_eq!(
        package
            .contents
            .projections
            .ordering_institutions
            .iter()
            .map(|entry| (entry.value.as_str(), entry.usage_count))
            .collect::<Vec<_>>(),
        [("Example Institute", 1)]
    );
    assert_eq!(
        package
            .contents
            .projections
            .orderers
            .iter()
            .map(|entry| entry.value.as_str())
            .collect::<Vec<_>>(),
        ["Ada Lovelace"]
    );
    assert_eq!(
        package
            .contents
            .projections
            .projects
            .iter()
            .map(|entry| entry.value.as_str())
            .collect::<Vec<_>>(),
        ["Telescope"]
    );
    assert_eq!(
        package
            .contents
            .projections
            .order_groups
            .iter()
            .map(|entry| entry.value.as_str())
            .collect::<Vec<_>>(),
        ["Summer build"]
    );
}

#[test]
fn newer_vendor_evidence_falls_back_with_the_specific_stale_reason() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    filesystem.put_change_evidence("/package/.spielorders/vendors.json", 1, 21);

    let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
        panic!("newer Vendors must select the canonical rebuild path")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[crate::canonical::PackageIndexStaleReason::VendorsChanged]
    );
}

#[test]
fn equal_change_evidence_never_proves_the_index_current() {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        "/package/.spielorders/index.json",
    ] {
        filesystem.put_change_evidence(path, 1, 20);
    }

    let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
        panic!("equal evidence must select canonical rebuild")
    };
    assert_eq!(
        failed.contents.index.stale_reasons,
        &[crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
    );
}

#[test]
fn newer_config_or_version_evidence_uses_the_generic_uncertain_reason() {
    for newer_path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
    ] {
        let filesystem = minimal_package();
        filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
        for path in [
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/vendors.json",
        ] {
            filesystem.put_change_evidence(path, 1, 10);
        }
        filesystem.put_change_evidence(newer_path, 1, 21);
        filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);

        let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
            panic!("newer {newer_path} must select canonical rebuild")
        };
        assert_eq!(
            failed.contents.index.stale_reasons,
            &[crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
        );
    }
}

#[test]
fn newer_order_metadata_and_present_readme_keep_their_specific_stale_reasons() {
    for (newer_path, expected_reason) in [
        (
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
            crate::canonical::PackageIndexStaleReason::OrderMetadataChanged,
        ),
        (
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
            crate::canonical::PackageIndexStaleReason::OrderReadmeChanged,
        ),
    ] {
        let filesystem = full_canonical_filesystem();
        filesystem.put_change_evidence(newer_path, 1, 21);

        let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
            panic!("newer {newer_path} must select canonical rebuild")
        };
        assert_eq!(failed.contents.index.stale_reasons, &[expected_reason]);
    }
}

#[test]
fn simultaneous_newer_inputs_collect_every_specific_stale_reason_in_contract_order() {
    let filesystem = full_canonical_filesystem();
    for path in [
        "/package/.spielorders/vendors.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    ] {
        filesystem.put_change_evidence(path, 1, 21);
    }

    let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
        panic!("newer canonical inputs must select the canonical rebuild path")
    };

    assert_eq!(
        failed.contents.index.stale_reasons,
        &[
            crate::canonical::PackageIndexStaleReason::VendorsChanged,
            crate::canonical::PackageIndexStaleReason::OrderMetadataChanged,
            crate::canonical::PackageIndexStaleReason::OrderReadmeChanged,
        ]
    );
}

#[test]
fn add_remove_rename_and_index_path_mismatch_all_reject_membership() {
    let added = minimal_package();
    added.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    added.put_entry("/package/Orders/Added", FileSystemEntryKind::Directory);
    added.put_file(
        "/package/Orders/Added/.spielorders/order.json",
        MINIMAL_ORDER,
    );

    let removed = minimal_package();
    removed.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    removed.put_file(
        "/package/.spielorders/index.json",
        &Index::from_json_bytes(FULL_INDEX)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    );

    let renamed = minimal_package();
    renamed.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    renamed.put_file(
        "/package/.spielorders/index.json",
        &Index::from_json_bytes(FULL_INDEX)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    );
    renamed.put_entry("/package/Orders/Renamed", FileSystemEntryKind::Directory);
    renamed.put_file(
        "/package/Orders/Renamed/.spielorders/order.json",
        FULL_ORDER,
    );

    let wrong_path = minimal_package();
    wrong_path.put_file("/package/.spielorders/index.json", WRONG_PATH_INDEX);
    wrong_path.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    wrong_path.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );

    for (filesystem, expected) in [
        (
            added,
            vec![
                crate::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            removed,
            vec![
                crate::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            renamed,
            vec![
                crate::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            wrong_path,
            vec![
                crate::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                crate::canonical::PackageIndexStaleReason::OrderPathMismatch,
                crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
    ] {
        let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
            panic!("membership mismatch must select canonical rebuild")
        };
        assert_eq!(failed.contents.index.stale_reasons, expected);
    }
}

#[test]
fn unavailable_incomparable_uncertain_and_contradictory_evidence_all_fall_back() {
    let unavailable = proven_current_minimal_package();
    unavailable.clear_change_evidence("/package/.spielorders/vendors.json");

    let incomparable = proven_current_minimal_package();
    incomparable.put_change_evidence("/package/.spielorders/vendors.json", 2, 10);

    let uncertain = proven_current_minimal_package();
    uncertain.clear_change_evidence("/package/.spielorders/index.json");

    let contradictory = proven_current_minimal_package();
    contradictory
        .on(Operation::ObserveFileChange)
        .at("/package/.spielorders/index.json")
        .answers(Answer::Change(FileChangeObservation {
            kind: Some(FileSystemEntryKind::Directory),
            evidence: FileChangeEvidence::Comparable {
                comparison_domain: 1,
                sequence: 20,
            },
        }));

    for filesystem in [unavailable, incomparable, uncertain, contradictory] {
        let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
            panic!("non-comparable evidence must select canonical rebuild")
        };
        assert_eq!(
            failed.contents.index.stale_reasons,
            &[crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
        );
    }
}

#[test]
fn every_change_observation_failure_falls_back_without_touching_supporting_material() {
    let root_paths = [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
        "/package/.spielorders/vendors.json",
    ];
    let order_paths = [
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    ];
    for failed_path in root_paths.into_iter().chain(order_paths) {
        let filesystem = full_canonical_filesystem();
        let supporting_a = "/package/Orders/2026_07_19_Tarnwick_Laser_components/supporting-a.bin";
        let supporting_b =
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/nested/supporting-b.bin";
        filesystem.put_file(supporting_a, SUPPORTING_A);
        filesystem.put_file(supporting_b, SUPPORTING_B);
        filesystem
            .on(Operation::ObserveFileChange)
            .at(failed_path)
            .fails();

        let Ok(failed) = open_preserving_supporting_material(&filesystem) else {
            panic!("failed observation at {failed_path} must select canonical rebuild")
        };
        assert_eq!(
            failed.contents.index.stale_reasons,
            &[crate::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
        );
        assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
        assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
        assert!(filesystem.calls().iter().all(|call| {
            call.paths()
                .into_iter()
                .all(|path| path != Path::new(supporting_a) && path != Path::new(supporting_b))
        }));
    }
}

#[test]
fn fast_success_exposes_no_unexamined_diagnostics_or_supporting_material_evidence() {
    let (filesystem, _) = full_index_filesystem();
    let supporting_a = "/package/Orders/2026_07_19_Tarnwick_Laser_components/supporting-a.bin";
    let supporting_b =
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/nested/supporting-b.bin";
    filesystem.put_file(supporting_a, SUPPORTING_A);
    filesystem.put_file(supporting_b, SUPPORTING_B);
    filesystem.put_entry("/package/README.md", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/.spielorders/.vendors.json.spielorders-stage",
        FileSystemEntryKind::File,
    );

    let Ok(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("unexamined content cannot prevent a proven-current fast load")
    };

    assert!(package.contents.diagnostic_groups.is_empty());
    assert!(package.contents.damaged_orders.is_empty());
    assert_eq!(
        package
            .contents
            .context
            .package_readme
            .location
            .package_relative_path
            .as_deref(),
        Some("README.md")
    );
    assert_eq!(
        package.contents.healthy_orders[0].readme_text.as_deref(),
        Some("User-authored Markdown text")
    );
    assert!(filesystem.calls().iter().all(|call| {
        call.paths().into_iter().all(|path| {
            path != Path::new(supporting_a)
                && path != Path::new(supporting_b)
                && path != Path::new("/package/.spielorders/.vendors.json.spielorders-stage")
        })
    }));
    // The README is the open's own document, read beside the Package and judged there.
    assert_eq!(package.readme.text, None);
    assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
    assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
}

#[test]
fn fast_projection_is_independent_of_filesystem_enumeration_order() {
    let forward = two_order_current_filesystem(&["Earlier", "Later"]);
    let reverse = two_order_current_filesystem(&["Later", "Earlier"]);

    let Ok(forward_package) = open_preserving_supporting_material(&forward) else {
        panic!("forward enumeration must use the current index")
    };
    let Ok(reverse_package) = open_preserving_supporting_material(&reverse) else {
        panic!("reverse enumeration must use the current index")
    };

    assert_eq!(forward_package, reverse_package);
    assert_eq!(
        forward_package
            .contents
            .healthy_orders
            .iter()
            .map(|order| order
                .paths
                .directory
                .package_relative_path
                .as_ref()
                .unwrap())
            .collect::<Vec<_>>(),
        ["Orders/Later", "Orders/Earlier"]
    );
}

#[test]
fn every_wrong_no_follow_input_kind_rejects_fast_trust() {
    let root_paths = [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        "/package/.spielorders/index.json",
    ];
    let order_paths = [
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    ];
    for kind in [
        FileSystemEntryKind::Directory,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        for path in root_paths {
            let filesystem = proven_current_minimal_package();
            filesystem.put_entry(path, kind);
            assert_not_fast_success(
                open_preserving_supporting_material(&filesystem),
                &filesystem,
            );
        }
        for path in order_paths {
            let filesystem = full_canonical_filesystem();
            filesystem.put_entry(path, kind);
            assert_not_fast_success(
                open_preserving_supporting_material(&filesystem),
                &filesystem,
            );
        }
    }
}

#[test]
fn every_missing_managed_or_readme_input_rejects_fast_trust() {
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        "/package/.spielorders/index.json",
    ] {
        let filesystem = proven_current_minimal_package();
        filesystem.remove(path);
        assert_not_fast_success(
            open_preserving_supporting_material(&filesystem),
            &filesystem,
        );
    }
    for path in [
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    ] {
        let filesystem = full_canonical_filesystem();
        filesystem.remove(path);
        assert_not_fast_success(
            open_preserving_supporting_material(&filesystem),
            &filesystem,
        );
    }
}

#[test]
fn fast_load_sources_only_direct_child_structural_warnings_from_membership_listing() {
    let filesystem = proven_current_minimal_package();
    filesystem.put_entry("/package/Orders/loose.txt", FileSystemEntryKind::File);
    filesystem.put_entry("/package/Orders/.hidden", FileSystemEntryKind::Directory);
    budget_reads_to(
        &filesystem,
        &[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ],
    );

    let Ok(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("direct-child warnings do not prevent a proven-current fast load")
    };

    let [group] = package.contents.diagnostic_groups.as_slice() else {
        panic!("one Package warning group is projected")
    };
    let [warning] = group.diagnostics.as_slice() else {
        panic!("one visible direct-child warning is projected")
    };
    assert_eq!(
        warning.code,
        crate::readme::DiagnosticCode::OrdersDirectoryEntryDisallowed
    );
    assert_eq!(
        warning.primary_location.package_relative_path.as_deref(),
        Some("Orders/loose.txt")
    );
    assert_eq!(
        filesystem.paths(Operation::ReadDirectory),
        [Path::new("/package/Orders")]
    );
    assert!(filesystem.calls().iter().all(|call| {
        call.paths().into_iter().all(|path| {
            path != Path::new("/package/Orders/loose.txt")
                && path != Path::new("/package/Orders/.hidden")
        })
    }));
}

fn full_index_filesystem() -> (ScriptedFileSystem, Vec<u8>) {
    let filesystem = minimal_package();
    let order_directory = "/package/Orders/2026_07_19_Tarnwick_Laser_components";
    let order_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json";
    let readme_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md";
    filesystem.put_entry(order_directory, FileSystemEntryKind::Directory);
    filesystem.put_entry(order_path, FileSystemEntryKind::File);
    filesystem.put_entry(readme_path, FileSystemEntryKind::File);
    let deterministic_index = Index::from_json_bytes(FULL_INDEX)
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic_index);
    budget_reads_to(
        &filesystem,
        &[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ],
    );
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        order_path,
        readme_path,
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    (filesystem, deterministic_index)
}

fn full_canonical_filesystem() -> ScriptedFileSystem {
    let (filesystem, _) = full_index_filesystem();
    filesystem.on(Operation::Read).proceeds();
    filesystem.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    filesystem.put_file(
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        FULL_ORDER,
    );
    filesystem.put_file(
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
        b"User-authored Markdown text",
    );
    filesystem
}

fn two_order_current_filesystem(enumeration: &[&str]) -> ScriptedFileSystem {
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
    let index = Index {
        schema_version: 1,
        orders: vec![
            IndexedOrder {
                order_directory: "Orders/Later".into(),
                order: serde_json::from_slice::<OrderDocument>(later.as_bytes()).unwrap(),
                readme: "later notes".into(),
            },
            IndexedOrder {
                order_directory: "Orders/Earlier".into(),
                order: serde_json::from_slice::<OrderDocument>(MINIMAL_ORDER).unwrap(),
                readme: "earlier notes".into(),
            },
        ],
        vendors: Vec::new(),
        values: None,
    };
    filesystem.put_file(
        "/package/.spielorders/index.json",
        &index.to_json_bytes().unwrap(),
    );
    for name in ["Earlier", "Later"] {
        let directory = format!("/package/Orders/{name}");
        let order = format!("{directory}/.spielorders/order.json");
        let readme = format!("{directory}/README.md");
        filesystem.put_entry(&directory, FileSystemEntryKind::Directory);
        filesystem.put_entry(&order, FileSystemEntryKind::File);
        filesystem.put_entry(&readme, FileSystemEntryKind::File);
        filesystem.put_change_evidence(&order, 1, 10);
        filesystem.put_change_evidence(&readme, 1, 10);
    }
    enumerate_orders_as(&filesystem, enumeration);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    budget_reads_to(
        &filesystem,
        &[
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/index.json",
        ],
    );
    filesystem
}

/// Any read outside `paths` fails, so a load that needs more than these cannot pass.
fn budget_reads_to(filesystem: &ScriptedFileSystem, paths: &[&str]) {
    filesystem.on(Operation::Read).fails();
    for path in paths {
        filesystem.on(Operation::Read).at(path).proceeds();
    }
}

/// Opens the Package with Supporting Material beside its Order, when it has that
/// Order, and proves the open wrote nothing and neither touched nor changed it.
fn open_preserving_supporting_material(
    filesystem: &ScriptedFileSystem,
) -> Result<Package, Failure> {
    if filesystem.exists("/package/Orders/2026_07_19_Tarnwick_Laser_components") {
        filesystem.put_file(SUPPORTING_A_PATH, SUPPORTING_A);
        filesystem.put_file(SUPPORTING_B_PATH, SUPPORTING_B);
    }
    let supporting_a_before = filesystem.bytes(SUPPORTING_A_PATH);
    let supporting_b_before = filesystem.bytes(SUPPORTING_B_PATH);

    let result = opened(Path::new("/package"), filesystem);

    assert_eq!(filesystem.effects(), [], "opening writes nothing");
    assert_eq!(filesystem.bytes(SUPPORTING_A_PATH), supporting_a_before);
    assert_eq!(filesystem.bytes(SUPPORTING_B_PATH), supporting_b_before);
    assert!(filesystem.calls().iter().all(|call| {
        call.paths().into_iter().all(|path| {
            path != Path::new(SUPPORTING_A_PATH) && path != Path::new(SUPPORTING_B_PATH)
        })
    }));
    result.result
}

/// Asserts the open did not trust the stored index: a Package it made available was
/// projected from the canonical records, which the current-index fast path never reads.
fn assert_not_fast_success(result: Result<Package, Failure>, filesystem: &ScriptedFileSystem) {
    if result.is_ok() {
        assert!(
            filesystem
                .paths(Operation::Read)
                .iter()
                .any(|path| path.ends_with(".spielorders/vendors.json")),
            "an untrusted index is answered from the canonical records"
        );
    }
}
