//! Drafts: an editor's whole Order, Vendor, or file text, holding whatever the user typed.
//! Value errors come from checking or planning a draft, so an editor never loses input.

use crate::interface::{
    PathBytes, SPIELORDERS_CONTACT_FIELD_EMAIL, SPIELORDERS_CONTACT_FIELD_NAME,
    SPIELORDERS_CONTACT_FIELD_PHONE, SPIELORDERS_CONTACT_FIELD_ROLE, SPIELORDERS_DRAFT_KIND_ORDER,
    SPIELORDERS_DRAFT_KIND_TEXT, SPIELORDERS_DRAFT_KIND_VENDOR,
    SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION, SPIELORDERS_FAILURE_INVALID_ITEM_POSITION,
    SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, SPIELORDERS_ITEM_FIELD_DESCRIPTION,
    SPIELORDERS_ITEM_FIELD_QUANTITY, SPIELORDERS_ITEM_FIELD_TOTAL,
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE, SPIELORDERS_ORDER_FIELD_CURRENCY,
    SPIELORDERS_ORDER_FIELD_DATE_ORDERED, SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
    SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
    SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_STATUS, SPIELORDERS_ORDER_FIELD_TITLE,
    SPIELORDERS_ORDER_FIELD_TOTAL, SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_RESULT_BAD_ARGUMENT,
    SPIELORDERS_RESULT_INTERNAL, SPIELORDERS_RESULT_OK, SPIELORDERS_RESULT_REFUSED,
    SPIELORDERS_SEVERITY_ERROR, SPIELORDERS_TEXT_FIELD_CONTENT, SPIELORDERS_VENDOR_FIELD_ADDRESS,
    SPIELORDERS_VENDOR_FIELD_EMAIL, SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE,
    SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES, SPIELORDERS_VENDOR_FIELD_WEBSITE,
    SpielordersConstant, SpielordersEntries, SpielordersOrder, SpielordersPackage,
    SpielordersResult, SpielordersVendor, Text,
    call::{Session, deliver, fallible, quietly, settle},
    entries::{EMPTY, Entry, Subject, c_text},
    library::name,
    records::{Facts, contact_facts, item_facts, number, order_facts, vendor_facts},
};
use crate::{
    Assignments, Change, OrderRef, draft_date_ordered, draft_item_total, draft_order_total,
    model::{OrderDocument, Vendor},
    new_order_defaults,
    ports::Effects,
};
use std::{
    ffi::CString,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

const ORDER_FIELDS: [SpielordersConstant; 9] = [
    SPIELORDERS_ORDER_FIELD_TITLE,
    SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
    SPIELORDERS_ORDER_FIELD_STATUS,
    SPIELORDERS_ORDER_FIELD_VENDOR,
    SPIELORDERS_ORDER_FIELD_ORDERER,
    SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
    SPIELORDERS_ORDER_FIELD_CURRENCY,
    SPIELORDERS_ORDER_FIELD_PROJECT,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
];
const ITEM_FIELDS: [SpielordersConstant; 4] = [
    SPIELORDERS_ITEM_FIELD_DESCRIPTION,
    SPIELORDERS_ITEM_FIELD_QUANTITY,
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE,
    SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER,
];
const VENDOR_FIELDS: [SpielordersConstant; 6] = [
    SPIELORDERS_VENDOR_FIELD_NAME,
    SPIELORDERS_VENDOR_FIELD_PHONE,
    SPIELORDERS_VENDOR_FIELD_EMAIL,
    SPIELORDERS_VENDOR_FIELD_WEBSITE,
    SPIELORDERS_VENDOR_FIELD_ADDRESS,
    SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
];
const CONTACT_FIELDS: [SpielordersConstant; 4] = [
    SPIELORDERS_CONTACT_FIELD_NAME,
    SPIELORDERS_CONTACT_FIELD_ROLE,
    SPIELORDERS_CONTACT_FIELD_PHONE,
    SPIELORDERS_CONTACT_FIELD_EMAIL,
];
const REUSABLE_FIELDS: [SpielordersConstant; 4] = [
    SPIELORDERS_ORDER_FIELD_ORDERER,
    SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
    SPIELORDERS_ORDER_FIELD_PROJECT,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
];

/// What a draft saves as when it is planned.
#[derive(Clone, Debug)]
pub(crate) enum Origin {
    NewOrder {
        root: PathBuf,
    },
    EditOrder {
        directory: PathBuf,
        original: Box<OrderDocument>,
    },
    Adoption {
        root: PathBuf,
        directory: PathBuf,
    },
    NewVendor {
        root: PathBuf,
    },
    EditVendor {
        root: PathBuf,
        original: Box<Vendor>,
    },
    /// A diagnosis draft, which only its diagnosis plans.
    Repair,
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The row of the read record this row began as, if it did.
    pub(crate) origin: Option<usize>,
    facts: Vec<(SpielordersConstant, String)>,
}

/// An editor's draft of one Order, one Vendor, or one file's text. Use it from one thread
/// at a time. Its strings live until the next call that changes it.
#[repr(C)]
pub struct SpielordersDraft {
    data: Box<SpielordersDraftData>,
}

pub(crate) struct SpielordersDraftData {
    pub(crate) kind: SpielordersConstant,
    pub(crate) origin: Origin,
    poisoned: AtomicBool,
    facts: Vec<(SpielordersConstant, String)>,
    pub(crate) rows: Vec<Row>,
    confirmed: Vec<SpielordersConstant>,
    initial: Initial,
    /// What the accessors read, rebuilt after every change.
    shown: Shown,
}

#[derive(Default)]
struct Shown {
    facts: Facts,
    rows: Vec<Facts>,
    total: CString,
}

impl SpielordersDraftData {
    pub(crate) fn settable(&self) -> &'static [SpielordersConstant] {
        match self.kind {
            SPIELORDERS_DRAFT_KIND_ORDER => &ORDER_FIELDS,
            SPIELORDERS_DRAFT_KIND_VENDOR => &VENDOR_FIELDS,
            _ => &[SPIELORDERS_TEXT_FIELD_CONTENT],
        }
    }

    pub(crate) fn row_settable(&self) -> &'static [SpielordersConstant] {
        match self.kind {
            SPIELORDERS_DRAFT_KIND_ORDER => &ITEM_FIELDS,
            SPIELORDERS_DRAFT_KIND_VENDOR => &CONTACT_FIELDS,
            _ => &[],
        }
    }

    pub(crate) fn fact(&self, field: SpielordersConstant) -> &str {
        self.facts
            .iter()
            .find(|(each, _)| *each == field)
            .map_or("", |(_, value)| value.as_str())
    }

    fn put(
        facts: &mut Vec<(SpielordersConstant, String)>,
        field: SpielordersConstant,
        text: String,
    ) {
        match facts.iter_mut().find(|(each, _)| *each == field) {
            Some((_, value)) => *value = text,
            None => facts.push((field, text)),
        }
    }

    fn row_value(row: &Row, field: SpielordersConstant) -> &str {
        row.facts
            .iter()
            .find(|(each, _)| *each == field)
            .map_or("", |(_, value)| value.as_str())
    }

    fn refresh(&mut self) {
        let mut rows = Vec::with_capacity(self.rows.len());
        let mut totals = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let mut facts = row.facts.clone();
            if self.kind == SPIELORDERS_DRAFT_KIND_ORDER {
                let total = draft_item_total(
                    Self::row_value(row, SPIELORDERS_ITEM_FIELD_QUANTITY),
                    Self::row_value(row, SPIELORDERS_ITEM_FIELD_UNIT_PRICE),
                );
                totals.push(total);
                facts.push((
                    SPIELORDERS_ITEM_FIELD_TOTAL,
                    total.map(number).unwrap_or_default(),
                ));
            }
            rows.push(Facts::of(facts));
        }
        let total = if self.kind == SPIELORDERS_DRAFT_KIND_ORDER {
            draft_order_total(&totals).map(number).unwrap_or_default()
        } else {
            String::new()
        };
        let mut facts = self.facts.clone();
        if self.kind == SPIELORDERS_DRAFT_KIND_ORDER {
            facts.push((SPIELORDERS_ORDER_FIELD_TOTAL, total.clone()));
        }
        self.shown = Shown {
            facts: Facts::of(facts),
            rows,
            total: c_text(total),
        };
    }

    /// The facts and rows as the assignments a plan takes.
    pub(crate) fn assignments(&self) -> (Assignments, Vec<Assignments>) {
        let mut facts = Assignments::new();
        for (field, value) in &self.facts {
            let field_name = name(*field).unwrap_or_default();
            facts = if self.confirmed.contains(field) {
                facts.introduce(field_name, value.clone())
            } else {
                facts.assign(field_name, value.clone())
            };
        }
        let rows = self
            .rows
            .iter()
            .map(|row| {
                row.facts
                    .iter()
                    .fold(Assignments::new(), |row, (field, value)| {
                        row.assign(name(*field).unwrap_or_default(), value.clone())
                    })
            })
            .collect();
        (facts, rows)
    }

    /// The change this draft saves as, or None for a diagnosis draft.
    pub(crate) fn change(&self) -> Option<Change> {
        let (assignments, rows) = self.assignments();
        Some(match &self.origin {
            Origin::NewOrder { root } => Change::CreateOrder {
                root: root.clone(),
                assignments,
                items: rows,
            },
            Origin::EditOrder {
                directory,
                original,
            } => Change::SaveOrder {
                order: OrderRef::Directory(directory.clone()),
                assignments,
                items: rows,
                original: (**original).clone(),
            },
            Origin::Adoption { root, directory } => Change::AdoptOrder {
                root: root.clone(),
                directory: directory.clone(),
                assignments,
                items: rows,
            },
            Origin::NewVendor { root } => Change::CreateVendor {
                root: root.clone(),
                assignments,
                contacts: rows,
            },
            Origin::EditVendor { root, original } => Change::SaveVendor {
                root: root.clone(),
                assignments,
                contacts: rows,
                original: (**original).clone(),
            },
            Origin::Repair => return None,
        })
    }

    pub(crate) fn subject(&self) -> Subject {
        match self.kind {
            SPIELORDERS_DRAFT_KIND_ORDER => Subject::Order,
            SPIELORDERS_DRAFT_KIND_VENDOR => Subject::Vendor,
            _ => Subject::Text,
        }
    }

    /// The text of `field`, for a diagnosis that plans this draft.
    pub(crate) fn text(&self, field: SpielordersConstant) -> String {
        self.fact(field).to_owned()
    }

    /// The record this draft began from, as its facts and rows.
    pub(crate) fn initial(&self) -> &Initial {
        &self.initial
    }
}

/// What a draft held when it was made: the read it edits, or nothing for a new record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Initial {
    pub(crate) facts: Vec<(SpielordersConstant, String)>,
    pub(crate) rows: Vec<Vec<(SpielordersConstant, String)>>,
}

impl SpielordersDraft {
    pub(crate) fn new(
        kind: SpielordersConstant,
        origin: Origin,
        facts: Vec<(SpielordersConstant, String)>,
        rows: Vec<Vec<(SpielordersConstant, String)>>,
    ) -> Self {
        let mut data = SpielordersDraftData {
            kind,
            origin,
            poisoned: AtomicBool::new(false),
            facts: Vec::new(),
            rows: rows
                .into_iter()
                .enumerate()
                .map(|(index, facts)| Row {
                    origin: Some(index),
                    facts,
                })
                .collect(),
            confirmed: Vec::new(),
            initial: Initial::default(),
            shown: Shown::default(),
        };
        let settable = data.settable();
        data.facts = settable
            .iter()
            .map(|field| {
                let value = facts
                    .iter()
                    .find(|(each, _)| each == field)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default();
                (*field, value)
            })
            .collect();
        let row_settable = data.row_settable();
        for row in &mut data.rows {
            row.facts.retain(|(field, _)| row_settable.contains(field));
        }
        let new_record = matches!(
            data.origin,
            Origin::NewOrder { .. } | Origin::NewVendor { .. } | Origin::Adoption { .. }
        );
        if !new_record {
            data.initial = Initial {
                facts: data.facts.clone(),
                rows: data.rows.iter().map(|row| row.facts.clone()).collect(),
            };
        }
        data.refresh();
        Self {
            data: Box::new(data),
        }
    }

    /// Runs `body` on this draft unless it is poisoned; a panic poisons it.
    pub(crate) fn guarded<T>(
        &self,
        poisoned: T,
        panicked: T,
        body: impl FnOnce(&SpielordersDraftData) -> T,
    ) -> T {
        if self.data.poisoned.load(Ordering::SeqCst) {
            return poisoned;
        }
        catch_unwind(AssertUnwindSafe(|| body(&self.data))).unwrap_or_else(|_| {
            self.data.poisoned.store(true, Ordering::SeqCst);
            panicked
        })
    }

    fn change(
        &mut self,
        outcome: Option<&mut Option<Box<SpielordersEntries>>>,
        body: impl FnOnce(&mut SpielordersDraftData, &mut Vec<Entry>) -> SpielordersResult,
    ) -> SpielordersResult {
        fallible(outcome, |entries| {
            if self.data.poisoned.load(Ordering::SeqCst) {
                return SPIELORDERS_RESULT_BAD_ARGUMENT;
            }
            let data = &mut *self.data;
            match catch_unwind(AssertUnwindSafe(|| {
                let result = body(data, entries);
                data.refresh();
                result
            })) {
                Ok(result) => result,
                Err(_) => {
                    self.data.poisoned.store(true, Ordering::SeqCst);
                    SPIELORDERS_RESULT_INTERNAL
                }
            }
        })
    }
}

fn index(row: i64, count: usize, inclusive: bool) -> Option<usize> {
    let row = usize::try_from(row).ok()?;
    (row < count || (inclusive && row == count)).then_some(row)
}

/// The refusal of a row position a draft does not have.
fn bad_row(data: &SpielordersDraftData, row: i64, entries: &mut Vec<Entry>) -> SpielordersResult {
    if data.kind == SPIELORDERS_DRAFT_KIND_TEXT {
        return SPIELORDERS_RESULT_BAD_ARGUMENT;
    }
    let code = if data.kind == SPIELORDERS_DRAFT_KIND_ORDER {
        SPIELORDERS_FAILURE_INVALID_ITEM_POSITION
    } else {
        SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION
    };
    let mut entry = Entry::new(
        code,
        SPIELORDERS_SEVERITY_ERROR,
        c_text(format!("No row {row}; the draft has {}", data.rows.len())),
    );
    entry.row = row;
    entries.push(entry);
    SPIELORDERS_RESULT_REFUSED
}

/// A draft of `order`, saved against this read: a save refuses if the Order changed on
/// disk after it.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_draft(
    order: &SpielordersOrder,
    draft: &mut Option<Box<SpielordersDraft>>,
) -> SpielordersResult {
    fallible(None, |_| {
        let read = &order.data().read;
        let rows = read
            .order
            .document
            .items
            .iter()
            .map(|item| item_facts(item, None))
            .collect();
        deliver(
            Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_ORDER,
                Origin::EditOrder {
                    directory: order.directory().to_path_buf(),
                    original: Box::new(read.order.document.clone()),
                },
                order_facts(&read.order, read.vendor.as_ref()),
                rows,
            )),
            draft,
        )
    })
}

/// A draft of `vendor`, saved against this read.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_draft(
    vendor: &SpielordersVendor,
    draft: &mut Option<Box<SpielordersDraft>>,
) -> SpielordersResult {
    fallible(None, |_| {
        let read = &vendor.data().read;
        deliver(
            Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_VENDOR,
                Origin::EditVendor {
                    root: vendor.root(),
                    original: Box::new(read.vendor.clone()),
                },
                vendor_facts(&read.vendor),
                read.vendor.contacts.iter().map(contact_facts).collect(),
            )),
            draft,
        )
    })
}

/// A draft of a new Order in `package`, holding its Date Ordered and Status defaults.
/// Saving it creates the Order with its facts and Item rows.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_new_order_draft(
    package: &SpielordersPackage,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let result = package.healthy(entries).map(|_| {
            let (date, status) = new_order_defaults(Session::uncancellable().today());
            SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_ORDER,
                Origin::NewOrder {
                    root: package.root().to_path_buf(),
                },
                vec![
                    (SPIELORDERS_ORDER_FIELD_DATE_ORDERED, date),
                    (SPIELORDERS_ORDER_FIELD_STATUS, status.to_owned()),
                ],
                Vec::new(),
            )
        });
        deliver(result, draft)
    })
}

/// A draft of a new Vendor in `package`. Saving it creates the Vendor with its facts and
/// Contact rows.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_new_vendor_draft(
    package: &SpielordersPackage,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let result = package.healthy(entries).map(|_| {
            SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_VENDOR,
                Origin::NewVendor {
                    root: package.root().to_path_buf(),
                },
                Vec::new(),
                Vec::new(),
            )
        });
        deliver(result, draft)
    })
}

/// A draft adopting the existing directory `directory` as a new Order of `package`. It
/// holds no defaults and never takes today's date: every fact is the user's.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_adoption_draft(
    package: &SpielordersPackage,
    directory: PathBytes<'_>,
    token: u64,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = package.healthy(entries).and_then(|_| {
            if session.is_cancelled() {
                return Err(crate::interface::SPIELORDERS_RESULT_CANCELLED);
            }
            Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_ORDER,
                Origin::Adoption {
                    root: package.root().to_path_buf(),
                    directory: directory.as_path().to_path_buf(),
                },
                Vec::new(),
                Vec::new(),
            ))
        });
        deliver(result, draft)
    })
}

/// The `SPIELORDERS_DRAFT_KIND_*` of `draft`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_kind(draft: &SpielordersDraft) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || draft.data.kind)
}

/// Sets `field` of `draft` to `text` exactly as typed, and applies the live rules:
/// choosing Ordered as the Status takes today's Date Ordered, except in an adoption
/// draft. BAD_ARGUMENT for a field the draft does not have or text that is not UTF-8;
/// value errors come from checking or planning the draft.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_set(
    draft: &mut SpielordersDraft,
    field: SpielordersConstant,
    text: Text<'_>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, _| {
        let Some(text) = text.to_str() else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        if !data.settable().contains(&field) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        if field == SPIELORDERS_ORDER_FIELD_STATUS
            && !matches!(data.origin, Origin::Adoption { .. })
        {
            let date = draft_date_ordered(
                data.fact(SPIELORDERS_ORDER_FIELD_STATUS),
                text,
                data.fact(SPIELORDERS_ORDER_FIELD_DATE_ORDERED),
                Session::uncancellable().today(),
            );
            SpielordersDraftData::put(&mut data.facts, SPIELORDERS_ORDER_FIELD_DATE_ORDERED, date);
        }
        data.confirmed.retain(|confirmed| *confirmed != field);
        SpielordersDraftData::put(&mut data.facts, field, text.to_owned());
        SPIELORDERS_RESULT_OK
    })
}

/// Inserts an empty row before `row`; `row` equal to the row count appends.
/// BAD_ARGUMENT for a diagnosis draft, which holds no rows.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_insert(
    draft: &mut SpielordersDraft,
    row: i64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, entries| {
        if matches!(data.origin, Origin::Repair) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        match index(row, data.rows.len(), true) {
            Some(at) if data.kind != SPIELORDERS_DRAFT_KIND_TEXT => {
                let facts = data
                    .row_settable()
                    .iter()
                    .map(|field| (*field, String::new()))
                    .collect();
                data.rows.insert(
                    at,
                    Row {
                        origin: None,
                        facts,
                    },
                );
                SPIELORDERS_RESULT_OK
            }
            _ => bad_row(data, row, entries),
        }
    })
}

/// Removes row `row`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_remove(
    draft: &mut SpielordersDraft,
    row: i64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, entries| {
        match index(row, data.rows.len(), false) {
            Some(at) => {
                data.rows.remove(at);
                SPIELORDERS_RESULT_OK
            }
            None => bad_row(data, row, entries),
        }
    })
}

/// Moves row `from` so that it becomes row `to`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_move(
    draft: &mut SpielordersDraft,
    from: i64,
    to: i64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, entries| {
        let count = data.rows.len();
        match (index(from, count, false), index(to, count, false)) {
            (Some(from), Some(to)) => {
                let moved = data.rows.remove(from);
                data.rows.insert(to, moved);
                SPIELORDERS_RESULT_OK
            }
            (None, _) => bad_row(data, from, entries),
            (_, None) => bad_row(data, to, entries),
        }
    })
}

/// Sets `field` of row `row` to `text` exactly as typed, as `spielorders_draft_set` does.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_set(
    draft: &mut SpielordersDraft,
    row: i64,
    field: SpielordersConstant,
    text: Text<'_>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, entries| {
        let Some(text) = text.to_str() else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        if !data.row_settable().contains(&field) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        match index(row, data.rows.len(), false) {
            Some(at) => {
                SpielordersDraftData::put(&mut data.rows[at].facts, field, text.to_owned());
                SPIELORDERS_RESULT_OK
            }
            None => bad_row(data, row, entries),
        }
    })
}

/// Confirms the value `field` now holds as a new reusable value. `field` is
/// SPIELORDERS_ORDER_FIELD_ORDERER, _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP and
/// `row` is -1. Changing the field again withdraws the confirmation.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_confirm(
    draft: &mut SpielordersDraft,
    field: SpielordersConstant,
    row: i64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |data, _| {
        if data.kind != SPIELORDERS_DRAFT_KIND_ORDER
            || row != -1
            || !REUSABLE_FIELDS.contains(&field)
        {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        if !data.confirmed.contains(&field) {
            data.confirmed.push(field);
        }
        SPIELORDERS_RESULT_OK
    })
}

/// The text `field` of `draft` holds, or the live SPIELORDERS_ORDER_FIELD_TOTAL; "" for
/// a field the draft does not have.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_field(
    draft: &SpielordersDraft,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || draft.data.shown.facts.text(field))
}

/// How many rows `draft` holds.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_count(draft: &SpielordersDraft) -> i64 {
    quietly(-1, || i64::try_from(draft.data.rows.len()).unwrap_or(-1))
}

/// The text `field` of row `row` holds, or its live SPIELORDERS_ITEM_FIELD_TOTAL; "" out
/// of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_row_field(
    draft: &SpielordersDraft,
    row: i64,
    field: SpielordersConstant,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        usize::try_from(row)
            .ok()
            .and_then(|row| draft.data.shown.rows.get(row))
            .map_or(Text::new(EMPTY), |facts| facts.text(field))
    })
}

/// The live Order Total of an Order draft, unrounded; "" while a row's numbers do not
/// parse, and for other drafts.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_total(draft: &SpielordersDraft) -> Text<'_> {
    quietly(Text::new(EMPTY), || Text::new(&draft.data.shown.total))
}

/// Every error saving `draft` would refuse with now, each located by field and row,
/// without making a plan. OK when there are none.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_check(
    draft: &SpielordersDraft,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        draft.guarded(
            SPIELORDERS_RESULT_BAD_ARGUMENT,
            SPIELORDERS_RESULT_INTERNAL,
            |data| {
                let Some(change) = data.change() else {
                    return SPIELORDERS_RESULT_BAD_ARGUMENT;
                };
                let session = Session::uncancellable();
                match settle(
                    crate::plan(&change, &session),
                    &session,
                    data.subject(),
                    entries,
                ) {
                    Ok(_) => SPIELORDERS_RESULT_OK,
                    Err(status) => status,
                }
            },
        )
    })
}

/// Frees `draft`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_free(draft: Option<Box<SpielordersDraft>>) {
    quietly((), || drop(draft));
}
