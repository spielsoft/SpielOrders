//! Planning and safe in-place persistence for one existing Order Directory.

use crate::vendors::vendor_aggregate;
use crate::{
    Answer, Assignments, Failure, Warning,
    canonical::{Touched, load_every_record},
    metadata::{OrderDocument, OrderStatus, StoredValues, Vendor},
    mutation::{
        CleanupFailure, CommitFailure, IndexRefresh, MutationReport, OperationKind,
        PlannedOperation,
    },
    operations::{FailedOperation, FailedPath},
    order_edits::{NewOrderFacts, new_order_item},
    package::{ReusableValue, ReusableValues, canonicalize_reusable_value},
    ports::{
        FileChangeEvidence, FileChangeObservation, FileSystem, FileSystemEntryKind,
        NewFileLinkOutcome,
    },
    readme::{Location, located_in_package, package_relative_path_text},
    vendors::vendor_by_name,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_ORDER_ID: &str = crate::mutation::PLANNING_IDENTITY;
const ORDER_STAGING_NAME: &str = ".order.json.spielorders-stage";
const README_STAGING_NAME: &str = ".README.md.spielorders-stage";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OrderItemInput {
    description: String,
    quantity: f64,
    unit_price: f64,
    catalog_number: Option<String>,
    notes: Option<String>,
}

impl OrderItemInput {
    /// The Item one canonical assignment set names, or every reason it names none.
    ///
    /// Adjudicated against the one list of Item fields every Item mutation uses — see
    /// [`new_order_item`].
    pub fn from_assignments(assignments: &Assignments) -> Result<Self, Failure> {
        let item = new_order_item(assignments)?;
        Ok(Self {
            description: item.description.as_str().to_owned(),
            quantity: item.quantity.get(),
            unit_price: item.unit_price.get(),
            catalog_number: item.catalog_number,
            notes: item.notes,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OrderAdoptionInput {
    title: String,
    date_ordered: String,
    status: OrderStatus,
    /// The Vendor Name that selects this Order's Vendor. See
    /// [`crate::creation::OrderCreationInput`], whose field this shares its rule with.
    vendor_name: Option<String>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
    /// The fields whose new value this Order's change adds to the Package.
    introduced: Vec<String>,
    items: Vec<OrderItemInput>,
}

impl OrderAdoptionInput {
    /// The facts one canonical assignment set names, or every reason it named none.
    ///
    /// Adjudicated against the same one list of assignable fields as
    /// [`crate::creation::OrderCreationInput::from_assignments`]. Title, Date Ordered, and
    /// Status are required; nothing parses the candidate's name or contents to propose them.
    pub fn from_assignments(assignments: &Assignments) -> Result<Self, Failure> {
        let facts = NewOrderFacts::of(assignments)?;
        // All three asked for at once, so a command line naming none of them is told about
        // all three rather than made to discover them one round trip at a time.
        let [title, date_ordered, _] = facts.required(["title", "date_ordered", "status"])?;
        let status = facts
            .status()
            .expect("the required Order Status was found above");
        let mut input = Self::new(title, date_ordered, status);
        input.vendor_name = facts.optional("vendor");
        input.orderer = facts.optional("orderer");
        input.ordering_institution = facts.optional("ordering_institution");
        input.currency = facts.optional("currency");
        input.project = facts.optional("project");
        input.order_group = facts.optional("order_group");
        input.introduced = facts.introduced();
        Ok(input)
    }

    pub fn new(
        title: impl Into<String>,
        date_ordered: impl Into<String>,
        status: OrderStatus,
    ) -> Self {
        Self {
            title: title.into(),
            date_ordered: date_ordered.into(),
            status,
            vendor_name: None,
            orderer: None,
            ordering_institution: None,
            currency: None,
            project: None,
            order_group: None,
            introduced: Vec::new(),
            items: Vec::new(),
        }
    }

    pub fn item(mut self, item: OrderItemInput) -> Self {
        self.items.push(item);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionPlan {
    root: PathBuf,
    directory: Location,
    pub(crate) document: OrderDocument,
    pub(crate) vendor: Option<Vendor>,
    pub(crate) reusable_values: ReusableValues,
    /// The new values this change adds to those the Package keeps.
    additions: StoredValues,
    candidate_observation: FileChangeObservation,
    readme_kind: Option<FileSystemEntryKind>,
    pub(crate) operations: Vec<PlannedOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionResult {
    /// The adopted Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderAdoptionReport = MutationReport<OrderAdoptionResult>;

pub(crate) fn plan_order_adoption<F: FileSystem>(
    selected_root: &Path,
    selected_directory: &Path,
    input: &OrderAdoptionInput,
    filesystem: &F,
) -> Answer<OrderAdoptionPlan> {
    Answer::of(|| {
        let package = load_every_record(selected_root, filesystem)?;
        let root_location = package.context.root.clone();
        let root = root_location.absolute_path.clone();
        let orders = package
            .context
            .orders_directory
            .as_ref()
            .expect("a current Package has an Orders Directory");
        if selected_directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            return Err(Failure::HiddenCandidate {
                path: selected_directory.to_path_buf(),
            });
        }
        match observe(&root_location, selected_directory, filesystem)? {
            Some(FileSystemEntryKind::Directory) => {}
            kind => {
                return Err(Failure::IneligibleCandidate {
                    path: selected_directory.to_path_buf(),
                    kind,
                });
            }
        }
        let directory = filesystem
            .canonicalize(selected_directory)
            .map_err(|error| {
                // Resolving the caller's own selected path is the operation that failed, so
                // nothing here located it and no Package spells it.
                Failure::from(
                    FailedOperation::filesystem(
                        FailedPath::of(selected_directory),
                        &error,
                        filesystem,
                    )
                    .under(&root_location),
                )
            })?;
        if directory.parent() != Some(&orders.absolute_path) {
            return Err(Failure::NotDirectChild {
                location: located_in_package(&root, directory),
            });
        }
        // Adoption is the one capability that names an Order Directory instead of
        // discovering it, so the classification that protects every reader does not
        // protect this. Asking the same question here keeps a directory the Package
        // declines to index from being adopted into it.
        if package_relative_path_text(&root, &directory).is_none() {
            return Err(Failure::NameUnusableCandidate {
                location: located_in_package(&root, directory),
            });
        }
        let candidate_observation = filesystem
            .observe_file_change(&directory)
            .map_err(|error| observation_failed(&root_location, &directory, &error, filesystem))?;

        let metadata_directory = directory.join(".spielorders");
        if let Some(kind) = observe(&root_location, &metadata_directory, filesystem)? {
            let order_path = metadata_directory.join("order.json");
            let already_managed = kind == FileSystemEntryKind::Directory
                && observe(&root_location, &order_path, filesystem)?
                    == Some(FileSystemEntryKind::File)
                && filesystem
                    .read(&order_path)
                    .ok()
                    .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
                    .is_some();
            return Err(if already_managed {
                Failure::AlreadyAnOrder {
                    location: located(&root, order_path),
                }
            } else {
                Failure::ConflictingReservedState {
                    location: located(&root, metadata_directory),
                }
            });
        }

        let readme_path = directory.join("README.md");
        let readme_kind = observe(&root_location, &readme_path, filesystem)?;
        let order_staging = metadata_directory.join(ORDER_STAGING_NAME);
        let readme_staging = directory.join(README_STAGING_NAME);
        for path in [&order_staging, &readme_staging] {
            if observe(&root_location, path, filesystem)?.is_some() {
                return Err(Failure::StagingCollision {
                    location: located(&root, path.clone()),
                });
            }
        }

        let orders_relative = orders
            .package_relative_path
            .as_ref()
            .expect("the Orders Directory is inside the Package");
        let package_root_locator =
            std::iter::repeat_n("..", orders_relative.split('/').count() + 1)
                .collect::<Vec<_>>()
                .join("/");
        // The Name selects; the record stores what it selected. See
        // [`crate::creation::plan_order_creation`], which resolves it the same way.
        let vendor = input
            .vendor_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|name| {
                vendor_by_name(name, &package.vendors).ok_or_else(|| Failure::UnknownVendor {
                    name: name.to_owned(),
                    scope: vendor_aggregate(&package),
                })
            })
            .transpose()?;
        let vendor_id = vendor.map(|vendor| vendor.vendor_id.as_str());
        let project = canonical_reusable(input.project.as_deref(), &package.projections.projects);
        let reusable_values = ReusableValues {
            orderer: canonical_reusable(input.orderer.as_deref(), &package.projections.orderers),
            ordering_institution: canonical_reusable(
                input.ordering_institution.as_deref(),
                &package.projections.ordering_institutions,
            ),
            order_group: canonical_reusable(
                input.order_group.as_deref(),
                &package.order_groups_of(reusable_text(&project)),
            ),
            project,
        };
        crate::order_edits::refuse_unconfirmed(&reusable_values, &input.introduced)?;
        let additions = crate::values::additions(&reusable_values);
        let items = input
            .items
            .iter()
            .map(|item| {
                let mut value = json!({
                    "description": item.description,
                    "quantity": item.quantity,
                    "unit_price": item.unit_price,
                });
                insert_optional(
                    value
                        .as_object_mut()
                        .expect("an Order Item candidate is an object"),
                    "catalog_number",
                    item.catalog_number
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty()),
                );
                insert_optional(
                    value
                        .as_object_mut()
                        .expect("an Order Item candidate is an object"),
                    "notes",
                    item.notes
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty()),
                );
                value
            })
            .collect::<Vec<_>>();
        let mut candidate = json!({
            "schema_version": 1,
            "package_root": package_root_locator,
            "order_id": PLANNING_ORDER_ID,
            "title": input.title,
            "date_ordered": input.date_ordered.trim(),
            "status": input.status,
            "items": items,
        });
        let object = candidate
            .as_object_mut()
            .expect("an Order candidate is an object");
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
        let document = serde_json::to_vec(&candidate)
            .ok()
            .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
            .and_then(|order| order.validate().ok())
            .map(|validated| validated.order().clone())
            .ok_or(Failure::InvalidFacts)?;

        let mut operations = vec![
            operation(
                &root,
                OperationKind::CreateMetadataDirectory,
                metadata_directory,
            ),
            operation(
                &root,
                OperationKind::StageOrderRecord,
                order_staging.clone(),
            ),
        ];
        if readme_kind.is_none() {
            operations.extend([
                operation(&root, OperationKind::StageReadme, readme_staging.clone()),
                operation(&root, OperationKind::CommitReadme, readme_path),
            ]);
        }
        operations.push(operation(
            &root,
            OperationKind::CommitOrderRecord,
            directory.join(".spielorders/order.json"),
        ));
        if readme_kind.is_none() {
            operations.push(operation(
                &root,
                OperationKind::RemoveStaging,
                readme_staging,
            ));
        }
        operations.push(operation(
            &root,
            OperationKind::RemoveStaging,
            order_staging,
        ));
        if !additions.is_empty() {
            operations.push(operation(
                &root,
                OperationKind::ReplaceVendorRecord,
                package.context.vendors.absolute_path.clone(),
            ));
        }
        operations.push(operation(
            &root,
            OperationKind::RefreshPackageIndex,
            package.context.index.absolute_path.clone(),
        ));

        Ok(OrderAdoptionPlan {
            vendor: vendor.cloned(),
            directory: located(&root, directory),
            root,
            document,
            reusable_values,
            additions,
            candidate_observation,
            readme_kind,
            operations,
        })
    })
}

pub(crate) fn commit_order_adoption<F: FileSystem>(
    plan: OrderAdoptionPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<OrderAdoptionReport> {
    Answer::of(|| {
        let current = load_every_record(&plan.root, filesystem)?;
        let metadata_directory = plan.directory.absolute_path.join(".spielorders");
        let order_staging = metadata_directory.join(ORDER_STAGING_NAME);
        let order_path = metadata_directory.join("order.json");
        let readme_staging = plan.directory.absolute_path.join(README_STAGING_NAME);
        let readme_path = plan.directory.absolute_path.join("README.md");
        match filesystem.observe_file_change(&plan.directory.absolute_path) {
            Ok(current) if observations_prove_same(plan.candidate_observation, current) => {}
            Ok(_) => {
                let path = plan.directory.absolute_path.clone();
                return Err(preconditions_changed(&plan, path));
            }
            Err(error) => {
                let failed = FailedOperation::inside(
                    &plan.root,
                    plan.directory.absolute_path.clone(),
                    &error,
                    filesystem,
                );
                return Err(failed.into());
            }
        }
        for (path, expected) in [
            (
                plan.directory.absolute_path.clone(),
                Some(FileSystemEntryKind::Directory),
            ),
            (metadata_directory.clone(), None),
            (readme_path.clone(), plan.readme_kind),
            (readme_staging.clone(), None),
        ] {
            match filesystem.symlink_metadata(&path) {
                Ok(observed) if observed == expected => {}
                Ok(_) => return Err(preconditions_changed(&plan, path.to_path_buf())),
                Err(error) => {
                    let failed =
                        FailedOperation::inside(&plan.root, path.to_path_buf(), &error, filesystem);
                    return Err(failed.into());
                }
            }
        }
        if let Some(vendor_id) = plan.document.vendor_id.as_ref()
            && !current
                .vendors
                .iter()
                .any(|vendor| vendor.vendor_id == *vendor_id)
        {
            let path = current.context.vendors.absolute_path.clone();
            return Err(preconditions_changed(&plan, path));
        }
        if reusable_values_for_document(&plan.document, &current) != plan.reusable_values {
            let path = plan.root.clone();
            return Err(preconditions_changed(&plan, path));
        }

        let generated = new_uuid();
        let order = match committed_document(&plan.document, &generated) {
            Some(order) => order,
            None => {
                return Err(Failure::InvalidGeneratedIdentity { generated });
            }
        };
        let collision = current
            .healthy_orders
            .iter()
            .any(|existing| existing.document.order_id == order.order_id)
            || current.damaged_orders.iter().any(|existing| {
                existing
                    .order_id
                    .as_ref()
                    .is_some_and(|id| *id == order.order_id)
            });
        if collision {
            return Err(Failure::GeneratedIdentityCollision {
                order_id: generated,
            });
        }
        let candidate = order
            .to_json_bytes()
            .expect("a validated newest-version Order serializes deterministically");
        let values = crate::values::ValuesWrite::prepare(&current, &plan.additions, filesystem)?;

        if filesystem.create_directory(&metadata_directory).is_err() {
            let failure = CommitFailure::new(located(&plan.root, metadata_directory));
            return Err(rejected(failure, Vec::new()));
        }
        if filesystem.write_new(&order_staging, &candidate).is_err() {
            return Err(rollback_failure(&plan, order_staging, false, filesystem));
        }
        let staged = match filesystem.read(&order_staging) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(rollback_failure(&plan, order_staging, false, filesystem));
            }
        };
        if staged != candidate || OrderDocument::from_json_bytes(&staged).is_err() {
            return Err(rollback_failure(&plan, order_staging, false, filesystem));
        }
        if filesystem.sync_file(&order_staging).is_err() {
            return Err(rollback_failure(&plan, order_staging, false, filesystem));
        }
        let create_readme = plan.readme_kind.is_none();
        if create_readme {
            if filesystem.write_new(&readme_staging, b"").is_err() {
                return Err(rollback_failure(&plan, readme_staging, false, filesystem));
            }
            if filesystem.sync_file(&readme_staging).is_err() {
                return Err(rollback_failure(&plan, readme_staging, false, filesystem));
            }
            match filesystem.link_new_file(&readme_staging, &readme_path) {
                Ok(NewFileLinkOutcome::Linked) => {}
                Ok(NewFileLinkOutcome::DestinationExists) => {
                    return Err(rollback_failure(&plan, readme_path, false, filesystem));
                }
                Err(_) => {
                    return Err(rollback_failure(&plan, readme_path, false, filesystem));
                }
            }
        }
        if let Some(values) = &values
            && let Err(failure) = values.write(filesystem)
        {
            return Err(crate::values::with_cleanup(
                failure,
                cleanup_adoption(&plan, create_readme, filesystem),
                located(&plan.root, values.path().to_path_buf()),
            ));
        }
        match filesystem.link_new_file(&order_staging, &order_path) {
            Ok(NewFileLinkOutcome::Linked) => {}
            Ok(NewFileLinkOutcome::DestinationExists) => {
                let failure = rollback_failure(&plan, order_path, create_readme, filesystem);
                return Err(match &values {
                    Some(values) => values.rollback(failure, filesystem),
                    None => failure,
                });
            }
            Err(_) => {
                let failure = rollback_failure(&plan, order_path, create_readme, filesystem);
                return Err(match &values {
                    Some(values) => values.rollback(failure, filesystem),
                    None => failure,
                });
            }
        }

        let mut warnings = Vec::new();
        for path in [&order_staging, &readme_staging] {
            if let Some(failure) = remove_if_present(&plan.root, path.clone(), filesystem) {
                warnings.push(Warning::StagingCleanupFailed(failure));
            }
        }
        let touched = Touched::orders([plan.directory.absolute_path.clone()])
            .with_vendors(crate::mutation::replaces_vendors(&plan.operations));
        let mut refresh = IndexRefresh::after_commit(&plan.root, &touched, filesystem);
        // A refreshed Package that no longer carries the adopted Order cannot confirm it,
        // and the commit says so instead of silently claiming a confirmation it did not get.
        let confirmed = refresh.package().is_some_and(|package| {
            package
                .healthy_orders
                .iter()
                .any(|candidate| candidate.document.order_id == order.order_id)
        });
        if !confirmed {
            refresh.unconfirmed();
        }
        let result = OrderAdoptionResult {
            directory: plan.directory.clone(),
        };
        warnings.extend(refresh.warning());
        Ok(MutationReport::committed(result, warnings))
    })
}

/// One path inside the candidate, in the Package's own spelling for it.
///
/// The Package-relative spelling holds by construction: every caller is below the check
/// that refuses a candidate whose own name this Package cannot express, and each path is
/// that directory joined with one of this module's literal managed names.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned adoption path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned adoption path is normalized")
}

/// One observation, with the Root this adoption resolved so a failure can name where it
/// happened in the Package's own spelling. The Root is carried rather than derived because
/// the first observation is of the caller's own selected path, which may be outside the
/// Package entirely — and `FailedPath::inside` is what answers both cases.
fn observe<F: FileSystem>(
    root: &Location,
    path: &Path,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, Failure> {
    filesystem
        .symlink_metadata(path)
        .map_err(|error| observation_failed(root, path, &error, filesystem))
}

fn observation_failed<F: FileSystem>(
    root: &Location,
    path: &Path,
    error: &F::Error,
    filesystem: &F,
) -> Failure {
    Failure::from(
        FailedOperation::filesystem(
            FailedPath::inside(&root.absolute_path, path.to_path_buf()),
            error,
            filesystem,
        )
        .under(root),
    )
}

fn observations_prove_same(planned: FileChangeObservation, current: FileChangeObservation) -> bool {
    if planned.kind != Some(FileSystemEntryKind::Directory)
        || current.kind != Some(FileSystemEntryKind::Directory)
    {
        return false;
    }
    matches!(
        (planned.evidence, current.evidence),
        (
            FileChangeEvidence::Comparable {
                comparison_domain: planned_domain,
                sequence: planned_sequence,
            },
            FileChangeEvidence::Comparable {
                comparison_domain: current_domain,
                sequence: current_sequence,
            },
        ) if planned_domain == current_domain && planned_sequence == current_sequence
    )
}

fn canonical_reusable(
    proposed: Option<&str>,
    existing: &[crate::canonical::ReusableValueProjection],
) -> ReusableValue {
    let values = existing
        .iter()
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>();
    canonicalize_reusable_value(proposed, &values)
}

fn reusable_values_for_document(
    document: &OrderDocument,
    package: &crate::canonical::CurrentPackageCandidate,
) -> ReusableValues {
    ReusableValues {
        orderer: canonical_reusable(document.orderer.as_deref(), &package.projections.orderers),
        ordering_institution: canonical_reusable(
            document.ordering_institution.as_deref(),
            &package.projections.ordering_institutions,
        ),
        order_group: canonical_reusable(
            document.order_group.as_deref(),
            &package.order_groups_of(document.project.as_deref()),
        ),
        project: canonical_reusable(document.project.as_deref(), &package.projections.projects),
    }
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
        object.insert(key.into(), serde_json::Value::String(value.into()));
    }
}

/// One planned operation, at a path this Package can name.
///
/// The Package-relative spelling holds by construction: the candidate's own directory is
/// refused outright when its name has none — [`Failure::NameUnusableCandidate`]
/// — and every path here is that directory joined with one of this module's own literal
/// managed names.
fn operation(root: &Path, kind: OperationKind, path: PathBuf) -> PlannedOperation {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned adoption path has a Package-relative spelling");
    PlannedOperation::new(
        kind,
        Location::within_package(path, relative).expect("a planned adoption path is normalized"),
    )
}

fn committed_document(planned: &OrderDocument, order_id: &str) -> Option<OrderDocument> {
    let mut value = serde_json::to_value(planned).ok()?;
    value.as_object_mut()?.insert(
        "order_id".into(),
        serde_json::Value::String(order_id.into()),
    );
    OrderDocument::from_json_bytes(&serde_json::to_vec(&value).ok()?)
        .ok()?
        .validate()
        .ok()
        .map(|validated| validated.order().clone())
}

/// One path this plan named inside its own Package, as that Package locates it.
fn preconditions_changed(plan: &OrderAdoptionPlan, path: PathBuf) -> Failure {
    Failure::PreconditionsChanged {
        location: located(&plan.root, path),
    }
}

fn rejected(failure: CommitFailure, rollback_failures: Vec<CleanupFailure>) -> Failure {
    Failure::commit(failure, rollback_failures)
}

/// A commit that stopped at `path`, after removing what this adoption created.
fn rollback_failure<F: FileSystem>(
    plan: &OrderAdoptionPlan,
    path: PathBuf,
    created_readme: bool,
    filesystem: &F,
) -> Failure {
    let failure = CommitFailure::new(located(&plan.root, path));
    rolled_back(plan, failure, created_readme, filesystem)
}

/// `failure`, after removing what this adoption created before it.
fn rolled_back<F: FileSystem>(
    plan: &OrderAdoptionPlan,
    failure: CommitFailure,
    created_readme: bool,
    filesystem: &F,
) -> Failure {
    rejected(failure, cleanup_adoption(plan, created_readme, filesystem))
}

fn cleanup_adoption<F: FileSystem>(
    plan: &OrderAdoptionPlan,
    created_readme: bool,
    filesystem: &F,
) -> Vec<CleanupFailure> {
    let mut rollback_failures = Vec::new();
    let metadata_directory = plan.directory.absolute_path.join(".spielorders");
    let created_readme = created_readme.then(|| plan.directory.absolute_path.join("README.md"));
    let stages = [
        plan.directory.absolute_path.join(README_STAGING_NAME),
        metadata_directory.join(ORDER_STAGING_NAME),
    ];
    for path in created_readme.into_iter().chain(stages) {
        if let Some(failure) = remove_if_present(&plan.root, path, filesystem) {
            rollback_failures.push(failure);
        }
    }
    if filesystem.remove_directory(&metadata_directory).is_err() {
        rollback_failures.push(CleanupFailure::new(located(&plan.root, metadata_directory)));
    }
    rollback_failures
}

/// Removes the file at `path` where one stands, or says why it could not.
fn remove_if_present<F: FileSystem>(
    root: &Path,
    path: PathBuf,
    filesystem: &F,
) -> Option<CleanupFailure> {
    match filesystem.symlink_metadata(&path) {
        Ok(None) => return None,
        Ok(Some(_)) => {
            filesystem.remove_file(&path).err()?;
        }
        Err(_) => {}
    }
    Some(CleanupFailure::new(located(root, path)))
}
