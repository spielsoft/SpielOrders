//! Decision sources: Define the stable M2 diagnostic vocabulary and evidence
//! contract; Define the complete M2 Package read projection and load-state
//! contract; Decide how Package Index freshness is proven without canonical rereads.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::canonical::{
    CanonicalPackageLoad, MetadataState, UpgradeArtifactState, inspect_canonical_package,
};
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use spielorders_core::readme::{DiagnosticCode, DiagnosticEvidence, EvidenceScope, Severity};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const VERSION: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/version.json"
));
const VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/vendors.json"
));
const FULL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/order.json"
));
const FULL_VENDORS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/full/vendors.json"
));
const MINIMAL_ORDER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/examples/v1/minimal/order.json"
));
const INVALID_ORDER_ITEM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/order-invalid-item.json"
));
const INVALID_VENDOR_CONTACT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/schema-invalid/vendor-invalid-contact.json"
));
const SUPPORTING_A: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-a.bin"
));
const SUPPORTING_B: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../contracts/fixtures/assets/supporting-b.bin"
));

#[derive(Clone, Debug, Eq, PartialEq)]
enum Effect {
    Read(PathBuf),
    Canonicalize(PathBuf),
    ObserveKind(PathBuf),
    ListDirectory(PathBuf),
}

#[derive(Default)]
struct RecordingRealFileSystem {
    effects: RefCell<Vec<Effect>>,
}

impl FileSystem for RecordingRealFileSystem {
    type Error = std::io::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Read(path.to_path_buf()));
        RealFileSystem.read(path)
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never writes")
    }

    fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never stages")
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never syncs")
    }

    fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never replaces")
    }

    fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never removes")
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Canonicalize(path.to_path_buf()));
        RealFileSystem.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveKind(path.to_path_buf()));
        RealFileSystem.symlink_metadata(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ListDirectory(path.to_path_buf()));
        RealFileSystem.read_directory(path)
    }
}

#[derive(Default)]
struct TestFileSystem {
    entries: BTreeMap<PathBuf, FileSystemEntryKind>,
    files: BTreeMap<PathBuf, Vec<u8>>,
    failed_reads: BTreeSet<PathBuf>,
    failed_canonicalizations: BTreeSet<PathBuf>,
    failed_observations: BTreeSet<PathBuf>,
    failed_lists: BTreeSet<PathBuf>,
    canonical_paths: BTreeMap<PathBuf, PathBuf>,
    effects: RefCell<Vec<Effect>>,
}

impl TestFileSystem {
    fn directory(mut self, path: &str) -> Self {
        self.entries
            .insert(PathBuf::from(path), FileSystemEntryKind::Directory);
        self
    }

    fn file(mut self, path: &str, contents: &[u8]) -> Self {
        self.entries
            .insert(PathBuf::from(path), FileSystemEntryKind::File);
        self.files.insert(PathBuf::from(path), contents.to_vec());
        self
    }

    fn entry(mut self, path: &str, kind: FileSystemEntryKind) -> Self {
        self.entries.insert(PathBuf::from(path), kind);
        self
    }

    fn fail_read(mut self, path: &str) -> Self {
        self.failed_reads.insert(PathBuf::from(path));
        self
    }

    fn fail_canonicalize(mut self, path: &str) -> Self {
        self.failed_canonicalizations.insert(PathBuf::from(path));
        self
    }

    fn fail_observe(mut self, path: &str) -> Self {
        self.failed_observations.insert(PathBuf::from(path));
        self
    }

    fn fail_list(mut self, path: &str) -> Self {
        self.failed_lists.insert(PathBuf::from(path));
        self
    }

    fn canonical_path(mut self, path: &str, resolved: &str) -> Self {
        self.canonical_paths
            .insert(PathBuf::from(path), PathBuf::from(resolved));
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedError {
    Read,
    Canonicalize,
    Observe,
    List,
}

fn minimal_package_filesystem() -> TestFileSystem {
    TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", VENDORS)
        .directory("/package/Orders")
}

impl FileSystem for TestFileSystem {
    type Error = InjectedError;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Read(path.to_path_buf()));
        if self.failed_reads.contains(path) {
            Err(InjectedError::Read)
        } else {
            Ok(self.files.get(path).cloned().unwrap_or_default())
        }
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never writes")
    }

    fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never stages")
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never syncs")
    }

    fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never replaces")
    }

    fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
        unreachable!("canonical inspection never removes")
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::Canonicalize(path.to_path_buf()));
        if self.failed_canonicalizations.contains(path) {
            Err(InjectedError::Canonicalize)
        } else {
            Ok(self
                .canonical_paths
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.to_path_buf()))
        }
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ObserveKind(path.to_path_buf()));
        if self.failed_observations.contains(path) {
            Err(InjectedError::Observe)
        } else {
            Ok(self.entries.get(path).copied())
        }
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.effects
            .borrow_mut()
            .push(Effect::ListDirectory(path.to_path_buf()));
        if self.failed_lists.contains(path) {
            return Err(InjectedError::List);
        }
        Ok(self
            .entries
            .iter()
            .filter(|(candidate, _)| candidate.parent() == Some(path))
            .map(|(candidate, kind)| {
                DirectoryEntry::new(candidate.file_name().expect("entry has a name"), *kind)
            })
            .collect())
    }
}

#[test]
fn unmanaged_and_minimal_current_packages_have_distinct_canonical_results() {
    let unmanaged_filesystem = TestFileSystem::default().directory("/unmanaged");

    let unmanaged = inspect_canonical_package(Path::new("/unmanaged"), &unmanaged_filesystem);

    let CanonicalPackageLoad::Unmanaged(unmanaged) = unmanaged else {
        panic!("a wholly absent root metadata directory is unmanaged")
    };
    assert_eq!(unmanaged.root().absolute_path(), Path::new("/unmanaged"));
    assert_eq!(unmanaged.root().package_relative_path(), None);

    let managed_filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", VENDORS)
        .directory("/package/Orders");

    let managed = inspect_canonical_package(Path::new("/package"), &managed_filesystem);

    let CanonicalPackageLoad::CurrentCandidate(package) = managed else {
        panic!("current canonical inputs produce a pre-index candidate")
    };
    assert_eq!(package.context().root().package_relative_path(), Some("."));
    assert_eq!(package.metadata_version(), 1);
    assert!(package.healthy_orders().is_empty());
    assert!(package.damaged_orders().is_empty());
    assert!(package.vendors().is_empty());
    assert!(package.diagnostic_groups().is_empty());
    assert_eq!(package.index().state().as_str(), "not_assessed");
    assert_eq!(package.index().refresh().as_str(), "not_attempted");
}

#[test]
fn missing_package_canonical_artifact_is_diagnostic_only_without_partial_projection() {
    let filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .directory("/package/Orders");

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::DiagnosticOnly(damaged) = result else {
        panic!("missing canonical Vendors must damage the Package")
    };
    assert!(damaged.context().is_some());
    assert_eq!(damaged.index().state().as_str(), "not_assessed");
    assert_eq!(damaged.index().refresh().as_str(), "not_attempted");
    let [group] = damaged.diagnostic_groups() else {
        panic!("the missing artifact has exactly one Package diagnostic group")
    };
    let [diagnostic] = group.diagnostics() else {
        panic!("the missing artifact is diagnosed exactly once")
    };
    assert_eq!(diagnostic.code(), DiagnosticCode::ManagedArtifactMissing);
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert_eq!(diagnostic.scope(), EvidenceScope::Package);
    assert_eq!(
        diagnostic.primary_location().package_relative_path(),
        Some(".spielorders/vendors.json")
    );
}

#[test]
fn healthy_order_projection_contains_complete_facts_totals_readme_vendor_and_reusable_values() {
    let filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", FULL_VENDORS)
        .directory("/package/Orders")
        .directory("/package/Orders/Optics")
        .directory("/package/Orders/Optics/.spielorders")
        .file("/package/Orders/Optics/.spielorders/order.json", FULL_ORDER)
        .file(
            "/package/Orders/Optics/README.md",
            b"# Exact Order notes\r\n",
        );

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::CurrentCandidate(package) = result else {
        panic!("healthy canonical state produces a pre-index candidate")
    };
    let [order] = package.healthy_orders() else {
        panic!("the healthy Order is projected exactly once")
    };
    assert_eq!(
        order.paths().directory().package_relative_path(),
        Some("Orders/Optics")
    );
    assert_eq!(
        order.paths().metadata().package_relative_path(),
        Some("Orders/Optics/.spielorders")
    );
    assert_eq!(
        order.paths().order().package_relative_path(),
        Some("Orders/Optics/.spielorders/order.json")
    );
    assert_eq!(order.document().title.as_str(), "Laser components");
    assert_eq!(order.item_totals(), &[169.0]);
    assert_eq!(order.order_total(), 169.0);
    assert_eq!(order.readme_text(), Some("# Exact Order notes\r\n"));
    assert!(order.document().vendor_id.is_some());
    assert_eq!(package.vendors()[0].name.as_str(), "Tarnwick");
    assert_eq!(
        package.vendor_for_order(0).unwrap().name.as_str(),
        "Tarnwick"
    );
    assert!(package.damaged_orders().is_empty());
    assert!(package.diagnostic_groups().is_empty());
    assert_eq!(package.ordering_institutions()[0].value(), "Northwind");
    assert_eq!(package.orderers()[0].value(), "Ada Lovelace");
    assert_eq!(package.projects()[0].value(), "Telescope");
    assert_eq!(package.order_groups()[0].value(), "Summer build");
    assert_eq!(package.order_groups()[0].usage_count(), 1);
}

#[test]
fn malformed_order_is_quarantined_beside_a_healthy_order_with_untrusted_identity_absent() {
    let filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", VENDORS)
        .directory("/package/Orders")
        .directory("/package/Orders/Bad")
        .file(
            "/package/Orders/Bad/.spielorders/order.json",
            b"{ malformed",
        )
        .directory("/package/Orders/Good")
        .file(
            "/package/Orders/Good/.spielorders/order.json",
            MINIMAL_ORDER,
        );

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::CurrentCandidate(package) = result else {
        panic!("Order-local damage does not damage the Package")
    };
    assert_eq!(package.healthy_orders().len(), 1);
    let [damaged] = package.damaged_orders() else {
        panic!("the damaged candidate has one placeholder")
    };
    assert_eq!(
        damaged.paths().directory().package_relative_path(),
        Some("Orders/Bad")
    );
    assert_eq!(damaged.order_id(), None);
    let group = package
        .diagnostic_groups()
        .iter()
        .find(|group| group.location() == damaged.paths().directory())
        .expect("the placeholder has one matching Order diagnostic group");
    assert_eq!(group.scope(), EvidenceScope::Order);
    assert_eq!(
        group.diagnostics()[0].code(),
        DiagnosticCode::ManagedArtifactInvalidSyntax
    );
}

#[test]
fn package_artifact_encoding_syntax_shape_and_version_failures_keep_distinct_codes() {
    let cases: [(&str, &[u8], DiagnosticCode); 7] = [
        (
            "/package/.spielorders/config.toml",
            b"orders_path = \xff",
            DiagnosticCode::ManagedArtifactInvalidEncoding,
        ),
        (
            "/package/.spielorders/config.toml",
            b"orders_path = \"",
            DiagnosticCode::ManagedArtifactInvalidSyntax,
        ),
        (
            "/package/.spielorders/config.toml",
            b"unknown = true\n",
            DiagnosticCode::ManagedArtifactInvalidShape,
        ),
        (
            "/package/.spielorders/version.json",
            b"{ malformed",
            DiagnosticCode::ManagedArtifactInvalidSyntax,
        ),
        (
            "/package/.spielorders/version.json",
            br#"{"metadata_version": 999}"#,
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
        (
            "/package/.spielorders/vendors.json",
            br#"{"schema_version":1,"vendors":{}}"#,
            DiagnosticCode::ManagedArtifactInvalidShape,
        ),
        (
            "/package/.spielorders/vendors.json",
            br#"{"schema_version":2,"vendors":[]}"#,
            DiagnosticCode::ManagedArtifactWrongVersion,
        ),
    ];

    for (path, bytes, expected_code) in cases {
        let filesystem = minimal_package_filesystem().file(path, bytes);

        let result = inspect_canonical_package(Path::new("/package"), &filesystem);

        let CanonicalPackageLoad::DiagnosticOnly(damaged) = result else {
            panic!("{path} failure must be Package diagnostic-only")
        };
        let codes = damaged
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>();
        assert!(
            codes.contains(&expected_code),
            "wrong code for {path}: {codes:?}"
        );
    }
}

#[test]
fn configuration_failures_and_visible_direct_child_kinds_use_their_closed_boundaries() {
    let invalid_path = minimal_package_filesystem().file(
        "/package/.spielorders/config.toml",
        b"orders_path = \".\"\n",
    );
    let overlap = minimal_package_filesystem().file(
        "/package/.spielorders/config.toml",
        b"orders_path = \"Removed/Active\"\nremoved_path = \"Removed\"\n",
    );
    let unavailable = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", VENDORS)
        .file("/package/Orders", b"not a directory");

    for (filesystem, expected) in [
        (invalid_path, DiagnosticCode::ConfigurationPathInvalid),
        (overlap, DiagnosticCode::ConfigurationPathsOverlap),
        (
            unavailable,
            DiagnosticCode::ConfiguredOrdersDirectoryUnavailable,
        ),
    ] {
        let CanonicalPackageLoad::DiagnosticOnly(damaged) =
            inspect_canonical_package(Path::new("/package"), &filesystem)
        else {
            panic!("unsafe configured paths damage the Package")
        };
        assert!(
            damaged
                .diagnostic_groups()
                .iter()
                .flat_map(|group| group.diagnostics())
                .any(|diagnostic| diagnostic.code() == expected)
        );
    }

    let warned = minimal_package_filesystem()
        .file("/package/Orders/loose.txt", b"opaque")
        .entry(
            "/package/Orders/VisibleAlias",
            FileSystemEntryKind::SymbolicLink,
        )
        .entry("/package/Orders/.hidden", FileSystemEntryKind::SymbolicLink)
        .entry("/package/Orders/device", FileSystemEntryKind::Other);
    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(Path::new("/package"), &warned)
    else {
        panic!("structural warnings do not block the canonical candidate")
    };
    let warnings = package
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 3);
    assert!(warnings.iter().all(|warning| {
        warning.code() == DiagnosticCode::OrdersDirectoryEntryDisallowed
            && warning.severity() == Severity::Warning
            && warning.scope() == EvidenceScope::Package
    }));
}

#[test]
fn path_failure_retains_and_orders_all_previously_observed_package_diagnostics() {
    let filesystem = minimal_package_filesystem()
        .file(
            "/package/.spielorders/version.json",
            br#"{"metadata_version":999}"#,
        )
        .file("/package/Orders", b"not a directory");

    let CanonicalPackageLoad::DiagnosticOnly(damaged) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("the independently observed Package failures are diagnostic-only")
    };
    let diagnostics = damaged
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ManagedArtifactWrongVersion })
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ConfiguredOrdersDirectoryUnavailable
    }));
}

#[test]
fn early_package_damage_normalizes_diagnostics_before_returning() {
    let filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/version.json", b"{ malformed")
        .directory("/package/Orders");

    let CanonicalPackageLoad::DiagnosticOnly(damaged) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("multiple required-artifact failures are diagnostic-only")
    };
    let diagnostics = damaged
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.windows(2).all(|pair| pair[0] <= pair[1]));
    let unique = diagnostics.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), diagnostics.len());
}

#[test]
fn duplicate_vendor_id_and_name_damage_the_package_with_trustworthy_evidence() {
    let vendors = r#"{
  "schema_version": 1,
  "vendors": [
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":" Straße Instruments ","contacts":[]},
    {"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Other","contacts":[]},
    {"vendor_id":"f1990d31-06aa-42e9-97c8-088e3e894822","name":"STRASSE Instruments","contacts":[]}
  ]
}"#;
    let filesystem =
        minimal_package_filesystem().file("/package/.spielorders/vendors.json", vendors.as_bytes());

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::DiagnosticOnly(damaged) = result else {
        panic!("an untrustworthy Vendor aggregate damages the Package")
    };
    let diagnostics = damaged
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .collect::<Vec<_>>();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::VendorIdDuplicate)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::VendorNameDuplicate)
    );
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic.scope() == EvidenceScope::Package
            && diagnostic.severity() == Severity::Error
            && diagnostic.primary_location().package_relative_path()
                == Some(".spielorders/vendors.json")
    }));
}

#[test]
fn semantic_locator_and_vendor_reference_failures_quarantine_only_their_orders() {
    let minimal = String::from_utf8(MINIMAL_ORDER.to_vec()).expect("fixture is UTF-8");
    let semantic = minimal.replacen("2026-07-19", "2026-02-31", 1);
    let locator = minimal
        .replacen(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "f1111111-1111-4111-8111-111111111111",
            1,
        )
        .replacen("../..", "../../..", 1);
    let unresolved = minimal
        .replacen(
            "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
            "a2222222-2222-4222-8222-222222222222",
            1,
        )
        .replacen(
            "  \"status\": \"in_preparation\",",
            "  \"status\": \"in_preparation\",\n  \"vendor_id\": \"e0990d31-06aa-42e9-97c8-088e3e894822\",",
            1,
        );
    let healthy = minimal.replacen(
        "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "b3333333-3333-4333-8333-333333333333",
        1,
    );
    let filesystem = minimal_package_filesystem()
        .directory("/package/Orders/A-Semantic")
        .file(
            "/package/Orders/A-Semantic/.spielorders/order.json",
            semantic.as_bytes(),
        )
        .directory("/package/Orders/B-Locator")
        .file(
            "/package/Orders/B-Locator/.spielorders/order.json",
            locator.as_bytes(),
        )
        .directory("/package/Orders/C-Unresolved")
        .file(
            "/package/Orders/C-Unresolved/.spielorders/order.json",
            unresolved.as_bytes(),
        )
        .directory("/package/Orders/D-Healthy")
        .file(
            "/package/Orders/D-Healthy/.spielorders/order.json",
            healthy.as_bytes(),
        );

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::CurrentCandidate(package) = result else {
        panic!("Order-local failures preserve the Package candidate")
    };
    assert_eq!(package.healthy_orders().len(), 1);
    assert_eq!(package.damaged_orders().len(), 3);
    assert!(
        package
            .damaged_orders()
            .iter()
            .all(|order| order.order_id().is_some())
    );
    let codes = package
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .map(|diagnostic| diagnostic.code())
        .collect::<Vec<_>>();
    assert!(codes.contains(&DiagnosticCode::OrderSemanticInvalid));
    assert!(codes.contains(&DiagnosticCode::OrderPackageLocatorMismatch));
    assert!(codes.contains(&DiagnosticCode::VendorReferenceUnresolved));
    assert_eq!(
        package
            .damaged_orders()
            .iter()
            .map(|order| order.paths().directory().package_relative_path().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "Orders/A-Semantic",
            "Orders/B-Locator",
            "Orders/C-Unresolved",
        ]
    );
}

#[test]
fn every_duplicate_order_id_participant_is_damaged_with_the_complete_conflict_set() {
    let filesystem = minimal_package_filesystem()
        .directory("/package/Orders/C")
        .file("/package/Orders/C/.spielorders/order.json", MINIMAL_ORDER)
        .directory("/package/Orders/A")
        .file("/package/Orders/A/.spielorders/order.json", MINIMAL_ORDER)
        .directory("/package/Orders/B")
        .file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER);

    let result = inspect_canonical_package(Path::new("/package"), &filesystem);

    let CanonicalPackageLoad::CurrentCandidate(package) = result else {
        panic!("duplicate identities remain Order-local damage")
    };
    assert!(package.healthy_orders().is_empty());
    assert_eq!(package.damaged_orders().len(), 3);
    assert!(package.damaged_orders().iter().all(|order| {
        order.order_id().map(|id| id.as_str()) == Some("8b6e3f0a-8ec5-4d47-95e1-412652a56d16")
    }));
    let duplicate_diagnostics = package
        .diagnostic_groups()
        .iter()
        .flat_map(|group| group.diagnostics())
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::OrderIdDuplicate)
        .collect::<Vec<_>>();
    assert_eq!(duplicate_diagnostics.len(), 3);
    for diagnostic in duplicate_diagnostics {
        let DiagnosticEvidence::OrderIdDuplicate {
            conflict_locations, ..
        } = diagnostic.evidence()
        else {
            panic!("duplicate diagnostic carries typed conflict evidence")
        };
        assert_eq!(
            conflict_locations
                .iter()
                .map(|location| location.package_relative_path().unwrap())
                .collect::<Vec<_>>(),
            vec!["Orders/A", "Orders/B", "Orders/C"]
        );
    }
}

#[test]
fn order_artifact_failures_are_distinct_and_preserve_only_trustworthy_identity() {
    let order_path = "/package/Orders/Bad/.spielorders/order.json";
    let missing_items = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Missing items",
  "date_ordered": "2026-07-19",
  "status": "ordered"
}"#;
    let malformed_id = br#"{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "NOT-AN-ID",
  "title": "Bad identity",
  "date_ordered": "2026-07-19",
  "status": "ordered",
  "items": []
}"#;
    let cases = [
        (
            minimal_package_filesystem().directory("/package/Orders/Bad"),
            DiagnosticCode::ManagedArtifactMissing,
            None,
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .entry(order_path, FileSystemEntryKind::Directory),
            DiagnosticCode::ManagedArtifactWrongKind,
            None,
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(order_path, MINIMAL_ORDER)
                .fail_read(order_path),
            DiagnosticCode::ManagedArtifactUnreadable,
            None,
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(order_path, &[0xff]),
            DiagnosticCode::ManagedArtifactInvalidEncoding,
            None,
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(order_path, missing_items),
            DiagnosticCode::ManagedArtifactInvalidShape,
            Some("8b6e3f0a-8ec5-4d47-95e1-412652a56d16"),
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(order_path, malformed_id),
            DiagnosticCode::ManagedArtifactInvalidShape,
            None,
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(
                    order_path,
                    br#"{"schema_version":2,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Future","date_ordered":"2026-07-19","status":"ordered","items":[]}"#,
                ),
            DiagnosticCode::ManagedArtifactWrongVersion,
            None,
        ),
    ];

    for (filesystem, expected_code, expected_id) in cases {
        let CanonicalPackageLoad::CurrentCandidate(package) =
            inspect_canonical_package(Path::new("/package"), &filesystem)
        else {
            panic!("Order artifact failure must remain Order-local")
        };
        let [damaged] = package.damaged_orders() else {
            panic!("each failed candidate produces one placeholder")
        };
        assert_eq!(damaged.order_id().map(|id| id.as_str()), expected_id);
        assert!(
            package
                .diagnostic_groups()
                .iter()
                .flat_map(|group| group.diagnostics())
                .any(|diagnostic| diagnostic.code() == expected_code)
        );
    }
}

#[test]
fn duplicate_identity_detection_includes_participants_with_other_order_damage() {
    let invalid_date = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let filesystem = minimal_package_filesystem()
        .directory("/package/Orders/A")
        .file(
            "/package/Orders/A/.spielorders/order.json",
            invalid_date.as_bytes(),
        )
        .directory("/package/Orders/B")
        .file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER);

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("composed Order-local failures preserve the Package")
    };
    assert!(package.healthy_orders().is_empty());
    assert_eq!(package.damaged_orders().len(), 2);
    assert_eq!(
        package
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::OrderIdDuplicate)
            .count(),
        2
    );
}

#[test]
fn healthy_order_sorting_drives_unicode_reusable_value_spelling_counts_and_order() {
    let order = |id: &str, title: &str, date: &str, value: &str| {
        String::from_utf8(MINIMAL_ORDER.to_vec())
            .expect("fixture is UTF-8")
            .replacen("8b6e3f0a-8ec5-4d47-95e1-412652a56d16", id, 1)
            .replacen("Laser components", title, 1)
            .replacen("2026-07-19", date, 1)
            .replacen(
                "  \"status\": \"in_preparation\",",
                &format!(
                    "  \"status\": \"in_preparation\",\n  \"orderer\": \"{value}\",\n  \"ordering_institution\": \"{value}\",\n  \"project\": \"{value}\",\n  \"order_group\": \"{value}\","
                ),
                1,
            )
    };
    let first = order(
        "a1111111-1111-4111-8111-111111111111",
        "  Alpha  ",
        "2026-07-19",
        " Straße Team ",
    );
    let second = order(
        "b2222222-2222-4222-8222-222222222222",
        "Zulu",
        "2026-07-19",
        "STRASSE TEAM",
    );
    let older = order(
        "c3333333-3333-4333-8333-333333333333",
        "Newest path but older date",
        "2025-01-01",
        "Cedarbrook",
    );
    let filesystem = minimal_package_filesystem()
        .directory("/package/Orders/Z-Older")
        .file(
            "/package/Orders/Z-Older/.spielorders/order.json",
            older.as_bytes(),
        )
        .directory("/package/Orders/B-First")
        .file(
            "/package/Orders/B-First/.spielorders/order.json",
            first.as_bytes(),
        )
        .directory("/package/Orders/A-Second")
        .file(
            "/package/Orders/A-Second/.spielorders/order.json",
            second.as_bytes(),
        );

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("valid values produce a canonical candidate")
    };
    assert_eq!(
        package
            .healthy_orders()
            .iter()
            .map(|order| order.document().title.as_str())
            .collect::<Vec<_>>(),
        vec!["Alpha", "Zulu", "Newest path but older date"]
    );
    for values in [
        package.ordering_institutions(),
        package.orderers(),
        package.projects(),
        package.order_groups(),
    ] {
        assert_eq!(
            values
                .iter()
                .map(|value| (value.value(), value.usage_count()))
                .collect::<Vec<_>>(),
            vec![("Cedarbrook", 1), ("Straße Team", 2)]
        );
    }
}

#[test]
fn order_readme_absence_and_unavailability_keep_orders_healthy_with_exact_cache_semantics() {
    let absent = minimal_package_filesystem()
        .directory("/package/Orders/Order")
        .file(
            "/package/Orders/Order/.spielorders/order.json",
            MINIMAL_ORDER,
        );
    let CanonicalPackageLoad::CurrentCandidate(absent_package) =
        inspect_canonical_package(Path::new("/package"), &absent)
    else {
        panic!("README absence is ordinary")
    };
    assert_eq!(absent_package.healthy_orders()[0].readme_text(), Some(""));
    assert!(absent_package.diagnostic_groups().is_empty());

    for filesystem in [
        minimal_package_filesystem()
            .directory("/package/Orders/Order")
            .file(
                "/package/Orders/Order/.spielorders/order.json",
                MINIMAL_ORDER,
            )
            .entry(
                "/package/Orders/Order/README.md",
                FileSystemEntryKind::Directory,
            ),
        minimal_package_filesystem()
            .directory("/package/Orders/Order")
            .file(
                "/package/Orders/Order/.spielorders/order.json",
                MINIMAL_ORDER,
            )
            .file("/package/Orders/Order/README.md", &[0xff]),
    ] {
        let CanonicalPackageLoad::CurrentCandidate(package) =
            inspect_canonical_package(Path::new("/package"), &filesystem)
        else {
            panic!("README unavailability does not damage an Order")
        };
        assert_eq!(package.healthy_orders().len(), 1);
        assert_eq!(
            package.healthy_orders()[0].readme_text(),
            None,
            "a README that exists and could not be read carries no text, \
             which the absent case above shows is not the same as an empty one"
        );
        let [group] = package.diagnostic_groups() else {
            panic!("one unavailable README produces one document group")
        };
        assert_eq!(group.scope(), EvidenceScope::Document);
        assert_eq!(
            group.diagnostics()[0].code(),
            DiagnosticCode::ReadmeUnavailable
        );
        assert_eq!(group.diagnostics()[0].severity(), Severity::Warning);
    }
}

#[test]
fn unclassifiable_filesystem_failures_return_exact_operation_evidence() {
    use spielorders_core::readme::{FileSystemOperation, OperationEvidence};

    let cases = [
        (
            minimal_package_filesystem().fail_canonicalize("/package"),
            FileSystemOperation::Canonicalize,
            "/package",
        ),
        (
            minimal_package_filesystem().fail_observe("/package/.spielorders"),
            FileSystemOperation::ObserveKind,
            "/package/.spielorders",
        ),
        (
            minimal_package_filesystem().fail_list("/package/Orders"),
            FileSystemOperation::ListDirectory,
            "/package/Orders",
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Order")
                .file(
                    "/package/Orders/Order/.spielorders/order.json",
                    MINIMAL_ORDER,
                )
                .fail_observe("/package/Orders/Order/README.md"),
            FileSystemOperation::ObserveKind,
            "/package/Orders/Order/README.md",
        ),
    ];

    for (filesystem, operation, expected_path) in cases {
        let CanonicalPackageLoad::OperationFailed(failure) =
            inspect_canonical_package(Path::new("/package"), &filesystem)
        else {
            panic!("unclassifiable filesystem failure must remain an operation result")
        };
        assert_eq!(
            failure.primary_location().absolute_path(),
            Path::new(expected_path)
        );
        assert_eq!(
            failure.evidence(),
            &OperationEvidence::FileSystem { operation }
        );
    }
}

#[test]
fn metadata_state_distinguishes_current_unsupported_and_not_assessed_without_fake_schemas() {
    let CanonicalPackageLoad::CurrentCandidate(current) =
        inspect_canonical_package(Path::new("/package"), &minimal_package_filesystem())
    else {
        panic!("minimal version 1 is current")
    };
    assert_eq!(current.metadata_state(), MetadataState::Current);

    let future = minimal_package_filesystem().file(
        "/package/.spielorders/version.json",
        br#"{"metadata_version":999}"#,
    );
    let CanonicalPackageLoad::DiagnosticOnly(future) =
        inspect_canonical_package(Path::new("/package"), &future)
    else {
        panic!("recognizable future metadata is diagnostic-only")
    };
    assert_eq!(future.metadata_state(), MetadataState::Unsupported);

    let future_vendors = minimal_package_filesystem().file(
        "/package/.spielorders/vendors.json",
        br#"{"schema_version":2,"vendors":[]}"#,
    );
    let CanonicalPackageLoad::DiagnosticOnly(future_vendors) =
        inspect_canonical_package(Path::new("/package"), &future_vendors)
    else {
        panic!("a recognizable future canonical artifact is diagnostic-only")
    };
    assert_eq!(future_vendors.metadata_state(), MetadataState::Unsupported);

    let missing_version = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file("/package/.spielorders/config.toml", b"")
        .file("/package/.spielorders/vendors.json", VENDORS)
        .directory("/package/Orders");
    let CanonicalPackageLoad::DiagnosticOnly(missing_version) =
        inspect_canonical_package(Path::new("/package"), &missing_version)
    else {
        panic!("missing version evidence is diagnostic-only")
    };
    assert_eq!(missing_version.metadata_state(), MetadataState::NotAssessed);

    assert_eq!(MetadataState::UpgradeRequired.as_str(), "upgrade_required");
    assert_eq!(
        MetadataState::UpgradeResumable.as_str(),
        "upgrade_resumable"
    );
    assert_eq!(MetadataState::UpgradeBlocked.as_str(), "upgrade_blocked");
    assert_eq!(UpgradeArtifactState::Source.as_str(), "source");
    assert_eq!(UpgradeArtifactState::Target.as_str(), "target");
    assert_eq!(UpgradeArtifactState::Blocked.as_str(), "blocked");
    assert_eq!(UpgradeArtifactState::NotAssessed.as_str(), "not_assessed");
}

#[test]
fn real_canonical_inspection_never_lists_or_touches_supporting_material_and_preserves_both_sentinels()
 {
    let temporary = TemporaryTree::new("canonical-package-preservation");
    let root = temporary.path().join("package");
    let metadata = root.join(".spielorders");
    let order = root.join("Orders/Order");
    let order_metadata = order.join(".spielorders");
    let supporting_a = order.join("quote.bin");
    let supporting_b = order.join("nested/drawing.bin");
    std::fs::create_dir_all(&metadata).expect("Package metadata directory is created");
    std::fs::create_dir_all(&order_metadata).expect("Order metadata directory is created");
    std::fs::create_dir_all(supporting_b.parent().unwrap())
        .expect("nested Supporting Material directory is created");
    std::fs::write(metadata.join("config.toml"), b"").unwrap();
    std::fs::write(metadata.join("version.json"), VERSION).unwrap();
    std::fs::write(metadata.join("vendors.json"), VENDORS).unwrap();
    std::fs::write(order_metadata.join("order.json"), MINIMAL_ORDER).unwrap();
    std::fs::write(order.join("README.md"), b"exact README\n").unwrap();
    std::fs::write(&supporting_a, SUPPORTING_A).unwrap();
    std::fs::write(&supporting_b, SUPPORTING_B).unwrap();
    let before_a = std::fs::read(&supporting_a).unwrap();
    let before_b = std::fs::read(&supporting_b).unwrap();
    let canonical_supporting_a = std::fs::canonicalize(&supporting_a).unwrap();
    let canonical_supporting_b = std::fs::canonicalize(&supporting_b).unwrap();
    let canonical_nested = canonical_supporting_b.parent().unwrap().to_path_buf();
    let filesystem = RecordingRealFileSystem::default();

    let result = inspect_canonical_package(&root, &filesystem);

    assert!(matches!(result, CanonicalPackageLoad::CurrentCandidate(_)));
    assert_eq!(std::fs::read(&supporting_a).unwrap(), before_a);
    assert_eq!(std::fs::read(&supporting_b).unwrap(), before_b);
    let effects = filesystem.effects.into_inner();
    let listed = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ListDirectory(path) => Some(path.as_path()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].ends_with("package/Orders"));
    assert!(effects.iter().all(|effect| {
        let path = match effect {
            Effect::Read(path)
            | Effect::Canonicalize(path)
            | Effect::ObserveKind(path)
            | Effect::ListDirectory(path) => path,
        };
        path != &canonical_supporting_a
            && path != &canonical_supporting_b
            && !path.starts_with(&canonical_nested)
    }));
}

#[test]
fn invalid_shape_diagnostics_retain_stable_known_field_paths() {
    let cases: Vec<(TestFileSystem, &str)> = vec![
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/config.toml",
                b"unexpected = true\n",
            ),
            "unexpected",
        ),
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor"}]}"#,
            ),
            "vendors[0].contacts",
        ),
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor","contacts":[{"name":17}]}]}"#,
            ),
            "vendors[0].contacts[0].name",
        ),
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/vendors.json",
                INVALID_VENDOR_CONTACT,
            ),
            "vendors[0].contacts[0].name",
        ),
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":"Vendor","contacts":[],"hidden":"true"}]}"#,
            ),
            "vendors[0].hidden",
        ),
        (
            minimal_package_filesystem().file(
                "/package/.spielorders/vendors.json",
                br#"{"schema_version":1,"vendors":[{"vendor_id":"e0990d31-06aa-42e9-97c8-088e3e894822","name":17,"contacts":[],"hidden":true}]}"#,
            ),
            "vendors[0].name",
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    br#"{"schema_version":1,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Order","date_ordered":"2026-07-19","status":"ordered"}"#,
            ),
            "items",
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    br#"{"schema_version":1,"package_root":"../..","order_id":"8b6e3f0a-8ec5-4d47-95e1-412652a56d16","title":"Order","date_ordered":"2026-07-19","status":"ordered","items":[{"description":"Part","quantity":"one","unit_price":1.0}]}"#,
            ),
            "items[0].quantity",
        ),
        (
            minimal_package_filesystem()
                .directory("/package/Orders/Bad")
                .file(
                    "/package/Orders/Bad/.spielorders/order.json",
                    INVALID_ORDER_ITEM,
                ),
            "items[0].unit_price",
        ),
    ];

    for (filesystem, expected_field) in cases {
        let result = inspect_canonical_package(Path::new("/package"), &filesystem);
        let diagnostics = match &result {
            CanonicalPackageLoad::DiagnosticOnly(package) => package
                .diagnostic_groups()
                .iter()
                .flat_map(|group| group.diagnostics())
                .collect::<Vec<_>>(),
            CanonicalPackageLoad::CurrentCandidate(package) => package
                .diagnostic_groups()
                .iter()
                .flat_map(|group| group.diagnostics())
                .collect::<Vec<_>>(),
            _ => panic!("shape failure is a canonical diagnostic"),
        };
        let diagnostic = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.code() == DiagnosticCode::ManagedArtifactInvalidShape)
            .expect("shape diagnostic exists");
        let DiagnosticEvidence::ManagedArtifact { field, .. } = diagnostic.evidence() else {
            panic!("shape diagnostic uses managed-artifact evidence")
        };
        assert_eq!(field.as_deref(), Some(expected_field));
    }
}

#[test]
fn nested_configuration_projects_every_exact_managed_and_readme_location() {
    let order = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("../..", "../../..", 1);
    let filesystem = TestFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .file(
            "/package/.spielorders/config.toml",
            b"orders_path = \"Purchases/Active\"\nremoved_path = \"Retired\"\n",
        )
        .file("/package/.spielorders/version.json", VERSION)
        .file("/package/.spielorders/vendors.json", VENDORS)
        .directory("/package/Purchases")
        .directory("/package/Purchases/Active")
        .directory("/package/Purchases/Active/Order")
        .file(
            "/package/Purchases/Active/Order/.spielorders/order.json",
            order.as_bytes(),
        );

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("safe nested configuration is current")
    };
    let context = package.context();
    assert_eq!(
        context.metadata_directory().package_relative_path(),
        Some(".spielorders")
    );
    assert_eq!(
        context.config().package_relative_path(),
        Some(".spielorders/config.toml")
    );
    assert_eq!(
        context.version().package_relative_path(),
        Some(".spielorders/version.json")
    );
    assert_eq!(
        context.vendors().package_relative_path(),
        Some(".spielorders/vendors.json")
    );
    assert_eq!(
        context.index().package_relative_path(),
        Some(".spielorders/index.json")
    );
    assert_eq!(
        context.orders_directory().unwrap().package_relative_path(),
        Some("Purchases/Active")
    );
    assert_eq!(
        context
            .removed_orders_directory()
            .unwrap()
            .package_relative_path(),
        Some("Retired")
    );
    assert_eq!(
        context.package_readme().location().package_relative_path(),
        Some("README.md")
    );
    let paths = package.healthy_orders()[0].paths();
    assert_eq!(
        paths.readme().location().package_relative_path(),
        Some("Purchases/Active/Order/README.md")
    );
}

#[test]
fn resolved_orders_directory_failures_use_only_outside_or_reserved_reasons() {
    use spielorders_core::readme::ConfiguredOrdersDirectoryReason;

    let outside = minimal_package_filesystem()
        .directory("/package/Mount")
        .file(
            "/package/.spielorders/config.toml",
            b"orders_path = \"Mount/Orders\"\n",
        )
        .canonical_path("/package/Mount", "/outside");
    let reserved =
        minimal_package_filesystem().canonical_path("/package/Orders", "/package/.spielorders");
    let reserved_similar_prefix = minimal_package_filesystem()
        .directory("/package/.spielorders-link")
        .directory("/package/.spielorders-link/Orders")
        .file(
            "/package/.spielorders/config.toml",
            b"orders_path = \".spielorders-link/Orders\"\n",
        )
        .canonical_path("/package/.spielorders-link", "/package/.spielorders")
        .canonical_path(
            "/package/.spielorders-link/Orders",
            "/package/.spielorders/Orders",
        );

    for (filesystem, expected_reason) in [
        (outside, ConfiguredOrdersDirectoryReason::OutsidePackage),
        (reserved, ConfiguredOrdersDirectoryReason::ReservedMetadata),
        (
            reserved_similar_prefix,
            ConfiguredOrdersDirectoryReason::ReservedMetadata,
        ),
    ] {
        let CanonicalPackageLoad::DiagnosticOnly(package) =
            inspect_canonical_package(Path::new("/package"), &filesystem)
        else {
            panic!("unsafe resolved Orders location damages the Package")
        };
        let diagnostic = package
            .diagnostic_groups()
            .iter()
            .flat_map(|group| group.diagnostics())
            .find(|diagnostic| {
                diagnostic.code() == DiagnosticCode::ConfiguredOrdersDirectoryUnavailable
            })
            .expect("configured-directory diagnostic exists");
        let DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable { reason, .. } =
            diagnostic.evidence()
        else {
            panic!("configured-directory evidence is typed")
        };
        assert_eq!(*reason, expected_reason);
    }
}

#[test]
fn ticket_two_public_vocabulary_uses_every_closed_stable_spelling() {
    use spielorders_core::canonical::{PackageIndexRefresh, PackageIndexState};
    use spielorders_core::readme::{
        ConfigurationPathKind, ConfigurationPathReason, ConfiguredOrdersDirectoryReason,
        FileSystemOperation, ManagedArtifactKind, OrderLocatorReason, OrderSemanticReason,
    };

    let codes = [
        (
            DiagnosticCode::ManagedArtifactMissing,
            "managed_artifact_missing",
        ),
        (
            DiagnosticCode::ManagedArtifactWrongKind,
            "managed_artifact_wrong_kind",
        ),
        (
            DiagnosticCode::ManagedArtifactUnreadable,
            "managed_artifact_unreadable",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidEncoding,
            "managed_artifact_invalid_encoding",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidSyntax,
            "managed_artifact_invalid_syntax",
        ),
        (
            DiagnosticCode::ManagedArtifactInvalidShape,
            "managed_artifact_invalid_shape",
        ),
        (
            DiagnosticCode::ManagedArtifactWrongVersion,
            "managed_artifact_wrong_version",
        ),
        (
            DiagnosticCode::ConfigurationPathInvalid,
            "configuration_path_invalid",
        ),
        (
            DiagnosticCode::ConfigurationPathsOverlap,
            "configuration_paths_overlap",
        ),
        (
            DiagnosticCode::ConfiguredOrdersDirectoryUnavailable,
            "configured_orders_directory_unavailable",
        ),
        (
            DiagnosticCode::OrderSemanticInvalid,
            "order_semantic_invalid",
        ),
        (
            DiagnosticCode::OrderPackageLocatorMismatch,
            "order_package_locator_mismatch",
        ),
        (DiagnosticCode::OrderIdDuplicate, "order_id_duplicate"),
        (DiagnosticCode::VendorIdDuplicate, "vendor_id_duplicate"),
        (DiagnosticCode::VendorNameDuplicate, "vendor_name_duplicate"),
        (
            DiagnosticCode::VendorReferenceUnresolved,
            "vendor_reference_unresolved",
        ),
        (
            DiagnosticCode::OrdersDirectoryEntryDisallowed,
            "orders_directory_entry_disallowed",
        ),
        (DiagnosticCode::ReadmeUnavailable, "readme_unavailable"),
    ];
    for (code, spelling) in codes {
        assert_eq!(code.as_str(), spelling);
    }
    assert_eq!(Severity::Error.as_str(), "error");
    assert_eq!(Severity::Warning.as_str(), "warning");
    assert_eq!(EvidenceScope::Package.as_str(), "package");
    assert_eq!(EvidenceScope::Order.as_str(), "order");
    for (artifact, spelling) in [
        (ManagedArtifactKind::Config, "config"),
        (ManagedArtifactKind::Version, "version"),
        (ManagedArtifactKind::Vendors, "vendors"),
        (ManagedArtifactKind::Order, "order"),
    ] {
        assert_eq!(artifact.as_str(), spelling);
    }
    assert_eq!(ConfigurationPathKind::OrdersPath.as_str(), "orders_path");
    assert_eq!(ConfigurationPathKind::RemovedPath.as_str(), "removed_path");
    for (reason, spelling) in [
        (ConfigurationPathReason::Absolute, "absolute"),
        (
            ConfigurationPathReason::CurrentDirectory,
            "current_directory",
        ),
        (ConfigurationPathReason::ParentTraversal, "parent_traversal"),
        (ConfigurationPathReason::EmptyComponent, "empty_component"),
        (
            ConfigurationPathReason::PlatformSeparator,
            "platform_separator",
        ),
        (
            ConfigurationPathReason::ReservedMetadata,
            "reserved_metadata",
        ),
        (ConfigurationPathReason::NotDirectChild, "not_direct_child"),
        (ConfigurationPathReason::NotOrdinary, "not_ordinary"),
    ] {
        assert_eq!(reason.as_str(), spelling);
    }
    assert_eq!(
        ConfiguredOrdersDirectoryReason::NotDirectory.as_str(),
        "not_directory"
    );
    assert_eq!(
        ConfiguredOrdersDirectoryReason::OutsidePackage.as_str(),
        "outside_package"
    );
    assert_eq!(
        ConfiguredOrdersDirectoryReason::ReservedMetadata.as_str(),
        "reserved_metadata"
    );
    assert_eq!(
        OrderSemanticReason::InvalidGregorianDate.as_str(),
        "invalid_gregorian_date"
    );
    assert_eq!(
        OrderLocatorReason::NotActiveDirectChild.as_str(),
        "not_active_direct_child"
    );
    assert_eq!(
        OrderLocatorReason::DoesNotResolveToPackage.as_str(),
        "does_not_resolve_to_package"
    );
    for (operation, spelling) in [
        (FileSystemOperation::Canonicalize, "canonicalize"),
        (FileSystemOperation::ObserveKind, "observe_kind"),
        (FileSystemOperation::ListDirectory, "list_directory"),
        (FileSystemOperation::Read, "read"),
    ] {
        assert_eq!(operation.as_str(), spelling);
    }
    assert_eq!(PackageIndexState::NotAssessed.as_str(), "not_assessed");
    assert_eq!(PackageIndexRefresh::NotAttempted.as_str(), "not_attempted");
}

#[test]
fn diagnostic_groups_and_composed_order_diagnostics_follow_stable_scope_path_and_code_order() {
    let invalid_date = String::from_utf8(MINIMAL_ORDER.to_vec())
        .expect("fixture is UTF-8")
        .replacen("2026-07-19", "2026-02-31", 1);
    let filesystem = minimal_package_filesystem()
        .file("/package/Orders/loose.txt", b"warning")
        .directory("/package/Orders/B")
        .file("/package/Orders/B/.spielorders/order.json", MINIMAL_ORDER)
        .directory("/package/Orders/A")
        .file(
            "/package/Orders/A/.spielorders/order.json",
            invalid_date.as_bytes(),
        );

    let CanonicalPackageLoad::CurrentCandidate(package) =
        inspect_canonical_package(Path::new("/package"), &filesystem)
    else {
        panic!("warnings and Order damage retain a candidate")
    };
    let groups = package.diagnostic_groups();
    assert_eq!(groups[0].scope(), EvidenceScope::Package);
    assert_eq!(
        groups[1].location().package_relative_path(),
        Some("Orders/A")
    );
    assert_eq!(
        groups[2].location().package_relative_path(),
        Some("Orders/B")
    );
    assert_eq!(
        groups[1]
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect::<Vec<_>>(),
        vec![
            DiagnosticCode::OrderIdDuplicate,
            DiagnosticCode::OrderSemanticInvalid,
        ]
    );
    assert_eq!(
        groups[2].diagnostics()[0].code(),
        DiagnosticCode::OrderIdDuplicate
    );
}
