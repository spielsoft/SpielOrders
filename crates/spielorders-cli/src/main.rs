//! The `spielorders` executable, the command-line adapter for SpielOrders.
//!
//! It parses, targets, invokes a core capability, presents what the core answered, and
//! exits. It owns no Package rule. The executable is this crate's whole product: its
//! integration suite drives the built executable and imports nothing from it.
//!
//! It reaches the SpielOrders library only through the library's interface, by the same
//! exported functions every other caller uses, and with no `unsafe`.
//!
//! Decision sources: Define the Rust CLI capability contract; Effect and adapter
//! boundaries, Interface rules.

#![forbid(unsafe_code)]
#![deny(unreachable_pub)]
// No way out of the process but a status returned to `main`, no panic on anything the
// process was given, and no byte reaching a stream except through `output`. Lifted under
// `cfg(test)`.
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

mod cli;
mod commands;
mod contract;
mod effects;
mod exit;
mod human;
mod library;
mod output;
mod platform;
mod resources;
mod response;

use exit::Exit;
use output::{Mode, Output, color_permitted};
use response::{Answer, Human, Shown, present};

/// Runs one command and reports the status the process should carry.
fn main() -> std::process::ExitCode {
    command().code()
}

fn command() -> Exit {
    let arguments = effects::arguments();
    let no_color = effects::no_color_request();
    let stdout_color = color_permitted(effects::stdout_is_terminal(), no_color.as_deref());
    let stderr_color = color_permitted(effects::stderr_is_terminal(), no_color.as_deref());

    match cli::parse(&arguments) {
        Ok(parsed) => {
            let output = Output::new(parsed.mode(), stdout_color, stderr_color);
            present(&commands::run(&parsed.family), &output)
        }
        Err(error) => {
            // `--help` and `--version` are answers rather than failures, and always the
            // human reference text.
            if cli::is_reference_request(&error) {
                let output = Output::new(Mode::Human, stdout_color, stderr_color);
                let _ = output.reference(&rendered(&error, output.stdout_color()));
                return Exit::Success;
            }
            // The parse produced no options, so the mode is read off the raw arguments.
            if cli::json_requested(&arguments) {
                let output = Output::new(Mode::Json, stdout_color, stderr_color);
                // A usage error reached no command, so it carries the one identity every
                // usage error has and no command's.
                let shown = Shown {
                    contract: contract::ResponseContract::Usage,
                    root: None,
                    answer: Answer::Failed {
                        error: serde_json::json!({
                            "code": "usage_invalid",
                            "detail": cli::usage_message(&error),
                        }),
                        findings: Vec::new(),
                    },
                    human: Human::Whole,
                };
                let _ = present(&shown, &output);
            } else {
                let output = Output::new(Mode::Human, stdout_color, stderr_color);
                let _ = output.usage_text(&rendered(&error, output.stderr_color()));
            }
            Exit::Usage
        }
    }
}

/// The parser's own rendered text, styled only where the color policy allows.
fn rendered(error: &clap::Error, color: bool) -> String {
    if color {
        error.render().ansi().to_string()
    } else {
        error.render().to_string()
    }
}
