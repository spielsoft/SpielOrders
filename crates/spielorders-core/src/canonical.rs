//! Canonical Package classification, projection, ordinary Package Index refresh, and exhaustive
//! managed-evidence validation.

use crate::{
    filesystem::{ValidatedReplacementFailure, replace_validated_file, staging_path},
    metadata::{
        Config, IndexDocument, IndexOrder, IndexOrderDirectory, MetadataErrorKind, OrderDocument,
        OrderId, Vendor, VendorsDocument, Version1, VersionDocument, config_shape_field,
        order_shape_field, vendors_shape_field, version_shape_field,
    },
    operations::{FileSystemOperation, OperationFailure, PackageIndexReplacementPhase},
    order::ValidatedOrder,
    package::{PackageMetadata, PackageMetadataValidationFailure, index_semantics_are_valid},
    paths::{
        ConfiguredPath, ConfiguredPathFailure, DirectChildClassification, DirectChildObservation,
        PackagePathError, discover_order_direct_children, resolve_package_layout,
    },
    ports::{
        DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
    },
    readme::{
        ConfigurationPathKind, ConfigurationPathReason, ConfiguredOrdersDirectoryReason,
        Diagnostic, DiagnosticCode, DiagnosticEvidence, EvidenceScope, Location,
        ManagedArtifactKind, OrderLocatorReason, OrderSemanticReason, ReadmeKind, ReadmeRead,
        ReadmeReference, Severity, read_readme,
    },
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnmanagedPackage {
    root: Location,
}

impl UnmanagedPackage {
    pub fn root(&self) -> &Location {
        &self.root
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedContext {
    root: Location,
    metadata_directory: Location,
    config: Location,
    version: Location,
    vendors: Location,
    index: Location,
    orders_directory: Option<Location>,
    removed_orders_directory: Option<Location>,
    package_readme: ReadmeReference,
}

impl ManagedContext {
    pub fn root(&self) -> &Location {
        &self.root
    }

    pub fn metadata_directory(&self) -> &Location {
        &self.metadata_directory
    }

    pub fn config(&self) -> &Location {
        &self.config
    }

    pub fn version(&self) -> &Location {
        &self.version
    }

    pub fn vendors(&self) -> &Location {
        &self.vendors
    }

    pub fn index(&self) -> &Location {
        &self.index
    }

    pub fn orders_directory(&self) -> Option<&Location> {
        self.orders_directory.as_ref()
    }

    pub fn removed_orders_directory(&self) -> Option<&Location> {
        self.removed_orders_directory.as_ref()
    }

    pub fn package_readme(&self) -> &ReadmeReference {
        &self.package_readme
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageIndexState {
    Current,
    Missing,
    Malformed,
    WrongVersion,
    Stale,
    NotAssessed,
}

impl PackageIndexState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::WrongVersion => "wrong_version",
            Self::Stale => "stale",
            Self::NotAssessed => "not_assessed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageIndexRefresh {
    NotNeeded,
    Rebuilt,
    ReplacementFailed,
    NotAttempted,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageIndexMalformedReason {
    WrongKind,
    Unreadable,
    InvalidEncoding,
    InvalidSyntax,
    InvalidShape,
    InvalidSemantics,
    NonDeterministicSerialization,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageIndexStaleReason {
    OrderMembershipChanged,
    OrderPathMismatch,
    ManagedInputWrongKind,
    VendorsChanged,
    OrderMetadataChanged,
    OrderReadmeChanged,
    ChangeEvidenceUncertain,
}

impl PackageIndexStaleReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrderMembershipChanged => "order_membership_changed",
            Self::OrderPathMismatch => "order_path_mismatch",
            Self::ManagedInputWrongKind => "managed_input_wrong_kind",
            Self::VendorsChanged => "vendors_changed",
            Self::OrderMetadataChanged => "order_metadata_changed",
            Self::OrderReadmeChanged => "order_readme_changed",
            Self::ChangeEvidenceUncertain => "change_evidence_uncertain",
        }
    }
}

impl PackageIndexMalformedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WrongKind => "wrong_kind",
            Self::Unreadable => "unreadable",
            Self::InvalidEncoding => "invalid_encoding",
            Self::InvalidSyntax => "invalid_syntax",
            Self::InvalidShape => "invalid_shape",
            Self::InvalidSemantics => "invalid_semantics",
            Self::NonDeterministicSerialization => "non_deterministic_serialization",
        }
    }
}

impl PackageIndexRefresh {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotNeeded => "not_needed",
            Self::Rebuilt => "rebuilt",
            Self::ReplacementFailed => "replacement_failed",
            Self::NotAttempted => "not_attempted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageIndexRecord {
    location: Location,
    state: PackageIndexState,
    refresh: PackageIndexRefresh,
    malformed_reasons: Vec<PackageIndexMalformedReason>,
    stale_reasons: Vec<PackageIndexStaleReason>,
    observed_version: Option<u64>,
    expected_version: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataState {
    Current,
    UpgradeRequired,
    UpgradeResumable,
    UpgradeBlocked,
    Unsupported,
    NotAssessed,
}

impl MetadataState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::UpgradeRequired => "upgrade_required",
            Self::UpgradeResumable => "upgrade_resumable",
            Self::UpgradeBlocked => "upgrade_blocked",
            Self::Unsupported => "unsupported",
            Self::NotAssessed => "not_assessed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeArtifactState {
    Source,
    Target,
    Blocked,
    NotAssessed,
}

impl UpgradeArtifactState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
            Self::Blocked => "blocked",
            Self::NotAssessed => "not_assessed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeArtifactObservation {
    location: Location,
    state: UpgradeArtifactState,
}

impl UpgradeArtifactObservation {
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn state(&self) -> UpgradeArtifactState {
        self.state
    }
}

impl PackageIndexRecord {
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn state(&self) -> PackageIndexState {
        self.state
    }

    pub fn refresh(&self) -> PackageIndexRefresh {
        self.refresh
    }

    pub fn malformed_reasons(&self) -> &[PackageIndexMalformedReason] {
        &self.malformed_reasons
    }

    pub fn stale_reasons(&self) -> &[PackageIndexStaleReason] {
        &self.stale_reasons
    }

    pub fn observed_version(&self) -> Option<u64> {
        self.observed_version
    }

    pub fn expected_version(&self) -> Option<u64> {
        self.expected_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderPaths {
    directory: Location,
    metadata: Location,
    order: Location,
    readme: ReadmeReference,
}

impl OrderPaths {
    pub fn directory(&self) -> &Location {
        &self.directory
    }

    pub fn metadata(&self) -> &Location {
        &self.metadata
    }

    pub fn order(&self) -> &Location {
        &self.order
    }

    pub fn readme(&self) -> &ReadmeReference {
        &self.readme
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HealthyOrder {
    paths: OrderPaths,
    document: OrderDocument,
    item_totals: Vec<f64>,
    order_total: f64,
    index_readme_text: String,
}

impl HealthyOrder {
    pub fn paths(&self) -> &OrderPaths {
        &self.paths
    }

    pub fn document(&self) -> &OrderDocument {
        &self.document
    }

    pub fn item_totals(&self) -> &[f64] {
        &self.item_totals
    }

    pub fn order_total(&self) -> f64 {
        self.order_total
    }

    pub fn index_readme_text(&self) -> &str {
        &self.index_readme_text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DamagedOrder {
    paths: OrderPaths,
    order_id: Option<OrderId>,
}

impl DamagedOrder {
    pub fn paths(&self) -> &OrderPaths {
        &self.paths
    }

    pub fn order_id(&self) -> Option<&OrderId> {
        self.order_id.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableValueProjection {
    value: String,
    usage_count: usize,
}

impl ReusableValueProjection {
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn usage_count(&self) -> usize {
        self.usage_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticGroup {
    scope: EvidenceScope,
    location: Location,
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticGroup {
    pub fn scope(&self) -> EvidenceScope {
        self.scope
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurrentPackageCandidate {
    context: ManagedContext,
    metadata_version: u64,
    metadata_state: MetadataState,
    healthy_orders: Vec<HealthyOrder>,
    damaged_orders: Vec<DamagedOrder>,
    vendors: Vec<Vendor>,
    ordering_institutions: Vec<ReusableValueProjection>,
    orderers: Vec<ReusableValueProjection>,
    projects: Vec<ReusableValueProjection>,
    order_groups: Vec<ReusableValueProjection>,
    diagnostic_groups: Vec<DiagnosticGroup>,
    index: PackageIndexRecord,
}

impl CurrentPackageCandidate {
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    pub fn metadata_version(&self) -> u64 {
        self.metadata_version
    }

    pub fn metadata_state(&self) -> MetadataState {
        self.metadata_state
    }

    pub fn healthy_orders(&self) -> &[HealthyOrder] {
        &self.healthy_orders
    }

    pub fn damaged_orders(&self) -> &[DamagedOrder] {
        &self.damaged_orders
    }

    pub fn vendors(&self) -> &[Vendor] {
        &self.vendors
    }

    pub fn vendor_for_order(&self, order_index: usize) -> Option<&Vendor> {
        let vendor_id = self
            .healthy_orders
            .get(order_index)?
            .document
            .vendor_id
            .as_ref()?;
        self.vendors
            .iter()
            .find(|vendor| vendor.vendor_id == *vendor_id)
    }

    pub fn ordering_institutions(&self) -> &[ReusableValueProjection] {
        &self.ordering_institutions
    }

    pub fn orderers(&self) -> &[ReusableValueProjection] {
        &self.orderers
    }

    pub fn projects(&self) -> &[ReusableValueProjection] {
        &self.projects
    }

    pub fn order_groups(&self) -> &[ReusableValueProjection] {
        &self.order_groups
    }

    pub fn diagnostic_groups(&self) -> &[DiagnosticGroup] {
        &self.diagnostic_groups
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DamagedPackageProjection {
    context: Option<ManagedContext>,
    metadata_state: MetadataState,
    diagnostic_groups: Vec<DiagnosticGroup>,
    index: PackageIndexRecord,
}

impl DamagedPackageProjection {
    pub fn context(&self) -> Option<&ManagedContext> {
        self.context.as_ref()
    }

    pub fn diagnostic_groups(&self) -> &[DiagnosticGroup] {
        &self.diagnostic_groups
    }

    pub fn metadata_state(&self) -> MetadataState {
        self.metadata_state
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeProjection {
    context: ManagedContext,
    state: MetadataState,
    source_version: u64,
    target_version: u64,
    observations: Vec<UpgradeArtifactObservation>,
    diagnostic_groups: Vec<DiagnosticGroup>,
    index: PackageIndexRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationFailedProjection {
    root: Option<Location>,
    context: Option<ManagedContext>,
    failure: OperationFailure,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexReplacementFailedProjection {
    canonical_evidence: CurrentPackageCandidate,
    old_index_bytes: Option<Vec<u8>>,
    failure: OperationFailure,
}

impl IndexReplacementFailedProjection {
    pub fn canonical_evidence(&self) -> &CurrentPackageCandidate {
        &self.canonical_evidence
    }

    pub fn index(&self) -> &PackageIndexRecord {
        self.canonical_evidence.index()
    }

    pub fn old_index_bytes(&self) -> Option<&[u8]> {
        self.old_index_bytes.as_deref()
    }

    pub fn failure(&self) -> &OperationFailure {
        &self.failure
    }
}

impl OperationFailedProjection {
    pub fn root(&self) -> Option<&Location> {
        self.root
            .as_ref()
            .or_else(|| self.context.as_ref().map(ManagedContext::root))
    }

    pub fn context(&self) -> Option<&ManagedContext> {
        self.context.as_ref()
    }

    pub fn failure(&self) -> &OperationFailure {
        &self.failure
    }
}

fn operation_failed(
    root: Option<Location>,
    context: Option<ManagedContext>,
    failure: OperationFailure,
) -> CanonicalPackageLoad {
    CanonicalPackageLoad::OperationFailed(OperationFailedProjection {
        root,
        context,
        failure,
    })
}

impl std::ops::Deref for OperationFailedProjection {
    type Target = OperationFailure;

    fn deref(&self) -> &Self::Target {
        &self.failure
    }
}

impl UpgradeProjection {
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    pub fn state(&self) -> MetadataState {
        self.state
    }

    pub fn source_version(&self) -> u64 {
        self.source_version
    }

    pub fn target_version(&self) -> u64 {
        self.target_version
    }

    pub fn observations(&self) -> &[UpgradeArtifactObservation] {
        &self.observations
    }

    pub fn diagnostic_groups(&self) -> &[DiagnosticGroup] {
        &self.diagnostic_groups
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalPackageLoad {
    Unmanaged(UnmanagedPackage),
    CurrentCandidate(CurrentPackageCandidate),
    DiagnosticOnly(DamagedPackageProjection),
    UpgradePending(UpgradeProjection),
    OperationFailed(OperationFailedProjection),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PackageLoad {
    Unmanaged(UnmanagedPackage),
    Available(CurrentPackageCandidate),
    DiagnosticOnly(DamagedPackageProjection),
    UpgradePending(UpgradeProjection),
    IndexReplacementFailed(IndexReplacementFailedProjection),
    OperationFailed(OperationFailedProjection),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExhaustiveValidationReport {
    context: ManagedContext,
    valid: bool,
    diagnostic_groups: Vec<DiagnosticGroup>,
    package_readme: ReadmeRead,
    order_readmes: Vec<ReadmeRead>,
    index: PackageIndexRecord,
}

impl ExhaustiveValidationReport {
    pub fn context(&self) -> &ManagedContext {
        &self.context
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn diagnostic_groups(&self) -> &[DiagnosticGroup] {
        &self.diagnostic_groups
    }

    pub fn package_readme(&self) -> &ReadmeRead {
        &self.package_readme
    }

    pub fn order_readmes(&self) -> &[ReadmeRead] {
        &self.order_readmes
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExhaustivePackageValidation {
    Unmanaged(UnmanagedPackage),
    Complete(ExhaustiveValidationReport),
    OperationFailed(OperationFailedProjection),
}

pub fn validate_package_exhaustively<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> ExhaustivePackageValidation {
    let mut exhaustive_evidence = ExhaustiveInspectionEvidence::default();
    let canonical = inspect_canonical_package_with_breadth(
        selected_root,
        filesystem,
        CanonicalInspectionBreadth::Exhaustive,
        &mut exhaustive_evidence,
    );
    let mut order_paths = exhaustive_evidence.order_paths;
    order_paths.sort_by(|left, right| left.directory.cmp(&right.directory));
    let (context, mut diagnostic_groups, index) = match canonical {
        CanonicalPackageLoad::Unmanaged(package) => {
            return ExhaustivePackageValidation::Unmanaged(package);
        }
        CanonicalPackageLoad::OperationFailed(failed) => {
            return ExhaustivePackageValidation::OperationFailed(failed);
        }
        CanonicalPackageLoad::CurrentCandidate(package) => {
            (package.context, package.diagnostic_groups, package.index)
        }
        CanonicalPackageLoad::DiagnosticOnly(package) => {
            let context = package
                .context
                .expect("canonical diagnostic-only results establish managed context");
            (context, package.diagnostic_groups, package.index)
        }
        CanonicalPackageLoad::UpgradePending(package) => {
            (package.context, package.diagnostic_groups, package.index)
        }
    };

    let package_readme = match read_readme(&context.package_readme, filesystem) {
        Ok(readme) => readme,
        Err(failure) => {
            return ExhaustivePackageValidation::OperationFailed(OperationFailedProjection {
                root: None,
                context: Some(context),
                failure,
            });
        }
    };
    append_document_diagnostics(&mut diagnostic_groups, &package_readme);

    if let Err(failure) =
        append_reserved_diagnostics(&context, &order_paths, filesystem, &mut diagnostic_groups)
    {
        return ExhaustivePackageValidation::OperationFailed(OperationFailedProjection {
            root: None,
            context: Some(context),
            failure,
        });
    }

    let mut order_readmes = Vec::with_capacity(order_paths.len());
    for paths in &order_paths {
        let readme = match read_readme(&paths.readme, filesystem) {
            Ok(readme) => readme,
            Err(failure) => {
                return ExhaustivePackageValidation::OperationFailed(OperationFailedProjection {
                    root: None,
                    context: Some(context),
                    failure,
                });
            }
        };
        append_document_diagnostics(&mut diagnostic_groups, &readme);
        order_readmes.push(readme);
    }
    sort_diagnostic_groups(&mut diagnostic_groups);
    let valid = !diagnostic_groups.iter().any(|group| {
        group
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
    });

    ExhaustivePackageValidation::Complete(ExhaustiveValidationReport {
        context,
        valid,
        diagnostic_groups,
        package_readme,
        order_readmes,
        index,
    })
}

fn append_document_diagnostics(groups: &mut Vec<DiagnosticGroup>, readme: &ReadmeRead) {
    if readme.diagnostics().is_empty() {
        return;
    }
    if let Some(group) = groups.iter_mut().find(|group| {
        group.scope == EvidenceScope::Document && group.location == *readme.reference().location()
    }) {
        group.diagnostics.extend_from_slice(readme.diagnostics());
        normalize_diagnostics(&mut group.diagnostics);
    } else {
        groups.push(DiagnosticGroup {
            scope: EvidenceScope::Document,
            location: readme.reference().location().clone(),
            diagnostics: readme.diagnostics().to_vec(),
        });
    }
}

fn append_reserved_diagnostics<F: FileSystem>(
    context: &ManagedContext,
    orders: &[OrderPaths],
    filesystem: &F,
    groups: &mut Vec<DiagnosticGroup>,
) -> Result<(), OperationFailure> {
    let package_expected = ["config.toml", "version.json", "vendors.json", "index.json"];
    let package = ReservedDirectorySpec {
        directory: &context.metadata_directory,
        scope: EvidenceScope::Package,
        group_location: &context.root,
        expected: &package_expected,
    };
    let diagnostics =
        reserved_directory_diagnostics(context.root.absolute_path(), package, filesystem, groups)?;
    append_reserved_group(groups, package.scope, package.group_location, diagnostics);
    for order in orders {
        let expected = ["order.json"];
        let specification = ReservedDirectorySpec {
            directory: &order.metadata,
            scope: EvidenceScope::Order,
            group_location: &order.directory,
            expected: &expected,
        };
        let diagnostics = reserved_directory_diagnostics(
            context.root.absolute_path(),
            specification,
            filesystem,
            groups,
        )?;
        append_reserved_group(
            groups,
            specification.scope,
            specification.group_location,
            diagnostics,
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ReservedDirectorySpec<'a> {
    directory: &'a Location,
    scope: EvidenceScope,
    group_location: &'a Location,
    expected: &'a [&'a str],
}

fn reserved_directory_diagnostics<F: FileSystem>(
    root: &Path,
    specification: ReservedDirectorySpec<'_>,
    filesystem: &F,
    groups: &[DiagnosticGroup],
) -> Result<Vec<Diagnostic>, OperationFailure> {
    match filesystem.symlink_metadata(specification.directory.absolute_path()) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        Ok(_) => return Ok(Vec::new()),
        Err(_) => {
            return Err(OperationFailure::filesystem(
                FileSystemOperation::ObserveKind,
                specification.directory.clone(),
                Vec::new(),
            ));
        }
    }
    let entries = filesystem
        .read_directory(specification.directory.absolute_path())
        .map_err(|_| {
            OperationFailure::filesystem(
                FileSystemOperation::ListDirectory,
                specification.directory.clone(),
                Vec::new(),
            )
        })?;
    let mut diagnostics = Vec::new();
    for entry in entries {
        let Some(name) = entry.name().to_str() else {
            return Err(OperationFailure::filesystem(
                FileSystemOperation::ListDirectory,
                specification.directory.clone(),
                Vec::new(),
            ));
        };
        if specification.expected.contains(&name) {
            continue;
        }
        let path = specification.directory.absolute_path().join(name);
        let primary = managed_location(path.clone(), relative_path(root, &path));
        if let Some(destination_name) = recognized_staging_destination(name, specification.expected)
        {
            let destination_path = specification
                .directory
                .absolute_path()
                .join(destination_name);
            let destination = managed_location(
                destination_path.clone(),
                relative_path(root, &destination_path),
            );
            let destination_valid = (destination_name != "index.json").then(|| {
                !groups.iter().any(|group| {
                    group.diagnostics.iter().any(|diagnostic| {
                        diagnostic.severity() == Severity::Error
                            && diagnostic.primary_location() == &destination
                    })
                })
            });
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::StagingArtifactPresent,
                Severity::Warning,
                specification.scope,
                primary,
                vec![destination],
                DiagnosticEvidence::StagingArtifactPresent { destination_valid },
            ));
        } else {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ReservedContentUnexpected,
                Severity::Warning,
                specification.scope,
                primary,
                Vec::new(),
                DiagnosticEvidence::ReservedContentUnexpected {
                    observed_kind: entry.kind(),
                },
            ));
        }
    }
    Ok(diagnostics)
}

fn append_reserved_group(
    groups: &mut Vec<DiagnosticGroup>,
    scope: EvidenceScope,
    group_location: &Location,
    diagnostics: Vec<Diagnostic>,
) {
    if diagnostics.is_empty() {
        return;
    }
    if let Some(group) = groups
        .iter_mut()
        .find(|group| group.scope == scope && group.location == *group_location)
    {
        group.diagnostics.extend(diagnostics);
    } else {
        groups.push(DiagnosticGroup {
            scope,
            location: group_location.clone(),
            diagnostics,
        });
    }
}

fn recognized_staging_destination<'a>(name: &str, expected: &'a [&str]) -> Option<&'a str> {
    let destination = name.strip_prefix('.')?.strip_suffix(".spielorders-stage")?;
    expected
        .iter()
        .copied()
        .find(|expected_name| *expected_name == destination)
}

pub fn open_package<F: FileSystem>(selected_root: &Path, filesystem: &F) -> PackageLoad {
    let filesystem = PackageOpenFileSystem::new(filesystem);
    let mut fast_stale_reasons = Vec::new();
    if let Some(package) =
        try_open_current_index(selected_root, &filesystem, &mut fast_stale_reasons)
    {
        return PackageLoad::Available(package);
    }
    open_package_canonically(selected_root, &filesystem, fast_stale_reasons)
}

fn open_package_canonically<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
    fast_stale_reasons: Vec<PackageIndexStaleReason>,
) -> PackageLoad {
    match inspect_canonical_package(selected_root, filesystem) {
        CanonicalPackageLoad::Unmanaged(package) => PackageLoad::Unmanaged(package),
        CanonicalPackageLoad::DiagnosticOnly(mut package) => {
            package.index = assess_index_without_freshness(package.index, filesystem);
            PackageLoad::DiagnosticOnly(package)
        }
        CanonicalPackageLoad::UpgradePending(mut package) => {
            package.index = assess_index_without_freshness(package.index, filesystem);
            PackageLoad::UpgradePending(package)
        }
        CanonicalPackageLoad::OperationFailed(package) => PackageLoad::OperationFailed(package),
        CanonicalPackageLoad::CurrentCandidate(mut package) => {
            let index = index_document(&package);
            let candidate = index
                .to_json_bytes()
                .expect("validated canonical facts serialize as metadata version 1");
            let index_path = package.context.index.absolute_path().to_path_buf();
            let assessment = match assess_index(package.index.clone(), filesystem) {
                Ok(assessment) => assessment,
                Err(failure) => {
                    return PackageLoad::OperationFailed(OperationFailedProjection {
                        root: None,
                        context: Some(package.context.clone()),
                        failure,
                    });
                }
            };
            let old_index_bytes = assessment.bytes;
            package.index = assessment.record;
            if package.index.state == PackageIndexState::NotAssessed {
                let mut stale_reasons = stale_index_reasons(
                    assessment
                        .document
                        .as_ref()
                        .expect("a valid deterministic index remains available"),
                    &package,
                );
                if !fast_stale_reasons.is_empty() {
                    if !fast_stale_reasons
                        .contains(&PackageIndexStaleReason::ChangeEvidenceUncertain)
                    {
                        stale_reasons.retain(|reason| {
                            *reason != PackageIndexStaleReason::ChangeEvidenceUncertain
                        });
                    }
                    stale_reasons.extend(fast_stale_reasons.iter().copied());
                    stale_reasons.sort();
                    stale_reasons.dedup();
                }
                package.index.stale_reasons = stale_reasons;
                package.index.state = PackageIndexState::Stale;
            }
            let replacement =
                replace_validated_file(&index_path, &candidate, filesystem, |staged| {
                    if staged != candidate {
                        return Err(crate::metadata::MetadataError::invalid_semantics());
                    }
                    let parsed = IndexDocument::from_json_bytes(staged)?;
                    if !index_semantics_are_valid(&parsed) {
                        Err(crate::metadata::MetadataError::invalid_semantics())
                    } else if parsed.to_json_bytes()? == staged {
                        Ok(())
                    } else {
                        Err(crate::metadata::MetadataError::non_deterministic())
                    }
                });
            if let Err(error) = replacement {
                package.index.refresh = PackageIndexRefresh::ReplacementFailed;
                let phase = match error.failure() {
                    ValidatedReplacementFailure::StagingWrite(_) => {
                        PackageIndexReplacementPhase::WriteStage
                    }
                    ValidatedReplacementFailure::ValidationRead(_) => {
                        PackageIndexReplacementPhase::ReadStage
                    }
                    ValidatedReplacementFailure::CandidateValidation(_) => {
                        PackageIndexReplacementPhase::CandidateValidation
                    }
                    ValidatedReplacementFailure::Durability(_) => {
                        PackageIndexReplacementPhase::SyncStage
                    }
                    ValidatedReplacementFailure::Replacement(_) => {
                        PackageIndexReplacementPhase::Replace
                    }
                    ValidatedReplacementFailure::InvalidDestination => {
                        PackageIndexReplacementPhase::WriteStage
                    }
                };
                let cleanup_failure = error
                    .cleanup_failure()
                    .map(|_| PackageIndexReplacementPhase::RemoveStage);
                let staging = staging_path(&index_path)
                    .expect("the established Package Index has a staging path");
                let staging_relative =
                    relative_path(package.context.root.absolute_path(), &staging);
                let related_locations = vec![managed_location(staging, staging_relative)];
                let failure = OperationFailure::package_index_replacement(
                    package.context.index.clone(),
                    related_locations,
                    phase,
                    cleanup_failure,
                );
                return PackageLoad::IndexReplacementFailed(IndexReplacementFailedProjection {
                    canonical_evidence: package,
                    old_index_bytes,
                    failure,
                });
            }
            package.index.state = PackageIndexState::Current;
            package.index.refresh = PackageIndexRefresh::Rebuilt;
            package.index.malformed_reasons.clear();
            package.index.stale_reasons.clear();
            package.index.observed_version = None;
            package.index.expected_version = None;
            PackageLoad::Available(package)
        }
    }
}

fn try_open_current_index<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
    stale_reasons: &mut Vec<PackageIndexStaleReason>,
) -> Option<CurrentPackageCandidate> {
    let root = filesystem.canonicalize(selected_root).ok()?;
    if filesystem
        .symlink_metadata(&root.join(".spielorders"))
        .ok()?
        != Some(FileSystemEntryKind::Directory)
    {
        return None;
    }
    let context_without_paths = managed_root_context(&root);
    let config_observation = filesystem
        .observe_file_change(context_without_paths.config.absolute_path())
        .ok()?;
    let version_observation = filesystem
        .observe_file_change(context_without_paths.version.absolute_path())
        .ok()?;
    let index_observation = filesystem
        .observe_file_change(context_without_paths.index.absolute_path())
        .ok()?;
    if [config_observation, version_observation, index_observation]
        .into_iter()
        .any(|observation| observation.kind() != Some(FileSystemEntryKind::File))
    {
        return None;
    }

    let config = Config::from_toml_bytes(
        &filesystem
            .read(context_without_paths.config.absolute_path())
            .ok()?,
    )
    .ok()?;
    let version = VersionDocument::from_json_bytes(
        &filesystem
            .read(context_without_paths.version.absolute_path())
            .ok()?,
    )
    .ok()?;
    if version.metadata_version != Version1 {
        return None;
    }
    let assessment = assess_index(
        PackageIndexRecord {
            location: context_without_paths.index.clone(),
            state: PackageIndexState::NotAssessed,
            refresh: PackageIndexRefresh::NotAttempted,
            malformed_reasons: Vec::new(),
            stale_reasons: Vec::new(),
            observed_version: None,
            expected_version: None,
        },
        filesystem,
    )
    .ok()?;
    if assessment.record.state != PackageIndexState::NotAssessed {
        return None;
    }
    let index = assessment.document?;

    let vendors_observation = filesystem
        .observe_file_change(context_without_paths.vendors.absolute_path())
        .ok()?;
    if vendors_observation.kind() != Some(FileSystemEntryKind::File) {
        return None;
    }
    for observation in [config_observation, version_observation] {
        if let Some(reason) = cutoff_reason(
            index_observation,
            observation,
            PackageIndexStaleReason::ChangeEvidenceUncertain,
        ) {
            stale_reasons.push(reason);
        }
    }
    if let Some(reason) = cutoff_reason(
        index_observation,
        vendors_observation,
        PackageIndexStaleReason::VendorsChanged,
    ) {
        stale_reasons.push(reason);
    }
    let tracking_filesystem = InspectionFileSystem::new(filesystem);
    let layout = resolve_package_layout(&root, &config, &tracking_filesystem).ok()?;
    let layout = discover_order_direct_children(layout, &tracking_filesystem).ok()?;
    let context = managed_context(
        &root,
        layout.orders_directory(),
        layout.removed_orders_directory(),
    );

    let mut candidate_paths = layout
        .direct_children()
        .iter()
        .filter(|entry| entry.classification() == DirectChildClassification::OrderCandidate)
        .map(|entry| {
            (
                relative_path(&root, entry.path()),
                entry.path().to_path_buf(),
            )
        })
        .collect::<Vec<_>>();
    candidate_paths.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let indexed_paths = index
        .orders
        .iter()
        .map(|entry| entry.order_directory.as_str())
        .collect::<Vec<_>>();
    if indexed_paths
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        != candidate_paths
            .iter()
            .map(|(relative, _)| relative.as_str())
            .collect::<std::collections::BTreeSet<_>>()
    {
        return None;
    }
    let orders_directory = context.orders_directory.as_ref()?.package_relative_path()?;
    let prefix = format!("{orders_directory}/");
    if indexed_paths.iter().any(|path| {
        path.strip_prefix(&prefix)
            .is_none_or(|remainder| remainder.is_empty() || remainder.contains('/'))
    }) {
        return None;
    }

    for (_, order_directory) in &candidate_paths {
        let paths = order_paths(&root, order_directory);
        let order_observation = filesystem
            .observe_file_change(paths.order.absolute_path())
            .ok()?;
        if order_observation.kind() != Some(FileSystemEntryKind::File) {
            return None;
        }
        if let Some(reason) = cutoff_reason(
            index_observation,
            order_observation,
            PackageIndexStaleReason::OrderMetadataChanged,
        ) {
            stale_reasons.push(reason);
        }
        let readme_observation = filesystem
            .observe_file_change(paths.readme.location().absolute_path())
            .ok()?;
        match readme_observation.kind() {
            None | Some(FileSystemEntryKind::File) => {
                if let Some(reason) = cutoff_reason(
                    index_observation,
                    readme_observation,
                    PackageIndexStaleReason::OrderReadmeChanged,
                ) {
                    stale_reasons.push(reason);
                }
            }
            Some(_) => return None,
        }
    }
    stale_reasons.sort();
    stale_reasons.dedup();
    if !stale_reasons.is_empty() {
        return None;
    }

    let mut package_diagnostics = layout
        .direct_children()
        .iter()
        .filter_map(|observation| {
            let DirectChildClassification::Disallowed(kind) = observation.classification() else {
                return None;
            };
            Some(orders_directory_entry_disallowed_diagnostic(
                &root,
                observation,
                kind,
            ))
        })
        .collect::<Vec<_>>();
    normalize_diagnostics(&mut package_diagnostics);
    let mut diagnostic_groups = if package_diagnostics.is_empty() {
        Vec::new()
    } else {
        vec![DiagnosticGroup {
            scope: EvidenceScope::Package,
            location: context.root.clone(),
            diagnostics: package_diagnostics,
        }]
    };
    sort_diagnostic_groups(&mut diagnostic_groups);
    let healthy_orders = index
        .orders
        .iter()
        .map(|entry| {
            let order_directory = root.join(entry.order_directory.as_str());
            let validated = entry
                .order
                .validate()
                .expect("a semantically valid index contains valid Orders");
            healthy_order_from_validated(
                order_paths(&root, &order_directory),
                &validated,
                entry.readme.clone(),
            )
        })
        .collect::<Vec<_>>();
    Some(current_package_candidate(
        context,
        healthy_orders,
        Vec::new(),
        index.vendors,
        diagnostic_groups,
        PackageIndexRecord {
            location: context_without_paths.index,
            state: PackageIndexState::Current,
            refresh: PackageIndexRefresh::NotNeeded,
            malformed_reasons: Vec::new(),
            stale_reasons: Vec::new(),
            observed_version: None,
            expected_version: None,
        },
    ))
}

fn cutoff_reason(
    index: FileChangeObservation,
    input: FileChangeObservation,
    changed_reason: PackageIndexStaleReason,
) -> Option<PackageIndexStaleReason> {
    match (index.evidence(), input.evidence()) {
        (
            FileChangeEvidence::Comparable {
                comparison_domain: index_domain,
                sequence: index_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: input_domain,
                sequence: input_sequence,
            },
        ) if index_domain == input_domain && index_sequence > input_sequence => None,
        (
            FileChangeEvidence::Comparable {
                comparison_domain: index_domain,
                sequence: index_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: input_domain,
                sequence: input_sequence,
            },
        ) if index_domain == input_domain && input_sequence > index_sequence => {
            Some(changed_reason)
        }
        _ => Some(PackageIndexStaleReason::ChangeEvidenceUncertain),
    }
}

fn index_document(package: &CurrentPackageCandidate) -> IndexDocument {
    IndexDocument {
        schema_version: Version1,
        orders: package
            .healthy_orders
            .iter()
            .map(|order| IndexOrder {
                order_directory: IndexOrderDirectory::new(
                    order
                        .paths
                        .directory
                        .package_relative_path()
                        .expect("healthy Order paths are established"),
                )
                .expect("healthy Order paths are valid index paths"),
                order: order.document.clone(),
                readme: order.index_readme_text.clone(),
            })
            .collect(),
        vendors: package.vendors.clone(),
    }
}

fn assess_index_without_freshness<F: FileSystem>(
    record: PackageIndexRecord,
    filesystem: &F,
) -> PackageIndexRecord {
    assess_index(record.clone(), filesystem)
        .map(|assessment| assessment.record)
        .unwrap_or(record)
}

struct IndexAssessment {
    record: PackageIndexRecord,
    bytes: Option<Vec<u8>>,
    document: Option<IndexDocument>,
}

fn assess_index<F: FileSystem>(
    mut record: PackageIndexRecord,
    filesystem: &F,
) -> Result<IndexAssessment, OperationFailure> {
    let path = record.location.absolute_path().to_path_buf();
    let kind = filesystem
        .symlink_metadata(&path)
        .map_err(|_| OperationFailure::observe_kind(&record.location))?;
    let Some(kind) = kind else {
        record.state = PackageIndexState::Missing;
        return Ok(IndexAssessment {
            record,
            bytes: None,
            document: None,
        });
    };
    if kind != FileSystemEntryKind::File {
        record.state = PackageIndexState::Malformed;
        record.malformed_reasons = vec![PackageIndexMalformedReason::WrongKind];
        return Ok(IndexAssessment {
            record,
            bytes: None,
            document: None,
        });
    }
    let bytes = match filesystem.read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            record.state = PackageIndexState::Malformed;
            record.malformed_reasons = vec![PackageIndexMalformedReason::Unreadable];
            return Ok(IndexAssessment {
                record,
                bytes: None,
                document: None,
            });
        }
    };
    if let Some(observed) = recognizable_version(&bytes, "schema_version")
        && observed != 1
    {
        record.state = PackageIndexState::WrongVersion;
        record.observed_version = Some(observed);
        record.expected_version = Some(1);
        return Ok(IndexAssessment {
            record,
            bytes: Some(bytes),
            document: None,
        });
    }
    let document = match IndexDocument::from_json_bytes(&bytes) {
        Ok(document) => document,
        Err(error) => {
            record.state = PackageIndexState::Malformed;
            record.malformed_reasons = vec![match error.kind() {
                MetadataErrorKind::Encoding => PackageIndexMalformedReason::InvalidEncoding,
                MetadataErrorKind::Syntax => PackageIndexMalformedReason::InvalidSyntax,
                MetadataErrorKind::Shape | MetadataErrorKind::Serialization => {
                    PackageIndexMalformedReason::InvalidShape
                }
            }];
            return Ok(IndexAssessment {
                record,
                bytes: Some(bytes),
                document: None,
            });
        }
    };
    if !index_semantics_are_valid(&document) {
        record.state = PackageIndexState::Malformed;
        record.malformed_reasons = vec![PackageIndexMalformedReason::InvalidSemantics];
    } else if document.to_json_bytes().ok().as_deref() != Some(bytes.as_slice()) {
        record.state = PackageIndexState::Malformed;
        record.malformed_reasons = vec![PackageIndexMalformedReason::NonDeterministicSerialization];
    }
    Ok(IndexAssessment {
        record,
        bytes: Some(bytes),
        document: Some(document),
    })
}

fn stale_index_reasons(
    index: &IndexDocument,
    package: &CurrentPackageCandidate,
) -> Vec<PackageIndexStaleReason> {
    let indexed_paths = index
        .orders
        .iter()
        .map(|entry| entry.order_directory.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let discovered_paths = package
        .healthy_orders
        .iter()
        .map(|order| &order.paths)
        .chain(package.damaged_orders.iter().map(|order| &order.paths))
        .map(|paths| {
            paths
                .directory
                .package_relative_path()
                .expect("discovered Order paths are established")
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut reasons = Vec::new();
    if indexed_paths != discovered_paths {
        reasons.push(PackageIndexStaleReason::OrderMembershipChanged);
    }
    let orders_directory = package
        .context
        .orders_directory
        .as_ref()
        .and_then(Location::package_relative_path)
        .expect("a current Package has an established Orders Directory");
    let direct_child_prefix = format!("{orders_directory}/");
    if index.orders.iter().any(|entry| {
        entry
            .order_directory
            .as_str()
            .strip_prefix(&direct_child_prefix)
            .is_none_or(|remainder| remainder.is_empty() || remainder.contains('/'))
    }) {
        reasons.push(PackageIndexStaleReason::OrderPathMismatch);
    }
    if package.diagnostic_groups.iter().any(|group| {
        group
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ManagedArtifactWrongKind)
    }) {
        reasons.push(PackageIndexStaleReason::ManagedInputWrongKind);
    }
    if index.vendors != package.vendors {
        reasons.push(PackageIndexStaleReason::VendorsChanged);
    }
    if package.healthy_orders.iter().any(|order| {
        let path = order
            .paths
            .directory
            .package_relative_path()
            .expect("healthy Order paths are established");
        index
            .orders
            .iter()
            .find(|entry| entry.order_directory.as_str() == path)
            .is_some_and(|entry| entry.order != order.document)
    }) {
        reasons.push(PackageIndexStaleReason::OrderMetadataChanged);
    }
    if package.healthy_orders.iter().any(|order| {
        let path = order
            .paths
            .directory
            .package_relative_path()
            .expect("healthy Order paths are established");
        index
            .orders
            .iter()
            .find(|entry| entry.order_directory.as_str() == path)
            .is_some_and(|entry| entry.readme != order.index_readme_text)
    }) {
        reasons.push(PackageIndexStaleReason::OrderReadmeChanged);
    }
    reasons.push(PackageIndexStaleReason::ChangeEvidenceUncertain);
    reasons
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalInspectionBreadth {
    Ordinary,
    Exhaustive,
}

#[derive(Default)]
struct ExhaustiveInspectionEvidence {
    order_paths: Vec<OrderPaths>,
}

pub fn inspect_canonical_package<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> CanonicalPackageLoad {
    let mut ignored_evidence = ExhaustiveInspectionEvidence::default();
    inspect_canonical_package_with_breadth(
        selected_root,
        filesystem,
        CanonicalInspectionBreadth::Ordinary,
        &mut ignored_evidence,
    )
}

fn inspect_canonical_package_with_breadth<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
    breadth: CanonicalInspectionBreadth,
    exhaustive_evidence: &mut ExhaustiveInspectionEvidence,
) -> CanonicalPackageLoad {
    let root = match filesystem.canonicalize(selected_root) {
        Ok(root) => root,
        Err(_) => {
            let location = Location::absolute(selected_root.to_path_buf())
                .expect("selected root must be absolute when canonicalization fails");
            return operation_failed(
                Some(location.clone()),
                None,
                OperationFailure::filesystem(
                    FileSystemOperation::Canonicalize,
                    location,
                    Vec::new(),
                ),
            );
        }
    };
    let absolute_root = Location::absolute(root.clone()).expect("canonical root is absolute");
    let metadata_path = root.join(".spielorders");
    let metadata_kind = match filesystem.symlink_metadata(&metadata_path) {
        Ok(kind) => kind,
        Err(_) => {
            return operation_failed(
                Some(absolute_root),
                None,
                OperationFailure::filesystem(
                    FileSystemOperation::ObserveKind,
                    managed_location(metadata_path, ".spielorders"),
                    Vec::new(),
                ),
            );
        }
    };
    if metadata_kind.is_none() {
        return CanonicalPackageLoad::Unmanaged(UnmanagedPackage {
            root: absolute_root,
        });
    }
    // A present non-directory metadata entry is managed rather than unmanaged.
    // Classification continues through the exact required child observations;
    // a production filesystem normally reports that those observations failed,
    // while a substitutable port may persistently report the children absent.

    let config_path = metadata_path.join("config.toml");
    let version_path = metadata_path.join("version.json");
    let vendors_path = metadata_path.join("vendors.json");
    let mut package_diagnostics = Vec::new();
    let config_bytes = match required_artifact(
        &root,
        &config_path,
        ManagedArtifactKind::Config,
        EvidenceScope::Package,
        filesystem,
        &mut package_diagnostics,
    ) {
        Ok(bytes) => bytes,
        Err(failure) => return operation_failed(None, Some(managed_root_context(&root)), failure),
    };
    let version_bytes = match required_artifact(
        &root,
        &version_path,
        ManagedArtifactKind::Version,
        EvidenceScope::Package,
        filesystem,
        &mut package_diagnostics,
    ) {
        Ok(bytes) => bytes,
        Err(failure) => return operation_failed(None, Some(managed_root_context(&root)), failure),
    };
    let vendors_bytes = match required_artifact(
        &root,
        &vendors_path,
        ManagedArtifactKind::Vendors,
        EvidenceScope::Package,
        filesystem,
        &mut package_diagnostics,
    ) {
        Ok(bytes) => bytes,
        Err(failure) => return operation_failed(None, Some(managed_root_context(&root)), failure),
    };
    let marker_version = version_bytes
        .as_deref()
        .and_then(|bytes| recognizable_version(bytes, "metadata_version"));
    let vendors_version = vendors_bytes
        .as_deref()
        .and_then(|bytes| recognizable_version(bytes, "schema_version"));
    let metadata_state = metadata_state(marker_version, vendors_version);
    let config = config_bytes.and_then(|bytes| match Config::from_toml_bytes(&bytes) {
        Ok(config) => Some(config),
        Err(error) => {
            package_diagnostics.push(metadata_diagnostic(
                &root,
                &config_path,
                ManagedArtifactKind::Config,
                EvidenceScope::Package,
                error.kind(),
                (error.kind() == MetadataErrorKind::Shape)
                    .then(|| config_shape_field(&bytes))
                    .flatten(),
            ));
            None
        }
    });
    let version = version_bytes
        .and_then(|bytes| decode_version(&root, &version_path, &bytes, &mut package_diagnostics));
    let vendors = vendors_bytes
        .and_then(|bytes| decode_vendors(&root, &vendors_path, &bytes, &mut package_diagnostics));
    let Some(config) = config else {
        return damaged_without_context(&root, metadata_state, package_diagnostics);
    };
    let tracking_filesystem = InspectionFileSystem::new(filesystem);
    let layout = match resolve_package_layout(&root, &config, &tracking_filesystem) {
        Ok(layout) => layout,
        Err(PackagePathError::FileSystem(_)) => {
            return operation_failed(
                None,
                Some(managed_root_context(&root)),
                tracking_filesystem
                    .take_failure(&root)
                    .expect("filesystem errors record their attempted operation"),
            );
        }
        Err(PackagePathError::RootNotDirectory) => {
            return operation_failed(
                Some(absolute_root),
                None,
                OperationFailure::filesystem(
                    FileSystemOperation::ObserveKind,
                    managed_location(root.clone(), "."),
                    Vec::new(),
                ),
            );
        }
        Err(error) => {
            package_diagnostics.push(layout_diagnostic(&root, &config, error));
            return damaged_without_context(&root, metadata_state, package_diagnostics);
        }
    };
    let context = managed_context(
        &root,
        layout.orders_directory(),
        layout.removed_orders_directory(),
    );
    let index = PackageIndexRecord {
        location: context.index.clone(),
        state: PackageIndexState::NotAssessed,
        refresh: PackageIndexRefresh::NotAttempted,
        malformed_reasons: Vec::new(),
        stale_reasons: Vec::new(),
        observed_version: None,
        expected_version: None,
    };

    if !package_diagnostics.is_empty() && breadth == CanonicalInspectionBreadth::Ordinary {
        normalize_diagnostics(&mut package_diagnostics);
        return CanonicalPackageLoad::DiagnosticOnly(DamagedPackageProjection {
            context: Some(context),
            metadata_state,
            diagnostic_groups: vec![DiagnosticGroup {
                scope: EvidenceScope::Package,
                location: managed_location(root.clone(), "."),
                diagnostics: package_diagnostics,
            }],
            index,
        });
    }
    let vendors_are_trustworthy = vendors.is_some();
    let vendors = vendors.unwrap_or(VendorsDocument {
        schema_version: Version1,
        vendors: Vec::new(),
    });
    let validation_version = version.unwrap_or(VersionDocument {
        metadata_version: Version1,
    });
    let normalized_vendors = match PackageMetadata::new(&validation_version, &[], &vendors)
        .validate()
    {
        Ok(validated) => validated.vendors().clone(),
        Err(failures) => {
            for failure in failures {
                match failure {
                    PackageMetadataValidationFailure::DuplicateVendorId { vendor_indices } => {
                        let vendor_id = vendors.vendors[vendor_indices[0]].vendor_id.clone();
                        package_diagnostics.push(Diagnostic::new(
                            DiagnosticCode::VendorIdDuplicate,
                            Severity::Error,
                            EvidenceScope::Package,
                            context.vendors.clone(),
                            Vec::new(),
                            DiagnosticEvidence::VendorIdDuplicate { vendor_id },
                        ));
                    }
                    PackageMetadataValidationFailure::DuplicateVendorName { vendor_indices } => {
                        let vendor_ids = vendor_indices
                            .iter()
                            .map(|index| vendors.vendors[*index].vendor_id.clone())
                            .collect();
                        let observed_names = vendor_indices
                            .iter()
                            .map(|index| vendors.vendors[*index].name.as_str().to_owned())
                            .collect();
                        package_diagnostics.push(Diagnostic::new(
                            DiagnosticCode::VendorNameDuplicate,
                            Severity::Error,
                            EvidenceScope::Package,
                            context.vendors.clone(),
                            Vec::new(),
                            DiagnosticEvidence::VendorNameDuplicate {
                                vendor_ids,
                                observed_names,
                            },
                        ));
                    }
                    _ => unreachable!("no Orders or index participate in Vendor validation"),
                }
            }
            if breadth == CanonicalInspectionBreadth::Ordinary {
                normalize_diagnostics(&mut package_diagnostics);
                return CanonicalPackageLoad::DiagnosticOnly(DamagedPackageProjection {
                    context: Some(context),
                    metadata_state,
                    diagnostic_groups: vec![DiagnosticGroup {
                        scope: EvidenceScope::Package,
                        location: managed_location(root.clone(), "."),
                        diagnostics: package_diagnostics,
                    }],
                    index,
                });
            }
            vendors.clone()
        }
    };
    let layout = match discover_order_direct_children(layout, &tracking_filesystem) {
        Ok(layout) => layout,
        Err(_) => {
            return operation_failed(
                None,
                Some(context.clone()),
                tracking_filesystem
                    .take_failure(&root)
                    .expect("directory-list failures record their attempted operation"),
            );
        }
    };
    let mut healthy_orders = Vec::new();
    let mut damaged_orders = Vec::new();
    let mut diagnostic_groups = Vec::new();
    for observation in layout.direct_children() {
        match observation.classification() {
            DirectChildClassification::IgnoredHidden => continue,
            DirectChildClassification::Disallowed(kind) => {
                package_diagnostics.push(orders_directory_entry_disallowed_diagnostic(
                    &root,
                    observation,
                    kind,
                ));
                continue;
            }
            DirectChildClassification::OrderCandidate => {}
        }
        let paths = order_paths(&root, observation.path());
        if breadth == CanonicalInspectionBreadth::Exhaustive {
            exhaustive_evidence.order_paths.push(paths.clone());
        }
        let mut order_diagnostics = Vec::new();
        let order_bytes = match required_artifact(
            &root,
            paths.order.absolute_path(),
            ManagedArtifactKind::Order,
            EvidenceScope::Order,
            filesystem,
            &mut order_diagnostics,
        ) {
            Ok(bytes) => bytes,
            Err(failure) => return operation_failed(None, Some(context.clone()), failure),
        };
        let Some(order_bytes) = order_bytes else {
            quarantine_order(
                paths,
                None,
                order_diagnostics,
                &mut damaged_orders,
                &mut diagnostic_groups,
            );
            continue;
        };
        let recognizable_order_version = recognizable_version(&order_bytes, "schema_version");
        let trustworthy_order_id = (recognizable_order_version == Some(1))
            .then(|| extract_order_id(&order_bytes))
            .flatten();
        let order = match OrderDocument::from_json_bytes(&order_bytes) {
            Ok(order) => order,
            Err(error) => {
                if recognizable_order_version.is_some_and(|version| version != 1) {
                    order_diagnostics.push(Diagnostic::managed_artifact_with_details(
                        DiagnosticCode::ManagedArtifactWrongVersion,
                        EvidenceScope::Order,
                        paths.order.clone(),
                        ManagedArtifactKind::Order,
                        None,
                        Some("schema_version".into()),
                        recognizable_order_version,
                        Some(1),
                    ));
                } else {
                    let code = match error.kind() {
                        MetadataErrorKind::Encoding => {
                            DiagnosticCode::ManagedArtifactInvalidEncoding
                        }
                        MetadataErrorKind::Syntax => DiagnosticCode::ManagedArtifactInvalidSyntax,
                        MetadataErrorKind::Shape | MetadataErrorKind::Serialization => {
                            DiagnosticCode::ManagedArtifactInvalidShape
                        }
                    };
                    order_diagnostics.push(Diagnostic::managed_artifact_with_details(
                        code,
                        EvidenceScope::Order,
                        paths.order.clone(),
                        ManagedArtifactKind::Order,
                        None,
                        (error.kind() == MetadataErrorKind::Shape)
                            .then(|| order_shape_field(&order_bytes))
                            .flatten(),
                        None,
                        None,
                    ));
                }
                quarantine_order(
                    paths,
                    trustworthy_order_id,
                    order_diagnostics,
                    &mut damaged_orders,
                    &mut diagnostic_groups,
                );
                continue;
            }
        };
        let validated = match order.validate() {
            Ok(validated) => validated,
            Err(_failures) => {
                order_diagnostics.push(Diagnostic::new(
                    DiagnosticCode::OrderSemanticInvalid,
                    Severity::Error,
                    EvidenceScope::Order,
                    paths.order.clone(),
                    Vec::new(),
                    DiagnosticEvidence::OrderSemanticInvalid {
                        order_id: order.order_id.clone(),
                        field: "date_ordered".into(),
                        reason: OrderSemanticReason::InvalidGregorianDate,
                    },
                ));
                quarantine_order(
                    paths,
                    Some(order.order_id.clone()),
                    order_diagnostics,
                    &mut damaged_orders,
                    &mut diagnostic_groups,
                );
                continue;
            }
        };
        let resolved_order_directory = match tracking_filesystem.canonicalize(observation.path()) {
            Ok(path) => path,
            Err(_) => {
                return operation_failed(
                    None,
                    Some(context.clone()),
                    tracking_filesystem
                        .take_failure(&root)
                        .expect("filesystem errors record their attempted operation"),
                );
            }
        };
        let locator_reason = if resolved_order_directory.parent() != Some(layout.orders_directory())
        {
            Some(OrderLocatorReason::NotActiveDirectChild)
        } else {
            let parent_count = validated.order().package_root.as_str().split('/').count();
            (resolved_order_directory.ancestors().nth(parent_count) != Some(root.as_path()))
                .then_some(OrderLocatorReason::DoesNotResolveToPackage)
        };
        if let Some(reason) = locator_reason {
            order_diagnostics.push(Diagnostic::new(
                DiagnosticCode::OrderPackageLocatorMismatch,
                Severity::Error,
                EvidenceScope::Order,
                paths.order.clone(),
                vec![context.root.clone()],
                DiagnosticEvidence::OrderPackageLocatorMismatch {
                    order_id: validated.order().order_id.clone(),
                    package_root: validated.order().package_root.as_str().to_owned(),
                    reason,
                },
            ));
            quarantine_order(
                paths,
                Some(validated.order().order_id.clone()),
                order_diagnostics,
                &mut damaged_orders,
                &mut diagnostic_groups,
            );
            continue;
        }
        if vendors_are_trustworthy
            && let Some(vendor_id) = &validated.order().vendor_id
            && !normalized_vendors
                .vendors
                .iter()
                .any(|vendor| vendor.vendor_id == *vendor_id)
        {
            order_diagnostics.push(Diagnostic::new(
                DiagnosticCode::VendorReferenceUnresolved,
                Severity::Error,
                EvidenceScope::Order,
                paths.order.clone(),
                Vec::new(),
                DiagnosticEvidence::VendorReferenceUnresolved {
                    order_id: validated.order().order_id.clone(),
                    vendor_id: vendor_id.clone(),
                },
            ));
            quarantine_order(
                paths,
                Some(validated.order().order_id.clone()),
                order_diagnostics,
                &mut damaged_orders,
                &mut diagnostic_groups,
            );
            continue;
        }
        let index_readme_text = if breadth == CanonicalInspectionBreadth::Ordinary {
            let readme = match read_readme(&paths.readme, filesystem) {
                Ok(readme) => readme,
                Err(failure) => return operation_failed(None, Some(context.clone()), failure),
            };
            if !readme.diagnostics().is_empty() {
                diagnostic_groups.push(DiagnosticGroup {
                    scope: EvidenceScope::Document,
                    location: paths.readme.location().clone(),
                    diagnostics: readme.diagnostics().to_vec(),
                });
            }
            readme.text().unwrap_or("").to_owned()
        } else {
            String::new()
        };
        healthy_orders.push(healthy_order_from_validated(
            paths,
            &validated,
            index_readme_text,
        ));
    }
    quarantine_duplicate_order_ids(
        &mut healthy_orders,
        &mut damaged_orders,
        &mut diagnostic_groups,
    );
    sort_healthy_orders(&mut healthy_orders);
    damaged_orders.sort_by(|left, right| left.paths.directory.cmp(&right.paths.directory));
    sort_diagnostic_groups(&mut diagnostic_groups);
    let package_has_errors = package_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error);
    if !package_diagnostics.is_empty() {
        normalize_diagnostics(&mut package_diagnostics);
        diagnostic_groups.push(DiagnosticGroup {
            scope: EvidenceScope::Package,
            location: context.root.clone(),
            diagnostics: package_diagnostics,
        });
        sort_diagnostic_groups(&mut diagnostic_groups);
    }

    if package_has_errors && breadth == CanonicalInspectionBreadth::Exhaustive {
        return CanonicalPackageLoad::DiagnosticOnly(DamagedPackageProjection {
            context: Some(context),
            metadata_state,
            diagnostic_groups,
            index,
        });
    }

    CanonicalPackageLoad::CurrentCandidate(current_package_candidate(
        context,
        healthy_orders,
        damaged_orders,
        normalized_vendors.vendors,
        diagnostic_groups,
        index,
    ))
}

fn orders_directory_entry_disallowed_diagnostic(
    root: &Path,
    observation: &DirectChildObservation,
    observed_kind: FileSystemEntryKind,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::OrdersDirectoryEntryDisallowed,
        Severity::Warning,
        EvidenceScope::Package,
        managed_location(
            observation.path().to_path_buf(),
            relative_path(root, observation.path()),
        ),
        Vec::new(),
        DiagnosticEvidence::OrdersDirectoryEntryDisallowed { observed_kind },
    )
}

fn healthy_order_from_validated(
    paths: OrderPaths,
    validated: &ValidatedOrder,
    index_readme_text: String,
) -> HealthyOrder {
    HealthyOrder {
        paths,
        document: validated.order().clone(),
        item_totals: validated.item_totals().to_vec(),
        order_total: validated.order_total(),
        index_readme_text,
    }
}

fn current_package_candidate(
    context: ManagedContext,
    healthy_orders: Vec<HealthyOrder>,
    damaged_orders: Vec<DamagedOrder>,
    vendors: Vec<Vendor>,
    diagnostic_groups: Vec<DiagnosticGroup>,
    index: PackageIndexRecord,
) -> CurrentPackageCandidate {
    let ordering_institutions = reusable_values(
        healthy_orders
            .iter()
            .filter_map(|order| order.document.ordering_institution.as_deref()),
    );
    let orderers = reusable_values(
        healthy_orders
            .iter()
            .filter_map(|order| order.document.orderer.as_deref()),
    );
    let projects = reusable_values(
        healthy_orders
            .iter()
            .filter_map(|order| order.document.project.as_deref()),
    );
    let order_groups = reusable_values(
        healthy_orders
            .iter()
            .filter_map(|order| order.document.order_group.as_deref()),
    );

    CurrentPackageCandidate {
        context,
        metadata_version: 1,
        metadata_state: MetadataState::Current,
        healthy_orders,
        damaged_orders,
        vendors,
        ordering_institutions,
        orderers,
        projects,
        order_groups,
        diagnostic_groups,
        index,
    }
}

fn extract_order_id(bytes: &[u8]) -> Option<OrderId> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    serde_json::from_value(value.get("order_id")?.clone()).ok()
}

fn quarantine_order(
    paths: OrderPaths,
    order_id: Option<OrderId>,
    mut diagnostics: Vec<Diagnostic>,
    damaged_orders: &mut Vec<DamagedOrder>,
    diagnostic_groups: &mut Vec<DiagnosticGroup>,
) {
    normalize_diagnostics(&mut diagnostics);
    diagnostic_groups.push(DiagnosticGroup {
        scope: EvidenceScope::Order,
        location: paths.directory.clone(),
        diagnostics,
    });
    damaged_orders.push(DamagedOrder { paths, order_id });
}

fn quarantine_duplicate_order_ids(
    healthy_orders: &mut Vec<HealthyOrder>,
    damaged_orders: &mut Vec<DamagedOrder>,
    diagnostic_groups: &mut Vec<DiagnosticGroup>,
) {
    let mut conflicts_by_id = std::collections::BTreeMap::<String, Vec<Location>>::new();
    for (order_id, location) in healthy_orders
        .iter()
        .map(|order| (&order.document.order_id, &order.paths.directory))
        .chain(damaged_orders.iter().filter_map(|order| {
            order
                .order_id
                .as_ref()
                .map(|id| (id, &order.paths.directory))
        }))
    {
        conflicts_by_id
            .entry(order_id.as_str().to_owned())
            .or_default()
            .push(location.clone());
    }
    conflicts_by_id.retain(|_, locations| locations.len() > 1);
    for locations in conflicts_by_id.values_mut() {
        locations.sort();
    }
    if conflicts_by_id.is_empty() {
        return;
    }

    let mut unique_orders = Vec::with_capacity(healthy_orders.len());
    for order in healthy_orders.drain(..) {
        let Some(_conflict_locations) = conflicts_by_id.get(order.document.order_id.as_str())
        else {
            unique_orders.push(order);
            continue;
        };
        let order_id = order.document.order_id.clone();
        quarantine_order(
            order.paths,
            Some(order_id),
            Vec::new(),
            damaged_orders,
            diagnostic_groups,
        );
    }
    *healthy_orders = unique_orders;

    for order in damaged_orders.iter() {
        let Some(order_id) = &order.order_id else {
            continue;
        };
        let Some(conflict_locations) = conflicts_by_id.get(order_id.as_str()) else {
            continue;
        };
        let group = diagnostic_groups
            .iter_mut()
            .find(|group| {
                group.scope == EvidenceScope::Order && group.location == order.paths.directory
            })
            .expect("every damaged Order has one diagnostic group");
        group.diagnostics.push(Diagnostic::new(
            DiagnosticCode::OrderIdDuplicate,
            Severity::Error,
            EvidenceScope::Order,
            order.paths.order.clone(),
            conflict_locations.clone(),
            DiagnosticEvidence::OrderIdDuplicate {
                order_id: order_id.clone(),
                conflict_locations: conflict_locations.clone(),
            },
        ));
        normalize_diagnostics(&mut group.diagnostics);
    }
}

fn layout_diagnostic<E>(root: &Path, config: &Config, error: PackagePathError<E>) -> Diagnostic {
    let config_location = managed_location(
        root.join(".spielorders/config.toml"),
        ".spielorders/config.toml",
    );
    match error {
        PackagePathError::InvalidConfiguration {
            path: ConfiguredPath::Orders,
            failure: ConfiguredPathFailure::ReservedMetadata,
        } if config.orders_path.split('/').next() != Some(".spielorders") => {
            configured_directory_diagnostic(
                root,
                config,
                ConfiguredOrdersDirectoryReason::ReservedMetadata,
            )
        }
        PackagePathError::InvalidConfiguration { path, failure } => Diagnostic::new(
            DiagnosticCode::ConfigurationPathInvalid,
            Severity::Error,
            EvidenceScope::Package,
            config_location,
            Vec::new(),
            DiagnosticEvidence::ConfigurationPathInvalid {
                path: configuration_path_kind(path),
                configured_value: match path {
                    ConfiguredPath::Orders => config.orders_path.clone(),
                    ConfiguredPath::Removed => config.removed_path.clone(),
                },
                reason: configuration_path_reason(failure),
            },
        ),
        PackagePathError::ManagedPathsOverlap => Diagnostic::new(
            DiagnosticCode::ConfigurationPathsOverlap,
            Severity::Error,
            EvidenceScope::Package,
            config_location,
            Vec::new(),
            DiagnosticEvidence::ConfigurationPathsOverlap {
                orders_path: config.orders_path.clone(),
                removed_path: config.removed_path.clone(),
            },
        ),
        PackagePathError::OrdersDirectoryNotDirectory => configured_directory_diagnostic(
            root,
            config,
            ConfiguredOrdersDirectoryReason::NotDirectory,
        ),
        PackagePathError::OrdersDirectoryOutsideRoot => configured_directory_diagnostic(
            root,
            config,
            ConfiguredOrdersDirectoryReason::OutsidePackage,
        ),
        PackagePathError::RemovedOrdersDirectoryNotOrdinary => Diagnostic::new(
            DiagnosticCode::ConfigurationPathInvalid,
            Severity::Error,
            EvidenceScope::Package,
            config_location,
            Vec::new(),
            DiagnosticEvidence::ConfigurationPathInvalid {
                path: ConfigurationPathKind::RemovedPath,
                configured_value: config.removed_path.clone(),
                reason: ConfigurationPathReason::NotOrdinary,
            },
        ),
        PackagePathError::RootNotDirectory | PackagePathError::FileSystem(_) => {
            unreachable!("filesystem and selected-root failures are mapped before diagnostics")
        }
    }
}

fn configured_directory_diagnostic(
    root: &Path,
    config: &Config,
    reason: ConfiguredOrdersDirectoryReason,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ConfiguredOrdersDirectoryUnavailable,
        Severity::Error,
        EvidenceScope::Package,
        managed_location(root.join(&config.orders_path), config.orders_path.clone()),
        Vec::new(),
        DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable {
            configured_value: config.orders_path.clone(),
            reason,
        },
    )
}

fn configuration_path_kind(path: ConfiguredPath) -> ConfigurationPathKind {
    match path {
        ConfiguredPath::Orders => ConfigurationPathKind::OrdersPath,
        ConfiguredPath::Removed => ConfigurationPathKind::RemovedPath,
    }
}

fn configuration_path_reason(reason: ConfiguredPathFailure) -> ConfigurationPathReason {
    match reason {
        ConfiguredPathFailure::Absolute => ConfigurationPathReason::Absolute,
        ConfiguredPathFailure::CurrentDirectory => ConfigurationPathReason::CurrentDirectory,
        ConfiguredPathFailure::ParentTraversal => ConfigurationPathReason::ParentTraversal,
        ConfiguredPathFailure::EmptyComponent => ConfigurationPathReason::EmptyComponent,
        ConfiguredPathFailure::PlatformSeparator => ConfigurationPathReason::PlatformSeparator,
        ConfiguredPathFailure::ReservedMetadata => ConfigurationPathReason::ReservedMetadata,
        ConfiguredPathFailure::NotDirectChild => ConfigurationPathReason::NotDirectChild,
    }
}

struct PackageOpenFileSystem<'a, F> {
    inner: &'a F,
    directory_entries: RefCell<BTreeMap<PathBuf, Vec<DirectoryEntry>>>,
}

impl<'a, F> PackageOpenFileSystem<'a, F> {
    fn new(inner: &'a F) -> Self {
        Self {
            inner,
            directory_entries: RefCell::new(BTreeMap::new()),
        }
    }
}

impl<F: FileSystem> FileSystem for PackageOpenFileSystem<'_, F> {
    type Error = F::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.inner.read(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.inner.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.inner.canonicalize(path)
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.inner.symlink_metadata(path)
    }

    fn observe_file_change(&self, path: &Path) -> Result<FileChangeObservation, Self::Error> {
        self.inner.observe_file_change(path)
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        if let Some(entries) = self.directory_entries.borrow().get(path) {
            return Ok(entries.clone());
        }
        let entries = self.inner.read_directory(path)?;
        self.directory_entries
            .borrow_mut()
            .insert(path.to_path_buf(), entries.clone());
        Ok(entries)
    }
}

struct InspectionFileSystem<'a, F> {
    inner: &'a F,
    failure: RefCell<Option<(FileSystemOperation, PathBuf)>>,
}

impl<'a, F> InspectionFileSystem<'a, F> {
    fn new(inner: &'a F) -> Self {
        Self {
            inner,
            failure: RefCell::new(None),
        }
    }

    fn record_failure<T>(
        &self,
        operation: FileSystemOperation,
        path: &Path,
        result: Result<T, F::Error>,
    ) -> Result<T, F::Error>
    where
        F: FileSystem,
    {
        if result.is_err() {
            *self.failure.borrow_mut() = Some((operation, path.to_path_buf()));
        }
        result
    }

    fn take_failure(&self, root: &Path) -> Option<OperationFailure> {
        let (operation, path) = self.failure.borrow_mut().take()?;
        let location = if path == root {
            managed_location(path, ".")
        } else if path.starts_with(root) {
            managed_location(path.clone(), relative_path(root, &path))
        } else {
            Location::absolute(path).expect("filesystem operation location is absolute")
        };
        Some(OperationFailure::filesystem(
            operation,
            location,
            Vec::new(),
        ))
    }
}

impl<F: FileSystem> FileSystem for InspectionFileSystem<'_, F> {
    type Error = F::Error;

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.record_failure(FileSystemOperation::Read, path, self.inner.read(path))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write(path, contents)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file(path)
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.inner.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.record_failure(
            FileSystemOperation::Canonicalize,
            path,
            self.inner.canonicalize(path),
        )
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.record_failure(
            FileSystemOperation::ObserveKind,
            path,
            self.inner.symlink_metadata(path),
        )
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.record_failure(
            FileSystemOperation::ListDirectory,
            path,
            self.inner.read_directory(path),
        )
    }
}

fn decode_version(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<VersionDocument> {
    if let Some(observed) = recognizable_version(bytes, "metadata_version")
        && observed != 1
    {
        diagnostics.push(Diagnostic::managed_artifact_with_details(
            DiagnosticCode::ManagedArtifactWrongVersion,
            EvidenceScope::Package,
            managed_location(path.to_path_buf(), relative_path(root, path)),
            ManagedArtifactKind::Version,
            None,
            Some("metadata_version".into()),
            Some(observed),
            Some(1),
        ));
        return None;
    }
    match VersionDocument::from_json_bytes(bytes) {
        Ok(version) => Some(version),
        Err(error) => {
            diagnostics.push(metadata_diagnostic(
                root,
                path,
                ManagedArtifactKind::Version,
                EvidenceScope::Package,
                error.kind(),
                (error.kind() == MetadataErrorKind::Shape)
                    .then(|| version_shape_field(bytes))
                    .flatten(),
            ));
            None
        }
    }
}

fn decode_vendors(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<VendorsDocument> {
    if let Some(observed) = recognizable_version(bytes, "schema_version")
        && observed != 1
    {
        diagnostics.push(Diagnostic::managed_artifact_with_details(
            DiagnosticCode::ManagedArtifactWrongVersion,
            EvidenceScope::Package,
            managed_location(path.to_path_buf(), relative_path(root, path)),
            ManagedArtifactKind::Vendors,
            None,
            Some("schema_version".into()),
            Some(observed),
            Some(1),
        ));
        return None;
    }
    match VendorsDocument::from_json_bytes(bytes) {
        Ok(vendors) => Some(vendors),
        Err(error) => {
            diagnostics.push(metadata_diagnostic(
                root,
                path,
                ManagedArtifactKind::Vendors,
                EvidenceScope::Package,
                error.kind(),
                (error.kind() == MetadataErrorKind::Shape)
                    .then(|| vendors_shape_field(bytes))
                    .flatten(),
            ));
            None
        }
    }
}

fn recognizable_version(bytes: &[u8], field: &str) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()?
        .get(field)?
        .as_u64()
}

fn metadata_state(marker_version: Option<u64>, vendors_version: Option<u64>) -> MetadataState {
    if [marker_version, vendors_version]
        .into_iter()
        .flatten()
        .any(|version| version != 1)
    {
        MetadataState::Unsupported
    } else if marker_version == Some(1) && vendors_version == Some(1) {
        MetadataState::Current
    } else {
        MetadataState::NotAssessed
    }
}

fn metadata_diagnostic(
    root: &Path,
    path: &Path,
    artifact: ManagedArtifactKind,
    scope: EvidenceScope,
    kind: MetadataErrorKind,
    field: Option<String>,
) -> Diagnostic {
    let code = match kind {
        MetadataErrorKind::Encoding => DiagnosticCode::ManagedArtifactInvalidEncoding,
        MetadataErrorKind::Syntax => DiagnosticCode::ManagedArtifactInvalidSyntax,
        MetadataErrorKind::Shape | MetadataErrorKind::Serialization => {
            DiagnosticCode::ManagedArtifactInvalidShape
        }
    };
    Diagnostic::managed_artifact_with_details(
        code,
        scope,
        managed_location(path.to_path_buf(), relative_path(root, path)),
        artifact,
        None,
        field,
        None,
        None,
    )
}

fn sort_diagnostic_groups(groups: &mut [DiagnosticGroup]) {
    for group in groups.iter_mut() {
        normalize_diagnostics(&mut group.diagnostics);
    }
    groups.sort_by(|left, right| {
        let key = |group: &DiagnosticGroup| {
            let path = group
                .location
                .package_relative_path()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    group
                        .location
                        .absolute_path()
                        .to_string_lossy()
                        .into_owned()
                });
            match group.scope {
                EvidenceScope::Package => (0, String::new(), 0),
                EvidenceScope::Document if path == "README.md" => (1, String::new(), 0),
                EvidenceScope::Order => (2, path, 0),
                EvidenceScope::Document => {
                    let order_path = path.strip_suffix("/README.md").unwrap_or(&path).to_owned();
                    (2, order_path, 1)
                }
                EvidenceScope::Operation => (3, path, 0),
            }
        };
        key(left).cmp(&key(right))
    });
}

fn normalize_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort();
    diagnostics.dedup();
}

fn order_paths(root: &Path, order_directory: &Path) -> OrderPaths {
    let directory_relative = relative_path(root, order_directory);
    let metadata = order_directory.join(".spielorders");
    let order = metadata.join("order.json");
    let readme = order_directory.join("README.md");
    let readme_location = managed_location(readme, format!("{directory_relative}/README.md"));
    OrderPaths {
        directory: managed_location(order_directory.to_path_buf(), directory_relative.clone()),
        metadata: managed_location(metadata, format!("{directory_relative}/.spielorders")),
        order: managed_location(
            order,
            format!("{directory_relative}/.spielorders/order.json"),
        ),
        readme: ReadmeReference::new(ReadmeKind::OrderReadme, readme_location)
            .expect("discovered Order README reference is exact"),
    }
}

fn sort_healthy_orders(orders: &mut [HealthyOrder]) {
    orders.sort_by(|left, right| {
        right
            .document
            .date_ordered
            .as_str()
            .cmp(left.document.date_ordered.as_str())
            .then_with(|| {
                left.document
                    .title
                    .as_str()
                    .as_bytes()
                    .cmp(right.document.title.as_str().as_bytes())
            })
            .then_with(|| {
                left.document
                    .order_id
                    .as_str()
                    .cmp(right.document.order_id.as_str())
            })
    });
}

fn reusable_values<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<ReusableValueProjection> {
    let mut projections = Vec::<ReusableValueProjection>::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(existing) = projections
            .iter_mut()
            .find(|existing| focaccia::unicode_full_case_eq(&existing.value, value))
        {
            existing.usage_count += 1;
        } else {
            projections.push(ReusableValueProjection {
                value: value.to_owned(),
                usage_count: 1,
            });
        }
    }
    projections.sort_by(|left, right| left.value.as_bytes().cmp(right.value.as_bytes()));
    projections
}

fn required_artifact<F: FileSystem>(
    root: &Path,
    path: &Path,
    artifact: ManagedArtifactKind,
    scope: EvidenceScope,
    filesystem: &F,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Vec<u8>>, OperationFailure> {
    let location = managed_location(path.to_path_buf(), relative_path(root, path));
    match filesystem.symlink_metadata(path) {
        Ok(None) => {
            diagnostics.push(Diagnostic::managed_artifact(
                DiagnosticCode::ManagedArtifactMissing,
                scope,
                location,
                artifact,
                None,
            ));
            Ok(None)
        }
        Ok(Some(FileSystemEntryKind::File)) => match filesystem.read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(_) => {
                diagnostics.push(Diagnostic::managed_artifact(
                    DiagnosticCode::ManagedArtifactUnreadable,
                    scope,
                    location,
                    artifact,
                    None,
                ));
                Ok(None)
            }
        },
        Ok(Some(kind)) => {
            diagnostics.push(Diagnostic::managed_artifact(
                DiagnosticCode::ManagedArtifactWrongKind,
                scope,
                location,
                artifact,
                Some(kind),
            ));
            Ok(None)
        }
        Err(_) => Err(OperationFailure::filesystem(
            FileSystemOperation::ObserveKind,
            location,
            Vec::new(),
        )),
    }
}

fn damaged_without_context(
    root: &Path,
    metadata_state: MetadataState,
    mut diagnostics: Vec<Diagnostic>,
) -> CanonicalPackageLoad {
    normalize_diagnostics(&mut diagnostics);
    let index_location = managed_location(
        root.join(".spielorders/index.json"),
        ".spielorders/index.json",
    );
    CanonicalPackageLoad::DiagnosticOnly(DamagedPackageProjection {
        context: Some(managed_root_context(root)),
        metadata_state,
        diagnostic_groups: vec![DiagnosticGroup {
            scope: EvidenceScope::Package,
            location: managed_location(root.to_path_buf(), "."),
            diagnostics,
        }],
        index: PackageIndexRecord {
            location: index_location,
            state: PackageIndexState::NotAssessed,
            refresh: PackageIndexRefresh::NotAttempted,
            malformed_reasons: Vec::new(),
            stale_reasons: Vec::new(),
            observed_version: None,
            expected_version: None,
        },
    })
}

fn managed_context(root: &Path, orders: &Path, removed: &Path) -> ManagedContext {
    let mut context = managed_root_context(root);
    context.orders_directory = Some(managed_location(
        orders.to_path_buf(),
        relative_path(root, orders),
    ));
    context.removed_orders_directory = Some(managed_location(
        removed.to_path_buf(),
        relative_path(root, removed),
    ));
    context
}

fn managed_root_context(root: &Path) -> ManagedContext {
    let package_readme_location = managed_location(root.join("README.md"), "README.md");
    ManagedContext {
        root: managed_location(root.to_path_buf(), "."),
        metadata_directory: managed_location(root.join(".spielorders"), ".spielorders"),
        config: managed_location(
            root.join(".spielorders/config.toml"),
            ".spielorders/config.toml",
        ),
        version: managed_location(
            root.join(".spielorders/version.json"),
            ".spielorders/version.json",
        ),
        vendors: managed_location(
            root.join(".spielorders/vendors.json"),
            ".spielorders/vendors.json",
        ),
        index: managed_location(
            root.join(".spielorders/index.json"),
            ".spielorders/index.json",
        ),
        orders_directory: None,
        removed_orders_directory: None,
        package_readme: ReadmeReference::new(ReadmeKind::PackageReadme, package_readme_location)
            .expect("Package README reference is exact"),
    }
}

fn managed_location(path: PathBuf, relative: impl Into<String>) -> Location {
    Location::within_package(path, relative).expect("managed location is normalized")
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("managed path is inside root")
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .expect("managed version-1 paths are UTF-8")
        })
        .collect::<Vec<_>>()
        .join("/")
}
