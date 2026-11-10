//! Every command: target, call the library's interface, and build the answer it shows.
//!
//! A read answers with what the library read and a mutation with what its plan did, or
//! will do under `--dry-run`; either refuses with the library's own failure, and the
//! library's findings travel beside either.
//!
//! Decision sources: Define the Rust CLI capability contract.

use crate::{
    cli::{
        self, ContactCommand, Execution, Family, FieldAssignment, IndexCommand, ItemCommand,
        ListCommand, OrderCommand, OrderTarget, PackageCommand, RepairRequest, SearchCommand,
        ValueKind, VendorCommand,
    },
    contract::ResponseContract as Contract,
    library::{self, Calls, c_path, c_text, constant, resolution_name},
    platform,
    response::{Answer, Human, Shown},
};
use serde_json::{Map, Value, json};
use spielorders::*;
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    time::Duration,
};

/// How long a Supporting Material provider has to settle one query. A choice of this
/// executable: long enough for an ordinary index, short enough not to hold a terminal.
const PROVIDER_BOUND: Duration = Duration::from_secs(10);

type Read = Result<Value, Answer>;

/// Runs the command `family` names and answers with what it will show.
pub(crate) fn run(family: &Family) -> Shown<'_> {
    let (contract, root, answer, human) = match family {
        Family::Init {
            assignments,
            target,
            execution,
            ..
        } => {
            let assigned = |key: &str| {
                assignments
                    .iter()
                    .find(|assignment| assignment.key == key)
                    .map(|assignment| c_text(assignment.value.as_str()))
            };
            let (orders, removed) = (assigned(cli::ORDERS_PATH), assigned(cli::REMOVED_PATH));
            fn optional(path: &Option<CString>) -> OptionalPathBytes<'_> {
                path.as_deref()
                    .map_or(OptionalPathBytes::null(), OptionalPathBytes::new)
            }
            let root = c_path(&target.root);
            let answer = answered(|calls| {
                let plan = calls.get(|plan, outcome| {
                    spielorders_plan_initialize(
                        PathBytes::new(&root),
                        optional(&orders),
                        optional(&removed),
                        0,
                        plan,
                        outcome,
                    )
                })?;
                mutate(calls, &plan, execution)
            });
            (Contract::Init, &target.root, answer, Human::Whole)
        }
        Family::AddVendor {
            subject,
            target,
            execution,
            ..
        } => {
            let assignments = cli::subject_assignments(subject, cli::VENDOR_NAME_FIELD);
            let answer = answered(|calls| {
                let package = open(calls, &target.root)?;
                let mut draft = calls.get(|draft, outcome| {
                    spielorders_package_new_vendor_draft(&package, draft, outcome)
                })?;
                without_default(&mut draft, SPIELORDERS_VENDOR_FIELD_NAME);
                save(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_VENDOR_FIELD,
                    &assignments,
                    execution,
                )
            });
            (Contract::AddVendor, &target.root, answer, Human::Whole)
        }
        Family::AddOrder {
            subject,
            target,
            execution,
            ..
        } => {
            let assignments = cli::subject_assignments(subject, cli::ORDER_TITLE_FIELD);
            let answer = answered(|calls| {
                let package = open(calls, &target.root)?;
                let mut draft = calls.get(|draft, outcome| {
                    spielorders_package_new_order_draft(&package, draft, outcome)
                })?;
                without_default(&mut draft, SPIELORDERS_ORDER_FIELD_TITLE);
                save(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_ORDER_FIELD,
                    &assignments,
                    execution,
                )
            });
            (Contract::AddOrder, &target.root, answer, Human::Whole)
        }
        Family::AdoptOrder {
            directory,
            assignments,
            target,
            execution,
            ..
        } => {
            let (facts, items) = cli::adoption_assignments(assignments);
            let answer = answered(|calls| {
                let package = open(calls, &target.root)?;
                let directory = c_path(directory);
                let mut draft = calls.get(|draft, outcome| {
                    spielorders_package_adoption_draft(
                        &package,
                        PathBytes::new(&directory),
                        0,
                        draft,
                        outcome,
                    )
                })?;
                for item in &items {
                    let row = spielorders_draft_row_count(&draft);
                    calls.act(|outcome| spielorders_draft_row_insert(&mut draft, row, outcome))?;
                    assign(
                        calls,
                        &mut draft,
                        SPIELORDERS_FAMILY_ITEM_FIELD,
                        Some(row),
                        item,
                    )?;
                }
                save(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_ORDER_FIELD,
                    &facts,
                    execution,
                )
            });
            (Contract::AdoptOrder, &target.root, answer, Human::Whole)
        }
        Family::Package { command } => package(command),
        Family::Validate { target, .. } => (
            Contract::Validate,
            &target.root,
            answered(|calls| {
                let package = open_quietly(&target.root)?;
                let diagnosis = calls.get(|diagnosis, outcome| {
                    spielorders_package_validate(&package, 0, diagnosis, outcome)
                })?;
                verdict(calls, &diagnosis, SPIELORDERS_FAILURE_PACKAGE_INVALID)
            }),
            Human::Verdict("valid"),
        ),
        Family::Repair {
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::Repair,
            &target.root,
            answered(|calls| {
                let diagnose = |calls: &mut Calls| {
                    let package = open_quietly(&target.root)?;
                    calls.get(|diagnosis, outcome| {
                        spielorders_package_diagnose(&package, 0, diagnosis, outcome)
                    })
                };
                let diagnosis = diagnose(calls)?;
                repair(calls, &diagnosis, assignments, execution, diagnose)
            }),
            Human::Repair("spielorders repair"),
        ),
        Family::Index { command } => match command {
            IndexCommand::Status { target, .. } => (
                Contract::IndexStatus,
                &target.root,
                answered(|calls| {
                    let package = open(calls, &target.root)?;
                    // A Package whose contents the library will not read has no index
                    // status to report, which the library says by refusing a read of them.
                    Calls::default().get(|list, outcome| {
                        spielorders_package_values(
                            &package,
                            SPIELORDERS_ORDER_FIELD_PROJECT,
                            list,
                            outcome,
                        )
                    })?;
                    calls.found(&package);
                    Ok(index(&package))
                }),
                Human::Whole,
            ),
            IndexCommand::Rebuild {
                target, execution, ..
            } => (
                Contract::IndexRebuild,
                &target.root,
                answered(|calls| {
                    let package = open(calls, &target.root)?;
                    let plan = calls.get(|plan, outcome| {
                        spielorders_package_plan_rebuild_index(&package, 0, plan, outcome)
                    })?;
                    let mut data = mutate(calls, &plan, execution)?;
                    if !execution.dry_run {
                        // The refreshed result of a rebuild is the index as a new open finds it.
                        data["index"] = index(&*open_quietly(&target.root)?);
                    }
                    Ok(data)
                }),
                Human::Whole,
            ),
        },
        Family::Order { command } => order(command),
        Family::Vendor { command } => vendor(command),
        Family::Search { command } => search(command),
    };
    Shown {
        contract,
        root: Some(root.as_path()),
        answer,
        human,
    }
}

type Command<'a> = (Contract, &'a PathBuf, Answer, Human);

/// What one Item or Contact command does to the draft of the record that holds its rows.
type RowEdit<'a> = Box<dyn Fn(&mut Calls, &mut SpielordersDraft) -> Result<(), Answer> + 'a>;

fn package(command: &PackageCommand) -> Command<'_> {
    match command {
        PackageCommand::Readme { target, .. } => (
            Contract::PackageReadme,
            &target.root,
            answered(|calls| {
                let package = open(calls, &target.root)?;
                let readme = calls.get(|readme, outcome| {
                    spielorders_package_read_readme(&package, 0, readme, outcome)
                })?;
                Ok(library::readme(&readme))
            }),
            Human::Bare(&["path"]),
        ),
        PackageCommand::Show { all, target, .. } => (
            Contract::PackageShow,
            &target.root,
            answered(|calls| show(calls, &target.root)),
            if *all { Human::Whole } else { Human::Offered },
        ),
        PackageCommand::List {
            command: ListCommand::Orders { target, .. },
        } => (
            Contract::PackageListOrders,
            &target.root,
            answered(|calls| orders(calls, &target.root, "", 0)),
            Human::Whole,
        ),
        PackageCommand::List {
            command: ListCommand::Vendors { all, target, .. },
        } => (
            Contract::PackageListVendors,
            &target.root,
            answered(|calls| vendors(calls, &target.root, "", hidden(*all))),
            Human::Whole,
        ),
        PackageCommand::Values { kind, target, .. } => {
            let (contract, field) = match kind {
                ValueKind::OrderingInstitution => (
                    Contract::PackageValuesOrderingInstitution,
                    SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
                ),
                ValueKind::Orderer => (
                    Contract::PackageValuesOrderer,
                    SPIELORDERS_ORDER_FIELD_ORDERER,
                ),
                ValueKind::Project => (
                    Contract::PackageValuesProject,
                    SPIELORDERS_ORDER_FIELD_PROJECT,
                ),
                ValueKind::OrderGroup => (
                    Contract::PackageValuesOrderGroup,
                    SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
                ),
            };
            (
                contract,
                &target.root,
                answered(|calls| {
                    let package = open(calls, &target.root)?;
                    values(calls, &package, field)
                }),
                Human::Whole,
            )
        }
    }
}

fn order(command: &OrderCommand) -> Command<'_> {
    match command {
        OrderCommand::Show { target, .. } => (
            Contract::OrderShow,
            &target.root,
            answered(|calls| Ok(library::order(&*read_order(calls, target)?))),
            Human::Whole,
        ),
        OrderCommand::Readme { target, .. } => (
            Contract::OrderReadme,
            &target.root,
            answered(|calls| {
                let order = read_order(calls, target)?;
                Ok(spielorders_order_readme(&order)
                    .map_or(Value::Null, |readme| library::readme(&readme)))
            }),
            Human::Bare(&["path"]),
        ),
        OrderCommand::Validate { target, .. } => (
            Contract::OrderValidate,
            &target.root,
            answered(|calls| {
                let (package, directory) = order_directory(calls, target)?;
                let diagnosis = calls.get(|diagnosis, outcome| {
                    spielorders_package_validate_order(
                        &package,
                        PathBytes::new(&directory),
                        0,
                        diagnosis,
                        outcome,
                    )
                })?;
                verdict(calls, &diagnosis, SPIELORDERS_FAILURE_ORDER_INVALID)
            }),
            Human::Verdict("valid"),
        ),
        OrderCommand::Repair {
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::OrderRepair,
            &target.root,
            answered(|calls| {
                let diagnose = |calls: &mut Calls| {
                    let (package, directory) = order_directory(calls, target)?;
                    calls.get(|diagnosis, outcome| {
                        spielorders_package_diagnose_order(
                            &package,
                            PathBytes::new(&directory),
                            0,
                            diagnosis,
                            outcome,
                        )
                    })
                };
                let diagnosis = diagnose(calls)?;
                repair(calls, &diagnosis, assignments, execution, diagnose)
            }),
            Human::Repair("spielorders order repair"),
        ),
        OrderCommand::Edit {
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::OrderEdit,
            &target.root,
            answered(|calls| {
                let draft = order_draft(calls, target)?;
                save(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_ORDER_FIELD,
                    assignments,
                    execution,
                )
            }),
            Human::Whole,
        ),
        OrderCommand::Align {
            target, execution, ..
        } => (
            Contract::OrderAlign,
            &target.root,
            answered(|calls| {
                let order = read_order(calls, target)?;
                let plan = calls
                    .get(|plan, outcome| spielorders_order_plan_align(&order, 0, plan, outcome))?;
                mutate(calls, &plan, execution)
            }),
            Human::Whole,
        ),
        OrderCommand::Remove {
            delete,
            target,
            execution,
            ..
        } => (
            Contract::OrderRemove,
            &target.root,
            answered(|calls| {
                let order = read_order(calls, target)?;
                let plan = calls
                    .get(|plan, outcome| spielorders_order_plan_remove(&order, 0, plan, outcome))?;
                let mut data = mutate(calls, &plan, execution)?;
                if *delete {
                    trash(calls, &mut data, execution)?;
                }
                Ok(data)
            }),
            Human::Whole,
        ),
        OrderCommand::Item { command } => item(command),
    }
}

/// Moves the directory a removal produced to the platform Trash, as this front end's own
/// act; a dry run says that it would. A Trash move that fails leaves the Order removed
/// and recoverable where the removal put it, and says where.
fn trash(calls: &mut Calls, data: &mut Value, execution: &Execution) -> Result<(), Answer> {
    let moved = data["operations"]
        .as_array()
        .and_then(|operations| {
            operations
                .iter()
                .find(|operation| {
                    operation["kind"].as_str()
                        == library::name(SPIELORDERS_OPERATION_MOVE_ORDER_DIRECTORY)
                })
                .map(|operation| operation["path"].clone())
        })
        .unwrap_or(Value::Null);
    if execution.dry_run {
        data["trash_move"] = json!({ "source": moved });
        return Ok(());
    }
    let removed = data["final_path"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_default();
    match platform::trash(&removed) {
        Ok(landed) => {
            data["trash_location"] = landed.map_or(Value::Null, |landed| {
                Value::from(landed.to_string_lossy().into_owned())
            });
            Ok(())
        }
        Err(error) => Err(calls.refuse_own(
            "trash_failed",
            json!({ "path": data["final_path"], "reason": error.to_string() }),
        )),
    }
}

fn item(command: &ItemCommand) -> Command<'_> {
    let (contract, target, execution, edit): (_, _, _, RowEdit<'_>) = match command {
        ItemCommand::List { target, .. } => {
            return (
                Contract::OrderShow,
                &target.root,
                answered(|calls| Ok(library::order(&*read_order(calls, target)?))),
                Human::Member(&["items"]),
            );
        }
        ItemCommand::Add {
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::OrderItemAdd,
            target,
            execution,
            Box::new(|calls, draft| {
                add_row(calls, draft, SPIELORDERS_FAMILY_ITEM_FIELD, assignments)
            }),
        ),
        ItemCommand::Edit {
            position,
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::OrderItemEdit,
            target,
            execution,
            Box::new(|calls, draft| {
                let row = named_row(calls, draft, position)?;
                assign(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_ITEM_FIELD,
                    Some(row),
                    assignments,
                )
            }),
        ),
        ItemCommand::Move {
            position,
            destination,
            target,
            execution,
            ..
        } => (
            Contract::OrderItemMove,
            target,
            execution,
            Box::new(|calls, draft| move_row(calls, draft, position, &destination.position)),
        ),
        ItemCommand::Remove {
            position,
            target,
            execution,
            ..
        } => (
            Contract::OrderItemRemove,
            target,
            execution,
            Box::new(|calls, draft| remove_row(calls, draft, position)),
        ),
    };
    let answer = answered(|calls| {
        let mut draft = order_draft(calls, target)?;
        edit(calls, &mut draft)?;
        save(calls, draft, SPIELORDERS_FAMILY_ORDER_FIELD, &[], execution)
    });
    (contract, &target.root, answer, Human::Whole)
}

fn vendor(command: &VendorCommand) -> Command<'_> {
    match command {
        VendorCommand::Show { name, target, .. } => (
            Contract::VendorShow,
            &target.root,
            answered(|calls| Ok(library::vendor(&*read_vendor(calls, &target.root, name)?))),
            Human::Whole,
        ),
        VendorCommand::Edit {
            name,
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::VendorEdit,
            &target.root,
            answered(|calls| {
                let draft = vendor_draft(calls, &target.root, name)?;
                save(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_VENDOR_FIELD,
                    assignments,
                    execution,
                )
            }),
            Human::Whole,
        ),
        VendorCommand::Hide {
            name,
            target,
            execution,
            ..
        } => (
            Contract::VendorHide,
            &target.root,
            answered(|calls| hide(calls, &target.root, name, true, execution)),
            Human::Whole,
        ),
        VendorCommand::Unhide {
            name,
            target,
            execution,
            ..
        } => (
            Contract::VendorUnhide,
            &target.root,
            answered(|calls| hide(calls, &target.root, name, false, execution)),
            Human::Whole,
        ),
        VendorCommand::Contact { command } => contact(command),
    }
}

fn hide(calls: &mut Calls, root: &Path, name: &str, hidden: bool, execution: &Execution) -> Read {
    let vendor = read_vendor(calls, root, name)?;
    let plan = calls.get(|plan, outcome| {
        spielorders_vendor_plan_hidden(&vendor, i32::from(hidden), 0, plan, outcome)
    })?;
    mutate(calls, &plan, execution)
}

fn contact(command: &ContactCommand) -> Command<'_> {
    let (contract, name, target, execution, edit): (_, _, _, _, RowEdit<'_>) = match command {
        ContactCommand::List { name, target, .. } => {
            return (
                Contract::VendorShow,
                &target.root,
                answered(|calls| Ok(library::vendor(&*read_vendor(calls, &target.root, name)?))),
                Human::Member(&["contacts"]),
            );
        }
        ContactCommand::Add {
            name,
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::VendorContactAdd,
            name,
            target,
            execution,
            Box::new(|calls, draft| {
                add_row(calls, draft, SPIELORDERS_FAMILY_CONTACT_FIELD, assignments)
            }),
        ),
        ContactCommand::Edit {
            name,
            position,
            assignments,
            target,
            execution,
            ..
        } => (
            Contract::VendorContactEdit,
            name,
            target,
            execution,
            Box::new(|calls, draft| {
                let row = named_row(calls, draft, position)?;
                assign(
                    calls,
                    draft,
                    SPIELORDERS_FAMILY_CONTACT_FIELD,
                    Some(row),
                    assignments,
                )
            }),
        ),
        ContactCommand::Move {
            name,
            position,
            destination,
            target,
            execution,
            ..
        } => (
            Contract::VendorContactMove,
            name,
            target,
            execution,
            Box::new(|calls, draft| move_row(calls, draft, position, &destination.position)),
        ),
        ContactCommand::Remove {
            name,
            position,
            target,
            execution,
            ..
        } => (
            Contract::VendorContactRemove,
            name,
            target,
            execution,
            Box::new(|calls, draft| remove_row(calls, draft, position)),
        ),
    };
    let answer = answered(|calls| {
        let mut draft = vendor_draft(calls, &target.root, name)?;
        edit(calls, &mut draft)?;
        save(
            calls,
            draft,
            SPIELORDERS_FAMILY_VENDOR_FIELD,
            &[],
            execution,
        )
    });
    (contract, &target.root, answer, Human::Whole)
}

fn search(command: &SearchCommand) -> Command<'_> {
    match command {
        SearchCommand::Orders {
            keywords,
            matching,
            target,
            ..
        } => (
            Contract::SearchOrders,
            &target.root,
            answered(|calls| {
                orders(
                    calls,
                    &target.root,
                    &keywords.keywords.join(" "),
                    case(matching.case_sensitive),
                )
            }),
            Human::Whole,
        ),
        SearchCommand::Vendors {
            all,
            keywords,
            matching,
            target,
            ..
        } => (
            Contract::SearchVendors,
            &target.root,
            answered(|calls| {
                vendors(
                    calls,
                    &target.root,
                    &keywords.keywords.join(" "),
                    case(matching.case_sensitive) | hidden(*all),
                )
            }),
            Human::Whole,
        ),
        SearchCommand::SupportingMaterial { query, target, .. } => {
            let answer = match open(&mut Calls::default(), &target.root) {
                Err(refused) => refused,
                Ok(package) => {
                    let directory = spielorders_package_material_directory(&package);
                    let directory =
                        Some(directory.as_path()).filter(|path| !path.as_os_str().is_empty());
                    let found = platform::search_supporting_material(
                        directory,
                        &query.join(" "),
                        PROVIDER_BOUND,
                    );
                    supporting_material(
                        &package,
                        found.as_ref().map(|(hits, state)| (&**hits, *state)),
                    )
                }
            };
            (
                Contract::SearchSupportingMaterial,
                &target.root,
                answer,
                Human::Whole,
            )
        }
    }
}

/// What a front end's provider found in `package`, as the library groups it: `None` is a
/// build with no provider for its platform.
fn supporting_material(
    package: &SpielordersPackage,
    found: Option<(&SpielordersHits, SpielordersConstant)>,
) -> Answer {
    // With no hits the library refuses as unsupported whatever the state says.
    let (hits, state) = found.map_or(
        (None, SPIELORDERS_PROVIDER_STATE_COMPLETE),
        |(hits, state)| (Some(hits), state),
    );
    answered(|calls| {
        let search = calls.get(|search, outcome| {
            spielorders_package_search_material(package, hits, state, 0, search, outcome)
        })?;
        Ok(library::search(&search))
    })
}

// ---------------------------------------------------------------------------
// The shared shapes.
// ---------------------------------------------------------------------------

/// The answer `body` reads, with every finding its calls gave beside it.
fn answered(body: impl FnOnce(&mut Calls) -> Read) -> Answer {
    let mut calls = Calls::default();
    match body(&mut calls) {
        Ok(data) => calls.answered(data),
        Err(refused) => refused,
    }
}

fn open(calls: &mut Calls, root: &Path) -> Result<Box<SpielordersPackage>, Answer> {
    let root = c_path(root);
    calls.get(|package, outcome| {
        spielorders_package_open(PathBytes::new(&root), 0, package, outcome)
    })
}

/// The Package at `root`, for a call whose own answer carries what opening it found.
fn open_quietly(root: &Path) -> Result<Box<SpielordersPackage>, Answer> {
    open(&mut Calls::default(), root)
}

/// The Order Directory an `order` command line names, and the Package that holds it:
/// `--root` itself, or `--uuid` within the current-directory Package.
fn order_directory(
    calls: &mut Calls,
    target: &OrderTarget,
) -> Result<(Box<SpielordersPackage>, CString), Answer> {
    let Some(order_id) = &target.uuid else {
        let directory = c_path(&target.root);
        let package = Calls::default().get(|package, outcome| {
            spielorders_package_open_from_order(PathBytes::new(&directory), 0, package, outcome)
        })?;
        return Ok((package, directory));
    };
    let package = open(calls, &target.root)?;
    let list = calls.get(|list, outcome| {
        spielorders_package_orders(
            &package,
            OptionalText::null(),
            0,
            SpielordersConstant(-1),
            0,
            0,
            list,
            outcome,
        )
    })?;
    // A Damaged Order Directory is found by its identity too, so a read of it says what
    // damaged it rather than that no such Order exists.
    let found = (0..spielorders_list_count(&list)).find(|row| {
        spielorders_list_field(&list, *row, SPIELORDERS_ORDER_FIELD_ORDER_ID).to_str()
            == Some(order_id.as_str())
    });
    match found {
        Some(row) => {
            let directory = c_path(spielorders_list_path(&list, row).as_path());
            Ok((package, directory))
        }
        None => Err(calls.refuse(
            SPIELORDERS_FAILURE_UNKNOWN_ORDER_ID,
            json!({ "order_id": order_id }),
        )),
    }
}

fn read_order(calls: &mut Calls, target: &OrderTarget) -> Result<Box<SpielordersOrder>, Answer> {
    let (package, directory) = order_directory(calls, target)?;
    calls.get(|order, outcome| {
        spielorders_package_read_order(&package, PathBytes::new(&directory), 0, order, outcome)
    })
}

fn order_draft(calls: &mut Calls, target: &OrderTarget) -> Result<Box<SpielordersDraft>, Answer> {
    let order = read_order(calls, target)?;
    calls.get(|draft, _| spielorders_order_draft(&order, draft))
}

/// The Vendor a Name selects, Hidden or not.
fn read_vendor(
    calls: &mut Calls,
    root: &Path,
    name: &str,
) -> Result<Box<SpielordersVendor>, Answer> {
    let package = open(calls, root)?;
    let name = c_text(name);
    calls.get(|vendor, outcome| {
        spielorders_package_read_vendor(
            &package,
            SPIELORDERS_VENDOR_SELECTOR_NAME,
            Text::new(&name),
            0,
            vendor,
            outcome,
        )
    })
}

fn vendor_draft(
    calls: &mut Calls,
    root: &Path,
    name: &str,
) -> Result<Box<SpielordersDraft>, Answer> {
    let vendor = read_vendor(calls, root, name)?;
    calls.get(|draft, _| spielorders_vendor_draft(&vendor, draft))
}

/// Sets each assignment on `draft` at `row`, or on the record's own facts where it is
/// `None`, as a field of the family `family`. Status goes first, so a Date Ordered on the same command line is the
/// one kept; an assignment is the confirmation of any new reusable value it names.
fn assign(
    calls: &mut Calls,
    draft: &mut SpielordersDraft,
    family: SpielordersConstant,
    row: Option<i64>,
    assignments: &[FieldAssignment],
) -> Result<(), Answer> {
    let mut rejected = Vec::new();
    let mut fields = Vec::new();
    for (index, assignment) in assignments.iter().enumerate() {
        let repeated = assignments[..index]
            .iter()
            .any(|earlier| earlier.field == assignment.field);
        match constant(family, &assignment.field) {
            _ if repeated => rejected.push((assignment, SPIELORDERS_REASON_DUPLICATE)),
            Some(field) => fields.push((field, assignment)),
            None => rejected.push((assignment, SPIELORDERS_REASON_UNKNOWN_FIELD)),
        }
    }
    fields.sort_by_key(|(field, _)| *field != SPIELORDERS_ORDER_FIELD_STATUS);
    for (field, assignment) in fields {
        let value = c_text(assignment.value.as_str());
        let set = calls.attempt(|outcome| match row {
            None => spielorders_draft_set(draft, field, Text::new(&value), outcome),
            Some(row) => spielorders_draft_row_set(draft, row, field, Text::new(&value), outcome),
        });
        match set {
            // A field of the family that a draft only reads.
            Err((SPIELORDERS_RESULT_BAD_ARGUMENT, _)) => {
                rejected.push((assignment, SPIELORDERS_REASON_UNKNOWN_FIELD));
            }
            // A row the draft does not have.
            Err((_, refused)) => return Err(refused),
            // Only the four reusable fields take a confirmation; the others refuse it.
            Ok(()) if row.is_none() => {
                let _ = spielorders_draft_confirm(draft, field, -1, None);
            }
            Ok(()) => {}
        }
    }
    if rejected.is_empty() {
        return Ok(());
    }
    let detail: Vec<Value> = rejected
        .into_iter()
        .map(|(assignment, reason)| {
            json!({ "field": assignment.field, "value": assignment.value, "reason": library::named(reason) })
        })
        .collect();
    Err(calls.refuse(
        SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS,
        Value::Array(detail),
    ))
}

/// A one-based position as the draft row it names. A position that is not a whole number
/// is refused here; one that names no row, by the draft.
fn row(calls: &mut Calls, position: &str) -> Result<i64, Answer> {
    match position.parse::<u32>() {
        Ok(position) => Ok(i64::from(position) - 1),
        Err(_) => Err(calls.refuse(
            SPIELORDERS_FAILURE_POSITION_NOT_A_NUMBER,
            json!({ "position": position }),
        )),
    }
}

/// The row a position names in `draft`, refused by the draft where it names none, even
/// when the command line then sets nothing on it.
fn named_row(
    calls: &mut Calls,
    draft: &mut SpielordersDraft,
    position: &str,
) -> Result<i64, Answer> {
    let at = row(calls, position)?;
    calls.act(|outcome| spielorders_draft_row_move(draft, at, at, outcome))?;
    Ok(at)
}

fn add_row(
    calls: &mut Calls,
    draft: &mut SpielordersDraft,
    family: SpielordersConstant,
    assignments: &[FieldAssignment],
) -> Result<(), Answer> {
    let (position, facts) = cli::insertion_assignments(assignments);
    let at = match position {
        Some(position) => row(calls, position)?,
        None => spielorders_draft_row_count(draft),
    };
    calls.act(|outcome| spielorders_draft_row_insert(draft, at, outcome))?;
    assign(calls, draft, family, Some(at), &facts)
}

fn move_row(
    calls: &mut Calls,
    draft: &mut SpielordersDraft,
    from: &str,
    to: &str,
) -> Result<(), Answer> {
    let (from, to) = (row(calls, from)?, row(calls, to)?);
    calls.act(|outcome| spielorders_draft_row_move(draft, from, to, outcome))
}

fn remove_row(
    calls: &mut Calls,
    draft: &mut SpielordersDraft,
    position: &str,
) -> Result<(), Answer> {
    let at = row(calls, position)?;
    calls.act(|outcome| spielorders_draft_row_remove(draft, at, outcome))
}

/// Saves `draft` with `assignments` set on its own facts: the one mutation shape a
/// record edit takes.
/// Clears the name a new draft starts with: on the command line, the Order Title and the
/// Vendor Name are the user's to give.
fn without_default(draft: &mut SpielordersDraft, field: SpielordersConstant) {
    let blank = c_text("");
    let _ = spielorders_draft_set(draft, field, Text::new(&blank), None);
}

fn save(
    calls: &mut Calls,
    mut draft: Box<SpielordersDraft>,
    family: SpielordersConstant,
    assignments: &[FieldAssignment],
    execution: &Execution,
) -> Read {
    assign(calls, &mut draft, family, None, assignments)?;
    let plan = calls.get(|plan, outcome| spielorders_draft_plan(&draft, 0, plan, outcome))?;
    mutate(calls, &plan, execution)
}

/// What `plan` did once committed, or will do under `--dry-run`.
fn mutate(calls: &mut Calls, plan: &SpielordersPlan, execution: &Execution) -> Read {
    let planned = library::plan(plan);
    if execution.dry_run {
        return Ok(Value::Object(library::planned(planned)));
    }
    let before = calls.warned();
    let report = calls.get(|report, outcome| spielorders_plan_commit(plan, report, outcome))?;
    let warned = !before && calls.warned();
    Ok(Value::Object(library::report(&report, planned, warned)))
}

/// A validation's answer: the diagnosis of a valid scope, or `invalid`'s refusal.
fn verdict(
    calls: &mut Calls,
    diagnosis: &SpielordersDiagnosis,
    invalid: SpielordersConstant,
) -> Read {
    let judged = library::diagnosis(diagnosis);
    if judged["valid"] == true {
        Ok(judged)
    } else {
        Err(calls.refuse(invalid, judged))
    }
}

/// A bare `repair`'s diagnosis, or the one action a command line applies to one case and
/// what `diagnose` then finds remaining.
fn repair(
    calls: &mut Calls,
    diagnosis: &SpielordersDiagnosis,
    assignments: &[FieldAssignment],
    execution: &Execution,
    diagnose: impl FnOnce(&mut Calls) -> Result<Box<SpielordersDiagnosis>, Answer>,
) -> Read {
    let RepairRequest::Apply {
        case: handle,
        action: wanted,
        resolutions,
    } = cli::repair_request(assignments)
    else {
        return Ok(library::diagnosis(diagnosis));
    };
    let Some(case) = (0..spielorders_diagnosis_case_count(diagnosis))
        .find(|case| library::case_handle(diagnosis, *case) == handle)
    else {
        return Err(calls.refuse(
            SPIELORDERS_FAILURE_UNKNOWN_CASE,
            json!({ "handle": handle }),
        ));
    };
    let Some(action) = (0..spielorders_diagnosis_action_count(diagnosis, case)).find(|action| {
        library::name(spielorders_diagnosis_action(diagnosis, case, *action))
            == Some(wanted.as_str())
    }) else {
        return Err(calls.refuse(
            SPIELORDERS_FAILURE_UNAUTHORIZED_ACTION,
            json!({ "handle": handle, "action": wanted }),
        ));
    };
    let fields: Vec<SpielordersConstant> =
        (0..spielorders_diagnosis_action_field_count(diagnosis, case, action))
            .map(|field| spielorders_diagnosis_action_field(diagnosis, case, action, field))
            .collect();
    if let Some(unexpected) = resolutions.iter().find(|resolution| {
        !fields
            .iter()
            .any(|field| resolution_name(*field) == resolution.field)
    }) {
        return Err(calls.refuse(
            SPIELORDERS_FAILURE_REPAIR_RESOLUTION_UNEXPECTED,
            json!({ "action": wanted, "field": unexpected.field }),
        ));
    }
    // An action whose values the command line supplies none of plans with no draft, and
    // the library refuses it for the values it is owed.
    let mut draft = if resolutions.is_empty() {
        None
    } else {
        Some(calls.get(|draft, outcome| {
            spielorders_diagnosis_draft(diagnosis, case, action, draft, outcome)
        })?)
    };
    if let Some(draft) = &mut draft {
        for resolution in &resolutions {
            if let Some(field) = fields
                .iter()
                .find(|field| resolution_name(**field) == resolution.field)
            {
                let value = c_text(resolution.value.as_str());
                calls.act(|outcome| {
                    spielorders_draft_set(draft, *field, Text::new(&value), outcome)
                })?;
            }
        }
    }
    let plan = calls.get(|plan, outcome| {
        spielorders_diagnosis_plan_repair(
            diagnosis,
            case,
            action,
            draft.as_deref(),
            0,
            plan,
            outcome,
        )
    })?;
    let mut data = mutate(calls, &plan, execution)?;
    if !execution.dry_run {
        data["remaining"] = library::diagnosis(&*diagnose(calls)?);
    }
    Ok(data)
}

fn index(package: &SpielordersPackage) -> Value {
    let reasons: Vec<Value> = (0..spielorders_package_index_reason_count(package))
        .map(|reason| library::named(spielorders_package_index_reason(package, reason)))
        .collect();
    json!({
        "state": library::named(spielorders_package_index_state(package)),
        "reasons": reasons,
    })
}

/// Everything a front end needs to open a Package, in one answer.
fn show(calls: &mut Calls, root: &Path) -> Read {
    let package = open(calls, root)?;
    let orders = list_orders(calls, &package, "", 0)?;
    let vendors = list_vendors(calls, &package, "", SPIELORDERS_LIST_INCLUDE_HIDDEN)?;
    let mut values = Map::new();
    for field in [
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        SPIELORDERS_ORDER_FIELD_ORDERER,
        SPIELORDERS_ORDER_FIELD_PROJECT,
        SPIELORDERS_ORDER_FIELD_ORDER_GROUP,
    ] {
        let listed = self::values(calls, &package, field)?;
        values.insert(library::name(field).unwrap_or_default().to_owned(), listed);
    }
    let readme = calls
        .get(|readme, outcome| spielorders_package_read_readme(&package, 0, readme, outcome))?;
    Ok(json!({
        "root": library::path(spielorders_package_root(&package)),
        "index": index(&package),
        "readme": library::readme(&readme),
        "orders": orders,
        "vendors": vendors,
        "values": values,
    }))
}

fn orders(calls: &mut Calls, root: &Path, query: &str, flags: u32) -> Read {
    let package = open(calls, root)?;
    list_orders(calls, &package, query, flags)
}

fn list_orders(calls: &mut Calls, package: &SpielordersPackage, query: &str, flags: u32) -> Read {
    let query = c_text(query);
    let list = calls.get(|list, outcome| {
        spielorders_package_orders(
            package,
            OptionalText::new(&query),
            flags,
            SpielordersConstant(-1),
            0,
            0,
            list,
            outcome,
        )
    })?;
    Ok(library::rows(&list, SPIELORDERS_ORDER_FIELD_TITLE))
}

fn vendors(calls: &mut Calls, root: &Path, query: &str, flags: u32) -> Read {
    let package = open(calls, root)?;
    list_vendors(calls, &package, query, flags)
}

fn list_vendors(calls: &mut Calls, package: &SpielordersPackage, query: &str, flags: u32) -> Read {
    let query = c_text(query);
    let list = calls.get(|list, outcome| {
        spielorders_package_vendors(
            package,
            OptionalText::new(&query),
            flags,
            SpielordersConstant(-1),
            0,
            0,
            list,
            outcome,
        )
    })?;
    Ok(library::rows(&list, SPIELORDERS_VENDOR_FIELD_NAME))
}

fn values(calls: &mut Calls, package: &SpielordersPackage, field: SpielordersConstant) -> Read {
    let list =
        calls.get(|list, outcome| spielorders_package_values(package, field, list, outcome))?;
    Ok(library::rows(&list, SPIELORDERS_VALUE_FIELD_VALUE))
}

fn case(sensitive: bool) -> u32 {
    if sensitive {
        SPIELORDERS_LIST_CASE_SENSITIVE
    } else {
        0
    }
}

fn hidden(all: bool) -> u32 {
    if all {
        SPIELORDERS_LIST_INCLUDE_HIDDEN
    } else {
        0
    }
}

/// `search supporting-material`'s answers, replayed from supplied provider facts.
///
/// The executable asks only its platform's live index, whose contents and availability no
/// test controls, so the answer cells and the runtime index states are checked here: the
/// provider's facts go in through the library's interface as the Spotlight query hands
/// them over, and the document comes out of the same answer and envelope a run writes.
/// Fixture Packages are built by this executable's own commands.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli,
        platform::{Found, Hit, hits},
        response::document,
    };
    use std::ffi::OsString;

    // Decision sources: Define lookup and search behavior; Define the Rust CLI capability
    // contract.

    /// A Package built in a disposable directory, removed afterwards.
    struct Package {
        tree: PathBuf,
        root: PathBuf,
    }

    impl Package {
        fn initialized(label: &str) -> Self {
            let tree = std::env::temp_dir().join(format!(
                "spielorders-cli-commands-{label}-{}",
                std::process::id()
            ));
            std::fs::create_dir(&tree).unwrap();
            let root = std::fs::canonicalize(&tree).unwrap();
            let package = Self { tree, root };
            package.command(&["init"], &[]);
            package
        }

        /// Runs one command line against this Package, which must answer.
        fn command(&self, line: &[&str], assignments: &[&str]) {
            let arguments: Vec<OsString> = std::iter::once("spielorders")
                .chain(line.iter().copied())
                .chain(assignments.iter().copied())
                .map(OsString::from)
                .chain([OsString::from("--root"), self.root.clone().into()])
                .collect();
            let parsed = cli::parse(&arguments).unwrap();
            let shown = run(&parsed.family);
            assert!(
                matches!(shown.answer, Answer::Answered { .. }),
                "{line:?} {assignments:?} answers"
            );
        }

        /// The document this Package's answer is for a provider that found `found`, or for
        /// a build with no provider, with the Package Root replaced as every recorded cell
        /// replaces it.
        fn answered(&self, found: Option<&Found>) -> String {
            let package = open_quietly(&self.root).unwrap_or_else(|_| panic!("the Package opens"));
            let gathered = found.map(|found| (hits("spotlight", found).unwrap(), found.state));
            let answer = supporting_material(
                &package,
                gathered.as_ref().map(|(hits, state)| (&**hits, *state)),
            );
            let bytes = document(&Shown {
                contract: Contract::SearchSupportingMaterial,
                root: Some(&self.root),
                answer,
                human: Human::Whole,
            })
            .unwrap();
            String::from_utf8(bytes)
                .unwrap()
                .replace(self.root.to_str().unwrap(), "<PACKAGE_ROOT>")
        }
    }

    impl Drop for Package {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tree);
        }
    }

    fn complete(hits: Vec<Hit>) -> Found {
        Found {
            state: SPIELORDERS_PROVIDER_STATE_COMPLETE,
            hits,
            said: Vec::new(),
        }
    }

    /// One recorded `search supporting-material` cell.
    fn recorded(cell: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/cli/search/supporting_material/v1")
            .join(format!("{cell}.json"));
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    #[test]
    fn a_reported_file_matches_the_recorded_success() {
        let package = Package::initialized("material-success");
        package.command(
            &["add-order"],
            &[
                "title=Laser components",
                "status=ordered",
                "date_ordered=2026-08-22",
            ],
        );
        let path = package
            .root
            .join("Orders/2026_08_22_Laser_components/quote-coherence.txt");
        std::fs::write(&path, b"Supporting Material contract capture: coherence.\n").unwrap();

        let actual = package.answered(Some(&complete(vec![Hit {
            path,
            kind: Some("txt".to_owned()),
            modified_unix_seconds: Some(1_784_937_600),
        }])));

        assert_eq!(actual, recorded("success"));
    }

    #[test]
    fn nothing_found_matches_the_recorded_empty_result() {
        let package = Package::initialized("material-empty");
        package.command(&["add-order"], &["title=Laser components"]);

        assert_eq!(
            package.answered(Some(&complete(Vec::new()))),
            recorded("empty-result")
        );
    }

    #[test]
    fn a_damaged_order_beside_the_answer_matches_the_recorded_warning() {
        let package = Package::initialized("material-warning");
        package.command(&["add-vendor"], &["name=Nethermoor"]);
        package.command(
            &["add-order"],
            &[
                "title=Mirror mounts",
                "date_ordered=2026-08-21",
                "vendor=Nethermoor",
                "orderer=Ada Lovelace",
            ],
        );
        std::fs::write(
            package.root.join(
                "Orders/2026_08_21_Nethermoor_Mirror_mounts_Ada_Lovelace/.spielorders/order.json",
            ),
            b"{ not json",
        )
        .unwrap();

        assert_eq!(
            package.answered(Some(&complete(Vec::new()))),
            recorded("warning")
        );
    }

    /// A provider that could not look refuses under its own code; one that looked answers,
    /// partially or not; and a build with no provider says it has none.
    #[test]
    fn each_runtime_index_state_is_answered_or_refused_by_its_own_code() {
        let package = Package::initialized("material-states");
        package.command(&["add-order"], &["title=Laser components"]);
        let response = |found: Option<Found>| -> Value {
            serde_json::from_str(&package.answered(found.as_ref())).unwrap()
        };

        for (state, code) in [
            (
                SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED,
                "indexing_disabled",
            ),
            (
                SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED,
                "volume_not_indexed",
            ),
            (SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED, "access_denied"),
            (SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, "query_refused"),
        ] {
            let document = response(Some(Found {
                state,
                ..complete(Vec::new())
            }));
            assert_eq!(document["ok"], false, "{code}");
            assert!(document["data"].is_null(), "{code}");
            assert_eq!(document["error"]["code"], code);
        }
        for (state, name) in [
            (SPIELORDERS_PROVIDER_STATE_COMPLETE, "complete"),
            (SPIELORDERS_PROVIDER_STATE_TIMED_OUT, "timed_out"),
            (SPIELORDERS_PROVIDER_STATE_CANCELLED, "cancelled"),
        ] {
            let document = response(Some(Found {
                state,
                ..complete(Vec::new())
            }));
            assert_eq!(document["ok"], true, "{name}");
            assert!(document["error"].is_null(), "{name}");
            assert_eq!(document["data"]["provider"], "spotlight", "{name}");
            assert_eq!(document["data"]["state"], name);
            assert_eq!(document["data"]["files"], json!([]), "{name}");
        }
        let document = response(None);
        assert_eq!(document["ok"], false);
        assert!(document["data"].is_null());
        assert_eq!(document["error"]["code"], "unsupported");
    }
}
