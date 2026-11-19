//! Decision sources: Define the SpielOrders package topology; Define the Rust CLI
//! capability contract; Define legacy Order adoption and metadata bootstrapping;
//! Define durable metadata schemas and version evolution.

use crate::Failure;
use crate::Report;
use crate::canonical::PackageIndexState;
use crate::tests::support::assert_reported_beside;

use crate::ports::FileChangeEvidence;
use crate::tests::support::{
    Answer, FailedCommit, Operation, PlatformFileSystem, Script, ScriptedFileSystem, TemporaryTree,
    effects, initialization, initialize, opened,
};
use crate::{paths::ConfiguredPath, paths::ConfiguredPathFailure};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedBoundary {
    CreateDirectory,
    WriteStage,
    WriteStageWithConcurrentEvidence,
    ReadStage,
    CorruptStage,
    SyncStage,
    InstallDestination,
    DestinationAppearsBeforeInstall,
    RemoveInstalledStage,
    WriteVersionStage,
    ObserveDestination,
    ObserveStageCleanup,
    ObserveCreatedFileCleanup,
    ObserveCreatedDirectoryCleanup,
    RemoveStage,
    RemoveCreatedFile,
    RemoveCreatedDirectory,
    ReplaceCreatedFileBeforeCleanup,
}

fn is_stage(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.as_encoded_bytes().ends_with(b".spielorders-stage"))
}

const CONFIG: &str = ".spielorders/config.toml";

/// Real effects, except that `primary` fails once and, after it, `cleanup` fails once
/// in the rollback that answers it.
fn injected(primary: InjectedBoundary, cleanup: Option<InjectedBoundary>) -> ScriptedFileSystem {
    let filesystem = ScriptedFileSystem::new();
    if let Some(cleanup) = cleanup {
        match cleanup {
            InjectedBoundary::ReplaceCreatedFileBeforeCleanup => {
                script(&filesystem, cleanup)
                    .after_an_injected_failure()
                    .once()
                    .preceded_by(|outside| {
                        outside.write(&outside.call().path, b"concurrent canonical bytes")
                    })
                    .proceeds();
            }
            _ => script(&filesystem, cleanup)
                .after_an_injected_failure()
                .once()
                .fails(),
        }
    }
    let primary_script = script(&filesystem, primary).once();
    match primary {
        InjectedBoundary::CorruptStage => {
            primary_script.answers(Answer::Bytes(b"corrupted candidate".to_vec()))
        }
        InjectedBoundary::WriteStageWithConcurrentEvidence => primary_script
            .preceded_by(|outside| {
                outside.write(&outside.call().path, b"concurrent staging evidence")
            })
            .fails(),
        InjectedBoundary::DestinationAppearsBeforeInstall => primary_script
            .preceded_by(|outside| {
                outside.write(&outside.call().path, b"concurrent destination evidence")
            })
            .proceeds(),
        _ => primary_script.fails(),
    }
    filesystem
}

/// The call a boundary names.
fn script(filesystem: &ScriptedFileSystem, boundary: InjectedBoundary) -> Script<'_> {
    match boundary {
        InjectedBoundary::CreateDirectory => filesystem.on(Operation::CreateDirectory),
        InjectedBoundary::WriteStage | InjectedBoundary::WriteStageWithConcurrentEvidence => {
            filesystem.on(Operation::WriteNew).matching(is_stage)
        }
        InjectedBoundary::ReadStage | InjectedBoundary::CorruptStage => {
            filesystem.on(Operation::Read).matching(is_stage)
        }
        InjectedBoundary::SyncStage => filesystem.on(Operation::SyncFile).matching(is_stage),
        InjectedBoundary::InstallDestination
        | InjectedBoundary::DestinationAppearsBeforeInstall => {
            filesystem.on(Operation::LinkNewFile)
        }
        InjectedBoundary::RemoveInstalledStage | InjectedBoundary::RemoveStage => {
            filesystem.on(Operation::RemoveFile).matching(is_stage)
        }
        InjectedBoundary::WriteVersionStage => filesystem
            .on(Operation::WriteNew)
            .ending_in(".version.json.spielorders-stage"),
        InjectedBoundary::ObserveDestination => filesystem
            .on(Operation::SymlinkMetadata)
            .ending_in(".spielorders"),
        InjectedBoundary::ObserveStageCleanup => filesystem
            .on(Operation::ObserveFileChange)
            .matching(is_stage),
        InjectedBoundary::ObserveCreatedFileCleanup
        | InjectedBoundary::ReplaceCreatedFileBeforeCleanup => filesystem
            .on(Operation::ObserveFileChange)
            .ending_in(CONFIG),
        InjectedBoundary::ObserveCreatedDirectoryCleanup => filesystem
            .on(Operation::ObserveFileChange)
            .ending_in("Orders"),
        InjectedBoundary::RemoveCreatedFile => {
            filesystem.on(Operation::RemoveFile).ending_in(CONFIG)
        }
        InjectedBoundary::RemoveCreatedDirectory => filesystem.on(Operation::RemoveDirectory),
    }
}

#[test]
fn successful_commit_creates_a_healthy_current_package_and_preserves_other_content() {
    let tree = TemporaryTree::new("initialization-commit");
    let root = tree.path();
    fs::create_dir(root.join("ordinary-directory")).unwrap();
    fs::write(
        root.join("ordinary-directory/sentinel.bin"),
        b"\0opaque\xff",
    )
    .unwrap();
    fs::write(root.join("notes.txt"), b"existing notes\n").unwrap();
    let plan = crate::plan(
        &initialize(PathBuf::from(root)),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the Package plan is valid");

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Ok(Report::Committed { .. }) = report else {
        panic!("a valid complete plan commits")
    };
    assert_eq!(fs::read(root.join("README.md")).unwrap(), b"");
    assert_eq!(
        fs::read(root.join("ordinary-directory/sentinel.bin")).unwrap(),
        b"\0opaque\xff"
    );
    assert_eq!(
        fs::read(root.join("notes.txt")).unwrap(),
        b"existing notes\n"
    );

    let Ok(package) = opened(root, &PlatformFileSystem).result else {
        panic!("the loader accepts initialized canonical state")
    };
    assert_eq!(package.contents.metadata_version, 1);
    assert_eq!(package.contents.index.state, PackageIndexState::Current);
    assert!(package.contents.healthy_orders.is_empty());
    assert!(package.contents.vendors.is_empty());
}

#[test]
fn a_configured_path_collision_after_planning_rejects_before_the_first_write() {
    let tree = TemporaryTree::new("initialization-stale-plan");
    let root = tree.path();
    let plan = crate::plan(
        &initialize(PathBuf::from(root)),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the initial dry-run plan is valid");
    fs::write(root.join("Removed"), b"late collision").unwrap();

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;

    let Err(rejection) = report else {
        panic!("a changed complete-plan precondition must reject")
    };
    let Failure::PreconditionsChanged { location } = rejection else {
        panic!("the rejection identifies a stale precondition")
    };
    assert_eq!(
        location.absolute_path,
        fs::canonicalize(root).unwrap().join("Removed")
    );
    assert!(!root.join(".spielorders").exists());
    assert!(!root.join("Orders").exists());
    assert!(!root.join("README.md").exists());
    assert_eq!(fs::read(root.join("Removed")).unwrap(), b"late collision");
}

#[test]
fn configured_paths_round_trip_through_the_written_configuration() {
    let tree = TemporaryTree::new("initialization-config-candidate");
    let options = ("Orders\u{1b}", "Removed\"");

    let plan = crate::plan(
        &initialization(tree.path(), &options),
        &effects(&PlatformFileSystem),
    )
    .result
    .expect("the configured paths satisfy the closed relative-path grammar");
    assert!(!tree.path().join(".spielorders").exists());

    let report = crate::commit(plan, &effects(&PlatformFileSystem)).result;
    assert!(matches!(report, Ok(Report::Committed { .. })), "{report:?}");
    let Ok(package) = opened(tree.path(), &PlatformFileSystem).result else {
        panic!("the Package opens with the configured paths it was initialized with")
    };
    let configured = |path: &Option<crate::readme::Location>| {
        path.as_ref()
            .and_then(|location| location.package_relative_path.clone())
    };
    assert_eq!(
        configured(&package.contents.context.orders_directory).as_deref(),
        Some("Orders\u{1b}")
    );
    assert_eq!(
        configured(&package.contents.context.removed_orders_directory).as_deref(),
        Some("Removed\"")
    );
}

#[test]
fn incomplete_staging_cleanup_is_explicit_in_the_rejection() {
    let tree = TemporaryTree::new("initialization-incomplete-cleanup");
    let filesystem = injected_with_cleanup(
        InjectedBoundary::ReadStage,
        InjectedBoundary::ObserveStageCleanup,
    );
    let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
        .result
        .expect("the fault is armed only for commit staging");

    let report = crate::commit(plan, &effects(&filesystem)).result;

    let Err(rejection) = report else {
        panic!("the injected read failure rejects initialization")
    };
    assert_reported_beside(&rejection);
    assert_eq!(rejection.cleanup_failures().len(), 2);
    assert!(
        tree.path()
            .join(".spielorders/.config.toml.spielorders-stage")
            .exists(),
        "unremoved staging evidence remains available for explicit recovery"
    );
}

#[test]
fn precommit_observation_failure_is_not_reported_as_an_ordinary_state_change() {
    let tree = TemporaryTree::new("initialization-precommit-observation");
    let filesystem = ScriptedFileSystem::new();
    let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
        .result
        .expect("planning succeeds before the observation fault is armed");
    script(&filesystem, InjectedBoundary::ObserveDestination)
        .once()
        .fails();

    let report = crate::commit(plan, &effects(&filesystem)).result;

    let Err(rejection) = report else {
        panic!("the injected observation failure rejects initialization")
    };
    assert_reported_beside(&rejection);
    assert!(rejection.cleanup_failures().is_empty());
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("Orders").exists());
}

#[test]
fn unsafe_roots_paths_collisions_and_links_reject_without_creating_artifacts() {
    let managed = TemporaryTree::new("initialization-reject-managed");
    fs::write(managed.path().join(".spielorders"), b"damaged evidence").unwrap();
    assert!(matches!(
        crate::plan(&initialize(managed.path()), &effects(&PlatformFileSystem)).result,
        Err(Failure::AlreadyAPackage { .. })
    ));
    assert_eq!(
        fs::read(managed.path().join(".spielorders")).unwrap(),
        b"damaged evidence"
    );

    let file_root = TemporaryTree::new("initialization-reject-file-root");
    let ordinary_file = file_root.path().join("not-a-directory");
    fs::write(&ordinary_file, b"ordinary file").unwrap();
    assert!(matches!(
        crate::plan(
            &initialize(ordinary_file.clone()),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::RootNotOrdinaryDirectory)
    ));
    assert_eq!(fs::read(&ordinary_file).unwrap(), b"ordinary file");

    let invalid = TemporaryTree::new("initialization-reject-invalid");
    for (options, refused, expected_failure) in [
        (
            (".", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::CurrentDirectory,
        ),
        (
            ("../Orders", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ParentTraversal,
        ),
        (
            (".spielorders/Orders", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ReservedMetadata,
        ),
        (
            ("/outside", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::Absolute,
        ),
        (
            ("Orders//Active", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::EmptyComponent,
        ),
        (
            ("Orders\\Active", "Removed"),
            ConfiguredPath::Orders,
            ConfiguredPathFailure::PlatformSeparator,
        ),
        (
            ("Orders", "Removed/Nested"),
            ConfiguredPath::Removed,
            ConfiguredPathFailure::NotDirectChild,
        ),
    ] {
        let error = crate::plan(
            &initialization(invalid.path(), &options),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("an invalid configured path is refused");
        let Failure::InvalidConfiguredPath { path, failure, .. } = error else {
            panic!("the refusal names the configured path at fault, not {error:?}")
        };
        assert_eq!(path, refused);
        assert_eq!(failure, expected_failure);
        assert_eq!(fs::read_dir(invalid.path()).unwrap().count(), 0);
    }

    let overlapping = TemporaryTree::new("initialization-reject-overlap");
    assert!(matches!(
        crate::plan(
            &initialization(overlapping.path(), &("Removed/Active", "Removed")),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::ManagedPathsOverlap { .. })
    ));
    assert_eq!(fs::read_dir(overlapping.path()).unwrap().count(), 0);

    let collisions = TemporaryTree::new("initialization-reject-collision");
    fs::write(collisions.path().join("Orders"), b"ordinary file").unwrap();
    fs::create_dir(collisions.path().join("README.md")).unwrap();
    assert!(matches!(
        crate::plan(
            &initialize(collisions.path()),
            &effects(&PlatformFileSystem)
        )
        .result,
        Err(Failure::DestinationCollision { .. })
    ));
    assert_eq!(
        fs::read(collisions.path().join("Orders")).unwrap(),
        b"ordinary file"
    );
    assert!(collisions.path().join("README.md").is_dir());
    assert!(!collisions.path().join(".spielorders").exists());

    #[cfg(unix)]
    {
        let linked = TemporaryTree::new("initialization-reject-link");
        let outside = TemporaryTree::new("initialization-link-target");
        std::os::unix::fs::symlink(outside.path(), linked.path().join("Orders")).unwrap();
        assert!(matches!(
            crate::plan(&initialize(linked.path()), &effects(&PlatformFileSystem)).result,
            Err(Failure::DisallowedLink { .. })
        ));
        assert!(!linked.path().join(".spielorders").exists());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[test]
fn configured_readme_and_staging_collisions_reject_during_planning() {
    for options in [
        ("README.md", "Removed"),
        ("README.md/Active", "Removed"),
        ("Orders", "README.md"),
        (".README.md.spielorders-stage", "Removed"),
        ("Orders", ".README.md.spielorders-stage"),
        ("readme.md", "Removed"),
        ("Orders", "ReadMe.Md"),
        (".readme.md.spielorders-stage", "Removed"),
    ] {
        let tree = TemporaryTree::new("initialization-readme-role-collision");
        assert!(matches!(
            crate::plan(
                &initialization(tree.path(), &options),
                &effects(&PlatformFileSystem)
            )
            .result,
            Err(Failure::PlannedPathCollision { .. })
        ));
        assert_eq!(fs::read_dir(tree.path()).unwrap().count(), 0);
    }

    let tree = TemporaryTree::new("initialization-existing-readme-stage");
    let stage = tree.path().join(".README.md.spielorders-stage");
    fs::write(&stage, b"abandoned evidence").unwrap();
    assert!(matches!(
        crate::plan(&initialize(tree.path()), &effects(&PlatformFileSystem)).result,
        Err(Failure::DestinationCollision { .. })
    ));
    assert_eq!(fs::read(stage).unwrap(), b"abandoned evidence");
    assert!(!tree.path().join(".spielorders").exists());
}

#[test]
fn rollback_reports_rather_than_removes_artifacts_whose_creation_it_cannot_prove() {
    let tree = TemporaryTree::new("initialization-uncertain-evidence");
    let root = fs::canonicalize(tree.path()).unwrap();
    let filesystem = offering_no_change_evidence(ScriptedFileSystem::new());

    let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
        .result
        .expect("planning needs no ownership proof for a new entry");

    let report = crate::commit(plan, &effects(&filesystem)).result;

    let Err(rejection) = report else {
        panic!("a failed creation rejects")
    };
    let failed = rejection;
    let (failure, cleanup_failures) = (failed.failure(), failed.cleanup_failures());
    assert_reported_beside(&failed);
    assert_eq!(
        failure.location.absolute_path,
        root.join(".spielorders/.config.toml.spielorders-stage")
            .as_path()
    );

    // Each artifact created so far is named as one this operation
    // declined to remove — rather than deleted on an ownership claim it cannot support.
    for cleanup in cleanup_failures {
        assert!(
            cleanup.location.absolute_path.exists(),
            "{cleanup:?} is left in place"
        );
    }
    assert!(root.join(".spielorders/config.toml").is_file());
    assert!(
        root.join(".spielorders/.config.toml.spielorders-stage")
            .is_file()
    );
    assert!(root.join("Orders").is_dir());
    assert!(!root.join("README.md").exists());
}

#[test]
fn configured_paths_naming_the_metadata_directory_are_refused_before_any_write() {
    for (options, refused) in [
        ((".spielorders", "Removed"), ConfiguredPath::Orders),
        ((".SpielOrders", "Removed"), ConfiguredPath::Orders),
        ((".SPIELORDERS", "Removed"), ConfiguredPath::Orders),
        ((".SpielOrders/Active", "Removed"), ConfiguredPath::Orders),
        (("Orders", ".spielorders"), ConfiguredPath::Removed),
        (("Orders", ".SpielOrders"), ConfiguredPath::Removed),
        (("Orders", ".SPIELORDERS"), ConfiguredPath::Removed),
    ] {
        let tree = TemporaryTree::new("initialization-reserved-metadata-name");
        let error = crate::plan(
            &initialization(tree.path(), &options),
            &effects(&PlatformFileSystem),
        )
        .result
        .expect_err("a configured path naming the metadata directory is refused");

        let Failure::InvalidConfiguredPath { path, failure, .. } = error else {
            panic!("the refusal names the reserved configured path, not {error:?}")
        };
        assert_eq!(path, refused);
        assert_eq!(failure, ConfiguredPathFailure::ReservedMetadata);
        assert_eq!(fs::read_dir(tree.path()).unwrap().count(), 0);
    }
}

#[test]
fn every_creation_and_installation_phase_failure_rolls_back_cleanly() {
    // The first planned file is the configuration, so every staging fault lands on its
    // stage and the installation fault lands on the configuration itself.
    let first_stage = ".spielorders/.config.toml.spielorders-stage";
    let cases = [
        (InjectedBoundary::CreateDirectory, ".spielorders"),
        (InjectedBoundary::WriteStage, first_stage),
        (InjectedBoundary::ReadStage, first_stage),
        (InjectedBoundary::CorruptStage, first_stage),
        (InjectedBoundary::SyncStage, first_stage),
        (
            InjectedBoundary::InstallDestination,
            ".spielorders/config.toml",
        ),
        (InjectedBoundary::RemoveInstalledStage, first_stage),
    ];

    for (boundary, expected_relative_path) in cases {
        let tree = TemporaryTree::new("initialization-phase-rollback");
        fs::write(tree.path().join("untouched.bin"), b"untouched").unwrap();
        let root = fs::canonicalize(tree.path()).unwrap();
        let filesystem = injected(boundary, None);
        let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
            .result
            .expect("the fault is armed only for commit effects");

        let report = crate::commit(plan, &effects(&filesystem)).result;

        let Err(rejection) = report else {
            panic!("{boundary:?} must reject")
        };
        let failed = rejection;
        let (failure, cleanup_failures) = (failed.failure(), failed.cleanup_failures());
        assert_eq!(
            failure.location.absolute_path,
            root.join(expected_relative_path).as_path(),
            "{boundary:?}"
        );
        assert_reported_beside(&failed);
        assert!(
            cleanup_failures.is_empty(),
            "{boundary:?}: {cleanup_failures:?}"
        );
        assert_eq!(
            fs::read(tree.path().join("untouched.bin")).unwrap(),
            b"untouched"
        );
        assert!(!tree.path().join(".spielorders").exists(), "{boundary:?}");
        assert!(!tree.path().join("Orders").exists(), "{boundary:?}");
        assert!(!tree.path().join("README.md").exists(), "{boundary:?}");
    }
}

#[test]
fn failure_after_one_canonical_creation_restores_the_absent_state_and_existing_readme() {
    let tree = TemporaryTree::new("initialization-partial-rollback");
    fs::write(tree.path().join("README.md"), b"user README\r\n\xff").unwrap();
    let filesystem = injected(InjectedBoundary::WriteVersionStage, None);
    let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
        .result
        .expect("the Package plan preserves the existing README");

    let report = crate::commit(plan, &effects(&filesystem)).result;

    let Err(rejection) = report else {
        panic!("the later staged-file failure rejects initialization")
    };
    assert_reported_beside(&rejection);
    assert!(rejection.cleanup_failures().is_empty());
    assert_eq!(
        fs::read(tree.path().join("README.md")).unwrap(),
        b"user README\r\n\xff"
    );
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("Orders").exists());
}

#[test]
fn every_cleanup_removal_failure_is_reported_beside_the_initiating_failure() {
    let cases = [
        (
            InjectedBoundary::WriteVersionStage,
            InjectedBoundary::ObserveCreatedFileCleanup,
        ),
        (
            InjectedBoundary::WriteStage,
            InjectedBoundary::ObserveCreatedDirectoryCleanup,
        ),
        (InjectedBoundary::ReadStage, InjectedBoundary::RemoveStage),
        (
            InjectedBoundary::WriteVersionStage,
            InjectedBoundary::RemoveCreatedFile,
        ),
        (
            InjectedBoundary::WriteStage,
            InjectedBoundary::RemoveCreatedDirectory,
        ),
    ];

    for (primary, cleanup) in cases {
        let tree = TemporaryTree::new("initialization-cleanup-phase");
        let filesystem = injected_with_cleanup(primary, cleanup);
        let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
            .result
            .expect("the cleanup fault is armed only after commit begins");

        let Err(failed) = crate::commit(plan, &effects(&filesystem)).result else {
            panic!("{cleanup:?} follows an injected commit rejection")
        };
        assert_reported_beside(&failed);
        assert!(
            !failed.cleanup_failures().is_empty(),
            "{primary:?} then {cleanup:?}: what cleanup left is named"
        );
    }
}

#[test]
fn rolling_back_a_nested_orders_directory_leaves_no_created_parent_behind() {
    let tree = TemporaryTree::new("initialization-nested-directory-rollback");
    let filesystem = injected(InjectedBoundary::WriteStage, None);
    let plan = crate::plan(
        &initialization(tree.path(), &("Purchases/Active", "Removed")),
        &effects(&filesystem),
    )
    .result
    .expect("the nested Orders plan is valid");

    let Err(rejection) = crate::commit(plan, &effects(&filesystem)).result else {
        panic!("the injected staging failure rejects")
    };
    let failed = rejection;
    let cleanup_failures = failed.cleanup_failures();
    assert!(cleanup_failures.is_empty(), "{cleanup_failures:?}");
    assert!(!tree.path().join("Purchases").exists());
    assert!(!tree.path().join(".spielorders").exists());
    assert!(!tree.path().join("README.md").exists());
}

#[test]
fn rollback_never_deletes_unowned_staging_or_replaced_canonical_bytes() {
    let stage_tree = TemporaryTree::new("initialization-concurrent-stage");
    let stage_filesystem = injected(InjectedBoundary::WriteStageWithConcurrentEvidence, None);
    let stage_plan = crate::plan(&initialize(stage_tree.path()), &effects(&stage_filesystem))
        .result
        .unwrap();
    let Err(failed) = crate::commit(stage_plan, &effects(&stage_filesystem)).result else {
        panic!("the injected staging collision rejects")
    };
    assert_reported_beside(&failed);
    let stage = stage_tree
        .path()
        .join(".spielorders/.config.toml.spielorders-stage");
    assert_eq!(fs::read(&stage).unwrap(), b"concurrent staging evidence");

    let canonical_tree = TemporaryTree::new("initialization-concurrent-canonical");
    let canonical_filesystem = injected_with_cleanup(
        InjectedBoundary::WriteVersionStage,
        InjectedBoundary::ReplaceCreatedFileBeforeCleanup,
    );
    let canonical_plan = crate::plan(
        &initialize(canonical_tree.path()),
        &effects(&canonical_filesystem),
    )
    .result
    .unwrap();
    let Err(failed) = crate::commit(canonical_plan, &effects(&canonical_filesystem)).result else {
        panic!("the later injected staging failure rejects")
    };
    assert_reported_beside(&failed);
    let config = canonical_tree.path().join(".spielorders/config.toml");
    assert_eq!(fs::read(&config).unwrap(), b"concurrent canonical bytes");
}

#[test]
fn cleanup_does_not_claim_an_unrelated_file_with_the_same_initial_change_time() {
    let tree = TemporaryTree::new("initialization-equal-status-times");
    let filesystem = ScriptedFileSystem::new();
    let root = fs::canonicalize(tree.path()).unwrap();
    let config = root.join(".spielorders/config.toml");
    let version = root.join(".spielorders/version.json");
    let version_stage = root.join(".spielorders/.version.json.spielorders-stage");
    let initial = FileChangeEvidence::Comparable {
        comparison_domain: 1,
        sequence: 10,
    };
    filesystem
        .on(Operation::ObserveFileChange)
        .at(&config)
        .reports_evidence(initial);
    filesystem
        .on(Operation::ObserveFileChange)
        .at(&version_stage)
        .reports_evidence(initial);
    filesystem
        .on(Operation::ObserveFileChange)
        .at(&config)
        .after_an_injected_failure()
        .reports_evidence(FileChangeEvidence::Comparable {
            comparison_domain: 1,
            sequence: 20,
        });
    let changed = config.clone();
    filesystem
        .on(Operation::LinkNewFile)
        .at(&version)
        .preceded_by(move |outside| outside.write(&changed, b"Another writer's config"))
        .fails();
    let plan = crate::plan(&initialize(&root), &effects(&filesystem))
        .result
        .unwrap();

    let failure = crate::commit(plan, &effects(&filesystem))
        .result
        .unwrap_err();

    assert_eq!(fs::read(&config).unwrap(), b"Another writer's config");
    assert!(matches!(
        failure,
        Failure::CommitFailedCleanupIncomplete { .. }
    ));
    assert!(
        failure
            .cleanup_failures()
            .iter()
            .any(|left| left.location.absolute_path == config)
    );
    assert!(!version.exists());
    assert!(!version_stage.exists());
}

#[test]
fn concurrent_destination_appearance_is_never_overwritten_or_owned_by_rollback() {
    let tree = TemporaryTree::new("initialization-concurrent-destination");
    let filesystem = injected(InjectedBoundary::DestinationAppearsBeforeInstall, None);
    let plan = crate::plan(&initialize(tree.path()), &effects(&filesystem))
        .result
        .expect("the destination appears only after the commit precondition check");

    let Err(rejection) = crate::commit(plan, &effects(&filesystem)).result else {
        panic!("an atomically detected destination collision rejects initialization")
    };
    assert_reported_beside(&rejection);
    assert_eq!(
        fs::read(tree.path().join(".spielorders/config.toml")).unwrap(),
        b"concurrent destination evidence"
    );
    assert!(!tree.path().join(".spielorders/version.json").exists());
}

fn injected_with_cleanup(
    primary: InjectedBoundary,
    cleanup: InjectedBoundary,
) -> ScriptedFileSystem {
    injected(primary, Some(cleanup))
}

/// Observes what an entry is but supplies no comparable change evidence, the position
/// of any platform whose timestamps are too coarse to tell one write from another.
fn offering_no_change_evidence(filesystem: ScriptedFileSystem) -> ScriptedFileSystem {
    filesystem
        .on(Operation::ObserveFileChange)
        .reports_evidence(FileChangeEvidence::Uncertain);
    filesystem
}
