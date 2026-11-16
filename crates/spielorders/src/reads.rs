//! Every read a caller asks of a Package, each answering with owned values a caller can
//! hold across threads. The long-running ones take a cancellation; a read that settles
//! after its caller cancelled answers `cancelled`, and stops at its next effect.
//!
//! Decision sources: Define the Rust CLI capability contract; Define lookup and search
//! behavior; Define integrity, validation, and repair boundaries; Effect and adapter
//! boundaries.

use crate::{
    Answer, Failure, OrderRef, Scope, VendorRef,
    canonical::{self, Package},
    diagnosis::{self, Breadth, Diagnosis},
    order_reads::{self, OrderRead},
    ports::{
        Cancellation, DirectoryEntry, Effects, FileChangeObservation, FileSystem,
        FileSystemEntryKind, NewFileLinkOutcome, RenameOutcome,
    },
    readme::ReadmeRead,
    vendor_reads::{self, VendorRead},
};
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// Opens the Package at `root`: loads and checks it, assesses its stored Package Index
/// without writing anything, and reads its README. The desktop's first read.
pub fn open<E: Effects>(
    root: &Path,
    effects: &E,
    cancellation: &dyn Cancellation,
) -> Answer<Package> {
    cancellable(effects, cancellation, |filesystem| {
        canonical::open(root, filesystem).into()
    })
}

/// Reads the Order `order` names, which must be healthy, with the findings local to it.
pub fn read_order<E: Effects>(order: &OrderRef, effects: &E) -> Answer<OrderRead> {
    let filesystem = effects.file_system();
    let OrderRef::Directory(directory) = order;
    order_reads::read_order(directory, filesystem)
}

/// Reads the Vendor `vendor` names in the Package at `root`, Hidden Vendors included.
pub fn read_vendor<E: Effects>(root: &Path, vendor: &VendorRef, effects: &E) -> Answer<VendorRead> {
    vendor_reads::read_vendor(root, vendor, effects.file_system()).into()
}

/// Reads the conventional README of `scope`. A Package's answers whatever state its
/// records are in; an Order's needs a healthy Order.
pub fn read_readme<E: Effects>(scope: &Scope, effects: &E) -> Answer<ReadmeRead> {
    match scope {
        Scope::Package(root) => canonical::read_package_readme(root, effects.file_system()).into(),
        Scope::Order(order) => read_order(order, effects).map(|read| read.readme),
    }
}

/// Validates `scope`: a whole Package, every Order Directory and cross-record constraint
/// included, or one Order. A scope whose findings include an error is invalid, and refuses
/// as `package_invalid` or `order_invalid` carrying its diagnosis.
pub fn validate<E: Effects>(
    scope: &Scope,
    effects: &E,
    cancellation: &dyn Cancellation,
) -> Answer<Diagnosis> {
    cancellable(effects, cancellation, |filesystem| {
        Answer::from(diagnosed(scope, Breadth::Whole, filesystem).and_then(
            |diagnosis| match scope {
                _ if diagnosis.is_valid() => Ok(diagnosis),
                Scope::Package(_) => Err(Failure::PackageInvalid(Box::new(diagnosis))),
                Scope::Order(_) => Err(Failure::OrderInvalid(Box::new(diagnosis))),
            },
        ))
    })
}

/// Diagnoses the repairs `scope` offers: a Package's own artifacts, or one Order. A
/// diagnosis answers whatever it found; an invalid scope is what it is for.
pub fn diagnose<E: Effects>(
    scope: &Scope,
    effects: &E,
    cancellation: &dyn Cancellation,
) -> Answer<Diagnosis> {
    cancellable(effects, cancellation, |filesystem| {
        diagnosed(scope, Breadth::OwnArtifacts, filesystem).into()
    })
}

/// The diagnosis of `scope` at `breadth`. An Order named by ID is the Directory of the
/// Order it resolves to, and a Directory is diagnosed damaged or not.
fn diagnosed<F: FileSystem>(
    scope: &Scope,
    breadth: Breadth,
    filesystem: &F,
) -> Result<Diagnosis, Failure> {
    match scope {
        Scope::Package(root) => diagnosis::of_package(root, breadth, filesystem),
        Scope::Order(order) => diagnosis::of_order(&order.any_directory(filesystem)?, filesystem),
    }
}

/// `read`'s answer over a filesystem that refuses every effect once `cancellation` is
/// set, or `cancelled` where the caller cancelled before it settled.
fn cancellable<E: Effects, T>(
    effects: &E,
    cancellation: &dyn Cancellation,
    read: impl FnOnce(&Cancellable<'_, E::FileSystem>) -> Answer<T>,
) -> Answer<T> {
    let answer = read(&Cancellable {
        inner: effects.file_system(),
        cancellation,
    });
    if cancellation.is_cancelled() {
        Err(Failure::Cancelled).into()
    } else {
        answer
    }
}

struct Cancellable<'a, F> {
    inner: &'a F,
    cancellation: &'a dyn Cancellation,
}

enum Interrupted<E> {
    Cancelled,
    Failed(E),
}

impl<E: fmt::Display> fmt::Display for Interrupted<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::Failed(error) => error.fmt(formatter),
        }
    }
}

impl<F: FileSystem> Cancellable<'_, F> {
    fn unless_cancelled<T>(
        &self,
        effect: impl FnOnce(&F) -> Result<T, F::Error>,
    ) -> Result<T, Interrupted<F::Error>> {
        if self.cancellation.is_cancelled() {
            return Err(Interrupted::Cancelled);
        }
        effect(self.inner).map_err(Interrupted::Failed)
    }
}

impl<F: FileSystem> FileSystem for Cancellable<'_, F> {
    type Error = Interrupted<F::Error>;

    fn is_access_denied(&self, error: &Self::Error) -> bool {
        matches!(error, Interrupted::Failed(error) if self.inner.is_access_denied(error))
    }

    fn is_unsupported(&self, error: &Self::Error) -> bool {
        matches!(error, Interrupted::Failed(error) if self.inner.is_unsupported(error))
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.unless_cancelled(|inner| inner.read(path))
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.write_new(path, contents))
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.sync_file(path))
    }

    fn sync_file_contents(&self, path: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.sync_file_contents(path))
    }

    fn remembered_package(&self, root: &Path) -> Option<crate::canonical::CurrentPackageCandidate> {
        self.inner.remembered_package(root)
    }

    fn remember_package(&self, package: &crate::canonical::CurrentPackageCandidate) {
        self.inner.remember_package(package);
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.replace_file(source, destination))
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.remove_file(path))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.unless_cancelled(|inner| inner.canonicalize(path))
    }

    fn entry_name(&self, path: &Path) -> Result<Option<std::ffi::OsString>, Self::Error> {
        self.unless_cancelled(|inner| inner.entry_name(path))
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.unless_cancelled(|inner| inner.symlink_metadata(path))
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.unless_cancelled(|inner| inner.observe_file_change(path))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.unless_cancelled(|inner| inner.read_directory(path))
    }

    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.create_directory(path))
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.unless_cancelled(|inner| inner.remove_directory(path))
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        self.unless_cancelled(|inner| inner.link_new_file(source, destination))
    }

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error> {
        self.unless_cancelled(|inner| inner.rename_new(source, destination))
    }
}
