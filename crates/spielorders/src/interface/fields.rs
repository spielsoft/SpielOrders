//! One field table per record kind: which `SPIELORDERS_*_FIELD_*` constant names which
//! field of an Order, an Item, a Vendor, a Contact, or a file's text.
//!
//! **The library's own field types are the other side of each table**, so a field a
//! caller sets reaches the library as the field it is, and a field the library names in
//! a refusal reaches the caller as its constant, without either side spelling the other's
//! name. A fact a record answers but no caller assigns is a read-only column under the
//! name the library reports it by.

use crate::interface::{
    SPIELORDERS_CONTACT_FIELD_EMAIL, SPIELORDERS_CONTACT_FIELD_NAME,
    SPIELORDERS_CONTACT_FIELD_PHONE, SPIELORDERS_CONTACT_FIELD_ROLE,
    SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, SPIELORDERS_ITEM_FIELD_DESCRIPTION,
    SPIELORDERS_ITEM_FIELD_NOTES, SPIELORDERS_ITEM_FIELD_QUANTITY, SPIELORDERS_ITEM_FIELD_TOTAL,
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE, SPIELORDERS_ORDER_FIELD_CURRENCY,
    SPIELORDERS_ORDER_FIELD_DAMAGE, SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
    SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME, SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
    SPIELORDERS_ORDER_FIELD_ORDER_ID, SPIELORDERS_ORDER_FIELD_ORDERER,
    SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION, SPIELORDERS_ORDER_FIELD_PROJECT,
    SPIELORDERS_ORDER_FIELD_STATUS, SPIELORDERS_ORDER_FIELD_TITLE, SPIELORDERS_ORDER_FIELD_TOTAL,
    SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_ORDER_FIELD_VENDOR_ID,
    SPIELORDERS_TEXT_FIELD_CONTENT, SPIELORDERS_VENDOR_FIELD_ADDRESS,
    SPIELORDERS_VENDOR_FIELD_EMAIL, SPIELORDERS_VENDOR_FIELD_HIDDEN, SPIELORDERS_VENDOR_FIELD_NAME,
    SPIELORDERS_VENDOR_FIELD_PHONE, SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
    SPIELORDERS_VENDOR_FIELD_VENDOR_ID, SPIELORDERS_VENDOR_FIELD_WEBSITE, SpielordersConstant,
};
use crate::{
    drafting::{Fact, RowFact},
    order_facts::{ItemField, OrderField},
    values::ReusableValueKind,
    vendors::{ContactField, VendorField},
};

/// One field of a record kind: one a caller assigns, or one the record only answers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Column<F> {
    Fact(F),
    Read(&'static str),
}

/// A record kind whose fields have a table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Table {
    Order,
    Item,
    Vendor,
    Contact,
    Text,
}

const ORDER: [(SpielordersConstant, Column<OrderField>); 14] = [
    (
        SPIELORDERS_ORDER_FIELD_TITLE,
        Column::Fact(OrderField::Title),
    ),
    (
        SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
        Column::Fact(OrderField::DateOrdered),
    ),
    (
        SPIELORDERS_ORDER_FIELD_STATUS,
        Column::Fact(OrderField::Status),
    ),
    (
        SPIELORDERS_ORDER_FIELD_VENDOR,
        Column::Fact(OrderField::Vendor),
    ),
    (
        SPIELORDERS_ORDER_FIELD_ORDERER,
        Column::Fact(OrderField::Orderer),
    ),
    (
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        Column::Fact(OrderField::OrderingInstitution),
    ),
    (
        SPIELORDERS_ORDER_FIELD_CURRENCY,
        Column::Fact(OrderField::Currency),
    ),
    (
        SPIELORDERS_ORDER_FIELD_PROJECT,
        Column::Fact(OrderField::Project),
    ),
    (
        SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
        Column::Fact(OrderField::OrderGroup),
    ),
    (SPIELORDERS_ORDER_FIELD_ORDER_ID, Column::Read("order_id")),
    (
        SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
        Column::Read("directory_name"),
    ),
    (SPIELORDERS_ORDER_FIELD_TOTAL, Column::Read("total")),
    (SPIELORDERS_ORDER_FIELD_DAMAGE, Column::Read("damage")),
    (SPIELORDERS_ORDER_FIELD_VENDOR_ID, Column::Read("vendor_id")),
];

const ITEM: [(SpielordersConstant, Column<ItemField>); 6] = [
    (
        SPIELORDERS_ITEM_FIELD_DESCRIPTION,
        Column::Fact(ItemField::Description),
    ),
    (
        SPIELORDERS_ITEM_FIELD_QUANTITY,
        Column::Fact(ItemField::Quantity),
    ),
    (
        SPIELORDERS_ITEM_FIELD_UNIT_PRICE,
        Column::Fact(ItemField::UnitPrice),
    ),
    (
        SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER,
        Column::Fact(ItemField::CatalogNumber),
    ),
    (SPIELORDERS_ITEM_FIELD_TOTAL, Column::Read("total")),
    (SPIELORDERS_ITEM_FIELD_NOTES, Column::Fact(ItemField::Notes)),
];

const VENDOR: [(SpielordersConstant, Column<VendorField>); 8] = [
    (
        SPIELORDERS_VENDOR_FIELD_NAME,
        Column::Fact(VendorField::Name),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_PHONE,
        Column::Fact(VendorField::Phone),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_EMAIL,
        Column::Fact(VendorField::Email),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_WEBSITE,
        Column::Fact(VendorField::Website),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_ADDRESS,
        Column::Fact(VendorField::Address),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
        Column::Fact(VendorField::PurchasingNotes),
    ),
    (
        SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
        Column::Read("vendor_id"),
    ),
    (SPIELORDERS_VENDOR_FIELD_HIDDEN, Column::Read("hidden")),
];

const CONTACT: [(SpielordersConstant, Column<ContactField>); 4] = [
    (
        SPIELORDERS_CONTACT_FIELD_NAME,
        Column::Fact(ContactField::Name),
    ),
    (
        SPIELORDERS_CONTACT_FIELD_ROLE,
        Column::Fact(ContactField::Role),
    ),
    (
        SPIELORDERS_CONTACT_FIELD_PHONE,
        Column::Fact(ContactField::Phone),
    ),
    (
        SPIELORDERS_CONTACT_FIELD_EMAIL,
        Column::Fact(ContactField::Email),
    ),
];

/// A text draft's one field: the whole text.
const TEXT: [(SpielordersConstant, Column<&str>); 1] =
    [(SPIELORDERS_TEXT_FIELD_CONTENT, Column::Fact("content"))];

/// The name the library gives a field, as a fact or as a read-only column.
trait Named: Copy {
    fn name(self) -> &'static str;
}

macro_rules! named {
    ($($field:ty),*) => {$(
        impl Named for $field {
            fn name(self) -> &'static str {
                <$field>::name(self)
            }
        }
    )*};
}

named!(OrderField, ItemField, VendorField, ContactField);

impl Named for &'static str {
    fn name(self) -> &'static str {
        self
    }
}

impl<F: Named> Column<F> {
    fn name(self) -> &'static str {
        match self {
            Self::Fact(field) => field.name(),
            Self::Read(name) => name,
        }
    }

    fn fact(self) -> Option<F> {
        match self {
            Self::Fact(field) => Some(field),
            Self::Read(_) => None,
        }
    }
}

/// Each column of `table` as its constant, its library name, and whether a caller assigns
/// it.
fn columns(table: Table) -> Vec<(SpielordersConstant, &'static str, bool)> {
    fn each<F: Named>(
        table: &[(SpielordersConstant, Column<F>)],
    ) -> Vec<(SpielordersConstant, &'static str, bool)> {
        table
            .iter()
            .map(|(constant, column)| (*constant, column.name(), column.fact().is_some()))
            .collect()
    }
    match table {
        Table::Order => each(&ORDER),
        Table::Item => each(&ITEM),
        Table::Vendor => each(&VENDOR),
        Table::Contact => each(&CONTACT),
        Table::Text => each(&TEXT),
    }
}

/// The fields of `table` a caller assigns, in the order an editor lists them.
pub(crate) fn assignable(table: Table) -> Vec<SpielordersConstant> {
    columns(table)
        .into_iter()
        .filter(|(_, _, assigned)| *assigned)
        .map(|(constant, _, _)| constant)
        .collect()
}

/// Whether `field` is one of the fields of `table` a caller assigns.
pub(crate) fn assigns(table: Table, field: SpielordersConstant) -> bool {
    assignable(table).contains(&field)
}

/// The constant of the field of `table` the library names `name`, or -1.
pub(crate) fn constant(table: Table, name: &str) -> SpielordersConstant {
    columns(table)
        .into_iter()
        .find(|(_, each, _)| *each == name)
        .map_or(SpielordersConstant::NONE, |(constant, _, _)| constant)
}

/// The Order field `field` names, where it is one a caller assigns.
pub(crate) fn order_field(field: SpielordersConstant) -> Option<OrderField> {
    find(&ORDER, field)
}

/// The kind of Reusable Free-Entry Value the Order field `field` holds, for the four
/// value fields.
pub(crate) fn reusable(field: SpielordersConstant) -> Option<ReusableValueKind> {
    order_field(field)?.reusable()
}

/// The drafted fact `field` names, where a draft sets it.
pub(crate) fn fact(field: SpielordersConstant) -> Option<Fact> {
    find(&ORDER, field)
        .map(Fact::Order)
        .or_else(|| find(&VENDOR, field).map(Fact::Vendor))
        .or_else(|| find(&TEXT, field).map(|_| Fact::Content))
}

/// The assigned field `field` names in `table`.
fn find<F: Named>(
    table: &[(SpielordersConstant, Column<F>)],
    field: SpielordersConstant,
) -> Option<F> {
    table
        .iter()
        .find(|(constant, _)| *constant == field)
        .and_then(|(_, column)| column.fact())
}

/// The constant of the drafted fact `fact`.
pub(crate) fn fact_constant(fact: Fact) -> SpielordersConstant {
    fn find<F: Named + PartialEq>(
        table: &[(SpielordersConstant, Column<F>)],
        field: F,
    ) -> SpielordersConstant {
        table
            .iter()
            .find(|(_, column)| *column == Column::Fact(field))
            .map_or(SpielordersConstant::NONE, |(constant, _)| *constant)
    }
    match fact {
        Fact::Order(field) => find(&ORDER, field),
        Fact::Vendor(field) => find(&VENDOR, field),
        Fact::Content => SPIELORDERS_TEXT_FIELD_CONTENT,
    }
}

/// The drafted row fact `field` names, where a draft's row sets it.
pub(crate) fn row_fact(field: SpielordersConstant) -> Option<RowFact> {
    find(&ITEM, field)
        .map(RowFact::Item)
        .or_else(|| find(&CONTACT, field).map(RowFact::Contact))
}

/// The constant of the drafted row fact `fact`.
pub(crate) fn row_fact_constant(fact: RowFact) -> SpielordersConstant {
    let constant = match fact {
        RowFact::Item(field) => ITEM
            .iter()
            .find(|(_, column)| *column == Column::Fact(field))
            .map(|(constant, _)| *constant),
        RowFact::Contact(field) => CONTACT
            .iter()
            .find(|(_, column)| *column == Column::Fact(field))
            .map(|(constant, _)| *constant),
    };
    constant.unwrap_or(SpielordersConstant::NONE)
}
