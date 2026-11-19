//! A stored Package Index judged as a valid Package's derived index.

use crate::{
    integrity::{Aggregate, resolves},
    metadata::{IndexDocument, VendorsDocument, Version1},
};
use std::collections::BTreeSet;

/// Whether a stored index holds what a valid Package's derived index holds: its Vendor
/// aggregate valid and normalized, each Order valid and normalized, one entry per Order
/// ID and per Order Directory, every Vendor Reference resolving, every locator as deep as
/// its Directory, and the entries in list order.
pub(crate) fn index_semantics_are_valid(index: &IndexDocument) -> bool {
    let vendors = VendorsDocument {
        schema_version: Version1,
        vendors: index.vendors.clone(),
        values: index.values.clone(),
    };
    let vendors_are_valid_and_normalized =
        Aggregate::judge(&vendors).is_ok_and(|aggregate| aggregate.document() == &vendors);
    let orders_are_valid_and_normalized = index.orders.iter().all(|entry| {
        entry
            .order
            .validate()
            .is_ok_and(|validated| validated.order() == &entry.order)
    });
    let identities_are_unique = index
        .orders
        .iter()
        .map(|entry| entry.order.order_id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == index.orders.len();
    let references_resolve = index.orders.iter().all(|entry| {
        entry
            .order
            .vendor_id
            .as_ref()
            .is_none_or(|vendor_id| resolves(&index.vendors, vendor_id))
    });
    let directories_are_unique = index
        .orders
        .iter()
        .map(|entry| entry.order_directory.as_str())
        .collect::<BTreeSet<_>>()
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
        .all(|pair| pair[0].order.listing_order(&pair[1].order).is_le());
    vendors_are_valid_and_normalized
        && orders_are_valid_and_normalized
        && identities_are_unique
        && references_resolve
        && directories_are_unique
        && locators_match_directories
        && orders_are_sorted
}

#[cfg(test)]
mod tests;
