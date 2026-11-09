//! What a front end's own platform index found under a loaded Package's Order Directories.
//!
//! **A loaded Package rather than a path, so nothing here reads a Package.** The directory
//! a front end searches, the managed documents its hits exclude, and the Order each hit
//! groups under are all in the projection the ordinary load already produced. What the
//! filesystem is asked is only which entry a path names, because a provider is free to
//! spell a path differently from the Package and only the volume knows whether two
//! spellings are one entry.
//!
//! **The front end looks; this decides what the answer means.** A front end queries its
//! platform's index once, over the directory [`Package::supporting_material_directory`]
//! names, and hands over the paths it found with the facts its provider stated. Which
//! Order a path belongs to, which paths are this application's own managed documents,
//! which of them are the same file reported twice, and what order they come back in are
//! Package rules, so they are settled here rather than in each front end.
//!
//! **Nothing here walks or indexes Supporting Material.** Where no provider could look,
//! the answer is the failure naming which platform state prevented it, so a caller
//! cannot mistake "the index found nothing" for "nothing looked".
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract; Effect and adapter boundaries.

use crate::{
    Answer, Failure, Package,
    answer::ProviderUnavailable,
    canonical::{CurrentPackageCandidate, HealthyOrder},
    ports::{Cancellation, Effects, FileSystem, FileSystemEntryKind},
    readme::Location,
};
use std::{
    cell::OnceCell,
    cmp::Ordering,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

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

/// Why an answer is only part of what is in scope.
///
/// Both are ordinary outcomes rather than failures: the caller asked and got what there
/// was. Neither is a state a provider can be in — they are things that happened to one
/// query — which is why they are here rather than among the provider failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartialReason {
    /// The bound elapsed.
    TimedOut,
    /// The caller cancelled.
    Cancelled,
}

/// A condition reported by a provider that exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderUnavailableReason {
    IndexingDisabled,
    VolumeNotIndexed,
    AccessDenied,
    QueryRefused,
}

impl ProviderUnavailableReason {
    /// The failure this reason is, for the provider that gave it.
    fn failure(self, provider: ProviderUnavailable) -> Failure {
        match self {
            Self::IndexingDisabled => Failure::IndexingDisabled(provider),
            Self::VolumeNotIndexed => Failure::VolumeNotIndexed(provider),
            Self::AccessDenied => Failure::AccessDenied(provider),
            Self::QueryRefused => Failure::QueryRefused(provider),
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

/// One diagnostic beside an answer.
///
/// **A provider can only construct the third kind.** The two that say why an answer is
/// partial are derived from its [`Completeness`] here, so the typed reason and the
/// diagnostic naming it cannot come to disagree — which they could if a provider supplied
/// both halves and set one of them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDiagnostic {
    pub code: ProviderDiagnosticCode,
    /// The provider's own words, where it had any. This core composes no sentence.
    pub detail: Option<String>,
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
        reason: ProviderUnavailableReason,
        diagnostics: Vec<ProviderDiagnostic>,
    },
}

/// What a front end's provider answered, under the provider's own name.
///
/// A front end with no provider for its platform supplies none, which is the
/// `unsupported` refusal rather than an answer.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderReport {
    /// The service's established name, which the answer reports and a view labels itself
    /// with.
    pub provider: String,
    pub answer: ProviderAnswer,
}

/// One file a provider found, in the Package's own terms.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedFile {
    /// The file's absolute path and the Package's own spelling for it.
    pub location: Location,
    /// The native-component spelling within the associated Order Directory.
    pub order_relative_path: String,
    /// One of the two facts the deterministic fallback ordering sorts by, the other being
    /// the path beside it.
    pub filename: String,
    pub kind: Option<String>,
    pub modified_unix_seconds: Option<i64>,
    /// The provider's own relevance, or `None` where it offered none for this file.
    pub rank: Option<f64>,
}

/// The files one Order holds, beside the Order that holds them.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderMatches {
    pub order: HealthyOrder,
    /// Never empty: an Order no file was found in is not one of the groups.
    pub files: Vec<MatchedFile>,
}

/// What a provider that looked found about one Package's Supporting Material, whether it
/// found anything or not and whether it looked at everything or not. A provider that could
/// not look is a [`Failure`] instead, so an index that found no matches cannot be mistaken
/// for an index that could not be asked.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportingMaterialSnapshot {
    /// The provider that answered, in its own name.
    pub provider: String,
    pub completeness: Completeness,
    /// One group per Order holding at least one file, in the Package's own Order order.
    pub orders: Vec<OrderMatches>,
    pub diagnostics: Vec<ProviderDiagnostic>,
}

impl Package {
    /// The one directory a front end's provider searches: the Orders Directory, where
    /// every healthy Order Directory is. `None` where the Package has no healthy Order,
    /// so there is nowhere for Supporting Material to be and no query to run.
    pub fn supporting_material_directory(&self) -> Option<&Path> {
        let package = &self.contents;
        if package.healthy_orders.is_empty() {
            return None;
        }
        package
            .context
            .orders_directory
            .as_ref()
            .map(|location| location.absolute_path.as_path())
    }
}

/// The Supporting Material of `package` among what a front end's provider found, and
/// one settled answer, with the Package's findings beside it.
///
/// **`None` is a front end with no provider for its platform**, which refuses as
/// `unsupported`: a platform state rather than a caller error.
///
/// Synchronous and settled: the front end ran its query, bounded and cancellable, before
/// calling this, so no asynchronous runtime enters the core.
pub fn search_supporting_material<E: Effects>(
    package: &Package,
    provider: Option<&ProviderReport>,
    effects: &E,
    cancellation: &dyn Cancellation,
) -> Answer<SupportingMaterialSnapshot> {
    let package = &package.contents;
    Answer::from(search(
        package,
        provider,
        effects.file_system(),
        cancellation,
    ))
    .beside(package.findings())
}

fn search<F: FileSystem>(
    package: &CurrentPackageCandidate,
    provider: Option<&ProviderReport>,
    filesystem: &F,
    cancellation: &dyn Cancellation,
) -> Result<SupportingMaterialSnapshot, Failure> {
    let Some(ProviderReport { provider, answer }) = provider else {
        return Err(Failure::Unsupported(ProviderUnavailable {
            provider: None,
            diagnostics: Vec::new(),
        }));
    };
    match answer.clone() {
        ProviderAnswer::Unavailable {
            reason,
            diagnostics,
        } => Err(reason.failure(ProviderUnavailable {
            provider: Some(provider.clone()),
            diagnostics,
        })),
        ProviderAnswer::Looked {
            completeness,
            hits,
            mut diagnostics,
        } => {
            let orders = grouped(package, hits, filesystem);
            // A cancelled request never reports completion, whatever the provider said: the
            // caller withdrew the question, so what came back is what had been gathered when
            // they did. Asked after grouping, because grouping asks the filesystem.
            let completeness = if cancellation.is_cancelled() {
                Completeness::Partial(PartialReason::Cancelled)
            } else {
                completeness
            };
            if let Completeness::Partial(reason) = completeness {
                diagnostics.insert(0, ProviderDiagnostic::partial(reason));
            }
            Ok(SupportingMaterialSnapshot {
                provider: provider.clone(),
                completeness,
                orders,
                diagnostics,
            })
        }
    }
}

/// The hits that are this Package's Supporting Material, under the Orders that hold them.
///
/// **Every decision is about the entry a hit names.** Which Order holds it, whether it is a
/// managed document, and whether it repeats an earlier hit are all asked of the
/// filesystem's own spelling of both sides, so a provider that spells a path differently
/// from the Package — Spotlight decomposes Unicode, for one — is still matched, and two
/// spellings the volume keeps apart are never merged here.
///
/// **Deduplication is by that entry alone.** One file reported twice, in whatever
/// spellings, is one result; two paths holding identical bytes are two, because they are
/// two files and disposing of one would not dispose of the other. The first report of a
/// file is the one kept, for the same reason: a repeat is the same file.
///
/// **A file is answered in the Package's own spelling**: the Order Directory as the
/// Package names it, and the rest of the path as the filesystem stores it.
fn grouped<F: FileSystem>(
    package: &CurrentPackageCandidate,
    hits: Vec<ProviderHit>,
    filesystem: &F,
) -> Vec<OrderMatches> {
    let root = &package.context.root.absolute_path;
    let orders = SpelledOrders::new(&package.healthy_orders, filesystem);
    let mut found: Vec<Vec<MatchedFile>> =
        package.healthy_orders.iter().map(|_| Vec::new()).collect();
    let mut reported: BTreeSet<PathBuf> = BTreeSet::new();
    for hit in hits {
        let named = spelling(filesystem, &hit.path);
        if !reported.insert(named.clone()) {
            continue;
        }
        // A file with no Order to group it under is not one of this Package's results: the
        // group is how a file is navigated to, so a path outside every Order Directory has
        // nowhere to be reported.
        let Some((position, within)) = orders.holding(&named) else {
            continue;
        };
        if orders.is_managed(position, &named) {
            continue;
        }
        let directory = &orders.orders[position].paths.directory.absolute_path;
        let path = directory.join(within);
        let Some(order_location) = Location::supporting_material(directory, path.clone()) else {
            continue;
        };
        let Some(order_relative_path) = order_location.package_relative_path.clone() else {
            continue;
        };
        let Some(location) = Location::supporting_material(root, path) else {
            continue;
        };
        let relative = location
            .package_relative_path
            .as_deref()
            .expect("user file is within its Package");
        // The last segment of a normalized Package-relative path, which is the whole of it
        // where the path has no separator.
        let filename = match relative.rfind('/') {
            Some(separator) => relative[separator + 1..].to_owned(),
            None => relative.to_owned(),
        };
        found[position].push(MatchedFile {
            location,
            order_relative_path,
            filename,
            kind: hit.kind,
            modified_unix_seconds: hit.modified_unix_seconds,
            rank: hit.rank,
        });
    }
    package
        .healthy_orders
        .iter()
        .zip(found)
        .filter(|(_, files)| !files.is_empty())
        .map(|(order, mut files)| {
            ordered(&mut files);
            OrderMatches {
                order: order.clone(),
                files: files.to_vec(),
            }
        })
        .collect()
}

/// The healthy Orders' own paths in the filesystem's spelling, each asked for only once and
/// only when a hit needs it.
struct SpelledOrders<'a, F> {
    orders: &'a [HealthyOrder],
    filesystem: &'a F,
    directories: Vec<OnceCell<PathBuf>>,
    /// The README and the reserved metadata directory.
    managed: Vec<OnceCell<(PathBuf, PathBuf)>>,
}

impl<'a, F: FileSystem> SpelledOrders<'a, F> {
    fn new(orders: &'a [HealthyOrder], filesystem: &'a F) -> Self {
        Self {
            orders,
            filesystem,
            directories: orders.iter().map(|_| OnceCell::new()).collect(),
            managed: orders.iter().map(|_| OnceCell::new()).collect(),
        }
    }

    fn directory(&self, position: usize) -> &Path {
        self.directories[position].get_or_init(|| {
            spelling(
                self.filesystem,
                &self.orders[position].paths.directory.absolute_path,
            )
        })
    }

    /// The Order whose Directory holds `named`, beside the path within it. Holding requires
    /// `named` to be inside the Directory rather than to be it: these results are
    /// file-centric, and the directory is the group.
    fn holding<'p>(&self, named: &'p Path) -> Option<(usize, &'p Path)> {
        (0..self.orders.len()).find_map(|position| {
            let within = named.strip_prefix(self.directory(position)).ok()?;
            (!within.as_os_str().is_empty()).then_some((position, within))
        })
    }

    /// Whether `named` is one of this Order's own managed documents.
    ///
    /// The Order's conventional README and its reserved metadata subtree, taken from the
    /// Order's own locations rather than from the names they are spelled with — so a nested
    /// file that merely has a familiar name is unrelated Supporting Material and stays a
    /// result, which is what the exclusion says and what spelling the names again here
    /// would quietly break.
    fn is_managed(&self, position: usize, named: &Path) -> bool {
        let (readme, metadata) = self.managed[position].get_or_init(|| {
            let paths = &self.orders[position].paths;
            (
                spelling(self.filesystem, &paths.readme.location.absolute_path),
                spelling(self.filesystem, &paths.metadata.absolute_path),
            )
        });
        named == readme || named.starts_with(metadata)
    }
}

/// The filesystem's own spelling of the entry `path` names, or `path` as given where the
/// filesystem describes no entry there.
///
/// Canonicalizing answers the spelling the filesystem reports for an entry. Where that is
/// the stored spelling of every component, as on macOS, two spellings the volume makes one
/// entry come out equal; where the platform answers the spelling it was given, they stay
/// apart. Either way, two spellings the volume keeps apart never come out equal.
///
/// A final link names the link rather than what it points at, so only the directories
/// above it are resolved and its own name is kept as given — a choice, since the port
/// answers a spelling only by following. It can leave one link reported in two spellings
/// as two results, never two entries as one.
///
/// A path naming nothing, a relative path, and one behind a refusal are judged by the
/// provider's spelling, which is how every hit was judged before the filesystem was asked:
/// such a hit is matched only to a Package path spelled byte for byte the same.
fn spelling<F: FileSystem>(filesystem: &F, path: &Path) -> PathBuf {
    let named = || {
        if !path.is_absolute() {
            return None;
        }
        match filesystem.symlink_metadata(path).ok()?? {
            FileSystemEntryKind::SymbolicLink => Some(
                filesystem
                    .canonicalize(path.parent()?)
                    .ok()?
                    .join(path.file_name()?),
            ),
            _ => filesystem.canonicalize(path).ok(),
        }
    };
    named().unwrap_or_else(|| path.to_path_buf())
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
