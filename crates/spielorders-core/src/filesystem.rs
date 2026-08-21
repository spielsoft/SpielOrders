//! Low-level filesystem mechanics for validated canonical-file replacement.

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
}

impl<'a> FileReplacement<'a> {
    /// Declares one operation in the caller-defined deterministic batch order.
    pub fn new(destination: &'a Path, candidate: &'a [u8]) -> Self {
        Self {
            destination,
            candidate,
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
}

impl<E, V> MultiFileReplacementError<E, V> {
    pub fn failure(&self) -> &MultiFileReplacementFailure<E, V> {
        &self.failure
    }

    pub fn rollback_failures(&self) -> &[RollbackFailure<E, V>] {
        &self.rollback_failures
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
                });
            }
        }
    }

    for (operation_index, replacement) in plan.iter().enumerate() {
        if let Err(error) = replace_validated_file(
            replacement.destination,
            replacement.candidate,
            filesystem,
            |bytes| validate(replacement.destination, bytes),
        ) {
            let mut rollback_failures = Vec::new();
            for rollback_index in (0..operation_index).rev() {
                let rollback = &plan[rollback_index];
                if let Err(error) = replace_validated_file(
                    rollback.destination,
                    &prior_contents[rollback_index],
                    filesystem,
                    |bytes| validate(rollback.destination, bytes),
                ) {
                    rollback_failures.push(RollbackFailure {
                        operation_index: rollback_index,
                        destination: rollback.destination.to_path_buf(),
                        error,
                    });
                }
            }
            return Err(MultiFileReplacementError {
                failure: MultiFileReplacementFailure::Replacement {
                    operation_index,
                    destination: replacement.destination.to_path_buf(),
                    error,
                },
                rollback_failures,
            });
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum ValidatedReplacementFailure<E, V> {
    InvalidDestination,
    StagingWrite(E),
    ValidationRead(E),
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
    let staging = staging_path(destination).ok_or(ValidatedReplacementError {
        failure: ValidatedReplacementFailure::InvalidDestination,
        cleanup_failure: None,
    })?;

    filesystem
        .write_new(&staging, candidate)
        .map_err(|error| ValidatedReplacementError {
            failure: ValidatedReplacementFailure::StagingWrite(error),
            cleanup_failure: None,
        })?;

    let staged = match filesystem.read(&staging) {
        Ok(staged) => staged,
        Err(error) => {
            return Err(with_cleanup(
                ValidatedReplacementFailure::ValidationRead(error),
                &staging,
                filesystem,
            ));
        }
    };
    if let Err(error) = validate(&staged) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::CandidateValidation(error),
            &staging,
            filesystem,
        ));
    }
    if let Err(error) = filesystem.sync_file(&staging) {
        return Err(with_cleanup(
            ValidatedReplacementFailure::Durability(error),
            &staging,
            filesystem,
        ));
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
