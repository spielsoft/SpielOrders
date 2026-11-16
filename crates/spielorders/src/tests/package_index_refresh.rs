//! Decision sources: Define durable metadata schemas and version evolution; Durable
//! managed-file writes.
//!
//! After a committed change the derived index is updated from the records the change
//! wrote, and it is exactly what a full rebuild of the same Package writes, current for
//! the next open.

use crate::tests::support::{
    FixedUuid, PlatformFileSystem, align_order, committed, created_order, edit_order, effects,
    index_is_current, initialized_package, opened, order_scope, rebuild_index, remove_order,
    write_readme,
};
use crate::{
    Assignments, Change, ChangeResult, model::ReusableValueKind, readme_writes::ReadmeOriginal,
};
use std::path::Path;

/// The index as stored, and as a rebuild from every record writes it.
fn stored_and_rebuilt(root: &Path) -> (Vec<u8>, Vec<u8>) {
    let path = root.join(".spielorders/index.json");
    let stored = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    committed(&rebuild_index(root), &effects(&PlatformFileSystem));
    (stored, std::fs::read(&path).unwrap())
}

fn assert_refreshed(root: &Path, after: &str) {
    assert!(index_is_current(root), "the index is current after {after}");
    let (stored, rebuilt) = stored_and_rebuilt(root);
    assert_eq!(
        String::from_utf8(stored).unwrap(),
        String::from_utf8(rebuilt).unwrap(),
        "the index after {after} is what a rebuild writes"
    );
}

#[test]
fn every_committed_change_leaves_the_index_a_rebuild_would_write() {
    let (_tree, root) = initialized_package("index-refresh");
    let first = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Lenses")
            .introduce("orderer", "Ada")
            .introduce("project", "Clock")
            .introduce("order_group", "Vacuum"),
    );
    assert_refreshed(&root, "a creation that adds values");
    let second = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Mounts")
            .assign("orderer", "ada"),
    );
    assert_refreshed(&root, "a second creation");

    committed(
        &edit_order(
            first.clone(),
            Assignments::new().assign("title", "Better lenses"),
        ),
        &effects(&PlatformFileSystem),
    );
    assert_refreshed(&root, "an edit");

    committed(
        &write_readme(
            order_scope(first.clone()),
            ReadmeOriginal::Bytes(std::fs::read(first.join("README.md")).unwrap()),
            "Quote attached\n",
        ),
        &effects(&PlatformFileSystem),
    );
    assert_refreshed(&root, "an Order README write");

    let ChangeResult::AlignOrder(aligned) =
        committed(&align_order(first.clone()), &effects(&PlatformFileSystem))
    else {
        panic!("an alignment reports its directory")
    };
    let first = aligned.directory.absolute_path.to_path_buf();
    assert_refreshed(&root, "an alignment");

    committed(
        &Change::CreateVendor {
            root: root.clone(),
            assignments: Assignments::new().assign("name", "Tarnwick"),
            contacts: Vec::new(),
        },
        &effects(&PlatformFileSystem)
            .minting(FixedUuid("e0990d31-06aa-42e9-97c8-088e3e894822".into())),
    );
    assert_refreshed(&root, "a Vendor creation");

    committed(
        &Change::RenameValue {
            root: root.clone(),
            kind: ReusableValueKind::Orderer,
            project: None,
            value: "Ada".into(),
            to: "Ada Lovelace".into(),
        },
        &effects(&PlatformFileSystem),
    );
    assert_refreshed(&root, "a value rename across Orders");

    committed(&remove_order(second), &effects(&PlatformFileSystem));
    assert_refreshed(&root, "a removal");
    assert!(first.exists());
}

#[test]
fn an_edit_refreshes_another_orders_external_changes_without_rewriting_them() {
    for changed in ["record", "readme"] {
        let (_tree, root) = initialized_package("index-external-change");
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
        let plan = crate::plan(
            &edit_order(first, Assignments::new().assign("title", "Better lenses")),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();
        // An ordinary writer changes another Order while this plan waits. The index
        // is untouched, and a restore may preserve the old modification time.
        let (path, external) = if changed == "record" {
            let path = second.join(".spielorders/order.json");
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text.replace("\"Mounts\"", "\"Fresh mounts\""))
        } else {
            (second.join("README.md"), "Updated quote notes\n".to_owned())
        };
        std::fs::write(&path, &external).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| {
                file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(86_400))
            })
            .unwrap();

        crate::commit(plan, &effects(&PlatformFileSystem))
            .result
            .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), external.as_bytes());
        let package = opened(&root, &PlatformFileSystem).result.unwrap();
        let other = package
            .contents
            .healthy_orders
            .iter()
            .find(|order| order.paths.directory.absolute_path == second)
            .unwrap();
        if changed == "record" {
            assert_eq!(&*other.document.title, "Fresh mounts");
        } else {
            assert_eq!(other.readme_text.as_deref(), Some(external.as_str()));
        }
        assert_refreshed(&root, "an edit after another Order changed externally");
    }
}

#[test]
fn a_record_restored_with_an_older_modification_time_is_read_rather_than_the_index() {
    let (_tree, root) = initialized_package("index-restored-record");
    let order = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    assert!(index_is_current(&root));
    // A restore, `cp -p`, or sync puts back the record with the modification time it had
    // long before the index was written.
    let record = order.join(".spielorders/order.json");
    let restored = std::fs::read_to_string(&record)
        .unwrap()
        .replace("\"Lenses\"", "\"Mounts\"");
    std::fs::write(&record, restored).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&record)
        .and_then(|file| {
            file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(86_400))
        })
        .unwrap();

    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        &*package.contents.healthy_orders[0].document.title,
        "Mounts"
    );
    assert!(!index_is_current(&root));
}

#[test]
fn an_order_without_a_readme_is_proven_by_its_directory() {
    use crate::tests::support::{Operation, ScriptedFileSystem};
    let (_tree, root) = initialized_package("index-absent-readme");
    let order = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Lenses"),
    );
    let readme = order.join("README.md");
    std::fs::remove_file(&readme).unwrap();
    committed(&rebuild_index(&root), &effects(&PlatformFileSystem));

    // A record that cannot be read now is still shown as the current index holds it,
    // which only a load that took it from the index can do.
    let record = order.join(".spielorders/order.json");
    let filesystem = ScriptedFileSystem::new().failing(Operation::Read, &record);
    let package = opened(&root, &filesystem).result.unwrap();
    let [shown] = package.contents.healthy_orders.as_slice() else {
        panic!("the Order is shown from the current index")
    };
    assert_eq!(shown.readme_text.as_deref(), Some(""));
    assert!(index_is_current(&root));

    std::fs::write(&readme, "Quote requested\n").unwrap();
    let package = opened(&root, &PlatformFileSystem).result.unwrap();
    assert_eq!(
        package.contents.healthy_orders[0].readme_text.as_deref(),
        Some("Quote requested\n")
    );
    assert!(!index_is_current(&root));
}
