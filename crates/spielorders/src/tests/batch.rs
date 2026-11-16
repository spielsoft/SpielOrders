//! Decision sources: Define the Rust CLI capability contract; Durable managed-file writes.
//!
//! A batch plans each change against the Package as the changes before it leave it, and
//! commits every record they touched together, or nothing.

use crate::batch::Batch;
use crate::tests::support::{
    FixedUuid, PlatformFileSystem, created_order, effects, index_is_current, initialized_package,
    snapshot,
};
use crate::{Assignments, Change, OrderRef, Report, commit, metadata::OrderDocument};
use std::path::Path;

fn record(directory: &Path) -> OrderDocument {
    OrderDocument::from_json_bytes(
        &std::fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .unwrap()
}

fn loaded(root: &Path) -> Batch {
    Batch::new(crate::canonical::load_package(root, &PlatformFileSystem).unwrap())
}

fn today() -> crate::ports::LocalDate {
    crate::ports::LocalDate {
        year: 2026,
        month: 11,
        day: 15,
    }
}

/// An edit of the Order at `directory` as `batch` holds it.
fn edit(batch: &Batch, directory: &Path, assignments: Assignments) -> Change {
    let order = batch
        .package()
        .healthy_orders
        .iter()
        .find(|order| order.paths.directory.absolute_path == directory)
        .unwrap();
    Change::SaveOrder {
        order: OrderRef::Directory(directory.to_path_buf()),
        assignments,
        items: order
            .document
            .items
            .iter()
            .map(|item| {
                Assignments::new()
                    .assign("description", item.description.as_str())
                    .assign("quantity", item.quantity.get().to_string())
                    .assign("unit_price", item.unit_price.get().to_string())
            })
            .collect(),
        original: order.document.clone(),
    }
}

#[test]
fn a_batch_commits_every_change_in_order_as_one_write() {
    let (_tree, root) = initialized_package("batch-commit");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let second = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Mounts"),
    );
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    let change = edit(
        &batch,
        &first,
        Assignments::new().assign("title", "Better lenses"),
    );
    batch
        .add(&change, &today, &uuid, &PlatformFileSystem)
        .unwrap();
    let change = edit(&batch, &first, Assignments::new().assign("currency", "USD"));
    batch
        .add(&change, &today, &uuid, &PlatformFileSystem)
        .unwrap();
    batch
        .add(
            &Change::CreateVendor {
                root: root.clone(),
                assignments: Assignments::new().assign("name", "Tarnwick"),
                contacts: Vec::new(),
            },
            &today,
            &uuid,
            &PlatformFileSystem,
        )
        .unwrap();
    let change = edit(
        &batch,
        &second,
        Assignments::new()
            .assign("vendor", "Tarnwick")
            .introduce("project", "Clock"),
    );
    batch
        .add(&change, &today, &uuid, &PlatformFileSystem)
        .unwrap();
    let before = snapshot(&root);

    let plan = crate::change::plan_batch(&batch).result.unwrap();
    assert_eq!(plan.order_count(), 2);
    assert_eq!(snapshot(&root), before, "planning writes nothing");
    let report = commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(uuid())),
    )
    .result;
    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");

    assert_eq!(record(&first).title.as_str(), "Better lenses");
    assert_eq!(record(&first).currency.as_deref(), Some("USD"));
    assert_eq!(
        record(&second)
            .vendor_id
            .map(|id| id.to_string())
            .as_deref(),
        Some("e0990d31-06aa-42e9-97c8-088e3e894822")
    );
    assert_eq!(record(&second).project.as_deref(), Some("Clock"));
    assert!(index_is_current(&root));
}

#[test]
fn a_refused_change_leaves_the_batch_as_it_was() {
    let (_tree, root) = initialized_package("batch-refusal");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    let change = edit(
        &batch,
        &first,
        Assignments::new().assign("title", "Better lenses"),
    );
    batch
        .add(&change, &today, &uuid, &PlatformFileSystem)
        .unwrap();
    let refused = edit(
        &batch,
        &first,
        Assignments::new().assign("project", "Unknown"),
    );
    assert!(
        batch
            .add(&refused, &today, &uuid, &PlatformFileSystem)
            .is_err()
    );
    assert_eq!(batch.len(), 1);
    assert!(matches!(
        batch.add(
            &Change::RemoveOrder {
                order: OrderRef::Directory(first.clone())
            },
            &today,
            &uuid,
            &PlatformFileSystem
        ),
        Err(crate::Failure::NotBatchable)
    ));
}

#[test]
fn a_record_changed_after_the_batch_was_loaded_refuses_the_commit() {
    let (_tree, root) = initialized_package("batch-stale");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    let change = edit(
        &batch,
        &first,
        Assignments::new().assign("title", "Batch title"),
    );
    batch
        .add(&change, &today, &uuid, &PlatformFileSystem)
        .unwrap();
    let plan = crate::change::plan_batch(&batch).result.unwrap();
    crate::tests::support::committed(
        &crate::tests::support::edit_order(
            first.clone(),
            Assignments::new().assign("title", "Someone else's title"),
        ),
        &effects(&PlatformFileSystem),
    );
    let before = snapshot(&root);

    assert!(matches!(
        commit(plan, &effects(&PlatformFileSystem)).result,
        Err(crate::Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_record_that_cannot_be_staged_leaves_every_record_untouched() {
    let (_tree, root) = initialized_package("batch-staging-failure");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let second = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Mounts"),
    );
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    for directory in [&first, &second] {
        let change = edit(
            &batch,
            directory,
            Assignments::new().assign("currency", "USD"),
        );
        batch
            .add(&change, &today, &uuid, &PlatformFileSystem)
            .unwrap();
    }
    let plan = crate::change::plan_batch(&batch).result.unwrap();
    // What an interrupted earlier write leaves at one record's staging name.
    let blocked = [&first, &second]
        .into_iter()
        .map(|directory| directory.join(".spielorders/.order.json.spielorders-stage"))
        .next_back()
        .unwrap();
    std::fs::create_dir(&blocked).unwrap();
    let before = snapshot(&root);

    assert!(commit(plan, &effects(&PlatformFileSystem)).result.is_err());
    assert_eq!(
        snapshot(&root),
        before,
        "no record replaced and no stage left"
    );
}

#[test]
fn retained_stages() {
    use crate::tests::support::{FailedCommit, Operation, ScriptedFileSystem};

    for boundary in [Operation::WriteNew, Operation::ReplaceFile] {
        let (_tree, root) = initialized_package("batch-cleanup");
        let first = created_order(
            &root,
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Lenses"),
        );
        let second = created_order(
            &root,
            "11111111-1111-4111-8111-111111111111",
            Assignments::new().assign("title", "Mounts"),
        );
        let mut batch = loaded(&root);
        let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
        for directory in [&first, &second] {
            let change = edit(
                &batch,
                directory,
                Assignments::new().assign("currency", "USD"),
            );
            batch
                .add(&change, &today, &uuid, &PlatformFileSystem)
                .unwrap();
        }
        let plan = crate::change::plan_batch(&batch).result.unwrap();
        let before = snapshot(&root);
        let first_stage = first.join(".spielorders/.order.json.spielorders-stage");
        let second_stage = second.join(".spielorders/.order.json.spielorders-stage");
        let (failing, retained) = match boundary {
            Operation::WriteNew => (second_stage.clone(), first_stage),
            Operation::ReplaceFile => (first.join(".spielorders/order.json"), second_stage),
            _ => unreachable!(),
        };
        let filesystem = ScriptedFileSystem::new();
        filesystem.on(boundary).at(failing).once().fails();
        filesystem.on(Operation::RemoveFile).at(&retained).fails();

        let failure = commit(plan, &effects(&filesystem)).result.unwrap_err();
        assert!(retained.is_file(), "the failed cleanup leaves its stage");
        assert!(
            failure
                .cleanup_failures()
                .iter()
                .any(|cleanup| { cleanup.location.absolute_path == retained }),
            "every retained stage must be reported: {failure:?}"
        );
        std::fs::remove_file(retained).unwrap();
        assert_eq!(snapshot(&root), before, "canonical records stay unchanged");
    }
}

#[test]
fn a_stage_kept_beside_a_changed_destination_is_reported() {
    use crate::tests::support::{FailedCommit, Operation, ScriptedFileSystem};

    let (_tree, root) = initialized_package("batch-changed-cleanup");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let second = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Mounts"),
    );
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    for directory in [&first, &second] {
        let change = edit(
            &batch,
            directory,
            Assignments::new().assign("currency", "USD"),
        );
        batch
            .add(&change, &today, &uuid, &PlatformFileSystem)
            .unwrap();
    }
    let plan = crate::change::plan_batch(&batch).result.unwrap();
    let second_record = second.join(".spielorders/order.json");
    let second_stage = second.join(".spielorders/.order.json.spielorders-stage");
    let theirs = std::fs::read_to_string(&second_record)
        .unwrap()
        .replace("\"Mounts\"", "\"Their mounts\"");
    let filesystem = ScriptedFileSystem::new();
    // Someone else rewrites the second record while the first is being promoted.
    let written = theirs.clone();
    let target = second_record.clone();
    filesystem
        .on(Operation::ReplaceFile)
        .at(first.join(".spielorders/order.json"))
        .once()
        .followed_by(move |outside| outside.write(&target, written.as_bytes()))
        .proceeds();
    filesystem
        .on(Operation::RemoveFile)
        .at(&second_stage)
        .fails();

    let failure = commit(plan, &effects(&filesystem)).result.unwrap_err();
    assert!(
        second_stage.is_file(),
        "the failed cleanup leaves its stage"
    );
    assert!(
        failure
            .cleanup_failures()
            .iter()
            .any(|cleanup| cleanup.location.absolute_path == second_stage),
        "the stage left beside the changed record must be reported: {failure:?}"
    );
    assert_eq!(std::fs::read_to_string(&second_record).unwrap(), theirs);
    assert_eq!(&*record(&first).title, "Lenses");
}

#[test]
fn a_batch_edits_an_order_from_its_record_rather_than_the_index() {
    let (_tree, root) = initialized_package("batch-record");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    // An index edited by hand after its records: a display may show it, a write may not
    // build on it.
    let index = root.join(".spielorders/index.json");
    let edited = std::fs::read_to_string(&index)
        .unwrap()
        .replace("\"title\": \"Lenses\"", "\"title\": \"Wrong\"");
    std::fs::write(&index, edited).unwrap();
    let mut batch = loaded(&root);
    let uuid = || "e0990d31-06aa-42e9-97c8-088e3e894822".to_owned();
    let change = edit(&batch, &first, Assignments::new().assign("currency", "USD"));
    let before = snapshot(&root);

    assert!(matches!(
        batch.add(&change, &today, &uuid, &PlatformFileSystem),
        Err(crate::Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(batch.len(), 0);
    assert_eq!(snapshot(&root), before);
    assert_eq!(&*record(&first).title, "Lenses");
}
