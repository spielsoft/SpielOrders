//! Planning and persistence for the Order record's own mutations: one atomic
//! scalar-fact assignment set, and the ordered embedded Order Item collection.

use crate::{
    canonical::{
        CanonicalPackageLoad, CurrentPackageCandidate, HealthyOrder, PackageIndexRecord,
        PackageLoad, ReusableValueProjection, inspect_canonical_package, open_package,
        project_committed_order,
    },
    filesystem::{
        DestinationExpectation, ValidatedReplacementError, replace_validated_file_expecting,
    },
    metadata::{OrderDocument, OrderItem, OrderStatus},
    mutation::{MutationReport, RejectedAssignment, RejectedAssignmentReason},
    operations::OperationFailure,
    package::{ReusableValue, canonicalize_reusable_value},
    ports::{FileSystem, FileSystemEntryKind, LocalCalendar},
};
use std::path::{Path, PathBuf};

const REUSABLE_FIELDS: [&str; 4] = ["orderer", "ordering_institution", "project", "order_group"];

const ASSIGNABLE_FIELDS: [&str; 9] = [
    "title",
    "date_ordered",
    "status",
    "vendor_id",
    "orderer",
    "ordering_institution",
    "currency",
    "project",
    "order_group",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssignmentIntent {
    Assign,
    Introduce,
}

/// One order-independent atomic assignment set for an Order's mutable scalar facts.
/// A genuinely new Reusable Free-Entry Value requires `introduce` as its explicit
/// confirmation; `assign` alone reuses an existing canonical spelling or clears.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrderAssignments {
    entries: Vec<(String, String, AssignmentIntent)>,
}

impl OrderAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries
            .push((field.into(), value.into(), AssignmentIntent::Assign));
        self
    }

    pub fn introduce(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries
            .push((field.into(), value.into(), AssignmentIntent::Introduce));
        self
    }
}

/// Where the committed Date Ordered came from, making the `ordered` entry rule
/// observable without inspecting the stored value alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateOrderedSource {
    Retained,
    Assigned,
    AppliedOnEnteringOrdered,
}

impl DateOrderedSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retained => "retained",
            Self::Assigned => "assigned",
            Self::AppliedOnEnteringOrdered => "applied_on_entering_ordered",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrderFacts {
    document: OrderDocument,
    item_totals: Vec<f64>,
    order_total: f64,
}

impl PlannedOrderFacts {
    pub fn order_id(&self) -> &str {
        self.document.order_id.as_str()
    }

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

    /// The complete embedded Item collection in its exact user-controlled order.
    pub fn items(&self) -> &[OrderItem] {
        &self.document.items
    }

    pub fn item_totals(&self) -> &[f64] {
        &self.item_totals
    }

    pub fn order_total(&self) -> f64 {
        self.order_total
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderEditReusableValues {
    orderer: ReusableValue,
    ordering_institution: ReusableValue,
    project: ReusableValue,
    order_group: ReusableValue,
}

impl OrderEditReusableValues {
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
pub enum OrderEditOperationKind {
    ReplaceOrderRecord,
    RefreshPackageIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderEditOperation {
    kind: OrderEditOperationKind,
    path: PathBuf,
}

impl OrderEditOperation {
    pub fn kind(&self) -> OrderEditOperationKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditPlan {
    root: PathBuf,
    directory: PathBuf,
    order_path: PathBuf,
    facts: PlannedOrderFacts,
    prior_document: OrderDocument,
    reusable_values: OrderEditReusableValues,
    date_ordered_source: DateOrderedSource,
    prior_index: PackageIndexRecord,
    operations: Vec<OrderEditOperation>,
}

impl OrderEditPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The Order Directory, whose name an edit never changes.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn facts(&self) -> &PlannedOrderFacts {
        &self.facts
    }

    pub fn reusable_values(&self) -> &OrderEditReusableValues {
        &self.reusable_values
    }

    pub fn date_ordered_source(&self) -> DateOrderedSource {
        self.date_ordered_source
    }

    pub fn operations(&self) -> &[OrderEditOperation] {
        &self.operations
    }

    /// Whether committing this plan would change the canonical Order record.
    pub fn changes_canonical_order(&self) -> bool {
        self.facts.document != self.prior_document
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderEditPlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound {
        path: PathBuf,
    },
    DamagedOrder {
        path: PathBuf,
    },
    InvalidAssignments(Vec<RejectedAssignment>),
    UnconfirmedReusableValue {
        field: String,
        proposed: String,
    },
    UnknownVendor {
        vendor_id: String,
    },
    /// A one-based Item position outside the pre-mutation collection the operation
    /// evaluates it against. An append alone may name one position beyond the last.
    InvalidItemPosition {
        role: ItemPositionRole,
        position: usize,
        minimum: usize,
        maximum: usize,
    },
    MissingRequiredItemFacts {
        fields: Vec<String>,
    },
    InvalidFacts,
    FileSystem {
        path: PathBuf,
        error: E,
    },
}

pub fn plan_order_edit<F: FileSystem>(
    order_directory: &Path,
    assignments: &OrderAssignments,
    calendar: &impl LocalCalendar,
    filesystem: &F,
) -> Result<OrderEditPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let assignments = accepted_assignments(assignments)?;
    let prior_document = order.document().clone();

    let status = match assigned(&assignments, "status") {
        Some(value) => parse_status(value).ok_or(OrderEditPlanError::InvalidFacts)?,
        None => prior_document.status,
    };
    let (date_ordered, date_ordered_source) = match assigned(&assignments, "date_ordered") {
        Some(value) => (value.trim().to_owned(), DateOrderedSource::Assigned),
        None if status == OrderStatus::Ordered && prior_document.status != OrderStatus::Ordered => {
            let today = calendar.today();
            (
                format!("{:04}-{:02}-{:02}", today.year, today.month, today.day),
                DateOrderedSource::AppliedOnEnteringOrdered,
            )
        }
        None => (
            prior_document.date_ordered.as_str().to_owned(),
            DateOrderedSource::Retained,
        ),
    };

    let mut object = serde_json::to_value(&prior_document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
    object.insert(
        "date_ordered".into(),
        serde_json::Value::String(date_ordered),
    );
    for (field, value, _) in &assignments {
        if field == "date_ordered" {
            continue;
        }
        if value.trim().is_empty() {
            object.remove(field);
        } else {
            object.insert(
                field.clone(),
                serde_json::Value::String(value.trim().to_owned()),
            );
        }
    }

    let reusable_values = resolve_reusable_values(&assignments, &mut object, &package)?;
    if let Some(vendor_id) = object.get("vendor_id").and_then(serde_json::Value::as_str)
        && !package
            .vendors()
            .iter()
            .any(|vendor| vendor.vendor_id.as_str() == vendor_id)
    {
        return Err(OrderEditPlanError::UnknownVendor {
            vendor_id: vendor_id.to_owned(),
        });
    }

    order_record_plan(
        &package,
        &order,
        prior_document,
        object,
        reusable_values,
        date_ordered_source,
    )
}

/// Assembles the one complete validated Order-record replacement every Order
/// mutation commits, from a candidate object its own capability built. An unchanged
/// record is committed by performing nothing, so its plan must not announce managed
/// operations its commit will never reach.
fn order_record_plan<E>(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    prior_document: OrderDocument,
    candidate: serde_json::Map<String, serde_json::Value>,
    reusable_values: OrderEditReusableValues,
    date_ordered_source: DateOrderedSource,
) -> Result<OrderEditPlan, OrderEditPlanError<E>> {
    let validated = serde_json::to_vec(&serde_json::Value::Object(candidate))
        .ok()
        .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
        .and_then(|document| document.validate().ok())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
    let facts = PlannedOrderFacts {
        document: validated.order().clone(),
        item_totals: validated.item_totals().to_vec(),
        order_total: validated.order_total(),
    };

    let order_path = order.paths().order().absolute_path().to_path_buf();
    let index_path = package.context().index().absolute_path().to_path_buf();
    let operations = if facts.document == prior_document {
        Vec::new()
    } else {
        vec![
            OrderEditOperation {
                kind: OrderEditOperationKind::ReplaceOrderRecord,
                path: order_path.clone(),
            },
            OrderEditOperation {
                kind: OrderEditOperationKind::RefreshPackageIndex,
                path: index_path,
            },
        ]
    };

    Ok(OrderEditPlan {
        root: package.context().root().absolute_path().to_path_buf(),
        directory: order.paths().directory().absolute_path().to_path_buf(),
        order_path,
        facts,
        prior_document,
        reusable_values,
        date_ordered_source,
        prior_index: package.index().clone(),
        operations,
    })
}

/// Canonicalizes every Reusable Free-Entry Value the resulting Order carries and
/// refuses a genuinely new value the caller did not explicitly introduce.
fn resolve_reusable_values<E>(
    assignments: &[(String, String, AssignmentIntent)],
    object: &mut serde_json::Map<String, serde_json::Value>,
    package: &CurrentPackageCandidate,
) -> Result<OrderEditReusableValues, OrderEditPlanError<E>> {
    let mut outcomes = Vec::new();
    for field in REUSABLE_FIELDS {
        let existing = existing_values(field, package);
        let present = object.get(field).and_then(serde_json::Value::as_str);
        let outcome = canonicalize_reusable_value(present, &existing);
        if let ReusableValue::New(proposed) = &outcome
            && !assignments.iter().any(|(assigned, _, intent)| {
                assigned == field && *intent == AssignmentIntent::Introduce
            })
        {
            return Err(OrderEditPlanError::UnconfirmedReusableValue {
                field: field.to_owned(),
                proposed: proposed.clone(),
            });
        }
        match &outcome {
            ReusableValue::Absent => {
                object.remove(field);
            }
            ReusableValue::Reused(value) | ReusableValue::New(value) => {
                object.insert(field.into(), serde_json::Value::String(value.clone()));
            }
        }
        outcomes.push(outcome);
    }
    let mut outcomes = outcomes.into_iter();

    Ok(OrderEditReusableValues {
        orderer: outcomes.next().expect("orderer is a reusable field"),
        ordering_institution: outcomes
            .next()
            .expect("ordering_institution is a reusable field"),
        project: outcomes.next().expect("project is a reusable field"),
        order_group: outcomes.next().expect("order_group is a reusable field"),
    })
}

fn existing_values<'a>(field: &str, package: &'a CurrentPackageCandidate) -> Vec<&'a str> {
    let projections = match field {
        "orderer" => package.orderers(),
        "ordering_institution" => package.ordering_institutions(),
        "project" => package.projects(),
        _ => package.order_groups(),
    };
    projections
        .iter()
        .map(ReusableValueProjection::value)
        .collect()
}

fn assigned<'a>(
    assignments: &'a [(String, String, AssignmentIntent)],
    field: &str,
) -> Option<&'a str> {
    assignments
        .iter()
        .find(|(assigned, _, _)| assigned == field)
        .map(|(_, value, _)| value.as_str())
}

fn parse_status(value: &str) -> Option<OrderStatus> {
    serde_json::from_value(serde_json::Value::String(value.trim().to_owned())).ok()
}

fn accepted_assignments<E>(
    assignments: &OrderAssignments,
) -> Result<Vec<(String, String, AssignmentIntent)>, OrderEditPlanError<E>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, String, AssignmentIntent)>::new();
    for (field, value, intent) in &assignments.entries {
        let reason = match field.as_str() {
            "order_id" => Some(RejectedAssignmentReason::ImmutableIdentity),
            "schema_version" | "package_root" => {
                Some(RejectedAssignmentReason::ManagedTechnicalField)
            }
            "items" => Some(RejectedAssignmentReason::OrderedCollection),
            field if !ASSIGNABLE_FIELDS.contains(&field) => {
                Some(RejectedAssignmentReason::UnknownField)
            }
            _ if accepted.iter().any(|(accepted, _, _)| accepted == field) => {
                Some(RejectedAssignmentReason::Duplicate)
            }
            field
                if *intent == AssignmentIntent::Introduce && !REUSABLE_FIELDS.contains(&field) =>
            {
                Some(RejectedAssignmentReason::NotAReusableValue)
            }
            "title" | "date_ordered" | "status" if value.trim().is_empty() => {
                Some(RejectedAssignmentReason::BlankRequiredValue)
            }
            _ => None,
        };
        match reason {
            Some(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
            None => accepted.push((field.clone(), value.clone(), *intent)),
        }
    }
    if !rejected.is_empty() {
        return Err(OrderEditPlanError::InvalidAssignments(rejected));
    }
    Ok(accepted)
}

/// Why one Order Directory could not be resolved to a healthy Order of its own
/// Package. Every Order-scoped capability refuses on these same terms, so they are
/// named once here rather than restated per capability.
#[derive(Debug, PartialEq)]
pub(crate) enum OrderResolutionFailure<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    OrderNotFound { path: PathBuf },
    DamagedOrder { path: PathBuf },
    FileSystem { path: PathBuf, error: E },
}

impl<E> From<OrderResolutionFailure<E>> for OrderEditPlanError<E> {
    fn from(failure: OrderResolutionFailure<E>) -> Self {
        match failure {
            OrderResolutionFailure::UnmanagedPackage => Self::UnmanagedPackage,
            OrderResolutionFailure::DamagedPackage => Self::DamagedPackage,
            OrderResolutionFailure::UpgradePending => Self::UpgradePending,
            OrderResolutionFailure::PackageOperationFailed => Self::PackageOperationFailed,
            OrderResolutionFailure::OrderNotFound { path } => Self::OrderNotFound { path },
            OrderResolutionFailure::DamagedOrder { path } => Self::DamagedOrder { path },
            OrderResolutionFailure::FileSystem { path, error } => Self::FileSystem { path, error },
        }
    }
}

/// Resolves the Order Directory's own Package through its normalized locator, then
/// requires the directory to be one healthy Order of that Package.
pub(crate) fn resolve_order<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<(CurrentPackageCandidate, HealthyOrder), OrderResolutionFailure<F::Error>> {
    let order_path = order_directory.join(".spielorders").join("order.json");
    // A missing, unreadable, wrongly typed, or unparseable record is one Damaged
    // Order, exactly as a semantically invalid one is; an edit never repairs or
    // adopts it. Only a failure to observe the path at all remains an operation
    // failure, matching how the Package loader classifies a managed artifact.
    if filesystem.symlink_metadata(&order_path).map_err(|error| {
        OrderResolutionFailure::FileSystem {
            path: order_path.clone(),
            error,
        }
    })? != Some(FileSystemEntryKind::File)
    {
        return Err(OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        });
    }
    let bytes = filesystem
        .read(&order_path)
        .map_err(|_| OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        })?;
    let document = OrderDocument::from_json_bytes(&bytes).map_err(|_| {
        OrderResolutionFailure::DamagedOrder {
            path: order_directory.to_path_buf(),
        }
    })?;
    let directory = filesystem.canonicalize(order_directory).map_err(|error| {
        OrderResolutionFailure::FileSystem {
            path: order_directory.to_path_buf(),
            error,
        }
    })?;
    let parent_count = document.package_root.as_str().split('/').count();
    let root = directory
        .ancestors()
        .nth(parent_count)
        .ok_or_else(|| OrderResolutionFailure::OrderNotFound {
            path: directory.clone(),
        })?
        .to_path_buf();

    let package = match inspect_canonical_package(&root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        CanonicalPackageLoad::Unmanaged(_) => return Err(OrderResolutionFailure::UnmanagedPackage),
        CanonicalPackageLoad::DiagnosticOnly(_) => {
            return Err(OrderResolutionFailure::DamagedPackage);
        }
        CanonicalPackageLoad::UpgradePending(_) => {
            return Err(OrderResolutionFailure::UpgradePending);
        }
        CanonicalPackageLoad::OperationFailed(_) => {
            return Err(OrderResolutionFailure::PackageOperationFailed);
        }
    };
    if let Some(order) = package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.paths().directory().absolute_path() == directory)
        .cloned()
    {
        return Ok((package, order));
    }
    if package
        .damaged_orders()
        .iter()
        .any(|candidate| candidate.paths().directory().absolute_path() == directory)
    {
        return Err(OrderResolutionFailure::DamagedOrder { path: directory });
    }
    Err(OrderResolutionFailure::OrderNotFound { path: directory })
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderEditResult {
    order: HealthyOrder,
    index: PackageIndexRecord,
}

impl OrderEditResult {
    /// The complete refreshed Order projection, including calculated totals.
    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

/// An Order edit's candidate is `plan.facts.document` serialized, and that document
/// was produced by parsing and validating in `order_record_plan`, which refuses with
/// `OrderEditPlanError::InvalidFacts` when it cannot. There is no way to reach a
/// commit holding a candidate that was never validated, so no candidate validation
/// failure is possible, which this states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderCandidateFailure {}

#[derive(Debug, PartialEq)]
pub enum OrderEditRejection<E> {
    PreconditionsChanged {
        path: PathBuf,
    },
    ReplacementFailed {
        error: ValidatedReplacementError<E, OrderCandidateFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderEditWarning {
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

pub type OrderEditReport<E> =
    MutationReport<OrderEditPlan, OrderEditResult, OrderEditRejection<E>, OrderEditWarning>;

pub fn commit_order_edit<F: FileSystem>(
    plan: OrderEditPlan,
    filesystem: &F,
) -> OrderEditReport<F::Error> {
    let current = match inspect_canonical_package(&plan.root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        _ => {
            let path = plan.root.clone();
            return preconditions_changed(plan, path);
        }
    };
    let Some(order) = current
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.paths().directory().absolute_path() == plan.directory)
    else {
        let path = plan.directory.clone();
        return preconditions_changed(plan, path);
    };
    if order.document() != &plan.prior_document {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
    }
    if let Some(vendor_id) = plan.facts.vendor_id()
        && !current
            .vendors()
            .iter()
            .any(|vendor| vendor.vendor_id.as_str() == vendor_id)
    {
        let path = current.context().vendors().absolute_path().to_path_buf();
        return preconditions_changed(plan, path);
    }
    if !reusable_values_still_current(&plan, &current) {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
    }

    if !plan.changes_canonical_order() {
        let result = OrderEditResult {
            order: order.clone(),
            index: current.index().clone(),
        };
        return MutationReport::Unchanged { plan, result };
    }
    // An Order mutation replaces only the record, so what the load knew before the
    // replacement is what the committed Order holds. A projection built without a
    // refreshed Package still owes the caller the complete Order, README included.
    let readme_text = order.readme_text().map(str::to_owned);
    let Ok(candidate) = plan.facts.document.to_json_bytes() else {
        let path = plan.order_path.clone();
        return preconditions_changed(plan, path);
    };
    // The Order revalidated above is the one this edit is authorized to replace, so a
    // writer landing between that revalidation and the rename is refused rather than
    // overwritten. Meaning is what is compared, not bytes — which rule to carry where,
    // and what comparing meaning gives up, are in `docs/architecture/durable-writes.md`.
    let replacement = {
        // Projected the way the load projects it, because that projection — not the
        // raw parse — is what the revalidation above compared. A record whose stored
        // facts carry untrimmed text projects to the same Order, and refusing it would
        // mean refusing to edit a file a person reformatted by hand.
        let still_holds_the_revalidated_order = |current: &[u8]| {
            OrderDocument::from_json_bytes(current)
                .ok()
                .and_then(|document| document.validate().ok())
                .is_some_and(|projected| projected.order() == &plan.prior_document)
        };
        replace_validated_file_expecting(
            &plan.order_path,
            &candidate,
            DestinationExpectation::Equivalent(&still_holds_the_revalidated_order),
            filesystem,
            |_| Ok::<(), OrderCandidateFailure>(()),
        )
    };
    if let Err(error) = replacement {
        return MutationReport::Rejected {
            plan,
            rejection: OrderEditRejection::ReplacementFailed { error },
        };
    }

    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => {
            let result = result_from_package(&plan, &package, readme_text.as_deref());
            MutationReport::Committed { plan, result }
        }
        PackageLoad::IndexReplacementFailed(failed) => {
            let result =
                result_from_package(&plan, failed.canonical_evidence(), readme_text.as_deref());
            let warning = OrderEditWarning::IndexRefreshFailed(failed.failure().clone());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        PackageLoad::OperationFailed(failed) => {
            let warning = OrderEditWarning::IndexRefreshFailed(failed.failure().clone());
            let result = result_without_refresh(&plan, readme_text.as_deref());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![warning],
            }
        }
        _ => {
            let result = result_without_refresh(&plan, readme_text.as_deref());
            MutationReport::CommittedWithWarning {
                plan,
                result,
                warnings: vec![OrderEditWarning::RefreshUnavailable],
            }
        }
    }
}

/// Re-canonicalizes every planned Reusable Free-Entry Value against current Package
/// state so a concurrently introduced canonical spelling cannot commit stale text.
fn reusable_values_still_current(plan: &OrderEditPlan, current: &CurrentPackageCandidate) -> bool {
    REUSABLE_FIELDS.iter().all(|field| {
        let planned = match *field {
            "orderer" => plan.facts.orderer(),
            "ordering_institution" => plan.facts.ordering_institution(),
            "project" => plan.facts.project(),
            _ => plan.facts.order_group(),
        };
        let existing = existing_values(field, current);
        match canonicalize_reusable_value(planned, &existing) {
            ReusableValue::Absent => planned.is_none(),
            ReusableValue::Reused(value) | ReusableValue::New(value) => {
                planned == Some(value.as_str())
            }
        }
    })
}

fn preconditions_changed<E>(plan: OrderEditPlan, path: PathBuf) -> OrderEditReport<E> {
    MutationReport::Rejected {
        plan,
        rejection: OrderEditRejection::PreconditionsChanged { path },
    }
}

fn result_from_package(
    plan: &OrderEditPlan,
    package: &CurrentPackageCandidate,
    readme_text: Option<&str>,
) -> OrderEditResult {
    let order = package
        .healthy_orders()
        .iter()
        .find(|candidate| candidate.document().order_id == plan.facts.document.order_id)
        .cloned()
        .unwrap_or_else(|| projected_order(plan, readme_text));
    OrderEditResult {
        order,
        index: package.index().clone(),
    }
}

fn result_without_refresh(plan: &OrderEditPlan, readme_text: Option<&str>) -> OrderEditResult {
    OrderEditResult {
        order: projected_order(plan, readme_text),
        index: plan
            .prior_index
            .after_committed_order_metadata_change_without_refresh(),
    }
}

fn projected_order(plan: &OrderEditPlan, readme_text: Option<&str>) -> HealthyOrder {
    project_committed_order(
        &plan.root,
        &plan.directory,
        &plan.facts.document,
        readme_text.map(str::to_owned),
    )
}

const ITEM_FIELDS: [&str; 4] = ["description", "quantity", "unit_price", "catalog_number"];

const REQUIRED_ITEM_FIELDS: [&str; 3] = ["description", "quantity", "unit_price"];

/// One order-independent canonical `field=value` assignment set for one Order Item.
/// An empty value clears the optional Catalog Number; a required Item fact is never
/// blank.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ItemAssignments {
    entries: Vec<(String, String)>,
}

impl ItemAssignments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assign(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.push((field.into(), value.into()));
        self
    }
}

/// Which end of an ordered Item operation a one-based position names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemPositionRole {
    Source,
    Destination,
}

impl ItemPositionRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Destination => "destination",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderItemMutationKind {
    Add,
    Edit,
    Move,
    Remove,
}

impl OrderItemMutationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Edit => "edit",
            Self::Move => "move",
            Self::Remove => "remove",
        }
    }
}

/// A complete, validated mutation of one Order's embedded Item collection. It
/// rewrites the whole owning Order record once and changes no other fact.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderItemMutationPlan {
    kind: OrderItemMutationKind,
    source_position: Option<usize>,
    destination_position: Option<usize>,
    subject_item: OrderItem,
    order_plan: OrderEditPlan,
}

impl OrderItemMutationPlan {
    pub fn root(&self) -> &Path {
        self.order_plan.root()
    }

    /// The Order Directory, whose name an Item mutation never changes.
    pub fn directory(&self) -> &Path {
        self.order_plan.directory()
    }

    pub fn kind(&self) -> OrderItemMutationKind {
        self.kind
    }

    pub fn source_position(&self) -> Option<usize> {
        self.source_position
    }

    pub fn destination_position(&self) -> Option<usize> {
        self.destination_position
    }

    /// The added, edited, moved, or removed Item itself.
    pub fn subject_item(&self) -> &OrderItem {
        &self.subject_item
    }

    /// The complete refreshed Order this mutation would commit, with its exact
    /// recalculated Item and Order totals.
    pub fn facts(&self) -> &PlannedOrderFacts {
        self.order_plan.facts()
    }

    /// Every Reusable Free-Entry Value the Order carries. An Item mutation assigns
    /// no scalar fact, so each one is retained rather than reused or introduced.
    pub fn reusable_values(&self) -> &OrderEditReusableValues {
        self.order_plan.reusable_values()
    }

    pub fn operations(&self) -> &[OrderEditOperation] {
        self.order_plan.operations()
    }

    pub fn changes_canonical_order(&self) -> bool {
        self.order_plan.changes_canonical_order()
    }
}

/// Lists one Order's embedded Items in their exact user-controlled order, together
/// with the complete owning Order and its calculated, never stored, totals.
pub fn list_order_items<F: FileSystem>(
    order_directory: &Path,
    filesystem: &F,
) -> Result<HealthyOrder, OrderEditPlanError<F::Error>> {
    let (_package, order) = resolve_order(order_directory, filesystem)?;
    Ok(order)
}

/// Appends one Item, or inserts it before the pre-mutation Item at an explicit
/// one-based position.
pub fn plan_order_item_add<F: FileSystem>(
    order_directory: &Path,
    assignments: &ItemAssignments,
    position: Option<usize>,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let count = prior_document.items.len();
    let destination_position = position.unwrap_or(count + 1);
    let destination_index = item_index(
        ItemPositionRole::Destination,
        destination_position,
        count + 1,
    )?;
    let assignments = accepted_item_assignments(assignments)?;
    let item = planned_item(None, &assignments)?;
    let mut items = prior_document.items.clone();
    items.insert(destination_index, item);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[destination_index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Add,
        source_position: None,
        destination_position: Some(destination_position),
        subject_item,
        order_plan,
    })
}

/// Replaces the facts of the Item at one pre-mutation one-based position, leaving
/// its position and every other Item exactly as stored.
pub fn plan_order_item_edit<F: FileSystem>(
    order_directory: &Path,
    position: usize,
    assignments: &ItemAssignments,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let index = item_index(
        ItemPositionRole::Source,
        position,
        prior_document.items.len(),
    )?;
    let assignments = accepted_item_assignments(assignments)?;
    let item = planned_item(Some(&prior_document.items[index]), &assignments)?;
    let mut items = prior_document.items.clone();
    items[index] = item;

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Edit,
        source_position: Some(position),
        destination_position: Some(position),
        subject_item,
        order_plan,
    })
}

/// Lifts the Item at one pre-mutation one-based position and reinserts it at
/// another. Both positions name Items that already exist, so a move can neither
/// reach past the last Item nor change how many Items the Order has.
pub fn plan_order_item_move<F: FileSystem>(
    order_directory: &Path,
    source_position: usize,
    destination_position: usize,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let count = prior_document.items.len();
    let source_index = item_index(ItemPositionRole::Source, source_position, count)?;
    let destination_index = item_index(ItemPositionRole::Destination, destination_position, count)?;
    let mut items = prior_document.items.clone();
    let item = items.remove(source_index);
    items.insert(destination_index, item);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    let subject_item = order_plan.facts.document.items[destination_index].clone();
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Move,
        source_position: Some(source_position),
        destination_position: Some(destination_position),
        subject_item,
        order_plan,
    })
}

/// Drops the Item at one pre-mutation one-based position. Its subject is absent
/// from the resulting collection, so it is reported from the one it left.
pub fn plan_order_item_remove<F: FileSystem>(
    order_directory: &Path,
    position: usize,
    filesystem: &F,
) -> Result<OrderItemMutationPlan, OrderEditPlanError<F::Error>> {
    let (package, order) = resolve_order(order_directory, filesystem)?;
    let prior_document = order.document().clone();
    let index = item_index(
        ItemPositionRole::Source,
        position,
        prior_document.items.len(),
    )?;
    let mut items = prior_document.items.clone();
    let subject_item = items.remove(index);

    let order_plan = order_item_plan(&package, &order, prior_document, items)?;
    Ok(OrderItemMutationPlan {
        kind: OrderItemMutationKind::Remove,
        source_position: Some(position),
        destination_position: None,
        subject_item,
        order_plan,
    })
}

/// Rewrites the whole owning Order record with a candidate Item collection,
/// retaining every scalar fact exactly as stored.
fn order_item_plan<E>(
    package: &CurrentPackageCandidate,
    order: &HealthyOrder,
    prior_document: OrderDocument,
    items: Vec<OrderItem>,
) -> Result<OrderEditPlan, OrderEditPlanError<E>> {
    let mut object = serde_json::to_value(&prior_document)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(OrderEditPlanError::InvalidFacts)?;
    object.insert(
        "items".into(),
        serde_json::to_value(&items).map_err(|_| OrderEditPlanError::InvalidFacts)?,
    );
    let reusable_values = retained_reusable_values(&prior_document);
    order_record_plan(
        package,
        order,
        prior_document,
        object,
        reusable_values,
        DateOrderedSource::Retained,
    )
}

/// An Item mutation assigns no scalar fact, so every Reusable Free-Entry Value the
/// Order already carries is retained with its stored canonical spelling.
fn retained_reusable_values(document: &OrderDocument) -> OrderEditReusableValues {
    let retained = |value: Option<&String>| match value {
        Some(value) => ReusableValue::Reused(value.clone()),
        None => ReusableValue::Absent,
    };
    OrderEditReusableValues {
        orderer: retained(document.orderer.as_ref()),
        ordering_institution: retained(document.ordering_institution.as_ref()),
        project: retained(document.project.as_ref()),
        order_group: retained(document.order_group.as_ref()),
    }
}

fn item_index<E>(
    role: ItemPositionRole,
    position: usize,
    maximum: usize,
) -> Result<usize, OrderEditPlanError<E>> {
    if position == 0 || position > maximum {
        Err(OrderEditPlanError::InvalidItemPosition {
            role,
            position,
            minimum: 1,
            maximum,
        })
    } else {
        Ok(position - 1)
    }
}

/// One accepted Item assignment: a candidate value, or the explicit clearing of an
/// optional Item fact.
enum ItemValue {
    Clear,
    Set(serde_json::Value),
}

fn accepted_item_assignments<E>(
    assignments: &ItemAssignments,
) -> Result<Vec<(String, ItemValue)>, OrderEditPlanError<E>> {
    let mut rejected = Vec::new();
    let mut accepted = Vec::<(String, ItemValue)>::new();
    for (field, value) in &assignments.entries {
        let duplicate = accepted.iter().any(|(accepted, _)| accepted == field);
        match item_assignment(field, value, duplicate) {
            Ok(value) => accepted.push((field.clone(), value)),
            Err(reason) => rejected.push(RejectedAssignment::new(field.clone(), reason)),
        }
    }
    if rejected.is_empty() {
        Ok(accepted)
    } else {
        Err(OrderEditPlanError::InvalidAssignments(rejected))
    }
}

/// Accepts one Item assignment as the exact candidate value it will store, so an
/// accepted numeric value is one the durable record can already hold.
fn item_assignment(
    field: &str,
    value: &str,
    duplicate: bool,
) -> Result<ItemValue, RejectedAssignmentReason> {
    if !ITEM_FIELDS.contains(&field) {
        return Err(RejectedAssignmentReason::UnknownField);
    }
    if duplicate {
        return Err(RejectedAssignmentReason::Duplicate);
    }
    let value = value.trim();
    if value.is_empty() {
        return if REQUIRED_ITEM_FIELDS.contains(&field) {
            Err(RejectedAssignmentReason::BlankRequiredValue)
        } else {
            Ok(ItemValue::Clear)
        };
    }
    match field {
        // Quantity and Unit Price accept any contract-valid zero or negative value,
        // and refuse text the durable record could not hold as a finite number.
        "quantity" | "unit_price" => value
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .map(ItemValue::Set)
            .ok_or(RejectedAssignmentReason::NotAFiniteNumber),
        _ => Ok(ItemValue::Set(serde_json::Value::String(value.to_owned()))),
    }
}

fn planned_item<E>(
    existing: Option<&OrderItem>,
    assignments: &[(String, ItemValue)],
) -> Result<OrderItem, OrderEditPlanError<E>> {
    let mut object = match existing {
        Some(item) => serde_json::to_value(item)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or(OrderEditPlanError::InvalidFacts)?,
        None => serde_json::Map::new(),
    };
    for (field, value) in assignments {
        match value {
            ItemValue::Clear => {
                object.remove(field);
            }
            ItemValue::Set(value) => {
                object.insert(field.clone(), value.clone());
            }
        }
    }
    let missing = REQUIRED_ITEM_FIELDS
        .iter()
        .filter(|field| !object.contains_key(**field))
        .map(|field| (*field).to_owned())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(OrderEditPlanError::MissingRequiredItemFacts { fields: missing });
    }
    serde_json::from_value::<OrderItem>(serde_json::Value::Object(object))
        .map_err(|_| OrderEditPlanError::InvalidFacts)
}

pub type OrderItemMutationReport<E> =
    MutationReport<OrderItemMutationPlan, OrderEditResult, OrderEditRejection<E>, OrderEditWarning>;

/// Commits one already validated Item plan. The collection lives inside its owning
/// Order, so this is the same single validated record replacement, rollback, and
/// derived-index refresh a scalar edit performs.
pub fn commit_order_item_mutation<F: FileSystem>(
    plan: OrderItemMutationPlan,
    filesystem: &F,
) -> OrderItemMutationReport<F::Error> {
    let OrderItemMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_item,
        order_plan,
    } = plan;
    let wrap = |order_plan| OrderItemMutationPlan {
        kind,
        source_position,
        destination_position,
        subject_item,
        order_plan,
    };
    match commit_order_edit(order_plan, filesystem) {
        MutationReport::Rejected { plan, rejection } => MutationReport::Rejected {
            plan: wrap(plan),
            rejection,
        },
        MutationReport::Unchanged { plan, result } => MutationReport::Unchanged {
            plan: wrap(plan),
            result,
        },
        MutationReport::Committed { plan, result } => MutationReport::Committed {
            plan: wrap(plan),
            result,
        },
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        } => MutationReport::CommittedWithWarning {
            plan: wrap(plan),
            result,
            warnings,
        },
    }
}
