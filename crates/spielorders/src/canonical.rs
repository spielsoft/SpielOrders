//! Canonical Package classification, projection, ordinary Package Index refresh, and exhaustive
//! managed-evidence validation.

use crate::code::Coded;
use crate::{
    Answer, Failure,
    digest::ContentDigest,
    filesystem::{DestinationExpectation, replace_validated_file_expecting},
    integrity::{Aggregate, AggregateFault, Records, named_vendor, resolves},
    metadata::{
        Config, IndexDocument, IndexOrder, IndexOrderDirectory, MetadataErrorKind, OrderDocument,
        OrderId, PackageRoot, PackageRootLocator, StoredValues, Vendor, VendorId, VendorsDocument,
        Version1, VersionDocument, config_shape_field, order_shape_field, vendors_shape_field,
        version_shape_field,
    },
    mutation::{MutationReport, Never, OperationKind, PlannedOperation},
    operations::refuse_linked_root,
    operations::{FailedOperation, FailedPath},
    order::{OrderValidationFailure, ValidatedOrder},
    package::index_semantics_are_valid,
    paths::{
        ConfiguredPath, ConfiguredPathFailure, DirectChildClassification, DirectChildObservation,
        PackagePathError, discover_order_direct_children, locator_resolves_to_root,
        resolve_package_layout,
    },
    ports::{
        DirectoryEntry, FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
        NewFileLinkOutcome, RenameOutcome,
    },
    readme::{
        ConfigurationPathKind, ConfigurationPathReason, ConfiguredOrdersDirectoryReason,
        Diagnostic, DiagnosticCode, DiagnosticEvidence, EvidenceScope, Location,
        ManagedArtifactKind, OrderLocatorReason, OrderSemanticReason, ReadmeKind, ReadmeRead,
        ReadmeReference, Severity, package_relative_path_text, read_document,
    },
    values::ValueProjections,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnmanagedPackage {
    pub root: Location,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ManagedContext {
    pub root: Location,
    pub metadata_directory: Location,
    pub config: Location,
    pub version: Location,
    pub vendors: Location,
    pub index: Location,
    pub orders_directory: Option<Location>,
    pub removed_orders_directory: Option<Location>,
    pub package_readme: ReadmeReference,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageIndexRecord {
    pub location: Location,
    pub state: PackageIndexState,
    pub malformed_reasons: Vec<PackageIndexMalformedReason>,
    pub stale_reasons: Vec<PackageIndexStaleReason>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataState {
    Current,
    Unsupported,
    NotAssessed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderPaths {
    pub directory: Location,
    pub metadata: Location,
    pub order: Location,
    pub readme: ReadmeReference,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HealthyOrder {
    pub paths: OrderPaths,
    pub document: OrderDocument,
    pub item_totals: Vec<f64>,
    pub order_total: f64,
    /// The Order README text, or `None` when this projection does not know it.
    ///
    /// An absent README reads as `Some("")`, because absence is a text: there is
    /// nothing written. `None` says the opposite — the document exists and could not be
    /// read, is not valid UTF-8, or is not a file at all; the load's document diagnostics
    /// say which. Collapsing the two would report a file that holds bytes as one a reader
    /// could safely treat as blank.
    ///
    /// **Every capability that reports an Order carries this distinction unchanged.**
    /// None of them writes the Order README, so each reports what the load knew — and
    /// `None` when the load knew nothing.
    pub readme_text: Option<String>,
}

impl HealthyOrder {
    /// This Order holding `document` instead, its totals recalculated; `None` for a
    /// document that does not validate.
    pub(crate) fn with_document(&self, document: &OrderDocument) -> Option<Self> {
        let validated = document.validate().ok()?;
        Some(healthy_order_from_validated(
            self.paths.clone(),
            &validated,
            self.readme_text.clone(),
        ))
    }

    /// This Order's technical locator, which a healthy Order always carries: being healthy
    /// is the load having followed that locator from this Directory to this Root.
    pub(crate) fn locator(&self) -> &PackageRoot {
        self.document
            .package_root
            .spelled()
            .expect("a healthy Order is one whose locator was followed to its Package")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DamagedOrder {
    pub paths: OrderPaths,
    pub order_id: Option<OrderId>,
    pub vendor_id: Option<VendorId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableValueProjection {
    pub value: String,
    pub usage_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticGroup {
    pub scope: EvidenceScope,
    pub location: Location,
    pub diagnostics: Vec<Diagnostic>,
}

/// One loaded Package's contents: its managed paths, its healthy and Damaged Orders, its
/// Vendors, its Reusable Free-Entry Values, its findings grouped at the narrowest scope,
/// and the state of its derived Package Index.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CurrentPackageCandidate {
    pub context: ManagedContext,
    pub metadata_version: u64,
    pub metadata_state: MetadataState,
    pub healthy_orders: Vec<HealthyOrder>,
    pub damaged_orders: Vec<DamagedOrder>,
    /// Every Vendor in the Package, Hidden Vendors included, each carrying its own
    /// visibility.
    pub vendors: Vec<Vendor>,
    /// The values `vendors.json` keeps, normalized.
    pub values: StoredValues,
    /// Every value the Package keeps or a healthy Order uses, with its usage count.
    pub projections: ValueProjections,
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    pub index: PackageIndexRecord,
}

impl CurrentPackageCandidate {
    /// The Order Groups of one Project, or of the Orders with no Project for `None`, each
    /// with the number of healthy Orders using it.
    ///
    /// A group belongs to the Project of the Orders that use it or that keeps it, so the
    /// same name under two Projects is two groups. Projects compare as values do: trimmed
    /// and case-insensitively.
    pub fn order_groups_of(&self, project: Option<&str>) -> Vec<ReusableValueProjection> {
        self.projections.order_groups_of(project).to_vec()
    }

    /// Puts `order` in place of the healthy Order at `position`, and `values` in place of
    /// the kept values, recomputing the value projections only where a value changed.
    pub(crate) fn replace_order(
        &mut self,
        position: usize,
        order: HealthyOrder,
        values: StoredValues,
    ) {
        let before = &self.healthy_orders[position].document;
        let values_changed = values != self.values
            || before.orderer != order.document.orderer
            || before.ordering_institution != order.document.ordering_institution
            || before.project != order.document.project
            || before.order_group != order.document.order_group;
        self.healthy_orders[position] = order;
        self.values = values;
        if values_changed {
            self.recompute_projections();
        }
    }

    /// Puts `vendors` in place of the Package's Vendors.
    pub(crate) fn replace_vendors(&mut self, vendors: Vec<Vendor>) {
        self.vendors = vendors;
    }

    fn recompute_projections(&mut self) {
        self.projections = crate::values::project(
            &self.values,
            self.healthy_orders.iter().map(|order| &order.document),
        );
    }

    /// The Vendor the healthy Order at `order_index` references, Hidden Vendors included.
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

    /// `value`, read from this Package, with the Package's findings beside it.
    pub(crate) fn answer<T>(&self, value: T) -> Answer<T> {
        Answer::from(Ok(value)).beside(self.findings())
    }

    /// Every finding of this Package, in its groups' order.
    pub(crate) fn findings(&self) -> Vec<Diagnostic> {
        self.diagnostic_groups
            .iter()
            .flat_map(|group| group.diagnostics.iter().cloned())
            .collect()
    }

    /// Diagnostics assigned to one Order's directory and documents, in core order.
    pub(crate) fn order_diagnostics(&self, paths: &OrderPaths) -> Vec<Diagnostic> {
        let directory = &paths.directory.absolute_path;
        self.diagnostic_groups
            .iter()
            .filter(|group| group.location.absolute_path.starts_with(directory.clone()))
            .flat_map(|group| group.diagnostics.iter().cloned())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DamagedPackageProjection {
    pub context: ManagedContext,
    pub metadata_state: MetadataState,
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    pub index: PackageIndexRecord,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IndexReplacementFailedProjection {
    pub canonical_evidence: CurrentPackageCandidate,
    pub failure: Failure,
}

impl IndexReplacementFailedProjection {
    pub(crate) fn into_parts(self) -> (CurrentPackageCandidate, Failure) {
        (self.canonical_evidence, self.failure)
    }
}

/// A load stopped by an operation, inside the Root it had resolved where it had one.
fn operation_failed<T>(root: Option<&Location>, failure: FailedOperation) -> Result<T, Failure> {
    Err(match root {
        Some(root) => failure.under(root),
        None => failure,
    }
    .into())
}

/// What a Package load that refused established about the Package.
impl Failure {
    /// A Package the load read and could not trust, under the code its metadata state
    /// earns: an uninterpretable version is diagnostic-only whatever else is wrong.
    fn damaged(projection: DamagedPackageProjection) -> Self {
        match projection.metadata_state {
            MetadataState::Unsupported => Self::PackageMetadataUnsupported(Box::new(projection)),
            MetadataState::Current | MetadataState::NotAssessed => {
                Self::DamagedPackage(Box::new(projection))
            }
        }
    }

    /// The Package metadata at `path` could not be observed.
    pub(crate) fn unobservable<F: FileSystem>(
        path: &Path,
        error: &F::Error,
        filesystem: &F,
    ) -> Self {
        FailedOperation::filesystem(FailedPath::of(path), error, filesystem).into()
    }

    /// No ancestor of `directory` is a SpielOrders Root.
    pub(crate) fn unmanaged(directory: &Path) -> Self {
        Self::UnmanagedPackage(Box::new(UnmanagedPackage {
            root: Location::absolute(directory.to_path_buf())
                .expect("a canonicalized directory is absolute"),
        }))
    }

    /// The damaged projection this refusal carries, where the load got that far.
    fn damaged_projection_mut(&mut self) -> Option<&mut DamagedPackageProjection> {
        match self {
            Self::PackageMetadataUnsupported(projection) | Self::DamagedPackage(projection) => {
                Some(projection)
            }
            _ => None,
        }
    }
}

/// A Package the refresh after a commit reached: with its derived Package Index current,
/// or with the canonical evidence whose rebuilt index could not be written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PackageLoad {
    Available(CurrentPackageCandidate),
    PackageIndexReplacementFailed(IndexReplacementFailedProjection),
}
/// What one exhaustive validation of a Package found: every finding, grouped, and every
/// Order Directory it examined, healthy or damaged, in one deterministic order.
pub(crate) struct ExhaustiveValidationReport {
    pub(crate) context: ManagedContext,
    metadata_state: MetadataState,
    pub(crate) diagnostic_groups: Vec<DiagnosticGroup>,
    pub(crate) examined_orders: Vec<OrderPaths>,
    index: PackageIndexRecord,
    records: ExhaustiveInspectionEvidence,
}

impl ExhaustiveValidationReport {
    pub(crate) fn metadata_state(&self) -> MetadataState {
        self.metadata_state
    }

    /// The Order records this validation examined, with the Vendor aggregate as decoded,
    /// or `None` where it could not be decoded, however damaged the Package is.
    pub(crate) fn records(&self) -> Records<'_> {
        Records::new(
            self.records.vendors.as_deref(),
            &self.records.healthy,
            &self.records.damaged,
        )
    }

    /// The Package this report validated, as a refusal, when its own scope carries an
    /// error: only the Package-scope findings travel with it.
    pub(crate) fn package_damage(&self) -> Option<Failure> {
        let diagnostic_groups = self
            .diagnostic_groups
            .iter()
            .filter(|group| group.scope == EvidenceScope::Package)
            .cloned()
            .collect::<Vec<_>>();
        let damaged = diagnostic_groups.iter().any(|group| {
            group
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
        });
        damaged.then(|| {
            Failure::damaged(DamagedPackageProjection {
                context: self.context.clone(),
                metadata_state: self.metadata_state,
                diagnostic_groups,
                index: self.index.clone(),
            })
        })
    }
}

/// The complete Package's validation report. A damaged Package is a complete report whose
/// findings are the damage, so only an unmanaged Root or a failed operation refuses.
pub(crate) fn validate_package<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<ExhaustiveValidationReport, Failure> {
    let mut exhaustive_evidence = ExhaustiveInspectionEvidence::default();
    let canonical = inspect(
        selected_root,
        filesystem,
        InspectionBreadth::Exhaustive,
        &mut exhaustive_evidence,
    );
    let mut order_paths = std::mem::take(&mut exhaustive_evidence.order_paths);
    order_paths.sort_by(|left, right| left.directory.cmp(&right.directory));
    let (context, metadata_state, mut diagnostic_groups, index) = match canonical {
        Ok(package) => (
            package.context,
            package.metadata_state,
            package.diagnostic_groups,
            package.index,
        ),
        Err(Failure::PackageMetadataUnsupported(package) | Failure::DamagedPackage(package)) => (
            package.context,
            package.metadata_state,
            package.diagnostic_groups,
            package.index,
        ),
        Err(unusable) => return Err(unusable),
    };

    let read_readme = |readme, groups: &mut Vec<DiagnosticGroup>| {
        read_document(readme, filesystem)
            .map(|readme| append_document_diagnostics(groups, &readme))
            .or_else(|failure| operation_failed(Some(&context.root), failure))
    };
    read_readme(&context.package_readme, &mut diagnostic_groups)?;
    if let Err(failure) =
        append_reserved_diagnostics(&context, &order_paths, filesystem, &mut diagnostic_groups)
    {
        return operation_failed(Some(&context.root), failure);
    }
    for paths in &order_paths {
        read_readme(&paths.readme, &mut diagnostic_groups)?;
    }
    sort_diagnostic_groups(&mut diagnostic_groups);

    Ok(ExhaustiveValidationReport {
        context,
        metadata_state,
        diagnostic_groups,
        examined_orders: order_paths,
        index,
        records: exhaustive_evidence,
    })
}

/// One opened Package: its contents, with the stored Package Index assessed and not
/// replaced, and its README. It answers live list filtering itself.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Package {
    pub contents: CurrentPackageCandidate,
    pub readme: ReadmeRead,
}

/// Opens the Package at `selected_root` without writing anything.
pub(crate) fn open<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<Package, Failure> {
    let package = assessed(selected_root, filesystem)?.package;
    let readme = read_document(&package.context.package_readme, filesystem)
        .or_else(|failure| operation_failed(Some(&package.context.root), failure))?;
    Ok(Package {
        contents: package,
        readme,
    })
}

/// The conventional Package README of the Package at `selected_root`.
///
/// Where it is depends only on the Root's managed paths, so a Package whose records are
/// damaged, or at a metadata version this build does not interpret, still answers.
pub(crate) fn read_readme<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<ReadmeRead, Failure> {
    let context = match load_package(selected_root, filesystem) {
        Ok(package) => package.context,
        Err(Failure::PackageMetadataUnsupported(package) | Failure::DamagedPackage(package)) => {
            package.context
        }
        Err(unusable) => return Err(unusable),
    };
    read_document(&context.package_readme, filesystem)
        .or_else(|failure| operation_failed(Some(&context.root), failure))
}

fn append_document_diagnostics(groups: &mut Vec<DiagnosticGroup>, readme: &ReadmeRead) {
    if readme.diagnostics.is_empty() {
        return;
    }
    if let Some(group) = groups.iter_mut().find(|group| {
        group.scope == EvidenceScope::Document && group.location == readme.reference.location
    }) {
        group.diagnostics.extend_from_slice(&readme.diagnostics);
        normalize_diagnostics(&mut group.diagnostics);
    } else {
        groups.push(DiagnosticGroup {
            scope: EvidenceScope::Document,
            location: readme.reference.location.clone(),
            diagnostics: readme.diagnostics.to_vec(),
        });
    }
}

fn append_reserved_diagnostics<F: FileSystem>(
    context: &ManagedContext,
    orders: &[OrderPaths],
    filesystem: &F,
    groups: &mut Vec<DiagnosticGroup>,
) -> Result<(), FailedOperation> {
    let package_expected = ["config.toml", "version.json", "vendors.json", "index.json"];
    let package = ReservedDirectorySpec {
        directory: &context.metadata_directory,
        scope: EvidenceScope::Package,
        group_location: &context.root,
        expected: &package_expected,
    };
    let diagnostics =
        reserved_directory_diagnostics(&context.root.absolute_path, package, filesystem, groups)?;
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
            &context.root.absolute_path,
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
) -> Result<Vec<Diagnostic>, FailedOperation> {
    match filesystem.symlink_metadata(&specification.directory.absolute_path) {
        Ok(Some(FileSystemEntryKind::Directory)) => {}
        Ok(_) => return Ok(Vec::new()),
        Err(error) => {
            return Err(FailedOperation::filesystem(
                specification.directory.clone(),
                &error,
                filesystem,
            ));
        }
    }
    let entries = filesystem
        .read_directory(&specification.directory.absolute_path)
        .map_err(|error| {
            FailedOperation::filesystem(specification.directory.clone(), &error, filesystem)
        })?;
    let mut diagnostics = Vec::new();
    for entry in entries {
        // An entry this application cannot name is refused here for the same
        // reason a name that is not Unicode already was: every diagnostic below
        // needs a Package-relative location for it, and there is none to give.
        // Both are one condition, so they get one answer.
        let named = entry.name.to_str().and_then(|name| {
            let path = specification.directory.absolute_path.join(name);
            let relative = package_relative_path_text(root, &path)?;
            Some((name.to_owned(), path, relative))
        });
        let Some((name, path, relative)) = named else {
            return Err(FailedOperation::unusable_answer(
                specification.directory.clone(),
            ));
        };
        let name = name.as_str();
        if specification.expected.contains(&name) {
            continue;
        }
        let primary = managed_location(path, relative);
        if let Some(destination_name) = recognized_staging_destination(name, specification.expected)
        {
            let destination_path = specification.directory.absolute_path.join(destination_name);
            let destination = managed_location(
                destination_path.clone(),
                relative_path(root, &destination_path),
            );
            let destination_valid = (destination_name != "index.json").then(|| {
                !groups.iter().any(|group| {
                    group.diagnostics.iter().any(|diagnostic| {
                        diagnostic.severity == Severity::Error
                            && diagnostic.primary_location == destination
                    })
                })
            });
            // Which candidate is staged here, so a confirmation to promote it stops
            // authorizing anything once a different candidate takes its place. Only an
            // entry the listing reported as a regular file is read: a directory has no
            // content to promote, and a link is not this application's to follow.
            // Neither yields identifying evidence, and neither is given a stand-in.
            let candidate_digest = match entry.kind {
                FileSystemEntryKind::File => filesystem
                    .read(&primary.absolute_path)
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
                    observed_kind: entry.kind,
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

/// One Package read with its stored Package Index assessed against it, having written
/// nothing.
struct Assessed {
    package: CurrentPackageCandidate,
    /// The stored index, where change evidence did not prove it current and so a rebuild
    /// would replace it.
    replaceable: Option<StoredIndex>,
}

/// What a rebuild replacing the stored index expects to find there.
struct StoredIndex {
    bytes: Option<Vec<u8>>,
    /// Whether nothing at all was observed at the index path. Not the same question as
    /// carrying no bytes: an index that is a directory, or a file whose read failed,
    /// carries none either and is plainly there.
    observed_absent: bool,
}

/// The Package at `selected_root` with its stored Package Index assessed: from the index
/// itself where change evidence proves it current, and from the canonical records
/// otherwise.
fn assessed<F: FileSystem>(selected_root: &Path, filesystem: &F) -> Result<Assessed, Failure> {
    // A Package this call already proved current, with nothing written since, is still
    // that Package.
    if refuse_linked_root(selected_root, filesystem).is_ok()
        && let Ok(root) = filesystem.canonicalize(selected_root)
        && let Some(package) = filesystem.remembered_package(&root)
        && package.index.state == PackageIndexState::Current
    {
        return Ok(Assessed {
            package,
            replaceable: None,
        });
    }
    let filesystem = PackageOpenFileSystem::new(filesystem);
    if let Some(package) = try_open_current_index(selected_root, &filesystem) {
        return Ok(Assessed {
            package,
            replaceable: None,
        });
    }
    let package = load_records(selected_root, &filesystem)
        .map_err(|unusable| assessed_without_freshness(unusable, &filesystem))?;
    let context = package.context.clone();
    let StoredIndexAssessment {
        package,
        stored_bytes,
        observed_absent,
    } = assess_stored_index(package, &filesystem)
        .map_err(|failure| Failure::from(failure.under(&context.root)))?;
    Ok(Assessed {
        package,
        replaceable: Some(StoredIndex {
            bytes: stored_bytes,
            observed_absent,
        }),
    })
}

/// The Package at `selected_root`, its derived Package Index rebuilt from canonical
/// records where change evidence does not prove the stored one current.
pub(crate) fn load_package_and_index<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<PackageLoad, Failure> {
    let Assessed {
        mut package,
        replaceable,
    } = assessed(selected_root, filesystem)?;
    let Some(StoredIndex {
        bytes: stored_bytes,
        observed_absent,
    }) = replaceable
    else {
        filesystem.remember_package(&package);
        return Ok(PackageLoad::Available(package));
    };
    // A load that could not read an Order README has nothing to cache for it, so it
    // caches nothing: the stored index keeps whatever a load that could read it left
    // there, and its assessment stands as found. A failed read is not evidence that the
    // document is empty, so the projection must not replace its last known text with "".
    let Some(index) = index_document(&package) else {
        return Ok(PackageLoad::Available(package));
    };
    let index_path = package.context.index.absolute_path.clone();
    let candidate = index
        .to_json_bytes()
        .expect("validated canonical facts serialize as metadata version 1");
    let replacement = replace_validated_file_expecting(
        &index_path,
        &candidate,
        index_expectation(stored_bytes.as_deref(), observed_absent),
        filesystem,
        |_| Ok::<(), Never>(()),
    );
    if let Err(error) = replacement {
        let failure =
            Failure::of_replacement(&package.context.root.absolute_path, &index_path, error);
        return Ok(PackageLoad::PackageIndexReplacementFailed(
            IndexReplacementFailedProjection {
                canonical_evidence: package,
                failure,
            },
        ));
    }
    mark_index_rebuilt(&mut package.index);
    filesystem.remember_package(&package);
    Ok(PackageLoad::Available(package))
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
/// heals it, permanently, and blame a concurrent writer for it.
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

/// The index record a completed rebuild leaves: current, and carrying none of the reasons
/// it was rebuilt for.
fn mark_index_rebuilt(record: &mut PackageIndexRecord) {
    record.state = PackageIndexState::Current;
    record.malformed_reasons.clear();
    record.stale_reasons.clear();
}

/// What a deliberate Package Index rebuild would write, and the state it found.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageIndexRebuildPlan {
    package: CurrentPackageCandidate,
    candidate: Vec<u8>,
    stored_bytes: Option<Vec<u8>>,
    observed_absent: bool,
    pub(crate) operations: Vec<PlannedOperation>,
}

/// A rejected rebuild is a failed commit: the replacement did not happen and the stored
/// index holds what it held, with any stage it could not remove named as its cleanup.
pub(crate) type IndexRebuildReport = MutationReport<()>;

/// Plans a deliberate replacement of the derived Package Index from healthy canonical
/// records.
///
/// The canonical records are the source, never the stored index. The comparison against
/// the stored projection supplies the state the rebuild reports.
///
/// Nothing here writes. [`commit_index_rebuild`] is what replaces the file, and a
/// plan that is never committed is the dry run.
pub(crate) fn plan_index_rebuild<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Answer<PackageIndexRebuildPlan> {
    Answer::of(|| plan_rebuild_of(selected_root, filesystem))
}

fn plan_rebuild_of<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<PackageIndexRebuildPlan, Failure> {
    let filesystem = PackageOpenFileSystem::new(filesystem);
    // A Package whose canonical state cannot be trusted has no healthy records to
    // project, and the index is never evidence for what they should have been. The
    // index's own validity is still assessed, because that is what the refusal is about
    // the index of.
    let package = load_records(selected_root, &filesystem)
        .map_err(|unusable| assessed_without_freshness(unusable, &filesystem))?;
    let context = package.context.clone();
    let StoredIndexAssessment {
        package,
        stored_bytes,
        observed_absent,
    } = assess_stored_index(package, &filesystem)
        .map_err(|failure| Failure::from(failure.under(&context.root)))?;
    let Some(document) = index_document(&package) else {
        return Err(Failure::OrderReadmeTextUnavailable(Box::new(package)));
    };
    let candidate = document
        .to_json_bytes()
        .expect("validated canonical facts serialize as metadata version 1");
    let operations = vec![PlannedOperation::new(
        OperationKind::RefreshPackageIndex,
        package.context.index.clone(),
    )];
    Ok(PackageIndexRebuildPlan {
        package,
        candidate,
        stored_bytes,
        observed_absent,
        operations,
    })
}

/// Replaces the derived Package Index with the planned projection.
///
/// The replacement is the established beside-destination staging, durability, destination
/// comparison, and atomic rename, against the index this plan assessed — so a writer that
/// landed since is refused rather than overwritten. A refusal leaves the stored index
/// exactly as it was.
pub(crate) fn commit_index_rebuild<F: FileSystem>(
    plan: PackageIndexRebuildPlan,
    filesystem: &F,
) -> Answer<IndexRebuildReport> {
    let index_path = plan.package.context.index.absolute_path.clone();
    // The records are read again now. An index written from a plan's earlier read would be
    // newer than a record changed since, and so would prove that change's stale facts.
    match plan_rebuild_of(&plan.package.context.root.absolute_path, filesystem) {
        Ok(now) if now.candidate == plan.candidate => {}
        Ok(_) => {
            return Answer::of(|| {
                Err(Failure::PreconditionsChanged {
                    location: plan.package.context.index.clone(),
                })
            });
        }
        Err(failure) => return Answer::of(|| Err(failure)),
    }
    let replacement = replace_validated_file_expecting(
        &index_path,
        &plan.candidate,
        index_expectation(plan.stored_bytes.as_deref(), plan.observed_absent),
        filesystem,
        |_| Ok::<(), Never>(()),
    );
    match replacement {
        Err(error) => Err(Failure::of_replacement(
            &plan.package.context.root.absolute_path,
            &index_path,
            error,
        )),
        Ok(()) => Ok(MutationReport::Committed { result: () }),
    }
    .into()
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
/// **Nothing here writes**, per Durable managed-file writes.
fn assess_stored_index<F: FileSystem>(
    mut package: CurrentPackageCandidate,
    filesystem: &F,
) -> Result<StoredIndexAssessment, FailedOperation> {
    let assessment = assess_index(package.index.clone(), filesystem)?;
    let observed_absent = assessment.record.state == PackageIndexState::Missing;
    let stored_bytes = assessment.bytes;
    package.index = assessment.record;
    if package.index.state == PackageIndexState::NotAssessed {
        let document = assessment
            .document
            .as_ref()
            .expect("a valid deterministic index remains available");
        if package.damaged_orders.is_empty() && index_document(&package).as_ref() == Some(document)
        {
            package.index.state = PackageIndexState::Current;
        } else {
            package.index.stale_reasons = stale_index_reasons(document, &package);
            package.index.state = PackageIndexState::Stale;
        }
    }
    Ok(StoredIndexAssessment {
        package,
        stored_bytes,
        observed_absent,
    })
}

/// A Package whose canonical state cannot be trusted has no records to compare the
/// index against, so the freshness half of the question is unanswerable and only the
/// index's own validity is reported.
fn assessed_without_freshness<F: FileSystem>(mut unusable: Failure, filesystem: &F) -> Failure {
    if let Some(projection) = unusable.damaged_projection_mut() {
        projection.index = assess_index_without_freshness(projection.index.clone(), filesystem);
    }
    unusable
}

/// The Package the stored index proves current, read from it rather than from every
/// record.
fn try_open_current_index<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Option<CurrentPackageCandidate> {
    open_index(selected_root, filesystem, &Touched::default()).map(|(package, _)| package)
}

/// The last index document this process judged valid, with the exact bytes it was read
/// from, so that the same bytes are not judged again.
static ASSESSED_INDEX: std::sync::Mutex<Option<(Vec<u8>, IndexDocument)>> =
    std::sync::Mutex::new(None);

/// The document `bytes` hold, where this process already judged exactly these bytes a
/// valid index.
fn remembered_index(bytes: &[u8]) -> Option<IndexDocument> {
    let remembered = ASSESSED_INDEX.lock().ok()?;
    let (own, document) = remembered.as_ref()?;
    (own.as_slice() == bytes).then(|| document.clone())
}

fn remember_index(bytes: &[u8], document: &IndexDocument) {
    if let Ok(mut remembered) = ASSESSED_INDEX.lock() {
        *remembered = Some((bytes.to_vec(), document.clone()));
    }
}

/// The records a committed change wrote: the Order Directories it created, rewrote,
/// moved, or removed, and whether it replaced the Vendor record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Touched {
    pub(crate) orders: std::collections::BTreeSet<PathBuf>,
    pub(crate) vendors: bool,
}

impl Touched {
    pub(crate) fn orders(orders: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            orders: orders.into_iter().collect(),
            vendors: false,
        }
    }

    pub(crate) fn with_vendors(mut self, vendors: bool) -> Self {
        self.vendors |= vendors;
        self
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty() && !self.vendors
    }
}

/// The Package the stored index and `touched` together prove, with the stored index
/// bytes: every record `touched` names is read from disk, and every other one must be
/// proven unchanged since the index was written. `None` wherever anything is in doubt,
/// including a touched record that is not plainly healthy, so the caller reads every
/// record instead.
fn open_index<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
    touched: &Touched,
) -> Option<(CurrentPackageCandidate, Vec<u8>)> {
    refuse_linked_root(selected_root, filesystem).ok()?;
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
        .observe_file_change(&context_without_paths.config.absolute_path)
        .ok()?;
    let version_observation = filesystem
        .observe_file_change(&context_without_paths.version.absolute_path)
        .ok()?;
    let index_observation = filesystem
        .observe_file_change(&context_without_paths.index.absolute_path)
        .ok()?;
    if [config_observation, version_observation, index_observation]
        .into_iter()
        .any(|observation| observation.kind != Some(FileSystemEntryKind::File))
    {
        return None;
    }

    let config = Config::from_toml_bytes(
        &filesystem
            .read(&context_without_paths.config.absolute_path)
            .ok()?,
    )
    .ok()?;
    let version = VersionDocument::from_json_bytes(
        &filesystem
            .read(&context_without_paths.version.absolute_path)
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
            malformed_reasons: Vec::new(),
            stale_reasons: Vec::new(),
        },
        filesystem,
    )
    .ok()?;
    if assessment.record.state != PackageIndexState::NotAssessed {
        return None;
    }
    let index = assessment.document?;
    let assessed_bytes = assessment.bytes?;

    let vendors_observation = filesystem
        .observe_file_change(&context_without_paths.vendors.absolute_path)
        .ok()?;
    if vendors_observation.kind != Some(FileSystemEntryKind::File) {
        return None;
    }
    if !newer(index_observation, config_observation)
        || !newer(index_observation, version_observation)
        || (!touched.vendors && !newer(index_observation, vendors_observation))
    {
        return None;
    }
    let stored_bytes = assessed_bytes;
    let tracking_filesystem = InspectionFileSystem::new(filesystem);
    let layout = resolve_package_layout(&root, &config, &tracking_filesystem).ok()?;
    let layout = discover_order_direct_children(layout, &tracking_filesystem).ok()?;
    let context = managed_context(
        &root,
        &layout.orders_directory,
        &layout.removed_orders_directory,
    );

    let mut candidate_paths = layout
        .direct_children
        .iter()
        .filter(|entry| entry.classification == DirectChildClassification::OrderCandidate)
        .map(|entry| (relative_path(&root, &entry.path), entry.path.to_path_buf()))
        .collect::<Vec<_>>();
    candidate_paths.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    // A record outside this Package is no part of what the index can prove.
    let touched_paths = touched
        .orders
        .iter()
        .map(|directory| package_relative_path_text(&root, directory))
        .collect::<Option<std::collections::BTreeSet<_>>>()?;
    let indexed_paths = index
        .orders
        .iter()
        .map(|entry| entry.order_directory.as_str())
        .collect::<Vec<_>>();
    if indexed_paths
        .iter()
        .copied()
        .filter(|path| !touched_paths.contains(*path))
        .collect::<std::collections::BTreeSet<_>>()
        != candidate_paths
            .iter()
            .map(|(relative, _)| relative.as_str())
            .filter(|path| !touched_paths.contains(*path))
            .collect::<std::collections::BTreeSet<_>>()
    {
        return None;
    }
    let orders_directory = context
        .orders_directory
        .as_ref()?
        .package_relative_path
        .clone()?;
    let prefix = format!("{orders_directory}/");
    if indexed_paths.iter().any(|path| {
        path.strip_prefix(&prefix)
            .is_none_or(|remainder| remainder.is_empty() || remainder.contains('/'))
    }) {
        return None;
    }

    for (_, order_directory) in &candidate_paths {
        if touched_paths.contains(&relative_path(&root, order_directory)) {
            // A record reached through a linked metadata directory is not this Order's own.
            if filesystem
                .symlink_metadata(&order_directory.join(".spielorders"))
                .ok()?
                != Some(FileSystemEntryKind::Directory)
            {
                return None;
            }
            continue;
        }
        // An Order Directory renamed, moved in, or swapped with another since the index
        // was written is newer than the index, though the records inside it keep their
        // own status. Its status also proves every entry inside it the one the index was
        // written beside: adding, removing, or renaming one makes the directory newer. So
        // an absent README is still absent, and the metadata directory the indexed record
        // was read through is still a directory and not a link.
        let directory_observation = filesystem.observe_file_change(order_directory).ok()?;
        if directory_observation.kind != Some(FileSystemEntryKind::Directory)
            || !newer(index_observation, directory_observation)
        {
            return None;
        }
        let paths = order_paths(&root, order_directory);
        let order_observation = filesystem
            .observe_file_change(&paths.order.absolute_path)
            .ok()?;
        if order_observation.kind != Some(FileSystemEntryKind::File) {
            return None;
        }
        if !newer(index_observation, order_observation) {
            return None;
        }
        let readme_observation = filesystem
            .observe_file_change(&paths.readme.location.absolute_path)
            .ok()?;
        match readme_observation.kind {
            Some(FileSystemEntryKind::File) => {
                if !newer(index_observation, readme_observation) {
                    return None;
                }
            }
            None => {}
            Some(_) => return None,
        }
    }
    let mut package_diagnostics = layout
        .direct_children
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
    let (vendors, values) = if touched.vendors {
        let document = VendorsDocument::from_json_bytes(
            &filesystem
                .read(&context_without_paths.vendors.absolute_path)
                .ok()?,
        )
        .ok()?;
        let normalized = Aggregate::judge(&document).ok()?.into_document();
        (normalized.vendors, normalized.values)
    } else {
        (index.vendors, index.values)
    };
    let mut healthy_orders = Vec::new();
    for (relative, order_directory) in &candidate_paths {
        if touched_paths.contains(relative) {
            healthy_orders.push(touched_order(
                &root,
                &layout.orders_directory,
                order_directory,
                &vendors,
                filesystem,
            )?);
        }
    }
    let untouched = index
        .orders
        .iter()
        .filter(|entry| !touched_paths.contains(entry.order_directory.as_str()))
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
    healthy_orders.extend(untouched);
    if !touched.is_empty() {
        // The records read here join the proven ones only as a valid Package would hold
        // them: one Order per identity, and every Vendor Reference resolving.
        if !Records::new(Some(&vendors), &healthy_orders, &[]).sound() {
            return None;
        }
        sort_healthy_orders(&mut healthy_orders);
    }
    Some((
        current_package_candidate(
            context,
            healthy_orders,
            Vec::new(),
            vendors,
            values,
            diagnostic_groups,
            PackageIndexRecord {
                location: context_without_paths.index,
                state: PackageIndexState::Current,
                malformed_reasons: Vec::new(),
                stale_reasons: Vec::new(),
            },
        ),
        stored_bytes,
    ))
}

/// The Order at `order_directory`, read from its records, where it is plainly healthy:
/// [`order_health`] finds it healthy against `vendors`, and its README reads without
/// findings.
fn touched_order<F: FileSystem>(
    root: &Path,
    orders_directory: &Path,
    order_directory: &Path,
    vendors: &[Vendor],
    filesystem: &F,
) -> Option<HealthyOrder> {
    let paths = order_paths(root, order_directory);
    let OrderHealth::Healthy(validated) =
        order_health(root, orders_directory, &paths, Some(vendors), filesystem).ok()?
    else {
        return None;
    };
    let readme = read_document(&paths.readme, filesystem).ok()?;
    if !readme.diagnostics.is_empty() {
        return None;
    }
    let text = readme.text?;
    Some(healthy_order_from_validated(paths, &validated, Some(text)))
}

/// The Package at `root` after a committed change that wrote the records `touched`
/// names, with its derived index updated from those records alone where every other
/// record is proven unchanged, and otherwise rebuilt as [`load_package_and_index`]
/// rebuilds it.
pub(crate) fn load_package_and_refresh_index<F: FileSystem>(
    root: &Path,
    touched: &Touched,
    filesystem: &F,
) -> Result<PackageLoad, Failure> {
    if touched.is_empty() {
        return load_package_and_index(root, filesystem);
    }
    let cached = PackageOpenFileSystem::new(filesystem);
    let Some((mut package, stored)) = open_index(root, &cached, touched) else {
        return load_package_and_index(root, filesystem);
    };
    let Some(index) = index_document(&package) else {
        return load_package_and_index(root, filesystem);
    };
    let index_path = package.context.index.absolute_path.clone();
    let candidate = index
        .to_json_bytes()
        .expect("validated canonical facts serialize as metadata version 1");
    if candidate == stored {
        filesystem.remember_package(&package);
        return Ok(PackageLoad::Available(package));
    }
    if let Err(error) = replace_validated_file_expecting(
        &index_path,
        &candidate,
        DestinationExpectation::Exactly(&stored),
        filesystem,
        |_| Ok::<(), Never>(()),
    ) {
        let failure =
            Failure::of_replacement(&package.context.root.absolute_path, &index_path, error);
        return Ok(PackageLoad::PackageIndexReplacementFailed(
            IndexReplacementFailedProjection {
                canonical_evidence: package,
                failure,
            },
        ));
    }
    mark_index_rebuilt(&mut package.index);
    filesystem.remember_package(&package);
    Ok(PackageLoad::Available(package))
}

/// Whether the evidence proves `input` unchanged since `index` was written: both
/// comparable in one domain, the index strictly later. Equal, uncertain, or
/// incomparable evidence proves nothing.
fn newer(index: FileChangeObservation, input: FileChangeObservation) -> bool {
    matches!(
        (index.evidence, input.evidence),
        (
            FileChangeEvidence::Comparable {
                comparison_domain: index_domain,
                sequence: index_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: input_domain,
                sequence: input_sequence,
            },
        ) if index_domain == input_domain && index_sequence > input_sequence
    )
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
                            .package_relative_path
                            .as_ref()
                            .expect("healthy Order paths are established"),
                    )
                    .expect("healthy Order paths are valid index paths"),
                    order: order.document.clone(),
                    readme: order.readme_text.clone()?,
                })
            })
            .collect::<Option<Vec<_>>>()?,
        vendors: package.vendors.clone(),
        values: package.values.clone(),
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
) -> Result<IndexAssessment, FailedOperation> {
    let path = record.location.absolute_path.clone();
    let kind = filesystem.symlink_metadata(&path).map_err(|error| {
        FailedOperation::filesystem(record.location.clone(), &error, filesystem)
    })?;
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
        Err(error) if filesystem.is_access_denied(&error) => {
            return Err(FailedOperation::filesystem(
                record.location.clone(),
                &error,
                filesystem,
            ));
        }
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
    // Exactly the bytes of an index this process already judged valid are that index.
    if let Some(document) = remembered_index(&bytes) {
        return Ok(IndexAssessment {
            record,
            bytes: Some(bytes),
            document: Some(document),
        });
    }
    let document = match IndexDocument::from_json_bytes(&bytes) {
        Ok(document) => document,
        // Only a document this build cannot read can be at another version, so the
        // untyped look at its version is taken only then.
        Err(_)
            if recognizable_version(&bytes, "schema_version")
                .is_some_and(|observed| observed != 1) =>
        {
            record.state = PackageIndexState::WrongVersion;
            return Ok(IndexAssessment {
                record,
                bytes: Some(bytes),
                document: None,
            });
        }
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
    } else {
        remember_index(&bytes, &document);
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
                .package_relative_path
                .as_deref()
                .expect("discovered Order paths are established")
        })
        .collect::<std::collections::BTreeSet<_>>();
    let diagnostics = package
        .diagnostic_groups
        .iter()
        .flat_map(|group| &group.diagnostics)
        .collect::<Vec<_>>();
    let mut reasons = Vec::new();
    if indexed_paths != discovered_paths {
        reasons.push(PackageIndexStaleReason::OrderMembershipChanged);
    }
    let orders_directory = package
        .context
        .orders_directory
        .as_ref()
        .and_then(|location| location.package_relative_path.as_deref())
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
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ManagedArtifactWrongKind)
    }) {
        reasons.push(PackageIndexStaleReason::ManagedInputWrongKind);
    }
    if index.vendors != package.vendors || index.values != package.values {
        reasons.push(PackageIndexStaleReason::VendorsChanged);
    }
    if package.healthy_orders.iter().any(|order| {
        let path = order
            .paths
            .directory
            .package_relative_path
            .as_deref()
            .expect("healthy Order paths are established");
        index
            .orders
            .iter()
            .find(|entry| entry.order_directory.as_str() == path)
            .is_some_and(|entry| entry.order != order.document)
    }) || package.damaged_orders.iter().any(|order| {
        let path = order
            .paths
            .directory
            .package_relative_path
            .as_deref()
            .expect("discovered Order paths are established");
        let Some(entry) = index
            .orders
            .iter()
            .find(|entry| entry.order_directory.as_str() == path)
        else {
            return false;
        };
        order.order_id.as_ref().is_some_and(|id| {
            *id != entry.order.order_id || order.vendor_id != entry.order.vendor_id
        }) || diagnostics.iter().any(|diagnostic| {
            diagnostic.primary_location == order.paths.order
                && matches!(
                    diagnostic.code,
                    DiagnosticCode::ManagedArtifactInvalidEncoding
                        | DiagnosticCode::ManagedArtifactInvalidShape
                        | DiagnosticCode::ManagedArtifactInvalidSyntax
                        | DiagnosticCode::ManagedArtifactWrongVersion
                        | DiagnosticCode::ManagedArtifactMissing
                        | DiagnosticCode::OrderSemanticInvalid
                        | DiagnosticCode::OrderPackageLocatorMismatch
                )
        })
    }) {
        // Invalid bytes cannot agree with the valid Order stored in the index.
        reasons.push(PackageIndexStaleReason::OrderMetadataChanged);
    }
    if package.healthy_orders.iter().any(|order| {
        let path = order
            .paths
            .directory
            .package_relative_path
            .as_deref()
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
    }) || diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.evidence,
            DiagnosticEvidence::ReadmeUnavailable {
                document_kind: ReadmeKind::OrderReadme,
                reason: crate::readme::ReadmeUnavailableReason::WrongKind
                    | crate::readme::ReadmeUnavailableReason::InvalidEncoding,
            }
        )
    }) {
        reasons.push(PackageIndexStaleReason::OrderReadmeChanged);
    }
    let unknown = diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ManagedArtifactUnreadable
            || matches!(
                diagnostic.evidence,
                DiagnosticEvidence::ReadmeUnavailable {
                    document_kind: ReadmeKind::OrderReadme,
                    reason: crate::readme::ReadmeUnavailableReason::Unreadable
                        | crate::readme::ReadmeUnavailableReason::AccessDenied,
                }
            )
    });
    if reasons.is_empty() || unknown {
        reasons.push(PackageIndexStaleReason::ChangeEvidenceUncertain);
    }
    reasons
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InspectionBreadth {
    Ordinary,
    Exhaustive,
}

/// What only an exhaustive inspection keeps: every Order Directory it examined, and the
/// Order records and Vendor aggregate it decoded, whether or not the Package is damaged.
#[derive(Default)]
struct ExhaustiveInspectionEvidence {
    order_paths: Vec<OrderPaths>,
    healthy: Vec<HealthyOrder>,
    damaged: Vec<DamagedOrder>,
    vendors: Option<Vec<Vendor>>,
}

/// The current candidate at `selected_root`, or why there is none. It loads without
/// rebuilding the index, per `docs/architecture/durable-writes.md`, which makes it the
/// Package-load guard every capability consults.
///
/// A stored index that change evidence proves current supplies the facts, so the load
/// reads it instead of every Order record; any doubt reads the records. What it loads
/// serves a display; a change loads with [`load_package_to_change`].
pub(crate) fn load_package<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, Failure> {
    // Loading the same Package twice in one call, with nothing written between, finds
    // the same Package; the seam remembers it until its next write.
    if refuse_linked_root(selected_root, filesystem).is_ok()
        && let Ok(root) = filesystem.canonicalize(selected_root)
        && let Some(package) = filesystem.remembered_package(&root)
    {
        return Ok(package);
    }
    let cached = PackageOpenFileSystem::new(filesystem);
    let package = match try_open_current_index(selected_root, &cached) {
        Some(package) => package,
        None => load_records(selected_root, &cached)?,
    };
    filesystem.remember_package(&package);
    Ok(package)
}

/// The Package at `selected_root` as a change to the records `changed` names sees it:
/// those records, and the Vendor record where it names that, are read from disk, and
/// the rest come from a stored index that change evidence proves current. A stale index
/// can mislead a display, never a write.
pub(crate) fn load_package_to_change<F: FileSystem>(
    selected_root: &Path,
    changed: &Touched,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, Failure> {
    // A Package this call already loaded, with nothing written since, supplies every
    // record the change does not name.
    if refuse_linked_root(selected_root, filesystem).is_ok()
        && let Ok(root) = filesystem.canonicalize(selected_root)
        && let Some(loaded) = filesystem.remembered_package(&root)
        && let Some(package) = reread(loaded, changed, filesystem)
    {
        filesystem.remember_package(&package);
        return Ok(package);
    }
    let cached = PackageOpenFileSystem::new(filesystem);
    let package = match open_index(selected_root, &cached, changed) {
        Some((package, _)) => package,
        None => load_records(selected_root, &cached)?,
    };
    filesystem.remember_package(&package);
    Ok(package)
}

/// `loaded` with the records `changed` names read again from disk: each a healthy Order
/// `loaded` holds that still reads plainly healthy, and the Vendor record where `changed`
/// names it. `None` wherever anything is in doubt, so the caller loads the Package anew.
fn reread<F: FileSystem>(
    loaded: CurrentPackageCandidate,
    changed: &Touched,
    filesystem: &F,
) -> Option<CurrentPackageCandidate> {
    if loaded.metadata_state != MetadataState::Current {
        return None;
    }
    let root = loaded.context.root.absolute_path.clone();
    let orders_directory = loaded
        .context
        .orders_directory
        .as_ref()?
        .absolute_path
        .clone();
    let named = |path: &Path| changed.orders.contains(path);
    // An Order the load found damaged, or with findings of its own, is read whole.
    if loaded
        .damaged_orders
        .iter()
        .any(|order| named(&order.paths.directory.absolute_path))
        || loaded.diagnostic_groups.iter().any(|group| {
            changed
                .orders
                .iter()
                .any(|directory| group.location.absolute_path.starts_with(directory))
        })
    {
        return None;
    }
    let (vendors, values) = if changed.vendors {
        let document = VendorsDocument::from_json_bytes(
            &filesystem
                .read(&loaded.context.vendors.absolute_path)
                .ok()?,
        )
        .ok()?;
        let normalized = Aggregate::judge(&document).ok()?.into_document();
        (normalized.vendors, normalized.values)
    } else {
        (loaded.vendors, loaded.values)
    };
    let mut healthy_orders = Vec::with_capacity(loaded.healthy_orders.len());
    let mut reread = 0;
    for order in loaded.healthy_orders {
        let directory = &order.paths.directory.absolute_path;
        if !named(directory) {
            healthy_orders.push(order);
            continue;
        }
        healthy_orders.push(touched_order(
            &root,
            &orders_directory,
            directory,
            &vendors,
            filesystem,
        )?);
        reread += 1;
    }
    if reread != changed.orders.len() || !Records::new(Some(&vendors), &healthy_orders, &[]).sound()
    {
        return None;
    }
    sort_healthy_orders(&mut healthy_orders);
    Some(current_package_candidate(
        loaded.context,
        healthy_orders,
        loaded.damaged_orders,
        vendors,
        values,
        loaded.diagnostic_groups,
        loaded.index,
    ))
}

/// The Package at `selected_root` read from every record, for a change that selects the
/// records it rewrites by what they hold.
pub(crate) fn load_every_record<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, Failure> {
    let package = load_records(selected_root, &PackageOpenFileSystem::new(filesystem))?;
    filesystem.remember_package(&package);
    Ok(package)
}

/// The current candidate at `selected_root` read from every canonical record, never from
/// the stored index.
fn load_records<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
) -> Result<CurrentPackageCandidate, Failure> {
    let mut ignored_evidence = ExhaustiveInspectionEvidence::default();
    inspect(
        selected_root,
        filesystem,
        InspectionBreadth::Ordinary,
        &mut ignored_evidence,
    )
}

fn inspect<F: FileSystem>(
    selected_root: &Path,
    filesystem: &F,
    breadth: InspectionBreadth,
    exhaustive_evidence: &mut ExhaustiveInspectionEvidence,
) -> Result<CurrentPackageCandidate, Failure> {
    refuse_linked_root(selected_root, filesystem)?;
    let root = match filesystem.canonicalize(selected_root) {
        Ok(root) => root,
        Err(error) => {
            // The failure is the resolution, so the root is whatever the caller named
            // and nothing more. A relative one has no `Location` at all; reporting it
            // as the path it arrived as is the whole of what this load honestly knows.
            let selected = FailedPath::of(selected_root);
            let root = selected.location().cloned();
            return operation_failed(
                root.as_ref(),
                FailedOperation::filesystem(selected, &error, filesystem),
            );
        }
    };
    let absolute_root = Location::absolute(root.clone()).expect("canonical root is absolute");
    let metadata_path = root.join(".spielorders");
    let metadata_kind = match filesystem.symlink_metadata(&metadata_path) {
        Ok(kind) => kind,
        Err(error) => {
            return operation_failed(
                Some(&absolute_root),
                FailedOperation::filesystem(
                    managed_location(metadata_path, ".spielorders"),
                    &error,
                    filesystem,
                ),
            );
        }
    };
    if metadata_kind.is_none() {
        return Err(Failure::UnmanagedPackage(Box::new(UnmanagedPackage {
            root: absolute_root,
        })));
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
        Err(failure) => return operation_failed(Some(&absolute_root), failure),
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
        Err(failure) => return operation_failed(Some(&absolute_root), failure),
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
        Err(failure) => return operation_failed(Some(&absolute_root), failure),
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
    let _version = version_bytes
        .and_then(|bytes| decode_version(&root, &version_path, &bytes, &mut package_diagnostics));
    let vendors = vendors_bytes
        .and_then(|bytes| decode_vendors(&root, &vendors_path, &bytes, &mut package_diagnostics));
    if breadth == InspectionBreadth::Exhaustive {
        exhaustive_evidence.vendors = vendors.as_ref().map(|document| document.vendors.clone());
    }
    let Some(config) = config else {
        return damaged_without_context(&root, metadata_state, package_diagnostics);
    };
    let tracking_filesystem = InspectionFileSystem::new(filesystem);
    let layout = match resolve_package_layout(&root, &config, &tracking_filesystem) {
        Ok(layout) => layout,
        Err(PackagePathError::FilesystemOperationFailed(_)) => {
            return operation_failed(
                Some(&absolute_root),
                tracking_filesystem
                    .take_failure(&root)
                    .expect("filesystem errors record their attempted operation"),
            );
        }
        Err(PackagePathError::RootNotDirectory) => {
            return operation_failed(
                Some(&absolute_root),
                FailedOperation::unusable_answer(managed_location(root.clone(), ".")),
            );
        }
        Err(error) => {
            package_diagnostics.push(layout_diagnostic(&root, &config, error));
            return damaged_without_context(&root, metadata_state, package_diagnostics);
        }
    };
    let context = managed_context(
        &root,
        &layout.orders_directory,
        &layout.removed_orders_directory,
    );
    let index = PackageIndexRecord {
        location: context.index.clone(),
        state: PackageIndexState::NotAssessed,
        malformed_reasons: Vec::new(),
        stale_reasons: Vec::new(),
    };

    if !package_diagnostics.is_empty() && breadth == InspectionBreadth::Ordinary {
        normalize_diagnostics(&mut package_diagnostics);
        return Err(Failure::damaged(DamagedPackageProjection {
            context,
            metadata_state,
            diagnostic_groups: vec![DiagnosticGroup {
                scope: EvidenceScope::Package,
                location: managed_location(root.clone(), "."),
                diagnostics: package_diagnostics,
            }],
            index,
        }));
    }
    let vendors_are_trustworthy = vendors.is_some();
    let vendors = vendors.unwrap_or(VendorsDocument {
        schema_version: Version1,
        vendors: Vec::new(),
        values: StoredValues::default(),
    });
    let normalized_vendors = match Aggregate::judge(&vendors) {
        Ok(aggregate) => aggregate.into_document(),
        Err(faults) => {
            for fault in faults {
                match fault {
                    AggregateFault::DuplicateVendorId { vendor_indices } => {
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
                    AggregateFault::DuplicateVendorName { vendor_indices } => {
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
                }
            }
            if breadth == InspectionBreadth::Ordinary {
                normalize_diagnostics(&mut package_diagnostics);
                return Err(Failure::damaged(DamagedPackageProjection {
                    context,
                    metadata_state,
                    diagnostic_groups: vec![DiagnosticGroup {
                        scope: EvidenceScope::Package,
                        location: managed_location(root.clone(), "."),
                        diagnostics: package_diagnostics,
                    }],
                    index,
                }));
            }
            vendors.clone()
        }
    };
    let layout = match discover_order_direct_children(layout, &tracking_filesystem) {
        Ok(layout) => layout,
        Err(_) => {
            return operation_failed(
                Some(&context.root),
                tracking_filesystem
                    .take_failure(&root)
                    .expect("directory-list failures record their attempted operation"),
            );
        }
    };
    let known_vendors = vendors_are_trustworthy.then_some(normalized_vendors.vendors.as_slice());
    let mut healthy_orders = Vec::new();
    let mut damaged_orders = Vec::new();
    let mut diagnostic_groups = Vec::new();
    for observation in layout.direct_children {
        if observation.classification != DirectChildClassification::OrderCandidate {
            package_diagnostics.extend(unusable_direct_child_diagnostic(&root, &observation));
            continue;
        }
        let paths = order_paths(&root, &observation.path);
        if breadth == InspectionBreadth::Exhaustive {
            exhaustive_evidence.order_paths.push(paths.clone());
        }
        let validated = match order_health(
            &root,
            &layout.orders_directory,
            &paths,
            known_vendors,
            filesystem,
        ) {
            Ok(OrderHealth::Healthy(validated)) => validated,
            Ok(OrderHealth::Damaged {
                order_id,
                vendor_id,
                diagnostics,
            }) => {
                quarantine_order(
                    paths,
                    order_id,
                    vendor_id,
                    diagnostics,
                    &mut damaged_orders,
                    &mut diagnostic_groups,
                );
                continue;
            }
            Err(failure) => return operation_failed(Some(&context.root), failure),
        };
        let readme_text = if breadth == InspectionBreadth::Ordinary {
            let readme = match read_document(&paths.readme, filesystem) {
                Ok(readme) => readme,
                Err(failure) => return operation_failed(Some(&context.root), failure),
            };
            if !readme.diagnostics.is_empty() {
                diagnostic_groups.push(DiagnosticGroup {
                    scope: EvidenceScope::Document,
                    location: paths.readme.location.clone(),
                    diagnostics: readme.diagnostics.to_vec(),
                });
            }
            // `read_document` already separates absence, which is an empty text, from
            // a document that exists and could not be read, which is no text at all.
            // The projection carries that separation instead of flattening it.
            readme.text.as_deref().map(str::to_owned)
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
    if breadth == InspectionBreadth::Exhaustive {
        exhaustive_evidence.healthy = healthy_orders.clone();
        exhaustive_evidence.damaged = damaged_orders.clone();
    }
    let package_has_errors = package_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    if !package_diagnostics.is_empty() {
        normalize_diagnostics(&mut package_diagnostics);
        diagnostic_groups.push(DiagnosticGroup {
            scope: EvidenceScope::Package,
            location: context.root.clone(),
            diagnostics: package_diagnostics,
        });
        sort_diagnostic_groups(&mut diagnostic_groups);
    }

    if package_has_errors && breadth == InspectionBreadth::Exhaustive {
        return Err(Failure::damaged(DamagedPackageProjection {
            context,
            metadata_state,
            diagnostic_groups,
            index,
        }));
    }

    Ok(current_package_candidate(
        context,
        healthy_orders,
        damaged_orders,
        normalized_vendors.vendors,
        normalized_vendors.values,
        diagnostic_groups,
        index,
    ))
}

/// Which field and which reason one Order-local semantic failure reports.
///
/// **The failure decides both**: matching on the core's own [`OrderValidationFailure`]
/// makes a new failure a compile error here rather than a diagnostic naming the wrong
/// field.
///
/// `validate` returns on the first failure it finds, so the first is the whole answer;
/// the empty case cannot arise, and reporting the date reason for it would be a claim
/// about a record nothing objected to.
fn semantic_evidence(failures: &[OrderValidationFailure]) -> (&'static str, OrderSemanticReason) {
    match failures.first() {
        Some(OrderValidationFailure::NonFiniteTotal { total }) => {
            (total.code(), OrderSemanticReason::NonFiniteTotal)
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
            observation.path.to_path_buf(),
            relative_path(root, &observation.path),
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
        Location::absolute(observation.path.to_path_buf())
            .expect("a direct child of the canonical Orders Directory is absolute"),
        Vec::new(),
        DiagnosticEvidence::OrdersDirectoryEntryNameUnusable { observed_kind },
    )
}

/// What one direct child of the Orders Directory that is not an Order candidate
/// reports, or `None` when it reports nothing.
///
/// Classification determines whether a direct child is ignored, diagnosed, or read
/// as an Order.
fn unusable_direct_child_diagnostic(
    root: &Path,
    observation: &DirectChildObservation,
) -> Option<Diagnostic> {
    match observation.classification {
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

fn current_package_candidate(
    context: ManagedContext,
    healthy_orders: Vec<HealthyOrder>,
    damaged_orders: Vec<DamagedOrder>,
    vendors: Vec<Vendor>,
    values: StoredValues,
    diagnostic_groups: Vec<DiagnosticGroup>,
    index: PackageIndexRecord,
) -> CurrentPackageCandidate {
    let projections =
        crate::values::project(&values, healthy_orders.iter().map(|order| &order.document));
    CurrentPackageCandidate {
        context,
        metadata_version: 1,
        metadata_state: MetadataState::Current,
        healthy_orders,
        damaged_orders,
        vendors,
        values,
        projections,
        diagnostic_groups,
        index,
    }
}

fn stored_order_id(bytes: &[u8]) -> Option<OrderId> {
    let record = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    serde_json::from_value(record.get("order_id")?.clone()).ok()
}

/// What one Order Directory's records make of it, judged by the rules every read of an
/// Order applies.
enum OrderHealth {
    /// The record is present, valid, located in this Package, and names a Vendor that
    /// resolves.
    Healthy(Box<ValidatedOrder>),
    /// The record fails one of those rules; `diagnostics` say which. The identity and the
    /// Vendor are the ones the record can still be trusted to name.
    Damaged {
        order_id: Option<OrderId>,
        vendor_id: Option<VendorId>,
        diagnostics: Vec<Diagnostic>,
    },
}

/// The health of the Order at `paths`, an Order Directory of the Package at `root` whose
/// Orders Directory is `orders_directory`. Its Vendor Reference is judged against
/// `vendors` where those are known; the README is the caller's to read.
fn order_health<F: FileSystem>(
    root: &Path,
    orders_directory: &Path,
    paths: &OrderPaths,
    vendors: Option<&[Vendor]>,
    filesystem: &F,
) -> Result<OrderHealth, FailedOperation> {
    let mut diagnostics = Vec::new();
    let Some(order_bytes) = required_artifact(
        root,
        &paths.order.absolute_path,
        ManagedArtifactKind::Order,
        EvidenceScope::Order,
        filesystem,
        &mut diagnostics,
    )?
    else {
        return Ok(OrderHealth::Damaged {
            order_id: None,
            vendor_id: None,
            diagnostics,
        });
    };
    let recognizable_order_version = recognizable_version(&order_bytes, "schema_version");
    // An identity is trusted only from a record of the version this build reads, so an
    // unsupported record joins no duplicate detection. The Vendor a record names is
    // kept as stored, whatever its version, so no change takes away a Vendor even a
    // record this build cannot read still names.
    let trustworthy_order_id = if recognizable_order_version == Some(1) {
        stored_order_id(&order_bytes)
    } else {
        None
    };
    let order = match OrderDocument::from_json_bytes(&order_bytes) {
        Ok(order) => order,
        Err(error) => {
            if recognizable_order_version.is_some_and(|version| version != 1) {
                diagnostics.push(Diagnostic::managed_artifact_with_details(
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
                    MetadataErrorKind::Encoding => DiagnosticCode::ManagedArtifactInvalidEncoding,
                    MetadataErrorKind::Syntax => DiagnosticCode::ManagedArtifactInvalidSyntax,
                    MetadataErrorKind::Shape | MetadataErrorKind::Serialization => {
                        DiagnosticCode::ManagedArtifactInvalidShape
                    }
                };
                diagnostics.push(Diagnostic::managed_artifact_with_details(
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
            return Ok(OrderHealth::Damaged {
                order_id: trustworthy_order_id,
                vendor_id: named_vendor(&order_bytes),
                diagnostics,
            });
        }
    };
    let validated = match order.validate() {
        Ok(validated) => validated,
        Err(failures) => {
            let (field, reason) = semantic_evidence(&failures);
            diagnostics.push(Diagnostic::new(
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
            return Ok(OrderHealth::Damaged {
                order_id: Some(order.order_id.clone()),
                vendor_id: order.vendor_id.clone(),
                diagnostics,
            });
        }
    };
    let resolved_order_directory = filesystem
        .canonicalize(&paths.directory.absolute_path)
        .map_err(|error| {
            FailedOperation::filesystem(paths.directory.clone(), &error, filesystem)
        })?;
    // The four damages the integrity decision calls repairable by recalculation, asked
    // in one place so that all four earn the one diagnosis that offers that repair. A
    // locator that is not there and one that is not a locator reach it here rather
    // than failing the whole record: see `PackageRootLocator`.
    let locator_reason = if resolved_order_directory.parent() != Some(orders_directory) {
        Some(OrderLocatorReason::NotActiveDirectChild)
    } else {
        match &validated.order().package_root {
            PackageRootLocator::Absent => Some(OrderLocatorReason::Absent),
            PackageRootLocator::Malformed(_) => Some(OrderLocatorReason::Malformed),
            PackageRootLocator::Spelled(locator) => {
                (!locator_resolves_to_root(&resolved_order_directory, locator, root))
                    .then_some(OrderLocatorReason::DoesNotResolveToPackage)
            }
        }
    };
    if let Some(reason) = locator_reason {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::OrderPackageLocatorMismatch,
            Severity::Error,
            EvidenceScope::Order,
            paths.order.clone(),
            vec![managed_location(root.to_path_buf(), ".")],
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
        return Ok(OrderHealth::Damaged {
            order_id: Some(validated.order().order_id.clone()),
            vendor_id: validated.order().vendor_id.clone(),
            diagnostics,
        });
    }
    if let Some(vendors) = vendors
        && let Some(vendor_id) = &validated.order().vendor_id
        && !resolves(vendors, vendor_id)
    {
        diagnostics.push(Diagnostic::new(
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
        return Ok(OrderHealth::Damaged {
            order_id: Some(validated.order().order_id.clone()),
            vendor_id: validated.order().vendor_id.clone(),
            diagnostics,
        });
    }
    Ok(OrderHealth::Healthy(Box::new(validated)))
}

fn quarantine_order(
    paths: OrderPaths,
    order_id: Option<OrderId>,
    vendor_id: Option<VendorId>,
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
    damaged_orders.push(DamagedOrder {
        paths,
        order_id,
        vendor_id,
    });
}

fn quarantine_duplicate_order_ids(
    healthy_orders: &mut Vec<HealthyOrder>,
    damaged_orders: &mut Vec<DamagedOrder>,
    diagnostic_groups: &mut Vec<DiagnosticGroup>,
) {
    let conflicts_by_id = Records::new(None, healthy_orders, damaged_orders).contested();
    if conflicts_by_id.is_empty() {
        return;
    }

    let mut unique_orders = Vec::with_capacity(healthy_orders.len());
    for order in healthy_orders.drain(..) {
        let Some(_conflict_locations) = conflicts_by_id.get(&order.document.order_id) else {
            unique_orders.push(order);
            continue;
        };
        let order_id = order.document.order_id.clone();
        quarantine_order(
            order.paths,
            Some(order_id),
            order.document.vendor_id.clone(),
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
        let Some(conflict_locations) = conflicts_by_id.get(order_id) else {
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
                configuration_path: path.into(),
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
                configuration_path: ConfigurationPathKind::RemovedPath,
                configured_value: config.removed_path.clone(),
                reason: ConfigurationPathReason::NotOrdinary,
            },
        ),
        PackagePathError::RootNotDirectory | PackagePathError::FilesystemOperationFailed(_) => {
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

    fn is_access_denied(&self, error: &Self::Error) -> bool {
        self.inner.is_access_denied(error)
    }

    fn is_unsupported(&self, error: &Self::Error) -> bool {
        self.inner.is_unsupported(error)
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.inner.read(path)
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file(path)
    }

    fn sync_file_contents(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file_contents(path)
    }

    fn remembered_package(&self, root: &Path) -> Option<CurrentPackageCandidate> {
        self.inner.remembered_package(root)
    }

    fn remember_package(&self, package: &CurrentPackageCandidate) {
        self.inner.remember_package(package);
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

    fn entry_name(&self, path: &Path) -> Result<Option<std::ffi::OsString>, Self::Error> {
        self.inner.entry_name(path)
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

    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        self.inner.link_new_file(source, destination)
    }

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error> {
        self.inner.rename_new(source, destination)
    }
}

struct InspectionFileSystem<'a, F> {
    inner: &'a F,
    failure: RefCell<Option<FailedOperation>>,
}

impl<'a, F> InspectionFileSystem<'a, F> {
    fn new(inner: &'a F) -> Self {
        Self {
            inner,
            failure: RefCell::new(None),
        }
    }

    fn record_failure<T>(&self, path: &Path, result: Result<T, F::Error>) -> Result<T, F::Error>
    where
        F: FileSystem,
    {
        if let Err(error) = &result {
            *self.failure.borrow_mut() = Some(FailedOperation::filesystem(
                FailedPath::of(path),
                error,
                self.inner,
            ));
        }
        result
    }

    fn take_failure(&self, root: &Path) -> Option<FailedOperation> {
        let failure = self.failure.borrow_mut().take()?;
        let path = failure.projection().path.as_path().to_path_buf();
        let location = if path == root {
            managed_location(path, ".")
        } else if path.starts_with(root) {
            managed_location(path.clone(), relative_path(root, &path))
        } else {
            Location::absolute(path).expect("filesystem operation location is absolute")
        };
        Some(failure.at(location))
    }
}

impl<F: FileSystem> FileSystem for InspectionFileSystem<'_, F> {
    type Error = F::Error;

    fn is_access_denied(&self, error: &Self::Error) -> bool {
        self.inner.is_access_denied(error)
    }

    fn is_unsupported(&self, error: &Self::Error) -> bool {
        self.inner.is_unsupported(error)
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, Self::Error> {
        self.record_failure(path, self.inner.read(path))
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_new(path, contents)
    }

    fn sync_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file(path)
    }

    fn sync_file_contents(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.sync_file_contents(path)
    }

    fn remembered_package(&self, root: &Path) -> Option<CurrentPackageCandidate> {
        self.inner.remembered_package(root)
    }

    fn remember_package(&self, package: &CurrentPackageCandidate) {
        self.inner.remember_package(package);
    }

    fn replace_file(&self, source: &Path, destination: &Path) -> Result<(), Self::Error> {
        self.inner.replace_file(source, destination)
    }

    fn remove_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        self.record_failure(path, self.inner.canonicalize(path))
    }

    fn entry_name(&self, path: &Path) -> Result<Option<std::ffi::OsString>, Self::Error> {
        self.record_failure(path, self.inner.entry_name(path))
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        self.record_failure(path, self.inner.symlink_metadata(path))
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        self.record_failure(path, self.inner.read_directory(path))
    }

    fn create_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.create_directory(path)
    }

    fn remove_directory(&self, path: &Path) -> Result<(), Self::Error> {
        self.inner.remove_directory(path)
    }

    fn link_new_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<NewFileLinkOutcome, Self::Error> {
        self.inner.link_new_file(source, destination)
    }

    fn rename_new(&self, source: &Path, destination: &Path) -> Result<RenameOutcome, Self::Error> {
        self.inner.rename_new(source, destination)
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
                .package_relative_path
                .clone()
                .unwrap_or_else(|| group.location.absolute_path.to_string_lossy().into_owned());
            match group.scope {
                EvidenceScope::Package => (0, String::new(), 0),
                EvidenceScope::Document if path == "README.md" => (1, String::new(), 0),
                EvidenceScope::Order => (2, path, 0),
                EvidenceScope::Document => {
                    let order_path = path.strip_suffix("/README.md").unwrap_or(&path).to_owned();
                    (2, order_path, 1)
                }
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
    orders.sort_by(|left, right| left.document.listing_order(&right.document));
}

fn required_artifact<F: FileSystem>(
    root: &Path,
    path: &Path,
    artifact: ManagedArtifactKind,
    scope: EvidenceScope,
    filesystem: &F,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Vec<u8>>, FailedOperation> {
    let location = managed_location(path.to_path_buf(), relative_path(root, path));
    // Observing only the leaf follows a linked .spielorders parent. Such a record is
    // not owned by this Package, even when its bytes parse, and must never authorize a
    // mutation through the link.
    if artifact == ManagedArtifactKind::Order
        && let Some(parent) = path.parent()
    {
        match filesystem.symlink_metadata(parent) {
            Ok(Some(kind)) if kind != FileSystemEntryKind::Directory => {
                diagnostics.push(Diagnostic::managed_artifact(
                    DiagnosticCode::ManagedArtifactWrongKind,
                    scope,
                    managed_location(parent.to_path_buf(), relative_path(root, parent)),
                    artifact,
                    Some(kind),
                ));
                return Ok(None);
            }
            Err(error) => {
                return Err(FailedOperation::filesystem(
                    managed_location(parent.to_path_buf(), relative_path(root, parent)),
                    &error,
                    filesystem,
                ));
            }
            _ => {}
        }
    }
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
            Err(error) if filesystem.is_access_denied(&error) => {
                Err(FailedOperation::filesystem(location, &error, filesystem))
            }
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
        Err(error) => Err(FailedOperation::filesystem(location, &error, filesystem)),
    }
}

fn damaged_without_context(
    root: &Path,
    metadata_state: MetadataState,
    mut diagnostics: Vec<Diagnostic>,
) -> Result<CurrentPackageCandidate, Failure> {
    normalize_diagnostics(&mut diagnostics);
    let index_location = managed_location(
        root.join(".spielorders/index.json"),
        ".spielorders/index.json",
    );
    Err(Failure::damaged(DamagedPackageProjection {
        context: managed_root_context(root),
        metadata_state,
        diagnostic_groups: vec![DiagnosticGroup {
            scope: EvidenceScope::Package,
            location: managed_location(root.to_path_buf(), "."),
            diagnostics,
        }],
        index: PackageIndexRecord {
            location: index_location,
            state: PackageIndexState::NotAssessed,
            malformed_reasons: Vec::new(),
            stale_reasons: Vec::new(),
        },
    }))
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
/// A configured managed path reached through a symbolic link resolves to a real name no
/// configured value was checked against, so `resolve_managed_paths` asks the same grammar
/// of the name it resolved to and refuses a Package whose managed directory has no
/// spelling.
fn relative_path(root: &Path, path: &Path) -> String {
    package_relative_path_text(root, path).expect("managed paths have a Package-relative spelling")
}
