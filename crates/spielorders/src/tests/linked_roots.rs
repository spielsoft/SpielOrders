//! Decision sources: Define the Rust CLI capability contract; Define integrity,
//! validation, and repair boundaries.
//!
//! A root a caller names that is a link is refused, and nothing behind it is used.

use crate::Failure;
use crate::repair_cases::RepairAction;
use crate::tests::support::{
    FixedCalendar, UNCANCELLED, adoption, align_order, create_order, create_vendor, edit_order,
    edit_vendor, effects, failed_path, minimal_package, order_scope, package_scope, rebuild_index,
    remove_order, repair, save_order, save_vendor, set_vendor_hidden, write_readme,
};
use crate::{Assignments, Change, OrderRef, Scope, VendorRef};
use std::path::PathBuf;

use crate::readme_writes::ReadmeOriginal;

use crate::{diagnose, open, read_order, read_readme, read_vendor, validate};
use std::path::Path;

/// Every capability that takes a root or an Order Directory refuses one that is a link,
/// before it opens anything behind it.
///
/// The link leads to a healthy Package, so a capability that followed it would find one
/// there and answer: refusing the root as a link is the only way to pass. Every change
/// also carries input the core would refuse on its own, so the link is refused before the
/// change's input is judged.
#[test]
fn every_capability_refuses_a_linked_root_before_opening_anything_behind_it() {
    let filesystem = minimal_package().with_link("/linked", "/package");
    let fs = &filesystem;
    let linked = Path::new("/linked");
    let calendar = FixedCalendar::on(2026, 9, 1);
    let original = ReadmeOriginal::Absent;

    let reader = effects(fs);
    let package = || Scope::Package(PathBuf::from(linked));
    let order = || Scope::Order(OrderRef::Directory(PathBuf::from(linked)));
    let refusals = [
        ("open", open(linked, &reader, &UNCANCELLED).result.err()),
        (
            "Package README",
            read_readme(&package(), &reader).result.err(),
        ),
        (
            "validate",
            validate(&package(), &reader, &UNCANCELLED).result.err(),
        ),
        (
            "diagnose Package",
            diagnose(&package(), &reader, &UNCANCELLED).result.err(),
        ),
        (
            "validate Order",
            validate(&order(), &reader, &UNCANCELLED).result.err(),
        ),
        (
            "diagnose Order",
            diagnose(&order(), &reader, &UNCANCELLED).result.err(),
        ),
        ("Order README", read_readme(&order(), &reader).result.err()),
        (
            "read Order",
            read_order(&OrderRef::Directory(PathBuf::from(linked)), &reader)
                .result
                .err(),
        ),
        (
            "read Vendor",
            read_vendor(linked, &VendorRef::Name("Tarnwick".into()), &reader)
                .result
                .err(),
        ),
        (
            "initialize",
            crate::plan(
                &Change::InitializePackage {
                    root: PathBuf::from(linked),
                    orders_path: Some("/outside".into()),
                    removed_path: None,
                },
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "rebuild index",
            crate::plan(&rebuild_index(PathBuf::from(linked)), &effects(fs))
                .result
                .err(),
        ),
        (
            "create Order",
            crate::plan(
                &create_order(
                    PathBuf::from(linked),
                    Assignments::new().assign("colour", "red"),
                ),
                &effects(fs).dated(&calendar),
            )
            .result
            .err(),
        ),
        (
            "adopt Order",
            crate::plan(
                &adoption(
                    PathBuf::from(linked),
                    PathBuf::from(&linked.join("Orders/Candidate")),
                    &(
                        Assignments::new()
                            .assign("title", "Optics")
                            .assign("date_ordered", "2026-08-24")
                            .assign("status", "hold"),
                        vec![Assignments::new().assign("quantity", "lots")],
                    ),
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "edit Order",
            crate::plan(
                &edit_order(
                    PathBuf::from(linked),
                    Assignments::new().assign("title", " "),
                ),
                &effects(fs).dated(&calendar),
            )
            .result
            .err(),
        ),
        (
            "save Order",
            crate::plan(
                &save_order(
                    PathBuf::from(linked),
                    serde_json::from_value(serde_json::json!({
                        "schema_version": 1,
                        "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
                        "title": "Optics",
                        "date_ordered": "2026-08-24",
                        "status": "hold",
                        "items": [],
                    }))
                    .expect("an Order record"),
                    Assignments::new().assign("colour", "red"),
                    vec![Assignments::new().assign("quantity", "lots")],
                ),
                &effects(fs).dated(&calendar),
            )
            .result
            .err(),
        ),
        (
            "align Order",
            crate::plan(&align_order(PathBuf::from(linked)), &effects(fs))
                .result
                .err(),
        ),
        (
            "remove Order",
            crate::plan(&remove_order(PathBuf::from(linked)), &effects(fs))
                .result
                .err(),
        ),
        (
            "create Vendor",
            crate::plan(
                &create_vendor(
                    PathBuf::from(linked),
                    Assignments::new().assign("name", " "),
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "edit Vendor",
            crate::plan(
                &edit_vendor(
                    PathBuf::from(linked),
                    VendorRef::Name("Nonesuch Optics".into()),
                    Assignments::new().assign("vendor_id", "x"),
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "save Vendor",
            crate::plan(
                &save_vendor(
                    PathBuf::from(linked),
                    serde_json::from_value(serde_json::json!({
                        "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
                        "name": "Tarnwick",
                        "contacts": [],
                    }))
                    .expect("a Vendor record"),
                    Assignments::new().assign("vendor_id", "x"),
                    vec![Assignments::new().assign("role", "Sales")],
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "hide Vendor",
            crate::plan(
                &set_vendor_hidden(
                    PathBuf::from(linked),
                    VendorRef::Name("Nonesuch Optics".into()),
                    true,
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "write Package README",
            crate::plan(
                &write_readme(
                    package_scope(PathBuf::from(linked)),
                    original.clone(),
                    "# Notes\n",
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "write Order README",
            crate::plan(
                &write_readme(
                    order_scope(PathBuf::from(linked)),
                    original.clone(),
                    "# Notes\n",
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "repair Package",
            crate::plan(
                &repair(
                    package_scope(PathBuf::from(linked)),
                    "case",
                    RepairAction::AssignExistingVendor,
                    Assignments::new(),
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
        (
            "repair Order",
            crate::plan(
                &repair(
                    order_scope(PathBuf::from(linked)),
                    "case",
                    RepairAction::ClearVendorReference,
                    Assignments::new().assign("vendor", "Tarnwick"),
                ),
                &effects(fs),
            )
            .result
            .err(),
        ),
    ];

    for (capability, refusal) in refusals {
        assert!(
            matches!(&refusal, Some(Failure::RootIsALink { root }) if failed_path(root) == linked),
            "{capability} refuses the linked root as one: {refusal:?}"
        );
    }
    assert_eq!(filesystem.effects(), [], "nothing is written");
}
