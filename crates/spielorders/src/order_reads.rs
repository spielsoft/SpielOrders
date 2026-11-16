//! Reading one Order from the Order Directory alone, through its own locator.
//!
//! **No ancestor is searched.** The route to the Package is the Order record's own
//! required `package_root` locator, so the Root is *computed* from it rather than hunted
//! for.
//!
//! **A broken locator is named as such.** The locator is the one route this read has, so
//! when it is missing, malformed, stale, or mismatched, the failure says so and carries
//! the locator itself — as the field at fault where the record would not parse, and as
//! the stored value where it parsed and led somewhere that holds no such Order. The
//! `OrderPackageLocatorMismatch` *diagnostic* is a different thing and is not reachable
//! from here: it is emitted only by a load rooted at the Package's real root, which a
//! caller holding a stale locator has no way to name.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record
//! and lifecycle contract; Define durable metadata schemas and version evolution.

use crate::{
    Answer, Failure,
    canonical::{CurrentPackageCandidate, HealthyOrder, ManagedContext, OrderPaths, load_package},
    metadata::{OrderDocument, OrderId, PackageRoot, Vendor, order_shape_field},
    operations::{FailedOperation, FailedPath, refuse_linked_root},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{Diagnostic, ReadmeRead, read_document},
};
use std::path::Path;

/// One Order's complete current read, together with the diagnostics local to it.
///
/// The Vendor travels with the Order because resolving one is the Package's rule and the
/// candidate that holds both is not exposed here: `vendor_for_order` searches the
/// complete Vendor list, so a Hidden Vendor an Order references resolves exactly as any
/// other does, and a caller that looked one up itself would be a second place resolving
/// a Vendor.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderRead {
    /// The managed paths of the Package this Order belongs to.
    pub context: ManagedContext,
    /// The Order itself: its paths, its record, its calculated Item Totals and Order
    /// Total, and its README text.
    pub order: HealthyOrder,
    /// The Vendor this Order's record resolves to, Hidden Vendors included. `None` when
    /// the record names none.
    pub vendor: Option<Vendor>,
    /// This Order's conventional README, read.
    ///
    /// **Read here rather than left to the caller, because the load does not separate the
    /// one fact a caller needs.** `HealthyOrder::readme_text` is `Some("")` both for a
    /// README that is absent and for one that is present and empty, so existence cannot
    /// be recovered from it — and a caller that read the document itself would be
    /// reaching past this read for a fact about the Order it is reading. Its diagnostics
    /// are the same ones the load already grouped with this Order, so they reach a caller
    /// through [`Self::diagnostics`] rather than twice.
    pub readme: ReadmeRead,
    /// Every diagnostic the load assigned to this Order, in the core's own order.
    ///
    /// Flat rather than grouped, because the subject is one Order: a grouping over one
    /// Order Directory would be a structure with exactly one place for everything in it.
    /// The Package-level grouping is still what produced these — `diagnostic_groups`
    /// keyed on this Order's own directory and on the documents under it — so a caller
    /// neither constructs a grouping nor decides which diagnostics are an Order's.
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads the Order whose Directory is named exactly, reaching its Package through the
/// Order's own locator.
pub(crate) fn read_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Answer<OrderRead> {
    Answer::of(|| {
        refuse_linked_root(order_directory, filesystem)?;
        // The selected path's own kind first. Joining the record's name onto a file yields a
        // path that cannot exist, and a filesystem asked about one answers that the operation
        // failed, which would report a wrong-kind root as a Package this read could not read.
        match filesystem.symlink_metadata(order_directory) {
            Ok(Some(FileSystemEntryKind::Directory)) => {}
            Ok(found) => {
                return Err(Failure::NotAnOrderDirectory {
                    directory: as_given(order_directory),
                    found,
                });
            }
            Err(error) => {
                return Err(operation(order_directory, &error, filesystem));
            }
        }
        // A record reached through a linked metadata directory is not this Order's own;
        // asking about the record alone would follow the link.
        let metadata = order_directory.join(".spielorders");
        if let Ok(Some(kind)) = filesystem.symlink_metadata(&metadata)
            && kind != FileSystemEntryKind::Directory
        {
            return Err(Failure::DamagedRecord {
                directory: as_given(order_directory),
                field: None,
            });
        }
        let record = metadata.join("order.json");
        match filesystem.symlink_metadata(&record) {
            Ok(Some(FileSystemEntryKind::File)) => {}
            // Nothing stands where an Order Directory keeps its record, so this directory is
            // not one. A directory or a link *does* stand there in the other arm, which is a
            // record this read cannot use rather than an absent one.
            Ok(None) => {
                return Err(Failure::NotAnOrderDirectory {
                    directory: as_given(order_directory),
                    found: Some(FileSystemEntryKind::Directory),
                });
            }
            Ok(Some(_)) => {
                return Err(Failure::DamagedRecord {
                    directory: as_given(order_directory),
                    field: None,
                });
            }
            Err(error) => {
                return Err(operation(&record, &error, filesystem));
            }
        }
        // A record the filesystem would not let this read is not evidence of a damaged one.
        let bytes = match filesystem.read(&record) {
            Ok(bytes) => bytes,
            Err(error) if filesystem.is_access_denied(&error) => {
                return Err(operation(&record, &error, filesystem));
            }
            Err(_) => {
                return Err(Failure::DamagedRecord {
                    directory: as_given(order_directory),
                    field: None,
                });
            }
        };
        let document = match OrderDocument::from_json_bytes(&bytes) {
            Ok(document) => document,
            Err(_) => {
                return Err(Failure::DamagedRecord {
                    directory: as_given(order_directory),
                    field: order_shape_field(&bytes),
                });
            }
        };
        // The locator is this read's only route to a Package, so a record carrying none this
        // build can follow is a failure *about the locator* rather than one more damaged
        // record. The record itself parses — the locator carries its own outcome — so this is
        // the field's own verdict rather than a guess at which field failed the record.
        let Some(locator) = document.package_root.spelled().map(PackageRoot::as_str) else {
            return Err(Failure::LocatorUnusable {
                directory: as_given(order_directory),
            });
        };
        let locator = locator.to_owned();

        let directory = filesystem
            .canonicalize(order_directory)
            .map_err(|error| operation(order_directory, &error, filesystem))?;
        // The locator is a normalized relative path of parent-only segments, so the Root is
        // this many ancestors up and nothing searches for it. `nth` yields `None` where the
        // locator reaches past the filesystem root, which is a locator that leads nowhere.
        let Some(root) = directory
            .ancestors()
            .nth(locator.split('/').count())
            .map(Path::to_path_buf)
        else {
            return Err(Failure::LocatorReachesNoPackageHoldingThisOrder {
                directory: FailedPath::of(&directory),
                package_root: locator,
            });
        };

        let package = match load_package(&root, filesystem) {
            Ok(package) => package,
            // A locator that names something which is not a SpielOrders Root at all is a
            // locator that does not reach this Order's Package, which is the answer a caller
            // can act on: nothing about the directory it named is a Package problem.
            Err(Failure::UnmanagedPackage(_)) => {
                return Err(Failure::LocatorReachesNoPackageHoldingThisOrder {
                    directory: FailedPath::of(&directory),
                    package_root: locator,
                });
            }
            Err(unusable) => return Err(unusable),
        };
        match found_at(&package, &directory, filesystem) {
            Some(found) => found,
            // The locator resolved to a Package this Order is not in, which is the stale or
            // mismatched case: the Order Directory was moved by hand, or its Orders Directory
            // was reconfigured to a different depth, and the stored locator now leads
            // somewhere else.
            None => Err(Failure::LocatorReachesNoPackageHoldingThisOrder {
                directory: FailedPath::of(&directory),
                package_root: locator,
            }),
        }
    })
}

/// This Order Directory's answer within a Package that holds it, or `None` where the
/// Package holds no Order at that directory at all.
fn found_at<F: FileSystem>(
    package: &CurrentPackageCandidate,
    directory: &Path,
    filesystem: &F,
) -> Option<Result<OrderRead, Failure>> {
    if let Some(index) = package
        .healthy_orders
        .iter()
        .position(|order| order.paths.directory.absolute_path == directory)
    {
        return Some(read_of(package, index, filesystem));
    }
    package
        .damaged_orders
        .iter()
        .find(|order| order.paths.directory.absolute_path == directory)
        .map(|damaged| {
            Err(damaged_order(
                package,
                &damaged.paths,
                damaged.order_id.as_ref(),
            ))
        })
}

fn read_of<F: FileSystem>(
    package: &CurrentPackageCandidate,
    index: usize,
    filesystem: &F,
) -> Result<OrderRead, Failure> {
    let order = package.healthy_orders[index].clone();
    let readme = read_document(&order.paths.readme, filesystem)?;
    Ok(OrderRead {
        context: package.context.clone(),
        diagnostics: package.order_diagnostics(&order.paths),
        vendor: package.vendor_for_order(index).cloned(),
        readme,
        order,
    })
}

fn damaged_order(
    package: &CurrentPackageCandidate,
    paths: &OrderPaths,
    order_id: Option<&OrderId>,
) -> Failure {
    Failure::DamagedOrder {
        context: Box::new(package.context.clone()),
        diagnostics: package.order_diagnostics(paths),
        paths: Box::new(paths.clone()),
        order_id: order_id.cloned(),
    }
}

/// A path exactly as the caller supplied it.
///
/// Deliberately not `FailedPath::of`, which promotes an absolute path to a `Location`:
/// nothing has resolved this path yet, so claiming a resolved spelling for a string that
/// merely looks absolute would say the resolution happened when it did not.
fn as_given(path: &Path) -> FailedPath {
    FailedPath::Unlocated(path.to_path_buf())
}

fn operation<F: FileSystem>(path: &Path, error: &F::Error, filesystem: &F) -> Failure {
    FailedOperation::filesystem(FailedPath::of(path), error, filesystem).into()
}
