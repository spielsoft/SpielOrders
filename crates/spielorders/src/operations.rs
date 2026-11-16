//! Stable evidence for failed filesystem operations.

use crate::{Failure, readme::Location};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileSystemOperation {
    Canonicalize,
    ObserveKind,
    Read,
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
    pub(crate) fn as_path(&self) -> &Path {
        match self {
            Self::Located(location) => &location.absolute_path,
            Self::Unlocated(path) => path,
        }
    }

    /// The `Location` this failure is about, where this application has one.
    pub(crate) fn location(&self) -> Option<&Location> {
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

/// A filesystem operation a capability needed and could not perform, as it is built up
/// before it becomes the [`Failure`] whether the filesystem denied access earns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FailedOperation {
    PackageAccessDenied(Box<OperationFailedProjection>),
    PackageOperationFailed(Box<OperationFailedProjection>),
    FilesystemUnsupported(Box<OperationFailedProjection>),
}

impl From<FailedOperation> for Failure {
    fn from(failed: FailedOperation) -> Self {
        match failed {
            FailedOperation::PackageAccessDenied(projection) => {
                Self::PackageAccessDenied(projection)
            }
            FailedOperation::PackageOperationFailed(projection) => {
                Self::PackageOperationFailed(projection)
            }
            FailedOperation::FilesystemUnsupported(projection) => {
                Self::FilesystemUnsupported(projection)
            }
        }
    }
}

impl FailedOperation {
    pub(crate) fn filesystem<F: crate::ports::FileSystem>(
        path: impl Into<FailedPath>,
        error: &F::Error,
        filesystem: &F,
    ) -> Self {
        let projection = Box::new(OperationFailedProjection {
            root: None,
            path: path.into(),
        });
        if filesystem.is_access_denied(error) {
            Self::PackageAccessDenied(projection)
        } else if filesystem.is_unsupported(error) {
            Self::FilesystemUnsupported(projection)
        } else {
            Self::PackageOperationFailed(projection)
        }
    }

    /// The same, for an operation on `path` inside the Root at `root`, so the failure
    /// names both in that Root's spelling.
    pub(crate) fn inside<F: crate::ports::FileSystem>(
        root: &Path,
        path: PathBuf,
        error: &F::Error,
        filesystem: &F,
    ) -> Self {
        Self::filesystem(FailedPath::of(&path), error, filesystem).within(root)
    }

    /// The same failure, met inside the Root at `root`, naming its path in that Root's
    /// spelling.
    pub(crate) fn within(mut self, root: &Path) -> Self {
        let projection = self.projection_mut();
        projection.path = FailedPath::inside(root, projection.path.as_path().to_path_buf());
        self.under(&crate::readme::located_in_package(root, root.to_path_buf()))
    }

    /// An operation whose answer this application could not use, so the filesystem
    /// reported no error of its own.
    pub(crate) fn unusable_answer(path: impl Into<FailedPath>) -> Self {
        Self::PackageOperationFailed(Box::new(OperationFailedProjection {
            root: None,
            path: path.into(),
        }))
    }

    /// The same failure, met inside `root`, where it was not already placed in one.
    pub(crate) fn under(mut self, root: &Location) -> Self {
        let projection = self.projection_mut();
        if projection.root.is_none() {
            projection.root = Some(root.clone());
        }
        self
    }

    /// The same failure, naming `path` as the one it failed on.
    pub(crate) fn at(mut self, path: impl Into<FailedPath>) -> Self {
        self.projection_mut().path = path.into();
        self
    }

    pub(crate) fn projection(&self) -> &OperationFailedProjection {
        match self {
            Self::PackageAccessDenied(projection)
            | Self::PackageOperationFailed(projection)
            | Self::FilesystemUnsupported(projection) => projection,
        }
    }

    fn projection_mut(&mut self) -> &mut OperationFailedProjection {
        match self {
            Self::PackageAccessDenied(projection)
            | Self::PackageOperationFailed(projection)
            | Self::FilesystemUnsupported(projection) => projection,
        }
    }
}

/// What a failed filesystem operation established: the Root it was met inside, where
/// one had resolved, and the path it failed on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationFailedProjection {
    /// The Root the operation was met inside. `None` where nothing had resolved one.
    pub root: Option<Location>,
    /// The path the operation failed on, named however this application can name it.
    pub path: FailedPath,
}

/// Refuses `selected` when it is a link, before anything behind it is opened. A path that
/// cannot be observed is not refused here: what stands there is the capability's to answer.
pub(crate) fn refuse_linked_root<F: crate::ports::FileSystem>(
    selected: &Path,
    filesystem: &F,
) -> Result<(), Failure> {
    // The OS follows a link spelled with a trailing separator; observe the entry itself.
    let entry: std::path::PathBuf = selected.components().collect();
    match filesystem.symlink_metadata(&entry) {
        Ok(Some(crate::ports::FileSystemEntryKind::SymbolicLink)) => Err(Failure::RootIsALink {
            root: FailedPath::Unlocated(selected.to_path_buf()),
        }),
        _ => Ok(()),
    }
}
