//! Supporting Material: the directory a front end's provider searches, the hits that
//! front end supplies, and the eligible files the core groups and orders from them.
//!
//! The front end runs its platform's own index query; the library performs none.

use crate::interface::{
    PathBytes, SPIELORDERS_FAMILY_PROVIDER_STATE, SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED,
    SPIELORDERS_PROVIDER_STATE_CANCELLED, SPIELORDERS_PROVIDER_STATE_COMPLETE,
    SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED, SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED,
    SPIELORDERS_PROVIDER_STATE_TIMED_OUT, SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED,
    SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_OK, SpielordersConstant,
    SpielordersEntries, SpielordersPackage, SpielordersResult, Text,
    call::{Session, deliver, fallible, quietly, settle},
    entries::{EMPTY, Subject, c_text},
    library::is_member,
    records::path_text,
};
use crate::{
    ProviderReport, SupportingMaterialSnapshot,
    model::{
        Completeness, PartialReason, ProviderAnswer, ProviderDiagnostic, ProviderHit,
        ProviderUnavailableReason,
    },
};
use std::ffi::CString;

/// The one directory a front end's Supporting Material provider searches for `package`:
/// its Orders Directory. "" where there is no healthy Order, so no query is needed, and
/// for a Damaged Package.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_material_directory(
    package: &SpielordersPackage,
) -> PathBytes<'_> {
    quietly(PathBytes::new(EMPTY), || {
        PathBytes::new(package.material_directory())
    })
}

/// What a front end's Supporting Material provider found, gathered one hit at a time
/// for spielorders_package_search_material. Like a Draft, it changes in place and must
/// not be used from two threads at once.
#[repr(C)]
pub struct SpielordersHits {
    data: Box<SpielordersHitsData>,
}

struct SpielordersHitsData {
    provider: String,
    hits: Vec<ProviderHit>,
    diagnostics: Vec<ProviderDiagnostic>,
}

/// A new, empty set of hits from the provider named `provider`, such as "spotlight".
/// SPIELORDERS_RESULT_BAD_ARGUMENT when `provider` is not UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_hits_new(
    provider: Text<'_>,
    hits: &mut Option<Box<SpielordersHits>>,
) -> SpielordersResult {
    crate::interface::call::put(hits, None);
    fallible(None, |_| {
        let result = provider
            .to_str()
            .map(|provider| SpielordersHits {
                data: Box::new(SpielordersHitsData {
                    provider: provider.to_owned(),
                    hits: Vec::new(),
                    diagnostics: Vec::new(),
                }),
            })
            .ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT);
        deliver(result, hits)
    })
}

/// Adds one file the provider reported, with the facts it stated: `kind` in the
/// provider's own words, `modified` as whole seconds since the Unix epoch written in
/// decimal digits (a fraction does not parse), and `rank` as the provider's decimal
/// relevance, each "" where the provider stated none. Every
/// path the provider reported is added as it reported it, including ones the library
/// will leave out. SPIELORDERS_RESULT_BAD_ARGUMENT for a fact that does not parse.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_hits_add(
    hits: &mut SpielordersHits,
    path: PathBytes<'_>,
    kind: Text<'_>,
    modified: Text<'_>,
    rank: Text<'_>,
) -> SpielordersResult {
    fallible(None, |_| {
        let stated = |text: Text<'_>| {
            text.to_str()
                .map(|text| (!text.is_empty()).then(|| text.to_owned()))
        };
        let (Some(kind), Some(modified), Some(rank)) =
            (stated(kind), stated(modified), stated(rank))
        else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        let Ok(modified_unix_seconds) = modified.map(|text| text.parse::<i64>()).transpose() else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        let Ok(rank) = rank.map(|text| text.parse::<f64>()).transpose() else {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        };
        if rank.is_some_and(|rank| !rank.is_finite()) {
            return SPIELORDERS_RESULT_BAD_ARGUMENT;
        }
        hits.data.hits.push(ProviderHit {
            path: path.as_path().to_path_buf(),
            kind,
            modified_unix_seconds,
            rank,
        });
        SPIELORDERS_RESULT_OK
    })
}

/// Adds something the provider said about this query, in its own words. A search reads
/// each back with spielorders_search_diagnostic; a refusal carries each as an entry
/// after its failure.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_hits_diagnostic(
    hits: &mut SpielordersHits,
    said: Text<'_>,
) -> SpielordersResult {
    fallible(None, |_| match said.to_str() {
        Some(said) => {
            hits.data
                .diagnostics
                .push(ProviderDiagnostic::reported(said));
            SPIELORDERS_RESULT_OK
        }
        None => SPIELORDERS_RESULT_BAD_ARGUMENT,
    })
}

/// Frees `hits`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_hits_free(hits: Option<Box<SpielordersHits>>) {
    quietly((), || drop(hits));
}

/// The Supporting Material of `package` among `hits`, from a provider in `state` (a
/// `SPIELORDERS_PROVIDER_STATE_*`): one file per filesystem entry, however the provider
/// spelled its path, only under a healthy Order and never one of its managed documents,
/// each answered in the Package's own spelling, grouped by Order in the Package's own
/// Order order, and within an Order by rank descending, then filename, then path. `hits` (or
/// NULL) is NULL where the caller has no provider for its platform, which refuses as
/// SPIELORDERS_FAILURE_UNSUPPORTED. A state in which the provider could not look refuses
/// with its failure.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_search_material(
    package: &SpielordersPackage,
    hits: Option<&SpielordersHits>,
    state: SpielordersConstant,
    token: u64,
    search: &mut Option<Box<SpielordersSearch>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    crate::interface::call::put(search, None);
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let result = (|| {
            if !is_member(SPIELORDERS_FAMILY_PROVIDER_STATE, state) {
                return Err(SPIELORDERS_RESULT_BAD_ARGUMENT);
            }
            let report = hits.map(|hits| ProviderReport {
                provider: hits.data.provider.clone(),
                answer: answer(state, &hits.data),
            });
            let opened = package.healthy(entries)?;
            let snapshot = settle(
                crate::search_supporting_material(
                    opened,
                    report.as_ref(),
                    &session,
                    session.cancellation(),
                ),
                &session,
                Subject::Other,
                entries,
            )?;
            Ok(SpielordersSearch::of(&snapshot))
        })();
        deliver(result, search)
    })
}

/// What the provider in `state` answered with `hits`.
fn answer(state: SpielordersConstant, hits: &SpielordersHitsData) -> ProviderAnswer {
    let looked = |completeness| ProviderAnswer::Looked {
        completeness,
        hits: hits.hits.clone(),
        diagnostics: hits.diagnostics.clone(),
    };
    let unavailable = |reason| ProviderAnswer::Unavailable {
        reason,
        diagnostics: hits.diagnostics.clone(),
    };
    match state {
        SPIELORDERS_PROVIDER_STATE_TIMED_OUT => {
            looked(Completeness::Partial(PartialReason::TimedOut))
        }
        SPIELORDERS_PROVIDER_STATE_CANCELLED => {
            looked(Completeness::Partial(PartialReason::Cancelled))
        }
        SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED => {
            unavailable(ProviderUnavailableReason::IndexingDisabled)
        }
        SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED => {
            unavailable(ProviderUnavailableReason::VolumeNotIndexed)
        }
        SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED => {
            unavailable(ProviderUnavailableReason::AccessDenied)
        }
        SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED => {
            unavailable(ProviderUnavailableReason::QueryRefused)
        }
        _ => looked(Completeness::Complete),
    }
}

/// The eligible files a Supporting Material search found, in the order they are shown.
#[repr(C)]
pub struct SpielordersSearch {
    data: Box<SpielordersSearchData>,
}

struct SpielordersSearchData {
    /// The Order Directories holding a found file, as the Package spells them.
    orders: Vec<std::path::PathBuf>,
    provider: CString,
    state: SpielordersConstant,
    files: Vec<File>,
    diagnostics: Vec<CString>,
}

struct File {
    order_directory: CString,
    path: CString,
    relative_path: CString,
    filename: CString,
    kind: CString,
    modified: CString,
    rank: CString,
}

impl SpielordersSearch {
    fn of(snapshot: &SupportingMaterialSnapshot) -> Self {
        let files = snapshot
            .orders
            .iter()
            .flat_map(|group| {
                group.files.iter().map(|file| File {
                    order_directory: path_text(&group.order.paths.directory.absolute_path),
                    path: path_text(&file.location.absolute_path),
                    relative_path: c_text(file.order_relative_path.as_str()),
                    filename: c_text(file.filename.as_str()),
                    kind: c_text(file.kind.clone().unwrap_or_default()),
                    modified: c_text(
                        file.modified_unix_seconds
                            .map(|seconds| seconds.to_string())
                            .unwrap_or_default(),
                    ),
                    rank: c_text(file.rank.map(|rank| rank.to_string()).unwrap_or_default()),
                })
            })
            .collect();
        Self {
            data: Box::new(SpielordersSearchData {
                orders: snapshot
                    .orders
                    .iter()
                    .filter(|group| !group.files.is_empty())
                    .map(|group| group.order.paths.directory.absolute_path.clone())
                    .collect(),
                provider: c_text(snapshot.provider.as_str()),
                state: match snapshot.completeness {
                    Completeness::Complete => SPIELORDERS_PROVIDER_STATE_COMPLETE,
                    Completeness::Partial(PartialReason::TimedOut) => {
                        SPIELORDERS_PROVIDER_STATE_TIMED_OUT
                    }
                    Completeness::Partial(PartialReason::Cancelled) => {
                        SPIELORDERS_PROVIDER_STATE_CANCELLED
                    }
                },
                files,
                diagnostics: snapshot
                    .diagnostics
                    .iter()
                    .filter_map(|diagnostic| diagnostic.detail.as_deref())
                    .map(c_text)
                    .collect(),
            }),
        }
    }

    /// The Order Directories holding a found file.
    pub(crate) fn orders(&self) -> &[std::path::PathBuf] {
        &self.data.orders
    }

    fn file(&self, index: i64) -> Option<&File> {
        self.data.files.get(usize::try_from(index).ok()?)
    }
}

/// The provider that answered `search`, in its own name.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_provider(search: &SpielordersSearch) -> Text<'_> {
    quietly(Text::new(EMPTY), || Text::new(&search.data.provider))
}

/// Whether the provider looked at everything in scope: SPIELORDERS_PROVIDER_STATE_COMPLETE,
/// _TIMED_OUT, or _CANCELLED. A search whose token was cancelled is never complete.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_state(search: &SpielordersSearch) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || search.data.state)
}

/// How many files `search` found.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_count(search: &SpielordersSearch) -> i64 {
    quietly(-1, || {
        i64::try_from(search.data.files.len()).unwrap_or(i64::MAX)
    })
}

/// The Order Directory file `index` is grouped under; consecutive files share it.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_order_directory(
    search: &SpielordersSearch,
    index: i64,
) -> PathBytes<'_> {
    path_of(search, index, |file| &file.order_directory)
}

/// The absolute path of file `index`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_path(search: &SpielordersSearch, index: i64) -> PathBytes<'_> {
    path_of(search, index, |file| &file.path)
}

/// The path of file `index` within its Order Directory.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_relative_path(
    search: &SpielordersSearch,
    index: i64,
) -> Text<'_> {
    text_of(search, index, |file| &file.relative_path)
}

/// The filename of file `index`.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_filename(search: &SpielordersSearch, index: i64) -> Text<'_> {
    text_of(search, index, |file| &file.filename)
}

/// The kind of file `index`, in the provider's own words, or "".
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_kind(search: &SpielordersSearch, index: i64) -> Text<'_> {
    text_of(search, index, |file| &file.kind)
}

/// When file `index` was modified, as whole seconds since the Unix epoch, or "".
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_modified(search: &SpielordersSearch, index: i64) -> Text<'_> {
    text_of(search, index, |file| &file.modified)
}

/// The provider's decimal relevance for file `index`, or "" where it stated none.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_rank(search: &SpielordersSearch, index: i64) -> Text<'_> {
    text_of(search, index, |file| &file.rank)
}

/// How many things the provider said about this query, in its own words.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_diagnostic_count(search: &SpielordersSearch) -> i64 {
    quietly(-1, || {
        i64::try_from(search.data.diagnostics.len()).unwrap_or(i64::MAX)
    })
}

/// Thing `index` the provider said about this query, in its own words, in the order it
/// was supplied; "" out of range.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_diagnostic(
    search: &SpielordersSearch,
    index: i64,
) -> Text<'_> {
    quietly(Text::new(EMPTY), || {
        usize::try_from(index)
            .ok()
            .and_then(|index| search.data.diagnostics.get(index))
            .map_or(Text::new(EMPTY), |said| Text::new(said))
    })
}

/// Frees `search`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_search_free(search: Option<Box<SpielordersSearch>>) {
    quietly((), || drop(search));
}

fn text_of<'a>(
    search: &'a SpielordersSearch,
    index: i64,
    text: impl FnOnce(&'a File) -> &'a CString,
) -> Text<'a> {
    quietly(Text::new(EMPTY), || {
        search
            .file(index)
            .map_or(Text::new(EMPTY), |file| Text::new(text(file)))
    })
}

fn path_of<'a>(
    search: &'a SpielordersSearch,
    index: i64,
    path: impl FnOnce(&'a File) -> &'a CString,
) -> PathBytes<'a> {
    quietly(PathBytes::new(EMPTY), || {
        search
            .file(index)
            .map_or(PathBytes::new(EMPTY), |file| PathBytes::new(path(file)))
    })
}
