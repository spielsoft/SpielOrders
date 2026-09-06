//! Reading one Vendor of a Package by the Vendor Name that selects it.
//!
//! **Why this exists rather than a caller assembling it.** Both public Vendor reads take
//! a Vendor ID — `show_vendor` and `list_vendor_contacts` — while every Vendor command
//! line names a Vendor Name, and the Name comparison is the Package's own rule: trimmed,
//! case-insensitive, package-unique. An adapter that resolved a Name itself would be one
//! more place spelling that comparison, and the spelling is exactly the thing that must
//! not differ between the command that reads a Vendor and the ones that edit it.
//!
//! **The read carries its own load's diagnostics.** `VendorProjection` carries the Vendor
//! and its referencing Orders and no observations, so a caller that wanted them had no
//! route but to load the Package a second time and pick them out — which is a second load
//! answering one question, and a second place deciding which observations bear on a
//! Vendor read. They bear on it directly: a Damaged Order's record cannot be read, so it
//! names no Vendor and appears in no referencing summary, and the referencing collection
//! is therefore partial in exactly the cases the load has something to say.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, DamagedPackageProjection, ManagedContext,
        OperationFailedProjection, UnmanagedPackage, UpgradeProjection, inspect_canonical_package,
    },
    metadata::Vendor,
    operations::FailedPath,
    ports::FileSystem,
    readme::Diagnostic,
    vendors::{ReferencingOrderSummary, referencing_orders, vendor_by_name},
};
use std::path::Path;

/// One Vendor's complete current read, together with the diagnostics of the load that
/// produced it.
///
/// Every fact resolves live from the current Vendor record: an Order retains no Vendor
/// snapshot, so a renamed Vendor reads as its new Name here and on every Order that
/// references it.
#[derive(Clone, Debug, PartialEq)]
pub struct VendorRead {
    context: ManagedContext,
    vendor: Vendor,
    referencing_orders: Vec<ReferencingOrderSummary>,
    diagnostics: Vec<Diagnostic>,
}

impl VendorRead {
    /// The managed paths of the Package this Vendor belongs to.
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    /// The Vendor itself, with its shared facts, its ordered Contacts, and its
    /// visibility. A Hidden Vendor reads exactly as any other: hiding governs what a
    /// Package offers for new use and never what resolves.
    pub fn vendor(&self) -> &Vendor {
        &self.vendor
    }

    /// The live Orders whose records name this Vendor, in the load's own Order order.
    ///
    /// The healthy Orders only, because a Damaged Order's record could not be read and
    /// the Vendor it names is therefore unknown — the load's diagnostics are what say so.
    pub fn referencing_orders(&self) -> &[ReferencingOrderSummary] {
        &self.referencing_orders
    }

    /// Every diagnostic the load made, flat and in the core's own order.
    ///
    /// **Every group, unfiltered**, which is where this differs from reading one Order: an
    /// Order has a Directory, so its own observations are the ones located under it, while
    /// a Vendor has no directory to select by. Nor would a narrower selection be right —
    /// the Vendor record is one Package-level document and the Orders that reference it
    /// are spread across the Package, so an observation anywhere in it can be the reason
    /// this answer is partial.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Why one Vendor could not be read.
///
/// The four Package-load states travel as the core's own projections so that a Vendor
/// command classifies a Package exactly as a Package-level command does rather than
/// folding every one of them into "unreadable".
#[derive(Debug, PartialEq)]
pub enum VendorReadFailure {
    /// The Package holds Vendors and none of them carries this Vendor Name.
    VendorNameNotFound {
        context: Box<ManagedContext>,
        name: String,
    },
    /// The Package holds no Vendors at all, so no Name can resolve here.
    ///
    /// Typed apart from the variant above because the two have different actions behind
    /// them: one is a Name to check, the other a Vendor to create. A Vendor is never
    /// deleted, so an emptied-out Package is not how this arises — an unpopulated one is.
    PackageHasNoVendors {
        context: Box<ManagedContext>,
        name: String,
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
}

/// Reads the one Vendor of the Package at `selected_root` whose Vendor Name is `name`.
///
/// The comparison is the Package's own — trimmed and case-insensitive — and it lives in
/// [`crate::vendors`] beside the duplicate-Name rule that shares it, so a Name resolves
/// here exactly as it resolves for the mutations. Hidden Vendors are searched like any
/// other, because hiding governs what is offered and never what resolves.
pub fn read_vendor_by_name<F: FileSystem>(
    selected_root: &Path,
    name: &str,
    filesystem: &F,
) -> Result<VendorRead, VendorReadFailure> {
    let package = loaded(
        selected_root,
        inspect_canonical_package(selected_root, filesystem),
    )?;
    let Some(vendor) = vendor_by_name(name, package.vendors()) else {
        let context = Box::new(package.context().clone());
        let name = name.to_owned();
        return Err(if package.vendors().is_empty() {
            VendorReadFailure::PackageHasNoVendors { context, name }
        } else {
            VendorReadFailure::VendorNameNotFound { context, name }
        });
    };
    Ok(VendorRead {
        context: package.context().clone(),
        referencing_orders: referencing_orders(vendor.vendor_id.as_str(), package.healthy_orders()),
        diagnostics: package
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics().iter().cloned())
            .collect(),
        vendor: vendor.clone(),
    })
}

/// The reading load's one current-candidate answer, with every other state typed.
///
/// `inspect_canonical_package` rather than `open_package`, because the opening load
/// durably replaces the derived Package Index and `docs/architecture/durable-writes.md`
/// reserves the rebuilding load for a place a canonical write has already happened.
/// Reading one Vendor writes nothing.
fn loaded(
    root: &Path,
    load: CanonicalPackageLoad,
) -> Result<CurrentPackageCandidate, VendorReadFailure> {
    match load {
        CanonicalPackageLoad::CurrentCandidate(package) => Ok(package),
        CanonicalPackageLoad::Unmanaged(package) => {
            Err(VendorReadFailure::UnmanagedPackage(Box::new(package)))
        }
        CanonicalPackageLoad::DiagnosticOnly(package) => Err(VendorReadFailure::DamagedPackage {
            root: FailedPath::of(root),
            projection: Box::new(package),
        }),
        CanonicalPackageLoad::UpgradePending(package) => {
            Err(VendorReadFailure::UpgradePending(Box::new(package)))
        }
        CanonicalPackageLoad::OperationFailed(package) => {
            Err(VendorReadFailure::PackageOperationFailed(Box::new(package)))
        }
    }
}
