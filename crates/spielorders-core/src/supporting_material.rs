//! What a platform's own index found under a loaded Package's Order Directories.
//!
//! **A loaded Package rather than a path, so nothing here reads.** The Order Directories a
//! query is scoped to, the managed documents it excludes, and the Order each answer groups
//! under are all in the projection the ordinary load already produced. A capability that
//! took a root would load the Package a second time.
//!
//! **The platform looks; this decides what the answer means.** A provider is asked once,
//! for the whole Package, and hands back the paths its index holds. Which Order a path
//! belongs to, which paths are this application's own managed documents, which of them are
//! the same file reported twice, and what order they come back in are Package rules, so
//! they are settled here rather than in each platform's adapter — where a second platform
//! would settle them again, differently.
//!
//! **Nothing here walks or indexes Supporting Material.** Where no provider can look, the
//! answer says which platform state prevented it. There is no fallback that reads the tree
//! itself, which is why the two are separate variants of [`SupportingMaterialSnapshot`]
//! rather than an empty result with a note attached: a caller cannot mistake "the index
//! found nothing" for "nothing looked".
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract; Effect and adapter boundaries.

use crate::{
    canonical::{CurrentPackageCandidate, HealthyOrder},
    ports::{Cancellation, SupportingMaterialIndex},
    readme::Location,
};
use std::{
    cmp::Ordering,
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

/// One query, and the bound the provider has to answer it within.
///
/// The text is handed to the provider as written. The provider governs matching — there is
/// no case control and no structured filter here — so nothing tokenizes, folds, or
/// rewrites it on the way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportingMaterialQuery {
    text: String,
    timeout: Duration,
}

impl SupportingMaterialQuery {
    pub fn new(text: &str, timeout: Duration) -> Self {
        Self {
            text: text.to_owned(),
            timeout,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// One query as the provider receives it: the text, the bound, and where to look.
///
/// **The scopes are the Package's healthy Order Directories**, so a provider never decides
/// for itself what belongs to this Package. A Damaged Order is absent, and not by being
/// filtered: its record could not be read, so it is not one of the Orders a file could be
/// navigated to and it never enters the list this is built from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportingMaterialRequest<'a> {
    text: &'a str,
    timeout: Duration,
    scopes: Vec<&'a Path>,
}

impl<'a> SupportingMaterialRequest<'a> {
    pub(crate) fn new(text: &'a str, timeout: Duration, scopes: Vec<&'a Path>) -> Self {
        Self {
            text,
            timeout,
            scopes,
        }
    }

    pub fn text(&self) -> &str {
        self.text
    }

    /// The bound this answer must settle within. A provider that reaches it returns what it
    /// has gathered, marked [`Completeness::Partial`].
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The Order Directories to look inside, and nothing outside them.
    pub fn scopes(&self) -> &[&'a Path] {
        &self.scopes
    }
}

/// One file a provider's index holds, in the facts the index itself carries.
///
/// Every fact beside the path is optional because an index is free to hold none of them,
/// and reading one off the filesystem instead would be this application inspecting
/// Supporting Material. `rank` is the provider's own relevance: `None` is the provider
/// offering none for this file, which the deterministic fallback ordering is for.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderHit {
    pub path: PathBuf,
    /// The kind of file, in the provider's own words.
    pub kind: Option<String>,
    /// Seconds since the Unix epoch. The one spelling every platform index can produce,
    /// and one nothing has to be parsed to compare — rendering a date is presentation.
    pub modified_unix_seconds: Option<i64>,
    pub rank: Option<f64>,
}

/// Whether a provider looked at everything in scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completeness {
    Complete,
    /// Part of what is in scope was not looked at, for this reason. The results gathered so
    /// far are still the answer.
    Partial(PartialReason),
}

impl Completeness {
    pub fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Why an answer is only part of what is in scope.
///
/// Both are ordinary outcomes rather than failures: the caller asked and got what there
/// was. Neither is a state a provider can be in — they are things that happened to one
/// query — which is why they are here rather than beside [`UnavailableReason`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartialReason {
    /// The bound elapsed.
    TimedOut,
    /// The caller cancelled.
    Cancelled,
}

impl PartialReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
        }
    }
}

/// What prevented any provider from looking.
///
/// **Each is its own value rather than one generic failure**, because a caller can act on
/// some of them and not on others: switching indexing on, moving a Package to an indexed
/// volume, and granting access are three different things to do, and a build with no
/// provider at all is none of them. An adapter reporting these to a user branches on them
/// and spells each correction once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableReason {
    /// The platform has an index and it is switched off.
    IndexingDisabled,
    /// The volume this Package is on is not indexed, or is one the provider does not index.
    VolumeNotIndexed,
    /// Access needed to answer was denied: to the index, or to the directories an answer
    /// would have been about.
    AccessDenied,
    /// The provider refused this query.
    QueryRefused,
    /// This build has no Supporting Material index provider for this platform. The one
    /// value that describes the absence of a provider rather than a provider's condition.
    Unsupported,
}

impl UnavailableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IndexingDisabled => "indexing_disabled",
            Self::VolumeNotIndexed => "volume_not_indexed",
            Self::AccessDenied => "access_denied",
            Self::QueryRefused => "query_refused",
            Self::Unsupported => "unsupported",
        }
    }
}

/// What state the lookup was in.
///
/// Derived from the snapshot rather than stored beside it — see
/// [`SupportingMaterialSnapshot::state`] — so a snapshot that carries results and a state
/// saying nothing looked cannot be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderState {
    Available,
    Unavailable(UnavailableReason),
}

impl ProviderState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable(reason) => reason.as_str(),
        }
    }
}

/// What a diagnostic beside an answer is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderDiagnosticCode {
    TimedOut,
    Cancelled,
    /// Something the provider itself reported, in its own words.
    ProviderReported,
}

impl ProviderDiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TimedOut => PartialReason::TimedOut.as_str(),
            Self::Cancelled => PartialReason::Cancelled.as_str(),
            Self::ProviderReported => "provider_reported",
        }
    }
}

/// One diagnostic beside an answer.
///
/// **A provider can only construct the third kind.** The two that say why an answer is
/// partial are derived from its [`Completeness`] here, so the typed reason and the
/// diagnostic naming it cannot come to disagree — which they could if a provider supplied
/// both halves and set one of them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDiagnostic {
    code: ProviderDiagnosticCode,
    detail: Option<String>,
}

impl ProviderDiagnostic {
    /// What the provider itself said about this query, in its own words.
    pub fn reported(detail: &str) -> Self {
        Self {
            code: ProviderDiagnosticCode::ProviderReported,
            detail: Some(detail.to_owned()),
        }
    }

    fn partial(reason: PartialReason) -> Self {
        Self {
            code: match reason {
                PartialReason::TimedOut => ProviderDiagnosticCode::TimedOut,
                PartialReason::Cancelled => ProviderDiagnosticCode::Cancelled,
            },
            detail: None,
        }
    }

    pub fn code(&self) -> ProviderDiagnosticCode {
        self.code
    }

    /// The provider's own words, where it had any. This core composes no sentence.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// One settled answer from one provider.
///
/// Two variants and no third, because a provider either looked or did not: an empty
/// [`Self::Looked`] is an index that found nothing, and that is a different sentence from
/// an index that could not be asked.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderAnswer {
    Looked {
        completeness: Completeness,
        hits: Vec<ProviderHit>,
        diagnostics: Vec<ProviderDiagnostic>,
    },
    Unavailable {
        reason: UnavailableReason,
        diagnostics: Vec<ProviderDiagnostic>,
    },
}

/// One file a provider found, in the Package's own terms.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedFile {
    location: Location,
    filename: String,
    kind: Option<String>,
    modified_unix_seconds: Option<i64>,
    rank: Option<f64>,
}

impl MatchedFile {
    /// The file's absolute path and the Package's own spelling for it.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// One of the two facts the deterministic fallback ordering sorts by, the other being
    /// the path beside it.
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    pub fn modified_unix_seconds(&self) -> Option<i64> {
        self.modified_unix_seconds
    }

    /// The provider's own relevance, or `None` where it offered none for this file.
    pub fn rank(&self) -> Option<f64> {
        self.rank
    }
}

/// The files one Order holds, beside the Order that holds them.
///
/// The position is the Order's own among [`CurrentPackageCandidate::healthy_orders`], so a
/// caller resolves its Vendor through
/// [`CurrentPackageCandidate::vendor_for_order`] exactly as it does for the whole list.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderMatches<'a> {
    position: usize,
    order: &'a HealthyOrder,
    files: Vec<MatchedFile>,
}

impl<'a> OrderMatches<'a> {
    pub fn position(&self) -> usize {
        self.position
    }

    pub fn order(&self) -> &'a HealthyOrder {
        self.order
    }

    /// Never empty: an Order no file was found in is not one of the groups.
    pub fn files(&self) -> &[MatchedFile] {
        &self.files
    }
}

/// One settled answer about one Package's Supporting Material.
///
/// **The two variants are the two sentences.** [`Self::Answered`] is a provider that
/// looked, whether it found anything or not and whether it looked at everything or not;
/// [`Self::Unavailable`] is nothing having looked, and why. Collapsing them would make an
/// index that found no matches indistinguishable from an index that could not be asked,
/// which is the one distinction a caller most needs.
#[derive(Clone, Debug, PartialEq)]
pub enum SupportingMaterialSnapshot<'a> {
    Answered {
        /// The provider that answered, in its own name.
        provider: &'static str,
        completeness: Completeness,
        /// One group per Order holding at least one file, in the Package's own Order order.
        orders: Vec<OrderMatches<'a>>,
        diagnostics: Vec<ProviderDiagnostic>,
    },
    Unavailable {
        /// The provider that could not look, or `None` where there was none to ask.
        provider: Option<&'static str>,
        reason: UnavailableReason,
        diagnostics: Vec<ProviderDiagnostic>,
    },
}

impl SupportingMaterialSnapshot<'_> {
    /// What state the lookup was in, as one value.
    pub fn state(&self) -> ProviderState {
        match self {
            Self::Answered { .. } => ProviderState::Available,
            Self::Unavailable { reason, .. } => ProviderState::Unavailable(*reason),
        }
    }

    pub fn provider(&self) -> Option<&'static str> {
        match self {
            Self::Answered { provider, .. } => Some(provider),
            Self::Unavailable { provider, .. } => *provider,
        }
    }

    pub fn diagnostics(&self) -> &[ProviderDiagnostic] {
        match self {
            Self::Answered { diagnostics, .. } | Self::Unavailable { diagnostics, .. } => {
                diagnostics
            }
        }
    }
}

/// Asks one provider for the Supporting Material of `package` that matches `query`, and
/// returns one settled answer.
///
/// **Synchronous, and settled.** It takes a cancellation and a bounded timeout and hands
/// back a value rather than a stream or a future, so no asynchronous runtime enters this
/// core or the adapters over it. Running it off a user interface thread is the caller's
/// arrangement: the desktop shell calls it from a worker, and a command-line caller waits
/// and prints once.
///
/// **`None` is a provider this build does not have**, which is one of the platform states
/// the answer can carry rather than a caller error: a build with no provider for its
/// platform reports [`UnavailableReason::Unsupported`] and looks at nothing.
pub fn search_supporting_material<'a>(
    package: &'a CurrentPackageCandidate,
    query: &SupportingMaterialQuery,
    provider: Option<&dyn SupportingMaterialIndex>,
    cancellation: &dyn Cancellation,
) -> SupportingMaterialSnapshot<'a> {
    let Some(provider) = provider else {
        return SupportingMaterialSnapshot::Unavailable {
            provider: None,
            reason: UnavailableReason::Unsupported,
            diagnostics: Vec::new(),
        };
    };
    let scopes: Vec<&Path> = package
        .healthy_orders()
        .iter()
        .map(|order| order.paths().directory().absolute_path())
        .collect();
    let request = SupportingMaterialRequest::new(query.text(), query.timeout(), scopes);
    match provider.search(&request, cancellation) {
        ProviderAnswer::Unavailable {
            reason,
            diagnostics,
        } => SupportingMaterialSnapshot::Unavailable {
            provider: Some(provider.name()),
            reason,
            diagnostics,
        },
        ProviderAnswer::Looked {
            completeness,
            hits,
            mut diagnostics,
        } => {
            // A cancelled request never reports completion, whatever the provider said: the
            // caller withdrew the question, so what came back is what had been gathered when
            // they did. Asked here rather than trusted from the answer because the provider
            // is the one thing that cannot be relied on to have noticed.
            let completeness = if cancellation.is_cancelled() {
                Completeness::Partial(PartialReason::Cancelled)
            } else {
                completeness
            };
            if let Completeness::Partial(reason) = completeness {
                diagnostics.insert(0, ProviderDiagnostic::partial(reason));
            }
            SupportingMaterialSnapshot::Answered {
                provider: provider.name(),
                completeness,
                orders: grouped(package, hits),
                diagnostics,
            }
        }
    }
}

/// The hits that are this Package's Supporting Material, under the Orders that hold them.
///
/// **Deduplication is by standardized path alone.** One path reported twice is one result;
/// two paths holding identical bytes are two, because they are two files and disposing of
/// one would not dispose of the other. The comparison is the path's own components rather
/// than its text, so a redundant separator or a `.` component does not make a second
/// result — a choice this code makes, since nothing decides how a provider spells a path.
/// The first report of a path is the one kept, for the same reason: a repeat is the same
/// file.
fn grouped<'a>(
    package: &'a CurrentPackageCandidate,
    hits: Vec<ProviderHit>,
) -> Vec<OrderMatches<'a>> {
    let root = package.context().root().absolute_path();
    let orders = package.healthy_orders();
    let mut found: Vec<Vec<MatchedFile>> = orders.iter().map(|_| Vec::new()).collect();
    let mut reported: BTreeSet<PathBuf> = BTreeSet::new();
    for hit in hits {
        if !reported.insert(hit.path.clone()) {
            continue;
        }
        // A file with no Order to group it under is not one of this Package's results: the
        // group is how a file is navigated to, so a path outside every Order Directory has
        // nowhere to be reported.
        let Some((position, order)) = orders
            .iter()
            .enumerate()
            .find(|(_, order)| holds(order, &hit.path))
        else {
            continue;
        };
        if is_managed(order, &hit.path) {
            continue;
        }
        let Some(location) = Location::supporting_material(root, hit.path) else {
            continue;
        };
        let relative = location
            .package_relative_path()
            .expect("user file is within its Package");
        // The last segment of a normalized Package-relative path, which is the whole of it
        // where the path has no separator.
        let filename = match relative.rfind('/') {
            Some(separator) => relative[separator + 1..].to_owned(),
            None => relative.to_owned(),
        };
        found[position].push(MatchedFile {
            location,
            filename,
            kind: hit.kind,
            modified_unix_seconds: hit.modified_unix_seconds,
            rank: hit.rank,
        });
    }
    orders
        .iter()
        .enumerate()
        .zip(found)
        .filter(|(_, files)| !files.is_empty())
        .map(|((position, order), mut files)| {
            ordered(&mut files);
            OrderMatches {
                position,
                order,
                files,
            }
        })
        .collect()
}

/// Whether this Order Directory holds `path`, which requires `path` to be inside it rather
/// than to be it: these results are file-centric, and the directory is the group.
fn holds(order: &HealthyOrder, path: &Path) -> bool {
    let directory = order.paths().directory().absolute_path();
    path != directory && path.starts_with(directory)
}

/// Whether `path` is one of this Order's own managed documents.
///
/// The Order's conventional README and its reserved metadata subtree, taken from the
/// Order's own locations rather than from the names they are spelled with — so a nested
/// file that merely has a familiar name is unrelated Supporting Material and stays a
/// result, which is what the exclusion says and what spelling the names again here would
/// quietly break.
fn is_managed(order: &HealthyOrder, path: &Path) -> bool {
    path == order.paths().readme().location().absolute_path()
        || path.starts_with(order.paths().metadata().absolute_path())
}

/// The decided order within one Order's group: the provider's relevance descending, then
/// filename, then the path.
///
/// A file the provider offered no relevance for sorts after every file it did — a choice,
/// since the decision says only that the deterministic keys settle unavailable relevance
/// and not where such a file stands among ranked ones. Equal ranks fall through to the same
/// two keys. `total_cmp` rather than a partial comparison so the order is total whatever a
/// provider puts in the field.
fn ordered(files: &mut [MatchedFile]) {
    files.sort_by(|left, right| {
        match (left.rank, right.rank) {
            (Some(left), Some(right)) => right.total_cmp(&left),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| left.filename.cmp(&right.filename))
        .then_with(|| left.location.cmp(&right.location))
    });
}
