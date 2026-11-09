//! Planning and persistence for creating one newest-version Order.

use crate::vendors::vendor_aggregate;
use crate::{
    Answer, Assignments, Failure,
    answer::DraftFailure,
    canonical::{CurrentPackageCandidate, load_package},
    evidence::EvidenceFingerprint,
    metadata::{OrderDocument, OrderItem, OrderStatus, Vendor},
    mutation::{
        CleanupFailure, CommitFailure, IndexRefresh, MutationReport, OperationKind,
        PlannedOperation,
    },
    operations::{FailedOperation, FailedPath},
    order_edits::{NewOrderFacts, draft_item},
    package::{ReusableValue, ReusableValues, canonicalize_reusable_value},
    ports::{FileSystem, LocalDate, NewDirectoryRenameOutcome},
    readme::{Location, package_relative_path_text},
    vendors::vendor_by_name,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_ORDER_ID: &str = crate::mutation::PLANNING_IDENTITY;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderCreationDefaults {
    pub date_ordered: String,
    pub status: OrderStatus,
}

impl OrderCreationDefaults {
    pub(crate) const STATUS: OrderStatus = OrderStatus::InPreparation;

    pub fn new(today: &dyn Fn() -> LocalDate) -> Self {
        let today = today();
        Self {
            date_ordered: format!("{:04}-{:02}-{:02}", today.year, today.month, today.day),
            status: Self::STATUS,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OrderCreationInput {
    title: String,
    date_ordered: Option<String>,
    status: Option<OrderStatus>,
    /// The Vendor Name that selects this Order's Vendor, resolved as every Vendor Name is.
    /// The record stores the Vendor ID the Name resolves to.
    vendor_name: Option<String>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
    items: Vec<OrderItem>,
}

impl OrderCreationInput {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            date_ordered: None,
            status: None,
            vendor_name: None,
            orderer: None,
            ordering_institution: None,
            currency: None,
            project: None,
            order_group: None,
            items: Vec::new(),
        }
    }

    /// The facts one canonical assignment set names, or every reason it named none.
    ///
    /// **Adjudicated here rather than by the caller**, because which fields an Order has
    /// and which of them may be assigned are the Package's own rules — see
    /// [`Assignments`], whose vocabulary this shares with an
    /// edit so the two cannot come to accept different fields. A caller supplying a
    /// `field=value` language therefore refuses nothing itself: an unknown field, an
    /// immutable one, a technical one, a repeated one, and a blank required one are all
    /// reported from here, together.
    ///
    /// A creation's one required fact is the Order Title. Every other fact takes its
    /// established default, and an empty value is simply an absent fact.
    pub fn from_assignments(assignments: &Assignments) -> Result<Self, Failure> {
        let facts = NewOrderFacts::of(assignments)?;
        let [title] = facts.required(["title"])?;
        let mut input = Self::new(title);
        input.date_ordered = facts.optional("date_ordered");
        input.status = facts.status();
        input.vendor_name = facts.optional("vendor");
        input.orderer = facts.optional("orderer");
        input.ordering_institution = facts.optional("ordering_institution");
        input.currency = facts.optional("currency");
        input.project = facts.optional("project");
        input.order_group = facts.optional("order_group");
        Ok(input)
    }

    /// The facts and Items an editor's whole draft of a new Order names, or every rule it
    /// broke, the facts' first and then each row's.
    pub(crate) fn from_draft(
        assignments: &Assignments,
        rows: &[Assignments],
    ) -> Result<Self, Failure> {
        let mut faults = Vec::new();
        let input = Self::from_assignments(assignments)
            .map_err(|failure| faults.push(DraftFailure { row: None, failure }))
            .ok();
        let items = DraftFailure::rows(rows, &mut faults, draft_item);
        match input {
            Some(mut input) if faults.is_empty() => {
                input.items = items;
                Ok(input)
            }
            _ => Err(Failure::InvalidDraft(faults)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrderFacts {
    pub(crate) document: OrderDocument,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationPlan {
    root: PathBuf,
    directory: Location,
    staging_directory: Location,
    pub(crate) facts: PlannedOrderFacts,
    pub(crate) vendor: Option<Vendor>,
    pub(crate) reusable_values: ReusableValues,
    evidence: EvidenceFingerprint,
    pub(crate) operations: Vec<PlannedOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderCreationResult {
    /// The created Order's Directory.
    pub directory: Location,
}

pub(crate) type OrderCreationReport = MutationReport<OrderCreationResult>;

pub(crate) fn plan_order_creation<F: FileSystem>(
    selected_root: &Path,
    input: &OrderCreationInput,
    today: &dyn Fn() -> LocalDate,
    filesystem: &F,
) -> Answer<OrderCreationPlan> {
    Answer::of(|| {
        let package = load_package(selected_root, filesystem)?;
        let root = package.context.root.absolute_path.clone();
        let orders = package
            .context
            .orders_directory
            .as_ref()
            .expect("a current Package has an Orders Directory");
        let orders_relative = orders
            .package_relative_path
            .as_ref()
            .expect("the Orders Directory is inside the Package");
        let package_root_locator =
            std::iter::repeat_n("..", orders_relative.split('/').count() + 1)
                .collect::<Vec<_>>()
                .join("/");
        let date_ordered = input
            .date_ordered
            .as_deref()
            .map(str::trim)
            .map(str::to_owned)
            .unwrap_or_else(|| OrderCreationDefaults::new(today).date_ordered);
        // The Name selects; the record stores what it selected. `vendor_by_name` is the
        // Package's own comparison, shared with every other place a Name resolves, so a Vendor
        // reached here is the one `vendor show` would report for the same text.
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
        let reusable_values = ReusableValues {
            orderer: canonical_reusable(input.orderer.as_deref(), &package.orderers),
            ordering_institution: canonical_reusable(
                input.ordering_institution.as_deref(),
                &package.ordering_institutions,
            ),
            project: canonical_reusable(input.project.as_deref(), &package.projects),
            order_group: canonical_reusable(input.order_group.as_deref(), &package.order_groups),
        };
        let mut candidate = json!({
            "schema_version": 1,
            "package_root": package_root_locator,
            "order_id": PLANNING_ORDER_ID,
            "title": input.title,
            "date_ordered": date_ordered,
            "status": input.status.unwrap_or(OrderCreationDefaults::STATUS),
            "items": input.items,
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
            .ok_or(Failure::InvalidFacts)?;
        let order = candidate.order();
        let title = order.title.as_str().to_owned();
        let title_component = normalize_filename_component(&title);
        if title_component.is_empty() {
            return Err(Failure::EmptyProjectedComponent);
        }
        let basename = projected_order_basename(
            date_ordered.as_str(),
            vendor.map(|vendor| vendor.name.as_str()),
            &title_component,
            order.orderer.as_deref(),
        );
        let directory = orders.absolute_path.join(&basename);
        let staging_directory = orders
            .absolute_path
            .join(format!(".{basename}.spielorders-stage"));
        for path in [&directory, &staging_directory] {
            match filesystem.symlink_metadata(path) {
                Ok(None) => {}
                Ok(found @ Some(_)) => {
                    return Err(Failure::DestinationCollision {
                        root: package.context.root.clone(),
                        location: located(&root, path.to_path_buf()),
                        found,
                    });
                }
                Err(error) => {
                    return Err(Failure::from(
                        FailedOperation::filesystem(
                            FailedPath::inside(&root, path.to_path_buf()),
                            &error,
                            filesystem,
                        )
                        .under(&package.context.root),
                    ));
                }
            }
        }
        let metadata_directory = staging_directory.join(".spielorders");
        let operations = vec![
            operation(
                &root,
                OperationKind::CreateStagingDirectory,
                staging_directory.clone(),
            ),
            operation(
                &root,
                OperationKind::CreateMetadataDirectory,
                metadata_directory.clone(),
            ),
            operation(
                &root,
                OperationKind::WriteOrderRecord,
                metadata_directory.join("order.json"),
            ),
            operation(
                &root,
                OperationKind::WriteReadme,
                staging_directory.join("README.md"),
            ),
            operation(&root, OperationKind::CommitDirectory, directory.clone()),
            operation(
                &root,
                OperationKind::RefreshPackageIndex,
                package.context.index.absolute_path.clone(),
            ),
        ];

        Ok(OrderCreationPlan {
            vendor: vendor.cloned(),
            directory: located(&root, directory),
            staging_directory: located(&root, staging_directory),
            root,
            facts: PlannedOrderFacts {
                document: order.clone(),
            },
            reusable_values,
            evidence: creation_evidence(&package, order),
            operations,
        })
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
    let fingerprint = match package.context.orders_directory.as_ref() {
        Some(orders) => fingerprint.observed(
            "orders_directory",
            orders.absolute_path.as_os_str().as_encoded_bytes(),
        ),
        None => fingerprint.absent("orders_directory"),
    };
    let vendor_name = document
        .vendor_id
        .as_ref()
        .and_then(|vendor_id| {
            package
                .vendors
                .iter()
                .find(|vendor| vendor.vendor_id == *vendor_id)
        })
        .map(|vendor| vendor.name.as_str());
    let mut fingerprint = match vendor_name {
        Some(name) => fingerprint.observed("vendor_name", name),
        None => fingerprint.absent("vendor_name"),
    };
    for (name, proposed, existing) in [
        (
            "orderer",
            document.orderer.as_deref(),
            package.orderers.clone(),
        ),
        (
            "ordering_institution",
            document.ordering_institution.as_deref(),
            package.ordering_institutions.clone(),
        ),
        (
            "project",
            document.project.as_deref(),
            package.projects.clone(),
        ),
        (
            "order_group",
            document.order_group.as_deref(),
            package.order_groups.clone(),
        ),
    ] {
        // The canonical spelling *and* the reuse decision are both reported to the
        // caller, so a value that becomes reusable between planning and the commit
        // changes this fact even though its own text did not.
        fingerprint = match canonical_reusable(proposed, &existing) {
            ReusableValue::Absent => fingerprint.absent(name),
            ReusableValue::Reused(value) => fingerprint.observed(name, format!("reused:{value}")),
            ReusableValue::New(value) => fingerprint.observed(name, format!("new:{value}")),
        };
    }
    fingerprint
}

pub(crate) fn commit_order_creation<F: FileSystem>(
    plan: OrderCreationPlan,
    new_uuid: &dyn Fn() -> String,
    filesystem: &F,
) -> Answer<OrderCreationReport> {
    Answer::of(|| {
        let current = load_package(&plan.root, filesystem)?;
        // Before the destination, because a destination projected from an Orders Directory
        // this Package no longer configures is not the path whose absence means anything.
        let outcome = plan
            .evidence
            .confirm(&creation_evidence(&current, &plan.facts.document));
        if !outcome.is_unchanged() {
            return Err(Failure::PlanEvidenceUnconfirmed { outcome });
        }
        for path in [
            plan.directory.absolute_path.clone(),
            plan.staging_directory.absolute_path.clone(),
        ] {
            match filesystem.symlink_metadata(&path) {
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => {
                    let location = located(&plan.root, path.to_path_buf());
                    return Err(Failure::PreconditionsChanged { location });
                }
            }
        }

        let generated = new_uuid();
        let order = match committed_order_document(&plan, &generated) {
            Some(order) => order,
            None => {
                return Err(Failure::InvalidGeneratedIdentity { generated });
            }
        };
        let collision = current
            .healthy_orders
            .iter()
            .any(|existing| existing.document.order_id.as_str() == generated)
            || current.damaged_orders.iter().any(|existing| {
                existing
                    .order_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == generated)
            });
        if collision {
            return Err(Failure::GeneratedIdentityCollision {
                order_id: generated,
            });
        }
        let candidate = order
            .to_json_bytes()
            .expect("a validated newest-version Order serializes deterministically");
        let metadata_directory = plan.staging_directory.absolute_path.join(".spielorders");
        let order_path = metadata_directory.join("order.json");
        let readme_path = plan.staging_directory.absolute_path.join("README.md");

        if filesystem
            .create_directory(&plan.staging_directory.absolute_path)
            .is_err()
        {
            let failure = CommitFailure::new(plan.staging_directory.clone());
            return Err(rejected(failure, Vec::new()));
        }
        if filesystem.create_directory(&metadata_directory).is_err() {
            return Err(commit_failure_with_cleanup(
                &plan,
                metadata_directory,
                &[],
                filesystem,
            ));
        }
        if filesystem.write_new(&order_path, &candidate).is_err() {
            return Err(commit_failure_with_cleanup(
                &plan,
                order_path,
                &[],
                filesystem,
            ));
        }
        let staged = match filesystem.read(&order_path) {
            Ok(staged) => staged,
            Err(_) => {
                return Err(commit_failure_with_cleanup(
                    &plan,
                    order_path.clone(),
                    std::slice::from_ref(&order_path),
                    filesystem,
                ));
            }
        };
        if staged != candidate || OrderDocument::from_json_bytes(&staged).is_err() {
            let path = order_path.clone();
            return Err(commit_failure_with_cleanup(
                &plan,
                path,
                &[order_path],
                filesystem,
            ));
        }
        if filesystem.sync_file(&order_path).is_err() {
            return Err(commit_failure_with_cleanup(
                &plan,
                order_path.clone(),
                std::slice::from_ref(&order_path),
                filesystem,
            ));
        }
        if filesystem.write_new(&readme_path, b"").is_err() {
            return Err(commit_failure_with_cleanup(
                &plan,
                readme_path,
                std::slice::from_ref(&order_path),
                filesystem,
            ));
        }
        if filesystem.sync_file(&readme_path).is_err() {
            return Err(commit_failure_with_cleanup(
                &plan,
                readme_path.clone(),
                &[readme_path.clone(), order_path.clone()],
                filesystem,
            ));
        }
        match filesystem.rename_new_directory(
            &plan.staging_directory.absolute_path,
            &plan.directory.absolute_path,
        ) {
            Ok(NewDirectoryRenameOutcome::Renamed) => {}
            Ok(NewDirectoryRenameOutcome::DestinationExists) => {
                let path = plan.directory.absolute_path.clone();
                return Err(commit_failure_with_cleanup(
                    &plan,
                    path,
                    &[readme_path, order_path],
                    filesystem,
                ));
            }
            Err(_) => {
                let failure_path = plan.staging_directory.absolute_path.clone();
                return Err(commit_failure_with_cleanup(
                    &plan,
                    failure_path,
                    &[readme_path, order_path],
                    filesystem,
                ));
            }
        }

        let mut refresh = IndexRefresh::after_commit(&plan.root, filesystem);
        // A concurrent external change can remove or damage the Order between the commit
        // and the refresh; the creation still happened, and the refresh cannot confirm it.
        if refresh
            .package()
            .is_some_and(|package| !holds(package, &order))
        {
            refresh.unconfirmed();
        }
        Ok(refresh.report(OrderCreationResult {
            directory: plan.directory.clone(),
        }))
    })
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

/// Whether the refreshed `package` holds the committed `order`.
fn holds(package: &CurrentPackageCandidate, order: &OrderDocument) -> bool {
    package
        .healthy_orders
        .iter()
        .any(|candidate| candidate.document.order_id == order.order_id)
}

fn rejected(failure: CommitFailure, cleanup_failures: Vec<CleanupFailure>) -> Failure {
    Failure::commit(failure, cleanup_failures)
}

fn commit_failure_with_cleanup<F: FileSystem>(
    plan: &OrderCreationPlan,
    path: PathBuf,
    files: &[PathBuf],
    filesystem: &F,
) -> Failure {
    let location = located(&plan.root, path);
    let failure = CommitFailure::new(location);
    let cleanup_failures = cleanup_owned(
        &plan.root,
        &plan.staging_directory.absolute_path,
        files,
        filesystem,
    );
    rejected(failure, cleanup_failures)
}

fn cleanup_owned<F: FileSystem>(
    root: &Path,
    staging_directory: &Path,
    files: &[PathBuf],
    filesystem: &F,
) -> Vec<CleanupFailure> {
    let mut failures = Vec::new();
    for path in files {
        if filesystem.remove_file(path).is_err() {
            failures.push(CleanupFailure::new(located(root, path.clone())));
        }
    }
    for path in [
        staging_directory.join(".spielorders"),
        staging_directory.to_path_buf(),
    ] {
        if filesystem.remove_directory(&path).is_err() {
            failures.push(CleanupFailure::new(located(root, path)));
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
        .map(|item| item.value.as_str())
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

/// One planned operation, at a path this Package can name.
///
/// The Package-relative spelling holds by construction rather than by assertion, as it does
/// for a planned initialization artifact: every path here is the canonical root joined with
/// the configured Orders Directory — which the load has already confirmed has a
/// Package-relative spelling — and then with a projected basename whose components
/// `normalize_filename_component` has already stripped of separators, or with one of this
/// module's own literal managed names.
fn operation(root: &Path, kind: OperationKind, path: PathBuf) -> PlannedOperation {
    PlannedOperation::new(kind, located(root, path))
}

/// One planned path, as this Package locates it.
///
/// The same construction [`operation`] reports a planned artifact through, so a path this
/// creation *refuses* about is spelled the way the same path is spelled when it succeeds.
/// The Package-relative spelling holds for the reason given above, and for the Root itself,
/// which every Package spells `.`.
fn located(root: &Path, path: PathBuf) -> Location {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned creation path has a Package-relative spelling");
    Location::within_package(path, relative).expect("a planned creation path is normalized")
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
