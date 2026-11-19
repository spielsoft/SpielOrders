//! Plans and commits. A plan writes nothing; a dry run is a plan never committed. A commit
//! re-checks every piece of evidence its plan was made against before its first write.

use crate::interface::{
    OptionalPathBytes, OptionalText, PathBytes, SPIELORDERS_CHANGE_FACT,
    SPIELORDERS_CHANGE_NEW_IDENTITY, SPIELORDERS_CHANGE_RECORD_REMOVED,
    SPIELORDERS_CHANGE_ROW_ADDED, SPIELORDERS_CHANGE_ROW_MOVED, SPIELORDERS_CHANGE_ROW_REMOVED,
    SPIELORDERS_DRAFT_KIND_ORDER, SPIELORDERS_FAILURE_PANICKED,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP, SPIELORDERS_REPORT_COMMITTED,
    SPIELORDERS_REPORT_UNCHANGED, SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_INTERNAL,
    SPIELORDERS_REUSE_NEW, SPIELORDERS_REUSE_REUSED, SPIELORDERS_SEVERITY_ERROR,
    SPIELORDERS_VENDOR_FIELD_HIDDEN, SPIELORDERS_VENDOR_FIELD_NAME, SpielordersConstant,
    SpielordersDraft, SpielordersEntries, SpielordersList, SpielordersOrder, SpielordersPackage,
    SpielordersReadme, SpielordersResult, SpielordersVendor, Text,
    call::{Session, deliver, fallible, put, quietly, settle},
    codes,
    draft::SpielordersDraftData,
    entries::{EMPTY, Entry, Subject, c_text},
    fields::{self, Table},
    list::value_list,
    package::{open, text_or_empty},
    records::{ReadmeScope, contact_facts, document_facts, item_facts, path_text, vendor_facts},
    text::{paths, texts},
};
use crate::{
    Change, ChangeResult, OrderRef, Plan, Report, Scope, VendorRef,
    change::Merged,
    drafting::Origin,
    model::{OrderDocument, Vendor},
    values::ReusableValue,
};
use std::{
    ffi::CString,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

/// One change a plan makes to a record, as the plan accessors read it.
#[derive(Clone, Debug)]
pub(crate) struct PlannedChange {
    kind: SpielordersConstant,
    field: SpielordersConstant,
    row: i64,
    before: CString,
    after: CString,
    reuse: SpielordersConstant,
    /// The zero-based batch addition the change belongs to, in a batch's plan.
    addition: i64,
}

impl PlannedChange {
    fn new(
        kind: SpielordersConstant,
        field: SpielordersConstant,
        row: i64,
        before: &str,
        after: &str,
    ) -> Self {
        Self {
            kind,
            field,
            row,
            before: c_text(before),
            after: c_text(after),
            reuse: SpielordersConstant::NONE,
            addition: -1,
        }
    }

    /// The same change, as one batch addition `addition` makes.
    pub(crate) fn of_addition(mut self, addition: usize) -> Self {
        self.addition = i64::try_from(addition).unwrap_or(-1);
        self
    }
}

/// A validated change that has written nothing: its operations, the changes it makes to
/// its record, and the evidence its commit re-checks.
#[repr(C)]
pub struct SpielordersPlan {
    data: Box<SpielordersPlanData>,
}

struct SpielordersPlanData {
    plan: crate::Plan,
    root: PathBuf,
    subject: Subject,
    operations: Vec<(SpielordersConstant, CString)>,
    changes: Vec<PlannedChange>,
    restored_order_id: CString,
    directories: Vec<bool>,
}

fn operations(plan: &Plan) -> Vec<(SpielordersConstant, CString)> {
    plan.operations()
        .into_iter()
        .map(|(act, path)| (codes::operation(act), path_text(&path.absolute_path)))
        .collect()
}

type Record = (
    Vec<(SpielordersConstant, String)>,
    Vec<Vec<(SpielordersConstant, String)>>,
);

/// The changes a merge makes to the record it keeps, and each Vendor a Vendor merge or
/// deletion removes.
fn merge_changes(plan: &Plan) -> Vec<PlannedChange> {
    let mut changes = match plan.merged() {
        Some(Merged::Order([(before, before_vendor), (after, after_vendor)])) => record_changes(
            &order_record(before, before_vendor),
            &order_record(after, after_vendor),
        ),
        Some(Merged::Vendor([before, after])) => {
            record_changes(&vendor_record(before), &vendor_record(after))
        }
        None => Vec::new(),
    };
    changes.extend(plan.removed_vendors().into_iter().map(|vendor| {
        PlannedChange::new(
            SPIELORDERS_CHANGE_RECORD_REMOVED,
            SPIELORDERS_VENDOR_FIELD_NAME,
            -1,
            vendor.name.as_str(),
            "",
        )
    }));
    changes
}

/// An Order record in the fields its draft edits.
fn order_record(document: &OrderDocument, vendor: Option<&Vendor>) -> Record {
    let mut facts = document_facts(document, vendor);
    facts.retain(|(field, _)| fields::assigns(Table::Order, *field));
    let rows = document
        .items
        .iter()
        .map(|item| {
            let mut row = item_facts(item, None);
            row.retain(|(field, _)| fields::assigns(Table::Item, *field));
            row
        })
        .collect();
    (facts, rows)
}

/// A Vendor record in the fields its draft edits.
fn vendor_record(vendor: &Vendor) -> Record {
    let mut facts = vendor_facts(vendor);
    facts.retain(|(field, _)| fields::assigns(Table::Vendor, *field));
    let rows = vendor
        .contacts
        .iter()
        .map(|contact| {
            let mut row = contact_facts(contact);
            row.retain(|(field, _)| fields::assigns(Table::Contact, *field));
            row
        })
        .collect();
    (facts, rows)
}

/// What changes from `before` to `after`: each fact, each fact of a row both hold, each
/// row `after` adds with its facts, and each row it drops.
fn record_changes(before: &Record, after: &Record) -> Vec<PlannedChange> {
    let mut changes = Vec::new();
    for (field, value) in &after.0 {
        let prior = value_of(&before.0, *field);
        if prior != value {
            changes.push(PlannedChange::new(
                SPIELORDERS_CHANGE_FACT,
                *field,
                -1,
                prior,
                value,
            ));
        }
    }
    for (index, row) in after.1.iter().enumerate() {
        let at = i64::try_from(index).unwrap_or(-1);
        match before.1.get(index) {
            Some(prior_row) => {
                for (field, value) in row {
                    let prior = value_of(prior_row, *field);
                    if prior != value {
                        changes.push(PlannedChange::new(
                            SPIELORDERS_CHANGE_FACT,
                            *field,
                            at,
                            prior,
                            value,
                        ));
                    }
                }
            }
            None => {
                changes.push(PlannedChange::new(
                    SPIELORDERS_CHANGE_ROW_ADDED,
                    SpielordersConstant::NONE,
                    at,
                    "",
                    "",
                ));
                for (field, value) in row.iter().filter(|(_, value)| !value.is_empty()) {
                    changes.push(PlannedChange::new(
                        SPIELORDERS_CHANGE_FACT,
                        *field,
                        at,
                        "",
                        value,
                    ));
                }
            }
        }
    }
    for index in after.1.len()..before.1.len() {
        changes.push(PlannedChange::new(
            SPIELORDERS_CHANGE_ROW_REMOVED,
            SpielordersConstant::NONE,
            i64::try_from(index).unwrap_or(-1),
            "",
            "",
        ));
    }
    changes
}

/// The record a plan would write, read in the fields its draft edits.
fn planned_record(draft: &SpielordersDraftData, plan: &Plan) -> Option<Record> {
    let (mut facts, mut rows): Record = if draft.kind() == SPIELORDERS_DRAFT_KIND_ORDER {
        let facts = plan.order_record()?;
        let (document, vendor) = (&facts.document, facts.vendor.as_ref());
        (
            document_facts(document, vendor),
            document
                .items
                .iter()
                .map(|item| item_facts(item, None))
                .collect(),
        )
    } else {
        let vendor = plan.vendor_record()?;
        (
            vendor_facts(vendor),
            vendor.contacts.iter().map(contact_facts).collect(),
        )
    };
    let (settable, row_settable) = (draft.settable(), draft.row_settable());
    facts.retain(|(field, _)| settable.contains(field));
    for row in &mut rows {
        row.retain(|(field, _)| row_settable.contains(field));
    }
    Some((facts, rows))
}

/// Whether `field`'s planned value reuses a value the Package already has, or is new.
fn reuse_kind(plan: &Plan, field: SpielordersConstant) -> SpielordersConstant {
    let Some(values) = plan.order_record().map(|facts| &facts.values) else {
        return SpielordersConstant::NONE;
    };
    let Some(kind) = fields::reusable(field) else {
        return SpielordersConstant::NONE;
    };
    match values.of(kind) {
        ReusableValue::Reused(_) => SPIELORDERS_REUSE_REUSED,
        ReusableValue::New(_) => SPIELORDERS_REUSE_NEW,
        ReusableValue::Absent => SpielordersConstant::NONE,
    }
}

fn value_of(facts: &[(SpielordersConstant, String)], field: SpielordersConstant) -> &str {
    facts
        .iter()
        .find(|(each, _)| *each == field)
        .map_or("", |(_, value)| value.as_str())
}

/// Every change the plan of `draft` makes to the record the draft began from.
pub(crate) fn draft_changes(draft: &SpielordersDraftData, plan: &Plan) -> Vec<PlannedChange> {
    let Some((facts, rows)) = planned_record(draft, plan) else {
        return Vec::new();
    };
    let initial = draft.initial();
    let mut changes = Vec::new();
    if draft.draft().origin().is_new_record() {
        changes.push(PlannedChange::new(
            SPIELORDERS_CHANGE_NEW_IDENTITY,
            SpielordersConstant::NONE,
            -1,
            "",
            "",
        ));
    }
    for (field, after) in &facts {
        let before = value_of(&initial.facts, *field);
        if before != after {
            let mut change = PlannedChange::new(SPIELORDERS_CHANGE_FACT, *field, -1, before, after);
            change.reuse = reuse_kind(plan, *field);
            changes.push(change);
        }
    }
    let mut kept = vec![false; initial.rows.len()];
    for (index, row) in draft.draft().rows().iter().enumerate() {
        let at = i64::try_from(index).unwrap_or(-1);
        let planned = rows.get(index).cloned().unwrap_or_default();
        let before_row = row.origin.and_then(|origin| {
            if let Some(kept) = kept.get_mut(origin) {
                *kept = true;
            }
            initial.rows.get(origin)
        });
        match (row.origin, before_row) {
            (Some(origin), Some(before_row)) => {
                if origin != index {
                    changes.push(PlannedChange::new(
                        SPIELORDERS_CHANGE_ROW_MOVED,
                        SpielordersConstant::NONE,
                        at,
                        &origin.to_string(),
                        &index.to_string(),
                    ));
                }
                for (field, after) in &planned {
                    let before = value_of(before_row, *field);
                    if before != after {
                        changes.push(PlannedChange::new(
                            SPIELORDERS_CHANGE_FACT,
                            *field,
                            at,
                            before,
                            after,
                        ));
                    }
                }
            }
            _ => {
                changes.push(PlannedChange::new(
                    SPIELORDERS_CHANGE_ROW_ADDED,
                    SpielordersConstant::NONE,
                    at,
                    "",
                    "",
                ));
                for (field, after) in planned.iter().filter(|(_, after)| !after.is_empty()) {
                    changes.push(PlannedChange::new(
                        SPIELORDERS_CHANGE_FACT,
                        *field,
                        at,
                        "",
                        after,
                    ));
                }
            }
        }
    }
    for (origin, _) in kept.iter().enumerate().filter(|(_, kept)| !**kept) {
        changes.push(PlannedChange::new(
            SPIELORDERS_CHANGE_ROW_REMOVED,
            SpielordersConstant::NONE,
            i64::try_from(origin).unwrap_or(-1),
            "",
            "",
        ));
    }
    changes
}

/// Plans writing the bundled agent guidance into `.agents/skills/spielorders` under the
/// existing `directory`: each guidance file is created where missing and replaced
/// (SPIELORDERS_OPERATION_REPLACE_FILE) where it differs, and one already current is kept.
/// Reads no Package metadata, so any directory, a Package, or a Damaged Package plans
/// alike. Never reads or changes AGENTS.md or any other file. Refuses a `directory` that is
/// a link or not a directory; a guidance destination that exists as the wrong kind of
/// entry, such as a link, a file where a directory belongs, or a directory where a file
/// belongs; and a leftover staging entry beside a missing guidance file. Commit rechecks
/// every destination, replaces a file only while it holds the bytes planned against, and
/// never overwrites an entry created after planning.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_provision_agent(
    directory: PathBytes<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let root = directory.as_path().to_path_buf();
        let result = plan_change(
            &Change::ProvisionAgent { root: root.clone() },
            root,
            Subject::Other,
            token,
            entries,
            |_| Vec::new(),
        );
        deliver(result, plan)
    })
}

impl SpielordersPlan {
    pub(crate) fn of(
        plan: crate::Plan,
        root: PathBuf,
        subject: Subject,
        changes: impl FnOnce(&Plan) -> Vec<PlannedChange>,
    ) -> Self {
        Self {
            data: Box::new(SpielordersPlanData {
                operations: operations(&plan),
                directories: plan.operation_directories(),
                changes: changes(&plan),
                restored_order_id: c_text(plan.restored_order_id().unwrap_or_default()),
                plan,
                root,
                subject,
            }),
        }
    }
}

/// Plans `change`, honoring `token`, as a plan of `subject` under `root`.
pub(crate) fn plan_change(
    change: &Change,
    root: PathBuf,
    subject: Subject,
    token: u64,
    entries: &mut Vec<Entry>,
    changes: impl FnOnce(&Plan) -> Vec<PlannedChange>,
) -> Result<SpielordersPlan, SpielordersResult> {
    let session = Session::new(token);
    settle(crate::plan(change, &session), &session, subject, entries)
        .map(|plan| SpielordersPlan::of(plan, root, subject, changes))
}

/// Plans saving `draft`: every rule it breaks refuses together, each entry located by
/// field and row. The plan of an edit is guarded by the read the draft was made from.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_draft_plan(
    draft: &SpielordersDraft,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let result = draft.guarded(
            Err(SPIELORDERS_RESULT_BAD_ARGUMENT),
            Err(SPIELORDERS_RESULT_INTERNAL),
            |data| {
                let change = data.change().ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
                let root = match data.draft().origin() {
                    Origin::NewOrder { root }
                    | Origin::Adoption { root, .. }
                    | Origin::NewVendor { root }
                    | Origin::EditVendor { root, .. } => root.clone(),
                    Origin::EditOrder { directory, .. } => directory.clone(),
                    Origin::Repair => PathBuf::new(),
                };
                plan_change(&change, root, data.subject(), token, entries, |value| {
                    draft_changes(data, value)
                })
            },
        );
        deliver(result, plan)
    })
}

/// Plans renaming the Order Directory of `order` to the name its facts project.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_plan_align(
    order: &SpielordersOrder,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let directory = order.directory().to_path_buf();
        let change = Change::AlignOrder {
            order: OrderRef::Directory(directory.clone()),
        };
        deliver(
            plan_change(&change, directory, Subject::Order, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
    })
}

/// Plans removing `order` to the Package's Removed Orders Directory, where it stays
/// recoverable. Moving it on to the platform Trash is the caller's own act, on the
/// directory the committed report's final path names.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_plan_remove(
    order: &SpielordersOrder,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let directory = order.directory().to_path_buf();
        let change = Change::RemoveOrder {
            order: OrderRef::Directory(directory.clone()),
        };
        deliver(
            plan_change(&change, directory, Subject::Order, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
    })
}

/// Plans merging the `count` Orders whose Directories are at `merged` into `order`, in
/// precedence order: `order` is kept, and each merged Order only fills the facts, Items,
/// and directory entries the result still lacks, after which what remains of it is removed
/// into the Removed Orders Directory. Its operations name every entry moved
/// (SPIELORDERS_OPERATION_MOVE_MERGED_ENTRY) and discarded
/// (SPIELORDERS_OPERATION_DISCARD_MERGED_ENTRY) by its path in the merged Order; a caller
/// that confirms discards asks before committing. A merged Order that cannot be removed
/// after its entries moved is SPIELORDERS_WARNING_ORDER_NOT_REMOVED. The report's Order is
/// the kept one.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_order_plan_merge(
    order: &SpielordersOrder,
    merged: Option<std::ptr::NonNull<PathBytes<'_>>>,
    count: i64,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let directory = order.directory().to_path_buf();
        let result = paths(merged, count)
            .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)
            .and_then(|merged| {
                let change = Change::MergeOrders {
                    orders: std::iter::once(directory.clone())
                        .chain(merged.into_iter().map(Path::to_path_buf))
                        .map(OrderRef::Directory)
                        .collect(),
                };
                plan_change(
                    &change,
                    directory,
                    Subject::Order,
                    token,
                    entries,
                    merge_changes,
                )
            });
        deliver(result, plan)
    })
}

/// Plans hiding (`hidden` 1) or unhiding (0) `vendor`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_plan_hidden(
    vendor: &SpielordersVendor,
    hidden: i32,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let read = &vendor.data().read;
        let was = read.vendor.is_hidden();
        let change = Change::SetVendorHidden {
            root: vendor.root(),
            vendor: VendorRef::Id(read.vendor.vendor_id.to_string()),
            hidden: hidden != 0,
        };
        let result = plan_change(
            &change,
            vendor.root(),
            Subject::Vendor,
            token,
            entries,
            |_| {
                if was == (hidden != 0) {
                    Vec::new()
                } else {
                    vec![PlannedChange::new(
                        SPIELORDERS_CHANGE_FACT,
                        SPIELORDERS_VENDOR_FIELD_HIDDEN,
                        -1,
                        &was.to_string(),
                        &(hidden != 0).to_string(),
                    )]
                }
            },
        );
        deliver(result, plan)
    })
}

/// Plans deleting `vendor` and its Contacts. Refuses with
/// SPIELORDERS_FAILURE_VENDOR_REFERENCED, one entry per referencing Order, while any
/// readable reference in a supported Order names it, including a damaged Order; hiding
/// is how a Vendor in use stops being offered.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_plan_delete(
    vendor: &SpielordersVendor,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let read = &vendor.data().read;
        let change = Change::DeleteVendor {
            root: vendor.root(),
            vendor: VendorRef::Id(read.vendor.vendor_id.to_string()),
            in_use: crate::InUse::Refuse,
        };
        let result = plan_change(
            &change,
            vendor.root(),
            Subject::Vendor,
            token,
            entries,
            |plan| {
                let vendor = plan
                    .vendor_record()
                    .expect("a Vendor deletion plans its subject record");
                vec![PlannedChange::new(
                    SPIELORDERS_CHANGE_RECORD_REMOVED,
                    SPIELORDERS_VENDOR_FIELD_NAME,
                    -1,
                    vendor.name.as_str(),
                    "",
                )]
            },
        );
        deliver(result, plan)
    })
}

/// The value kind an Order field constant names, for the four value fields.
fn value_kind(
    field: SpielordersConstant,
) -> Result<crate::model::ReusableValueKind, SpielordersResult> {
    fields::reusable(field).ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)
}

/// `text` as UTF-8, or BAD_ARGUMENT.
fn utf8(text: Text<'_>) -> Result<String, SpielordersResult> {
    text.to_str()
        .map(str::to_owned)
        .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)
}

/// `project` for an Order Group (NULL or "" for the Orders with no Project), or none for
/// another field.
fn value_project(
    field: SpielordersConstant,
    project: OptionalText<'_>,
) -> Result<Option<String>, SpielordersResult> {
    let project = text_or_empty(project)?;
    Ok(
        (field == SPIELORDERS_ORDER_FIELD_ORDER_GROUP && !project.trim().is_empty())
            .then_some(project),
    )
}

/// Plans `change` on the values of `package`, a plan whose changes are its Order count.
fn value_plan(
    package: &SpielordersPackage,
    change: impl FnOnce(PathBuf) -> Result<Change, SpielordersResult>,
    subject: Subject,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    entries: &mut Vec<Entry>,
) -> SpielordersResult {
    let root = package.root().to_path_buf();
    let result = change(root.clone())
        .and_then(|change| plan_change(&change, root, subject, token, entries, merge_changes));
    deliver(result, plan)
}

/// Plans keeping `value` among the values of `field` (SPIELORDERS_ORDER_FIELD_ORDERER,
/// _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP), used by an Order or not. An Order
/// Group belongs to `project`, or to the Orders with no Project for NULL or "". A value
/// the Package already has plans no change. Refuses SPIELORDERS_FAILURE_BLANK_VALUE for a
/// blank value and _UNKNOWN_VALUE for a Project the Package lacks.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_add_value(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    project: OptionalText<'_>,
    value: Text<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let change = |root| {
            Ok(Change::AddValue {
                root,
                kind: value_kind(field)?,
                project: value_project(field, project)?,
                value: utf8(value)?,
            })
        };
        value_plan(package, change, Subject::Order, token, plan, entries)
    })
}

/// Plans respelling the value `value` of `field` as `to` in every Order that uses it;
/// renaming onto another value the Package has merges the two under `to`.
/// spielorders_plan_order_count is how many Orders change. Refuses
/// SPIELORDERS_FAILURE_UNKNOWN_VALUE for a value the Package lacks and _BLANK_VALUE for a
/// blank `to`. `field` and `project` are as for spielorders_package_plan_add_value.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_rename_value(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    project: OptionalText<'_>,
    value: Text<'_>,
    to: Text<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let change = |root| {
            Ok(Change::RenameValue {
                root,
                kind: value_kind(field)?,
                project: value_project(field, project)?,
                value: utf8(value)?,
                to: utf8(to)?,
            })
        };
        value_plan(package, change, Subject::Order, token, plan, entries)
    })
}

/// Plans making the `count` values of `field` at `values` one value spelled `keep`, which
/// every Order using one of them then uses; a Project's groups go with it.
/// spielorders_plan_order_count is how many Orders change. Refuses
/// SPIELORDERS_FAILURE_UNKNOWN_VALUE for a value the Package lacks. `field` and `project`
/// are as for spielorders_package_plan_add_value.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn spielorders_package_plan_merge_values(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    project: OptionalText<'_>,
    values: Option<std::ptr::NonNull<Text<'_>>>,
    count: i64,
    keep: Text<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let change = |root| {
            Ok(Change::MergeValues {
                root,
                kind: value_kind(field)?,
                project: value_project(field, project)?,
                values: texts(values, count)
                    .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                keep: utf8(keep)?,
            })
        };
        value_plan(package, change, Subject::Order, token, plan, entries)
    })
}

/// Plans taking the value `value` of `field` out of the Package. `replacement` says what
/// each Order using it uses instead: another value the Package has, "" for none, or NULL
/// to refuse with SPIELORDERS_FAILURE_VALUE_IN_USE, one entry per Order, while any does.
/// A deleted Project's groups go with its Orders. spielorders_plan_order_count is how many
/// Orders change. Refuses _UNKNOWN_VALUE for a value or replacement the Package lacks.
/// `field` and `project` are as for spielorders_package_plan_add_value.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_delete_value(
    package: &SpielordersPackage,
    field: SpielordersConstant,
    project: OptionalText<'_>,
    value: Text<'_>,
    replacement: OptionalText<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let change = |root| {
            Ok(Change::DeleteValue {
                root,
                kind: value_kind(field)?,
                project: value_project(field, project)?,
                value: utf8(value)?,
                in_use: in_use_choice(replacement, str::to_owned)?,
            })
        };
        value_plan(package, change, Subject::Order, token, plan, entries)
    })
}

/// What `replacement` says of Orders using a deleted value: NULL refuses, "" clears, and
/// any other text names the replacement.
fn in_use_choice<T>(
    replacement: OptionalText<'_>,
    named: impl FnOnce(&str) -> T,
) -> Result<crate::InUse<T>, SpielordersResult> {
    Ok(match replacement.as_c_str() {
        None => crate::InUse::Refuse,
        Some(text) => match text.to_str().map_err(|_| SPIELORDERS_RESULT_BAD_ARGUMENT)? {
            text if text.trim().is_empty() => crate::InUse::Clear,
            text => crate::InUse::Replace(named(text)),
        },
    })
}

/// Plans merging the `count` Vendors whose Vendor IDs are at `vendor_ids` into the Vendor
/// `keep` names by ID: every Order referencing one of them references `keep`, which gains
/// their Contacts, and the others are deleted. spielorders_plan_order_count is how many
/// Orders change. Refuses SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID for an ID the Package lacks.
/// Refuses _VENDOR_REFERENCED when a damaged Order still references a removed Vendor,
/// because its record cannot safely be rewritten.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_merge_vendors(
    package: &SpielordersPackage,
    vendor_ids: Option<std::ptr::NonNull<Text<'_>>>,
    count: i64,
    keep: Text<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let change = |root| {
            Ok(Change::MergeVendors {
                root,
                vendors: texts(vendor_ids, count)
                    .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?
                    .into_iter()
                    .map(|id| VendorRef::Id(id.to_owned()))
                    .collect(),
                keep: VendorRef::Id(utf8(keep)?),
            })
        };
        value_plan(package, change, Subject::Vendor, token, plan, entries)
    })
}

/// Plans deleting `vendor` and its Contacts. `replacement` says what each Order
/// referencing it references instead: another Vendor by Vendor ID, "" for none, or NULL to
/// refuse as spielorders_vendor_plan_delete does. spielorders_plan_order_count is how many
/// Orders change. A damaged Order's readable reference still refuses _VENDOR_REFERENCED:
/// correct that record before deleting the Vendor it references.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_vendor_plan_delete_replacing(
    vendor: &SpielordersVendor,
    replacement: OptionalText<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let root = vendor.root();
        let result =
            in_use_choice(replacement, |id| VendorRef::Id(id.to_owned())).and_then(|in_use| {
                let change = Change::DeleteVendor {
                    root: root.clone(),
                    vendor: VendorRef::Id(vendor.data().read.vendor.vendor_id.to_string()),
                    in_use,
                };
                plan_change(
                    &change,
                    root,
                    Subject::Vendor,
                    token,
                    entries,
                    merge_changes,
                )
            });
        deliver(result, plan)
    })
}

/// How many Order records committing `plan` writes: the Orders a value change, merge, or
/// replacement changes, or the one Order a save writes.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_order_count(plan: &SpielordersPlan) -> i64 {
    quietly(-1, || {
        i64::try_from(plan.data.plan.order_count()).unwrap_or(-1)
    })
}

/// Plans replacing the text of `readme` with `text`, guarded by the bytes `readme` read.
/// Refuses for a README that could not be read.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_readme_plan_write(
    readme: &SpielordersReadme,
    text: Text<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let Some(text) = text.to_str() else {
            return deliver(Err(SPIELORDERS_RESULT_BAD_ARGUMENT), plan);
        };
        let data = readme.data();
        let (scope, root) = match &data.scope {
            ReadmeScope::Package(root) => (Scope::Package(root.clone()), root.clone()),
            ReadmeScope::Order(directory) => (
                Scope::Order(OrderRef::Directory(directory.clone())),
                directory.clone(),
            ),
        };
        let change = Change::WriteReadme {
            scope,
            original: (&data.read).into(),
            text: text.to_owned(),
        };
        deliver(
            plan_change(&change, root, Subject::Other, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
    })
}

/// Plans restoring the removed Order at `removed`, a directory in the Removed Orders
/// Directory of `package` such as a removal's final path, to its Orders Directory under the
/// same name. Refuses SPIELORDERS_FAILURE_NOT_DIRECT_CHILD for a path outside the Removed
/// Orders Directory, _NOT_AN_ORDER_DIRECTORY or _DAMAGED_RECORD for one that is not a
/// readable Order with ordinary managed metadata, _UNRESOLVED_VENDOR when its Vendor
/// reference no longer resolves, _DESTINATION_COLLISION when the Orders Directory already
/// holds that name, and _ORDER_IDENTITY_IN_USE when another Order, including a damaged one
/// with a readable identity, carries its Order ID. Commit refuses when any of that or the
/// configured destination changed after planning. The report's final path is the restored
/// Order.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_restore_order(
    package: &SpielordersPackage,
    removed: PathBytes<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let root = package.root().to_path_buf();
        let change = Change::RestoreOrder {
            root: root.clone(),
            removed: removed.as_path().to_path_buf(),
        };
        deliver(
            plan_change(&change, root, Subject::Order, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
    })
}

/// Plans rebuilding the derived Package Index of `package`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_rebuild_index(
    package: &SpielordersPackage,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let root = package.root().to_path_buf();
        let change = Change::RebuildIndex { root: root.clone() };
        deliver(
            plan_change(&change, root, Subject::Other, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
    })
}

/// Plans initializing a new Package at `root`. `orders_path` and `removed_path` (or NULL
/// for the conventional ones) are Package-relative directories.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_initialize(
    root: PathBytes<'_>,
    orders_path: OptionalPathBytes<'_>,
    removed_path: OptionalPathBytes<'_>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let text = |path: OptionalPathBytes<'_>| {
            path.as_path()
                .map(|path| {
                    path.to_str()
                        .map(str::to_owned)
                        .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)
                })
                .transpose()
        };
        let result = (|| {
            let change = Change::InitializePackage {
                root: root.as_path().to_path_buf(),
                orders_path: text(orders_path)?,
                removed_path: text(removed_path)?,
            };
            plan_change(
                &change,
                root.as_path().to_path_buf(),
                Subject::Other,
                token,
                entries,
                |_| Vec::new(),
            )
        })();
        deliver(result, plan)
    })
}

impl SpielordersPlan {
    fn operation(&self, index: i64) -> Option<&(SpielordersConstant, CString)> {
        self.data.operations.get(usize::try_from(index).ok()?)
    }

    fn planned_change(&self, index: i64) -> Option<&PlannedChange> {
        self.data.changes.get(usize::try_from(index).ok()?)
    }
}

/// The immutable Order ID of the removed Order a restore `plan` will restore; "" for
/// another kind of plan. A caller undoing a specific removal compares this with that
/// Order's ID before committing, because the removed path may now hold another Order.
/// The plan owns the borrowed text, and commit rechecks its subject's identity.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_restored_order_id(plan: &SpielordersPlan) -> Text<'_> {
    quietly(Text::new(EMPTY), || Text::new(&plan.data.restored_order_id))
}

/// How many filesystem operations committing `plan` performs.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_operation_count(plan: &SpielordersPlan) -> i64 {
    quietly(-1, || {
        i64::try_from(plan.data.operations.len()).unwrap_or(-1)
    })
}

/// The `SPIELORDERS_OPERATION_*` of operation `index`, or -1 out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_operation_kind(
    plan: &SpielordersPlan,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        plan.operation(index)
            .map_or(SpielordersConstant::NONE, |(kind, _)| *kind)
    })
}

/// The path operation `index` acts on; "" out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_operation_path(
    plan: &SpielordersPlan,
    index: i64,
) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        plan.operation(index)
            .map_or(PathBytes::new(EMPTY), |(_, path)| PathBytes::new(path))
    })
}

/// 1 when operation `index` acts on a directory — an entry of a merged Order that is a
/// folder — otherwise 0, including out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_operation_directory(plan: &SpielordersPlan, index: i64) -> i32 {
    quietly(0, || {
        usize::try_from(index)
            .ok()
            .and_then(|index| plan.data.directories.get(index))
            .map_or(0, |directory| i32::from(*directory))
    })
}

/// How many changes `plan` makes to the record its draft edits, or, for an Order or Vendor
/// merge, to the record it keeps, with each Vendor a Vendor merge or deletion removes.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_count(plan: &SpielordersPlan) -> i64 {
    quietly(-1, || i64::try_from(plan.data.changes.len()).unwrap_or(-1))
}

/// The zero-based addition of a batch that change `index` of the batch's plan belongs to;
/// -1 for any other plan or out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_addition(plan: &SpielordersPlan, index: i64) -> i64 {
    quietly(-1, || {
        plan.planned_change(index)
            .map_or(-1, |change| change.addition)
    })
}

/// The `SPIELORDERS_CHANGE_*` of change `index`, or -1 out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_kind(
    plan: &SpielordersPlan,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        plan.planned_change(index)
            .map_or(SpielordersConstant::NONE, |change| change.kind)
    })
}

/// The field change `index` changes: a `SPIELORDERS_*_FIELD_*` constant, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_field(
    plan: &SpielordersPlan,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        plan.planned_change(index)
            .map_or(SpielordersConstant::NONE, |change| change.field)
    })
}

/// The zero-based draft row change `index` is about (for a removed row, the row of the
/// read record), or -1 for the record's own facts.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_row(plan: &SpielordersPlan, index: i64) -> i64 {
    quietly(-1, || {
        plan.planned_change(index).map_or(-1, |change| change.row)
    })
}

/// What change `index` changes from; "" when nothing was there.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_before(plan: &SpielordersPlan, index: i64) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        plan.planned_change(index)
            .map_or(Text::new(EMPTY), |change| Text::new(&change.before))
    })
}

/// What change `index` changes to, as the plan normalized it; "" when nothing will be.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_after(plan: &SpielordersPlan, index: i64) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        plan.planned_change(index)
            .map_or(Text::new(EMPTY), |change| Text::new(&change.after))
    })
}

/// For a change to a reusable-value field, SPIELORDERS_REUSE_REUSED or
/// SPIELORDERS_REUSE_NEW; otherwise -1.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_reuse(
    plan: &SpielordersPlan,
    index: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        plan.planned_change(index)
            .map_or(SpielordersConstant::NONE, |change| change.reuse)
    })
}

/// What a commit did, and the record it refreshed.
#[repr(C)]
pub struct SpielordersReport {
    data: Box<SpielordersReportData>,
}

struct SpielordersReportData {
    kind: SpielordersConstant,
    order: Option<SpielordersOrder>,
    vendor: Option<SpielordersVendor>,
    values: Option<Vec<crate::canonical::ReusableValueProjection>>,
    final_path: CString,
    package: Option<SpielordersPackage>,
}

/// What `report` committed, read again through the commit's own `session`, which
/// remembers the Package its refresh left.
fn refreshed(
    report: &Report,
    root: &std::path::Path,
    subject: Subject,
    session: &Session,
) -> SpielordersReportData {
    // Capture the commit's Package before record readback: an external locator edit may
    // make that read load a different Package even when it then refuses the Order.
    let current_root = session.current_root();
    let (kind, result) = match report {
        Report::Unchanged { result, .. } => (SPIELORDERS_REPORT_UNCHANGED, result),
        Report::Committed { result, .. } | Report::CommittedWithWarning { result, .. } => {
            (SPIELORDERS_REPORT_COMMITTED, result)
        }
    };
    let (removed, written, vendor_id) = match result {
        ChangeResult::RemoveOrder(removal) => (
            Some(removal.final_location.absolute_path.clone()),
            None,
            None,
        ),
        ChangeResult::CreateOrder(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::AdoptOrder(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::EditOrder(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::AlignOrder(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::RestoreOrder(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::MergeOrders(result) => {
            (None, Some(result.directory.absolute_path.clone()), None)
        }
        ChangeResult::Vendor(result) => (None, None, Some(result.vendor_id.to_string())),
        _ => (None, None, None),
    };
    let order = match (&written, subject) {
        (Some(directory), Subject::Order | Subject::Other) => {
            crate::read_order(&OrderRef::Directory(directory.clone()), session)
                .result
                .ok()
                .map(SpielordersOrder::of)
        }
        _ => None,
    };
    let vendor = vendor_id
        .filter(|_| subject == Subject::Vendor)
        .and_then(|id| {
            crate::read_vendor(root, &VendorRef::Id(id), session)
                .result
                .ok()
                .map(SpielordersVendor::of)
        })
        .or_else(|| match result {
            ChangeResult::Values(result) => result.vendor.clone().map(SpielordersVendor::of),
            _ => None,
        });
    let values = match result {
        ChangeResult::Values(result) => result.values.clone(),
        _ => None,
    };
    let final_path = removed
        .or(written)
        .map(|path| path_text(&path))
        .unwrap_or_default();
    // Normally the session still remembers this Package, so opening it reads only its
    // README. If a changed locator led readback elsewhere, open the original Package.
    let package = current_root.and_then(|root| open(&root, session, &mut Vec::new()).ok());
    SpielordersReportData {
        kind,
        order,
        vendor,
        values,
        final_path,
        package,
    }
}

/// Commits `plan`. Takes no token: it ends committed, refused before its first write
/// (SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED when anything it was planned against
/// changed, or SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED naming each fact it was
/// planned against that moved: a saved Order's Vendor and values, or the facts an Order
/// alignment, removal, or restoration found), or failed with its rollback reported.
/// Warnings arrive in the outcome with SPIELORDERS_RESULT_OK. A plan may be committed once; a second commit refuses as its
/// evidence no longer stands.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_commit(
    plan: &SpielordersPlan,
    report: &mut Option<Box<SpielordersReport>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(report, None);
    fallible(outcome, |entries| {
        let session = Session::uncancellable();
        // The read-back after the commit is inside the catch too: a panic there follows a
        // write, and the caller is told so.
        let committed = catch_unwind(AssertUnwindSafe(|| {
            let answer = crate::commit(plan.data.plan.clone(), &session);
            settle(answer, &session, plan.data.subject, entries).map(|done| {
                if let Report::CommittedWithWarning { warnings, .. } = &done {
                    for warning in warnings {
                        Entry::warnings(warning, entries);
                    }
                }
                SpielordersReport {
                    data: Box::new(refreshed(
                        &done,
                        &plan.data.root,
                        plan.data.subject,
                        &session,
                    )),
                }
            })
        }));
        let Ok(result) = committed else {
            let mut entry = Entry::new(
                SPIELORDERS_FAILURE_PANICKED,
                SPIELORDERS_SEVERITY_ERROR,
                c_text("The commit stopped on an internal failure"),
            );
            entry.after = c_text(if session.wrote() {
                "replaced"
            } else {
                "unchanged"
            });
            entries.push(entry);
            put(report, None);
            return SPIELORDERS_RESULT_INTERNAL;
        };
        deliver(result, report)
    })
}

/// Frees `plan`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_free(plan: Option<Box<SpielordersPlan>>) {
    quietly((), || drop(plan));
}

/// SPIELORDERS_REPORT_COMMITTED or SPIELORDERS_REPORT_UNCHANGED.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_kind(report: &SpielordersReport) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || report.data.kind)
}

/// A fresh read of the Order the commit wrote, as a new handle the caller frees, or NULL
/// when it wrote none or removed it.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_order(
    report: &SpielordersReport,
) -> Option<Box<SpielordersOrder>> {
    quietly(None, || {
        report
            .data
            .order
            .as_ref()
            .map(|order| Box::new(SpielordersOrder::of(order.data().read.clone())))
    })
}

/// The Vendor the commit wrote or kept in a merge or replacement deletion, as a new
/// handle the caller frees, or NULL when it wrote or kept none.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_vendor(
    report: &SpielordersReport,
) -> Option<Box<SpielordersVendor>> {
    quietly(None, || {
        report
            .data
            .vendor
            .as_ref()
            .map(|vendor| Box::new(SpielordersVendor::of(vendor.data().read.clone())))
    })
}

/// The affected value list after the committed value change, as a new handle the caller
/// frees, or NULL for another kind of change. Order Groups are scoped to the Project the
/// change named. The list, including an empty one after deletion, retains this report's
/// result even when the Package changes again.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_values(
    report: &SpielordersReport,
) -> Option<Box<SpielordersList>> {
    quietly(None, || {
        report
            .data
            .values
            .clone()
            .map(|values| Box::new(value_list(values)))
    })
}

/// Where the committed Order Directory now is, or "". Lists refresh from
/// spielorders_report_package, or by opening the Package again.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_final_path(report: &SpielordersReport) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        PathBytes::new(&report.data.final_path)
    })
}

/// The Package as this commit left it, opened as spielorders_package_open opens it.
/// Each call returns an independent new handle: free it with spielorders_package_free;
/// freeing the report does not invalidate it. Returns NULL when no reusable Package is
/// available, including a refresh that could not prove a current index or a report for
/// non-Package work; then open the Package again. An unchanged commit may supply one.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_package(
    report: &SpielordersReport,
) -> Option<Box<SpielordersPackage>> {
    quietly(None, || report.data.package.clone().map(Box::new))
}

/// Frees `report`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_free(report: Option<Box<SpielordersReport>>) {
    quietly((), || drop(report));
}
