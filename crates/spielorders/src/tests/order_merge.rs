//! Decision sources: Define the Order record and lifecycle contract; Define the SpielOrders
//! package topology.
//!
//! Merging Orders in precedence order keeps the first, fills it from each later one in
//! turn — facts, Items, and directory entries it lacks — and removes what remains of the
//! others recoverably.

use crate::tests::support::{
    PlatformFileSystem, committed, created_order, effects, initialized_package, operations, order,
    planned, snapshot,
};
use crate::{Assignments, Change, ChangeResult};
use std::fs;
use std::path::{Path, PathBuf};

fn write(path: PathBuf, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

fn record(directory: &Path) -> crate::metadata::OrderDocument {
    crate::metadata::OrderDocument::from_json_bytes(
        &fs::read(directory.join(".spielorders/order.json")).unwrap(),
    )
    .unwrap()
}

fn item(description: &str) -> Assignments {
    Assignments::new()
        .assign("description", description)
        .assign("quantity", "1")
        .assign("unit_price", "2")
}

fn merge(orders: &[&PathBuf]) -> Change {
    Change::MergeOrders {
        orders: orders
            .iter()
            .map(|directory| order((*directory).clone()))
            .collect(),
    }
}

fn with_items(root: &Path, id: &str, assignments: Assignments, items: Vec<Assignments>) -> PathBuf {
    let result = committed(
        &Change::CreateOrder {
            root: root.into(),
            assignments,
            items,
        },
        &effects(&PlatformFileSystem).minting(crate::tests::support::FixedUuid(id.into())),
    );
    let ChangeResult::CreateOrder(created) = result else {
        panic!("a creation reports its directory")
    };
    created.directory.absolute_path.to_path_buf()
}

#[test]
fn the_kept_order_gains_only_what_it_lacks_and_the_other_is_removed() {
    let (_tree, root) = initialized_package("merge-orders");
    let kept = with_items(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Laser components")
            .assign("currency", "USD")
            .introduce("project", "Clock"),
        vec![item("Lens")],
    );
    let other = with_items(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Laser parts")
            .introduce("orderer", "Ada")
            .assign("currency", "USD")
            .introduce("project", "Atoms")
            .introduce("order_group", "Vacuum"),
        vec![item("Lens"), item("Mirror")],
    );
    write(kept.join("README.md"), "Kept notes\n");
    write(kept.join("quote.pdf"), "kept quote");
    write(kept.join("shared/notes.txt"), "kept notes");
    write(other.join("README.md"), "Other notes\n");
    write(other.join("quote.pdf"), "other quote");
    write(other.join("invoice.pdf"), "invoice");
    write(other.join("shared/notes.txt"), "other notes");
    write(other.join("shared/drawing.txt"), "drawing");
    write(other.join("photos/bench/one.jpg"), "photo");

    let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let mut discarded = operations(&plan)
        .into_iter()
        .filter(|operation| operation.starts_with("discard_merged_entry"))
        .collect::<Vec<_>>();
    discarded.sort();
    assert_eq!(discarded.len(), 3, "{discarded:?}");

    committed(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));

    let document = record(&kept);
    assert_eq!(document.title.as_str(), "Laser components");
    assert_eq!(document.orderer.as_deref(), Some("Ada"));
    assert_eq!(document.currency.as_deref(), Some("USD"));
    assert_eq!(document.project.as_deref(), Some("Clock"));
    assert_eq!(
        document.order_group, None,
        "a group comes only with its own Project"
    );
    let items = document
        .items
        .iter()
        .map(|item| item.description.as_str())
        .collect::<Vec<_>>();
    assert_eq!(items, ["Lens", "Mirror"]);

    assert_eq!(read(kept.join("README.md")), "Kept notes\n");
    assert_eq!(read(kept.join("quote.pdf")), "kept quote");
    assert_eq!(read(kept.join("shared/notes.txt")), "kept notes");
    assert_eq!(read(kept.join("invoice.pdf")), "invoice");
    assert_eq!(read(kept.join("shared/drawing.txt")), "drawing");
    assert_eq!(read(kept.join("photos/bench/one.jpg")), "photo");

    assert!(!other.exists(), "the merged Order is removed");
    let removed = root.join("Removed").join(other.file_name().unwrap());
    assert_eq!(
        read(removed.join("quote.pdf")),
        "other quote",
        "discards stay recoverable"
    );
    assert!(!removed.join("invoice.pdf").exists());
}

#[test]
fn each_later_order_fills_only_what_earlier_ones_left() {
    let (_tree, root) = initialized_package("merge-orders-precedence");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let second = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Second")
            .introduce("orderer", "One"),
    );
    let third = created_order(
        &root,
        "22222222-2222-4222-8222-222222222222",
        Assignments::new()
            .assign("title", "Third")
            .introduce("orderer", "Two")
            .introduce("ordering_institution", "Example Institute"),
    );
    write(second.join("quote.pdf"), "second");
    write(third.join("quote.pdf"), "third");

    committed(
        &merge(&[&kept, &second, &third]),
        &effects(&PlatformFileSystem),
    );

    let document = record(&kept);
    assert_eq!(document.orderer.as_deref(), Some("One"));
    assert_eq!(
        document.ordering_institution.as_deref(),
        Some("Example Institute")
    );
    assert_eq!(read(kept.join("quote.pdf")), "second");
    assert!(!second.exists() && !third.exists());
}

#[test]
fn a_merged_directory_that_changes_after_planning_refuses_the_commit() {
    let (_tree, root) = initialized_package("merge-orders-stale");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Other"),
    );
    let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    write(other.join("late.pdf"), "late");
    let before = snapshot(&root);

    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Err(crate::Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(snapshot(&root), before);
}

// A read-only source prevents the move, including when an implementation first
// creates a hard link and then attempts to remove the original name.
#[test]
fn denied_move() {
    use crate::tests::support::{Operation, ScriptedFileSystem};
    let (_tree, root) = initialized_package("merge-denied-move");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Other")
            .assign("currency", "USD"),
    );
    let source = other.join("quote.txt");
    let destination = kept.join("quote.txt");
    write(source.clone(), "Original quote");
    let before = snapshot(&root);
    let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let filesystem = ScriptedFileSystem::new()
        .denying(Operation::RemoveFile, &source)
        .denying(Operation::RenameNew, &destination);
    assert!(crate::commit(plan, &effects(&filesystem)).result.is_err());
    assert_eq!(snapshot(&root), before);
}

#[test]
fn interrupted_merge() {
    use crate::tests::support::{FailedCommit, Operation, ScriptedFileSystem};
    for cleanup_fails in [false, true] {
        let (_tree, root) = initialized_package("merge-interrupted");
        let kept = created_order(
            &root,
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Kept"),
        );
        let other = created_order(
            &root,
            "11111111-1111-4111-8111-111111111111",
            Assignments::new()
                .assign("title", "Other")
                .assign("currency", "USD"),
        );
        write(other.join("first.txt"), "First quote");
        write(other.join("second.txt"), "Second quote");
        let before = snapshot(&root);
        let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
        let filesystem =
            ScriptedFileSystem::new().denying(Operation::RenameNew, kept.join("second.txt"));
        if cleanup_fails {
            filesystem
                .on(Operation::RenameNew)
                .at(other.join("first.txt"))
                .denies_access();
        }
        let failure = crate::commit(plan, &effects(&filesystem))
            .result
            .unwrap_err();
        // A merge that stopped after it wrote is a failed commit naming what it left.
        assert!(
            matches!(
                (&failure, cleanup_fails),
                (crate::Failure::CommitFailed { .. }, false)
                    | (crate::Failure::CommitFailedCleanupIncomplete { .. }, true)
            ),
            "{failure:?}"
        );
        assert_eq!(read(other.join("second.txt")), "Second quote");
        assert_eq!(record(&kept).currency, None);
        if cleanup_fails {
            assert_eq!(read(kept.join("first.txt")), "First quote");
            assert!(
                failure
                    .cleanup_failures()
                    .iter()
                    .any(|failure| failure.location.absolute_path == kept.join("first.txt"))
            );
        } else {
            assert_eq!(snapshot(&root), before);
        }
    }
}

#[test]
fn concurrent_destination() {
    use crate::tests::support::{Operation, ScriptedFileSystem};
    let (_tree, root) = initialized_package("merge-concurrent-destination");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Other")
            .assign("currency", "USD"),
    );
    write(other.join("quote.txt"), "Original quote");
    let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::RenameNew)
        .at(kept.join("quote.txt"))
        .once()
        .preceded_by(|outside| outside.write(&outside.call().path, b"Concurrent quote"))
        .proceeds();
    let failure = crate::commit(plan, &effects(&filesystem))
        .result
        .unwrap_err();
    assert!(
        matches!(failure, crate::Failure::CommitFailed { .. }),
        "{failure:?}"
    );
    assert_eq!(read(other.join("quote.txt")), "Original quote");
    assert_eq!(read(kept.join("quote.txt")), "Concurrent quote");
    assert_eq!(record(&kept).currency, None);
}

/// Names resolve as the volume resolves them. A moved directory is part of the
/// virtual kept tree, including its descendants, before the next Order is planned.
#[test]
fn merged_names() {
    use crate::tests::support::resolves_case_insensitively;
    let (_tree, root) = initialized_package("merge-entry-names");
    let aliases = resolves_case_insensitively(&root);
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let earlier = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Earlier"),
    );
    let later = created_order(
        &root,
        "22222222-2222-4222-8222-222222222222",
        Assignments::new().assign("title", "Later"),
    );
    let last = created_order(
        &root,
        "33333333-3333-4333-8333-333333333333",
        Assignments::new().assign("title", "Last"),
    );
    write(last.join("Docs/nested/extra.txt"), "Last quote");
    write(earlier.join("docs/Nested/quote.txt"), "Earlier quote");
    write(later.join("Docs/nested/quote.txt"), "Later quote");
    write(later.join("Docs/nested/extra.txt"), "Extra quote");
    write(earlier.join("Original"), "Shared bytes");
    fs::hard_link(earlier.join("Original"), earlier.join("Hardlink")).unwrap();
    write(later.join("hardlink"), "Later bytes");
    std::os::unix::fs::symlink("Original", earlier.join("Link")).unwrap();
    std::os::unix::fs::symlink("Original", earlier.join("AnotherLink")).unwrap();
    std::os::unix::fs::symlink("missing", earlier.join("Dangling")).unwrap();
    std::os::unix::fs::symlink("other-missing", later.join("dangling")).unwrap();
    std::os::unix::fs::symlink("different-target", later.join("link")).unwrap();

    committed(
        &merge(&[&kept, &earlier, &later, &last]),
        &effects(&PlatformFileSystem),
    );

    assert_eq!(read(kept.join("docs/Nested/quote.txt")), "Earlier quote");
    assert_eq!(read(kept.join("Docs/nested/extra.txt")), "Extra quote");
    assert_eq!(read(kept.join("Original")), "Shared bytes");
    assert_eq!(read(kept.join("Hardlink")), "Shared bytes");
    assert_eq!(
        read(kept.join("hardlink")),
        if aliases {
            "Shared bytes"
        } else {
            "Later bytes"
        }
    );
    assert_eq!(
        read(kept.join("Docs/nested/quote.txt")),
        if aliases {
            "Earlier quote"
        } else {
            "Later quote"
        }
    );
    assert_eq!(
        fs::read_link(kept.join("Link")).unwrap(),
        Path::new("Original")
    );
    assert_eq!(
        fs::read_link(kept.join("AnotherLink")).unwrap(),
        Path::new("Original")
    );
    assert_eq!(
        fs::read_link(kept.join("Dangling")).unwrap(),
        Path::new("missing")
    );
    assert_eq!(
        fs::read_link(kept.join("dangling")).unwrap(),
        Path::new(if aliases { "missing" } else { "other-missing" })
    );
    assert_eq!(
        fs::read_link(kept.join("link")).unwrap(),
        Path::new(if aliases {
            "Original"
        } else {
            "different-target"
        })
    );
    assert!(!earlier.exists());
    assert!(!later.exists());
    assert!(!last.exists());
}

#[test]
fn a_volume_without_exclusive_rename_is_named_as_the_obstacle() {
    use crate::tests::support::{Operation, ScriptedFileSystem};
    let (_tree, root) = initialized_package("merge-unsupported");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Other"),
    );
    write(other.join("quote.txt"), "Quote");
    let before = snapshot(&root);
    let plan = planned(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let filesystem = ScriptedFileSystem::new();
    filesystem
        .on(Operation::RenameNew)
        .fails_with(std::io::ErrorKind::Unsupported);

    let failure = crate::commit(plan, &effects(&filesystem))
        .result
        .unwrap_err();
    assert!(
        matches!(failure, crate::Failure::FilesystemUnsupported(_)),
        "{failure:?}"
    );
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_merge_preserves_managed_evidence_under_the_volumes_spelling() {
    use crate::tests::support::ScriptedFileSystem;
    let (_tree, root) = initialized_package("merge-managed-spelling");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Other"),
    );
    fs::rename(other.join(".spielorders"), other.join(".SPIELORDERS")).unwrap();
    let stage = ".order.json.spielorders-stage";
    write(
        other.join(".SPIELORDERS").join(stage),
        "Prior metadata evidence",
    );
    write(other.join("quote.txt"), "Quote");
    let filesystem = ScriptedFileSystem::case_insensitive_over(&root);

    committed(&merge(&[&kept, &other]), &effects(&filesystem));

    assert_eq!(read(kept.join("quote.txt")), "Quote");
    assert!(!kept.join(".spielorders").join(stage).exists());
    let removed = root.join("Removed").join(other.file_name().unwrap());
    assert_eq!(
        read(removed.join(".SPIELORDERS").join(stage)),
        "Prior metadata evidence"
    );
}

#[test]
fn merged_directories_follow_the_volumes_name_aliases() {
    use crate::tests::support::ScriptedFileSystem;
    for (prefix, suffix) in [("", "."), ("", " "), (" ", "")] {
        let (_tree, root) = initialized_package("merge-trailing-alias");
        let kept = created_order(
            &root,
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            Assignments::new().assign("title", "Kept"),
        );
        let earlier = created_order(
            &root,
            "11111111-1111-4111-8111-111111111111",
            Assignments::new().assign("title", "Earlier"),
        );
        let later = created_order(
            &root,
            "22222222-2222-4222-8222-222222222222",
            Assignments::new().assign("title", "Later"),
        );
        write(earlier.join("docs/first.txt"), "First quote");
        write(
            later.join(format!("{prefix}docs{suffix}/second.txt")),
            "Second quote",
        );
        let filesystem = ScriptedFileSystem::trimmed_names_over(&root);

        committed(&merge(&[&kept, &earlier, &later]), &effects(&filesystem));

        assert_eq!(read(kept.join("docs/first.txt")), "First quote");
        assert_eq!(read(kept.join("docs/second.txt")), "Second quote");
        assert!(!earlier.exists());
        assert!(!later.exists());
    }
}

#[test]
fn a_merged_order_that_could_not_be_removed_refuses_the_merge() {
    let (_tree, root) = initialized_package("merge-unremovable");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let other = created_order(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Other"),
    );
    write(other.join("invoice.pdf"), "Invoice");
    // An earlier removal of an Order with the same name holds the merged Order's place.
    let earlier = root.join("Removed").join(other.file_name().unwrap());
    fs::create_dir_all(&earlier).unwrap();
    let before = snapshot(&root);

    assert!(
        crate::plan(&merge(&[&kept, &other]), &effects(&PlatformFileSystem))
            .result
            .is_err()
    );
    assert_eq!(snapshot(&root), before);
    assert_eq!(read(other.join("invoice.pdf")), "Invoice");
}

#[test]
fn items_priced_in_another_currency_refuse_the_merge() {
    let (_tree, root) = initialized_package("merge-currency");
    let kept = with_items(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Kept")
            .assign("currency", "USD"),
        vec![item("Lens")],
    );
    let euros = with_items(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Euros")
            .assign("currency", "EUR"),
        vec![item("Mirror")],
    );
    let before = snapshot(&root);
    assert!(matches!(
        crate::plan(&merge(&[&kept, &euros]), &effects(&PlatformFileSystem)).result,
        Err(crate::Failure::CurrencyDiffers { .. })
    ));
    assert_eq!(snapshot(&root), before);

    // Adding no Item, another currency changes nothing the kept Order states.
    let same = with_items(
        &root,
        "22222222-2222-4222-8222-222222222222",
        Assignments::new()
            .assign("title", "Same")
            .assign("currency", "EUR"),
        vec![item("Lens")],
    );
    committed(&merge(&[&kept, &same]), &effects(&PlatformFileSystem));
    let result = record(&kept);
    assert_eq!(result.currency.as_deref(), Some("USD"));
    assert_eq!(result.items.len(), 1);
}

/// The refusal a merge of `orders` answers when currencies differ, located at the Order
/// whose currency does.
fn currency_refusal(orders: &[&PathBuf]) -> PathBuf {
    match crate::plan(&merge(orders), &effects(&PlatformFileSystem)).result {
        Err(crate::Failure::CurrencyDiffers { location }) => location.absolute_path.clone(),
        other => panic!("refused as differing currencies: {other:?}"),
    }
}

#[test]
fn a_merge_never_labels_prices_that_were_unlabeled() {
    let (_tree, root) = initialized_package("merge-unlabeled-prices");
    let id = |n: u8| format!("{n:08x}-1111-4111-8111-111111111111");
    let unlabeled = with_items(
        &root,
        &id(1),
        Assignments::new().assign("title", "Unlabeled"),
        vec![item("Lens")],
    );
    let dollars = with_items(
        &root,
        &id(2),
        Assignments::new()
            .assign("title", "Dollars")
            .assign("currency", "USD"),
        vec![item("Mirror")],
    );
    let before = snapshot(&root);
    // Whichever side lacks the currency, and the refusal names the later Order.
    assert_eq!(currency_refusal(&[&unlabeled, &dollars]), dollars);
    assert_eq!(currency_refusal(&[&dollars, &unlabeled]), unlabeled);
    assert_eq!(snapshot(&root), before);

    // An Order bringing no Items never refuses and labels nothing.
    let euros = with_items(
        &root,
        &id(3),
        Assignments::new()
            .assign("title", "Euros")
            .assign("currency", "EUR"),
        Vec::new(),
    );
    committed(&merge(&[&unlabeled, &euros]), &effects(&PlatformFileSystem));
    assert_eq!(record(&unlabeled).currency, None);
}

#[test]
fn a_result_that_states_a_currency_keeps_it_without_items() {
    let (_tree, root) = initialized_package("merge-stated-currency");
    let id = |n: u8| format!("{n:08x}-3333-4333-8333-333333333333");
    let euros = with_items(
        &root,
        &id(1),
        Assignments::new()
            .assign("title", "Euros")
            .assign("currency", "EUR"),
        Vec::new(),
    );
    let dollars = with_items(
        &root,
        &id(2),
        Assignments::new()
            .assign("title", "Dollars")
            .assign("currency", "USD"),
        vec![item("Lens")],
    );
    let unlabeled = with_items(
        &root,
        &id(3),
        Assignments::new().assign("title", "Unlabeled"),
        vec![item("Mirror")],
    );
    let before = snapshot(&root);
    assert_eq!(currency_refusal(&[&euros, &dollars]), dollars);
    assert_eq!(currency_refusal(&[&euros, &unlabeled]), unlabeled);
    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_result_with_no_items_takes_the_currency_of_the_first_order_that_brings_some() {
    let (_tree, root) = initialized_package("merge-first-priced");
    let id = |n: u8| format!("{n:08x}-2222-4222-8222-222222222222");
    let empty = |n: u8| {
        with_items(
            &root,
            &id(n),
            Assignments::new().assign("title", format!("Empty {n}")),
            Vec::new(),
        )
    };
    let mirror = with_items(
        &root,
        &id(10),
        Assignments::new().assign("title", "Mirror"),
        vec![item("Mirror")],
    );
    let lens = with_items(
        &root,
        &id(11),
        Assignments::new()
            .assign("title", "Lens")
            .assign("currency", "USD"),
        vec![item("Lens")],
    );
    let first = empty(1);
    // Either order refuses: the first Order bringing Items sets the result's currency.
    assert_eq!(currency_refusal(&[&first, &mirror, &lens]), lens);
    assert_eq!(currency_refusal(&[&first, &lens, &mirror]), mirror);

    committed(&merge(&[&first, &lens]), &effects(&PlatformFileSystem));
    assert_eq!(record(&first).currency.as_deref(), Some("USD"));
    let second = empty(2);
    committed(&merge(&[&second, &mirror]), &effects(&PlatformFileSystem));
    assert_eq!(record(&second).currency, None);
}

#[test]
fn an_order_gaining_a_project_takes_that_projects_group() {
    let (_tree, root) = initialized_package("merge-group");
    let kept = with_items(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new()
            .assign("title", "Kept")
            .introduce("order_group", "Vacuum"),
        Vec::new(),
    );
    let other = with_items(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new()
            .assign("title", "Other")
            .introduce("project", "Atoms")
            .introduce("order_group", "Optics"),
        Vec::new(),
    );
    committed(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let result = record(&kept);
    assert_eq!(result.project.as_deref(), Some("Atoms"));
    assert_eq!(result.order_group.as_deref(), Some("Optics"));
}

#[test]
fn repeated_lines_of_a_merged_order_are_all_kept() {
    let (_tree, root) = initialized_package("merge-repeated-lines");
    let kept = with_items(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
        vec![item("Lens")],
    );
    let other = with_items(
        &root,
        "11111111-1111-4111-8111-111111111111",
        Assignments::new().assign("title", "Other"),
        vec![item("Lens"), item("Shipping"), item("Shipping")],
    );
    committed(&merge(&[&kept, &other]), &effects(&PlatformFileSystem));
    let descriptions = record(&kept)
        .items
        .iter()
        .map(|item| item.description.to_string())
        .collect::<Vec<_>>();
    assert_eq!(descriptions, ["Lens", "Shipping", "Shipping"]);
}

/// A merge plans what its commit then does: each merged Order's removal, creating the
/// Removed Orders Directory once where there is none yet.
#[test]
fn a_merge_plans_each_removal_it_commits() {
    let (_tree, root) = initialized_package("merge-plans-removals");
    let kept = with_items(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Laser components"),
        Vec::new(),
    );
    let others = [
        ("11111111-1111-4111-8111-111111111111", "Laser parts"),
        ("22222222-2222-4222-8222-222222222222", "Laser spares"),
    ]
    .map(|(id, title)| {
        with_items(
            &root,
            id,
            Assignments::new().assign("title", title),
            Vec::new(),
        )
    });
    let plan = planned(
        &merge(&[&kept, &others[0], &others[1]]),
        &effects(&PlatformFileSystem),
    );
    let kinds = crate::tests::support::operation_kinds(&plan);

    assert_eq!(
        kinds
            .iter()
            .filter(|kind| *kind == "create_removed_orders_directory")
            .count(),
        1,
        "{kinds:?}"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| *kind == "move_order_directory")
            .count(),
        2,
        "{kinds:?}"
    );
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("refresh_package_index")
    );
}

#[test]
fn a_merge_whose_index_cannot_refresh_warns_once() {
    use crate::tests::support::{Operation, ScriptedFileSystem};
    let (_tree, root) = initialized_package("merge-refresh-once");
    let kept = created_order(
        &root,
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        Assignments::new().assign("title", "Kept"),
    );
    let others = [
        ("11111111-1111-4111-8111-111111111111", "Other"),
        ("22222222-2222-4222-8222-222222222222", "Another"),
    ]
    .map(|(id, title)| created_order(&root, id, Assignments::new().assign("title", title)));
    let plan = planned(
        &merge(&[&kept, &others[0], &others[1]]),
        &effects(&PlatformFileSystem),
    );
    let filesystem = ScriptedFileSystem::new()
        .failing_ending_in(Operation::ReplaceFile, ".spielorders/index.json");

    let report = crate::commit(plan, &effects(&filesystem)).result;

    let Ok(crate::Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("the merge commits whatever the refresh does: {report:?}")
    };
    assert!(
        matches!(warnings.as_slice(), [crate::Warning::IndexRefreshFailed(_)]),
        "{warnings:?}"
    );
    assert!(!others[0].exists() && !others[1].exists());
}
