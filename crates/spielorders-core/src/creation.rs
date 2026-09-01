//! Planning and persistence for creating one newest-version Order.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, HealthyOrder, PackageIndexRecord,
        PackageLoad, inspect_canonical_package, open_package, project_created_order,
    },
    evidence::{EvidenceFingerprint, EvidenceOutcome},
    metadata::{OrderDocument, OrderStatus},
    mutation::MutationReport,
    operations::OperationFailure,
    package::{ReusableValue, canonicalize_reusable_value},
    ports::{
        DirectoryFileSystem, FileSystem, LocalCalendar, NewDirectoryRenameOutcome, UuidGenerator,
    },
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_ORDER_ID: &str = "00000000-0000-4000-8000-000000000000";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderCreationInput {
    title: String,
    date_ordered: Option<String>,
    vendor_id: Option<String>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
}

impl OrderCreationInput {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            date_ordered: None,
            vendor_id: None,
            orderer: None,
            ordering_institution: None,
            currency: None,
            project: None,
            order_group: None,
        }
    }

    pub fn date_ordered(mut self, value: impl Into<String>) -> Self {
        self.date_ordered = Some(value.into());
        self
    }

    pub fn vendor_id(mut self, value: impl Into<String>) -> Self {
        self.vendor_id = Some(value.into());
        self
    }

    pub fn orderer(mut self, value: impl Into<String>) -> Self {
        self.orderer = Some(value.into());
        self
    }

    pub fn ordering_institution(mut self, value: impl Into<String>) -> Self {
        self.ordering_institution = Some(value.into());
        self
    }

    pub fn currency(mut self, value: impl Into<String>) -> Self {
        self.currency = Some(value.into());
        self
    }

    pub fn project(mut self, value: impl Into<String>) -> Self {
        self.project = Some(value.into());
        self
    }

    pub fn order_group(mut self, value: impl Into<String>) -> Self {
        self.order_group = Some(value.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannedOrderIdentity {
    GeneratedAtCommit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrderFacts {
    document: OrderDocument,
}

impl PlannedOrderFacts {
    pub fn title(&self) -> &str {
        self.document.title.as_str()
    }

    pub fn date_ordered(&self) -> &str {
        self.document.date_ordered.as_str()
    }

    pub fn status(&self) -> OrderStatus {
        self.document.status
    }

    pub fn vendor_id(&self) -> Option<&str> {
        self.document.vendor_id.as_ref().map(|value| value.as_str())
    }

    pub fn orderer(&self) -> Option<&str> {
        self.document.orderer.as_deref()
    }

    pub fn ordering_institution(&self) -> Option<&str> {
        self.document.ordering_institution.as_deref()
    }

    pub fn currency(&self) -> Option<&str> {
        self.document.currency.as_ref().map(|value| value.as_str())
    }

    pub fn project(&self) -> Option<&str> {
        self.document.project.as_deref()
    }

    pub fn order_group(&self) -> Option<&str> {
        self.document.order_group.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderCreationReusableValues {
    orderer: ReusableValue,
    ordering_institution: ReusableValue,
    project: ReusableValue,
    order_group: ReusableValue,
}

impl OrderCreationReusableValues {
    pub fn orderer(&self) -> &ReusableValue {
        &self.orderer
    }

    pub fn ordering_institution(&self) -> &ReusableValue {
        &self.ordering_institution
    }

    pub fn project(&self) -> &ReusableValue {
        &self.project
    }

    pub fn order_group(&self) -> &ReusableValue {
        &self.order_group
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderCreationOperationKind {
    CreateStagingDirectory,
    CreateMetadataDirectory,
    WriteOrderRecord,
    WriteReadme,
    CommitDirectory,
    RefreshPackageIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderCreationOperation {
    kind: OrderCreationOperationKind,
    path: PathBuf,
}

impl OrderCreationOperation {
    pub fn kind(&self) -> OrderCreationOperationKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationPlan {
    root: PathBuf,
    directory: PathBuf,
    staging_directory: PathBuf,
    package_root_locator: String,
    facts: PlannedOrderFacts,
    reusable_values: OrderCreationReusableValues,
    prior_index: PackageIndexRecord,
    evidence: EvidenceFingerprint,
    operations: Vec<OrderCreationOperation>,
}

impl OrderCreationPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn staging_directory(&self) -> &Path {
        &self.staging_directory
    }

    pub fn package_root_locator(&self) -> &str {
        &self.package_root_locator
    }

    pub fn identity(&self) -> PlannedOrderIdentity {
        PlannedOrderIdentity::GeneratedAtCommit
    }

    pub fn facts(&self) -> &PlannedOrderFacts {
        &self.facts
    }

    pub fn reusable_values(&self) -> &OrderCreationReusableValues {
        &self.reusable_values
    }

    pub fn operations(&self) -> &[OrderCreationOperation] {
        &self.operations
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderCreationPlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    InvalidFacts,
    UnknownVendor { vendor_id: String },
    EmptyProjectedComponent,
    DestinationCollision { path: PathBuf },
    FileSystem { path: PathBuf, error: E },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalOrderOutcome {
    Created,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationResult {
    canonical_outcome: CanonicalOrderOutcome,
    order: HealthyOrder,
    index: PackageIndexRecord,
}

impl OrderCreationResult {
    pub fn canonical_outcome(&self) -> CanonicalOrderOutcome {
        self.canonical_outcome
    }

    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderCreationCommitPhase {
    ObservePreconditions,
    CreateStagingDirectory,
    CreateMetadataDirectory,
    WriteOrderRecord,
    ReadOrderRecord,
    ValidateOrderRecord,
    SyncOrderRecord,
    WriteReadme,
    SyncReadme,
    CommitDirectory,
}

#[derive(Debug, PartialEq)]
pub struct OrderCreationCommitFailure<E> {
    phase: OrderCreationCommitPhase,
    path: PathBuf,
    cause: OrderCreationCommitFailureCause<E>,
}

#[derive(Debug, PartialEq)]
pub enum OrderCreationCommitFailureCause<E> {
    FileSystem(E),
    CandidateChanged,
    DestinationChanged,
}

impl<E> OrderCreationCommitFailure<E> {
    pub fn phase(&self) -> OrderCreationCommitPhase {
        self.phase
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cause(&self) -> &OrderCreationCommitFailureCause<E> {
        &self.cause
    }
}

#[derive(Debug, PartialEq)]
pub struct OrderCreationCleanupFailure<E> {
    path: PathBuf,
    error: E,
}

impl<E> OrderCreationCleanupFailure<E> {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn error(&self) -> &E {
        &self.error
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderCreationRejection<E> {
    PreconditionsChanged {
        path: PathBuf,
    },
    /// The facts the plan was built from no longer stand, so the plan is no longer
    /// authorized. The seam's own answer is carried rather than re-encoded, so a
    /// caller can say which fact moved; it is only ever constructed from an outcome
    /// that was not [`EvidenceOutcome::Unchanged`].
    PlanEvidenceUnconfirmed {
        outcome: EvidenceOutcome,
    },
    InvalidGeneratedIdentity {
        generated: String,
    },
    GeneratedIdentityCollision {
        order_id: String,
    },
    CommitFailed {
        failure: OrderCreationCommitFailure<E>,
        cleanup_failures: Vec<OrderCreationCleanupFailure<E>>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderCreationWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

pub type OrderCreationReport<E> = MutationReport<
    OrderCreationPlan,
    OrderCreationResult,
    OrderCreationRejection<E>,
    OrderCreationWarning,
>;

pub fn plan_order_creation<F: FileSystem>(
    selected_root: &Path,
    input: &OrderCreationInput,
    calendar: &impl LocalCalendar,
    filesystem: &F,
) -> Result<OrderCreationPlan, OrderCreationPlanError<F::Error>> {
    let package = match inspect_canonical_package(selected_root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        CanonicalPackageLoad::Unmanaged(_) => {
            return Err(OrderCreationPlanError::UnmanagedPackage);
        }
        CanonicalPackageLoad::DiagnosticOnly(_) => {
            return Err(OrderCreationPlanError::DamagedPackage);
        }
        CanonicalPackageLoad::UpgradePending(_) => {
            return Err(OrderCreationPlanError::UpgradePending);
        }
        CanonicalPackageLoad::OperationFailed(_) => {
            return Err(OrderCreationPlanError::PackageOperationFailed);
        }
    };
    let root = package.context().root().absolute_path().to_path_buf();
    let orders = package
        .context()
        .orders_directory()
        .expect("a current Package has an Orders Directory");
    let orders_relative = orders
        .package_relative_path()
        .expect("the Orders Directory is inside the Package");
    let package_root_locator = std::iter::repeat_n("..", orders_relative.split('/').count() + 1)
        .collect::<Vec<_>>()
        .join("/");
    let date_ordered = input
        .date_ordered
        .as_deref()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let today = calendar.today();
            format!("{:04}-{:02}-{:02}", today.year, today.month, today.day)
        });
    let vendor_id = input
        .vendor_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let vendor = vendor_id
        .map(|vendor_id| {
            package
                .vendors()
                .iter()
                .find(|vendor| vendor.vendor_id.as_str() == vendor_id)
                .ok_or_else(|| OrderCreationPlanError::UnknownVendor {
                    vendor_id: vendor_id.to_owned(),
                })
        })
        .transpose()?;
    let reusable_values = OrderCreationReusableValues {
        orderer: canonical_reusable(input.orderer.as_deref(), package.orderers()),
        ordering_institution: canonical_reusable(
            input.ordering_institution.as_deref(),
            package.ordering_institutions(),
        ),
        project: canonical_reusable(input.project.as_deref(), package.projects()),
        order_group: canonical_reusable(input.order_group.as_deref(), package.order_groups()),
    };
    let mut candidate = json!({
        "schema_version": 1,
        "package_root": package_root_locator,
        "order_id": PLANNING_ORDER_ID,
        "title": input.title,
        "date_ordered": date_ordered,
        "status": "in_preparation",
        "items": [],
    });
    let object = candidate
        .as_object_mut()
        .expect("the planned Order candidate is an object");
    insert_optional(object, "vendor_id", vendor_id);
    insert_optional(object, "orderer", reusable_text(&reusable_values.orderer));
    insert_optional(
        object,
        "ordering_institution",
        reusable_text(&reusable_values.ordering_institution),
    );
    insert_optional(
        object,
        "currency",
        input
            .currency
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    );
    insert_optional(object, "project", reusable_text(&reusable_values.project));
    insert_optional(
        object,
        "order_group",
        reusable_text(&reusable_values.order_group),
    );
    let candidate = serde_json::to_vec(&candidate)
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .and_then(|order| order.validate().ok())
        .ok_or(OrderCreationPlanError::InvalidFacts)?;
    let order = candidate.order();
    let title = order.title.as_str().to_owned();
    let title_component = normalize_filename_component(&title);
    if title_component.is_empty() {
        return Err(OrderCreationPlanError::EmptyProjectedComponent);
    }
    let basename = projected_order_basename(
        date_ordered.as_str(),
        vendor.map(|vendor| vendor.name.as_str()),
        &title_component,
        order.orderer.as_deref(),
    );
    let directory = orders.absolute_path().join(&basename);
    let staging_directory = orders
        .absolute_path()
        .join(format!(".{basename}.spielorders-stage"));
    for path in [&directory, &staging_directory] {
        match filesystem.symlink_metadata(path) {
            Ok(None) => {}
            Ok(Some(_)) => {
                return Err(OrderCreationPlanError::DestinationCollision {
                    path: path.to_path_buf(),
                });
            }
            Err(error) => {
                return Err(OrderCreationPlanError::FileSystem {
                    path: path.to_path_buf(),
                    error,
                });
            }
        }
    }
    let metadata_directory = staging_directory.join(".spielorders");
    let operations = vec![
        operation(
            OrderCreationOperationKind::CreateStagingDirectory,
            staging_directory.clone(),
        ),
        operation(
            OrderCreationOperationKind::CreateMetadataDirectory,
            metadata_directory.clone(),
        ),
        operation(
            OrderCreationOperationKind::WriteOrderRecord,
            metadata_directory.join("order.json"),
        ),
        operation(
            OrderCreationOperationKind::WriteReadme,
            staging_directory.join("README.md"),
        ),
        operation(
            OrderCreationOperationKind::CommitDirectory,
            directory.clone(),
        ),
        operation(
            OrderCreationOperationKind::RefreshPackageIndex,
            package.context().index().absolute_path().to_path_buf(),
        ),
    ];

    Ok(OrderCreationPlan {
        root,
        directory,
        staging_directory,
        package_root_locator,
        facts: PlannedOrderFacts {
            document: order.clone(),
        },
        reusable_values,
        prior_index: package.index().clone(),
        evidence: creation_evidence(&package, order),
        operations,
    })
}

/// The facts an Order-creation plan is authorized against, as they stand in `package`.
///
/// The destination's absence is not enough. The plan projects its directory under the
/// *configured* Orders Directory, spells that directory's basename with the live
/// Vendor's *name*, and reports each reusable value as reused or new against the
/// values the Package already holds. Each of those can move between planning and the
/// commit while the destination stays absent, so each is recorded here and re-observed
/// through [`EvidenceFingerprint::confirm`] before the first effect.
fn creation_evidence(
    package: &CurrentPackageCandidate,
    document: &OrderDocument,
) -> EvidenceFingerprint {
    let fingerprint = EvidenceFingerprint::new();
    let fingerprint = match package.context().orders_directory() {
        Some(orders) => fingerprint.observed(
            "orders_directory",
            orders.absolute_path().as_os_str().as_encoded_bytes(),
        ),
        None => fingerprint.absent("orders_directory"),
    };
    let vendor_name = document
        .vendor_id
        .as_ref()
        .and_then(|vendor_id| {
            package
                .vendors()
                .iter()
                .find(|vendor| vendor.vendor_id == *vendor_id)
        })
        .map(|vendor| vendor.name.as_str());
    let mut fingerprint = match vendor_name {
        Some(name) => fingerprint.observed("vendor_name", name),
        None => fingerprint.absent("vendor_name"),
    };
    for (name, proposed, existing) in [
        ("orderer", document.orderer.as_deref(), package.orderers()),
        (
            "ordering_institution",
            document.ordering_institution.as_deref(),
            package.ordering_institutions(),
        ),
        ("project", document.project.as_deref(), package.projects()),
        (
            "order_group",
            document.order_group.as_deref(),
            package.order_groups(),
        ),
    ] {
        // The canonical spelling *and* the reuse decision are both reported to the
        // caller, so a value that becomes reusable between planning and the commit
        // changes this fact even though its own text did not.
        fingerprint = match canonical_reusable(proposed, existing) {
            ReusableValue::Absent => fingerprint.absent(name),
            ReusableValue::Reused(value) => fingerprint.observed(name, format!("reused:{value}")),
            ReusableValue::New(value) => fingerprint.observed(name, format!("new:{value}")),
        };
    }
    fingerprint
}

pub fn commit_order_creation<F: DirectoryFileSystem>(
    plan: OrderCreationPlan,
    uuid_generator: &mut impl UuidGenerator,
    filesystem: &F,
) -> OrderCreationReport<F::Error> {
    let current = match inspect_canonical_package(&plan.root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        _ => {
            let path = plan.root.clone();
            return MutationReport::Rejected {
                plan,
                rejection: OrderCreationRejection::PreconditionsChanged { path },
            };
        }
    };
    // Before the destination, because a destination projected from an Orders Directory
    // this Package no longer configures is not the path whose absence means anything.
    let outcome = plan
        .evidence
        .confirm(&creation_evidence(&current, &plan.facts.document));
    if !outcome.is_unchanged() {
        return MutationReport::Rejected {
            plan,
            rejection: OrderCreationRejection::PlanEvidenceUnconfirmed { outcome },
        };
    }
    for path in [plan.directory.clone(), plan.staging_directory.clone()] {
        match filesystem.symlink_metadata(&path) {
            Ok(None) => {}
            Ok(Some(_)) | Err(_) => {
                return MutationReport::Rejected {
                    plan,
                    rejection: OrderCreationRejection::PreconditionsChanged { path },
                };
            }
        }
    }

    let generated = uuid_generator.generate_v4();
    let order = match committed_order_document(&plan, &generated) {
        Some(order) => order,
        None => {
            return MutationReport::Rejected {
                plan,
                rejection: OrderCreationRejection::InvalidGeneratedIdentity { generated },
            };
        }
    };
    let collision = current
        .healthy_orders()
        .iter()
        .any(|existing| existing.document().order_id.as_str() == generated)
        || current.damaged_orders().iter().any(|existing| {
            existing
                .order_id()
                .is_some_and(|id| id.as_str() == generated)
        });
    if collision {
        return MutationReport::Rejected {
            plan,
            rejection: OrderCreationRejection::GeneratedIdentityCollision {
                order_id: generated,
            },
        };
    }
    let candidate = order
        .to_json_bytes()
        .expect("a validated newest-version Order serializes deterministically");
    let metadata_directory = plan.staging_directory.join(".spielorders");
    let order_path = metadata_directory.join("order.json");
    let readme_path = plan.staging_directory.join("README.md");

    if let Err(error) = filesystem.create_directory(&plan.staging_directory) {
        return commit_failure(
            plan,
            OrderCreationCommitPhase::CreateStagingDirectory,
            error,
            Vec::new(),
        );
    }
    if let Err(error) = filesystem.create_directory(&metadata_directory) {
        return commit_failure_with_cleanup(
            plan,
            OrderCreationCommitPhase::CreateMetadataDirectory,
            metadata_directory,
            error,
            &[],
            filesystem,
        );
    }
    if let Err(error) = filesystem.write_new(&order_path, &candidate) {
        return commit_failure_with_cleanup(
            plan,
            OrderCreationCommitPhase::WriteOrderRecord,
            order_path,
            error,
            &[],
            filesystem,
        );
    }
    let staged = match filesystem.read(&order_path) {
        Ok(staged) => staged,
        Err(error) => {
            return commit_failure_with_cleanup(
                plan,
                OrderCreationCommitPhase::ReadOrderRecord,
                order_path.clone(),
                error,
                std::slice::from_ref(&order_path),
                filesystem,
            );
        }
    };
    if staged != candidate || OrderDocument::from_json_bytes(&staged).is_err() {
        let path = order_path.clone();
        return commit_failure_cause_with_cleanup(
            plan,
            OrderCreationCommitPhase::ValidateOrderRecord,
            path,
            OrderCreationCommitFailureCause::CandidateChanged,
            &[order_path],
            filesystem,
        );
    }
    if let Err(error) = filesystem.sync_file(&order_path) {
        return commit_failure_with_cleanup(
            plan,
            OrderCreationCommitPhase::SyncOrderRecord,
            order_path.clone(),
            error,
            std::slice::from_ref(&order_path),
            filesystem,
        );
    }
    if let Err(error) = filesystem.write_new(&readme_path, b"") {
        return commit_failure_with_cleanup(
            plan,
            OrderCreationCommitPhase::WriteReadme,
            readme_path,
            error,
            std::slice::from_ref(&order_path),
            filesystem,
        );
    }
    if let Err(error) = filesystem.sync_file(&readme_path) {
        return commit_failure_with_cleanup(
            plan,
            OrderCreationCommitPhase::SyncReadme,
            readme_path.clone(),
            error,
            &[readme_path.clone(), order_path.clone()],
            filesystem,
        );
    }
    match filesystem.rename_new_directory(&plan.staging_directory, &plan.directory) {
        Ok(NewDirectoryRenameOutcome::Renamed) => {}
        Ok(NewDirectoryRenameOutcome::DestinationExists) => {
            let path = plan.directory.clone();
            return commit_failure_cause_with_cleanup(
                plan,
                OrderCreationCommitPhase::CommitDirectory,
                path,
                OrderCreationCommitFailureCause::DestinationChanged,
                &[readme_path, order_path],
                filesystem,
            );
        }
        Err(error) => {
            let failure_path = plan.staging_directory.clone();
            return commit_failure_with_cleanup(
                plan,
                OrderCreationCommitPhase::CommitDirectory,
                failure_path,
                error,
                &[readme_path, order_path],
                filesystem,
            );
        }
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => match result_from_package(&order, &package) {
            Some(result) => MutationReport::Committed { plan, result },
            None => {
                // The refresh ran; it simply cannot see this Order. Its own assessment
                // of the derived index is therefore the true one, and only the Order
                // comes from this commit's projection.
                let index = package.index().clone();
                let result = result_from_commit(&plan, &order, index);
                MutationReport::CommittedWithWarning {
                    plan,
                    result,
                    warnings: vec![OrderCreationWarning::RefreshUnavailable],
                }
            }
        },
        PackageLoad::IndexReplacementFailed(failed) => {
            let warning = OrderCreationWarning::IndexRefreshFailed(failed.failure().clone());
            let result =
                result_from_package(&order, failed.canonical_evidence()).unwrap_or_else(|| {
                    result_from_commit(&plan, &order, failed.canonical_evidence().index().clone())
                });
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        PackageLoad::OperationFailed(failed) => {
            // No refresh assessed anything, so the only honest index evidence is the
            // one this commit's own membership change makes of the record it planned
            // against.
            let result = result_from_commit(
                &plan,
                &order,
                plan.prior_index
                    .after_committed_membership_change_without_refresh(),
            );
            let warning = OrderCreationWarning::IndexRefreshFailed(failed.failure().clone());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        _ => {
            // An unmanaged, damaged, or upgrade-pending load carries an index record,
            // but no rebuild was attempted over it, so it cannot describe a Package
            // this commit has just added an Order to.
            let result = result_from_commit(
                &plan,
                &order,
                plan.prior_index
                    .after_committed_membership_change_without_refresh(),
            );
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![OrderCreationWarning::RefreshUnavailable],
            }
        }
    }
}

fn committed_order_document(plan: &OrderCreationPlan, order_id: &str) -> Option<OrderDocument> {
    let mut value = serde_json::to_value(&plan.facts.document).ok()?;
    value.as_object_mut()?.insert(
        "order_id".into(),
        serde_json::Value::String(order_id.into()),
    );
    let bytes = serde_json::to_vec(&value).ok()?;
    OrderDocument::from_json_bytes(&bytes)
        .ok()?
        .validate()
        .ok()
        .map(|validated| validated.order().clone())
}

/// The created Order as the refreshed Package sees it, when it can still see it. A
/// concurrent external change can remove or damage the Order between the commit and
/// the refresh; the creation still happened, so that is reported as a committed
/// result the caller can trust rather than raised as a panic.
fn result_from_package(
    order: &OrderDocument,
    package: &CurrentPackageCandidate,
) -> Option<OrderCreationResult> {
    let refreshed = package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.document().order_id == order.order_id)?;
    Some(OrderCreationResult {
        canonical_outcome: CanonicalOrderOutcome::Created,
        order: refreshed.clone(),
        index: package.index().clone(),
    })
}

/// The created Order as this commit itself left it, for every arm where the refreshed
/// Package cannot supply it.
///
/// The index is a parameter rather than derived here. A refresh that ran and could not
/// see this Order, a refresh whose replacement failed, and a refresh that never
/// happened are three different facts about the derived index, and the caller is the
/// only one that knows which of them it is looking at.
fn result_from_commit(
    plan: &OrderCreationPlan,
    order: &OrderDocument,
    index: PackageIndexRecord,
) -> OrderCreationResult {
    OrderCreationResult {
        canonical_outcome: CanonicalOrderOutcome::Created,
        order: project_created_order(&plan.root, &plan.directory, order),
        index,
    }
}

fn commit_failure<E>(
    plan: OrderCreationPlan,
    phase: OrderCreationCommitPhase,
    error: E,
    cleanup_failures: Vec<OrderCreationCleanupFailure<E>>,
) -> OrderCreationReport<E> {
    let path = plan.staging_directory.clone();
    MutationReport::Rejected {
        plan,
        rejection: OrderCreationRejection::CommitFailed {
            failure: OrderCreationCommitFailure {
                phase,
                path,
                cause: OrderCreationCommitFailureCause::FileSystem(error),
            },
            cleanup_failures,
        },
    }
}

fn commit_failure_with_cleanup<F: DirectoryFileSystem>(
    plan: OrderCreationPlan,
    phase: OrderCreationCommitPhase,
    path: PathBuf,
    error: F::Error,
    files: &[PathBuf],
    filesystem: &F,
) -> OrderCreationReport<F::Error> {
    commit_failure_cause_with_cleanup(
        plan,
        phase,
        path,
        OrderCreationCommitFailureCause::FileSystem(error),
        files,
        filesystem,
    )
}

fn commit_failure_cause_with_cleanup<F: DirectoryFileSystem>(
    plan: OrderCreationPlan,
    phase: OrderCreationCommitPhase,
    path: PathBuf,
    cause: OrderCreationCommitFailureCause<F::Error>,
    files: &[PathBuf],
    filesystem: &F,
) -> OrderCreationReport<F::Error> {
    let cleanup_failures = cleanup_owned(&plan.staging_directory, files, filesystem);
    MutationReport::Rejected {
        plan,
        rejection: OrderCreationRejection::CommitFailed {
            failure: OrderCreationCommitFailure { phase, path, cause },
            cleanup_failures,
        },
    }
}

fn cleanup_owned<F: DirectoryFileSystem>(
    staging_directory: &Path,
    files: &[PathBuf],
    filesystem: &F,
) -> Vec<OrderCreationCleanupFailure<F::Error>> {
    let mut failures = Vec::new();
    for path in files {
        if let Err(error) = filesystem.remove_file(path) {
            failures.push(OrderCreationCleanupFailure {
                path: path.clone(),
                error,
            });
        }
    }
    for path in [
        staging_directory.join(".spielorders"),
        staging_directory.to_path_buf(),
    ] {
        if let Err(error) = filesystem.remove_directory(&path) {
            failures.push(OrderCreationCleanupFailure { path, error });
        }
    }
    failures
}

fn canonical_reusable(
    proposed: Option<&str>,
    existing: &[crate::canonical::ReusableValueProjection],
) -> ReusableValue {
    let values = existing
        .iter()
        .map(crate::canonical::ReusableValueProjection::value)
        .collect::<Vec<_>>();
    canonicalize_reusable_value(proposed, &values)
}

fn reusable_text(value: &ReusableValue) -> Option<&str> {
    match value {
        ReusableValue::Absent => None,
        ReusableValue::Reused(value) | ReusableValue::New(value) => Some(value),
    }
}

fn insert_optional(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        object.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
    }
}

fn operation(kind: OrderCreationOperationKind, path: PathBuf) -> OrderCreationOperation {
    OrderCreationOperation { kind, path }
}

/// The one decided aligned Order Directory basename,
/// `YYYY_MM_DD[_Vendor]_Title[_Orderer]`. The Vendor and Orderer components are
/// omitted when absent rather than emitted empty. Creation and explicit alignment
/// share this projection so the two can never disagree about a directory's name.
pub(crate) fn projected_order_basename(
    date_ordered: &str,
    vendor_name: Option<&str>,
    title_component: &str,
    orderer: Option<&str>,
) -> String {
    let mut components = vec![date_ordered.replace('-', "_")];
    if let Some(vendor_name) = vendor_name {
        components.push(normalize_filename_component(vendor_name));
    }
    components.push(title_component.to_owned());
    if let Some(orderer) = orderer {
        components.push(normalize_filename_component(orderer));
    }
    components.join("_")
}

pub(crate) fn normalize_filename_component(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_underscore = false;
    for character in value.trim().chars() {
        let replace = character.is_whitespace()
            || character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            );
        if replace || character == '_' {
            if !last_was_underscore {
                normalized.push('_');
                last_was_underscore = true;
            }
        } else {
            normalized.push(character);
            last_was_underscore = false;
        }
    }
    normalized
}
