//! Decision sources: Define the Order record and lifecycle contract; Define durable
//! metadata schemas and version evolution.
//!
//! A Package keeps Ordering Institutions, Orderers, Projects, and each Project's Order
//! Groups beside its Vendors, and offers them whether or not an Order uses them, beside
//! every value its Orders use.

use crate::model::ReusableValueKind;
use crate::tests::support::{
    Index, PlatformFileSystem, canonical_bytes, committed, create_vendor, created_order, effects,
    initialized_package, opened, snapshot,
};
use crate::{Assignments, canonical::ReusableValueProjection};
use serde_json::json;
use std::path::Path;

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const VENDOR_ID: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

/// Has the Package at `root` keep `values`, as a record written by hand would.
fn keep(root: &Path, values: serde_json::Value) {
    let path = root.join(".spielorders/vendors.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["values"] = values;
    std::fs::write(&path, canonical_bytes(&document)).unwrap();
}

fn kept(
    institutions: &[&str],
    orderers: &[&str],
    projects: serde_json::Value,
    groups: &[&str],
) -> serde_json::Value {
    json!({
        "ordering_institutions": institutions,
        "orderers": orderers,
        "projects": projects,
        "order_groups": groups,
    })
}

fn listed(values: &[ReusableValueProjection]) -> Vec<(&str, usize)> {
    values
        .iter()
        .map(|value| (value.value.as_str(), value.usage_count))
        .collect()
}

fn values(root: &Path, kind: ReusableValueKind) -> Vec<(String, usize)> {
    let package = opened(root, &PlatformFileSystem).result.unwrap();
    let values = package.values(kind).result.unwrap();
    listed(&values)
        .into_iter()
        .map(|(value, count)| (value.to_owned(), count))
        .collect()
}

#[test]
fn a_kept_value_is_offered_before_any_order_uses_it() {
    let (_tree, root) = initialized_package("kept-values-unused");
    keep(
        &root,
        kept(
            &["Example Institute"],
            &["Emmy Noether"],
            json!([{ "name": "Telescope", "order_groups": ["Summer build"] }]),
            &["Loose ends"],
        ),
    );

    assert_eq!(
        values(&root, ReusableValueKind::OrderingInstitution),
        [("Example Institute".to_owned(), 0)]
    );
    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("Emmy Noether".to_owned(), 0)]
    );
    assert_eq!(
        values(&root, ReusableValueKind::Project),
        [("Telescope".to_owned(), 0)]
    );
    assert_eq!(
        values(&root, ReusableValueKind::OrderGroup),
        [("Loose ends".to_owned(), 0), ("Summer build".to_owned(), 0)]
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some(" teleSCOPE "))),
        [("Summer build", 0)]
    );
    assert_eq!(
        listed(&package.contents.order_groups_of(None)),
        [("Loose ends", 0)]
    );
}

#[test]
fn an_order_counts_toward_a_kept_value_under_the_kept_spelling() {
    let (_tree, root) = initialized_package("kept-values-spelling");
    created_order(
        &root,
        ORDER_ID,
        Assignments::new()
            .assign("title", "Ruby cell")
            .introduce("orderer", "abc")
            .introduce("project", "telescope")
            .introduce("order_group", "summer BUILD"),
    );
    keep(
        &root,
        kept(
            &[],
            &["AbC"],
            json!([{ "name": "TeleScope", "order_groups": ["Summer build"] }]),
            &[],
        ),
    );

    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("AbC".to_owned(), 1)]
    );
    assert_eq!(
        values(&root, ReusableValueKind::Project),
        [("TeleScope".to_owned(), 1)]
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("TeleScope"))),
        [("Summer build", 1)]
    );
}

#[test]
fn a_package_that_keeps_no_values_offers_those_its_orders_use_and_opens_unchanged() {
    let (_tree, root) = initialized_package("kept-values-absent");
    created_order(
        &root,
        ORDER_ID,
        Assignments::new()
            .assign("title", "Ruby cell")
            .introduce("orderer", "Ada Lovelace")
            .introduce("project", "Telescope")
            .introduce("order_group", "Summer build"),
    );
    let before = snapshot(&root);

    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("Ada Lovelace".to_owned(), 1)]
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("Telescope"))),
        [("Summer build", 1)]
    );
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_value_kept_twice_is_one_value() {
    let (_tree, root) = initialized_package("kept-values-twice");
    keep(
        &root,
        kept(
            &[],
            &["Ada Lovelace", " ada lovelace "],
            json!([
                { "name": "Telescope", "order_groups": ["Summer build"] },
                { "name": "TELESCOPE", "order_groups": ["Winter build", "summer build"] }
            ]),
            &[],
        ),
    );

    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("Ada Lovelace".to_owned(), 0)]
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("Telescope"))),
        [("Summer build", 0), ("Winter build", 0)]
    );
}

#[test]
fn a_vendor_change_keeps_every_kept_value_and_the_index_carries_them() {
    let (_tree, root) = initialized_package("kept-values-vendor-change");
    let values = kept(
        &["Example Institute"],
        &["Emmy Noether"],
        json!([{ "name": "Telescope", "order_groups": ["Summer build"] }]),
        &[],
    );
    keep(&root, values.clone());

    committed(
        &create_vendor(&root, Assignments::new().assign("name", "Tarnwick")),
        &effects(&PlatformFileSystem).minting(crate::tests::support::FixedUuid(VENDOR_ID.into())),
    );

    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join(".spielorders/vendors.json")).unwrap())
            .unwrap();
    assert_eq!(record["values"], values);
    let index =
        Index::from_json_bytes(&std::fs::read(root.join(".spielorders/index.json")).unwrap())
            .unwrap();
    assert_eq!(serde_json::to_value(index.values).unwrap(), values);
    assert_eq!(
        self::values(&root, ReusableValueKind::Orderer),
        [("Emmy Noether".to_owned(), 0)]
    );
}

// Adding, renaming, merging, and deleting values, and merging and replacing Vendors.

const SECOND_ORDER: &str = "11111111-1111-4111-8111-111111111111";
const THIRD_ORDER: &str = "22222222-2222-4222-8222-222222222222";
const NETHERMOOR: &str = "6b0d9e77-2a41-4c8e-9f10-0d3b5a7c2e18";

fn order_with(root: &Path, id: &str, title: &str, facts: &[(&str, &str)]) -> std::path::PathBuf {
    let mut assignments = Assignments::new().assign("title", title);
    for (field, value) in facts {
        assignments = assignments.introduce(*field, *value);
    }
    created_order(root, id, assignments)
}

fn document(directory: &Path) -> crate::metadata::OrderDocument {
    crate::metadata::OrderDocument::from_json_bytes(
        &std::fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .unwrap()
}

fn change(change: crate::Change) -> crate::ChangeResult {
    committed(&change, &effects(&PlatformFileSystem))
}

fn refusal(change: &crate::Change) -> crate::Failure {
    crate::plan(change, &effects(&PlatformFileSystem))
        .result
        .expect_err("the change is refused")
}

fn order_count(change: &crate::Change) -> usize {
    crate::tests::support::planned(change, &effects(&PlatformFileSystem)).order_count()
}

#[test]
fn an_added_value_is_offered_and_adding_it_again_changes_nothing() {
    let (_tree, root) = initialized_package("value-add");
    let add = |kind, project: Option<&str>, value: &str| crate::Change::AddValue {
        root: root.clone(),
        kind,
        project: project.map(str::to_owned),
        value: value.to_owned(),
    };
    change(add(ReusableValueKind::Orderer, None, " Emmy Noether "));
    change(add(ReusableValueKind::Project, None, "Telescope"));
    change(add(
        ReusableValueKind::OrderGroup,
        Some("telescope"),
        "Summer build",
    ));

    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("Emmy Noether".to_owned(), 0)]
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("Telescope"))),
        [("Summer build", 0)]
    );

    let before = snapshot(&root);
    let again = crate::commit(
        crate::tests::support::planned(
            &add(ReusableValueKind::Orderer, None, "emmy noether"),
            &effects(&PlatformFileSystem),
        ),
        &effects(&PlatformFileSystem),
    );
    assert!(matches!(again.result, Ok(crate::Report::Unchanged { .. })));
    assert_eq!(snapshot(&root), before);
    assert!(matches!(
        refusal(&add(ReusableValueKind::OrderGroup, Some("Weather station"), "Vacuum")),
        crate::Failure::UnknownValue { field, value } if field == "project" && value == "Weather station"
    ));
    assert!(matches!(
        refusal(&add(ReusableValueKind::Orderer, None, "  ")),
        crate::Failure::BlankValue { field } if field == "orderer"
    ));
}

#[test]
fn renaming_a_value_changes_every_order_that_uses_it() {
    let (_tree, root) = initialized_package("value-rename");
    let first = order_with(&root, ORDER_ID, "Lenses", &[("orderer", "Ada")]);
    let second = order_with(&root, SECOND_ORDER, "Mounts", &[("orderer", "ada")]);
    let other = order_with(&root, THIRD_ORDER, "Probes", &[("orderer", "Grace")]);
    let rename = crate::Change::RenameValue {
        root: root.clone(),
        kind: ReusableValueKind::Orderer,
        project: None,
        value: "ADA".into(),
        to: "Ada Lovelace".into(),
    };

    assert_eq!(order_count(&rename), 2);
    change(rename);

    assert_eq!(document(&first).orderer.as_deref(), Some("Ada Lovelace"));
    assert_eq!(document(&second).orderer.as_deref(), Some("Ada Lovelace"));
    assert_eq!(document(&other).orderer.as_deref(), Some("Grace"));
    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("Ada Lovelace".to_owned(), 2), ("Grace".to_owned(), 1)]
    );
}

#[test]
fn renaming_onto_an_existing_value_merges_the_two() {
    let (_tree, root) = initialized_package("value-rename-merge");
    let abc = order_with(&root, ORDER_ID, "Cell", &[("orderer", "AbC")]);
    let ab_c = order_with(&root, SECOND_ORDER, "Oven", &[("orderer", "Ab_C")]);
    change(crate::Change::RenameValue {
        root: root.clone(),
        kind: ReusableValueKind::Orderer,
        project: None,
        value: "Ab_C".into(),
        to: "AbC".into(),
    });

    assert_eq!(document(&abc).orderer.as_deref(), Some("AbC"));
    assert_eq!(document(&ab_c).orderer.as_deref(), Some("AbC"));
    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("AbC".to_owned(), 2)]
    );
}

#[test]
fn merging_keeps_the_chosen_spelling_and_counts_the_orders_that_change() {
    let (_tree, root) = initialized_package("value-merge");
    let abc = order_with(&root, ORDER_ID, "Cell", &[("orderer", "AbC")]);
    let ab_c = order_with(&root, SECOND_ORDER, "Oven", &[("orderer", "Ab_C")]);
    let ab_c_too = order_with(&root, THIRD_ORDER, "Lamp", &[("orderer", "Ab_C")]);
    let merge = crate::Change::MergeValues {
        root: root.clone(),
        kind: ReusableValueKind::Orderer,
        project: None,
        values: vec!["AbC".into(), "Ab_C".into()],
        keep: "AbC".into(),
    };

    assert_eq!(order_count(&merge), 2);
    let crate::ChangeResult::Values(result) = change(merge) else {
        panic!("a merge reports a value change")
    };
    assert_eq!(result.orders, 2);

    for directory in [&abc, &ab_c, &ab_c_too] {
        assert_eq!(document(directory).orderer.as_deref(), Some("AbC"));
    }
    assert_eq!(
        values(&root, ReusableValueKind::Orderer),
        [("AbC".to_owned(), 3)]
    );
}

#[test]
fn a_merge_keeps_a_value_the_package_has_in_the_first_spelling() {
    let (_tree, root) = initialized_package("value-merge-keep");
    let ada = order_with(&root, ORDER_ID, "Cell", &[("orderer", "Ada")]);
    let grace = order_with(&root, SECOND_ORDER, "Oven", &[("orderer", "Grace")]);
    let merge = |keep: &str| crate::Change::MergeValues {
        root: root.clone(),
        kind: ReusableValueKind::Orderer,
        project: None,
        values: vec![keep.into(), "Grace".into()],
        keep: keep.into(),
    };

    let before = snapshot(&root);
    assert!(matches!(
        crate::plan(&merge("Nobody"), &effects(&PlatformFileSystem)).result,
        Err(crate::Failure::UnknownValue { .. })
    ));
    assert_eq!(snapshot(&root), before);

    change(merge("ADA"));
    assert_eq!(document(&ada).orderer.as_deref(), Some("ADA"));
    assert_eq!(document(&grace).orderer.as_deref(), Some("ADA"));
}

#[test]
fn a_used_value_is_deleted_only_with_a_replacement_or_cleared() {
    let (_tree, root) = initialized_package("value-delete");
    let first = order_with(
        &root,
        ORDER_ID,
        "Lenses",
        &[("ordering_institution", "Old Lab")],
    );
    let second = order_with(
        &root,
        SECOND_ORDER,
        "Mounts",
        &[("ordering_institution", "New Lab")],
    );
    let delete = |value: &str, in_use| crate::Change::DeleteValue {
        root: root.clone(),
        kind: ReusableValueKind::OrderingInstitution,
        project: None,
        value: value.to_owned(),
        in_use,
    };

    let before = snapshot(&root);
    let crate::Failure::ValueInUse { field, orders, .. } =
        refusal(&delete("old lab", crate::InUse::Refuse))
    else {
        panic!("a used value is refused")
    };
    assert_eq!(field, "ordering_institution");
    assert_eq!(orders.len(), 1);
    assert_eq!(snapshot(&root), before);
    assert!(matches!(
        refusal(&delete("Old Lab", crate::InUse::Replace("Nowhere".into()))),
        crate::Failure::UnknownValue { .. }
    ));

    change(delete("Old Lab", crate::InUse::Replace("new lab".into())));
    assert_eq!(
        document(&first).ordering_institution.as_deref(),
        Some("New Lab")
    );
    change(delete("New Lab", crate::InUse::Clear));
    assert_eq!(document(&first).ordering_institution, None);
    assert_eq!(document(&second).ordering_institution, None);
    assert!(values(&root, ReusableValueKind::OrderingInstitution).is_empty());

    change(crate::Change::AddValue {
        root: root.clone(),
        kind: ReusableValueKind::OrderingInstitution,
        project: None,
        value: "Spare Lab".into(),
    });
    change(delete("Spare Lab", crate::InUse::Refuse));
    assert!(values(&root, ReusableValueKind::OrderingInstitution).is_empty());
}

#[test]
fn a_project_keeps_its_groups_when_renamed_or_cleared() {
    let (_tree, root) = initialized_package("value-project-groups");
    let order = order_with(
        &root,
        ORDER_ID,
        "Chamber",
        &[("project", "Clock"), ("order_group", "Vacuum")],
    );
    change(crate::Change::AddValue {
        root: root.clone(),
        kind: ReusableValueKind::OrderGroup,
        project: Some("Clock".into()),
        value: "Spares".into(),
    });
    change(crate::Change::RenameValue {
        root: root.clone(),
        kind: ReusableValueKind::Project,
        project: None,
        value: "Clock".into(),
        to: "Telescope".into(),
    });

    assert_eq!(document(&order).project.as_deref(), Some("Telescope"));
    assert_eq!(document(&order).order_group.as_deref(), Some("Vacuum"));
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("Telescope"))),
        [("Spares", 0), ("Vacuum", 1)]
    );
    assert!(package.contents.order_groups_of(Some("Clock")).is_empty());

    change(crate::Change::DeleteValue {
        root: root.clone(),
        kind: ReusableValueKind::Project,
        project: None,
        value: "Telescope".into(),
        in_use: crate::InUse::Clear,
    });
    assert_eq!(document(&order).project, None);
    assert_eq!(document(&order).order_group.as_deref(), Some("Vacuum"));
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(None)),
        [("Spares", 0), ("Vacuum", 1)]
    );
}

#[test]
fn merging_projects_keeps_the_kept_projects_group_spelling_on_every_order() {
    let (_tree, root) = initialized_package("value-project-merge-groups");
    let alpha = order_with(
        &root,
        ORDER_ID,
        "Chamber",
        &[("project", "Alpha"), ("order_group", "lab")],
    );
    let beta = order_with(
        &root,
        SECOND_ORDER,
        "Oven",
        &[("project", "Beta"), ("order_group", "Lab")],
    );
    change(crate::Change::MergeValues {
        root: root.clone(),
        kind: ReusableValueKind::Project,
        project: None,
        values: vec!["Beta".into(), "Alpha".into()],
        keep: "Beta".into(),
    });

    for directory in [&alpha, &beta] {
        let order = document(directory);
        assert_eq!(
            (order.project.as_deref(), order.order_group.as_deref()),
            (Some("Beta"), Some("Lab"))
        );
    }
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        listed(&package.contents.order_groups_of(Some("Beta"))),
        [("Lab", 2)]
    );
}

#[test]
fn an_order_group_change_stays_within_its_project() {
    let (_tree, root) = initialized_package("value-group-scope");
    let clock = order_with(
        &root,
        ORDER_ID,
        "Chamber",
        &[("project", "Clock"), ("order_group", "Vacuum")],
    );
    let atoms = order_with(
        &root,
        SECOND_ORDER,
        "Gauges",
        &[("project", "Atoms"), ("order_group", "Vacuum")],
    );
    change(crate::Change::RenameValue {
        root: root.clone(),
        kind: ReusableValueKind::OrderGroup,
        project: Some("clock".into()),
        value: "Vacuum".into(),
        to: "Pumps".into(),
    });

    assert_eq!(document(&clock).order_group.as_deref(), Some("Pumps"));
    assert_eq!(document(&atoms).order_group.as_deref(), Some("Vacuum"));
}

#[test]
fn a_change_to_an_order_after_planning_refuses_the_commit() {
    let (_tree, root) = initialized_package("value-stale");
    let order = order_with(&root, ORDER_ID, "Lenses", &[("orderer", "Ada")]);
    let plan = crate::tests::support::planned(
        &crate::Change::RenameValue {
            root: root.clone(),
            kind: ReusableValueKind::Orderer,
            project: None,
            value: "Ada".into(),
            to: "Ada Lovelace".into(),
        },
        &effects(&PlatformFileSystem),
    );
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("title", "Better lenses"),
        ),
        &effects(&PlatformFileSystem),
    );
    let before = snapshot(&root);

    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Err(crate::Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(snapshot(&root), before);
}

#[test]
fn merging_vendors_repoints_their_orders_and_keeps_their_contacts() {
    let (_tree, root) = initialized_package("vendor-merge");
    let tarnwick = crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    let nethermoor = crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    committed(
        &crate::tests::support::add_contact(
            root.clone(),
            crate::VendorRef::Name("Nethermoor".into()),
            Assignments::new().assign("name", "Grace Hopper"),
        ),
        &effects(&PlatformFileSystem),
    );
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    );
    let merge = crate::Change::MergeVendors {
        root: root.clone(),
        vendors: vec![
            crate::VendorRef::Id(tarnwick.clone()),
            crate::VendorRef::Id(nethermoor),
        ],
        keep: crate::VendorRef::Name("Tarnwick".into()),
    };

    assert_eq!(order_count(&merge), 1);
    change(merge);

    assert_eq!(
        document(&order).vendor_id.map(|id| id.to_string()),
        Some(tarnwick)
    );
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    let [vendor] = package.contents.vendors.as_slice() else {
        panic!("one Vendor remains")
    };
    assert_eq!(vendor.name.as_str(), "Tarnwick");
    assert_eq!(vendor.contacts[0].name.as_str(), "Grace Hopper");
}

#[test]
fn a_removed_order_still_restores_after_its_vendor_is_merged_away() {
    let (_tree, root) = initialized_package("vendor-merge-removed");
    let tarnwick = crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    );
    committed(
        &crate::Change::RemoveOrder {
            order: crate::OrderRef::Directory(order.clone()),
        },
        &effects(&PlatformFileSystem),
    );
    let removed = root.join("Removed").join(order.file_name().unwrap());
    change(crate::Change::MergeVendors {
        root: root.clone(),
        vendors: vec![
            crate::VendorRef::Name("Tarnwick".into()),
            crate::VendorRef::Name("Nethermoor".into()),
        ],
        keep: crate::VendorRef::Name("Tarnwick".into()),
    });

    change(crate::Change::RestoreOrder {
        root: root.clone(),
        removed,
    });
    assert_eq!(
        document(&order).vendor_id.map(|id| id.to_string()),
        Some(tarnwick)
    );
}

#[test]
fn a_vendor_a_removed_order_names_is_deleted_only_with_a_replacement_or_cleared() {
    let (_tree, root) = initialized_package("vendor-delete-removed");
    crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let lenses = order_with(&root, ORDER_ID, "Lenses", &[]);
    let mounts = order_with(&root, SECOND_ORDER, "Mounts", &[]);
    for (order, vendor) in [(&lenses, "Nethermoor"), (&mounts, "Tarnwick")] {
        committed(
            &crate::tests::support::edit_order(
                order.clone(),
                Assignments::new().assign("vendor", vendor),
            ),
            &effects(&PlatformFileSystem),
        );
        committed(
            &crate::Change::RemoveOrder {
                order: crate::OrderRef::Directory(order.clone()),
            },
            &effects(&PlatformFileSystem),
        );
    }
    let removed = |order: &Path| root.join("Removed").join(order.file_name().unwrap());
    // The removed Mounts record no longer validates, but still names its Vendor.
    let record = removed(&mounts).join(".spielorders/order.json");
    let mut damaged: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    damaged["title"] = serde_json::Value::Null;
    std::fs::write(&record, serde_json::to_vec(&damaged).unwrap()).unwrap();
    let named = |failure: crate::Failure| match failure {
        crate::Failure::VendorReferenced { orders, .. } => orders
            .iter()
            .map(|order| order.absolute_path.clone())
            .collect::<Vec<_>>(),
        other => panic!("refused as {other:?}"),
    };

    let refused = [
        crate::Change::DeleteVendor {
            root: root.clone(),
            vendor: crate::VendorRef::Name("Nethermoor".into()),
            in_use: crate::InUse::Refuse,
        },
        crate::Change::DeleteVendor {
            root: root.clone(),
            vendor: crate::VendorRef::Name("Tarnwick".into()),
            in_use: crate::InUse::Clear,
        },
        crate::Change::MergeVendors {
            root: root.clone(),
            vendors: vec![
                crate::VendorRef::Name("Nethermoor".into()),
                crate::VendorRef::Name("Tarnwick".into()),
            ],
            keep: crate::VendorRef::Name("Nethermoor".into()),
        },
    ]
    .map(|change| named(refusal(&change)));
    assert_eq!(
        refused,
        [
            vec![removed(&lenses)],
            vec![removed(&mounts)],
            vec![removed(&mounts)],
        ]
    );
}

/// An Order record of a version this build cannot read still names its Vendor as stored,
/// so no deletion or merge takes that Vendor away from it.
#[test]
fn a_vendor_an_unsupported_order_record_names_is_neither_deleted_nor_merged_away() {
    let (_tree, root) = initialized_package("vendor-delete-unsupported");
    crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    );
    let record = order.join(".spielorders/order.json");
    let mut future: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    future["schema_version"] = serde_json::json!(2);
    std::fs::write(&record, serde_json::to_vec(&future).unwrap()).unwrap();
    let delete = |in_use| crate::Change::DeleteVendor {
        root: root.clone(),
        vendor: crate::VendorRef::Name("Nethermoor".into()),
        in_use,
    };

    for change in [
        delete(crate::InUse::Refuse),
        delete(crate::InUse::Replace(crate::VendorRef::Name(
            "Tarnwick".into(),
        ))),
        delete(crate::InUse::Clear),
        crate::Change::MergeVendors {
            root: root.clone(),
            vendors: vec![
                crate::VendorRef::Name("Tarnwick".into()),
                crate::VendorRef::Name("Nethermoor".into()),
            ],
            keep: crate::VendorRef::Name("Tarnwick".into()),
        },
    ] {
        match refusal(&change) {
            crate::Failure::VendorReferenced { orders, .. } => assert_eq!(
                orders
                    .iter()
                    .map(|order| order.absolute_path.clone())
                    .collect::<Vec<_>>(),
                vec![order.clone()],
                "{change:?}"
            ),
            other => panic!("{change:?} refused as {other:?}"),
        }
    }
}

/// A Removed Orders Directory that cannot be listed, or a removed Order record that cannot
/// be read, might name any Vendor, so no deletion or merge proceeds as though it named
/// none.
#[test]
fn an_unreadable_removed_order_refuses_vendor_deletion_and_merge() {
    use crate::tests::support::{Operation, ScriptedFileSystem, failed_path};
    let (_tree, root) = initialized_package("vendor-delete-unreadable-removed");
    crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    );
    committed(
        &crate::Change::RemoveOrder {
            order: crate::OrderRef::Directory(order.clone()),
        },
        &effects(&PlatformFileSystem),
    );
    let removed = root.join("Removed");
    let record = removed
        .join(order.file_name().unwrap())
        .join(".spielorders/order.json");
    let delete = |in_use| crate::Change::DeleteVendor {
        root: root.clone(),
        vendor: crate::VendorRef::Name("Nethermoor".into()),
        in_use,
    };

    for (operation, unreadable, denied) in [
        (Operation::ReadDirectory, &removed, false),
        (Operation::Read, &record, false),
        (Operation::Read, &record, true),
    ] {
        let filesystem = if denied {
            ScriptedFileSystem::new().denying(operation, unreadable)
        } else {
            ScriptedFileSystem::new().failing(operation, unreadable)
        };
        for change in [
            delete(crate::InUse::Refuse),
            delete(crate::InUse::Replace(crate::VendorRef::Name(
                "Tarnwick".into(),
            ))),
            delete(crate::InUse::Clear),
            crate::Change::MergeVendors {
                root: root.clone(),
                vendors: vec![
                    crate::VendorRef::Name("Tarnwick".into()),
                    crate::VendorRef::Name("Nethermoor".into()),
                ],
                keep: crate::VendorRef::Name("Tarnwick".into()),
            },
        ] {
            let refusal = crate::plan(&change, &effects(&filesystem))
                .result
                .expect_err("an unreadable removed Order refuses the change");
            let crate::Failure::RemovedOrderUnreadable(read) = &refusal else {
                panic!("{change:?} refuses for the unreadable removed Order: {refusal:?}")
            };
            let failure = match read.as_ref() {
                crate::Failure::PackageAccessDenied(failure) if denied => failure,
                crate::Failure::PackageOperationFailed(failure) if !denied => failure,
                other => panic!("{change:?} names the read that failed: {other:?}"),
            };
            assert_eq!(
                failed_path(&failure.path),
                unreadable.as_path(),
                "{change:?}"
            );
        }
    }
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(package.contents.vendors.len(), 2);
}

#[test]
fn linked_removed_metadata_refuses_vendor_changes_without_following_or_replacing_it() {
    use crate::tests::support::{TemporaryTree, failed_path};
    use std::{fs, os::unix::fs::symlink};

    for linked_part in ["record", "metadata", "directory"] {
        let (_tree, root) = initialized_package("vendor-linked-removed");
        crate::tests::support::created_vendor(&root, "North Supplies", VENDOR_ID);
        crate::tests::support::created_vendor(&root, "South Supplies", NETHERMOOR);
        let order = order_with(&root, ORDER_ID, "Lenses", &[]);
        committed(
            &crate::tests::support::edit_order(
                order.clone(),
                Assignments::new().assign("vendor", "South Supplies"),
            ),
            &effects(&PlatformFileSystem),
        );
        committed(
            &crate::Change::RemoveOrder {
                order: crate::OrderRef::Directory(order.clone()),
            },
            &effects(&PlatformFileSystem),
        );
        let removed = root.join("Removed").join(order.file_name().unwrap());
        let linked = match linked_part {
            "record" => removed.join(".spielorders/order.json"),
            "metadata" => removed.join(".spielorders"),
            "directory" => removed,
            _ => unreachable!(),
        };
        let outside = TemporaryTree::new("vendor-linked-removed-outside");
        let target = outside.path().join("metadata");
        fs::rename(&linked, &target).unwrap();
        symlink(&target, &linked).unwrap();
        let before = snapshot(&root);
        let outside_before = snapshot(outside.path());
        let delete = |in_use| crate::Change::DeleteVendor {
            root: root.clone(),
            vendor: crate::VendorRef::Name("South Supplies".into()),
            in_use,
        };

        for change in [
            delete(crate::InUse::Refuse),
            delete(crate::InUse::Clear),
            delete(crate::InUse::Replace(crate::VendorRef::Name(
                "North Supplies".into(),
            ))),
            crate::Change::MergeVendors {
                root: root.clone(),
                vendors: vec![
                    crate::VendorRef::Name("North Supplies".into()),
                    crate::VendorRef::Name("South Supplies".into()),
                ],
                keep: crate::VendorRef::Name("North Supplies".into()),
            },
        ] {
            let failure = refusal(&change);
            let crate::Failure::RemovedOrderUnreadable(read) = failure else {
                panic!("linked removed metadata refuses the change: {change:?}: {failure:?}")
            };
            let crate::Failure::PackageOperationFailed(failed) = *read else {
                panic!("the linked metadata is named as unreadable")
            };
            assert_eq!(failed_path(&failed.path), linked);
            assert_eq!(fs::read_link(&linked).unwrap(), target);
            assert_eq!(snapshot(&root), before);
            assert_eq!(snapshot(outside.path()), outside_before);
        }
    }
}

#[test]
fn a_vendor_deletion_refuses_an_order_that_names_it_after_planning() {
    let (_tree, root) = initialized_package("vendor-delete-late-reference");
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let effects = effects(&PlatformFileSystem);
    let plan = crate::plan(
        &crate::Change::DeleteVendor {
            root: root.clone(),
            vendor: crate::VendorRef::Name("Nethermoor".into()),
            in_use: crate::InUse::Refuse,
        },
        &effects,
    )
    .result
    .expect("an unused Vendor's deletion is planned");
    // After planning, an Order comes to name the Vendor and is removed.
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects,
    );
    committed(
        &crate::Change::RemoveOrder {
            order: crate::OrderRef::Directory(order),
        },
        &effects,
    );

    crate::commit(plan, &effects)
        .result
        .expect_err("the removed Order still names the Vendor");
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(package.contents.vendors.len(), 1);
}

#[test]
fn a_used_vendor_is_deleted_with_a_replacement_or_cleared() {
    let (_tree, root) = initialized_package("vendor-delete-replacing");
    let tarnwick = crate::tests::support::created_vendor(&root, "Tarnwick", VENDOR_ID);
    crate::tests::support::created_vendor(&root, "Nethermoor", NETHERMOOR);
    let order = order_with(&root, ORDER_ID, "Lenses", &[]);
    committed(
        &crate::tests::support::edit_order(
            order.clone(),
            Assignments::new().assign("vendor", "Nethermoor"),
        ),
        &effects(&PlatformFileSystem),
    );

    change(crate::Change::DeleteVendor {
        root: root.clone(),
        vendor: crate::VendorRef::Name("Nethermoor".into()),
        in_use: crate::InUse::Replace(crate::VendorRef::Name("Tarnwick".into())),
    });
    assert_eq!(
        document(&order).vendor_id.map(|id| id.to_string()),
        Some(tarnwick)
    );
    change(crate::Change::DeleteVendor {
        root: root.clone(),
        vendor: crate::VendorRef::Name("Tarnwick".into()),
        in_use: crate::InUse::Clear,
    });
    assert_eq!(document(&order).vendor_id, None);
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert!(package.contents.vendors.is_empty());
}

#[test]
fn failed_order_publication_rolls_back_its_values_without_overwriting_a_later_writer() {
    use crate::tests::support::{FixedUuid, Operation, ScriptedFileSystem};
    for adoption in [false, true] {
        for concurrent_writer in [false, true] {
            let (_tree, root) = initialized_package("order-values-publication");
            let assignments = Assignments::new()
                .assign("title", "Lenses")
                .assign("date_ordered", "2026-11-13")
                .assign("status", "ordered")
                .introduce("orderer", "Ada");
            let change = if adoption {
                let directory = root.join("Orders/Legacy");
                std::fs::create_dir(&directory).unwrap();
                std::fs::write(directory.join("quote.txt"), "Supporting text").unwrap();
                crate::Change::AdoptOrder {
                    root: root.clone(),
                    directory,
                    assignments,
                    items: Vec::new(),
                }
            } else {
                crate::Change::CreateOrder {
                    root: root.clone(),
                    assignments,
                    items: Vec::new(),
                }
            };
            let plan = crate::tests::support::planned(&change, &effects(&PlatformFileSystem));
            let before = snapshot(&root);
            let vendors = root.join(".spielorders/vendors.json");
            let mut concurrent: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&vendors).unwrap()).unwrap();
            concurrent["values"] = kept(&[], &["Ada", "Grace"], json!([]), &[]);
            let concurrent = canonical_bytes(&concurrent);
            let filesystem = ScriptedFileSystem::new();
            let script = if adoption {
                filesystem
                    .on(Operation::LinkNewFile)
                    .ending_in(".spielorders/order.json")
            } else {
                filesystem.on(Operation::RenameNew)
            };
            if concurrent_writer {
                let vendors = vendors.clone();
                let bytes = concurrent.clone();
                script
                    .once()
                    .preceded_by(move |outside| outside.write(&vendors, &bytes))
                    .fails();
            } else {
                script.once().fails();
            }
            let result = crate::commit(
                plan,
                &effects(&filesystem).minting(FixedUuid(ORDER_ID.into())),
            )
            .result;
            if concurrent_writer {
                assert!(matches!(
                    result,
                    Err(crate::Failure::CommitFailedCleanupIncomplete { .. })
                ));
                assert_eq!(std::fs::read(&vendors).unwrap(), concurrent);
                assert!(
                    values(&root, ReusableValueKind::Orderer)
                        .iter()
                        .any(|(name, _)| name == "Grace")
                );
            } else {
                assert!(matches!(result, Err(crate::Failure::CommitFailed { .. })));
                assert_eq!(snapshot(&root), before);
            }
            if adoption {
                let directory = root.join("Orders/Legacy");
                assert!(!directory.join(".spielorders").exists());
                assert_eq!(
                    std::fs::read_to_string(directory.join("quote.txt")).unwrap(),
                    "Supporting text"
                );
            }
        }
    }
}

#[test]
fn a_merged_vendor_fills_only_what_the_kept_one_lacks_in_list_order() {
    let (_tree, root) = initialized_package("vendor-merge-fill");
    let vendor = |name: &str, id: &str, facts: &[(&str, &str)], contact: Option<(&str, &str)>| {
        crate::tests::support::created_vendor(&root, name, id);
        let mut assignments = Assignments::new();
        for (field, value) in facts {
            assignments = assignments.assign(*field, *value);
        }
        committed(
            &crate::tests::support::edit_vendor(
                root.clone(),
                crate::VendorRef::Name(name.into()),
                assignments,
            ),
            &effects(&PlatformFileSystem),
        );
        if let Some((contact, role)) = contact {
            committed(
                &crate::tests::support::add_contact(
                    root.clone(),
                    crate::VendorRef::Name(name.into()),
                    Assignments::new()
                        .assign("name", contact)
                        .assign("role", role),
                ),
                &effects(&PlatformFileSystem),
            );
        }
    };
    vendor(
        "Tarnwick",
        VENDOR_ID,
        &[("website", "https://example.com")],
        Some(("Grace Hopper", "")),
    );
    vendor(
        "Thor Labs",
        NETHERMOOR,
        &[("website", "https://example.org"), ("phone", "+1 555 0100")],
        Some(("grace hopper", "Sales")),
    );
    vendor(
        "Tarnwick Inc",
        "33333333-3333-4333-8333-333333333333",
        &[("phone", "+1 555 0199"), ("email", "sales@example.com")],
        Some(("Ada Lovelace", "Accounts")),
    );

    change(crate::Change::MergeVendors {
        root: root.clone(),
        vendors: vec![
            crate::VendorRef::Name("Thor Labs".into()),
            crate::VendorRef::Name("Tarnwick Inc".into()),
        ],
        keep: crate::VendorRef::Name("Tarnwick".into()),
    });

    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    let [kept] = package.contents.vendors.as_slice() else {
        panic!("one Vendor remains")
    };
    assert_eq!(kept.name.as_str(), "Tarnwick");
    assert_eq!(kept.website.as_deref(), Some("https://example.com"));
    assert_eq!(kept.phone.as_deref(), Some("+1 555 0100"));
    assert_eq!(kept.email.as_deref(), Some("sales@example.com"));
    let contacts = kept
        .contacts
        .iter()
        .map(|contact| (contact.name.as_str(), contact.role.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        contacts,
        [
            ("Grace Hopper", Some("Sales")),
            ("Ada Lovelace", Some("Accounts"))
        ]
    );
}
