//! The `package` family.
//!
//! Decision sources: Define the Rust CLI capability contract; Define the SpielOrders
//! package topology; Define integrity, validation, and repair boundaries; Define
//! durable metadata schemas and version evolution.

use crate::{
    cli::ValueKind,
    commands::shared::{
        DiagnosticGroupProjection, Failed, IndexProjection, MetadataProjection,
        OrderPathsProjection, VendorSummary, as_given, assessed_package_candidate,
        diagnostic_groups, diagnostics, failed_context, failed_document, flattened, index,
        metadata, not_representable, operation_failure, package_candidate, package_context,
        report_diagnostic, text, undeliverable, unmanaged_failure,
    },
    contract::ResponseContract,
    effects::PlatformFileSystem,
    exit::Exit,
    money::displayed_amount,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, LocationProjection, NotRepresentable, Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, DamagedOrder, HealthyOrder, ManagedContext,
        ReusableValueProjection, assess_package_index, inspect_canonical_package,
    },
    metadata::{Vendor, Version1},
    readme::{Diagnostic, Location, ReadmeRead, read_readme},
};
use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

/// A loaded Package, or the typed reason there is none to report.
///
/// **Every command that reports what is *inside* a Package goes through one of the two
/// loads below to get one of these**, so the load-state-to-exit-status mapping is one
/// mapping rather than seven. Two commands answering an unmanaged directory differently —
/// or one of them folding a Package at an uninterpretable metadata version into plain
/// damage — is a defect that appears in neither command's own diff, and this is the shape
/// in which it cannot happen. The two loads differ only in what they ask of the stored
/// derived index; both classify through the same shared mapping.
enum PackageContents {
    Loaded(Box<CurrentPackageCandidate>),
    Failed(Failed),
}

/// The reading load, and one typed answer for every state it can report.
///
/// `inspect_canonical_package` rather than `open_package`: the opening load durably
/// replaces the derived Package Index as a side effect, and
/// `docs/architecture/durable-writes.md` reserves the rebuilding load for a place a
/// canonical write has already happened. Reading a Package writes nothing.
///
/// It asks the stored index nothing at all, which is right for the three narrower reads:
/// they report Orders, Vendors, and value lists, and none of them reports index state.
///
/// **A Package that is damaged, unmanaged, or awaiting an upgrade answers too, because
/// saying so is the answer.** Each state gets its own code, so a consumer branches on
/// what is wrong rather than on one generic failure, and the states come from the core's
/// typed result rather than being inferred from anything this adapter observed.
///
/// The *classification* is what is shared here, and it is shared because these commands
/// all report Package contents: contents are exactly what a Package the load cannot trust
/// has none of. `package readme` classifies differently on purpose — where a README would
/// be is a fact about topology, so it answers a damaged Package rather than failing on
/// one.
fn load_package_contents(selected_root: &Path) -> PackageContents {
    contents(package_candidate(
        selected_root,
        inspect_canonical_package(selected_root, &PlatformFileSystem),
    ))
}

/// The same read, plus the answer to what state the stored derived index is in.
///
/// **`package show` alone**, because it is the only Package-contents read whose contract
/// carries index state. `assess_package_index` is the core's non-rebuilding assessment: it
/// takes the canonical load and judges the stored index against it, and it writes nothing,
/// so a read stays a read. It is what `index status` answers from too, which is what keeps
/// the complete Package read and the command whose whole subject is the index from coming
/// to disagree about one Package.
fn load_assessed_package_contents(selected_root: &Path) -> PackageContents {
    contents(assessed_package_candidate(
        selected_root,
        assess_package_index(selected_root, &PlatformFileSystem),
    ))
}

fn contents(candidate: Result<CurrentPackageCandidate, Box<Failed>>) -> PackageContents {
    match candidate {
        Ok(package) => PackageContents::Loaded(Box::new(package)),
        Err(failed) => PackageContents::Failed(*failed),
    }
}

// ---------------------------------------------------------------------------
// `package readme`
// ---------------------------------------------------------------------------

const README_CONTRACT: ResponseContract = ResponseContract::PackageReadme;

/// The `package readme` payload: where the conventional Package README is, and whether
/// it is presently there.
///
/// The location comes from the core's own `ManagedContext`. The adapter never joins
/// `README.md` onto a root: which document is conventional is a Package rule, and an
/// adapter that assembled the path itself would be a second place that decides it.
#[derive(Debug, Serialize)]
struct Readme {
    location: LocationProjection,
    exists: bool,
}

/// What the command decided, before anything is written anywhere.
///
/// Deciding and presenting are separate so that the four output modes present one
/// answer rather than each recomputing it — the shape in which two modes come to
/// disagree about the same run.
enum ReadmeAnswer {
    Answered {
        package_root: PathBuf,
        readme: Location,
        exists: bool,
        diagnostics: Vec<Diagnostic>,
    },
    Failed(Failed),
}

pub(crate) fn readme(selected_root: &Path, output: &Output) -> Exit {
    present_readme(decide_readme(selected_root), output)
}

/// The target reaches the core exactly as the caller wrote it.
///
/// The adapter resolves nothing. Making a relative `--root` absolute here would be a
/// Package rule decided twice: the core canonicalizes the selected root through the
/// filesystem port because its own rules require it, and it reports a root it cannot
/// resolve as a typed operational failure naming the caller's own text. Joining the
/// process working directory on first would hide that answer behind a path this adapter
/// invented — and did, which is why a `--root` that does not resolve never reached the
/// core at all.
///
/// No ancestor is consulted anywhere on this path, so a directory inside a Package is
/// not that Package.
fn decide_readme(selected_root: &Path) -> ReadmeAnswer {
    // The reading load. `open_package` would durably replace the derived Package Index
    // as a side effect, and `docs/architecture/durable-writes.md` reserves the
    // rebuilding load for the place a canonical write has already happened. A command
    // that only reports where a document is does not write one.
    let load = inspect_canonical_package(selected_root, &PlatformFileSystem);
    let context = match &load {
        CanonicalPackageLoad::CurrentCandidate(package) => Some(package.context()),
        CanonicalPackageLoad::UpgradePending(package) => Some(package.context()),
        CanonicalPackageLoad::DiagnosticOnly(package) => package.context(),
        CanonicalPackageLoad::Unmanaged(_) | CanonicalPackageLoad::OperationFailed(_) => None,
    };

    match context {
        Some(context) => readme_of(context),
        None => readme_unavailable(&load, selected_root),
    }
}

/// The conventional README of a Package whose managed paths the core established.
///
/// Establishing the context is the whole precondition. Where the README is, is a fact
/// about the Root's topology and about nothing else: the optional document's absence
/// does not damage a Package, and adjudicating the Package's health is `validate`'s
/// contract rather than this one's. So a Package carrying diagnostics still answers
/// this question, and a Root whose managed paths could not be resolved has no answer to
/// give.
fn readme_of(context: &ManagedContext) -> ReadmeAnswer {
    let reference = context.package_readme();
    match read_readme(reference, &PlatformFileSystem) {
        Ok(read) => ReadmeAnswer::Answered {
            package_root: context.root().absolute_path().to_path_buf(),
            readme: reference.location().clone(),
            exists: read.exists(),
            // The document's own diagnostics, which are the observations about the
            // subject in `data`. The Package load's diagnostics are not forwarded
            // here; reporting Package health belongs to the commands whose contracts
            // are about it.
            diagnostics: read.diagnostics().to_vec(),
        },
        // The Root was established — that is what produced this `context` — so the
        // failure reports it even though the document under it could not be read.
        Err(failure) => ReadmeAnswer::Failed(operation_failure(&failure, Some(context.root()))),
    }
}

/// The `selected_root` here is the caller's own text, because that is what this command
/// was given and it resolved nothing. Where the core resolved something it is the core's
/// path that gets reported; `selected_root` is the fallback for the one load that
/// establishes no path this adapter can see.
fn readme_unavailable(load: &CanonicalPackageLoad, selected_root: &Path) -> ReadmeAnswer {
    ReadmeAnswer::Failed(match load {
        CanonicalPackageLoad::Unmanaged(package) => unmanaged_failure(package),
        // A damaged Package resolved its Root — the load got past canonicalization —
        // but this projection exposes no `Location` for it when it has no managed
        // context, so the only path this command can name is the one it was given.
        CanonicalPackageLoad::DiagnosticOnly(_) => Failed {
            code: FailureCode::PackageRootDamaged,
            scope: FailureScope::Package,
            message: format!(
                "the Package at {} is damaged in a way that leaves it with no managed \
                 paths to report. Run `spielorders validate` for the diagnosis.",
                selected_root.display()
            ),
            paths: vec![as_given(selected_root)],
            identities: Vec::new(),
            // This projection exposes no `Location` for the Root it resolved when it
            // has no managed context, so there is none here to name.
            package_root: None,
            // Reporting a damaged Package's health is `package show` and `validate`,
            // and this command duplicating it would give one fact two contracts.
            diagnostics: Vec::new(),
        },
        CanonicalPackageLoad::OperationFailed(package) => {
            operation_failure(package.failure(), package.root())
        }
        // Both are contexts this function is only ever reached without.
        CanonicalPackageLoad::CurrentCandidate(_) | CanonicalPackageLoad::UpgradePending(_) => {
            Failed {
                code: FailureCode::PackageUnreadable,
                scope: FailureScope::Operation,
                message: "the Package could not be read".to_owned(),
                paths: vec![as_given(selected_root)],
                identities: Vec::new(),
                // This arm is reached by no load, so it establishes nothing to name; the
                // two states above it are handled where they are actually met.
                package_root: None,
                diagnostics: Vec::new(),
            }
        }
    })
}

fn present_readme(answer: ReadmeAnswer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match readme_document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(README_CONTRACT, package_context(readme_root(&answer))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_readme(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the load established one.
fn readme_root(answer: &ReadmeAnswer) -> Option<&Path> {
    match answer {
        ReadmeAnswer::Answered { package_root, .. } => Some(package_root.as_path()),
        ReadmeAnswer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn readme_document(answer: &ReadmeAnswer) -> Result<(Response<Readme>, Exit), NotRepresentable> {
    match answer {
        ReadmeAnswer::Answered {
            package_root,
            readme,
            exists,
            diagnostics: observed,
        } => Ok((
            Response::answered(
                README_CONTRACT,
                Context::rooted_at(package_root)?,
                Readme {
                    location: LocationProjection::of(readme)?,
                    exists: *exists,
                },
                diagnostics(observed)?,
            ),
            Exit::Success,
        )),
        ReadmeAnswer::Failed(failed) => Ok((
            failed_document(README_CONTRACT, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

fn human_readme(answer: &ReadmeAnswer, output: &Output) -> io::Result<Exit> {
    match answer {
        ReadmeAnswer::Answered {
            package_root,
            readme,
            exists,
            diagnostics,
        } => {
            output.result_path(readme.absolute_path())?;
            output.detail("Package Root", package_root.as_os_str())?;
            output.detail(
                "Package-relative path",
                OsStr::new(readme.package_relative_path().unwrap_or("")),
            )?;
            output.detail("Exists", OsStr::new(if *exists { "yes" } else { "no" }))?;
            for diagnostic in diagnostics {
                report_diagnostic(output, Some(package_root), diagnostic)?;
            }
            Ok(Exit::Success)
        }
        ReadmeAnswer::Failed(failed) => {
            output.error(&failed.message)?;
            Ok(Exit::Failure)
        }
    }
}

// ---------------------------------------------------------------------------
// `package show`
// ---------------------------------------------------------------------------

const SHOW_CONTRACT: ResponseContract = ResponseContract::PackageShow;

/// The `package show` payload: everything a client needs to open this Package.
///
/// Every member is a projection of the core's own typed result. The adapter computes no
/// total, resolves no Vendor, derives no value list, and invents no diagnostic — where a
/// fact here looks derived, the derivation happened in the core and this reads its
/// answer.
#[derive(Debug, Serialize)]
struct Show {
    paths: PackagePaths,
    metadata: MetadataProjection,
    readme: ReadmeProjection,
    orders: Vec<OrderProjection>,
    damaged_orders: Vec<DamagedOrderProjection>,
    vendors: Vec<VendorProjection>,
    /// Whether the `vendors` list above is the complete one.
    ///
    /// The default omits Hidden Vendors, so without this a consumer cannot tell a
    /// Vendor that is hidden from one that does not exist. It is a fact about the answer
    /// rather than about the Package, which is why it sits beside the list.
    hidden_vendors_included: bool,
    values: ValueLists,
    index: IndexProjection,
    /// The **Package load's** diagnostics, at the narrowest scope the core assigned them.
    ///
    /// This is where the grouping survives: a group's `location` is the Order Directory
    /// or the Package Root that owns it, which is the one fact a flat list cannot carry,
    /// and it is what attributes an Order-scoped diagnostic to the Damaged Order
    /// placeholder that names the same directory. The core groups; the adapter does not,
    /// because a grouping this adapter invented would be a domain decision.
    ///
    /// **The envelope's `diagnostics` is a superset of what is here, not a copy of it.**
    /// It carries every observation *this read* made — these groups plus the Package
    /// README read's own, which the load did not perform and therefore did not group. The
    /// envelope is the member a consumer reads without knowing which command produced the
    /// response, and it is the only channel a failed read has at all, since `data` is
    /// `null` then; this member is the load's own structure, which only a successful read
    /// has to report.
    diagnostic_groups: Vec<DiagnosticGroupProjection>,
}

/// The managed locations that have nowhere else in this payload to be named.
///
/// **Every location appears exactly once.** The Root is not here because
/// `context.package_root` carries it, and a root's own Package-relative spelling is `.`
/// and says nothing. The conventional README is `readme.location` and the derived index
/// is `index.location`, each beside the state that is the reason a client asks about it —
/// the same shape `package readme` uses, and the shape an Order's `paths` uses for the
/// locations that have no state of their own.
#[derive(Debug, Serialize)]
struct PackagePaths {
    metadata_directory: LocationProjection,
    config: LocationProjection,
    version: LocationProjection,
    vendors: LocationProjection,
    /// `null` where the load established no Orders Directory for this Package.
    orders_directory: Option<LocationProjection>,
    /// `null` where the load established no Removed Orders Directory. It is created
    /// lazily, so its absence is ordinary.
    removed_orders_directory: Option<LocationProjection>,
}

/// The Package README: where it is, whether it is there, and what it holds.
#[derive(Debug, Serialize)]
struct ReadmeProjection {
    location: LocationProjection,
    exists: bool,
    /// The document's text. An absent README reads as `""`, because absence is a text:
    /// there is nothing written. `null` says the opposite — this read does not know what
    /// the document holds, because it exists and could not be read, is not valid UTF-8,
    /// or is not a file at all. The diagnostics say which. Collapsing the two would
    /// report a file that holds bytes as one a reader could safely treat as blank.
    text: Option<String>,
}

/// One healthy Order, complete.
#[derive(Debug, Serialize)]
struct OrderProjection {
    paths: OrderPathsProjection,
    /// This Order record's own declared schema version, as the record declares it. The
    /// core's marker type serializes itself, so the adapter carries no version number of
    /// its own for a fact the record owns.
    schema_version: Version1,
    /// The technical Package locator this record carries — the relative path from the
    /// Order Directory back to its Package Root.
    package_root: String,
    order_id: String,
    title: String,
    date_ordered: String,
    status: &'static str,
    /// The Vendor this Order names, as the record carries it. `null` when it names none.
    vendor_id: Option<String>,
    /// That Vendor as the core resolved it, Hidden Vendors included — hiding governs
    /// what is offered for new use and never what is resolved, so an Order referencing a
    /// Hidden Vendor still reports its facts whether or not `--all` was supplied.
    vendor: Option<VendorSummary>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
    items: Vec<ItemProjection>,
    /// The Order Total the core calculated. The adapter computes no total.
    order_total: f64,
    /// The Order README's text, with the same meaning `readme.text` has above.
    readme_text: Option<String>,
}

#[derive(Debug, Serialize)]
struct ItemProjection {
    description: String,
    quantity: f64,
    unit_price: f64,
    catalog_number: Option<String>,
    /// The Item Total the core calculated. The adapter multiplies nothing.
    ///
    /// Every Item the core reports has one. `Option` only because the types cannot prove
    /// that — the core exposes the totals as a list beside the Items — and the
    /// alternatives are worse: asserting the lengths agree is a panic, which is an
    /// undecided fourth exit status, and substituting a number would invent a domain
    /// fact. Pairing them in the core instead is the milestone's ordinary escape and is
    /// the right fix if a second caller ever needs the pair; nothing about this response
    /// contract changes when it happens, because the member is already one per Item.
    item_total: Option<f64>,
}

/// One Damaged Order, as a placeholder rather than as an Order.
///
/// A Damaged Order is never reported among the healthy ones: its record could not be
/// read or did not validate, so it has no facts to report. Its diagnostics are the
/// group in `diagnostic_groups` whose `location` is this `paths.directory`.
#[derive(Debug, Serialize)]
struct DamagedOrderProjection {
    paths: OrderPathsProjection,
    /// The identity the record claimed, where the load could trust one. `null` where the
    /// record was too damaged to read an identity from — the core does not guess one.
    order_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct VendorProjection {
    vendor_id: String,
    name: String,
    phone: Option<String>,
    email: Option<String>,
    website: Option<String>,
    address: Option<String>,
    purchasing_notes: Option<String>,
    /// Whether this is a Hidden Vendor: still resolvable for every Order that references
    /// it, no longer offered for new use.
    hidden: bool,
    contacts: Vec<ContactProjection>,
}

#[derive(Debug, Serialize)]
struct ContactProjection {
    name: String,
    role: Option<String>,
    phone: Option<String>,
    email: Option<String>,
}

/// The four Reusable Free-Entry Value lists, each in its canonical spelling with its
/// usage count.
///
/// The core derives all four once from the loaded Package. The adapter does not scan
/// Order Directories and does not recompute a list per dropdown.
#[derive(Debug, Serialize)]
struct ValueLists {
    ordering_institutions: Vec<ReusableValue>,
    orderers: Vec<ReusableValue>,
    projects: Vec<ReusableValue>,
    order_groups: Vec<ReusableValue>,
}

#[derive(Debug, Serialize)]
struct ReusableValue {
    value: String,
    usage_count: usize,
}

/// What the command decided, before anything is written anywhere.
///
/// The answered arm holds the core's own values rather than a projection of them, so the
/// human and machine modes present one answer instead of each recomputing it.
enum ShowAnswer {
    Answered {
        package: Box<CurrentPackageCandidate>,
        readme: ReadmeRead,
        /// Whether Hidden Vendors were asked for. A presentation choice, so it is
        /// carried into presenting rather than applied to the core's answer.
        all: bool,
    },
    Failed(Failed),
}

pub(crate) fn show(selected_root: &Path, all: bool, output: &Output) -> Exit {
    present_show(decide_show(selected_root, all), output)
}

/// The shared Package-contents load, plus the one subject that is this command's alone.
///
/// The load states and their codes come from the same classification every command
/// reporting Package contents uses; this one is the load that also assesses the derived
/// index, because this command's contract carries its state. The Package README is what
/// `package show` adds: it is a second subject rather than a narrower view of the first,
/// which is why this command has a decision of its own at all.
fn decide_show(selected_root: &Path, all: bool) -> ShowAnswer {
    match load_assessed_package_contents(selected_root) {
        PackageContents::Loaded(package) => {
            // The Package README, read through the core's own reference. The adapter
            // joins no managed path: which document is conventional is a Package rule.
            match read_readme(package.context().package_readme(), &PlatformFileSystem) {
                Ok(readme) => ShowAnswer::Answered {
                    package,
                    readme,
                    all,
                },
                // The Root was established, so the failure reports it even though the
                // document under it could not be read.
                Err(failure) => {
                    ShowAnswer::Failed(operation_failure(&failure, Some(package.context().root())))
                }
            }
        }
        PackageContents::Failed(failed) => ShowAnswer::Failed(failed),
    }
}

fn present_show(answer: ShowAnswer, output: &Output) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match show_document(&answer) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(SHOW_CONTRACT, package_context(show_root(&answer))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_show(&answer, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the load established one.
fn show_root(answer: &ShowAnswer) -> Option<&Path> {
    match answer {
        ShowAnswer::Answered { package, .. } => Some(package.context().root().absolute_path()),
        ShowAnswer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn show_document(answer: &ShowAnswer) -> Result<(Response<Show>, Exit), NotRepresentable> {
    match answer {
        ShowAnswer::Answered {
            package,
            readme,
            all,
        } => {
            let context = package.context();
            // Every diagnostic the load grouped, plus the README read's own. The
            // envelope's flat list is what a consumer reads without knowing this
            // command; the grouping travels in `data`.
            let mut observed = flattened(package.diagnostic_groups());
            observed.extend(readme.diagnostics().iter().cloned());
            Ok((
                Response::answered(
                    SHOW_CONTRACT,
                    Context::rooted_at(context.root().absolute_path())?,
                    Show {
                        paths: package_paths(context)?,
                        metadata: metadata(package.metadata_version(), package.metadata_state()),
                        readme: ReadmeProjection {
                            location: LocationProjection::of(readme.reference().location())?,
                            exists: readme.exists(),
                            text: readme.text().map(str::to_owned),
                        },
                        orders: orders(package)?,
                        damaged_orders: package
                            .damaged_orders()
                            .iter()
                            .map(damaged_order)
                            .collect::<Result<_, _>>()?,
                        vendors: presented_vendors(package, *all)
                            .into_iter()
                            .map(vendor)
                            .collect(),
                        hidden_vendors_included: *all,
                        values: ValueLists {
                            ordering_institutions: values_of(package.ordering_institutions()),
                            orderers: values_of(package.orderers()),
                            projects: values_of(package.projects()),
                            order_groups: values_of(package.order_groups()),
                        },
                        index: index(package.index())?,
                        diagnostic_groups: diagnostic_groups(package.diagnostic_groups())?,
                    },
                    diagnostics(&observed)?,
                ),
                Exit::Success,
            ))
        }
        ShowAnswer::Failed(failed) => Ok((
            failed_document(SHOW_CONTRACT, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The Vendors this run presents.
///
/// **The default is the Vendors this Package offers for new use, and `--all` is the
/// complete list.** Both come from the core, which exposes each as its own list and
/// filters neither: this default is the CLI's presentation choice, and the desktop
/// Vendors table consumes the complete list for its show/hide control. Every Vendor
/// carries its visibility either way, so the shape does not depend on the flag.
fn presented_vendors(package: &CurrentPackageCandidate, all: bool) -> Vec<&Vendor> {
    if all {
        package.vendors().iter().collect()
    } else {
        package.offered_vendors()
    }
}

fn package_paths(context: &ManagedContext) -> Result<PackagePaths, NotRepresentable> {
    Ok(PackagePaths {
        metadata_directory: LocationProjection::of(context.metadata_directory())?,
        config: LocationProjection::of(context.config())?,
        version: LocationProjection::of(context.version())?,
        vendors: LocationProjection::of(context.vendors())?,
        orders_directory: optional_location(context.orders_directory())?,
        removed_orders_directory: optional_location(context.removed_orders_directory())?,
    })
}

fn optional_location(
    location: Option<&Location>,
) -> Result<Option<LocationProjection>, NotRepresentable> {
    location.map(LocationProjection::of).transpose()
}

/// Every healthy Order, each with the Vendor the core resolved for it.
///
/// The Vendor comes from `vendor_for_order`, which searches the complete Vendor list, so
/// a Hidden Vendor an Order references resolves exactly as any other does. Resolving it
/// against the presented list instead would make an Order's Vendor disappear when
/// `--all` was omitted, which is the defect the visibility default is most likely to
/// cause.
fn orders(package: &CurrentPackageCandidate) -> Result<Vec<OrderProjection>, NotRepresentable> {
    package
        .healthy_orders()
        .iter()
        .enumerate()
        .map(|(index, order)| order_projection(order, package.vendor_for_order(index)))
        .collect()
}

fn order_projection(
    order: &HealthyOrder,
    resolved: Option<&Vendor>,
) -> Result<OrderProjection, NotRepresentable> {
    let document = order.document();
    let totals = order.item_totals();
    Ok(OrderProjection {
        paths: OrderPathsProjection::of(order.paths())?,
        schema_version: document.schema_version,
        package_root: order.package_root().as_str().to_owned(),
        order_id: document.order_id.as_str().to_owned(),
        title: document.title.as_str().to_owned(),
        date_ordered: document.date_ordered.as_str().to_owned(),
        status: document.status.as_str(),
        vendor_id: document
            .vendor_id
            .as_ref()
            .map(|vendor_id| vendor_id.as_str().to_owned()),
        vendor: resolved.map(VendorSummary::of),
        orderer: document.orderer.clone(),
        ordering_institution: document.ordering_institution.clone(),
        currency: document
            .currency
            .as_ref()
            .map(|currency| currency.as_str().to_owned()),
        project: document.project.clone(),
        order_group: document.order_group.clone(),
        items: document
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| ItemProjection {
                description: item.description.as_str().to_owned(),
                quantity: item.quantity.get(),
                unit_price: item.unit_price.get(),
                catalog_number: item.catalog_number.clone(),
                item_total: totals.get(position).copied(),
            })
            .collect(),
        order_total: order.order_total(),
        readme_text: order.readme_text().map(str::to_owned),
    })
}

fn damaged_order(order: &DamagedOrder) -> Result<DamagedOrderProjection, NotRepresentable> {
    Ok(DamagedOrderProjection {
        paths: OrderPathsProjection::of(order.paths())?,
        order_id: order
            .order_id()
            .map(|order_id| order_id.as_str().to_owned()),
    })
}

fn vendor(vendor: &Vendor) -> VendorProjection {
    VendorProjection {
        vendor_id: vendor.vendor_id.as_str().to_owned(),
        name: vendor.name.as_str().to_owned(),
        phone: vendor.phone.clone(),
        email: vendor.email.clone(),
        website: vendor.website.clone(),
        address: vendor.address.clone(),
        purchasing_notes: vendor.purchasing_notes.clone(),
        hidden: vendor.is_hidden(),
        contacts: vendor
            .contacts
            .iter()
            .map(|contact| ContactProjection {
                name: contact.name.as_str().to_owned(),
                role: contact.role.clone(),
                phone: contact.phone.clone(),
                email: contact.email.clone(),
            })
            .collect(),
    }
}

/// One Reusable Free-Entry Value list, as every command that reports one projects it.
///
/// The canonical spelling and the usage count are the core's: it groups by the decided
/// trimmed, case-insensitive comparison and counts the healthy Orders in each group. This
/// neither trims, nor compares, nor counts.
fn values_of(projected: &[ReusableValueProjection]) -> Vec<ReusableValue> {
    projected
        .iter()
        .map(|value| ReusableValue {
            value: value.value().to_owned(),
            usage_count: value.usage_count(),
        })
        .collect()
}

/// A concise labelled summary, naming the same entities the machine response carries.
///
/// Human output is never a parsing contract, so this may be reworded without advancing
/// the response contract. UUIDs appear only under `--verbose`: a human reading a Package
/// wants titles and names, and the machine response carries the identities always.
fn human_show(answer: &ShowAnswer, output: &Output) -> io::Result<Exit> {
    match answer {
        ShowAnswer::Answered {
            package,
            readme,
            all,
        } => {
            let context = package.context();
            output.summary("Package Root", context.root().absolute_path().as_os_str())?;
            output.summary(
                "Metadata",
                &text(&format!(
                    "version {}, {}",
                    package.metadata_version(),
                    package.metadata_state().as_str()
                )),
            )?;
            output.summary(
                "README",
                &text(match (readme.exists(), readme.text()) {
                    (false, _) => "absent",
                    (true, Some(_)) => "present",
                    (true, None) => "present but unreadable",
                }),
            )?;
            output.detail(
                "Package README path",
                readme.reference().location().absolute_path().as_os_str(),
            )?;

            human_orders(package, output)?;
            human_damaged_orders(package, output)?;
            human_vendors(package, *all, output)?;
            human_values(package, output)?;

            output.summary("Index", &text(package.index().state().as_str()))?;
            let root = context.root().absolute_path();
            for group in package.diagnostic_groups() {
                for diagnostic in group.diagnostics() {
                    report_diagnostic(output, Some(root), diagnostic)?;
                }
            }
            for diagnostic in readme.diagnostics() {
                report_diagnostic(output, Some(root), diagnostic)?;
            }
            Ok(Exit::Success)
        }
        ShowAnswer::Failed(failed) => {
            output.error(&failed.message)?;
            for diagnostic in &failed.diagnostics {
                report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

fn human_orders(package: &CurrentPackageCandidate, output: &Output) -> io::Result<()> {
    if package.healthy_orders().is_empty() {
        return output.summary("Orders", &text("none"));
    }
    for (index, order) in package.healthy_orders().iter().enumerate() {
        let document = order.document();
        let vendor = match package.vendor_for_order(index) {
            Some(vendor) => format!("  {}", vendor.name.as_str()),
            None => String::new(),
        };
        output.summary(
            "Order",
            &text(&format!(
                "{}  {}  {}{vendor}  {} item(s), total {}",
                document.date_ordered.as_str(),
                document.title.as_str(),
                document.status.as_str(),
                document.items.len(),
                // The core's unrounded total, rendered for a reader by the one thing
                // that renders money in this crate. The number itself is untouched:
                // `OrderProjection::order_total` carries it into `package show`'s
                // response exactly as the core calculated it.
                //
                // This line serves `package list orders` too, and that command's
                // response carries no total at all — deliberately, as `OrderSummary`
                // argues below. Sharing the line is what keeps the two commands from
                // describing one Order two ways; it is not a claim that they carry the
                // same members.
                displayed_amount(order.order_total())
            )),
        )?;
        output.detail("Order ID", OsStr::new(document.order_id.as_str()))?;
    }
    Ok(())
}

fn human_damaged_orders(package: &CurrentPackageCandidate, output: &Output) -> io::Result<()> {
    if package.damaged_orders().is_empty() {
        return output.summary("Damaged Orders", &text("none"));
    }
    for order in package.damaged_orders() {
        let directory = order.paths().directory();
        // The Package-relative spelling where the entry has one, and the absolute path
        // where it has none — an entry whose name this application cannot express is
        // exactly the one a reader most needs named, and an empty value names nothing.
        output.summary(
            "Damaged Order",
            match directory.package_relative_path() {
                Some(relative) => OsStr::new(relative),
                None => directory.absolute_path().as_os_str(),
            },
        )?;
        output.detail("Damaged Order path", directory.absolute_path().as_os_str())?;
        output.detail(
            "Damaged Order ID",
            OsStr::new(
                order
                    .order_id()
                    .map(|order_id| order_id.as_str())
                    .unwrap_or("unknown"),
            ),
        )?;
    }
    Ok(())
}

fn human_vendors(package: &CurrentPackageCandidate, all: bool, output: &Output) -> io::Result<()> {
    let presented = presented_vendors(package, all);
    if presented.is_empty() {
        output.summary("Vendors", &text("none"))?;
    }
    for vendor in &presented {
        output.summary(
            "Vendor",
            &text(&if vendor.is_hidden() {
                format!("{} (hidden)", vendor.name.as_str())
            } else {
                vendor.name.as_str().to_owned()
            }),
        )?;
        output.detail("Vendor ID", OsStr::new(vendor.vendor_id.as_str()))?;
    }
    // Said whether or not any Vendor is hidden, because the point is what the list is:
    // without it a reader cannot tell a Vendor that is hidden from one that is absent.
    output.summary(
        "Hidden Vendors",
        &text(if all { "included" } else { "omitted" }),
    )
}

fn human_values(package: &CurrentPackageCandidate, output: &Output) -> io::Result<()> {
    for kind in VALUE_KINDS {
        let facts = value_kind(kind);
        human_value_list(facts.label, (facts.list)(package), output)?;
    }
    Ok(())
}

/// One Reusable Free-Entry Value list, as both `package show` and `package values` print
/// it.
///
/// Shared rather than written twice, so the narrower command and the complete read cannot
/// come to render one list differently — which is a disagreement no reader would notice
/// until they ran both.
///
/// Each list says `none` on its own, as the Orders, Damaged Orders, and Vendors above
/// already do. Reporting only when a list has values leaves a reader unable to tell an
/// empty list from one the summary forgot — and a Package with three populated lists and
/// one empty is the ordinary case, not the empty Package.
fn human_value_list(
    label: &str,
    list: &[ReusableValueProjection],
    output: &Output,
) -> io::Result<()> {
    if list.is_empty() {
        return output.summary(label, &text("none"));
    }
    for value in list {
        output.summary(
            label,
            &text(&format!("{} ({})", value.value(), value.usage_count())),
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `package list orders`, `package list vendors`, and `package values <kind>`
// ---------------------------------------------------------------------------
//
// Six commands, six independently versioned contracts, and one presenter. Each is a
// narrower view of the projection `package show` already presents: the core derives the
// Orders, the Vendors, and the four value lists once from the loaded Package, and nothing
// below recomputes an ordering, a value list, or a Vendor resolution.

/// The `package list orders` payload: what Orders are here.
///
/// Two collections rather than one, with the same names and the same meanings
/// `package show` gives them. **A Damaged Order is never in `orders`**, because its record
/// could not be read and it has no facts to report — and it is never absent either,
/// because a listing that dropped an Order Directory would say the Order is gone.
#[derive(Debug, Serialize)]
struct OrderListing {
    orders: Vec<OrderSummary>,
    damaged_orders: Vec<DamagedOrderSummary>,
}

/// One Order, in the least a listing needs to name it.
///
/// The identity, the Order Directory root, the four facts an Order carries at every
/// status, and the Vendor the core resolved for it. The Items, the calculated totals, the
/// Order Currency, the Reusable Free-Entry Values, and the README text are the *complete*
/// read — `package show` and, for one Order, `order show` — and a contract that carried
/// them would be one of those under another name.
///
/// **The exclusion is about this contract, not about the human mode.** `list_orders`
/// hands `present_contents` the same summary line `package show` prints, which names the
/// Item count and the calculated Order Total — sharing the line is what keeps the two
/// commands from describing one Order two ways, and human output is never a parsing
/// contract. A consumer that needs a total asks the command whose contract carries one.
#[derive(Debug, Serialize)]
struct OrderSummary {
    /// The Order Directory root, which is what makes a listed Order actionable.
    ///
    /// Named directly rather than nested in a `paths` object holding one member: the
    /// Order's metadata directory, record, and README are locations the *complete* read
    /// reports, and a listing is about where the Order is.
    directory: LocationProjection,
    order_id: String,
    title: String,
    date_ordered: String,
    status: &'static str,
    /// The Vendor this Order names, as the record carries it. `null` when it names none.
    vendor_id: Option<String>,
    /// That Vendor as the core resolved it, Hidden Vendors included.
    ///
    /// **This command has no `--all`, and needs none.** Hiding governs what a Package
    /// offers for new use and never what is resolved, so resolution goes through the
    /// core's `vendor_for_order`, which searches the complete list. Resolving against the
    /// presented list is the defect the Vendor visibility default is most likely to
    /// cause, and a command with no such option would carry it silently.
    vendor: Option<VendorSummary>,
}

/// One Damaged Order, as a placeholder rather than as an Order.
///
/// The same two facts `package show`'s placeholder carries: where the Order Directory is,
/// and the identity the load could still trust. `null` where the record was too damaged
/// to read one — the core does not guess, and neither does this.
///
/// No inline diagnostics, for the reason `package show` has none: `DamagedOrder` exposes
/// none, and inlining them would mean this adapter constructing a grouping. The load's
/// observations travel in the envelope's `diagnostics`.
#[derive(Debug, Serialize)]
struct DamagedOrderSummary {
    directory: LocationProjection,
    order_id: Option<String>,
}

/// The `package list vendors` payload: what Vendors this Package can use.
#[derive(Debug, Serialize)]
struct VendorListing {
    vendors: Vec<VendorSummary>,
    /// Whether the `vendors` list above is the complete one.
    ///
    /// **The same member `package show` carries, for the same reason and with the same
    /// default.** Without it a consumer cannot tell a Vendor that is hidden from one that
    /// does not exist. It is a fact about the answer rather than about the Package, which
    /// is why it sits beside the list.
    hidden_vendors_included: bool,
}

/// The `package values <kind>` payload: one Reusable Free-Entry Value list.
///
/// One member named `values` in all four contracts rather than the plural category name,
/// because the contract identity is what says *which* list this is. Four sibling contracts
/// with one payload shape means one consumer shape reads all four; repeating the kind
/// inside `data` would put one fact in two members of the same document with nothing
/// binding them.
#[derive(Debug, Serialize)]
struct ValueListing {
    values: Vec<ReusableValue>,
}

/// The four Reusable Free-Entry Value kinds, in the one order this application uses.
///
/// `ValueKind`'s declaration order, which is `package show`'s published `values` payload
/// order — see that type for why. One order for the command line's possible values, for
/// `package show`'s summary, and for anything that iterates the four is one fewer
/// convention to keep in step.
///
/// A literal, so a fifth category would compile against it untouched and go missing from
/// every iteration here while its own command worked. `every_value_kind_is_iterated` in
/// `tests/package_lists_and_values.rs` binds this array to `ValueKind`'s variants so that
/// cannot happen quietly.
const VALUE_KINDS: [ValueKind; 4] = [
    ValueKind::OrderingInstitution,
    ValueKind::Orderer,
    ValueKind::Project,
    ValueKind::OrderGroup,
];

/// Everything one Reusable Free-Entry Value kind decides.
struct ValueKindFacts {
    /// This kind's own independently versioned response contract.
    contract: ResponseContract,
    /// The label both `package show` and `package values` print this list under, so one
    /// list reads the same however it was asked for.
    label: &'static str,
    /// The core list this kind reports. A function pointer because the list belongs to a
    /// loaded Package and this decision does not.
    list: fn(&CurrentPackageCandidate) -> &[ReusableValueProjection],
}

/// One exhaustive match rather than three, so a kind cannot come to carry one contract
/// identity here and a different label there — and a fifth category, if the domain ever
/// gained one, is a compile error until all three are supplied.
fn value_kind(kind: ValueKind) -> ValueKindFacts {
    match kind {
        ValueKind::Project => ValueKindFacts {
            contract: ResponseContract::PackageValuesProject,
            label: "Projects",
            list: CurrentPackageCandidate::projects,
        },
        ValueKind::Orderer => ValueKindFacts {
            contract: ResponseContract::PackageValuesOrderer,
            label: "Orderers",
            list: CurrentPackageCandidate::orderers,
        },
        ValueKind::OrderingInstitution => ValueKindFacts {
            contract: ResponseContract::PackageValuesOrderingInstitution,
            label: "Ordering institutions",
            list: CurrentPackageCandidate::ordering_institutions,
        },
        ValueKind::OrderGroup => ValueKindFacts {
            contract: ResponseContract::PackageValuesOrderGroup,
            label: "Order groups",
            list: CurrentPackageCandidate::order_groups,
        },
    }
}

pub(crate) fn list_orders(selected_root: &Path, output: &Output) -> Exit {
    present_contents(
        ResponseContract::PackageListOrders,
        &load_package_contents(selected_root),
        order_listing,
        |package, output| {
            human_orders(package, output)?;
            human_damaged_orders(package, output)
        },
        output,
    )
}

pub(crate) fn list_vendors(selected_root: &Path, all: bool, output: &Output) -> Exit {
    present_contents(
        ResponseContract::PackageListVendors,
        &load_package_contents(selected_root),
        |package| Ok(vendor_listing(package, all)),
        |package, output| human_vendors(package, all, output),
        output,
    )
}

pub(crate) fn values(selected_root: &Path, kind: ValueKind, output: &Output) -> Exit {
    let ValueKindFacts {
        contract,
        label,
        list,
    } = value_kind(kind);
    present_contents(
        contract,
        &load_package_contents(selected_root),
        |package| {
            Ok(ValueListing {
                values: values_of(list(package)),
            })
        },
        |package, output| human_value_list(label, list(package), output),
        output,
    )
}

/// Presents one narrower view of a loaded Package.
///
/// **All six of this ticket's commands go through here**, so the envelope, the exit
/// status, which diagnostics the answer carries, the unrepresentable-path fallback, and
/// the undeliverable-result fallback are decided in one place for all six rather than six
/// times. Two of these commands disagreeing about an empty collection, about a load state,
/// or about whether a Damaged Order's diagnostic travels is the defect this ticket's
/// review boundary exists to catch, and this is the shape in which it cannot happen.
///
/// **The load's diagnostics travel in the envelope**, because each of these six reports a
/// projection of what one load could read, and the envelope is where a response says what
/// its read observed. That is `diagnostics` being "about the subject in `data`" for a
/// command whose subject is the Package's contents — as against `package readme`, whose
/// subject is a path and which forwards none of them.
///
/// For the four value lists the consequence is sharper than for the two listings. A
/// Damaged Order contributes to no value list, so `package values project` over a Package
/// holding one answers with a smaller count than the same Package undamaged; without the
/// observation beside it, that partial answer would be indistinguishable from a complete
/// one. `package list orders` reports the Damaged Order itself and `package list vendors`
/// is unaffected by one, so for those two the diagnostics are the read reporting what it
/// saw rather than a caveat on the answer.
///
/// The load's *grouping* is not repeated here — the flat envelope list is the
/// cross-command contract, and `package show` is the command whose scope is the grouping.
fn present_contents<T: Serialize>(
    contract: ResponseContract,
    contents: &PackageContents,
    project: impl Fn(&CurrentPackageCandidate) -> Result<T, NotRepresentable>,
    human: impl Fn(&CurrentPackageCandidate, &Output) -> io::Result<()>,
    output: &Output,
) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match contents_document(contract, contents, project) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(contract, package_context(contents_root(contents))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_contents(contents, human, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the load established one.
fn contents_root(contents: &PackageContents) -> Option<&Path> {
    match contents {
        PackageContents::Loaded(package) => Some(package.context().root().absolute_path()),
        PackageContents::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn contents_document<T: Serialize>(
    contract: ResponseContract,
    contents: &PackageContents,
    project: impl Fn(&CurrentPackageCandidate) -> Result<T, NotRepresentable>,
) -> Result<(Response<T>, Exit), NotRepresentable> {
    match contents {
        PackageContents::Loaded(package) => Ok((
            Response::answered(
                contract,
                Context::rooted_at(package.context().root().absolute_path())?,
                project(package)?,
                diagnostics(&flattened(package.diagnostic_groups()))?,
            ),
            Exit::Success,
        )),
        PackageContents::Failed(failed) => Ok((
            failed_document(contract, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The human counterpart, with the diagnostics routed to the stream each severity earns.
///
/// The `--verbose` Package Root line is here rather than in each command, because which
/// Package a listing is of is the same detail for all six — and the machine response
/// carries it in `context` in every mode.
fn human_contents(
    contents: &PackageContents,
    human: impl Fn(&CurrentPackageCandidate, &Output) -> io::Result<()>,
    output: &Output,
) -> io::Result<Exit> {
    match contents {
        PackageContents::Loaded(package) => {
            output.detail(
                "Package Root",
                package.context().root().absolute_path().as_os_str(),
            )?;
            human(package, output)?;
            for group in package.diagnostic_groups() {
                for diagnostic in group.diagnostics() {
                    report_diagnostic(
                        output,
                        Some(package.context().root().absolute_path()),
                        diagnostic,
                    )?;
                }
            }
            Ok(Exit::Success)
        }
        PackageContents::Failed(failed) => {
            output.error(&failed.message)?;
            for diagnostic in &failed.diagnostics {
                report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

fn order_listing(package: &CurrentPackageCandidate) -> Result<OrderListing, NotRepresentable> {
    Ok(OrderListing {
        // Presented in the order the core sorted them into — newest Date Ordered first,
        // then Title, then Order ID — which is the decided default ordering and the one
        // the stored Package Index is validated against. Nothing here sorts.
        orders: package
            .healthy_orders()
            .iter()
            .enumerate()
            .map(|(index, order)| order_summary(order, package.vendor_for_order(index)))
            .collect::<Result<_, _>>()?,
        damaged_orders: package
            .damaged_orders()
            .iter()
            .map(damaged_order_summary)
            .collect::<Result<_, _>>()?,
    })
}

/// One Order summary, with the Vendor the core resolved for it.
///
/// The Vendor arrives as an argument for the same reason it does in `package show`: it
/// comes from `vendor_for_order`, which searches the complete Vendor list, and an adapter
/// that looked one up itself would be a second place resolving a Vendor.
fn order_summary(
    order: &HealthyOrder,
    resolved: Option<&Vendor>,
) -> Result<OrderSummary, NotRepresentable> {
    let document = order.document();
    Ok(OrderSummary {
        directory: LocationProjection::of(order.paths().directory())?,
        order_id: document.order_id.as_str().to_owned(),
        title: document.title.as_str().to_owned(),
        date_ordered: document.date_ordered.as_str().to_owned(),
        status: document.status.as_str(),
        vendor_id: document
            .vendor_id
            .as_ref()
            .map(|vendor_id| vendor_id.as_str().to_owned()),
        vendor: resolved.map(VendorSummary::of),
    })
}

fn damaged_order_summary(order: &DamagedOrder) -> Result<DamagedOrderSummary, NotRepresentable> {
    Ok(DamagedOrderSummary {
        directory: LocationProjection::of(order.paths().directory())?,
        order_id: order
            .order_id()
            .map(|order_id| order_id.as_str().to_owned()),
    })
}

/// The Vendors this run presents, in the core's own order.
///
/// `presented_vendors` is the one adapter function that decides the visibility default,
/// and it is shared with `package show` rather than copied — which is what makes the two
/// commands' defaults the same default instead of two that happen to agree.
fn vendor_listing(package: &CurrentPackageCandidate, all: bool) -> VendorListing {
    VendorListing {
        vendors: presented_vendors(package, all)
            .into_iter()
            .map(VendorSummary::of)
            .collect(),
        hidden_vendors_included: all,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Decision sources: Define the Rust CLI capability contract.

    /// A path with no JSON spelling still names the Root the command was about.
    ///
    /// The defect this catches is the one Ticket 002's second pass found one layer down:
    /// a failure reporting `package_root: null` while the load had established a Root.
    /// `package show` projects every managed location and every load diagnostic, so the
    /// ordinary case here is an unrepresentable path *inside* a Package whose Root is
    /// perfectly representable — and a consumer reading `context` to learn what the
    /// command was about would get nothing.
    ///
    /// A unit test rather than a process-level one because a filesystem name that is not
    /// valid UTF-8 cannot be created on this repository's own platform: APFS rejects one.
    /// The claim is about which context this response carries, and that is exercisable
    /// without such a name existing.
    #[test]
    fn an_unrepresentable_path_still_names_the_root_the_command_was_about() {
        let rooted: Response<()> = not_representable(
            SHOW_CONTRACT,
            package_context(Some(Path::new("/packages/optics"))),
        );
        let document = serde_json::to_value(&rooted).expect("the response serializes");
        assert_eq!(document["context"]["package_root"], "/packages/optics");
        assert_eq!(document["error"]["code"], "path_not_representable");
        assert_eq!(document["ok"], false);

        // The converse, so the two cannot be collapsed: where even the Root has no
        // spelling there is genuinely none to name.
        let rootless: Response<()> = not_representable(SHOW_CONTRACT, package_context(None));
        let document = serde_json::to_value(&rootless).expect("the response serializes");
        assert!(document["context"]["package_root"].is_null());
    }
}
