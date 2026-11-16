//! Decision sources: Define the SpielOrders package topology; Define the Rust CLI
//! capability contract; Define integrity, validation, and repair boundaries; Define
//! durable metadata schemas and version evolution.

use crate::tests::support::assert_reported_beside;
use crate::tests::support::index_is_current;
use crate::tests::support::operation_path;
use crate::tests::support::{
    EditPoint, FailedCommit, FixedUuid, Index, ManagedFileBoundary, Operation, PlatformFileSystem,
    ScriptedFileSystem, adoption, another_writer_replacing, current_readme, effects,
    failing_replacement, operations, order_scope, package_scope, package_tree, readme_original,
    with_stage_cleanup_failure, write_readme,
};
use crate::{Assignments, Plan, Report};

use crate::readme::ReadmeUnavailableReason;
use crate::readme_writes::ReadmeOriginal;
use crate::{Failure, Warning};
use std::fs;
use std::path::{Path, PathBuf};

/// Deliberately non-normalized text. It carries CRLF, a lone CR, trailing spaces, a
/// multi-byte character, and no trailing newline, so a writer that normalized line
/// endings, trimmed, or re-encoded would change the bytes instead of preserving them.
const AWKWARD: &str =
    "# Läser components\r\n\r\nlone CR next:\rtrailing spaces:   \nno final newline";

fn package_readme(root: &Path) -> Vec<u8> {
    fs::read(root.join("README.md")).expect("the Package README exists")
}

#[test]
fn a_package_readme_write_round_trips_exact_supplied_bytes() {
    let tree = package_tree("readme-write-package-exact");
    let root = tree.path();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();
    assert_eq!(
        package_readme(root),
        b"",
        "initialization leaves an empty conventional document"
    );

    let plan = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an established Package README can be written");

    assert!(!operations(&plan).is_empty());
    // A Package README is not indexed, so its write owns no derived-index action.
    assert_eq!(operations(&plan), ["replace_readme_document README.md"]);
    assert_eq!(package_readme(root), b"", "planning writes nothing");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("a valid Package README write commits: {report:?}");
    };
    assert_eq!(
        package_readme(root),
        AWKWARD.as_bytes(),
        "the document holds the exact supplied bytes"
    );

    // Reading back through the established read capability returns the same bytes.
    let read = crate::read_readme(
        &crate::Scope::Package(root.to_path_buf()),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the README reads");
    assert_eq!(read.text.as_deref(), Some(AWKWARD));
    assert!(read.exists);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before,
        "a Package README write replaces no derived index"
    );
}

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";

/// An adopted Order whose README already carries user-authored text, so a write has
/// prior content to replace rather than only an empty document to fill.
fn order_with_readme(root: &Path, readme: &[u8]) -> PathBuf {
    let directory = root.join("Orders/Legacy_laser_purchase");
    fs::create_dir(&directory).expect("the fixture Order Directory is created");
    fs::write(directory.join("README.md"), readme).expect("the fixture README is written");
    let plan = crate::plan(
        &adoption(
            PathBuf::from(root),
            directory.clone(),
            &(
                Assignments::new()
                    .assign("title", "Laser components")
                    .assign("date_ordered", "2026-08-22")
                    .assign("status", "hold"),
                Vec::new(),
            ),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the fixture directory can be adopted");
    let report = crate::commit(
        plan,
        &effects(&PlatformFileSystem).minting(FixedUuid(ORDER_ID.into())),
    )
    .result;
    assert!(
        matches!(report, Ok(Report::Committed { .. })),
        "the fixture Order commits: {report:?}"
    );
    directory
}

fn indexed_readme(root: &Path) -> String {
    let index = Index::from_json_bytes(&fs::read(root.join(".spielorders/index.json")).unwrap())
        .expect("the derived index parses");
    index.orders[0].readme.clone()
}

/// Commits `plan`, which the test expects to commit, returning the README it writes.
fn committed(plan: Plan) -> PathBuf {
    let path = operation_path(&plan, "replace_readme_document");
    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;
    let Ok(Report::Committed { .. }) = report else {
        panic!("a valid README write commits: {report:?}");
    };
    path
}

#[test]
fn an_order_readme_write_replaces_its_document_and_refreshes_the_derived_index() {
    let tree = package_tree("readme-write-order-index");
    let root = tree.path();
    let directory = order_with_readme(
        root,
        b"prior order notes
",
    );
    assert_eq!(
        indexed_readme(root),
        "prior order notes
"
    );

    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("a healthy Order README can be written");

    // The Order README is projected into the derived index, so this write owns the
    // refresh the Package README write does not.
    assert_eq!(
        operations(&plan),
        [
            "replace_readme_document Orders/Legacy_laser_purchase/README.md",
            "refresh_package_index .spielorders/index.json",
        ]
    );

    let path = committed(plan);

    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
    assert!(index_is_current(root));
    // Criterion 4: a later load observes the new text through canonical evidence
    // rather than reporting the cached prior text as current.
    assert_eq!(indexed_readme(root), AWKWARD);
    assert_eq!(
        package_readme(root),
        b"",
        "an Order README write leaves the Package README alone"
    );
}

/// The Package-relative destinations a capability replaced.
fn replaced(filesystem: &ScriptedFileSystem, root: &Path) -> Vec<String> {
    filesystem
        .paths(Operation::ReplaceFile)
        .iter()
        .map(|path| relative(root, path))
        .collect()
}

/// The Package-relative paths of every call a capability made.
fn touched(filesystem: &ScriptedFileSystem, root: &Path) -> Vec<String> {
    filesystem
        .calls()
        .iter()
        .map(|call| relative(root, &call.path))
        .collect()
}

/// Effect paths arrive canonicalized, while a fixture root may still contain a
/// symlinked prefix, so both are canonicalized before comparison.
fn relative(root: &Path, path: &Path) -> String {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

const CANONICAL_METADATA: [&str; 4] = ["order.json", "vendors.json", "version.json", "config.toml"];

fn assert_no_metadata_write(scripted: &ScriptedFileSystem, label: &str) {
    for call in scripted.calls() {
        let written = match call.operation {
            Operation::WriteNew | Operation::ReplaceFile => call.path,
            _ => continue,
        };
        let name = written.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            !CANONICAL_METADATA.contains(&name.as_str()),
            "{label} writes no canonical metadata, but wrote {name}"
        );
    }
}

fn assert_no_supporting_material_touch(scripted: &ScriptedFileSystem, root: &Path, label: &str) {
    for touched in touched(scripted, root) {
        assert!(
            !touched.contains("quote.bin") && !touched.contains("Quotes"),
            "{label} never inspects Supporting Material, but touched {touched}"
        );
    }
}

#[test]
fn a_package_readme_commit_replaces_only_its_own_document() {
    let tree = package_tree("readme-write-package-scripted");
    let root = tree.path();
    let plan = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            "new package prose\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Package README can be written");

    let scripted = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&scripted)).result;

    assert!(matches!(report, Ok(Report::Committed { .. })));
    // A Package README is not indexed, so its write replaces exactly one path and
    // never reaches the derived index. Byte equality alone could not show this: a
    // rebuilt index of unchanged Orders is byte-identical to the one it replaced.
    assert_eq!(replaced(&scripted, root), ["README.md"]);
    assert_eq!(
        scripted.paths(Operation::ReadDirectory),
        Vec::<PathBuf>::new(),
        "a Package README write lists no directory"
    );
    assert_no_metadata_write(&scripted, "a Package README write");
    // The directory holding the document is observed, to refuse writing through a link,
    // and nothing else is.
    for touched in touched(&scripted, root) {
        assert!(
            touched.is_empty()
                || touched.starts_with("README.md")
                || touched.starts_with(".README.md"),
            "a Package README write touches only its own document, but touched {touched}"
        );
    }
}

#[test]
fn an_order_readme_commit_replaces_only_its_document_and_the_derived_index() {
    let tree = package_tree("readme-write-order-scripted");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::create_dir(directory.join("Quotes")).unwrap();
    fs::write(directory.join("quote.bin"), b"opaque supporting material").unwrap();
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "new order prose\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Order README can be written");

    let scripted = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&scripted)).result;

    assert!(matches!(report, Ok(Report::Committed { .. })));
    // Exactly the document and the derived index it is projected into. The refresh
    // lists the Orders Directory, which is that declared derived-index action.
    assert_eq!(
        replaced(&scripted, root),
        [
            "Orders/Legacy_laser_purchase/README.md",
            ".spielorders/index.json"
        ]
    );
    assert_no_metadata_write(&scripted, "an Order README write");
    assert_no_supporting_material_touch(&scripted, root, "an Order README write");
}

#[test]
fn planning_either_readme_write_performs_no_stage_write_or_cleanup() {
    let tree = package_tree("readme-write-dry-run");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let package_before = package_readme(root);
    let order_before = fs::read(directory.join("README.md")).unwrap();
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    for (label, plan) in [
        (
            "package",
            crate::plan(
                &write_readme(
                    package_scope(PathBuf::from(root)),
                    current_readme(root),
                    String::from(AWKWARD),
                ),
                &effects(&PlatformFileSystem),
            )
            .result
            .unwrap(),
        ),
        (
            "order",
            crate::plan(
                &write_readme(
                    order_scope(directory.clone()),
                    current_readme(&directory),
                    String::from(AWKWARD),
                ),
                &effects(&PlatformFileSystem),
            )
            .result
            .unwrap(),
        ),
    ] {
        assert!(!plan.operations().is_empty(), "{label}");
    }

    let scripted = ScriptedFileSystem::new();
    let _ = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            String::from(AWKWARD),
        ),
        &effects(&scripted),
    )
    .result
    .unwrap();
    let _ = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&scripted),
    )
    .result
    .unwrap();

    assert_eq!(
        scripted.effects(),
        [],
        "a dry run stages, writes, and cleans up nothing"
    );
    assert_eq!(package_readme(root), package_before);
    assert_eq!(fs::read(directory.join("README.md")).unwrap(), order_before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

fn modified_at(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .expect("the document exists")
        .modified()
        .expect("the platform reports a modification time")
}

#[test]
fn writing_identical_text_is_unchanged_and_replaces_nothing() {
    let tree = package_tree("readme-write-no-op");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let order_readme = directory.join("README.md");
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());
    let modified_before = modified_at(&order_readme);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("identical text is planned like any other write");

    assert!(operations(&plan).is_empty());
    assert!(
        plan.operations().is_empty(),
        "a write that replaces nothing announces no managed operation"
    );

    let scripted = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&scripted)).result;

    let Ok(Report::Unchanged { .. }) = report else {
        panic!("identical text is an accepted no-op: {report:?}");
    };
    // Byte equality cannot show this on its own, since rewriting identical content is
    // byte-identical. The effect log proves no replacement happened at all, and the
    // untouched modification time is the independent second signal.
    assert_eq!(scripted.effects(), []);
    assert_eq!(modified_at(&order_readme), modified_before);
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn empty_text_is_an_ordinary_document_in_both_directions() {
    let tree = package_tree("readme-write-empty");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");

    // Clearing a populated document to empty is a write, not a no-op.
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("empty text can be written");
    assert!(!operations(&plan).is_empty());
    let path = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), b"");
    assert!(path.is_file(), "an empty document still exists");
    assert_eq!(indexed_readme(root), "");

    // Writing empty text over an already empty document changes nothing.
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    assert!(operations(&plan).is_empty());
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Ok(Report::Unchanged { .. })
    ));

    // And the document can be filled again from empty.
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let path = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());
}

#[test]
fn an_absent_readme_is_a_creatable_conventional_document() {
    let tree = package_tree("readme-write-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    // A missing README is normal; the user may simply have deleted it.
    fs::remove_file(&order_readme).unwrap();
    fs::remove_file(root.join("README.md")).unwrap();

    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an absent Order README is creatable");
    assert!(!operations(&plan).is_empty());
    let path = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), AWKWARD.as_bytes());

    let plan = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            "created package prose\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an absent Package README is creatable");
    let path = committed(plan);
    assert_eq!(fs::read(&path).unwrap(), b"created package prose\n");

    // Absence and emptiness are different states, so creating an empty document is
    // still a change. A writer that compared only bytes would leave it absent.
    let tree = package_tree("readme-write-absent-empty");
    let root = tree.path();
    let package_readme_path = root.join("README.md");
    fs::remove_file(&package_readme_path).unwrap();
    let plan = crate::plan(
        &write_readme(package_scope(PathBuf::from(root)), current_readme(root), ""),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an absent Package README accepts empty text");
    assert!(
        !operations(&plan).is_empty(),
        "creating an empty document is a change, not a no-op"
    );
    let path = committed(plan);
    assert!(path.exists(), "the empty document is created");
    assert_eq!(fs::read(&path).unwrap(), b"");
}

const README_STAGE: &str = ".README.md.spielorders-stage";

#[test]
fn injected_write_failures_preserve_the_prior_document_exactly() {
    for boundary in [
        ManagedFileBoundary::WriteStage,
        ManagedFileBoundary::SyncStage,
        ManagedFileBoundary::Replace,
    ] {
        let tree = package_tree("readme-write-failure");
        let root = tree.path();
        let directory = order_with_readme(root, AWKWARD.as_bytes());
        let order_readme = directory.join("README.md");
        let plan = crate::plan(
            &write_readme(
                order_scope(directory.clone()),
                current_readme(&directory),
                "replacement prose\n",
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect("the write can be planned");

        let report =
            crate::commit(plan, &effects(&failing_replacement("README.md", boundary))).result;

        let Err(rejection) = report else {
            panic!("an injected {boundary:?} failure rejects: {report:?}");
        };
        let error = &rejection;
        assert_reported_beside(error);
        assert!(error.cleanup_failures().is_empty());
        assert_eq!(
            fs::read(&order_readme).unwrap(),
            AWKWARD.as_bytes(),
            "the prior document survives an injected {boundary:?} failure byte for byte"
        );
        assert!(!directory.join(README_STAGE).exists());
    }
}

#[test]
fn an_injected_failure_over_an_absent_document_preserves_its_absence() {
    let tree = package_tree("readme-write-failure-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    fs::remove_file(&order_readme).unwrap();
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an absent document can be planned");

    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            "README.md",
            ManagedFileBoundary::Replace,
        )),
    )
    .result;

    assert!(report.is_err());
    assert!(
        !order_readme.exists(),
        "a failed creation leaves the document absent rather than half written"
    );
    assert!(!directory.join(README_STAGE).exists());
}

#[test]
fn a_staging_cleanup_failure_is_retained_beside_the_rejection() {
    let tree = package_tree("readme-write-cleanup-failure");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "replacement prose\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&with_stage_cleanup_failure(
            "README.md",
            ManagedFileBoundary::SyncStage,
        )),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a corrupted staged document rejects: {report:?}");
    };
    let error = &rejection;
    assert!(!error.cleanup_failures().is_empty());
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes()
    );
    assert!(directory.join(README_STAGE).exists());
    fs::remove_file(directory.join(README_STAGE)).unwrap();
}

#[test]
fn a_post_commit_index_failure_keeps_the_committed_order_readme() {
    for boundary in [
        ManagedFileBoundary::IndexReplacement,
        ManagedFileBoundary::RefreshListing,
    ] {
        let tree = package_tree("readme-write-index-failure");
        let root = tree.path();
        let directory = order_with_readme(root, b"prior order notes\n");
        let plan = crate::plan(
            &write_readme(
                order_scope(directory.clone()),
                current_readme(&directory),
                String::from(AWKWARD),
            ),
            &effects(&PlatformFileSystem),
        )
        .result
        .unwrap();

        let report =
            crate::commit(plan, &effects(&failing_replacement("README.md", boundary))).result;

        let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
            panic!("a failed {boundary:?} refresh remains committed: {report:?}");
        };
        assert!(matches!(
            warnings.as_slice(),
            [Warning::IndexRefreshFailed(_)]
        ));
        // The document write stands, so retrying it is not what a caller should do.
        assert_eq!(
            fs::read(directory.join("README.md")).unwrap(),
            AWKWARD.as_bytes()
        );
    }
}

#[test]
fn a_changed_target_rejects_before_any_write_without_touching_siblings() {
    let tree = package_tree("readme-write-target-race");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // Another writer replaces the same document between planning and commit.
    fs::write(&order_readme, b"someone else's prose\n").unwrap();
    let package_before = package_readme(root);
    let index_before = fs::read(root.join(".spielorders/index.json")).unwrap();

    let scripted = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&scripted)).result;

    let Err(rejection) = report else {
        panic!("a changed target rejects: {report:?}");
    };
    let Failure::PreconditionsChanged { location } = &rejection else {
        panic!("a changed target is a precondition change: {rejection:?}");
    };
    assert!(location.absolute_path.ends_with("README.md"));
    assert_eq!(
        fs::read(&order_readme).unwrap(),
        b"someone else's prose\n",
        "the intervening write is not silently overwritten"
    );
    assert_eq!(replaced(&scripted, root), Vec::<String>::new());
    assert_eq!(package_readme(root), package_before);
    assert_eq!(
        fs::read(root.join(".spielorders/index.json")).unwrap(),
        index_before
    );
}

#[test]
fn a_target_that_became_absent_or_wrongly_typed_rejects() {
    // Planned against a present document, committed after it vanished.
    let tree = package_tree("readme-write-target-absent");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    fs::remove_file(directory.join("README.md")).unwrap();
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Err(Failure::PreconditionsChanged { .. })
    ));
    assert!(!directory.join("README.md").exists());

    // Planned against an absent document, committed after one appeared.
    let tree = package_tree("readme-write-target-appeared");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::remove_file(directory.join("README.md")).unwrap();
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    fs::write(directory.join("README.md"), b"appeared\n").unwrap();
    assert!(matches!(
        crate::commit(plan, &effects(&PlatformFileSystem)).result,
        Err(Failure::PreconditionsChanged { .. })
    ));
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        b"appeared\n"
    );
}

#[test]
fn a_wrongly_typed_destination_is_refused_without_conversion() {
    let tree = package_tree("readme-write-wrong-kind");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    fs::remove_file(directory.join("README.md")).unwrap();
    fs::create_dir(directory.join("README.md")).unwrap();
    fs::write(directory.join("README.md/inner.txt"), b"not a README\n").unwrap();

    let error = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a directory at the conventional path is not a writable document");

    let Failure::UnwritableDestination { reason, .. } = &error else {
        panic!("a wrongly typed destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::WrongKind);
    // Never converted, never removed.
    assert!(directory.join("README.md").is_dir());
    assert_eq!(
        fs::read(directory.join("README.md/inner.txt")).unwrap(),
        b"not a README\n"
    );
}

#[test]
fn a_damaged_scope_refuses_the_write_without_touching_the_document() {
    let tree = package_tree("readme-write-damaged-order");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());
    let order_readme = directory.join("README.md");
    fs::write(directory.join(".spielorders/order.json"), b"{ not json").unwrap();

    let error = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "replacement\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a Damaged Order's README is not writable");
    assert!(
        matches!(
            error,
            Failure::DamagedRecord { .. } | Failure::DamagedOrder { .. }
        ),
        "a damaged Order scope refuses: {error:?}"
    );
    assert_eq!(fs::read(&order_readme).unwrap(), AWKWARD.as_bytes());

    // A damaged Package refuses its own README the same way.
    let tree = package_tree("readme-write-damaged-package");
    let root = tree.path();
    fs::write(root.join(".spielorders/version.json"), b"{ not json").unwrap();
    let error = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            "replacement\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a Damaged Package's README is not writable");
    assert!(
        matches!(error, Failure::DamagedPackage(_)),
        "a damaged Package scope refuses: {error:?}"
    );
    assert_eq!(package_readme(root), b"");
}

#[test]
fn an_unreadable_destination_is_refused_without_writing_over_it() {
    let tree = package_tree("readme-write-unreadable-destination");
    let root = tree.path();
    let directory = order_with_readme(root, AWKWARD.as_bytes());

    // Only the destination document itself cannot be read, so the plan sees a file it
    // cannot read rather than one that is missing or wrongly typed.
    let unreadable = ScriptedFileSystem::new().failing_ending_in(Operation::Read, "README.md");
    let error = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            "replacement\n",
        ),
        &effects(&unreadable),
    )
    .result
    .expect_err("a destination that cannot be read is not blindly replaced");

    let Failure::UnwritableDestination { reason, .. } = &error else {
        panic!("an unreadable destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::Unreadable);
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes(),
        "the document it could not read is left exactly as it was"
    );
}

#[cfg(unix)]
#[test]
fn a_link_at_the_conventional_path_is_refused_without_following_it() {
    let tree = package_tree("readme-write-link-destination");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let target = directory.join("elsewhere.md");
    fs::write(&target, b"linked prose\n").unwrap();
    fs::remove_file(directory.join("README.md")).unwrap();
    std::os::unix::fs::symlink(&target, directory.join("README.md")).unwrap();

    let error = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("a link at the conventional path is not a writable document");

    let Failure::UnwritableDestination { reason, .. } = &error else {
        panic!("a link destination names its reason: {error:?}");
    };
    assert_eq!(*reason, ReadmeUnavailableReason::WrongKind);
    // The link is neither followed nor replaced, so its target is untouched.
    assert_eq!(fs::read(&target).unwrap(), b"linked prose\n");
    assert!(
        fs::symlink_metadata(directory.join("README.md"))
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn an_unreadable_utf8_document_cannot_authorize_replacing_its_bytes() {
    let tree = package_tree("readme-write-invalid-utf8-prior");
    let root = tree.path();
    let directory = order_with_readme(root, b"placeholder\n");
    fs::write(directory.join("README.md"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let original = current_readme(&directory);
    assert_eq!(original, ReadmeOriginal::Unavailable);
    let error = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            original.clone(),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect_err("the shipping reader cannot establish original text from invalid UTF-8");
    assert!(matches!(error, Failure::PreconditionsChanged { .. }));
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        [0xff, 0xfe, 0x00, 0x41]
    );
}

#[test]
fn a_package_readme_write_leaves_the_derived_index_alone() {
    let tree = package_tree("readme-write-package-index-record");
    let root = tree.path();

    let plan = crate::plan(
        &write_readme(
            package_scope(PathBuf::from(root)),
            current_readme(root),
            "package prose\n",
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();
    let filesystem = ScriptedFileSystem::new();
    let report = crate::commit(plan, &effects(&filesystem)).result;

    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");
    // The Package README is not indexed, so this write leaves the derived index alone.
    assert!(
        filesystem
            .paths(Operation::ReplaceFile)
            .iter()
            .all(|path| !path.ends_with(".spielorders/index.json"))
    );
}

#[test]
fn a_package_that_becomes_unloadable_after_the_write_reports_an_unavailable_refresh() {
    let tree = package_tree("readme-write-refresh-unavailable");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    // The document is replaced, and only then does the Package stop loading. The
    // canonical write stands and the report says the refresh could not be attempted.
    let report = crate::commit(
        plan,
        &effects(&failing_replacement(
            "README.md",
            ManagedFileBoundary::PackageDamagedInRefresh,
        )),
    )
    .result;

    let Ok(Report::CommittedWithWarning { warnings, .. }) = report else {
        panic!("an unloadable Package after the write remains committed: {report:?}");
    };
    assert_eq!(warnings, [Warning::RefreshUnavailable]);
    assert_eq!(
        fs::read(directory.join("README.md")).unwrap(),
        AWKWARD.as_bytes()
    );
}

/// A README rewritten between this write's revalidation and its rename belongs to
/// whoever wrote it. The write refuses rather than replacing their text.
#[test]
fn a_readme_rewritten_after_revalidation_is_refused_rather_than_overwritten() {
    let tree = package_tree("readme-write-concurrent-edit");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let destination = directory.join("README.md");
    const THEIRS: &[u8] = b"# written by someone else\n";

    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .unwrap();

    let report = crate::commit(
        plan,
        &effects(&another_writer_replacing(
            &destination,
            THEIRS,
            EditPoint::StagingWrite,
        )),
    )
    .result;

    let Err(rejection) = report else {
        panic!("a README rewritten underneath this write is refused: {report:?}");
    };
    // Refused because someone else changed it — distinct from a failed write.
    let _error = rejection;
    assert_eq!(
        fs::read(&destination).unwrap(),
        THEIRS,
        "the other writer's README survives"
    );
}

/// Observing nothing at the destination is an observation, so a creation is authorized
/// against it exactly as a rewrite is authorized against the bytes it read. A README
/// that appears in the window — an editor saving, an adoption, a restore from backup —
/// is user-authored text nobody asked this write to replace.
#[test]
fn a_readme_created_after_this_write_observed_none_is_refused_rather_than_destroyed() {
    let tree = package_tree("readme-write-concurrent-creation");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let destination = directory.join("README.md");
    fs::remove_file(&destination).unwrap();
    const THEIRS: &[u8] = b"# restored from a backup\n";

    let plan = crate::plan(
        &write_readme(
            order_scope(directory.clone()),
            current_readme(&directory),
            String::from(AWKWARD),
        ),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("an absent Order README is creatable");

    let filesystem = another_writer_replacing(&destination, THEIRS, EditPoint::StagingWrite);
    let report = crate::commit(plan, &effects(&filesystem)).result;

    assert!(filesystem.disturbed(), "the outside write really landed");
    let Err(rejection) = report else {
        panic!("a README created underneath this write is refused: {report:?}");
    };
    let _error = rejection;
    assert_eq!(
        fs::read(&destination).unwrap(),
        THEIRS,
        "the other writer's README survives"
    );
    assert!(
        !directory.join(README_STAGE).exists(),
        "the refused creation leaves no staging file of its own behind"
    );
}

fn same_file(left: &Path, right: &Path) -> bool {
    fs::canonicalize(left).unwrap() == fs::canonicalize(right).unwrap()
}

/// A writer drafts against what it read. A document that changed since, appeared
/// since, or whose read established no content is not the one the draft replaces, so
/// planning refuses it and the other writer's document stays exactly as it is.
#[test]
fn a_write_planned_from_a_stale_original_read_is_refused_for_either_readme() {
    let tree = package_tree("readme-write-stale-original");
    let root = tree.path();
    let directory = order_with_readme(root, b"prior order notes\n");
    let order_readme = directory.join("README.md");
    let package_readme_path = root.join("README.md");

    let order_original = readme_original(&order_readme);
    let package_original = readme_original(&package_readme_path);
    fs::write(&order_readme, b"other order editor\n").unwrap();
    fs::write(&package_readme_path, b"other package editor\n").unwrap();

    for (label, error, path, theirs) in [
        (
            "order",
            crate::plan(
                &write_readme(
                    order_scope(directory.clone()),
                    order_original.clone(),
                    String::from(AWKWARD),
                ),
                &effects(&PlatformFileSystem),
            )
            .result
            .expect_err("a changed Order README refuses the stale draft"),
            &order_readme,
            &b"other order editor\n"[..],
        ),
        (
            "package",
            crate::plan(
                &write_readme(
                    package_scope(PathBuf::from(root)),
                    package_original.clone(),
                    String::from(AWKWARD),
                ),
                &effects(&PlatformFileSystem),
            )
            .result
            .expect_err("a changed Package README refuses the stale draft"),
            &package_readme_path,
            &b"other package editor\n"[..],
        ),
    ] {
        assert!(
            matches!(&error, Failure::PreconditionsChanged { location } if same_file(&location.absolute_path, path)),
            "{label}: {error:?}"
        );
        assert_eq!(fs::read(path).unwrap(), theirs, "{label}");
    }

    // A document observed absent authorizes only a creation.
    fs::remove_file(&order_readme).unwrap();
    let absent = readme_original(&order_readme);
    fs::write(&order_readme, b"").unwrap();
    assert!(
        matches!(
            crate::plan(&write_readme(order_scope(directory.clone()), absent.clone(), String::from(AWKWARD)), &effects(&PlatformFileSystem)).result,
            Err(Failure::PreconditionsChanged { location }) if same_file(&location.absolute_path, &order_readme)
        ),
        "a README created since the read is not the absence the writer observed"
    );

    // A read that established no content authorizes replacing nothing.
    fs::write(&order_readme, [0xff]).unwrap();
    assert!(matches!(
        crate::plan(&write_readme(order_scope(directory.clone()), ReadmeOriginal::Unavailable, String::from(AWKWARD)), &effects(&PlatformFileSystem)).result,
        Err(Failure::PreconditionsChanged { location }) if same_file(&location.absolute_path, &order_readme)
    ));
    assert_eq!(fs::read(&order_readme).unwrap(), [0xff]);
}
