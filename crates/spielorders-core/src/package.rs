//! Cross-record metadata validation for a steady-state SpielOrder Package.

use crate::metadata::{
    IndexDocument, OrderDocument, Vendor, VendorId, VendorsDocument, VersionDocument,
    normalize_optional_multiline, normalize_optional_single_line,
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
