//! Low-level filesystem mechanics for validated canonical-file replacement.
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
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FileReplacement<'a> {
    destination: &'a Path,
    candidate: &'a [u8],
    observed: Option<&'a [u8]>,
}

impl<'a> FileReplacement<'a> {
    pub(crate) fn destination(&self) -> &'a Path {
        self.destination
    }

    /// The same operation, carrying the exact bytes the caller last observed at this
    /// destination. The replacement is refused unless the destination still holds
    /// them, which extends the guard back to the caller's own observation rather than
    /// starting it at this batch's snapshot read.
    pub(crate) fn observed(destination: &'a Path, candidate: &'a [u8], observed: &'a [u8]) -> Self {
        Self {
            destination,
            candidate,
            observed: Some(observed),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum ReplacementPlanFailure {
    InvalidDestination {
        operation_index: usize,
    },
    DestinationIdentity {
        operation_index: usize,
        error: FailedOperation,
    },
    DuplicateDestination {
        operation_index: usize,
    },
    CandidateValidation {
        operation_index: usize,
    },
}

#[derive(Debug, PartialEq)]
pub(crate) enum MultiFileReplacementFailure<V> {
    InvalidPlan(Vec<ReplacementPlanFailure>),
    /// The destination no longer holds what this operation observed, so someone else
    /// wrote it after the plan read it. Refusing keeps their bytes.
    DestinationChanged {
        destination: PathBuf,
    },
    PriorRead {
        destination: PathBuf,
        error: FailedOperation,
    },
    Replacement {
        destination: PathBuf,
        error: ValidatedReplacementError<V>,
    },
}

/// A restoration this operation declined to perform, because the destination no
/// longer holds the bytes this operation wrote there. Restoring would destroy a
/// change made by someone else. Distinct from `RollbackFailure`, which is a
/// restoration that was attempted and failed.
#[derive(Debug, PartialEq)]
pub(crate) struct RefusedRollback {
    pub(crate) destination: PathBuf,
}

#[derive(Debug, PartialEq)]
pub(crate) struct RollbackFailure<V> {
    pub(crate) destination: PathBuf,
    pub(crate) error: ValidatedReplacementError<V>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct MultiFileReplacementError<V> {
    pub(crate) failure: Box<MultiFileReplacementFailure<V>>,
    pub(crate) cleanup_failures: Vec<FailedOperation>,
    pub(crate) rollback_failures: Vec<RollbackFailure<V>>,
    /// Restorations declined because another writer had since changed the
    /// destination. Empty on every uncontended path.
    pub(crate) refused_rollbacks: Vec<RefusedRollback>,
}

/// Validates an entire ordered plan before its first write, then replaces each
/// destination in slice order. A later failure restores earlier destinations in
/// reverse slice order and retains every restoration failure in the returned error.
pub(crate) fn replace_validated_files<F, V, ValidationError>(
    plan: &[FileReplacement<'_>],
    filesystem: &F,
    validate: V,
) -> Result<(), MultiFileReplacementError<ValidationError>>
where
    F: FileSystem,
    V: Fn(&Path, &[u8]) -> Result<(), ValidationError>,
{
    let mut failures = Vec::new();
    let mut destination_identities = BTreeSet::new();
    for (operation_index, replacement) in plan.iter().enumerate() {
        if staging_path(replacement.destination).is_none() {
            failures.push(ReplacementPlanFailure::InvalidDestination { operation_index });
        }
        match filesystem.canonicalize(replacement.destination) {
            Ok(destination_identity) => {
                if !destination_identities.insert(destination_identity) {
                    failures.push(ReplacementPlanFailure::DuplicateDestination { operation_index });
                }
            }
            Err(error) => failures.push(ReplacementPlanFailure::DestinationIdentity {
                operation_index,
                error: failed(replacement.destination, &error, filesystem),
            }),
        }
    }
    failures.extend(
        plan.iter()
            .enumerate()
            .filter_map(|(operation_index, replacement)| {
                validate(replacement.destination, replacement.candidate)
                    .err()
                    .map(|_| ReplacementPlanFailure::CandidateValidation { operation_index })
            }),
    );

    if !failures.is_empty() {
        return Err(MultiFileReplacementError {
            failure: Box::new(MultiFileReplacementFailure::InvalidPlan(failures)),
            cleanup_failures: Vec::new(),
            rollback_failures: Vec::new(),
            refused_rollbacks: Vec::new(),
        });
    }

    let mut prior_contents = Vec::with_capacity(plan.len());
    for replacement in plan {
        match filesystem.read(replacement.destination) {
            Ok(contents) => prior_contents.push(contents),
            Err(error) => {
                return Err(MultiFileReplacementError {
                    failure: Box::new(MultiFileReplacementFailure::PriorRead {
                        destination: replacement.destination.to_path_buf(),
                        error: failed(replacement.destination, &error, filesystem),
                    }),
                    cleanup_failures: Vec::new(),
                    rollback_failures: Vec::new(),
                    refused_rollbacks: Vec::new(),
                });
            }
        }
    }

    // Every candidate is staged and handed to the device first, and one full sync then
    // makes them all durable together, before any destination is taken over: a full
    // sync drains the device's whole cache, so one per plan does what one per file did.
    let mut staged = Vec::with_capacity(plan.len());
    for replacement in plan {
        match stage_candidate(replacement, filesystem, &validate) {
            Ok(staging) => staged.push(staging),
            Err(error) => {
                let cleanup_failures = remove_stages(&staged, filesystem);
                let destination = replacement.destination.to_path_buf();
                return Err(MultiFileReplacementError {
                    failure: Box::new(MultiFileReplacementFailure::Replacement {
                        destination,
                        error,
                    }),
                    cleanup_failures,
                    rollback_failures: Vec::new(),
                    refused_rollbacks: Vec::new(),
                });
            }
        }
    }
    if let Some(last) = staged.last()
        && let Err(error) = filesystem.sync_file(last)
    {
        let failure = ValidatedReplacementError {
            failure: ValidatedReplacementFailure::Durability(failed(last, &error, filesystem)),
            cleanup_failure: None,
        };
        let cleanup_failures = remove_stages(&staged, filesystem);
        return Err(MultiFileReplacementError {
            failure: Box::new(MultiFileReplacementFailure::Replacement {
                destination: plan[plan.len() - 1].destination.to_path_buf(),
                error: failure,
            }),
            cleanup_failures,
            rollback_failures: Vec::new(),
            refused_rollbacks: Vec::new(),
        });
    }

    for (operation_index, replacement) in plan.iter().enumerate() {
        // Each destination carries what this operation observed there, so a write by
        // anyone else since that observation is refused rather than overwritten.
        if let Err(error) = promote_stage(
            &staged[operation_index],
            replacement.destination,
            DestinationExpectation::Exactly(
                replacement
                    .observed
                    .unwrap_or(&prior_contents[operation_index]),
            ),
            filesystem,
        ) {
            // The refused stage attempted its own cleanup; later stages were never promoted.
            let mut cleanup_failures = remove_stages(&staged[operation_index + 1..], filesystem);
            let (rollback_failures, refused_rollbacks) = roll_back(
                plan,
                &prior_contents,
                operation_index,
                filesystem,
                &validate,
            );
            let destination = replacement.destination.to_path_buf();
            let failure = if refused_for_changed_destination(&error) {
                cleanup_failures.extend(error.cleanup_failure);
                MultiFileReplacementFailure::DestinationChanged { destination }
            } else {
                MultiFileReplacementFailure::Replacement { destination, error }
            };
            return Err(MultiFileReplacementError {
                failure: Box::new(failure),
                cleanup_failures,
                rollback_failures,
                refused_rollbacks,
            });
        }
    }

    Ok(())
}

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

/// Validates `replacement`'s candidate and writes it to its stage, handing its contents
/// to the device; the stage, or why there is none, cleaned up after itself.
fn stage_candidate<F, V, ValidationError>(
    replacement: &FileReplacement<'_>,
    filesystem: &F,
    validate: &V,
) -> Result<PathBuf, ValidatedReplacementError<ValidationError>>
where
    F: FileSystem,
    V: Fn(&Path, &[u8]) -> Result<(), ValidationError>,
{
    let staging = staging_path(replacement.destination)
        .filter(|_| in_ordinary_directory(replacement.destination, filesystem))
        .ok_or(ValidatedReplacementError {
            failure: ValidatedReplacementFailure::InvalidDestination,
            cleanup_failure: None,
        })?;
    if let Err(error) = validate(replacement.destination, replacement.candidate) {
        return Err(ValidatedReplacementError {
            failure: ValidatedReplacementFailure::CandidateValidation(error),
            cleanup_failure: None,
        });
    }
    filesystem
        .write_new(&staging, replacement.candidate)
        .map_err(|error| ValidatedReplacementError {
            failure: ValidatedReplacementFailure::StagingWrite(failed(
                &staging, &error, filesystem,
            )),
            cleanup_failure: None,
        })?;
    if let Err(error) = filesystem.sync_file_contents(&staging) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::Durability(failed(&staging, &error, filesystem)),
            &staging,
            filesystem,
        ));
    }
    Ok(staging)
}

/// Takes `destination` over with the durable `staging`, refusing when the destination no
/// longer matches what the caller observed; the stage is cleaned up on refusal.
fn promote_stage<F: FileSystem, V>(
    staging: &Path,
    destination: &Path,
    expected: DestinationExpectation<'_>,
    filesystem: &F,
) -> Result<(), ValidatedReplacementError<V>> {
    match verify_destination(&expected, destination, filesystem) {
        DestinationVerdict::Satisfied => {}
        DestinationVerdict::Changed => {
            return Err(with_cleanup(
                ValidatedReplacementFailure::DestinationChanged,
                staging,
                filesystem,
            ));
        }
        DestinationVerdict::Unreadable(error) => {
            return Err(with_cleanup(
                ValidatedReplacementFailure::DestinationUnreadable(error),
                staging,
                filesystem,
            ));
        }
    }
    filesystem
        .replace_file(staging, destination)
        .map_err(|error| {
            with_cleanup(
                ValidatedReplacementFailure::Replacement(failed(destination, &error, filesystem)),
                staging,
                filesystem,
            )
        })
}

/// Removes every unpromoted stage and reports any evidence left behind alongside the
/// failure that stopped the commit.
fn remove_stages<F: FileSystem>(stages: &[PathBuf], filesystem: &F) -> Vec<FailedOperation> {
    stages
        .iter()
        .filter_map(|staging| {
            filesystem
                .remove_file(staging)
                .err()
                .map(|error| failed(staging, &error, filesystem))
        })
        .collect()
}

/// Whether a replacement stopped because the destination no longer matched, rather
/// than because the write itself failed.
fn refused_for_changed_destination<V>(error: &ValidatedReplacementError<V>) -> bool {
    matches!(
        error.failure,
        ValidatedReplacementFailure::DestinationChanged
    )
}

/// Undoes the operations that already completed, in reverse order.
///
/// A destination is restored only while it still holds what this operation wrote
/// there. Once someone else has changed it, restoring the prior bytes would destroy a
/// change this operation never made, so the restoration is declined and reported
/// instead — the recovery path is held to the same rule as the forward path.
fn roll_back<F, V, ValidationError>(
    plan: &[FileReplacement<'_>],
    prior_contents: &[Vec<u8>],
    failed_operation_index: usize,
    filesystem: &F,
    validate: &V,
) -> (Vec<RollbackFailure<ValidationError>>, Vec<RefusedRollback>)
where
    F: FileSystem,
    V: Fn(&Path, &[u8]) -> Result<(), ValidationError>,
{
    let mut rollback_failures = Vec::new();
    let mut refused_rollbacks = Vec::new();
    for rollback_index in (0..failed_operation_index).rev() {
        let rollback = &plan[rollback_index];
        match replace_validated_file_expecting(
            rollback.destination,
            &prior_contents[rollback_index],
            DestinationExpectation::Exactly(rollback.candidate),
            filesystem,
            |bytes| validate(rollback.destination, bytes),
        ) {
            Ok(()) => {}
            Err(error) if refused_for_changed_destination(&error) => {
                refused_rollbacks.push(RefusedRollback {
                    destination: rollback.destination.to_path_buf(),
                });
            }
            Err(error) => rollback_failures.push(RollbackFailure {
                destination: rollback.destination.to_path_buf(),
                error,
            }),
        }
    }
    (rollback_failures, refused_rollbacks)
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
/// Byte evidence answers "does this file still hold the bytes I read?"; a semantic
/// expectation answers "does this file still *mean* what I read?". Which one a caller
/// carries is decided in "Preserve evidence and other writers" in
/// `docs/architecture/durable-writes.md`.
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
    /// The destination must still satisfy the caller's own reading of it. The
    /// predicate receives the destination's current bytes and answers whether they
    /// still represent what the caller observed; a caller holding a parsed record
    /// parses these bytes and compares documents.
    ///
    /// It must answer `false` for bytes it cannot interpret. Refusing is the safe
    /// answer when the destination stopped being readable as the thing the caller
    /// planned against.
    Equivalent(&'a dyn Fn(&[u8]) -> bool),
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
        DestinationExpectation::Equivalent(still_holds) => {
            read_and_ask(destination, filesystem, still_holds)
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

#[derive(Debug, PartialEq)]
pub(crate) enum StagedPromotionFailure {
    /// The destination stopped matching what the caller observed, so someone wrote it
    /// after the caller looked. Nothing was written and the stage is untouched, so the
    /// candidate can still be examined and promoted after a fresh look.
    DestinationChanged,
    /// Whether the destination still matches is unknown, which is not the same as
    /// changed. Nothing was written.
    DestinationUnreadable(FailedOperation),
    /// The stage could not be made durable, so it is not published. It is left where
    /// it was.
    Durability(FailedOperation),
    /// The rename itself failed. The destination holds what it held, and the stage is
    /// still beside it.
    Replacement(FailedOperation),
}

/// Publishes a staging file that already sits beside its destination, refusing when
/// the destination no longer matches what the caller observed.
///
/// This is the second half of the pattern above, for the case where something else
/// performed the first half: an interrupted replacement left a durable stage, and
/// finishing it means renaming that stage rather than writing a new one. It cannot go
/// through [`replace_validated_file_expecting`], because the path a fresh stage would
/// take is the one this stage already occupies.
///
/// The stage is made durable first. It survived long enough to be observed, but the
/// write that abandoned it may have stopped between creating it and syncing it, and
/// this is the moment it becomes a canonical file.
///
/// Nothing is cleaned up on any path here. The stage is evidence this operation did
/// not create, and this pattern removes only what it wrote itself.
pub(crate) fn promote_staged_file<F: FileSystem>(
    staged: &Path,
    destination: &Path,
    expected: DestinationExpectation<'_>,
    filesystem: &F,
) -> Result<(), StagedPromotionFailure> {
    filesystem
        .sync_file(staged)
        .map_err(|error| StagedPromotionFailure::Durability(failed(staged, &error, filesystem)))?;
    // Last possible moment before the destination is taken over, exactly as in the
    // full pattern, and with the same caveat: this narrows the window rather than
    // closing it.
    match verify_destination(&expected, destination, filesystem) {
        DestinationVerdict::Satisfied => {}
        DestinationVerdict::Changed => return Err(StagedPromotionFailure::DestinationChanged),
        DestinationVerdict::Unreadable(error) => {
            return Err(StagedPromotionFailure::DestinationUnreadable(error));
        }
    }
    filesystem
        .replace_file(staged, destination)
        .map_err(|error| {
            StagedPromotionFailure::Replacement(failed(destination, &error, filesystem))
        })
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
