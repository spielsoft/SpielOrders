//! Command-line adapter for SpielOrders.
//!
//! The `spielorders` executable is an adapter over the shared typed Rust core: it
//! parses, targets, invokes a core capability, presents, and exits. It owns no Package
//! rule — it does not validate domain facts, compute totals, resolve Vendors, decide
//! repair actions, or write managed metadata.
//!
//! Five things are established here for every later command to consume, each in one
//! module so that each can be inventoried as a set rather than read as a diff. The first
//! four are consumed by all of them; the fifth by every command that displays money:
//!
//! - [`effects`] — every production effect the executable implements;
//! - [`output`] — every path by which a byte reaches standard output or standard error,
//!   and the four mutually exclusive output modes;
//! - [`exit`] — every exit from the process and the status it carried;
//! - [`contract`] and [`response`] — the response envelope and the registry that keeps
//!   `contract` identities distinct; and
//! - [`money`] — the one rendering of a monetary value for a human, so that a Package's
//!   money reads the same way in every command that shows it.
//!
//! **The executable is this crate's product; the library exists for its own test
//! target.** A module is reachable from outside only because that target names something
//! in it: the platform effects it drives a real Package with, the value kinds it
//! enumerates against the ones a command line accepts and the global exclusions it sweeps
//! every command with, and the response contract identities it locates fixtures by.
//! Everything else is crate-private, so an adapter
//! boundary is a wall rather than a convention, and `unreachable_pub` keeps a module from
//! drifting back out.
//!
//! Decision sources: Define the Rust CLI capability contract.

#![deny(unreachable_pub)]
// Claims about the shipped executable, held by name rather than by reading this crate's
// own source: no way out of the process but a status returned to `main`, no panic on
// anything the process was given, and no byte reaching a stream except through
// [`output`]. The spellings clippy has no lint for are in `clippy.toml`. Lifted under
// `cfg(test)`, where a unit test asserting what its own fixture set up is exactly the
// thing these forbid in shipping code.
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

pub mod cli;
pub mod contract;
pub mod effects;

pub(crate) mod commands;
pub(crate) mod exit;
pub(crate) mod money;
pub(crate) mod output;
pub(crate) mod response;

use cli::{
    ContactCommand, Family, IndexCommand, ItemCommand, ListCommand, OrderCommand, PackageCommand,
    VendorCommand,
};
use exit::Exit;
use output::{Mode, Output, color_permitted};
use response::{Context, Failure, FailureCode, FailureScope, Response};

/// Runs one command and reports the status the process should carry.
///
/// The status travels back as a value. Nothing in this crate calls
/// `std::process::exit`, which would abandon buffered output and skip every
/// destructor.
pub fn run() -> std::process::ExitCode {
    command().code()
}

/// The command itself, whose answer is one of the three [`Exit`] statuses.
///
/// Separate from [`run`] so that the enum stays crate-private: `main` needs an
/// `ExitCode` and nothing outside this crate needs the closed set behind it.
fn command() -> Exit {
    let arguments = effects::arguments();
    let no_color = effects::no_color_request();
    let stdout_color = color_permitted(effects::stdout_is_terminal(), no_color.as_deref());
    let stderr_color = color_permitted(effects::stderr_is_terminal(), no_color.as_deref());

    match cli::parse(&arguments) {
        Ok(parsed) => {
            let output = Output::new(parsed.options.mode(), stdout_color, stderr_color);
            match parsed.family {
                Family::Init {
                    assignments,
                    execution,
                } => commands::init::init(
                    &parsed.options.root,
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                // The Name shorthand is expanded into the assignment it stands for, so
                // both `add-vendor` forms reach the core as one assignment set.
                Family::AddVendor { subject, execution } => commands::vendor::add(
                    &parsed.options.root,
                    &cli::subject_assignments(&subject, cli::VENDOR_NAME_FIELD),
                    execution.dry_run,
                    &output,
                ),
                // The Title shorthand is expanded into the assignment it stands for, so
                // both `add-order` forms reach the core as one assignment set.
                Family::AddOrder { subject, execution } => commands::order::add(
                    &parsed.options.root,
                    &cli::subject_assignments(&subject, cli::ORDER_TITLE_FIELD),
                    execution.dry_run,
                    &output,
                ),
                // The Order's own facts and each Order Item's are told apart here, because
                // grouping is a fact about the command line — see
                // `cli::adoption_assignments`. Every assignment is still the core's to
                // adjudicate.
                Family::AdoptOrder {
                    directory,
                    assignments,
                    execution,
                } => {
                    let (facts, items) = cli::adoption_assignments(&assignments);
                    commands::order::adopt(
                        &parsed.options.root,
                        &directory,
                        &facts,
                        &items,
                        execution.dry_run,
                        &output,
                    )
                }
                Family::Package {
                    command: PackageCommand::Readme,
                } => commands::package::readme(&parsed.options.root, &output),
                Family::Package {
                    command: PackageCommand::Show { all },
                } => commands::package::show(&parsed.options.root, all, &output),
                Family::Package {
                    command:
                        PackageCommand::List {
                            command: ListCommand::Orders,
                        },
                } => commands::package::list_orders(&parsed.options.root, &output),
                Family::Package {
                    command:
                        PackageCommand::List {
                            command: ListCommand::Vendors { all },
                        },
                } => commands::package::list_vendors(&parsed.options.root, all, &output),
                Family::Package {
                    command: PackageCommand::Values { kind },
                } => commands::package::values(&parsed.options.root, kind, &output),
                Family::Validate => commands::validate::package(&parsed.options.root, &output),
                // `case=` and `action=` are read here, because which of the command's two
                // moods a command line is in is a fact about the command line. Every
                // resolution beside them is still the core's to adjudicate.
                Family::Repair {
                    assignments,
                    execution,
                } => commands::repair::package(
                    &parsed.options.root,
                    &cli::repair_request(&assignments),
                    execution.dry_run,
                    &output,
                ),
                Family::Upgrade { execution } => {
                    commands::upgrade::upgrade(&parsed.options.root, execution.dry_run, &output)
                }
                Family::Index {
                    command: IndexCommand::Status,
                } => commands::index::status(&parsed.options.root, &output),
                Family::Index {
                    command: IndexCommand::Rebuild { execution },
                } => commands::index::rebuild(&parsed.options.root, execution.dry_run, &output),
                // One targeting decision for every `order` command, made here because it
                // is a fact about the command line rather than about any one command,
                // and made once so that a command added to the family is targeted the
                // same way without deciding anything for itself.
                // `--root` and `--uuid` are mutually exclusive, so exactly one
                // route is named; with `--uuid`, `--root` is necessarily its own default
                // and is therefore the current-directory Package the identity resolves
                // against.
                Family::Order { command, targeting } => {
                    let target = match &targeting.uuid {
                        Some(order_id) => commands::order::Target::Identity {
                            package_root: &parsed.options.root,
                            order_id,
                        },
                        None => commands::order::Target::Directory(&parsed.options.root),
                    };
                    match command {
                        OrderCommand::Show => commands::order::show(&target, &output),
                        OrderCommand::Edit {
                            assignments,
                            execution,
                        } => {
                            commands::order::edit(&target, &assignments, execution.dry_run, &output)
                        }
                        OrderCommand::Readme => commands::order::readme(&target, &output),
                        OrderCommand::Validate => commands::validate::order(&target, &output),
                        OrderCommand::Repair {
                            assignments,
                            execution,
                        } => commands::repair::order(
                            &target,
                            &cli::repair_request(&assignments),
                            execution.dry_run,
                            &output,
                        ),
                        OrderCommand::Align { execution } => {
                            commands::order::align(&target, execution.dry_run, &output)
                        }
                        OrderCommand::Remove { delete, execution } => {
                            commands::order::remove(&target, delete, execution.dry_run, &output)
                        }
                        OrderCommand::Item {
                            command: ItemCommand::List,
                        } => commands::order::item_list(&target, &output),
                        // The insertion point is taken out of the assignment set here, for
                        // the reason a Contact's is: where an Item goes in the collection is
                        // not one of its facts.
                        OrderCommand::Item {
                            command:
                                ItemCommand::Add {
                                    assignments,
                                    execution,
                                },
                        } => {
                            let (position, facts) = cli::insertion_assignments(&assignments);
                            commands::order::item_add(
                                &target,
                                &facts,
                                position,
                                execution.dry_run,
                                &output,
                            )
                        }
                        OrderCommand::Item {
                            command:
                                ItemCommand::Edit {
                                    position,
                                    assignments,
                                    execution,
                                },
                        } => commands::order::item_edit(
                            &target,
                            &position,
                            &assignments,
                            execution.dry_run,
                            &output,
                        ),
                        OrderCommand::Item {
                            command:
                                ItemCommand::Move {
                                    position,
                                    destination,
                                    execution,
                                },
                        } => commands::order::item_move(
                            &target,
                            &position,
                            &destination.position,
                            execution.dry_run,
                            &output,
                        ),
                        OrderCommand::Item {
                            command:
                                ItemCommand::Remove {
                                    position,
                                    execution,
                                },
                        } => commands::order::item_remove(
                            &target,
                            &position,
                            execution.dry_run,
                            &output,
                        ),
                    }
                }
                Family::Vendor {
                    command: VendorCommand::Show { name },
                } => commands::vendor::show(&parsed.options.root, &name, &output),
                Family::Vendor {
                    command:
                        VendorCommand::Edit {
                            name,
                            assignments,
                            execution,
                        },
                } => commands::vendor::edit(
                    &parsed.options.root,
                    &name,
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command: VendorCommand::Hide { name, execution },
                } => {
                    commands::vendor::hide(&parsed.options.root, &name, execution.dry_run, &output)
                }
                Family::Vendor {
                    command: VendorCommand::Unhide { name, execution },
                } => commands::vendor::unhide(
                    &parsed.options.root,
                    &name,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command: ContactCommand::List { name },
                        },
                } => commands::vendor::contact_list(&parsed.options.root, &name, &output),
                // The insertion point is taken out of the assignment set here, because
                // where a Contact goes in the collection is not one of its facts — see
                // `cli::contact_assignments`.
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command:
                                ContactCommand::Add {
                                    name,
                                    assignments,
                                    execution,
                                },
                        },
                } => {
                    let (position, facts) = cli::insertion_assignments(&assignments);
                    commands::vendor::contact_add(
                        &parsed.options.root,
                        &name,
                        &facts,
                        position,
                        execution.dry_run,
                        &output,
                    )
                }
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command:
                                ContactCommand::Edit {
                                    name,
                                    position,
                                    assignments,
                                    execution,
                                },
                        },
                } => commands::vendor::contact_edit(
                    &parsed.options.root,
                    &name,
                    &position,
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command:
                                ContactCommand::Move {
                                    name,
                                    position,
                                    destination,
                                    execution,
                                },
                        },
                } => commands::vendor::contact_move(
                    &parsed.options.root,
                    &name,
                    &position,
                    &destination.position,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command:
                                ContactCommand::Remove {
                                    name,
                                    position,
                                    execution,
                                },
                        },
                } => commands::vendor::contact_remove(
                    &parsed.options.root,
                    &name,
                    &position,
                    execution.dry_run,
                    &output,
                ),
            }
        }
        Err(error) => {
            // The parse produced no options, so the reporting mode is answered from
            // the raw arguments instead. `--help` and `--version` are answers rather
            // than failures and are always the human reference text.
            let mode = if !cli::is_reference_request(&error) && cli::json_requested(&arguments) {
                Mode::Json
            } else {
                Mode::Human
            };
            let output = Output::new(mode, stdout_color, stderr_color);
            if cli::is_reference_request(&error) {
                let text = rendered(&error, output.stdout_color());
                let _ = output.reference(&text);
                return Exit::Success;
            }
            if mode.is_json() {
                let _ = output.document(&Response::<()>::failed(
                    // A usage error never reached a command, so it carries no command's
                    // contract identity — it carries the one identity every usage error
                    // has. See `contract::ResponseContract::Usage`.
                    contract::ResponseContract::Usage,
                    Context::untargeted(),
                    Failure::new(
                        FailureCode::UsageInvalid,
                        FailureScope::Usage,
                        cli::usage_message(&error),
                    ),
                    // Nothing about the subject was attempted, so nothing was observed.
                    Vec::new(),
                ));
            } else {
                let _ = output.usage_text(&rendered(&error, output.stderr_color()));
            }
            Exit::Usage
        }
    }
}

/// The parser's own rendered text, styled only where this crate's color policy allows.
fn rendered(error: &clap::Error, color: bool) -> String {
    if color {
        error.render().ansi().to_string()
    } else {
        error.render().to_string()
    }
}
