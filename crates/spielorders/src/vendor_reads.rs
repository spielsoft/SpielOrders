//! Reading one Vendor of a Package by the Vendor Name or Vendor ID that selects it.
//!
//! **A Name resolves here as it does for every Vendor mutation**, through the Package's
//! own comparison, so the read and the edits cannot disagree about which Vendor a Name
//! selects.
//!
//! **The read carries its own load's diagnostics**, because they bear on it directly: a
//! Damaged Order's record cannot be read, so it names no Vendor and appears in no
//! referencing summary, and the referencing collection is therefore partial in exactly
//! the cases the load has something to say.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics.

use crate::vendors::vendor_aggregate;
use crate::{
    Failure, VendorRef,
    canonical::{ManagedContext, load_package},
    metadata::Vendor,
    ports::FileSystem,
    readme::Diagnostic,
    vendors::vendor_by_name,
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
    /// The managed paths of the Package this Vendor belongs to.
    pub context: ManagedContext,
    /// The Vendor itself, with its shared facts, its ordered Contacts, and its
    /// visibility. A Hidden Vendor reads exactly as any other: hiding governs what a
    /// Package offers for new use and never what resolves.
    pub vendor: Vendor,
    /// Every diagnostic the load made, flat and in the core's own order.
    ///
    /// **Every group, unfiltered**, which is where this differs from reading one Order: an
    /// Order has a Directory, so its own observations are the ones located under it, while
    /// a Vendor has no directory to select by. Nor would a narrower selection be right —
    /// the Vendor record is one Package-level document and the Orders that reference it
    /// are spread across the Package, so an observation anywhere in it can be the reason
    /// this answer is partial.
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads the one Vendor of the Package at `selected_root` that `vendor` names.
///
/// A Name compares by the Package's own rule in [`crate::vendors`], beside the
/// duplicate-Name rule that shares it. Hidden Vendors resolve like any other.
pub(crate) fn read_vendor<F: FileSystem>(
    selected_root: &Path,
    vendor: &VendorRef,
    filesystem: &F,
) -> Result<VendorRead, Failure> {
    let package = load_package(selected_root, filesystem)?;
    let found = match vendor {
        VendorRef::Name(name) => vendor_by_name(name, &package.vendors),
        VendorRef::Id(id) => package
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id.as_str() == id),
    };
    let Some(found) = found else {
        return Err(match vendor {
            VendorRef::Id(id) => Failure::UnknownVendorId {
                vendor_id: id.clone(),
            },
            VendorRef::Name(name) if package.vendors.is_empty() => Failure::PackageHasNoVendors {
                context: Box::new(package.context.clone()),
                name: name.clone(),
            },
            VendorRef::Name(name) => Failure::UnknownVendor {
                name: name.clone(),
                scope: vendor_aggregate(&package),
            },
        });
    };
    Ok(VendorRead {
        context: package.context.clone(),
        diagnostics: package.findings(),
        vendor: found.clone(),
    })
}
