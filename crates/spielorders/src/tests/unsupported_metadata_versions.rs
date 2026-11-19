//! Decision sources: Define durable metadata schemas and version evolution; Define
//! integrity, validation, and repair boundaries.
//!
//! A Package at a metadata version other than 1 is unsupported and diagnostic-only: this
//! build interprets version 1 alone and has no upgrade, so a version behind it is exactly
//! as uninterpretable as one ahead of it.

use crate::Failure;
use crate::tests::support::{
    FixedCalendar, PlatformFileSystem, TemporaryTree, adopted_order, adoption, align_order,
    create_order, create_vendor, created_vendor, current_readme, diagnosed_handle, diagnosed_order,
    diagnosed_package, edit_order, effects, initialized_package, opened, order_at, order_scope,
    package_scope, rebuild_index, record_path, remove_order, repair, set_vendor_hidden, snapshot,
    valid, validated_package, vendor_named, write_readme,
};
use crate::{Assignments, VendorRef};

use crate::{canonical::DiagnosticGroup, canonical::MetadataState};
use crate::{readme::Diagnostic, readme::DiagnosticCode};

use crate::repair_cases::RepairAction;

use std::fs;
use std::path::PathBuf;

const VENDOR_ID: &str = "5a2f1c88-9b30-4c17-a4de-2f6b7c8d9e01";
const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";
const UNRESOLVED_ORDER_ID: &str = "1c4d2e6f-3a5b-4c7d-8e9f-0a1b2c3d4e5f";
const ABSENT_VENDOR: &str = "e0990d31-06aa-42e9-97c8-088e3e894822";

/// A Package every capability would ordinarily serve: a Vendor, a healthy Order with a
/// README, an Order whose Vendor Reference resolves to nothing, an adoption candidate, a
/// staged version-1 marker beside the real one, and the handles the repair cases for the
/// unresolved reference and the staged marker carried while the Package was current.
struct Prepared {
    _tree: TemporaryTree,
    root: PathBuf,
    order: PathBuf,
    unresolved: PathBuf,
    candidate: PathBuf,
    handle: String,
    staged_marker_handle: String,
}

fn package_declaring(version: u64) -> Prepared {
    let (tree, root) = initialized_package(&format!("version-{version}"));
    created_vendor(&root, "Tarnwick", VENDOR_ID);
    let order = adopted_order(&root, "2026_08_22_Laser_components", ORDER_ID);
    fs::write(order.join("README.md"), "# Laser components\n").unwrap();
    let unresolved = adopted_order(&root, "2026_08_23_Optics", UNRESOLVED_ORDER_ID);
    let text = fs::read_to_string(record_path(&unresolved))
        .unwrap()
        .replace(
            "\"items\": []",
            &format!("\"vendor_id\": \"{ABSENT_VENDOR}\",\n  \"items\": []"),
        );
    fs::write(record_path(&unresolved), text).unwrap();
    let handle = diagnosed_handle(&unresolved, DiagnosticCode::VendorReferenceUnresolved);
    let candidate = root.join("Orders/2026_08_24_Legacy");
    fs::create_dir(&candidate).unwrap();
    fs::write(
        root.join(".spielorders/.version.json.spielorders-stage"),
        "{\n  \"metadata_version\": 1\n}\n",
    )
    .unwrap();
    let staged_marker_handle = diagnosed_package(&root, &PlatformFileSystem)
        .result
        .expect("the current Package diagnoses")
        .cases
        .iter()
        .find(|case| {
            case.actions
                .contains(&RepairAction::PromoteStagingCandidate)
        })
        .expect("a current Package offers to promote the staged marker")
        .handle
        .to_owned();
    fs::write(
        root.join(".spielorders/version.json"),
        format!("{{\n  \"metadata_version\": {version}\n}}\n"),
    )
    .unwrap();
    Prepared {
        _tree: tree,
        root,
        order,
        unresolved,
        candidate,
        handle,
        staged_marker_handle,
    }
}

/// Whether the diagnosis names the marker's version as the one this build does not read.
fn names_the_version(groups: &[DiagnosticGroup], version: u64) -> bool {
    let findings: Vec<Diagnostic> = groups
        .iter()
        .flat_map(|item| item.diagnostics.iter().cloned())
        .collect();
    names_the_version_among(&findings, version)
}

fn names_the_version_among(findings: &[Diagnostic], version: u64) -> bool {
    findings.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ManagedArtifactWrongVersion
            && matches!(
                &diagnostic.evidence,
                crate::readme::DiagnosticEvidence::ManagedArtifact {
                    observed_version: Some(observed),
                    ..
                } if *observed == version
            )
    })
}

fn refused<T>(result: Result<T, Failure>) -> Failure {
    match result {
        Ok(_) => panic!("the capability refuses a Package at an unsupported version"),
        Err(error) => error,
    }
}

fn unsupported(failure: &Failure) -> bool {
    matches!(failure, Failure::PackageMetadataUnsupported(_))
}

/// Every load reports the Package as diagnostic-only and names the version it declares.
#[test]
fn every_load_reports_another_version_as_unsupported_and_diagnostic_only() {
    for version in [0, 2] {
        let prepared = package_declaring(version);
        let root = &prepared.root;
        let before = snapshot(root);

        let Err(Failure::PackageMetadataUnsupported(opened)) =
            opened(root, &PlatformFileSystem).result
        else {
            panic!("version {version}: opening is diagnostic-only")
        };
        assert_eq!(opened.metadata_state, MetadataState::Unsupported);
        assert!(names_the_version(&opened.diagnostic_groups, version));

        assert_eq!(snapshot(root), before, "version {version}: no load writes");
    }
}

/// Validation and repair diagnose the version, offer no repair for it, and the repair a
/// current Package offered is refused once the Package is not current.
#[test]
fn validation_and_repair_diagnose_another_version_and_repair_nothing() {
    for version in [0, 2] {
        let prepared = package_declaring(version);
        let root = &prepared.root;
        let before = snapshot(root);

        let validation = validated_package(root, &PlatformFileSystem);
        let Err(Failure::PackageInvalid(validation)) = validation.result else {
            panic!("version {version}: exhaustive validation finds the Package invalid")
        };
        assert!(names_the_version(&validation.diagnostic_groups, version));

        let diagnosis = diagnosed_package(root, &PlatformFileSystem)
            .result
            .expect("the Package diagnoses");
        assert!(!valid(&diagnosis), "version {version}");
        assert!(
            diagnosis.cases.is_empty(),
            "version {version}: no repair interprets an unsupported version: {:?}",
            diagnosis.cases
        );
        let failure = refused(diagnosed_order(&prepared.order, &PlatformFileSystem).result);
        assert!(unsupported(&failure), "version {version}: {failure:?}");

        for (label, code) in [
            (
                "semantic repair",
                refused(
                    crate::plan(
                        &repair(
                            order_scope(PathBuf::from(&prepared.unresolved)),
                            String::from(&prepared.handle),
                            RepairAction::ClearVendorReference,
                            Assignments::new(),
                        ),
                        &effects(&PlatformFileSystem),
                    )
                    .result,
                ),
            ),
            (
                "metadata repair",
                refused(
                    crate::plan(
                        &repair(
                            order_scope(PathBuf::from(&prepared.order)),
                            "any",
                            RepairAction::PromoteStagingCandidate,
                            Assignments::new(),
                        ),
                        &effects(&PlatformFileSystem),
                    )
                    .result,
                ),
            ),
        ] {
            assert!(unsupported(&code), "version {version}: {label}: {code:?}");
        }
        // The staged version-1 marker a current Package offered to promote would rewrite
        // the version this Package declares, so it is no longer offered.
        let failure = refused(
            crate::plan(
                &repair(
                    package_scope(PathBuf::from(root)),
                    String::from(&prepared.staged_marker_handle),
                    RepairAction::PromoteStagingCandidate,
                    Assignments::new(),
                ),
                &effects(&PlatformFileSystem),
            )
            .result,
        );
        assert!(
            matches!(failure, Failure::UnknownCase { .. }),
            "version {version}: {failure:?}"
        );

        assert_eq!(
            snapshot(root),
            before,
            "version {version}: nothing is written"
        );
    }
}

/// Every read and every mutation refuses the Package, and nothing is written.
#[test]
fn every_read_and_mutation_refuses_another_version() {
    for version in [0, 2] {
        let prepared = package_declaring(version);
        let root = &prepared.root;
        let order = &prepared.order;
        let fs = &PlatformFileSystem;
        let calendar = FixedCalendar::on(2026, 9, 1);
        let package_readme = current_readme(root);
        let order_readme = current_readme(order);
        let before = snapshot(root);

        for (label, code) in [
            ("order read", refused(order_at(order, fs).result)),
            (
                "vendor read",
                refused(vendor_named(root, "Tarnwick", fs).result),
            ),
            (
                "order creation",
                refused(
                    crate::plan(
                        &create_order(
                            PathBuf::from(root),
                            Assignments::new().assign("title", "Optical table"),
                        ),
                        &effects(fs).dated(&calendar),
                    )
                    .result,
                ),
            ),
            (
                "order adoption",
                refused(
                    crate::plan(
                        &adoption(
                            PathBuf::from(root),
                            PathBuf::from(&prepared.candidate),
                            &(
                                Assignments::new()
                                    .assign("title", "Legacy")
                                    .assign("date_ordered", "2018-02-20")
                                    .assign("status", "received"),
                                Vec::new(),
                            ),
                        ),
                        &effects(fs),
                    )
                    .result,
                ),
            ),
            (
                "order edit",
                refused(
                    crate::plan(
                        &edit_order(
                            PathBuf::from(order),
                            Assignments::new().assign("title", "Renamed"),
                        ),
                        &effects(fs).dated(&calendar),
                    )
                    .result,
                ),
            ),
            (
                "order alignment",
                refused(crate::plan(&align_order(PathBuf::from(order)), &effects(fs)).result),
            ),
            (
                "order removal",
                refused(crate::plan(&remove_order(PathBuf::from(order)), &effects(fs)).result),
            ),
            (
                "package README write",
                refused(
                    crate::plan(
                        &write_readme(
                            package_scope(PathBuf::from(root)),
                            package_readme.clone(),
                            "prose\n",
                        ),
                        &effects(fs),
                    )
                    .result,
                ),
            ),
            (
                "order README write",
                refused(
                    crate::plan(
                        &write_readme(
                            order_scope(PathBuf::from(order)),
                            order_readme.clone(),
                            "prose\n",
                        ),
                        &effects(fs),
                    )
                    .result,
                ),
            ),
            (
                "vendor creation",
                refused(
                    crate::plan(
                        &create_vendor(
                            PathBuf::from(root),
                            Assignments::new().assign("name", "Nethermoor"),
                        ),
                        &effects(fs),
                    )
                    .result,
                ),
            ),
            (
                "vendor hide",
                refused(
                    crate::plan(
                        &set_vendor_hidden(
                            PathBuf::from(root),
                            VendorRef::Id(String::from(VENDOR_ID)),
                            true,
                        ),
                        &effects(fs),
                    )
                    .result,
                ),
            ),
            (
                "index rebuild",
                refused(crate::plan(&rebuild_index(PathBuf::from(root)), &effects(fs)).result),
            ),
        ] {
            assert!(unsupported(&code), "version {version}: {label}: {code:?}");
        }

        assert_eq!(
            snapshot(root),
            before,
            "version {version}: nothing is written"
        );
    }
}
