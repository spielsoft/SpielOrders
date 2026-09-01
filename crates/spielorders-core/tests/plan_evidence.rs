//! Decision sources: Define integrity, validation, and repair boundaries; Define the
//! Order record and lifecycle contract; Define Vendor identity and historical
//! semantics.

use crate::support::{RealFileSystem, adopted_order, initialized_package, order_record as record};
use spielorders_core::evidence::{EvidenceFingerprint, EvidenceOutcome, FactChange};
use spielorders_core::order_diagnosis::diagnose_order;
use spielorders_core::readme::{Diagnostic, DiagnosticCode, DiagnosticEvidence};
use std::fs;
use std::path::Path;

/// Two plans built over the same state agree, whatever order they recorded their
/// facts in.
#[test]
fn identical_evidence_produces_identical_fingerprints() {
    let one = EvidenceFingerprint::new()
        .observed("order_id", "8b6e3f0a")
        .observed("vendor_reference", "absent-vendor")
        .absent("staging_artifact");
    let other = EvidenceFingerprint::new()
        .absent("staging_artifact")
        .observed("vendor_reference", "absent-vendor")
        .observed("order_id", "8b6e3f0a");

    assert_eq!(one, other, "recording order is not part of the evidence");
    assert_eq!(one.confirm(&other), EvidenceOutcome::Unchanged);
}

/// Observing nothing is not the same as observing an empty value. A repair authorized
/// against "there is no Vendor Reference" must not proceed against one that is present
/// and empty.
#[test]
fn an_absent_fact_is_distinct_from_an_empty_one() {
    let planned = EvidenceFingerprint::new().absent("vendor_reference");
    let now = EvidenceFingerprint::new().observed("vendor_reference", "");

    let EvidenceOutcome::Changed(stale) = planned.confirm(&now) else {
        panic!("absence and emptiness are different evidence");
    };
    assert_eq!(stale.changed().len(), 1);
    assert_eq!(stale.changed()[0].name(), "vendor_reference");
    assert_eq!(
        stale.changed()[0].change(),
        FactChange::Appeared,
        "a value where the plan saw nothing is an appearance, not a difference"
    );
}

/// The requirement this contract exists for, proven against real diagnoses rather
/// than hand-built maps.
///
/// All three conditions carry the same diagnostic code and the same path, so the
/// handle the milestone shipped is byte-identical across them. The evidence must not
/// be. A fingerprint over identity alone passes a test built from string literals and
/// fails here.
#[test]
fn one_code_and_path_still_distinguishes_a_different_unresolved_vendor() {
    let (_tree, root) = initialized_package("plan-evidence-unresolved-vendor");
    let directory = adopted_order(
        &root,
        "2026_08_22_A",
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
    );

    let point_at = |vendor_id: &str| {
        let text = fs::read_to_string(record(&directory)).unwrap().replace(
            "\"items\": []",
            &format!("\"vendor_id\": \"{vendor_id}\",\n  \"items\": []"),
        );
        fs::write(record(&directory), text).unwrap();
        let report = diagnose_order(&directory, &RealFileSystem).expect("diagnosable");
        let diagnostic = report
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::VendorReferenceUnresolved)
            .expect("an unresolved Vendor Reference is diagnosed")
            .clone();
        // Put the record back so the next call starts from the same state.
        let restored = fs::read_to_string(record(&directory))
            .unwrap()
            .replace(&format!("\"vendor_id\": \"{vendor_id}\",\n  "), "");
        fs::write(record(&directory), restored).unwrap();
        diagnostic
    };

    let first = point_at("11111111-1111-4111-8111-111111111111");
    let second = point_at("22222222-2222-4222-8222-222222222222");

    // The shipped handle cannot tell these apart.
    assert_eq!(first.code(), second.code());
    assert_eq!(
        first.primary_location().package_relative_path(),
        second.primary_location().package_relative_path(),
        "same code, same path — this is the state the handle collapses"
    );

    let planned = EvidenceFingerprint::of_diagnostic(&first);
    let EvidenceOutcome::Changed(stale) =
        planned.confirm(&EvidenceFingerprint::of_diagnostic(&second))
    else {
        panic!("a different unresolved Vendor is different evidence");
    };
    assert!(
        stale
            .changed()
            .iter()
            .any(|fact| fact.name() == "unresolved_vendor_id"),
        "the refusal names the Vendor that changed: {:?}",
        stale.changed()
    );
    // And a diagnosis confirms against itself, so this is discrimination, not a
    // fingerprint that refuses everything.
    assert!(
        planned
            .confirm(&EvidenceFingerprint::of_diagnostic(&first))
            .is_unchanged()
    );
}

/// The staging case the diagnosis now covers itself.
///
/// Promotion writes the candidate's exact bytes, so the diagnosis carries evidence of
/// which candidate it found. Two candidates at one staging name derive different
/// fingerprints without an adopter having to supply anything.
#[test]
fn a_staged_candidate_is_identified_by_the_diagnosis_that_found_it() {
    let (_tree, root) = initialized_package("plan-evidence-staging");
    let directory = adopted_order(
        &root,
        "2026_08_22_A",
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
    );
    let staged = directory.join(".spielorders/.order.json.spielorders-stage");

    let stage = |bytes: &str| {
        fs::write(&staged, bytes).unwrap();
        staging_diagnostic(&directory)
    };

    let first = stage("staged candidate one");
    let second = stage("staged candidate two");

    let EvidenceOutcome::Changed(stale) = EvidenceFingerprint::of_diagnostic(&first)
        .confirm(&EvidenceFingerprint::of_diagnostic(&second))
    else {
        panic!("a different staged candidate is different evidence");
    };
    assert!(
        stale
            .changed()
            .iter()
            .any(|fact| fact.name() == "staging_content"),
        "the refusal names the staged content that changed: {:?}",
        stale.changed()
    );
    // And a diagnosis confirms against itself, so this is discrimination rather than a
    // fingerprint that refuses everything.
    assert!(
        EvidenceFingerprint::of_diagnostic(&first)
            .confirm(&EvidenceFingerprint::of_diagnostic(&first))
            .is_unchanged()
    );
}

/// A staged entry with no readable content has nothing that tells it apart from
/// another in the same state, so the fingerprint records absence rather than inventing
/// a stand-in. Real content appearing there is then a change the seam refuses on.
#[test]
fn a_staged_candidate_with_no_readable_content_records_no_content_evidence() {
    let (_tree, root) = initialized_package("plan-evidence-staging-opaque");
    let directory = adopted_order(
        &root,
        "2026_08_22_A",
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
    );
    let staged = directory.join(".spielorders/.order.json.spielorders-stage");
    fs::create_dir(&staged).unwrap();

    let opaque = staging_diagnostic(&directory);
    assert!(
        matches!(
            opaque.evidence(),
            DiagnosticEvidence::StagingArtifactPresent {
                candidate_digest: None,
                ..
            }
        ),
        "a directory at the staging name has no candidate content: {:?}",
        opaque.evidence()
    );

    fs::remove_dir(&staged).unwrap();
    fs::write(&staged, "a real candidate").unwrap();
    let EvidenceOutcome::Changed(stale) = EvidenceFingerprint::of_diagnostic(&opaque).confirm(
        &EvidenceFingerprint::of_diagnostic(&staging_diagnostic(&directory)),
    ) else {
        panic!("content appearing where the plan recorded none is a change");
    };
    let staging_content = stale
        .changed()
        .iter()
        .find(|fact| fact.name() == "staging_content")
        .unwrap_or_else(|| {
            panic!(
                "the refusal names the staged content: {:?}",
                stale.changed()
            )
        });
    assert_eq!(staging_content.change(), FactChange::Appeared);
}

/// The staging diagnostic the current diagnosis reports for this Order.
fn staging_diagnostic(directory: &Path) -> Diagnostic {
    diagnose_order(directory, &RealFileSystem)
        .expect("diagnosable")
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::StagingArtifactPresent)
        .expect("a staging artifact is diagnosed")
        .clone()
}

/// A fact the caller does not re-observe is not confirmed. Silence is not evidence.
#[test]
fn a_fact_the_caller_never_rechecked_is_refused_rather_than_assumed() {
    let planned = EvidenceFingerprint::new()
        .observed("order_id", "8b6e3f0a")
        .observed("vendor_name", "Tarnwick");
    let now = EvidenceFingerprint::new().observed("order_id", "8b6e3f0a");

    let EvidenceOutcome::Changed(stale) = planned.confirm(&now) else {
        panic!("a fact nothing re-observed cannot be treated as confirmed");
    };
    assert_eq!(stale.changed().len(), 1);
    assert_eq!(stale.changed()[0].name(), "vendor_name");
    assert_eq!(stale.changed()[0].change(), FactChange::Unobserved);
}

/// A plan that recorded no evidence confirms nothing. Answering "unchanged" would be
/// the same empty reassurance as "the path still exists".
#[test]
fn a_plan_that_recorded_no_evidence_is_never_confirmed() {
    let outcome = EvidenceFingerprint::new()
        .confirm(&EvidenceFingerprint::new().observed("order_id", "8b6e3f0a"));

    assert_eq!(outcome, EvidenceOutcome::NoEvidence);
    assert!(!outcome.is_unchanged());
}

/// A fact the plan never relied on appearing later is not a reason to refuse: the plan
/// made no claim about it.
#[test]
fn a_fact_the_plan_never_relied_on_does_not_refuse_it() {
    let planned = EvidenceFingerprint::new().observed("order_id", "8b6e3f0a");
    let now = EvidenceFingerprint::new()
        .observed("order_id", "8b6e3f0a")
        .observed("readme_text", "something the plan never read");

    assert!(planned.confirm(&now).is_unchanged());
}

/// The worked example the contract owes: a directory that is still a directory, but is
/// no longer the Order the plan was built against. `symlink_metadata` reports an
/// ordinary directory in both states, which is exactly why that check is not enough.
#[test]
fn a_directory_replaced_by_a_different_order_is_refused_though_it_is_still_a_directory() {
    let (_tree, root) = initialized_package("plan-evidence-replaced-order");
    let directory = adopted_order(
        &root,
        "2026_08_22_A",
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
    );

    let observe = |directory: &Path| {
        let report =
            diagnose_order(directory, &RealFileSystem).expect("the Order can be diagnosed");
        EvidenceFingerprint::new()
            .observed("directory", directory.to_string_lossy().as_bytes())
            .observed("order_record", fs::read(record(directory)).unwrap())
            .observed(
                "diagnostic_codes",
                report
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.code().as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            )
    };

    let planned = observe(&directory);

    // Someone replaces the Order at that exact path with a different one.
    fs::remove_dir_all(&directory).unwrap();
    let replacement = adopted_order(
        &root,
        "2026_08_22_A",
        "1c2d3e4f-8ec5-4d47-95e1-412652a56d16",
    );
    assert_eq!(
        replacement, directory,
        "the replacement occupies the same path"
    );
    assert!(
        directory.is_dir(),
        "the path-kind check that shipped still passes here, which is the point"
    );

    let EvidenceOutcome::Changed(stale) = planned.confirm(&observe(&directory)) else {
        panic!("a different Order at the same path is not the plan's subject");
    };
    assert!(
        stale
            .changed()
            .iter()
            .any(|fact| fact.name() == "order_record"),
        "the refusal names the record that changed: {:?}",
        stale.changed()
    );
}

/// The subject going away entirely is named as such, so a caller can tell "someone
/// changed this" from "this is gone".
#[test]
fn a_vanished_subject_is_named_differently_from_a_changed_one() {
    let planned = EvidenceFingerprint::new().observed("order_record", "the planned bytes");
    let now = EvidenceFingerprint::new().absent("order_record");

    let EvidenceOutcome::Changed(stale) = planned.confirm(&now) else {
        panic!("a vanished subject is stale evidence");
    };
    assert_eq!(stale.changed()[0].change(), FactChange::Vanished);
}

/// Every changed fact is reported, not just the first, and in deterministic order.
#[test]
fn every_changed_fact_is_named_in_deterministic_order() {
    let planned = EvidenceFingerprint::new()
        .observed("alpha", "1")
        .observed("beta", "2")
        .observed("gamma", "3");
    let now = EvidenceFingerprint::new()
        .observed("alpha", "changed")
        .observed("beta", "2")
        .absent("gamma");

    let EvidenceOutcome::Changed(stale) = planned.confirm(&now) else {
        panic!("two facts moved");
    };
    assert_eq!(
        stale
            .changed()
            .iter()
            .map(|fact| (fact.name(), fact.change()))
            .collect::<Vec<_>>(),
        [
            ("alpha", FactChange::Differs),
            ("gamma", FactChange::Vanished)
        ]
    );
}

/// Re-recording a name replaces the earlier observation rather than leaving a stale
/// one behind.
#[test]
fn re_recording_a_fact_replaces_the_earlier_observation() {
    let refined = EvidenceFingerprint::new()
        .observed("vendor_name", "  Tarnwick  ")
        .observed("vendor_name", "Tarnwick");

    assert_eq!(refined.names().collect::<Vec<_>>(), ["vendor_name"]);
    assert!(
        refined
            .confirm(&EvidenceFingerprint::new().observed("vendor_name", "Tarnwick"))
            .is_unchanged()
    );
}

/// Discrimination must not depend on this contract having anticipated the diagnostic.
///
/// `ManagedArtifact` is deliberately not one of the payloads named field by field
/// above. Two invalid Order records at the same path produce the same diagnostic code
/// and differ only inside that payload, so the whole-payload fact is the only thing
/// telling them apart. Without it an adopter repairing any un-anticipated diagnosis
/// gets the identity-only behavior this ticket exists to remove.
#[test]
fn a_diagnosis_this_contract_never_anticipated_still_discriminates() {
    let (_tree, root) = initialized_package("plan-evidence-unanticipated");
    let directory = adopted_order(
        &root,
        "2026_08_22_A",
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
    );
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
        let diagnostic = diagnose_order(&directory, &RealFileSystem)
            .expect("diagnosable")
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::ManagedArtifactInvalidShape)
            .expect("an invalid Order record is diagnosed")
            .clone();
        fs::write(record(&directory), &original).unwrap();
        diagnostic
    };

    let titled = invalidate("title");
    let items = invalidate("items");

    // Same code, same path — and this contract names no field of this payload.
    assert_eq!(titled.code(), items.code());
    assert_eq!(
        titled.primary_location().package_relative_path(),
        items.primary_location().package_relative_path()
    );

    let planned = EvidenceFingerprint::of_diagnostic(&titled);
    let EvidenceOutcome::Changed(stale) =
        planned.confirm(&EvidenceFingerprint::of_diagnostic(&items))
    else {
        panic!("a different condition under an unanticipated code is different evidence");
    };
    assert!(
        stale
            .changed()
            .iter()
            .any(|fact| fact.name() == "diagnostic_evidence"),
        "the whole-payload fact is what catches it: {:?}",
        stale.changed()
    );
    assert!(
        planned
            .confirm(&EvidenceFingerprint::of_diagnostic(&titled))
            .is_unchanged()
    );
}
