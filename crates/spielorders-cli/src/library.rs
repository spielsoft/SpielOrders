//! Calls into the library's interface, and the JSON this executable builds from what it
//! answers.
//!
//! Every value is read through the interface's own accessors, and every key is the name
//! the library gives the constant it reads, so a fact the library adds reaches the
//! document without a change here. Positions are one-based, as the command line writes
//! them.
//!
//! Decision sources: Define the Rust CLI capability contract; Effect and adapter
//! boundaries, Interface rules.

use crate::response::Answer;
use serde_json::{Map, Value, json};
use spielorders::*;
use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

pub(crate) type Outcome = Option<Box<SpielordersEntries>>;

/// Text for the interface. Nothing a command line carries holds a NUL, and one that did
/// would reach the library as the empty text it then refuses.
pub(crate) fn c_text(text: impl Into<Vec<u8>>) -> CString {
    CString::new(text).unwrap_or_default()
}

pub(crate) fn c_path(path: &Path) -> CString {
    c_text(path.as_os_str().as_bytes())
}

/// The library's name for `constant`, or `None` for -1 and anything else unnamed.
pub(crate) fn name(constant: SpielordersConstant) -> Option<&'static str> {
    spielorders_constant_name(constant)
        .to_str()
        .filter(|name| !name.is_empty())
}

pub(crate) fn named(constant: SpielordersConstant) -> Value {
    name(constant).map_or(Value::Null, Value::from)
}

/// The constant of `family` that `name` spells.
pub(crate) fn constant(family: SpielordersConstant, name: &str) -> Option<SpielordersConstant> {
    let name = c_text(name);
    Some(spielorders_constant_from_name(family, Text::new(&name)))
        .filter(|constant| *constant != SpielordersConstant(-1))
}

fn text(text: Text<'_>) -> Value {
    match text.to_str() {
        Some("") => Value::Null,
        Some(text) => Value::from(text),
        None => Value::from(text.as_c_str().to_string_lossy().into_owned()),
    }
}

pub(crate) fn path(path: PathBytes<'_>) -> Value {
    match path.as_path().to_string_lossy() {
        empty if empty.is_empty() => Value::Null,
        path => Value::from(path.into_owned()),
    }
}

/// A one-based position, from the interface's zero-based row; -1 is none.
fn position(row: i64) -> Value {
    if row < 0 {
        Value::Null
    } else {
        Value::from(row + 1)
    }
}

/// Every entry: failures, warnings, and findings alike.
pub(crate) fn entries(entries: Option<&SpielordersEntries>) -> Vec<Value> {
    (0..spielorders_entries_count(entries))
        .map(|index| {
            json!({
                "code": named(spielorders_entries_code(entries, index)),
                "severity": named(spielorders_entries_severity(entries, index)),
                "message": text(spielorders_entries_message(entries, index)),
                "field": named(spielorders_entries_field(entries, index)),
                "reason": named(spielorders_entries_reason(entries, index)),
                "position": position(spielorders_entries_row(entries, index)),
                "path": text(spielorders_entries_path(entries, index)),
                "before": text(spielorders_entries_before(entries, index)),
                "after": text(spielorders_entries_after(entries, index)),
            })
        })
        .collect()
}

/// The calls one command makes, and the findings they gave beside their answers.
#[derive(Default)]
pub(crate) struct Calls {
    findings: Vec<Value>,
}

impl Calls {
    /// The handle a fallible call gives, keeping its entries as findings; or its refusal.
    pub(crate) fn get<T>(
        &mut self,
        call: impl FnOnce(&mut Option<Box<T>>, Option<&mut Outcome>) -> SpielordersResult,
    ) -> Result<Box<T>, Answer> {
        let mut value = None;
        let mut outcome = None;
        let result = call(&mut value, Some(&mut outcome));
        let listed = entries(outcome.as_deref());
        match value {
            Some(value) if result == SPIELORDERS_RESULT_OK => {
                self.findings.extend(listed);
                Ok(value)
            }
            _ => Err(self.refused(result, listed)),
        }
    }

    /// A fallible call that gives no handle.
    pub(crate) fn act(
        &mut self,
        call: impl FnOnce(Option<&mut Outcome>) -> SpielordersResult,
    ) -> Result<(), Answer> {
        self.attempt(call).map_err(|(_, refused)| refused)
    }

    /// A fallible call that gives no handle, saying how it refused.
    pub(crate) fn attempt(
        &mut self,
        call: impl FnOnce(Option<&mut Outcome>) -> SpielordersResult,
    ) -> Result<(), (SpielordersResult, Answer)> {
        let mut outcome = None;
        let result = call(Some(&mut outcome));
        let listed = entries(outcome.as_deref());
        if result == SPIELORDERS_RESULT_OK {
            self.findings.extend(listed);
            Ok(())
        } else {
            Err((result, self.refused(result, listed)))
        }
    }

    /// The findings opening `package` made, which a read that carries no entries of its own
    /// reports beside its answer.
    pub(crate) fn found(&mut self, package: &SpielordersPackage) {
        self.findings
            .extend(entries(spielorders_package_findings(package).as_deref()));
    }

    /// Whether any call so far warned.
    pub(crate) fn warned(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding["severity"] == "warning")
    }

    pub(crate) fn answered(self, data: Value) -> Answer {
        Answer::Answered {
            data,
            findings: self.findings,
        }
    }

    /// A refusal: its first failure is the error, and every other entry stands beside it.
    fn refused(&mut self, result: SpielordersResult, mut listed: Vec<Value>) -> Answer {
        let failure = listed.iter().position(|entry| {
            entry["code"]
                .as_str()
                .and_then(|code| constant(SPIELORDERS_FAMILY_FAILURE, code))
                .is_some()
        });
        let (code, detail) = match failure {
            Some(at) => {
                let mut entry = listed.remove(at);
                let code = entry["code"].take();
                if let Some(detail) = entry.as_object_mut() {
                    detail.shift_remove("code");
                }
                (code, entry)
            }
            None => (named(SpielordersConstant(result.0)), Value::Null),
        };
        let mut findings = std::mem::take(&mut self.findings);
        findings.extend(listed);
        Answer::Failed {
            error: json!({"code": code, "detail": detail}),
            findings,
        }
    }

    /// A refusal this executable makes itself, under the library's name for it.
    pub(crate) fn refuse(&mut self, code: SpielordersConstant, detail: Value) -> Answer {
        self.refuse_own(name(code).unwrap_or_default(), detail)
    }

    /// A refusal of this executable's own platform service, under its own code.
    pub(crate) fn refuse_own(&mut self, code: &str, detail: Value) -> Answer {
        Answer::Failed {
            error: json!({"code": code, "detail": detail}),
            findings: std::mem::take(&mut self.findings),
        }
    }
}

/// The members of one family, from its first, each named as the library names it.
fn fields(
    first: SpielordersConstant,
    read: impl Fn(SpielordersConstant) -> Value,
) -> Map<String, Value> {
    (first.0..)
        .map(SpielordersConstant)
        .map_while(|field| name(field).map(|name| (name.to_owned(), read(field))))
        .collect()
}

pub(crate) fn order(order: &SpielordersOrder) -> Value {
    let mut facts = fields(SPIELORDERS_ORDER_FIELD_TITLE, |field| {
        text(spielorders_order_field(order, field))
    });
    facts.insert(
        "directory".to_owned(),
        path(spielorders_order_directory(order)),
    );
    let items = (0..spielorders_order_item_count(order))
        .map(|row| {
            let mut item = Map::from_iter([("position".to_owned(), position(row))]);
            item.extend(fields(SPIELORDERS_ITEM_FIELD_DESCRIPTION, |field| {
                text(spielorders_order_item_field(order, row, field))
            }));
            Value::Object(item)
        })
        .collect();
    facts.insert("items".to_owned(), Value::Array(items));
    if let Some(readme) = spielorders_order_readme(order) {
        facts.insert("readme".to_owned(), self::readme(&readme));
    }
    Value::Object(facts)
}

pub(crate) fn vendor(vendor: &SpielordersVendor) -> Value {
    let mut facts = fields(SPIELORDERS_VENDOR_FIELD_NAME, |field| {
        text(spielorders_vendor_field(vendor, field))
    });
    let contacts = (0..spielorders_vendor_contact_count(vendor))
        .map(|row| {
            let mut contact = Map::from_iter([("position".to_owned(), position(row))]);
            contact.extend(fields(SPIELORDERS_CONTACT_FIELD_NAME, |field| {
                text(spielorders_vendor_contact_field(vendor, row, field))
            }));
            Value::Object(contact)
        })
        .collect();
    facts.insert("contacts".to_owned(), Value::Array(contacts));
    Value::Object(facts)
}

pub(crate) fn readme(readme: &SpielordersReadme) -> Value {
    json!({
        "state": named(spielorders_readme_state(readme)),
        "path": path(spielorders_readme_path(readme)),
    })
}

/// The rows of a list whose fields are the family beginning at `first`.
pub(crate) fn rows(list: &SpielordersList, first: SpielordersConstant) -> Value {
    let orders = first == SPIELORDERS_ORDER_FIELD_TITLE;
    (0..spielorders_list_count(list))
        .map(|row| {
            let mut facts = fields(first, |field| {
                text(spielorders_list_field(list, row, field))
            });
            if orders {
                facts.insert(
                    "damaged".to_owned(),
                    Value::from(spielorders_list_damaged(list, row) == 1),
                );
                facts.insert(
                    "directory".to_owned(),
                    path(spielorders_list_path(list, row)),
                );
            }
            Value::Object(facts)
        })
        .collect()
}

/// What a plan will do: its filesystem operations, and its changes to the record.
pub(crate) fn plan(plan: &SpielordersPlan) -> Map<String, Value> {
    let operations = (0..spielorders_plan_operation_count(plan))
        .map(|index| {
            json!({
                "kind": named(spielorders_plan_operation_kind(plan, index)),
                "path": path(spielorders_plan_operation_path(plan, index)),
            })
        })
        .collect();
    let changes = (0..spielorders_plan_change_count(plan))
        .map(|index| {
            let kind = spielorders_plan_change_kind(plan, index);
            let mut before = text(spielorders_plan_change_before(plan, index));
            let mut after = text(spielorders_plan_change_after(plan, index));
            // A moved row's two ends are rows, which this command line numbers from one.
            if kind == SPIELORDERS_CHANGE_ROW_MOVED {
                for end in [&mut before, &mut after] {
                    if let Some(row) = end.as_str().and_then(|row| row.parse::<i64>().ok()) {
                        *end = position(row);
                    }
                }
            }
            json!({
                "kind": named(kind),
                "field": named(spielorders_plan_change_field(plan, index)),
                "position": position(spielorders_plan_change_row(plan, index)),
                "before": before,
                "after": after,
                "reuse": named(spielorders_plan_change_reuse(plan, index)),
            })
        })
        .collect();
    Map::from_iter([
        ("operations".to_owned(), Value::Array(operations)),
        ("changes".to_owned(), Value::Array(changes)),
    ])
}

/// What a commit did, and the record it refreshed. A commit that found nothing to change
/// is still committed, and says `unchanged`; one that warned says so.
pub(crate) fn report(
    report: &SpielordersReport,
    planned: Map<String, Value>,
    warned: bool,
) -> Map<String, Value> {
    let kind = spielorders_report_kind(report);
    let outcome = if warned && kind == SPIELORDERS_REPORT_COMMITTED {
        Value::from("committed_with_warning")
    } else {
        named(kind)
    };
    let mut data = Map::from_iter([
        ("outcome".to_owned(), outcome),
        ("committed".to_owned(), Value::from(true)),
    ]);
    data.extend(planned);
    if let Some(order) = spielorders_report_order(report) {
        data.insert("order".to_owned(), self::order(&order));
    }
    if let Some(vendor) = spielorders_report_vendor(report) {
        data.insert("vendor".to_owned(), self::vendor(&vendor));
    }
    data.insert(
        "final_path".to_owned(),
        path(spielorders_report_final_path(report)),
    );
    data
}

/// A dry run's answer: the plan, never committed.
pub(crate) fn planned(planned: Map<String, Value>) -> Map<String, Value> {
    let mut data = Map::from_iter([
        ("outcome".to_owned(), Value::from("planned")),
        ("committed".to_owned(), Value::from(false)),
    ]);
    data.extend(planned);
    data
}

/// The handle a diagnosis gives a case, which names it to a later repair only while the
/// evidence it was diagnosed from stands.
pub(crate) fn case_handle(diagnosis: &SpielordersDiagnosis, case: i64) -> String {
    spielorders_diagnosis_case_handle(diagnosis, case)
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

/// The name a repair's command line gives one required value: the field's own, except
/// a whole replacement file, which it calls `record`.
pub(crate) fn resolution_name(field: SpielordersConstant) -> &'static str {
    if field == SPIELORDERS_TEXT_FIELD_CONTENT {
        "record"
    } else {
        name(field).unwrap_or_default()
    }
}

pub(crate) fn diagnosis(diagnosis: &SpielordersDiagnosis) -> Value {
    let cases: Vec<Value> = (0..spielorders_diagnosis_case_count(diagnosis))
        .map(|case| {
            let actions: Vec<Value> = (0..spielorders_diagnosis_action_count(diagnosis, case))
                .map(|action| {
                    let required: Vec<Value> =
                        (0..spielorders_diagnosis_action_field_count(diagnosis, case, action))
                            .map(|field| {
                                Value::from(resolution_name(spielorders_diagnosis_action_field(
                                    diagnosis, case, action, field,
                                )))
                            })
                            .collect();
                    json!({
                        "action": named(spielorders_diagnosis_action(diagnosis, case, action)),
                        "required_values": required,
                    })
                })
                .collect();
            json!({
                "handle": case_handle(diagnosis, case),
                "code": named(spielorders_diagnosis_case_code(diagnosis, case)),
                "path": text(spielorders_diagnosis_case_path(diagnosis, case)),
                "actions": actions,
            })
        })
        .collect();
    json!({
        "valid": spielorders_diagnosis_is_valid(diagnosis) == 1,
        "findings": entries(spielorders_diagnosis_findings(diagnosis).as_deref()),
        "cases": cases,
    })
}

pub(crate) fn search(search: &SpielordersSearch) -> Value {
    let files: Vec<Value> = (0..spielorders_search_count(search))
        .map(|index| {
            json!({
                "order_directory": path(spielorders_search_order_directory(search, index)),
                "path": path(spielorders_search_path(search, index)),
                "relative_path": text(spielorders_search_relative_path(search, index)),
                "filename": text(spielorders_search_filename(search, index)),
                "kind": text(spielorders_search_kind(search, index)),
                "modified": text(spielorders_search_modified(search, index)),
                "rank": text(spielorders_search_rank(search, index)),
            })
        })
        .collect();
    let said: Vec<Value> = (0..spielorders_search_diagnostic_count(search))
        .map(|index| text(spielorders_search_diagnostic(search, index)))
        .collect();
    json!({
        "provider": text(spielorders_search_provider(search)),
        "state": named(spielorders_search_state(search)),
        "files": files,
        "provider_diagnostics": said,
    })
}
