//! Low-level filesystem mechanics for validated canonical-file replacement.
//!
//! Every managed write follows one pattern: validate the candidate, stage beside the
//! destination, make it durable, check the destination against what the caller
//! observed, then rename. `docs/architecture/durable-writes.md` states what each step
//! buys and what was deliberately removed. Read it before changing anything here.

use crate::ports::FileSystem;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug)]
pub struct FileReplacement<'a> {
    destination: &'a Path,
    candidate: &'a [u8],
    observed: Option<&'a [u8]>,
}

impl<'a> FileReplacement<'a> {
    /// Declares one operation in the caller-defined deterministic batch order.
    ///
    /// The destination is guarded against the batch's own snapshot read. Prefer
    /// [`FileReplacement::observed`] where the caller has already read the
    /// destination, which guards the wider window back to that read.
    pub fn new(destination: &'a Path, candidate: &'a [u8]) -> Self {
        Self {
            destination,
            candidate,
            observed: None,
        }
    }

    /// The same operation, carrying the exact bytes the caller last observed at this
    /// destination. The replacement is refused unless the destination still holds
    /// them, which extends the guard back to the caller's own observation rather than
    /// starting it at this batch's snapshot read.
    pub fn observed(destination: &'a Path, candidate: &'a [u8], observed: &'a [u8]) -> Self {
        Self {
            destination,
            candidate,
            observed: Some(observed),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ReplacementPlanFailure<E, V> {
    InvalidDestination {
        operation_index: usize,
    },
    DestinationIdentity {
        operation_index: usize,
        error: E,
    },
    DuplicateDestination {
        first_operation_index: usize,
        operation_index: usize,
    },
    CandidateValidation {
        operation_index: usize,
        error: V,
    },
}

#[derive(Debug, PartialEq)]
pub enum MultiFileReplacementFailure<E, V> {
    InvalidPlan(Vec<ReplacementPlanFailure<E, V>>),
    /// The destination no longer holds what this operation observed, so someone else
    /// wrote it after the plan read it. Refusing keeps their bytes.
    DestinationChanged {
        operation_index: usize,
        destination: PathBuf,
    },
    PriorRead {
        operation_index: usize,
        destination: PathBuf,
        error: E,
    },
    Replacement {
        operation_index: usize,
        destination: PathBuf,
        error: ValidatedReplacementError<E, V>,
    },
}

/// A restoration this operation declined to perform, because the destination no
/// longer holds the bytes this operation wrote there. Restoring would destroy a
/// change made by someone else. Distinct from `RollbackFailure`, which is a
/// restoration that was attempted and failed.
#[derive(Debug, PartialEq)]
pub struct RefusedRollback {
    operation_index: usize,
    destination: PathBuf,
}

impl RefusedRollback {
    pub fn operation_index(&self) -> usize {
        self.operation_index
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Debug, PartialEq)]
pub struct RollbackFailure<E, V> {
    operation_index: usize,
    destination: PathBuf,
    error: ValidatedReplacementError<E, V>,
}

impl<E, V> RollbackFailure<E, V> {
    pub fn operation_index(&self) -> usize {
        self.operation_index
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn error(&self) -> &ValidatedReplacementError<E, V> {
        &self.error
    }
}

#[derive(Debug, PartialEq)]
pub struct MultiFileReplacementError<E, V> {
    failure: MultiFileReplacementFailure<E, V>,
    rollback_failures: Vec<RollbackFailure<E, V>>,
    refused_rollbacks: Vec<RefusedRollback>,
}

impl<E, V> MultiFileReplacementError<E, V> {
    pub fn failure(&self) -> &MultiFileReplacementFailure<E, V> {
        &self.failure
    }

    pub fn rollback_failures(&self) -> &[RollbackFailure<E, V>] {
        &self.rollback_failures
    }

    /// Restorations declined because another writer had since changed the
    /// destination. Empty on every uncontended path.
    pub fn refused_rollbacks(&self) -> &[RefusedRollback] {
        &self.refused_rollbacks
    }
}

/// Validates an entire ordered plan before its first write, then replaces each
/// destination in slice order. A later failure restores earlier destinations in
/// reverse slice order and retains every restoration failure in the returned error.
pub fn replace_validated_files<F, V, ValidationError>(
    plan: &[FileReplacement<'_>],
    filesystem: &F,
    validate: V,
) -> Result<(), MultiFileReplacementError<F::Error, ValidationError>>
where
    F: FileSystem,
    V: Fn(&Path, &[u8]) -> Result<(), ValidationError>,
{
    let mut failures = Vec::new();
    let mut first_operation_by_destination_identity = BTreeMap::new();
    for (operation_index, replacement) in plan.iter().enumerate() {
        if staging_path(replacement.destination).is_none() {
            failures.push(ReplacementPlanFailure::InvalidDestination { operation_index });
        }
        match filesystem.canonicalize(replacement.destination) {
            Ok(destination_identity) => {
                if let Some(first_operation_index) =
                    first_operation_by_destination_identity.get(&destination_identity)
                {
                    failures.push(ReplacementPlanFailure::DuplicateDestination {
                        first_operation_index: *first_operation_index,
                        operation_index,
                    });
                } else {
                    first_operation_by_destination_identity
                        .insert(destination_identity, operation_index);
                }
            }
            Err(error) => failures.push(ReplacementPlanFailure::DestinationIdentity {
                operation_index,
                error,
            }),
        }
    }
    failures.extend(
        plan.iter()
            .enumerate()
            .filter_map(|(operation_index, replacement)| {
                validate(replacement.destination, replacement.candidate)
                    .err()
                    .map(|error| ReplacementPlanFailure::CandidateValidation {
                        operation_index,
                        error,
                    })
            }),
    );

    if !failures.is_empty() {
        return Err(MultiFileReplacementError {
            failure: MultiFileReplacementFailure::InvalidPlan(failures),
            rollback_failures: Vec::new(),
            refused_rollbacks: Vec::new(),
        });
    }

    let mut prior_contents = Vec::with_capacity(plan.len());
    for (operation_index, replacement) in plan.iter().enumerate() {
        match filesystem.read(replacement.destination) {
            Ok(contents) => prior_contents.push(contents),
            Err(error) => {
                return Err(MultiFileReplacementError {
                    failure: MultiFileReplacementFailure::PriorRead {
                        operation_index,
                        destination: replacement.destination.to_path_buf(),
                        error,
                    },
                    rollback_failures: Vec::new(),
                    refused_rollbacks: Vec::new(),
                });
            }
        }
    }

    for (operation_index, replacement) in plan.iter().enumerate() {
        // Each destination carries what this operation observed there, so a write by
        // anyone else since that observation is refused rather than overwritten.
        if let Err(error) = replace_validated_file_expecting(
            replacement.destination,
            replacement.candidate,
            DestinationExpectation::Exactly(
                replacement
                    .observed
                    .unwrap_or(&prior_contents[operation_index]),
            ),
            filesystem,
            |bytes| validate(replacement.destination, bytes),
        ) {
            let (rollback_failures, refused_rollbacks) = roll_back(
                plan,
                &prior_contents,
                operation_index,
                filesystem,
                &validate,
            );
            let destination = replacement.destination.to_path_buf();
            let failure = if refused_for_changed_destination(&error) {
                MultiFileReplacementFailure::DestinationChanged {
                    operation_index,
                    destination,
                }
            } else {
                MultiFileReplacementFailure::Replacement {
                    operation_index,
                    destination,
                    error,
                }
            };
            return Err(MultiFileReplacementError {
                failure,
                rollback_failures,
                refused_rollbacks,
            });
        }
    }

    Ok(())
}

/// Undoes the operations that already completed, in reverse order.
///
/// A destination is restored only while it still holds what this operation wrote
/// there. Once someone else has changed it, restoring the prior bytes would destroy a
/// change this operation never made, so the restoration is declined and reported
/// instead — the recovery path is held to the same rule as the forward path.
/// Whether a replacement stopped because the destination no longer matched, rather
/// than because the write itself failed.
fn refused_for_changed_destination<E, V>(error: &ValidatedReplacementError<E, V>) -> bool {
    matches!(
        error.failure(),
        ValidatedReplacementFailure::DestinationChanged
    )
}

fn roll_back<F, V, ValidationError>(
    plan: &[FileReplacement<'_>],
    prior_contents: &[Vec<u8>],
    failed_operation_index: usize,
    filesystem: &F,
    validate: &V,
) -> (
    Vec<RollbackFailure<F::Error, ValidationError>>,
    Vec<RefusedRollback>,
)
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
                    operation_index: rollback_index,
                    destination: rollback.destination.to_path_buf(),
                });
            }
            Err(error) => rollback_failures.push(RollbackFailure {
                operation_index: rollback_index,
                destination: rollback.destination.to_path_buf(),
                error,
            }),
        }
    }
    (rollback_failures, refused_rollbacks)
}

#[derive(Debug, PartialEq)]
pub enum ValidatedReplacementFailure<E, V> {
    InvalidDestination,
    /// The destination stopped matching the bytes the caller observed, between that
    /// observation and this replacement. Nothing was written.
    DestinationChanged,
    /// The destination could not be read to check it against what the caller
    /// observed, so whether it still matches is unknown. Refusing is the safe answer,
    /// but this is not evidence that anyone else wrote it. Nothing was written.
    DestinationUnreadable(E),
    StagingWrite(E),
    CandidateValidation(V),
    Durability(E),
    Replacement(E),
}

#[derive(Debug, PartialEq)]
pub struct ValidatedReplacementError<E, V> {
    failure: ValidatedReplacementFailure<E, V>,
    cleanup_failure: Option<E>,
}

impl<E, V> ValidatedReplacementError<E, V> {
    pub fn failure(&self) -> &ValidatedReplacementFailure<E, V> {
        &self.failure
    }

    pub fn cleanup_failure(&self) -> Option<&E> {
        self.cleanup_failure.as_ref()
    }

    /// Takes the cleanup failure out, for a caller that reports the initiating
    /// failure in its own words and must not drop the cleanup one on the way.
    pub fn into_cleanup_failure(self) -> Option<E> {
        self.cleanup_failure
    }
}

/// What the caller observed at a destination, carried into the replacement so the
/// primitive can refuse rather than overwrite a change it never saw.
///
/// Two kinds of evidence, because two different questions get asked here. Byte
/// evidence answers "does this file still hold the bytes I read?"; a semantic
/// expectation answers "does this file still *mean* what I read?".
///
/// Which one to carry follows from what the caller's own work was defined over, not
/// from what kind of file it writes. A capability that revalidated a parsed record
/// asks the semantic question. Byte evidence goes where bytes are genuinely the
/// subject: the derived index, a cache this application owns and whose canonical
/// serialization it enforces; a README, exact user text with no parsed form to
/// compare against; and a Metadata Upgrade's artifact replacements, which carry bytes
/// even for an Order record or the Vendors aggregate because the transformation is
/// defined over the bytes it read. That same upgrade advances the Package marker on
/// semantic evidence: it did not transform the marker, it read a version from it.
///
/// See "Destination evidence" in `docs/architecture/durable-writes.md`.
#[derive(Clone, Copy)]
pub enum DestinationExpectation<'a> {
    /// No evidence supplied; the destination is replaced whatever it now holds.
    ///
    /// This is the historical behavior. It is the default only because adopting
    /// evidence is per-caller work: a caller must have observed the destination to
    /// supply what it observed. A caller reaching this variant has no protection
    /// against a concurrent writer.
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
enum DestinationVerdict<E> {
    Satisfied,
    Changed,
    /// Whether it still matches is unknown. Refusing is the safe answer, and it is
    /// not evidence that anyone else wrote it.
    Unreadable(E),
}

fn verify_destination<F: FileSystem>(
    expected: &DestinationExpectation<'_>,
    destination: &Path,
    filesystem: &F,
) -> DestinationVerdict<F::Error> {
    match expected {
        DestinationExpectation::Unchecked => DestinationVerdict::Satisfied,
        DestinationExpectation::Absent => match filesystem.symlink_metadata(destination) {
            Ok(None) => DestinationVerdict::Satisfied,
            Ok(Some(_)) => DestinationVerdict::Changed,
            Err(error) => DestinationVerdict::Unreadable(error),
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
) -> DestinationVerdict<F::Error> {
    match filesystem.read(destination) {
        Ok(current) if still_holds(&current) => DestinationVerdict::Satisfied,
        Ok(_) => DestinationVerdict::Changed,
        Err(error) => DestinationVerdict::Unreadable(error),
    }
}

impl std::fmt::Debug for DestinationExpectation<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unchecked => formatter.write_str("Unchecked"),
            Self::Absent => formatter.write_str("Absent"),
            Self::Exactly(observed) => formatter.debug_tuple("Exactly").field(observed).finish(),
            // A predicate has no representation worth printing; which kind of
            // expectation this is, is the part a reader needs.
            Self::Equivalent(_) => formatter.write_str("Equivalent(..)"),
        }
    }
}

pub fn replace_validated_file<F, V, ValidationError>(
    destination: &Path,
    candidate: &[u8],
    filesystem: &F,
    validate: V,
) -> Result<(), ValidatedReplacementError<F::Error, ValidationError>>
where
    F: FileSystem,
    V: FnOnce(&[u8]) -> Result<(), ValidationError>,
{
    replace_validated_file_expecting(
        destination,
        candidate,
        DestinationExpectation::Unchecked,
        filesystem,
        validate,
    )
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
pub fn replace_validated_file_expecting<F, V, ValidationError>(
    destination: &Path,
    candidate: &[u8],
    expected: DestinationExpectation<'_>,
    filesystem: &F,
    validate: V,
) -> Result<(), ValidatedReplacementError<F::Error, ValidationError>>
where
    F: FileSystem,
    V: FnOnce(&[u8]) -> Result<(), ValidationError>,
{
    let staging = staging_path(destination).ok_or(ValidatedReplacementError {
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
            failure: ValidatedReplacementFailure::StagingWrite(error),
            cleanup_failure: None,
        })?;

    if let Err(error) = filesystem.sync_file(&staging) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::Durability(error),
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
            ValidatedReplacementFailure::Replacement(error),
            &staging,
            filesystem,
        ));
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum StagedPromotionFailure<E> {
    /// The destination stopped matching what the caller observed, so someone wrote it
    /// after the caller looked. Nothing was written and the stage is untouched, so the
    /// candidate can still be examined and promoted after a fresh look.
    DestinationChanged,
    /// Whether the destination still matches is unknown, which is not the same as
    /// changed. Nothing was written.
    DestinationUnreadable(E),
    /// The stage could not be made durable, so it is not published. It is left where
    /// it was.
    Durability(E),
    /// The rename itself failed. The destination holds what it held, and the stage is
    /// still beside it.
    Replacement(E),
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
pub fn promote_staged_file<F: FileSystem>(
    staged: &Path,
    destination: &Path,
    expected: DestinationExpectation<'_>,
    filesystem: &F,
) -> Result<(), StagedPromotionFailure<F::Error>> {
    filesystem
        .sync_file(staged)
        .map_err(StagedPromotionFailure::Durability)?;
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
        .map_err(StagedPromotionFailure::Replacement)
}

fn with_cleanup<F: FileSystem, V>(
    failure: ValidatedReplacementFailure<F::Error, V>,
    staging: &Path,
    filesystem: &F,
) -> ValidatedReplacementError<F::Error, V> {
    ValidatedReplacementError {
        failure,
        cleanup_failure: filesystem.remove_file(staging).err(),
    }
}

pub(crate) fn staging_path(destination: &Path) -> Option<PathBuf> {
    let mut staging_name = OsString::from(".");
    staging_name.push(destination.file_name()?);
    staging_name.push(".spielorders-stage");
    Some(destination.with_file_name(staging_name))
}
