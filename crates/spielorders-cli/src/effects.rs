//! CLI process inputs.
//!
//! Decision sources: Define the Rust CLI capability contract; Effect and adapter boundaries.

use std::{
    ffi::OsString,
    io::{self, IsTerminal},
};

/// The process arguments, including the program name clap expects first.
#[expect(
    clippy::disallowed_methods,
    reason = "the arguments are one of the two things this executable asks its \
              environment for, and this is the boundary that asks"
)]
pub(crate) fn arguments() -> Vec<OsString> {
    std::env::args_os().collect()
}

// The process working directory is deliberately not among these effects. A relative
// `--root` reaches the library exactly as the caller wrote it, and the library resolves
// it against the process working directory. Reading it here to join it on first would
// resolve the target twice, in two places, by two rules.

/// The `NO_COLOR` request exactly as the environment carries it.
///
/// The raw value rather than a decision: whether it disables color is a policy, and
/// the policy lives in `crate::output` where it can be exercised without an
/// environment.
#[expect(
    clippy::disallowed_methods,
    reason = "the color request is the other, and no third reason has been argued for \
              here — there is no pager, no `COLUMNS`, and no configuration file of the \
              CLI's own"
)]
pub(crate) fn no_color_request() -> Option<OsString> {
    std::env::var_os("NO_COLOR")
}

/// Whether this process's standard output is an interactive terminal.
pub(crate) fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}

/// Whether this process's standard error is an interactive terminal.
pub(crate) fn stderr_is_terminal() -> bool {
    io::stderr().is_terminal()
}
