//! Decision sources: Define the stable diagnostic vocabulary and evidence
//! contract; Define the complete Package read projection and load-state
//! contract; Define the SpielOrders package topology.

use crate::Failure;
use crate::ports::FileSystemEntryKind;
use crate::tests::support::{
    Operation, ScriptedFileSystem, Spelled, effects, failed_location, location, minimal_package,
};
use crate::{Answer, OrderRef, Scope};
use crate::{
    ReadmeRead, readme::DiagnosticCode, readme::DiagnosticEvidence, readme::EvidenceScope,
    readme::ReadmeKind, readme::ReadmeReference, readme::ReadmeUnavailableReason, readme::Severity,
};

const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SCENARIOS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/scenarios.json"
));

const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

const MINIMAL_ORDER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));

/// A minimal Package holding the healthy Orders `Optics`, `A`, and `B`, and whatever
/// `place` puts there.
fn volume(place: impl FnOnce(&ScriptedFileSystem)) -> ScriptedFileSystem {
    let filesystem = minimal_package();
    for (index, name) in ["Optics", "A", "B"].into_iter().enumerate() {
        let directory = format!("/package/Orders/{name}");
        filesystem.put_entry(&directory, FileSystemEntryKind::Directory);
        filesystem.put_entry(
            format!("{directory}/.spielorders"),
            FileSystemEntryKind::Directory,
        );
        filesystem.put_file(
            format!("{directory}/.spielorders/order.json"),
            MINIMAL_ORDER
                .replacen(
                    "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
                    &format!("00000000-0000-4000-8000-{index:012}"),
                    1,
                )
                .as_bytes(),
        );
    }
    place(&filesystem);
    filesystem
}

/// The README `reference` names, read through the read of the scope it belongs to.
fn read_readme(reference: &Reference, filesystem: &ScriptedFileSystem) -> Answer<ReadmeRead> {
    let directory = reference
        .location
        .absolute_path
        .parent()
        .expect("a README is inside its scope's directory")
        .to_path_buf();
    let scope = match reference.kind {
        ReadmeKind::PackageReadme => Scope::Package(directory),
        ReadmeKind::OrderReadme => Scope::Order(OrderRef::Directory(directory)),
    };
    crate::read_readme(&scope, &effects(filesystem))
}

fn unreadable_readme(readme: &'static str) -> ScriptedFileSystem {
    volume(|filesystem| {
        filesystem.put_file(readme, b"unreadable");
        filesystem.on(Operation::Read).at(readme).fails();
    })
}

#[test]
fn absent_package_readme_is_an_empty_document_without_a_diagnostic() {
    assert!(
        scenario("README absence")["tree"]
            .as_array()
            .expect("absence recipe has a tree")
            .is_empty()
    );
    let location = location("/package/README.md", "README.md");
    let reference = readme_reference(ReadmeKind::PackageReadme, location);
    let filesystem = volume(|_| {});

    let result = read_readme(&reference, &filesystem)
        .result
        .expect("absence is an ordinary result");

    assert_eq!(result.reference, reference);
    assert!(!result.exists);
    assert_eq!(result.text.as_deref(), Some(""));
    assert!(result.diagnostics.is_empty());
}

#[test]
fn readable_order_readme_returns_exact_utf8_text_from_the_requested_path() {
    let location = location(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    );
    let reference = readme_reference(ReadmeKind::OrderReadme, location);
    let readme = "/package/Orders/Optics/README.md";
    let supporting_a = "/package/Orders/Optics/quote.bin";
    let supporting_b = "/package/Orders/Optics/nested/drawing.bin";
    let filesystem = volume(|filesystem| {
        filesystem.put_file(readme, "# Café optics\r\n\r\nΔλ stays exact.\n".as_bytes());
        filesystem.put_file(supporting_a, SUPPORTING_A);
        filesystem.put_file(supporting_b, SUPPORTING_B);
    });

    let result = read_readme(&reference, &filesystem)
        .result
        .expect("ordinary UTF-8 is readable");

    assert_eq!(result.reference, reference);
    assert!(result.exists);
    assert_eq!(
        result.text.as_deref(),
        Some("# Café optics\r\n\r\nΔλ stays exact.\n")
    );
    assert!(result.diagnostics.is_empty());
    assert_eq!(filesystem.bytes(supporting_a).unwrap(), SUPPORTING_A);
    assert_eq!(filesystem.bytes(supporting_b).unwrap(), SUPPORTING_B);
}

#[test]
fn present_non_file_readme_is_unavailable() {
    for kind in [
        FileSystemEntryKind::Directory,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        let location = location("/package/README.md", "README.md");
        let reference = readme_reference(ReadmeKind::PackageReadme, location.clone());
        let filesystem = volume(|filesystem| filesystem.put_entry("/package/README.md", kind));

        let result = read_readme(&reference, &filesystem)
            .result
            .expect("wrong kind is a document result");

        assert_unavailable(
            &result,
            &location,
            ReadmeKind::PackageReadme,
            ReadmeUnavailableReason::WrongKind,
        );
    }
}

#[test]
fn injected_read_failure_is_a_stable_unreadable_document_warning() {
    let location = location(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    );
    let reference = readme_reference(ReadmeKind::OrderReadme, location.clone());
    let filesystem = unreadable_readme("/package/Orders/Optics/README.md");

    let result = read_readme(&reference, &filesystem)
        .result
        .expect("an unreadable README is an unavailable document result");

    assert_unavailable(
        &result,
        &location,
        ReadmeKind::OrderReadme,
        ReadmeUnavailableReason::Unreadable,
    );
}

#[test]
fn invalid_utf8_is_unavailable_instead_of_lossily_decoded() {
    let recipe = scenario("README unreadable");
    assert_eq!(
        recipe["tree"][0]["source"],
        "fixtures/assets/supporting-b.bin"
    );
    assert_eq!(recipe["tree"][0]["readability"], "non_utf8_recipe");
    let location = location(
        "/package/Orders/Optics/README.md",
        "Orders/Optics/README.md",
    );
    let reference = readme_reference(ReadmeKind::OrderReadme, location.clone());
    let mut non_utf8_recipe = SUPPORTING_B.to_vec();
    non_utf8_recipe.extend_from_slice(&[0xff, 0xfe, 0xfd]);
    let filesystem = volume(|filesystem| {
        filesystem.put_file("/package/Orders/Optics/README.md", &non_utf8_recipe)
    });

    let result = read_readme(&reference, &filesystem)
        .result
        .expect("invalid encoding is an unavailable document result");

    assert_unavailable(
        &result,
        &location,
        ReadmeKind::OrderReadme,
        ReadmeUnavailableReason::InvalidEncoding,
    );
}

#[test]
fn observation_failure_returns_stable_operation_evidence_without_fabricating_existence() {
    let location = location("/package/README.md", "README.md");
    let reference = readme_reference(ReadmeKind::PackageReadme, location.clone());
    let filesystem = volume(|filesystem| {
        filesystem
            .on(Operation::SymlinkMetadata)
            .at("/package/README.md")
            .fails();
    });

    let failure = read_readme(&reference, &filesystem)
        .result
        .expect_err("failed observation cannot produce a document result");

    let Failure::PackageOperationFailed(failure) = failure else {
        panic!("a failed observation is not a denial: {failure:?}");
    };
    assert_eq!(
        failed_location(&failure.path).expect("the failure names a Location"),
        &location
    );
}

#[test]
fn diagnostic_equality_and_order_use_only_stable_typed_evidence() {
    let location_a = location("/package/Orders/A/README.md", "Orders/A/README.md");
    let reference_a = readme_reference(ReadmeKind::OrderReadme, location_a);
    // Two different underlying failures of the same read.
    let primary = unreadable_readme("/package/Orders/A/README.md");
    let alternate = volume(|filesystem| {
        filesystem.put_file("/package/Orders/A/README.md", b"unreadable");
        filesystem
            .on(Operation::Read)
            .at("/package/Orders/A/README.md")
            .fails_with(std::io::ErrorKind::Interrupted);
    });

    let primary_warning = read_readme(&reference_a, &primary)
        .result
        .expect("read failure becomes a warning")
        .diagnostics[0]
        .clone();
    let alternate_warning = read_readme(&reference_a, &alternate)
        .result
        .expect("alternate read failure becomes a warning")
        .diagnostics[0]
        .clone();

    assert_eq!(primary_warning, alternate_warning);

    let location_b = location("/package/Orders/B/README.md", "Orders/B/README.md");
    let reference_b = readme_reference(ReadmeKind::OrderReadme, location_b);
    let warning_b = read_readme(
        &reference_b,
        &unreadable_readme("/package/Orders/B/README.md"),
    )
    .result
    .expect("read failure becomes a warning")
    .diagnostics[0]
        .clone();
    let mut warnings = [warning_b, primary_warning.clone()];

    warnings.sort();

    assert_eq!(warnings[0], primary_warning);
}

/// The README a test expects: its kind, and where it is.
#[derive(Debug)]
struct Reference {
    kind: ReadmeKind,
    location: Spelled,
}

impl PartialEq<Reference> for ReadmeReference {
    fn eq(&self, expected: &Reference) -> bool {
        self.kind == expected.kind && self.location == expected.location
    }
}

fn readme_reference(kind: ReadmeKind, location: Spelled) -> Reference {
    Reference { kind, location }
}

fn assert_unavailable(
    result: &ReadmeRead,
    location: &Spelled,
    document_kind: ReadmeKind,
    reason: ReadmeUnavailableReason,
) {
    assert!(result.exists);
    assert_eq!(result.text.as_deref(), None);
    let [warning] = result.diagnostics.as_slice() else {
        panic!("unavailable README must emit exactly one warning")
    };
    assert_eq!(warning.code, DiagnosticCode::ReadmeUnavailable);
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.scope, EvidenceScope::Document);
    assert_eq!(warning.primary_location, *location);
    assert!(warning.related_locations.is_empty());
    assert_eq!(
        warning.evidence,
        DiagnosticEvidence::ReadmeUnavailable {
            document_kind,
            reason,
        }
    );
}

fn scenario(name: &str) -> serde_json::Value {
    let corpus: serde_json::Value =
        serde_json::from_str(SCENARIOS).expect("canonical scenario corpus parses");
    corpus["scenarios"]
        .as_array()
        .expect("canonical corpus has scenarios")
        .iter()
        .find(|scenario| scenario["name"] == name)
        .unwrap_or_else(|| panic!("canonical scenario {name:?} exists"))
        .clone()
}
