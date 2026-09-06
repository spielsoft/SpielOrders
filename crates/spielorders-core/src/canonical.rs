//! Canonical Package classification, projection, ordinary Package Index refresh, and exhaustive
//! managed-evidence validation.

use crate::{
    digest::ContentDigest,
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, ValidatedReplacementFailure,
        replace_validated_file_expecting, staging_path,
    },
    metadata::{
        Config, IndexDocument, IndexOrder, IndexOrderDirectory, MetadataErrorKind, OrderDocument,
        OrderId, PackageRoot, PackageRootLocator, Vendor, VendorsDocument, Version1,
        VersionDocument, config_shape_field, order_shape_field, vendors_shape_field,
        version_shape_field,
    },
    mutation::MutationReport,
    operations::{FailedPath, FileSystemOperation, OperationFailure, PackageIndexReplacementPhase},
    order::{OrderValidationFailure, ValidatedOrder},
    package::{PackageMetadata, PackageMetadataValidationFailure, index_semantics_are_valid},
    paths::{
        ConfiguredPath, ConfiguredPathFailure, DirectChildClassification, DirectChildObservation,
        PackagePathError, discover_order_direct_children, locator_resolves_to_root,
        resolve_package_layout,
    },
    ports::{
        DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
    },
    readme::{
        ConfigurationPathKind, ConfigurationPathReason, ConfiguredOrdersDirectoryReason,
        Diagnostic, DiagnosticCode, DiagnosticEvidence, EvidenceScope, Location,
        ManagedArtifactKind, OrderLocatorReason, OrderSemanticReason, ReadmeKind, ReadmeRead,
        ReadmeReference, Severity, package_relative_path_text, read_readme,
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

    pub(crate) fn after_committed_membership_change_without_refresh(&self) -> Self {
        self.after_committed_change_without_refresh(PackageIndexStaleReason::OrderMembershipChanged)
    }

    pub(crate) fn after_committed_vendor_change_without_refresh(&self) -> Self {
        self.after_committed_change_without_refresh(PackageIndexStaleReason::VendorsChanged)
    }

    pub(crate) fn after_committed_order_metadata_change_without_refresh(&self) -> Self {
        self.after_committed_change_without_refresh(PackageIndexStaleReason::OrderMetadataChanged)
    }

    pub(crate) fn after_committed_order_readme_change_without_refresh(&self) -> Self {
        self.after_committed_change_without_refresh(PackageIndexStaleReason::OrderReadmeChanged)
    }

    fn after_committed_change_without_refresh(&self, reason: PackageIndexStaleReason) -> Self {
        let mut record = self.clone();
        record.state = PackageIndexState::Stale;
        record.refresh = PackageIndexRefresh::NotAttempted;
        record.malformed_reasons.clear();
        record.stale_reasons = vec![reason];
        record.observed_version = None;
        record.expected_version = None;
        record
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
    readme_text: Option<String>,
}

impl HealthyOrder {
    pub fn paths(&self) -> &OrderPaths {
        &self.paths
    }

    pub fn document(&self) -> &OrderDocument {
        &self.document
    }

    /// This Order's technical locator, which a healthy Order always carries.
    ///
    /// Not an `Option`, and not re-derived from the record by a caller: the record's own
    /// field carries whether the locator parsed at all — see [`PackageRootLocator`] — and
    /// being healthy is precisely the load having followed that locator from this
    /// Directory to this Root. A caller reporting the locator therefore has one to report
    /// without deciding what to do when there is none.
    pub fn package_root(&self) -> &PackageRoot {
        self.document
            .package_root
            .spelled()
            .expect("a healthy Order is one whose locator was followed to its Package")
    }

    pub fn item_totals(&self) -> &[f64] {
        &self.item_totals
    }

    pub fn order_total(&self) -> f64 {
        self.order_total
    }

    /// The Order README text this projection carries, or `None` when it carries none.
    ///
    /// An absent README reads as `Some("")`, because absence is a text: there is
    /// nothing written. `None` says the opposite — this projection does not know what
    /// the document holds, because it exists and could not be read, is not valid
    /// UTF-8, or is not a file at all; the load's document diagnostics say which.
    /// Collapsing the two would report a file that holds bytes as one a reader could
    /// safely treat as blank.
    ///
    /// **Every capability that reports an Order carries this distinction unchanged.**
    /// None of them writes the Order README, so each reports what the load knew — and
    /// `None` when the load knew nothing. Their own comments say why the load's
    /// knowledge is still current for that operation, not what `None` means.
    pub fn readme_text(&self) -> Option<&str> {
        self.readme_text.as_deref()
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

    /// Every Vendor in the Package, Hidden Vendors included, each carrying its own
    /// visibility. A caller that needs one either way — `package show`, the Vendors
    /// table and its show/hide control — reads this rather than a filtered list.
    pub fn vendors(&self) -> &[Vendor] {
        &self.vendors
    }

    /// The Vendors this Package offers for new use. A Hidden Vendor is excluded here
    /// and still resolves through `vendors` and `vendor_for_order`, because hiding
    /// governs what is offered and never what is resolved.
    pub fn offered_vendors(&self) -> Vec<&Vendor> {
        self.vendors
            .iter()
            .filter(|vendor| !vendor.is_hidden())
            .collect()
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
    examined_orders: Vec<OrderPaths>,
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

    /// Every Order Directory this run examined, healthy or damaged, in one
    /// deterministic order.
    ///
    /// **Reported rather than left to be derived from the READMEs beside it.** A caller
    /// that took each README's parent directory would be a second place deciding where an
    /// Order keeps its conventional document — and without this, a validation with no
    /// findings cannot be told from one that examined nothing.
    pub fn examined_orders(&self) -> &[OrderPaths] {
        &self.examined_orders
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
        examined_orders: order_paths,
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
        // An entry this application cannot name is refused here for the same
        // reason a name that is not Unicode already was: every diagnostic below
        // needs a Package-relative location for it, and there is none to give.
        // Both are one condition, so they get one answer.
        let named = entry.name().to_str().and_then(|name| {
            let path = specification.directory.absolute_path().join(name);
            let relative = package_relative_path_text(root, &path)?;
            Some((name.to_owned(), path, relative))
        });
        let Some((name, path, relative)) = named else {
            return Err(OperationFailure::filesystem(
                FileSystemOperation::ListDirectory,
                specification.directory.clone(),
                Vec::new(),
            ));
        };
        let name = name.as_str();
        if specification.expected.contains(&name) {
            continue;
        }
        let primary = managed_location(path, relative);
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
            // Which candidate is staged here, so a confirmation to promote it stops
            // authorizing anything once a different candidate takes its place. Only an
            // entry the listing reported as a regular file is read: a directory has no
            // content to promote, and a link is not this application's to follow.
            // Neither yields identifying evidence, and neither is given a stand-in.
            let candidate_digest = match entry.kind() {
                FileSystemEntryKind::File => filesystem
                    .read(primary.absolute_path())
                    .ok()
                    .map(|bytes| ContentDigest::of(&bytes)),
                _ => None,
            };
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::StagingArtifactPresent,
                Severity::Warning,
                specification.scope,
                primary,
                vec![destination],
                DiagnosticEvidence::StagingArtifactPresent {
                    destination_valid,
                    candidate_digest,
                },
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

/// No candidate validation failure is possible when rebuilding the derived index, and
/// this states it in a form the compiler checks.
///
/// The rebuild's candidate is `index_document(&package)` serialized, built from a
/// `CurrentPackageCandidate` whose Orders were validated and normalized at load, whose
/// duplicate ids were quarantined before sorting, and whose Vendors validated as an
/// aggregate. `sort_healthy_orders` orders by exactly the rule `compare_index_orders`
/// validates by. Nothing on disk reaches the candidate, so there was never a condition
/// for a guard here to refuse.
///
/// The index still gets both checks — semantics and canonical serialization — at load,
/// in `assess_index`. That is where a *stored* index can be wrong, and it is the
/// deliberate exception a cache this application owns keeps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndexCandidateFailure {}

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
            let rebuilt = index_document(&package);
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
            // Whether this load observed nothing at the index path, taken from the
            // assessment before the bookkeeping below rewrites the state it recorded.
            // Carrying no bytes is not the same question: an index that is a directory,
            // or a file whose read failed, carries none either and is plainly there.
            let observed_no_index = assessment.record.state == PackageIndexState::Missing;
            package.index = assessment.record;
            if package.index.state == PackageIndexState::NotAssessed {
                package.index.stale_reasons = merged_stale_reasons(
                    stale_index_reasons(
                        assessment
                            .document
                            .as_ref()
                            .expect("a valid deterministic index remains available"),
                        &package,
                    ),
                    fast_stale_reasons,
                );
                package.index.state = PackageIndexState::Stale;
            }
            // A load that could not read an Order README has nothing to cache for it,
            // so it caches nothing: the stored index keeps whatever a load that could
            // read it left there, and its assessment stands as found. The alternative
            // is writing an empty README into the durable record and then, on the next
            // load, proving that record current against change evidence — which is how
            // a momentary read failure became a permanent silent one.
            let Some(index) = rebuilt else {
                package.index.refresh = PackageIndexRefresh::NotAttempted;
                return PackageLoad::Available(package);
            };
            let candidate = index
                .to_json_bytes()
                .expect("validated canonical facts serialize as metadata version 1");
            // The index as this load assessed it. Rebuilding over a different one
            // would discard a concurrent writer's index; refusing lets the caller
            // reload instead. Observing no index is an observation like any other, so
            // an index appearing at the path since is a changed destination — checked
            // by observing the path, because there are no bytes to compare.
            //
            // An index that is there but could not be read as bytes is neither
            // observation. This contract can say "these bytes" and "nothing", not
            // "something I could not read", so that case carries no evidence, exactly
            // as it did before absence was distinguished from it. Claiming absence
            // instead would refuse the rebuild that heals it, permanently, and blame a
            // concurrent writer for it. Give single-file replacements destination
            // evidence names this as the one remaining unevidenced index write.
            let replacement = replace_validated_file_expecting(
                &index_path,
                &candidate,
                index_expectation(old_index_bytes.as_deref(), observed_no_index),
                filesystem,
                |_| Ok::<(), IndexCandidateFailure>(()),
            );
            if let Err(error) = replacement {
                package.index.refresh = PackageIndexRefresh::ReplacementFailed;
                let failure = index_replacement_failure(&package.context, &error);
                return PackageLoad::IndexReplacementFailed(IndexReplacementFailedProjection {
                    canonical_evidence: package,
                    old_index_bytes,
                    failure,
                });
            }
            mark_index_rebuilt(&mut package.index);
            PackageLoad::Available(package)
        }
    }
}

/// The destination evidence a rebuild carries for the index it assessed.
///
/// Rebuilding over a different index would discard a concurrent writer's work; refusing
/// lets the caller reload instead. Observing no index is an observation like any other,
/// so an index appearing at the path since is a changed destination — checked by
/// observing the path, because there are no bytes to compare.
///
/// An index that is there and could not be read as bytes is neither observation. This
/// contract can say "these bytes" and "nothing", not "something I could not read", so
/// that case carries no evidence. Claiming absence instead would refuse the rebuild that
/// heals it, permanently, and blame a concurrent writer for it. Give single-file
/// replacements destination evidence names this as the one remaining unevidenced index
/// write.
fn index_expectation(
    stored_bytes: Option<&[u8]>,
    observed_absent: bool,
) -> DestinationExpectation<'_> {
    match stored_bytes {
        Some(bytes) => DestinationExpectation::Exactly(bytes),
        None if observed_absent => DestinationExpectation::Absent,
        None => DestinationExpectation::Unchecked,
    }
}

/// One failed index replacement, as the typed operational failure it is.
///
/// Shared by the refresh that follows a canonical write and by a deliberate rebuild,
/// because a rename that would not happen is the same failure however the rebuild was
/// asked for — and two mappings would let one of them name a phase the other does not.
fn index_replacement_failure<E>(
    context: &ManagedContext,
    error: &ValidatedReplacementError<E, IndexCandidateFailure>,
) -> OperationFailure {
    let phase = match error.failure() {
        ValidatedReplacementFailure::StagingWrite(_) => PackageIndexReplacementPhase::WriteStage,
        // Forced to stay because `error.failure()` hands back a reference, and rustc
        // wants a pattern for an uninhabited type matched behind one. Discharged by an
        // empty match on the dereferenced value, which the compiler checks and which
        // produces no phase — so the rebuild has no spelled failure code that nothing
        // can emit.
        ValidatedReplacementFailure::CandidateValidation(impossible) => match *impossible {},
        ValidatedReplacementFailure::Durability(_) => PackageIndexReplacementPhase::SyncStage,
        ValidatedReplacementFailure::Replacement(_) => PackageIndexReplacementPhase::Replace,
        // The rebuild carries the index it assessed, so it can be refused. A caller that
        // cannot tell a concurrent writer from a failed rename would retry the rebuild
        // and discard their index, which is the whole reason the evidence is carried.
        ValidatedReplacementFailure::DestinationChanged => {
            PackageIndexReplacementPhase::DestinationChanged
        }
        ValidatedReplacementFailure::DestinationUnreadable(_) => {
            PackageIndexReplacementPhase::DestinationUnreadable
        }
        ValidatedReplacementFailure::InvalidDestination => PackageIndexReplacementPhase::WriteStage,
    };
    let cleanup_failure = error
        .cleanup_failure()
        .map(|_| PackageIndexReplacementPhase::RemoveStage);
    let staging = staging_path(context.index.absolute_path())
        .expect("the established Package Index has a staging path");
    let staging_relative = relative_path(context.root.absolute_path(), &staging);
    OperationFailure::package_index_replacement(
        context.index.clone(),
        vec![managed_location(staging, staging_relative)],
        phase,
        cleanup_failure,
    )
}

/// The index record a completed rebuild leaves: current, rebuilt, and carrying none of
/// the reasons it was rebuilt for.
fn mark_index_rebuilt(record: &mut PackageIndexRecord) {
    record.state = PackageIndexState::Current;
    record.refresh = PackageIndexRefresh::Rebuilt;
    record.malformed_reasons.clear();
    record.stale_reasons.clear();
    record.observed_version = None;
    record.expected_version = None;
}

/// The one managed operation a deliberate Package Index rebuild performs.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageIndexRebuildOperationKind {
    RefreshPackageIndex,
}

impl PackageIndexRebuildOperationKind {
    /// The same spelling every capability that refreshes the derived index reports — see
    /// [`crate::order_edits::OrderEditOperationKind::as_str`].
    ///
    /// **One act, one name.** A deliberate rebuild replaces exactly the file the refresh
    /// after a canonical write replaces, so a caller reading managed operations across
    /// commands sees one operation rather than two that happen to do the same thing.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageIndexRebuildOperation {
    kind: PackageIndexRebuildOperationKind,
    location: Location,
}

impl PackageIndexRebuildOperation {
    pub fn kind(&self) -> PackageIndexRebuildOperationKind {
        self.kind
    }

    /// Where this operation acts, as this Package locates it — so a caller reporting it
    /// carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }
}

/// What a deliberate Package Index rebuild would write, and the state it found.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageIndexRebuildPlan {
    package: CurrentPackageCandidate,
    candidate: Vec<u8>,
    stored_bytes: Option<Vec<u8>>,
    observed_absent: bool,
    indexed_orders: Vec<Location>,
    operations: Vec<PackageIndexRebuildOperation>,
}

impl PackageIndexRebuildPlan {
    /// The Package this rebuild is of: its managed paths, its metadata version and
    /// state, the index record carrying the state this plan found, and the diagnostic
    /// groups carrying what the load observed on the way.
    pub fn package(&self) -> &CurrentPackageCandidate {
        &self.package
    }

    /// The bytes the rebuild would write.
    ///
    /// Deterministic: the projection is sorted by the rule the stored index is validated
    /// against, and every fact in it comes from a record the load validated and
    /// normalized — so the same healthy Package projects the same bytes.
    pub fn candidate(&self) -> &[u8] {
        &self.candidate
    }

    /// Whether those bytes differ from the index presently stored.
    ///
    /// A rebuild replaces the file either way, because replacing it is what makes the
    /// derived index provably current again; this is what says whether the projection
    /// itself moved.
    pub fn changed(&self) -> bool {
        self.stored_bytes.as_deref() != Some(self.candidate.as_slice())
    }

    /// The Order Directories the rebuilt index carries, in the projection's own order.
    ///
    /// A Damaged Order is absent — its record could not be read, so it has nothing to
    /// project — and stays visible through `package().diagnostic_groups()`.
    pub fn indexed_orders(&self) -> &[Location] {
        &self.indexed_orders
    }

    /// The managed operations committing this plan performs, in the vocabulary every
    /// mutating capability reports them in. Exactly one: the derived index is the only
    /// file a rebuild touches.
    pub fn operations(&self) -> &[PackageIndexRebuildOperation] {
        &self.operations
    }
}

/// Why no rebuild could be planned.
///
/// The four Package-load states travel as the core's own projections, so a caller
/// classifies a Package exactly as any other capability does. The fifth is this
/// capability's own.
#[derive(Clone, Debug, PartialEq)]
pub enum PackageIndexRebuildPlanError {
    Unmanaged(Box<UnmanagedPackage>),
    DamagedPackage(Box<DamagedPackageProjection>),
    UpgradePending(Box<UpgradeProjection>),
    OperationFailed(Box<OperationFailedProjection>),
    /// An Order README exists and its text was not established, so a version-1 index
    /// entry has nothing truthful to record for it — see [`index_document`]. The Package
    /// the refusal is about travels with it, carrying the observation behind it.
    OrderReadmeTextUnavailable(Box<CurrentPackageCandidate>),
}

/// A rebuild has no committed-with-warnings outcome, and this states it in a form the
/// compiler checks.
///
/// The warning other mutations carry is that their canonical write succeeded and the
/// derived index refresh did not — and the derived index is this command's whole subject,
/// so a rebuild that could not replace it wrote nothing and is a refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageIndexRebuildWarning {}

#[derive(Clone, Debug, PartialEq)]
pub enum PackageIndexRebuildRejection {
    /// The replacement did not happen. The stored index holds what it held.
    ReplacementFailed { failure: OperationFailure },
}

pub type PackageIndexRebuildReport = MutationReport<
    PackageIndexRebuildPlan,
    CurrentPackageCandidate,
    PackageIndexRebuildRejection,
    PackageIndexRebuildWarning,
>;

/// Plans a deliberate replacement of the derived Package Index from healthy canonical
/// records.
///
/// **The canonical records are the source, never the stored index.** The current-index
/// path projects a Package *out of* the index, so a rebuild planned from it would rebuild
/// the index from itself and could never correct one — which is why this takes the
/// canonical load and asks the freshness question only for the state it reports.
///
/// Nothing here writes. [`commit_package_index_rebuild`] is what replaces the file, and a
/// plan that is never committed is the dry run.
pub fn plan_package_index_rebuild<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<PackageIndexRebuildPlan, PackageIndexRebuildPlanError> {
    let filesystem = PackageOpenFileSystem::new(filesystem);
    let package = match inspect_canonical_package(selected_root, &filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        CanonicalPackageLoad::Unmanaged(package) => {
            return Err(PackageIndexRebuildPlanError::Unmanaged(Box::new(package)));
        }
        // A Package whose canonical state cannot be trusted has no healthy records to
        // project, and the index is never evidence for what they should have been. The
        // index's own validity is still assessed, because that is what the refusal is
        // about the index of.
        CanonicalPackageLoad::DiagnosticOnly(mut package) => {
            package.index = assess_index_without_freshness(package.index, &filesystem);
            return Err(PackageIndexRebuildPlanError::DamagedPackage(Box::new(
                package,
            )));
        }
        CanonicalPackageLoad::UpgradePending(mut package) => {
            package.index = assess_index_without_freshness(package.index, &filesystem);
            return Err(PackageIndexRebuildPlanError::UpgradePending(Box::new(
                package,
            )));
        }
        CanonicalPackageLoad::OperationFailed(package) => {
            return Err(PackageIndexRebuildPlanError::OperationFailed(Box::new(
                package,
            )));
        }
    };
    let context = package.context.clone();
    let StoredIndexAssessment {
        package,
        stored_bytes,
        observed_absent,
    } = assess_stored_index(selected_root, package, &filesystem).map_err(|failure| {
        PackageIndexRebuildPlanError::OperationFailed(Box::new(OperationFailedProjection {
            root: None,
            context: Some(context),
            failure,
        }))
    })?;
    let Some(document) = index_document(&package) else {
        return Err(PackageIndexRebuildPlanError::OrderReadmeTextUnavailable(
            Box::new(package),
        ));
    };
    // The healthy Orders in the order the load sorted them into, which is the order the
    // projection carries and the one a stored index is validated against.
    let indexed_orders = package
        .healthy_orders
        .iter()
        .map(|order| order.paths.directory.clone())
        .collect();
    let candidate = document
        .to_json_bytes()
        .expect("validated canonical facts serialize as metadata version 1");
    let operations = vec![PackageIndexRebuildOperation {
        kind: PackageIndexRebuildOperationKind::RefreshPackageIndex,
        location: package.context.index.clone(),
    }];
    Ok(PackageIndexRebuildPlan {
        package,
        candidate,
        stored_bytes,
        observed_absent,
        indexed_orders,
        operations,
    })
}

/// Replaces the derived Package Index with the planned projection.
///
/// The replacement is the established beside-destination staging, durability, destination
/// comparison, and atomic rename, against the index this plan assessed — so a writer that
/// landed since is refused rather than overwritten. A refusal leaves the stored index
/// exactly as it was.
pub fn commit_package_index_rebuild<F: FileSystem>(
    mut plan: PackageIndexRebuildPlan,
    filesystem: &F,
) -> PackageIndexRebuildReport {
    let index_path = plan.package.context.index.absolute_path().to_path_buf();
    let replacement = replace_validated_file_expecting(
        &index_path,
        &plan.candidate,
        index_expectation(plan.stored_bytes.as_deref(), plan.observed_absent),
        filesystem,
        |_| Ok::<(), IndexCandidateFailure>(()),
    );
    match replacement {
        Err(error) => {
            let failure = index_replacement_failure(&plan.package.context, &error);
            plan.package.index.refresh = PackageIndexRefresh::ReplacementFailed;
            MutationReport::Rejected {
                plan,
                rejection: PackageIndexRebuildRejection::ReplacementFailed { failure },
            }
        }
        Ok(()) => {
            let mut result = plan.package.clone();
            mark_index_rebuilt(&mut result.index);
            MutationReport::Committed { plan, result }
        }
    }
}

/// Every reason one stored index is stale: what comparing it with the canonical records
/// found, refined by what the change evidence proved.
///
/// **The evidence refines the comparison rather than replacing it.** Comparing content
/// cannot prove that no input has been rewritten since the index was, so the canonical
/// comparison always ends in `ChangeEvidenceUncertain`; where the evidence *is*
/// comparable it names which input changed, and the uncertainty is no longer part of the
/// answer. One home for that rule, because the opening load and the assessment that
/// writes nothing owe the same reasons about the same Package.
fn merged_stale_reasons(
    mut compared: Vec<PackageIndexStaleReason>,
    evidence: Vec<PackageIndexStaleReason>,
) -> Vec<PackageIndexStaleReason> {
    if evidence.is_empty() {
        return compared;
    }
    if !evidence.contains(&PackageIndexStaleReason::ChangeEvidenceUncertain) {
        compared.retain(|reason| *reason != PackageIndexStaleReason::ChangeEvidenceUncertain);
    }
    compared.extend(evidence);
    compared.sort();
    compared.dedup();
    compared
}

/// One assessment of the stored derived Package Index, having written nothing.
struct StoredIndexAssessment {
    /// The canonical Package, its index record carrying the assessed state.
    package: CurrentPackageCandidate,
    /// The stored index bytes this assessment read, where it read any.
    stored_bytes: Option<Vec<u8>>,
    /// Whether it observed nothing at all at the index path. Not the same question as
    /// carrying no bytes: an index that is a directory, or a file whose read failed,
    /// carries none either and is plainly there.
    observed_absent: bool,
}

/// Assesses the stored index of a Package whose canonical records have been loaded.
///
/// **Nothing here writes.** The opening load answers the same question and then durably
/// replaces the index, which `docs/architecture/durable-writes.md` reserves for a place a
/// canonical write has already happened — so a caller that only wants to know what state
/// the index is in has no way to ask through it. This is that way, and it is shared by
/// the assessment capability and by a deliberate rebuild's plan so the two cannot come to
/// disagree about one Package's index.
fn assess_stored_index<F: FileSystem>(
    selected_root: &Path,
    mut package: CurrentPackageCandidate,
    filesystem: &F,
) -> Result<StoredIndexAssessment, OperationFailure> {
    let assessment = assess_index(package.index.clone(), filesystem)?;
    let observed_absent = assessment.record.state == PackageIndexState::Missing;
    let stored_bytes = assessment.bytes;
    package.index = assessment.record;
    if package.index.state == PackageIndexState::NotAssessed {
        let compared = stale_index_reasons(
            assessment
                .document
                .as_ref()
                .expect("a valid deterministic index remains available"),
            &package,
        );
        // The stored index is a valid, deterministic projection; whether it is *current*
        // is the freshness question, and the current-index path is the one place that
        // answers it — it proves every canonical input older than the index rather than
        // comparing content, which content comparison cannot do.
        let mut evidence = Vec::new();
        if try_open_current_index(selected_root, filesystem, &mut evidence).is_some() {
            package.index.state = PackageIndexState::Current;
            package.index.refresh = PackageIndexRefresh::NotNeeded;
        } else {
            package.index.stale_reasons = merged_stale_reasons(compared, evidence);
            package.index.state = PackageIndexState::Stale;
        }
    }
    Ok(StoredIndexAssessment {
        package,
        stored_bytes,
        observed_absent,
    })
}

/// Reports what state the derived Package Index is in, without writing anything.
///
/// The reading load leaves the index `not_assessed` and the opening load rebuilds it, so
/// before this there was no way to ask the question without answering it by writing. The
/// answer is [`PackageLoad`] like any other load's, and the record under
/// [`CurrentPackageCandidate::index`] is the assessment: `current`, `missing`,
/// `malformed`, `wrong_version`, or `stale`, each with its own typed reasons.
///
/// [`PackageLoad::IndexReplacementFailed`] is unreachable from here, because nothing is
/// replaced.
pub fn assess_package_index<F: FileSystem>(selected_root: &Path, filesystem: &F) -> PackageLoad {
    let filesystem = PackageOpenFileSystem::new(filesystem);
    match inspect_canonical_package(selected_root, &filesystem) {
        CanonicalPackageLoad::Unmanaged(package) => PackageLoad::Unmanaged(package),
        // A Package whose canonical state cannot be trusted has no records to compare the
        // index against, so the freshness half of the question is unanswerable and only
        // the index's own validity is reported.
        CanonicalPackageLoad::DiagnosticOnly(mut package) => {
            package.index = assess_index_without_freshness(package.index, &filesystem);
            PackageLoad::DiagnosticOnly(package)
        }
        CanonicalPackageLoad::UpgradePending(mut package) => {
            package.index = assess_index_without_freshness(package.index, &filesystem);
            PackageLoad::UpgradePending(package)
        }
        CanonicalPackageLoad::OperationFailed(package) => PackageLoad::OperationFailed(package),
        CanonicalPackageLoad::CurrentCandidate(package) => {
            let context = package.context.clone();
            match assess_stored_index(selected_root, package, &filesystem) {
                Ok(assessment) => PackageLoad::Available(assessment.package),
                Err(failure) => PackageLoad::OperationFailed(OperationFailedProjection {
                    root: None,
                    context: Some(context),
                    failure,
                }),
            }
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
        .filter_map(|observation| unusable_direct_child_diagnostic(&root, observation))
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
                // A stored entry is a text some load read from that README, and this
                // path proved nothing has replaced it since. Only the rebuild can put
                // a text here, and it refuses to invent one.
                Some(entry.readme.clone()),
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

/// The index this Package's canonical facts would be cached as, or `None` when one
/// of them is not known.
///
/// A version-1 entry must carry a `readme` string, and there is no spelling in that
/// closed schema for "the README exists and this load could not read it". Writing
/// the empty string there would record a document that holds bytes as one that holds
/// none, in the durable file a later load proves itself current against. Returning
/// `None` instead is what makes the refusal at the single rebuild site unmissable.
fn index_document(package: &CurrentPackageCandidate) -> Option<IndexDocument> {
    Some(IndexDocument {
        schema_version: Version1,
        orders: package
            .healthy_orders
            .iter()
            .map(|order| {
                Some(IndexOrder {
                    order_directory: IndexOrderDirectory::new(
                        order
                            .paths
                            .directory
                            .package_relative_path()
                            .expect("healthy Order paths are established"),
                    )
                    .expect("healthy Order paths are valid index paths"),
                    order: order.document.clone(),
                    readme: order.readme_text.clone()?,
                })
            })
            .collect::<Option<Vec<_>>>()?,
        vendors: package.vendors.clone(),
    })
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
            .is_some_and(|entry| {
                // Only a text this load actually read can show the stored one is out
                // of date. A README this load could not read is not evidence that it
                // changed, and reporting it as changed would name the wrong reason
                // for an index this load is about to decline to rewrite anyway.
                order
                    .readme_text
                    .as_deref()
                    .is_some_and(|text| entry.readme != text)
            })
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
            // The failure is the resolution, so the root is whatever the caller named
            // and nothing more. A relative one has no `Location` at all; reporting it
            // as the path it arrived as is the whole of what this load honestly knows.
            let selected = FailedPath::of(selected_root);
            return operation_failed(
                selected.location().cloned(),
                None,
                OperationFailure::filesystem(
                    FileSystemOperation::Canonicalize,
                    selected,
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
        if observation.classification() != DirectChildClassification::OrderCandidate {
            package_diagnostics.extend(unusable_direct_child_diagnostic(&root, observation));
            continue;
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
            Err(failures) => {
                let (field, reason) = semantic_evidence(&failures);
                order_diagnostics.push(Diagnostic::new(
                    DiagnosticCode::OrderSemanticInvalid,
                    Severity::Error,
                    EvidenceScope::Order,
                    paths.order.clone(),
                    Vec::new(),
                    DiagnosticEvidence::OrderSemanticInvalid {
                        order_id: order.order_id.clone(),
                        field: field.into(),
                        reason,
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
        // The four damages the integrity decision calls repairable by recalculation, asked
        // in one place so that all four earn the one diagnosis that offers that repair. A
        // locator that is not there and one that is not a locator reach it here rather
        // than failing the whole record: see `PackageRootLocator`.
        let locator_reason = if resolved_order_directory.parent() != Some(layout.orders_directory())
        {
            Some(OrderLocatorReason::NotActiveDirectChild)
        } else {
            match &validated.order().package_root {
                PackageRootLocator::Absent => Some(OrderLocatorReason::Absent),
                PackageRootLocator::Malformed(_) => Some(OrderLocatorReason::Malformed),
                PackageRootLocator::Spelled(locator) => {
                    (!locator_resolves_to_root(&resolved_order_directory, locator, &root))
                        .then_some(OrderLocatorReason::DoesNotResolveToPackage)
                }
            }
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
                    package_root: validated
                        .order()
                        .package_root
                        .stored_text()
                        .map(str::to_owned),
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
        let readme_text = if breadth == CanonicalInspectionBreadth::Ordinary {
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
            // `read_readme` already separates absence, which is an empty text, from
            // a document that exists and could not be read, which is no text at all.
            // The projection carries that separation instead of flattening it.
            readme.text().map(str::to_owned)
        } else {
            // The exhaustive breadth reads and reports every README on its own and
            // discards these Orders, so this loop consults none and claims none.
            None
        };
        healthy_orders.push(healthy_order_from_validated(paths, &validated, readme_text));
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

/// Which field and which reason one Order-local semantic failure reports.
///
/// **The failure decides both**, which is the whole reason this exists: the site that
/// raises the diagnostic used to spell `date_ordered` and the one date reason as
/// literals, because a date was the only failure an Order could have. A second failure
/// means the diagnostic has to carry which one occurred, and matching on the core's own
/// [`OrderValidationFailure`] is what makes a third a compile error here rather than a
/// diagnostic naming the wrong field.
///
/// `validate` returns on the first failure it finds, so the first is the whole answer;
/// the empty case cannot arise, and reporting the date reason for it would be a claim
/// about a record nothing objected to.
fn semantic_evidence(failures: &[OrderValidationFailure]) -> (&'static str, OrderSemanticReason) {
    match failures.first() {
        Some(OrderValidationFailure::NonFiniteTotal { total }) => {
            (total.as_str(), OrderSemanticReason::NonFiniteTotal)
        }
        Some(OrderValidationFailure::InvalidDateOrdered) | None => {
            ("date_ordered", OrderSemanticReason::InvalidGregorianDate)
        }
    }
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

/// Names one direct child of the Orders Directory the application cannot express
/// as a Package-relative path, so that a reader learns which entry was skipped
/// and why rather than losing the whole Package to an assertion.
///
/// The location is absolute only. Every other managed location also carries a
/// Package-relative path, and this one cannot: having none is the very condition
/// being reported, and fabricating one would name a different entry.
fn orders_directory_entry_name_unusable_diagnostic(
    observation: &DirectChildObservation,
    observed_kind: FileSystemEntryKind,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::OrdersDirectoryEntryNameUnusable,
        Severity::Warning,
        EvidenceScope::Package,
        // The path is the canonicalized Orders Directory joined with one entry
        // name, so it is absolute for the same reason the Orders Directory is.
        Location::absolute(observation.path().to_path_buf())
            .expect("a direct child of the canonical Orders Directory is absolute"),
        Vec::new(),
        DiagnosticEvidence::OrdersDirectoryEntryNameUnusable { observed_kind },
    )
}

/// What one direct child of the Orders Directory that is not an Order candidate
/// reports, or `None` when it reports nothing.
///
/// The ordinary load and the current-index fast path both walk the same
/// classified list and owe the same answer for it, so they read that answer here
/// rather than each carrying a copy of the mapping.
fn unusable_direct_child_diagnostic(
    root: &Path,
    observation: &DirectChildObservation,
) -> Option<Diagnostic> {
    match observation.classification() {
        DirectChildClassification::Disallowed(kind) => Some(
            orders_directory_entry_disallowed_diagnostic(root, observation, kind),
        ),
        DirectChildClassification::NameUnusable(kind) => Some(
            orders_directory_entry_name_unusable_diagnostic(observation, kind),
        ),
        // A hidden child is ignored rather than reported, and an Order candidate
        // is projected as an Order instead.
        DirectChildClassification::IgnoredHidden | DirectChildClassification::OrderCandidate => {
            None
        }
    }
}

fn healthy_order_from_validated(
    paths: OrderPaths,
    validated: &ValidatedOrder,
    readme_text: Option<String>,
) -> HealthyOrder {
    HealthyOrder {
        paths,
        document: validated.order().clone(),
        item_totals: validated.item_totals().to_vec(),
        order_total: validated.order_total(),
        readme_text,
    }
}

pub(crate) fn project_created_order(
    root: &Path,
    directory: &Path,
    document: &OrderDocument,
) -> HealthyOrder {
    // Creation writes the conventional README itself, as an empty document, so the
    // created Order's text is known rather than merely unread.
    project_committed_order(root, directory, document, Some(String::new()))
}

/// The text of the Order README at `directory`, as `read_readme` reads it.
///
/// `Some("")` for an absent README, `Some(text)` for one that was read, and `None`
/// for one that exists and could not be read, is not valid UTF-8, or is not a file.
/// For projections built without a refreshed Package, which have no loaded Order to
/// take the text from.
pub(crate) fn order_readme_text<F: FileSystem>(
    root: &Path,
    directory: &Path,
    filesystem: &F,
) -> Option<String> {
    read_readme(&order_paths(root, directory).readme, filesystem)
        .ok()
        .and_then(|readme| readme.text().map(str::to_owned))
}

pub(crate) fn project_committed_order(
    root: &Path,
    directory: &Path,
    document: &OrderDocument,
    readme_text: Option<String>,
) -> HealthyOrder {
    let validated = document
        .validate()
        .expect("a committed Order was validated before its first write");
    healthy_order_from_validated(order_paths(root, directory), &validated, readme_text)
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
                path: path.into(),
                configured_value: match path {
                    ConfiguredPath::Orders => config.orders_path.clone(),
                    ConfiguredPath::Removed => config.removed_path.clone(),
                },
                reason: failure.into(),
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
        PackagePathError::OrdersDirectoryNameUnusable => Diagnostic::new(
            DiagnosticCode::ConfiguredOrdersDirectoryNameUnusable,
            Severity::Error,
            EvidenceScope::Package,
            managed_location(root.join(&config.orders_path), config.orders_path.clone()),
            Vec::new(),
            DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable {
                configured_value: config.orders_path.clone(),
            },
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

/// A `Location` for a managed path with a Package-relative spelling already in hand.
///
/// The expectation holds by construction rather than by assertion. Every caller
/// supplies either one of this module's own literal managed names, an
/// `IndexOrderDirectory` whose validator enforces the same grammar, or a value
/// `relative_path` produced — and `relative_path` only ever produces normalized
/// text, because the grammar is what decides whether it produces any at all.
fn managed_location(path: PathBuf, relative: impl Into<String>) -> Location {
    Location::within_package(path, relative).expect("managed location is normalized")
}

/// The Package-relative spelling of a managed path inside `root`.
///
/// Every caller passes a path this crate assembled from its own literal managed
/// names, or a direct child of the Orders Directory that
/// `discover_order_direct_children` has already classified — and a child whose
/// name has no spelling is classified `NameUnusable` and never reaches a
/// projection. So the expectation is discharged by the classification rather
/// than asserted here.
///
/// The configured managed paths were once outside that argument: an Orders
/// Directory reached through a symbolic link resolves to a real name no configured
/// value was checked against. `resolve_managed_paths` now asks the same grammar of
/// the name it resolved to and refuses a Package whose managed directory has no
/// spelling, so those paths reach this expectation already discharged too.
fn relative_path(root: &Path, path: &Path) -> String {
    package_relative_path_text(root, path).expect("managed paths have a Package-relative spelling")
}
