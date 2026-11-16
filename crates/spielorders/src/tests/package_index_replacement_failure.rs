//! Decision sources: Define durable metadata schemas and version evolution;
//! Define the stable diagnostic vocabulary and evidence contract; Define the complete
//! Package read projection and load-state contract.
//! A deliberate Package Index rebuild whose replacement fails: evidence and effect budget.

use crate::commit;
use crate::ports::FileSystemEntryKind;
use crate::tests::support::assert_reported_beside;
use crate::tests::support::{
    FailedCommit, Operation, effects, minimal_package, planned, rebuild_index,
};
use std::{collections::BTreeSet, path::Path};

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
    for (failing, stage_remains) in [
        (&[Operation::WriteNew][..], false),
        (&[Operation::SyncFile], false),
        (&[Operation::ReplaceFile], false),
        (&[Operation::SyncFile, Operation::RemoveFile], true),
    ] {
        let filesystem = minimal_package();
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
        for operation in failing {
            filesystem.on(*operation).fails();
        }

        let plan = planned(&rebuild_index("/package"), &effects(&filesystem));
        let Err(failure) = commit(plan, &effects(&filesystem)).result else {
            panic!("{failing:?} refuses the rebuild")
        };

        let index = failure.failure();
        assert_reported_beside(&failure);
        assert_eq!(
            index.location.package_relative_path.as_deref(),
            Some(".spielorders/index.json")
        );
        assert_eq!(
            index.location.absolute_path,
            Path::new("/package/.spielorders/index.json")
        );
        let left = failure
            .cleanup_failures()
            .iter()
            .map(|cleanup| cleanup.location.package_relative_path.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            left,
            if stage_remains {
                vec![Some(".spielorders/.index.json.spielorders-stage")]
            } else {
                Vec::new()
            },
            "the stage is named exactly where it remains: {failing:?}"
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

        assert_eq!(
            filesystem.paths(Operation::ReadDirectory),
            [Path::new("/package/Orders")],
            "{failing:?} may list only the Orders Directory"
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
        let calls = filesystem.calls();
        assert!(
            calls
                .iter()
                .all(|call| (allowed_paths.contains(call.path.as_path())
                    || (call.operation == Operation::SymlinkMetadata
                        && call.path == Path::new("/package/Orders/Order/.spielorders")))
                    && call
                        .source
                        .as_deref()
                        .is_none_or(|source| allowed_paths.contains(source))),
            "{failing:?} exceeded the exact canonical/index effect budget: {calls:?}"
        );
    }
}
