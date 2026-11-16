//! Planning and persistence for exact conventional README documents.
//!
//! A README is user-authored prose, never generated or normalized content. These
//! capabilities write the caller's exact UTF-8 bytes to one already established
//! Package or Order README reference and change nothing else.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, Touched, load_package},
    filesystem::{DestinationExpectation, replace_validated_file_expecting},
    mutation::{IndexRefresh, MutationReport, Never, OperationKind, PlannedOperation},
    operations::FailedOperation,
    order_edits::resolve_order,
    ports::{FileSystem, FileSystemEntryKind},
    readme::{ReadmeKind, ReadmeRead, ReadmeReference, ReadmeUnavailableReason},
};
use std::path::{Path, PathBuf};

/// One complete, validated write of one exact conventional README document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeWritePlan {
    root: PathBuf,
    reference: ReadmeReference,
    text: String,
    prior_exists: bool,
    prior_bytes: Vec<u8>,
    pub(crate) operations: Vec<PlannedOperation>,
}

impl ReadmeWritePlan {
    /// Whether committing this plan would replace the document's bytes.
    pub fn changes_document(&self) -> bool {
        !self.prior_exists || self.prior_bytes != self.text.as_bytes()
    }
}

/// The document as the writer read it before drafting. A write replaces only the
/// document this read observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadmeOriginal {
    /// Nothing stood at the conventional path.
    Absent,
    /// The document held exactly these bytes.
    Bytes(Vec<u8>),
    /// The read established no content, so it authorizes replacing nothing.
    Unavailable,
}

/// The original a README read reported: whether the document existed and the text the
/// read established, if any.
impl From<&ReadmeRead> for ReadmeOriginal {
    fn from(read: &ReadmeRead) -> Self {
        match (read.exists, &read.text) {
            (false, _) => Self::Absent,
            (true, Some(text)) => Self::Bytes(text.as_bytes().to_vec()),
            (true, None) => Self::Unavailable,
        }
    }
}

impl ReadmeOriginal {
    fn observed(&self, exists: bool, bytes: &[u8]) -> bool {
        match self {
            Self::Absent => !exists,
            Self::Bytes(original) => exists && original == bytes,
            Self::Unavailable => false,
        }
    }
}

/// Plans an exact write of the Package README of the Package rooted at
/// `selected_root`, replacing the document `original` observed.
pub(crate) fn plan_package_readme_write<F: FileSystem>(
    selected_root: &Path,
    original: &ReadmeOriginal,
    text: &str,
    filesystem: &F,
) -> Answer<ReadmeWritePlan> {
    Answer::of(|| {
        let package = load_package(selected_root, filesystem)?;
        let reference = package.context.package_readme.clone();
        readme_write_plan(&package, reference, original, text, filesystem)
    })
}

/// Plans an exact write of the README of the healthy Order at `order_directory`,
/// replacing the document `original` observed.
pub(crate) fn plan_order_readme_write<F: FileSystem>(
    order_directory: &Path,
    original: &ReadmeOriginal,
    text: &str,
    filesystem: &F,
) -> Answer<ReadmeWritePlan> {
    Answer::of(|| {
        let (package, order) = resolve_order(order_directory, filesystem)?;
        let reference = order.paths.readme.clone();
        readme_write_plan(&package, reference, original, text, filesystem)
    })
}

/// Reads the established destination's current bytes and assembles the plan. An
/// absent README is an ordinary creatable conventional document.
fn readme_write_plan<F: FileSystem>(
    package: &CurrentPackageCandidate,
    reference: ReadmeReference,
    original: &ReadmeOriginal,
    text: &str,
    filesystem: &F,
) -> Result<ReadmeWritePlan, Failure> {
    let path = reference.location.absolute_path.clone();
    let kind = filesystem.symlink_metadata(&path).map_err(|error| {
        Failure::from(
            FailedOperation::filesystem(reference.location.clone(), &error, filesystem)
                .under(&package.context.root),
        )
    })?;
    let (prior_exists, prior_bytes) = match kind {
        None => (false, Vec::new()),
        Some(FileSystemEntryKind::File) => {
            let bytes = filesystem
                .read(&path)
                .map_err(|_| Failure::UnwritableDestination {
                    path: reference.location.clone(),
                    reason: ReadmeUnavailableReason::Unreadable,
                })?;
            (true, bytes)
        }
        // A directory or a link at the conventional path is not a document this
        // capability may replace, and it never converts or removes one.
        Some(_) => {
            return Err(Failure::UnwritableDestination {
                path: reference.location.clone(),
                reason: ReadmeUnavailableReason::WrongKind,
            });
        }
    };
    if !original.observed(prior_exists, &prior_bytes) {
        return Err(Failure::PreconditionsChanged {
            location: reference.location.clone(),
        });
    }

    let mut operations = Vec::new();
    if !prior_exists || prior_bytes != text.as_bytes() {
        operations.push(PlannedOperation::new(
            OperationKind::ReplaceReadmeDocument,
            reference.location.clone(),
        ));
        // Only the Order README is projected into the derived index.
        if reference.kind == ReadmeKind::OrderReadme {
            operations.push(PlannedOperation::new(
                OperationKind::RefreshPackageIndex,
                package.context.index.clone(),
            ));
        }
    }

    Ok(ReadmeWritePlan {
        root: package.context.root.absolute_path.clone(),
        reference,
        text: text.to_owned(),
        prior_exists,
        prior_bytes,
        operations,
    })
}

pub(crate) type ReadmeWriteReport = MutationReport<()>;

pub(crate) fn commit_readme_write<F: FileSystem>(
    plan: ReadmeWritePlan,
    filesystem: &F,
) -> Answer<ReadmeWriteReport> {
    Answer::of(|| {
        let path = plan.reference.location.absolute_path.clone();
        let unobservable = |error: F::Error| {
            FailedOperation::filesystem(plan.reference.location.clone(), &error, filesystem).under(
                &crate::readme::located_in_package(&plan.root, plan.root.clone()),
            )
        };
        let observed = match filesystem.symlink_metadata(&path) {
            Ok(observed) => observed,
            Err(error) => {
                let failed = unobservable(error);
                return Err(failed.into());
            }
        };
        let current_bytes = match observed {
            None if plan.prior_exists => return Err(preconditions_changed(&plan)),
            None => Vec::new(),
            Some(FileSystemEntryKind::File) if !plan.prior_exists => {
                return Err(preconditions_changed(&plan));
            }
            Some(FileSystemEntryKind::File) => match filesystem.read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let failed = unobservable(error);
                    return Err(failed.into());
                }
            },
            Some(_) => return Err(preconditions_changed(&plan)),
        };
        if current_bytes != plan.prior_bytes {
            return Err(preconditions_changed(&plan));
        }

        if !plan.changes_document() {
            return Ok(MutationReport::Unchanged { result: () });
        }

        let candidate = plan.text.as_bytes().to_vec();
        // What the revalidation above observed is what this write is authorized against,
        // so a writer landing between that observation and the rename is refused rather
        // than overwritten. Observing nothing is one of the two observations: a document
        // observed absent carries no bytes to compare, but a README appearing at the path
        // afterwards is a changed destination like any other, and it is user-authored text.
        let expectation = if plan.prior_exists {
            DestinationExpectation::Exactly(&current_bytes)
        } else {
            DestinationExpectation::Absent
        };
        if let Err(error) = replace_validated_file_expecting(
            &path,
            &candidate,
            expectation,
            filesystem,
            // A README is exact user bytes with no schema, so there is nothing to
            // validate. It is written as given.
            |_| Ok::<(), Never>(()),
        ) {
            return Err(Failure::of_replacement(&plan.root, &path, error));
        }

        // Only the Order README is projected into the derived index, so a Package README
        // write is complete without touching it.
        if plan.reference.kind == ReadmeKind::PackageReadme {
            return Ok(MutationReport::Committed { result: () });
        }

        // A Package README is no part of the index; an Order README is part of its Order's
        // entry.
        let touched = match plan.reference.kind {
            ReadmeKind::OrderReadme => Touched::orders(
                plan.reference
                    .location
                    .absolute_path
                    .parent()
                    .map(Path::to_path_buf),
            ),
            ReadmeKind::PackageReadme => Touched::default(),
        };
        let refresh = IndexRefresh::after_commit(&plan.root, &touched, filesystem);
        Ok(refresh.report(()))
    })
}

fn preconditions_changed(plan: &ReadmeWritePlan) -> Failure {
    Failure::PreconditionsChanged {
        location: plan.reference.location.clone(),
    }
}
