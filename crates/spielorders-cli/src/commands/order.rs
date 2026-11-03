//! Every command about one Order: three reads of it, the two that give a directory its
//! first record, the two that move the directory whole, the edit of its own facts, the four
//! that maintain its ordered Items, and the targeting they share.
//!
//! **Targeting is what this module owns most.** For an `order` command `--root` names the
//! **Order Directory**, not the SpielOrders Root, and it names it exactly: the Package is
//! reached from there through the Order record's own `package_root` locator, which is a
//! computation and never a search. `--uuid` is the same pair the other way round — the
//! Package is the current directory and the Order Directory is what the identity resolves
//! to. Those are the only two ways in, they are mutually exclusive, and neither walks an
//! ancestor chain. M3 shipped a repair that counted two parents to find its Package and
//! refreshed a stranger's directory under a nested layout; the same mistake in a targeting
//! layer would be wrong for every `order` command at once, which is why there is exactly
//! one function here that turns a command line into an Order.
//!
//! **The three reads are one set.** They agree on Item numbering, on the calculated
//! totals, on how a path is represented, on which diagnostics travel, and on how a
//! Damaged Order is answered — because all three ask [`read`] and all three present
//! through [`present`]. A consumer reading `order show` and `order item list` over one
//! Order gets the same Items with the same numbers.
//!
//! **Every mutation that answers with an Order answers with that same projection.**
//! [`Show`] is what `order show` renders and what six of the seven mutations return beside
//! their own typed report, under their own contract identity — so a client that can display
//! an Order can display the one a mutation left, without a second read and without a second
//! shape to learn. `order remove` is the exception and says why: the Order it took out is no
//! longer one of this Package's Orders.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the Order record
//! and lifecycle contract; Define durable metadata schemas and version evolution.

use crate::{
    cli::FieldAssignment,
    commands::{
        repair_suggestion::{RepairSuggestion, Rooting},
        shared::{
            Failed, IndexProjection, OrderPathsProjection, RootKind, VendorSummary,
            damaged_failure, diagnostics, failed_document, in_package, index, link_named_as_a_root,
            not_representable, operation_failure, order_root_not_a_directory, package_candidate,
            report_diagnostic, resolved, text, undeliverable, upgrade_pending_failure,
        },
    },
    contract::ResponseContract,
    effects::{PlatformCalendar, PlatformFileSystem, PlatformTrash, PlatformUuids},
    exit::Exit,
    money::displayed_amount,
    output::Output,
    response::{
        FailureCode, FailureScope, Identity, LocationProjection, NotRepresentable, OrderContext,
        Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    adoption::{
        OrderAdoptionInput, OrderAdoptionPlan, OrderAdoptionPlanError, OrderAdoptionRejection,
        OrderAdoptionWarning, OrderItemInput, commit_order_adoption, plan_order_adoption,
    },
    alignment::{
        OrderAlignmentPlanError, OrderAlignmentRejection, OrderAlignmentWarning,
        commit_order_alignment, plan_order_alignment,
    },
    canonical::{HealthyOrder, ManagedContext, PackageIndexRecord, inspect_canonical_package},
    creation::{
        OrderCreationInput, OrderCreationPlan, OrderCreationPlanError, OrderCreationRejection,
        OrderCreationWarning, commit_order_creation, plan_order_creation,
    },
    filesystem::ValidatedReplacementFailure,
    metadata::{OrderItem, Vendor, Version1},
    mutation::{MutationReport, RejectedAssignment},
    operations::FailedPath,
    order_edits::{
        ItemAssignments, ItemPositionRole, NewItemAssignmentFailure, NewOrderAssignmentFailure,
        OrderAssignments, OrderEditOperation, OrderEditPlan, OrderEditPlanError,
        OrderEditRejection, OrderEditResult, OrderEditReusableValues, OrderEditWarning,
        OrderFactKind, OrderItemMutationKind, OrderItemMutationPlan, commit_order_edit,
        commit_order_item_mutation, plan_order_edit, plan_order_item_add, plan_order_item_edit,
        plan_order_item_move, plan_order_item_remove,
    },
    order_reads::{OrderRead, OrderReadFailure, read_order, read_order_by_id},
    package::ReusableValue,
    ports::FileSystemEntryKind,
    readme::{DiagnosticCode, EvidenceScope, Location},
    removal::{
        LocatorOutcome, OrderRemovalPlanError, OrderRemovalRejection, OrderRemovalResult,
        OrderRemovalWarning, RemovalDisposition, RemovedDirectoryCleanup, commit_order_removal,
        plan_order_removal,
    },
};
use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

// ---------------------------------------------------------------------------
// Targeting: the one way in.
// ---------------------------------------------------------------------------

/// Which Order the command line named, and by which of the two routes.
///
/// **`--root` and `--uuid` are mutually exclusive**, so a command line names exactly one
/// of these. The parser refuses the combination as a usage error rather than letting one
/// take precedence, which is what makes this an enum rather than two optional fields whose
/// interaction something here would have to decide.
pub(crate) enum Target<'a> {
    /// The Order Directory, named exactly. `--root`, whose default is `.` — so standing in
    /// an Order Directory and naming no option at all is this route.
    Directory(&'a Path),
    /// One Order of the current-directory Package, by its immutable Order ID.
    ///
    /// **Provisional**, reserved for GUI and scripting evaluation during MVP, and it may
    /// be removed before the CLI contract is declared stable. Removing it costs this
    /// variant, its arm in [`read`], and the option in [`crate::cli`] — nothing past
    /// [`read`] knows which route was taken — which is what "the implementation should not
    /// make that removal expensive" asks for.
    Identity {
        /// The current-directory Package. Necessarily `--root`'s own default, because
        /// supplying `--root` alongside `--uuid` is a usage error — so the working
        /// directory reaches the core from exactly where it does for every other command,
        /// as the caller's own `.`, and this adapter resolves nothing.
        package_root: &'a Path,
        order_id: &'a str,
    },
}

/// The one read every `order` command performs.
///
/// Both routes end in the same [`OrderRead`], which is why no command in this family can
/// come to disagree with another about an Order — the reads that answer with one and the
/// mutations that resolve their subject through it alike: there is one projection of it and
/// it is the core's. The typed failure is built here for the same reason: an `order`
/// command that classified its own would be the second place doing it.
///
/// A `--root` naming a link is refused before the core is asked, because a link is never
/// followed and what stands behind this one is not what the command line named.
fn read(target: &Target) -> Result<OrderRead, Box<OrderFailed>> {
    match target {
        Target::Directory(directory) => match link_named_as_a_root(directory, RootKind::Order) {
            Some(failed) => Err(Box::new(OrderFailed {
                failed,
                order_root: None,
            })),
            None => read_order(directory, &PlatformFileSystem)
                .map_err(|reason| Box::new(failure(reason))),
        },
        Target::Identity {
            package_root,
            order_id,
        } => read_order_by_id(package_root, order_id, &PlatformFileSystem)
            .map_err(|reason| Box::new(failure(reason))),
    }
}

/// The Order this command read, or the typed reason there is none to report.
enum Answer {
    Answered(Box<OrderRead>),
    Failed(OrderFailed),
}

/// One typed failure, plus the Order Directory root an `order` context also names.
///
/// The [`Failed`] inside is the shared classification, so an `order` command reports an
/// unmanaged, damaged, upgrade-pending, or unreadable Package with the same code and the
/// same message a Package-level command does. What is added is the second root: an
/// `order` command's context names the Order Directory as well as the Package, the two
/// are established by different routes, and a failure can hold either, both, or neither.
pub(crate) struct OrderFailed {
    pub failed: Failed,
    pub order_root: Option<PathBuf>,
}

impl OrderFailed {
    /// A failure about the Order Directory itself, which this read never resolved.
    ///
    /// The three of these are the failures reached before anything was canonicalized: the
    /// caller's own text is in `error.paths` with its `as_given` spelling, and neither
    /// root was established.
    pub(crate) fn unresolved(code: FailureCode, message: String, path: FailedPath) -> Self {
        Self {
            failed: Failed {
                code,
                scope: FailureScope::Order,
                message,
                paths: vec![path],
                identities: Vec::new(),
                // The locator is the only route from an Order Directory to a Package, and
                // each of these failures is a reason there was no usable locator to
                // follow. Naming a Package would name one nothing reached.
                package_root: None,
                diagnostics: Vec::new(),
            },
            order_root: None,
        }
    }
}

/// Every way an `order` command can fail, mapped to one stable code.
///
/// The four Package-load states go through the shared constructors unchanged, so an
/// `order` command and a Package-level command answer them identically. The five that are
/// this family's own each get their own code, because the action behind each differs: a
/// directory that is no Order at all, a record that will not read, a locator that cannot
/// be used, a locator that leads somewhere else, and an Order ID that names nothing.
fn failure(reason: OrderReadFailure) -> OrderFailed {
    match reason {
        OrderReadFailure::RootNotADirectory { directory } => {
            order_root_not_a_directory(directory.as_path())
        }
        OrderReadFailure::NotAnOrderDirectory { directory } => OrderFailed::unresolved(
            FailureCode::OrderRootUnmanaged,
            format!(
                "{} is not an Order Directory: it holds no Order record. For an `order` \
                 command --root names the Order Directory itself — no ancestor of it is \
                 searched, and a SpielOrders Root is not one of its own Orders.",
                directory.as_path().display()
            ),
            directory,
        ),
        OrderReadFailure::DamagedRecord { directory, field } => OrderFailed::unresolved(
            FailureCode::OrderRecordDamaged,
            match &field {
                Some(field) => format!(
                    "the Order record at {} could not be read as one: `{field}` is the \
                     first field at fault. `spielorders order validate` is the diagnosis.",
                    directory.as_path().display()
                ),
                None => format!(
                    "the Order record at {} could not be read as one. \
                     `spielorders order validate` is the diagnosis.",
                    directory.as_path().display()
                ),
            },
            directory,
        ),
        OrderReadFailure::LocatorUnusable { directory } => OrderFailed::unresolved(
            FailureCode::OrderLocatorUnusable,
            format!(
                "the Order record at {} carries no usable `package_root` locator, so \
                 nothing can reach this Order's Package from its Directory. It is \
                 recalculated from where the directory sits; {}",
                directory.as_path().display(),
                RepairSuggestion::for_condition(
                    Rooting::Order,
                    DiagnosticCode::OrderPackageLocatorMismatch,
                    EvidenceScope::Order,
                )
            ),
            directory,
        ),
        OrderReadFailure::LocatorReachesNoPackageHoldingThisOrder {
            directory,
            package_root,
        } => OrderFailed {
            // The Order Directory did resolve — resolving it is how the locator was read
            // at all — so the context names it. What the locator names is what holds no
            // such Order, and no Package was established.
            order_root: directory
                .location()
                .map(|location| location.absolute_path().to_path_buf()),
            failed: Failed {
                code: FailureCode::OrderLocatorStale,
                scope: FailureScope::Order,
                message: format!(
                    "the Order record at {} carries the `package_root` locator \
                     `{package_root}`, and the place that locator names holds no such \
                     Order — so it is stale, or it belongs to another Package. It is \
                     recalculated from where the directory sits; {}",
                    directory.as_path().display(),
                    RepairSuggestion::for_condition(
                        Rooting::Order,
                        DiagnosticCode::OrderPackageLocatorMismatch,
                        EvidenceScope::Order,
                    )
                ),
                paths: vec![directory],
                identities: Vec::new(),
                // The locator is the only route from here to a Package, and where it leads
                // holds no such Order — so no Package was established to name.
                package_root: None,
                diagnostics: Vec::new(),
            },
        },
        OrderReadFailure::DamagedOrder {
            context,
            paths,
            order_id,
            diagnostics,
        } => OrderFailed {
            order_root: Some(paths.directory().absolute_path().to_path_buf()),
            failed: Failed {
                code: FailureCode::OrderRecordDamaged,
                scope: FailureScope::Order,
                message: format!(
                    "the Order at {} is damaged, so it has no facts to report. The \
                     diagnostics below are the diagnosis; {}",
                    paths.directory().absolute_path().display(),
                    // This Order's own findings, and this response is rooted where an
                    // Order command would be run.
                    RepairSuggestion::for_findings(Rooting::Order, &diagnostics)
                ),
                paths: vec![paths.directory().clone().into()],
                // The identity where the load could still trust one. It does not guess,
                // and neither does this.
                identities: order_id
                    .iter()
                    .map(|order_id| Identity::order(order_id.as_str()))
                    .collect(),
                package_root: Some(context.root().absolute_path().to_path_buf()),
                // **This Order's own diagnostics, from the core.** `data` is `null` on a
                // failure, so the envelope is the only channel this answer has — the same
                // shape a damaged Package's diagnosis travels in. What a Damaged Order
                // does *not* get is a projection of facts it has none of, or a
                // `diagnostics` member inside a placeholder: the core selected these from
                // the groups it made, so nothing here constructed a grouping.
                diagnostics,
            },
        },
        OrderReadFailure::OrderIdNotFound { context, order_id } => OrderFailed {
            // An Order ID that resolves to nothing has no Order Directory to name; the
            // Package it was resolved against is the root this answer is about.
            order_root: None,
            failed: Failed {
                code: FailureCode::OrderNotFound,
                scope: FailureScope::Order,
                message: format!(
                    "the Package at {} holds no Order carrying the Order ID {order_id}. \
                     --uuid resolves an identity against the current-directory Package \
                     and searches no other.",
                    context.root().absolute_path().display()
                ),
                // About an identity and about no path, which is the case the contract's
                // "relevant paths **or** identities" is for.
                paths: Vec::new(),
                identities: vec![Identity::order(&order_id)],
                package_root: Some(context.root().absolute_path().to_path_buf()),
                diagnostics: Vec::new(),
            },
        },
        // Reachable only through `--uuid`. The locator route maps an unmanaged locator
        // target to a stale locator instead, because the locator is the only reason it
        // looked there at all — so this arm's sentence can name the route the caller
        // actually took. The *code* is the shared one, because a directory that is not a
        // SpielOrders Root is the same answer whichever command asked; only the wording
        // differs, and `shared::unmanaged_failure`'s names `--root`, which a `--uuid`
        // caller never supplied.
        OrderReadFailure::UnmanagedPackage(package) => OrderFailed {
            order_root: None,
            failed: Failed {
                code: FailureCode::PackageRootUnmanaged,
                scope: FailureScope::Package,
                message: format!(
                    "{} is not a SpielOrders Root: it has no .spielorders directory. \
                     --uuid resolves an Order ID against the current-directory Package, \
                     and no ancestor of that directory is consulted.",
                    package.root().absolute_path().display()
                ),
                paths: vec![package.root().clone().into()],
                identities: Vec::new(),
                // A resolved directory, but not a SpielOrders Root. Naming it as
                // `package_root` would report a Package that is precisely what is absent.
                package_root: None,
                diagnostics: Vec::new(),
            },
        },
        // The three remaining Package-load states, classified exactly as a Package-level
        // command classifies them, message included. `order_root` is `null` for each: the
        // answer is about the Package, and the path the failure names is the Package's.
        OrderReadFailure::DamagedPackage { root, projection } => {
            rooted_at_package(damaged_failure(&projection, &root))
        }
        OrderReadFailure::UpgradePending(package) => {
            rooted_at_package(upgrade_pending_failure(&package))
        }
        OrderReadFailure::PackageOperationFailed(package) => rooted_at_package(operation_failure(
            package.failure(),
            package.context().map(|context| context.root()),
        )),
        OrderReadFailure::Operation(operation) => {
            rooted_at_package(operation_failure(&operation, None))
        }
    }
}

fn rooted_at_package(failed: Failed) -> OrderFailed {
    OrderFailed {
        failed,
        order_root: None,
    }
}

// ---------------------------------------------------------------------------
// The payloads.
// ---------------------------------------------------------------------------

/// One Order's facts, borrowed from wherever this run has them.
///
/// **The one place the sources are reconciled.** A read and a committed mutation both have
/// a stored Order; a `--dry-run` has the Order a plan *would* leave, which is the same
/// projection before any directory carries it. [`Show::of`] and [`human_order`] are both
/// built from this, so a fact reaches a reader the same way however the run got it and
/// neither surface is a projection of the other.
///
/// Borrowed rather than owned, and the human surface is why: a path that is not valid UTF-8
/// has no JSON spelling and is reported as an operational failure under `--json`, while
/// human output writes its own bytes. Going through an owned projection would make the
/// human modes fail on an Order the machine mode is merely unable to describe.
struct OrderFacts<'a> {
    /// `null` in exactly one case: a creation or an adoption that was planned and not
    /// committed. An Order ID is minted at the commit, so a `--dry-run` has no identity to
    /// report and reporting the planning placeholder would report an Order ID that will
    /// never exist. Every read and every committed mutation carries the record's own.
    order_id: Option<&'a str>,
    /// The Order itself: its paths, its record, its calculated totals, and its README text.
    order: &'a HealthyOrder,
    /// The Vendor this Order's record resolves to, as the core resolved it.
    vendor: Option<&'a Vendor>,
    /// Whether this Order's conventional README is presently there.
    ///
    /// Beside the text because the text alone cannot say: an absent README and an empty one
    /// both read as `""`. A read observes it; both mutations leave one there, because
    /// creation writes an empty conventional README and adoption preserves the candidate's
    /// own or creates an empty one where it has none.
    readme_exists: bool,
}

impl<'a> OrderFacts<'a> {
    /// One Order as a read of it found it.
    fn read(read: &'a OrderRead) -> Self {
        Self {
            order_id: Some(read.order().document().order_id.as_str()),
            order: read.order(),
            vendor: read.vendor(),
            readme_exists: read.readme().exists(),
        }
    }

    /// One Order as a plan would leave it, for the run that stops before the commit.
    ///
    /// The two constructors agreeing is not left to inspection:
    /// `tests/order_creation_and_adoption.rs` compares a dry run's Order against the real
    /// run's, so a fact one of them forgot is a failure rather than a silence.
    fn planned(order: &'a HealthyOrder, vendor: Option<&'a Vendor>) -> Self {
        Self {
            order_id: None,
            order,
            vendor,
            readme_exists: true,
        }
    }

    /// One Order as a committed mutation left it.
    fn committed(order: &'a HealthyOrder, vendor: Option<&'a Vendor>) -> Self {
        Self {
            order_id: Some(order.document().order_id.as_str()),
            order,
            vendor,
            readme_exists: true,
        }
    }

    /// One Order a mutation of an existing Order left, or would leave.
    ///
    /// **README existence is the caller's**, which is the whole difference from
    /// [`Self::committed`]: the two creating commands each leave a README behind, while
    /// these commands move whatever document was there with the directory or write only
    /// the record beside it — so what stood there before is what stands there after, and
    /// the read that targeted the Order is what observed it.
    fn mutated(order: &'a HealthyOrder, vendor: Option<&'a Vendor>, readme_exists: bool) -> Self {
        Self {
            order_id: Some(order.document().order_id.as_str()),
            order,
            vendor,
            readme_exists,
        }
    }
}

/// The `order show` payload: this Order, complete — and the refreshed result every Order
/// mutation returns beside its report.
///
/// Every member is the core's own answer. The adapter multiplies no Quantity, sums no
/// total, resolves no Vendor, and invents no diagnostic.
#[derive(Debug, Serialize)]
struct Show {
    paths: OrderPathsProjection,
    /// This record's own declared schema version, as the record declares it. The core's
    /// marker type serializes itself, so nothing here carries a version of its own for a
    /// fact the record owns.
    schema_version: Version1,
    /// The technical Package locator this record carries — the relative path from the
    /// Order Directory back to its Package Root, and the route this read took to get
    /// here. Reported because it is the fact a locator repair is about.
    package_root: String,
    /// See [`OrderFacts::order_id`], which is where the one `null` case is.
    order_id: Option<String>,
    title: String,
    date_ordered: String,
    status: &'static str,
    /// The Vendor this Order names, as the record carries it. `null` when it names none.
    vendor_id: Option<String>,
    /// That Vendor as the core resolved it, Hidden Vendors included — hiding governs what
    /// a Package offers for new use and never what is resolved, so an Order referencing a
    /// Hidden Vendor reports its facts here like any other. There is no `--all` on an
    /// `order` command and none is needed: this is a resolution, not a listing.
    vendor: Option<VendorSummary>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
    items: Vec<ItemProjection>,
    /// The Order Total the core calculated, unrounded. The two-decimal rule is about
    /// displayed money; a machine response carries the number the core summed.
    order_total: f64,
    /// See [`OrderFacts::readme_exists`].
    readme_exists: bool,
    /// The Order README's text. An absent README reads as `""` — absence is a text, there
    /// is nothing written — while `null` says this read does not know what the document
    /// holds, because it exists and could not be read. The diagnostics say which.
    readme_text: Option<String>,
}

/// One Order Item, at its one-based position.
///
/// **`position` is carried rather than left to the array index**, because one-based
/// positions are how every Item mutation addresses an Item — `order item edit 2`,
/// `order item move 2 position=1` — and an off-by-one between the number a reader sees
/// and the number a mutation takes is exactly the defect a contract should settle. The
/// order is the stored array's, which is user-controlled; nothing here sorts.
#[derive(Debug, Serialize)]
struct ItemProjection {
    position: usize,
    description: String,
    quantity: f64,
    unit_price: f64,
    catalog_number: Option<String>,
    /// The Item Total the core calculated. The adapter multiplies nothing.
    ///
    /// Every Item the core reports has one. `Option` only because the types cannot prove
    /// it — the core exposes the totals as a list beside the Items — and the alternatives
    /// are worse, exactly as they are for `package show`: asserting the lengths agree is a
    /// panic, which is an undecided fourth exit status, and substituting a number would
    /// invent a domain fact.
    item_total: Option<f64>,
}

/// The `order readme` payload: where this Order's conventional README is, and whether it
/// is presently there.
///
/// The same three facts `package readme` answers about a different document, which is why
/// the two are sibling contracts rather than one. The location comes from the core's own
/// `OrderPaths`; the adapter joins no `README.md` onto a directory, because which document
/// is conventional is a Package rule.
#[derive(Debug, Serialize)]
struct Readme {
    location: LocationProjection,
    exists: bool,
}

// ---------------------------------------------------------------------------
// The three commands.
// ---------------------------------------------------------------------------

pub(crate) fn show(target: &Target, output: &Output) -> Exit {
    present(
        ResponseContract::OrderShow,
        target,
        showing,
        human_show,
        output,
    )
}

/// The one projection of one Order, which the reads and both mutations share.
///
/// See [`ResponseContract::OrderShow`], which names the decision the sharing comes from,
/// and [`ResponseContract::AddOrder`], which names why a mutation carries this projection
/// under its own identity.
fn showing(read: &OrderRead) -> Result<Show, NotRepresentable> {
    Show::of(&OrderFacts::read(read))
}

impl Show {
    fn of(facts: &OrderFacts<'_>) -> Result<Self, NotRepresentable> {
        let order = facts.order;
        let document = order.document();
        Ok(Self {
            paths: OrderPathsProjection::of(order.paths())?,
            schema_version: document.schema_version,
            package_root: order.package_root().as_str().to_owned(),
            order_id: facts.order_id.map(str::to_owned),
            title: document.title.as_str().to_owned(),
            date_ordered: document.date_ordered.as_str().to_owned(),
            status: document.status.as_str(),
            vendor_id: document
                .vendor_id
                .as_ref()
                .map(|vendor_id| vendor_id.as_str().to_owned()),
            vendor: facts.vendor.map(VendorSummary::of),
            orderer: document.orderer.clone(),
            ordering_institution: document.ordering_institution.clone(),
            currency: document
                .currency
                .as_ref()
                .map(|code| code.as_str().to_owned()),
            project: document.project.clone(),
            order_group: document.order_group.clone(),
            items: items(order),
            order_total: order.order_total(),
            readme_exists: facts.readme_exists,
            readme_text: order.readme_text().map(str::to_owned),
        })
    }
}

/// `order item list` answers with `order show`'s response, rendered down to the Items.
///
/// It carries that contract's identity and emits its document byte for byte; the human
/// rendering below is the whole difference between the two command lines. See [`showing`].
pub(crate) fn item_list(target: &Target, output: &Output) -> Exit {
    present(
        ResponseContract::OrderShow,
        target,
        showing,
        |read, output| {
            // Which Order these Items belong to, which the machine response says in
            // `order_id`. Named here rather than in the shared Item lines because
            // `order show` already names it in its own summary, and one fact printed
            // twice reads as two.
            output.detail(
                "Order ID",
                OsStr::new(read.order().document().order_id.as_str()),
            )?;
            human_items(read.order(), output)
        },
        output,
    )
}

pub(crate) fn readme(target: &Target, output: &Output) -> Exit {
    present(
        ResponseContract::OrderReadme,
        target,
        |read| {
            Ok(Readme {
                location: LocationProjection::of(read.readme().reference().location())?,
                exists: read.readme().exists(),
            })
        },
        |read, output| {
            let location = read.readme().reference().location();
            // The path *is* the result, so it goes to standard output on its own line and
            // carries its own bytes — a human composes `cat`, `vi`, or another ordinary
            // tool with it.
            output.result_path(location.absolute_path())?;
            output.detail(
                "Package-relative path",
                OsStr::new(location.package_relative_path().unwrap_or("")),
            )?;
            output.detail(
                "Exists",
                &text(if read.readme().exists() { "yes" } else { "no" }),
            )
        },
        output,
    )
}

/// The Items of one Order, numbered from one in the stored array's own order.
fn items(order: &HealthyOrder) -> Vec<ItemProjection> {
    let totals = order.item_totals();
    order
        .document()
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| ItemProjection {
            position: index + 1,
            description: item.description.as_str().to_owned(),
            quantity: item.quantity.get(),
            unit_price: item.unit_price.get(),
            catalog_number: item.catalog_number.clone(),
            item_total: totals.get(index).copied(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Presenting, once for all three.
// ---------------------------------------------------------------------------

/// Presents one read of one Order.
///
/// **All three commands go through here**, so the envelope, the two-root context, the
/// exit status, which diagnostics the answer carries, the unrepresentable-path fallback,
/// and the undeliverable-result fallback are decided once for all three rather than three
/// times. Two of these commands disagreeing about a Damaged Order, about an empty Item
/// collection, or about whether an Order's diagnostics travel is the defect this ticket's
/// review boundary exists to catch, and this is the shape in which it cannot happen.
///
/// **The Order's own diagnostics travel in the envelope** for all three, because all three
/// report a projection of what one read of one Order observed, and the envelope is where a
/// response says what its read saw. An Order README that exists and is not a readable
/// document is the reachable case: the answer stands, and the observation stands beside it.
fn present<T: Serialize>(
    contract: ResponseContract,
    target: &Target,
    project: impl Fn(&OrderRead) -> Result<T, NotRepresentable>,
    human: impl Fn(&OrderRead, &Output) -> io::Result<()>,
    output: &Output,
) -> Exit {
    let answer = match read(target) {
        Ok(read) => Answer::Answered(Box::new(read)),
        Err(failed) => Answer::Failed(*failed),
    };
    if output.mode().is_json() {
        let (document, exit) = match document(contract, &answer, project) {
            Ok(built) => built,
            Err(NotRepresentable) => (not_representable(contract, context(&answer)), Exit::Failure),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_answer(&answer, human, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The two roots this answer was about, each named wherever it was established.
///
/// A successful read established both. A failure names what it reached: the Order
/// Directory for a stale locator and a Damaged Order, the Package for an Order ID that
/// resolved to nothing and for every Package-load state, and neither where the path the
/// caller gave was never resolved at all — the failure's own `paths` names that one, as
/// the caller's own text.
fn context(answer: &Answer) -> OrderContext {
    match answer {
        Answer::Answered(read) => OrderContext::rooted_at(
            read.context().root().absolute_path(),
            read.order().paths().directory().absolute_path(),
        ),
        Answer::Failed(failed) => failed_order_context(failed),
    }
}

/// The two roots one failed `order` command established, each named where it was.
///
/// Shared with the validating command, which reaches an Order the ordinary read cannot
/// and still owes the same two-root context for the same failures.
pub(crate) fn failed_order_context(failed: &OrderFailed) -> OrderContext {
    OrderContext::of(
        failed.failed.package_root.as_deref(),
        failed.order_root.as_deref(),
    )
}

/// The Order Directory a command line named, for the one command that must reach an
/// Order the ordinary read declines to.
///
/// **`order validate` is that command.** Every other `order` command reports a Damaged
/// Order as a failure, because a record that could not be read has no facts to report —
/// and judging exactly that Order is what validating one is for. So `--root` reaches the
/// core as the caller wrote it, and `--uuid` still resolves through the Package, where a
/// Damaged Order the load could identify is a resolution rather than a failure.
pub(crate) fn targeted_directory(target: &Target) -> Result<PathBuf, Box<OrderFailed>> {
    match target {
        Target::Directory(directory) => match link_named_as_a_root(directory, RootKind::Order) {
            Some(failed) => Err(Box::new(OrderFailed {
                failed,
                order_root: None,
            })),
            None => Ok(directory.to_path_buf()),
        },
        Target::Identity {
            package_root,
            order_id,
        } => match read_order_by_id(package_root, order_id, &PlatformFileSystem) {
            Ok(read) => Ok(read
                .order()
                .paths()
                .directory()
                .absolute_path()
                .to_path_buf()),
            Err(OrderReadFailure::DamagedOrder { paths, .. }) => {
                Ok(paths.directory().absolute_path().to_path_buf())
            }
            Err(reason) => Err(Box::new(failure(reason))),
        },
    }
}

fn document<T: Serialize>(
    contract: ResponseContract,
    answer: &Answer,
    project: impl Fn(&OrderRead) -> Result<T, NotRepresentable>,
) -> Result<(Response<T, OrderContext>, Exit), NotRepresentable> {
    match answer {
        Answer::Answered(read) => Ok((
            Response::answered(
                contract,
                context(answer),
                project(read)?,
                diagnostics(read.diagnostics())?,
            ),
            Exit::Success,
        )),
        Answer::Failed(failed) => Ok((
            failed_document(contract, context(answer), &failed.failed)?,
            Exit::Failure,
        )),
    }
}

/// The human counterpart, with the diagnostics routed to the stream each severity earns.
///
/// The two roots are `--verbose` detail for all three commands, because which Order and
/// which Package a read is of is the same detail for all of them — and the machine
/// response carries both in `context` in every mode.
fn human_answer(
    answer: &Answer,
    human: impl Fn(&OrderRead, &Output) -> io::Result<()>,
    output: &Output,
) -> io::Result<Exit> {
    match answer {
        Answer::Answered(read) => {
            human(read, output)?;
            output.detail(
                "Order Directory",
                read.order().paths().directory().absolute_path().as_os_str(),
            )?;
            output.detail(
                "Package Root",
                read.context().root().absolute_path().as_os_str(),
            )?;
            for diagnostic in read.diagnostics() {
                report_diagnostic(
                    output,
                    Some(read.context().root().absolute_path()),
                    diagnostic,
                )?;
            }
            Ok(Exit::Success)
        }
        Answer::Failed(failed) => {
            output.error(&failed.failed.message)?;
            for diagnostic in &failed.failed.diagnostics {
                report_diagnostic(output, failed.failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

/// A concise labelled summary naming the same Order the machine response carries.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. UUIDs appear only under `--verbose`: a human reading an Order wants
/// its title, its Vendor, and its Items, and the machine response carries the identities
/// always.
fn human_show(read: &OrderRead, output: &Output) -> io::Result<()> {
    output.summary("Order", &text(read.order().document().title.as_str()))?;
    human_order(&OrderFacts::read(read), output)
}

/// One Order's facts, its Items, and its totals, as every command that reports a whole
/// Order prints them.
///
/// **The subject line is the caller's**, which is the whole difference between a read and a
/// mutation on this surface: `Order` for the read, and what the mutation did for the two
/// that produce one. Everything below it is one rendering, so an Order read after a
/// creation cannot come to display differently from the creation that produced it.
fn human_order(facts: &OrderFacts<'_>, output: &Output) -> io::Result<()> {
    let order = facts.order;
    let document = order.document();
    output.summary("Date Ordered", &text(document.date_ordered.as_str()))?;
    output.summary("Status", &text(document.status.as_str()))?;
    output.summary(
        "Vendor",
        &text(&match facts.vendor {
            // Said because it is a fact about the Vendor a reader would otherwise have to
            // ask another command for: hiding governs what is offered for new use, and
            // this Order references it either way.
            Some(vendor) if vendor.is_hidden() => format!("{} (hidden)", vendor.name.as_str()),
            Some(vendor) => vendor.name.as_str().to_owned(),
            None => "none".to_owned(),
        }),
    )?;
    // A planned Order has no Order ID yet, and the subject line already says the mutation
    // was not carried out. A line reporting one would have to invent it.
    if let Some(order_id) = facts.order_id {
        output.detail("Order ID", OsStr::new(order_id))?;
    }
    output.detail(
        "Vendor ID",
        OsStr::new(
            document
                .vendor_id
                .as_ref()
                .map(|vendor_id| vendor_id.as_str())
                .unwrap_or("none"),
        ),
    )?;
    for (label, value) in [
        ("Orderer", document.orderer.as_deref()),
        (
            "Ordering institution",
            document.ordering_institution.as_deref(),
        ),
        (
            "Currency",
            document.currency.as_ref().map(|code| code.as_str()),
        ),
        ("Project", document.project.as_deref()),
        ("Order group", document.order_group.as_deref()),
    ] {
        output.summary(label, &text(value.unwrap_or("none")))?;
    }
    output.summary(
        "README",
        &text(match (facts.readme_exists, order.readme_text()) {
            (false, _) => "absent",
            (true, Some(_)) => "present",
            (true, None) => "present but unreadable",
        }),
    )?;
    human_items(order, output)
}

/// One Order's Items, numbered from one, as both commands that show them print it.
///
/// Shared rather than written twice, so `order show` and `order item list` cannot come to
/// number or total one Order's Items differently — a disagreement no reader would notice
/// until they ran both.
///
/// The Unit Price, the Item Total, and the Order Total are rendered by the one thing that
/// renders money in this crate; the Quantity beside them is not money and is shown as the
/// record stores it. The numbers themselves are untouched either way: the machine
/// responses carry the core's unrounded calculations.
fn human_items(order: &HealthyOrder, output: &Output) -> io::Result<()> {
    let totals = order.item_totals();
    if order.document().items.is_empty() {
        output.summary("Items", &text("none"))?;
    }
    for (index, item) in order.document().items.iter().enumerate() {
        output.summary(
            "Item",
            &text(&format!(
                "{}. {}  {} x {}  {}",
                index + 1,
                item.description.as_str(),
                displayed_quantity(item.quantity.get()),
                displayed_amount(item.unit_price.get()),
                match totals.get(index) {
                    Some(total) => displayed_amount(*total),
                    None => "unknown".to_owned(),
                }
            )),
        )?;
        output.detail(
            "Catalog number",
            OsStr::new(item.catalog_number.as_deref().unwrap_or("none")),
        )?;
    }
    output.summary("Order Total", &text(&displayed_amount(order.order_total())))
}

/// One Order Item's Quantity, as the record stores it.
///
/// **Deliberately not [`displayed_amount`]**, which exists to give a monetary value two
/// decimal places. A Quantity is a count of things and has no such convention: sent
/// through the monetary renderer, `0.005` of a kilogram reads as `0.01` and `1.005` metres
/// reads as `1.00`, and a reader is told nothing was rounded. Nothing here rounds, so what
/// is displayed is what the record holds.
fn displayed_quantity(value: f64) -> String {
    value.to_string()
}

// ---------------------------------------------------------------------------
// The two mutations that give an Order Directory its first record.
// ---------------------------------------------------------------------------

/// The payload of both mutations: what the mutation did, and the Order it leaves behind.
///
/// The two members the mutation contract calls for — the typed report, and the refreshed
/// affected result. The refreshed result is [`Show`], the same projection the reads render,
/// so a client that can display an Order can display the one a mutation returns without a
/// second read and without a second shape to learn.
#[derive(Debug, Serialize)]
struct Mutated {
    report: Report,
    order: Show,
}

/// The typed mutation report, in the shape `add-vendor` established.
///
/// `committed` is whether the plan was carried out, and it is the one member `--dry-run`
/// decides. `changed` is whether the mutation changes the Package's canonical records at
/// all — always true here, because both of these commands write an Order record that did
/// not exist, so neither has an idempotent no-op — and it is carried because the outcome
/// vocabulary is one vocabulary across every mutating command rather than a shape each
/// invents.
///
/// **`outcome` is the canonical half of "canonical versus derived" and `index` is the
/// derived half.** A committed canonical mutation whose index refresh failed says so here
/// and in `warnings`, and still exits `0` — the record was written, and re-running the
/// mutation is not the action, so nothing about this report is retry-shaped.
///
/// **`reusable_values` is the reuse-versus-introduction report** for the four Reusable
/// Free-Entry Value categories: an assignment matching a spelling this Package already uses
/// reuses that canonical spelling, and a genuinely new value is introduced — which a CLI
/// assignment is itself the confirmation of.
///
/// **There are no old-and-new paths.** Creation has no old path, and a candidate is adopted
/// where it stands: neither command moves anything, and `order align` is what renames an
/// Order Directory. A member with nothing to say would be a member every consumer had to
/// learn to ignore.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    outcome: &'static str,
    reusable_values: ReusableValues,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every mutation whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// What this mutation did with each of the four Reusable Free-Entry Value categories.
///
/// All four always, because the answer for a category the command line said nothing about
/// is that this Order carries no value from it — which is a fact, and one a caller reading
/// four keys does not have to reconstruct from an absence.
#[derive(Clone, Debug, Serialize)]
struct ReusableValues {
    ordering_institution: ReusableValueProjection,
    orderer: ReusableValueProjection,
    project: ReusableValueProjection,
    order_group: ReusableValueProjection,
}

/// One Reusable Free-Entry Value category's outcome, and the spelling it settled on.
///
/// `value` is the canonical spelling: for a reused value it is the one this Package already
/// uses, which is what makes "naming the canonical spelling a reused value matched"
/// observable rather than implied — a caller that wrote `northwind` and reused `Northwind` is told
/// which. It is `null` exactly when the category is absent.
#[derive(Clone, Debug, Serialize)]
struct ReusableValueProjection {
    outcome: &'static str,
    value: Option<String>,
}

impl ReusableValues {
    /// The four categories one Order-record mutation settled, in the published order.
    ///
    /// A constructor rather than four inline fields per command, so a category cannot come
    /// to be reported under a neighbour's name by one of them.
    fn of(values: &OrderEditReusableValues) -> Self {
        Self {
            ordering_institution: ReusableValueProjection::of(values.ordering_institution()),
            orderer: ReusableValueProjection::of(values.orderer()),
            project: ReusableValueProjection::of(values.project()),
            order_group: ReusableValueProjection::of(values.order_group()),
        }
    }

    /// The four categories with the label each is reported under, in the published order.
    ///
    /// One list rather than one per surface, so the human lines and the machine members
    /// cannot come to disagree about which four categories there are or what order they
    /// are in.
    fn reported(&self) -> [(&'static str, &ReusableValueProjection); 4] {
        [
            ("Ordering institution", &self.ordering_institution),
            ("Orderer", &self.orderer),
            ("Project", &self.project),
            ("Order group", &self.order_group),
        ]
    }
}

impl ReusableValueProjection {
    fn of(value: &ReusableValue) -> Self {
        match value {
            ReusableValue::Absent => Self {
                outcome: "absent",
                value: None,
            },
            ReusableValue::Reused(value) => Self {
                outcome: "reused",
                value: Some(value.clone()),
            },
            ReusableValue::New(value) => Self {
                outcome: "introduced",
                value: Some(value.clone()),
            },
        }
    }
}

/// One planned managed operation, in the core's own vocabulary.
#[derive(Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    location: LocationProjection,
}

/// What one Order mutation decided, before anything is written anywhere.
///
/// Deciding and presenting are separate so that the four output modes present one answer
/// rather than each recomputing it — the shape in which two modes come to disagree about
/// the same run.
enum Mutation {
    Planned(Box<Planned>),
    Failed(Box<Failed>),
}

/// One decided Order mutation, in the terms every output mode reports it in.
///
/// **Both commands are normalized into this one shape**, which is why there is one
/// presentation rather than two: creating an Order and adopting a directory as one produce
/// different plans and different results, and everything a response says about either is
/// the same set of facts. Two presentations agreeing would be a coincidence to maintain.
struct Planned {
    root: PathBuf,
    kind: Kind,
    /// The Order this plan would leave, and the Vendor its reference resolves to. What a
    /// run that stops before the commit reports.
    order: HealthyOrder,
    vendor: Option<Vendor>,
    outcome: &'static str,
    reusable_values: ReusableValues,
    operations: Vec<(&'static str, Location)>,
    prior_index: PackageIndexRecord,
    committed: Option<Committed>,
}

/// Which of the two commands this run was.
///
/// **A discriminator of its own rather than a reading of `outcome`.** `outcome` is the
/// core's own `as_str` spelling, and a surface that re-dispatched on that text would go
/// silently wrong if the core renamed it — no compile error, just a human line naming the
/// wrong command. The one thing this crate decides for itself gets its own type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Created,
    Adopted,
}

impl Kind {
    /// The one line that says which outcome this run reached.
    ///
    /// A dry run says what it *would* do, because a reader who skimmed one line of a plan
    /// and took it for a result is the whole hazard of reporting a plan in the same shape as
    /// a result. There is no unchanged case: both commands write a record that did not
    /// exist.
    fn label(self, committed: bool) -> &'static str {
        match (self, committed) {
            (Self::Created, true) => "Created",
            (Self::Created, false) => "Would create",
            (Self::Adopted, true) => "Adopted",
            (Self::Adopted, false) => "Would adopt",
        }
    }
}

/// What committing one plan produced.
struct Committed {
    order: HealthyOrder,
    vendor: Option<Vendor>,
    index: PackageIndexRecord,
    /// Each warning the commit carried, in the core's own order: its stable spelling, and
    /// the human sentence for it. A committed mutation with a warning is still committed.
    ///
    /// **The sentence is built where the typed warning is still in hand**, so a warning the
    /// core adds is a compile error rather than a sentence about the wrong thing — the
    /// property matching on a `&str` here would give up.
    warnings: Vec<Warning>,
}

/// One warning a commit carried: what a machine reads, and what a human reads.
///
/// The sentence is owned rather than borrowed because one of them names a repair command,
/// and such a sentence is composed against what the core authorizes rather than written
/// out here — see [`crate::commands::repair_suggestion`].
#[derive(Clone, Debug)]
struct Warning {
    code: &'static str,
    sentence: String,
}

/// The stable spelling of each warning a commit carried, which is what a consumer
/// branches on.
///
/// The human sentence beside it in [`Warning`] is not a parsing contract and stays out of
/// the document. Shared by every mutation in this module, so a warning cannot come to be
/// spelled one way by the command that creates an Order and another by the command that
/// moves one.
fn warning_codes(warnings: &[Warning]) -> Vec<&'static str> {
    warnings.iter().map(|warning| warning.code).collect()
}

/// The sentence for one creation warning, saying what did *not* happen rather than what did.
///
/// A committed mutation is committed: each sentence points at the command that puts the
/// derived state right, never at running this one again, because re-running it would be the
/// retry a report must not encourage.
fn creation_warning(warning: &OrderCreationWarning) -> Warning {
    Warning {
        code: warning.as_str(),
        sentence: match warning {
            OrderCreationWarning::IndexRefreshFailed(_)
            | OrderCreationWarning::RefreshUnavailable => STALE_INDEX.to_owned(),
        },
    }
}

/// The same, for one adoption warning. Adoption has one warning creation does not: it
/// stages beside the candidate's own documents, so a stage it could not remove again
/// outlives the commit.
fn adoption_warning<E>(warning: &OrderAdoptionWarning<E>) -> Warning {
    Warning {
        code: warning.as_str(),
        sentence: match warning {
            // The stage stands inside the saved Order's own reserved directory, so the
            // finding is one Order's and the command that answers for it is that Order's.
            OrderAdoptionWarning::StagingCleanupFailed(_) => format!(
                "the Order was saved and a staging artifact this adoption created could not \
                 be removed again; `spielorders validate` reports it and {}",
                RepairSuggestion::for_condition(
                    Rooting::Package,
                    DiagnosticCode::StagingArtifactPresent,
                    EvidenceScope::Order,
                )
            ),
            OrderAdoptionWarning::IndexRefreshFailed(_)
            | OrderAdoptionWarning::RefreshUnavailable => STALE_INDEX.to_owned(),
        },
    }
}

const STALE_INDEX: &str = "the Order was saved and the derived Package Index was not refreshed, so it is stale. \
     `spielorders index rebuild` replaces it.";

impl Planned {
    /// The Order this run has to report, and the derived index it leaves behind.
    ///
    /// **One selection rather than one per surface.** Which of the plan and the commit
    /// answers each of those two questions is the same answer in every output mode, and two
    /// modes deciding it separately is exactly the disagreement [`Mutation`] exists to
    /// prevent.
    fn answered(&self) -> (OrderFacts<'_>, &PackageIndexRecord) {
        match &self.committed {
            Some(committed) => (
                OrderFacts::committed(&committed.order, committed.vendor.as_ref()),
                &committed.index,
            ),
            None => (
                OrderFacts::planned(&self.order, self.vendor.as_ref()),
                &self.prior_index,
            ),
        }
    }
}

/// `add-order`: the one Order command whose subject does not exist yet.
///
/// The assignment set is the whole input, because the Title shorthand is expanded into one
/// — see [`crate::cli::subject_assignments`]. Package-rooted, because there is no Order
/// Directory to name yet: where it goes is the Package's own configured Orders Directory.
pub(crate) fn add(
    selected_root: &Path,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_package(selected_root) {
        Ok(context) => created(selected_root, assignments, dry_run, &context),
        Err(failed) => Mutation::Failed(failed),
    };
    present_mutation(ResponseContract::AddOrder, mutation, output)
}

/// `adopt-order`: one existing directory brought under management where it stands.
///
/// **Nothing here reads the candidate.** Its name is not parsed, its contents are not
/// inspected, and neither is consulted for a default: the Order's facts arrive as
/// assignments, and whether the directory is eligible is the core's judgement. So a
/// candidate whose name embeds an impossible calendar date is adopted with the date the
/// caller supplied, uncorrected.
pub(crate) fn adopt(
    selected_root: &Path,
    directory: &Path,
    assignments: &[FieldAssignment],
    items: &[Vec<FieldAssignment>],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_package(selected_root) {
        Ok(context) => adopted(
            selected_root,
            directory,
            assignments,
            items,
            dry_run,
            &context,
        ),
        Err(failed) => Mutation::Failed(failed),
    };
    present_mutation(ResponseContract::AdoptOrder, mutation, output)
}

/// The Package this mutation is about, or the typed reason there is none.
///
/// Asked before the plan so that every refusal below can name the Root it happened under
/// and report that Package's own diagnosis: the core's creation and adoption refusals are
/// about a candidate and a field rather than about a Package's health, and a response that
/// named no Package would be dropping a fact this command holds.
fn resolved_package(selected_root: &Path) -> Result<ManagedContext, Box<Failed>> {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return Err(Box::new(failed));
    }
    package_candidate(
        selected_root,
        inspect_canonical_package(selected_root, &PlatformFileSystem),
    )
    .map(|package| package.context().clone())
}

/// The assignment set as the core takes it, in the order the command line supplied.
///
/// Nothing here inspects a field or a value. A repeated field, an unknown one, an immutable
/// one, a technical one, and a blank required one are all the core's refusals, reported
/// together — see [`spielorders_core::order_edits::OrderAssignments`].
///
/// **Every assignment is confirmed**, because a CLI assignment of a genuinely new Reusable
/// Free-Entry Value is itself the explicit confirmation the decision requires. Which four
/// fields that applies to stays in the core, which is what
/// [`OrderAssignments::confirm`] is for: an adapter choosing between `assign` and
/// `introduce` per field would be keeping a second copy of a Package rule.
fn assigned(assignments: &[FieldAssignment]) -> OrderAssignments {
    assignments
        .iter()
        .fold(OrderAssignments::new(), |set, assignment| {
            set.confirm(assignment.field.clone(), assignment.value.clone())
        })
}

/// Plans a creation, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a write
/// path: a run that was asked to plan returns here, so no directory, no staging file, and
/// no index refresh exists afterward.
fn created(
    selected_root: &Path,
    assignments: &[FieldAssignment],
    dry_run: bool,
    context: &ManagedContext,
) -> Mutation {
    let input = match OrderCreationInput::from_assignments(&assigned(assignments)) {
        Ok(input) => input,
        Err(failure) => return Mutation::Failed(Box::new(assignment_refusal(&failure, context))),
    };
    let plan = match plan_order_creation(
        selected_root,
        &input,
        &PlatformCalendar,
        &PlatformFileSystem,
    ) {
        Ok(plan) => plan,
        Err(reason) => return Mutation::Failed(Box::new(creation_refusal(reason, context))),
    };
    let planned = Planned {
        root: plan.root().to_path_buf(),
        kind: Kind::Created,
        order: plan.projected_order().clone(),
        vendor: plan.vendor().cloned(),
        outcome: plan.canonical_outcome().as_str(),
        reusable_values: ReusableValues {
            ordering_institution: ReusableValueProjection::of(
                plan.reusable_values().ordering_institution(),
            ),
            orderer: ReusableValueProjection::of(plan.reusable_values().orderer()),
            project: ReusableValueProjection::of(plan.reusable_values().project()),
            order_group: ReusableValueProjection::of(plan.reusable_values().order_group()),
        },
        operations: plan
            .operations()
            .iter()
            .map(|operation| (operation.kind().as_str(), operation.location().clone()))
            .collect(),
        prior_index: plan.prior_index().clone(),
        committed: None,
    };
    if dry_run {
        return Mutation::Planned(Box::new(planned));
    }
    match commit_order_creation(plan, &mut PlatformUuids, &PlatformFileSystem) {
        // `Unchanged` beside `Committed` because both say the commit succeeded, and what
        // separates them is `changed` — which the report reads off the plan.
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            Mutation::Planned(Box::new(planned.committed(
                result.order(),
                result.vendor(),
                result.index(),
                Vec::new(),
            )))
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => Mutation::Planned(Box::new(planned.committed(
            result.order(),
            result.vendor(),
            result.index(),
            warnings.iter().map(creation_warning).collect(),
        ))),
        MutationReport::Rejected { plan, rejection } => {
            Mutation::Failed(Box::new(creation_commit_refused(context, &plan, rejection)))
        }
    }
}

impl Planned {
    /// The same plan, carrying what committing it produced.
    ///
    /// One constructor for both commands, because a commit produces the same three facts
    /// either way: the Order it left, the Vendor that Order resolves, and the derived index
    /// afterwards.
    fn committed(
        mut self,
        order: &HealthyOrder,
        vendor: Option<&Vendor>,
        index: &PackageIndexRecord,
        warnings: Vec<Warning>,
    ) -> Self {
        self.committed = Some(Committed {
            order: order.clone(),
            vendor: vendor.cloned(),
            index: index.clone(),
            warnings,
        });
        self
    }
}

/// Plans an adoption, and commits unless this run was asked to plan only.
///
/// [`created`]'s counterpart, and the only place an adoption commit is called — so a
/// `--dry-run` here is the same absence of a write path it is there.
fn adopted(
    selected_root: &Path,
    directory: &Path,
    assignments: &[FieldAssignment],
    items: &[Vec<FieldAssignment>],
    dry_run: bool,
    context: &ManagedContext,
) -> Mutation {
    let mut input = match OrderAdoptionInput::from_assignments(&assigned(assignments)) {
        Ok(input) => input,
        Err(failure) => return Mutation::Failed(Box::new(assignment_refusal(&failure, context))),
    };
    // **The only mutation that may carry Items at creation**, because an Order created from
    // nothing starts with none. Each group reaches the core as one Item-assignment set, in
    // the order the command line wrote them, and the core adjudicates each against the one
    // list of Item fields every Item command uses.
    for (index, item) in items.iter().enumerate() {
        match OrderItemInput::from_assignments(&item_assigned(item)) {
            Ok(item) => input = input.item(item),
            Err(failure) => {
                return Mutation::Failed(Box::new(item_assignment_refusal(
                    &failure,
                    index + 1,
                    context,
                )));
            }
        }
    }
    let plan = match plan_order_adoption(selected_root, directory, &input, &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => return Mutation::Failed(Box::new(adoption_refusal(reason, context))),
    };
    let planned = Planned {
        root: plan.root().to_path_buf(),
        kind: Kind::Adopted,
        order: plan.projected_order().clone(),
        vendor: plan.vendor().cloned(),
        outcome: plan.canonical_outcome().as_str(),
        reusable_values: ReusableValues {
            ordering_institution: ReusableValueProjection::of(
                plan.reusable_values().ordering_institution(),
            ),
            orderer: ReusableValueProjection::of(plan.reusable_values().orderer()),
            project: ReusableValueProjection::of(plan.reusable_values().project()),
            order_group: ReusableValueProjection::of(plan.reusable_values().order_group()),
        },
        operations: plan
            .operations()
            .iter()
            .map(|operation| (operation.kind().as_str(), operation.location().clone()))
            .collect(),
        prior_index: plan.prior_index().clone(),
        committed: None,
    };
    if dry_run {
        return Mutation::Planned(Box::new(planned));
    }
    match commit_order_adoption(plan, &mut PlatformUuids, &PlatformFileSystem) {
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            Mutation::Planned(Box::new(planned.committed(
                result.order(),
                result.vendor(),
                result.index(),
                Vec::new(),
            )))
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => Mutation::Planned(Box::new(planned.committed(
            result.order(),
            result.vendor(),
            result.index(),
            warnings.iter().map(adoption_warning).collect(),
        ))),
        MutationReport::Rejected { plan, rejection } => {
            Mutation::Failed(Box::new(adoption_commit_refused(context, &plan, rejection)))
        }
    }
}

// ---------------------------------------------------------------------------
// The refusals.
// ---------------------------------------------------------------------------

/// Why the assignment set named no Order, as one typed failure.
///
/// Shared by both commands, because the assignment language is one language: what differs
/// is which facts each requires, and the core's own failure carries that. So a refused
/// field is refused identically whichever command supplied it.
fn assignment_refusal(failure: &NewOrderAssignmentFailure, context: &ManagedContext) -> Failed {
    match failure {
        NewOrderAssignmentFailure::InvalidAssignments(rejected) => refused(
            FailureCode::OrderAssignmentRejected,
            format!(
                "no assignment was applied, because an assignment set is one atomic \
                 mutation and {} refused: {}.",
                if rejected.len() == 1 {
                    "one of them was"
                } else {
                    "some of them were"
                },
                listed(rejected)
            ),
            context,
            rejected
                .iter()
                .map(|assignment| {
                    Identity::rejected_assignment(assignment.field(), assignment.reason())
                })
                .collect(),
        ),
        NewOrderAssignmentFailure::MissingRequiredFacts { fields } => refused(
            FailureCode::OrderFactsRequired,
            format!(
                "this command requires {} and the command line assigned {}, so nothing was \
                 written. Nothing reads an Order's facts off a directory name.",
                named_fields(fields),
                if fields.len() == 1 {
                    "none"
                } else {
                    "none of them"
                }
            ),
            context,
            fields
                .iter()
                .map(|field| Identity::required_fact(field))
                .collect(),
        ),
        // The same refusal an edit reports for the same mistake, from the one place all
        // three of these facts are refused — see [`invalid_fact`].
        NewOrderAssignmentFailure::InvalidFactValue { fact, value } => {
            let (code, message, identity) = invalid_fact(*fact, value);
            refused(code, message, context, vec![identity])
        }
    }
}

/// The same, for an assignment set that named no Order Item.
///
/// **Two arms, and both differ from the Order's own.** A refused assignment is refused
/// about the Item rather than about the Order, so the sentence names the Item's fields;
/// and a missing required fact is the Item's, so a code and a sentence about an *Order's*
/// required facts would send a caller to the Order's own assignments over an Item they can
/// simply describe. Only adoption reaches this: the four Item commands are refused by the
/// capability that has a collection to address, through [`edit_refusal`].
///
/// **The sentence says which Item**, counting the `item` groups of the command line from
/// one, because an adoption may carry several and the refusal is about one of them. It is a
/// sentence rather than an identity: the adopted Order has no collection yet, so the number
/// names a position on the command line rather than one in an Item collection.
fn item_assignment_refusal(
    failure: &NewItemAssignmentFailure,
    position: usize,
    context: &ManagedContext,
) -> Failed {
    match failure {
        NewItemAssignmentFailure::InvalidAssignments(rejected) => refused(
            FailureCode::OrderAssignmentRejected,
            format!(
                "no assignment was applied, because an assignment set is one atomic \
                 mutation and {} of Order Item {position}'s was refused: {}.",
                if rejected.len() == 1 { "one" } else { "some" },
                listed(rejected)
            ),
            context,
            rejected
                .iter()
                .map(|assignment| {
                    Identity::rejected_assignment(assignment.field(), assignment.reason())
                })
                .collect(),
        ),
        NewItemAssignmentFailure::MissingRequiredFacts { fields } => refused(
            FailureCode::OrderItemFactsRequired,
            format!("Order Item {position}: {}", missing_item_facts(fields)),
            context,
            fields
                .iter()
                .map(|field| Identity::required_fact(field))
                .collect(),
        ),
        // Not reachable through this adapter: every accepted assignment is already a value
        // the record's own type accepted, and the required facts were found. It carries the
        // code an assembled record that did not validate carries, which is what it is.
        NewItemAssignmentFailure::InvalidFacts => refused(
            FailureCode::OrderFactsInvalid,
            format!(
                "the Order Item {position} this assignment set assembled is not one the \
                 record can hold, so nothing was written."
            ),
            context,
            Vec::new(),
        ),
    }
}

/// The sentence an Item assigned none of its required facts earns.
///
/// One home for it, because two capabilities refuse that: adoption, which has no collection
/// yet, and the four Item commands, which have one. A caller is told the same thing either
/// way.
fn missing_item_facts(fields: &[String]) -> String {
    format!(
        "an Order Item requires {} and this command line assigned {}, so nothing was \
         written. Its Catalog Number is optional, and its totals are calculated rather \
         than assigned.",
        named_fields(fields),
        if fields.len() == 1 {
            "none"
        } else {
            "none of them"
        }
    )
}

/// Every refused assignment of one set, in the order the command line supplied them.
///
/// Each names the field and the rule it broke, in the core's own spelling — the same
/// spelling the response carries as an identity, so a human reading this sentence and a
/// script reading the document are told the same thing.
fn listed(rejected: &[RejectedAssignment]) -> String {
    rejected
        .iter()
        .map(|assignment| {
            format!(
                "`{}` ({})",
                assignment.field(),
                assignment.reason().as_str()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One list of field names, as a sentence names them.
fn named_fields(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| format!("`{field}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why the creation was refused before any write, as one typed failure.
///
/// **Total over the core's own reasons**, so a reason added there is a compile error here
/// rather than a mutation refused under a code that means something else. The four
/// Package-load states are reachable only between [`resolved_package`] and this plan, which
/// is the same race the commit refuses — so they carry the code a changed precondition
/// carries, and this Package's own diagnosis reached the caller through
/// [`resolved_package`] when the Package was already untrustworthy.
fn creation_refusal(reason: OrderCreationPlanError<io::Error>, context: &ManagedContext) -> Failed {
    match reason {
        OrderCreationPlanError::UnmanagedPackage
        | OrderCreationPlanError::DamagedPackage
        | OrderCreationPlanError::UpgradePending
        | OrderCreationPlanError::PackageOperationFailed => preconditions_changed(context),
        OrderCreationPlanError::InvalidFacts => invalid_facts(context),
        OrderCreationPlanError::UnknownVendor { name } => unknown_vendor(&name, context),
        OrderCreationPlanError::EmptyProjectedComponent => refused(
            FailureCode::OrderTitleUnusable,
            "this Order Title projects no usable Order Directory name, so there is nothing \
             to call the directory this creation would make. A Title made only of \
             separators and whitespace is what reaches this."
                .to_owned(),
            context,
            Vec::new(),
        ),
        OrderCreationPlanError::DestinationCollision { location } => mutation_failed(
            FailureCode::OrderDirectoryOccupied,
            format!(
                "{} already exists, so this Order Directory cannot be created there. It is \
                 refused rather than given an invented suffix; `spielorders order align` \
                 is what renames an Order Directory.",
                location.absolute_path().display()
            ),
            context.root().absolute_path(),
            vec![in_package(&location)],
            Vec::new(),
        ),
        OrderCreationPlanError::FileSystem { path, error } => {
            filesystem_refusal(path, &error.to_string(), context)
        }
    }
}

/// Why the adoption was refused before any write, as one typed failure.
///
/// **Every candidate refusal has its own code**, because each is a different thing to do
/// about it: a link is never followed, a hidden entry is outside discovery, a name with no
/// Package-relative spelling can never be referred to, an already managed directory is
/// already an Order, and reserved state that is not a readable record is left for
/// `validate` and explicit human disposition. Each leaves the candidate exactly as it is.
fn adoption_refusal(reason: OrderAdoptionPlanError<io::Error>, context: &ManagedContext) -> Failed {
    let orders = context
        .orders_directory()
        .map(|orders| orders.absolute_path().display().to_string())
        .unwrap_or_else(|| "this Package's Orders Directory".to_owned());
    match reason {
        OrderAdoptionPlanError::UnmanagedPackage
        | OrderAdoptionPlanError::DamagedPackage
        | OrderAdoptionPlanError::UpgradePending
        | OrderAdoptionPlanError::PackageOperationFailed => preconditions_changed(context),
        OrderAdoptionPlanError::NotDirectChild { location } => candidate_refused(
            FailureCode::OrderCandidateNotADirectChild,
            format!(
                "{} is not a direct child of {orders}, so it is not one of this Package's \
                 Orders. A candidate is adopted where it stands and is never moved to \
                 become one.",
                location.absolute_path().display()
            ),
            in_package(&location),
            context,
        ),
        OrderAdoptionPlanError::HiddenCandidate { path } => candidate_refused(
            FailureCode::OrderCandidateHidden,
            format!(
                "{} is a hidden entry, which this Package's discovery rules exclude, so it \
                 cannot be adopted as an Order.",
                path.display()
            ),
            // No Package surrounds it: this is refused on the caller's own selected
            // path, before anything resolved it or placed it inside a Package.
            resolved(&path),
            context,
        ),
        OrderAdoptionPlanError::IneligibleCandidate { path, kind } => {
            let (code, message) = match kind {
                Some(FileSystemEntryKind::SymbolicLink) => (
                    FailureCode::OrderCandidateLink,
                    format!(
                        "{} is a link. Links are never followed and never adopted, so an \
                         alias view of an Order Directory cannot become an Order.",
                        path.display()
                    ),
                ),
                observed => (
                    FailureCode::OrderCandidateIneligible,
                    format!(
                        "{} is not an ordinary directory ({}), so there is nothing there to \
                         adopt as an Order.",
                        path.display(),
                        match observed {
                            Some(kind) => kind.as_str(),
                            None => "nothing stands there",
                        }
                    ),
                ),
            };
            // No Package surrounds it, for the reason the hidden candidate above gives:
            // this is refused before eligibility, on the caller's own selected path.
            candidate_refused(code, message, resolved(&path), context)
        }
        OrderAdoptionPlanError::NameUnusableCandidate { location } => candidate_refused(
            FailureCode::OrderCandidateNameUnusable,
            format!(
                "{} has no name this Package can express, so nothing describing this \
                 Package could refer to an Order there. It is not indexed and is left \
                 exactly as it is.",
                location.absolute_path().display()
            ),
            in_package(&location),
            context,
        ),
        OrderAdoptionPlanError::AlreadyManaged { location } => candidate_refused(
            FailureCode::OrderCandidateAlreadyManaged,
            format!(
                "{} is already an Order record, so this directory is already one of this \
                 Package's Orders. Nothing was overwritten; `spielorders order edit` \
                 changes an Order's facts.",
                location.absolute_path().display()
            ),
            in_package(&location),
            context,
        ),
        OrderAdoptionPlanError::ConflictingReservedState { location } => candidate_refused(
            FailureCode::OrderCandidateReservedStateConflict,
            format!(
                "{} holds reserved Package metadata that is not a readable Order record, so \
                 this candidate cannot safely be adopted or told apart from a partial \
                 earlier one. It is left untouched; `spielorders validate` reports it.",
                location.absolute_path().display()
            ),
            in_package(&location),
            context,
        ),
        OrderAdoptionPlanError::StagingCollision { location } => candidate_refused(
            FailureCode::OrderCandidateStagingOccupied,
            format!(
                // **No command is named, because none would answer.** A candidate whose
                // reserved directory exists at all is refused before this check, so the
                // only staging name that reaches it is the one beside the conventional
                // README — which stands at the Directory's own root rather than inside
                // `.spielorders`, and the walk that diagnoses staging artifacts covers
                // only the reserved directories. Nothing diagnoses this file, so nothing
                // repairs it either.
                "{} is a staging name this adoption needs and something already stands \
                 there, which is an interrupted earlier write. The candidate is left \
                 untouched, and nothing here diagnoses or removes that file — moving it \
                 aside is what lets the adoption proceed.",
                location.absolute_path().display()
            ),
            in_package(&location),
            context,
        ),
        OrderAdoptionPlanError::InvalidFacts => invalid_facts(context),
        OrderAdoptionPlanError::UnknownVendor { name } => unknown_vendor(&name, context),
        OrderAdoptionPlanError::FileSystem { path, error } => {
            filesystem_refusal(path, &error.to_string(), context)
        }
    }
}

/// Why the creation's commit was refused, as one typed failure.
///
/// **The Package rather than the plan's root path**, which is the shape every commit
/// refusal takes here: the Package this command resolved is what the response's context
/// names, and taking it whole is what stops a flattened root from standing in for a
/// located one.
fn creation_commit_refused(
    context: &ManagedContext,
    plan: &OrderCreationPlan,
    rejection: OrderCreationRejection<io::Error>,
) -> Failed {
    match rejection {
        OrderCreationRejection::PreconditionsChanged { location } => changed_between(
            context,
            &location,
            "so what would have been written is no longer what was validated",
        ),
        OrderCreationRejection::PlanEvidenceUnconfirmed { .. } => changed_between(
            context,
            context.root(),
            "so the facts this creation was planned against no longer stand",
        ),
        OrderCreationRejection::InvalidGeneratedIdentity { generated } => mutation_failed(
            FailureCode::OrderIdentityInvalid,
            format!("`{generated}` is not a usable Order ID, so this Order was not created."),
            context.root().absolute_path(),
            vec![in_package(plan.projected_order().paths().directory())],
            Vec::new(),
        ),
        OrderCreationRejection::GeneratedIdentityCollision { order_id } => mutation_failed(
            FailureCode::OrderIdentityCollision,
            "the identity minted for this Order is already carried by an Order of this \
             Package, so nothing was written and nothing was overwritten."
                .to_owned(),
            context.root().absolute_path(),
            vec![in_package(plan.projected_order().paths().directory())],
            vec![Identity::order(&order_id)],
        ),
        OrderCreationRejection::CommitFailed {
            failure,
            cleanup_failures,
        } => {
            let remaining: Vec<FailedPath> = cleanup_failures
                .iter()
                .map(|cleanup| in_package(cleanup.location()))
                .collect();
            commit_failed(
                context,
                failure.location(),
                failure.phase().as_str(),
                remaining,
                "the Order Directory this creation staged",
            )
        }
    }
}

/// Why the adoption's commit was refused, as one typed failure.
///
/// The Package rather than the plan's root path, for the reason
/// [`creation_commit_refused`] takes it.
fn adoption_commit_refused(
    context: &ManagedContext,
    plan: &OrderAdoptionPlan,
    rejection: OrderAdoptionRejection<io::Error>,
) -> Failed {
    match rejection {
        OrderAdoptionRejection::PreconditionsChanged { location } => changed_between(
            context,
            &location,
            "so what would have been written is no longer what was validated",
        ),
        OrderAdoptionRejection::InvalidGeneratedIdentity { generated } => mutation_failed(
            FailureCode::OrderIdentityInvalid,
            format!("`{generated}` is not a usable Order ID, so this directory was not adopted."),
            context.root().absolute_path(),
            vec![in_package(plan.projected_order().paths().directory())],
            Vec::new(),
        ),
        OrderAdoptionRejection::GeneratedIdentityCollision { order_id } => mutation_failed(
            FailureCode::OrderIdentityCollision,
            "the identity minted for this Order is already carried by an Order of this \
             Package, so nothing was written and nothing was overwritten."
                .to_owned(),
            context.root().absolute_path(),
            vec![in_package(plan.projected_order().paths().directory())],
            vec![Identity::order(&order_id)],
        ),
        OrderAdoptionRejection::CommitFailed {
            failure,
            rollback_failures,
        } => {
            let remaining: Vec<FailedPath> = rollback_failures
                .iter()
                .map(|cleanup| in_package(cleanup.location()))
                .collect();
            commit_failed(
                context,
                failure.location(),
                failure.phase().as_str(),
                remaining,
                "the metadata this adoption staged",
            )
        }
    }
}

/// A commit that failed partway, in the one of two codes it earns.
///
/// **A failed cleanup is its own code**, because the action differs: where everything this
/// attempt created was removed again the command can simply be run again, and where it was
/// not, what remains is named and this application will not guess whether it may delete it.
fn commit_failed(
    context: &ManagedContext,
    location: &Location,
    phase: &str,
    remaining: Vec<FailedPath>,
    staged: &str,
) -> Failed {
    let cleanup_failed = !remaining.is_empty();
    let mut paths = vec![in_package(location)];
    paths.extend(remaining);
    mutation_failed(
        if cleanup_failed {
            FailureCode::OrderMutationCleanupFailed
        } else {
            FailureCode::OrderMutationFailed
        },
        format!(
            "this mutation failed at {} and no Order record was committed.{}",
            phase,
            if cleanup_failed {
                format!(
                    " Some of {staged} could not be removed again; the paths beside this \
                     failure are what remains, and `spielorders validate` reports it."
                )
            } else {
                format!(" Everything {staged} was removed again.")
            }
        ),
        context.root().absolute_path(),
        paths,
        Vec::new(),
    )
}

/// The Package or the candidate moved under this mutation's feet.
///
/// **A `Location` and not a path**, which is what makes this refusal report a path the way
/// the success beside it does. It is only ever reached under a Package this command already
/// resolved — that is what "changed between planning and performing" means — so there is
/// always a Package to spell the subject relative to, and a signature taking a bare path
/// could not produce that spelling even in principle. See [Decision traceability and
/// contract-test conventions].
///
/// The race this reports cannot be driven by a test, so the type is the evidence: every
/// caller hands over a `Location` the core resolved or the read established, and one that
/// tried to hand over a path does not compile.
///
/// [Decision traceability and contract-test conventions]: ../../../../docs/development/traceability.md
fn changed_between(context: &ManagedContext, subject: &Location, because: &str) -> Failed {
    mutation_failed(
        FailureCode::OrderMutationPreconditionsChanged,
        format!(
            "{} changed between planning this mutation and performing it, {because}. \
             Nothing was written.",
            subject.absolute_path().display()
        ),
        context.root().absolute_path(),
        vec![in_package(subject)],
        Vec::new(),
    )
}

/// The Package stopped being a current candidate between resolving it and planning.
fn preconditions_changed(context: &ManagedContext) -> Failed {
    changed_between(
        context,
        context.root(),
        "so this Package is no longer one this mutation can be planned against",
    )
}

/// The residual refusal, for a record no assignment of the set can be blamed for.
///
/// The three facts the record constrains are named individually before a candidate is
/// assembled — see [`invalid_fact`] — so what is left here is a whole record that failed to
/// validate for a reason no single field names.
fn invalid_facts(context: &ManagedContext) -> Failed {
    refused(
        FailureCode::OrderFactsInvalid,
        "the complete Order record this mutation assembled is not valid for this Package, \
         so nothing was written. Every fact of the set was individually acceptable, so \
         `spielorders validate` is what reports what the assembled record broke."
            .to_owned(),
        context,
        Vec::new(),
    )
}

fn unknown_vendor(name: &str, context: &ManagedContext) -> Failed {
    refused(
        FailureCode::OrderVendorUnknown,
        unknown_vendor_message(&context.root().absolute_path().display().to_string(), name),
        context,
        vec![Identity::vendor_name(name)],
    )
}

/// The one sentence a Name that selects no Vendor earns, whichever mutation assigned it.
///
/// **A Name rather than a Vendor ID**, because a Name is what a caller writes: it resolves
/// trimmed and without regard to letter case, it is unique within a Package, and it selects
/// a Hidden Vendor like any other. The listing the sentence names is the one that shows
/// every Name there is to choose from.
fn unknown_vendor_message(root: &str, name: &str) -> String {
    format!(
        "the Package at {root} has no Vendor named `{name}`, so this Order references \
         none. A Vendor Name resolves trimmed and without regard to letter case; an Order \
         mutation chooses a Vendor and never creates one, `spielorders add-vendor` is what \
         creates one, and `spielorders package list vendors --all` lists every Vendor this \
         Package has."
    )
}

/// One operation this mutation could not perform, on the path it failed on.
///
/// **The core's own [`FailedPath`] and not a path**, because these two commands reach
/// operations on both sides of the Package: probing the destination inside the Orders
/// Directory carries this Package's spelling, and resolving the candidate directory a
/// caller named is the operation that fails before there is anything to name. The core is
/// what tells them apart, so nothing here re-derives a spelling or drops one.
fn filesystem_refusal(path: FailedPath, error: &str, context: &ManagedContext) -> Failed {
    Failed {
        code: FailureCode::PackageUnreadable,
        scope: FailureScope::Operation,
        message: format!(
            "{} could not be examined: {error}",
            path.as_path().display()
        ),
        paths: vec![path],
        identities: Vec::new(),
        package_root: Some(context.root().absolute_path().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// One refusal about the candidate directory a command named.
///
/// The path is the candidate itself, because it is what the refusal is about and what the
/// caller has to look at — and the Root is named in `context`, because a mutation that got
/// as far as resolving its Package established one.
///
/// **The caller chooses the path's spelling**, because these refusals do not agree about
/// one: most are raised after the candidate resolved, so the core hands back a `Location`
/// carrying whatever spelling this Package has for it — which is the Package-relative one
/// for a candidate inside the Package, and the path alone for one outside it or with
/// precisely no name this Package can express. The two that remain are refused on the
/// caller's own selected path before anything resolved it, and say so at their own sites.
fn candidate_refused(
    code: FailureCode,
    message: String,
    path: FailedPath,
    context: &ManagedContext,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Order,
        message,
        paths: vec![path],
        identities: Vec::new(),
        package_root: Some(context.root().absolute_path().to_path_buf()),
        // A mutation reports what it did. Assessing this Package is `package show` and
        // `validate`, and a refusal on Package-load grounds carries that diagnosis through
        // the shared constructors instead.
        diagnostics: Vec::new(),
    }
}

/// One refusal about the Order's facts, under a Package that resolved.
///
/// The path is the Orders Directory, because a mutation refused on its facts has no Order
/// Directory yet to name — the place the Order would have gone is what the refusal is about.
fn refused(
    code: FailureCode,
    message: String,
    context: &ManagedContext,
    identities: Vec<Identity>,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Order,
        message,
        paths: context
            .orders_directory()
            .map(|orders| vec![orders.clone().into()])
            .unwrap_or_default(),
        identities,
        package_root: Some(context.root().absolute_path().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// One refusal that happened after this mutation was planned.
fn mutation_failed(
    code: FailureCode,
    message: String,
    root: &Path,
    paths: Vec<FailedPath>,
    identities: Vec<Identity>,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Order,
        message,
        paths,
        identities,
        package_root: Some(root.to_path_buf()),
        diagnostics: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Presenting one mutation.
// ---------------------------------------------------------------------------

fn present_mutation(contract: ResponseContract, mutation: Mutation, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match mutation_document(contract, &mutation) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(contract, mutation_context(&mutation)),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_mutation(&mutation, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The two roots this answer was about, each named wherever it was established.
///
/// **Both commands are Package-rooted and still answer in an `order` context**, because
/// what they report is an Order: the Order Directory is the one the mutation created or
/// adopted, and a refusal that never reached a plan has none to name.
fn mutation_context(mutation: &Mutation) -> OrderContext {
    match mutation {
        Mutation::Planned(planned) => OrderContext::rooted_at(
            &planned.root,
            planned
                .answered()
                .0
                .order
                .paths()
                .directory()
                .absolute_path(),
        ),
        Mutation::Failed(failed) => OrderContext::of(failed.package_root.as_deref(), None),
    }
}

fn mutation_document(
    contract: ResponseContract,
    mutation: &Mutation,
) -> Result<(Response<Mutated, OrderContext>, Exit), NotRepresentable> {
    match mutation {
        Mutation::Planned(planned) => Ok((
            Response::answered(
                contract,
                mutation_context(mutation),
                mutated(planned)?,
                // A mutation reports what it did to one Order. The Package's own health is
                // `package show` and `validate`, and giving one fact two contracts is what
                // the envelope's `diagnostics` rule forbids.
                Vec::new(),
            ),
            Exit::Success,
        )),
        Mutation::Failed(failed) => Ok((
            failed_document(contract, mutation_context(mutation), failed)?,
            Exit::Failure,
        )),
    }
}

/// The report, and the Order the mutation leaves behind.
///
/// **Everything but `committed` is the same question of a plan and of a commit**, which is
/// what makes a dry run the real run's answer rather than a description of a different one:
/// the outcome, the reuse decisions, and the operations are read off the plan in both
/// cases, and the two members a commit genuinely produces — the refreshed index and the
/// warnings — are the ones a dry run has none of.
fn mutated(planned: &Planned) -> Result<Mutated, NotRepresentable> {
    let (facts, record) = planned.answered();
    Ok(Mutated {
        report: Report {
            committed: planned.committed.is_some(),
            // Always true: both of these commands write an Order record that did not exist,
            // so neither has an idempotent no-op to report.
            changed: true,
            outcome: planned.outcome,
            reusable_values: planned.reusable_values.clone(),
            operations: planned
                .operations
                .iter()
                .map(|(operation, location)| {
                    Ok(OperationProjection {
                        operation,
                        location: LocationProjection::of(location)?,
                    })
                })
                .collect::<Result<_, NotRepresentable>>()?,
            index: index(record)?,
            warnings: planned
                .committed
                .as_ref()
                .map(|committed| warning_codes(&committed.warnings))
                .unwrap_or_default(),
        },
        order: Show::of(&facts)?,
    })
}

/// The human counterpart: what this command did, and the Order as it now stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. A dry run says what it *would* do in its first line, because a reader
/// who skimmed one line of a plan and took it for a result is the whole hazard of reporting
/// a plan in the same shape as a result.
///
/// **The Order is rendered exactly as `order show` renders it**, and the index outcome is
/// beside it because a mutation reports one. The individual managed operations and the
/// reuse decisions are `--verbose` detail, which is the decided split between what a
/// mutation reports and what only a reader asking for detail sees.
fn human_mutation(mutation: &Mutation, output: &Output) -> io::Result<Exit> {
    match mutation {
        Mutation::Planned(planned) => {
            let (facts, record) = planned.answered();
            output.summary(
                planned.kind.label(planned.committed.is_some()),
                &text(facts.order.document().title.as_str()),
            )?;
            human_order(&facts, output)?;
            output.summary("Index", &text(record.state().as_str()))?;
            human_reusable_values(&planned.reusable_values, output)?;
            for (operation, location) in &planned.operations {
                output.detail(operation, location.absolute_path().as_os_str())?;
            }
            human_warnings(
                planned
                    .committed
                    .as_ref()
                    .map(|committed| committed.warnings.as_slice()),
                output,
            )?;
            Ok(Exit::Success)
        }
        Mutation::Failed(failed) => {
            output.error(&failed.message)?;
            for diagnostic in &failed.diagnostics {
                report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

/// What one mutation did with each of the four Reusable Free-Entry Value categories.
///
/// **Each category this Order carries a value from is an affected fact**, so it is
/// reported rather than held back for `--verbose`. A category the command line said
/// nothing about is not: four lines saying `absent` on every mutation would bury the ones
/// that mean something, and the machine response carries all four in every mode.
///
/// Shared by every command that reports these four, so the split between what a mutation
/// reports and what only `--verbose` shows is made once.
fn human_reusable_values(values: &ReusableValues, output: &Output) -> io::Result<()> {
    for (label, value) in values.reported() {
        match &value.value {
            Some(spelling) => {
                output.summary(label, &text(&format!("{} {spelling}", value.outcome)))?;
            }
            None => output.detail(label, &text(value.outcome))?,
        }
    }
    Ok(())
}

/// Every warning one commit carried, on the stream a warning earns.
///
/// `None` is a run that was asked to plan only, which has no commit to have carried one.
/// Shared by every mutation in this module so that a warning reaches a human the same way
/// whichever command committed it.
fn human_warnings(warnings: Option<&[Warning]>, output: &Output) -> io::Result<()> {
    for warning in warnings.unwrap_or_default() {
        output.warning(&warning.sentence)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The mutations of an Order that already exists: what all seven share.
// ---------------------------------------------------------------------------

/// The two roots an Order-rooted mutation's response names.
///
/// **Both are always known**, which is what separates these commands from the two creating
/// ones: an Order to move, edit, or maintain the Items of is one that already resolved, so
/// this context never reports one root without the other. It comes from the read that
/// targeted the Order — which is also what turns a `--uuid` into the Order Directory the
/// core capabilities take.
/// **Both are the core's own `Location`s**, kept rather than flattened to paths, because
/// a refusal about this Order owes the Package-relative spelling of the Order Directory
/// exactly as a success does — and these commands' refusals are about the Order Directory
/// far more often than about any path the core hands back with the refusal. Flattening
/// here is what left every Item-position and fact refusal naming a path with no spelling.
struct OrderRoots {
    package_root: Location,
    order_root: Location,
}

impl OrderRoots {
    fn of(read: &OrderRead) -> Self {
        Self {
            package_root: read.context().root().clone(),
            order_root: read.order().paths().directory().clone(),
        }
    }

    /// The SpielOrders Root as a path, for a caller that names a root rather than a
    /// location inside one.
    fn package_path(&self) -> &Path {
        self.package_root.absolute_path()
    }

    /// The Order Directory as a path, which is what the core capabilities are targeted at.
    fn order_path(&self) -> &Path {
        self.order_root.absolute_path()
    }
}

/// What one mutation of an existing Order decided, before anything is written or moved.
///
/// [`Mutation`]'s counterpart for the Order-rooted mutations — the two that move the whole
/// directory, the fact edit, and the four Item commands. It carries [`OrderFailed`] rather
/// than [`Failed`] for the reason the reads do: an `order` command's context names the
/// Order Directory as well as the Package.
enum OrderMutation<P> {
    Planned { roots: OrderRoots, plan: Box<P> },
    Failed(Box<OrderFailed>),
}

/// Every managed operation one mutation planned, in the core's own order.
///
/// The same projection `add-order` and `adopt-order` report their operations through, so
/// one managed path is spelled one way — absolute and Package-relative — whichever
/// command acted on it.
fn operation_projections(
    operations: &[(&'static str, Location)],
) -> Result<Vec<OperationProjection>, NotRepresentable> {
    operations
        .iter()
        .map(|(operation, location)| {
            Ok(OperationProjection {
                operation,
                location: LocationProjection::of(location)?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// `order align`.
// ---------------------------------------------------------------------------

/// The `order align` payload: what the rename did, and the Order at its new path.
///
/// The two members the mutation contract calls for. The refreshed result is [`Show`], the
/// same projection the reads render, because alignment changes no fact — so a client that
/// can display an Order can display the one this returns, at its new path, without a
/// second read.
#[derive(Debug, Serialize)]
struct Aligned {
    report: AlignmentReport,
    order: Show,
}

/// The typed alignment report.
///
/// **`old_path` and `new_path` are this command's own.** The mutation contract asks a
/// report for "old and new paths where relevant", and a rename is where they are: every
/// other Order mutation writes a record where the Order already stands, which is why
/// `add-order`'s report has no such member for a consumer to ignore. They are equal when
/// the Order is already aligned, which is the run that renames nothing.
///
/// **`basename` is the projection itself**, and it is reported rather than left to be
/// read off `new_path` because it is the answer to what alignment is for: the name this
/// Order's current facts compose.
///
/// **There is no `outcome` member.** Its value elsewhere is the core's own canonical
/// outcome spelling, and this capability defines none — an alignment writes no record at
/// all, so the canonical half of this report is the directory's own move. `changed` says
/// whether it moves and the two paths say from where to where; a spelling invented here
/// would be a second vocabulary for a fact the core already states.
///
/// **`reusable_values` is absent** for a checkable reason: this command takes no
/// assignment, so it can neither reuse nor introduce a Reusable Free-Entry Value.
#[derive(Debug, Serialize)]
struct AlignmentReport {
    committed: bool,
    /// Whether this alignment renames the directory. `false` for an Order already
    /// carrying the name its facts project, which is committed by performing nothing —
    /// drift is ordinary allowed state, so an aligned Order is not an error.
    changed: bool,
    old_path: LocationProjection,
    new_path: LocationProjection,
    basename: String,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every alignment whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// One decided alignment, in the terms every output mode reports it in.
struct Alignment {
    source: Location,
    destination: Location,
    basename: String,
    changes_directory_name: bool,
    operations: Vec<(&'static str, Location)>,
    prior_index: PackageIndexRecord,
    /// The Order as the read that targeted it found it, and the Vendor its reference
    /// resolves to.
    ///
    /// What a run asked to plan only reports: an alignment writes no fact, so the Order a
    /// dry run would leave is the Order standing there now — at the path it is still
    /// standing at, because nothing moved.
    order: HealthyOrder,
    vendor: Option<Vendor>,
    /// Whether this Order's conventional README is presently there, as the read observed.
    /// Carried across the commit because a rename moves the document with its directory.
    readme_exists: bool,
    committed: Option<AlignmentCommitted>,
}

/// What committing one alignment produced.
struct AlignmentCommitted {
    order: HealthyOrder,
    index: PackageIndexRecord,
    warnings: Vec<Warning>,
}

impl Alignment {
    /// The Order this run has to report, and the derived index it leaves behind.
    ///
    /// One selection rather than one per surface, for the reason [`Planned::answered`]
    /// has one: which of the plan and the commit answers each question is the same answer
    /// in every output mode.
    fn answered(&self) -> (OrderFacts<'_>, &PackageIndexRecord) {
        match &self.committed {
            Some(committed) => (
                OrderFacts::mutated(&committed.order, self.vendor.as_ref(), self.readme_exists),
                &committed.index,
            ),
            None => (
                OrderFacts::mutated(&self.order, self.vendor.as_ref(), self.readme_exists),
                &self.prior_index,
            ),
        }
    }
}

/// `order align`: the one command that renames an Order Directory.
///
/// **Nothing inside the directory is touched.** One rename moves the whole tree, so the
/// Order's README, its managed metadata, and its opaque Supporting Material travel
/// byte-identical — and none of them is enumerated, read, or rewritten to get there. That
/// is the core's guarantee; this command's part is to ask for it and report what happened.
pub(crate) fn align(target: &Target, dry_run: bool, output: &Output) -> Exit {
    let decided = match read(target) {
        Ok(read) => aligned(&read, dry_run),
        Err(failed) => OrderMutation::Failed(failed),
    };
    present_order_mutation(
        ResponseContract::OrderAlign,
        decided,
        alignment_payload,
        human_alignment,
        output,
    )
}

/// Plans an alignment, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a write
/// path: a run that was asked to plan returns here, so no directory was renamed and no
/// index was refreshed afterward.
fn aligned(read: &OrderRead, dry_run: bool) -> OrderMutation<Alignment> {
    let roots = OrderRoots::of(read);
    let plan = match plan_order_alignment(roots.order_path(), &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => return OrderMutation::Failed(Box::new(alignment_refusal(reason, &roots))),
    };
    let planned = Alignment {
        source: plan.source().clone(),
        destination: plan.destination().clone(),
        basename: plan.basename().to_owned(),
        changes_directory_name: plan.changes_directory_name(),
        operations: plan
            .operations()
            .iter()
            .map(|operation| (operation.kind().as_str(), operation.location().clone()))
            .collect(),
        prior_index: plan.prior_index().clone(),
        order: read.order().clone(),
        vendor: read.vendor().cloned(),
        readme_exists: read.readme().exists(),
        committed: None,
    };
    if dry_run {
        return OrderMutation::Planned {
            roots,
            plan: Box::new(planned),
        };
    }
    match commit_order_alignment(plan, &PlatformFileSystem) {
        // `Unchanged` beside `Committed` because both say the commit succeeded, and what
        // separates them is `changed` — which the report reads off the plan.
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            OrderMutation::Planned {
                roots,
                plan: Box::new(planned.committed(result.order(), result.index(), Vec::new())),
            }
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => OrderMutation::Planned {
            roots,
            plan: Box::new(planned.committed(
                result.order(),
                result.index(),
                warnings.iter().map(alignment_warning).collect(),
            )),
        },
        MutationReport::Rejected { rejection, .. } => {
            OrderMutation::Failed(Box::new(alignment_commit_refused(rejection, &roots)))
        }
    }
}

impl Alignment {
    /// The same plan, carrying what committing it produced.
    fn committed(
        mut self,
        order: &HealthyOrder,
        index: &PackageIndexRecord,
        warnings: Vec<Warning>,
    ) -> Self {
        self.committed = Some(AlignmentCommitted {
            order: order.clone(),
            index: index.clone(),
            warnings,
        });
        self
    }
}

/// The sentence for one alignment warning, saying what did *not* happen rather than what
/// did.
///
/// The rename is committed, so the sentence points at the command that puts the derived
/// state right and never at running this one again — which would be the retry a report
/// must not encourage.
fn alignment_warning(warning: &OrderAlignmentWarning) -> Warning {
    Warning {
        code: warning.as_str(),
        sentence: match warning {
            OrderAlignmentWarning::IndexRefreshFailed(_)
            | OrderAlignmentWarning::RefreshUnavailable => STALE_INDEX_AFTER_RELOCATION.to_owned(),
        },
    }
}

const STALE_INDEX_AFTER_RELOCATION: &str = "the Order Directory was moved and the derived Package Index was not refreshed, so it \
     is stale. `spielorders index rebuild` replaces it.";

/// The report, and the Order the alignment leaves behind.
fn alignment_payload(planned: &Alignment) -> Result<Aligned, NotRepresentable> {
    let (facts, record) = planned.answered();
    Ok(Aligned {
        report: AlignmentReport {
            committed: planned.committed.is_some(),
            changed: planned.changes_directory_name,
            old_path: LocationProjection::of(&planned.source)?,
            new_path: LocationProjection::of(&planned.destination)?,
            basename: planned.basename.clone(),
            operations: operation_projections(&planned.operations)?,
            index: index(record)?,
            warnings: planned
                .committed
                .as_ref()
                .map(|committed| warning_codes(&committed.warnings))
                .unwrap_or_default(),
        },
        order: Show::of(&facts)?,
    })
}

/// The human counterpart: what this command did, and the Order as it now stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing
/// the response contract. A dry run says what it *would* do in its first line, because a
/// reader who skimmed one line of a plan and took it for a result is the whole hazard of
/// reporting a plan in the same shape as a result.
fn human_alignment(planned: &Alignment, output: &Output) -> io::Result<()> {
    let (facts, record) = planned.answered();
    output.summary(
        match (planned.changes_directory_name, planned.committed.is_some()) {
            (false, _) => "Already aligned",
            (true, true) => "Aligned",
            (true, false) => "Would align",
        },
        &text(&planned.basename),
    )?;
    human_order(&facts, output)?;
    output.summary("Index", &text(record.state().as_str()))?;
    output.detail("Old path", planned.source.absolute_path().as_os_str())?;
    output.detail("New path", planned.destination.absolute_path().as_os_str())?;
    for (operation, location) in &planned.operations {
        output.detail(operation, location.absolute_path().as_os_str())?;
    }
    human_warnings(
        planned
            .committed
            .as_ref()
            .map(|committed| committed.warnings.as_slice()),
        output,
    )
}

// ---------------------------------------------------------------------------
// The refusals the two relocations reach.
// ---------------------------------------------------------------------------

/// Why the alignment was refused before any rename, as one typed failure.
///
/// **Total over the core's own reasons**, so a reason added there is a compile error here
/// rather than a mutation refused under a code that means something else. The four
/// Package-load states and the two Order-resolution ones are reachable only between the
/// read that targeted this Order and this plan, which is the same race the commit
/// refuses — so they carry the code a changed precondition carries, and the read is what
/// reported this Package's own diagnosis when it was already untrustworthy.
fn alignment_refusal(
    reason: OrderAlignmentPlanError<io::Error>,
    roots: &OrderRoots,
) -> OrderFailed {
    match reason {
        OrderAlignmentPlanError::UnmanagedPackage
        | OrderAlignmentPlanError::DamagedPackage
        | OrderAlignmentPlanError::UpgradePending
        | OrderAlignmentPlanError::PackageOperationFailed => {
            order_preconditions_changed(&roots.package_root, roots)
        }
        // The Order Directory the read located, which is the directory every one of these
        // names: the core reports the subject it was handed, and the read is what holds
        // this Package's spelling for it.
        OrderAlignmentPlanError::OrderNotFound { .. }
        | OrderAlignmentPlanError::DamagedOrder { .. } => {
            order_preconditions_changed(&roots.order_root, roots)
        }
        OrderAlignmentPlanError::EmptyProjectedComponent => order_mutation_refused(
            FailureCode::OrderTitleUnusable,
            "this Order Title projects no usable Order Directory name, so there is \
             nothing to rename the directory to. A Title made only of separators and \
             whitespace is what reaches this."
                .to_owned(),
            roots,
            vec![in_package(&roots.order_root)],
            Vec::new(),
        ),
        OrderAlignmentPlanError::DestinationCollision { location } => order_mutation_refused(
            FailureCode::OrderDirectoryOccupied,
            format!(
                "{} already exists, so this Order Directory cannot be renamed to it. It \
                 is refused rather than given an invented suffix, and nothing was moved.",
                location.absolute_path().display()
            ),
            roots,
            vec![in_package(&location)],
            Vec::new(),
        ),
        OrderAlignmentPlanError::UnresolvedVendor { vendor_id } => order_mutation_refused(
            FailureCode::OrderVendorUnresolved,
            format!(
                "this Order references a Vendor the Package at {} does not hold, and the \
                 aligned name is composed from that Vendor's current Name — so there is \
                 no name to rename the directory to. {}",
                roots.package_path().display(),
                RepairSuggestion::for_condition(
                    Rooting::Order,
                    DiagnosticCode::VendorReferenceUnresolved,
                    EvidenceScope::Order,
                )
            ),
            roots,
            vec![in_package(&roots.order_root)],
            vec![Identity::vendor(&vendor_id)],
        ),
        OrderAlignmentPlanError::FileSystem { path, error } => {
            order_filesystem_refusal(path, &error.to_string(), roots)
        }
    }
}

/// Why the alignment's commit was refused, as one typed failure.
fn alignment_commit_refused(
    rejection: OrderAlignmentRejection<io::Error>,
    roots: &OrderRoots,
) -> OrderFailed {
    match rejection {
        OrderAlignmentRejection::PreconditionsChanged { location } => {
            order_preconditions_changed(&location, roots)
        }
        OrderAlignmentRejection::PlanEvidenceUnconfirmed { .. } => {
            order_preconditions_changed(&roots.order_root, roots)
        }
        OrderAlignmentRejection::DestinationCollision { location } => order_mutation_refused(
            FailureCode::OrderDirectoryOccupied,
            format!(
                "{} was taken between planning this rename and performing it, so nothing \
                 was moved. It is refused rather than given an invented suffix.",
                location.absolute_path().display()
            ),
            roots,
            vec![in_package(&location)],
            Vec::new(),
        ),
        OrderAlignmentRejection::RenameFailed { location, error } => order_mutation_refused(
            FailureCode::OrderMutationFailed,
            format!(
                "this Order Directory could not be renamed to {}: {error}. It is still \
                 where it was, with everything in it.",
                location.absolute_path().display()
            ),
            roots,
            vec![in_package(&location)],
            Vec::new(),
        ),
    }
}

/// The Package, the Order, or the facts this mutation was authorized against moved under
/// its feet.
///
/// **One code for both halves of that**, because the action is the same: a plan that no
/// longer names the right Order and a plan whose projected name is no longer the right
/// one are both answered by planning again against what stands there now. Which fact
/// moved is the core's own `EvidenceOutcome`, and a caller that needs it reads the
/// Package rather than a string in this message.
///
/// **A `Location` and not a path**, for the reason [`changed_between`] takes one: this
/// refusal is only reachable after the read that targeted the Order established both
/// roots, so there is always a Package to spell the subject relative to — and the race is
/// not something a test can drive, so the type is what holds the rule.
fn order_preconditions_changed(subject: &Location, roots: &OrderRoots) -> OrderFailed {
    order_mutation_refused(
        FailureCode::OrderMutationPreconditionsChanged,
        format!(
            "{} changed between planning this mutation and performing it, so what would \
             have been moved is no longer what was validated. Nothing was moved.",
            subject.absolute_path().display()
        ),
        roots,
        vec![in_package(subject)],
        Vec::new(),
    )
}

/// One operation an Order-rooted mutation could not perform, on the path it failed on.
///
/// The core's own [`FailedPath`] and not a path, for the reason [`filesystem_refusal`]
/// takes one: an operation inside a Root this capability established carries that Root's
/// spelling, while the walk from an Order Directory up to the Package that holds it is how
/// the Root is found — so an operation failing there is reached before any Root is
/// resolved, and no Package surrounds the path it names.
fn order_filesystem_refusal(path: FailedPath, error: &str, roots: &OrderRoots) -> OrderFailed {
    OrderFailed {
        failed: Failed {
            code: FailureCode::PackageUnreadable,
            scope: FailureScope::Operation,
            message: format!(
                "{} could not be examined: {error}",
                path.as_path().display()
            ),
            paths: vec![path],
            identities: Vec::new(),
            package_root: Some(roots.package_path().to_path_buf()),
            diagnostics: Vec::new(),
        },
        order_root: Some(roots.order_path().to_path_buf()),
    }
}

/// One refusal of an Order-rooted mutation, under an Order and a Package the read
/// established.
///
/// **Both roots are named**, which is what an `order` command's context owes and what
/// every one of these commands can always supply: the Order resolved, or the refusal would
/// have come from the read instead. `diagnostics` is empty because a mutation reports what it
/// did — assessing this Package is `package show` and `validate`, and the read is what
/// carried that diagnosis where the Package was already untrustworthy.
fn order_mutation_refused(
    code: FailureCode,
    message: String,
    roots: &OrderRoots,
    paths: Vec<FailedPath>,
    identities: Vec<Identity>,
) -> OrderFailed {
    OrderFailed {
        failed: Failed {
            code,
            scope: FailureScope::Order,
            message,
            paths,
            identities,
            package_root: Some(roots.package_path().to_path_buf()),
            diagnostics: Vec::new(),
        },
        order_root: Some(roots.order_path().to_path_buf()),
    }
}

// ---------------------------------------------------------------------------
// Presenting one mutation of an Order that already exists.
// ---------------------------------------------------------------------------

/// Presents one mutation of an Order this command line resolved.
///
/// **All seven go through here**, so the envelope, the two-root context, the exit status,
/// the unrepresentable-path fallback, and the undeliverable-result fallback are decided
/// once rather than seven times. What each command supplies is its own payload and its own
/// human rendering, which is the whole of what differs between renaming a directory,
/// taking one out of the Package, changing a fact, and maintaining the Item collection.
fn present_order_mutation<P, T: Serialize>(
    contract: ResponseContract,
    decided: OrderMutation<P>,
    payload: impl Fn(&P) -> Result<T, NotRepresentable>,
    human: impl Fn(&P, &Output) -> io::Result<()>,
    output: &Output,
) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match order_mutation_document(contract, &decided, payload) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(contract, order_mutation_context(&decided)),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_order_mutation(&decided, human, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn order_mutation_context<P>(decided: &OrderMutation<P>) -> OrderContext {
    match decided {
        OrderMutation::Planned { roots, .. } => {
            OrderContext::rooted_at(roots.package_path(), roots.order_path())
        }
        OrderMutation::Failed(failed) => OrderContext::of(
            failed.failed.package_root.as_deref(),
            failed.order_root.as_deref(),
        ),
    }
}

fn order_mutation_document<P, T: Serialize>(
    contract: ResponseContract,
    decided: &OrderMutation<P>,
    payload: impl Fn(&P) -> Result<T, NotRepresentable>,
) -> Result<(Response<T, OrderContext>, Exit), NotRepresentable> {
    match decided {
        OrderMutation::Planned { plan, .. } => Ok((
            Response::answered(
                contract,
                order_mutation_context(decided),
                payload(plan)?,
                // A mutation reports what it did to one Order. The Package's own health is
                // `package show` and `validate`, and giving one fact two contracts is what
                // the envelope's `diagnostics` rule forbids.
                Vec::new(),
            ),
            Exit::Success,
        )),
        OrderMutation::Failed(failed) => Ok((
            failed_document(contract, order_mutation_context(decided), &failed.failed)?,
            Exit::Failure,
        )),
    }
}

fn human_order_mutation<P>(
    decided: &OrderMutation<P>,
    human: impl Fn(&P, &Output) -> io::Result<()>,
    output: &Output,
) -> io::Result<Exit> {
    match decided {
        OrderMutation::Planned { plan, .. } => {
            human(plan, output)?;
            Ok(Exit::Success)
        }
        OrderMutation::Failed(failed) => {
            output.error(&failed.failed.message)?;
            for diagnostic in &failed.failed.diagnostics {
                report_diagnostic(output, failed.failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

// ---------------------------------------------------------------------------
// `order remove`.
// ---------------------------------------------------------------------------

/// The `order remove` payload: what the disposition did, and the Order it took out.
///
/// **The removed entity summary and its disposition**, which the CLI contract names as
/// this command's own payload rather than `order show`'s projection. See
/// [`ResponseContract::OrderRemove`].
#[derive(Debug, Serialize)]
struct Disposed {
    report: RemovalReport,
    removed: RemovedOrder,
}

/// The typed disposition report.
///
/// **`old_path` and `new_path` are this command's own**, as they are `order align`'s and
/// no other Order mutation's: these two commands move a directory, and the rest write a
/// record where the Order already stands.
///
/// **`changed` is always true.** A disposition always moves the whole directory — there
/// is no Order this command can be asked about twice, because after the first run it is
/// no longer one of this Package's Orders. It is carried because the outcome vocabulary
/// is one vocabulary across every mutating command rather than a shape each invents.
///
/// **There is no `outcome` member**, for the reason [`AlignmentReport`] has none: the core
/// defines no canonical outcome for a disposition, whose canonical half is the
/// directory's own move. `disposition` is what a consumer branches on, and it is the
/// core's own spelling.
///
/// **The lazily created Removed Orders Directory is reported once**, as the
/// `create_removed_orders_directory` operation, which is planned only when the directory
/// is not there. A second member saying the same thing would be a second home for one
/// fact.
#[derive(Debug, Serialize)]
struct RemovalReport {
    committed: bool,
    changed: bool,
    /// Where the caller asked the directory to go, in the core's own spelling.
    disposition: &'static str,
    /// Where the Order Directory stood when this run planned.
    old_path: LocationProjection,
    /// Where the directory goes, as far as this run knows. See [`Landed`].
    new_path: Option<LocationProjection>,
    /// What this disposition did to the Order's technical Package locator.
    locator: LocatorProjection,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every disposition whose derived refresh and locator rewrite
    /// both succeeded.
    warnings: Vec<&'static str>,
}

/// What one disposition did to the moved Order's technical Package locator.
///
/// The locator is a relative path from the Order Directory to its Package Root, so it
/// depends only on how deep the directory sits: a destination at the same depth keeps the
/// stored value, one at a different depth needs it rewritten, and the Trash leaves the
/// Package entirely so no Package-relative locator applies at all.
///
/// `from` and `to` are the same value for `unchanged`, because the stored locator is
/// correct at the destination too — which is the fact that outcome states. Both are
/// `null` for `not_applicable`.
#[derive(Debug, Serialize)]
struct LocatorProjection {
    outcome: &'static str,
    from: Option<String>,
    to: Option<String>,
}

impl LocatorProjection {
    fn of(locator: &LocatorOutcome) -> Self {
        let (from, to) = match locator {
            LocatorOutcome::Unchanged { locator } => (Some(locator.clone()), Some(locator.clone())),
            LocatorOutcome::Updated { from, to } => (Some(from.clone()), Some(to.clone())),
            LocatorOutcome::NotApplicable => (None, None),
        };
        Self {
            outcome: locator.as_str(),
            from,
            to,
        }
    }
}

/// The Order this disposition took out of the Package, as it stood.
///
/// **A summary, not [`Show`]**: the Order is no longer one of this Package's Orders, so
/// reporting its current paths, README, and local diagnostics would be reporting them
/// about a directory this Package does not hold. Its facts and its identity are what a
/// caller needs to say what was removed; where it was and where it went are the report's,
/// which is the one home each of those paths has.
#[derive(Debug, Serialize)]
struct RemovedOrder {
    order_id: String,
    title: String,
    date_ordered: String,
    status: &'static str,
    /// The Vendor this Order named, as the record carried it. `null` when it named none.
    vendor_id: Option<String>,
    /// That Vendor as the core resolved it, Hidden Vendors included — hiding governs what
    /// a Package offers for new use and never what is resolved.
    vendor: Option<VendorSummary>,
}

/// Where a disposition puts the directory, as far as this run knows.
///
/// Three answers rather than an optional path, because the two that name one name it in
/// different terms: a destination inside the Package is a place this Package locates, and
/// a trashed directory is outside every Package — which is exactly the difference a
/// `Location`'s Package-relative spelling records.
enum Landed<'a> {
    /// The Removed Orders Directory destination this core chose. A dry run has it too,
    /// because the core chose it while planning.
    InPackage(&'a Location),
    /// Where this platform's Trash reported putting the directory. Outside any Package,
    /// so there is no Package-relative spelling to carry — and this application did not
    /// choose it.
    InTrash(&'a Path),
    /// Not known: a disposal that was only planned, or one the platform completed without
    /// saying where. The platform owns where a trashed directory goes, and reporting a
    /// guess would be inventing it.
    Unknown,
}

/// One decided disposition, in the terms every output mode reports it in.
struct Disposition {
    disposition: RemovalDisposition,
    source: Location,
    /// The destination this core chose, for the disposition that has one. The Trash has
    /// none to plan.
    destination: Option<Location>,
    locator: LocatorOutcome,
    operations: Vec<(&'static str, Location)>,
    prior_index: PackageIndexRecord,
    /// The Order as the read that targeted it found it, and the Vendor its reference
    /// resolves to. What a run asked to plan only reports, and what a committed one
    /// reports too: a disposition writes no domain fact, so the Order it took out is the
    /// Order that was standing there.
    order: HealthyOrder,
    vendor: Option<Vendor>,
    /// Whether this Order's conventional README was there, as the read observed. Carried
    /// across the disposition because the document moves with its directory.
    readme_exists: bool,
    committed: Option<DispositionCommitted>,
}

/// What committing one disposition produced.
struct DispositionCommitted {
    /// Where the directory finally landed, where that is knowable. The platform Trash
    /// does not always report a location.
    final_location: Option<PathBuf>,
    index: PackageIndexRecord,
    warnings: Vec<Warning>,
}

impl Disposition {
    /// The derived index this run leaves behind, and where the directory ended up.
    ///
    /// One selection rather than one per surface, for the reason [`Alignment::answered`]
    /// is one: which of the plan and the commit answers each question is the same answer
    /// in every output mode.
    ///
    /// The planned destination answers for a recoverable disposition whether or not this
    /// run committed, because that is the path the core chose and the path it moved the
    /// directory to. Only the Trash has an answer the commit alone can supply.
    fn answered(&self) -> (Landed<'_>, &PackageIndexRecord) {
        let landed = match (&self.destination, &self.committed) {
            (Some(destination), _) => Landed::InPackage(destination),
            (None, Some(committed)) => match &committed.final_location {
                Some(path) => Landed::InTrash(path),
                None => Landed::Unknown,
            },
            (None, None) => Landed::Unknown,
        };
        match &self.committed {
            Some(committed) => (landed, &committed.index),
            None => (landed, &self.prior_index),
        }
    }
}

/// `order remove`: the one command that may dispose of Supporting Material.
///
/// **It disposes of it only as part of one whole Order Directory**, and never by erasing
/// anything: the directory is moved into the Package's Removed Orders Directory, or
/// submitted whole to the platform Trash under `--delete`. Nothing here enumerates,
/// reads, or rewrites the opaque tree to move it, and there is no force flag, no
/// empty-trash operation, and no fallback that deletes when the Trash is unavailable.
pub(crate) fn remove(target: &Target, delete: bool, dry_run: bool, output: &Output) -> Exit {
    let decided = match read(target) {
        Ok(read) => disposed(&read, delete, dry_run),
        Err(failed) => OrderMutation::Failed(failed),
    };
    present_order_mutation(
        ResponseContract::OrderRemove,
        decided,
        disposition_payload,
        human_disposition,
        output,
    )
}

/// Plans a disposition, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a write
/// path: a run that was asked to plan returns here, so no directory was created, nothing
/// was moved, and the Trash was never asked for anything.
fn disposed(read: &OrderRead, delete: bool, dry_run: bool) -> OrderMutation<Disposition> {
    let roots = OrderRoots::of(read);
    let disposition = if delete {
        RemovalDisposition::PlatformTrash
    } else {
        RemovalDisposition::RecoverableDirectory
    };
    let plan = match plan_order_removal(roots.order_path(), disposition, &PlatformFileSystem) {
        Ok(plan) => plan,
        Err(reason) => return OrderMutation::Failed(Box::new(removal_refusal(reason, &roots))),
    };
    let planned = Disposition {
        disposition: plan.disposition(),
        source: plan.source().clone(),
        destination: plan.destination().cloned(),
        locator: plan.locator().clone(),
        operations: plan
            .operations()
            .iter()
            .map(|operation| (operation.kind().as_str(), operation.location().clone()))
            .collect(),
        prior_index: plan.prior_index().clone(),
        order: read.order().clone(),
        vendor: read.vendor().cloned(),
        readme_exists: read.readme().exists(),
        committed: None,
    };
    if dry_run {
        return OrderMutation::Planned {
            roots,
            plan: Box::new(planned),
        };
    }
    match commit_order_removal(plan, &PlatformFileSystem, &PlatformTrash) {
        MutationReport::Committed { result, .. } | MutationReport::Unchanged { result, .. } => {
            OrderMutation::Planned {
                roots,
                plan: Box::new(planned.committed(&result, Vec::new())),
            }
        }
        MutationReport::CommittedWithWarning {
            result, warnings, ..
        } => {
            let reported = warnings.iter().map(removal_warning).collect();
            OrderMutation::Planned {
                roots,
                plan: Box::new(planned.committed(&result, reported)),
            }
        }
        MutationReport::Rejected { rejection, .. } => {
            OrderMutation::Failed(Box::new(removal_commit_refused(rejection, &roots)))
        }
    }
}

impl Disposition {
    /// The same plan, carrying what committing it produced.
    fn committed(mut self, result: &OrderRemovalResult, warnings: Vec<Warning>) -> Self {
        self.committed = Some(DispositionCommitted {
            final_location: result.final_location().map(Path::to_path_buf),
            index: result.index().clone(),
            warnings,
        });
        self
    }
}

/// The sentence for one disposition warning, saying what did *not* happen rather than
/// what did.
///
/// The directory has already gone, so each sentence points at what to correct and never
/// at running this command again — which would be the retry a report must not encourage,
/// and here would be a retry of a removal whose Order is no longer there to remove.
fn removal_warning<E>(warning: &OrderRemovalWarning<E>) -> Warning {
    Warning {
        code: warning.as_str(),
        sentence: match warning {
            OrderRemovalWarning::IndexRefreshFailed(_)
            | OrderRemovalWarning::RefreshUnavailable => STALE_INDEX_AFTER_RELOCATION.to_owned(),
            OrderRemovalWarning::LocatorUpdateFailed(_) => {
                "the Order Directory was moved and the technical locator in its own \
                 record could not be rewritten at the new path. The Order is already out \
                 of this Package's active scope, so nothing the Package loads is damaged; \
                 moving the directory back is what makes the locator matter again."
                    .to_owned()
            }
        },
    }
}

/// The report, and the Order this disposition took out.
fn disposition_payload(planned: &Disposition) -> Result<Disposed, NotRepresentable> {
    let (landed, record) = planned.answered();
    let document = planned.order.document();
    Ok(Disposed {
        report: RemovalReport {
            committed: planned.committed.is_some(),
            // Always true: a disposition moves the whole directory out of the Orders
            // Directory, so there is no idempotent no-op to report.
            changed: true,
            disposition: planned.disposition.as_str(),
            old_path: LocationProjection::of(&planned.source)?,
            new_path: match landed {
                Landed::InPackage(destination) => Some(LocationProjection::of(destination)?),
                Landed::InTrash(path) => Some(LocationProjection::outside_a_package(path)?),
                Landed::Unknown => None,
            },
            locator: LocatorProjection::of(&planned.locator),
            operations: operation_projections(&planned.operations)?,
            index: index(record)?,
            warnings: planned
                .committed
                .as_ref()
                .map(|committed| warning_codes(&committed.warnings))
                .unwrap_or_default(),
        },
        removed: RemovedOrder {
            order_id: document.order_id.as_str().to_owned(),
            title: document.title.as_str().to_owned(),
            date_ordered: document.date_ordered.as_str().to_owned(),
            status: document.status.as_str(),
            vendor_id: document
                .vendor_id
                .as_ref()
                .map(|vendor_id| vendor_id.as_str().to_owned()),
            vendor: planned.vendor.as_ref().map(VendorSummary::of),
        },
    })
}

/// The human counterpart: what this command did with the whole directory, and the Order
/// it took out.
///
/// Human output is never a parsing contract, so this may be reworded without advancing
/// the response contract. The Order is rendered exactly as `order show` renders it,
/// because a user about to lose an Order from their Package should see which Order it is;
/// a dry run says what it *would* do in its first line.
fn human_disposition(planned: &Disposition, output: &Output) -> io::Result<()> {
    let (landed, record) = planned.answered();
    let committed = planned.committed.is_some();
    output.summary(
        match (planned.disposition, committed) {
            (RemovalDisposition::RecoverableDirectory, true) => "Removed",
            (RemovalDisposition::RecoverableDirectory, false) => "Would remove",
            (RemovalDisposition::PlatformTrash, true) => "Trashed",
            (RemovalDisposition::PlatformTrash, false) => "Would trash",
        },
        &text(planned.order.document().title.as_str()),
    )?;
    human_order(
        &OrderFacts::mutated(
            &planned.order,
            planned.vendor.as_ref(),
            planned.readme_exists,
        ),
        output,
    )?;
    output.summary(
        "Disposition",
        &text(match planned.disposition {
            RemovalDisposition::RecoverableDirectory => "the Removed Orders Directory",
            RemovalDisposition::PlatformTrash => "the platform Trash",
        }),
    )?;
    let arrival = if committed { "Now at" } else { "Would go to" };
    match landed {
        Landed::InPackage(destination) => {
            output.summary(arrival, destination.absolute_path().as_os_str())?
        }
        Landed::InTrash(path) => output.summary(arrival, path.as_os_str())?,
        // The platform owns where a trashed directory goes and is free not to say — and a
        // run that only planned never asked it. Said rather than left blank, because a
        // missing line reads as an omission.
        Landed::Unknown => {
            output.summary(arrival, &text("wherever this platform's Trash keeps it"))?
        }
    }
    output.summary("Index", &text(record.state().as_str()))?;
    output.detail("Old path", planned.source.absolute_path().as_os_str())?;
    output.detail("Locator", &text(planned.locator.as_str()))?;
    for (operation, location) in &planned.operations {
        output.detail(operation, location.absolute_path().as_os_str())?;
    }
    human_warnings(
        planned
            .committed
            .as_ref()
            .map(|committed| committed.warnings.as_slice()),
        output,
    )
}

/// Why the disposition was refused before anything moved, as one typed failure.
///
/// **Total over the core's own reasons**, so a reason added there is a compile error here
/// rather than a mutation refused under a code that means something else.
fn removal_refusal(reason: OrderRemovalPlanError<io::Error>, roots: &OrderRoots) -> OrderFailed {
    match reason {
        OrderRemovalPlanError::UnmanagedPackage
        | OrderRemovalPlanError::DamagedPackage
        | OrderRemovalPlanError::UpgradePending
        | OrderRemovalPlanError::PackageOperationFailed => {
            order_preconditions_changed(&roots.package_root, roots)
        }
        // The Order Directory the read located, for the reason `alignment_refusal` names
        // it: the core reports the subject it was handed, and the read holds the spelling.
        OrderRemovalPlanError::OrderNotFound { .. }
        | OrderRemovalPlanError::DamagedOrder { .. } => {
            order_preconditions_changed(&roots.order_root, roots)
        }
        OrderRemovalPlanError::DestinationCollision { location } => order_mutation_refused(
            FailureCode::OrderDirectoryOccupied,
            format!(
                "{} already exists, so this Order Directory cannot be moved there. It is \
                 refused rather than given an invented suffix, and nothing was moved.",
                location.absolute_path().display()
            ),
            roots,
            vec![in_package(&location)],
            Vec::new(),
        ),
        OrderRemovalPlanError::FileSystem { path, error } => {
            order_filesystem_refusal(path, &error.to_string(), roots)
        }
    }
}

/// Why the disposition's commit was refused, as one typed failure.
///
/// **A rollback travels beside the failure that initiated it, never in place of it**,
/// which is what `docs/architecture/durable-writes.md` requires: the code names why the
/// disposition did not happen, and a Removed Orders Directory this attempt created and
/// could not take back again is named beside it, in the message and among the paths. An
/// Order stranded half out of the Package is what that forbids, and neither of these
/// leaves one: the directory is where it was, with everything in it.
fn removal_commit_refused(
    rejection: OrderRemovalRejection<io::Error>,
    roots: &OrderRoots,
) -> OrderFailed {
    match rejection {
        OrderRemovalRejection::PreconditionsChanged { location } => {
            order_preconditions_changed(&location, roots)
        }
        OrderRemovalRejection::PlanEvidenceUnconfirmed { .. } => {
            order_preconditions_changed(&roots.order_root, roots)
        }
        OrderRemovalRejection::DestinationCollision { location, cleanup } => {
            let (clause, retained) = undone(&cleanup);
            let mut paths = vec![in_package(&location)];
            paths.extend(retained);
            order_mutation_refused(
                FailureCode::OrderDirectoryOccupied,
                format!(
                    "{} was taken between planning this removal and performing it, so \
                     nothing was moved. It is refused rather than given an invented \
                     suffix.{clause}",
                    location.absolute_path().display()
                ),
                roots,
                paths,
                Vec::new(),
            )
        }
        OrderRemovalRejection::RemovedDirectoryCreationFailed { location, error } => {
            order_mutation_refused(
                FailureCode::OrderMutationFailed,
                format!(
                    "this Package's Removed Orders Directory could not be created at {}: \
                     {error}. Nothing was moved, and the Order is still where it was.",
                    location.absolute_path().display()
                ),
                roots,
                vec![in_package(&location)],
                Vec::new(),
            )
        }
        OrderRemovalRejection::MoveFailed {
            location,
            error,
            cleanup,
        } => {
            let (clause, retained) = undone(&cleanup);
            let mut paths = vec![in_package(&location)];
            paths.extend(retained);
            order_mutation_refused(
                FailureCode::OrderMutationFailed,
                format!(
                    "this Order Directory could not be moved to {}: {error}. It is still \
                     where it was, with everything in it, and this removal can be asked \
                     for again.{clause}",
                    location.absolute_path().display()
                ),
                roots,
                paths,
                Vec::new(),
            )
        }
        OrderRemovalRejection::TrashSubmissionFailed { location, error } => {
            let unsupported = error.kind() == io::ErrorKind::Unsupported;
            order_mutation_refused(
                if unsupported {
                    FailureCode::OrderTrashUnsupported
                } else {
                    FailureCode::OrderTrashSubmissionFailed
                },
                if unsupported {
                    format!(
                        "{error}, so this Order Directory was not disposed of and nothing \
                         was erased. `spielorders order remove` without --delete moves it \
                         into this Package's Removed Orders Directory instead."
                    )
                } else {
                    format!(
                        "{} could not be submitted to this platform's Trash: {error}. It \
                         is still where it was, with everything in it, and nothing was \
                         erased.",
                        location.absolute_path().display()
                    )
                },
                roots,
                vec![in_package(&location)],
                Vec::new(),
            )
        }
    }
}

/// What undoing this attempt's own Removed Orders Directory left, as a clause and a path.
///
/// The clause is empty and no path is added where there is nothing to say: this operation
/// created no such directory, or the one it created is gone. The other two outcomes are
/// what the acceptance criterion means by reporting both — a directory left standing
/// because something arrived inside it, and one the rollback could not complete at all.
fn undone(cleanup: &RemovedDirectoryCleanup<io::Error>) -> (String, Vec<FailedPath>) {
    match cleanup {
        RemovedDirectoryCleanup::NothingCreated | RemovedDirectoryCleanup::Removed { .. } => {
            (String::new(), Vec::new())
        }
        RemovedDirectoryCleanup::Retained { location } => (
            format!(
                " The Removed Orders Directory this attempt created at {} was left in \
                 place, because it is no longer the empty directory it created.",
                location.absolute_path().display()
            ),
            vec![in_package(location)],
        ),
        RemovedDirectoryCleanup::Failed { location, error } => (
            format!(
                " The Removed Orders Directory this attempt created at {} could not be \
                 removed again: {error}.",
                location.absolute_path().display()
            ),
            vec![in_package(location)],
        ),
    }
}

// ---------------------------------------------------------------------------
// `order edit`.
// ---------------------------------------------------------------------------

/// The `order edit` payload: what the edit did, and the Order as it now stands.
///
/// The two members the mutation contract calls for. The refreshed result is [`Show`], the
/// same projection the reads render, so a client that can display an Order can display the
/// one this returns without a second read and without a second shape to learn.
#[derive(Debug, Serialize)]
struct Edited {
    report: EditReport,
    order: Show,
}

/// The typed report of one Order fact edit.
///
/// **`date_ordered_source` is this command's own.** Entering `ordered` with no date applies
/// today's local date, and this is what separates that from a date the command line
/// supplied and from the one the Order already carried — a distinction a reader cannot make
/// from the date itself, and the reason it is a member rather than a sentence. Its spelling
/// is the core's own.
///
/// **`reusable_values` is the reuse-versus-introduction report** for the four Reusable
/// Free-Entry Value categories, as it is on the two creating commands: an assignment
/// matching a spelling this Package already uses reuses that canonical spelling, and a
/// genuinely new value is introduced — which a CLI assignment is itself the confirmation
/// of.
///
/// **There is no `outcome` member**, for the reason [`AlignmentReport`] has none: its value
/// elsewhere is the core's own canonical outcome spelling, and this capability defines none.
/// `changed` says whether the record changes and the refreshed Order says what it now
/// holds; a spelling invented here would be a second vocabulary for a fact the core already
/// states.
///
/// **There are no old-and-new paths.** An edit writes the record where the Order stands and
/// deliberately leaves the directory name alone; `order align` is what renames one.
#[derive(Debug, Serialize)]
struct EditReport {
    committed: bool,
    /// Whether this edit changes the canonical Order record. `false` for an assignment set
    /// that names the facts the Order already carries, which is committed by performing
    /// nothing.
    changed: bool,
    date_ordered_source: &'static str,
    reusable_values: ReusableValues,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every edit whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// One decided fact edit, in the terms every output mode reports it in.
struct Edit {
    plan: OrderEditPlan,
    /// Whether this Order's conventional README is presently there, as the read observed.
    /// Carried across the commit because an edit writes only the Order record.
    readme_exists: bool,
    committed: Option<EditCommitted>,
}

/// What committing one fact edit produced.
struct EditCommitted {
    result: OrderEditResult,
    warnings: Vec<Warning>,
}

impl Edit {
    /// The Order this run has to report, and the derived index it leaves behind.
    ///
    /// One selection rather than one per surface, for the reason [`Planned::answered`] has
    /// one: which of the plan and the commit answers each question is the same answer in
    /// every output mode.
    ///
    /// **README existence is `true` either way**, because an Order that resolved has the
    /// conventional document or does not, and an edit writes only the record: the text the
    /// projection carries is the load's, and whether the file is there is what the read
    /// that targeted this Order already observed.
    fn answered(&self) -> (OrderFacts<'_>, &PackageIndexRecord) {
        match &self.committed {
            Some(committed) => (
                OrderFacts::mutated(
                    committed.result.order(),
                    committed.result.vendor(),
                    self.readme_exists,
                ),
                committed.result.index(),
            ),
            None => (
                OrderFacts::mutated(
                    self.plan.projected_order(),
                    self.plan.vendor(),
                    self.readme_exists,
                ),
                self.plan.prior_index(),
            ),
        }
    }
}

/// `order edit`: the command that changes an Order's own facts.
///
/// **The directory is deliberately left alone.** An edited Title or Date Ordered makes the
/// Order Directory's name drift from the name the facts now project, which is ordinary
/// allowed state; `order align` is the explicit action that renames it.
pub(crate) fn edit(
    target: &Target,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let decided = match read(target) {
        Ok(read) => edited(&read, assignments, dry_run),
        Err(failed) => OrderMutation::Failed(failed),
    };
    present_order_mutation(
        ResponseContract::OrderEdit,
        decided,
        edit_payload,
        human_edit,
        output,
    )
}

/// What committing one Order-record mutation produced, or the rejection it earned.
///
/// **One place decides that an unchanged commit is a success**: `Unchanged` says the commit
/// happened, and what separates it from `Committed` is `changed`, which the report reads
/// off the plan. The fact edit and the four Item commands therefore cannot come to disagree
/// about that, or about which warnings a commit carried. The plan travels back because the
/// core hands it back with the report.
fn after_committing<P>(
    report: MutationReport<P, OrderEditResult, OrderEditRejection<io::Error>, OrderEditWarning>,
) -> Result<(P, EditCommitted), OrderEditRejection<io::Error>> {
    match report {
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            Ok((
                plan,
                EditCommitted {
                    result,
                    warnings: Vec::new(),
                },
            ))
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => Ok((
            plan,
            EditCommitted {
                result,
                warnings: warnings.iter().map(edit_warning).collect(),
            },
        )),
        MutationReport::Rejected { rejection, .. } => Err(rejection),
    }
}

/// Plans a fact edit, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a write
/// path: a run that was asked to plan returns here, so no record was replaced, no staging
/// file exists, and no index was refreshed afterward.
fn edited(read: &OrderRead, assignments: &[FieldAssignment], dry_run: bool) -> OrderMutation<Edit> {
    let roots = OrderRoots::of(read);
    let plan = match plan_order_edit(
        roots.order_path(),
        &assigned(assignments),
        &PlatformCalendar,
        &PlatformFileSystem,
    ) {
        Ok(plan) => plan,
        Err(reason) => return OrderMutation::Failed(Box::new(edit_refusal(reason, &roots))),
    };
    if dry_run {
        return OrderMutation::Planned {
            roots,
            plan: Box::new(Edit {
                plan,
                readme_exists: read.readme().exists(),
                committed: None,
            }),
        };
    }
    let (plan, committed) = match commit_order_edit(plan, &PlatformFileSystem) {
        // `Unchanged` beside `Committed` because both say the commit succeeded, and what
        // separates them is `changed` — which the report reads off the plan.
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            (
                plan,
                EditCommitted {
                    result,
                    warnings: Vec::new(),
                },
            )
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => (
            plan,
            EditCommitted {
                result,
                warnings: warnings.iter().map(edit_warning).collect(),
            },
        ),
        MutationReport::Rejected { rejection, .. } => {
            return OrderMutation::Failed(Box::new(edit_commit_refused(rejection, &roots)));
        }
    };
    OrderMutation::Planned {
        roots,
        plan: Box::new(Edit {
            plan,
            readme_exists: read.readme().exists(),
            committed: Some(committed),
        }),
    }
}

/// The report, and the Order the edit leaves behind.
///
/// **Everything but `committed` is the same question of a plan and of a commit**, which is
/// what makes a dry run the real run's answer rather than a description of a different one:
/// `changed`, the Date Ordered's source, the reuse decisions, and the operations are read
/// off the plan in both cases, and the two members a commit genuinely produces — the
/// refreshed index and the warnings — are the ones a dry run has none of.
fn edit_payload(decided: &Edit) -> Result<Edited, NotRepresentable> {
    let (facts, record) = decided.answered();
    Ok(Edited {
        report: EditReport {
            committed: decided.committed.is_some(),
            changed: decided.plan.changes_canonical_order(),
            date_ordered_source: decided.plan.date_ordered_source().as_str(),
            reusable_values: ReusableValues::of(decided.plan.reusable_values()),
            operations: operation_projections(&record_operations(decided.plan.operations()))?,
            index: index(record)?,
            warnings: decided
                .committed
                .as_ref()
                .map(|committed| warning_codes(&committed.warnings))
                .unwrap_or_default(),
        },
        order: Show::of(&facts)?,
    })
}

/// Every managed operation one Order-record mutation planned, in the core's own order.
///
/// One function for the fact edit and the four Item commands, because all five commit one
/// Order record and refresh one derived index — the plan is what says whether there is
/// anything to do.
fn record_operations(operations: &[OrderEditOperation]) -> Vec<(&'static str, Location)> {
    operations
        .iter()
        .map(|operation| (operation.kind().as_str(), operation.location().clone()))
        .collect()
}

/// The sentence for one Order-record mutation's warning, saying what did *not* happen
/// rather than what did.
///
/// The record is committed, so the sentence points at the command that puts the derived
/// state right and never at running this one again — which would be the retry a report must
/// not encourage. Shared by the fact edit and the four Item mutations, because both commit
/// the same one record and carry the same warnings.
fn edit_warning(warning: &OrderEditWarning) -> Warning {
    Warning {
        code: match warning {
            OrderEditWarning::IndexRefreshFailed(_) => "index_refresh_failed",
            OrderEditWarning::RefreshUnavailable => "refresh_unavailable",
        },
        sentence: STALE_INDEX.to_owned(),
    }
}

/// The human counterpart: what this command did, and the Order as it now stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. A dry run says what it *would* do in its first line, because a reader
/// who skimmed one line of a plan and took it for a result is the whole hazard of reporting
/// a plan in the same shape as a result.
///
/// **The Order is rendered exactly as `order show` renders it**, and the index outcome is
/// beside it because a mutation reports one. The individual managed operations, the reuse
/// decisions with nothing to say, and where the Date Ordered came from are `--verbose`
/// detail.
fn human_edit(decided: &Edit, output: &Output) -> io::Result<()> {
    let (facts, record) = decided.answered();
    output.summary(
        match (
            decided.plan.changes_canonical_order(),
            decided.committed.is_some(),
        ) {
            (false, _) => "Unchanged",
            (true, true) => "Edited",
            (true, false) => "Would edit",
        },
        &text(facts.order.document().title.as_str()),
    )?;
    human_order(&facts, output)?;
    output.summary("Index", &text(record.state().as_str()))?;
    output.detail(
        "Date Ordered source",
        &text(decided.plan.date_ordered_source().as_str()),
    )?;
    human_reusable_values(&ReusableValues::of(decided.plan.reusable_values()), output)?;
    for (operation, location) in &record_operations(decided.plan.operations()) {
        output.detail(operation, location.absolute_path().as_os_str())?;
    }
    human_warnings(
        decided
            .committed
            .as_ref()
            .map(|committed| committed.warnings.as_slice()),
        output,
    )
}

// ---------------------------------------------------------------------------
// The refusals the record mutations reach.
// ---------------------------------------------------------------------------

/// Why the fact edit or one Item mutation was refused before any write, as one typed
/// failure.
///
/// **Total over the core's own reasons**, so a reason added there is a compile error here
/// rather than a mutation refused under a code that means something else. One function for
/// all five commands, because they are refused by one core capability on one set of terms:
/// what differs is which of them can reach a given reason, and the code a caller branches
/// on must not depend on that.
///
/// The four Package-load states and the two Order-resolution ones are reachable only
/// between the read that targeted this Order and this plan, which is the same race the
/// commit refuses — so they carry the code a changed precondition carries, and the read is
/// what reported this Package's own diagnosis when it was already untrustworthy.
fn edit_refusal(reason: OrderEditPlanError<io::Error>, roots: &OrderRoots) -> OrderFailed {
    match reason {
        OrderEditPlanError::UnmanagedPackage
        | OrderEditPlanError::DamagedPackage
        | OrderEditPlanError::UpgradePending
        | OrderEditPlanError::PackageOperationFailed => {
            order_preconditions_changed(&roots.package_root, roots)
        }
        // The Order Directory the read located, for the reason `alignment_refusal` names
        // it: the core reports the subject it was handed, and the read holds the spelling.
        OrderEditPlanError::OrderNotFound { .. } | OrderEditPlanError::DamagedOrder { .. } => {
            order_preconditions_changed(&roots.order_root, roots)
        }
        OrderEditPlanError::InvalidAssignments(rejected) => order_mutation_refused(
            FailureCode::OrderAssignmentRejected,
            format!(
                "no assignment was applied, because an assignment set is one atomic \
                 mutation and {} refused: {}.",
                if rejected.len() == 1 {
                    "one of them was"
                } else {
                    "some of them were"
                },
                listed(&rejected)
            ),
            roots,
            vec![in_package(&roots.order_root)],
            rejected
                .iter()
                .map(|assignment| {
                    Identity::rejected_assignment(assignment.field(), assignment.reason())
                })
                .collect(),
        ),
        // Not reachable through this adapter: a CLI assignment is itself the explicit
        // confirmation a genuinely new Reusable Free-Entry Value requires, so every set
        // this command hands the core is a confirmed one. See
        // `FailureCode::OrderReusableValueUnconfirmed`.
        OrderEditPlanError::UnconfirmedReusableValue { field, proposed } => order_mutation_refused(
            FailureCode::OrderReusableValueUnconfirmed,
            format!(
                "`{proposed}` is a value this Package does not use yet, and it was proposed \
                 for `{field}` without confirmation, so nothing was written."
            ),
            roots,
            vec![in_package(&roots.order_root)],
            Vec::new(),
        ),
        OrderEditPlanError::UnknownVendor { name } => order_mutation_refused(
            FailureCode::OrderVendorUnknown,
            unknown_vendor_message(&roots.package_path().display().to_string(), &name),
            roots,
            vec![in_package(&roots.order_root)],
            vec![Identity::vendor_name(&name)],
        ),
        // **The range is the positions the *command* can address**, not the collection's
        // own: an insertion may name the place after the last Item, so `add` accepts one
        // number more than `edit` does.
        OrderEditPlanError::InvalidItemPosition {
            role,
            position,
            minimum,
            maximum,
        } => order_mutation_refused(
            FailureCode::OrderItemPositionInvalid,
            format!(
                "{position} is not a position this command can address: {}.",
                if maximum == 0 {
                    "this Order has no Items".to_owned()
                } else {
                    format!("they run from {minimum} to {maximum}")
                }
            ),
            roots,
            vec![in_package(&roots.order_root)],
            vec![Identity::item_position(role, &position.to_string())],
        ),
        OrderEditPlanError::MissingRequiredItemFacts { fields } => order_mutation_refused(
            FailureCode::OrderItemFactsRequired,
            missing_item_facts(&fields),
            roots,
            vec![in_package(&roots.order_root)],
            fields
                .iter()
                .map(|field| Identity::required_fact(field))
                .collect(),
        ),
        OrderEditPlanError::InvalidFactValue { fact, value } => {
            let (code, message, identity) = invalid_fact(fact, &value);
            order_mutation_refused(
                code,
                message,
                roots,
                vec![in_package(&roots.order_root)],
                vec![identity],
            )
        }
        OrderEditPlanError::InvalidFacts => order_mutation_refused(
            FailureCode::OrderFactsInvalid,
            "the complete Order record this mutation assembled is not valid for this \
             Package, so nothing was written."
                .to_owned(),
            roots,
            vec![in_package(&roots.order_root)],
            Vec::new(),
        ),
        OrderEditPlanError::FileSystem { path, error } => {
            order_filesystem_refusal(path, &error.to_string(), roots)
        }
    }
}

/// The refusal one assigned value the Order record cannot hold earns: its code, the
/// sentence naming the rule the value broke, and the identity quoting the value back.
///
/// **One home for all three facts**, so the same mistake is refused identically whichever
/// command carried the assignment: a Date Ordered `add-order` refuses and one `order edit`
/// refuses are one code, one sentence, and one identity. Each fact has its own code and its
/// own identity
/// `kind` because each is a different thing to do about it — a Status is one of a closed
/// set this sentence can list, a date is a calendar, and a Currency is a three-letter code.
fn invalid_fact(fact: OrderFactKind, value: &str) -> (FailureCode, String, Identity) {
    match fact {
        OrderFactKind::Status => (
            FailureCode::OrderStatusInvalid,
            format!(
                "`{value}` is not an Order Status. An Order is at one of \
                 `in_preparation`, `hold`, `ordered`, `received`, or `complete`."
            ),
            Identity::order_status(value),
        ),
        OrderFactKind::DateOrdered => (
            FailureCode::OrderDateOrderedInvalid,
            format!(
                "`{value}` is not a Date Ordered. A Date Ordered is a real calendar day \
                 written `YYYY-MM-DD`."
            ),
            Identity::order_date_ordered(value),
        ),
        OrderFactKind::Currency => (
            FailureCode::OrderCurrencyInvalid,
            format!(
                "`{value}` is not an Order Currency. An Order Currency is a three-letter \
                 uppercase code such as `USD`, and an empty value clears it."
            ),
            Identity::order_currency(value),
        ),
    }
}

/// Why the record mutation's commit was refused, as one typed failure.
///
/// One function for all five commands, because an Item mutation and a fact edit are
/// refused identically at the commit: both replace one Order record, and the rejections
/// are one set.
fn edit_commit_refused(
    rejection: OrderEditRejection<io::Error>,
    roots: &OrderRoots,
) -> OrderFailed {
    match rejection {
        OrderEditRejection::PreconditionsChanged { location } => {
            order_preconditions_changed(&location, roots)
        }
        OrderEditRejection::ReplacementFailed { error } => {
            let stage_remains = error.cleanup_failure().is_some();
            let (code, message) = match error.failure() {
                ValidatedReplacementFailure::DestinationChanged => (
                    FailureCode::OrderMutationPreconditionsChanged,
                    "the Order record changed between planning this mutation and performing \
                     it, so what would have been written is no longer what was validated. \
                     Nothing was written."
                        .to_owned(),
                ),
                // A candidate this mutation could not have produced. `OrderCandidateFailure`
                // has no variants: the record was parsed and validated when it was planned
                // and the Order was revalidated against the current Package before this
                // write, so there is no third judgement left for a candidate to fail.
                ValidatedReplacementFailure::CandidateValidation(failure) => match *failure {},
                ValidatedReplacementFailure::InvalidDestination
                | ValidatedReplacementFailure::DestinationUnreadable(_)
                | ValidatedReplacementFailure::StagingWrite(_)
                | ValidatedReplacementFailure::Durability(_)
                | ValidatedReplacementFailure::Replacement(_) => (
                    if stage_remains {
                        FailureCode::OrderMutationCleanupFailed
                    } else {
                        FailureCode::OrderMutationFailed
                    },
                    format!(
                        "this Order's record could not be replaced and holds what it held.{}",
                        if stage_remains {
                            // The stage stands beside this Order's own record, inside its
                            // reserved directory, so the finding is this Order's.
                            format!(
                                " A staging artifact this attempt created could not be \
                                 removed again; `spielorders validate` reports it and {}",
                                RepairSuggestion::for_condition(
                                    Rooting::Order,
                                    DiagnosticCode::StagingArtifactPresent,
                                    EvidenceScope::Order,
                                )
                            )
                        } else {
                            String::new()
                        }
                    ),
                ),
            };
            order_mutation_refused(
                code,
                message,
                roots,
                vec![in_package(&roots.order_root)],
                Vec::new(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// The four `order item` mutations.
// ---------------------------------------------------------------------------

/// The payload of all four Item mutations: what the mutation did to one Item, and the
/// Order whose collection it changed.
///
/// The refreshed result is [`Show`], the same projection the reads render, so a client
/// that can display an Order can display the one an Item mutation returns — with the Item
/// it is about at the position the report names, and every total recalculated.
#[derive(Debug, Serialize)]
struct ItemMutated {
    report: ItemReport,
    order: Show,
}

/// The typed report of one Item mutation.
///
/// **`item` is the Item this mutation was about**, and it is here rather than left to be
/// found in `order.items` because a removal's subject is in no collection afterwards. For
/// the other three it is the Item standing at `destination_position`, so the report and the
/// refreshed Order agree by construction.
///
/// **The two positions are what "old and new" means for a collection addressed by
/// position**, exactly as they are for a Contact mutation: where the command line
/// addressed, and where the Item was left. Each is `null` for the one end its mutation does
/// not have — an `add` takes the Item from nowhere, and a `remove` leaves it nowhere.
///
/// **`reusable_values` is absent** for a checkable reason: an Item's four facts are its
/// Description, Quantity, Unit Price, and Catalog Number, and none of them is a Reusable
/// Free-Entry Value — so these commands can neither reuse nor introduce one. The same
/// reason makes `date_ordered_source` absent: an Item mutation assigns no scalar fact, so
/// the Order's Date Ordered is the one it already carried.
#[derive(Debug, Serialize)]
struct ItemReport {
    committed: bool,
    /// Whether this mutation changes the canonical Order record. `false` for a move to the
    /// position the Item already occupies and for an edit that assigns the facts it already
    /// carries, both committed by performing nothing.
    changed: bool,
    outcome: &'static str,
    item: ItemFacts,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every mutation whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// One Item's own facts, without the position that addresses it and without its total.
///
/// **A position is a fact about the collection rather than about the Item**, so the report
/// says which positions it addressed and repeating one inside the Item would be a third
/// spelling of a number that report already carries. The Item Total is likewise absent
/// here and present in the refreshed Order's own `items`: it is calculated, so the one
/// place it belongs is beside the collection the calculation is over.
#[derive(Debug, Serialize)]
struct ItemFacts {
    description: String,
    quantity: f64,
    unit_price: f64,
    catalog_number: Option<String>,
}

impl ItemFacts {
    fn of(item: &OrderItem) -> Self {
        Self {
            description: item.description.as_str().to_owned(),
            quantity: item.quantity.get(),
            unit_price: item.unit_price.get(),
            catalog_number: item.catalog_number.clone(),
        }
    }
}

/// One decided Item mutation, in the terms every output mode reports it in.
struct ItemMutation {
    plan: OrderItemMutationPlan,
    /// See [`Edit::readme_exists`], which this carries for the same reason.
    readme_exists: bool,
    committed: Option<EditCommitted>,
}

impl ItemMutation {
    /// The Order this run has to report, and the derived index it leaves behind.
    ///
    /// [`Edit::answered`]'s counterpart, made once for every output mode for the same
    /// reason.
    fn answered(&self) -> (OrderFacts<'_>, &PackageIndexRecord) {
        match &self.committed {
            Some(committed) => (
                OrderFacts::mutated(
                    committed.result.order(),
                    committed.result.vendor(),
                    self.readme_exists,
                ),
                committed.result.index(),
            ),
            None => (
                OrderFacts::mutated(
                    self.plan.projected_order(),
                    self.plan.vendor(),
                    self.readme_exists,
                ),
                self.plan.prior_index(),
            ),
        }
    }
}

/// `order item add`: one Item appended, or inserted where `position=` says.
///
/// The insertion point is taken out of the assignment set before the rest reaches the core
/// — see [`crate::cli::insertion_assignments`] — because where an Item goes is not one of
/// its facts. Every remaining assignment is the Package's own to adjudicate.
pub(crate) fn item_add(
    target: &Target,
    assignments: &[FieldAssignment],
    position: Option<&str>,
    dry_run: bool,
    output: &Output,
) -> Exit {
    item_mutation(
        ResponseContract::OrderItemAdd,
        target,
        dry_run,
        output,
        |roots| {
            let position = insertion_point(position, roots)?;
            plan_order_item_add(
                roots.order_path(),
                &item_assigned(assignments),
                position,
                &PlatformFileSystem,
            )
            .map_err(|reason| Box::new(edit_refusal(reason, roots)))
        },
    )
}

/// `order item edit`: the facts of the Item standing at one position.
pub(crate) fn item_edit(
    target: &Target,
    position: &str,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    item_mutation(
        ResponseContract::OrderItemEdit,
        target,
        dry_run,
        output,
        |roots| {
            let position = requested_position(position, ItemPositionRole::Source, roots)?;
            plan_order_item_edit(
                roots.order_path(),
                position,
                &item_assigned(assignments),
                &PlatformFileSystem,
            )
            .map_err(|reason| Box::new(edit_refusal(reason, roots)))
        },
    )
}

/// `order item move`: one Item taken from one position and put at another.
///
/// Both positions are the core's to evaluate against the pre-mutation collection, which is
/// what makes `move 3 position=1` mean the third Item a reader saw. A destination an Item
/// already occupies changes nothing and is not an error.
pub(crate) fn item_move(
    target: &Target,
    position: &str,
    destination: &str,
    dry_run: bool,
    output: &Output,
) -> Exit {
    item_mutation(
        ResponseContract::OrderItemMove,
        target,
        dry_run,
        output,
        |roots| {
            let source = requested_position(position, ItemPositionRole::Source, roots)?;
            let destination =
                requested_position(destination, ItemPositionRole::Destination, roots)?;
            plan_order_item_move(roots.order_path(), source, destination, &PlatformFileSystem)
                .map_err(|reason| Box::new(edit_refusal(reason, roots)))
        },
    )
}

/// `order item remove`: the Item standing at one position, and no disposal.
///
/// An Item is embedded in its Order, so there is nothing to move to a Removed collection or
/// to the platform Trash: the Order record is rewritten without it. An Order's last Item
/// may go — its Items are then present and empty.
pub(crate) fn item_remove(target: &Target, position: &str, dry_run: bool, output: &Output) -> Exit {
    item_mutation(
        ResponseContract::OrderItemRemove,
        target,
        dry_run,
        output,
        |roots| {
            let position = requested_position(position, ItemPositionRole::Source, roots)?;
            plan_order_item_remove(roots.order_path(), position, &PlatformFileSystem)
                .map_err(|reason| Box::new(edit_refusal(reason, roots)))
        },
    )
}

/// Targets the Order, plans, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a write
/// path: a run that was asked to plan returns before it, so no record was replaced, no
/// staging file exists, and no index was refreshed afterward.
///
/// One function for all four, so the order in which an Item command resolves its Order and
/// reads its position is decided once: a `--root` that is no Order is reported as that
/// rather than as a position refused against a collection this command never saw.
fn item_mutation(
    contract: ResponseContract,
    target: &Target,
    dry_run: bool,
    output: &Output,
    plan: impl Fn(&OrderRoots) -> Result<OrderItemMutationPlan, Box<OrderFailed>>,
) -> Exit {
    let decided = match read(target) {
        Ok(read) => {
            let roots = OrderRoots::of(&read);
            match plan(&roots) {
                Ok(plan) => item_decided(plan, roots, read.readme().exists(), dry_run),
                Err(failed) => OrderMutation::Failed(failed),
            }
        }
        Err(failed) => OrderMutation::Failed(failed),
    };
    present_order_mutation(contract, decided, item_payload, human_item_mutation, output)
}

/// Commits one planned Item mutation, or reports the plan a dry run stops at.
fn item_decided(
    plan: OrderItemMutationPlan,
    roots: OrderRoots,
    readme_exists: bool,
    dry_run: bool,
) -> OrderMutation<ItemMutation> {
    if dry_run {
        return OrderMutation::Planned {
            roots,
            plan: Box::new(ItemMutation {
                plan,
                readme_exists,
                committed: None,
            }),
        };
    }
    let (plan, committed) =
        match after_committing(commit_order_item_mutation(plan, &PlatformFileSystem)) {
            Ok(committed) => committed,
            Err(rejection) => {
                return OrderMutation::Failed(Box::new(edit_commit_refused(rejection, &roots)));
            }
        };
    OrderMutation::Planned {
        roots,
        plan: Box::new(ItemMutation {
            plan,
            readme_exists,
            committed: Some(committed),
        }),
    }
}

/// The optional insertion point an `add` command line supplied.
///
/// `None` is the append this command's absence of a `position=` means, and it is not a
/// refusal: an Item added without one goes after the last. An insertion point is always the
/// destination end, because an `add` takes its Item from no position at all.
fn insertion_point(
    position: Option<&str>,
    roots: &OrderRoots,
) -> Result<Option<usize>, Box<OrderFailed>> {
    position
        .map(|position| requested_position(position, ItemPositionRole::Destination, roots))
        .transpose()
}

/// One position as the command line wrote it, or the typed reason it is none.
///
/// **The whole number is read here and its meaning is the core's**, which is the split that
/// makes a position a domain refusal rather than a usage error: whether a token is a number
/// is a fact about the command line, and which numbers name an Item is a fact about the
/// collection. So all four ways of naming no position — a token that is not a number, a
/// negative one, zero, and one past the end — carry the one code a caller branches on, and
/// the position it was about travels as an identity because a message is not a contract.
///
/// Zero is deliberately *not* refused here: it parses, so the core refuses it beside every
/// other out-of-range position and reports the range in the same sentence.
fn requested_position(
    position: &str,
    role: ItemPositionRole,
    roots: &OrderRoots,
) -> Result<usize, Box<OrderFailed>> {
    position.parse::<usize>().map_err(|_| {
        Box::new(order_mutation_refused(
            FailureCode::OrderItemPositionInvalid,
            format!(
                "`{position}` is not an Item position. A position is a whole number \
                 counting this Order's Items from one."
            ),
            roots,
            vec![in_package(&roots.order_root)],
            vec![Identity::item_position(role, position)],
        ))
    })
}

/// The Item facts as the core takes them, in the order the command line supplied.
///
/// Nothing here inspects a field or a value. An unknown field, a repeated one, a blank
/// required one, and a Quantity or Unit Price the record could not hold as a finite number
/// are all the core's refusals, reported together.
fn item_assigned(assignments: &[FieldAssignment]) -> ItemAssignments {
    assignments
        .iter()
        .fold(ItemAssignments::new(), |set, assignment| {
            set.assign(assignment.field.clone(), assignment.value.clone())
        })
}

/// The report, and the Order the Item mutation leaves behind.
///
/// [`edit_payload`]'s counterpart, and the same division: everything but `committed`, the
/// refreshed index, and the warnings is read off the plan, so a dry run answers with the
/// real run's report rather than with a description of a different one.
fn item_payload(decided: &ItemMutation) -> Result<ItemMutated, NotRepresentable> {
    let (facts, record) = decided.answered();
    Ok(ItemMutated {
        report: ItemReport {
            committed: decided.committed.is_some(),
            changed: decided.plan.changes_canonical_order(),
            outcome: decided.plan.kind().canonical_outcome().as_str(),
            item: ItemFacts::of(decided.plan.subject_item()),
            source_position: decided.plan.source_position(),
            destination_position: decided.plan.destination_position(),
            operations: operation_projections(&record_operations(decided.plan.operations()))?,
            index: index(record)?,
            warnings: decided
                .committed
                .as_ref()
                .map(|committed| warning_codes(&committed.warnings))
                .unwrap_or_default(),
        },
        order: Show::of(&facts)?,
    })
}

/// The human counterpart: what this command did to which Item, and the Order as it now
/// stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. It names the Item, the position or positions the report carries, and
/// nothing the machine response does not have.
fn human_item_mutation(decided: &ItemMutation, output: &Output) -> io::Result<()> {
    let (facts, record) = decided.answered();
    output.summary(
        item_outcome_label(&decided.plan, decided.committed.is_some()),
        &text(&addressed(&decided.plan)),
    )?;
    human_order(&facts, output)?;
    output.summary("Index", &text(record.state().as_str()))?;
    for (operation, location) in &record_operations(decided.plan.operations()) {
        output.detail(operation, location.absolute_path().as_os_str())?;
    }
    human_warnings(
        decided
            .committed
            .as_ref()
            .map(|committed| committed.warnings.as_slice()),
        output,
    )
}

/// The Item this command was about, at the position a reader will find it.
///
/// A move says both numbers, because the command line named one and the collection now
/// shows the other; the rest say the one they have. A removal's number is where the Item
/// stood, which is the only number it has left.
fn addressed(plan: &OrderItemMutationPlan) -> String {
    let item = item_line(plan.subject_item());
    match plan.kind() {
        OrderItemMutationKind::Add | OrderItemMutationKind::Edit => {
            numbered(plan.destination_position(), &item)
        }
        OrderItemMutationKind::Move => {
            match (plan.source_position(), plan.destination_position()) {
                (Some(source), Some(destination)) => format!("{source} to {destination}. {item}"),
                _ => item,
            }
        }
        OrderItemMutationKind::Remove => numbered(plan.source_position(), &item),
    }
}

/// One Item on one line: its Description, its Quantity as the record stores it, its Unit
/// Price as money is displayed, and its Catalog Number where it carries one.
///
/// The subject line of an Item mutation. Its calculated total is deliberately absent:
/// [`human_items`] prints the collection with every total below, and the number a mutation
/// is *about* is its position.
fn item_line(item: &OrderItem) -> String {
    let mut line = format!(
        "{}  {} x {}",
        item.description.as_str(),
        displayed_quantity(item.quantity.get()),
        displayed_amount(item.unit_price.get())
    );
    if let Some(catalog_number) = item.catalog_number.as_deref() {
        line.push_str(&format!("  ({catalog_number})"));
    }
    line
}

/// One Item line with its position, where the plan has one to give.
///
/// Every plan carries the position its kind addresses; a line without one still names the
/// Item, which is the honest fallback for a rendering that is not a parsing contract.
fn numbered(position: Option<usize>, item: &str) -> String {
    match position {
        Some(position) => format!("{position}. {item}"),
        None => item.to_owned(),
    }
}

/// The one line that says which of the four outcomes this run reached.
///
/// Committed and changed, committed and unchanged, planned and would change, planned and
/// would change nothing — all four readable without counting members, because the human
/// surface owes the same distinction the report makes. A move to the position an Item
/// already occupies is the unchanged one.
fn item_outcome_label(plan: &OrderItemMutationPlan, committed: bool) -> &'static str {
    match (committed, plan.changes_canonical_order()) {
        (true, false) | (false, false) => "Unchanged",
        (true, true) => match plan.kind() {
            OrderItemMutationKind::Add => "Added",
            OrderItemMutationKind::Edit => "Edited",
            OrderItemMutationKind::Move => "Moved",
            OrderItemMutationKind::Remove => "Removed",
        },
        (false, true) => match plan.kind() {
            OrderItemMutationKind::Add => "Would add",
            OrderItemMutationKind::Edit => "Would edit",
            OrderItemMutationKind::Move => "Would move",
            OrderItemMutationKind::Remove => "Would remove",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    // Decision sources: Decision traceability and contract-test conventions; Define the
    // Rust CLI capability contract.

    /// The refusal a record or Package moving under a mutation's feet earns reports its
    /// path the way the success beside it does.
    ///
    /// A unit test over the projection rather than a process-level one, because no
    /// process can reach this refusal: it is the window between the read that targeted
    /// the Order and the write, and a test that manufactured one would pin the harness
    /// that opened the window rather than the contract. The type is what makes the
    /// spelling reachable at all — [`order_preconditions_changed`] takes a `Location`, so
    /// a caller holding a bare path does not compile — and this is what makes the
    /// spelling *arrive*, through the same `Failure::at` projection every refusal uses.
    #[test]
    fn a_race_window_refusal_carries_the_packages_own_spelling_for_its_path() {
        let roots = OrderRoots {
            package_root: Location::within_package("/packages/optics", ".")
                .expect("a Root is absolute and spells itself `.`"),
            order_root: Location::within_package(
                "/packages/optics/Orders/2026_07_20_Laser_components",
                "Orders/2026_07_20_Laser_components",
            )
            .expect("an Order Directory is inside its Package"),
        };

        let failed = order_preconditions_changed(&roots.order_root, &roots);
        let document = refusal_document(&failed);
        assert_eq!(document["ok"], false);
        assert_eq!(
            document["error"]["code"],
            "order_mutation_preconditions_changed"
        );
        assert_eq!(
            document["error"]["paths"][0]["path"],
            "/packages/optics/Orders/2026_07_20_Laser_components"
        );
        assert_eq!(
            document["error"]["paths"][0]["package_relative_path"],
            "Orders/2026_07_20_Laser_components",
            "a refusal spells its path the way a success does"
        );
        assert_eq!(
            document["context"]["package_root"], "/packages/optics",
            "the Package this refusal happened under is the one it names"
        );

        // The Root itself is the other subject these refusals take, and every Package
        // spells its own Root `.` — so a Package-load state that moved under a mutation
        // carries a spelling too rather than falling back to `null`.
        let failed = order_preconditions_changed(&roots.package_root, &roots);
        let document = refusal_document(&failed);
        assert_eq!(document["error"]["paths"][0]["package_relative_path"], ".");
    }

    /// A root with no JSON spelling leaves the other root named.
    ///
    /// The defect this catches: the two-root context was assembled by one constructor per
    /// combination, each taking both spellings at once and failing if either had none,
    /// and the caller answered that failure with the untargeted context — so one
    /// unspellable root reported `package_root: null` *and* `order_root: null`, discarding
    /// a Root the command had established and could name. `null` is for a root that was
    /// not established, and each root's own spelling is what decides its own member.
    ///
    /// A unit test rather than a process-level one, for the reason the Package-level
    /// claim's is one: APFS refuses a name that is not valid UTF-8, so this repository's
    /// own platform cannot hold the fixture a real process would need. Which context a
    /// response carries is decided here, and is exercisable without such a name existing.
    #[test]
    fn a_root_with_no_spelling_leaves_the_other_root_named() {
        let unspellable =
            PathBuf::from(OsString::from_vec(b"/packages/optics/Orders/\xff".to_vec()));
        let spellable = Path::new("/packages/optics");

        let package_only = OrderContext::of(Some(spellable), Some(&unspellable));
        let document = serde_json::to_value(&package_only).expect("the context serializes");
        assert_eq!(
            document["package_root"], "/packages/optics",
            "the Package this command resolved is named"
        );
        assert!(
            document["order_root"].is_null(),
            "and the root with no spelling is the only one that goes null"
        );

        let order_only = OrderContext::of(Some(&unspellable), Some(spellable));
        let document = serde_json::to_value(&order_only).expect("the context serializes");
        assert!(document["package_root"].is_null());
        assert_eq!(
            document["order_root"], "/packages/optics",
            "and the same holds the other way round"
        );

        // Neither established is still neither named, so the two cannot be collapsed.
        let neither = OrderContext::of(None, None);
        let document = serde_json::to_value(&neither).expect("the context serializes");
        assert!(document["package_root"].is_null());
        assert!(document["order_root"].is_null());
    }

    /// One refused `order` command, as the response contract carries it.
    fn refusal_document(failed: &OrderFailed) -> serde_json::Value {
        let response: Response<(), OrderContext> = failed_document(
            ResponseContract::OrderEdit,
            failed_order_context(failed),
            &failed.failed,
        )
        .expect("every path here has a JSON spelling");
        serde_json::to_value(&response).expect("the response serializes")
    }
}
