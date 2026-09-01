//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::adoption::{OrderAdoptionInput, commit_order_adoption, plan_order_adoption};
use spielorders_core::alignment::{commit_order_alignment, plan_order_alignment};
use spielorders_core::canonical::{
    CurrentPackageCandidate, PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState,
    PackageLoad, open_package,
};
use spielorders_core::initialization::{
    InitializationOptions, commit_package_initialization, plan_package_initialization,
};
use spielorders_core::metadata::{IndexDocument, OrderStatus};
use spielorders_core::mutation::MutationReport;
use spielorders_core::order_edits::{OrderAssignments, commit_order_edit, plan_order_edit};
use spielorders_core::ports::{
    DirectoryEntry, DirectoryFileSystem, FileChangeObservation, FileSystem, FileSystemEntryKind,
    LocalCalendar, LocalDate, NativeTrash, NewDirectoryRenameOutcome, NewFileLinkOutcome,
    UuidGenerator,
};
use spielorders_core::readme::DiagnosticCode;
use spielorders_core::removal::{RemovalDisposition, commit_order_removal, plan_order_removal};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
/// Deliberately not empty, and not what an empty README would produce, so a
/// projection that collapsed the two states is visible in the assertion.
const README: &[u8] = b"# Laser components\n\nExactly the text the user wrote.\n";

struct FixedUuid(String);

impl UuidGenerator for FixedUuid {
    fn generate_v4(&mut self) -> String {
        self.0.clone()
    }
}

struct UnreachableTrash;

impl NativeTrash for UnreachableTrash {
    type Error = std::io::Error;

    fn submit_directory(&self, path: &Path) -> Result<Option<PathBuf>, Self::Error> {
        panic!("a recoverable removal never reaches the platform Trash: {path:?}");
    }
}

struct FixedCalendar;

impl LocalCalendar for FixedCalendar {
    fn today(&self) -> LocalDate {
        LocalDate {
            year: 2026,
            month: 9,
            day: 1,
        }
    }
}

/// Fails only the read of an Order README, leaving every other effect real. This is
/// the shape `tests/order_diagnosis.rs` established for refusing a read at one path,
/// pointed at the document this ticket owns.
///
/// Two variants additionally deny a mutation the refreshed Package it would rather
/// report, because that is the only way to reach the projection each capability
/// builds for itself — the site this ticket names. When the refresh succeeds, a
/// mutation reports the Order the refreshed load returned, and the local projection
/// is never consulted; a test that only ever takes that route would pass with the
/// local site's defect back in place, which the injection pass demonstrated.
///
/// `vanish_published_record` removes the Order record the moment it is linked into
/// place, the way `tests/order_adoption.rs` models an outside change between an
/// adoption's commit and its refresh. `fail_refresh_listing` fails the directory
/// listing once the mutation's decisive rename has landed, the shape
/// `tests/order_fact_edits.rs` and `tests/order_alignment.rs` both established.
#[derive(Default)]
struct UnreadableReadme {
    vanish_published_record: bool,
    fail_refresh_listing: bool,
    published: std::cell::Cell<bool>,
    mutated: std::cell::Cell<bool>,
}

impl UnreadableReadme {
    fn vanishing_after_adopt() -> Self {
        Self {
            vanish_published_record: true,
            ..Self::default()
        }
    }

    fn failing_the_refresh_load() -> Self {
        Self {
            fail_refresh_listing: true,
            ..Self::default()
        }
    }
}

impl FileSystem for UnreadableReadme {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        if path.ends_with("README.md") {
            return Err(std::io::Error::other("injected unreadable README"));
        }
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
        RealFileSystem.replace_file(source, destination)?;
        if destination.ends_with(".spielorders/order.json") {
            self.mutated.set(true);
        }
        Ok(())
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
        // Only the post-mutation refresh load is failed, never the planning reads.
        if self.fail_refresh_listing && self.mutated.get() {
            return Err(std::io::Error::other("injected refresh listing failure"));
        }
        RealFileSystem.read_directory(path)
    }
}

impl DirectoryFileSystem for UnreadableReadme {
    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        RealFileSystem.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        let outcome = RealFileSystem.link_new_file(source, destination)?;
        if self.vanish_published_record
            && outcome == NewFileLinkOutcome::Linked
            && destination.ends_with(".spielorders/order.json")
            && !self.published.replace(true)
        {
            let _ = fs::remove_file(destination);
        }
        Ok(outcome)
    }

    fn rename_new_directory(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewDirectoryRenameOutcome, Self::Error> {
        let outcome = RealFileSystem.rename_new_directory(source, destination)?;
        if outcome == NewDirectoryRenameOutcome::Renamed {
            self.mutated.set(true);
        }
        Ok(outcome)
    }
}

/// A Package holding one adopted Order, with the derived index removed so the next
/// load must read the README rather than serve a cached copy of its text.
fn package_with_unindexed_order(label: &str, readme: Option<&[u8]>) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::default(),
        &RealFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = fs::canonicalize(tree.path()).expect("the test Package root resolves");
    let directory = root.join("Orders").join("2026_08_22_Laser_components");
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    if let Some(readme) = readme {
        fs::write(directory.join("README.md"), readme).expect("the fixture README is written");
    }
    let plan = plan_order_adoption(
        &root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the fixture directory can be adopted");
    assert!(matches!(
        commit_order_adoption(plan, &mut FixedUuid(ORDER_ID.into()), &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    fs::remove_file(root.join(".spielorders/index.json")).expect("the derived index is removed");
    (tree, root)
}

fn order_directory(root: &Path) -> PathBuf {
    root.join("Orders").join("2026_08_22_Laser_components")
}

fn diagnostic_codes(package: &CurrentPackageCandidate) -> Vec<DiagnosticCode> {
    package
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn available(load: PackageLoad) -> CurrentPackageCandidate {
    match load {
        PackageLoad::Available(package) => package,
        other => panic!("a README problem never damages the Package: {other:?}"),
    }
}

#[test]
fn a_readme_that_cannot_be_read_is_projected_as_unknown_rather_than_empty() {
    let (_tree, root) = package_with_unindexed_order("readme-unknown", Some(README));

    let package = available(open_package(&root, &UnreadableReadme::default()));

    let [order] = package.healthy_orders() else {
        panic!("an unreadable README leaves one healthy Order")
    };
    assert_eq!(
        order.readme_text(),
        None,
        "an Order README that exists but could not be read carries no text"
    );
    assert_eq!(
        diagnostic_codes(&package),
        vec![DiagnosticCode::ReadmeUnavailable],
        "the load still reports the README diagnostic it has always reported"
    );
    assert_eq!(
        fs::read(order_directory(&root).join("README.md")).unwrap(),
        README,
        "reading an Order never writes its README"
    );
}

#[test]
fn an_absent_readme_stays_distinguishable_from_one_that_could_not_be_read() {
    let (_tree, root) = package_with_unindexed_order("readme-absent", None);

    let package = available(open_package(&root, &RealFileSystem));

    let [order] = package.healthy_orders() else {
        panic!("an absent README leaves one healthy Order")
    };
    assert_eq!(
        order.readme_text(),
        Some(""),
        "an absent Order README reads as an empty one, which is a text this load knows"
    );
    assert!(
        diagnostic_codes(&package).is_empty(),
        "an absent README is ordinary and carries no diagnostic"
    );
}

#[test]
fn an_unreadable_readme_is_not_recorded_in_the_index_as_an_empty_one() {
    let (_tree, root) = package_with_unindexed_order("readme-index", Some(README));
    let index_path = root.join(".spielorders/index.json");

    let package = available(open_package(&root, &UnreadableReadme::default()));

    assert!(
        !index_path.exists(),
        "a load that could not read an Order README writes no index claiming its text"
    );
    assert_eq!(
        package.index().state(),
        PackageIndexState::Missing,
        "the index the load declined to write is reported as it was found"
    );
    assert_eq!(
        package.index().refresh(),
        PackageIndexRefresh::NotAttempted,
        "declining to record an unknown README text is a refresh not attempted"
    );
}

#[test]
fn a_stale_stored_index_is_left_intact_rather_than_overwritten_with_an_empty_readme() {
    let (_tree, root) = package_with_unindexed_order("readme-index-intact", Some(README));
    let index_path = root.join(".spielorders/index.json");
    let stored = {
        let package = available(open_package(&root, &RealFileSystem));
        assert_eq!(
            package.healthy_orders()[0].readme_text(),
            Some(std::str::from_utf8(README).unwrap())
        );
        fs::read(&index_path).expect("a readable README produces an index")
    };
    // The README is rewritten after the index, so its change evidence is no longer
    // covered by the index's and the load must read it rather than trust the cache.
    fs::write(
        root.join("Orders/2026_08_22_Laser_components/README.md"),
        README,
    )
    .unwrap();

    let package = available(open_package(&root, &UnreadableReadme::default()));

    assert_eq!(
        fs::read(&index_path).unwrap(),
        stored,
        "the index a readable load wrote is left exactly as it was"
    );
    assert_eq!(
        package.index().state(),
        PackageIndexState::Stale,
        "an index this load could not refresh is reported stale rather than current"
    );
    assert_eq!(
        package.index().refresh(),
        PackageIndexRefresh::NotAttempted,
        "declining to record an unknown README text is a refresh not attempted"
    );
    // Here the fast probe saw the README's change evidence overtake the index's, so
    // `order_readme_changed` is an observation this load really made. The test below
    // covers the case where it is not.
    assert_eq!(
        package.index().stale_reasons(),
        [PackageIndexStaleReason::OrderReadmeChanged],
        "the reported reason is the observation the load actually made"
    );
    assert_eq!(
        package.healthy_orders()[0].readme_text(),
        None,
        "the load reports what it could read, not what the stale cache still holds"
    );
    let index = IndexDocument::from_json_bytes(&stored).expect("the stored index parses");
    assert_eq!(
        index.orders[0].readme,
        std::str::from_utf8(README).unwrap(),
        "the text a load actually read stays in the index"
    );
}

#[test]
fn an_unreadable_readme_is_never_itself_the_evidence_that_the_stored_text_changed() {
    let (_tree, root) = package_with_unindexed_order("readme-stale-reason", Some(README));
    let stored = {
        let package = available(open_package(&root, &RealFileSystem));
        assert_eq!(
            package.healthy_orders()[0].readme_text(),
            Some(std::str::from_utf8(README).unwrap()),
            "the index this load writes holds the text it read"
        );
        fs::read(root.join(".spielorders/index.json")).expect("a readable load writes an index")
    };
    // The record, not the README, is what changed after the index was written. The
    // README's own bytes are untouched and its change evidence still sits behind the
    // index's, so nothing this load observes says its text differs from the stored one.
    let record = order_directory(&root).join(".spielorders/order.json");
    fs::write(&record, fs::read(&record).unwrap()).unwrap();

    let package = available(open_package(&root, &UnreadableReadme::default()));

    assert_eq!(
        package.index().stale_reasons(),
        [PackageIndexStaleReason::OrderMetadataChanged],
        "a README this load could not read adds no reason of its own — reporting \
         order_readme_changed here would name a comparison the load never made"
    );
    assert_eq!(package.healthy_orders()[0].readme_text(), None);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        stored,
        "and the index is still not rewritten"
    );
}

#[test]
fn an_adoption_that_cannot_confirm_its_order_reports_an_unreadable_readme_as_unknown() {
    let tree = TemporaryTree::new("readme-adopt");
    let plan = plan_package_initialization(
        tree.path(),
        &InitializationOptions::default(),
        &RealFileSystem,
    )
    .expect("the test Package can be planned");
    assert!(matches!(
        commit_package_initialization(plan, &RealFileSystem),
        MutationReport::Committed { .. }
    ));
    let root = fs::canonicalize(tree.path()).expect("the test Package root resolves");
    let directory = root.join("Orders").join("Legacy_laser_purchase");
    fs::create_dir_all(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), README).expect("the fixture README is written");
    let plan = plan_order_adoption(
        &root,
        &directory,
        &OrderAdoptionInput::new("Laser components", "2026-08-22", OrderStatus::Hold),
        &RealFileSystem,
    )
    .expect("the directory can be adopted");

    let report = commit_order_adoption(
        plan,
        &mut FixedUuid(ORDER_ID.into()),
        &UnreadableReadme::vanishing_after_adopt(),
    );

    let MutationReport::CommittedWithWarning { result, .. } = report else {
        panic!("an adoption whose refresh cannot confirm it is still committed: {report:?}")
    };
    assert_eq!(result.order().document().order_id.as_str(), ORDER_ID);
    assert_eq!(
        result.order().readme_text(),
        None,
        "adoption's own projection reports the README it could not read as unknown, \
         not as empty"
    );
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        README,
        "adoption never writes a README the directory already held"
    );
}

#[test]
fn once_the_readme_reads_again_every_later_load_reports_its_text() {
    let (_tree, root) = package_with_unindexed_order("readme-recovers", Some(README));

    let unreadable = available(open_package(&root, &UnreadableReadme::default()));
    assert_eq!(unreadable.healthy_orders()[0].readme_text(), None);

    let recovered = available(open_package(&root, &RealFileSystem));

    assert_eq!(
        recovered.healthy_orders()[0].readme_text(),
        Some(std::str::from_utf8(README).unwrap()),
        "once the README reads again, the load reports its text"
    );
    let again = available(open_package(&root, &RealFileSystem));
    assert_eq!(
        again.healthy_orders()[0].readme_text(),
        Some(std::str::from_utf8(README).unwrap()),
        "a load that proves the index current serves the text that index actually holds"
    );
    assert_eq!(again.index().state(), PackageIndexState::Current);
}

#[test]
fn an_order_edit_reports_a_readme_it_could_not_read_as_unknown() {
    let (_tree, root) = package_with_unindexed_order("readme-edit", Some(README));
    let directory = order_directory(&root);

    let plan = plan_order_edit(
        &directory,
        &OrderAssignments::new().assign("title", "Laser mounts"),
        &FixedCalendar,
        &UnreadableReadme::default(),
    )
    .expect("the edit can be planned");
    // The record is replaced and the Package then fails to reload, so the report is
    // the one the edit projects for itself. That is the projection this ticket names,
    // and a refreshed load would supply its own Order instead of consulting it.
    let report = commit_order_edit(plan, &UnreadableReadme::failing_the_refresh_load());

    let (MutationReport::Committed { result, .. }
    | MutationReport::CommittedWithWarning { result, .. }) = report
    else {
        panic!("an unreadable README never blocks an Order edit: {report:?}")
    };
    assert_eq!(result.order().document().title.as_str(), "Laser mounts");
    assert_eq!(
        result.order().readme_text(),
        None,
        "an edit reports the README it could not read as unknown, not as empty"
    );
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        README,
        "an Order edit never writes the README it reports"
    );
}

#[test]
fn an_alignment_reports_a_readme_it_could_not_read_as_unknown() {
    let (_tree, root) = package_with_unindexed_order("readme-align", Some(README));
    let directory = order_directory(&root);
    // Drift the directory name so the alignment has a rename to make.
    let drifted = root.join("Orders").join("Laser_components_drifted");
    fs::rename(&directory, &drifted).expect("the fixture Order Directory drifts");

    let plan =
        plan_order_alignment(&drifted, &UnreadableReadme::default()).expect("the alignment plans");
    // The rename succeeds and the Package then fails to reload, so the report is the
    // one the alignment projects for itself from the plan it carried.
    let report = commit_order_alignment(plan, &UnreadableReadme::failing_the_refresh_load());

    let (MutationReport::Committed { result, .. }
    | MutationReport::CommittedWithWarning { result, .. }) = report
    else {
        panic!("an unreadable README never blocks an alignment: {report:?}")
    };
    assert_eq!(
        result.order().readme_text(),
        None,
        "an alignment reports the README it could not read as unknown, not as empty"
    );
    assert_eq!(
        fs::read(
            result
                .order()
                .paths()
                .directory()
                .absolute_path()
                .join("README.md")
        )
        .unwrap(),
        README,
        "an alignment moves the README with its directory and never writes it"
    );
}

#[test]
fn a_removal_reports_a_readme_it_could_not_read_as_unknown() {
    let (_tree, root) = package_with_unindexed_order("readme-remove", Some(README));
    let directory = order_directory(&root);

    let plan = plan_order_removal(
        &directory,
        RemovalDisposition::RecoverableDirectory,
        &UnreadableReadme::default(),
    )
    .expect("the removal plans");
    let report = commit_order_removal(plan, &UnreadableReadme::default(), &UnreachableTrash);

    let (MutationReport::Committed { result, .. }
    | MutationReport::CommittedWithWarning { result, .. }) = report
    else {
        panic!("an unreadable README never blocks a removal: {report:?}")
    };
    assert_eq!(
        result.order().readme_text(),
        None,
        "a removal reports the README it could not read as unknown, not as empty"
    );
    let moved = result
        .final_location()
        .expect("a recoverable removal names where the Order went");
    assert_eq!(
        fs::read(moved.join("README.md")).unwrap(),
        README,
        "a removal moves the README with its directory and never writes it"
    );
}
