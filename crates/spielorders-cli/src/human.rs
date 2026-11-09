//! The human output: one rendering of any answer, read off the value the document carries.
//!
//! Human output is never a parsing contract. What is chosen here is presentation:
//! identities, managed-file operations, and a plan's changes appear only under
//! `--verbose`, as the Rust CLI capability contract decides; money reads to two decimal
//! places, a choice of this front end, which the Order record decision leaves to it; and a
//! filesystem time reads in the local zone. Findings go to stderr, errors even under
//! `--quiet`.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record and
//! lifecycle contract.

use crate::{
    output::{Mode, Output},
    response::{Answer, Human, Shown},
};
use serde_json::Value;
use std::io;

/// Members a reader needs only under `--verbose`. A choice of this presentation.
fn detail_only(key: &str) -> bool {
    key.ends_with("_id")
        || matches!(
            key,
            "rank" | "position" | "committed" | "operations" | "changes" | "message" | "severity"
        )
}

/// Members whose text is money: the Order and Item totals, and the Unit Price.
const MONEY: [&str; 2] = ["total", "unit_price"];

/// Members whose contents are findings, which go to stderr.
fn findings(key: &str) -> bool {
    matches!(key, "findings" | "diagnostics")
}

struct Rendering {
    verbose: bool,
    /// Whether a failure's detail is being rendered, which names the position it refused.
    failure: bool,
    /// The member a rendered list stands for, where the answer is one member's list.
    top_key: &'static str,
    offered_only: bool,
    lines: Vec<String>,
    findings: Vec<(bool, String)>,
}

pub(crate) fn present(shown: &Shown<'_>, output: &Output) -> io::Result<()> {
    let human = shown.human;
    let mut rendering = Rendering {
        verbose: output.mode() == Mode::Verbose,
        failure: matches!(shown.answer, Answer::Failed { .. }),
        offered_only: matches!(human, Human::Offered),
        top_key: match human {
            Human::Member(path) => path.last().copied().unwrap_or_default(),
            _ => "",
        },
        lines: Vec::new(),
        findings: Vec::new(),
    };
    match &shown.answer {
        Answer::Answered { data, findings } => {
            let data = &for_the_reader(data);
            match human {
                Human::Whole | Human::Offered => rendering.value(0, data),
                Human::Verdict(verdict) => {
                    rendering.lines.push(verdict.to_owned());
                    rendering.value(0, data);
                }
                Human::Repair(command) => {
                    rendering.value(0, data);
                    let root = shown.root.map_or(".".into(), |root| root.to_string_lossy());
                    rendering.applications(command, &root, data);
                }
                Human::Member(path) => {
                    if let Some(member) = at(data, path) {
                        rendering.value(0, member);
                    }
                }
                Human::Bare(path) => {
                    if let Some(text) = at(data, path).and_then(Value::as_str) {
                        rendering.lines.push(text.to_owned());
                    }
                    // The answer stays the first line, so `head -1` still takes it.
                    if rendering.verbose {
                        rendering.value(0, data);
                    }
                }
            }
            for finding in findings {
                rendering.finding(&for_the_reader(finding));
            }
            for line in &rendering.lines {
                output.line(line)?;
            }
        }
        Answer::Failed { error, findings } => {
            let code = error["code"].as_str().unwrap_or("failed");
            output.error(&code.replace('_', " "))?;
            rendering.value(1, &for_the_reader(&error["detail"]));
            for line in &rendering.lines {
                output.error_detail(line)?;
            }
            for finding in findings {
                rendering.finding(&for_the_reader(finding));
            }
        }
    }
    // A finding carried in two places is reported once.
    let mut reported = Vec::new();
    for (error, text) in &rendering.findings {
        if reported.contains(&text) {
            continue;
        }
        reported.push(text);
        if *error {
            output.error(text)?;
        } else {
            output.warning(text)?;
        }
    }
    Ok(())
}

/// `value` with each monetary text rounded for the reader: a money member's own text, and
/// what a change or a finding to a money field was and is. Text that is not a number is
/// shown as it is.
fn for_the_reader(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(for_the_reader).collect()),
        Value::Object(members) => {
            let money_field = members
                .get("field")
                .and_then(Value::as_str)
                .is_some_and(|field| MONEY.contains(&field));
            let money = |key: &str| {
                MONEY.contains(&key) || (money_field && matches!(key, "before" | "after"))
            };
            members
                .iter()
                .map(|(key, member)| {
                    let amount = member.as_str().and_then(|text| text.parse().ok());
                    let shown = match amount {
                        Some(amount) if money(key) => Value::String(two_decimals(amount)),
                        _ => for_the_reader(member),
                    };
                    (key.clone(), shown)
                })
                .collect()
        }
        scalar => scalar.clone(),
    }
}

/// One monetary value to two decimal places, rounded.
///
/// **A choice of this presentation:** a value equal to zero reads `0.00` whichever zero it is, since a negative zero carries
/// no sign worth showing; a nonzero credit that rounds to zero keeps its sign as `-0.00`,
/// because only the magnitude was rounded away. Decided on the value rather than on the
/// rounded text, which cannot tell the two apart.
fn two_decimals(amount: f64) -> String {
    if amount == 0.0 {
        return "0.00".to_owned();
    }
    format!("{amount:.2}")
}

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}

impl Rendering {
    fn push(&mut self, indent: usize, text: String) {
        self.lines.push(format!("{}{text}", "  ".repeat(indent)));
    }

    /// One value standing alone at `indent`.
    fn value(&mut self, indent: usize, value: &Value) {
        match value {
            Value::Object(members) => {
                for (key, member) in members {
                    self.member(indent, key, member);
                }
            }
            Value::Array(items) if items.is_empty() => self.push(indent, "none".to_owned()),
            Value::Array(items) => {
                for item in items {
                    self.item(indent, self.top_key, item);
                }
            }
            scalar => {
                if let Some(text) = scalar_text("", scalar) {
                    self.push(indent, text);
                }
            }
        }
    }

    fn member(&mut self, indent: usize, key: &str, value: &Value) {
        if findings(key) {
            value
                .as_array()
                .into_iter()
                .flatten()
                .for_each(|finding| self.finding(finding));
            return;
        }
        // A collection's member reads with its position as the line that opens it.
        let shown = if key == "position" {
            self.failure
        } else {
            self.verbose || !detail_only(key)
        };
        if !shown || value.is_null() {
            return;
        }
        let label = key.replace('_', " ");
        match value {
            Value::Array(items) if items.is_empty() => self.push(indent, format!("{label}: none")),
            Value::Object(_) => {
                self.push(indent, format!("{label}:"));
                self.value(indent + 1, value);
            }
            Value::Array(items) if items.iter().all(|item| !item.is_object()) => {
                let joined: Vec<String> = items
                    .iter()
                    .filter_map(|item| scalar_text(key, item))
                    .collect();
                self.push(indent, format!("{label}: {}", joined.join(", ")));
            }
            Value::Array(items) => {
                self.push(indent, format!("{label}:"));
                for item in items {
                    // Hidden Vendors are listed only when every Vendor is asked for.
                    if !(self.offered_only && key == "vendors" && item["hidden"] == "true") {
                        self.item(indent + 1, key, item);
                    }
                }
            }
            Value::String(text) if text.contains('\n') => {
                self.push(indent, format!("{label}:"));
                for line in text.lines() {
                    self.push(indent + 1, line.to_owned());
                }
            }
            scalar => {
                if let Some(text) = scalar_text(key, scalar) {
                    self.push(indent, format!("{label}: {text}"));
                }
            }
        }
    }

    /// One element of a list, opened with its one-based position where it has one, and
    /// with a dash otherwise.
    fn item(&mut self, indent: usize, key: &str, item: &Value) {
        let start = self.lines.len();
        if item.is_object() {
            self.value(indent + 1, item);
        } else if let Some(text) = scalar_text(key, item) {
            self.push(indent + 1, text);
        }
        if let Some(first) = self.lines.get_mut(start) {
            let body = first.trim_start().to_owned();
            let opener = match item["position"].as_u64() {
                Some(position) => format!("{position}."),
                None => "-".to_owned(),
            };
            *first = format!("{}{opener} {body}", "  ".repeat(indent));
        }
    }

    /// The exact command line that applies each action a diagnosed case offers, with
    /// every value the action requires left for the reader to fill.
    fn applications(&mut self, command: &str, root: &str, data: &Value) {
        for case in data["cases"].as_array().into_iter().flatten() {
            let handle = case["handle"].as_str().unwrap_or_default();
            for offered in case["actions"].as_array().into_iter().flatten() {
                let Some(action) = offered["action"].as_str() else {
                    continue;
                };
                let required: String = offered["required_values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|value| format!(" {value}="))
                    .collect();
                self.lines.push(format!(
                    "to apply: {command} --root {} case={} action={action}{required}",
                    shell_word(root),
                    shell_word(handle),
                ));
            }
        }
    }

    /// One finding: a failure, a warning, or a diagnostic.
    fn finding(&mut self, finding: &Value) {
        let code = finding["code"].as_str().unwrap_or("finding");
        let error = finding["severity"] == "error";
        let mut text = code.replace('_', " ");
        if let Some(path) = finding["path"].as_str() {
            text = format!("{text}: {path}");
        }
        // What a fact was and is, which for a finding is its evidence, such as an identity.
        let facts: Vec<String> = ["field", "position", "reason", "before", "after"]
            .into_iter()
            .filter(|key| self.verbose || !matches!(*key, "before" | "after"))
            .filter_map(|key| {
                scalar_text(key, &finding[key])
                    .map(|shown| format!("{}: {shown}", key.replace('_', " ")))
            })
            .collect();
        if !facts.is_empty() {
            text = format!("{text} ({})", facts.join(", "));
        }
        self.findings.push((error, text));
    }
}

fn scalar_text(key: &str, value: &Value) -> Option<String> {
    Some(match value {
        Value::Null | Value::Array(_) | Value::Object(_) => return None,
        Value::Bool(value) => (if *value { "yes" } else { "no" }).to_owned(),
        Value::Number(number) => number.to_string(),
        Value::String(text) if key == "modified" => {
            text.parse().map_or_else(|_| text.clone(), displayed_moment)
        }
        Value::String(text) => text.clone(),
    })
}

/// `value` as one shell word: as it is where a shell gives none of its characters a
/// meaning, and single-quoted otherwise.
fn shell_word(value: &str) -> String {
    const UNRESERVED: &str = "@%+=:,./-_";
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || UNRESERVED.contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

/// A filesystem time in the local zone, to the second. A count no calendar can place
/// reads back as itself.
fn displayed_moment(unix_seconds: i64) -> String {
    jiff::Timestamp::from_second(unix_seconds)
        .map(|moment| {
            moment
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| unix_seconds.to_string())
}
