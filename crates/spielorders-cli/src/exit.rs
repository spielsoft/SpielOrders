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

// Ctrl-C is absent from the enum above, and that is a choice this code makes rather than
// a rule it is held to. Two things are decided: an interrupted process exits by the
// platform's convention, `130` on POSIX, and nothing is left half-written — the latter
// held by the core staging its canonical writes and publishing them atomically, not by
// anything here.
//
// No command this executable runs today needs more than that, so it installs no signal
// handler and an interrupt keeps the platform's default disposition. A command that has
// work to unwind may install one: catching the signal, letting the work settle, and then
// re-raising or exiting `130` honors both decided constraints. What is fixed is the
// status a shell sees and the state left on disk, never the mechanism.

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
