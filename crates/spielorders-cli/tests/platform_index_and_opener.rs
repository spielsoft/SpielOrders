//! What this platform's own index provider keeps of what its query reported, and what its
//! opener is handed.
//!
//! **The provider and the opener this build ships for this platform**, so this file exists
//! only where that build does: the types below are compiled for one platform, and a build
//! for a platform with no index has no provider of its own for a suite to hold to anything.
//! What every build owes — that a provider's answer or the state that prevented one reaches
//! a caller — is `tests/supporting_material_search.rs`, which runs everywhere.
//!
//! **The platform query is a seam and the platform's opener is another**, because neither
//! can be exercised for real by an automated suite on the owner's machine: the index takes a
//! temporary directory late or never, so nothing here may depend on what it holds, and
//! launching an application is not something a test does. So the runs below prepare what the
//! query reported and record what would have been launched, and the two real handovers are
//! proven once, by hand, by the script Ticket 008 builds.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::support::{order_directory, orders_directory, populated_package};
use spielorders_cli::effects::{
    IndexRun, IndexState, Launched, PlatformIndex, PlatformOpener, Spotlight,
};
use spielorders_core::{
    canonical::{CanonicalPackageLoad, CurrentPackageCandidate, inspect_canonical_package},
    ports::{Cancellation, NativeFiles},
    supporting_material::{
        Completeness, PartialReason, SupportingMaterialQuery, SupportingMaterialSnapshot,
        UnavailableReason, search_supporting_material,
    },
};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

/// The bound every prepared query below carries. Nothing here is about it elapsing.
const BOUND: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// What the provider keeps of what the query reported.
// ---------------------------------------------------------------------------

/// The Package every prepared run below is answered about, and the paths it holds.
struct Prepared {
    _tree: crate::support::TemporaryTree,
    root: PathBuf,
    package: CurrentPackageCandidate,
    /// The paths a query scoped to the Orders Directory could report, healthy and not.
    reportable: Vec<PathBuf>,
    /// The Package-relative spelling of every path that is this Package's own Supporting
    /// Material, in no particular order.
    material: Vec<String>,
}

/// A Package holding one healthy Order, one Damaged Order, and a file of every kind a
/// query could report from inside its Orders Directory.
///
/// **Every excluded path is one that would otherwise have survived.** Each is a real file
/// inside the scope the one query is given, so an exclusion that was not applied would
/// report it — which is what makes the assertions below proofs rather than restatements.
/// The two nested files are the counter-case: their names are the managed ones, and they
/// are somebody else's files in somebody else's subdirectory.
fn prepared(label: &str) -> Prepared {
    let (tree, root, damaged) = crate::support::package_with_a_damaged_order(label);
    let healthy = order_directory(&root, "Laser_components");
    let orders = orders_directory(&root);
    let material = [
        "quote.pdf",
        "vendor-pack/README.md",
        "spectra/co\nherence.pdf",
    ];
    let nested_reserved = "vendor-pack/.spielorders/notes.json";
    let mut reportable = Vec::new();
    for relative in material.iter().chain([&nested_reserved]) {
        reportable.push(written(&healthy, relative));
    }
    reportable.extend([
        // The Order's own managed documents, which are not Supporting Material.
        healthy.join("README.md"),
        healthy.join(".spielorders/order.json"),
        // A file inside an Order whose record could not be read, so there is no Order for
        // it to be reported under.
        written(&damaged, "quote.pdf"),
        // A file loose in the Orders Directory, inside no Order Directory at all.
        written(&orders, "loose.pdf"),
        // A file outside the Orders Directory entirely.
        written(&root, "outside.pdf"),
    ]);
    let package = loaded(&root);
    let mut material: Vec<String> = material
        .iter()
        .chain([&nested_reserved])
        .map(|relative| {
            format!(
                "{}/{relative}",
                healthy
                    .strip_prefix(&root)
                    .expect("the Order Directory is inside the Package")
                    .display()
            )
        })
        .collect();
    material.sort();
    Prepared {
        _tree: tree,
        root,
        package,
        reportable,
        material,
    }
}

/// One file at a path relative to a directory, with every parent it needs.
fn written(directory: &Path, relative: &str) -> PathBuf {
    let path = directory.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the fixture file's parents are created");
    }
    std::fs::write(&path, b"").expect("the fixture file is written");
    path
}

/// The loaded Package a search answers about, read rather than opened: a search writes
/// nothing, and the opening load durably replaces the derived Package Index.
fn loaded(root: &Path) -> CurrentPackageCandidate {
    match inspect_canonical_package(root, &spielorders_cli::effects::PlatformFileSystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        other => panic!("the fixture Package loads as a current candidate: {other:?}"),
    }
}

/// What one prepared query reported, and how it ended.
enum Reported {
    /// The bytes the query wrote, and the reason it was stopped where it was.
    Wrote {
        output: Vec<u8>,
        stopped: Option<PartialReason>,
    },
    /// The query could not be run, in the platform's own words.
    Refused(String),
}

/// A platform index in a prepared state, reporting what a test prepared, and recording both
/// of the questions it was asked.
///
/// It records the asks because a fake that only returned what a test expected would prove
/// the plumbing and nothing else: how many runs there were and which directory each was
/// scoped to are the two facts the rule about one run is checked on, and whether a run
/// happened at all is what a state that prevents one is checked on.
struct PreparedQuery {
    state: IndexState,
    reported: Reported,
    state_asked: Mutex<Vec<PathBuf>>,
    asked: Mutex<Vec<(PathBuf, String, Duration)>>,
}

impl PreparedQuery {
    /// A query that reported these paths, terminated the way the real one terminates them.
    fn reporting(paths: &[PathBuf]) -> Self {
        let mut output = Vec::new();
        for path in paths {
            output.extend_from_slice(path.as_os_str().as_encoded_bytes());
            output.push(0);
        }
        Self::wrote(output, None)
    }

    fn wrote(output: Vec<u8>, stopped: Option<PartialReason>) -> Self {
        Self::prepared(IndexState::Enabled, Reported::Wrote { output, stopped })
    }

    fn refused(detail: &str) -> Self {
        Self::prepared(IndexState::Enabled, Reported::Refused(detail.to_owned()))
    }

    /// An index in `state`, which would report `paths` if it were ever asked for results.
    ///
    /// **Prepared to answer, because a state that prevents an answer has to be shown
    /// preventing one.** A fake that reported nothing would give an empty answer and an
    /// unavailable one the same visible result, and the defect this ticket repairs is exactly
    /// those two being confused.
    fn in_state(state: IndexState, paths: &[PathBuf]) -> Self {
        Self {
            state,
            ..Self::reporting(paths)
        }
    }

    fn prepared(state: IndexState, reported: Reported) -> Self {
        Self {
            state,
            reported,
            state_asked: Mutex::new(Vec::new()),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Every run this query was asked to make, in order.
    fn asked(&self) -> Vec<(PathBuf, String, Duration)> {
        self.asked
            .lock()
            .expect("the recorded asks are readable")
            .clone()
    }

    /// Every directory this index was asked the state of, in order.
    fn state_asked(&self) -> Vec<PathBuf> {
        self.state_asked
            .lock()
            .expect("the recorded state asks are readable")
            .clone()
    }
}

impl PlatformIndex for PreparedQuery {
    fn state(
        &self,
        directory: &Path,
        _bound: Duration,
        _cancellation: &dyn Cancellation,
    ) -> IndexState {
        self.state_asked
            .lock()
            .expect("the recorded state asks are writable")
            .push(directory.to_path_buf());
        self.state.clone()
    }

    fn run(
        &self,
        scope: &Path,
        text: &str,
        bound: Duration,
        _cancellation: &dyn Cancellation,
    ) -> IndexRun {
        self.asked
            .lock()
            .expect("the recorded asks are writable")
            .push((scope.to_path_buf(), text.to_owned(), bound));
        match &self.reported {
            Reported::Wrote { output, stopped } => IndexRun::Wrote {
                output: output.clone(),
                stopped: *stopped,
            },
            Reported::Refused(detail) => IndexRun::Refused(detail.clone()),
        }
    }
}

struct NotCancelled;

impl Cancellation for NotCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// One settled answer about `prepared`'s Package, from a query that reported `query`.
fn answered<'a>(
    prepared: &'a Prepared,
    query: &'a PreparedQuery,
) -> SupportingMaterialSnapshot<'a> {
    search_supporting_material(
        &prepared.package,
        &SupportingMaterialQuery::new("notes", BOUND),
        Some(&Spotlight::over(query)),
        &NotCancelled,
    )
}

/// The Package-relative spelling of every file a snapshot reports, flattened and sorted.
fn reported_material(snapshot: &SupportingMaterialSnapshot<'_>) -> Vec<String> {
    let SupportingMaterialSnapshot::Answered { orders, .. } = snapshot else {
        panic!("the prepared query looked: {snapshot:?}");
    };
    let mut reported: Vec<String> = orders
        .iter()
        .flat_map(|group| group.files())
        .map(|file| {
            file.location()
                .package_relative_path()
                .expect("a reported file is inside the Package")
                .to_owned()
        })
        .collect();
    reported.sort();
    reported
}

/// Only a file inside a healthy Order Directory that is not one of that Order's own managed
/// documents is a result.
///
/// **The claim is about what the shipped composition answers, because that is where the
/// exclusions are.** The one run is scoped to the Orders Directory, which is wider than the
/// Order Directories the request named, so the provider reports a file under a Damaged
/// Order, a file loose in the Orders Directory, a file outside it, and the Order's own
/// managed documents — every one of them a real file this query reported. Which of them
/// survives is `spielorders-core`'s to decide, and this is what a `search
/// supporting-material` run on this Package would report.
///
/// The two nested files are the counter-case: their names are the managed ones, and they are
/// somebody else's files in somebody else's subdirectory.
#[test]
fn only_supporting_material_inside_a_healthy_order_is_reported() {
    let prepared = prepared("supporting-material-kept");
    let query = PreparedQuery::reporting(&prepared.reportable);

    let snapshot = answered(&prepared, &query);

    assert_eq!(
        reported_material(&snapshot),
        prepared.material,
        "the excluded paths are all real files the query reported"
    );
}

/// A reported file carries its kind and when it was last written.
///
/// **The platform's query supplies neither.** `mdfind` prints paths, so a provider that
/// passed those paths straight through would report a file the desktop's file row cannot
/// describe — the row shows a kind and a modification date. Both come from the path the
/// query already handed over: the name's extension, and the filesystem's own stamp.
///
/// The kind is the extension and says so; it is not a content-type query, which would be a
/// second ask of the platform per result. A name with no extension carries no kind rather
/// than an invented one, which is why one of the two files here has none.
#[test]
fn a_reported_file_carries_its_kind_and_when_it_was_last_written() {
    let prepared = prepared("supporting-material-file-facts");
    let query = PreparedQuery::reporting(&prepared.reportable);

    let snapshot = answered(&prepared, &query);

    let SupportingMaterialSnapshot::Answered { orders, .. } = &snapshot else {
        panic!("the prepared query looked: {snapshot:?}");
    };
    let described: Vec<(Option<String>, bool)> = orders
        .iter()
        .flat_map(|group| group.files())
        .map(|file| {
            (
                file.kind().map(str::to_owned),
                file.modified_unix_seconds().is_some(),
            )
        })
        .collect();

    assert!(
        !described.is_empty(),
        "the fixture reports at least one file, or this asserts nothing"
    );
    assert!(
        described.iter().all(|(_, stamped)| *stamped),
        "every reported file was on disk when it was described: {described:?}"
    );
    assert!(
        described
            .iter()
            .any(|(kind, _)| kind.as_deref() == Some("pdf")),
        "a `.pdf` reports the kind its name gives it: {described:?}"
    );
    assert!(
        described.iter().all(|(kind, _)| kind
            .as_deref()
            .is_none_or(|kind| kind == kind.to_ascii_lowercase())),
        "a kind is the extension lowercased, never the name's own casing: {described:?}"
    );
}

/// The index is asked once, scoped to the directory that holds every Order Directory the
/// request named, and asked the caller's own words.
///
/// **The defect this catches is one run per Order.** A provider that asked once for each
/// scope would multiply a user's every keystroke by the number of Orders in their Package,
/// and the platform's query takes one directory to look in — so one run is scoped to the
/// directory above them all. Asked over a Package with more than one healthy Order, because
/// with one there is nothing for the scope to be shared between and a provider that asked
/// about the first Order Directory alone would pass.
#[test]
fn the_index_is_asked_once_over_the_directory_that_holds_every_scope() {
    let (_tree, root) = populated_package("supporting-material-one-run");
    let package = loaded(&root);
    let query = PreparedQuery::reporting(&[]);

    search_supporting_material(
        &package,
        &SupportingMaterialQuery::new("notes", BOUND),
        Some(&Spotlight::over(&query)),
        &NotCancelled,
    );

    let asked = query.asked();
    let [(scope, text, left)] = asked.as_slice() else {
        panic!("one run, and one only: {asked:?}");
    };
    assert_eq!(*scope, orders_directory(&root));
    assert_eq!(text, "notes", "carrying the caller's own words");
    assert!(
        *left <= BOUND && *left + Duration::from_secs(1) > BOUND,
        "carrying the caller's bound, less what learning the index state already spent: \
         {left:?}"
    );
}

/// The query reports no relevance, so every file carries none and the deterministic keys
/// decide the order.
///
/// **The defect this catches is an invented relevance.** The order a platform query happens
/// to print its results in is not a ranking, and a provider that passed the position off as
/// one would present the same Package differently from one run to the next. Reported here
/// in the order that is the reverse of the filename order, so a provider that kept the
/// query's own order fails.
#[test]
fn no_relevance_is_reported_and_the_deterministic_keys_decide() {
    let prepared = prepared("supporting-material-unranked");
    let healthy = order_directory(&prepared.root, "Laser_components");
    let query = PreparedQuery::reporting(&[
        written(&healthy, "zebra.pdf"),
        written(&healthy, "aardvark.pdf"),
    ]);

    let snapshot = answered(&prepared, &query);

    let SupportingMaterialSnapshot::Answered { orders, .. } = &snapshot else {
        panic!("the prepared query looked: {snapshot:?}");
    };
    let files: Vec<&spielorders_core::supporting_material::MatchedFile> =
        orders.iter().flat_map(|group| group.files()).collect();
    assert_eq!(
        files.iter().map(|file| file.filename()).collect::<Vec<_>>(),
        ["aardvark.pdf", "zebra.pdf"],
        "the filename decides, because the query offered nothing to rank by"
    );
    assert!(
        files.iter().all(|file| file.rank().is_none()),
        "a relevance the query never reported is absent rather than invented"
    );
    assert!(
        files
            .iter()
            .all(|file| file.kind() == Some("pdf") && file.modified_unix_seconds().is_some()),
        "a kind and a stamp come from the path, which is the one thing the query did report"
    );
}

/// A query that was stopped is what it had written by then, marked incomplete.
///
/// **The defect this catches is a truncated path reported as a file.** A run stopped part
/// way through a write leaves a path with no terminator after it, and reporting that
/// fragment would name a file that does not exist. The terminator is what says a path is
/// whole, which is also why a filename holding a newline survives the reading at all.
#[test]
fn a_stopped_query_reports_the_whole_paths_it_wrote_and_says_it_is_incomplete() {
    let prepared = prepared("supporting-material-stopped");
    let healthy = order_directory(&prepared.root, "Laser_components");
    let whole = written(&healthy, "co\nherence.pdf");
    let mut output = whole.as_os_str().as_encoded_bytes().to_vec();
    output.push(0);
    output.extend_from_slice(healthy.join("never-finish").as_os_str().as_encoded_bytes());
    let query = PreparedQuery::wrote(output, Some(PartialReason::TimedOut));

    let snapshot = answered(&prepared, &query);

    let SupportingMaterialSnapshot::Answered {
        completeness,
        orders,
        ..
    } = &snapshot
    else {
        panic!("a stopped query still looked: {snapshot:?}");
    };
    assert_eq!(
        *completeness,
        Completeness::Partial(PartialReason::TimedOut)
    );
    assert_eq!(
        reported_material(&snapshot),
        vec![
            whole
                .strip_prefix(&prepared.root)
                .expect("the fixture file is inside the Package")
                .display()
                .to_string()
        ],
        "the terminated path is a result and the unfinished one is not: {orders:?}"
    );
}

/// A query the platform refused is the state that prevented an answer, in its own words.
///
/// **The defect this catches is a refusal reported as an empty answer.** "The index found no
/// matching file" and "the index could not be asked" are the two sentences a caller most
/// needs to tell apart, and a refused run is the second one.
#[test]
fn a_refused_query_is_reported_as_the_state_that_prevented_an_answer() {
    let prepared = prepared("supporting-material-refused");
    let query = PreparedQuery::refused("mdfind: unknown option");

    let snapshot = answered(&prepared, &query);

    let SupportingMaterialSnapshot::Unavailable {
        provider,
        reason,
        diagnostics,
    } = &snapshot
    else {
        panic!("a refused query looked at nothing: {snapshot:?}");
    };
    assert_eq!(*provider, Some("spotlight"));
    assert_eq!(*reason, UnavailableReason::QueryRefused);
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.detail())
            .collect::<Vec<_>>(),
        [Some("mdfind: unknown option")],
        "the platform's own words travel beside the state"
    );
}

// ---------------------------------------------------------------------------
// What the platform said prevented an answer.
// ---------------------------------------------------------------------------

/// No run at all, which is what a state that prevented an answer leaves behind.
fn no_runs() -> Vec<(PathBuf, String, Duration)> {
    Vec::new()
}

/// The state and the platform's own words one prepared index settled as.
fn state_reported(snapshot: &SupportingMaterialSnapshot<'_>) -> (UnavailableReason, Vec<String>) {
    let SupportingMaterialSnapshot::Unavailable {
        provider,
        reason,
        diagnostics,
    } = snapshot
    else {
        panic!("an index that cannot answer looked at nothing: {snapshot:?}");
    };
    assert_eq!(
        *provider,
        Some("spotlight"),
        "the provider that could not look is still named"
    );
    (
        *reason,
        diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.detail())
            .map(str::to_owned)
            .collect(),
    )
}

/// An index the platform reports as switched off says so, and its query is never run.
///
/// **The defect this catches is the one this ticket exists for.** The platform's query prints
/// nothing and succeeds while indexing is off, so a provider that only ran it reports a search
/// that completed and matched nothing — a sentence that is false about the world rather than
/// incomplete about it. The prepared index here would report this Package's real Supporting
/// Material if it were ever asked for results, so a build with the detection removed answers
/// with file groups rather than with an empty result, and this fails either way it goes wrong.
#[test]
fn a_switched_off_index_is_reported_rather_than_answered_as_an_empty_result() {
    let prepared = prepared("supporting-material-indexing-disabled");
    let query = PreparedQuery::in_state(IndexState::Disabled, &prepared.reportable);

    let snapshot = answered(&prepared, &query);

    assert_eq!(
        state_reported(&snapshot),
        (UnavailableReason::IndexingDisabled, Vec::new()),
        "the platform named the state itself, so there is nothing of its own to carry"
    );
    assert_eq!(
        query.state_asked(),
        vec![orders_directory(&prepared.root)],
        "the state is asked about the directory the query would have been scoped to"
    );
    assert_eq!(
        query.asked(),
        no_runs(),
        "an index that cannot answer is not asked for results"
    );
}

/// A volume with no index and a volume the platform will not report on are one answer.
///
/// **The defect this catches is a distinction invented at the seam.** The two are different
/// things for the platform to have said and the same thing for a caller to be told: there is
/// one correction — move the Package to an indexed volume — and nothing a caller could do
/// differently on learning which of the two it was. What the platform said travels beside the
/// state where it said anything, which is the only part of the difference that survives.
#[test]
fn an_unindexed_volume_and_one_the_platform_will_not_report_on_answer_the_same_way() {
    let prepared = prepared("supporting-material-volume-not-indexed");
    let unknown = "Error: unknown indexing state.";

    let mut answers = Vec::new();
    for state in [
        IndexState::NotIndexed,
        IndexState::Unreported(unknown.to_owned()),
    ] {
        let query = PreparedQuery::in_state(state, &prepared.reportable);
        let snapshot = answered(&prepared, &query);
        assert_eq!(
            query.asked(),
            no_runs(),
            "a volume with nothing to answer from is not asked for results"
        );
        answers.push(state_reported(&snapshot));
    }

    assert_eq!(
        answers,
        vec![
            (UnavailableReason::VolumeNotIndexed, Vec::new()),
            (
                UnavailableReason::VolumeNotIndexed,
                vec![unknown.to_owned()]
            ),
        ],
        "one reason for both, carrying the platform's own words where it gave any"
    );
}

/// A directory this process cannot read, for as long as the value is held.
///
/// Restored on the way out, including while a failed assertion unwinds, so the temporary tree
/// can still be removed afterwards.
struct Unreadable(PathBuf, std::fs::Permissions);

impl Unreadable {
    /// **The refusal is established rather than assumed.** A run as a user these permissions
    /// do not refuse — root — would exercise the readable path and certify nothing, so this
    /// fails there instead of passing quietly.
    fn of(directory: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let held = std::fs::metadata(directory)
            .expect("the fixture directory is there")
            .permissions();
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o000))
            .expect("the fixture directory's permissions are writable");
        let held = Self(directory.to_path_buf(), held);
        assert!(
            std::fs::read_dir(directory).is_err(),
            "this process really cannot read {}: a run these permissions do not refuse \
             proves nothing about a refusal",
            directory.display()
        );
        held
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, self.1.clone());
    }
}

/// An Orders Directory this process cannot read is denied access, and the index is asked
/// nothing at all to establish it.
///
/// **The defect this catches is asking the index whether a directory is readable.** An index
/// is not the authority on that and answers for itself: it would report the files it holds
/// under a directory this process is refused, and a caller would be handed results it cannot
/// open. The filesystem is what knows, and the assertions that nothing was asked of the index
/// — neither its state nor a run — are what say so.
///
/// **What this does not prove is a `search supporting-material` run.** That command loads the
/// Package before it searches and the load lists this same directory, so the command answers
/// that the Package could not be read and never reaches the provider. The caller this holds
/// for is one searching a Package it loaded earlier, which is the ordering below: the Package
/// is loaded, and the directory is refused afterwards.
#[test]
fn an_orders_directory_this_process_cannot_read_is_denied_access_without_asking_the_index() {
    let prepared = prepared("supporting-material-unreadable");
    let orders = orders_directory(&prepared.root);
    let query = PreparedQuery::reporting(&prepared.reportable);
    let _readable_again = Unreadable::of(&orders);

    let snapshot = answered(&prepared, &query);

    let (reason, said) = state_reported(&snapshot);
    assert_eq!(reason, UnavailableReason::AccessDenied);
    assert!(
        said.iter()
            .any(|said| said.contains(&orders.display().to_string())),
        "the directory that was refused is named: {said:?}"
    );
    assert_eq!(
        query.state_asked(),
        Vec::<PathBuf>::new(),
        "the filesystem established this, so the index was not asked what state it is in"
    );
    assert_eq!(
        query.asked(),
        no_runs(),
        "and it was not asked for results either"
    );
}

// ---------------------------------------------------------------------------
// Opening a result and revealing it.
// ---------------------------------------------------------------------------

/// A platform opener that records what would have been handed to it, and launches nothing.
struct RecordedLaunches(Mutex<Vec<Vec<OsString>>>);

impl RecordedLaunches {
    fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    fn launched(&self) -> Vec<Vec<OsString>> {
        self.0
            .lock()
            .expect("the recorded launches are readable")
            .clone()
    }
}

impl Launched for RecordedLaunches {
    fn launch(&self, arguments: &[&OsStr]) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("the recorded launches are writable")
            .push(
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect(),
            );
        Ok(())
    }
}

/// Opening a result hands the platform that exact file, and revealing it hands over the
/// file selected where it stands.
///
/// **The defect this catches is revealing the directory instead of the file.** A result is
/// one file among however many an Order Directory holds, so an opener handed the parent
/// would leave a user to find it again themselves. The two differ by the reveal flag and by
/// nothing else, because which application opens a file is the user's setting.
#[test]
fn opening_and_revealing_hand_the_platform_the_exact_file() {
    let (_tree, root) = populated_package("supporting-material-open");
    let file = written(&order_directory(&root, "Laser_components"), "quote.pdf");
    let launches = RecordedLaunches::new();
    let opener = PlatformOpener::over(&launches);

    opener.open(&file).expect("the file is handed over");
    opener.reveal(&file).expect("the file is handed over");

    assert_eq!(
        launches.launched(),
        vec![
            vec![file.clone().into_os_string()],
            vec![OsString::from("-R"), file.into_os_string()],
        ]
    );
}

/// A file that has moved or gone since the index reported it is reported unavailable rather
/// than handed over.
///
/// **The defect this catches is acting on a path that no longer stands.** A result names the
/// file the index held when it answered, and a user may have moved or deleted it since;
/// handing that path to the platform anyway asks it to open something that is not there, and
/// its complaint about a launch is not the answer a caller can act on.
#[test]
fn a_file_that_has_gone_is_reported_unavailable_rather_than_handed_over() {
    let (_tree, root) = populated_package("supporting-material-vanished");
    let file = written(&order_directory(&root, "Laser_components"), "quote.pdf");
    std::fs::remove_file(&file).expect("the result is removed before it is acted on");
    let launches = RecordedLaunches::new();
    let opener = PlatformOpener::over(&launches);

    for outcome in [opener.open(&file), opener.reveal(&file)] {
        assert_eq!(
            outcome
                .expect_err("a file that is gone cannot be acted on")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    assert_eq!(
        launches.launched(),
        Vec::<Vec<OsString>>::new(),
        "nothing was launched, which is what `reported rather than acted on` means"
    );
}

/// The shipped opener really hands a file to the platform, watched by hand.
///
/// **Ignored, because it launches applications on the owner's machine.** Every other claim
/// about the opener is asserted against a recording double, which cannot show that the
/// platform accepts what it is handed — and this is the one that can, so it runs only when
/// somebody has asked for it and is watching. `scripts/smoke-spotlight.sh` runs it by name
/// at the step where it says a viewer and a Finder window should come forward.
///
/// The file is written outside any temporary tree and left behind, named on standard error,
/// because an application opens a file after this returns: a path deleted when the test ends
/// would have the platform show an error over a line claiming success.
#[test]
#[ignore = "launches applications; scripts/smoke-spotlight.sh runs it when someone is watching"]
fn the_shipped_opener_really_opens_and_reveals() {
    let file =
        std::env::temp_dir().join(format!("spielorders-handover-{}.txt", std::process::id()));
    std::fs::write(
        &file,
        b"SpielOrders handed this file to the platform's opener.\n",
    )
    .expect("a file outside any temporary tree is writable");
    eprintln!("handing over {}", file.display());

    spielorders_cli::effects::native_files()
        .open(&file)
        .expect("the platform accepted the file");
    spielorders_cli::effects::native_files()
        .reveal(&file)
        .expect("the platform accepted the reveal");

    eprintln!("left behind: {}", file.display());
}

/// The opener this build ships reports a file that has gone, and never reaches the platform.
///
/// **The one claim about the shipped opener an automated suite can make.** Handing a file
/// that is there to the real `open` would launch an application on the owner's machine, and
/// the by-hand proof script is where that happens; handing it one that is not there stops at
/// the same guard the recorded fake above proves, so this run reaches the composed opener
/// without the platform ever being asked. The defect it catches is a guard that only the
/// fake had.
#[test]
fn the_shipped_opener_reports_a_file_that_has_gone_without_asking_the_platform() {
    let (_tree, root) = populated_package("supporting-material-shipped-opener");
    let gone = order_directory(&root, "Laser_components").join("never-written.pdf");

    for outcome in [
        spielorders_cli::effects::native_files().open(&gone),
        spielorders_cli::effects::native_files().reveal(&gone),
    ] {
        assert_eq!(
            outcome
                .expect_err("a file that was never there cannot be acted on")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
}
