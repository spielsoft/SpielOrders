//! Command-line adapter for SpielOrders.
//!
//! The `spielorders` executable is an adapter over the shared typed Rust core: it
//! parses, targets, invokes a core capability, presents, and exits. It owns no Package
//! rule — it does not validate domain facts, compute totals, resolve Vendors, decide
//! repair actions, or write managed metadata.
//!
//! What every later command consumes is established here, a module apiece so that each can
//! be inventoried as a set rather than read as a diff. The effects, the streams, the exits
//! and the envelope are consumed by all of them; the two renderers by the commands that
//! show a monetary value or a moment in time:
//!
//! - [`effects`] — every production effect the executable implements;
//! - [`output`] — every path by which a byte reaches standard output or standard error,
//!   and the four mutually exclusive output modes;
//! - [`exit`] — every exit from the process and the status it carried;
//! - [`contract`] and [`response`] — the response envelope and the registry that keeps
//!   `contract` identities distinct;
//! - [`money`] — the one call site a command reaches the core's monetary spelling
//!   through, so that a Package's money reads the same way in every command that shows
//!   it and reads the way a search matches it; and
//! - [`moment`] — the one place a moment in time is rendered for a human, because a
//!   filesystem stamp arrives as a count of seconds and a reader cannot use one.
//!
//! **The executable is this crate's product; the library exists for its own test
//! target.** A module is reachable from outside only because that target names something
//! in it: the platform effects it drives a real Package with, the value kinds it
//! enumerates against the ones a command line accepts and the option exclusions it sweeps
//! every command with, and the response contract identities and pure projection it
//! compares fixtures against.
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
pub(crate) mod moment;
pub(crate) mod money;
pub(crate) mod output;
pub(crate) mod response;

use cli::{
    ContactCommand, Family, IndexCommand, ItemCommand, ListCommand, OrderCommand, PackageCommand,
    SearchCommand, VendorCommand,
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
            // Asked of the command that declares the option, before the one `Output` this
            // process reports through is built. See `cli::Cli::mode`.
            let output = Output::new(parsed.mode(), stdout_color, stderr_color);
            match parsed.family {
                Family::Init {
                    assignments,
                    target,
                    execution,
                    ..
                } => commands::init::init(&target.root, &assignments, execution.dry_run, &output),
                // The Name shorthand is expanded into the assignment it stands for, so
                // both `add-vendor` forms reach the core as one assignment set.
                Family::AddVendor {
                    subject,
                    target,
                    execution,
                    ..
                } => commands::vendor::add(
                    &target.root,
                    &cli::subject_assignments(&subject, cli::VENDOR_NAME_FIELD),
                    execution.dry_run,
                    &output,
                ),
                // The Title shorthand is expanded into the assignment it stands for, so
                // both `add-order` forms reach the core as one assignment set.
                Family::AddOrder {
                    subject,
                    target,
                    execution,
                    ..
                } => commands::order::add(
                    &target.root,
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
                    target,
                    execution,
                    ..
                } => {
                    let (facts, items) = cli::adoption_assignments(&assignments);
                    commands::order::adopt(
                        &target.root,
                        &directory,
                        &facts,
                        &items,
                        execution.dry_run,
                        &output,
                    )
                }
                Family::Package {
                    command: PackageCommand::Readme { target, .. },
                } => commands::package::readme(&target.root, &output),
                Family::Package {
                    command: PackageCommand::Show { all, target, .. },
                } => commands::package::show(&target.root, all, &output),
                Family::Package {
                    command:
                        PackageCommand::List {
                            command: ListCommand::Orders { target, .. },
                        },
                } => commands::package::list_orders(&target.root, &output),
                Family::Package {
                    command:
                        PackageCommand::List {
                            command: ListCommand::Vendors { all, target, .. },
                        },
                } => commands::package::list_vendors(&target.root, all, &output),
                Family::Package {
                    command: PackageCommand::Values { kind, target, .. },
                } => commands::package::values(&target.root, kind, &output),
                Family::Validate { target, .. } => {
                    commands::validate::package(&target.root, &output)
                }
                // `case=` and `action=` are read here, because which of the command's two
                // moods a command line is in is a fact about the command line. Every
                // resolution beside them is still the core's to adjudicate.
                Family::Repair {
                    assignments,
                    target,
                    execution,
                    ..
                } => commands::repair::package(
                    &target.root,
                    &cli::repair_request(&assignments),
                    execution.dry_run,
                    &output,
                ),
                Family::Upgrade {
                    target, execution, ..
                } => commands::upgrade::upgrade(&target.root, execution.dry_run, &output),
                Family::Index {
                    command: IndexCommand::Status { target, .. },
                } => commands::index::status(&target.root, &output),
                Family::Index {
                    command:
                        IndexCommand::Rebuild {
                            target, execution, ..
                        },
                } => commands::index::rebuild(&target.root, execution.dry_run, &output),
                Family::Order {
                    command: OrderCommand::Show { target, .. },
                } => commands::order::show(&order_route(&target), &output),
                Family::Order {
                    command:
                        OrderCommand::Edit {
                            assignments,
                            target,
                            execution,
                            ..
                        },
                } => commands::order::edit(
                    &order_route(&target),
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                Family::Order {
                    command: OrderCommand::Readme { target, .. },
                } => commands::order::readme(&order_route(&target), &output),
                Family::Order {
                    command: OrderCommand::Validate { target, .. },
                } => commands::validate::order(&order_route(&target), &output),
                Family::Order {
                    command:
                        OrderCommand::Repair {
                            assignments,
                            target,
                            execution,
                            ..
                        },
                } => commands::repair::order(
                    &order_route(&target),
                    &cli::repair_request(&assignments),
                    execution.dry_run,
                    &output,
                ),
                Family::Order {
                    command:
                        OrderCommand::Align {
                            target, execution, ..
                        },
                } => commands::order::align(&order_route(&target), execution.dry_run, &output),
                Family::Order {
                    command:
                        OrderCommand::Remove {
                            delete,
                            target,
                            execution,
                            ..
                        },
                } => commands::order::remove(
                    &order_route(&target),
                    delete,
                    execution.dry_run,
                    &output,
                ),
                Family::Order {
                    command:
                        OrderCommand::Item {
                            command: ItemCommand::List { target, .. },
                        },
                } => commands::order::item_list(&order_route(&target), &output),
                // The insertion point is taken out of the assignment set here, for the
                // reason a Contact's is: where an Item goes in the collection is not one
                // of its facts.
                Family::Order {
                    command:
                        OrderCommand::Item {
                            command:
                                ItemCommand::Add {
                                    assignments,
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => {
                    let (position, facts) = cli::insertion_assignments(&assignments);
                    commands::order::item_add(
                        &order_route(&target),
                        &facts,
                        position,
                        execution.dry_run,
                        &output,
                    )
                }
                Family::Order {
                    command:
                        OrderCommand::Item {
                            command:
                                ItemCommand::Edit {
                                    position,
                                    assignments,
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::order::item_edit(
                    &order_route(&target),
                    &position,
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                Family::Order {
                    command:
                        OrderCommand::Item {
                            command:
                                ItemCommand::Move {
                                    position,
                                    destination,
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::order::item_move(
                    &order_route(&target),
                    &position,
                    &destination.position,
                    execution.dry_run,
                    &output,
                ),
                Family::Order {
                    command:
                        OrderCommand::Item {
                            command:
                                ItemCommand::Remove {
                                    position,
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::order::item_remove(
                    &order_route(&target),
                    &position,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command: VendorCommand::Show { name, target, .. },
                } => commands::vendor::show(&target.root, &name, &output),
                Family::Vendor {
                    command:
                        VendorCommand::Edit {
                            name,
                            assignments,
                            target,
                            execution,
                            ..
                        },
                } => commands::vendor::edit(
                    &target.root,
                    &name,
                    &assignments,
                    execution.dry_run,
                    &output,
                ),
                Family::Vendor {
                    command:
                        VendorCommand::Hide {
                            name,
                            target,
                            execution,
                            ..
                        },
                } => commands::vendor::hide(&target.root, &name, execution.dry_run, &output),
                Family::Vendor {
                    command:
                        VendorCommand::Unhide {
                            name,
                            target,
                            execution,
                            ..
                        },
                } => commands::vendor::unhide(&target.root, &name, execution.dry_run, &output),
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command: ContactCommand::List { name, target, .. },
                        },
                } => commands::vendor::contact_list(&target.root, &name, &output),
                // The insertion point is taken out of the assignment set here, because
                // where a Contact goes in the collection is not one of its facts — see
                // `cli::insertion_assignments`.
                Family::Vendor {
                    command:
                        VendorCommand::Contact {
                            command:
                                ContactCommand::Add {
                                    name,
                                    assignments,
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => {
                    let (position, facts) = cli::insertion_assignments(&assignments);
                    commands::vendor::contact_add(
                        &target.root,
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
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::vendor::contact_edit(
                    &target.root,
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
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::vendor::contact_move(
                    &target.root,
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
                                    target,
                                    execution,
                                    ..
                                },
                        },
                } => commands::vendor::contact_remove(
                    &target.root,
                    &name,
                    &position,
                    execution.dry_run,
                    &output,
                ),
                // The words travel as the shell handed them over. How they become terms is
                // the core's, so that one quoted argument and the same words split reach one
                // set of terms; see `commands::search`.
                Family::Search {
                    command:
                        SearchCommand::Orders {
                            keywords,
                            matching,
                            target,
                            ..
                        },
                } => commands::search::orders(
                    &target.root,
                    &keywords.keywords,
                    matching.case_sensitive,
                    &output,
                ),
                Family::Search {
                    command:
                        SearchCommand::Vendors {
                            all,
                            keywords,
                            matching,
                            target,
                            ..
                        },
                } => commands::search::vendors(
                    &target.root,
                    &keywords.keywords,
                    all,
                    matching.case_sensitive,
                    &output,
                ),
                // The words are joined here, because how a shell handed them over is a fact
                // about the command line: the provider governs matching, so nothing folds or
                // tokenizes them and one argument or several is the same query.
                Family::Search {
                    command: SearchCommand::SupportingMaterial { query, target, .. },
                } => commands::search::supporting_material(&target.root, &query.join(" "), &output),
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

/// The route one `order` command line names to its Order.
///
/// One decision for every `order` command, made here because it is a fact about the
/// command line rather than about any one command, and made once so that a command added
/// to the family is targeted the same way without deciding anything for itself.
///
/// `--root` and `--uuid` share a group that admits one member, so exactly one route is
/// ever named; under `--uuid`, `--root` is necessarily its own default and is therefore
/// the current-directory Package the identity resolves against.
fn order_route(target: &cli::OrderTarget) -> commands::order::Target<'_> {
    match &target.uuid {
        Some(order_id) => commands::order::Target::Identity {
            package_root: &target.root,
            order_id,
        },
        None => commands::order::Target::Directory(&target.root),
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
