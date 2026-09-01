//! Decision sources: Decide how Package Index freshness is proven without canonical rereads;
//! Define the complete M2 Package read projection and load-state contract.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::ports::{FileChangeEvidence, FileSystem, FileSystemEntryKind};

#[test]
fn real_observation_is_no_follow_and_reports_comparable_evidence_or_uncertainty() {
    let temporary = TemporaryTree::new("portable-change-observation");
    let ordinary = temporary.path().join("ordinary.json");
    std::fs::write(&ordinary, b"{}\n").unwrap();

    let observed = RealFileSystem.observe_file_change(&ordinary).unwrap();

    assert_eq!(observed.kind(), Some(FileSystemEntryKind::File));
    assert!(matches!(
        observed.evidence(),
        FileChangeEvidence::Comparable { .. } | FileChangeEvidence::Uncertain
    ));

    #[cfg(unix)]
    {
        let link = temporary.path().join("link.json");
        std::os::unix::fs::symlink(&ordinary, &link).unwrap();
        assert_eq!(
            RealFileSystem.observe_file_change(&link).unwrap().kind(),
            Some(FileSystemEntryKind::SymbolicLink)
        );
    }
}
