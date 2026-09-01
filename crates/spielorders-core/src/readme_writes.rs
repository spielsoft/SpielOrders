//! Planning and persistence for exact conventional README documents.
//!
//! A README is user-authored prose, never generated or normalized content. These
//! capabilities write the caller's exact UTF-8 bytes to one already established
//! Package or Order README reference and change nothing else.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, PackageIndexRecord, PackageLoad,
        inspect_canonical_package, open_package,
    },
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, replace_validated_file_expecting,
    },
    mutation::MutationReport,
    operations::OperationFailure,
    order_edits::{OrderResolutionFailure, resolve_order},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{ReadmeKind, ReadmeReference, ReadmeUnavailableReason},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadmeWriteOperationKind {
    ReplaceReadmeDocument,
    RefreshPackageIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeWriteOperation {
    kind: ReadmeWriteOperationKind,
    path: PathBuf,
}

impl ReadmeWriteOperation {
    pub fn kind(&self) -> ReadmeWriteOperationKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// One complete, validated write of one exact conventional README document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeWritePlan {
    root: PathBuf,
    reference: ReadmeReference,
    text: String,
    prior_exists: bool,
    prior_bytes: Vec<u8>,
    prior_index: PackageIndexRecord,
    operations: Vec<ReadmeWriteOperation>,
}

impl ReadmeWritePlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The established document this write targets. A write never derives a new one.
    pub fn reference(&self) -> &ReadmeReference {
        &self.reference
    }

    pub fn kind(&self) -> ReadmeKind {
        self.reference.kind()
    }

    /// The exact text this write would store, byte for byte as supplied.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn prior_exists(&self) -> bool {
        self.prior_exists
    }

    /// The exact prior text, or `None` when the prior bytes were not valid UTF-8.
    pub fn prior_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.prior_bytes).ok()
    }

    pub fn operations(&self) -> &[ReadmeWriteOperation] {
        &self.operations
    }

    /// Whether committing this plan would replace the document's bytes.
    pub fn changes_document(&self) -> bool {
        !self.prior_exists || self.prior_bytes != self.text.as_bytes()
    }
}

/// Why a README write was refused before performing any write.
#[derive(Debug, PartialEq)]
pub enum ReadmeWritePlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound {
        path: PathBuf,
    },
    DamagedOrder {
        path: PathBuf,
    },
    /// The destination exists but is not a document this capability may replace.
    UnwritableDestination {
        path: PathBuf,
        reason: ReadmeUnavailableReason,
    },
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

/// Plans an exact write of the Package README of the Package rooted at
/// `selected_root`.
pub fn plan_package_readme_write<F: FileSystem>(
    selected_root: &Path,
    text: &str,
    filesystem: &F,
) -> Result<ReadmeWritePlan, ReadmeWritePlanError<F::Error>> {
    let package = current_package(selected_root, filesystem)?;
    let reference = package.context().package_readme().clone();
    readme_write_plan(&package, reference, text, filesystem)
}

/// Plans an exact write of the README of the healthy Order at `order_directory`.
pub fn plan_order_readme_write<F: FileSystem>(
    order_directory: &Path,
    text: &str,
    filesystem: &F,
) -> Result<ReadmeWritePlan, ReadmeWritePlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let reference = order.paths().readme().clone();
    readme_write_plan(&package, reference, text, filesystem)
}

impl<E> From<OrderResolutionFailure<E>> for ReadmeWritePlanError<E> {
    fn from(failure: OrderResolutionFailure<E>) -> Self {
        match failure {
            OrderResolutionFailure::UnmanagedPackage => Self::UnmanagedPackage,
            OrderResolutionFailure::DamagedPackage => Self::DamagedPackage,
            OrderResolutionFailure::UpgradePending => Self::UpgradePending,
            OrderResolutionFailure::PackageOperationFailed => Self::PackageOperationFailed,
            OrderResolutionFailure::OrderNotFound { path } => Self::OrderNotFound { path },
            OrderResolutionFailure::DamagedOrder { path } => Self::DamagedOrder { path },
            OrderResolutionFailure::FileSystem { path, error } => Self::FileSystem { path, error },
        }
    }
}

fn current_package<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, ReadmeWritePlanError<F::Error>> {
    match inspect_canonical_package(selected_root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => Ok(package),
        CanonicalPackageLoad::Unmanaged(_) => Err(ReadmeWritePlanError::UnmanagedPackage),
        CanonicalPackageLoad::DiagnosticOnly(_) => Err(ReadmeWritePlanError::DamagedPackage),
        CanonicalPackageLoad::UpgradePending(_) => Err(ReadmeWritePlanError::UpgradePending),
        CanonicalPackageLoad::OperationFailed(_) => {
            Err(ReadmeWritePlanError::PackageOperationFailed)
        }
    }
}

/// Reads the established destination's current bytes and assembles the plan. An
/// absent README is an ordinary creatable conventional document.
fn readme_write_plan<F: FileSystem>(
    package: &CurrentPackageCandidate,
    reference: ReadmeReference,
    text: &str,
    filesystem: &F,
) -> Result<ReadmeWritePlan, ReadmeWritePlanError<F::Error>> {
    let path = reference.location().absolute_path().to_path_buf();
    let kind =
        filesystem
            .symlink_metadata(&path)
            .map_err(|error| ReadmeWritePlanError::FileSystem {
                path: path.clone(),
                error,
            })?;
    let (prior_exists, prior_bytes) = match kind {
        None => (false, Vec::new()),
        Some(FileSystemEntryKind::File) => {
            let bytes = filesystem.read(&path).map_err(|_| {
                ReadmeWritePlanError::UnwritableDestination {
                    path: path.clone(),
                    reason: ReadmeUnavailableReason::Unreadable,
                }
            })?;
            (true, bytes)
        }
        // A directory or a link at the conventional path is not a document this
        // capability may replace, and it never converts or removes one.
        Some(_) => {
            return Err(ReadmeWritePlanError::UnwritableDestination {
                path,
                reason: ReadmeUnavailableReason::WrongKind,
            });
        }
    };

    let index_path = package.context().index().absolute_path().to_path_buf();
    let mut operations = Vec::new();
    if !prior_exists || prior_bytes != text.as_bytes() {
        operations.push(ReadmeWriteOperation {
            kind: ReadmeWriteOperationKind::ReplaceReadmeDocument,
            path: path.clone(),
        });
        // Only the Order README is projected into the derived index.
        if reference.kind() == ReadmeKind::OrderReadme {
            operations.push(ReadmeWriteOperation {
                kind: ReadmeWriteOperationKind::RefreshPackageIndex,
                path: index_path,
            });
        }
    }

    Ok(ReadmeWritePlan {
        root: package.context().root().absolute_path().to_path_buf(),
        reference,
        text: text.to_owned(),
        prior_exists,
        prior_bytes,
        prior_index: package.index().clone(),
        operations,
    })
}

/// The exact facts of one conventional README document. A README is prose, so the
/// only facts are whether the document exists and the text it holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeDocument {
    exists: bool,
    text: String,
}

impl ReadmeDocument {
    pub fn exists(&self) -> bool {
        self.exists
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadmeWriteResult {
    document: ReadmeDocument,
    index: PackageIndexRecord,
}

impl ReadmeWriteResult {
    /// The document as this write left it.
    pub fn document(&self) -> &ReadmeDocument {
        &self.document
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A README is exact user bytes with no schema, so there is nothing to validate and
/// no candidate validation failure is possible.
pub enum ReadmeCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum ReadmeWriteRejection<E> {
    /// The destination changed between planning and commit, so the plan's prior
    /// evidence is stale and the write would silently overwrite it.
    PreconditionsChanged { path: PathBuf },
    ReplacementFailed {
        error: ValidatedReplacementError<E, ReadmeCandidateFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadmeWriteWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

pub type ReadmeWriteReport<E> =
    MutationReport<ReadmeWritePlan, ReadmeWriteResult, ReadmeWriteRejection<E>, ReadmeWriteWarning>;

pub fn commit_readme_write<F: FileSystem>(
    plan: ReadmeWritePlan,
    filesystem: &F,
) -> ReadmeWriteReport<F::Error> {
    let path = plan.reference.location().absolute_path().to_path_buf();
    let observed = match filesystem.symlink_metadata(&path) {
        Ok(observed) => observed,
        Err(_) => return preconditions_changed(plan, path),
    };
    let current_bytes = match observed {
        None if plan.prior_exists => return preconditions_changed(plan, path),
        None => Vec::new(),
        Some(FileSystemEntryKind::File) if !plan.prior_exists => {
            return preconditions_changed(plan, path);
        }
        Some(FileSystemEntryKind::File) => match filesystem.read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return preconditions_changed(plan, path),
        },
        Some(_) => return preconditions_changed(plan, path),
    };
    if current_bytes != plan.prior_bytes {
        return preconditions_changed(plan, path);
    }

    if !plan.changes_document() {
        let result = ReadmeWriteResult {
            document: written_document(&plan),
            index: plan.prior_index.clone(),
        };
        return MutationReport::Unchanged { plan, result };
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
        |_| Ok::<(), ReadmeCandidateFailure>(()),
    ) {
        return MutationReport::Rejected {
            plan,
            rejection: ReadmeWriteRejection::ReplacementFailed { error },
        };
    }

    // Only the Order README is projected into the derived index, so a Package README
    // write is complete without touching it.
    if plan.reference.kind() == ReadmeKind::PackageReadme {
        let result = ReadmeWriteResult {
            document: written_document(&plan),
            index: plan.prior_index.clone(),
        };
        return MutationReport::Committed { plan, result };
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => {
            let result = ReadmeWriteResult {
                document: written_document(&plan),
                index: package.index().clone(),
            };
            MutationReport::Committed { plan, result }
        }
        PackageLoad::IndexReplacementFailed(failed) => {
            let warning = ReadmeWriteWarning::IndexRefreshFailed(failed.failure().clone());
            let result = ReadmeWriteResult {
                document: written_document(&plan),
                index: failed.canonical_evidence().index().clone(),
            };
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        PackageLoad::OperationFailed(failed) => {
            let warning = ReadmeWriteWarning::IndexRefreshFailed(failed.failure().clone());
            let result = result_without_refresh(&plan);
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        _ => {
            let result = result_without_refresh(&plan);
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![ReadmeWriteWarning::RefreshUnavailable],
            }
        }
    }
}

/// Only the Order README path reaches here; the Package README returns before the
/// refresh. What this write changed is the README text the index projects, not the
/// Order's canonical metadata, so the index goes stale for that reason.
fn result_without_refresh(plan: &ReadmeWritePlan) -> ReadmeWriteResult {
    ReadmeWriteResult {
        document: written_document(plan),
        index: plan
            .prior_index
            .after_committed_order_readme_change_without_refresh(),
    }
}

/// The document this write left behind. The staged candidate was validated byte for
/// byte before replacement, so the stored text is the supplied text and reporting it
/// costs no additional read of the document.
fn written_document(plan: &ReadmeWritePlan) -> ReadmeDocument {
    ReadmeDocument {
        exists: true,
        text: plan.text.clone(),
    }
}

fn preconditions_changed<E>(plan: ReadmeWritePlan, path: PathBuf) -> ReadmeWriteReport<E> {
    MutationReport::Rejected {
        plan,
        rejection: ReadmeWriteRejection::PreconditionsChanged { path },
    }
}
