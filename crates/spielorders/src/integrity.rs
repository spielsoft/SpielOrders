//! The one judge of a Package's cross-record rules: the rules no record can be held to
//! from its own bytes, because each is about the records beside it.
//!
//! A Vendor aggregate holds no Vendor ID twice and no Vendor Name twice. Every capability
//! that reads, plans, commits, or repairs a Vendor aggregate asks [`Aggregate::judge`], so
//! a load and a write can never come to accept aggregates on different terms.
//!
//! An Order ID is held by one Order Directory, and a Vendor Reference resolves to a Vendor
//! of the Package. [`Records`] answers both over the Package's Order records, so a load,
//! a commit, and a repair ask who holds an identity the same way. A removed Order is
//! restored as it was, so the Vendor it names counts as much as an active Order's.
//!
//! A change that replaces records is judged by [`Records::broken_by`] against the records
//! it leaves in place, so no write answers one condition by creating another.
//!
//! Decision sources: Define Vendor identity and historical semantics; Define integrity,
//! validation, and repair boundaries.

use crate::{
    Failure,
    canonical::{CurrentPackageCandidate, DamagedOrder, HealthyOrder, ManagedContext},
    metadata::{
        OrderDocument, OrderId, Vendor, VendorId, VendorsDocument, normalize_optional_multiline,
        normalize_optional_single_line, normalize_optional_visibility,
    },
    operations::{FailedOperation, FailedPath},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{Location, located_in_package},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// A Vendor aggregate a Package can hold, its facts normalized as the load projects them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Aggregate(VendorsDocument);

/// One rule a Vendor aggregate breaks, naming the positions of the Vendors that break it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AggregateFault {
    /// These Vendors share one Vendor ID.
    DuplicateVendorId { vendor_indices: Vec<usize> },
    /// These Vendors' Names match after trimming and full Unicode case folding.
    DuplicateVendorName { vendor_indices: Vec<usize> },
}

impl Aggregate {
    /// `document` as a Package Vendor aggregate, or every rule it breaks: each shared
    /// Vendor ID in Vendor ID order, then each shared Vendor Name in the order its first
    /// Vendor appears.
    pub(crate) fn judge(document: &VendorsDocument) -> Result<Self, Vec<AggregateFault>> {
        let vendors = &document.vendors;
        let mut faults = duplicate_indices(vendors.iter().map(|vendor| vendor.vendor_id.as_str()))
            .into_iter()
            .map(|vendor_indices| AggregateFault::DuplicateVendorId { vendor_indices })
            .collect::<Vec<_>>();

        let mut grouped = vec![false; vendors.len()];
        for first in 0..vendors.len() {
            if grouped[first] {
                continue;
            }
            grouped[first] = true;
            let name = vendors[first].name.as_str().trim();
            let mut vendor_indices = vec![first];
            for (other, grouped) in grouped.iter_mut().enumerate().skip(first + 1) {
                if crate::vendors::vendor_names_match(name, vendors[other].name.as_str()) {
                    *grouped = true;
                    vendor_indices.push(other);
                }
            }
            if vendor_indices.len() > 1 {
                faults.push(AggregateFault::DuplicateVendorName { vendor_indices });
            }
        }

        if !faults.is_empty() {
            return Err(faults);
        }
        let mut document = document.clone();
        normalize_vendors(&mut document.vendors);
        crate::values::normalize(&mut document.values);
        Ok(Self(document))
    }

    /// The aggregate, normalized.
    pub(crate) fn document(&self) -> &VendorsDocument {
        &self.0
    }

    pub(crate) fn into_document(self) -> VendorsDocument {
        self.0
    }
}

/// Whether `vendors` holds the Vendor `vendor_id` references.
pub(crate) fn resolves(vendors: &[Vendor], vendor_id: &VendorId) -> bool {
    vendors.iter().any(|vendor| vendor.vendor_id == *vendor_id)
}

/// The Vendor ID an Order record names, read from its bytes as stored, so that a record
/// that does not validate still shows which Vendor it needs.
pub(crate) fn named_vendor(bytes: &[u8]) -> Option<VendorId> {
    let record = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    serde_json::from_value(record.get("vendor_id")?.clone()).ok()
}

/// The Order records of one Package, healthy and damaged, with the Vendors their
/// references resolve against: `None` where the Vendor aggregate could not be trusted.
/// The removed Orders join them only where a caller reads them.
pub(crate) struct Records<'p> {
    vendors: Option<&'p [Vendor]>,
    healthy: &'p [HealthyOrder],
    damaged: &'p [DamagedOrder],
    removed: Removed,
}

/// Records a change puts in place of some of a Package's own: Orders, each by its Order
/// Directory, and the Vendor aggregate.
#[derive(Default)]
pub(crate) struct Substitution<'s> {
    pub(crate) orders: Vec<(&'s Path, &'s OrderDocument)>,
    pub(crate) vendors: Option<&'s Aggregate>,
}

/// The cross-record rule a [`Substitution`] breaks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Breach {
    /// Another Order Directory already holds a substituted Order's identity.
    OrderIdentityTaken,
    /// A Vendor Reference would name no Vendor of the Package.
    VendorReferenceUnresolved,
}

/// One Order Directory naming a Vendor.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Holder<'r> {
    /// A healthy or readable removed Order, whose record a change can rewrite.
    Rewritable {
        directory: &'r Location,
        record: &'r Path,
        order: &'r OrderDocument,
    },
    /// An Order whose record does not validate, so no change can rewrite it; it still
    /// names the Vendor its record holds as stored.
    Fixed { directory: &'r Location },
}

impl<'r> Holder<'r> {
    pub(crate) fn directory(&self) -> &'r Location {
        match self {
            Self::Rewritable { directory, .. } | Self::Fixed { directory } => directory,
        }
    }
}

/// The Orders in the Removed Orders Directory, which undoing a removal restores as they
/// were.
#[derive(Debug, Default)]
struct Removed {
    readable: Vec<RemovedOrder>,
    /// The directory of each removed Order whose record does not validate, with the
    /// Vendor its record still names.
    damaged: Vec<(Location, VendorId)>,
}

#[derive(Debug)]
struct RemovedOrder {
    directory: Location,
    record: PathBuf,
    order: OrderDocument,
}

impl Removed {
    /// The removed Orders of `context`'s Package, or the failure that kept one from being
    /// read. A Removed Orders Directory that does not exist holds none, and an entry with
    /// no record is no removed Order; a directory that cannot be listed, or a record that
    /// cannot be read, might name any Vendor, so it fails rather than naming none.
    fn read<F: FileSystem>(context: &ManagedContext, filesystem: &F) -> Result<Self, Failure> {
        let Some(removed) = &context.removed_orders_directory else {
            return Ok(Self::default());
        };
        let root = &context.root.absolute_path;
        let failed = |path: &Path, error: F::Error| {
            Failure::RemovedOrderUnreadable(Box::new(Failure::from(FailedOperation::inside(
                root,
                path.to_path_buf(),
                &error,
                filesystem,
            ))))
        };
        let unusable = |path: &Path| {
            Failure::RemovedOrderUnreadable(Box::new(
                FailedOperation::unusable_answer(FailedPath::of(path))
                    .within(root)
                    .into(),
            ))
        };
        let directory = &removed.absolute_path;
        match filesystem.symlink_metadata(directory) {
            Ok(None) => return Ok(Self::default()),
            Ok(Some(FileSystemEntryKind::Directory)) => {}
            Ok(Some(_)) => return Err(unusable(directory)),
            Err(error) => return Err(failed(directory, error)),
        }
        let entries = filesystem
            .read_directory(directory)
            .map_err(|error| failed(directory, error))?;
        let mut orders = Self::default();
        for entry in entries {
            let order_directory = directory.join(&entry.name);
            match entry.kind {
                FileSystemEntryKind::Directory => {}
                FileSystemEntryKind::SymbolicLink => return Err(unusable(&order_directory)),
                _ => continue,
            }
            let metadata = order_directory.join(".spielorders");
            match filesystem.symlink_metadata(&metadata) {
                Ok(Some(FileSystemEntryKind::Directory)) => {}
                Ok(None) => continue,
                Ok(Some(_)) => return Err(unusable(&metadata)),
                Err(error) => return Err(failed(&metadata, error)),
            }
            let record = metadata.join("order.json");
            match filesystem.symlink_metadata(&record) {
                Ok(None) => continue,
                Ok(Some(FileSystemEntryKind::File)) => {}
                Ok(Some(_)) => return Err(unusable(&record)),
                Err(error) => return Err(failed(&record, error)),
            }
            let bytes = filesystem
                .read(&record)
                .map_err(|error| failed(&record, error))?;
            let directory = located_in_package(root, order_directory);
            match OrderDocument::from_json_bytes(&bytes)
                .ok()
                .and_then(|document| document.validate().ok())
            {
                Some(order) => orders.readable.push(RemovedOrder {
                    directory,
                    record,
                    order: order.order().clone(),
                }),
                None => {
                    if let Some(vendor_id) = named_vendor(&bytes) {
                        orders.damaged.push((directory, vendor_id));
                    }
                }
            }
        }
        orders
            .readable
            .sort_by(|left, right| left.record.cmp(&right.record));
        orders.damaged.sort_by(|left, right| {
            (&left.0.absolute_path, left.1.as_str())
                .cmp(&(&right.0.absolute_path, right.1.as_str()))
        });
        Ok(orders)
    }
}

impl<'p> Records<'p> {
    pub(crate) fn new(
        vendors: Option<&'p [Vendor]>,
        healthy: &'p [HealthyOrder],
        damaged: &'p [DamagedOrder],
    ) -> Self {
        Self {
            vendors,
            healthy,
            damaged,
            removed: Removed::default(),
        }
    }

    /// These records and the removed Orders of `context`'s Package, or the failure that
    /// kept the removed Orders from being read.
    pub(crate) fn with_removed<F: FileSystem>(
        mut self,
        context: &ManagedContext,
        filesystem: &F,
    ) -> Result<Self, Failure> {
        self.removed = Removed::read(context, filesystem)?;
        Ok(self)
    }

    /// The records a loaded Package holds.
    pub(crate) fn of(package: &'p CurrentPackageCandidate) -> Self {
        Self::new(
            Some(&package.vendors),
            &package.healthy_orders,
            &package.damaged_orders,
        )
    }

    /// Each Order ID more than one Order Directory holds, with those Directories in
    /// order. A damaged Order still holds the identity its record names.
    pub(crate) fn contested(&self) -> BTreeMap<OrderId, Vec<Location>> {
        let mut holders = BTreeMap::<OrderId, Vec<Location>>::new();
        for (order_id, directory) in self.identities() {
            holders
                .entry(order_id.clone())
                .or_default()
                .push(directory.clone());
        }
        holders.retain(|_, directories| directories.len() > 1);
        for directories in holders.values_mut() {
            directories.sort();
        }
        holders
    }

    /// The Order Directory holding `order_id`, other than `except`; a healthy Order is
    /// named before a damaged one.
    pub(crate) fn holder(&self, order_id: &str, except: Option<&Path>) -> Option<&'p Location> {
        self.identities()
            .filter(|(_, directory)| Some(directory.absolute_path.as_path()) != except)
            .find_map(|(id, directory)| (id.as_str() == order_id).then_some(directory))
    }

    /// Whether these records hold together as a valid Package holds them: one Order
    /// Directory per identity, and every healthy Order's Vendor Reference resolving.
    pub(crate) fn sound(&self) -> bool {
        let Some(vendors) = self.vendors else {
            return false;
        };
        self.contested().is_empty()
            && self.healthy.iter().all(|order| {
                order
                    .document
                    .vendor_id
                    .as_ref()
                    .is_none_or(|vendor_id| resolves(vendors, vendor_id))
            })
    }

    /// Each Order naming one of `vendor_ids`: healthy in the Package's own order, then
    /// damaged, readable removed, and damaged removed, each in Directory order.
    pub(crate) fn naming(&self, vendor_ids: &[VendorId]) -> Vec<Holder<'_>> {
        self.references()
            .filter(|(_, vendor_id)| vendor_ids.contains(vendor_id))
            .map(|(holder, _)| holder)
            .collect()
    }

    /// The first rule `substitution` breaks against these records, when it breaks one.
    ///
    /// Each substituted Order's identity must be free of every other Order Directory, unless
    /// its Directory's own readable record already holds it, and its Vendor Reference must
    /// resolve in the substituted aggregate, or in these records' own where the aggregate
    /// stays. A substituted aggregate must keep every Vendor that
    /// an Order left in place names and that resolves now; with no trusted aggregate,
    /// every Vendor named counts. A reference that resolves nowhere already is not the
    /// substitution's doing.
    pub(crate) fn broken_by(&self, substitution: &Substitution) -> Option<Breach> {
        let vendors = substitution
            .vendors
            .map(|aggregate| aggregate.document().vendors.as_slice())
            .or(self.vendors);
        for (directory, order) in &substitution.orders {
            let kept = self.records().any(|(holder, _)| {
                matches!(holder, Holder::Rewritable { directory: own, order: current, .. }
                    if own.absolute_path == *directory && current.order_id == order.order_id)
            });
            if !kept && self.holder(&order.order_id, Some(directory)).is_some() {
                return Some(Breach::OrderIdentityTaken);
            }
            if let Some(vendor_id) = &order.vendor_id
                && !vendors.is_some_and(|vendors| resolves(vendors, vendor_id))
            {
                return Some(Breach::VendorReferenceUnresolved);
            }
        }
        let aggregate = &substitution.vendors?.document().vendors;
        let substituted = |holder: &Holder| {
            substitution
                .orders
                .iter()
                .any(|(directory, _)| *directory == holder.directory().absolute_path)
        };
        let unresolves = self
            .references()
            .filter(|(holder, _)| !substituted(holder))
            .filter(|(_, vendor_id)| self.vendors.is_none_or(|now| resolves(now, vendor_id)))
            .any(|(_, vendor_id)| !resolves(aggregate, vendor_id));
        unresolves.then_some(Breach::VendorReferenceUnresolved)
    }

    /// Each Vendor an Order names, with that Order, in [`Self::naming`]'s order.
    fn references(&self) -> impl Iterator<Item = (Holder<'_>, &VendorId)> {
        self.records()
            .filter_map(|(holder, vendor_id)| Some((holder, vendor_id?)))
    }

    /// Each Order record, with the Vendor it names: healthy in the Package's own order,
    /// then damaged, readable removed, and damaged removed, each in Directory order.
    fn records(&self) -> impl Iterator<Item = (Holder<'_>, Option<&VendorId>)> {
        let healthy = self.healthy.iter().map(|order| {
            let holder = Holder::Rewritable {
                directory: &order.paths.directory,
                record: &order.paths.order.absolute_path,
                order: &order.document,
            };
            (holder, order.document.vendor_id.as_ref())
        });
        let damaged = self.damaged.iter().map(|order| {
            let holder = Holder::Fixed {
                directory: &order.paths.directory,
            };
            (holder, order.vendor_id.as_ref())
        });
        let readable = self.removed.readable.iter().map(|removed| {
            let holder = Holder::Rewritable {
                directory: &removed.directory,
                record: &removed.record,
                order: &removed.order,
            };
            (holder, removed.order.vendor_id.as_ref())
        });
        let fixed = self
            .removed
            .damaged
            .iter()
            .map(|(directory, vendor_id)| (Holder::Fixed { directory }, Some(vendor_id)));
        healthy.chain(damaged).chain(readable).chain(fixed)
    }

    /// Each identity an Order Directory holds, healthy ones first.
    fn identities(&self) -> impl Iterator<Item = (&'p OrderId, &'p Location)> {
        let healthy = self
            .healthy
            .iter()
            .map(|order| (&order.document.order_id, &order.paths.directory));
        let damaged = self.damaged.iter().filter_map(|order| {
            order
                .order_id
                .as_ref()
                .map(|order_id| (order_id, &order.paths.directory))
        });
        healthy.chain(damaged)
    }
}

/// The positions of each key that occurs more than once, in key order.
fn duplicate_indices<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<Vec<usize>> {
    let mut indices_by_key = BTreeMap::<&str, Vec<usize>>::new();
    for (index, key) in keys.into_iter().enumerate() {
        indices_by_key.entry(key).or_default().push(index);
    }
    indices_by_key
        .into_values()
        .filter(|indices| indices.len() > 1)
        .collect()
}

fn normalize_vendors(vendors: &mut [Vendor]) {
    for vendor in vendors {
        vendor.name.normalize_single_line();
        normalize_optional_single_line(&mut vendor.phone);
        normalize_optional_single_line(&mut vendor.email);
        normalize_optional_single_line(&mut vendor.website);
        normalize_optional_multiline(&mut vendor.address);
        normalize_optional_multiline(&mut vendor.purchasing_notes);
        normalize_optional_visibility(&mut vendor.hidden);
        for contact in &mut vendor.contacts {
            contact.name.normalize_single_line();
            normalize_optional_single_line(&mut contact.role);
            normalize_optional_single_line(&mut contact.phone);
            normalize_optional_single_line(&mut contact.email);
        }
    }
}

#[cfg(test)]
mod tests;
