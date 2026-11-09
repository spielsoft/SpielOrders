//! The reads a caller asks of a Package: what they answer with, that opening writes
//! nothing, and that a long read stops when its caller cancels.
//!
//! Decision sources: Define the Rust CLI capability contract; Effect and adapter
//! boundaries.

use crate::OrderRead;
use crate::ReadmeRead;
use crate::VendorRead;
use crate::metadata::Vendor;
use crate::ports::FileSystemEntryKind;
use crate::search::OrderListEntry;
use crate::tests::support::{Operation, effects, enumerate_orders_as, minimal_package, opened};
use crate::{
    Answer, Diagnosis, Failure, OrderRef, Package, Plan, Report, Scope, Warning, diagnose, open,
    validate,
};
use crate::{ProviderReport, SupportingMaterialSnapshot};
use crate::{
    canonical::PackageIndexRecord, canonical::PackageIndexState,
    canonical::ReusableValueProjection, search::CaseSensitivity, search::ReusableValueKind,
    search::SearchQuery, search::VendorVisibility,
};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const MINIMAL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/derived-index/minimal-deterministic.json"
));
const MINIMAL_ORDER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));

/// Every result a caller receives is a value it owns and may hand to another thread: no
/// borrowed lifetime, and nothing a second thread could not share.
#[test]
fn every_public_result_is_owned_and_thread_safe() {
    fn owned<T: Send + Sync + 'static>() {}
    owned::<Answer<Package>>();
    owned::<Answer<OrderRead>>();
    owned::<Answer<VendorRead>>();
    owned::<Answer<ReadmeRead>>();
    owned::<Answer<Diagnosis>>();
    owned::<Answer<SupportingMaterialSnapshot>>();
    owned::<Answer<Plan>>();
    owned::<Answer<Report>>();
    owned::<Answer<Vec<OrderListEntry>>>();
    owned::<Answer<Vec<Vendor>>>();
    owned::<Answer<Vec<ReusableValueProjection>>>();
    owned::<Answer<PackageIndexRecord>>();
    owned::<ProviderReport>();
    owned::<Failure>();
    owned::<Warning>();
}

/// Every list read off an opened Package answers with that Package's findings beside its
/// rows, as every other read of the Package does, so a caller shows a list together with
/// what is wrong with the Package it came from.
#[test]
fn every_list_answer_carries_the_findings_of_its_package() {
    let filesystem = minimal_package();
    filesystem.put_entry("/package/Orders/Damaged", FileSystemEntryKind::Directory);
    filesystem.put_entry(
        "/package/Orders/Damaged/.spielorders",
        FileSystemEntryKind::Directory,
    );
    filesystem.put_file(
        "/package/Orders/Damaged/.spielorders/order.json",
        b"{ not json",
    );
    let package = opened(Path::new("/package"), &filesystem)
        .result
        .expect("a Damaged Order leaves its Package usable");
    let findings = package
        .contents
        .diagnostic_groups
        .iter()
        .flat_map(|group| group.diagnostics.clone())
        .collect::<Vec<_>>();
    assert!(!findings.is_empty(), "the Damaged Order is a finding");

    let everything = SearchQuery::new("", CaseSensitivity::default());
    assert_eq!(package.orders_matching(&everything).diagnostics, findings);
    assert_eq!(
        package
            .vendors_matching(&everything, VendorVisibility::All)
            .diagnostics,
        findings
    );
    assert_eq!(
        package.values(ReusableValueKind::Orderer).diagnostics,
        findings
    );
}

/// Opening only assesses the derived Package Index: a missing, malformed, or stale one is
/// reported as found, and nothing is written to correct it.
#[test]
fn opening_a_package_writes_nothing_whatever_its_index_needs() {
    for (index, state) in [
        (None, PackageIndexState::Missing),
        (
            Some(b"{ malformed".as_slice()),
            PackageIndexState::Malformed,
        ),
        (Some(MINIMAL_INDEX), PackageIndexState::Stale),
    ] {
        let filesystem = minimal_package();
        if let Some(index) = index {
            filesystem.put_file("/package/.spielorders/index.json", index);
        }

        let Ok(package) = opened(Path::new("/package"), &filesystem).result else {
            panic!("a Package whose only problem is its index opens")
        };

        assert_eq!(filesystem.effects(), [], "opening writes nothing");
        assert_eq!(package.contents.index.state, state);
    }
}

/// A Package of two Orders, `A` and `B`, listed in that order.
fn two_orders() -> crate::tests::support::ScriptedFileSystem {
    let filesystem = minimal_package();
    for (name, order_id) in [
        ("A", "00000000-0000-4000-8000-00000000000a"),
        ("B", "00000000-0000-4000-8000-00000000000b"),
    ] {
        let directory = format!("/package/Orders/{name}");
        filesystem.put_entry(&directory, FileSystemEntryKind::Directory);
        filesystem.put_entry(
            format!("{directory}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_file(
            format!("{directory}/.spielorders/order.json"),
            MINIMAL_ORDER
                .replacen("8b6e3f0a-8ec5-4d47-95e1-412652a56d16", order_id, 1)
                .as_bytes(),
        );
    }
    enumerate_orders_as(&filesystem, &["A", "B"]);
    filesystem
}

/// A long read whose caller cancels partway stops at its next effect and answers
/// `cancelled`: having read the first Order's record, it never reaches the second.
#[test]
fn a_long_read_stops_at_its_next_effect_once_cancelled() {
    type Read = fn(&Path, &crate::tests::support::ScriptedFileSystem, &AtomicBool) -> Failure;
    let reads: [(&str, Read); 3] = [
        ("open", |root, filesystem, cancellation| {
            open(root, &effects(filesystem), cancellation)
                .result
                .expect_err("a cancelled open answers nothing")
        }),
        ("validate", |root, filesystem, cancellation| {
            validate(
                &Scope::Package(root.into()),
                &effects(filesystem),
                cancellation,
            )
            .result
            .expect_err("a cancelled validation answers nothing")
        }),
        ("diagnose", |root, filesystem, cancellation| {
            diagnose(
                &Scope::Order(OrderRef::Directory(root.join("Orders/A"))),
                &effects(filesystem),
                cancellation,
            )
            .result
            .expect_err("a cancelled diagnosis answers nothing")
        }),
    ];
    for (read, answer) in reads {
        let filesystem = two_orders();
        let cancellation = Arc::new(AtomicBool::new(false));
        let caller = Arc::clone(&cancellation);
        filesystem
            .on(Operation::Read)
            .at("/package/Orders/A/.spielorders/order.json")
            .followed_by(move |_| caller.store(true, Ordering::SeqCst))
            .proceeds();

        let failure = answer(Path::new("/package"), &filesystem, &cancellation);

        assert_eq!(failure, Failure::Cancelled, "{read}");
        assert!(
            !filesystem
                .calls()
                .iter()
                .any(|call| call.path.starts_with("/package/Orders/B")),
            "{read} stopped before the second Order"
        );
    }
}
