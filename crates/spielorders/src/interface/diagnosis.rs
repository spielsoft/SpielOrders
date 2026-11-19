//! Validation, diagnosis, and repair: what is wrong with a Package or one Order, the
//! explicit actions each diagnosed case offers, and the plan of one of them.

use crate::drafting::Origin;
use crate::interface::{
    PathBytes, SPIELORDERS_DRAFT_KIND_ORDER, SPIELORDERS_DRAFT_KIND_TEXT,
    SPIELORDERS_DRAFT_KIND_VENDOR, SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_RESULT_BAD_ARGUMENT,
    SPIELORDERS_TEXT_FIELD_CONTENT, SPIELORDERS_VENDOR_FIELD_NAME, SpielordersConstant,
    SpielordersDraft, SpielordersEntries, SpielordersPackage, SpielordersPlan, SpielordersResult,
    Text,
    call::{Session, deliver, fallible, quietly, settle},
    codes,
    entries::{EMPTY, Entry, Subject, relative},
    plan::plan_change,
};
use crate::{
    Answer, Assignments, Change, Diagnosis, Failure, OrderRef, Scope,
    model::{RepairAction, Severity},
    ports::FileSystem,
};
use std::{ffi::CString, path::PathBuf};

/// A diagnosis of a whole Package or of one Order Directory, healthy or damaged.
#[repr(C)]
pub struct SpielordersDiagnosis {
    data: Box<SpielordersDiagnosisData>,
}

struct SpielordersDiagnosisData {
    diagnosis: Diagnosis,
    scope: Scope,
    root: PathBuf,
    findings: Vec<Entry>,
    cases: Vec<Case>,
}

struct Case {
    code: SpielordersConstant,
    path: CString,
    /// The Order Directory the case's evidence is in, or empty for a Package artifact.
    order: CString,
    handle: CString,
    actions: Vec<RepairAction>,
}

/// Each value a repair action may require, by the name the action reads it under, and
/// the field of the draft that holds it.
const REPAIR_VALUES: [(&str, SpielordersConstant); 3] = [
    ("record", SPIELORDERS_TEXT_FIELD_CONTENT),
    ("vendor", SPIELORDERS_ORDER_FIELD_VENDOR),
    ("name", SPIELORDERS_VENDOR_FIELD_NAME),
];

/// The values one repair action requires, each by the name the action reads it under and
/// the field of the draft that holds it.
fn action_fields(action: RepairAction) -> Vec<(&'static str, SpielordersConstant)> {
    action
        .required_values()
        .iter()
        .filter_map(|name| REPAIR_VALUES.iter().find(|(each, _)| each == name).copied())
        .collect()
}

impl SpielordersDiagnosis {
    fn of(diagnosis: Diagnosis, scope: Scope) -> Self {
        let findings = diagnosis
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .map(Entry::finding)
            .collect();
        let cases = diagnosis
            .cases
            .iter()
            .map(|case| Case {
                code: codes::finding(case.code),
                path: relative(&case.location),
                order: diagnosis
                    .examined_orders
                    .iter()
                    .map(|order| &order.directory.absolute_path)
                    .find(|directory| case.location.absolute_path.starts_with(directory))
                    .map_or_else(CString::default, |directory| {
                        crate::interface::records::path_text(directory)
                    }),
                handle: crate::interface::entries::c_text(case.handle.as_str()),
                actions: case.actions.clone(),
            })
            .collect();
        let root = diagnosis.context.root.absolute_path.clone();
        Self {
            data: Box::new(SpielordersDiagnosisData {
                diagnosis,
                scope,
                root,
                findings,
                cases,
            }),
        }
    }

    fn case(&self, case: i64) -> Option<&Case> {
        self.data.cases.get(usize::try_from(case).ok()?)
    }

    fn action(&self, case: i64, action: i64) -> Option<RepairAction> {
        self.case(case)?
            .actions
            .get(usize::try_from(action).ok()?)
            .copied()
    }
}

/// Diagnoses or validates `scope`. A validation's invalid verdict is its diagnosis, not a
/// refusal.
fn diagnose(
    scope: Scope,
    validate: bool,
    token: u64,
    diagnosis: &mut Option<Box<SpielordersDiagnosis>>,
    entries: &mut Vec<Entry>,
) -> SpielordersResult {
    let session = Session::new(token);
    let answer = if validate {
        crate::validate(&scope, &session, session.cancellation())
    } else {
        crate::diagnose(&scope, &session, session.cancellation())
    };
    let answer = Answer {
        result: answer.result.or_else(|failure| match failure {
            Failure::PackageInvalid(found) | Failure::OrderInvalid(found) => Ok(*found),
            other => Err(other),
        }),
        diagnostics: answer.diagnostics,
    };
    deliver(
        settle(answer, &session, Subject::Other, entries)
            .map(|found| SpielordersDiagnosis::of(found, scope)),
        diagnosis,
    )
}

/// Validates all of `package`: every Order Directory and every cross-record constraint.
/// Works on a Damaged Package.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_validate(
    package: &SpielordersPackage,
    token: u64,
    diagnosis: &mut Option<Box<SpielordersDiagnosis>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(diagnosis, None);
    fallible(outcome, |entries| {
        diagnose(
            Scope::Package(package.root().to_path_buf()),
            true,
            token,
            diagnosis,
            entries,
        )
    })
}

/// Validates one Order Directory of `package`, damaged or not.
/// A directory outside `package` refuses SPIELORDERS_FAILURE_ORDER_NOT_FOUND.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_validate_order(
    package: &SpielordersPackage,
    order_directory: PathBytes<'_>,
    token: u64,
    diagnosis: &mut Option<Box<SpielordersDiagnosis>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(diagnosis, None);
    fallible(outcome, |entries| {
        if let Err(result) = package.holds(order_directory.as_path(), &Session::new(token), entries)
        {
            return result;
        }
        let scope = Scope::Order(OrderRef::Directory(order_directory.as_path().to_path_buf()));
        diagnose(scope, true, token, diagnosis, entries)
    })
}

/// Diagnoses the repairs `package`'s own artifacts offer. Works on a Damaged Package.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_diagnose(
    package: &SpielordersPackage,
    token: u64,
    diagnosis: &mut Option<Box<SpielordersDiagnosis>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(diagnosis, None);
    fallible(outcome, |entries| {
        diagnose(
            Scope::Package(package.root().to_path_buf()),
            false,
            token,
            diagnosis,
            entries,
        )
    })
}

/// Diagnoses the repairs one Order Directory of `package` offers, damaged or not.
/// A directory outside `package` refuses SPIELORDERS_FAILURE_ORDER_NOT_FOUND.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_diagnose_order(
    package: &SpielordersPackage,
    order_directory: PathBytes<'_>,
    token: u64,
    diagnosis: &mut Option<Box<SpielordersDiagnosis>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(diagnosis, None);
    fallible(outcome, |entries| {
        if let Err(result) = package.holds(order_directory.as_path(), &Session::new(token), entries)
        {
            return result;
        }
        let scope = Scope::Order(OrderRef::Directory(order_directory.as_path().to_path_buf()));
        diagnose(scope, false, token, diagnosis, entries)
    })
}

/// 1 when no finding of `diagnosis` is an error, otherwise 0.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_is_valid(diagnosis: &SpielordersDiagnosis) -> i32 {
    quietly(0, || {
        i32::from(
            !diagnosis
                .data
                .diagnosis
                .diagnostic_groups
                .iter()
                .flat_map(|group| &group.diagnostics)
                .any(|finding| finding.severity == Severity::Error),
        )
    })
}

/// Every finding of `diagnosis`, as a new Entries handle the caller frees, or NULL when
/// there are none.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_findings(
    diagnosis: &SpielordersDiagnosis,
) -> Option<Box<SpielordersEntries>> {
    quietly(None, || {
        SpielordersEntries::clone_of(&diagnosis.data.findings)
    })
}

/// How many repair cases `diagnosis` offers.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_case_count(diagnosis: &SpielordersDiagnosis) -> i64 {
    quietly(-1, || {
        i64::try_from(diagnosis.data.cases.len()).unwrap_or(-1)
    })
}

/// The `SPIELORDERS_FINDING_*` case `case` was diagnosed from, or -1 out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_case_code(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        diagnosis
            .case(case)
            .map_or(SpielordersConstant::NONE, |case| case.code)
    })
}

/// The Package-relative path of the evidence case `case` was diagnosed from, or "".
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_case_path(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        diagnosis
            .case(case)
            .map_or(Text::new(EMPTY), |case| Text::new(&case.path))
    })
}

/// The Order Directory case `case` belongs to, which `spielorders_package_diagnose_order`
/// diagnoses and `spielorders_package_validate_order` validates; "" for a case about the
/// Package's own artifacts, which `spielorders_package_diagnose` offers, and out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_case_order(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        diagnosis
            .case(case)
            .map_or(PathBytes::new(EMPTY), |case| PathBytes::new(&case.order))
    })
}

/// The handle that names case `case` to a later repair: its code, its path, and the
/// digest of the evidence it was diagnosed from, so a handle names nothing once that
/// evidence changes; "" out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_case_handle(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        diagnosis
            .case(case)
            .map_or(Text::new(EMPTY), |case| Text::new(&case.handle))
    })
}

/// How many explicit actions case `case` offers.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_action_count(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
) -> i64 {
    quietly(-1, || {
        diagnosis
            .case(case)
            .map_or(-1, |case| i64::try_from(case.actions.len()).unwrap_or(-1))
    })
}

/// The `SPIELORDERS_REPAIR_ACTION_*` of action `action` of case `case`, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_action(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
    action: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        diagnosis
            .action(case, action)
            .map_or(SpielordersConstant::NONE, codes::repair_action)
    })
}

/// How many values action `action` of case `case` requires; its draft holds them.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_action_field_count(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
    action: i64,
) -> i64 {
    quietly(-1, || {
        diagnosis.action(case, action).map_or(-1, |action| {
            i64::try_from(action_fields(action).len()).unwrap_or(-1)
        })
    })
}

/// Required value `field` of action `action` of case `case`: the draft field that holds
/// it, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_action_field(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
    action: i64,
    field: i64,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        diagnosis
            .action(case, action)
            .and_then(|action| {
                action_fields(action)
                    .get(usize::try_from(field).ok()?)
                    .map(|(_, field)| *field)
            })
            .unwrap_or(SpielordersConstant::NONE)
    })
}

/// A draft holding the values action `action` of case `case` requires: a TEXT draft
/// prefilled with the file's current text for a whole-file replacement (empty where that
/// text is not UTF-8), an ORDER draft holding the Vendor to assign, or a VENDOR draft
/// holding the new Vendor's Name. BAD_ARGUMENT for an action that requires no values.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_draft(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
    action: i64,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(draft, None);
    fallible(outcome, |_| {
        let Some(chosen) = diagnosis.action(case, action) else {
            return deliver(Err(SPIELORDERS_RESULT_BAD_ARGUMENT), draft);
        };
        let made = match action_fields(chosen).first().map(|(_, field)| *field) {
            Some(SPIELORDERS_TEXT_FIELD_CONTENT) => {
                let location = &diagnosis.data.diagnosis.cases
                    [usize::try_from(case).unwrap_or_default()]
                .location;
                let text = Session::uncancellable()
                    .read(&location.absolute_path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .filter(|text| !text.contains('\0'))
                    .unwrap_or_default();
                Ok(SpielordersDraft::new(
                    SPIELORDERS_DRAFT_KIND_TEXT,
                    Origin::Repair,
                    vec![(SPIELORDERS_TEXT_FIELD_CONTENT, text)],
                    Vec::new(),
                ))
            }
            Some(SPIELORDERS_ORDER_FIELD_VENDOR) => Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_ORDER,
                Origin::Repair,
                Vec::new(),
                Vec::new(),
            )),
            Some(SPIELORDERS_VENDOR_FIELD_NAME) => Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_VENDOR,
                Origin::Repair,
                Vec::new(),
                Vec::new(),
            )),
            _ => Err(SPIELORDERS_RESULT_BAD_ARGUMENT),
        };
        deliver(made, draft)
    })
}

/// Plans action `action` of case `case` with the values `draft` (or NULL, for an action
/// that requires none) holds. Its commit refuses if the evidence the case was diagnosed
/// from changed.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_plan_repair(
    diagnosis: &SpielordersDiagnosis,
    case: i64,
    action: i64,
    draft: Option<&SpielordersDraft>,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(plan, None);
    fallible(outcome, |entries| {
        let result = (|| {
            let chosen = diagnosis
                .action(case, action)
                .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let handle = diagnosis.data.diagnosis.cases[usize::try_from(case).unwrap_or_default()]
                .handle
                .clone();
            let mut values = Assignments::new();
            for (name, field) in action_fields(chosen) {
                let value = draft
                    .map(|draft| {
                        draft
                            .guarded(None, None, |data| Some(data.text(field)))
                            .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)
                    })
                    .transpose()?
                    .unwrap_or_default();
                values = values.assign(name, value);
            }
            let change = Change::Repair {
                scope: diagnosis.data.scope.clone(),
                case: handle,
                action: chosen,
                values,
            };
            plan_change(
                &change,
                diagnosis.data.root.clone(),
                Subject::Other,
                token,
                entries,
                |_| Vec::new(),
            )
        })();
        deliver(result, plan)
    })
}

/// Frees `diagnosis`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_diagnosis_free(diagnosis: Option<Box<SpielordersDiagnosis>>) {
    quietly((), || drop(diagnosis));
}
