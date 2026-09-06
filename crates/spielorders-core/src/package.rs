//! Cross-record metadata validation for a steady-state SpielOrder Package.

use crate::metadata::{
    IndexDocument, IndexOrder, OrderDocument, Vendor, VendorId, VendorsDocument, Version1,
    VersionDocument, normalize_optional_multiline, normalize_optional_single_line,
    normalize_optional_visibility,
};
use crate::order::{OrderValidationFailure, ValidatedOrder};
use focaccia::unicode_full_case_eq;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReusableValue {
    Absent,
    Reused(String),
    New(String),
}

/// Whether this Vendor aggregate is a valid Package Vendor set on its own.
///
/// **The one place that judgement is made**, so a repair, an upgrade, and a canonical load
/// cannot come to accept aggregates on different terms. Three decisions live here and
/// nowhere else: the aggregate is asked of the Package-wide validator rather than scanned
/// locally, so nothing is accepted on terms canonical validation would reject; the marker
/// it is judged against is the version this build writes; and the Orders slice is empty
/// deliberately, because the constraints an aggregate answers for — duplicate Vendor IDs
/// and package-duplicate Vendor Names — are the ones that live in `vendors.json` itself,
/// while an Order's own record and the reference it carries are judged as an Order.
///
/// The verdict alone is returned. A caller that must publish bytes writes the document it
/// was given, canonicalized by that document's own serialization.
pub(crate) fn vendor_aggregate_is_package_valid(vendors: &VendorsDocument) -> bool {
    let version = VersionDocument {
        metadata_version: Version1,
    };
    PackageMetadata::new(&version, &[], vendors)
        .validate()
        .is_ok()
}

pub(crate) fn index_semantics_are_valid(index: &IndexDocument) -> bool {
    let version = VersionDocument {
        metadata_version: Version1,
    };
    let orders = index
        .orders
        .iter()
        .map(|entry| entry.order.clone())
        .collect::<Vec<_>>();
    let vendors = VendorsDocument {
        schema_version: Version1,
        vendors: index.vendors.clone(),
    };
    let normalized_index = PackageMetadata::new(&version, &orders, &vendors)
        .with_index(index)
        .validate()
        .ok()
        .and_then(|validated| validated.index().cloned());
    let metadata_is_valid_and_normalized = normalized_index.as_ref() == Some(index);
    let directories_are_unique = index
        .orders
        .iter()
        .map(|entry| entry.order_directory.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == index.orders.len();
    let locators_match_directories = index.orders.iter().all(|entry| {
        entry.order.package_root.spelled().is_some_and(|locator| {
            locator.as_str().split('/').count() == entry.order_directory.as_str().split('/').count()
        })
    });
    let orders_are_sorted = index
        .orders
        .windows(2)
        .all(|pair| compare_index_orders(&pair[0], &pair[1]).is_le());
    metadata_is_valid_and_normalized
        && directories_are_unique
        && locators_match_directories
        && orders_are_sorted
}

fn compare_index_orders(left: &IndexOrder, right: &IndexOrder) -> std::cmp::Ordering {
    right
        .order
        .date_ordered
        .as_str()
        .cmp(left.order.date_ordered.as_str())
        .then_with(|| {
            left.order
                .title
                .as_str()
                .as_bytes()
                .cmp(right.order.title.as_str().as_bytes())
        })
        .then_with(|| {
            left.order
                .order_id
                .as_str()
                .cmp(right.order.order_id.as_str())
        })
}

pub fn canonicalize_reusable_value<T: AsRef<str>>(
    proposed: Option<&str>,
    existing_values: &[T],
) -> ReusableValue {
    let Some(proposed) = proposed.map(str::trim).filter(|value| !value.is_empty()) else {
        return ReusableValue::Absent;
    };

    existing_values
        .iter()
        .map(AsRef::as_ref)
        .find(|existing| unicode_full_case_eq(proposed, existing.trim()))
        .map(|existing| ReusableValue::Reused(existing.trim().to_owned()))
        .unwrap_or_else(|| ReusableValue::New(proposed.to_owned()))
}

pub struct PackageMetadata<'a> {
    version: &'a VersionDocument,
    orders: &'a [OrderDocument],
    vendors: &'a VendorsDocument,
    index: Option<&'a IndexDocument>,
}

impl<'a> PackageMetadata<'a> {
    pub fn new(
        version: &'a VersionDocument,
        orders: &'a [OrderDocument],
        vendors: &'a VendorsDocument,
    ) -> Self {
        Self {
            version,
            orders,
            vendors,
            index: None,
        }
    }

    pub fn with_index(mut self, index: &'a IndexDocument) -> Self {
        self.index = Some(index);
        self
    }

    pub fn validate(
        self,
    ) -> Result<ValidatedPackageMetadata, Vec<PackageMetadataValidationFailure>> {
        let mut failures = Vec::new();
        let mut validated_orders = Vec::with_capacity(self.orders.len());

        failures.extend(
            duplicate_indices(self.orders.iter().map(|order| order.order_id.as_str()))
                .into_iter()
                .map(
                    |order_indices| PackageMetadataValidationFailure::DuplicateOrderId {
                        order_indices,
                    },
                ),
        );

        failures.extend(
            duplicate_indices(
                self.vendors
                    .vendors
                    .iter()
                    .map(|vendor| vendor.vendor_id.as_str()),
            )
            .into_iter()
            .map(|vendor_indices| {
                PackageMetadataValidationFailure::DuplicateVendorId { vendor_indices }
            }),
        );

        let mut grouped_vendor_names = vec![false; self.vendors.vendors.len()];
        for vendor_index in 0..self.vendors.vendors.len() {
            if grouped_vendor_names[vendor_index] {
                continue;
            }
            grouped_vendor_names[vendor_index] = true;
            let vendor_name = self.vendors.vendors[vendor_index].name.as_str().trim();
            let mut vendor_indices = vec![vendor_index];
            for (candidate_index, grouped) in grouped_vendor_names
                .iter_mut()
                .enumerate()
                .skip(vendor_index + 1)
            {
                if unicode_full_case_eq(
                    vendor_name,
                    self.vendors.vendors[candidate_index].name.as_str().trim(),
                ) {
                    *grouped = true;
                    vendor_indices.push(candidate_index);
                }
            }
            if vendor_indices.len() > 1 {
                failures
                    .push(PackageMetadataValidationFailure::DuplicateVendorName { vendor_indices });
            }
        }

        for (order_index, order) in self.orders.iter().enumerate() {
            match order.validate() {
                Ok(validated) => {
                    if let Some(vendor_id) = &validated.order().vendor_id
                        && !self
                            .vendors
                            .vendors
                            .iter()
                            .any(|vendor| vendor.vendor_id == *vendor_id)
                    {
                        failures.push(
                            PackageMetadataValidationFailure::UnresolvedVendorReference {
                                order_index,
                                vendor_id: vendor_id.clone(),
                            },
                        );
                    }
                    validated_orders.push(validated);
                }
                Err(order_failures) => {
                    failures.push(PackageMetadataValidationFailure::InvalidOrder {
                        order_index,
                        failures: order_failures,
                    });
                }
            }
        }

        let mut index = self.index.cloned();
        if let Some(index) = &mut index {
            for (index_order_index, entry) in index.orders.iter_mut().enumerate() {
                match entry.order.validate() {
                    Ok(validated) => entry.order = validated.order().clone(),
                    Err(index_failures) => {
                        failures.push(PackageMetadataValidationFailure::InvalidIndexOrder {
                            index_order_index,
                            failures: index_failures,
                        })
                    }
                }
            }
            normalize_vendors(&mut index.vendors);
        }

        if failures.is_empty() {
            let mut vendors = self.vendors.clone();
            normalize_vendors(&mut vendors.vendors);
            Ok(ValidatedPackageMetadata {
                version: self.version.clone(),
                orders: validated_orders,
                vendors,
                index,
            })
        } else {
            Err(failures)
        }
    }
}

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

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedPackageMetadata {
    version: VersionDocument,
    orders: Vec<ValidatedOrder>,
    vendors: VendorsDocument,
    index: Option<IndexDocument>,
}

impl ValidatedPackageMetadata {
    pub fn version(&self) -> &VersionDocument {
        &self.version
    }

    pub fn orders(&self) -> &[ValidatedOrder] {
        &self.orders
    }

    pub fn vendors(&self) -> &VendorsDocument {
        &self.vendors
    }

    pub fn index(&self) -> Option<&IndexDocument> {
        self.index.as_ref()
    }

    pub fn vendor_for_order(&self, order_index: usize) -> Option<&Vendor> {
        let vendor_id = self.orders.get(order_index)?.order().vendor_id.as_ref()?;
        self.vendors
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id == *vendor_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PackageMetadataValidationFailure {
    InvalidOrder {
        order_index: usize,
        failures: Vec<OrderValidationFailure>,
    },
    UnresolvedVendorReference {
        order_index: usize,
        vendor_id: VendorId,
    },
    DuplicateOrderId {
        order_indices: Vec<usize>,
    },
    DuplicateVendorName {
        vendor_indices: Vec<usize>,
    },
    DuplicateVendorId {
        vendor_indices: Vec<usize>,
    },
    InvalidIndexOrder {
        index_order_index: usize,
        failures: Vec<OrderValidationFailure>,
    },
}
