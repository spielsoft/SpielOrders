//! Every exit this process can take, and the status it carries.
//!
//! The set is deliberately coarse and closed. A command chooses one of these three;
//! it does not invent a fourth, and it does not call `std::process::exit` — that
//! abandons buffered output and skips every destructor, so the status travels back to
//! `main` as a value and the process ends by returning from `main`.
//!
//! **Both halves are held rather than asserted about.** `std::process::exit` and the
//! panicking macros are denied at both crate roots; `process::abort` and the `libc`
//! spellings this crate's dependency on `libc` makes reachable have no lint named after
//! them and are named in `clippy.toml` instead. None of them compiles. A panic is the
//! undecided fourth status `101`, and it was the shape of a shipped defect: a
//! caller-supplied path the filesystem declined to resolve aborted the process instead
//! of being reported. An `expect` is how that gets in without the word `panic` appearing
//! anywhere, which is why the denial names it too.
//!
//! Decision sources: Define the Rust CLI capability contract.

use std::process::ExitCode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Exit {
    /// The command did what was asked, including an idempotent no-op and an empty
    /// result.
    Success,
    /// An operational or domain failure: the subject could not be read, a mutation was
    /// rejected, a validation found the Package invalid, or the metadata state is one
    /// this build does not support.
    Failure,
    /// The command line itself was wrong. Nothing about the subject was attempted.
    Usage,
}

impl Exit {
    pub(crate) fn status(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::Usage => 2,
        }
    }

    pub(crate) fn code(self) -> ExitCode {
        ExitCode::from(self.status())
    }
}

// The response envelope's `ok` and the process status are two spellings of one fact and
// must never disagree. What keeps them together is that `response::Response` has exactly
// two constructors — `answered` can only produce `ok: true` and `failed` only
// `ok: false` — and that every process-level test asserts the document and the status
// together. There is deliberately no `Exit::is_success` for the envelope to consult: an
// accessor no code path reads would look like the mechanism while the constructors were
// actually doing the work, and `tests/response_contract.rs` is where the pair is checked.

// Ctrl-C is deliberately absent from the enum above. This executable installs no
// signal handler, so an interrupt keeps the platform's default disposition: the
// process is terminated by SIGINT and a POSIX shell reports `130`. That is the
// platform signal convention the contract names, and honoring it means not
// interfering with it — a handler that caught the signal and returned a status of its
// own would replace `130` with an ordinary exit and tell `wait` the process was never
// signalled. Nothing here is left half-written by an interrupt because a command's
// canonical writes are staged and atomically published by the core.
//
// No process can show that absence — an interrupt arrives too late to race a command
// that finishes in milliseconds — so the platform calls that would install a handler are
// named in `clippy.toml` instead. What that leaves is a signal-handling library, which
// this crate does not depend on and which would arrive as a dependency rather than as a
// line nobody looked at.

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Rust CLI capability contract.

    /// The defect: a status renumbered so that a shell can no longer tell a wrong
    /// command line from a failed one. `tests/output_modes_and_streams.rs` asserts the
    /// same three from real processes, which is where the contract claim belongs; this
    /// pins the mapping itself so a renumbering fails here rather than in a
    /// command's test.
    #[test]
    fn the_three_statuses_are_the_decided_ones() {
        assert_eq!(Exit::Success.status(), 0);
        assert_eq!(Exit::Failure.status(), 1);
        assert_eq!(Exit::Usage.status(), 2);
    }
}
