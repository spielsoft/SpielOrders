//! Reading one Order from the Order Directory alone, through its own locator.
//!
//! **Why this exists rather than a caller assembling it.** Before this module the only
//! public single-Order read was `order_edits::list_order_items`, whose whole body
//! discards the `CurrentPackageCandidate` that carries an Order's diagnostics;
//! `HealthyOrder` exposes paths, document, totals, and README text and no diagnostics;
//! and `order_edits::resolve_order` is `pub(crate)`, so no adapter could reach the
//! candidate either. An adapter that wanted this Order's diagnostics had no route but to
//! load the Package itself and pick the groups out — which is the adapter constructing a
//! grouping, and a second place deciding which diagnostics belong to an Order.
//!
//! **No ancestor is searched.** The route to the Package is the Order record's own
//! required `package_root` locator: a normalized relative path of parent-only segments,
//! so the Root is *computed* from it rather than hunted for. That is the whole reason the
//! locator is in the version-1 schema, and it is what makes an Order Directory
//! self-locating under a configured Orders Directory nested several levels below the
//! Root.
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
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, DamagedPackageProjection, HealthyOrder,
        ManagedContext, OperationFailedProjection, OrderPaths, UnmanagedPackage, UpgradeProjection,
        inspect_canonical_package,
    },
    metadata::{OrderDocument, OrderId, PackageRoot, Vendor, order_shape_field},
    operations::{FailedPath, FileSystemOperation, OperationFailure},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{Diagnostic, ReadmeRead, read_readme},
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
    context: ManagedContext,
    order: HealthyOrder,
    vendor: Option<Vendor>,
    readme: ReadmeRead,
    diagnostics: Vec<Diagnostic>,
}

impl OrderRead {
    /// The managed paths of the Package this Order belongs to.
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    /// The Order itself: its paths, its record, its calculated Item Totals and Order
    /// Total, and its README text.
    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    /// The Vendor this Order's record resolves to, Hidden Vendors included. `None` when
    /// the record names none.
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    /// This Order's conventional README, read.
    ///
    /// **Read here rather than left to the caller, because the load does not separate the
    /// one fact a caller needs.** `HealthyOrder::readme_text` is `Some("")` both for a
    /// README that is absent and for one that is present and empty, so existence cannot
    /// be recovered from it — and a caller that read the document itself would be
    /// reaching past this read for a fact about the Order it is reading. Its diagnostics
    /// are the same ones the load already grouped with this Order, so they reach a caller
    /// through [`Self::diagnostics`] rather than twice.
    pub fn readme(&self) -> &ReadmeRead {
        &self.readme
    }

    /// Every diagnostic the load assigned to this Order, in the core's own order.
    ///
    /// Flat rather than grouped, because the subject is one Order: a grouping over one
    /// Order Directory would be a structure with exactly one place for everything in it.
    /// The Package-level grouping is still what produced these — `diagnostic_groups`
    /// keyed on this Order's own directory and on the documents under it — so a caller
    /// neither constructs a grouping nor decides which diagnostics are an Order's.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Why one Order could not be read.
///
/// Every variant is a distinct answer with a distinct action behind it, and the four
/// Package-load states travel as the core's own projections so that an Order command
/// classifies a Package exactly as a Package-level command does rather than folding
/// every one of them into "unreadable".
#[derive(Debug, PartialEq)]
pub enum OrderReadFailure {
    /// The exact path given is not an Order Directory: no Order record stands under it.
    /// No ancestor was consulted, and neither was any descendant — a SpielOrders Root
    /// passed to an Order command lands here rather than being reinterpreted as one.
    NotAnOrderDirectory { directory: FailedPath },
    /// The exact path given is not an ordinary directory at all: nothing stands there, or
    /// what does is a file, a link, or something else a Package has no notion of.
    ///
    /// **Its own variant rather than the one above**, because the sentence differs: an
    /// Order Directory that holds no record is a directory, and telling a caller who named
    /// a file that it "holds no Order record" describes a place that cannot hold one. It is
    /// the same answer the diagnosing capability gives, which is what keeps one wrong-kind
    /// root from reaching two codes across the `order` family.
    RootNotADirectory { directory: FailedPath },
    /// The Order record could not be read as one. `field` names the first field at fault
    /// where the record was still JSON to inspect, and is `None` where it was not.
    DamagedRecord {
        directory: FailedPath,
        field: Option<String>,
    },
    /// The record's own technical locator is missing or malformed, so this Order names
    /// no Package that can be reached from the directory at all.
    LocatorUnusable { directory: FailedPath },
    /// The locator is well formed and the place it names holds no such Order, so it is
    /// stale or belongs to another Package. `package_root` is the locator as stored.
    LocatorReachesNoPackageHoldingThisOrder {
        directory: FailedPath,
        package_root: String,
    },
    /// This Order Directory is one of its Package's Damaged Orders. Its record could not
    /// be read or did not validate, so it has no facts to report; `diagnostics` is the
    /// diagnosis the load quarantined at this Order's own scope.
    DamagedOrder {
        context: Box<ManagedContext>,
        paths: Box<OrderPaths>,
        order_id: Option<OrderId>,
        diagnostics: Vec<Diagnostic>,
    },
    /// The Package holds no Order carrying the requested Order ID.
    OrderIdNotFound {
        context: Box<ManagedContext>,
        order_id: String,
    },
    /// The exact path given is a resolved directory and is not a SpielOrders Root.
    UnmanagedPackage(Box<UnmanagedPackage>),
    /// The Package could be read and not trusted. `root` is the best name this read has
    /// for the Root it loaded, which is the only thing left to report where the damage
    /// left the Package with no managed paths of its own.
    DamagedPackage {
        root: FailedPath,
        projection: Box<DamagedPackageProjection>,
    },
    /// The Package is mid-upgrade or awaiting one.
    UpgradePending(Box<UpgradeProjection>),
    /// The Package could not be read past its Root.
    PackageOperationFailed(Box<OperationFailedProjection>),
    /// A filesystem observation this read needed failed.
    Operation(Box<OperationFailure>),
}

/// Reads the Order whose Directory is named exactly, reaching its Package through the
/// Order's own locator.
pub fn read_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<OrderRead, OrderReadFailure> {
    // The selected path's own kind first. Joining the record's name onto a file yields a
    // path that cannot exist, and a filesystem asked about one answers that the operation
    // failed — so a wrong-kind root reached this read as a Package it could not read,
    // naming an internal operation and a path under a file. What kind of thing was named
    // is the answer, and it is the one the diagnosing capability already gives.
    match filesystem.symlink_metadata(order_directory) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        Ok(_) => {
            return Err(OrderReadFailure::RootNotADirectory {
                directory: as_given(order_directory),
            });
        }
        Err(_) => {
            return Err(operation(FileSystemOperation::ObserveKind, order_directory));
        }
    }
    let record = order_directory.join(".spielorders").join("order.json");
    match filesystem.symlink_metadata(&record) {
        Ok(Some(FileSystemEntryKind::File)) => {}
        // Nothing stands where an Order Directory keeps its record, so this directory is
        // not one. A directory or a link *does* stand there in the other arm, which is a
        // record this read cannot use rather than an absent one.
        Ok(None) => {
            return Err(OrderReadFailure::NotAnOrderDirectory {
                directory: as_given(order_directory),
            });
        }
        Ok(Some(_)) => {
            return Err(OrderReadFailure::DamagedRecord {
                directory: as_given(order_directory),
                field: None,
            });
        }
        Err(_) => return Err(operation(FileSystemOperation::ObserveKind, &record)),
    }
    let Ok(bytes) = filesystem.read(&record) else {
        return Err(OrderReadFailure::DamagedRecord {
            directory: as_given(order_directory),
            field: None,
        });
    };
    let document = match OrderDocument::from_json_bytes(&bytes) {
        Ok(document) => document,
        Err(_) => {
            return Err(OrderReadFailure::DamagedRecord {
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
        return Err(OrderReadFailure::LocatorUnusable {
            directory: as_given(order_directory),
        });
    };
    let locator = locator.to_owned();

    let Ok(directory) = filesystem.canonicalize(order_directory) else {
        return Err(operation(
            FileSystemOperation::Canonicalize,
            order_directory,
        ));
    };
    // The locator is a normalized relative path of parent-only segments, so the Root is
    // this many ancestors up and nothing searches for it. `nth` yields `None` where the
    // locator reaches past the filesystem root, which is a locator that leads nowhere.
    let Some(root) = directory
        .ancestors()
        .nth(locator.split('/').count())
        .map(Path::to_path_buf)
    else {
        return Err(OrderReadFailure::LocatorReachesNoPackageHoldingThisOrder {
            directory: FailedPath::of(&directory),
            package_root: locator,
        });
    };

    let package = match loaded(&root, inspect_canonical_package(&root, filesystem)) {
        Ok(package) => package,
        // A locator that names something which is not a SpielOrders Root at all is a
        // locator that does not reach this Order's Package, which is the answer a caller
        // can act on: nothing about the directory it named is a Package problem.
        Err(OrderReadFailure::UnmanagedPackage(_)) => {
            return Err(OrderReadFailure::LocatorReachesNoPackageHoldingThisOrder {
                directory: FailedPath::of(&directory),
                package_root: locator,
            });
        }
        Err(failure) => return Err(failure),
    };
    match found_at(&package, &directory, filesystem) {
        Some(found) => found,
        // The locator resolved to a Package this Order is not in, which is the stale or
        // mismatched case: the Order Directory was moved by hand, or its Orders Directory
        // was reconfigured to a different depth, and the stored locator now leads
        // somewhere else.
        None => Err(OrderReadFailure::LocatorReachesNoPackageHoldingThisOrder {
            directory: FailedPath::of(&directory),
            package_root: locator,
        }),
    }
}

/// Reads the one Order of the Package at `package_root` carrying `order_id`.
///
/// The Order ID is an opaque identity with no location in it, so the only route to an
/// Order it names is the Package's own loaded set. Supplying that here is what keeps a
/// caller from scanning Order Directories for a matching record.
pub fn read_order_by_id<F: FileSystem>(
    package_root: &Path,
    order_id: &str,
    filesystem: &F,
) -> Result<OrderRead, OrderReadFailure> {
    let package = loaded(
        package_root,
        inspect_canonical_package(package_root, filesystem),
    )?;
    if let Some(index) = package
        .healthy_orders()
        .iter()
        .position(|order| order.document().order_id.as_str() == order_id)
    {
        return read_of(&package, index, filesystem);
    }
    // A Damaged Order whose identity the load could still trust answers as a Damaged
    // Order rather than as an absent one: the Order exists and its record is the problem,
    // and reporting it missing would send a caller looking for the wrong thing.
    if let Some(damaged) = package
        .damaged_orders()
        .iter()
        .find(|order| order.order_id().is_some_and(|id| id.as_str() == order_id))
    {
        return Err(damaged_order(&package, damaged.paths(), damaged.order_id()));
    }
    Err(OrderReadFailure::OrderIdNotFound {
        context: Box::new(package.context().clone()),
        order_id: order_id.to_owned(),
    })
}

/// The reading load's one current-candidate answer, with every other state typed.
///
/// `inspect_canonical_package` rather than `open_package`, because the opening load
/// durably replaces the derived Package Index and `docs/architecture/durable-writes.md`
/// reserves the rebuilding load for a place a canonical write has already happened.
/// Reading one Order writes nothing.
fn loaded(
    root: &Path,
    load: CanonicalPackageLoad,
) -> Result<CurrentPackageCandidate, OrderReadFailure> {
    match load {
        CanonicalPackageLoad::CurrentCandidate(package) => Ok(package),
        CanonicalPackageLoad::Unmanaged(package) => {
            Err(OrderReadFailure::UnmanagedPackage(Box::new(package)))
        }
        CanonicalPackageLoad::DiagnosticOnly(package) => Err(OrderReadFailure::DamagedPackage {
            root: FailedPath::of(root),
            projection: Box::new(package),
        }),
        CanonicalPackageLoad::UpgradePending(package) => {
            Err(OrderReadFailure::UpgradePending(Box::new(package)))
        }
        CanonicalPackageLoad::OperationFailed(package) => {
            Err(OrderReadFailure::PackageOperationFailed(Box::new(package)))
        }
    }
}

/// This Order Directory's answer within a Package that holds it, or `None` where the
/// Package holds no Order at that directory at all.
fn found_at<F: FileSystem>(
    package: &CurrentPackageCandidate,
    directory: &Path,
    filesystem: &F,
) -> Option<Result<OrderRead, OrderReadFailure>> {
    if let Some(index) = package
        .healthy_orders()
        .iter()
        .position(|order| order.paths().directory().absolute_path() == directory)
    {
        return Some(read_of(package, index, filesystem));
    }
    package
        .damaged_orders()
        .iter()
        .find(|order| order.paths().directory().absolute_path() == directory)
        .map(|damaged| Err(damaged_order(package, damaged.paths(), damaged.order_id())))
}

fn read_of<F: FileSystem>(
    package: &CurrentPackageCandidate,
    index: usize,
    filesystem: &F,
) -> Result<OrderRead, OrderReadFailure> {
    let order = package.healthy_orders()[index].clone();
    let readme = read_readme(order.paths().readme(), filesystem)
        .map_err(|failure| OrderReadFailure::Operation(Box::new(failure)))?;
    Ok(OrderRead {
        context: package.context().clone(),
        diagnostics: local_diagnostics(package, order.paths()),
        vendor: package.vendor_for_order(index).cloned(),
        readme,
        order,
    })
}

fn damaged_order(
    package: &CurrentPackageCandidate,
    paths: &OrderPaths,
    order_id: Option<&OrderId>,
) -> OrderReadFailure {
    OrderReadFailure::DamagedOrder {
        context: Box::new(package.context().clone()),
        diagnostics: local_diagnostics(package, paths),
        paths: Box::new(paths.clone()),
        order_id: order_id.cloned(),
    }
}

/// Every diagnostic the load assigned to this one Order.
///
/// A group's location is the thing it is about, so an Order's own groups are those
/// located at its Directory or at a document inside it — the quarantine group keyed on
/// the Directory itself, and the Order README's group keyed on the README. Selecting by
/// containment rather than by scope is what keeps the Document-scoped README group with
/// the Order it belongs to instead of leaving it to a Package-level reader.
fn local_diagnostics(package: &CurrentPackageCandidate, paths: &OrderPaths) -> Vec<Diagnostic> {
    let directory = paths.directory().absolute_path();
    package
        .diagnostic_groups()
        .iter()
        .filter(|group| group.location().absolute_path().starts_with(directory))
        .flat_map(|group| group.diagnostics().iter().cloned())
        .collect()
}

/// A path exactly as the caller supplied it.
///
/// Deliberately not `FailedPath::of`, which promotes an absolute path to a `Location`:
/// nothing has resolved this path yet, so claiming a resolved spelling for a string that
/// merely looks absolute would say the resolution happened when it did not.
fn as_given(path: &Path) -> FailedPath {
    FailedPath::Unlocated(path.to_path_buf())
}

fn operation(operation: FileSystemOperation, path: &Path) -> OrderReadFailure {
    OrderReadFailure::Operation(Box::new(OperationFailure::filesystem(
        operation,
        FailedPath::of(path),
        Vec::new(),
    )))
}
