//! The shipped platform effects keep the ports' promises on a real volume: no-replace
//! and no-follow for the filesystem, and a real local date for the calendar.
//!
//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries; Decide how Package Index freshness is proven
//! without canonical rereads.

use crate::platform::{PlatformEffects, PlatformFileSystem};
use crate::ports::{
    Effects, FileChangeEvidence, FileSystem, FileSystemEntryKind, NewDirectoryRenameOutcome,
    NewFileLinkOutcome,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::{fs, path::Path, path::PathBuf, thread};

/// A temporary directory of this test's own, removed when the test ends.
struct Tree(PathBuf);

static TREE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl Tree {
    fn new(label: &str) -> Self {
        let sequence = TREE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "spielorders-platform-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("the temporary tree is created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("the temporary tree is removed");
    }
}

/// A complete Order Directory waiting to be published, exactly as creation stages one.
fn staged_order(parent: &Path, label: &str) -> PathBuf {
    let directory = parent.join(format!("staging-{label}"));
    fs::create_dir(&directory).expect("the staging directory is created");
    fs::create_dir(directory.join(".spielorders")).expect("its metadata directory is created");
    fs::write(directory.join(".spielorders/order.json"), b"{}").expect("its record is staged");
    fs::write(directory.join("README.md"), b"").expect("its README is staged");
    directory
}

fn entry_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory can be listed")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn publication_into_an_absent_destination_moves_the_complete_staging_directory() {
    let tree = Tree::new("directory-publication-absent");
    let source = staged_order(tree.path(), "absent");
    let destination = tree.path().join("2026_08_22_Laser_components");

    let outcome = PlatformFileSystem
        .rename_new_directory(&source, &destination)
        .expect("publishing at a free name succeeds");

    assert_eq!(outcome, NewDirectoryRenameOutcome::Renamed);
    assert!(
        !source.exists(),
        "the staging name is consumed by the publication"
    );
    assert_eq!(
        entry_names(&destination),
        vec![".spielorders".to_string(), "README.md".to_string()],
        "and the published directory holds everything that was staged"
    );
}

#[test]
fn publication_refuses_existing_destination_evidence_of_every_kind_and_preserves_it() {
    let tree = Tree::new("directory-publication-occupied");

    // An empty directory is the case that matters most. POSIX `rename` replaces an
    // empty destination directory, so it is the one kind of existing evidence a
    // publication can destroy without any error to report.
    let empty = tree.path().join("empty-directory");
    fs::create_dir(&empty).unwrap();
    // A directory with content, a plain file, and a dangling symbolic link are all
    // ordinary user space that publication must leave alone.
    let occupied = tree.path().join("occupied-directory");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("scan.pdf"), b"supporting material").unwrap();
    let file = tree.path().join("plain-file");
    fs::write(&file, b"user file").unwrap();
    let link = tree.path().join("dangling-link");
    std::os::unix::fs::symlink(tree.path().join("nowhere"), &link).unwrap();

    for (label, destination) in [
        ("empty", &empty),
        ("occupied", &occupied),
        ("file", &file),
        ("link", &link),
    ] {
        let source = staged_order(tree.path(), label);

        let outcome = PlatformFileSystem
            .rename_new_directory(&source, destination)
            .unwrap_or_else(|error| panic!("{label}: a collision is typed, not an error: {error}"));

        assert_eq!(
            outcome,
            NewDirectoryRenameOutcome::DestinationExists,
            "{label}: existing destination evidence is reported as a collision"
        );
        assert!(
            source.join(".spielorders/order.json").is_file(),
            "{label}: the refused publication keeps its staged Order for cleanup"
        );
    }

    assert!(
        entry_names(&empty).is_empty(),
        "the empty directory survives"
    );
    assert_eq!(entry_names(&occupied), vec!["scan.pdf".to_string()]);
    assert_eq!(fs::read(&file).unwrap(), b"user file");
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
}

/// A capability that creates a directory lazily and may have to undo that creation
/// tells what it created from what was already there by whether this call succeeded.
/// An adapter that adopted an existing directory would report success for a directory
/// the user made, and the rollback would then be entitled to remove it.
#[test]
fn creating_a_directory_refuses_existing_evidence_of_every_kind_and_preserves_it() {
    let tree = Tree::new("directory-creation-occupied");

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
}

#[test]
fn an_unpublishable_destination_is_an_error_rather_than_a_collision() {
    let tree = Tree::new("directory-publication-unpublishable");
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
    let tree = Tree::new("directory-publication-race");

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
                "{label}: and the staged Order is retained rather than moved away"
            );
        } else {
            assert_eq!(
                outcome,
                NewDirectoryRenameOutcome::Renamed,
                "{label}: this publication claimed the name first"
            );
            assert!(destination.join(".spielorders/order.json").is_file());
        }
    }

    assert!(
        contended > 0,
        "no attempt reached the destination first, so nothing about concurrent \
         destination evidence was exercised"
    );
}

#[test]
fn real_observation_is_no_follow_and_reports_comparable_evidence_or_uncertainty() {
    let temporary = Tree::new("portable-change-observation");
    let ordinary = temporary.path().join("ordinary.json");
    std::fs::write(&ordinary, b"{}\n").unwrap();

    let observed = PlatformFileSystem.observe_file_change(&ordinary).unwrap();

    assert_eq!(observed.kind, Some(FileSystemEntryKind::File));
    assert!(matches!(
        observed.evidence,
        FileChangeEvidence::Comparable { .. } | FileChangeEvidence::Uncertain
    ));

    #[cfg(unix)]
    {
        let link = temporary.path().join("link.json");
        std::os::unix::fs::symlink(&ordinary, &link).unwrap();
        assert_eq!(
            PlatformFileSystem.observe_file_change(&link).unwrap().kind,
            Some(FileSystemEntryKind::SymbolicLink)
        );
    }
}

#[test]
fn managed_bytes_survive_a_write_a_read_and_a_durable_replacement() {
    let tree = Tree::new("managed-bytes");
    let destination = tree.path().join("vendors.json");
    let stage = tree.path().join("vendors.json.stage");

    std::fs::write(&destination, b"first").expect("a managed file is written");
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
    let tree = Tree::new("write-new");
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
    let tree = Tree::new("topology");
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
        .map(|entry| (entry.name.to_string_lossy().into_owned(), entry.kind))
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
    let tree = Tree::new("canonicalize");
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
    let tree = Tree::new("change-evidence");
    let earlier = tree.path().join("index.json");
    let later = tree.path().join("order.json");
    std::fs::write(&earlier, b"earlier").unwrap();
    // Coarse filesystem timestamp granularity, not slowness: two writes inside one
    // tick are indistinguishable, and this test is about the ordering an adapter can
    // report rather than about how finely the volume records it.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&later, b"later").unwrap();

    let earlier = PlatformFileSystem.observe_file_change(&earlier).unwrap();
    let later = PlatformFileSystem.observe_file_change(&later).unwrap();
    assert_eq!(earlier.kind, Some(FileSystemEntryKind::File));

    match (earlier.evidence, later.evidence) {
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
    assert_eq!(absent.kind, None);
}

/// Linking a staged file into place must not consume the staged name and must not
/// replace destination evidence.
#[test]
fn linking_a_staged_file_preserves_the_stage_and_any_destination() {
    let tree = Tree::new("link-new-file");
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

/// The calendar effect answers with one real local calendar date, never the month or day
/// of zero a lossy conversion out of the library's own types would produce.
#[test]
fn the_calendar_answers_with_this_machines_own_local_date() {
    let today = PlatformEffects.today();

    assert!(
        (1..=12).contains(&today.month),
        "{today:?} names a month of the year"
    );
    assert!(
        (1..=31).contains(&today.day),
        "{today:?} names a day of the month"
    );
    assert!(today.year >= 2026, "{today:?} is not an epoch default");
}
