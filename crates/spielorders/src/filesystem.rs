//! Low-level filesystem mechanics for validated canonical-file replacement: the one
//! replacement the Package Index refresh performs, and the stage name every managed write
//! shares. Every other managed write is a step of a commit journal, in `journal.rs`.
//!
//! Every managed write follows one pattern: validate the candidate, stage beside the
//! destination, make it durable, check the destination against what the caller
//! observed, then rename. `docs/architecture/durable-writes.md` states what each step
//! buys. Read it before changing anything here.

use crate::{
    operations::{FailedOperation, FailedPath},
    ports::{FileSystem, FileSystemEntryKind},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Whether `destination` sits in an ordinary directory rather than one reached through a
/// link: a record behind a linked directory is not the Package's own, and nothing is ever
/// written through one.
fn in_ordinary_directory<F: FileSystem>(destination: &Path, filesystem: &F) -> bool {
    destination.parent().is_some_and(|parent| {
        matches!(
            filesystem.symlink_metadata(parent),
            Ok(Some(FileSystemEntryKind::Directory))
        )
    })
}

#[derive(Debug, PartialEq)]
pub(crate) enum ValidatedReplacementFailure<V> {
    InvalidDestination,
    /// The destination stopped matching the bytes the caller observed, between that
    /// observation and this replacement. Nothing was written.
    DestinationChanged,
    /// The destination could not be read to check it against what the caller
    /// observed, so whether it still matches is unknown. Refusing is the safe answer,
    /// but this is not evidence that anyone else wrote it. Nothing was written.
    DestinationUnreadable(FailedOperation),
    StagingWrite(FailedOperation),
    CandidateValidation(V),
    Durability(FailedOperation),
    Replacement(FailedOperation),
}

#[derive(Debug, PartialEq)]
pub(crate) struct ValidatedReplacementError<V> {
    pub(crate) failure: ValidatedReplacementFailure<V>,
    pub(crate) cleanup_failure: Option<FailedOperation>,
}

/// What the caller observed at a destination, carried into the replacement so the
/// primitive can refuse rather than overwrite a change it never saw.
///
/// A managed record's meaning, rather than its bytes, is carried by the commit journal's
/// own replacements; which evidence a caller carries is decided in "Preserve evidence and
/// other writers" in `docs/architecture/durable-writes.md`.
#[derive(Clone, Copy)]
pub(crate) enum DestinationExpectation<'a> {
    /// No evidence supplied; the destination is replaced whatever it now holds. A caller
    /// reaching this variant has no protection against a concurrent writer.
    Unchecked,
    /// Nothing must have appeared at the destination since the caller observed it
    /// empty.
    ///
    /// A destination that does not exist yet is a real observation, not a missing
    /// one: an interrupted write can leave a durable stage beside a canonical file
    /// it never created. Absence is checked by observing the path rather than by
    /// reading it, because a read cannot tell "nothing is there, exactly as
    /// observed" from "something is there and it could not be read".
    Absent,
    /// The destination must still hold exactly these bytes.
    Exactly(&'a [u8]),
}

/// Whether a destination still matches what the caller observed, asked the way the
/// caller's own expectation requires.
enum DestinationVerdict {
    Satisfied,
    Changed,
    /// Whether it still matches is unknown. Refusing is the safe answer, and it is
    /// not evidence that anyone else wrote it.
    Unreadable(FailedOperation),
}

fn verify_destination<F: FileSystem>(
    expected: &DestinationExpectation<'_>,
    destination: &Path,
    filesystem: &F,
) -> DestinationVerdict {
    match expected {
        DestinationExpectation::Unchecked => DestinationVerdict::Satisfied,
        DestinationExpectation::Absent => match filesystem.symlink_metadata(destination) {
            Ok(None) => DestinationVerdict::Satisfied,
            Ok(Some(_)) => DestinationVerdict::Changed,
            Err(error) => DestinationVerdict::Unreadable(failed(destination, &error, filesystem)),
        },
        DestinationExpectation::Exactly(observed) => {
            read_and_ask(destination, filesystem, |current| current == *observed)
        }
    }
}

/// Reads the destination and asks the caller's own question of what it holds.
fn read_and_ask<F: FileSystem>(
    destination: &Path,
    filesystem: &F,
    still_holds: impl Fn(&[u8]) -> bool,
) -> DestinationVerdict {
    match filesystem.read(destination) {
        Ok(current) if still_holds(&current) => DestinationVerdict::Satisfied,
        Ok(_) => DestinationVerdict::Changed,
        Err(error) => DestinationVerdict::Unreadable(failed(destination, &error, filesystem)),
    }
}

/// The same replacement, refusing when the destination no longer matches what the
/// caller observed.
///
/// The comparison happens as late as this primitive can place it: after the candidate
/// is validated and staged and the stage is durable, immediately before the atomic
/// rename. That narrows the window from "anything since the caller looked" to the gap
/// between this check and the rename.
///
/// It does **not** close that gap. Replacing atomically only if the destination still
/// matches would need a compare-and-swap this port does not offer, and building one is
/// outside this contract. An adopter must not treat this as making its own
/// revalidation unnecessary — it removes the wide window, not the race.
pub(crate) fn replace_validated_file_expecting<F, V, ValidationError>(
    destination: &Path,
    candidate: &[u8],
    expected: DestinationExpectation<'_>,
    filesystem: &F,
    validate: V,
) -> Result<(), ValidatedReplacementError<ValidationError>>
where
    F: FileSystem,
    V: FnOnce(&[u8]) -> Result<(), ValidationError>,
{
    let staging = staging_path(destination)
        .filter(|_| in_ordinary_directory(destination, filesystem))
        .ok_or(ValidatedReplacementError {
            failure: ValidatedReplacementFailure::InvalidDestination,
            cleanup_failure: None,
        })?;

    // The candidate is validated before anything is written. A capability that cannot
    // produce a valid candidate is a bug in this code, and it fails here with no
    // staging file to clean up.
    if let Err(error) = validate(candidate) {
        return Err(ValidatedReplacementError {
            failure: ValidatedReplacementFailure::CandidateValidation(error),
            cleanup_failure: None,
        });
    }

    filesystem
        .write_new(&staging, candidate)
        .map_err(|error| ValidatedReplacementError {
            failure: ValidatedReplacementFailure::StagingWrite(failed(
                &staging, &error, filesystem,
            )),
            cleanup_failure: None,
        })?;

    if let Err(error) = filesystem.sync_file(&staging) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::Durability(failed(&staging, &error, filesystem)),
            &staging,
            filesystem,
        ));
    }
    // Last possible moment before the destination is taken over. A change that
    // arrived any time after the caller's observation is caught here, and nothing has
    // been written to the destination yet when it is.
    match verify_destination(&expected, destination, filesystem) {
        DestinationVerdict::Satisfied => {}
        DestinationVerdict::Changed => {
            return Err(with_cleanup(
                ValidatedReplacementFailure::DestinationChanged,
                &staging,
                filesystem,
            ));
        }
        // Unknown is not the same as changed, and a caller deciding what to do
        // next needs to be able to tell them apart.
        DestinationVerdict::Unreadable(error) => {
            return Err(with_cleanup(
                ValidatedReplacementFailure::DestinationUnreadable(error),
                &staging,
                filesystem,
            ));
        }
    }
    if let Err(error) = filesystem.replace_file(&staging, destination) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::Replacement(failed(destination, &error, filesystem)),
            &staging,
            filesystem,
        ));
    }

    Ok(())
}

fn with_cleanup<F: FileSystem, V>(
    failure: ValidatedReplacementFailure<V>,
    staging: &Path,
    filesystem: &F,
) -> ValidatedReplacementError<V> {
    ValidatedReplacementError {
        failure,
        cleanup_failure: filesystem
            .remove_file(staging)
            .err()
            .map(|error| failed(staging, &error, filesystem)),
    }
}

/// `error`, which an operation on `path` met, as every failed filesystem operation is
/// reported.
fn failed<F: FileSystem>(path: &Path, error: &F::Error, filesystem: &F) -> FailedOperation {
    FailedOperation::filesystem(FailedPath::of(path), error, filesystem)
}

pub(crate) fn staging_path(destination: &Path) -> Option<PathBuf> {
    let mut staging_name = OsString::from(".");
    staging_name.push(destination.file_name()?);
    staging_name.push(".spielorders-stage");
    Some(destination.with_file_name(staging_name))
}
