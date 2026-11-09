//! Fresh reads of one Order, one Vendor, and one README, each the evidence a later save
//! of it is guarded by.

use crate::interface::{
    PathBytes, SPIELORDERS_CONTACT_FIELD_EMAIL, SPIELORDERS_CONTACT_FIELD_NAME,
    SPIELORDERS_CONTACT_FIELD_PHONE, SPIELORDERS_CONTACT_FIELD_ROLE,
    SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, SPIELORDERS_ITEM_FIELD_DESCRIPTION,
    SPIELORDERS_ITEM_FIELD_QUANTITY, SPIELORDERS_ITEM_FIELD_TOTAL,
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE, SPIELORDERS_ORDER_FIELD_CURRENCY,
    SPIELORDERS_ORDER_FIELD_DATE_ORDERED, SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP, SPIELORDERS_ORDER_FIELD_ORDER_ID,
    SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
    SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_STATUS, SPIELORDERS_ORDER_FIELD_TITLE,
    SPIELORDERS_ORDER_FIELD_TOTAL, SPIELORDERS_ORDER_FIELD_VENDOR,
    SPIELORDERS_ORDER_FIELD_VENDOR_ID, SPIELORDERS_README_STATE_MISSING,
    SPIELORDERS_README_STATE_PRESENT, SPIELORDERS_README_STATE_UNREADABLE,
    SPIELORDERS_VENDOR_FIELD_ADDRESS, SPIELORDERS_VENDOR_FIELD_EMAIL,
    SPIELORDERS_VENDOR_FIELD_HIDDEN, SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE,
    SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES, SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
    SPIELORDERS_VENDOR_FIELD_WEBSITE, SpielordersConstant, Text,
    call::quietly,
    entries::{EMPTY, c_text},
};
use crate::{
    Coded, OrderRead, ReadmeRead, VendorRead,
    model::{HealthyOrder, OrderDocument, OrderItem, Vendor, VendorContact},
};
use std::{
    ffi::{CStr, CString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

/// A path as the bytes a caller passes back unchanged.
pub(crate) fn path_text(path: &Path) -> CString {
    c_text(path.as_os_str().as_bytes())
}

/// The text of each field a record or row answers.
#[derive(Clone, Debug, Default)]
pub(crate) struct Facts(Vec<(SpielordersConstant, CString)>);

impl Facts {
    pub(crate) fn of(facts: Vec<(SpielordersConstant, String)>) -> Self {
        Self(
            facts
                .into_iter()
                .map(|(field, value)| (field, c_text(value)))
                .collect(),
        )
    }

    pub(crate) fn get(&self, field: SpielordersConstant) -> Option<&CStr> {
        self.0
            .iter()
            .find(|(each, _)| *each == field)
            .map(|(_, value)| value.as_c_str())
    }

    pub(crate) fn text(&self, field: SpielordersConstant) -> Text<'_> {
        Text::new(self.get(field).unwrap_or(EMPTY))
    }
}

/// A number as the shortest decimal that reads back as exactly that `f64`: a stored
/// Quantity or Unit Price, and a calculated total, unrounded.
pub(crate) fn number(value: f64) -> String {
    value.to_string()
}

/// The facts a draft sets, as `document` holds them, with `vendor`'s Name.
pub(crate) fn document_facts(
    document: &OrderDocument,
    vendor: Option<&Vendor>,
) -> Vec<(SpielordersConstant, String)> {
    let optional = |value: &Option<String>| value.clone().unwrap_or_default();
    vec![
        (SPIELORDERS_ORDER_FIELD_TITLE, document.title.to_string()),
        (
            SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
            document.date_ordered.to_string(),
        ),
        (
            SPIELORDERS_ORDER_FIELD_STATUS,
            document.status.code().to_owned(),
        ),
        (
            SPIELORDERS_ORDER_FIELD_VENDOR,
            vendor
                .map(|vendor| vendor.name.to_string())
                .unwrap_or_default(),
        ),
        (SPIELORDERS_ORDER_FIELD_ORDERER, optional(&document.orderer)),
        (
            SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
            optional(&document.ordering_institution),
        ),
        (
            SPIELORDERS_ORDER_FIELD_CURRENCY,
            document
                .currency
                .as_ref()
                .map(|currency| currency.to_string())
                .unwrap_or_default(),
        ),
        (SPIELORDERS_ORDER_FIELD_PROJECT, optional(&document.project)),
        (
            SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
            optional(&document.order_group),
        ),
    ]
}

/// Every fact of a read Order: what a draft sets, and what it only reads.
pub(crate) fn order_facts(
    order: &HealthyOrder,
    vendor: Option<&Vendor>,
) -> Vec<(SpielordersConstant, String)> {
    let document = &order.document;
    let mut facts = document_facts(document, vendor);
    facts.extend([
        (
            SPIELORDERS_ORDER_FIELD_ORDER_ID,
            document.order_id.to_string(),
        ),
        (
            SPIELORDERS_ORDER_FIELD_VENDOR_ID,
            document
                .vendor_id
                .as_ref()
                .map(|id| id.as_str().to_owned())
                .unwrap_or_default(),
        ),
        (
            SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
            directory_name(&order.paths.directory.absolute_path),
        ),
        (SPIELORDERS_ORDER_FIELD_TOTAL, number(order.order_total)),
    ]);
    facts
}

pub(crate) fn directory_name(directory: &Path) -> String {
    directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn item_facts(
    item: &OrderItem,
    total: Option<f64>,
) -> Vec<(SpielordersConstant, String)> {
    let mut facts = vec![
        (
            SPIELORDERS_ITEM_FIELD_DESCRIPTION,
            item.description.to_string(),
        ),
        (SPIELORDERS_ITEM_FIELD_QUANTITY, number(*item.quantity)),
        (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, number(*item.unit_price)),
        (
            SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER,
            item.catalog_number.clone().unwrap_or_default(),
        ),
    ];
    if let Some(total) = total {
        facts.push((SPIELORDERS_ITEM_FIELD_TOTAL, number(total)));
    }
    facts
}

pub(crate) fn vendor_facts(vendor: &Vendor) -> Vec<(SpielordersConstant, String)> {
    let optional = |value: &Option<String>| value.clone().unwrap_or_default();
    vec![
        (SPIELORDERS_VENDOR_FIELD_NAME, vendor.name.to_string()),
        (SPIELORDERS_VENDOR_FIELD_PHONE, optional(&vendor.phone)),
        (SPIELORDERS_VENDOR_FIELD_EMAIL, optional(&vendor.email)),
        (SPIELORDERS_VENDOR_FIELD_WEBSITE, optional(&vendor.website)),
        (SPIELORDERS_VENDOR_FIELD_ADDRESS, optional(&vendor.address)),
        (
            SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
            optional(&vendor.purchasing_notes),
        ),
        (
            SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
            vendor.vendor_id.to_string(),
        ),
        (
            SPIELORDERS_VENDOR_FIELD_HIDDEN,
            vendor.is_hidden().to_string(),
        ),
    ]
}

pub(crate) fn contact_facts(contact: &VendorContact) -> Vec<(SpielordersConstant, String)> {
    let optional = |value: &Option<String>| value.clone().unwrap_or_default();
    vec![
        (SPIELORDERS_CONTACT_FIELD_NAME, contact.name.to_string()),
        (SPIELORDERS_CONTACT_FIELD_ROLE, optional(&contact.role)),
        (SPIELORDERS_CONTACT_FIELD_PHONE, optional(&contact.phone)),
        (SPIELORDERS_CONTACT_FIELD_EMAIL, optional(&contact.email)),
    ]
}

fn row<T>(rows: &[T], row: i64) -> Option<&T> {
    rows.get(usize::try_from(row).ok()?)
}

fn count<T>(rows: &[T]) -> i64 {
    i64::try_from(rows.len()).unwrap_or(i64::MAX)
}

/// A fresh read of one healthy Order: its facts, its Item rows, and its README.
#[repr(C)]
pub struct SpielordersOrder {
    data: Box<SpielordersOrderData>,
}

pub(crate) struct SpielordersOrderData {
    pub(crate) read: OrderRead,
    facts: Facts,
    items: Vec<Facts>,
    directory: CString,
}

impl SpielordersOrder {
    pub(crate) fn of(read: OrderRead) -> Self {
        let facts = Facts::of(order_facts(&read.order, read.vendor.as_ref()));
        let items = read
            .order
            .document
            .items
            .iter()
            .zip(
                read.order
                    .item_totals
                    .iter()
                    .copied()
                    .map(Some)
                    .chain(std::iter::repeat(None)),
            )
            .map(|(item, total)| Facts::of(item_facts(item, total)))
            .collect();
        let directory = path_text(&read.order.paths.directory.absolute_path);
        Self {
            data: Box::new(SpielordersOrderData {
                read,
                facts,
                items,
                directory,
            }),
        }
    }

    pub(crate) fn data(&self) -> &SpielordersOrderData {
        &self.data
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.data.read.order.paths.directory.absolute_path
    }
}

/// A fact of `order`: a `SPIELORDERS_ORDER_FIELD_*` constant. "" for an absent fact or a
/// field of another family.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_field(
    order: &SpielordersOrder,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || order.data.facts.text(field))
}

/// How many Item rows `order` holds.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_item_count(order: &SpielordersOrder) -> i64 {
    quietly(-1, || count(&order.data.items))
}

/// A fact of Item row `row` of `order`: a `SPIELORDERS_ITEM_FIELD_*` constant. "" when
/// out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_item_field(
    order: &SpielordersOrder,
    row_index: i64,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        row(&order.data.items, row_index).map_or(Text::new(EMPTY), |facts| facts.text(field))
    })
}

/// The Order Directory of `order`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_directory(order: &SpielordersOrder) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        PathBytes::new(&order.data.directory)
    })
}

/// The README of `order`, read with it; always a handle the caller frees.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_readme(
    order: &SpielordersOrder,
) -> Option<Box<SpielordersReadme>> {
    quietly(None, || {
        Some(Box::new(SpielordersReadme::of(
            order.data.read.readme.clone(),
            ReadmeScope::Order(order.directory().to_path_buf()),
        )))
    })
}

/// Frees `order`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_free(order: Option<Box<SpielordersOrder>>) {
    quietly((), || drop(order));
}

/// A fresh read of one Vendor, Hidden or not, and its Contact rows.
#[repr(C)]
pub struct SpielordersVendor {
    data: Box<SpielordersVendorData>,
}

pub(crate) struct SpielordersVendorData {
    pub(crate) read: VendorRead,
    facts: Facts,
    contacts: Vec<Facts>,
}

impl SpielordersVendor {
    pub(crate) fn of(read: VendorRead) -> Self {
        let facts = Facts::of(vendor_facts(&read.vendor));
        let contacts = read
            .vendor
            .contacts
            .iter()
            .map(|contact| Facts::of(contact_facts(contact)))
            .collect();
        Self {
            data: Box::new(SpielordersVendorData {
                read,
                facts,
                contacts,
            }),
        }
    }

    pub(crate) fn data(&self) -> &SpielordersVendorData {
        &self.data
    }

    pub(crate) fn root(&self) -> PathBuf {
        self.data.read.context.root.absolute_path.clone()
    }
}

/// A fact of `vendor`: a `SPIELORDERS_VENDOR_FIELD_*` constant. "" for an absent fact or
/// a field of another family.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_field(
    vendor: &SpielordersVendor,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || vendor.data.facts.text(field))
}

/// How many Contact rows `vendor` holds.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_contact_count(vendor: &SpielordersVendor) -> i64 {
    quietly(-1, || count(&vendor.data.contacts))
}

/// A fact of Contact row `row` of `vendor`: a `SPIELORDERS_CONTACT_FIELD_*` constant. ""
/// when out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_contact_field(
    vendor: &SpielordersVendor,
    row_index: i64,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        row(&vendor.data.contacts, row_index).map_or(Text::new(EMPTY), |facts| facts.text(field))
    })
}

/// Frees `vendor`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_free(vendor: Option<Box<SpielordersVendor>>) {
    quietly((), || drop(vendor));
}

/// Whose README a read is.
#[derive(Clone, Debug)]
pub(crate) enum ReadmeScope {
    Package(PathBuf),
    Order(PathBuf),
}

/// One conventional README: the Package's or one Order's. An unreadable README leaves
/// its Order readable and refuses only a plan to write it.
#[repr(C)]
pub struct SpielordersReadme {
    data: Box<SpielordersReadmeData>,
}

pub(crate) struct SpielordersReadmeData {
    pub(crate) read: ReadmeRead,
    pub(crate) scope: ReadmeScope,
    text: CString,
    path: CString,
}

impl SpielordersReadme {
    pub(crate) fn of(read: ReadmeRead, scope: ReadmeScope) -> Self {
        let text = c_text(read.text.clone().unwrap_or_default());
        let path = path_text(&read.reference.location.absolute_path);
        Self {
            data: Box::new(SpielordersReadmeData {
                read,
                scope,
                text,
                path,
            }),
        }
    }

    pub(crate) fn data(&self) -> &SpielordersReadmeData {
        &self.data
    }
}

/// The `SPIELORDERS_README_STATE_*` of `readme`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_readme_state(readme: &SpielordersReadme) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        match (readme.data.read.exists, &readme.data.read.text) {
            (false, _) => SPIELORDERS_README_STATE_MISSING,
            (true, Some(_)) => SPIELORDERS_README_STATE_PRESENT,
            (true, None) => SPIELORDERS_README_STATE_UNREADABLE,
        }
    })
}

/// The text of `readme`; "" when it is missing or unreadable.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_readme_text(readme: &SpielordersReadme) -> Text<'_> {
    quietly(Text::new(EMPTY), || Text::new(&readme.data.text))
}

/// Where `readme` is, whether or not it exists.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_readme_path(readme: &SpielordersReadme) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || PathBytes::new(&readme.data.path))
}

/// Frees `readme`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_readme_free(readme: Option<Box<SpielordersReadme>>) {
    quietly((), || drop(readme));
}
