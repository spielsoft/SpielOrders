//! What a front end's platform index found under a loaded Package's Order Directories,
//! supplied as its hits.
//!
//! Decision sources: Define lookup and search behavior; Define the Rust CLI capability
//! contract.

use crate::ports::FileSystem;
use crate::supporting_material::ProviderDiagnosticCode;
use crate::tests::support::{
    PlatformFileSystem, ScriptedFileSystem, TemporaryTree, effects, opened,
};
use crate::{Answer, Failure, answer::ProviderUnavailable};
use crate::{Change, Report};
use crate::{
    Package, ProviderReport, SupportingMaterialSnapshot, ports::Cancellation,
    supporting_material::Completeness, supporting_material::PartialReason,
    supporting_material::ProviderAnswer, supporting_material::ProviderDiagnostic,
    supporting_material::ProviderHit, supporting_material::ProviderUnavailableReason,
};
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// The configured Orders Directory of every fixture below.
///
/// Deliberately not the conventional name, so a scope taken from a literal rather than
/// from the Package's own configuration would find nothing here.
const ORDERS_DIRECTORY: &str = "Bestellungen";

/// The configured Removed Orders Directory of every fixture below, for the same reason.
const REMOVED_DIRECTORY: &str = "Entfernte";

/// A committed Package whose two managed directories are configured rather than
/// conventional.
fn package(label: &str) -> (TemporaryTree, PathBuf) {
    let tree = TemporaryTree::new(label);
    let root = package_on(&tree, &PlatformFileSystem);
    (tree, root)
}

/// A committed Package on `filesystem` at the root of `tree`, configured as [`package`] is.
fn package_on<F: FileSystem>(tree: &TemporaryTree, filesystem: &F) -> PathBuf {
    let plan = crate::plan(
        &Change::InitializePackage {
            root: tree.path().into(),
            orders_path: Some(String::from(ORDERS_DIRECTORY)),
            removed_path: Some(String::from(REMOVED_DIRECTORY)),
        },
        &effects(filesystem),
    )
    .result
    .expect("the test Package can be planned");
    assert!(matches!(
        crate::commit(plan, &effects(filesystem)).result,
        Ok(Report::Committed { .. })
    ));
    std::fs::canonicalize(tree.path()).expect("the test Package root resolves")
}

/// One Order Directory carrying the record given, and its conventional README.
fn order(root: &Path, basename: &str, record: &str) -> PathBuf {
    let directory = root.join(ORDERS_DIRECTORY).join(basename);
    std::fs::create_dir_all(directory.join(".spielorders")).expect("the Order Directory");
    std::fs::write(directory.join(".spielorders/order.json"), record).expect("the Order record");
    std::fs::write(directory.join("README.md"), "").expect("the Order README");
    directory
}

/// One Order Directory on `volume` carrying the record given, its conventional README,
/// and each Supporting Material file named.
fn order_on(
    volume: &ScriptedFileSystem,
    root: &Path,
    basename: &str,
    record: &str,
    files: &[&str],
) -> PathBuf {
    let directory = root.join(ORDERS_DIRECTORY).join(basename);
    volume.put_file(directory.join(".spielorders/order.json"), record.as_bytes());
    volume.put_file(directory.join("README.md"), b"");
    for file in files {
        volume.put_file(directory.join(file), b"");
    }
    directory
}

/// What the core makes of `report` about the Package at `root`, both read through `volume`.
fn searched_on(
    volume: &ScriptedFileSystem,
    root: &Path,
    report: &ProviderReport,
) -> Answer<SupportingMaterialSnapshot> {
    let package = loaded_on(root, volume);
    crate::search_supporting_material(&package, Some(report), &effects(volume), &NotCancelled)
}

/// The Package's Vendor record, replacing whatever initialization wrote.
fn vendors(root: &Path, record: &str) {
    std::fs::write(root.join(".spielorders/vendors.json"), record).expect("the Vendor record");
}

/// One Supporting Material file at a path relative to an Order Directory.
fn supporting(directory: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = directory.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the Supporting Material directory");
    }
    std::fs::write(&path, contents).expect("the Supporting Material file");
    path
}

/// The opened Package a search answers about.
fn loaded(root: &Path) -> Package {
    loaded_on(root, &PlatformFileSystem)
}

/// The opened Package a search answers about, read through `filesystem`.
fn loaded_on<F: FileSystem>(root: &Path, filesystem: &F) -> Package {
    match opened(root, filesystem).result {
        Ok(package) => package,
        other => panic!("the fixture Package loads as a current candidate: {other:?}"),
    }
}

/// What the core makes of `report` about `package`.
fn search_supporting_material(
    package: &Package,
    report: Option<&ProviderReport>,
    cancellation: &dyn Cancellation,
) -> Answer<SupportingMaterialSnapshot> {
    crate::search_supporting_material(package, report, &effects(&PlatformFileSystem), cancellation)
}

/// A provider's answer, under the provider's name.
fn reported(answer: ProviderAnswer) -> ProviderReport {
    ProviderReport {
        provider: "fake".to_owned(),
        answer,
    }
}

/// A provider that looked at everything in scope and found these files.
fn found(hits: Vec<ProviderHit>) -> ProviderReport {
    reported(ProviderAnswer::Looked {
        completeness: Completeness::Complete,
        hits,
        diagnostics: Vec::new(),
    })
}

/// A caller who has not cancelled.
struct NotCancelled;

impl Cancellation for NotCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// A caller who withdrew the question.
struct AlreadyCancelled;

impl Cancellation for AlreadyCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

/// One hit carrying nothing but its path, which is what a provider always has.
fn hit(path: &Path) -> ProviderHit {
    ProviderHit {
        path: path.to_path_buf(),
        kind: None,
        modified_unix_seconds: None,
        rank: None,
    }
}

/// One hit the provider offered a relevance for.
fn ranked(path: &Path, rank: f64) -> ProviderHit {
    ProviderHit {
        rank: Some(rank),
        ..hit(path)
    }
}

/// The diagnostic codes a snapshot carries, in the order it carries them.
fn diagnostic_codes(answer: &Answer<SupportingMaterialSnapshot>) -> Vec<ProviderDiagnosticCode> {
    looked(answer)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// Every file a snapshot reports, flattened, in the order it carries them.
fn reported_paths(answer: &Answer<SupportingMaterialSnapshot>) -> Vec<String> {
    grouped_paths(answer).concat()
}

/// The Order Titles a snapshot groups files under, in the order it carries them.
fn grouped_titles(answer: &Answer<SupportingMaterialSnapshot>) -> Vec<String> {
    looked(answer)
        .orders
        .iter()
        .map(|group| group.order.document.title.deref().to_owned())
        .collect()
}

/// The Package-relative spelling of every file a snapshot reports, group by group.
fn grouped_paths(answer: &Answer<SupportingMaterialSnapshot>) -> Vec<Vec<String>> {
    looked(answer)
        .orders
        .iter()
        .map(|group| {
            group
                .files
                .iter()
                .map(|file| {
                    file.location
                        .package_relative_path
                        .as_ref()
                        .expect("a reported file is inside the Package")
                        .to_owned()
                })
                .collect()
        })
        .collect()
}

/// What the provider found, when it looked.
fn looked(answer: &Answer<SupportingMaterialSnapshot>) -> &SupportingMaterialSnapshot {
    match &answer.result {
        Ok(snapshot) => snapshot,
        Err(failure) => panic!("a provider looked: {failure:?}"),
    }
}

const TARNWICK: &str = r#"{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "contacts": []
    }
  ]
}
"#;

const ALPHA: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d",
  "title": "Alpha Halterung",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
  "items": []
}
"#;

const BETA: &str = r#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e",
  "title": "Beta Spiegel",
  "date_ordered": "2026-07-19",
  "status": "hold",
  "items": []
}
"#;

/// Files group under the Order whose Directory holds them, in the Package's own Order
/// order, and each carries both spellings of its path.
///
/// The defect this catches is a result list that is the provider's list: the provider is
/// asked once for the whole Package, so an answer that reported its hits flat, or in the
/// order the index happened to return them, would leave a caller unable to say which Order
/// a file belongs to — which is the whole navigation contract for these results. The hits
/// are handed over in the opposite order to the one the Package holds its Orders in, so a
/// grouping that preserved the provider's order fails here.
#[test]
fn files_group_under_the_orders_whose_directories_hold_them() {
    let (_tree, root) = package("supporting-material-grouping");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let beta = order(&root, "Beta-Order", BETA);
    let coherence = supporting(&alpha, "spectra/coherence.pdf", "alpha");
    let invoice = supporting(&beta, "invoice.pdf", "beta");
    let package = loaded(&root);

    let index = found(vec![hit(&invoice), hit(&coherence)]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        grouped_titles(&snapshot),
        ["Alpha Halterung", "Beta Spiegel"],
        "the groups are the Package's Orders, in the Package's own order"
    );
    assert_eq!(
        grouped_paths(&snapshot),
        [
            vec!["Bestellungen/Alpha-Order/spectra/coherence.pdf".to_owned()],
            vec!["Bestellungen/Beta-Order/invoice.pdf".to_owned()],
        ],
        "each file is under the Order Directory that holds it"
    );

    let SupportingMaterialSnapshot {
        provider,
        completeness,
        orders,
        ..
    } = looked(&snapshot);
    assert_eq!(
        *provider, "fake",
        "the answer names the provider that gave it"
    );
    assert_eq!(*completeness, Completeness::Complete);
    assert_eq!(orders[0].order.document.title.deref(), "Alpha Halterung");
    let file = &orders[0].files[0];
    assert_eq!(
        file.location.absolute_path, coherence,
        "and its absolute path is the one the provider reported"
    );
    assert_eq!(file.filename, "coherence.pdf");

    assert_eq!(
        package.supporting_material_directory(),
        Some(root.join(ORDERS_DIRECTORY).as_path()),
        "the one directory a provider searches is the configured Orders Directory"
    );
}

/// One path reported twice is one result, and two paths holding identical bytes are two.
///
/// The defect this catches is deduplication by anything but the path. Both files below hold
/// the same bytes under the same filename, so an implementation that compared contents or
/// filenames would report one of them and silently lose the other — and losing one is worse
/// than a duplicate, because the file that disappeared is still on disk. The third hit is
/// the first path written with a redundant `.` component, which is the same file however a
/// provider spells it.
#[test]
fn one_path_reported_twice_is_one_result_and_identical_files_are_two() {
    let (_tree, root) = package("supporting-material-deduplication");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let twin = supporting(&alpha, "twin.pdf", "identical bytes");
    let nested_twin = supporting(&alpha, "nested/twin.pdf", "identical bytes");
    let package = loaded(&root);

    let index = found(vec![
        hit(&twin),
        hit(&nested_twin),
        hit(&twin),
        hit(&alpha.join("./twin.pdf")),
    ]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        reported_paths(&snapshot),
        [
            "Bestellungen/Alpha-Order/nested/twin.pdf",
            "Bestellungen/Alpha-Order/twin.pdf",
        ],
        "the same path appears once, and two paths are two Supporting Material files"
    );
}

/// An Order's own conventional README and reserved subtree are not Supporting Material, and
/// a nested file that merely has a familiar name is.
///
/// The defect this catches is an exclusion written as a filename: managed Markdown and JSON
/// reappearing as file results is what the exclusion is for, and a rule spelled `README.md`
/// would also swallow a vendor's own README inside a downloaded pack — a file the user put
/// there, which SpielOrders does not manage and must not hide.
#[test]
fn an_orders_managed_documents_are_excluded_and_a_familiar_nested_name_is_not() {
    let (_tree, root) = package("supporting-material-exclusions");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let quote = supporting(&alpha, "quote.pdf", "");
    let nested_readme = supporting(&alpha, "vendor-pack/README.md", "the vendor's own");
    let nested_reserved = supporting(&alpha, "vendor-pack/.spielorders/notes.json", "not ours");
    let package = loaded(&root);

    let index = found(vec![
        hit(&alpha.join("README.md")),
        hit(&alpha.join(".spielorders/order.json")),
        hit(&quote),
        hit(&nested_readme),
        hit(&nested_reserved),
    ]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        reported_paths(&snapshot),
        [
            "Bestellungen/Alpha-Order/vendor-pack/README.md",
            "Bestellungen/Alpha-Order/vendor-pack/.spielorders/notes.json",
            "Bestellungen/Alpha-Order/quote.pdf",
        ],
        "the Order's own README and record are gone, and the nested files are results"
    );
}

/// A hit is judged by the entry the volume says its path names: spellings it makes one
/// entry are one file, matched to its Order and excluded where it is a managed document,
/// and a path naming nothing is matched only by its exact spelling.
///
/// The defect this catches is a comparison of the provider's bytes with the Package's. A
/// provider spells a path its own way — Spotlight decomposes Unicode — and on a volume that
/// resolves names without regard to case, every hit below is the Order's file or managed
/// document whatever its case. A path naming nothing has no entry to ask about, so it keeps
/// the byte comparison every hit had before.
#[test]
fn spellings_the_volume_makes_one_entry_are_one_file() {
    let tree = TemporaryTree::new("supporting-material-one-entry");
    let volume = ScriptedFileSystem::case_insensitive_over(tree.path());
    let root = package_on(&tree, &volume);
    volume.put_file(root.join(".spielorders/vendors.json"), TARNWICK.as_bytes());
    let alpha = order_on(
        &volume,
        &root,
        "Alpha-Order",
        ALPHA,
        &["quote.pdf", "notes.txt"],
    );
    let shouted = root.join("BESTELLUNGEN/ALPHA-ORDER");

    let index = found(vec![
        hit(&shouted.join("QUOTE.PDF")),
        hit(&alpha.join("quote.pdf")),
        hit(&shouted.join("NOTES.TXT")),
        hit(&alpha.join("readme.md")),
        hit(&shouted.join(".SPIELORDERS/order.json")),
        hit(&shouted.join("missing.pdf")),
        hit(&alpha.join("missing.pdf")),
    ]);
    let snapshot = searched_on(&volume, &root, &index);

    assert_eq!(
        grouped_paths(&snapshot),
        [[
            "Bestellungen/Alpha-Order/missing.pdf",
            "Bestellungen/Alpha-Order/notes.txt",
            "Bestellungen/Alpha-Order/quote.pdf",
        ]],
        "each file once in the Package's spelling, no managed document, and the absent path \
         only where it is spelled the Package's way"
    );
    assert_eq!(
        looked(&snapshot).orders[0].files[2].location.absolute_path,
        alpha.join("quote.pdf")
    );
}

/// Spellings the volume keeps apart are two entries, however alike they look.
///
/// The defect this catches is identity decided here rather than by the volume. A rule that
/// folded Unicode normalization or case itself would merge the composed and decomposed
/// `Café` Orders, or `quote.pdf` with `Quote.pdf`, on a volume where each is its own entry
/// — reporting one Order's file under the other, losing a file as a duplicate, or hiding a
/// user's own `readme.md` as the Order's README.
#[test]
fn spellings_the_volume_keeps_apart_are_never_one_entry() {
    let tree = TemporaryTree::new("supporting-material-distinct");
    let volume = ScriptedFileSystem::byte_distinct_over(tree.path());
    let root = package_on(&tree, &volume);
    volume.put_file(root.join(".spielorders/vendors.json"), TARNWICK.as_bytes());
    let files = ["quote.pdf", "Quote.pdf", "readme.md"];
    let composed = order_on(&volume, &root, "Caf\u{e9}", ALPHA, &files);
    let decomposed = order_on(&volume, &root, "Cafe\u{301}", BETA, &["quote.pdf"]);

    let index = found(vec![
        hit(&decomposed.join("quote.pdf")),
        hit(&composed.join("quote.pdf")),
        hit(&composed.join("Quote.pdf")),
        hit(&composed.join("readme.md")),
        hit(&composed.join("README.md")),
    ]);
    let snapshot = searched_on(&volume, &root, &index);

    assert_eq!(
        grouped_paths(&snapshot),
        [
            vec![
                "Bestellungen/Caf\u{e9}/Quote.pdf",
                "Bestellungen/Caf\u{e9}/quote.pdf",
                "Bestellungen/Caf\u{e9}/readme.md",
            ],
            vec!["Bestellungen/Cafe\u{301}/quote.pdf"],
        ],
        "each entry under its own Order, and only the Order's own README excluded"
    );
}

/// Files sort by the provider's relevance descending, then by filename, then by path, and a
/// file the provider ranked at all comes before one it did not.
///
/// The defect this catches is an answer in the provider's own order: an index is free to
/// return its hits however it likes, so a group that preserved that order would present the
/// same Package differently from one run to the next. The two files ranked equally carry the
/// same filename, so they can only be separated by the path — which is the last key and the
/// one that makes the order total.
#[test]
fn files_sort_by_relevance_and_then_by_the_deterministic_keys() {
    let (_tree, root) = package("supporting-material-ordering");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let gamma = supporting(&alpha, "gamma.pdf", "");
    let first = supporting(&alpha, "alpha.pdf", "");
    let beta = supporting(&alpha, "beta.pdf", "");
    let nested_beta = supporting(&alpha, "nested/beta.pdf", "");
    let unranked = supporting(&alpha, "aaa-unranked.pdf", "");
    let package = loaded(&root);

    let index = found(vec![
        hit(&unranked),
        ranked(&first, 0.2),
        ranked(&nested_beta, 0.5),
        ranked(&gamma, 0.9),
        ranked(&beta, 0.5),
    ]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        reported_paths(&snapshot),
        [
            "Bestellungen/Alpha-Order/gamma.pdf",
            "Bestellungen/Alpha-Order/beta.pdf",
            "Bestellungen/Alpha-Order/nested/beta.pdf",
            "Bestellungen/Alpha-Order/alpha.pdf",
            "Bestellungen/Alpha-Order/aaa-unranked.pdf",
        ],
        "relevance first, then filename, then path, and no relevance last"
    );
}

/// A timed-out answer is the results gathered so far, marked incomplete, with a diagnostic
/// saying why — and it is an answer rather than a failure.
///
/// The defect this catches is a bound that discards what it has: a caller who asked and
/// waited is better served by the files the index did reach, and the mark is what stops them
/// reading a partial list as the whole one. The provider's own words travel beside the
/// reason rather than instead of it.
#[test]
fn a_timed_out_answer_keeps_what_was_gathered_and_says_it_is_partial() {
    let (_tree, root) = package("supporting-material-timeout");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let quote = supporting(&alpha, "quote.pdf", "");
    let package = loaded(&root);

    let index = reported(ProviderAnswer::Looked {
        completeness: Completeness::Partial(PartialReason::TimedOut),
        hits: vec![hit(&quote)],
        diagnostics: vec![ProviderDiagnostic::reported("the index stopped early")],
    });
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        reported_paths(&snapshot),
        ["Bestellungen/Alpha-Order/quote.pdf"],
        "what was gathered is the answer"
    );
    let SupportingMaterialSnapshot { completeness, .. } = looked(&snapshot);
    assert_eq!(
        *completeness,
        Completeness::Partial(PartialReason::TimedOut)
    );
    assert_eq!(
        diagnostic_codes(&snapshot),
        [
            ProviderDiagnosticCode::TimedOut,
            ProviderDiagnosticCode::ProviderReported
        ],
        "the reason it is partial leads, and the provider's own words follow"
    );
    assert_eq!(
        looked(&snapshot).diagnostics[1].detail.as_deref(),
        Some("the index stopped early")
    );
}

/// A cancelled request never reports completion, whatever the provider claims.
///
/// The defect this catches is a provider that did not notice: the caller withdrew the
/// question, and an answer marked complete would tell them the index had finished looking
/// when it had not. What was gathered is still the answer, at the same standing as a
/// timeout — neither is a failure.
#[test]
fn a_cancelled_request_is_never_reported_as_complete() {
    let (_tree, root) = package("supporting-material-cancellation");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let quote = supporting(&alpha, "quote.pdf", "");
    let package = loaded(&root);

    let index = found(vec![hit(&quote)]);
    let snapshot = search_supporting_material(&package, Some(&index), &AlreadyCancelled);

    let SupportingMaterialSnapshot { completeness, .. } = looked(&snapshot);
    assert_eq!(
        *completeness,
        Completeness::Partial(PartialReason::Cancelled),
        "the provider said it had looked at everything, and the caller had stopped asking"
    );
    assert_eq!(
        diagnostic_codes(&snapshot),
        [ProviderDiagnosticCode::Cancelled]
    );
    assert_eq!(
        reported_paths(&snapshot),
        ["Bestellungen/Alpha-Order/quote.pdf"],
        "and what was gathered before the cancellation is still the answer"
    );
}

/// A Package with no healthy Order has no directory for a provider to search.
#[test]
fn a_package_with_no_healthy_order_has_nowhere_to_search() {
    let (_tree, root) = package("supporting-material-nowhere");
    order(&root, "Damaged-Order", DAMAGED);
    assert_eq!(loaded(&root).supporting_material_directory(), None);
}

/// An index that found nothing is a different answer from an index that could not be asked.
///
/// The defect this catches is the two collapsing into one empty list. "The index found no
/// matching file" and "nothing looked for one" send a user to two different places, and a
/// caller that could not tell them apart would report the first when it meant the second.
#[test]
fn an_empty_answer_is_not_the_same_as_nothing_having_looked() {
    let (_tree, root) = package("supporting-material-empty");
    vendors(&root, TARNWICK);
    order(&root, "Alpha-Order", ALPHA);
    let package = loaded(&root);

    let index = found(Vec::new());
    let found_nothing = search_supporting_material(&package, Some(&index), &NotCancelled);
    let SupportingMaterialSnapshot {
        completeness,
        orders,
        ..
    } = looked(&found_nothing);
    assert!(orders.is_empty());
    assert_eq!(*completeness, Completeness::Complete);
    assert_eq!(looked(&found_nothing).provider, "fake");

    let nothing_looked = search_supporting_material(&package, None, &NotCancelled);
    assert_eq!(
        nothing_looked.result,
        Err(Failure::Unsupported(ProviderUnavailable {
            provider: None,
            diagnostics: Vec::new(),
        })),
        "a build with no provider names no provider and looks at nothing"
    );
}

/// Each platform state that prevents an answer travels as itself.
///
/// The defect this catches is one generic failure: switching indexing on, moving a Package
/// to an indexed volume, granting access, and rewording a query are four different things to
/// do, and a caller told only that the search failed can do none of them.
#[test]
fn every_state_that_prevents_an_answer_is_its_own_state() {
    let (_tree, root) = package("supporting-material-unavailable");
    vendors(&root, TARNWICK);
    order(&root, "Alpha-Order", ALPHA);
    let package = loaded(&root);

    let said = || ProviderUnavailable {
        provider: Some("fake".to_owned()),
        diagnostics: vec![ProviderDiagnostic::reported("what the platform said")],
    };
    for (reason, failure) in [
        (
            ProviderUnavailableReason::IndexingDisabled,
            Failure::IndexingDisabled(said()),
        ),
        (
            ProviderUnavailableReason::VolumeNotIndexed,
            Failure::VolumeNotIndexed(said()),
        ),
        (
            ProviderUnavailableReason::AccessDenied,
            Failure::AccessDenied(said()),
        ),
        (
            ProviderUnavailableReason::QueryRefused,
            Failure::QueryRefused(said()),
        ),
    ] {
        let index = reported(ProviderAnswer::Unavailable {
            reason,
            diagnostics: vec![ProviderDiagnostic::reported("what the platform said")],
        });
        let answer = search_supporting_material(&package, Some(&index), &NotCancelled);

        assert_eq!(
            answer.result,
            Err(failure),
            "{reason:?} reaches a caller as itself, beside the provider that reported it"
        );
    }
}

const DAMAGED: &str = "{ not a record";

/// A Damaged Order is not scoped and not grouped, and a file outside every Order Directory
/// is no result.
///
/// The defect this catches is a group a caller cannot navigate to: an Order whose record
/// could not be read has no Order to open, and a file beside the Orders Directory belongs to
/// none at all. Both are asked here rather than only the first, because the provider is free
/// to report whatever its index holds and the scope it was given is advice.
#[test]
fn a_file_with_no_healthy_order_to_group_under_is_no_result() {
    let (_tree, root) = package("supporting-material-scope");
    vendors(&root, TARNWICK);
    let alpha = order(&root, "Alpha-Order", ALPHA);
    let damaged = order(&root, "Damaged-Order", DAMAGED);
    let quote = supporting(&alpha, "quote.pdf", "");
    let inside_damaged = supporting(&damaged, "quote.pdf", "");
    let loose = supporting(&root, "loose.pdf", "");
    let package = loaded(&root);

    let index = found(vec![hit(&inside_damaged), hit(&loose), hit(&quote)]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);

    assert_eq!(
        reported_paths(&snapshot),
        ["Bestellungen/Alpha-Order/quote.pdf"],
        "only the file the healthy Order holds"
    );
    let findings = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| group.diagnostics.iter().cloned())
        .collect::<Vec<_>>();
    assert!(!findings.is_empty(), "the Damaged Order is a finding");
    assert_eq!(
        snapshot.diagnostics, findings,
        "and what damaged it travels beside the answer"
    );
}

#[cfg(unix)]
#[test]
fn legal_backslashes_in_supporting_filenames_survive_grouping() {
    let (_tree, root) = package("supporting-backslash-filename");
    vendors(&root, TARNWICK);
    let directory = order(&root, "Alpha-Order", ALPHA);
    let ordinary = supporting(&directory, "quote.pdf", "ordinary opaque bytes");
    let backslash = supporting(&directory, "vendor\\quote.pdf", "opaque vendor bytes");
    let package = loaded(&root);
    let index = found(vec![hit(&ordinary), hit(&backslash)]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);
    let paths = reported_paths(&snapshot);
    assert_eq!(
        paths.len(),
        2,
        "both real provider-reported files survive: {paths:?}"
    );
    let SupportingMaterialSnapshot { orders, .. } = looked(&snapshot);
    let matched = orders[0]
        .files
        .iter()
        .find(|file| file.filename == "vendor\\quote.pdf")
        .unwrap();
    assert_eq!(matched.location.absolute_path, backslash);
    assert_eq!(
        matched.location.package_relative_path.as_deref(),
        Some("Bestellungen/Alpha-Order/vendor\\quote.pdf")
    );
    assert_eq!(std::fs::read(backslash).unwrap(), b"opaque vendor bytes");
}

#[cfg(unix)]
#[test]
fn native_spelling_keeps_newlines_and_drops_unrepresentable_paths() {
    use std::os::unix::ffi::OsStringExt;
    let (_tree, root) = package("supporting-native-spelling");
    vendors(&root, TARNWICK);
    let directory = order(&root, "Alpha-Order", ALPHA);
    let newline = supporting(&directory, "quote\nnotes.pdf", "opaque vendor bytes");
    let unrepresentable = directory.join(std::ffi::OsString::from_vec(b"quote-\xff.pdf".to_vec()));
    let package = loaded(&root);
    let index = found(vec![hit(&unrepresentable), hit(&newline)]);
    let snapshot = search_supporting_material(&package, Some(&index), &NotCancelled);
    assert_eq!(
        reported_paths(&snapshot),
        vec![PathBuf::from("Bestellungen/Alpha-Order/quote\nnotes.pdf")]
    );
    let SupportingMaterialSnapshot { orders, .. } = looked(&snapshot);
    assert_eq!(orders[0].files[0].filename, "quote\nnotes.pdf");
    assert_eq!(orders[0].files[0].location.absolute_path, newline);
    assert_eq!(
        orders[0].files[0].location.package_relative_path.as_deref(),
        Some("Bestellungen/Alpha-Order/quote\nnotes.pdf")
    );
}
