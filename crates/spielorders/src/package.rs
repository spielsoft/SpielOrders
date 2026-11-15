//! Cross-record metadata validation for a steady-state SpielOrder Package.

use crate::metadata::{
    IndexDocument, IndexOrder, OrderDocument, Vendor, VendorsDocument, Version1,
    normalize_optional_multiline, normalize_optional_single_line, normalize_optional_visibility,
};
use focaccia::unicode_full_case_eq;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReusableValue {
    Absent,
    Reused(String),
    New(String),
}

/// How a mutation spells each of an Order's Reusable Free-Entry Values: reusing an
/// existing canonical spelling, introducing a new one, or leaving it absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableValues {
    pub orderer: ReusableValue,
    pub ordering_institution: ReusableValue,
    pub project: ReusableValue,
    pub order_group: ReusableValue,
}

/// Whether this Vendor aggregate is a valid Package Vendor set on its own.
///
/// **The one place that judgement is made**, so a repair and a canonical load cannot
/// come to accept aggregates on different terms. Three decisions live here and
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
    PackageMetadata::new(&[], vendors).validate().is_ok()
}

pub(crate) fn index_semantics_are_valid(index: &IndexDocument) -> bool {
    let orders = index
        .orders
        .iter()
        .map(|entry| entry.order.clone())
        .collect::<Vec<_>>();
    let vendors = VendorsDocument {
        schema_version: Version1,
        vendors: index.vendors.clone(),
        values: index.values.clone(),
    };
    let normalized_index = PackageMetadata::new(&orders, &vendors)
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
    orders: &'a [OrderDocument],
    vendors: &'a VendorsDocument,
    index: Option<&'a IndexDocument>,
}

impl<'a> PackageMetadata<'a> {
    pub fn new(orders: &'a [OrderDocument], vendors: &'a VendorsDocument) -> Self {
        Self {
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

        failures.extend(
            duplicate_indices(self.orders.iter().map(|order| order.order_id.as_str()))
                .into_iter()
                .map(|_| PackageMetadataValidationFailure::DuplicateOrderId),
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

        for order in self.orders {
            match order.validate() {
                Ok(validated) => {
                    if let Some(vendor_id) = &validated.order().vendor_id
                        && !self
                            .vendors
                            .vendors
                            .iter()
                            .any(|vendor| vendor.vendor_id == *vendor_id)
                    {
                        failures.push(PackageMetadataValidationFailure::UnresolvedVendorReference);
                    }
                }
                Err(_) => failures.push(PackageMetadataValidationFailure::InvalidOrder),
            }
        }

        let mut index = self.index.cloned();
        if let Some(index) = &mut index {
            for entry in &mut index.orders {
                match entry.order.validate() {
                    Ok(validated) => entry.order = validated.order().clone(),
                    Err(_) => failures.push(PackageMetadataValidationFailure::InvalidIndexOrder),
                }
            }
            normalize_vendors(&mut index.vendors);
            crate::values::normalize(&mut index.values);
        }

        if failures.is_empty() {
            let mut vendors = self.vendors.clone();
            normalize_vendors(&mut vendors.vendors);
            crate::values::normalize(&mut vendors.values);
            Ok(ValidatedPackageMetadata { vendors, index })
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
    vendors: VendorsDocument,
    index: Option<IndexDocument>,
}

impl ValidatedPackageMetadata {
    pub fn vendors(&self) -> &VendorsDocument {
        &self.vendors
    }

    pub fn index(&self) -> Option<&IndexDocument> {
        self.index.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PackageMetadataValidationFailure {
    InvalidOrder,
    UnresolvedVendorReference,
    DuplicateOrderId,
    DuplicateVendorName { vendor_indices: Vec<usize> },
    DuplicateVendorId { vendor_indices: Vec<usize> },
    InvalidIndexOrder,
}

#[cfg(test)]
mod tests;
