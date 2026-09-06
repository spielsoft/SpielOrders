//! The Vendor subject: two reads of one Vendor, the four mutations of one, the four
//! mutations of its ordered Contacts, and the Name that selects all of them.
//!
//! **A Vendor command is Package-rooted**, because Vendors have no directories. `--root`
//! names the SpielOrders Root exactly, as it does for a Package-level command, and the
//! Vendor is named by `NAME` — resolved by the core, which owns the trimmed,
//! case-insensitive, package-unique comparison. Nothing here trims a Name, folds its
//! case, compares it, counts what references a Vendor, or decides which field of a Vendor
//! may be assigned.
//!
//! **There is no selection by Vendor ID.** Names are the selector and identities are
//! returned: every machine response here carries the Vendor ID and the Order IDs whether
//! or not the human surface shows them. `add-vendor` is the one command with no selector
//! at all, because the Vendor it is about does not exist yet.
//!
//! **The reads are one projection, and so is a mutation's refreshed Vendor.** `vendor
//! show`, `vendor contact list`, and all eight mutations render one [`Show`] — so they
//! cannot come to disagree about Contact numbering, about which Orders reference a Vendor,
//! or about how a Hidden Vendor is answered. What a mutation adds is a report of what it
//! did, beside that projection rather than woven into it.
//!
//! **A Contact has no identity, so it is addressed by position.** Contacts are embedded in
//! the Vendor that owns them, and the positions [`ContactProjection`] reports are the
//! numbers the four Contact mutations take — one collection, numbered once. Which numbers
//! name a Contact is the collection's own business, so a position reaches the core as the
//! whole number the command line wrote and every way of writing one that names no Contact
//! is one refused mutation under one code.
//!
//! **Every mutation resolves its subject before it plans.** For the three that name a
//! Vendor that is the resolving read; for `add-vendor` it is the Package itself, which has
//! no Name to resolve and still owes a response that says which Package it was about and
//! what it observed. A question does not write — see `docs/architecture/durable-writes.md`
//! — so this costs a read and buys every refusal the Root, the record, and the diagnosis.
//!
//! Decision sources: Define the Rust CLI capability contract; Define Vendor identity and
//! historical semantics; Durable managed-file writes.

use crate::{
    cli::FieldAssignment,
    commands::{
        repair_suggestion::{RepairSuggestion, Rooting},
        shared::{
            Failed, IndexProjection, damaged_failure, diagnostics, failed_context, failed_document,
            in_package, index, not_representable, operation_failure, package_candidate,
            package_context, report_diagnostic, resolved, text, undeliverable, unmanaged_failure,
            upgrade_pending_failure,
        },
    },
    contract::ResponseContract,
    effects::{PlatformFileSystem, PlatformUuids},
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, Identity, LocationProjection, NotRepresentable,
        Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    canonical::{ManagedContext, PackageIndexRecord, inspect_canonical_package},
    filesystem::{ValidatedReplacementError, ValidatedReplacementFailure},
    metadata::{Vendor, VendorContact},
    mutation::{MutationReport, RejectedAssignment},
    operations::FailedPath,
    readme::{DiagnosticCode, EvidenceScope},
    vendor_reads::{VendorRead, VendorReadFailure, read_vendor_by_name},
    vendors::{
        ContactAssignments, ContactPositionRole, PlannedVendorFacts, ReferencingOrderSummary,
        VendorAssignments, VendorCandidateFailure, VendorCapabilityError,
        VendorContactMutationKind, VendorContactMutationPlan, VendorMutationKind,
        VendorMutationOperation, VendorMutationPlan, VendorMutationRejection, VendorMutationResult,
        VendorMutationWarning, commit_vendor_contact_mutation, commit_vendor_mutation,
        plan_vendor_contact_add, plan_vendor_contact_edit, plan_vendor_contact_move,
        plan_vendor_contact_remove, plan_vendor_creation, plan_vendor_edit, plan_vendor_hide,
        plan_vendor_unhide,
    },
};
use std::{ffi::OsStr, io, path::Path};

/// The Vendor this command read, or the typed reason there is none to report.
enum Answer {
    Answered(Box<VendorRead>),
    Failed(Failed),
}

/// Every way a `vendor` read can fail, mapped to one stable code.
///
/// The four Package-load states go through the shared constructors unchanged, so a
/// `vendor` command reports an unmanaged, damaged, upgrade-pending, or unreadable Package
/// with the same code and the same message a Package-level command does. The two that are
/// this family's own are the Name that resolved to nothing and the Package that has no
/// Vendors to resolve it against.
fn failure(reason: VendorReadFailure) -> Failed {
    match reason {
        VendorReadFailure::VendorNameNotFound { context, name } => unresolved(
            FailureCode::VendorNotFound,
            format!(
                "the Package at {} has no Vendor named `{name}`. A Vendor Name resolves \
                 trimmed and without regard to letter case, and `spielorders package list \
                 vendors --all` lists every Vendor this Package has.",
                context.root().absolute_path().display()
            ),
            &context,
            &name,
        ),
        VendorReadFailure::PackageHasNoVendors { context, name } => unresolved(
            FailureCode::PackageHasNoVendors,
            format!(
                "the Package at {} has no Vendors at all, so nothing in it is named \
                 `{name}`. `spielorders add-vendor` is what creates one.",
                context.root().absolute_path().display()
            ),
            &context,
            &name,
        ),
        VendorReadFailure::UnmanagedPackage(package) => unmanaged_failure(&package),
        VendorReadFailure::DamagedPackage { root, projection } => {
            damaged_failure(&projection, &root)
        }
        VendorReadFailure::UpgradePending(package) => upgrade_pending_failure(&package),
        VendorReadFailure::PackageOperationFailed(package) => {
            operation_failure(package.failure(), package.root())
        }
    }
}

/// A Name this Package resolved to no Vendor, under either of the two codes for it.
///
/// The path is the Vendor record, because the Vendor aggregate is one Package-level
/// document and it is the document that holds no such Vendor — and the identity is the
/// Name, because a Vendor that did not resolve has no Vendor ID to report.
fn unresolved(code: FailureCode, message: String, context: &ManagedContext, name: &str) -> Failed {
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths: vec![context.vendors().clone().into()],
        identities: vec![Identity::vendor_name(name)],
        // The Root resolved and the Package loaded — that is how this read knows there is
        // no such Vendor — so the context names what the command was about.
        package_root: Some(context.root().absolute_path().to_path_buf()),
        // Reporting this Package's health belongs to the commands whose contracts are
        // about it. What this failure observed is that a Name matched nothing.
        diagnostics: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The payload.
// ---------------------------------------------------------------------------

/// One Vendor, complete: the `vendor show` payload, and the refreshed result every
/// Vendor mutation returns beside its report.
///
/// Every fact is the current record's, resolved live: an Order retains no Vendor
/// snapshot, so a renamed Vendor reads as its new Name here and everywhere that resolves
/// it. The listing summary — identity, Name, visibility — is
/// [`crate::commands::shared::VendorSummary`]; this is the complete read, and the
/// Contacts carry the positions that address them.
#[derive(Debug, Serialize)]
struct Show {
    /// `null` in exactly one case: a creation that was planned and not committed. A
    /// Vendor ID is minted at the commit, so a `--dry-run` creation has no identity to
    /// report and reporting the planning placeholder would be reporting a Vendor ID that
    /// will never exist. Every other response here carries the record's own.
    vendor_id: Option<String>,
    /// The Vendor Name as the record stores it, which is the canonical spelling however
    /// the caller spelled the Name that resolved to it.
    name: String,
    phone: Option<String>,
    email: Option<String>,
    website: Option<String>,
    /// The one multiline address, as stored. Newlines are the record's own.
    address: Option<String>,
    purchasing_notes: Option<String>,
    /// Whether this is a Hidden Vendor: still resolvable for every Order that references
    /// it, no longer offered for new use. A reported fact and never a refusal — both
    /// commands answer a Hidden Vendor exactly as they answer any other.
    hidden: bool,
    contacts: Vec<ContactProjection>,
    /// The live Orders whose records name this Vendor.
    ///
    /// Exactly the Orders that resolve here, present and empty for a Vendor nothing
    /// references. A Damaged Order is in no such list: its record could not be read, so
    /// the Vendor it names is unknown — which is why the envelope's `diagnostics` matters
    /// to this answer, and is where the load says so.
    referencing_orders: Vec<ReferencingOrderProjection>,
}

/// The payload of all four Vendor mutations: what the mutation did, and the Vendor it
/// leaves behind.
///
/// The two members the mutation contract calls for — the typed report, and the refreshed
/// affected result. The refreshed result is [`Show`], the same projection the reads
/// render, so a client that can display a Vendor can display the one a mutation returns
/// without a second read and without a second shape to learn.
#[derive(Debug, Serialize)]
struct Mutated {
    report: Report,
    vendor: Show,
}

/// The typed mutation report.
///
/// `committed` is whether the plan was carried out, and it is the one member `--dry-run`
/// decides. `changed` is whether the mutation changes the Vendor record at all, which is a
/// fact about the plan rather than about whether it ran — so hiding an already Hidden
/// Vendor reports `committed: true, changed: false`, which is the idempotent no-op and not
/// an error. The four outcomes the vocabulary calls for are therefore all distinguishable:
/// committed is `ok` with `changed`, unchanged is `ok` without it, committed-with-warnings
/// is `ok` with a non-empty `warnings`, and a rejected mutation is `ok: false` with no
/// `data` at all.
///
/// **`outcome` is the canonical half of "canonical versus derived" and `index` is the
/// derived half.** A committed canonical mutation whose index refresh failed says so here
/// and in `warnings`, and still exits `0` — the record was replaced, and re-running the
/// mutation is not the action, so nothing about this report is retry-shaped.
///
/// **There are no old-and-new paths**, because a Vendor mutation moves nothing: Vendors
/// have no directories, and the one document it replaces is in `operations`. A member with
/// nothing to say would be a member every consumer had to learn to ignore.
#[derive(Debug, Serialize)]
struct Report {
    committed: bool,
    changed: bool,
    outcome: &'static str,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every mutation whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// One planned managed operation, in the core's own vocabulary.
///
/// No `destination_kind` beside the operation, which is where this differs from `init`'s:
/// every artifact a Vendor mutation writes is a managed file, so the member would carry
/// one value forever.
#[derive(Debug, Serialize)]
struct OperationProjection {
    operation: &'static str,
    location: LocationProjection,
}

impl OperationProjection {
    fn of(operation: &VendorMutationOperation) -> Result<Self, NotRepresentable> {
        Ok(Self {
            operation: operation.kind().as_str(),
            location: LocationProjection::of(operation.location())?,
        })
    }
}

/// One Vendor Contact, at its one-based position.
///
/// **`position` is carried rather than left to the array index**, because one-based
/// positions are how every Contact mutation addresses a Contact — `vendor contact edit
/// NAME 2`, `vendor contact move NAME 2 position=1` — and an off-by-one between the number
/// a reader sees and the number a mutation takes is exactly the defect a contract should
/// settle. The order is the stored collection's, which is user-controlled; nothing here
/// sorts.
#[derive(Debug, Serialize)]
struct ContactProjection {
    position: usize,
    #[serde(flatten)]
    facts: ContactFacts,
}

/// One Contact's own facts, without the position that addresses it.
///
/// **A position is a fact about the collection rather than about the Contact**, which is
/// why it is separable at all: a Contact carries a Name and its optional role, phone, and
/// email wherever it is reported, and the number beside it depends on where in the
/// collection it stands. A Contact mutation's report says which positions it addressed, so
/// repeating one inside the Contact would be a third spelling of a number that report
/// already carries.
#[derive(Debug, Serialize)]
struct ContactFacts {
    name: String,
    role: Option<String>,
    phone: Option<String>,
    email: Option<String>,
}

impl ContactFacts {
    fn of(contact: &VendorContact) -> Self {
        Self {
            name: contact.name.as_str().to_owned(),
            role: contact.role.clone(),
            phone: contact.phone.clone(),
            email: contact.email.clone(),
        }
    }
}

/// One Order referencing this Vendor, in the least a summary needs to name it.
///
/// The same four facts and the same Order Directory `package list orders` reports, and no
/// Vendor member: the Vendor is the subject of this response, and one fact carried twice
/// reads as two.
#[derive(Debug, Serialize)]
struct ReferencingOrderProjection {
    directory: LocationProjection,
    order_id: String,
    title: String,
    date_ordered: String,
    status: &'static str,
}

// ---------------------------------------------------------------------------
// The two commands.
// ---------------------------------------------------------------------------

pub(crate) fn show(selected_root: &Path, name: &str, output: &Output) -> Exit {
    present(
        ResponseContract::VendorShow,
        selected_root,
        name,
        showing,
        human_show,
        output,
    )
}

/// One Vendor's facts, borrowed from wherever this run has them.
///
/// **The one place the two sources are reconciled.** A committed mutation and a read both
/// have a stored [`Vendor`]; a `--dry-run` has a plan's [`PlannedVendorFacts`], which is
/// the same facts before any record carries them. Both surfaces are built from this — the
/// machine projection by [`Show::of`] and the human rendering by [`human_vendor`] — so a
/// fact reaches a reader the same way however the run got it, and neither surface is a
/// projection of the other.
///
/// Borrowed rather than owned, and the human surface is why: a path that is not valid
/// UTF-8 has no JSON spelling and is reported as an operational failure under `--json`,
/// while human output writes its own bytes. Going through an owned projection would make
/// the human modes fail on a Package the machine mode is merely unable to describe.
struct VendorFacts<'a> {
    vendor_id: Option<&'a str>,
    name: &'a str,
    phone: Option<&'a str>,
    email: Option<&'a str>,
    website: Option<&'a str>,
    address: Option<&'a str>,
    purchasing_notes: Option<&'a str>,
    hidden: bool,
    contacts: &'a [VendorContact],
    referencing_orders: &'a [ReferencingOrderSummary],
}

impl<'a> VendorFacts<'a> {
    /// One Vendor as a record holds it: every read, and every committed mutation.
    fn stored(vendor: &'a Vendor, referencing_orders: &'a [ReferencingOrderSummary]) -> Self {
        Self {
            vendor_id: Some(vendor.vendor_id.as_str()),
            name: vendor.name.as_str(),
            phone: vendor.phone.as_deref(),
            email: vendor.email.as_deref(),
            website: vendor.website.as_deref(),
            address: vendor.address.as_deref(),
            purchasing_notes: vendor.purchasing_notes.as_deref(),
            hidden: vendor.is_hidden(),
            contacts: &vendor.contacts,
            referencing_orders,
        }
    }

    /// One Vendor as a plan would leave it, for the run that stops before the commit.
    ///
    /// The two constructors agreeing is not left to inspection:
    /// `tests/vendor_mutations.rs` compares a dry run's Vendor against the real run's, so
    /// a fact one of them forgot is a failure rather than a silence.
    fn planned(
        facts: &'a PlannedVendorFacts,
        referencing_orders: &'a [ReferencingOrderSummary],
    ) -> Self {
        Self {
            vendor_id: facts.vendor_id(),
            name: facts.name(),
            phone: facts.phone(),
            email: facts.email(),
            website: facts.website(),
            address: facts.address(),
            purchasing_notes: facts.purchasing_notes(),
            hidden: facts.hidden(),
            contacts: facts.contacts(),
            referencing_orders,
        }
    }
}

/// The one projection of one Vendor, which every command here shares.
///
/// See [`ResponseContract::VendorShow`], which names the decision the sharing comes from.
fn showing(read: &VendorRead) -> Result<Show, NotRepresentable> {
    Show::of(&VendorFacts::stored(
        read.vendor(),
        read.referencing_orders(),
    ))
}

impl Show {
    fn of(facts: &VendorFacts<'_>) -> Result<Self, NotRepresentable> {
        Ok(Self {
            vendor_id: facts.vendor_id.map(str::to_owned),
            name: facts.name.to_owned(),
            phone: facts.phone.map(str::to_owned),
            email: facts.email.map(str::to_owned),
            website: facts.website.map(str::to_owned),
            address: facts.address.map(str::to_owned),
            purchasing_notes: facts.purchasing_notes.map(str::to_owned),
            hidden: facts.hidden,
            contacts: contacts(facts.contacts),
            referencing_orders: facts
                .referencing_orders
                .iter()
                .map(referencing_order)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// `vendor contact list` answers with `vendor show`'s response, rendered down to the
/// Contacts.
///
/// It carries that contract's identity and emits its document byte for byte; the human
/// rendering below is the whole difference between the two command lines. See [`showing`].
pub(crate) fn contact_list(selected_root: &Path, name: &str, output: &Output) -> Exit {
    present(
        ResponseContract::VendorShow,
        selected_root,
        name,
        showing,
        |read, output| {
            // Which Vendor these Contacts belong to, which the machine response says in
            // `vendor_id`. Named here rather than in the shared Contact lines because
            // `vendor show` already names it in its own summary, and one fact printed
            // twice reads as two.
            output.detail("Vendor ID", OsStr::new(read.vendor().vendor_id.as_str()))?;
            human_contacts(&read.vendor().contacts, output)
        },
        output,
    )
}

/// The Contacts of one Vendor, numbered from one in the stored collection's own order.
fn contacts(contacts: &[VendorContact]) -> Vec<ContactProjection> {
    contacts
        .iter()
        .enumerate()
        .map(|(index, contact)| ContactProjection {
            position: index + 1,
            facts: ContactFacts::of(contact),
        })
        .collect()
}

fn referencing_order(
    order: &ReferencingOrderSummary,
) -> Result<ReferencingOrderProjection, NotRepresentable> {
    Ok(ReferencingOrderProjection {
        directory: LocationProjection::of(order.directory())?,
        order_id: order.order_id().to_owned(),
        title: order.title().to_owned(),
        date_ordered: order.date_ordered().to_owned(),
        status: order.status().as_str(),
    })
}

// ---------------------------------------------------------------------------
// Presenting, once for both.
// ---------------------------------------------------------------------------

/// Presents one read of one Vendor.
///
/// **Both commands go through here**, so the envelope, the context, the exit status,
/// which diagnostics the answer carries, the unrepresentable-path fallback, and the
/// undeliverable-result fallback are decided once for both rather than twice. The two
/// disagreeing about an empty Contact collection, about a Hidden Vendor, or about whether
/// the load's observation travels is the defect this shape makes impossible.
///
/// **The load's diagnostics travel in the envelope**, because both commands report a
/// projection of what one load of one Package observed, and the envelope is where a
/// response says what its read saw. It matters most where the answer is partial: a
/// Damaged Order names no Vendor, so it appears in no referencing summary, and without
/// the observation beside it a shortened list would be indistinguishable from a complete
/// one.
///
/// The Name reaches the core as the caller wrote it. Resolving it is the Package's rule,
/// so this adapter has no business normalizing a domain value on the way in.
fn present<T: Serialize>(
    contract: ResponseContract,
    selected_root: &Path,
    name: &str,
    project: impl Fn(&VendorRead) -> Result<T, NotRepresentable>,
    human: impl Fn(&VendorRead, &Output) -> io::Result<()>,
    output: &Output,
) -> Exit {
    let answer = match read_vendor_by_name(selected_root, name, &PlatformFileSystem) {
        Ok(read) => Answer::Answered(Box::new(read)),
        Err(reason) => Answer::Failed(failure(reason)),
    };
    deliver(
        contract,
        root_of(&answer),
        || document(contract, &answer, project),
        |output| human_answer(&answer, human, output),
        output,
    )
}

/// Writes one decided answer in the mode this run selected.
///
/// **The one place a `vendor` answer becomes bytes**, shared by the reads and by both
/// families of mutation — so which stream an answer goes to, what a path with no JSON
/// spelling does to it, and what an undeliverable result exits with are decided once for
/// all ten command lines. Three copies of this dispatch is how one of them comes to exit
/// `0` after failing to write its own result.
///
/// The document is built lazily because it is only built in one mode, and building it is
/// what can discover that a path has no JSON spelling.
fn deliver<T: Serialize>(
    contract: ResponseContract,
    package_root: Option<&Path>,
    document: impl FnOnce() -> Result<(Response<T>, Exit), NotRepresentable>,
    human: impl FnOnce(&Output) -> io::Result<Exit>,
    output: &Output,
) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match document() {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(contract, package_context(package_root)),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human(output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the load established one.
fn root_of(answer: &Answer) -> Option<&Path> {
    match answer {
        Answer::Answered(read) => Some(read.context().root().absolute_path()),
        Answer::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn document<T: Serialize>(
    contract: ResponseContract,
    answer: &Answer,
    project: impl Fn(&VendorRead) -> Result<T, NotRepresentable>,
) -> Result<(Response<T>, Exit), NotRepresentable> {
    match answer {
        Answer::Answered(read) => Ok((
            Response::answered(
                contract,
                Context::rooted_at(read.context().root().absolute_path())?,
                project(read)?,
                diagnostics(read.diagnostics())?,
            ),
            Exit::Success,
        )),
        Answer::Failed(failed) => Ok((
            failed_document(contract, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The human counterpart, with the diagnostics routed to the stream each severity earns.
///
/// The Package Root is `--verbose` detail for both commands, because which Package a read
/// is of is the same detail for both — and the machine response carries it in `context` in
/// every mode.
fn human_answer(
    answer: &Answer,
    human: impl Fn(&VendorRead, &Output) -> io::Result<()>,
    output: &Output,
) -> io::Result<Exit> {
    match answer {
        Answer::Answered(read) => {
            human(read, output)?;
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
            output.error(&failed.message)?;
            for diagnostic in &failed.diagnostics {
                report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

/// A concise labelled summary naming the same Vendor the machine response carries.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. UUIDs appear only under `--verbose`: a human reading a Vendor wants
/// its Name, its shared facts, its Contacts, and what it is used by, and the machine
/// response carries the identities always.
fn human_show(read: &VendorRead, output: &Output) -> io::Result<()> {
    human_vendor(
        "Vendor",
        &VendorFacts::stored(read.vendor(), read.referencing_orders()),
        output,
    )
}

/// One Vendor's facts, its Contacts, and what it is used by, as every command that
/// reports a whole Vendor prints it.
///
/// **The label is the caller's**, which is the whole difference between a read and a
/// mutation on this surface: `Vendor` for the read, and what the mutation did for the
/// four that change one. Everything below the label is one rendering, so a Vendor read
/// after an edit cannot come to display differently from the edit that produced it.
fn human_vendor(label: &str, vendor: &VendorFacts<'_>, output: &Output) -> io::Result<()> {
    output.summary(label, &text(&named(vendor)))?;
    // A planned creation has no Vendor ID yet, and the label already says the mutation
    // was not carried out. A line reporting one would have to invent it.
    if let Some(vendor_id) = vendor.vendor_id {
        output.detail("Vendor ID", OsStr::new(vendor_id))?;
    }
    for (label, value) in [
        ("Phone", vendor.phone),
        ("Email", vendor.email),
        ("Website", vendor.website),
        ("Address", vendor.address),
        ("Purchasing notes", vendor.purchasing_notes),
    ] {
        output.summary(label, &text(&continued(value.unwrap_or("none"))))?;
    }
    human_contacts(vendor.contacts, output)?;
    human_referencing_orders(vendor.referencing_orders, output)
}

/// One shared fact, with any continuation lines indented under its label.
///
/// The address and the purchasing notes are the two Vendor facts that carry newlines, and
/// a second line flush against the margin reads as a fact of its own whose label went
/// missing. Applied to every fact rather than to those two, because a single-line value
/// passes through untouched and a special case would be one more thing to keep in step.
/// The machine response carries the record's own text either way.
fn continued(value: &str) -> String {
    value.replace('\n', "\n  ")
}

/// One Vendor's Name, saying so where it is a Hidden Vendor.
///
/// Said because it is the fact that governs whether this Vendor is offered for a new
/// Order, and a reader who cannot see it would have to ask another command.
fn named(vendor: &VendorFacts<'_>) -> String {
    if vendor.hidden {
        format!("{} (hidden)", vendor.name)
    } else {
        vendor.name.to_owned()
    }
}

/// One Vendor's Contacts, numbered from one, as every command that shows them prints it.
///
/// Shared rather than written per command, so `vendor show`, `vendor contact list`, and a
/// mutation's refreshed Vendor cannot come to number one Vendor's Contacts differently — a
/// disagreement no reader would notice until they ran both.
fn human_contacts(contacts: &[VendorContact], output: &Output) -> io::Result<()> {
    if contacts.is_empty() {
        return output.summary("Contacts", &text("none"));
    }
    for (index, contact) in contacts.iter().enumerate() {
        output.summary(
            "Contact",
            &text(&format!("{}. {}", index + 1, contact_line(contact))),
        )?;
    }
    Ok(())
}

/// One Contact on one line: its Name, and whichever of its optional facts it carries.
///
/// Shared by the numbered lines above and by the subject line a Contact mutation prints,
/// so a Contact a mutation reports and the same Contact read back cannot come to be
/// rendered differently.
fn contact_line(contact: &VendorContact) -> String {
    let mut line = contact.name.as_str().to_owned();
    for fact in [
        contact.role.as_deref(),
        contact.phone.as_deref(),
        contact.email.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        line.push_str("  ");
        line.push_str(fact);
    }
    line
}

/// The Orders this Vendor is used by, or that it is used by none.
///
/// Said either way, because a reader cannot otherwise tell a Vendor nothing references
/// from a summary that forgot to mention what does.
fn human_referencing_orders(orders: &[ReferencingOrderSummary], output: &Output) -> io::Result<()> {
    if orders.is_empty() {
        return output.summary("Referencing Orders", &text("none"));
    }
    for order in orders {
        output.summary(
            "Referencing Order",
            &text(&format!(
                "{}  {}  {}",
                order.date_ordered(),
                order.title(),
                order.status().as_str()
            )),
        )?;
        output.detail("Order ID", OsStr::new(order.order_id()))?;
        output.detail(
            "Order Directory",
            order.directory().absolute_path().as_os_str(),
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The four mutations.
// ---------------------------------------------------------------------------

/// What one Vendor mutation decided, before anything is written anywhere.
///
/// Deciding and presenting are separate so that the four output modes present one answer
/// rather than each recomputing it — the shape in which two modes come to disagree about
/// the same run.
enum Mutation {
    /// The plan this command produced, and what committing it did. A dry run's answer is
    /// this one with no commit: the plan is the report either way.
    Planned {
        plan: Box<VendorMutationPlan>,
        committed: Option<Committed>,
    },
    Failed(Failed),
}

/// What committing one plan produced.
struct Committed {
    result: Box<VendorMutationResult>,
    /// The stable spelling of each warning the commit carried, in the core's own order.
    /// A committed mutation with a warning is still committed.
    warnings: Vec<&'static str>,
}

/// `add-vendor`: the one Vendor command whose subject does not exist yet.
///
/// The assignment set is the whole input, because the Name shorthand is expanded into one
/// — see [`crate::cli::vendor_assignments`]. A Package that holds no Vendor by that Name
/// is not a precondition this command checks: uniqueness is the Package's rule, so the
/// core refuses a Name already in use and names the Vendor already carrying it.
pub(crate) fn add(
    selected_root: &Path,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_package(selected_root) {
        Ok(context) => decide(
            &context,
            plan_vendor_creation(selected_root, &assigned(assignments), &PlatformFileSystem),
            dry_run,
        ),
        Err(failed) => Mutation::Failed(*failed),
    };
    present_mutation(ResponseContract::AddVendor, mutation, output)
}

pub(crate) fn edit(
    selected_root: &Path,
    name: &str,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_vendor(selected_root, name) {
        Ok((context, vendor_id)) => decide(
            &context,
            plan_vendor_edit(
                selected_root,
                &vendor_id,
                &assigned(assignments),
                &PlatformFileSystem,
            ),
            dry_run,
        ),
        Err(failed) => Mutation::Failed(*failed),
    };
    present_mutation(ResponseContract::VendorEdit, mutation, output)
}

pub(crate) fn hide(selected_root: &Path, name: &str, dry_run: bool, output: &Output) -> Exit {
    visibility(
        ResponseContract::VendorHide,
        selected_root,
        name,
        plan_vendor_hide,
        dry_run,
        output,
    )
}

pub(crate) fn unhide(selected_root: &Path, name: &str, dry_run: bool, output: &Output) -> Exit {
    visibility(
        ResponseContract::VendorUnhide,
        selected_root,
        name,
        plan_vendor_unhide,
        dry_run,
        output,
    )
}

/// The two reversible visibility mutations, which differ only in which plan they ask for.
///
/// Neither takes an assignment: what they change is not a Vendor fact a caller supplies.
/// Neither is refused on reference grounds either, which is the core's rule and needs
/// nothing here — hiding governs what a Package offers for new use, never what resolves.
fn visibility(
    contract: ResponseContract,
    selected_root: &Path,
    name: &str,
    plan: impl Fn(
        &Path,
        &str,
        &PlatformFileSystem,
    ) -> Result<VendorMutationPlan, VendorCapabilityError<io::Error>>,
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_vendor(selected_root, name) {
        Ok((context, vendor_id)) => decide(
            &context,
            plan(selected_root, &vendor_id, &PlatformFileSystem),
            dry_run,
        ),
        Err(failed) => Mutation::Failed(*failed),
    };
    present_mutation(contract, mutation, output)
}

/// The assignment set as the core takes it, in the order the command line supplied.
///
/// Nothing here inspects a field or a value. A repeated field, an unknown one, an
/// immutable one, and a blank required one are all the core's refusals, reported together.
fn assigned(assignments: &[FieldAssignment]) -> VendorAssignments {
    assignments
        .iter()
        .fold(VendorAssignments::new(), |set, assignment| {
            set.assign(assignment.field.clone(), assignment.value.clone())
        })
}

/// The Package this mutation is about, or the typed reason there is none.
///
/// Asked before the plan so that every refusal below can name the Root it happened under
/// and the record it would have replaced: the core's own refusals are about a Vendor and a
/// field rather than about a place, and a response that named no Package would be dropping
/// a fact this command holds.
fn resolved_package(selected_root: &Path) -> Result<ManagedContext, Box<Failed>> {
    package_candidate(
        selected_root,
        inspect_canonical_package(selected_root, &PlatformFileSystem),
    )
    .map(|package| package.context().clone())
}

/// The Vendor a Name selects, and the Package it belongs to.
///
/// The Name reaches the core as the caller wrote it and the core resolves it, so a Name
/// resolves for a mutation exactly as it resolves for a read — which is the one thing
/// about these commands that must not differ from `vendor show`.
fn resolved_vendor(
    selected_root: &Path,
    name: &str,
) -> Result<(ManagedContext, String), Box<Failed>> {
    match read_vendor_by_name(selected_root, name, &PlatformFileSystem) {
        Ok(read) => Ok((
            read.context().clone(),
            read.vendor().vendor_id.as_str().to_owned(),
        )),
        Err(reason) => Err(Box::new(failure(reason))),
    }
}

/// Plans, and commits unless this run was asked to plan only.
///
/// **The commit is a separate core capability and this is the only place it is called**,
/// which is what makes `--dry-run` an absence rather than a flag threaded through a
/// write path: a run that was asked to plan returns here, so no staging file, no
/// replacement, and no index refresh exists afterward.
fn decide(
    context: &ManagedContext,
    planned: Result<VendorMutationPlan, VendorCapabilityError<io::Error>>,
    dry_run: bool,
) -> Mutation {
    let plan = match planned {
        Ok(plan) => plan,
        Err(reason) => return Mutation::Failed(refusal(reason, context)),
    };
    if dry_run {
        return Mutation::Planned {
            plan: Box::new(plan),
            committed: None,
        };
    }
    match commit_vendor_mutation(plan, &mut PlatformUuids, &PlatformFileSystem) {
        // `Unchanged` beside `Committed` because both say the commit succeeded, and what
        // separates them is `changed` — which the report reads off the plan, so an
        // idempotent no-op says so honestly rather than by this arm's choice.
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            Mutation::Planned {
                plan: Box::new(plan),
                committed: Some(Committed {
                    result: Box::new(result),
                    warnings: Vec::new(),
                }),
            }
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => Mutation::Planned {
            plan: Box::new(plan),
            committed: Some(Committed {
                result: Box::new(result),
                warnings: warnings.iter().map(VendorMutationWarning::as_str).collect(),
            }),
        },
        MutationReport::Rejected { rejection, .. } => {
            Mutation::Failed(commit_refused(context, rejection))
        }
    }
}

// ---------------------------------------------------------------------------
// The refusals.
// ---------------------------------------------------------------------------

/// Why the plan was refused, as one typed failure.
///
/// **Total over the core's own reasons**, so a reason added there is a compile error here
/// rather than a mutation refused under a code that means something else. The four
/// Package-load states go through the shared constructors unchanged, so a Vendor mutation
/// reports an unmanaged, damaged, upgrade-pending, or unreadable Package with the same
/// code, message, and diagnosis a read of the same Package gives.
fn refusal(reason: VendorCapabilityError<io::Error>, context: &ManagedContext) -> Failed {
    match reason {
        VendorCapabilityError::UnmanagedPackage(package) => unmanaged_failure(&package),
        VendorCapabilityError::DamagedPackage { root, projection } => {
            damaged_failure(&projection, &root)
        }
        VendorCapabilityError::UpgradePending(package) => upgrade_pending_failure(&package),
        VendorCapabilityError::PackageOperationFailed(package) => {
            operation_failure(package.failure(), package.root())
        }
        VendorCapabilityError::MissingRequiredName => refused(
            FailureCode::VendorNameRequired,
            "a Vendor requires a Vendor Name and this command line assigned none. \
             `add-vendor \"Tarnwick\"` is the shorthand for `add-vendor name=Tarnwick`."
                .to_owned(),
            context,
            Vec::new(),
        ),
        VendorCapabilityError::InvalidAssignments(rejected) => refused(
            FailureCode::VendorAssignmentRejected,
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
            context,
            rejected
                .iter()
                .map(|assignment| {
                    Identity::rejected_assignment(assignment.field(), assignment.reason())
                })
                .collect(),
        ),
        VendorCapabilityError::DuplicateName { vendor_id } => refused(
            FailureCode::VendorNameInUse,
            format!(
                "the Package at {} already has a Vendor with that Name, compared trimmed \
                 and without regard to letter case. There is no Vendor Merge: use the \
                 existing Vendor, choose a deliberately distinct Name, or hide the one \
                 that is no longer used.",
                context.root().absolute_path().display()
            ),
            context,
            vec![Identity::vendor(&vendor_id)],
        ),
        // The Vendor this command resolved is no longer in the Package, so what was
        // planned is no longer what was validated. Reachable only between the resolving
        // read and the plan, which is the same race the commit refuses under this code.
        VendorCapabilityError::UnknownVendor { vendor_id } => refused(
            FailureCode::VendorMutationPreconditionsChanged,
            format!(
                "the Vendor this command resolved is no longer in the Package at {}, so \
                 nothing was written.",
                context.root().absolute_path().display()
            ),
            context,
            vec![Identity::vendor(&vendor_id)],
        ),
        VendorCapabilityError::InvalidAggregate => refused(
            FailureCode::VendorAggregateInvalid,
            "the complete Vendor record this mutation assembled is not valid for this \
             Package, so nothing was written."
                .to_owned(),
            context,
            Vec::new(),
        ),
        // The Contact mutations are what reach this: every position is a Contact's, and
        // none of the four Vendor mutations addresses one. The arm is written rather than
        // folded into a neighbour because the code a caller branches on has to be the one
        // that says a position was wrong.
        //
        // **The range is the positions the *command* can address**, not the collection's
        // own: an insertion may name the place after the last Contact, so `add` accepts one
        // number more than `edit` does.
        VendorCapabilityError::InvalidContactPosition {
            role,
            position,
            minimum,
            maximum,
        } => refused(
            FailureCode::VendorContactPositionInvalid,
            format!(
                "{position} is not a position this command can address: {}.",
                if maximum == 0 {
                    "this Vendor has no Contacts".to_owned()
                } else {
                    format!("they run from {minimum} to {maximum}")
                }
            ),
            context,
            vec![Identity::contact_position(role, &position.to_string())],
        ),
        // No Vendor capability in the core reports a filesystem refusal of its own today:
        // every path it reads, it reads through the Package load, which reports one as an
        // unreadable Package above. The arm names the same thing that load would.
        //
        // No Package surrounds the path because no path arrives: nothing constructs this,
        // so there is no place inside a Package for one to have come from.
        VendorCapabilityError::FileSystem { path, error } => Failed {
            code: FailureCode::PackageUnreadable,
            scope: FailureScope::Operation,
            message: format!("{} could not be read: {error}", path.display()),
            paths: vec![resolved(&path)],
            identities: Vec::new(),
            package_root: Some(context.root().absolute_path().to_path_buf()),
            diagnostics: Vec::new(),
        },
    }
}

/// The same, for a mutation whose subject is a Contact rather than a Vendor.
///
/// **One arm differs, and it is the required Name.** Every other reason means the same
/// thing for both families — a Package that did not load, an assignment set that broke a
/// rule, a Vendor that vanished, an aggregate that did not validate — so they are
/// [`refusal`]'s and stay there. A missing required Name does not: the Vendor a Contact
/// command names has already resolved, so what this command line assigned no Name to is
/// the Contact, and a code and a sentence about a *Vendor's* Name would send a caller to
/// `add-vendor` over a Contact it can simply name.
fn contact_refusal(reason: VendorCapabilityError<io::Error>, context: &ManagedContext) -> Failed {
    match reason {
        VendorCapabilityError::MissingRequiredName => refused(
            FailureCode::VendorContactNameRequired,
            "a Contact requires a Name and this command line assigned none. \
             `vendor contact add \"Tarnwick\" name=\"Katherine Johnson\"` is the least an \
             added Contact needs; its role, phone, and email are optional."
                .to_owned(),
            context,
            Vec::new(),
        ),
        reason => refusal(reason, context),
    }
}

/// Every refused assignment of one set, in the order the command line supplied them.
///
/// Each names the field and the rule it broke, in the core's own spelling — the same
/// spelling the response carries as an identity, so a human reading this sentence and a
/// script reading the document are told the same thing.
fn listed(rejected: &[RejectedAssignment]) -> String {
    let mut sentence = String::new();
    for assignment in rejected {
        if !sentence.is_empty() {
            sentence.push_str(", ");
        }
        sentence.push('`');
        sentence.push_str(assignment.field());
        sentence.push_str("` (");
        sentence.push_str(assignment.reason().as_str());
        sentence.push(')');
    }
    sentence
}

/// Why the commit was refused, as one typed failure.
///
/// **A failed cleanup is its own code**, because the action differs: where the staging
/// artifact this attempt created was removed again the command can simply be run again,
/// and where it was not, a durable stage stands beside the Vendor record — which is a
/// diagnosed condition, so `spielorders validate` reports it and `repair` is what offers
/// to promote or discard it.
///
/// **The Package rather than the plan**, because a Vendor mutation and a Contact mutation
/// are refused identically at the commit: both replace one Vendor record, and the
/// rejections are one set. The two identity refusals are reachable only from `add-vendor`,
/// which is the one command that mints a Vendor ID at all — a Contact mutation retains its
/// owning Vendor's — and their wording says so.
///
/// It is the [`ManagedContext`] rather than the plan's two paths because the context
/// carries the core's own `Location` for each of them, and a refusal owes this Package's
/// spelling of the record it names exactly as a success does. The plan is made under the
/// Root this context resolved, so they name the same Package.
fn commit_refused(
    context: &ManagedContext,
    rejection: VendorMutationRejection<io::Error>,
) -> Failed {
    let root = context.root().absolute_path();
    match rejection {
        VendorMutationRejection::PreconditionsChanged { location } => mutation_failed(
            FailureCode::VendorMutationPreconditionsChanged,
            format!(
                "{} changed between planning this mutation and performing it, so what \
                 would have been written is no longer what was validated. Nothing was \
                 written.",
                location.absolute_path().display()
            ),
            root,
            vec![in_package(&location)],
            Vec::new(),
        ),
        VendorMutationRejection::InvalidGeneratedIdentity { generated } => mutation_failed(
            FailureCode::VendorIdentityInvalid,
            format!("`{generated}` is not a usable Vendor ID, so this Vendor was not created."),
            root,
            vec![in_package(context.vendors())],
            Vec::new(),
        ),
        VendorMutationRejection::GeneratedIdentityCollision { vendor_id } => mutation_failed(
            FailureCode::VendorIdentityCollision,
            "the identity minted for this Vendor is already carried by a Vendor of this \
             Package, so nothing was written."
                .to_owned(),
            root,
            vec![in_package(context.vendors())],
            vec![Identity::vendor(&vendor_id)],
        ),
        VendorMutationRejection::ReplacementFailed { error } => replacement_failed(context, error),
    }
}

/// The Vendor record could not be replaced, in the one of three codes it earns.
///
/// A destination that stopped matching what the plan observed is the same condition a
/// changed precondition is — someone else wrote the record — and carries that code, so a
/// caller branches on "re-plan and try again" once rather than twice.
fn replacement_failed(
    context: &ManagedContext,
    error: ValidatedReplacementError<io::Error, VendorCandidateFailure>,
) -> Failed {
    let vendors = context.vendors();
    let stage_remains = error.cleanup_failure().is_some();
    let (code, message) = match error.failure() {
        ValidatedReplacementFailure::DestinationChanged => (
            FailureCode::VendorMutationPreconditionsChanged,
            "the Vendor record changed between planning this mutation and performing it, \
             so what would have been written is no longer what was validated. Nothing was \
             written."
                .to_owned(),
        ),
        // A candidate this mutation could not have produced. `VendorCandidateFailure` has
        // no variants: the aggregate was validated when it was planned and revalidated
        // against the current Package before this write, so there is no third judgement
        // left for a candidate to fail.
        ValidatedReplacementFailure::CandidateValidation(failure) => match *failure {},
        ValidatedReplacementFailure::InvalidDestination
        | ValidatedReplacementFailure::DestinationUnreadable(_)
        | ValidatedReplacementFailure::StagingWrite(_)
        | ValidatedReplacementFailure::Durability(_)
        | ValidatedReplacementFailure::Replacement(_) => (
            if stage_remains {
                FailureCode::VendorMutationCleanupFailed
            } else {
                FailureCode::VendorMutationFailed
            },
            format!(
                "the Vendor record at {} could not be replaced and holds what it held.{}",
                vendors.absolute_path().display(),
                if stage_remains {
                    // The stage stands beside the Vendor record inside the Package's own
                    // reserved directory, so the finding is the Package's.
                    format!(
                        " A staging artifact this attempt created could not be removed \
                         again; `spielorders validate` reports it and {}",
                        RepairSuggestion::for_condition(
                            Rooting::Package,
                            DiagnosticCode::StagingArtifactPresent,
                            EvidenceScope::Package,
                        )
                    )
                } else {
                    String::new()
                }
            ),
        ),
    };
    mutation_failed(
        code,
        message,
        context.root().absolute_path(),
        vec![in_package(vendors)],
        Vec::new(),
    )
}

/// One refusal that happened after this mutation was planned.
///
/// The Root is named in `context` whatever the outcome, because what the command was
/// *about* is the question `context` answers — and a mutation that got as far as a plan
/// established one.
fn mutation_failed(
    code: FailureCode,
    message: String,
    root: &Path,
    paths: Vec<FailedPath>,
    identities: Vec<Identity>,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths,
        identities,
        package_root: Some(root.to_path_buf()),
        // A mutation reports what it did. Assessing the Package is `package show` and
        // `validate`, and a refusal on Package-load grounds carries that Package's
        // diagnosis through the shared constructors instead.
        diagnostics: Vec::new(),
    }
}

/// One refusal about a Vendor or a field, under a Package that resolved.
///
/// The path is the Vendor record, because the Vendor aggregate is one Package-level
/// document and it is the document this mutation would have replaced.
fn refused(
    code: FailureCode,
    message: String,
    context: &ManagedContext,
    identities: Vec<Identity>,
) -> Failed {
    Failed {
        code,
        scope: FailureScope::Package,
        message,
        paths: vec![context.vendors().clone().into()],
        identities,
        package_root: Some(context.root().absolute_path().to_path_buf()),
        diagnostics: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Presenting one mutation.
// ---------------------------------------------------------------------------

fn present_mutation(contract: ResponseContract, mutation: Mutation, output: &Output) -> Exit {
    deliver(
        contract,
        mutation_root(&mutation),
        || mutation_document(contract, &mutation),
        |output| human_mutation(&mutation, output),
        output,
    )
}

/// The Root this answer was about, wherever the run established one.
fn mutation_root(mutation: &Mutation) -> Option<&Path> {
    match mutation {
        Mutation::Planned { plan, .. } => Some(plan.root()),
        Mutation::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn mutation_document(
    contract: ResponseContract,
    mutation: &Mutation,
) -> Result<(Response<Mutated>, Exit), NotRepresentable> {
    match mutation {
        Mutation::Planned { plan, committed } => Ok((
            Response::answered(
                contract,
                Context::rooted_at(plan.root())?,
                mutated(plan, committed.as_ref())?,
                // A mutation reports what it did to one Vendor. The Package's own health
                // is `package show` and `validate`, and giving one fact two contracts is
                // what the envelope's `diagnostics` rule forbids.
                Vec::new(),
            ),
            Exit::Success,
        )),
        Mutation::Failed(failed) => Ok((
            failed_document(contract, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The report, and the Vendor the mutation leaves behind.
///
/// **Everything but `committed` is the same question of a plan and of a commit**, which is
/// what makes a dry run the real run's answer rather than a description of a different
/// one: `changed`, the outcome, and the operations are read off the plan in both cases,
/// and the two members a commit genuinely produces — the refreshed index and the warnings
/// — are the ones a dry run has none of.
fn mutated(
    plan: &VendorMutationPlan,
    committed: Option<&Committed>,
) -> Result<Mutated, NotRepresentable> {
    let (facts, record) = answered(plan, committed);
    Ok(Mutated {
        report: Report {
            committed: committed.is_some(),
            changed: plan.changes_canonical_vendors(),
            outcome: plan.kind().canonical_outcome().as_str(),
            operations: plan
                .operations()
                .iter()
                .map(OperationProjection::of)
                .collect::<Result<_, _>>()?,
            index: index(record)?,
            warnings: committed
                .map(|committed| committed.warnings.clone())
                .unwrap_or_default(),
        },
        vendor: Show::of(&facts)?,
    })
}

/// The Vendor this run has to report, and the derived index it leaves behind.
///
/// **One selection rather than one per surface.** Which of the plan and the commit answers
/// each of those two questions is the same answer in every output mode, and two modes
/// deciding it separately is exactly the disagreement [`Mutation`] exists to prevent.
fn answered<'a>(
    plan: &'a VendorMutationPlan,
    committed: Option<&'a Committed>,
) -> (VendorFacts<'a>, &'a PackageIndexRecord) {
    match committed {
        Some(committed) => (
            VendorFacts::stored(
                committed.result.vendor(),
                committed.result.referencing_orders(),
            ),
            committed.result.index(),
        ),
        None => (
            VendorFacts::planned(plan.subject(), plan.referencing_orders()),
            plan.prior_index(),
        ),
    }
}

/// The human counterpart: what this command did, and the Vendor as it now stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing
/// the response contract. A dry run says what it *would* do in its first line, because a
/// reader who skimmed one line of a plan and took it for a result is the whole hazard of
/// reporting a plan in the same shape as a result.
///
/// **The Vendor is rendered exactly as `vendor show` renders it**, and the index outcome
/// is beside it because a mutation reports one. The individual managed operations are
/// `--verbose` detail, which is the decided split between what a mutation reports and
/// what only a reader asking for detail sees.
fn human_mutation(mutation: &Mutation, output: &Output) -> io::Result<Exit> {
    match mutation {
        Mutation::Planned { plan, committed } => {
            let (facts, record) = answered(plan, committed.as_ref());
            human_vendor(outcome_label(plan, committed.is_some()), &facts, output)?;
            output.summary("Index", &text(record.state().as_str()))?;
            for operation in plan.operations() {
                output.detail(operation.kind().as_str(), operation.path().as_os_str())?;
            }
            for warning in committed.iter().flat_map(|committed| &committed.warnings) {
                output.warning(&warning_sentence(warning))?;
            }
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

/// The one line that says which of the four outcomes this run reached.
///
/// Committed and changed, committed and unchanged, planned and would change, planned and
/// would change nothing — all four readable without counting members, because the human
/// surface owes the same distinction the report makes.
fn outcome_label(plan: &VendorMutationPlan, committed: bool) -> &'static str {
    match (committed, plan.changes_canonical_vendors()) {
        (true, false) | (false, false) => "Unchanged",
        (true, true) => match plan.kind() {
            VendorMutationKind::Create => "Created",
            VendorMutationKind::Edit => "Edited",
            VendorMutationKind::Hide => "Hidden",
            VendorMutationKind::Unhide => "Unhidden",
        },
        (false, true) => match plan.kind() {
            VendorMutationKind::Create => "Would create",
            VendorMutationKind::Edit => "Would edit",
            VendorMutationKind::Hide => "Would hide",
            VendorMutationKind::Unhide => "Would unhide",
        },
    }
}

/// One sentence for one warning, saying what did *not* happen rather than what did.
///
/// A committed mutation is committed: the sentence points at the command that replaces a
/// stale derived index, never at running this one again, because re-running it would be
/// the retry a report must not encourage.
fn warning_sentence(warning: &str) -> String {
    format!(
        "the Vendor was saved and the derived Package Index was not refreshed \
         ({warning}), so it is stale. `spielorders index rebuild` replaces it."
    )
}

// ---------------------------------------------------------------------------
// The four Contact mutations.
// ---------------------------------------------------------------------------

/// What one Contact mutation decided, before anything is written anywhere.
///
/// The counterpart of [`Mutation`], separate because a Contact plan is a different type
/// and carries the positions it addressed. What the two share is [`Committed`]: a Contact
/// mutation replaces the owning Vendor record, so committing one produces the same result
/// a Vendor mutation's commit produces.
enum ContactMutation {
    Planned {
        plan: Box<VendorContactMutationPlan>,
        committed: Option<Committed>,
    },
    Failed(Failed),
}

/// The payload of all four Contact mutations: what the mutation did to one Contact, and
/// the Vendor whose collection it changed.
///
/// The refreshed result is [`Show`], the same projection the reads render, so a client
/// that can display a Vendor can display the one a Contact mutation returns — with the
/// Contact it is about at the position the report names.
#[derive(Debug, Serialize)]
struct ContactMutated {
    report: ContactReport,
    vendor: Show,
}

/// The typed report of one Contact mutation.
///
/// **`contact` is the Contact this mutation was about**, and it is here rather than left
/// to be found in `vendor.contacts` because a removal's subject is in no collection
/// afterwards. For the other three it is the Contact standing at `destination_position`,
/// so the report and the refreshed Vendor agree by construction.
///
/// **The two positions are what "old and new" means for a collection addressed by
/// position.** A Vendor mutation reports no old-and-new paths because it moves nothing;
/// these commands move something, and the pair is one fact together — where the command
/// line addressed, and where the Contact was left. Each is `null` for the one end its
/// mutation does not have: an `add` takes the Contact from nowhere, and a `remove` leaves
/// it nowhere.
///
/// Everything else is [`Report`]'s, and means the same: `committed` is the one member
/// `--dry-run` decides, `changed` is a fact about the plan, `outcome` is the canonical
/// half of the outcome and `index` the derived half, and a committed mutation with a
/// warning is still committed.
#[derive(Debug, Serialize)]
struct ContactReport {
    committed: bool,
    changed: bool,
    outcome: &'static str,
    contact: ContactFacts,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    operations: Vec<OperationProjection>,
    index: IndexProjection,
    /// Present and empty for every mutation whose derived refresh succeeded.
    warnings: Vec<&'static str>,
}

/// `vendor contact add`: one Contact appended, or inserted where `position=` says.
///
/// The insertion point is taken out of the assignment set before the rest reaches the core
/// — see [`crate::cli::contact_assignments`] — because where a Contact goes is not one of
/// its facts. Every remaining assignment is the Package's own to adjudicate.
pub(crate) fn contact_add(
    selected_root: &Path,
    name: &str,
    assignments: &[FieldAssignment],
    position: Option<&str>,
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match resolved_vendor(selected_root, name) {
        Ok((context, vendor_id)) => match insertion_point(position, &context) {
            Ok(position) => decide_contact(
                &context,
                plan_vendor_contact_add(
                    selected_root,
                    &vendor_id,
                    &contact_assigned(assignments),
                    position,
                    &PlatformFileSystem,
                ),
                dry_run,
            ),
            Err(failed) => ContactMutation::Failed(*failed),
        },
        Err(failed) => ContactMutation::Failed(*failed),
    };
    present_contact_mutation(ResponseContract::VendorContactAdd, mutation, output)
}

/// `vendor contact edit`: the facts of the Contact standing at one position.
pub(crate) fn contact_edit(
    selected_root: &Path,
    name: &str,
    position: &str,
    assignments: &[FieldAssignment],
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match addressed_position(selected_root, name, position) {
        Ok((context, vendor_id, position)) => decide_contact(
            &context,
            plan_vendor_contact_edit(
                selected_root,
                &vendor_id,
                position,
                &contact_assigned(assignments),
                &PlatformFileSystem,
            ),
            dry_run,
        ),
        Err(failed) => ContactMutation::Failed(*failed),
    };
    present_contact_mutation(ResponseContract::VendorContactEdit, mutation, output)
}

/// `vendor contact move`: one Contact taken from one position and put at another.
///
/// Both positions are the core's to evaluate against the pre-mutation collection, which is
/// what makes `move NAME 3 position=1` mean the third Contact a reader saw. A destination a
/// Contact already occupies changes nothing and is not an error.
pub(crate) fn contact_move(
    selected_root: &Path,
    name: &str,
    position: &str,
    destination: &str,
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match addressed_position(selected_root, name, position) {
        Ok((context, vendor_id, position)) => {
            match requested_position(destination, ContactPositionRole::Destination, &context) {
                Ok(destination) => decide_contact(
                    &context,
                    plan_vendor_contact_move(
                        selected_root,
                        &vendor_id,
                        position,
                        destination,
                        &PlatformFileSystem,
                    ),
                    dry_run,
                ),
                Err(failed) => ContactMutation::Failed(*failed),
            }
        }
        Err(failed) => ContactMutation::Failed(*failed),
    };
    present_contact_mutation(ResponseContract::VendorContactMove, mutation, output)
}

/// `vendor contact remove`: the Contact standing at one position, and no disposal.
///
/// A Contact is embedded in its Vendor, so there is nothing to move to a Removed collection
/// or to the platform Trash: the Vendor record is rewritten without it. A Vendor's last
/// Contact may go — its Contacts are then present and empty.
pub(crate) fn contact_remove(
    selected_root: &Path,
    name: &str,
    position: &str,
    dry_run: bool,
    output: &Output,
) -> Exit {
    let mutation = match addressed_position(selected_root, name, position) {
        Ok((context, vendor_id, position)) => decide_contact(
            &context,
            plan_vendor_contact_remove(selected_root, &vendor_id, position, &PlatformFileSystem),
            dry_run,
        ),
        Err(failed) => ContactMutation::Failed(*failed),
    };
    present_contact_mutation(ResponseContract::VendorContactRemove, mutation, output)
}

/// The Vendor, and the one-based position the command line addressed in its collection.
///
/// The three commands that take a `POSITION` positional resolve the same two things in the
/// same order, so a Name that matched no Vendor is reported as that rather than as a
/// position refused against a collection this command never saw. What a Contact command
/// resolves is the Vendor that *owns* the Contact, because a Contact has no identity of its
/// own to resolve.
fn addressed_position(
    selected_root: &Path,
    name: &str,
    position: &str,
) -> Result<(ManagedContext, String, usize), Box<Failed>> {
    let (context, vendor_id) = resolved_vendor(selected_root, name)?;
    let position = requested_position(position, ContactPositionRole::Source, &context)?;
    Ok((context, vendor_id, position))
}

/// The optional insertion point an `add` command line supplied.
///
/// `None` is the append this command's absence of a `position=` means, and it is not a
/// refusal: a Contact added without one goes after the last. An insertion point is always
/// the destination end, because an `add` takes its Contact from no position at all.
fn insertion_point(
    position: Option<&str>,
    context: &ManagedContext,
) -> Result<Option<usize>, Box<Failed>> {
    position
        .map(|position| requested_position(position, ContactPositionRole::Destination, context))
        .transpose()
}

/// One position as the command line wrote it, or the typed reason it is none.
///
/// **The whole number is read here and its meaning is the core's**, which is the split that
/// makes a position a domain refusal rather than a usage error: whether a token is a number
/// is a fact about the command line, and which numbers name a Contact is a fact about the
/// collection. So all four ways of naming no position — a token that is not a number, a
/// negative one, zero, and one past the end — carry the one code a caller branches on, and
/// the position it was about travels as an identity because a message is not a contract.
///
/// Zero is deliberately *not* refused here: it parses, so the core refuses it beside every
/// other out-of-range position and reports the range in the same sentence.
fn requested_position(
    position: &str,
    role: ContactPositionRole,
    context: &ManagedContext,
) -> Result<usize, Box<Failed>> {
    position.parse::<usize>().map_err(|_| {
        Box::new(refused(
            FailureCode::VendorContactPositionInvalid,
            format!(
                "`{position}` is not a Contact position. A position is a whole number \
                 counting this Vendor's Contacts from one."
            ),
            context,
            vec![Identity::contact_position(role, position)],
        ))
    })
}

/// The Contact facts as the core takes them, in the order the command line supplied.
///
/// Nothing here inspects a field or a value. An unknown field, a repeated one, and a blank
/// Name are all the core's refusals, reported together — the same adjudication a Vendor's
/// own assignments get from [`assigned`].
fn contact_assigned(assignments: &[FieldAssignment]) -> ContactAssignments {
    assignments
        .iter()
        .fold(ContactAssignments::new(), |set, assignment| {
            set.assign(assignment.field.clone(), assignment.value.clone())
        })
}

/// Plans, and commits unless this run was asked to plan only.
///
/// [`decide`]'s counterpart, and the only place a Contact commit is called — so a
/// `--dry-run` here is the same absence of a write path it is there. The commit needs no
/// UUID effect at all: a Contact mutation retains its owning Vendor's identity.
fn decide_contact(
    context: &ManagedContext,
    planned: Result<VendorContactMutationPlan, VendorCapabilityError<io::Error>>,
    dry_run: bool,
) -> ContactMutation {
    let plan = match planned {
        Ok(plan) => plan,
        Err(reason) => return ContactMutation::Failed(contact_refusal(reason, context)),
    };
    if dry_run {
        return ContactMutation::Planned {
            plan: Box::new(plan),
            committed: None,
        };
    }
    match commit_vendor_contact_mutation(plan, &PlatformFileSystem) {
        MutationReport::Committed { plan, result } | MutationReport::Unchanged { plan, result } => {
            ContactMutation::Planned {
                plan: Box::new(plan),
                committed: Some(Committed {
                    result: Box::new(result),
                    warnings: Vec::new(),
                }),
            }
        }
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => ContactMutation::Planned {
            plan: Box::new(plan),
            committed: Some(Committed {
                result: Box::new(result),
                warnings: warnings.iter().map(VendorMutationWarning::as_str).collect(),
            }),
        },
        MutationReport::Rejected { rejection, .. } => {
            ContactMutation::Failed(commit_refused(context, rejection))
        }
    }
}

fn present_contact_mutation(
    contract: ResponseContract,
    mutation: ContactMutation,
    output: &Output,
) -> Exit {
    deliver(
        contract,
        contact_mutation_root(&mutation),
        || contact_mutation_document(contract, &mutation),
        |output| human_contact_mutation(&mutation, output),
        output,
    )
}

/// The Root this answer was about, wherever the run established one.
fn contact_mutation_root(mutation: &ContactMutation) -> Option<&Path> {
    match mutation {
        ContactMutation::Planned { plan, .. } => Some(plan.root()),
        ContactMutation::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn contact_mutation_document(
    contract: ResponseContract,
    mutation: &ContactMutation,
) -> Result<(Response<ContactMutated>, Exit), NotRepresentable> {
    match mutation {
        ContactMutation::Planned { plan, committed } => Ok((
            Response::answered(
                contract,
                Context::rooted_at(plan.root())?,
                contact_mutated(plan, committed.as_ref())?,
                // A mutation reports what it did to one Contact of one Vendor. The
                // Package's own health is `package show` and `validate`.
                Vec::new(),
            ),
            Exit::Success,
        )),
        ContactMutation::Failed(failed) => Ok((
            failed_document(contract, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The report, and the Vendor the mutation leaves behind.
///
/// [`mutated`]'s counterpart, and the same division: everything but `committed`, the
/// refreshed index, and the warnings is read off the plan, so a dry run answers with the
/// real run's report rather than with a description of a different one.
fn contact_mutated(
    plan: &VendorContactMutationPlan,
    committed: Option<&Committed>,
) -> Result<ContactMutated, NotRepresentable> {
    let (facts, record) = contact_answered(plan, committed);
    Ok(ContactMutated {
        report: ContactReport {
            committed: committed.is_some(),
            changed: plan.changes_canonical_vendors(),
            outcome: plan.kind().canonical_outcome().as_str(),
            contact: ContactFacts::of(plan.subject_contact()),
            source_position: plan.source_position(),
            destination_position: plan.destination_position(),
            operations: plan
                .operations()
                .iter()
                .map(OperationProjection::of)
                .collect::<Result<_, _>>()?,
            index: index(record)?,
            warnings: committed
                .map(|committed| committed.warnings.clone())
                .unwrap_or_default(),
        },
        vendor: Show::of(&facts)?,
    })
}

/// The Vendor this run has to report, and the derived index it leaves behind.
///
/// [`answered`]'s counterpart, made once for every output mode for the same reason.
fn contact_answered<'a>(
    plan: &'a VendorContactMutationPlan,
    committed: Option<&'a Committed>,
) -> (VendorFacts<'a>, &'a PackageIndexRecord) {
    match committed {
        Some(committed) => (
            VendorFacts::stored(
                committed.result.vendor(),
                committed.result.referencing_orders(),
            ),
            committed.result.index(),
        ),
        None => (
            VendorFacts::planned(plan.vendor(), plan.referencing_orders()),
            plan.prior_index(),
        ),
    }
}

/// The human counterpart: what this command did to which Contact, and the Vendor as it
/// now stands.
///
/// Human output is never a parsing contract, so this may be reworded without advancing the
/// response contract. It names the Contact, the position or positions the report carries,
/// and nothing the machine response does not have.
fn human_contact_mutation(mutation: &ContactMutation, output: &Output) -> io::Result<Exit> {
    match mutation {
        ContactMutation::Planned { plan, committed } => {
            let (facts, record) = contact_answered(plan, committed.as_ref());
            output.summary(
                contact_outcome_label(plan, committed.is_some()),
                &text(&addressed(plan)),
            )?;
            human_vendor("Vendor", &facts, output)?;
            output.summary("Index", &text(record.state().as_str()))?;
            for operation in plan.operations() {
                output.detail(operation.kind().as_str(), operation.path().as_os_str())?;
            }
            for warning in committed.iter().flat_map(|committed| &committed.warnings) {
                output.warning(&warning_sentence(warning))?;
            }
            Ok(Exit::Success)
        }
        ContactMutation::Failed(failed) => {
            output.error(&failed.message)?;
            for diagnostic in &failed.diagnostics {
                report_diagnostic(output, failed.package_root.as_deref(), diagnostic)?;
            }
            Ok(Exit::Failure)
        }
    }
}

/// The Contact this command was about, at the position a reader will find it.
///
/// A move says both numbers, because the command line named one and the collection now
/// shows the other; the rest say the one they have. A removal's number is where the Contact
/// stood, which is the only number it has left.
fn addressed(plan: &VendorContactMutationPlan) -> String {
    let contact = contact_line(plan.subject_contact());
    match plan.kind() {
        VendorContactMutationKind::Add | VendorContactMutationKind::Edit => {
            numbered(plan.destination_position(), &contact)
        }
        VendorContactMutationKind::Move => {
            match (plan.source_position(), plan.destination_position()) {
                (Some(source), Some(destination)) => {
                    format!("{source} to {destination}. {contact}")
                }
                _ => contact,
            }
        }
        VendorContactMutationKind::Remove => numbered(plan.source_position(), &contact),
    }
}

/// One Contact line with its position, where the plan has one to give.
///
/// Every plan carries the position its kind addresses; a line without one still names the
/// Contact, which is the honest fallback for a rendering that is not a parsing contract.
fn numbered(position: Option<usize>, contact: &str) -> String {
    match position {
        Some(position) => format!("{position}. {contact}"),
        None => contact.to_owned(),
    }
}

/// The one line that says which of the four outcomes this run reached.
///
/// [`outcome_label`]'s counterpart, with the same four distinctions: committed and changed,
/// committed and unchanged, planned and would change, planned and would change nothing. A
/// move to the position a Contact already occupies is the unchanged one.
fn contact_outcome_label(plan: &VendorContactMutationPlan, committed: bool) -> &'static str {
    match (committed, plan.changes_canonical_vendors()) {
        (true, false) | (false, false) => "Unchanged",
        (true, true) => match plan.kind() {
            VendorContactMutationKind::Add => "Added",
            VendorContactMutationKind::Edit => "Edited",
            VendorContactMutationKind::Move => "Moved",
            VendorContactMutationKind::Remove => "Removed",
        },
        (false, true) => match plan.kind() {
            VendorContactMutationKind::Add => "Would add",
            VendorContactMutationKind::Edit => "Would edit",
            VendorContactMutationKind::Move => "Would move",
            VendorContactMutationKind::Remove => "Would remove",
        },
    }
}
