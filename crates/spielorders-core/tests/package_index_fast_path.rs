//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the stable M2 diagnostic vocabulary and evidence contract; Define the complete M2
//! Package read projection and load-state contract.

use crate::support::package_index::{Effect, ReplacementBoundary, TestFileSystem};
use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexState, PackageLoad, open_package,
};
use spielorders_core::metadata::{IndexDocument, IndexOrder, IndexOrderDirectory, OrderDocument};
use spielorders_core::ports::FileSystemEntryKind;
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
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem.allow_reads(&[
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
    ]);
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
        filesystem
            .effects
            .borrow()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Read(path) => Some(path.as_path()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            Path::new("/package/.spielorders/config.toml"),
            Path::new("/package/.spielorders/version.json"),
            Path::new("/package/.spielorders/index.json"),
        ]
    );
    assert_eq!(
        filesystem
            .effects
            .borrow()
            .iter()
            .filter_map(|effect| match effect {
                Effect::ObserveChange(path) => Some(path.as_path()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            Path::new("/package/.spielorders/config.toml"),
            Path::new("/package/.spielorders/version.json"),
            Path::new("/package/.spielorders/index.json"),
            Path::new("/package/.spielorders/vendors.json"),
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
    assert!(!filesystem.effects.borrow().iter().any(|effect| matches!(
        effect,
        Effect::WriteStage(..) | Effect::Sync(..) | Effect::Replace(..) | Effect::Remove(..)
    )));
    let PackageLoad::Available(package) = result else {
        panic!("a proven-current deterministic index must make the Package available")
    };
    assert_eq!(package.index().state(), PackageIndexState::Current);
    assert_eq!(package.index().refresh(), PackageIndexRefresh::NotNeeded);
    assert!(package.healthy_orders().is_empty());
    assert!(package.vendors().is_empty());
    assert!(package.diagnostic_groups().is_empty());
}

#[test]
fn current_full_index_supplies_complete_order_vendor_paths_and_cached_readme() {
    let filesystem = TestFileSystem::minimal_package();
    let order_directory = "/package/Orders/2026_07_19_Tarnwick_Laser_components";
    let order_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json";
    let readme_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md";
    filesystem.put_entry(order_directory, FileSystemEntryKind::Directory);
    filesystem.put_entry(order_path, FileSystemEntryKind::File);
    filesystem.put_entry(readme_path, FileSystemEntryKind::File);
    let deterministic_index = IndexDocument::from_json_bytes(FULL_INDEX)
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic_index);
    filesystem.allow_reads(&[
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
    ]);
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
    let PackageLoad::Available(package) = result else {
        panic!("the full proven-current index must make the Package available: {result:?}")
    };

    let [order] = package.healthy_orders() else {
        panic!("the indexed healthy Order is projected")
    };
    assert_eq!(order.document().title.as_str(), "Laser components");
    assert_eq!(
        order.paths().directory().package_relative_path(),
        Some("Orders/2026_07_19_Tarnwick_Laser_components")
    );
    assert_eq!(order.readme_text(), Some("User-authored Markdown text"));
    let [vendor] = package.vendors() else {
        panic!("the complete indexed Vendor is projected")
    };
    assert_eq!(vendor.name.as_str(), "Tarnwick");
    assert_eq!(vendor.contacts[0].name.as_str(), "Grace Hopper");
    assert_eq!(
        filesystem
            .effects
            .borrow()
            .iter()
            .filter_map(|effect| match effect {
                Effect::ObserveChange(path) => Some(path.as_path()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            Path::new("/package/.spielorders/config.toml"),
            Path::new("/package/.spielorders/version.json"),
            Path::new("/package/.spielorders/index.json"),
            Path::new("/package/.spielorders/vendors.json"),
            Path::new(order_path),
            Path::new(readme_path),
        ]
    );
    assert!(filesystem.effects.borrow().iter().all(|effect| !matches!(
        effect,
        Effect::ListDirectory(path) if path == Path::new(order_directory)
    )));
}

#[test]
fn current_index_derives_canonical_equivalent_totals_vendor_association_and_reusable_values() {
    let (filesystem, _) = full_index_filesystem();

    let PackageLoad::Available(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("the full proven-current index must make the Package available")
    };

    let [order] = package.healthy_orders() else {
        panic!("one healthy Order is projected")
    };
    assert_eq!(order.item_totals(), &[169.0]);
    assert_eq!(order.order_total(), 169.0);
    assert_eq!(
        package
            .vendor_for_order(0)
            .map(|vendor| vendor.name.as_str()),
        Some("Tarnwick")
    );
    assert_eq!(
        package
            .ordering_institutions()
            .iter()
            .map(|entry| (entry.value(), entry.usage_count()))
            .collect::<Vec<_>>(),
        [("Northwind", 1)]
    );
    assert_eq!(
        package
            .orderers()
            .iter()
            .map(|entry| entry.value())
            .collect::<Vec<_>>(),
        ["Ada Lovelace"]
    );
    assert_eq!(
        package
            .projects()
            .iter()
            .map(|entry| entry.value())
            .collect::<Vec<_>>(),
        ["Telescope"]
    );
    assert_eq!(
        package
            .order_groups()
            .iter()
            .map(|entry| entry.value())
            .collect::<Vec<_>>(),
        ["Summer build"]
    );
}

#[test]
fn newer_vendor_evidence_falls_back_with_the_specific_stale_reason() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    filesystem.put_change_evidence("/package/.spielorders/vendors.json", 1, 21);
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_preserving_supporting_material(&filesystem)
    else {
        panic!("newer Vendors must select the canonical rebuild path")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[spielorders_core::canonical::PackageIndexStaleReason::VendorsChanged]
    );
}

#[test]
fn equal_change_evidence_never_proves_the_index_current() {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
        "/package/.spielorders/index.json",
    ] {
        filesystem.put_change_evidence(path, 1, 20);
    }
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_preserving_supporting_material(&filesystem)
    else {
        panic!("equal evidence must select canonical rebuild")
    };
    assert_eq!(
        failed.index().stale_reasons(),
        &[spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
    );
}

#[test]
fn newer_config_or_version_evidence_uses_the_generic_uncertain_reason() {
    for newer_path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
    ] {
        let filesystem = TestFileSystem::minimal_package();
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
        filesystem.fail_at(ReplacementBoundary::WriteStage);

        let PackageLoad::IndexReplacementFailed(failed) =
            open_preserving_supporting_material(&filesystem)
        else {
            panic!("newer {newer_path} must select canonical rebuild")
        };
        assert_eq!(
            failed.index().stale_reasons(),
            &[spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
        );
    }
}

#[test]
fn newer_order_metadata_and_present_readme_keep_their_specific_stale_reasons() {
    for (newer_path, expected_reason) in [
        (
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
            spielorders_core::canonical::PackageIndexStaleReason::OrderMetadataChanged,
        ),
        (
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
            spielorders_core::canonical::PackageIndexStaleReason::OrderReadmeChanged,
        ),
    ] {
        let filesystem = full_canonical_filesystem();
        filesystem.put_change_evidence(newer_path, 1, 21);
        filesystem.fail_at(ReplacementBoundary::WriteStage);

        let PackageLoad::IndexReplacementFailed(failed) =
            open_preserving_supporting_material(&filesystem)
        else {
            panic!("newer {newer_path} must select canonical rebuild")
        };
        assert_eq!(failed.index().stale_reasons(), &[expected_reason]);
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
    filesystem.fail_at(ReplacementBoundary::WriteStage);

    let PackageLoad::IndexReplacementFailed(failed) =
        open_preserving_supporting_material(&filesystem)
    else {
        panic!("newer canonical inputs must select the canonical rebuild path")
    };

    assert_eq!(
        failed.index().stale_reasons(),
        &[
            spielorders_core::canonical::PackageIndexStaleReason::VendorsChanged,
            spielorders_core::canonical::PackageIndexStaleReason::OrderMetadataChanged,
            spielorders_core::canonical::PackageIndexStaleReason::OrderReadmeChanged,
        ]
    );
}

#[test]
fn add_remove_rename_and_index_path_mismatch_all_reject_membership() {
    let added = TestFileSystem::minimal_package();
    added.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    added.put_entry("/package/Orders/Added", FileSystemEntryKind::Directory);
    added.put_file(
        "/package/Orders/Added/.spielorders/order.json",
        MINIMAL_ORDER,
    );

    let removed = TestFileSystem::minimal_package();
    removed.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    removed.put_file(
        "/package/.spielorders/index.json",
        &IndexDocument::from_json_bytes(FULL_INDEX)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    );

    let renamed = TestFileSystem::minimal_package();
    renamed.put_file("/package/.spielorders/vendors.json", FULL_VENDORS);
    renamed.put_file(
        "/package/.spielorders/index.json",
        &IndexDocument::from_json_bytes(FULL_INDEX)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    );
    renamed.put_entry("/package/Orders/Renamed", FileSystemEntryKind::Directory);
    renamed.put_file(
        "/package/Orders/Renamed/.spielorders/order.json",
        FULL_ORDER,
    );

    let wrong_path = TestFileSystem::minimal_package();
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
                spielorders_core::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            removed,
            vec![
                spielorders_core::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            renamed,
            vec![
                spielorders_core::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
        (
            wrong_path,
            vec![
                spielorders_core::canonical::PackageIndexStaleReason::OrderMembershipChanged,
                spielorders_core::canonical::PackageIndexStaleReason::OrderPathMismatch,
                spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain,
            ],
        ),
    ] {
        filesystem.fail_at(ReplacementBoundary::WriteStage);
        let PackageLoad::IndexReplacementFailed(failed) =
            open_preserving_supporting_material(&filesystem)
        else {
            panic!("membership mismatch must select canonical rebuild")
        };
        assert_eq!(failed.index().stale_reasons(), expected);
    }
}

#[test]
fn unavailable_incomparable_uncertain_and_contradictory_evidence_all_fall_back() {
    let unavailable = minimal_current_filesystem();
    unavailable.clear_change_evidence("/package/.spielorders/vendors.json");

    let incomparable = minimal_current_filesystem();
    incomparable.put_change_evidence("/package/.spielorders/vendors.json", 2, 10);

    let uncertain = minimal_current_filesystem();
    uncertain.clear_change_evidence("/package/.spielorders/index.json");

    let contradictory = minimal_current_filesystem();
    contradictory.override_change_kind(
        "/package/.spielorders/index.json",
        Some(FileSystemEntryKind::Directory),
    );

    for filesystem in [unavailable, incomparable, uncertain, contradictory] {
        filesystem.fail_at(ReplacementBoundary::WriteStage);
        let PackageLoad::IndexReplacementFailed(failed) =
            open_preserving_supporting_material(&filesystem)
        else {
            panic!("non-comparable evidence must select canonical rebuild")
        };
        assert_eq!(
            failed.index().stale_reasons(),
            &[spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
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
        filesystem.fail_change_observation(failed_path);
        filesystem.fail_at(ReplacementBoundary::WriteStage);

        let PackageLoad::IndexReplacementFailed(failed) =
            open_preserving_supporting_material(&filesystem)
        else {
            panic!("failed observation at {failed_path} must select canonical rebuild")
        };
        assert_eq!(
            failed.index().stale_reasons(),
            &[spielorders_core::canonical::PackageIndexStaleReason::ChangeEvidenceUncertain]
        );
        assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
        assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
        assert!(filesystem.effects.borrow().iter().all(|effect| {
            effect
                .paths()
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

    let PackageLoad::Available(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("unexamined content cannot prevent a proven-current fast load")
    };

    assert!(package.diagnostic_groups().is_empty());
    assert!(package.damaged_orders().is_empty());
    assert_eq!(
        package
            .context()
            .package_readme()
            .location()
            .package_relative_path(),
        Some("README.md")
    );
    assert_eq!(
        package.healthy_orders()[0].readme_text(),
        Some("User-authored Markdown text")
    );
    let effects = filesystem.effects.borrow();
    assert!(
        effects
            .iter()
            .all(|effect| effect
                .paths()
                .into_iter()
                .all(|path| path != Path::new(supporting_a)
                    && path != Path::new(supporting_b)
                    && path != Path::new("/package/README.md")
                    && path != Path::new("/package/.spielorders/.vendors.json.spielorders-stage")))
    );
    assert_eq!(filesystem.bytes(supporting_a), Some(SUPPORTING_A.to_vec()));
    assert_eq!(filesystem.bytes(supporting_b), Some(SUPPORTING_B.to_vec()));
}

#[test]
fn fast_projection_is_independent_of_filesystem_enumeration_order() {
    let forward = two_order_current_filesystem(&["Earlier", "Later"]);
    let reverse = two_order_current_filesystem(&["Later", "Earlier"]);

    let PackageLoad::Available(forward_package) = open_preserving_supporting_material(&forward)
    else {
        panic!("forward enumeration must use the current index")
    };
    let PackageLoad::Available(reverse_package) = open_preserving_supporting_material(&reverse)
    else {
        panic!("reverse enumeration must use the current index")
    };

    assert_eq!(forward_package, reverse_package);
    assert_eq!(
        forward_package
            .healthy_orders()
            .iter()
            .map(|order| order.paths().directory().package_relative_path().unwrap())
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
            let filesystem = minimal_current_filesystem();
            filesystem.put_entry(path, kind);
            assert_not_fast_success(open_preserving_supporting_material(&filesystem));
        }
        for path in order_paths {
            let filesystem = full_canonical_filesystem();
            filesystem.put_entry(path, kind);
            assert_not_fast_success(open_preserving_supporting_material(&filesystem));
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
        let filesystem = minimal_current_filesystem();
        filesystem.remove_path(path);
        assert_not_fast_success(open_preserving_supporting_material(&filesystem));
    }
    for path in [
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json",
        "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md",
    ] {
        let filesystem = full_canonical_filesystem();
        filesystem.remove_path(path);
        assert_not_fast_success(open_preserving_supporting_material(&filesystem));
    }
}

#[test]
fn fast_load_sources_only_direct_child_structural_warnings_from_membership_listing() {
    let filesystem = minimal_current_filesystem();
    filesystem.put_entry("/package/Orders/loose.txt", FileSystemEntryKind::File);
    filesystem.put_entry("/package/Orders/.hidden", FileSystemEntryKind::Directory);
    filesystem.allow_reads(&[
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
    ]);

    let PackageLoad::Available(package) = open_preserving_supporting_material(&filesystem) else {
        panic!("direct-child warnings do not prevent a proven-current fast load")
    };

    let [group] = package.diagnostic_groups() else {
        panic!("one Package warning group is projected")
    };
    let [warning] = group.diagnostics() else {
        panic!("one visible direct-child warning is projected")
    };
    assert_eq!(
        warning.code(),
        spielorders_core::readme::DiagnosticCode::OrdersDirectoryEntryDisallowed
    );
    assert_eq!(
        warning.primary_location().package_relative_path(),
        Some("Orders/loose.txt")
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
    assert!(filesystem.effects.borrow().iter().all(|effect| {
        effect.paths().into_iter().all(|path| {
            path != Path::new("/package/Orders/loose.txt")
                && path != Path::new("/package/Orders/.hidden")
        })
    }));
}

fn full_index_filesystem() -> (TestFileSystem, Vec<u8>) {
    let filesystem = TestFileSystem::minimal_package();
    let order_directory = "/package/Orders/2026_07_19_Tarnwick_Laser_components";
    let order_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json";
    let readme_path = "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md";
    filesystem.put_entry(order_directory, FileSystemEntryKind::Directory);
    filesystem.put_entry(order_path, FileSystemEntryKind::File);
    filesystem.put_entry(readme_path, FileSystemEntryKind::File);
    let deterministic_index = IndexDocument::from_json_bytes(FULL_INDEX)
        .unwrap()
        .to_json_bytes()
        .unwrap();
    filesystem.put_file("/package/.spielorders/index.json", &deterministic_index);
    filesystem.allow_reads(&[
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
    ]);
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

fn full_canonical_filesystem() -> TestFileSystem {
    let (filesystem, _) = full_index_filesystem();
    filesystem.allow_all_reads();
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

fn two_order_current_filesystem(enumeration: &[&str]) -> TestFileSystem {
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
    let index = IndexDocument {
        schema_version: spielorders_core::metadata::Version1,
        orders: vec![
            IndexOrder {
                order_directory: IndexOrderDirectory::new("Orders/Later").unwrap(),
                order: OrderDocument::from_json_bytes(later.as_bytes()).unwrap(),
                readme: "later notes".into(),
            },
            IndexOrder {
                order_directory: IndexOrderDirectory::new("Orders/Earlier").unwrap(),
                order: OrderDocument::from_json_bytes(MINIMAL_ORDER).unwrap(),
                readme: "earlier notes".into(),
            },
        ],
        vendors: Vec::new(),
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
    filesystem.enumerate_orders_as(enumeration);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    filesystem.allow_reads(&[
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/index.json",
    ]);
    filesystem
}

fn open_preserving_supporting_material(filesystem: &TestFileSystem) -> PackageLoad {
    filesystem.put_file(SUPPORTING_A_PATH, SUPPORTING_A);
    filesystem.put_file(SUPPORTING_B_PATH, SUPPORTING_B);
    let supporting_a_before = filesystem.bytes(SUPPORTING_A_PATH);
    let supporting_b_before = filesystem.bytes(SUPPORTING_B_PATH);

    let result = open_package(Path::new("/package"), filesystem);

    assert_eq!(filesystem.bytes(SUPPORTING_A_PATH), supporting_a_before);
    assert_eq!(filesystem.bytes(SUPPORTING_B_PATH), supporting_b_before);
    assert!(filesystem.effects.borrow().iter().all(|effect| {
        effect.paths().into_iter().all(|path| {
            path != Path::new(SUPPORTING_A_PATH) && path != Path::new(SUPPORTING_B_PATH)
        })
    }));
    result
}

fn assert_not_fast_success(result: PackageLoad) {
    if let PackageLoad::Available(package) = result {
        assert_ne!(package.index().refresh(), PackageIndexRefresh::NotNeeded);
    }
}
