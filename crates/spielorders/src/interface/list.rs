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
    fields,
    library::is_member,
    package::text_or_empty,
    records::{Facts, directory_name, order_facts, path_text, vendor_facts},
};
use crate::interface::{
    SPIELORDERS_CONTACT_FIELD_EMAIL, SPIELORDERS_CONTACT_FIELD_NAME,
    SPIELORDERS_CONTACT_FIELD_PHONE, SPIELORDERS_CONTACT_FIELD_ROLE,
    SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, SPIELORDERS_ITEM_FIELD_DESCRIPTION,
    SPIELORDERS_ITEM_FIELD_NOTES, SPIELORDERS_ITEM_FIELD_QUANTITY,
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE, SPIELORDERS_ORDER_FIELD_CURRENCY,
    SPIELORDERS_ORDER_FIELD_DATE_ORDERED, SPIELORDERS_ORDER_FIELD_STATUS,
    SPIELORDERS_ORDER_FIELD_TITLE, SPIELORDERS_ORDER_FIELD_VENDOR,
    SPIELORDERS_VENDOR_FIELD_ADDRESS, SPIELORDERS_VENDOR_FIELD_EMAIL,
    SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE,
    SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES, SPIELORDERS_VENDOR_FIELD_VENDOR_ID,
    SPIELORDERS_VENDOR_FIELD_WEBSITE, SpielordersSearch,
};
use crate::{
    Coded,
    model::{
        CaseSensitivity, OrderFact, OrderListEntry, SearchQuery, VendorFact, VendorVisibility,
    },
};
use std::{cmp::Ordering, ffi::CString, path::PathBuf};

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

fn sensitivity(flags: u32) -> CaseSensitivity {
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
    crate::interface::call::put(list, None);
    order_list(
        package,
        query_text,
        flags,
        SearchScope::All,
        sort,
        descending,
        token,
        list,
        outcome,
    )
}

/// How many facts an Order search can be confined to.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_search_field_count() -> i64 {
    ORDER_SEARCH_FIELDS.len() as i64
}

/// Fact `index` an Order search can be confined to, in the order a menu offers them: a
/// `SPIELORDERS_ORDER_FIELD_*` or `SPIELORDERS_ITEM_FIELD_*` constant, or -1 out of range.
/// SPIELORDERS_ORDER_FIELD_VENDOR matches the referenced Vendor's Name; an Item fact
/// matches any of the Order's Items.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_search_field(index: i64) -> SpielordersConstant {
    usize::try_from(index)
        .ok()
        .and_then(|index| ORDER_SEARCH_FIELDS.get(index))
        .map_or(SpielordersConstant::NONE, |(constant, _)| *constant)
}

/// The Orders matching `query` (or NULL) in one searchable fact, `field`, from
/// spielorders_order_search_field, or in every searchable fact for -1. Otherwise as
/// spielorders_package_orders; SPIELORDERS_RESULT_BAD_ARGUMENT for any other `field`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_search_orders(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    field: SpielordersConstant,
    flags: u32,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    let scope = if field == SpielordersConstant::NONE {
        Some(SearchScope::All)
    } else {
        ORDER_SEARCH_FIELDS
            .iter()
            .find(|(constant, _)| *constant == field)
            .map(|(_, fact)| SearchScope::Fact(*fact))
    };
    match scope {
        Some(scope) => order_list(
            package, query_text, flags, scope, sort, descending, token, list, outcome,
        ),
        None => refuse_argument(list, outcome),
    }
}

/// Folder Contents: the Orders whose facts match `query` (or NULL) in every searchable
/// fact, under `flags` as for spielorders_package_orders, together with every Order holding
/// a file of `material`, a Supporting Material search of this Package (or NULL). Each
/// appears once, sorted as spielorders_package_orders sorts; case applies only to facts.
/// Read an Order's matched files from `material` by spielorders_search_order_directory,
/// which spells the Order Directory as spielorders_list_path does.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_search_contents(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    flags: u32,
    material: Option<&SpielordersSearch>,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    order_list(
        package,
        query_text,
        flags,
        SearchScope::Contents(material.map(SpielordersSearch::orders).unwrap_or_default()),
        sort,
        descending,
        token,
        list,
        outcome,
    )
}

/// What an Order search reaches.
enum SearchScope<'a> {
    All,
    Fact(OrderFact),
    Contents(&'a [PathBuf]),
}

/// Each fact an Order search can be confined to, by the constant a caller names it with.
const ORDER_SEARCH_FIELDS: [(SpielordersConstant, OrderFact); 15] = [
    (SPIELORDERS_ORDER_FIELD_TITLE, OrderFact::Title),
    (SPIELORDERS_ORDER_FIELD_DATE_ORDERED, OrderFact::DateOrdered),
    (SPIELORDERS_ORDER_FIELD_STATUS, OrderFact::Status),
    (SPIELORDERS_ORDER_FIELD_VENDOR, OrderFact::Vendor),
    (SPIELORDERS_ORDER_FIELD_ORDERER, OrderFact::Orderer),
    (
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        OrderFact::OrderingInstitution,
    ),
    (SPIELORDERS_ORDER_FIELD_PROJECT, OrderFact::Project),
    (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, OrderFact::OrderGroup),
    (SPIELORDERS_ORDER_FIELD_CURRENCY, OrderFact::Currency),
    (SPIELORDERS_ORDER_FIELD_ORDER_ID, OrderFact::OrderId),
    (SPIELORDERS_ITEM_FIELD_DESCRIPTION, OrderFact::Description),
    (
        SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER,
        OrderFact::CatalogNumber,
    ),
    (SPIELORDERS_ITEM_FIELD_QUANTITY, OrderFact::Quantity),
    (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, OrderFact::UnitPrice),
    (SPIELORDERS_ITEM_FIELD_NOTES, OrderFact::Notes),
];

/// Each fact a Vendor search can be confined to, by the constant a caller names it with.
const VENDOR_SEARCH_FIELDS: [(SpielordersConstant, VendorFact); 11] = [
    (SPIELORDERS_VENDOR_FIELD_NAME, VendorFact::Name),
    (SPIELORDERS_VENDOR_FIELD_PHONE, VendorFact::Phone),
    (SPIELORDERS_VENDOR_FIELD_EMAIL, VendorFact::Email),
    (SPIELORDERS_VENDOR_FIELD_WEBSITE, VendorFact::Website),
    (SPIELORDERS_VENDOR_FIELD_ADDRESS, VendorFact::Address),
    (
        SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
        VendorFact::PurchasingNotes,
    ),
    (SPIELORDERS_VENDOR_FIELD_VENDOR_ID, VendorFact::VendorId),
    (SPIELORDERS_CONTACT_FIELD_NAME, VendorFact::ContactName),
    (SPIELORDERS_CONTACT_FIELD_ROLE, VendorFact::ContactRole),
    (SPIELORDERS_CONTACT_FIELD_PHONE, VendorFact::ContactPhone),
    (SPIELORDERS_CONTACT_FIELD_EMAIL, VendorFact::ContactEmail),
];

fn refuse_argument(
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |_| {
        deliver::<SpielordersList>(Err(SPIELORDERS_RESULT_BAD_ARGUMENT), list)
    })
}

#[allow(clippy::too_many_arguments)]
fn order_list(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    flags: u32,
    scope: SearchScope<'_>,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            let text = text_or_empty(query_text)?;
            let sort = sort_field(SPIELORDERS_FAMILY_ORDER_FIELD, sort)?;
            let opened = package.healthy(entries)?;
            let contents = &opened.contents;
            let searched = SearchQuery::new(&text, sensitivity(flags));
            let found = settle(
                match scope {
                    SearchScope::All => opened.orders_matching(&searched),
                    SearchScope::Fact(fact) => {
                        opened.orders_matching(&searched.in_order_fact(fact))
                    }
                    SearchScope::Contents(holding) => {
                        opened.orders_matching_or_holding(&searched, holding)
                    }
                },
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
    crate::interface::call::put(list, None);
    vendor_list(
        package, query_text, None, flags, sort, descending, token, list, outcome,
    )
}

/// How many facts a Vendor search can be confined to.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_search_field_count() -> i64 {
    VENDOR_SEARCH_FIELDS.len() as i64
}

/// Fact `index` a Vendor search can be confined to, in the order a menu offers them: a
/// `SPIELORDERS_VENDOR_FIELD_*` or `SPIELORDERS_CONTACT_FIELD_*` constant, or -1 out of
/// range. A Contact fact matches any of the Vendor's Contacts.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_search_field(index: i64) -> SpielordersConstant {
    usize::try_from(index)
        .ok()
        .and_then(|index| VENDOR_SEARCH_FIELDS.get(index))
        .map_or(SpielordersConstant::NONE, |(constant, _)| *constant)
}

/// The Vendors matching `query` (or NULL) in one searchable fact, `field`, from
/// spielorders_vendor_search_field, or in every searchable fact for -1. Otherwise as
/// spielorders_package_vendors; SPIELORDERS_RESULT_BAD_ARGUMENT for any other `field`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_search_vendors(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    field: SpielordersConstant,
    flags: u32,
    sort: SpielordersConstant,
    descending: i32,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    if field == SpielordersConstant::NONE {
        return vendor_list(
            package, query_text, None, flags, sort, descending, token, list, outcome,
        );
    }
    match VENDOR_SEARCH_FIELDS
        .iter()
        .find(|(constant, _)| *constant == field)
    {
        Some((_, fact)) => vendor_list(
            package,
            query_text,
            Some(*fact),
            flags,
            sort,
            descending,
            token,
            list,
            outcome,
        ),
        None => refuse_argument(list, outcome),
    }
}

#[allow(clippy::too_many_arguments)]
fn vendor_list(
    package: &SpielordersPackage,
    query_text: OptionalText<'_>,
    fact: Option<VendorFact>,
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
            let text = text_or_empty(query_text)?;
            let sort = sort_field(SPIELORDERS_FAMILY_VENDOR_FIELD, sort)?;
            let visibility = if flags & SPIELORDERS_LIST_INCLUDE_HIDDEN == 0 {
                VendorVisibility::Offered
            } else {
                VendorVisibility::All
            };
            let found = settle(
                package.healthy(entries)?.vendors_matching(
                    &match fact {
                        Some(fact) => {
                            SearchQuery::new(&text, sensitivity(flags)).in_vendor_fact(fact)
                        }
                        None => SearchQuery::new(&text, sensitivity(flags)),
                    },
                    visibility,
                ),
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
    crate::interface::call::put(list, None);
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

/// The healthy Orders that use `value` of one reusable-value field
/// (SPIELORDERS_ORDER_FIELD_ORDERER, _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP), in
/// the Package's own order, each row read as spielorders_package_orders_of_vendor's are.
/// `value` matches as reuse does. An Order Group is one Project's: `project` names it, or
/// NULL or "" the Orders with no Project, and is ignored for any other field. The rows are
/// the Orders that spielorders_package_values, or spielorders_package_project_groups for
/// a group, counts for that value.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_orders_with_value(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    value: Text<'_>,
    project: OptionalText<'_>,
    token: u64,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            let kind = fields::reusable(field).ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let value = value.to_str().ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let project = text_or_empty(project)?;
            let contents = &package.healthy(entries)?.contents;
            let rows = contents
                .healthy_orders
                .iter()
                .enumerate()
                .filter(|(_, order)| {
                    crate::values::uses(&order.document, kind, Some(project.as_str()), value)
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

/// The Order Groups of `project`, or of the Orders with no Project for NULL or "", each
/// row read with `SPIELORDERS_VALUE_FIELD_*`. A group belongs to the Project of the Orders
/// that use it, so these are the suggestions for an Order in that Project, and the same
/// name under two Projects is two groups. `project` matches as reuse does: trimmed and
/// case-insensitively.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_project_groups(
    package: &SpielordersPackage,
    project: OptionalText<'_>,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    fallible(outcome, |entries| {
        let result = (|| {
            let project = text_or_empty(project)?;
            let groups = package
                .healthy(entries)?
                .contents
                .order_groups_of(Some(project.as_str()));
            Ok(value_list(groups))
        })();
        deliver(result, list)
    })
}

/// The current values of one reusable-value field (SPIELORDERS_ORDER_FIELD_ORDERER,
/// _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP), each row read with
/// `SPIELORDERS_VALUE_FIELD_*`. Order Groups are every group name across Projects, a
/// name used under several counted once; spielorders_package_project_groups gives one
/// Project's groups.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_values(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    list: &mut Option<Box<SpielordersList>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(list, None);
    fallible(outcome, |entries| {
        let session = Session::uncancellable();
        let result = (|| {
            let kind = fields::reusable(field).ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let values = settle(
                package.healthy(entries)?.values(kind),
                &session,
                Subject::Order,
                entries,
            )?;
            Ok(value_list(values))
        })();
        deliver(result, list)
    })
}

/// 1 when `left` and `right` name the same reusable value: surrounding whitespace is
/// ignored and Unicode letter case is folded. Canonically equivalent Unicode spellings
/// are not otherwise normalized. Returns 0 for different values or invalid UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_value_matches(left: Text<'_>, right: Text<'_>) -> i32 {
    quietly(0, || match (left.to_str(), right.to_str()) {
        (Some(left), Some(right)) => i32::from(crate::values::same_value(left, right)),
        _ => 0,
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

/// One row per reusable value, with how many healthy Orders use it.
pub(super) fn value_list(
    values: Vec<crate::canonical::ReusableValueProjection>,
) -> SpielordersList {
    SpielordersList::of(
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
    )
}
