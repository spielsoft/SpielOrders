//! Decision sources: Define the SpielOrders package topology; Define integrity,
//! validation, and repair boundaries.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::ports::{DirectoryFileSystem, NewDirectoryRenameOutcome};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::{fs, path::Path, path::PathBuf, thread};

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
    let tree = TemporaryTree::new("directory-publication-absent");
    let source = staged_order(tree.path(), "absent");
    let destination = tree.path().join("2026_08_22_Laser_components");

    let outcome = RealFileSystem
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
    let tree = TemporaryTree::new("directory-publication-occupied");

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

        let outcome = RealFileSystem
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
    let tree = TemporaryTree::new("directory-creation-occupied");

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
            RealFileSystem.create_directory(path).is_err(),
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
    RealFileSystem
        .create_directory(&fresh)
        .expect("a free name is created");
    assert!(fs::symlink_metadata(&fresh).unwrap().is_dir());
}

#[test]
fn an_unpublishable_destination_is_an_error_rather_than_a_collision() {
    let tree = TemporaryTree::new("directory-publication-unpublishable");
    let source = staged_order(tree.path(), "unreachable");

    // Nothing exists at this destination, so reporting a collision would tell creation
    // that concurrent evidence must be preserved when the real fact is a broken path.
    let outcome = RealFileSystem.rename_new_directory(&source, &tree.path().join("absent/child"));

    assert!(
        outcome.is_err(),
        "an absent parent is a failure to publish, not a collision: {outcome:?}"
    );
    assert!(source.join(".spielorders/order.json").is_file());
}

#[test]
fn a_destination_that_appears_during_publication_is_never_replaced() {
    let tree = TemporaryTree::new("directory-publication-race");

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
        let outcome = RealFileSystem.rename_new_directory(&source, &destination);
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
