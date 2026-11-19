//! Drafts as the C interface hands them out: the library's [`Draft`] behind a handle, its
//! fields read and set by their constants, and the text its accessors lend.

use crate::interface::{
    PathBytes, SPIELORDERS_DRAFT_KIND_ORDER, SPIELORDERS_DRAFT_KIND_TEXT,
    SPIELORDERS_DRAFT_KIND_VENDOR, SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION,
    SPIELORDERS_FAILURE_INVALID_ITEM_POSITION, SPIELORDERS_FAMILY_CONTACT_FIELD,
    SPIELORDERS_FAMILY_ITEM_FIELD, SPIELORDERS_FAMILY_ORDER_FIELD, SPIELORDERS_FAMILY_VENDOR_FIELD,
    SPIELORDERS_ITEM_FIELD_TOTAL, SPIELORDERS_ORDER_FIELD_TOTAL, SPIELORDERS_REASON_UNKNOWN_FIELD,
    SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_INTERNAL, SPIELORDERS_RESULT_OK,
    SPIELORDERS_RESULT_REFUSED, SPIELORDERS_SEVERITY_ERROR, SpielordersConstant,
    SpielordersEntries, SpielordersOrder, SpielordersPackage, SpielordersResult, SpielordersVendor,
    Text,
    call::{Session, deliver, fallible, quietly, settle},
    codes,
    entries::{EMPTY, Entry, Subject, c_text},
    fields,
    records::{Facts, contact_facts, item_facts, number, order_facts, vendor_facts},
};
use crate::{
    Change,
    drafting::{Draft, DraftKind, DraftRefusal, Origin},
    ports::Effects,
};
use std::{
    ffi::CString,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, Ordering},
};

/// An editor's draft of one Order, one Vendor, or one file's text. Use it from one thread
/// at a time. Its strings live until the next call that changes it.
#[repr(C)]
pub struct SpielordersDraft {
    data: Box<SpielordersDraftData>,
}

pub(crate) struct SpielordersDraftData {
    draft: Draft,
    poisoned: AtomicBool,
    /// What the accessors read, rebuilt after every change.
    shown: Shown,
}

#[derive(Default)]
struct Shown {
    facts: Facts,
    rows: Vec<Facts>,
    total: CString,
}

/// What a draft held when it was made, by the constants of its fields: the read it
/// edits, or nothing for a new record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Initial {
    pub(crate) facts: Vec<(SpielordersConstant, String)>,
    pub(crate) rows: Vec<Vec<(SpielordersConstant, String)>>,
}

impl SpielordersDraftData {
    /// The `SPIELORDERS_DRAFT_KIND_*` of this draft.
    pub(crate) fn kind(&self) -> SpielordersConstant {
        match self.draft.kind() {
            DraftKind::Order => SPIELORDERS_DRAFT_KIND_ORDER,
            DraftKind::Vendor => SPIELORDERS_DRAFT_KIND_VENDOR,
            DraftKind::Text => SPIELORDERS_DRAFT_KIND_TEXT,
        }
    }

    pub(crate) fn draft(&self) -> &Draft {
        &self.draft
    }

    /// The fields this draft sets, in the order an editor lists them.
    pub(crate) fn settable(&self) -> Vec<SpielordersConstant> {
        self.draft
            .facts()
            .iter()
            .map(|(fact, _)| fields::fact_constant(*fact))
            .collect()
    }

    /// The fields each of this draft's rows sets, in the order an editor lists them.
    pub(crate) fn row_settable(&self) -> Vec<SpielordersConstant> {
        self.draft
            .kind()
            .row_facts()
            .into_iter()
            .map(fields::row_fact_constant)
            .collect()
    }

    fn refresh(&mut self) {
        let draft = &self.draft;
        let order = draft.kind() == DraftKind::Order;
        let rows = draft
            .rows()
            .iter()
            .map(|row| {
                let mut facts = row
                    .facts()
                    .iter()
                    .map(|(fact, text)| (fields::row_fact_constant(*fact), text.clone()))
                    .collect::<Vec<_>>();
                if order {
                    facts.push((
                        SPIELORDERS_ITEM_FIELD_TOTAL,
                        draft.item_total(row).map(number).unwrap_or_default(),
                    ));
                }
                Facts::of(facts)
            })
            .collect();
        let total = if order {
            draft.order_total().map(number).unwrap_or_default()
        } else {
            String::new()
        };
        let mut facts = draft
            .facts()
            .iter()
            .map(|(fact, text)| (fields::fact_constant(*fact), text.clone()))
            .collect::<Vec<_>>();
        if order {
            facts.push((SPIELORDERS_ORDER_FIELD_TOTAL, total.clone()));
        }
        self.shown = Shown {
            facts: Facts::of(facts),
            rows,
            total: c_text(total),
        };
    }

    /// The change this draft saves as, or None for a diagnosis draft.
    pub(crate) fn change(&self) -> Option<Change> {
        self.draft.change()
    }

    pub(crate) fn subject(&self) -> Subject {
        match self.draft.kind() {
            DraftKind::Order => Subject::Order,
            DraftKind::Vendor => Subject::Vendor,
            DraftKind::Text => Subject::Text,
        }
    }

    /// The text of `field`, for a diagnosis that plans this draft.
    pub(crate) fn text(&self, field: SpielordersConstant) -> String {
        fields::fact(field)
            .map(|fact| self.draft.fact(fact).to_owned())
            .unwrap_or_default()
    }

    /// The record this draft began from, as its facts and rows.
    pub(crate) fn initial(&self) -> Initial {
        let initial = self.draft.initial();
        Initial {
            facts: initial
                .facts
                .iter()
                .map(|(fact, text)| (fields::fact_constant(*fact), text.clone()))
                .collect(),
            rows: initial
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(fact, text)| (fields::row_fact_constant(*fact), text.clone()))
                        .collect()
                })
                .collect(),
        }
    }
}

impl SpielordersDraft {
    /// A draft of `kind` (a `SPIELORDERS_DRAFT_KIND_*`) holding the fields of `facts` and
    /// `rows` it sets; a field it does not set is dropped.
    pub(crate) fn new(
        kind: SpielordersConstant,
        origin: Origin,
        facts: Vec<(SpielordersConstant, String)>,
        rows: Vec<Vec<(SpielordersConstant, String)>>,
    ) -> Self {
        let kind = match kind {
            SPIELORDERS_DRAFT_KIND_ORDER => DraftKind::Order,
            SPIELORDERS_DRAFT_KIND_VENDOR => DraftKind::Vendor,
            _ => DraftKind::Text,
        };
        let facts = facts
            .into_iter()
            .filter_map(|(field, text)| Some((fields::fact(field)?, text)))
            .collect::<Vec<_>>();
        let rows = rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .filter_map(|(field, text)| Some((fields::row_fact(field)?, text)))
                    .collect()
            })
            .collect();
        Self::of(Draft::new(kind, origin, &facts, rows))
    }

    fn of(draft: Draft) -> Self {
        let mut data = SpielordersDraftData {
            draft,
            poisoned: AtomicBool::new(false),
            shown: Shown::default(),
        };
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
        body: impl FnOnce(&mut Draft, &mut Vec<Entry>) -> SpielordersResult,
    ) -> SpielordersResult {
        fallible(outcome, |entries| {
            if self.data.poisoned.load(Ordering::SeqCst) {
                return SPIELORDERS_RESULT_BAD_ARGUMENT;
            }
            let data = &mut *self.data;
            match catch_unwind(AssertUnwindSafe(|| {
                let result = body(&mut data.draft, entries);
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

/// The result a draft change answers: OK, or BAD_ARGUMENT for a field the draft does not
/// have; a row it does not have is `bad_row`'s.
fn status(
    changed: Result<(), DraftRefusal>,
    draft: &Draft,
    row: i64,
    entries: &mut Vec<Entry>,
) -> SpielordersResult {
    match changed {
        Ok(()) => SPIELORDERS_RESULT_OK,
        Err(DraftRefusal::NotItsField) => SPIELORDERS_RESULT_BAD_ARGUMENT,
        Err(DraftRefusal::NoRow) => bad_row(draft, row, entries),
    }
}

fn index(row: i64, count: usize, inclusive: bool) -> Option<usize> {
    let row = usize::try_from(row).ok()?;
    (row < count || (inclusive && row == count)).then_some(row)
}

/// The refusal of a row position a draft does not have.
fn bad_row(draft: &Draft, row: i64, entries: &mut Vec<Entry>) -> SpielordersResult {
    if draft.kind() == DraftKind::Text {
        return SPIELORDERS_RESULT_BAD_ARGUMENT;
    }
    let code = if draft.kind() == DraftKind::Order {
        SPIELORDERS_FAILURE_INVALID_ITEM_POSITION
    } else {
        SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION
    };
    let mut entry = Entry::new(
        code,
        SPIELORDERS_SEVERITY_ERROR,
        c_text(format!(
            "No row {row}; the draft has {}",
            draft.rows().len()
        )),
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
    crate::interface::call::put(draft, None);
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
    crate::interface::call::put(draft, None);
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

/// A draft of a new Order in `package`, holding its Title, Date Ordered, and Status
/// defaults.
/// Saving it creates the Order with its facts and Item rows.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_new_order_draft(
    package: &SpielordersPackage,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(draft, None);
    fallible(outcome, |entries| {
        let result = package.healthy(entries).map(|_| {
            SpielordersDraft::of(Draft::new_order(
                package.root().to_path_buf(),
                Session::uncancellable().today(),
            ))
        });
        deliver(result, draft)
    })
}

/// A draft of a new Vendor in `package`, holding a Name no Vendor of the Package uses.
/// Saving it creates the Vendor with its facts and Contact rows.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_new_vendor_draft(
    package: &SpielordersPackage,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(draft, None);
    fallible(outcome, |entries| {
        let result = package.healthy(entries).map(|opened| {
            SpielordersDraft::of(Draft::new_vendor(
                package.root().to_path_buf(),
                &opened.contents.vendors,
            ))
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
    crate::interface::call::put(draft, None);
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
    quietly(SpielordersConstant::NONE, || draft.data.kind())
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
    draft.change(outcome, |draft, entries| {
        let (Some(text), Some(fact)) = (text.to_str(), fields::fact(field)) else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        let today = Session::uncancellable().today();
        status(draft.set(fact, text, today), draft, -1, entries)
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
    draft.change(outcome, |draft, entries| {
        if matches!(draft.origin(), Origin::Repair) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        match index(row, draft.rows().len(), true) {
            Some(at) => status(draft.insert_row(at), draft, row, entries),
            None => bad_row(draft, row, entries),
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
    draft.change(outcome, |draft, entries| {
        match index(row, draft.rows().len(), false) {
            Some(at) => status(draft.remove_row(at), draft, row, entries),
            None => bad_row(draft, row, entries),
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
    draft.change(outcome, |draft, entries| {
        let count = draft.rows().len();
        match (index(from, count, false), index(to, count, false)) {
            (Some(at), Some(into)) => status(draft.move_row(at, into), draft, from, entries),
            (None, _) => bad_row(draft, from, entries),
            (_, None) => bad_row(draft, to, entries),
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
    draft.change(outcome, |draft, entries| {
        let (Some(text), Some(fact)) = (text.to_str(), fields::row_fact(field)) else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        if !draft.kind().row_facts().contains(&fact) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        match index(row, draft.rows().len(), false) {
            Some(at) => status(draft.set_row(at, fact, text), draft, row, entries),
            None => bad_row(draft, row, entries),
        }
    })
}

/// Asks that saving add the value `field` now holds to the Package's values where the
/// Package lacks it; without this, a value the Package lacks is refused. `field` is
/// SPIELORDERS_ORDER_FIELD_ORDERER, _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP and
/// `row` is -1. Changing the field again withdraws the request.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_confirm(
    draft: &mut SpielordersDraft,
    field: SpielordersConstant,
    row: i64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    draft.change(outcome, |draft, entries| match fields::order_field(field) {
        Some(field) if row == -1 => status(draft.confirm(field), draft, row, entries),
        _ => SPIELORDERS_RESULT_BAD_ARGUMENT,
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
    quietly(-1, || {
        i64::try_from(draft.data.draft.rows().len()).unwrap_or(-1)
    })
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

/// The live Order Total of an Order draft, as computed in floating point and not rounded; "" while a row's numbers do not
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

/// Why a record refuses the assignment `name=value` on its own, whatever else a set
/// assigns with it: the `SPIELORDERS_REASON_*` constant for an identity, a managed
/// technical field, an ordered collection, a name `family` has no assignable field for, a
/// required value left blank, or an Item number that is not finite; or -1 when the record
/// takes it. `family` is SPIELORDERS_FAMILY_ORDER_FIELD, _ITEM_FIELD, _VENDOR_FIELD, or
/// _CONTACT_FIELD, and any other family takes no assignment. Lets a caller holding
/// `field=value` text adjudicate it as the library does before setting a Draft field.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_assignment_refusal(
    family: SpielordersConstant,
    name: Text<'_>,
    value: Text<'_>,
) -> SpielordersConstant {
    quietly(SPIELORDERS_REASON_UNKNOWN_FIELD, || {
        let (Some(name), Some(value)) = (name.to_str(), value.to_str()) else {
            return SPIELORDERS_REASON_UNKNOWN_FIELD;
        };
        let refusal = match family {
            SPIELORDERS_FAMILY_ORDER_FIELD => {
                crate::order_facts::order_assignment_refusal(name, value)
            }
            SPIELORDERS_FAMILY_ITEM_FIELD => {
                crate::order_facts::item_assignment_refusal(name, value)
            }
            SPIELORDERS_FAMILY_VENDOR_FIELD => {
                crate::vendors::vendor_assignment_refusal(name, value)
            }
            SPIELORDERS_FAMILY_CONTACT_FIELD => {
                crate::vendors::contact_assignment_refusal(name, value)
            }
            _ => return SPIELORDERS_REASON_UNKNOWN_FIELD,
        };
        refusal.map_or(SpielordersConstant::NONE, codes::reason)
    })
}
