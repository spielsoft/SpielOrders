//! Assessing the derived Package Index without writing, and rebuilding it on its own.
//!
//! The ordinary rebuild is fused to the opening load, which is where a rebuild belongs:
//! after a canonical write, as the derived refresh that follows it. What these tests are
//! about is the two questions a caller can ask about the index *by itself* — what state
//! it is in, and replacing it deliberately — neither of which has a canonical write in
//! front of it.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::support::package_index::{Effect, ReplacementBoundary, TestFileSystem};
use spielorders_core::{
    canonical::{
        PackageIndexMalformedReason, PackageIndexRebuildPlanError, PackageIndexRebuildRejection,
        PackageIndexRefresh, PackageIndexStaleReason, PackageIndexState, PackageLoad,
        assess_package_index, commit_package_index_rebuild, plan_package_index_rebuild,
    },
    mutation::MutationReport,
    ports::FileSystemEntryKind,
    readme::{DiagnosticCode, Location, OperationFailureCode},
};
use std::path::Path;

const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));

/// The assessment answers each of the five states with its own typed reasons, and writes
/// nothing on any of them.
///
/// **Writing nothing is the substance, not a nicety.** The only assessment this crate had
/// was fused to `open_package`, which durably replaces the index as a side effect — so a
/// caller that only wanted to know what state the index was in could not ask without
/// changing it, and the answer to "is it stale" would be "no, because asking fixed it".
#[test]
fn assessing_the_derived_index_distinguishes_every_state_without_writing() {
    // `current`: the index is deterministic, agrees with the canonical records, and the
    // change evidence proves nothing has replaced an input since it was written.
    let current = fresh_index_filesystem();
    assert_eq!(assessed(&current), (PackageIndexState::Current, Vec::new()));

    // `stale`: an Order Directory the stored index does not carry. The uncertainty
    // stands beside the definite reason because the freshness question stops at the
    // membership difference, before it has compared every input's own evidence.
    let stale = fresh_index_filesystem();
    add_order(&stale);
    assert_eq!(
        assessed(&stale),
        (
            PackageIndexState::Stale,
            vec![
                PackageIndexStaleReason::OrderMembershipChanged,
                PackageIndexStaleReason::ChangeEvidenceUncertain,
            ]
        )
    );

    // `stale` for a second reason, so the reasons are the answer rather than the state
    // alone: the Vendor records were replaced after the index was written.
    let vendors_changed = fresh_index_filesystem();
    vendors_changed.put_change_evidence("/package/.spielorders/vendors.json", 1, 30);
    assert_eq!(
        assessed(&vendors_changed),
        (
            PackageIndexState::Stale,
            vec![PackageIndexStaleReason::VendorsChanged]
        )
    );

    // `missing`: nothing stands at the index path at all.
    let missing = fresh_index_filesystem();
    missing.remove_path("/package/.spielorders/index.json");
    assert_eq!(assessed(&missing), (PackageIndexState::Missing, Vec::new()));

    // `malformed`: something stands there that is not an index this build can read.
    let malformed = fresh_index_filesystem();
    malformed.put_file("/package/.spielorders/index.json", b"{ not json");
    assert_eq!(
        assessed(&malformed),
        (PackageIndexState::Malformed, Vec::new())
    );
    assert_eq!(
        malformed_reasons(&malformed),
        vec![PackageIndexMalformedReason::InvalidSyntax]
    );

    // `wrong_version`: a recognizable `schema_version` this build does not project.
    let wrong_version = fresh_index_filesystem();
    wrong_version.put_file(
        "/package/.spielorders/index.json",
        b"{\n  \"schema_version\": 999,\n  \"orders\": [],\n  \"vendors\": []\n}\n",
    );
    assert_eq!(
        assessed(&wrong_version),
        (PackageIndexState::WrongVersion, Vec::new())
    );
}

/// A stale index is the derived cache being out of date, and says nothing about the
/// canonical records it is derived from.
///
/// The integrity decision makes `index.json` the one managed artifact that is rebuilt
/// rather than repaired, so a Package whose only problem is a stale index is a Package
/// with no findings at all.
#[test]
fn a_stale_index_is_not_reported_as_damage_to_canonical_metadata() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);

    let PackageLoad::Available(package) = assess_package_index(Path::new("/package"), &filesystem)
    else {
        panic!("a Package whose only problem is its derived index is available")
    };

    assert_eq!(package.index().state(), PackageIndexState::Stale);
    assert_eq!(package.diagnostic_groups(), &[]);
    assert_eq!(package.healthy_orders().len(), 1);
}

/// A deliberate rebuild replaces the derived index from the canonical records, writes
/// nothing else, and rebuilds the same Package to identical bytes.
///
/// **Not fused to a load.** The rebuild this crate had ran inside `open_package`, so it
/// could only happen as the derived refresh after a canonical write; a caller who wanted
/// the index replaced on purpose had to perform a canonical mutation to get one.
#[test]
fn a_deliberate_rebuild_replaces_the_index_from_canonical_records_deterministically() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);
    let index = "/package/.spielorders/index.json";
    let stale_bytes = filesystem.bytes(index).expect("the stale index is stored");

    let plan =
        plan_package_index_rebuild(Path::new("/package"), &filesystem).expect("the plan is made");
    assert!(plan.changed(), "the stale index is not the rebuilt one");
    assert_eq!(
        plan.indexed_orders()
            .iter()
            .filter_map(Location::package_relative_path)
            .collect::<Vec<_>>(),
        ["Orders/Order"]
    );
    assert_eq!(
        filesystem.bytes(index).as_deref(),
        Some(stale_bytes.as_slice()),
        "planning writes nothing"
    );
    let candidate = plan.candidate().to_vec();

    let MutationReport::Committed { result, .. } = commit_package_index_rebuild(plan, &filesystem)
    else {
        panic!("a healthy Package's index rebuilds")
    };

    assert_eq!(filesystem.bytes(index), Some(candidate.clone()));
    assert_eq!(result.index().state(), PackageIndexState::Current);
    assert_eq!(result.index().refresh(), PackageIndexRefresh::Rebuilt);
    // Nothing canonical was replaced, and the Supporting Material was not so much as
    // observed.
    assert!(
        filesystem
            .effects
            .borrow()
            .iter()
            .all(|effect| match effect {
                Effect::Replace(_, destination) => destination == Path::new(index),
                effect => effect
                    .paths()
                    .into_iter()
                    .all(|path| path != Path::new(SUPPORTING_MATERIAL)),
            }),
        "a rebuild replaces only the derived index and reads no Supporting Material"
    );

    // The second run rebuilds the same Package to the same bytes, so the deterministic
    // projection is a property of the records rather than of the run.
    let again =
        plan_package_index_rebuild(Path::new("/package"), &filesystem).expect("the plan is made");
    assert!(!again.changed(), "the rebuilt index is already the answer");
    assert_eq!(again.candidate(), candidate.as_slice());
    assert!(matches!(
        commit_package_index_rebuild(again, &filesystem),
        MutationReport::Committed { .. }
    ));
    assert_eq!(filesystem.bytes(index), Some(candidate));
}

/// A Damaged Order is absent from the rebuilt index and still reported.
///
/// The index is a projection of *healthy* canonical records, so an Order whose record
/// could not be read has nothing to project — and dropping it silently would make the
/// rebuild look like a repair that removed the problem.
#[test]
fn a_rebuilt_index_omits_a_damaged_order_and_still_reports_it() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);
    filesystem.put_entry("/package/Orders/Damaged", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Damaged/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Damaged/.spielorders/order.json",
        b"{ not json",
    );

    let plan =
        plan_package_index_rebuild(Path::new("/package"), &filesystem).expect("the plan is made");

    assert_eq!(
        plan.indexed_orders()
            .iter()
            .filter_map(Location::package_relative_path)
            .collect::<Vec<_>>(),
        ["Orders/Order"],
        "the Damaged Order is absent from the projection"
    );
    let reported = plan
        .package()
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .map(|diagnostic| diagnostic.code())
        .collect::<Vec<_>>();
    assert!(
        reported.contains(&DiagnosticCode::ManagedArtifactInvalidSyntax),
        "and stays visible diagnostically: {reported:?}"
    );
}

/// A rebuild whose replacement fails leaves the stored index exactly as it was and
/// reports the failure.
#[test]
fn a_rebuild_whose_replacement_fails_preserves_the_existing_index() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);
    let index = "/package/.spielorders/index.json";
    let stored = filesystem.bytes(index).expect("the stale index is stored");
    let plan =
        plan_package_index_rebuild(Path::new("/package"), &filesystem).expect("the plan is made");
    filesystem.fail_at(ReplacementBoundary::Replace);

    let MutationReport::Rejected { rejection, .. } =
        commit_package_index_rebuild(plan, &filesystem)
    else {
        panic!("a failed replacement rejects the rebuild")
    };

    let PackageIndexRebuildRejection::ReplacementFailed { failure } = rejection;
    assert_eq!(
        failure.code(),
        OperationFailureCode::PackageIndexReplacementFailed
    );
    assert_eq!(filesystem.bytes(index), Some(stored));
}

/// An Order README that exists and could not be read leaves the version-1 projection with
/// nothing truthful to record, so the rebuild is refused rather than inventing a text.
#[test]
fn a_rebuild_is_refused_where_an_order_readme_text_was_not_established() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);
    filesystem.put_entry(
        "/package/Orders/Order/README.md",
        FileSystemEntryKind::Directory,
    );

    let error = plan_package_index_rebuild(Path::new("/package"), &filesystem)
        .expect_err("no honest candidate exists");

    let PackageIndexRebuildPlanError::OrderReadmeTextUnavailable(package) = error else {
        panic!("the refusal names the reason: {error:?}")
    };
    assert!(
        package
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ReadmeUnavailable),
        "and carries the observation behind it"
    );
}

/// The state, and the stale reasons behind it, for one assessment that must not write.
fn assessed(filesystem: &TestFileSystem) -> (PackageIndexState, Vec<PackageIndexStaleReason>) {
    let load = assess_package_index(Path::new("/package"), filesystem);
    assert_wrote_nothing(filesystem);
    let PackageLoad::Available(package) = load else {
        panic!("a readable Package answers with its assessed index")
    };
    assert_eq!(
        package.index().refresh(),
        match package.index().state() {
            PackageIndexState::Current => PackageIndexRefresh::NotNeeded,
            _ => PackageIndexRefresh::NotAttempted,
        },
        "an assessment attempts no refresh"
    );
    (
        package.index().state(),
        package.index().stale_reasons().to_vec(),
    )
}

fn malformed_reasons(filesystem: &TestFileSystem) -> Vec<PackageIndexMalformedReason> {
    let PackageLoad::Available(package) = assess_package_index(Path::new("/package"), filesystem)
    else {
        panic!("a readable Package answers with its assessed index")
    };
    package.index().malformed_reasons().to_vec()
}

fn assert_wrote_nothing(filesystem: &TestFileSystem) {
    let effects = filesystem.effects.borrow();
    assert!(
        effects.iter().all(|effect| !matches!(
            effect,
            Effect::WriteStage(..) | Effect::Replace(..) | Effect::Remove(..) | Effect::Sync(..)
        )),
        "an assessment writes nothing: {effects:?}"
    );
}

/// A Package with no Orders whose stored index is deterministic, agrees with the
/// canonical records, and carries change evidence newer than every input.
fn fresh_index_filesystem() -> TestFileSystem {
    let filesystem = TestFileSystem::minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        "/package/.spielorders/vendors.json",
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence("/package/.spielorders/index.json", 1, 20);
    filesystem
}

/// One healthy Order the stored index does not carry, with Supporting Material under it.
///
/// The opaque file is what makes "neither index command inspects Supporting Material" a
/// claim about a real entry rather than about an empty directory.
fn add_order(filesystem: &TestFileSystem) {
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file(SUPPORTING_MATERIAL, b"Quote, 3 metres of fiber.\n");
    filesystem.put_change_evidence("/package/Orders/Order/.spielorders/order.json", 1, 10);
}

/// One opaque user file under an Order Directory.
const SUPPORTING_MATERIAL: &str = "/package/Orders/Order/Quotes/quote.txt";
