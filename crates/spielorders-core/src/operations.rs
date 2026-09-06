//! Stable evidence for filesystem and Package Index operation failures.

use crate::readme::{EvidenceScope, Location};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OperationFailureCode {
    FilesystemOperationFailed,
    PackageIndexReplacementFailed,
}

impl OperationFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FilesystemOperationFailed => "filesystem_operation_failed",
            Self::PackageIndexReplacementFailed => "package_index_replacement_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileSystemOperation {
    Canonicalize,
    ObserveKind,
    ListDirectory,
    Read,
}

impl FileSystemOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canonicalize => "canonicalize",
            Self::ObserveKind => "observe_kind",
            Self::ListDirectory => "list_directory",
            Self::Read => "read",
        }
    }
}

/// The path an operation failed on, in the only spelling this application has for it.
///
/// Almost every failure here happens *to* a path this application already expresses as
/// a `Location`, and names it. Resolving a caller-supplied path is the one operation
/// that can fail before there is anything to name: `Location` holds an absolute path by
/// definition, so a relative path the filesystem declines to canonicalize has no
/// `Location` at all. Nothing here can supply one — the filesystem refused the
/// resolution, and no port in this crate reports a working directory to join it onto.
///
/// So such a failure names the path back exactly as the caller wrote it. That is
/// strictly less than a `Location`, and saying so in the type is the point: a caller
/// can report which root it was asked about without this application having to invent
/// an absolute spelling for it, and a reader cannot mistake the caller's own text for a
/// path this application resolved.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FailedPath {
    /// A path this application expresses as a `Location`. Existing is a separate
    /// question: a `Location` names where something would be, and an operation on it
    /// fails for every ordinary filesystem reason.
    Located(Location),
    /// A path exactly as a caller supplied it, for which this application has no
    /// `Location`.
    Unlocated(PathBuf),
}

impl FailedPath {
    /// The best name this application has for `path`: its `Location` where `path` is
    /// one this application can express, and otherwise `path` exactly as it arrived.
    pub(crate) fn of(path: &Path) -> Self {
        match Location::absolute(path.to_path_buf()) {
            Ok(location) => Self::Located(location),
            Err(_) => Self::Unlocated(path.to_path_buf()),
        }
    }

    /// The same, for a path a capability reached inside a Root it had already resolved.
    ///
    /// **[`Self::of`] cannot carry the Package-relative spelling** — it names an absolute
    /// path and nothing else — and a response owes that spelling for every path a Package
    /// surrounds, in a refusal exactly as in a success. An operation that failed on a path
    /// this Package has no spelling for, or on one the filesystem never resolved, falls
    /// back to [`Self::of`], which is the honest answer for it.
    pub(crate) fn inside(root: &Path, path: PathBuf) -> Self {
        if path.is_absolute() {
            Self::Located(crate::readme::located_in_package(root, path))
        } else {
            Self::Unlocated(path)
        }
    }

    /// The path this failure is about. Always answerable, because naming the path is
    /// the least a failure about one can do.
    pub fn as_path(&self) -> &Path {
        match self {
            Self::Located(location) => location.absolute_path(),
            Self::Unlocated(path) => path,
        }
    }

    /// The `Location` this failure is about, where this application has one.
    pub fn location(&self) -> Option<&Location> {
        match self {
            Self::Located(location) => Some(location),
            Self::Unlocated(_) => None,
        }
    }
}

impl From<Location> for FailedPath {
    fn from(location: Location) -> Self {
        Self::Located(location)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OperationEvidence {
    FileSystem {
        operation: FileSystemOperation,
    },
    PackageIndexReplacement {
        phase: PackageIndexReplacementPhase,
        cleanup_failure: Option<PackageIndexReplacementPhase>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageIndexReplacementPhase {
    WriteStage,
    SyncStage,
    Replace,
    RemoveStage,
    /// The index on disk stopped matching the one this load assessed, so a concurrent
    /// writer changed it. Nothing was written, and rebuilding over them would have
    /// discarded their index.
    DestinationChanged,
    /// The index could not be read to compare it against what this load assessed, so
    /// whether it still matches is unknown. Nothing was written.
    DestinationUnreadable,
}

impl PackageIndexReplacementPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DestinationChanged => "destination_changed",
            Self::DestinationUnreadable => "destination_unreadable",
            Self::WriteStage => "write_stage",
            Self::SyncStage => "sync_stage",
            Self::Replace => "replace",
            Self::RemoveStage => "remove_stage",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OperationFailure {
    code: OperationFailureCode,
    scope: EvidenceScope,
    primary_path: FailedPath,
    related_locations: Vec<Location>,
    evidence: OperationEvidence,
}

impl OperationFailure {
    pub(crate) fn filesystem(
        operation: FileSystemOperation,
        primary_path: impl Into<FailedPath>,
        related_locations: Vec<Location>,
    ) -> Self {
        Self {
            code: OperationFailureCode::FilesystemOperationFailed,
            scope: EvidenceScope::Operation,
            primary_path: primary_path.into(),
            related_locations,
            evidence: OperationEvidence::FileSystem { operation },
        }
    }

    pub(crate) fn package_index_replacement(
        primary_location: Location,
        related_locations: Vec<Location>,
        phase: PackageIndexReplacementPhase,
        cleanup_failure: Option<PackageIndexReplacementPhase>,
    ) -> Self {
        Self {
            code: OperationFailureCode::PackageIndexReplacementFailed,
            scope: EvidenceScope::Operation,
            primary_path: primary_location.into(),
            related_locations,
            evidence: OperationEvidence::PackageIndexReplacement {
                phase,
                cleanup_failure,
            },
        }
    }

    pub(crate) fn observe_kind(location: &Location) -> Self {
        Self::filesystem(
            FileSystemOperation::ObserveKind,
            location.clone(),
            Vec::new(),
        )
    }

    pub fn code(&self) -> OperationFailureCode {
        self.code
    }

    pub fn scope(&self) -> EvidenceScope {
        self.scope
    }

    /// The path this operation failed on, named however this application can name it.
    pub fn primary_path(&self) -> &FailedPath {
        &self.primary_path
    }

    /// The `Location` this operation failed on. `None` where the failure was the
    /// resolution itself, and [`FailedPath::Unlocated`] carries the caller's own text.
    pub fn primary_location(&self) -> Option<&Location> {
        self.primary_path.location()
    }

    pub fn related_locations(&self) -> &[Location] {
        &self.related_locations
    }

    pub fn evidence(&self) -> &OperationEvidence {
        &self.evidence
    }
}
