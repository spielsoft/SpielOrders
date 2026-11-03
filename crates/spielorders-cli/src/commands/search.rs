//! The `search` family: which of a Package's entities match a query, and what the
//! platform's index found inside its Order Directories.
//!
//! **Two engines, and nothing shared but the Package they answer about.** The entity
//! searches ask the core, which decides matching over the records the load already read;
//! the Supporting Material search asks a platform service, which governs its own matching.
//! Neither borrows the other's rules, so `--case-sensitive` belongs to the two below and
//! to neither of the provider's answers.
//!
//! **A query nothing matches is an answer.** The question an entity search asks is which
//! Orders or which Vendors match, and "none of them" answers it: exit `0`, with a present
//! and empty collection. The failures these two have are the ones every Package read has —
//! a Root that is not one, a Package the load could read and not trust.
//!
//! # `search supporting-material`
//!
//! **The command waits and prints once.** The core capability is synchronous and returns
//! one settled snapshot, so there is no stream to render incrementally and no asynchronous
//! runtime here. A desktop shell calling the same capability from a worker is what keeps
//! the work off a user interface thread; a command-line caller has none to keep it off.
//!
//! **An index that found nothing and an index that could not be asked are two answers.**
//! The first is a success carrying no Order groups — the index found no matching file,
//! which is not the same as there being none. The second could not answer the question at
//! all, so it exits `1` under a code naming the platform state that prevented it, exactly
//! as any other read that could not read does. Nothing here falls back to walking or
//! indexing Supporting Material.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract; Effect and adapter boundaries.

use crate::{
    commands::shared::{
        Failed, OrderSummary, RootKind, VendorSummary, diagnostics, failed_context,
        failed_document, flattened, human_failure, link_named_as_a_root, not_representable,
        order_summary, package_candidate, package_context, presented_vendor, report_diagnostic,
        report_order, report_vendor, text, undeliverable,
    },
    contract::ResponseContract,
    effects::{PlatformFileSystem, UninterruptedQuery, supporting_material_index},
    exit::Exit,
    output::Output,
    response::{
        Context, FailureCode, FailureScope, Identity, LocationProjection, NotRepresentable,
        Response,
    },
};
use serde::Serialize;
use spielorders_core::{
    canonical::{CurrentPackageCandidate, inspect_canonical_package},
    metadata::Vendor,
    search::{CaseSensitivity, SearchQuery, matching_orders, matching_vendors},
    supporting_material::{
        Completeness, MatchedFile, ProviderDiagnostic, SupportingMaterialQuery,
        SupportingMaterialSnapshot, UnavailableReason, search_supporting_material,
    },
};
use std::{io, path::Path, time::Duration};

const CONTRACT: ResponseContract = ResponseContract::SearchSupportingMaterial;
const ORDERS_CONTRACT: ResponseContract = ResponseContract::SearchOrders;
const VENDORS_CONTRACT: ResponseContract = ResponseContract::SearchVendors;

/// How long a provider has to settle this process's query.
///
/// **A choice this code makes, not a rule it is held to.** What is decided is that the call
/// carries a bounded timeout and that reaching it returns what was gathered; how long the
/// bound is belongs to the caller, and there is no option for it because the decided option
/// set governs targeting, output, and execution and this is none of the three. Long enough
/// that an ordinary index answers, short enough that a wedged one does not hold a terminal.
const BOUND: Duration = Duration::from_secs(10);

/// The Package a search is about, or the typed reason there is none to search.
enum Lookup {
    Looked {
        package: Box<CurrentPackageCandidate>,
    },
    Failed(Box<Failed>),
}

// ---------------------------------------------------------------------------
// `search orders` and `search vendors`
// ---------------------------------------------------------------------------
//
// Two commands, two independently versioned contracts, and one presenter — the shape the
// narrower Package reads already use, and for the same reason: the envelope, the exit
// status, which diagnostics the answer carries, and both fallbacks are decided once for
// both rather than twice. The two disagreeing about an empty answer or about a load state
// is the defect that shape exists to prevent.

/// The `search orders` payload: what was asked, and which Orders match.
///
/// **No Damaged Order placeholder**, as against `package list orders`, which reports one.
/// A Damaged Order's record could not be read, so it has no facts to match on and is in no
/// answer here — while a placeholder in an answer about matching would claim it had been
/// considered. The load's observation about it travels in the envelope's `diagnostics`,
/// which is what says the answer is over less than the whole Package.
#[derive(Debug, Serialize)]
struct MatchedOrders {
    query: String,
    /// The Orders this query retained, in the Package's own ordering. Nothing here sorts,
    /// and a query never reorders what it did not remove.
    orders: Vec<OrderSummary>,
}

/// The `search vendors` payload: what was asked, and which Vendors match.
#[derive(Debug, Serialize)]
struct MatchedVendors {
    query: String,
    vendors: Vec<VendorSummary>,
    /// Whether the Vendors above were searched among every one of them.
    ///
    /// **The same member `package list vendors` carries, for the same reason.** Without it
    /// a consumer cannot tell a Vendor that is hidden from one that does not match, and an
    /// empty answer would read as "no such Vendor" when it means "none among the ones this
    /// Package offers".
    hidden_vendors_included: bool,
}

pub(crate) fn orders(
    selected_root: &Path,
    keywords: &[String],
    case_sensitive: bool,
    output: &Output,
) -> Exit {
    let asked = asked_for(keywords, case_sensitive);
    present_matches(
        ORDERS_CONTRACT,
        &load(selected_root),
        &asked,
        |package| {
            Ok(MatchedOrders {
                query: asked.spelled.clone(),
                orders: matching_orders(package, &asked.query)
                    .into_iter()
                    .map(|(position, order)| {
                        order_summary(order, package.vendor_for_order(position))
                    })
                    .collect::<Result<_, _>>()?,
            })
        },
        |package, output| {
            let matched = matching_orders(package, &asked.query);
            if matched.is_empty() {
                return output.summary("Orders", &text("none matched"));
            }
            for (position, order) in matched {
                report_order(output, order, package.vendor_for_order(position))?;
            }
            Ok(())
        },
        output,
    )
}

pub(crate) fn vendors(
    selected_root: &Path,
    keywords: &[String],
    all: bool,
    case_sensitive: bool,
    output: &Output,
) -> Exit {
    let asked = asked_for(keywords, case_sensitive);
    present_matches(
        VENDORS_CONTRACT,
        &load(selected_root),
        &asked,
        |package| {
            Ok(MatchedVendors {
                query: asked.spelled.clone(),
                vendors: presented(package, all, &asked.query)
                    .into_iter()
                    .map(VendorSummary::of)
                    .collect(),
                hidden_vendors_included: all,
            })
        },
        |package, output| {
            let matched = presented(package, all, &asked.query);
            if matched.is_empty() {
                output.summary("Vendors", &text("none matched"))?;
            }
            for vendor in matched {
                report_vendor(output, vendor)?;
            }
            // Said whether or not any Vendor is hidden, for the reason the payload member
            // above is carried: it is what the list *is*.
            output.summary(
                "Hidden Vendors",
                &text(if all { "included" } else { "omitted" }),
            )
        },
        output,
    )
}

/// One query, in the two forms an answer needs it: as it was asked for, and as the core
/// reads it.
struct Asked {
    /// The text the command line spelled, which the response reports back.
    spelled: String,
    query: SearchQuery,
}

/// The query one command line spells.
///
/// **The arguments are joined and the core separates the terms.** Whitespace separates
/// terms wherever it stands, so one quoted multiword argument and the same words written
/// as several arguments arrive as one text and come back as the same terms. Splitting them
/// here instead would be a second tokenizer for the core's to disagree with — and the
/// desktop, which hands the core one text out of one field, would be the one it disagreed
/// with.
fn asked_for(keywords: &[String], case_sensitive: bool) -> Asked {
    let spelled = keywords.join(" ");
    let case = if case_sensitive {
        CaseSensitivity::Sensitive
    } else {
        CaseSensitivity::Insensitive
    };
    Asked {
        query: SearchQuery::new(&spelled, case),
        spelled,
    }
}

/// The matching Vendors this run presents.
///
/// The visibility default is applied to what the query retained rather than to the
/// Package's whole list, which is what "search retains rows of the list a caller is
/// shown" means for a command line; [`presented_vendor`] is where the default itself
/// lives.
fn presented<'a>(
    package: &'a CurrentPackageCandidate,
    all: bool,
    query: &SearchQuery,
) -> Vec<&'a Vendor> {
    matching_vendors(package, query)
        .into_iter()
        .filter(|vendor| presented_vendor(vendor, all))
        .collect()
}

/// Presents which entities of one loaded Package match.
///
/// **Both commands go through here**, so an answer, a failure, the load's diagnostics, and
/// the two fallbacks are one decision for both. The load's diagnostics travel in the
/// envelope because each of these reports a projection of what one load could read, and a
/// Package holding an Order that could not be read answers over fewer Orders than it holds
/// — without the observation beside it, that partial answer would be indistinguishable
/// from a complete one.
fn present_matches<T: Serialize>(
    contract: ResponseContract,
    lookup: &Lookup,
    asked: &Asked,
    project: impl Fn(&CurrentPackageCandidate) -> Result<T, NotRepresentable>,
    human: impl Fn(&CurrentPackageCandidate, &Output) -> io::Result<()>,
    output: &Output,
) -> Exit {
    if output.mode().is_json() {
        let (document, exit) = match matches_document(contract, lookup, project) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(contract, package_context(searched_root(lookup))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_matches(lookup, human, &asked.spelled, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// The Root this answer was about, wherever the load established one.
fn searched_root(lookup: &Lookup) -> Option<&Path> {
    match lookup {
        Lookup::Looked { package } => Some(package.context().root().absolute_path()),
        Lookup::Failed(failed) => failed.package_root.as_deref(),
    }
}

fn matches_document<T: Serialize>(
    contract: ResponseContract,
    lookup: &Lookup,
    project: impl Fn(&CurrentPackageCandidate) -> Result<T, NotRepresentable>,
) -> Result<(Response<T>, Exit), NotRepresentable> {
    match lookup {
        Lookup::Looked { package } => Ok((
            Response::answered(
                contract,
                Context::rooted_at(package.context().root().absolute_path())?,
                project(package)?,
                diagnostics(&flattened(package.diagnostic_groups()))?,
            ),
            Exit::Success,
        )),
        Lookup::Failed(failed) => Ok((
            failed_document(contract, failed_context(failed)?, failed)?,
            Exit::Failure,
        )),
    }
}

/// The human counterpart. Nothing here is a parsing contract.
///
/// The Package Root and the query are the same two details for both commands, so they are
/// written here rather than in each; the machine responses carry the Root in `context` and
/// the query in `data` in every mode.
fn human_matches(
    lookup: &Lookup,
    human: impl Fn(&CurrentPackageCandidate, &Output) -> io::Result<()>,
    query: &str,
    output: &Output,
) -> io::Result<Exit> {
    match lookup {
        Lookup::Looked { package } => {
            let root = package.context().root().absolute_path();
            output.detail("Package Root", root.as_os_str())?;
            output.detail("Query", &text(query))?;
            human(package, output)?;
            for group in package.diagnostic_groups() {
                for diagnostic in group.diagnostics() {
                    report_diagnostic(output, Some(root), diagnostic)?;
                }
            }
            Ok(Exit::Success)
        }
        Lookup::Failed(failed) => human_failure(failed, output),
    }
}

// ---------------------------------------------------------------------------
// `search supporting-material`
// ---------------------------------------------------------------------------

pub(crate) fn supporting_material(selected_root: &Path, query: &str, output: &Output) -> Exit {
    match load(selected_root) {
        Lookup::Looked { package } => {
            // The snapshot borrows the Package it is about, so both live here for as long as
            // the answer is being presented.
            let snapshot = search_supporting_material(
                &package,
                &SupportingMaterialQuery::new(query, BOUND),
                supporting_material_index(),
                &UninterruptedQuery,
            );
            present(&package, query, &snapshot, output)
        }
        Lookup::Failed(failed) => present_failure(&failed, output),
    }
}

/// The Package this search is about, classified exactly as every other Package read
/// classifies its load.
fn load(selected_root: &Path) -> Lookup {
    if let Some(failed) = link_named_as_a_root(selected_root, RootKind::Package) {
        return Lookup::Failed(Box::new(failed));
    }
    match package_candidate(
        selected_root,
        inspect_canonical_package(selected_root, &PlatformFileSystem),
    ) {
        Ok(package) => Lookup::Looked {
            package: Box::new(package),
        },
        Err(failed) => Lookup::Failed(failed),
    }
}

/// The `search supporting-material` payload: what a provider was asked, and what it found.
///
/// **The provider's state is not a member here**, because inside `data` it never varies: a
/// document with a payload is a provider that looked, and every state that prevents one
/// reaches a caller as `ok: false` and a failure code naming it. What does vary is whether
/// the provider looked at everything in scope, which is the pair below.
///
/// **`diagnostics` here is the provider's, and the envelope's is the Package load's.** They
/// are two different subjects — what the platform said about this query, and what the load
/// observed about this Package — so collapsing them would give one member two meanings.
#[derive(Debug, Serialize)]
struct Found {
    /// The query as it was handed to the provider, which is as the caller wrote it.
    query: String,
    /// The provider that answered, in its own name.
    provider: String,
    complete: bool,
    /// Why the answer is only part of what is in scope; `null` where it is complete.
    partial_reason: Option<&'static str>,
    /// One group per Order holding at least one matching file, in the Package's own Order
    /// order. Relevance orders files within a group and never reorders the groups.
    orders: Vec<OrderMatches>,
    diagnostics: Vec<ProviderDiagnosticProjection>,
}

/// One Order, and the files the provider found inside its Directory.
///
/// **No count beside the list**, because the list is the count: a member saying how many
/// files a group holds would be a second spelling of `files.len()` for the two to disagree
/// about. A view that shows a count reads it off the list.
#[derive(Debug, Serialize)]
struct OrderMatches {
    order: OrderSummary,
    files: Vec<MatchedFileProjection>,
}

/// One matching file, in the facts the provider's index carried for it.
///
/// **`filename` is beside the path deliberately.** It is the first of the two deterministic
/// keys the files above it are ordered by — the path is the second — so a consumer can
/// reproduce that order from the response instead of re-deriving a filename from a path and
/// hoping it split the same way.
#[derive(Debug, Serialize)]
struct MatchedFileProjection {
    location: LocationProjection,
    filename: String,
    /// The kind of file in the provider's own words, or `null` where it offered none.
    kind: Option<String>,
    /// Seconds since the Unix epoch, or `null` where the provider offered none. Not a
    /// rendered date: which calendar and which time zone a date is shown in is
    /// presentation, and this is the fact underneath it.
    modified_unix_seconds: Option<i64>,
    /// The provider's own relevance, or `null` where it offered none for this file. Never a
    /// score to display — it orders files within one Order and reorders no Order.
    rank: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ProviderDiagnosticProjection {
    code: &'static str,
    /// What the provider itself said, where it said anything.
    detail: Option<String>,
}

impl ProviderDiagnosticProjection {
    fn of(diagnostic: &ProviderDiagnostic) -> Self {
        Self {
            code: diagnostic.code().as_str(),
            detail: diagnostic.detail().map(str::to_owned),
        }
    }
}

fn present(
    package: &CurrentPackageCandidate,
    query: &str,
    snapshot: &SupportingMaterialSnapshot<'_>,
    output: &Output,
) -> Exit {
    let root = package.context().root().absolute_path();
    if output.mode().is_json() {
        let (document, exit) = match document(package, query, snapshot) {
            Ok(built) => built,
            Err(NotRepresentable) => (
                not_representable(CONTRACT, package_context(Some(root))),
                Exit::Failure,
            ),
        };
        return match output.document(&document) {
            Ok(()) => exit,
            Err(error) => undeliverable(output, error),
        };
    }
    match human(package, query, snapshot, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

fn present_failure(failed: &Failed, output: &Output) -> Exit {
    if output.mode().is_json() {
        let document = match failed_context(failed).and_then(|context| {
            failed_document::<Found, _>(CONTRACT, context, failed).map(|document| (document,))
        }) {
            Ok((document,)) => document,
            Err(NotRepresentable) => {
                not_representable(CONTRACT, package_context(failed.package_root.as_deref()))
            }
        };
        return match output.document(&document) {
            Ok(()) => Exit::Failure,
            Err(error) => undeliverable(output, error),
        };
    }
    match human_failure(failed, output) {
        Ok(exit) => exit,
        Err(error) => undeliverable(output, error),
    }
}

/// Encodes a settled Supporting Material response using the executable's wire format.
///
/// This pure projection lets conformance checks supply provider facts without live indexing.
/// Returns an I/O error when a path is not representable or serialization fails.
pub fn supporting_material_document(
    package: &CurrentPackageCandidate,
    query: &str,
    snapshot: &SupportingMaterialSnapshot<'_>,
) -> io::Result<Vec<u8>> {
    let (response, _) = document(package, query, snapshot)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "response path is not UTF-8"))?;
    let mut bytes = Vec::new();
    crate::output::write_document(&mut bytes, &response)?;
    Ok(bytes)
}

/// The response document and the exit status, which are one fact reported two ways.
///
/// A provider that looked answers, whether it found anything or not. A state that prevented
/// one is the reason there is no answer, so it is a failure carrying the code that names
/// the state — and the code is the core's own reason, so a state added there is a compile
/// error rather than a condition folded onto a neighbour.
fn document(
    package: &CurrentPackageCandidate,
    query: &str,
    snapshot: &SupportingMaterialSnapshot<'_>,
) -> Result<(Response<Found>, Exit), NotRepresentable> {
    let root = package.context().root().absolute_path();
    match snapshot {
        SupportingMaterialSnapshot::Answered {
            provider,
            completeness,
            orders,
            diagnostics: reported,
        } => Ok((
            Response::answered(
                CONTRACT,
                Context::rooted_at(root)?,
                Found {
                    query: query.to_owned(),
                    provider: (*provider).to_owned(),
                    complete: completeness.is_complete(),
                    partial_reason: match completeness {
                        Completeness::Complete => None,
                        Completeness::Partial(reason) => Some(reason.as_str()),
                    },
                    orders: orders
                        .iter()
                        .map(|group| {
                            Ok(OrderMatches {
                                order: order_summary(
                                    group.order(),
                                    package.vendor_for_order(group.position()),
                                )?,
                                files: group
                                    .files()
                                    .iter()
                                    .map(file)
                                    .collect::<Result<_, NotRepresentable>>()?,
                            })
                        })
                        .collect::<Result<_, NotRepresentable>>()?,
                    diagnostics: reported
                        .iter()
                        .map(ProviderDiagnosticProjection::of)
                        .collect(),
                },
                // What the *load* observed about this Package, which is a different subject
                // from what the provider said about this query.
                diagnostics(&flattened(package.diagnostic_groups()))?,
            ),
            Exit::Success,
        )),
        SupportingMaterialSnapshot::Unavailable {
            provider,
            reason,
            diagnostics: reported,
        } => {
            let failed = Failed {
                diagnostics: flattened(package.diagnostic_groups()),
                ..unavailable(root, *provider, *reason, reported)
            };
            Ok((
                failed_document(CONTRACT, Context::rooted_at(root)?, &failed)?,
                Exit::Failure,
            ))
        }
    }
}

fn file(matched: &MatchedFile) -> Result<MatchedFileProjection, NotRepresentable> {
    Ok(MatchedFileProjection {
        location: LocationProjection::of(matched.location())?,
        filename: matched.filename().to_owned(),
        kind: matched.kind().map(str::to_owned),
        modified_unix_seconds: matched.modified_unix_seconds(),
        rank: matched.rank(),
    })
}

/// The typed failure for one platform state that prevented an answer.
///
/// **One sentence per state**, for the reason there is one code per state — see
/// [`FailureCode::SupportingMaterialUnsupported`], where that reason lives. The shared
/// clause every one of them ends with is the absence of a fallback, said once here rather
/// than five times.
///
/// The provider's own words travel in the message, which is the only channel a failure has
/// for them: `data` is `null` on every failure, so the payload that would otherwise carry
/// them does not exist.
fn unavailable(
    root: &Path,
    provider: Option<&'static str>,
    reason: UnavailableReason,
    reported: &[ProviderDiagnostic],
) -> Failed {
    let named = provider.unwrap_or("no provider");
    let mut message = match reason {
        UnavailableReason::Unsupported => {
            "this build has no Supporting Material index provider for this platform, so \
             nothing looked."
                .to_owned()
        }
        UnavailableReason::IndexingDisabled => {
            format!("{named} is switched off on this machine, so it has nothing to answer from.")
        }
        // Not "does not index", which claims more than was learned: this state is also what
        // a volume the platform would not report on at all settles as.
        UnavailableReason::VolumeNotIndexed => format!(
            "{named} has nothing to answer from for the volume {} is on.",
            root.display()
        ),
        // True of both things this state is: an index that refused this application, and a
        // Package this process is not allowed to read. Naming only the first would tell a
        // reader to look at the wrong permission.
        UnavailableReason::AccessDenied => {
            format!(
                "{named} could not be asked about {}, because access was denied.",
                root.display()
            )
        }
        UnavailableReason::QueryRefused => format!("{named} refused this query."),
    };
    for detail in reported.iter().filter_map(ProviderDiagnostic::detail) {
        message.push(' ');
        message.push_str(detail);
    }
    message.push_str(
        " SpielOrders never walks or indexes Supporting Material itself, so there is nothing \
         weaker to fall back to.",
    );
    Failed {
        code: FailureCode::of_supporting_material(reason),
        // The Package is fine and its Supporting Material is untouched: what could not be
        // performed is a platform operation.
        scope: FailureScope::Operation,
        message,
        // No path: this failure is about a platform service rather than about anything in
        // the Package, and the Root the command was about is the response's `context`.
        paths: Vec::new(),
        identities: provider
            .map(Identity::supporting_material_provider)
            .into_iter()
            .collect(),
        package_root: Some(root.to_path_buf()),
        diagnostics: Vec::new(),
    }
}

/// The human surface. Nothing here is a parsing contract.
fn human(
    package: &CurrentPackageCandidate,
    query: &str,
    snapshot: &SupportingMaterialSnapshot<'_>,
    output: &Output,
) -> io::Result<Exit> {
    let root = package.context().root().absolute_path();
    let exit = match snapshot {
        SupportingMaterialSnapshot::Answered {
            provider,
            completeness,
            orders,
            diagnostics: reported,
        } => {
            // Written inside this arm rather than above the match: a run that could not
            // answer leaves standard output empty, which is the same discipline every other
            // command's failure keeps.
            output.detail("Package Root", root.as_os_str())?;
            output.detail("Query", &text(query))?;
            output.summary("Provider", &text(provider))?;
            output.summary(
                "Results",
                &text(match completeness {
                    Completeness::Complete => "complete",
                    Completeness::Partial(reason) => reason.as_str(),
                }),
            )?;
            if orders.is_empty() {
                // What an empty answer says, and what it does not: the index found no
                // matching file, which is not a claim that none exists.
                output.summary("Matches", &text("none — the index found no matching file"))?;
            }
            for group in orders {
                output.summary("Order", &text(group.order().document().title.as_str()))?;
                for matched in group.files() {
                    output.summary(
                        "File",
                        matched.location().package_relative_path().map_or_else(
                            || matched.location().absolute_path().as_os_str(),
                            text_of,
                        ),
                    )?;
                    if let Some(kind) = matched.kind() {
                        output.detail("Kind", &text(kind))?;
                    }
                    if let Some(modified) = matched.modified_unix_seconds() {
                        output.detail(
                            "Modified",
                            &text(&crate::moment::displayed_moment(modified)),
                        )?;
                    }
                    // Provider rank is one of the three things the decided human surface
                    // shows only under `--verbose`.
                    if let Some(rank) = matched.rank() {
                        output.detail("Rank", &text(&rank.to_string()))?;
                    }
                }
            }
            for diagnostic in reported.iter() {
                output.warning(&reported_sentence(diagnostic))?;
            }
            Exit::Success
        }
        SupportingMaterialSnapshot::Unavailable {
            provider,
            reason,
            diagnostics: reported,
        } => {
            let failed = unavailable(root, *provider, *reason, reported);
            output.error(&failed.message)?;
            Exit::Failure
        }
    };
    for group in package.diagnostic_groups() {
        for diagnostic in group.diagnostics() {
            report_diagnostic(output, Some(root), diagnostic)?;
        }
    }
    Ok(exit)
}

/// One provider diagnostic as a sentence, which is this adapter's work: the core carries a
/// code and the provider's own words and composes nothing.
fn reported_sentence(diagnostic: &ProviderDiagnostic) -> String {
    let opening = match diagnostic.code() {
        spielorders_core::supporting_material::ProviderDiagnosticCode::TimedOut => {
            "the index did not settle within the time it was given, so these are the results \
             gathered so far"
        }
        spielorders_core::supporting_material::ProviderDiagnosticCode::Cancelled => {
            "the query was cancelled, so these are the results gathered so far"
        }
        spielorders_core::supporting_material::ProviderDiagnosticCode::ProviderReported => {
            "the index reported"
        }
    };
    match diagnostic.detail() {
        Some(detail) => format!("{opening}: {detail}"),
        None => opening.to_owned(),
    }
}

/// One borrowed string as the bytes an output line writes.
fn text_of(value: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spielorders_core::{
        canonical::CanonicalPackageLoad,
        initialization::{
            InitializationOptions, commit_package_initialization, plan_package_initialization,
        },
        mutation::MutationReport,
        ports::{Cancellation, DirectoryFileSystem, FileSystem, SupportingMaterialIndex},
        supporting_material::{
            PartialReason, ProviderAnswer, ProviderHit, SupportingMaterialRequest,
        },
    };
    use std::path::PathBuf;

    // Decision sources: Define lookup and search behavior; Define the Rust CLI capability
    // contract.

    // **Unit tests rather than process-level ones, for the reason this command has any:**
    // every claim below is about a snapshot a real run of the shipped process cannot be
    // made to produce — a found file, a partial answer, or a platform state the shipped
    // provider does not distinguish — because no test may depend on what a platform index
    // holds. What the shipped process does reach is asserted from real processes in
    // `tests/supporting_material_search.rs`.
    //
    // The fixtures write through `PlatformFileSystem`, which is this crate's own filesystem
    // boundary, so nothing here names `fs::` or `env::` — and each tree is removed again
    // through the same boundary.

    /// A Package with one Order holding two Supporting Material files, and its Root.
    ///
    /// One of the two carries [`ASKED`] in its name and in its text, so an answer that named
    /// it could only have come from something reading the tree — which is what makes the
    /// claim that nothing walks a proof here rather than a restatement.
    fn package_with_one_order(label: &str) -> (PathBuf, CurrentPackageCandidate) {
        let root = tree(label);
        let plan = plan_package_initialization(
            &root,
            &InitializationOptions::new("Orders", "Removed"),
            &PlatformFileSystem,
        )
        .expect("the fixture Package can be planned");
        assert!(matches!(
            commit_package_initialization(plan, &PlatformFileSystem),
            MutationReport::Committed { .. }
        ));
        let root = PlatformFileSystem
            .canonicalize(&root)
            .expect("the fixture Package Root resolves");
        let directory = root.join("Orders/Laser_components");
        PlatformFileSystem
            .create_directory(&directory)
            .expect("the fixture Order Directory");
        for (path, contents) in [
            (directory.join(".spielorders/order.json"), ORDER),
            (directory.join("README.md"), ""),
            (directory.join("quote.pdf"), ""),
            (directory.join("spectra/coherence.pdf"), ASKED),
        ] {
            if let Some(parent) = path.parent() {
                // A file at the top of the Order Directory has a parent that is already
                // there, which is the one failure ignored here.
                let _ = PlatformFileSystem.create_directory(parent);
            }
            PlatformFileSystem
                .write(&path, contents.as_bytes())
                .expect("the fixture file");
        }
        let package = match inspect_canonical_package(&root, &PlatformFileSystem) {
            CanonicalPackageLoad::CurrentCandidate(package) => package,
            other => panic!("the fixture Package loads as a current candidate: {other:?}"),
        };
        (root, package)
    }

    /// The word every query below asks for.
    ///
    /// Named because [`package_with_one_order`] holds a Supporting Material file carrying it,
    /// which is what a run that answered from the filesystem would have found.
    const ASKED: &str = "coherence";

    const ORDER: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Laser components",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "items": []
}
"#;

    /// One empty directory under this build's own output tree, unique to this process.
    ///
    /// `env!` is a compile-time macro rather than a read of the environment, so this names
    /// no effect: the path is fixed when the test is built.
    fn tree(label: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for component in [
            "fixtures".to_owned(),
            format!("{}-{label}", std::process::id()),
            "package".to_owned(),
        ] {
            path.push(component);
            // The first level is shared by every fixture in this run, so a directory that
            // is already there is expected — and the last one is this test's own, which the
            // initialization below refuses to plan into unless it is empty.
            let _ = PlatformFileSystem.create_directory(&path);
        }
        path
    }

    /// Removes the whole tree one fixture stands in, through the same boundary that wrote
    /// it, so a run leaves nothing behind in this build's output directory.
    fn discard_fixture(root: &Path) {
        discard(root.parent().unwrap_or(root));
    }

    /// Removes one directory and everything under it.
    fn discard(path: &Path) {
        let Ok(entries) = PlatformFileSystem.read_directory(path) else {
            return;
        };
        for entry in entries {
            let child = path.join(entry.name());
            match entry.kind() {
                spielorders_core::ports::FileSystemEntryKind::Directory => discard(&child),
                _ => {
                    let _ = PlatformFileSystem.remove_file(&child);
                }
            }
        }
        let _ = PlatformFileSystem.remove_directory(path);
    }

    /// A provider that answers with what a test prepared.
    struct FakeIndex {
        name: &'static str,
        answer: ProviderAnswer,
    }

    impl SupportingMaterialIndex for FakeIndex {
        fn name(&self) -> &'static str {
            self.name
        }

        fn search(
            &self,
            _request: &SupportingMaterialRequest<'_>,
            _cancellation: &dyn Cancellation,
        ) -> ProviderAnswer {
            self.answer.clone()
        }
    }

    struct NotCancelled;

    impl Cancellation for NotCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    /// The document and the exit status one snapshot produces, as a caller reads them.
    fn answered(
        package: &CurrentPackageCandidate,
        answer: ProviderAnswer,
    ) -> (serde_json::Value, Exit) {
        let index = FakeIndex {
            name: "fake",
            answer,
        };
        response_from(package, Some(&index))
    }

    /// The same, from the provider composition a build actually ships — `None` being the one
    /// an executable built for a platform with no index has.
    fn response_from(
        package: &CurrentPackageCandidate,
        provider: Option<&dyn SupportingMaterialIndex>,
    ) -> (serde_json::Value, Exit) {
        let snapshot = search_supporting_material(
            package,
            &SupportingMaterialQuery::new(ASKED, BOUND),
            provider,
            &NotCancelled,
        );
        let (response, exit) =
            document(package, ASKED, &snapshot).expect("the fixture paths are UTF-8");
        (
            serde_json::to_value(&response).expect("the response serializes"),
            exit,
        )
    }

    #[cfg(unix)]
    #[test]
    fn native_backslashes_survive_the_file_search_response() {
        let (root, package) = package_with_one_order("native-filename");
        let path = root.join("Orders/Laser_components/vendor\\quote.pdf");
        PlatformFileSystem.write(&path, b"opaque bytes").unwrap();
        let (document, exit) = answered(
            &package,
            ProviderAnswer::Looked {
                completeness: Completeness::Complete,
                hits: vec![ProviderHit {
                    path: path.clone(),
                    kind: None,
                    modified_unix_seconds: None,
                    rank: None,
                }],
                diagnostics: Vec::new(),
            },
        );
        discard_fixture(&root);
        assert_eq!(exit, Exit::Success);
        let file = &document["data"]["orders"][0]["files"][0];
        assert_eq!(file["filename"], "vendor\\quote.pdf");
        assert_eq!(file["location"]["absolute_path"], path.to_str().unwrap());
        assert_eq!(
            file["location"]["package_relative_path"],
            "Orders/Laser_components/vendor\\quote.pdf"
        );
    }

    /// An answer names the Order each file was found under, and every fact the provider
    /// carried for it, in both spellings of its path.
    ///
    /// **The defect this catches is a result a caller cannot act on.** A file row that
    /// carried only an absolute path would give the desktop nothing to show inside an Order
    /// and give a scripted caller nothing to compare against the Package it is reading; a
    /// group that named no Order would give neither of them anywhere to navigate to.
    #[test]
    fn an_answer_names_the_order_each_file_is_under_and_the_facts_the_provider_carried() {
        let (root, package) = package_with_one_order("answer");
        let directory = root.join("Orders/Laser_components");

        let (document, exit) = answered(
            &package,
            ProviderAnswer::Looked {
                completeness: Completeness::Complete,
                hits: vec![
                    ProviderHit {
                        path: directory.join("quote.pdf"),
                        kind: Some("PDF document".to_owned()),
                        modified_unix_seconds: Some(1_785_000_000),
                        rank: Some(0.25),
                    },
                    ProviderHit {
                        path: directory.join("spectra/coherence.pdf"),
                        kind: None,
                        modified_unix_seconds: None,
                        rank: Some(0.75),
                    },
                ],
                diagnostics: Vec::new(),
            },
        );
        discard_fixture(&root);

        assert_eq!(exit, Exit::Success);
        assert_eq!(document["ok"], true);
        assert_eq!(document["data"]["provider"], "fake");
        assert_eq!(document["data"]["query"], ASKED);
        assert_eq!(document["data"]["complete"], true);
        assert!(document["data"]["partial_reason"].is_null());
        assert_eq!(
            document["data"]["orders"][0]["order"]["title"], "Laser components",
            "the group names the Order the files are under: {document}"
        );

        let files = &document["data"]["orders"][0]["files"];
        assert_eq!(
            files[0]["location"]["package_relative_path"],
            "Orders/Laser_components/spectra/coherence.pdf",
            "the most relevant file first"
        );
        assert_eq!(
            files[0]["location"]["absolute_path"],
            directory
                .join("spectra/coherence.pdf")
                .to_str()
                .expect("the fixture path is UTF-8")
        );
        assert_eq!(files[0]["filename"], "coherence.pdf");
        assert!(files[0]["kind"].is_null(), "the provider offered none");
        assert!(files[0]["modified_unix_seconds"].is_null());
        assert_eq!(files[0]["rank"], 0.75);
        assert_eq!(files[1]["filename"], "quote.pdf");
        assert_eq!(files[1]["kind"], "PDF document");
        assert_eq!(files[1]["modified_unix_seconds"], 1_785_000_000_i64);
        assert_eq!(files[1]["rank"], 0.25);
    }

    /// A partial answer is an answer: it exits `0`, carries what was gathered, and says
    /// which of the two things made it partial.
    ///
    /// **The defect this catches is a bound reported as a failure.** A caller who asked and
    /// waited is better served by the files the index did reach than by an error, and the
    /// mark beside them is what stops a partial list being read as the whole one.
    #[test]
    fn a_partial_answer_exits_zero_and_says_which_of_the_two_made_it_partial() {
        let (root, package) = package_with_one_order("partial");
        let directory = root.join("Orders/Laser_components");
        let gathered = vec![ProviderHit {
            path: directory.join("quote.pdf"),
            kind: None,
            modified_unix_seconds: None,
            rank: None,
        }];

        for (reason, spelled) in [
            (PartialReason::TimedOut, "timed_out"),
            (PartialReason::Cancelled, "cancelled"),
        ] {
            let (document, exit) = answered(
                &package,
                ProviderAnswer::Looked {
                    completeness: Completeness::Partial(reason),
                    hits: gathered.clone(),
                    diagnostics: vec![ProviderDiagnostic::reported("what the index said")],
                },
            );

            assert_eq!(exit, Exit::Success, "{spelled}: a partial answer answers");
            assert_eq!(document["ok"], true, "{spelled}");
            assert_eq!(document["data"]["complete"], false, "{spelled}");
            assert_eq!(document["data"]["partial_reason"], spelled);
            assert_eq!(
                document["data"]["diagnostics"][0]["code"], spelled,
                "{spelled}: the answer carries a diagnostic saying why it is partial"
            );
            assert_eq!(
                document["data"]["diagnostics"][1]["detail"], "what the index said",
                "{spelled}: beside the provider's own words"
            );
            assert_eq!(
                document["data"]["orders"][0]["files"][0]["filename"], "quote.pdf",
                "{spelled}: and what was gathered is still the answer"
            );
        }
        discard_fixture(&root);
    }

    /// An index that found nothing answers; a state that prevented one does not, and says
    /// which state it was.
    ///
    /// **The defect this catches is the two reading alike.** "The index found no matching
    /// file" and "nothing looked for one" send a user to two different places — one to a
    /// different query, the other to the platform — and a response that reported an empty
    /// list for both would send them to the wrong one half the time.
    #[test]
    fn an_empty_answer_and_a_state_that_prevented_one_are_two_different_responses() {
        let (root, package) = package_with_one_order("empty-and-prevented");

        let (found_nothing, exit) = answered(
            &package,
            ProviderAnswer::Looked {
                completeness: Completeness::Complete,
                hits: Vec::new(),
                diagnostics: Vec::new(),
            },
        );
        assert_eq!(exit, Exit::Success);
        assert_eq!(found_nothing["ok"], true);
        assert_eq!(found_nothing["data"]["complete"], true);
        assert_eq!(
            found_nothing["data"]["orders"].as_array().map(Vec::len),
            Some(0),
            "an index that looked and found nothing answers with no groups"
        );

        // Each state's own sentence, gathered as they are reached. A code a consumer branches
        // on and a sentence a human reads are the two channels a state travels in, and two
        // states sharing either of them are indistinguishable in that channel.
        let mut sentences = Vec::new();
        for (reason, code) in [
            (
                UnavailableReason::IndexingDisabled,
                "supporting_material_indexing_disabled",
            ),
            (
                UnavailableReason::VolumeNotIndexed,
                "supporting_material_volume_not_indexed",
            ),
            (
                UnavailableReason::AccessDenied,
                "supporting_material_access_denied",
            ),
            (
                UnavailableReason::QueryRefused,
                "supporting_material_query_refused",
            ),
            (
                UnavailableReason::Unsupported,
                "supporting_material_unsupported",
            ),
        ] {
            let (prevented, exit) = answered(
                &package,
                ProviderAnswer::Unavailable {
                    reason,
                    diagnostics: vec![ProviderDiagnostic::reported("what the platform said")],
                },
            );
            sentences.push(prevented["error"]["message"].to_string());

            assert_eq!(exit, Exit::Failure, "{code}");
            assert_eq!(prevented["ok"], false, "{code}");
            assert!(
                prevented["data"].is_null(),
                "{code}: nothing looked, so there is no answer to carry"
            );
            assert_eq!(prevented["error"]["code"], code);
            assert_eq!(
                prevented["error"]["identities"][0]["kind"], "supporting_material_provider",
                "{code}: and the provider that reported it is named"
            );
            assert_eq!(prevented["error"]["identities"][0]["value"], "fake");
            let message = prevented["error"]["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{code}: a failure carries a message"));
            assert!(
                message.contains("what the platform said"),
                "{code}: the provider's own words reach the caller: {message}"
            );
            assert!(
                message.contains("never walks or indexes Supporting Material"),
                "{code}: and so does the absence of a fallback: {message}"
            );
        }
        discard_fixture(&root);

        let mut distinct = sentences.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            sentences.len(),
            "each state says something the others do not: {sentences:?}"
        );
    }

    /// A build with no provider for its platform says so, names no provider, and reports no
    /// file though the Package holds one the query matches.
    ///
    /// **This is that build's own answer rather than an imitation of it.** What an executable
    /// built for a platform with no index does differently is hand this capability `None` —
    /// `effects::supporting_material_index` is the whole of the difference — so the response
    /// below is the one such a build writes, produced on a platform that has a provider.
    ///
    /// **The defect this catches is a fallback.** The Package holds a Supporting Material file
    /// whose name and text both carry the word being asked for, so a run that walked the tree
    /// would have something to report; `data` is the only place a result could travel, and it
    /// is null. The other half is the sentence: a state that named the absent provider, or one
    /// generic failure covering every state, leaves a user with nothing to act on — and
    /// what they can act on here is not a query or a setting but shipping a provider for this
    /// platform.
    #[test]
    fn a_build_with_no_provider_says_so_and_reports_nothing_it_did_not_look_for() {
        let (root, package) = package_with_one_order("no-provider");

        let (prevented, exit) = response_from(&package, None);
        discard_fixture(&root);

        assert_eq!(exit, Exit::Failure);
        assert_eq!(prevented["ok"], false);
        assert!(
            prevented["data"].is_null(),
            "nothing looked, so there is no answer to carry: {prevented}"
        );
        assert_eq!(
            prevented["error"]["code"], "supporting_material_unsupported",
            "the state is the absence of a provider, not a provider's condition: {prevented}"
        );
        assert_eq!(
            prevented["error"]["identities"],
            serde_json::json!([]),
            "there was no provider to name: {prevented}"
        );
        let message = prevented["error"]["message"]
            .as_str()
            .expect("a failure carries a message");
        assert!(
            message.contains("no Supporting Material index provider for this platform"),
            "the sentence says which state this is: {message}"
        );
        assert!(
            message.contains("never walks or indexes Supporting Material"),
            "and that there is nothing weaker behind it: {message}"
        );
    }
}
