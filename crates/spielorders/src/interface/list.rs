//! Lists: Orders, Vendors, and reusable values, each row a set of field texts.

use crate::interface::{
    OptionalText, PathBytes, SPIELORDERS_FAMILY_ORDER_FIELD, SPIELORDERS_FAMILY_VENDOR_FIELD,
    SPIELORDERS_ORDER_FIELD_DAMAGE, SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP, SPIELORDERS_ORDER_FIELD_ORDER_ID,
    SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
    SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_TOTAL,
    SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_CANCELLED,
    SPIELORDERS_VALUE_FIELD_USAGE_COUNT, SPIELORDERS_VALUE_FIELD_VALUE, SpielordersConstant,
    SpielordersEntries, SpielordersPackage, SpielordersResult, Text,
    call::{Session, deliver, fallible, quietly, settle},
    entries::{EMPTY, Subject},
    library::is_member,
    package::query,
    records::{Facts, directory_name, order_facts, path_text, vendor_facts},
};
use crate::{
    Coded,
    model::{CaseSensitivity, OrderListEntry, ReusableValueKind, SearchQuery, VendorVisibility},
};
use std::{cmp::Ordering, ffi::CString};

/// The `flags` bit of a list call that matches case exactly.
pub const SPIELORDERS_LIST_CASE_SENSITIVE: u32 = 1;
/// The `flags` bit of a Vendor list call that includes Hidden Vendors.
pub const SPIELORDERS_LIST_INCLUDE_HIDDEN: u32 = 2;

struct Row {
    damaged: bool,
    path: CString,
    facts: Facts,
}

/// The rows of one list, in the order asked for.
#[repr(C)]
pub struct SpielordersList {
    data: Box<SpielordersListData>,
}

struct SpielordersListData {
    rows: Vec<Row>,
}

impl SpielordersList {
    fn of(rows: Vec<Row>) -> Self {
        Self {
            data: Box::new(SpielordersListData { rows }),
        }
    }

    fn row(&self, index: i64) -> Option<&Row> {
        self.data.rows.get(usize::try_from(index).ok()?)
    }
}

fn case(flags: u32) -> CaseSensitivity {
    if flags & SPIELORDERS_LIST_CASE_SENSITIVE == 0 {
        CaseSensitivity::Insensitive
    } else {
        CaseSensitivity::Sensitive
    }
}

/// The order two texts of `field` sort in: Totals as numbers, everything else by its
/// case-folded text and then its exact text. A choice of this interface.
fn compare(field: SpielordersConstant, left: &str, right: &str) -> Ordering {
    if field == SPIELORDERS_ORDER_FIELD_TOTAL {
        let number = |text: &str| text.parse::<f64>().unwrap_or(f64::NEG_INFINITY);
        return number(left).total_cmp(&number(right));
    }
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

/// Sorts healthy rows by `field`, leaving damaged rows after them in their own order.
fn sorted(mut rows: Vec<Row>, field: SpielordersConstant, descending: bool) -> Vec<Row> {
    if field == SpielordersConstant::NONE {
        return rows;
    }
    rows.sort_by(|left, right| match (left.damaged, right.damaged) {
        (false, false) => {
            let text = |row: &Row| {
                row.facts
                    .get(field)
                    .map(|text| text.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let order = compare(field, &text(left), &text(right));
            if descending { order.reverse() } else { order }
        }
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (true, true) => Ordering::Equal,
    });
    rows
}

fn sort_field(
    family: SpielordersConstant,
    field: SpielordersConstant,
) -> Result<SpielordersConstant, SpielordersResult> {
    if field == SpielordersConstant::NONE || is_member(family, field) {
        Ok(field)
    } else {
        Err(SPIELORDERS_RESULT_BAD_ARGUMENT)
    }
}

fn unless_cancelled<T>(session: &Session, value: T) -> Result<T, SpielordersResult> {
    if session.is_cancelled() {
        Err(SPIELORDERS_RESULT_CANCELLED)
    } else {
        Ok(value)
    }
}

/// The Orders matching `query` (or NULL), in the Package's own order or sorted by
/// `sort_field` (a `SPIELORDERS_ORDER_FIELD_*`, or -1). `flags`:
/// SPIELORDERS_LIST_CASE_SENSITIVE. Damaged Order Directories appear, after every
/// healthy Order, only when the query is NULL or empty.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_orders(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    flags: u32,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            let text = query(query_text)?;
            let sort = sort_field(SPIELORDERS_FAMILY_ORDER_FIELD, sort)?;
            let opened = package.healthy(entries)?;
            let contents = &opened.contents;
            let found = settle(
                opened.orders_matching(&SearchQuery::new(&text, case(flags))),
                &session,
                Subject::Order,
                entries,
            )?;
            let rows = found
                .into_iter()
                .map(|entry| match entry {
                    OrderListEntry::Healthy { position, order } => Row {
                        damaged: false,
                        path: path_text(&order.paths.directory.absolute_path),
                        facts: Facts::of(order_facts(&order, contents.vendor_for_order(position))),
                    },
                    OrderListEntry::Damaged(order) => {
                        let directory = &order.paths.directory;
                        let damage = contents
                            .diagnostic_groups
                            .iter()
                            .filter(|group| group.location == *directory)
                            .flat_map(|group| &group.diagnostics)
                            .next()
                            .map(|finding| finding.code.code().to_owned())
                            .unwrap_or_default();
                        Row {
                            damaged: true,
                            path: path_text(&directory.absolute_path),
                            facts: Facts::of(vec![
                                (
                                    SPIELORDERS_ORDER_FIELD_ORDER_ID,
                                    order
                                        .order_id
                                        .as_ref()
                                        .map(|id| id.to_string())
                                        .unwrap_or_default(),
                                ),
                                (
                                    SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME,
                                    directory_name(&directory.absolute_path),
                                ),
                                (SPIELORDERS_ORDER_FIELD_DAMAGE, damage),
                            ]),
                        }
                    }
                })
                .collect();
            unless_cancelled(
                &session,
                SpielordersList::of(sorted(rows, sort, descending != 0)),
            )
        })();
        deliver(result, list)
    })
}

/// The Vendors matching `query` (or NULL), in the Package's own order or sorted by
/// `sort_field` (a `SPIELORDERS_VENDOR_FIELD_*`, or -1). `flags`:
/// SPIELORDERS_LIST_CASE_SENSITIVE, SPIELORDERS_LIST_INCLUDE_HIDDEN.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_vendors(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    flags: u32,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            let text = query(query_text)?;
            let sort = sort_field(SPIELORDERS_FAMILY_VENDOR_FIELD, sort)?;
            let visibility = if flags & SPIELORDERS_LIST_INCLUDE_HIDDEN == 0 {
                VendorVisibility::Offered
            } else {
                VendorVisibility::All
            };
            let found = settle(
                package
                    .healthy(entries)?
                    .vendors_matching(&SearchQuery::new(&text, case(flags)), visibility),
                &session,
                Subject::Vendor,
                entries,
            )?;
            let rows = found
                .iter()
                .map(|vendor| Row {
                    damaged: false,
                    path: CString::default(),
                    facts: Facts::of(vendor_facts(vendor)),
                })
                .collect();
            unless_cancelled(
                &session,
                SpielordersList::of(sorted(rows, sort, descending != 0)),
            )
        })();
        deliver(result, list)
    })
}

/// The healthy Orders whose Vendor is `vendor_id`, in the Package's own order.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_orders_of_vendor(
    package: &SpielordersPackage,
    vendor_id: Text<'_>,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            let vendor_id = vendor_id.to_str().ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let contents = &package.healthy(entries)?.contents;
            let rows = contents
                .healthy_orders
                .iter()
                .enumerate()
                .filter(|(_, order)| {
                    order
                        .document
                        .vendor_id
                        .as_ref()
                        .is_some_and(|id| id.to_string() == vendor_id)
                })
                .map(|(position, order)| Row {
                    damaged: false,
                    path: path_text(&order.paths.directory.absolute_path),
                    facts: Facts::of(order_facts(order, contents.vendor_for_order(position))),
                })
                .collect();
            unless_cancelled(&session, SpielordersList::of(rows))
        })();
        deliver(result, list)
    })
}

/// The current values of one reusable-value field (SPIELORDERS_ORDER_FIELD_ORDERER,
/// _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP), each row read with
/// `SPIELORDERS_VALUE_FIELD_*`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_values(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::uncancellable();
        let result = (|| {
            let kind = match field {
                SPIELORDERS_ORDER_FIELD_ORDERER => ReusableValueKind::Orderer,
                SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION => {
                    ReusableValueKind::OrderingInstitution
                }
                SPIELORDERS_ORDER_FIELD_PROJECT => ReusableValueKind::Project,
                SPIELORDERS_ORDER_FIELD_ORDER_GROUP => ReusableValueKind::OrderGroup,
                _ => return Err(SPIELORDERS_RESULT_BAD_ARGUMENT),
            };
            let values = settle(
                package.healthy(entries)?.values(kind),
                &session,
                Subject::Order,
                entries,
            )?;
            Ok(SpielordersList::of(
                values
                    .into_iter()
                    .map(|value| Row {
                        damaged: false,
                        path: CString::default(),
                        facts: Facts::of(vec![
                            (SPIELORDERS_VALUE_FIELD_VALUE, value.value),
                            (
                                SPIELORDERS_VALUE_FIELD_USAGE_COUNT,
                                value.usage_count.to_string(),
                            ),
                        ]),
                    })
                    .collect(),
            ))
        })();
        deliver(result, list)
    })
}

/// How many rows `list` holds.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_list_count(list: &SpielordersList) -> i64 {
    quietly(-1, || i64::try_from(list.data.rows.len()).unwrap_or(-1))
}

/// 1 when row `index` is a Damaged Order Directory, otherwise 0.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_list_damaged(list: &SpielordersList, index: i64) -> i32 {
    quietly(0, || {
        list.row(index).map_or(0, |row| i32::from(row.damaged))
    })
}

/// A field of row `index`. A damaged row answers only SPIELORDERS_ORDER_FIELD_ORDER_ID
/// (its conflicting identity, for a duplicate Order ID), _DIRECTORY_NAME, and _DAMAGE.
/// "" for an absent fact, a field the row does not have, or out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_list_field(
    list: &SpielordersList,
    index: i64,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        list.row(index)
            .map_or(Text::new(EMPTY), |row| row.facts.text(field))
    })
}

/// The Order Directory of row `index` of an Order list; "" for other lists or out of
/// range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_list_path(list: &SpielordersList, index: i64) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        list.row(index)
            .map_or(PathBytes::new(EMPTY), |row| PathBytes::new(&row.path))
    })
}

/// Frees `list`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_list_free(list: Option<Box<SpielordersList>>) {
    quietly((), || drop(list));
}
