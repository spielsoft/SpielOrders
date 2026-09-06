//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries; Define legacy Order adoption and metadata
//! bootstrapping; Define the stable M2 diagnostic vocabulary and evidence contract.
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

use crate::support::{ObservingFileSystem, RealFileSystem, adopted_order, initialized_package};
use spielorders_core::adoption::{OrderAdoptionInput, OrderAdoptionPlanError, plan_order_adoption};
use spielorders_core::canonical::{
    CurrentPackageCandidate, ExhaustivePackageValidation, PackageIndexRefresh, PackageIndexState,
    PackageLoad, open_package, validate_package_exhaustively,
};
use spielorders_core::metadata::OrderStatus;
use spielorders_core::mutation::MutationReport;
use spielorders_core::ports::{
    DirectoryEntry, FileChangeObservation, FileSystem, FileSystemEntryKind,
};
use spielorders_core::readme::{
    Diagnostic, DiagnosticCode, DiagnosticEvidence, EvidenceScope, Severity,
};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

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
        diagnostic.code(),
        DiagnosticCode::OrdersDirectoryEntryNameUnusable
    );
    assert_eq!(diagnostic.severity(), Severity::Warning);
    assert_eq!(diagnostic.scope(), EvidenceScope::Package);
    assert_eq!(diagnostic.primary_location().absolute_path(), expected_path);
    // The entry is inside the Package and still has no Package-relative spelling.
    // That absence is the condition being reported, so it is asserted, not tolerated.
    assert_eq!(diagnostic.primary_location().package_relative_path(), None);
    assert!(diagnostic.related_locations().is_empty());
    assert_eq!(
        diagnostic.evidence(),
        &DiagnosticEvidence::OrdersDirectoryEntryNameUnusable {
            observed_kind: expected_kind,
        }
    );
}

/// Every diagnostic a Package projection carries, in its emitted order.
fn diagnostics(package: &CurrentPackageCandidate) -> Vec<&Diagnostic> {
    package
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect()
}

#[test]
fn opening_a_package_reports_a_directory_it_cannot_name_instead_of_failing() {
    let (_tree, root) = initialized_package("unnamable-directory");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir(&unusable).expect("the odd directory is created");

    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("a directory this application cannot name leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::Directory);
    assert_eq!(
        package.diagnostic_groups()[0].scope(),
        EvidenceScope::Package
    );
    assert_eq!(
        package.diagnostic_groups()[0].location().absolute_path(),
        root
    );
    // The entry is not offered as an Order under any projection.
    assert!(package.healthy_orders().is_empty());
    assert!(package.damaged_orders().is_empty());
}

#[test]
fn a_directory_it_cannot_name_is_never_looked_inside() {
    let (_tree, root) = initialized_package("unnamable-opaque");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir_all(unusable.join(".spielorders")).expect("the odd directory is created");
    std::fs::write(unusable.join(".spielorders/order.json"), b"{}").expect("bait is written");
    std::fs::write(unusable.join("README.md"), b"bait").expect("bait is written");

    let filesystem = ObservingFileSystem::default();
    let PackageLoad::Available(package) = open_package(&root, &filesystem) else {
        panic!("a directory this application cannot name leaves the Package usable")
    };

    assert_eq!(diagnostics(&package).len(), 1);
    for path in filesystem.reads().into_iter().chain(filesystem.listings()) {
        assert!(
            !path.starts_with(&unusable),
            "the refused directory was inspected at {path:?}"
        );
    }
}

#[test]
fn an_entry_of_any_kind_whose_name_cannot_be_expressed_is_reported_by_kind() {
    let (_tree, root) = initialized_package("unnamable-file");
    let unusable = root.join("Orders").join(UNUSABLE_FILE);
    std::fs::write(&unusable, b"opaque").expect("the odd file is created");

    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("a file this application cannot name leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    // Not the disallowed-kind warning: that code's evidence is a kind the Orders
    // Directory refuses, and this entry is refused for its name rather than its kind.
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::File);
}

#[test]
fn the_current_index_fast_path_reports_an_entry_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-fast-path");
    adopted_order(
        &root,
        "2026_08_22_Laser",
        "11111111-1111-4111-8111-111111111111",
    );
    assert!(matches!(
        open_package(&root, &RealFileSystem),
        PackageLoad::Available(_)
    ));

    let unusable = root.join("Orders").join(UNUSABLE_FILE);
    std::fs::write(&unusable, b"opaque").expect("the odd file is created");

    let filesystem = ObservingFileSystem::default();
    let PackageLoad::Available(package) = open_package(&root, &filesystem) else {
        panic!("an entry this application cannot name leaves the Package usable")
    };

    // The fast path proves the written index current from change evidence alone. A
    // canonical rebuild would have read the Order's record, so the absence of that
    // read is what identifies which path ran.
    assert!(
        filesystem
            .reads()
            .iter()
            .all(|path| !path.ends_with("order.json")),
        "the canonical path ran instead of the current-index fast path"
    );
    assert_eq!(package.index().state(), PackageIndexState::Current);
    assert_eq!(package.index().refresh(), PackageIndexRefresh::NotNeeded);
    assert_eq!(package.healthy_orders().len(), 1);
    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::File);
}

#[test]
fn exhaustive_validation_reports_an_entry_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-exhaustive");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir(&unusable).expect("the odd directory is created");

    let ExhaustivePackageValidation::Complete(report) =
        validate_package_exhaustively(&root, &RealFileSystem)
    else {
        panic!("exhaustive validation completes over a directory it cannot name")
    };

    let diagnostics = report
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(diagnostics[0], &unusable, FileSystemEntryKind::Directory);
    // The warning never makes the Package invalid.
    assert!(report.is_valid());
}

#[test]
fn adoption_refuses_a_candidate_whose_name_cannot_be_expressed() {
    let (_tree, root) = initialized_package("unnamable-candidate");
    let unusable = root.join("Orders").join(UNUSABLE_DIRECTORY);
    std::fs::create_dir(&unusable).expect("the odd directory is created");

    let error = plan_order_adoption(
        &root,
        &unusable,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect_err("a directory this application cannot name is not adoptable");

    let OrderAdoptionPlanError::NameUnusableCandidate { location } = error else {
        panic!("adoption names the candidate it refused: {error:?}")
    };
    assert_eq!(location.absolute_path(), unusable);
    assert_eq!(
        location.package_relative_path(),
        None,
        "having no name this Package can express is what the refusal reports"
    );
    // The refusal is a plan-time answer: nothing was written into the directory.
    assert!(!unusable.join(".spielorders").exists());
}

/// A `read_directory` that adds one entry whose name is not Unicode.
///
/// APFS refuses such a name outright, so no macOS filesystem can present one. The
/// core's boundary is the port rather than the platform, and a port implementation
/// on a filesystem that permits arbitrary bytes may hand it exactly this.
struct NonUnicodeEntryFileSystem {
    orders_directory: PathBuf,
    name: OsString,
}

impl FileSystem for NonUnicodeEntryFileSystem {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        RealFileSystem.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        RealFileSystem.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        RealFileSystem.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        RealFileSystem.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        RealFileSystem.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        let mut entries = RealFileSystem.read_directory(path)?;
        if path == self.orders_directory {
            entries.push(DirectoryEntry::new(
                self.name.clone(),
                FileSystemEntryKind::Directory,
            ));
        }
        Ok(entries)
    }
}

#[test]
fn an_entry_name_that_is_not_unicode_is_reported_rather_than_indexed() {
    let (_tree, root) = initialized_package("unnamable-non-unicode");
    let name = OsString::from_vec(b"bad\xffname".to_vec());
    let filesystem = NonUnicodeEntryFileSystem {
        orders_directory: root.join("Orders"),
        name: name.clone(),
    };

    let PackageLoad::Available(package) = open_package(&root, &filesystem) else {
        panic!("a name that is not Unicode leaves the Package usable")
    };

    let diagnostics = diagnostics(&package);
    assert_eq!(diagnostics.len(), 1);
    assert_name_unusable(
        diagnostics[0],
        &root.join("Orders").join(&name),
        FileSystemEntryKind::Directory,
    );
    assert!(package.healthy_orders().is_empty());
}

#[test]
fn a_hidden_entry_this_application_cannot_name_stays_undiagnosed() {
    let (_tree, root) = initialized_package("unnamable-hidden");
    std::fs::create_dir(root.join("Orders").join(".hidden\\scratch"))
        .expect("the odd hidden directory is created");

    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
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

    let plan = plan_order_adoption(
        &root,
        &ordinary,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("a colon-bearing name is an ordinary Order Directory name");
    assert!(matches!(
        spielorders_core::adoption::commit_order_adoption(
            plan,
            &mut crate::support::QueuedUuids::of(&["55555555-5555-4555-8555-555555555555"]),
            &RealFileSystem,
        ),
        MutationReport::Committed { .. }
    ));

    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("a colon-bearing Order Directory loads ordinarily")
    };
    assert!(diagnostics(&package).is_empty());
    assert_eq!(
        package.healthy_orders()[0]
            .paths()
            .directory()
            .package_relative_path(),
        Some("Orders/C:notes")
    );
}

#[test]
fn the_new_diagnostic_code_keeps_its_wire_spelling() {
    assert_eq!(
        DiagnosticCode::OrdersDirectoryEntryNameUnusable.as_str(),
        "orders_directory_entry_name_unusable"
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

    let PackageLoad::Available(package) = open_package(&root, &RealFileSystem) else {
        panic!("a healthy Order survives an entry this application cannot name")
    };

    assert_eq!(package.healthy_orders().len(), 1);
    assert_eq!(
        package.healthy_orders()[0]
            .paths()
            .directory()
            .absolute_path(),
        order
    );
    assert_eq!(
        package.healthy_orders()[0]
            .paths()
            .directory()
            .package_relative_path(),
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
        validate_package_exhaustively(&root, &RealFileSystem),
        ExhaustivePackageValidation::OperationFailed(_)
    ));
}

#[test]
fn an_ordinary_sibling_still_adopts_and_commits_beside_one_it_cannot_name() {
    let (_tree, root) = initialized_package("unnamable-sibling");
    std::fs::create_dir(root.join("Orders").join(UNUSABLE_DIRECTORY))
        .expect("the odd directory is created");
    let ordinary = root.join("Orders").join("2026_08_22_Laser");
    std::fs::create_dir(&ordinary).expect("the ordinary directory is created");

    let plan = plan_order_adoption(
        &root,
        &ordinary,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("an ordinary sibling is still adoptable");
    assert_eq!(plan.directory(), ordinary);

    // Committing is the half that matters: projecting the committed Order is
    // where the refused name would have been turned into a Location.
    let report = spielorders_core::adoption::commit_order_adoption(
        plan,
        &mut crate::support::QueuedUuids::of(&["44444444-4444-4444-8444-444444444444"]),
        &RealFileSystem,
    );
    let MutationReport::Committed { result, .. } = report else {
        panic!("the adoption commits beside an entry this application cannot name")
    };
    assert_eq!(
        result.order().paths().directory().package_relative_path(),
        Some("Orders/2026_08_22_Laser")
    );
}
