//! The response envelope, and the one way any command's answer reaches a stream.
//!
//! A command hands over the answer it built; nothing here knows which command produced
//! it. The envelope is the decided seven members in their order: `data` is what the
//! command read or did, `error` is the failure it refused with — `{code, detail}` — and
//! `diagnostics` is every finding beside either.
//!
//! Decision sources: Define the Rust CLI capability contract.

use crate::{contract::ResponseContract, exit::Exit, human, output::Output};
use serde_json::{Value, json};
use std::path::Path;

/// What one command answered: what it read or did, or its failure, and the findings
/// beside either.
pub(crate) enum Answer {
    Answered { data: Value, findings: Vec<Value> },
    Failed { error: Value, findings: Vec<Value> },
}

/// What the human output shows of a successful answer.
#[derive(Clone, Copy)]
pub(crate) enum Human {
    /// The whole answer.
    Whole,
    /// The whole answer, offering only the Vendors offered for new use.
    Offered,
    /// The whole answer of a validation that found the scope valid, under this verdict.
    Verdict(&'static str),
    /// The whole answer of a repair diagnosis, with the command line that applies each
    /// offered action through this command.
    Repair(&'static str),
    /// The one member at this path, rendered as the answer.
    Member(&'static [&'static str]),
    /// The one text at this path, printed bare so another tool can take it.
    Bare(&'static [&'static str]),
}

/// One command's answer, ready to present.
pub(crate) struct Shown<'a> {
    pub(crate) contract: ResponseContract,
    pub(crate) root: Option<&'a Path>,
    pub(crate) answer: Answer,
    pub(crate) human: Human,
}

/// The document `shown` is, as bytes.
pub(crate) fn document(shown: &Shown<'_>) -> std::io::Result<Vec<u8>> {
    let (data, error, findings) = match &shown.answer {
        Answer::Answered { data, findings } => (data, &Value::Null, findings),
        Answer::Failed { error, findings } => (&Value::Null, error, findings),
    };
    let envelope = json!({
        "contract": shown.contract.name(),
        "contract_version": shown.contract.version(),
        "ok": matches!(shown.answer, Answer::Answered { .. }),
        "context": { "root": shown.root.and_then(Path::to_str) },
        "data": data,
        "diagnostics": findings,
        "error": error,
    });
    let mut bytes = serde_json::to_vec_pretty(&envelope).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes `shown` in the output mode this process reports in, and answers its exit.
pub(crate) fn present(shown: &Shown<'_>, output: &Output) -> Exit {
    let exit = match shown.answer {
        Answer::Answered { .. } => Exit::Success,
        Answer::Failed { .. } => Exit::Failure,
    };
    let written = if output.mode().is_json() {
        document(shown).and_then(|bytes| output.document(&bytes))
    } else {
        human::present(shown, output)
    };
    match written {
        Ok(()) => exit,
        Err(error) => {
            let _ = output.error(&format!("the result could not be written: {error}"));
            Exit::Failure
        }
    }
}
