//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping; Define the stable diagnostic vocabulary and evidence contract.
//!
//! A direct child of the Orders Directory whose filesystem name this application
//! cannot express as a normalized Package-relative path is reported and skipped,
//! rather than crashing the reader that met it. Opening, inspecting, or
//! validating a Package returns an answer about such an entry instead of
//! asserting it cannot exist.
//!
//! The answer differs by where the entry sits. In the Orders Directory it is a
//! Package warning and the Package stays usable. Inside a reserved `.spielorders`
//! directory, which only exhaustive validation lists, it is an operation failure
//! — the same answer a name that is not Unicode already got there.

use crate::Failure;
use crate::tests::support::adopted_directory;
use crate::tests::support::committed_order;
use crate::tests::support::{
    Operation, PlatformFileSystem, QueuedUuids, ScriptedFileSystem, adopted_order, adoption,
    effects, initialized_package, opened, validated_package,
};
use crate::{Assignments, Package, Report};

use crate::canonical::PackageIndexState;

use crate::ports::{DirectoryEntry, FileSystemEntryKind};
use crate::{
    readme::Diagnostic, readme::DiagnosticCode, readme::DiagnosticEvidence, readme::EvidenceScope,
    readme::Severity,
};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

/// A directory name a user may legally create on macOS and Linux, and which the
/// normalized Package-relative grammar refuses because of the separator it reserves.
const UNUSABLE_DIRECTORY: &str = "scratch\\notes";
const UNUSABLE_FILE: &str = "loose\\file.txt";

/// The one warning a Package reports for an entry it cannot name, with every field
/// the diagnostic contract requires of it.
#[track_caller]
fn assert_name_unusable(
    diagnostic: &Diagnostic,
    expected_path: &Path,
    expected_kind: FileSystemEntryKind,
) {
    assert_eq!(
        diagnostic.code,
        DiagnosticCode::OrdersDirectoryEntryNameUnusable
    );
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert_eq!(diagnostic.scope, EvidenceScope::Package);
    assert_eq!(diagnostic.primary_location.absolute_path, expected_path);
    // The entry is inside the Package and still has no Package-relative spelling.
    // That absence is the condition being reported, so it is asserted, not tolerated.
    assert_eq!(
        diagnostic.primary_location.package_relative_path.as_deref(),
        None
    );
    assert!(diagnostic.related_locations.is_empty());
    assert_eq!(
        diagnostic.evidence,
        DiagnosticEvidence::OrdersDirectoryEntryNameUnusable {
            observed_kind: expected_kind,
        }
    );
}

/// Every diagnostic a Package projection carries, in its emitted order.
fn diagnostics(package: &Package) -> Vec<&Diagnostic> {
    package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect()
}

/// The directory holds what would make it an Order, so an entry offered as an Order or
/// diagnosed as a damaged one would show it was looked inside.
#[test]
fn opening_a_package_reports_a_directory_it_cannot_name_and_never_looks_inside() {
    let (_tree, root) = initialized_package("unnamable-directory");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir_all(unusable.join(".spielorders")).expect("the odd directory is created");
    std::fs::write(unusable.join(".spielorders/order.json"), b"{}").expect("bait is written");
    std::fs::write(unusable.join("README.md"), b"bait").expect("bait is written");

    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a directory this application cannot name leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::Directory);
    assert_eq!(
        package.contents.diagnostic_groups[0].scope,
        EvidenceScope::Package
    );
    assert_eq!(
        package.contents.diagnostic_groups[0].location.absolute_path,
        root
    );
    // The entry is not offered as an Order under any projection.
    assert!(package.contents.healthy_orders.is_empty());
    assert!(package.contents.damaged_orders.is_empty());
}

#[test]
fn an_entry_of_any_kind_whose_name_cannot_be_expressed_is_reported_by_kind() {
    let (_tree, root) = initialized_package("unnamable-file");
    let unusable = root.join("Orders").join(UNUSABLE_FILE);
    std::fs::write(&unusable, b"opaque").expect("the odd file is created");

    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a file this application cannot name leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    // Not the disallowed-kind warning: that code's evidence is a kind the Orders
    // Directory refuses, and this entry is refused for its name rather than its kind.
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::File);
}

#[test]
fn a_package_with_a_current_index_still_reports_an_entry_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-index");
    adopted_order(
        &root,
        "2026_08_22_Laser",
        "11111111-1111-4111-8111-111111111111",
    );
    assert!(opened(&root, &PlatformFileSystem).result.is_ok());

    let unusable = root.join("Orders").join(UNUSABLE_FILE);
    std::fs::write(&unusable, b"opaque").expect("the odd file is created");

    let filesystem = ScriptedFileSystem::new();
    let Ok(package) = opened(&root, &filesystem).result else {
        panic!("an entry this application cannot name leaves the Package usable")
    };

    assert_eq!(package.contents.index.state, PackageIndexState::Current);
    assert_eq!(package.contents.healthy_orders.len(), 1);
    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::File);
}

#[test]
fn exhaustive_validation_reports_an_entry_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-exhaustive");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir(&unusable).expect("the odd directory is created");

    // The warning never makes the Package invalid.
    let Ok(report) = validated_package(&root, &PlatformFileSystem).result else {
        panic!("exhaustive validation completes over a directory it cannot name, valid")
    };

    let diagnostics = report
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::Directory);
}

#[test]
fn adoption_refuses_a_candidate_whose_name_cannot_be_expressed() {
    let (_tree, root) = initialized_package("unnamable-candidate");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir(&unusable).expect("the odd directory is created");

    let error = crate::plan(
        &adoption(
            root.clone(),
            unusable.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a directory this application cannot name is not adoptable");

    let Failure::NameUnusableCandidate { location } = error else {
        panic!("adoption names the candidate it refused: {error:?}")
    };
    assert_eq!(location.absolute_path, unusable);
    assert_eq!(
        location.package_relative_path.as_deref(),
        None,
        "having no name this Package can express is what the refusal reports"
    );
    // The refusal is a plan-time answer: nothing was written into the directory.
    assert!(!unusable.join(".spielorders").exists());
}

#[test]
fn an_entry_name_that_is_not_unicode_is_reported_rather_than_indexed() {
    let (_tree, root) = initialized_package("unnamable-non-unicode");
    let name = OsString::from_vec(b"bad\xffname".to_vec());
    // APFS refuses such a name outright, so the listing reports it. The core's
    // boundary is the port, and a port over a filesystem that permits arbitrary bytes
    // may hand it exactly this.
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::ReadDirectory)
        .at(root.join("Orders"))
        .lists_also(vec![DirectoryEntry {
            name: name.clone(),
            kind: FileSystemEntryKind::Directory,
        }]);

    let Ok(package) = opened(&root, &filesystem).result else {
        panic!("a name that is not Unicode leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(
        diagnostics[0],
        &root.join("Orders").join(&name),
        FileSystemEntryKind::Directory,
    );
    assert!(package.contents.healthy_orders.is_empty());
}

#[test]
fn a_hidden_entry_this_application_cannot_name_stays_undiagnosed() {
    let (_tree, root) = initialized_package("unnamable-hidden");
    std::fs::create_dir(root.join("Orders").join(".hidden\\scratch"))
        .expect("the odd hidden directory is created");

    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a hidden entry leaves the Package usable")
    };

    // Hidden direct children of the Orders Directory produce no diagnostic, whatever
    // they are called.
    assert!(diagnostics(&package).is_empty());
}

#[test]
fn a_drive_letter_shaped_name_is_an_ordinary_order_directory() {
    // The Package-relative grammar refuses a leading drive letter, and this name
    // has one — but only as the last segment, where the grammar allows it. The
    // question asked of a direct child is whether the Package-relative path it
    // would actually occupy is expressible, not whether its bare name would be.
    // A name-shaped rule would refuse this directory and take the user's Order
    // with it.
    let (_tree, root) = initialized_package("drive-letter-shaped");
    let ordinary = root.join("Orders").join("C:notes");
    std::fs::create_dir(&ordinary).expect("the colon-bearing directory is created");

    let plan = crate::plan(
        &adoption(
            root.clone(),
            ordinary.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a colon-bearing name is an ordinary Order Directory name");
    assert!(matches!(
        crate::commit(
            plan,
            &effects(&PlatformFileSystem)
                .minting(QueuedUuids::of(&["55555555-5555-4555-8555-555555555555"]))
        )
        .result,
        Ok(Report::Committed { .. })
    ));

    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a colon-bearing Order Directory loads ordinarily")
    };
    assert!(diagnostics(&package).is_empty());
    assert_eq!(
        package.contents.healthy_orders[0]
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/C:notes")
    );
}

#[test]
fn an_order_beside_an_entry_it_cannot_name_remains_ordinarily_usable() {
    let (_tree, root) = initialized_package("unnamable-neighbour");
    let order = adopted_order(
        &root,
        "2026_08_22_Laser",
        "22222222-2222-4222-8222-222222222222",
    );
    std::fs::create_dir(root.join("Orders").join(UNUSABLE_DIRECTORY))
        .expect("the odd directory is created");

    let Ok(package) = opened(&root, &PlatformFileSystem).result else {
        panic!("a healthy Order survives an entry this application cannot name")
    };

    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(
        package.contents.healthy_orders[0]
            .paths
            .directory
            .absolute_path,
        order
    );
    assert_eq!(
        package.contents.healthy_orders[0]
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/2026_08_22_Laser")
    );
}

#[test]
fn reserved_content_this_application_cannot_name_fails_the_validation_it_stops() {
    let (_tree, root) = initialized_package("unnamable-reserved");
    let order = adopted_order(
        &root,
        "2026_08_22_Laser",
        "33333333-3333-4333-8333-333333333333",
    );
    std::fs::write(order.join(".spielorders").join(UNUSABLE_FILE), b"opaque")
        .expect("the odd reserved entry is created");

    // Exhaustive validation is the only reader that lists a reserved directory. An
    // entry it cannot name there is treated exactly as one whose name is not
    // Unicode already is: the listing cannot be reported, so it fails rather than
    // fabricating a Package-relative path for it.
    assert!(matches!(
        validated_package(&root, &PlatformFileSystem).result,
        Err(Failure::PackageOperationFailed(_))
    ));
}

#[test]
fn an_ordinary_sibling_still_adopts_and_commits_beside_one_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-sibling");
    std::fs::create_dir(root.join("Orders").join(UNUSABLE_DIRECTORY))
        .expect("the odd directory is created");
    let ordinary = root.join("Orders").join("2026_08_22_Laser");
    std::fs::create_dir(&ordinary).expect("the ordinary directory is created");

    let plan = crate::plan(
        &adoption(
            root.clone(),
            ordinary.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an ordinary sibling is still adoptable");
    assert_eq!(adopted_directory(&plan), ordinary);

    // Committing is the half that matters: projecting the committed Order is
    // where the refused name would have been turned into a Location.
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem)
            .minting(QueuedUuids::of(&["44444444-4444-4444-8444-444444444444"])),
    )
    .result;
    let Ok(Report::Committed { result, .. }) = report else {
        panic!("the adoption commits beside an entry this application cannot name")
    };
    assert_eq!(
        committed_order(&result)
            .paths
            .directory
            .package_relative_path
            .as_deref(),
        Some("Orders/2026_08_22_Laser")
    );
}
