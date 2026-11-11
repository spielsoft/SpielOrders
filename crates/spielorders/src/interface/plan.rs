//! Plans and commits. A plan writes nothing; a dry run is a plan never committed. A commit
//! re-checks every piece of evidence its plan was made against before its first write.

use crate::interface::{
    OptionalPathBytes, PathBytes, SPIELORDERS_CHANGE_FACT, SPIELORDERS_CHANGE_NEW_IDENTITY,
    SPIELORDERS_CHANGE_ROW_ADDED, SPIELORDERS_CHANGE_ROW_MOVED, SPIELORDERS_CHANGE_ROW_REMOVED,
    SPIELORDERS_DRAFT_KIND_ORDER, SPIELORDERS_FAILURE_PANICKED, SPIELORDERS_FAMILY_OPERATION,
    SPIELORDERS_ORDER_FIELD_ORDER_GROUP, SPIELORDERS_ORDER_FIELD_ORDERER,
    SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION, SPIELORDERS_ORDER_FIELD_PROJECT,
    SPIELORDERS_REPORT_COMMITTED, SPIELORDERS_REPORT_UNCHANGED, SPIELORDERS_RESULT_BAD_ARGUMENT,
    SPIELORDERS_RESULT_INTERNAL, SPIELORDERS_REUSE_NEW, SPIELORDERS_REUSE_REUSED,
    SPIELORDERS_SEVERITY_ERROR, SPIELORDERS_VENDOR_FIELD_HIDDEN, SpielordersConstant,
    SpielordersDraft, SpielordersEntries, SpielordersOrder, SpielordersPackage, SpielordersReadme,
    SpielordersResult, SpielordersVendor, Text,
    call::{Session, deliver, fallible, put, quietly, settle},
    draft::{Origin, SpielordersDraftData},
    entries::{EMPTY, Entry, Subject, c_text},
    library::member,
    records::{ReadmeScope, contact_facts, document_facts, item_facts, path_text, vendor_facts},
};
use crate::{
    Change, ChangeResult, OrderRef, Plan, Report, Scope, VendorRef, package::ReusableValue,
};
use std::{
    ffi::CString,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
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
        }
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
}

fn operations(plan: &Plan) -> Vec<(SpielordersConstant, CString)> {
    plan.operations()
        .into_iter()
        .map(|(kind, path)| {
            (
                member(SPIELORDERS_FAMILY_OPERATION, &kind).unwrap_or(SpielordersConstant::NONE),
                path_text(&path.absolute_path),
            )
        })
        .collect()
}

type Record = (
    Vec<(SpielordersConstant, String)>,
    Vec<Vec<(SpielordersConstant, String)>>,
);

/// The record a plan would write, read in the fields its draft edits.
fn planned_record(draft: &SpielordersDraftData, plan: &Plan) -> Option<Record> {
    let (mut facts, mut rows): Record = if draft.kind == SPIELORDERS_DRAFT_KIND_ORDER {
        let (document, vendor, _) = plan.order_record()?;
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
    facts.retain(|(field, _)| draft.settable().contains(field));
    for row in &mut rows {
        row.retain(|(field, _)| draft.row_settable().contains(field));
    }
    Some((facts, rows))
}

/// Whether `field`'s planned value reuses a value the Package already has, or is new.
fn reuse(plan: &Plan, field: SpielordersConstant) -> SpielordersConstant {
    let Some((_, _, values)) = plan.order_record() else {
        return SpielordersConstant::NONE;
    };
    let value = match field {
        SPIELORDERS_ORDER_FIELD_ORDERER => &values.orderer,
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION => &values.ordering_institution,
        SPIELORDERS_ORDER_FIELD_PROJECT => &values.project,
        SPIELORDERS_ORDER_FIELD_ORDER_GROUP => &values.order_group,
        _ => return SpielordersConstant::NONE,
    };
    match value {
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
fn draft_changes(draft: &SpielordersDraftData, plan: &Plan) -> Vec<PlannedChange> {
    let Some((facts, rows)) = planned_record(draft, plan) else {
        return Vec::new();
    };
    let initial = draft.initial();
    let mut changes = Vec::new();
    if matches!(
        draft.origin,
        Origin::NewOrder { .. } | Origin::NewVendor { .. } | Origin::Adoption { .. }
    ) {
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
            change.reuse = reuse(plan, *field);
            changes.push(change);
        }
    }
    let mut kept = vec![false; initial.rows.len()];
    for (index, row) in draft.rows.iter().enumerate() {
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

/// Plans missing Package-local agent guidance without reading or changing root AGENTS.md.
/// Existing regular guidance is preserved. Commit rechecks configuration and destinations.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_plan_provision_agent_setup(
    package: &SpielordersPackage,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let result = package.healthy(entries).and_then(|_| {
            let root = package.root().to_path_buf();
            planned(
                &Change::ProvisionAgentSetup { root: root.clone() },
                root,
                Subject::Other,
                token,
                entries,
                |_| Vec::new(),
            )
        });
        deliver(result, plan)
    })
}

impl SpielordersPlan {
    fn of(
        plan: crate::Plan,
        root: PathBuf,
        subject: Subject,
        changes: impl FnOnce(&Plan) -> Vec<PlannedChange>,
    ) -> Self {
        Self {
            data: Box::new(SpielordersPlanData {
                operations: operations(&plan),
                changes: changes(&plan),
                plan,
                root,
                subject,
            }),
        }
    }
}

/// Plans `change`, honoring `token`, as a plan of `subject` under `root`.
pub(crate) fn planned(
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
    fallible(outcome, |entries| {
        let result = draft.guarded(
            Err(SPIELORDERS_RESULT_BAD_ARGUMENT),
            Err(SPIELORDERS_RESULT_INTERNAL),
            |data| {
                let change = data.change().ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
                let root = match &data.origin {
                    Origin::NewOrder { root }
                    | Origin::Adoption { root, .. }
                    | Origin::NewVendor { root }
                    | Origin::EditVendor { root, .. } => root.clone(),
                    Origin::EditOrder { directory, .. } => directory.clone(),
                    Origin::Repair => PathBuf::new(),
                };
                planned(&change, root, data.subject(), token, entries, |value| {
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
    fallible(outcome, |entries| {
        let directory = order.directory().to_path_buf();
        let change = Change::AlignOrder {
            order: OrderRef::Directory(directory.clone()),
        };
        deliver(
            planned(&change, directory, Subject::Order, token, entries, |_| {
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
    fallible(outcome, |entries| {
        let directory = order.directory().to_path_buf();
        let change = Change::RemoveOrder {
            order: OrderRef::Directory(directory.clone()),
        };
        deliver(
            planned(&change, directory, Subject::Order, token, entries, |_| {
                Vec::new()
            }),
            plan,
        )
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
    fallible(outcome, |entries| {
        let read = &vendor.data().read;
        let was = read.vendor.is_hidden();
        let change = Change::SetVendorHidden {
            root: vendor.root(),
            vendor: VendorRef::Id(read.vendor.vendor_id.to_string()),
            hidden: hidden != 0,
        };
        let result = planned(
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
            planned(&change, root, Subject::Other, token, entries, |_| {
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
    fallible(outcome, |entries| {
        let root = package.root().to_path_buf();
        let change = Change::RebuildIndex { root: root.clone() };
        deliver(
            planned(&change, root, Subject::Other, token, entries, |_| {
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
            planned(
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

/// How many changes `plan` makes to the record its draft edits.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_change_count(plan: &SpielordersPlan) -> i64 {
    quietly(-1, || i64::try_from(plan.data.changes.len()).unwrap_or(-1))
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
    final_path: CString,
}

fn refreshed(report: &Report, root: &std::path::Path, subject: Subject) -> SpielordersReportData {
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
        ChangeResult::Vendor(result) => (None, None, Some(result.vendor_id.to_string())),
        _ => (None, None, None),
    };
    let session = Session::uncancellable();
    let order = match (&written, subject) {
        (Some(directory), Subject::Order | Subject::Other) => {
            crate::read_order(&OrderRef::Directory(directory.clone()), &session)
                .result
                .ok()
                .map(SpielordersOrder::of)
        }
        _ => None,
    };
    let vendor = vendor_id
        .filter(|_| subject == Subject::Vendor)
        .and_then(|id| {
            crate::read_vendor(root, &VendorRef::Id(id), &session)
                .result
                .ok()
                .map(SpielordersVendor::of)
        });
    let final_path = removed
        .or(written)
        .map(|path| path_text(&path))
        .unwrap_or_default();
    SpielordersReportData {
        kind,
        order,
        vendor,
        final_path,
    }
}

/// Commits `plan`. Takes no token: it ends committed, refused before its first write
/// (SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED when anything it was planned against
/// changed), or failed with its rollback reported. Warnings arrive in the outcome with
/// SPIELORDERS_RESULT_OK. A plan may be committed once; a second commit refuses as its
/// evidence no longer stands.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_plan_commit(
    plan: &SpielordersPlan,
    report: &mut Option<Box<SpielordersReport>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::uncancellable();
        let committed = catch_unwind(AssertUnwindSafe(|| {
            crate::commit(plan.data.plan.clone(), &session)
        }));
        let answer = match committed {
            Ok(answer) => answer,
            Err(_) => {
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
            }
        };
        let result = settle(answer, &session, plan.data.subject, entries).map(|done| {
            if let Report::CommittedWithWarning { warnings, .. } = &done {
                for warning in warnings {
                    Entry::warnings(warning, entries);
                }
            }
            SpielordersReport {
                data: Box::new(refreshed(&done, &plan.data.root, plan.data.subject)),
            }
        });
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

/// A fresh read of the Vendor the commit wrote, as a new handle the caller frees, or
/// NULL when it wrote none.
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

/// Where the committed Order Directory now is, or "". Lists refresh by opening the
/// Package again.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_final_path(report: &SpielordersReport) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        PathBytes::new(&report.data.final_path)
    })
}

/// Frees `report`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_report_free(report: Option<Box<SpielordersReport>>) {
    quietly((), || drop(report));
}
