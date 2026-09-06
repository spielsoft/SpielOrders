//! The production effects this executable hands to the core.
//!
//! `docs/architecture/durable-writes.md` states that each shipped adapter owes the
//! `create_directory` no-adopt and `rename_new_directory` no-replace guarantees that
//! `spielorders-core`'s own suite holds its test double to. These are that debt paid
//! for the adapter the `spielorders` executable ships.
//!
//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries; Define the Rust CLI capability contract.

use crate::support::{SUPPORTING_A, SUPPORTING_B, TemporaryTree, entry_names, managed_snapshot};
use spielorders_cli::effects::{PlatformFileSystem, PlatformTrash};
use spielorders_core::ports::{
    DirectoryFileSystem, FileChangeEvidence, FileSystem, FileSystemEntryKind, NativeTrash,
    NewDirectoryRenameOutcome, NewFileLinkOutcome,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::{fs, io, path::Path, path::PathBuf, thread};

/// A complete Order Directory waiting to be published, exactly as creation stages one.
fn staged_order(parent: &Path, label: &str) -> PathBuf {
    let directory = parent.join(format!("staging-{label}"));
    fs::create_dir(&directory).expect("the staging directory is created");
    fs::create_dir(directory.join(".spielorders")).expect("its metadata directory is created");
    fs::write(directory.join(".spielorders/order.json"), b"{}").expect("its record is staged");
    fs::write(directory.join("README.md"), b"").expect("its README is staged");
    directory
}

#[test]
fn managed_bytes_survive_a_write_a_read_and_a_durable_replacement() {
    let tree = TemporaryTree::new("effects-managed-bytes");
    let destination = tree.path().join("vendors.json");
    let stage = tree.path().join("vendors.json.stage");

    PlatformFileSystem
        .write(&destination, b"first")
        .expect("a managed file is written");
    assert_eq!(PlatformFileSystem.read(&destination).unwrap(), b"first");

    PlatformFileSystem
        .write_new(&stage, b"second")
        .expect("a fresh stage is created");
    PlatformFileSystem
        .sync_file(&stage)
        .expect("the stage is made durable");
    PlatformFileSystem
        .replace_file(&stage, &destination)
        .expect("the stage replaces the destination");

    assert_eq!(PlatformFileSystem.read(&destination).unwrap(), b"second");
    assert_eq!(
        PlatformFileSystem.symlink_metadata(&stage).unwrap(),
        None,
        "the replacement consumed the staged name"
    );

    PlatformFileSystem
        .remove_file(&destination)
        .expect("the managed file is removed");
    assert_eq!(
        PlatformFileSystem.symlink_metadata(&destination).unwrap(),
        None
    );
}

/// A pre-existing stage is evidence of an interrupted write that repair may promote,
/// so creating one must refuse rather than overwrite it.
#[test]
fn creating_a_new_file_refuses_to_overwrite_existing_staging_evidence() {
    let tree = TemporaryTree::new("effects-write-new");
    let stage = tree.path().join("index.json.stage");
    PlatformFileSystem
        .write_new(&stage, b"interrupted")
        .unwrap();

    assert!(
        PlatformFileSystem
            .write_new(&stage, b"replacement")
            .is_err(),
        "a name that is already taken is refused"
    );
    assert_eq!(PlatformFileSystem.read(&stage).unwrap(), b"interrupted");
}

/// Discovery classifies what it finds by the kind of the final entry, never by the
/// kind of a link's target, so this adapter must report links as links.
#[test]
fn topology_is_reported_without_following_a_link() {
    let tree = TemporaryTree::new("effects-topology");
    let directory = tree.path().join("Orders");
    fs::create_dir(&directory).unwrap();
    let file = tree.path().join("README.md");
    fs::write(&file, b"prose").unwrap();
    let link_to_directory = tree.path().join("link-to-directory");
    std::os::unix::fs::symlink(&directory, &link_to_directory).unwrap();
    let dangling = tree.path().join("dangling-link");
    std::os::unix::fs::symlink(tree.path().join("nowhere"), &dangling).unwrap();

    assert_eq!(
        PlatformFileSystem.symlink_metadata(&directory).unwrap(),
        Some(FileSystemEntryKind::Directory)
    );
    assert_eq!(
        PlatformFileSystem.symlink_metadata(&file).unwrap(),
        Some(FileSystemEntryKind::File)
    );
    assert_eq!(
        PlatformFileSystem
            .symlink_metadata(&link_to_directory)
            .unwrap(),
        Some(FileSystemEntryKind::SymbolicLink),
        "a link to a directory is a link"
    );
    assert_eq!(
        PlatformFileSystem.symlink_metadata(&dangling).unwrap(),
        Some(FileSystemEntryKind::SymbolicLink),
        "a link to nothing still exists"
    );
    assert_eq!(
        PlatformFileSystem
            .symlink_metadata(&tree.path().join("absent"))
            .unwrap(),
        None,
        "an absent entry is an answer rather than a failure"
    );

    let mut listed: Vec<(String, FileSystemEntryKind)> = PlatformFileSystem
        .read_directory(tree.path())
        .expect("the tree can be listed")
        .into_iter()
        .map(|entry| (entry.name().to_string_lossy().into_owned(), entry.kind()))
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            ("Orders".to_string(), FileSystemEntryKind::Directory),
            ("README.md".to_string(), FileSystemEntryKind::File),
            (
                "dangling-link".to_string(),
                FileSystemEntryKind::SymbolicLink
            ),
            (
                "link-to-directory".to_string(),
                FileSystemEntryKind::SymbolicLink
            ),
        ]
    );
}

/// Low-level batch replacement uses the canonical spelling as destination identity
/// before its first write, so two spellings of one entry must resolve alike.
#[test]
fn a_path_resolves_to_the_filesystem_s_own_spelling() {
    let tree = TemporaryTree::new("effects-canonicalize");
    let nested = tree.path().join("package/.spielorders");
    fs::create_dir_all(&nested).unwrap();

    let direct = PlatformFileSystem
        .canonicalize(&nested)
        .expect("an existing path resolves");
    let indirect = PlatformFileSystem
        .canonicalize(&tree.path().join("package/./.spielorders"))
        .expect("an equivalent spelling resolves");

    assert!(direct.is_absolute());
    assert_eq!(direct, indirect);
    assert!(
        PlatformFileSystem
            .canonicalize(&tree.path().join("absent"))
            .is_err(),
        "a path that does not resolve is a failure rather than a guess"
    );
}

/// The Package Index proves itself current by comparing change evidence, so an adapter
/// that cannot order two writes reduces every load to a rebuild.
#[test]
fn change_evidence_orders_a_later_write_after_an_earlier_one() {
    let tree = TemporaryTree::new("effects-change-evidence");
    let earlier = tree.path().join("index.json");
    let later = tree.path().join("order.json");
    PlatformFileSystem.write(&earlier, b"earlier").unwrap();
    // Coarse filesystem timestamp granularity, not slowness: two writes inside one
    // tick are indistinguishable, and this test is about the ordering an adapter can
    // report rather than about how finely the volume records it.
    std::thread::sleep(std::time::Duration::from_millis(20));
    PlatformFileSystem.write(&later, b"later").unwrap();

    let earlier = PlatformFileSystem.observe_file_change(&earlier).unwrap();
    let later = PlatformFileSystem.observe_file_change(&later).unwrap();
    assert_eq!(earlier.kind(), Some(FileSystemEntryKind::File));

    match (earlier.evidence(), later.evidence()) {
        (
            FileChangeEvidence::Comparable {
                comparison_domain: earlier_domain,
                sequence: earlier_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: later_domain,
                sequence: later_sequence,
            },
        ) => {
            assert_eq!(
                earlier_domain, later_domain,
                "one adapter reading one clock reports one comparison domain"
            );
            assert!(
                later_sequence > earlier_sequence,
                "the later write reports the later sequence"
            );
        }
        evidence => panic!("this adapter can supply comparable evidence: {evidence:?}"),
    }

    let absent = PlatformFileSystem
        .observe_file_change(&tree.path().join("absent"))
        .unwrap();
    assert_eq!(absent.kind(), None);
}

/// A capability that creates a directory lazily and may have to undo that creation
/// tells what it created from what was already there by whether this call succeeded.
/// An adapter that adopted an existing directory would report success for a directory
/// the user made, and the rollback would then be entitled to remove it.
#[test]
fn creating_a_directory_refuses_existing_evidence_of_every_kind_and_preserves_it() {
    let tree = TemporaryTree::new("effects-directory-creation");

    // An empty directory is the case that matters most: it is the one an adopting
    // adapter looks identical to a fresh creation on, and the one a rollback would
    // then find safe to remove.
    let empty = tree.path().join("empty-directory");
    fs::create_dir(&empty).unwrap();
    let occupied = tree.path().join("occupied-directory");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("scan.pdf"), b"supporting material").unwrap();
    let file = tree.path().join("plain-file");
    fs::write(&file, b"user file").unwrap();
    let link = tree.path().join("dangling-link");
    std::os::unix::fs::symlink(tree.path().join("nowhere"), &link).unwrap();

    for (label, path) in [
        ("empty", &empty),
        ("occupied", &occupied),
        ("file", &file),
        ("link", &link),
    ] {
        assert!(
            PlatformFileSystem.create_directory(path).is_err(),
            "{label}: creation refuses a path that is already taken"
        );
    }

    assert!(
        entry_names(&empty).is_empty(),
        "the empty directory survives untouched"
    );
    assert_eq!(entry_names(&occupied), vec!["scan.pdf".to_string()]);
    assert_eq!(fs::read(&file).unwrap(), b"user file");
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());

    // A free name is created, so the refusals above are about the existing evidence
    // rather than a creation that never works.
    let fresh = tree.path().join("fresh-directory");
    PlatformFileSystem
        .create_directory(&fresh)
        .expect("a free name is created");
    assert!(fs::symlink_metadata(&fresh).unwrap().is_dir());
    PlatformFileSystem
        .remove_directory(&fresh)
        .expect("an empty directory the caller selected is removed");
    assert_eq!(PlatformFileSystem.symlink_metadata(&fresh).unwrap(), None);
}

/// Linking a staged file into place must not consume the staged name and must not
/// replace destination evidence.
#[test]
fn linking_a_staged_file_preserves_the_stage_and_any_destination() {
    let tree = TemporaryTree::new("effects-link-new-file");
    let stage = tree.path().join("order.json.stage");
    fs::write(&stage, b"staged record").unwrap();
    let free = tree.path().join("order.json");

    assert_eq!(
        PlatformFileSystem.link_new_file(&stage, &free).unwrap(),
        NewFileLinkOutcome::Linked
    );
    assert_eq!(fs::read(&free).unwrap(), b"staged record");
    assert!(
        stage.is_file(),
        "publication did not consume the staged name"
    );

    let occupied = tree.path().join("occupied.json");
    fs::write(&occupied, b"someone else's record").unwrap();
    assert_eq!(
        PlatformFileSystem.link_new_file(&stage, &occupied).unwrap(),
        NewFileLinkOutcome::DestinationExists
    );
    assert_eq!(fs::read(&occupied).unwrap(), b"someone else's record");
}

#[test]
fn publication_into_an_absent_destination_moves_the_complete_staging_directory() {
    let tree = TemporaryTree::new("effects-directory-publication");
    let source = staged_order(tree.path(), "complete");
    let destination = tree.path().join("2026_08_22_Laser_components");

    assert_eq!(
        PlatformFileSystem
            .rename_new_directory(&source, &destination)
            .unwrap(),
        NewDirectoryRenameOutcome::Renamed
    );
    assert_eq!(
        entry_names(&destination),
        vec![".spielorders".to_string(), "README.md".to_string()]
    );
    assert_eq!(
        PlatformFileSystem.symlink_metadata(&source).unwrap(),
        None,
        "the staging name is gone once its contents are published"
    );
}

#[test]
fn an_unpublishable_destination_is_an_error_rather_than_a_collision() {
    let tree = TemporaryTree::new("effects-unpublishable");
    let source = staged_order(tree.path(), "unreachable");

    // Nothing exists at this destination, so reporting a collision would tell creation
    // that concurrent evidence must be preserved when the real fact is a broken path.
    let outcome =
        PlatformFileSystem.rename_new_directory(&source, &tree.path().join("absent/child"));

    assert!(
        outcome.is_err(),
        "an absent parent is a failure to publish, not a collision: {outcome:?}"
    );
    assert!(source.join(".spielorders/order.json").is_file());
}

#[test]
fn a_destination_that_appears_during_publication_is_never_replaced() {
    let tree = TemporaryTree::new("effects-publication-race");

    // A user creating a folder in Finder, or a second SpielOrders window, can claim the
    // destination name after publication has already found it free. The port promises
    // one filesystem operation, so exactly one of the two wins and the loser's evidence
    // survives. A check followed by a rename cannot keep that promise: the concurrent
    // directory is created inside the window and then silently replaced.
    //
    // Which side wins is the scheduler's to decide, so both outcomes are accepted and
    // the attempt is repeated: `contended` then proves the test really did race rather
    // than quietly becoming a series of uncontested publications.
    let mut contended = 0;
    for attempt in 0..64 {
        let label = format!("race-{attempt}");
        let source = staged_order(tree.path(), &label);
        let destination = Arc::new(tree.path().join(format!("destination-{attempt}")));
        let start = Arc::new(Barrier::new(2));
        let claimed = Arc::new(AtomicBool::new(false));
        let competitor = {
            let (destination, start, claimed) =
                (destination.clone(), start.clone(), claimed.clone());
            thread::spawn(move || {
                start.wait();
                claimed.store(fs::create_dir(&*destination).is_ok(), Ordering::SeqCst);
            })
        };

        start.wait();
        let outcome = PlatformFileSystem.rename_new_directory(&source, &destination);
        competitor.join().expect("the competing creation finishes");

        let outcome =
            outcome.unwrap_or_else(|error| panic!("{label}: publication failed: {error}"));
        if claimed.load(Ordering::SeqCst) {
            contended += 1;
            assert_eq!(
                outcome,
                NewDirectoryRenameOutcome::DestinationExists,
                "{label}: the concurrent directory claimed the name first"
            );
            assert!(
                entry_names(&destination).is_empty(),
                "{label}: so it is still the empty directory that was created there"
            );
            assert!(
                source.join(".spielorders/order.json").is_file(),
                "{label}: and the staged Order survives to be published elsewhere"
            );
        } else {
            assert_eq!(
                outcome,
                NewDirectoryRenameOutcome::Renamed,
                "{label}: an uncontested destination is published"
            );
            assert_eq!(
                entry_names(&destination),
                vec![".spielorders".to_string(), "README.md".to_string()],
                "{label}: with its complete contents"
            );
        }
    }

    assert!(
        contended > 0,
        "no attempt was actually contended, so this proved nothing"
    );
}

/// The calendar effect answers with one real local calendar date.
///
/// **A local date is not the UTC date**, so this is a claim about the effect rather than
/// about the clock: the date it returns is a date the calendar has, it is a date this
/// machine can plausibly be at right now, and it is what an Order created with no
/// `date_ordered` is stamped with. What the effect must not do is answer with a
/// month or day of zero, which is what a lossy conversion out of the library's own types
/// would produce.
#[test]
fn the_calendar_answers_with_this_machines_own_local_date() {
    use spielorders_cli::effects::PlatformCalendar;
    use spielorders_core::ports::LocalCalendar;

    let today = PlatformCalendar.today();

    assert!(
        (1..=12).contains(&today.month),
        "{today:?} names a month of the year"
    );
    assert!(
        (1..=31).contains(&today.day),
        "{today:?} names a day of the month"
    );
    // Read twice across the boundary a UTC-versus-local mistake shows up at: the two
    // readings are the same date unless this ran across midnight, and either way both are
    // dates rather than one being an epoch default.
    assert!(today.year >= 2026, "{today:?} is not an epoch default");
    assert_eq!(
        PlatformCalendar.today().year,
        today.year,
        "the effect reads the clock rather than caching a date"
    );
}

/// The platform Trash takes one complete directory away and says where it put it.
///
/// **The debt this pays is that nothing is erased.** The port promises to submit an
/// intact directory and never to erase content, and a delete would satisfy "the source is
/// gone" just as well — so the trashed directory is compared byte for byte against the
/// tree that was submitted. Two opaque sentinels, one of them nested, are what make that a
/// claim about a whole tree rather than about a name.
///
/// The trashed copy is then removed, because this machine's Trash belongs to its owner
/// and a suite that filled it would be leaving its own litter behind. Every observation is
/// taken before that cleanup runs, so a failing assertion cannot skip it.
#[test]
fn the_platform_trash_takes_a_complete_directory_and_reports_where_it_put_it() {
    let tree = TemporaryTree::new("effects-trash");
    let directory = tree.path().join("2026_08_22_Laser_components");
    fs::create_dir(&directory).expect("the directory is created");
    fs::create_dir(directory.join("Quotes")).expect("its nested subdirectory is created");
    fs::write(directory.join("supporting-a.bin"), SUPPORTING_A).expect("a sentinel is written");
    fs::write(directory.join("Quotes/supporting-b.bin"), SUPPORTING_B)
        .expect("the nested sentinel is written");
    let submitted = managed_snapshot(&directory);

    let location = match PlatformTrash.submit_directory(&directory) {
        Ok(location) => location,
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            return assert!(
                directory.is_dir(),
                "a platform with no supported Trash removes nothing"
            );
        }
        Err(error) => panic!("the platform Trash accepts an ordinary directory: {error}"),
    };

    let source_remains = directory.exists();
    let reported = location.clone();
    let arrived = reported.as_ref().map(|path| path.is_dir());
    let trashed = reported
        .as_ref()
        .filter(|path| path.is_dir())
        .map(|path| managed_snapshot(path));
    let cleaned = reported
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| fs::remove_dir_all(path).is_ok());

    assert!(!source_remains, "the directory left its original path");
    assert_eq!(
        arrived,
        Some(true),
        "this platform reports where it put the directory, and it is there"
    );
    assert_eq!(
        trashed,
        Some(submitted),
        "every byte of the submitted tree arrived, so nothing was erased"
    );
    assert_eq!(
        cleaned,
        Some(true),
        "and this suite took its own directory back out of the owner's Trash"
    );
}

/// Submitting something that is not there fails rather than reporting a disposition.
///
/// The distinction the core depends on: a rejected submission leaves the removal refused
/// with the Order still active, so an adapter that answered `Ok` for a path it never took
/// would report a directory as disposed of while it was still in the Package.
#[test]
fn the_platform_trash_refuses_a_path_it_cannot_take() {
    let tree = TemporaryTree::new("effects-trash-absent");
    let absent = tree.path().join("no-such-order");

    let outcome = PlatformTrash.submit_directory(&absent);

    assert!(
        outcome.is_err(),
        "an absent path is not a directory this platform took away"
    );
}
