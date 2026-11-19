//! A repair case handle names the evidence its condition was diagnosed from, so a
//! confirmation stops naming a case once that evidence moves even where the code and
//! the path stay the same.
//!
//! Decision sources: Define integrity, validation, and repair boundaries; Define the
//! Order record and lifecycle contract; Define Vendor identity and historical
//! semantics.

use crate::tests::support::{
    PlatformFileSystem, adopted_order, diagnosed_order, findings, initialized_package,
    record_path as record,
};
use crate::{readme::DiagnosticCode, readme::DiagnosticEvidence};
use std::fs;
use std::path::{Path, PathBuf};

const ORDER_ID: &str = "8b6e3f0a-8ec5-4d47-95e1-412652a56d16";

/// The handle of the one case the current diagnosis offers for `code`, and where it
/// was diagnosed.
fn case(directory: &Path, code: DiagnosticCode) -> (String, String) {
    let diagnosis = diagnosed_order(directory, &PlatformFileSystem)
        .result
        .expect("diagnosable");
    let case = diagnosis
        .cases
        .iter()
        .find(|case| case.code == code)
        .unwrap_or_else(|| panic!("a {code:?} case is offered"));
    (
        case.handle.to_owned(),
        case.location
            .package_relative_path
            .as_ref()
            .expect("the case is inside the Package")
            .to_owned(),
    )
}

/// Two diagnoses with one code at one path, which differ only in their evidence.
fn assert_distinguished(first: (String, String), second: (String, String), again: String) {
    assert_eq!(first.1, second.1, "same path: only the evidence differs");
    assert_ne!(
        first.0, second.0,
        "different evidence names a different case"
    );
    assert_eq!(
        first.0, again,
        "the same evidence names the same case again"
    );
}

fn order(label: &str) -> (crate::tests::support::TemporaryTree, PathBuf) {
    let (tree, root) = initialized_package(label);
    let directory = adopted_order(&root, "2026_08_22_A", ORDER_ID);
    (tree, directory)
}

#[test]
fn one_code_and_path_still_distinguishes_a_different_unresolved_vendor() {
    let (_tree, directory) = order("plan-evidence-unresolved-vendor");
    let original = fs::read_to_string(record(&directory)).unwrap();
    let point_at = |vendor_id: &str| {
        let text = original.replace(
            "\"items\": []",
            &format!("\"vendor_id\": \"{vendor_id}\",\n  \"items\": []"),
        );
        fs::write(record(&directory), text).unwrap();
        case(&directory, DiagnosticCode::VendorReferenceUnresolved)
    };

    let first = point_at("11111111-1111-4111-8111-111111111111");
    let second = point_at("22222222-2222-4222-8222-222222222222");
    let again = point_at("11111111-1111-4111-8111-111111111111").0;
    assert_distinguished(first, second, again);
}

#[test]
fn a_staged_candidate_is_identified_by_its_content() {
    let (_tree, directory) = order("plan-evidence-staging");
    let staged = directory.join(".spielorders/.order.json.spielorders-stage");
    let stage = |bytes: &str| {
        fs::write(&staged, bytes).unwrap();
        case(&directory, DiagnosticCode::StagingArtifactPresent)
    };

    let first = stage("staged candidate one");
    let second = stage("staged candidate two");
    let again = stage("staged candidate one").0;
    assert_distinguished(first, second, again);
}

/// An entry with no readable content records none rather than inventing a stand-in, so
/// content appearing there names a different case.
#[test]
fn content_appearing_at_an_opaque_staging_entry_names_a_different_case() {
    let (_tree, directory) = order("plan-evidence-staging-opaque");
    let staged = directory.join(".spielorders/.order.json.spielorders-stage");
    fs::create_dir(&staged).unwrap();
    let diagnosis = diagnosed_order(&directory, &PlatformFileSystem)
        .result
        .expect("diagnosable");
    assert!(
        findings(&diagnosis).any(|diagnostic| matches!(
            diagnostic.evidence,
            DiagnosticEvidence::StagingArtifactPresent {
                candidate_digest: None,
                ..
            }
        )),
        "a directory at the staging name has no candidate content"
    );
    let opaque = case(&directory, DiagnosticCode::StagingArtifactPresent);

    fs::remove_dir(&staged).unwrap();
    fs::write(&staged, "a real candidate").unwrap();
    let real = case(&directory, DiagnosticCode::StagingArtifactPresent);
    fs::remove_file(&staged).unwrap();
    fs::create_dir(&staged).unwrap();
    let again = case(&directory, DiagnosticCode::StagingArtifactPresent).0;
    assert_distinguished(opaque, real, again);
}

/// Discrimination does not depend on any rule having anticipated the diagnostic: two
/// invalid Order records differ only inside their whole evidence payload.
#[test]
fn a_diagnosis_no_rule_anticipated_still_discriminates() {
    let (_tree, directory) = order("plan-evidence-unanticipated");
    let original = fs::read_to_string(record(&directory)).unwrap();
    let invalidate = |field: &str| {
        let text = original
            .lines()
            .map(|line| {
                if line.trim_start().starts_with(&format!("\"{field}\"")) {
                    format!("  \"{field}\": 5,")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(record(&directory), &text).unwrap();
        case(&directory, DiagnosticCode::ManagedArtifactInvalidShape)
    };

    let titled = invalidate("title");
    let items = invalidate("items");
    let again = invalidate("title").0;
    assert_distinguished(titled, items, again);
}
