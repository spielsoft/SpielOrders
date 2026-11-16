//! A stored index is a derived projection. Browsing reports canonical facts and diagnoses
//! missing, stale, and unusable projections without changing any Package content.
//!
//! Decision sources: Define durable metadata schemas and version evolution;
//! Define the complete Package read projection and load-state contract.

use crate::canonical::{PackageIndexStaleReason, PackageIndexState};
use crate::metadata::OrderDocument;
use crate::ports::{FileChangeEvidence, FileSystemEntryKind};
use crate::tests::support::{
    Index, IndexedOrder, Operation, ScriptedFileSystem, enumerate_orders_as, minimal_package,
    opened,
};
use std::ops::Deref;
use std::path::Path;

const FULL_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/index.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const ORDER: &str = "/package/Orders/2026_07_19_Tarnwick_Laser_components";
const RECORD: &str = "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders/order.json";
const README: &str = "/package/Orders/2026_07_19_Tarnwick_Laser_components/README.md";
const INDEX: &str = "/package/.spielorders/index.json";
const VENDORS: &str = "/package/.spielorders/vendors.json";

/// Every unchanged input is older than the index in one comparison domain. Changing
/// one observation therefore tests that observation, not an unrelated fallback.
fn proven_package() -> ScriptedFileSystem {
    let filesystem = package();
    for path in [
        "/package/.spielorders/config.toml",
        "/package/.spielorders/version.json",
        VENDORS,
        RECORD,
        README,
    ] {
        filesystem.put_change_evidence(path, 1, 10);
    }
    filesystem.put_change_evidence(INDEX, 1, 20);
    filesystem
}

fn package() -> ScriptedFileSystem {
    let filesystem = minimal_package();
    filesystem.put_entry(ORDER, FileSystemEntryKind::Directory);
    filesystem.put_file(RECORD, FULL_ORDER);
    filesystem.put_file(README, b"User-authored Markdown text");
    filesystem.put_file(VENDORS, FULL_VENDORS);
    filesystem.put_file(
        INDEX,
        &Index::from_json_bytes(FULL_INDEX)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    );
    filesystem
}

fn open(filesystem: &ScriptedFileSystem) -> crate::Package {
    let result = opened(Path::new("/package"), filesystem).result.unwrap();
    assert!(filesystem.effects().is_empty(), "browsing writes nothing");
    result
}

#[test]
fn complete_projection() {
    let filesystem = package();
    let package = open(&filesystem);
    assert_eq!(package.contents.index.state, PackageIndexState::Current);
    let [order] = package.contents.healthy_orders.as_slice() else {
        panic!("one Order")
    };
    assert_eq!(order.document.title.deref(), "Laser components");
    assert_eq!(
        order.readme_text.as_deref(),
        Some("User-authored Markdown text")
    );
    assert_eq!(order.item_totals, [169.0]);
    assert_eq!(order.order_total, 169.0);
    let vendor = package.contents.vendor_for_order(0).unwrap();
    assert_eq!(vendor.name.deref(), "Tarnwick");
    assert_eq!(vendor.contacts[0].name.deref(), "Grace Hopper");
    assert_eq!(
        package.contents.projections.orderers[0].value,
        "Ada Lovelace"
    );
    assert_eq!(
        package.contents.projections.ordering_institutions[0].value,
        "Example Institute"
    );
    assert_eq!(
        package.contents.projections.projects[0].value,
        "Telescope"
    );
    assert_eq!(
        package.contents.projections.order_groups[0].value,
        "Summer build"
    );
}

#[test]
fn canonical_changes() {
    changed_input(FileChangeEvidence::Comparable {
        comparison_domain: 1,
        sequence: 30,
    });
}

#[test]
fn equal_ticks_read_canonical_changes() {
    changed_input(FileChangeEvidence::Comparable {
        comparison_domain: 1,
        sequence: 20,
    });
}

#[test]
fn incomparable_clocks_read_canonical_changes() {
    changed_input(FileChangeEvidence::Comparable {
        comparison_domain: 2,
        sequence: 10,
    });
}

#[test]
fn uncertain_clocks_read_canonical_changes() {
    changed_input(FileChangeEvidence::Uncertain);
}

fn changed_input(evidence: FileChangeEvidence) {
    for (path, before, after, reason) in [
        (
            VENDORS,
            "Tarnwick",
            "Example Supply",
            PackageIndexStaleReason::VendorsChanged,
        ),
        (
            RECORD,
            "Laser components",
            "Lenses",
            PackageIndexStaleReason::OrderMetadataChanged,
        ),
        (
            README,
            "User-authored Markdown text",
            "Updated notes",
            PackageIndexStaleReason::OrderReadmeChanged,
        ),
    ] {
        let filesystem = proven_package();
        let text = String::from_utf8(filesystem.bytes(path).unwrap()).unwrap();
        filesystem.put_file(path, text.replace(before, after).as_bytes());
        filesystem
            .on(Operation::ObserveFileChange)
            .at(path)
            .reports_evidence(evidence);
        let package = open(&filesystem);
        assert_eq!(package.contents.index.state, PackageIndexState::Stale);
        assert!(package.contents.index.stale_reasons.contains(&reason));
        let order = &package.contents.healthy_orders[0];
        let observed = match path {
            VENDORS => package.contents.vendors[0].name.deref(),
            RECORD => order.document.title.deref(),
            _ => order.readme_text.as_deref().unwrap(),
        };
        assert_eq!(observed, after);
    }
}

#[test]
fn matching_content() {
    for evidence in [None, Some(10), Some(20), Some(30)] {
        let filesystem = proven_package();
        if let Some(sequence) = evidence {
            filesystem.put_change_evidence(VENDORS, 1, sequence);
        } else {
            filesystem
                .on(Operation::ObserveFileChange)
                .at(VENDORS)
                .reports_evidence(FileChangeEvidence::Uncertain);
        }
        let package = open(&filesystem);
        assert_eq!(package.contents.index.state, PackageIndexState::Current);
        assert!(package.contents.index.stale_reasons.is_empty());
    }
}

#[test]
fn readme_availability() {
    let absent = package();
    absent.remove(README);
    let package = open(&absent);
    assert_eq!(
        package.contents.healthy_orders[0].readme_text.as_deref(),
        Some("")
    );
    assert!(
        package
            .contents
            .index
            .stale_reasons
            .contains(&PackageIndexStaleReason::OrderReadmeChanged)
    );

    let unreadable = self::package();
    unreadable.on(Operation::Read).at(README).fails();
    let package = open(&unreadable);
    assert_eq!(package.contents.healthy_orders[0].readme_text, None);
    assert_eq!(package.contents.index.state, PackageIndexState::Stale);
    assert!(
        package
            .contents
            .index
            .stale_reasons
            .contains(&PackageIndexStaleReason::ChangeEvidenceUncertain)
    );
    assert!(
        !package
            .contents
            .index
            .stale_reasons
            .contains(&PackageIndexStaleReason::OrderReadmeChanged)
    );
}

#[test]
fn unusable_records() {
    for kind in [
        FileSystemEntryKind::Directory,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        for path in [
            RECORD,
            "/package/Orders/2026_07_19_Tarnwick_Laser_components/.spielorders",
        ] {
            let filesystem = package();
            let wrong_kind =
                if path.ends_with("/.spielorders") && kind == FileSystemEntryKind::Directory {
                    FileSystemEntryKind::File
                } else {
                    kind
                };
            filesystem.put_entry(path, wrong_kind);
            let package = open(&filesystem);
            assert!(
                package.contents.healthy_orders.is_empty(),
                "{path}: {kind:?}"
            );
            assert_eq!(package.contents.damaged_orders.len(), 1);
            assert_eq!(package.contents.index.state, PackageIndexState::Stale);
        }
        let filesystem = package();
        filesystem.put_entry(VENDORS, kind);
        assert!(opened(Path::new("/package"), &filesystem).result.is_err());
        assert!(filesystem.effects().is_empty());
    }
}

#[test]
fn membership_changes() {
    let filesystem = package();
    filesystem.remove(ORDER);
    let package = open(&filesystem);
    assert!(package.contents.healthy_orders.is_empty());
    assert!(
        package
            .contents
            .index
            .stale_reasons
            .contains(&PackageIndexStaleReason::OrderMembershipChanged)
    );
}

#[test]
fn supporting_material() {
    let filesystem = package();
    let supporting = format!("{ORDER}/quote.pdf");
    filesystem.put_file(&supporting, b"Supporting content");
    filesystem.on(Operation::Read).at(&supporting).fails();
    filesystem.put_entry("/package/README.md", FileSystemEntryKind::Directory);
    let package = open(&filesystem);
    assert_eq!(package.contents.healthy_orders.len(), 1);
    assert_eq!(package.readme.text, None);
    assert!(!package.readme.diagnostics.is_empty());
    assert_eq!(
        filesystem.bytes(&supporting),
        Some(b"Supporting content".to_vec())
    );
}

#[test]
fn enumeration_order() {
    fn ordered(names: &[&str]) -> crate::Package {
        let filesystem = minimal_package();
        let mut later: OrderDocument = serde_json::from_slice(MINIMAL_ORDER).unwrap();
        later.order_id = serde_json::from_str(r#""11111111-1111-4111-8111-111111111111""#).unwrap();
        let earlier: OrderDocument = serde_json::from_slice(MINIMAL_ORDER).unwrap();
        let index = Index {
            schema_version: 1,
            orders: vec![
                IndexedOrder {
                    order_directory: "Orders/Earlier".into(),
                    order: earlier.clone(),
                    readme: "".into(),
                },
                IndexedOrder {
                    order_directory: "Orders/Later".into(),
                    order: later.clone(),
                    readme: "".into(),
                },
            ],
            vendors: Vec::new(),
            values: None,
        };
        for (name, order) in [("Earlier", earlier), ("Later", later)] {
            filesystem.put_file(
                format!("/package/Orders/{name}/.spielorders/order.json"),
                &order.to_json_bytes().unwrap(),
            );
        }
        filesystem.put_file(INDEX, &index.to_json_bytes().unwrap());
        enumerate_orders_as(&filesystem, names);
        open(&filesystem)
    }
    assert_eq!(
        ordered(&["Earlier", "Later"]),
        ordered(&["Later", "Earlier"])
    );
}

#[test]
fn record_evidence() {
    for bytes in [
        Some(b"{ invalid".as_slice()),
        Some(b"\xff".as_slice()),
        None,
    ] {
        let filesystem = package();
        if let Some(bytes) = bytes {
            filesystem.put_file(RECORD, bytes);
        } else {
            filesystem.on(Operation::Read).at(RECORD).fails();
        }
        let package = open(&filesystem);
        assert_eq!(package.contents.index.state, PackageIndexState::Stale);
        assert_eq!(package.contents.damaged_orders.len(), 1);
        assert_eq!(
            package.contents.index.stale_reasons,
            [if bytes.is_some() {
                PackageIndexStaleReason::OrderMetadataChanged
            } else {
                PackageIndexStaleReason::ChangeEvidenceUncertain
            }]
        );
    }
}

#[test]
fn changed_reference() {
    let filesystem = package();
    let mut order: serde_json::Value = serde_json::from_slice(FULL_ORDER).unwrap();
    order["vendor_id"] = serde_json::Value::String("11111111-1111-4111-8111-111111111111".into());
    filesystem.put_file(RECORD, &serde_json::to_vec(&order).unwrap());
    let package = open(&filesystem);
    assert_eq!(package.contents.damaged_orders.len(), 1);
    assert_eq!(
        package.contents.index.stale_reasons,
        [PackageIndexStaleReason::OrderMetadataChanged]
    );
}
