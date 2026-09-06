//! Every path by which a byte reaches standard output or standard error.
//!
//! There are eight, all of them methods on [`Output`], and no other module in this
//! crate writes to either stream. The set is small on purpose: "results to stdout,
//! warnings and errors to stderr" is only checkable if the places a byte can leave from
//! are countable.
//!
//! **What holds that.** The writing macros are denied at both crate roots, so the
//! accidental route cannot compile — a `println!` left in a command module writes to
//! standard output in every mode, including the one promised to carry exactly one JSON
//! document. And `tests/output_modes_and_streams.rs` asserts from real processes which
//! stream carries what in each mode, so a stray byte fails a test rather than passing
//! unnoticed. Neither closes a module that locks `io::stdout()` for itself, which is
//! deliberate work rather than an accident.
//!
//! **The four output modes are mutually exclusive.** Default human, `--json`,
//! `--quiet`, `--verbose`. Supplying more than one is a usage error rather than a
//! silent precedence, which is enforced where the command line is parsed.
//!
//! | | stdout | stderr |
//! | --- | --- | --- |
//! | human | the result | warnings and errors |
//! | `--verbose` | the result and its detail | warnings and errors |
//! | `--json` | exactly one JSON document | nothing |
//! | `--quiet` | nothing | errors only |
//!
//! Under `--json` the diagnostics and the failure travel *inside* the document, so
//! nothing incidental reaches either stream: a warning printed beside the document
//! would be the second thing on a stream promised to carry one.
//!
//! Generated `--help` and `--version` are reference text rather than a command
//! response. They go to stdout at exit `0` in every mode, because a request for the
//! reference is not a request for a result.
//!
//! **A usage error is reported to whichever audience the command line named.** Parsing
//! failed, so no mode was legitimately selected, and the executable errs toward the
//! machine: a command line carrying `--json` anywhere gets the one response document on
//! stdout via [`Output::document`], and every other one gets the parser's generated help
//! on stderr via [`Output::usage_text`]. That holds even when `--json` was named
//! *alongside* another mode — the conflict is what failed, and a script is worse served
//! by help text it cannot parse than a human is by a document they can still read.
//!
//! **Human output is never a parsing contract.** Its wording may change without
//! advancing any response contract, and only `--json` and the exit status carry
//! contract claims.
//!
//! Decision sources: Define the Rust CLI capability contract.

use serde::Serialize;
use std::{
    borrow::Cow,
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Human,
    Json,
    Quiet,
    Verbose,
}

impl Mode {
    pub(crate) fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Whether color may be used on a stream.
///
/// Color is interactive decoration and nothing more: it appears only when the stream
/// is an interactive terminal and the environment has not asked for none, so it
/// disappears the moment output is redirected into a file or a pipe. `NO_COLOR` set to
/// any nonempty value asks for none; the variable merely *existing* with an empty
/// value does not, which is the convention the variable comes from.
///
/// Taking the terminal answer as an argument rather than reading it here is what makes
/// the policy exercisable without a terminal: `crate::effects` observes, this decides.
pub(crate) fn color_permitted(stream_is_terminal: bool, no_color: Option<&OsStr>) -> bool {
    stream_is_terminal && !no_color.is_some_and(|request| !request.is_empty())
}

const RESET: &str = "\u{1b}[0m";
const RED: &str = "\u{1b}[31m";
const YELLOW: &str = "\u{1b}[33m";

/// The exact bytes of a filesystem path.
///
/// A platform path is not guaranteed to be valid UTF-8, and this command's whole
/// purpose is composition — `cat "$(spielorders package readme)"`. A lossy conversion
/// would print replacement characters and hand the shell a path that does not open, so
/// the path's own bytes are written.
#[cfg(unix)]
fn exact_bytes(value: &OsStr) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;

    Cow::Borrowed(value.as_bytes())
}

#[cfg(not(unix))]
fn exact_bytes(value: &OsStr) -> Cow<'_, [u8]> {
    // This platform exposes no byte view of its paths. The lossy spelling is all it
    // offers, and it is the same spelling its own shell would display.
    Cow::Owned(value.to_string_lossy().into_owned().into_bytes())
}

pub(crate) struct Output {
    mode: Mode,
    stdout_color: bool,
    stderr_color: bool,
}

impl Output {
    pub(crate) fn new(mode: Mode, stdout_color: bool, stderr_color: bool) -> Self {
        Self {
            mode,
            stdout_color,
            stderr_color,
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether color is permitted on standard output.
    ///
    /// A result and a JSON document are data and are never decorated, so the only
    /// thing this governs is generated reference text. It is exposed rather than
    /// applied here because the parser renders that text, and one holder of the color
    /// answer is what keeps a `NO_COLOR` request from being honored in half a program.
    pub(crate) fn stdout_color(&self) -> bool {
        self.stdout_color
    }

    /// Whether color is permitted on standard error.
    pub(crate) fn stderr_color(&self) -> bool {
        self.stderr_color
    }

    /// The command's result, as one line on standard output.
    ///
    /// Silent under `--quiet`, which produces no output on success. Never reached
    /// under `--json`, where the result travels inside the document.
    pub(crate) fn result_path(&self, path: &Path) -> io::Result<()> {
        if matches!(self.mode, Mode::Quiet | Mode::Json) {
            return Ok(());
        }
        let mut stream = io::stdout().lock();
        stream.write_all(&exact_bytes(path.as_os_str()))?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// One labelled line of the result itself, on standard output.
    ///
    /// The counterpart of [`Self::detail`]: this is part of the answer and appears in
    /// both human modes, where `detail` is extra and appears only under `--verbose`. A
    /// concise labelled summary is what a read command's human output is, and a command
    /// with more than one fact to report has no other way to say so — [`Self::result_path`]
    /// writes one bare path for composition into `cat` or `vi`.
    ///
    /// Silent under `--quiet`, which produces no output on success, and under `--json`,
    /// where the result travels inside the document.
    pub(crate) fn summary(&self, label: &str, value: &OsStr) -> io::Result<()> {
        if matches!(self.mode, Mode::Quiet | Mode::Json) {
            return Ok(());
        }
        self.labelled_line(label, value)
    }

    /// One labelled detail line, only under `--verbose`.
    pub(crate) fn detail(&self, label: &str, value: &OsStr) -> io::Result<()> {
        if self.mode != Mode::Verbose {
            return Ok(());
        }
        self.labelled_line(label, value)
    }

    /// `label: value` on standard output, with the value's own bytes.
    ///
    /// Held in one place so a summary line and a detail line cannot come to be spelled
    /// differently — they differ in which modes show them, not in how they read.
    fn labelled_line(&self, label: &str, value: &OsStr) -> io::Result<()> {
        let mut stream = io::stdout().lock();
        write!(stream, "{label}: ")?;
        stream.write_all(&exact_bytes(value))?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// One warning on standard error.
    ///
    /// Silent under `--quiet`, which produces no output on success, and under `--json`,
    /// where warnings are the document's `diagnostics`.
    pub(crate) fn warning(&self, message: &str) -> io::Result<()> {
        if matches!(self.mode, Mode::Quiet | Mode::Json) {
            return Ok(());
        }
        self.labelled(YELLOW, "warning", message)
    }

    /// One error on standard error.
    ///
    /// Never silenced by `--quiet`. Its rule is "no output on success while retaining
    /// errors on stderr", and an *error* is not only a failed command: an error-severity
    /// diagnostic about a Package that read fine is one too, so a `--quiet` run can write
    /// here and still exit `0`. Nothing about that is output *on success* in the sense
    /// the rule forbids — the result is what `--quiet` withholds, and it does.
    ///
    /// Silent under `--json`, where the failure is the document's `error` and a
    /// diagnostic is one of its `diagnostics`.
    pub(crate) fn error(&self, message: &str) -> io::Result<()> {
        if self.mode.is_json() {
            return Ok(());
        }
        self.labelled(RED, "error", message)
    }

    /// The label carries the meaning and the color only decorates it, so a stream
    /// without color loses nothing.
    fn labelled(&self, color: &str, label: &str, message: &str) -> io::Result<()> {
        let mut stream = io::stderr().lock();
        if self.stderr_color {
            write!(stream, "{color}{label}:{RESET} {message}")?;
        } else {
            write!(stream, "{label}: {message}")?;
        }
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// The one JSON document, on standard output.
    ///
    /// Pretty-printed and newline-terminated. It is still exactly one document, and
    /// the printing is what makes a golden fixture readable and its diff precise —
    /// which is the whole of the regression evidence the response contracts have.
    /// Formatting is not part of any contract's version, so this may change without
    /// advancing one; the fixtures move with it.
    pub(crate) fn document(&self, response: &impl Serialize) -> io::Result<()> {
        let mut stream = io::stdout().lock();
        serde_json::to_writer_pretty(&mut stream, response).map_err(io::Error::other)?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// Generated reference text — `--help` or `--version` — on standard output.
    pub(crate) fn reference(&self, text: &str) -> io::Result<()> {
        let mut stream = io::stdout().lock();
        stream.write_all(text.as_bytes())?;
        stream.flush()
    }

    /// A generated usage error, in full, on standard error.
    ///
    /// Only ever human output. Under `--json` the usage error is a response document
    /// like any other failure, so this is not the path it takes: generated help on a
    /// stream promised to carry one JSON document is exactly the contamination the
    /// contract forbids.
    pub(crate) fn usage_text(&self, text: &str) -> io::Result<()> {
        let mut stream = io::stderr().lock();
        stream.write_all(text.as_bytes())?;
        stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    // Decision sources: Define the Rust CLI capability contract.

    #[test]
    fn color_is_offered_only_to_an_interactive_stream() {
        assert!(
            color_permitted(true, None),
            "an interactive stream may have it"
        );
        assert!(
            !color_permitted(false, None),
            "a redirected stream never does"
        );
    }

    #[test]
    fn a_no_color_request_withdraws_it_even_from_a_terminal() {
        assert!(!color_permitted(true, Some(OsStr::new("1"))));
        assert!(!color_permitted(true, Some(OsStr::new("anything at all"))));
        assert!(!color_permitted(false, Some(OsStr::new("1"))));
    }

    /// The variable existing with no value is not a request. Treating it as one turns
    /// an empty inherited assignment into a silent, invisible preference.
    #[test]
    fn an_empty_no_color_value_is_not_a_request() {
        assert!(color_permitted(true, Some(&OsString::new())));
    }
}
