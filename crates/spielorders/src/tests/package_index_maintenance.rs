//! Assessing the derived Package Index without writing, and rebuilding it on its own.
//!
//! The ordinary rebuild is the derived refresh that follows a canonical write. What these
//! tests are about is the two questions a caller can ask about the index *by itself* —
//! what state it is in, and replacing it deliberately — neither of which has a canonical
//! write in front of it.
//!
//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries; Define durable metadata schemas and version
//! evolution.

use crate::Failure;
use crate::Report;
use crate::tests::support::{ScriptedFileSystem, effects, minimal_package, opened, rebuild_index};
use crate::{
    canonical::PackageIndexMalformedReason, canonical::PackageIndexStaleReason,
    canonical::PackageIndexState, ports::FileSystemEntryKind, readme::DiagnosticCode,
};
use std::path::Path;
use std::path::PathBuf;

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
/// **Writing nothing is the substance, not a nicety.** A caller that only wants to know
/// what state the index is in must be able to ask without changing it.
#[test]
fn assessing_the_derived_index_distinguishes_every_state_without_writing() {
    // `current`: the deterministic index agrees with the canonical records.
    let current = fresh_index_filesystem();
    assert_eq!(assessed(&current), (PackageIndexState::Current, Vec::new()));

    // `stale`: an Order Directory the stored index does not carry.
    let stale = fresh_index_filesystem();
    add_order(&stale);
    assert_eq!(
        assessed(&stale),
        (
            PackageIndexState::Stale,
            vec![PackageIndexStaleReason::OrderMembershipChanged,]
        )
    );

    // `stale` for a second reason: the Vendor record contains an uncached Vendor.
    let vendors_changed = fresh_index_filesystem();
    vendors_changed.put_file("/package/.spielorders/vendors.json", br#"{"schema_version":1,"vendors":[{"vendor_id":"11111111-1111-4111-8111-111111111111","name":"Example Supply","contacts":[]}]}"#);
    assert_eq!(
        assessed(&vendors_changed),
        (
            PackageIndexState::Stale,
            vec![PackageIndexStaleReason::VendorsChanged,]
        )
    );

    // `missing`: nothing stands at the index path at all.
    let missing = fresh_index_filesystem();
    missing.remove("/package/.spielorders/index.json");
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

    let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
        panic!("a Package whose only problem is its derived index is available")
    };

    assert_eq!(package.contents.index.state, PackageIndexState::Stale);
}

/// A deliberate rebuild replaces the derived index from the canonical records, writes
/// nothing else, needs nothing from Supporting Material, and rebuilds the same Package to
/// identical bytes.
///
/// **Not fused to a load.** A caller can replace the index on purpose without
/// performing a canonical mutation to get one.
#[test]
fn a_deliberate_rebuild_replaces_the_index_from_canonical_records_deterministically() {
    let filesystem = fresh_index_filesystem();
    add_order(&filesystem);
    let index = "/package/.spielorders/index.json";
    let stale_bytes = filesystem.bytes(index).expect("the stale index is stored");
    let record = "/package/Orders/Order/.spielorders/order.json";
    let canonical =
        [record, "/package/.spielorders/vendors.json"].map(|path| filesystem.bytes(path));
    let plan = crate::plan(
        &rebuild_index(PathBuf::from(Path::new("/package"))),
        &effects(&filesystem),
    )
    .result
    .expect("the plan is made");
    assert_eq!(
        filesystem.bytes(index).as_deref(),
        Some(stale_bytes.as_slice()),
        "planning writes nothing"
    );

    let Ok(Report::Committed { .. }) = crate::commit(plan, &effects(&filesystem)).result else {
        panic!("a healthy Package's index rebuilds")
    };

    let rebuilt = filesystem
        .bytes(index)
        .expect("the rebuilt index is stored");
    assert_ne!(
        rebuilt, stale_bytes,
        "the stale index is not the rebuilt one"
    );
    assert_eq!(
        [record, "/package/.spielorders/vendors.json"].map(|path| filesystem.bytes(path)),
        canonical,
        "a rebuild replaces only the derived index"
    );
    assert_eq!(
        filesystem.bytes(SUPPORTING_MATERIAL).as_deref(),
        Some(SUPPORTING_BYTES)
    );
    let inspected = filesystem.naming(|path| path.starts_with(SUPPORTING_DIRECTORY));
    assert!(
        inspected.is_empty(),
        "index work never inspects Supporting Material: {inspected:?}"
    );

    // The second run rebuilds the same Package to the same bytes, so the deterministic
    // projection is a property of the records rather than of the run.
    let again = crate::plan(
        &rebuild_index(PathBuf::from(Path::new("/package"))),
        &effects(&filesystem),
    )
    .result
    .expect("the plan is made");
    assert!(matches!(
        crate::commit(again, &effects(&filesystem)).result,
        Ok(Report::Committed { .. })
    ));
    assert_eq!(filesystem.bytes(index), Some(rebuilt));
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

    let error = crate::plan(
        &rebuild_index(PathBuf::from(Path::new("/package"))),
        &effects(&filesystem),
    )
    .result
    .expect_err("no honest candidate exists");

    let Failure::OrderReadmeTextUnavailable(package) = error else {
        panic!("the refusal names the reason: {error:?}")
    };
    assert!(
        package
            .diagnostic_groups
            .iter()
            .flat_map(|group| &group.diagnostics)
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ReadmeUnavailable),
        "and carries the observation behind it"
    );
}

/// The state, and the stale reasons behind it, for one assessment that must not write.
fn assessed(filesystem: &ScriptedFileSystem) -> (PackageIndexState, Vec<PackageIndexStaleReason>) {
    let load = opened(Path::new("/package"), filesystem);
    assert_wrote_nothing(filesystem);
    let Ok(package) = load.result else {
        panic!("a readable Package answers with its assessed index")
    };
    let index = package.contents.index;
    (index.state, index.stale_reasons.to_vec())
}

fn malformed_reasons(filesystem: &ScriptedFileSystem) -> Vec<PackageIndexMalformedReason> {
    let Ok(package) = opened(Path::new("/package"), filesystem).result else {
        panic!("a readable Package answers with its assessed index")
    };
    package.contents.index.malformed_reasons.to_vec()
}

fn assert_wrote_nothing(filesystem: &ScriptedFileSystem) {
    assert_eq!(filesystem.effects(), [], "an assessment writes nothing");
}

/// A Package with no Orders whose deterministic index agrees with canonical records.
fn fresh_index_filesystem() -> ScriptedFileSystem {
    let filesystem = minimal_package();
    filesystem.put_file("/package/.spielorders/index.json", MINIMAL_INDEX);
    filesystem
}

/// One healthy Order the stored index does not carry, with Supporting Material under it.
///
/// The opaque file is what makes "a rebuild needs nothing from Supporting Material" a
/// claim about a real entry rather than about an empty directory.
fn add_order(filesystem: &ScriptedFileSystem) {
    filesystem.put_entry("/package/Orders/Order", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Order/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Order/.spielorders/order.json",
        MINIMAL_ORDER,
    );
    filesystem.put_file(SUPPORTING_MATERIAL, SUPPORTING_BYTES);
    filesystem.put_change_evidence("/package/Orders/Order/.spielorders/order.json", 1, 10);
}

/// One opaque user file under an Order Directory.
const SUPPORTING_DIRECTORY: &str = "/package/Orders/Order/Quotes";
const SUPPORTING_MATERIAL: &str = "/package/Orders/Order/Quotes/quote.txt";
const SUPPORTING_BYTES: &[u8] = b"Quote, 3 metres of fiber.\n";
