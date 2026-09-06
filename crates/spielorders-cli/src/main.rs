//! The `spielorders` executable.
//!
//! A binary is its own crate root, so it carries the same fence `spielorders_cli` does
//! rather than inheriting it. The whole of this target is one call and one status, and
//! keeping the fence here is what makes that a rule instead of an observation.

#![cfg_attr(
    not(test),
    deny(
        clippy::dbg_macro,
        clippy::exit,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stderr,
        clippy::print_stdout,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used,
    )
)]

fn main() -> std::process::ExitCode {
    spielorders_cli::run()
}
