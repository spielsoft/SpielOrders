//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the stable M2 diagnostic vocabulary and evidence contract; Define the complete M2
//! Package read projection and load-state contract.
//! Package Index replacement-failure evidence and effect-budget integration tests.

mod support;

use spielorders_core::canonical::{
    PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState, PackageLoad, open_package,
};
use spielorders_core::ports::FileSystemEntryKind;
use spielorders_core::readme::{OperationEvidence, PackageIndexReplacementPhase};
use std::{collections::BTreeSet, path::Path};
use support::package_index::{Effect, ReplacementBoundary, TestFileSystem};

const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
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

#[test]
fn every_replacement_and_cleanup_failure_preserves_complete_evidence_and_effect_budget() {
    for (boundary, phase, cleanup_failure) in [
        (
            ReplacementBoundary::WriteStage,
            PackageIndexReplacementPhase::WriteStage,
            None,
        ),
        (
            ReplacementBoundary::ReadStage,
            PackageIndexReplacementPhase::ReadStage,
            None,
        ),
        (
            ReplacementBoundary::CandidateValidation,
            PackageIndexReplacementPhase::CandidateValidation,
            None,
        ),
        (
            ReplacementBoundary::SyncStage,
            PackageIndexReplacementPhase::SyncStage,
            None,
        ),
        (
            ReplacementBoundary::Replace,
            PackageIndexReplacementPhase::Replace,
            None,
        ),
        (
            ReplacementBoundary::RemoveStage,
            PackageIndexReplacementPhase::CandidateValidation,
            Some(PackageIndexReplacementPhase::RemoveStage),
        ),
    ] {
        let filesystem = TestFileSystem::minimal_package();
        filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
        filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
        filesystem.put_file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
        filesystem.put_file("/package/Orders/Order/supporting-a.bin", SUPPORTING_A);
        filesystem.put_file(
            "/package/Orders/Order/nested/supporting-b.bin",
            SUPPORTING_B,
        );
        filesystem.fail_at(boundary);

        let PackageLoad::IndexReplacementFailed(failed) =
            open_package(Path::new("/package"), &filesystem)
        else {
            panic!("{boundary:?} must block ordinary Package use")
        };

        assert_eq!(failed.index().state(), PackageIndexState::Stale);
        assert_eq!(
            failed.index().stale_reasons(),
            &[
                PackageIndexStaleReason::OrderMembershipChanged,
                PackageIndexStaleReason::ChangeEvidenceUncertain,
            ]
        );
        assert_eq!(
            failed.index().refresh(),
            PackageIndexRefresh::ReplacementFailed
        );
        assert_eq!(failed.old_index_bytes(), Some(MINIMAL_INDEX));
        assert_eq!(failed.canonical_evidence().healthy_orders().len(), 1);
        assert!(failed.canonical_evidence().damaged_orders().is_empty());
        assert!(failed.canonical_evidence().diagnostic_groups().is_empty());
        assert_eq!(
            failed.failure().evidence(),
            &OperationEvidence::PackageIndexReplacement {
                phase,
                cleanup_failure,
            }
        );
        assert_eq!(
            failed.failure().primary_location().package_relative_path(),
            Some(".spielorders/index.json")
        );
        assert_eq!(
            failed.failure().primary_location().absolute_path(),
            Path::new("/package/.spielorders/index.json")
        );
        assert_eq!(
            failed.failure().related_locations()[0].package_relative_path(),
            Some(".spielorders/.index.json.spielorders-stage")
        );
        assert_eq!(
            failed.failure().related_locations()[0].absolute_path(),
            Path::new("/package/.spielorders/.index.json.spielorders-stage")
        );
        assert_eq!(
            filesystem.bytes("/package/.spielorders/index.json"),
            Some(MINIMAL_INDEX.to_vec())
        );
        assert_eq!(
            filesystem.bytes("/package/Orders/Order/supporting-a.bin"),
            Some(SUPPORTING_A.to_vec())
        );
        assert_eq!(
            filesystem.bytes("/package/Orders/Order/nested/supporting-b.bin"),
            Some(SUPPORTING_B.to_vec())
        );

        let effects = filesystem.effects.borrow();
        assert_eq!(
            effects
                .iter()
                .filter_map(|effect| match effect {
                    Effect::ListDirectory(path) => Some(path.as_path()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [Path::new("/package/Orders")],
            "{boundary:?} may list only the Orders Directory"
        );
        let allowed_paths = [
            "/package",
            "/package/.spielorders",
            "/package/.spielorders/config.toml",
            "/package/.spielorders/version.json",
            "/package/.spielorders/vendors.json",
            "/package/.spielorders/index.json",
            "/package/.spielorders/.index.json.spielorders-stage",
            "/package/Orders",
            "/package/Removed",
            "/package/Orders/Order",
            "/package/Orders/Order/.spielorders/order.json",
            "/package/Orders/Order/README.md",
        ]
        .into_iter()
        .map(Path::new)
        .collect::<BTreeSet<_>>();
        assert!(
            effects.iter().all(|effect| effect
                .paths()
                .into_iter()
                .all(|path| allowed_paths.contains(path))),
            "{boundary:?} exceeded the exact canonical/index effect budget: {effects:?}"
        );
    }
}
