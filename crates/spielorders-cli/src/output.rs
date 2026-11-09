//! Every path by which a byte reaches standard output or standard error.
//!
//! The writing macros are denied at both crate roots, so these methods are the only
//! routes out, and `tests/output_modes_and_streams.rs` asserts from real processes which
//! stream carries what in each mode.
//!
//! | | stdout | stderr |
//! | --- | --- | --- |
//! | human | the result | warnings and errors |
//! | `--verbose` | the result and its detail | warnings and errors |
//! | `--json` | exactly one JSON document | nothing |
//! | `--quiet` | nothing | errors only |
//!
//! Generated `--help` and `--version` are reference text and go to stdout at exit `0` in
//! every mode. A usage error goes to whichever audience the command line named: a command
//! line carrying `--json` anywhere gets the one document on stdout, and every other one
//! gets the parser's help on stderr.
//!
//! Decision sources: Define the Rust CLI capability contract.

use std::{
    ffi::OsStr,
    io::{self, Write},
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

/// Whether a stream may carry color: only an interactive one, and never under a non-empty
/// `NO_COLOR`.
pub(crate) fn color_permitted(stream_is_terminal: bool, no_color: Option<&OsStr>) -> bool {
    stream_is_terminal && !no_color.is_some_and(|request| !request.is_empty())
}

const RESET: &str = "\u{1b}[0m";
const RED: &str = "\u{1b}[31m";
const YELLOW: &str = "\u{1b}[33m";

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

    pub(crate) fn stdout_color(&self) -> bool {
        self.stdout_color
    }

    pub(crate) fn stderr_color(&self) -> bool {
        self.stderr_color
    }

    /// One line of a human result. Nothing under `--quiet` or `--json`.
    pub(crate) fn line(&self, text: &str) -> io::Result<()> {
        if matches!(self.mode, Mode::Quiet | Mode::Json) {
            return Ok(());
        }
        let mut stream = io::stdout().lock();
        writeln!(stream, "{text}")?;
        stream.flush()
    }

    /// A warning. Nothing under `--quiet` or `--json`.
    pub(crate) fn warning(&self, message: &str) -> io::Result<()> {
        if matches!(self.mode, Mode::Quiet | Mode::Json) {
            return Ok(());
        }
        self.labelled(YELLOW, "warning", message)
    }

    /// An error, which `--quiet` still reports. Nothing under `--json`, whose document
    /// carries it.
    pub(crate) fn error(&self, message: &str) -> io::Result<()> {
        if self.mode.is_json() {
            return Ok(());
        }
        self.labelled(RED, "error", message)
    }

    /// A line of detail beneath an error, which `--quiet` still reports.
    pub(crate) fn error_detail(&self, text: &str) -> io::Result<()> {
        if self.mode.is_json() {
            return Ok(());
        }
        let mut stream = io::stderr().lock();
        writeln!(stream, "{text}")?;
        stream.flush()
    }

    fn labelled(&self, color: &str, label: &str, message: &str) -> io::Result<()> {
        let mut stream = io::stderr().lock();
        if self.stderr_color {
            writeln!(stream, "{color}{label}:{RESET} {message}")?;
        } else {
            writeln!(stream, "{label}: {message}")?;
        }
        stream.flush()
    }

    /// The one JSON document.
    pub(crate) fn document(&self, bytes: &[u8]) -> io::Result<()> {
        let mut stream = io::stdout().lock();
        stream.write_all(bytes)?;
        stream.flush()
    }

    /// Generated reference text: `--help` and `--version`.
    pub(crate) fn reference(&self, text: &str) -> io::Result<()> {
        let mut stream = io::stdout().lock();
        stream.write_all(text.as_bytes())?;
        stream.flush()
    }

    /// The parser's generated text for a usage error.
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

    #[test]
    fn color_is_offered_only_to_an_interactive_stream() {
        assert!(color_permitted(true, None));
        assert!(!color_permitted(false, None));
    }

    #[test]
    fn a_no_color_request_withdraws_it_even_from_a_terminal() {
        assert!(!color_permitted(true, Some(OsStr::new("1"))));
        assert!(!color_permitted(false, Some(OsStr::new("1"))));
    }

    #[test]
    fn an_empty_no_color_value_is_not_a_request() {
        assert!(color_permitted(true, Some(&OsString::new())));
    }
}
