//! Planning and safe in-place persistence for one existing Order Directory.

use crate::{
    canonical::{
        CanonicalPackageLoad, HealthyOrder, PackageIndexRecord, PackageLoad,
        inspect_canonical_package, open_package, order_readme_text, project_committed_order,
    },
    metadata::{OrderDocument, OrderStatus, Vendor},
    mutation::MutationReport,
    operations::{FailedPath, OperationFailure},
    order_edits::{
        ItemAssignments, NewItemAssignmentFailure, NewOrderAssignmentFailure, NewOrderFacts,
        OrderAssignments, new_order_item,
    },
    package::{ReusableValue, canonicalize_reusable_value},
    ports::{
        DirectoryFileSystem, FileChangeEvidence, FileChangeObservation, FileSystem,
        FileSystemEntryKind, NewFileLinkOutcome, UuidGenerator,
    },
    readme::{Location, located_in_package, package_relative_path_text},
};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLANNING_ORDER_ID: &str = "00000000-0000-4000-8000-000000000000";
const ORDER_STAGING_NAME: &str = ".order.json.spielorders-stage";
const README_STAGING_NAME: &str = ".README.md.spielorders-stage";

#[derive(Clone, Debug, PartialEq)]
pub struct OrderItemInput {
    description: String,
    quantity: f64,
    unit_price: f64,
    catalog_number: Option<String>,
}

impl OrderItemInput {
    /// The Item one canonical assignment set names, or every reason it names none.
    ///
    /// Adjudicated against the one list of Item fields every Item mutation uses — see
    /// [`new_order_item`]. **Adoption is the only capability that gives an Order Items
    /// before it has a record**, because an Order created from nothing starts with none.
    pub fn from_assignments(
        assignments: &ItemAssignments,
    ) -> Result<Self, NewItemAssignmentFailure> {
        let item = new_order_item(assignments)?;
        Ok(Self {
            description: item.description.as_str().to_owned(),
            quantity: item.quantity.get(),
            unit_price: item.unit_price.get(),
            catalog_number: item.catalog_number,
        })
    }

    pub fn new(description: impl Into<String>, quantity: f64, unit_price: f64) -> Self {
        Self {
            description: description.into(),
            quantity,
            unit_price,
            catalog_number: None,
        }
    }

    pub fn catalog_number(mut self, value: impl Into<String>) -> Self {
        self.catalog_number = Some(value.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionInput {
    title: String,
    date_ordered: String,
    status: OrderStatus,
    vendor_id: Option<String>,
    orderer: Option<String>,
    ordering_institution: Option<String>,
    currency: Option<String>,
    project: Option<String>,
    order_group: Option<String>,
    items: Vec<OrderItemInput>,
}

impl OrderAdoptionInput {
    /// The facts one canonical assignment set names, or every reason it named none.
    ///
    /// Adjudicated by the core for the reason [`OrderCreationInput::from_assignments`] is,
    /// and against the same one list of assignable fields. What differs is what adoption
    /// requires: an adopted Order's Title, Date Ordered, and Status are supplied
    /// explicitly, because a candidate's name is evidence for a human and never a field
    /// grammar — nothing here parses a basename or inspects a directory's contents to
    /// propose one.
    pub fn from_assignments(
        assignments: &OrderAssignments,
    ) -> Result<Self, NewOrderAssignmentFailure> {
        let facts = NewOrderFacts::of(assignments)?;
        // All three asked for at once, so a command line naming none of them is told about
        // all three rather than made to discover them one round trip at a time.
        let [title, date_ordered, _] = facts.required(["title", "date_ordered", "status"])?;
        let status = facts
            .status()
            .expect("the required Order Status was found above");
        let mut input = Self::new(title, date_ordered, status);
        input.vendor_id = facts.optional("vendor_id");
        input.orderer = facts.optional("orderer");
        input.ordering_institution = facts.optional("ordering_institution");
        input.currency = facts.optional("currency");
        input.project = facts.optional("project");
        input.order_group = facts.optional("order_group");
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
            vendor_id: None,
            orderer: None,
            ordering_institution: None,
            currency: None,
            project: None,
            order_group: None,
            items: Vec::new(),
        }
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

    pub fn item(mut self, item: OrderItemInput) -> Self {
        self.items.push(item);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannedAdoptionIdentity {
    GeneratedAtCommit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderAdoptionReusableValues {
    orderer: ReusableValue,
    ordering_institution: ReusableValue,
    project: ReusableValue,
    order_group: ReusableValue,
}

impl OrderAdoptionReusableValues {
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
pub enum OrderAdoptionOperationKind {
    CreateMetadataDirectory,
    StageOrderRecord,
    StageReadme,
    CommitReadme,
    CommitOrderRecord,
    RemoveStaging,
    RefreshPackageIndex,
}

impl OrderAdoptionOperationKind {
    /// This operation's one stable spelling.
    ///
    /// Beside the operation rather than in an adapter, so an operation reported by the CLI
    /// and the same operation reported to the desktop shell cannot come to be named
    /// differently.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateMetadataDirectory => "create_metadata_directory",
            Self::StageOrderRecord => "stage_order_record",
            Self::StageReadme => "stage_readme",
            Self::CommitReadme => "commit_readme",
            Self::CommitOrderRecord => "commit_order_record",
            Self::RemoveStaging => "remove_staging",
            Self::RefreshPackageIndex => "refresh_package_index",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderAdoptionOperation {
    kind: OrderAdoptionOperationKind,
    location: Location,
}

impl OrderAdoptionOperation {
    pub fn kind(&self) -> OrderAdoptionOperationKind {
        self.kind
    }

    /// Where this operation writes, as this Package locates it — so a caller reporting it
    /// carries the Package-relative spelling as well as the absolute path.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn path(&self) -> &Path {
        self.location.absolute_path()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionPlan {
    root: PathBuf,
    directory: PathBuf,
    package_root_locator: String,
    document: OrderDocument,
    projected_order: HealthyOrder,
    vendor: Option<Vendor>,
    reusable_values: OrderAdoptionReusableValues,
    candidate_observation: FileChangeObservation,
    readme_kind: Option<FileSystemEntryKind>,
    prior_index: PackageIndexRecord,
    operations: Vec<OrderAdoptionOperation>,
}

impl OrderAdoptionPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn package_root_locator(&self) -> &str {
        &self.package_root_locator
    }

    pub fn identity(&self) -> PlannedAdoptionIdentity {
        PlannedAdoptionIdentity::GeneratedAtCommit
    }

    pub fn document(&self) -> &OrderDocument {
        &self.document
    }

    /// The complete Order this plan would leave behind, as a caller reporting an Order
    /// reports one: its managed paths, its record, its calculated totals, and the text of
    /// the README the candidate holds or the empty one the commit will create.
    ///
    /// **The Order this plan would leave, not one that exists.** Its record carries the
    /// planning identity rather than an Order ID, because the identity is minted at the
    /// commit — see [`crate::creation::OrderCreationPlan::projected_order`], whose
    /// reasoning this shares.
    pub fn projected_order(&self) -> &HealthyOrder {
        &self.projected_order
    }

    /// The Vendor this Order's reference resolves to, Hidden Vendors included. `None` when
    /// it references none. See [`crate::creation::OrderCreationPlan::vendor`].
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    /// What committing this plan does to the canonical Order records.
    ///
    /// Read off the plan so that a dry run and the commit of the same plan cannot disagree
    /// about what the mutation was.
    pub fn canonical_outcome(&self) -> CanonicalAdoptionOutcome {
        CanonicalAdoptionOutcome::Adopted
    }

    pub fn reusable_values(&self) -> &OrderAdoptionReusableValues {
        &self.reusable_values
    }

    pub fn operations(&self) -> &[OrderAdoptionOperation] {
        &self.operations
    }

    /// The derived Package Index as it stood when this adoption was planned.
    ///
    /// The state a run that never commits has to report: committing refreshes the index and
    /// [`OrderAdoptionResult::index`] is that refreshed record, so a dry run has none of its
    /// own and answers with what it observed instead.
    pub fn prior_index(&self) -> &PackageIndexRecord {
        &self.prior_index
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderAdoptionPlanError<E> {
    UnmanagedPackage,
    DamagedPackage,
    UpgradePending,
    PackageOperationFailed,
    /// The candidate resolved, and it is not an entry of the Orders Directory. It may
    /// still stand inside the Package — a directory beside the Orders Directory is the
    /// ordinary case — so the `Location` carries this Package's spelling for it where
    /// there is one and the path alone for a candidate outside the Package entirely.
    NotDirectChild {
        location: Location,
    },
    /// The two refusals below name the caller's own selected path, refused before
    /// anything resolved it: a name beginning with a dot is outside discovery, and a
    /// candidate that is not an ordinary directory is nothing to adopt. Nothing has
    /// placed either inside a Package, so they carry the path exactly as it arrived.
    HiddenCandidate {
        path: PathBuf,
    },
    IneligibleCandidate {
        path: PathBuf,
        kind: Option<FileSystemEntryKind>,
    },
    /// The selected directory's name has no normalized Package-relative spelling,
    /// so an adopted Order there could be named by neither its own record's
    /// locator nor the Package Index. The directory is left exactly as it is.
    ///
    /// A `Location` like its neighbours, and the one whose spelling is *always* absent:
    /// having no name this Package can express is what the refusal reports, so the type
    /// states it rather than a caller asserting it.
    NameUnusableCandidate {
        location: Location,
    },
    /// The two refusals below name a path inside the Package.
    ///
    /// **A `Location` rather than a `PathBuf`**, because each is raised only after the
    /// candidate has been shown to be a direct child of the Orders Directory whose name
    /// this Package can express — so a Package-relative spelling exists for it, and a
    /// refusal owes that spelling exactly as a success does.
    AlreadyManaged {
        location: Location,
    },
    ConflictingReservedState {
        location: Location,
    },
    InvalidFacts,
    UnknownVendor {
        vendor_id: String,
    },
    StagingCollision {
        location: Location,
    },
    /// **A [`FailedPath`] rather than a `PathBuf`**, because the paths that reach here do
    /// not agree about whether a Package surrounds them: an operation on a candidate this
    /// Package has already established is a direct child of its Orders Directory carries
    /// that Package's spelling, and resolving the caller's own selected path is the one
    /// operation that fails before there is anything to name. The type tells them apart.
    FileSystem {
        path: FailedPath,
        error: E,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalAdoptionOutcome {
    Adopted,
}

impl CanonicalAdoptionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Adopted => "adopted",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderAdoptionResult {
    canonical_outcome: CanonicalAdoptionOutcome,
    order: HealthyOrder,
    vendor: Option<Vendor>,
    index: PackageIndexRecord,
}

impl OrderAdoptionResult {
    pub fn canonical_outcome(&self) -> CanonicalAdoptionOutcome {
        self.canonical_outcome
    }

    pub fn order(&self) -> &HealthyOrder {
        &self.order
    }

    /// The Vendor the adopted Order's reference resolves to, Hidden Vendors included.
    /// `None` when it references none. See [`OrderAdoptionPlan::vendor`].
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    pub fn index(&self) -> &PackageIndexRecord {
        &self.index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderAdoptionCommitPhase {
    ObservePreconditions,
    CreateMetadataDirectory,
    WriteOrderRecord,
    ReadOrderRecord,
    ValidateOrderRecord,
    SyncOrderRecord,
    WriteReadme,
    SyncReadme,
    CommitReadme,
    CommitOrderRecord,
}

impl OrderAdoptionCommitPhase {
    /// This phase's one stable spelling, for a caller reporting where a commit stopped.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObservePreconditions => "observe_preconditions",
            Self::CreateMetadataDirectory => "create_metadata_directory",
            Self::WriteOrderRecord => "write_order_record",
            Self::ReadOrderRecord => "read_order_record",
            Self::ValidateOrderRecord => "validate_order_record",
            Self::SyncOrderRecord => "sync_order_record",
            Self::WriteReadme => "write_readme",
            Self::SyncReadme => "sync_readme",
            Self::CommitReadme => "commit_readme",
            Self::CommitOrderRecord => "commit_order_record",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderAdoptionCommitFailureCause<E> {
    FileSystem(E),
    CandidateChanged,
    DestinationChanged,
}

#[derive(Debug, PartialEq)]
pub struct OrderAdoptionCommitFailure<E> {
    phase: OrderAdoptionCommitPhase,
    location: Location,
    cause: OrderAdoptionCommitFailureCause<E>,
}

impl<E> OrderAdoptionCommitFailure<E> {
    pub fn phase(&self) -> OrderAdoptionCommitPhase {
        self.phase
    }

    /// The path this commit stopped on, as this Package locates it.
    ///
    /// A `Location` rather than a path, for the reason
    /// [`OrderAdoptionOperation::location`] carries one: an adoption writes inside a
    /// candidate the Package has already established is one of its own direct children,
    /// and a refusal owes that Package's spelling for what it names exactly as a success
    /// does.
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn cause(&self) -> &OrderAdoptionCommitFailureCause<E> {
        &self.cause
    }
}

#[derive(Debug, PartialEq)]
pub struct OrderAdoptionCleanupFailure<E> {
    location: Location,
    error: E,
}

impl<E> OrderAdoptionCleanupFailure<E> {
    /// What this rollback left behind, as this Package locates it. See
    /// [`OrderAdoptionCommitFailure::location`].
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn error(&self) -> &E {
        &self.error
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderAdoptionRejection<E> {
    /// **A `Location` rather than a `PathBuf`**, because this refusal happens under a Root
    /// the plan established, about the Root itself or a path the plan derived inside it,
    /// and a response owes that Root's spelling for what it names — in a refusal exactly as
    /// in a success.
    PreconditionsChanged {
        location: Location,
    },
    InvalidGeneratedIdentity {
        generated: String,
    },
    GeneratedIdentityCollision {
        order_id: String,
    },
    CommitFailed {
        failure: OrderAdoptionCommitFailure<E>,
        rollback_failures: Vec<OrderAdoptionCleanupFailure<E>>,
    },
}

#[derive(Debug, PartialEq)]
pub enum OrderAdoptionWarning<E> {
    StagingCleanupFailed(OrderAdoptionCleanupFailure<E>),
    IndexRefreshFailed(OperationFailure),
    RefreshUnavailable,
}

impl<E> OrderAdoptionWarning<E> {
    /// This warning's one stable spelling.
    ///
    /// Beside the warning rather than in an adapter, so a committed adoption's warnings are
    /// named the same way in every surface that reports one.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StagingCleanupFailed(_) => "staging_cleanup_failed",
            Self::IndexRefreshFailed(_) => "index_refresh_failed",
            Self::RefreshUnavailable => "refresh_unavailable",
        }
    }
}

pub type OrderAdoptionReport<E> = MutationReport<
    OrderAdoptionPlan,
    OrderAdoptionResult,
    OrderAdoptionRejection<E>,
    OrderAdoptionWarning<E>,
>;

pub fn plan_order_adoption<F: FileSystem>(
    selected_root: &Path,
    selected_directory: &Path,
    input: &OrderAdoptionInput,
    filesystem: &F,
) -> Result<OrderAdoptionPlan, OrderAdoptionPlanError<F::Error>> {
    let package = match inspect_canonical_package(selected_root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        CanonicalPackageLoad::Unmanaged(_) => return Err(OrderAdoptionPlanError::UnmanagedPackage),
        CanonicalPackageLoad::DiagnosticOnly(_) => {
            return Err(OrderAdoptionPlanError::DamagedPackage);
        }
        CanonicalPackageLoad::UpgradePending(_) => {
            return Err(OrderAdoptionPlanError::UpgradePending);
        }
        CanonicalPackageLoad::OperationFailed(_) => {
            return Err(OrderAdoptionPlanError::PackageOperationFailed);
        }
    };
    let root = package.context().root().absolute_path().to_path_buf();
    let orders = package
        .context()
        .orders_directory()
        .expect("a current Package has an Orders Directory");
    if selected_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return Err(OrderAdoptionPlanError::HiddenCandidate {
            path: selected_directory.to_path_buf(),
        });
    }
    match observe(&root, selected_directory, filesystem)? {
        Some(FileSystemEntryKind::Directory) => {}
        kind => {
            return Err(OrderAdoptionPlanError::IneligibleCandidate {
                path: selected_directory.to_path_buf(),
                kind,
            });
        }
    }
    let directory = filesystem
        .canonicalize(selected_directory)
        .map_err(|error| OrderAdoptionPlanError::FileSystem {
            // Resolving the caller's own selected path is the operation that failed, so
            // nothing here located it and no Package spells it.
            path: FailedPath::of(selected_directory),
            error,
        })?;
    if directory.parent() != Some(orders.absolute_path()) {
        return Err(OrderAdoptionPlanError::NotDirectChild {
            location: located_in_package(&root, directory),
        });
    }
    // Adoption is the one capability that names an Order Directory instead of
    // discovering it, so the classification that protects every reader does not
    // protect this. Asking the same question here keeps a directory the Package
    // declines to index from being adopted into it.
    if package_relative_path_text(&root, &directory).is_none() {
        return Err(OrderAdoptionPlanError::NameUnusableCandidate {
            location: located_in_package(&root, directory),
        });
    }
    let candidate_observation = filesystem
        .observe_file_change(&directory)
        .map_err(|error| OrderAdoptionPlanError::FileSystem {
            path: FailedPath::inside(&root, directory.clone()),
            error,
        })?;

    let metadata_directory = directory.join(".spielorders");
    if let Some(kind) = observe(&root, &metadata_directory, filesystem)? {
        let order_path = metadata_directory.join("order.json");
        let already_managed = kind == FileSystemEntryKind::Directory
            && observe(&root, &order_path, filesystem)? == Some(FileSystemEntryKind::File)
            && filesystem
                .read(&order_path)
                .ok()
                .and_then(|bytes| OrderDocument::from_json_bytes(&bytes).ok())
                .is_some();
        return Err(if already_managed {
            OrderAdoptionPlanError::AlreadyManaged {
                location: located(&root, order_path),
            }
        } else {
            OrderAdoptionPlanError::ConflictingReservedState {
                location: located(&root, metadata_directory),
            }
        });
    }

    let readme_path = directory.join("README.md");
    let readme_kind = observe(&root, &readme_path, filesystem)?;
    let order_staging = metadata_directory.join(ORDER_STAGING_NAME);
    let readme_staging = directory.join(README_STAGING_NAME);
    for path in [&order_staging, &readme_staging] {
        if observe(&root, path, filesystem)?.is_some() {
            return Err(OrderAdoptionPlanError::StagingCollision {
                location: located(&root, path.clone()),
            });
        }
    }

    let orders_relative = orders
        .package_relative_path()
        .expect("the Orders Directory is inside the Package");
    let package_root_locator = std::iter::repeat_n("..", orders_relative.split('/').count() + 1)
        .collect::<Vec<_>>()
        .join("/");
    let vendor_id = input
        .vendor_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(vendor_id) = vendor_id
        && !package
            .vendors()
            .iter()
            .any(|vendor| vendor.vendor_id.as_str() == vendor_id)
    {
        return Err(OrderAdoptionPlanError::UnknownVendor {
            vendor_id: vendor_id.into(),
        });
    }
    let reusable_values = OrderAdoptionReusableValues {
        orderer: canonical_reusable(input.orderer.as_deref(), package.orderers()),
        ordering_institution: canonical_reusable(
            input.ordering_institution.as_deref(),
            package.ordering_institutions(),
        ),
        project: canonical_reusable(input.project.as_deref(), package.projects()),
        order_group: canonical_reusable(input.order_group.as_deref(), package.order_groups()),
    };
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
        .ok_or(OrderAdoptionPlanError::InvalidFacts)?;

    let mut operations = vec![
        operation(
            &root,
            OrderAdoptionOperationKind::CreateMetadataDirectory,
            metadata_directory,
        ),
        operation(
            &root,
            OrderAdoptionOperationKind::StageOrderRecord,
            order_staging.clone(),
        ),
    ];
    if readme_kind.is_none() {
        operations.extend([
            operation(
                &root,
                OrderAdoptionOperationKind::StageReadme,
                readme_staging.clone(),
            ),
            operation(&root, OrderAdoptionOperationKind::CommitReadme, readme_path),
        ]);
    }
    operations.push(operation(
        &root,
        OrderAdoptionOperationKind::CommitOrderRecord,
        directory.join(".spielorders/order.json"),
    ));
    if readme_kind.is_none() {
        operations.push(operation(
            &root,
            OrderAdoptionOperationKind::RemoveStaging,
            readme_staging,
        ));
    }
    operations.extend([
        operation(
            &root,
            OrderAdoptionOperationKind::RemoveStaging,
            order_staging,
        ),
        operation(
            &root,
            OrderAdoptionOperationKind::RefreshPackageIndex,
            package.context().index().absolute_path().to_path_buf(),
        ),
    ]);

    Ok(OrderAdoptionPlan {
        // The Order this plan would leave, in the shape every caller reports an Order in.
        // Adoption preserves the README the candidate already holds, so the projected
        // Order's text is read the way a load reads it; a candidate with none holds the
        // empty document the commit creates.
        projected_order: project_committed_order(
            &root,
            &directory,
            &document,
            match readme_kind {
                Some(_) => order_readme_text(&root, &directory, filesystem),
                None => Some(String::new()),
            },
        ),
        vendor: vendor_id
            .and_then(|vendor_id| {
                package
                    .vendors()
                    .iter()
                    .find(|vendor| vendor.vendor_id.as_str() == vendor_id)
            })
            .cloned(),
        root,
        directory,
        package_root_locator,
        document,
        reusable_values,
        candidate_observation,
        readme_kind,
        prior_index: package.index().clone(),
        operations,
    })
}

pub fn commit_order_adoption<F: DirectoryFileSystem>(
    plan: OrderAdoptionPlan,
    uuid_generator: &mut impl UuidGenerator,
    filesystem: &F,
) -> OrderAdoptionReport<F::Error> {
    let current = match inspect_canonical_package(&plan.root, filesystem) {
        CanonicalPackageLoad::CurrentCandidate(package) => package,
        _ => {
            let path = plan.root.clone();
            return preconditions_changed(plan, path);
        }
    };
    let metadata_directory = plan.directory.join(".spielorders");
    let order_staging = metadata_directory.join(ORDER_STAGING_NAME);
    let order_path = metadata_directory.join("order.json");
    let readme_staging = plan.directory.join(README_STAGING_NAME);
    let readme_path = plan.directory.join("README.md");
    if !filesystem
        .observe_file_change(&plan.directory)
        .is_ok_and(|current| observations_prove_same(plan.candidate_observation, current))
    {
        let path = plan.directory.clone();
        return preconditions_changed(plan, path);
    }
    for (path, expected) in [
        (plan.directory.clone(), Some(FileSystemEntryKind::Directory)),
        (metadata_directory.clone(), None),
        (readme_path.clone(), plan.readme_kind),
        (readme_staging.clone(), None),
    ] {
        if filesystem.symlink_metadata(&path).ok() != Some(expected) {
            return preconditions_changed(plan, path);
        }
    }
    if let Some(vendor_id) = plan.document.vendor_id.as_ref()
        && !current
            .vendors()
            .iter()
            .any(|vendor| vendor.vendor_id == *vendor_id)
    {
        let path = current.context().vendors().absolute_path().to_path_buf();
        return preconditions_changed(plan, path);
    }
    if reusable_values_for_document(&plan.document, &current) != plan.reusable_values {
        let path = plan.root.clone();
        return preconditions_changed(plan, path);
    }

    let generated = uuid_generator.generate_v4();
    let order = match committed_document(&plan.document, &generated) {
        Some(order) => order,
        None => {
            return MutationReport::Rejected {
                plan,
                rejection: OrderAdoptionRejection::InvalidGeneratedIdentity { generated },
            };
        }
    };
    let collision = current
        .healthy_orders()
        .iter()
        .any(|existing| existing.document().order_id == order.order_id)
        || current
            .damaged_orders()
            .iter()
            .any(|existing| existing.order_id().is_some_and(|id| *id == order.order_id));
    if collision {
        return MutationReport::Rejected {
            plan,
            rejection: OrderAdoptionRejection::GeneratedIdentityCollision {
                order_id: generated,
            },
        };
    }
    let candidate = order
        .to_json_bytes()
        .expect("a validated newest-version Order serializes deterministically");

    if let Err(error) = filesystem.create_directory(&metadata_directory) {
        return commit_failure(
            plan,
            OrderAdoptionCommitPhase::CreateMetadataDirectory,
            metadata_directory,
            error,
            Vec::new(),
        );
    }
    if let Err(error) = filesystem.write_new(&order_staging, &candidate) {
        return rollback_failure(
            plan,
            OrderAdoptionCommitPhase::WriteOrderRecord,
            order_staging,
            error,
            false,
            true,
            filesystem,
        );
    }
    let staged = match filesystem.read(&order_staging) {
        Ok(bytes) => bytes,
        Err(error) => {
            return rollback_failure(
                plan,
                OrderAdoptionCommitPhase::ReadOrderRecord,
                order_staging,
                error,
                false,
                true,
                filesystem,
            );
        }
    };
    if staged != candidate || OrderDocument::from_json_bytes(&staged).is_err() {
        return rollback_failure_cause(
            plan,
            OrderAdoptionCommitPhase::ValidateOrderRecord,
            order_staging,
            OrderAdoptionCommitFailureCause::CandidateChanged,
            false,
            true,
            filesystem,
        );
    }
    if let Err(error) = filesystem.sync_file(&order_staging) {
        return rollback_failure(
            plan,
            OrderAdoptionCommitPhase::SyncOrderRecord,
            order_staging,
            error,
            false,
            true,
            filesystem,
        );
    }
    let create_readme = plan.readme_kind.is_none();
    if create_readme {
        if let Err(error) = filesystem.write_new(&readme_staging, b"") {
            return rollback_failure(
                plan,
                OrderAdoptionCommitPhase::WriteReadme,
                readme_staging,
                error,
                false,
                true,
                filesystem,
            );
        }
        if let Err(error) = filesystem.sync_file(&readme_staging) {
            return rollback_failure(
                plan,
                OrderAdoptionCommitPhase::SyncReadme,
                readme_staging,
                error,
                false,
                true,
                filesystem,
            );
        }
        match filesystem.link_new_file(&readme_staging, &readme_path) {
            Ok(NewFileLinkOutcome::Linked) => {}
            Ok(NewFileLinkOutcome::DestinationExists) => {
                return rollback_failure_cause(
                    plan,
                    OrderAdoptionCommitPhase::CommitReadme,
                    readme_path,
                    OrderAdoptionCommitFailureCause::DestinationChanged,
                    false,
                    true,
                    filesystem,
                );
            }
            Err(error) => {
                return rollback_failure(
                    plan,
                    OrderAdoptionCommitPhase::CommitReadme,
                    readme_path,
                    error,
                    false,
                    true,
                    filesystem,
                );
            }
        }
    }
    match filesystem.link_new_file(&order_staging, &order_path) {
        Ok(NewFileLinkOutcome::Linked) => {}
        Ok(NewFileLinkOutcome::DestinationExists) => {
            return rollback_failure_cause(
                plan,
                OrderAdoptionCommitPhase::CommitOrderRecord,
                order_path,
                OrderAdoptionCommitFailureCause::DestinationChanged,
                create_readme,
                true,
                filesystem,
            );
        }
        Err(error) => {
            return rollback_failure(
                plan,
                OrderAdoptionCommitPhase::CommitOrderRecord,
                order_path,
                error,
                create_readme,
                true,
                filesystem,
            );
        }
    }

    let mut warnings = Vec::new();
    for path in [&order_staging, &readme_staging] {
        match filesystem.symlink_metadata(path) {
            Ok(None) => {}
            Ok(Some(_)) => {
                if let Err(error) = filesystem.remove_file(path) {
                    warnings.push(OrderAdoptionWarning::StagingCleanupFailed(
                        OrderAdoptionCleanupFailure {
                            location: located(&plan.root, path.clone()),
                            error,
                        },
                    ));
                }
            }
            Err(error) => warnings.push(OrderAdoptionWarning::StagingCleanupFailed(
                OrderAdoptionCleanupFailure {
                    location: located(&plan.root, path.clone()),
                    error,
                },
            )),
        }
    }
    let mut result = None;
    match open_package(&plan.root, filesystem) {
        PackageLoad::Available(package) => {
            result = result_from_package(&order, &package);
            // A refreshed Package that no longer carries the adopted Order cannot
            // confirm it, so the commit is reported over the projected Order instead
            // of silently claiming a confirmation it did not get.
            if result.is_none() {
                warnings.push(OrderAdoptionWarning::RefreshUnavailable);
            }
        }
        PackageLoad::IndexReplacementFailed(failed) => {
            result = result_from_package(&order, failed.canonical_evidence());
            warnings.push(OrderAdoptionWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
        }
        PackageLoad::OperationFailed(failed) => {
            warnings.push(OrderAdoptionWarning::IndexRefreshFailed(
                failed.failure().clone(),
            ));
        }
        _ => warnings.push(OrderAdoptionWarning::RefreshUnavailable),
    }
    let result = result.unwrap_or_else(|| result_without_refresh(&plan, &order, filesystem));
    if warnings.is_empty() {
        MutationReport::Committed { plan, result }
    } else {
        MutationReport::CommittedWithWarning {
            plan,
            result,
            warnings,
        }
    }
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
    root: &Path,
    path: &Path,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, OrderAdoptionPlanError<F::Error>> {
    filesystem
        .symlink_metadata(path)
        .map_err(|error| OrderAdoptionPlanError::FileSystem {
            path: FailedPath::inside(root, path.to_path_buf()),
            error,
        })
}

fn observations_prove_same(planned: FileChangeObservation, current: FileChangeObservation) -> bool {
    if planned.kind() != Some(FileSystemEntryKind::Directory)
        || current.kind() != Some(FileSystemEntryKind::Directory)
    {
        return false;
    }
    matches!(
        (planned.evidence(), current.evidence()),
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
        .map(crate::canonical::ReusableValueProjection::value)
        .collect::<Vec<_>>();
    canonicalize_reusable_value(proposed, &values)
}

fn reusable_values_for_document(
    document: &OrderDocument,
    package: &crate::canonical::CurrentPackageCandidate,
) -> OrderAdoptionReusableValues {
    OrderAdoptionReusableValues {
        orderer: canonical_reusable(document.orderer.as_deref(), package.orderers()),
        ordering_institution: canonical_reusable(
            document.ordering_institution.as_deref(),
            package.ordering_institutions(),
        ),
        project: canonical_reusable(document.project.as_deref(), package.projects()),
        order_group: canonical_reusable(document.order_group.as_deref(), package.order_groups()),
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
/// refused outright when its name has none — [`OrderAdoptionPlanError::NameUnusableCandidate`]
/// — and every path here is that directory joined with one of this module's own literal
/// managed names.
fn operation(
    root: &Path,
    kind: OrderAdoptionOperationKind,
    path: PathBuf,
) -> OrderAdoptionOperation {
    let relative = package_relative_path_text(root, &path)
        .expect("a planned adoption path has a Package-relative spelling");
    OrderAdoptionOperation {
        kind,
        location: Location::within_package(path, relative)
            .expect("a planned adoption path is normalized"),
    }
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
fn preconditions_changed<E>(plan: OrderAdoptionPlan, path: PathBuf) -> OrderAdoptionReport<E> {
    let location = located(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: OrderAdoptionRejection::PreconditionsChanged { location },
    }
}

fn commit_failure<E>(
    plan: OrderAdoptionPlan,
    phase: OrderAdoptionCommitPhase,
    path: PathBuf,
    error: E,
    rollback_failures: Vec<OrderAdoptionCleanupFailure<E>>,
) -> OrderAdoptionReport<E> {
    let location = located(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: OrderAdoptionRejection::CommitFailed {
            failure: OrderAdoptionCommitFailure {
                phase,
                location,
                cause: OrderAdoptionCommitFailureCause::FileSystem(error),
            },
            rollback_failures,
        },
    }
}

fn rollback_failure<F: DirectoryFileSystem>(
    plan: OrderAdoptionPlan,
    phase: OrderAdoptionCommitPhase,
    path: PathBuf,
    error: F::Error,
    created_readme: bool,
    metadata_created: bool,
    filesystem: &F,
) -> OrderAdoptionReport<F::Error> {
    rollback_failure_cause(
        plan,
        phase,
        path,
        OrderAdoptionCommitFailureCause::FileSystem(error),
        created_readme,
        metadata_created,
        filesystem,
    )
}

fn rollback_failure_cause<F: DirectoryFileSystem>(
    plan: OrderAdoptionPlan,
    phase: OrderAdoptionCommitPhase,
    path: PathBuf,
    cause: OrderAdoptionCommitFailureCause<F::Error>,
    created_readme: bool,
    metadata_created: bool,
    filesystem: &F,
) -> OrderAdoptionReport<F::Error> {
    let mut rollback_failures = Vec::new();
    let metadata_directory = plan.directory.join(".spielorders");
    let cleanup_files = [
        created_readme.then(|| plan.directory.join("README.md")),
        Some(plan.directory.join(README_STAGING_NAME)),
        Some(metadata_directory.join(ORDER_STAGING_NAME)),
    ];
    for path in cleanup_files.into_iter().flatten() {
        match filesystem.symlink_metadata(&path) {
            Ok(None) => {}
            Ok(Some(_)) => {
                if let Err(error) = filesystem.remove_file(&path) {
                    rollback_failures.push(OrderAdoptionCleanupFailure {
                        location: located(&plan.root, path),
                        error,
                    });
                }
            }
            Err(error) => {
                rollback_failures.push(OrderAdoptionCleanupFailure {
                    location: located(&plan.root, path),
                    error,
                });
            }
        }
    }
    if metadata_created && let Err(error) = filesystem.remove_directory(&metadata_directory) {
        rollback_failures.push(OrderAdoptionCleanupFailure {
            location: located(&plan.root, metadata_directory),
            error,
        });
    }
    let location = located(&plan.root, path);
    MutationReport::Rejected {
        plan,
        rejection: OrderAdoptionRejection::CommitFailed {
            failure: OrderAdoptionCommitFailure {
                phase,
                location,
                cause,
            },
            rollback_failures,
        },
    }
}

/// The adopted Order as the refreshed Package sees it, when it can still see it. A
/// concurrent external change can remove or damage the Order between the commit and
/// the refresh; the adoption still happened, so that is reported as a committed
/// result the caller can trust rather than raised as a panic.
fn result_from_package(
    order: &OrderDocument,
    package: &crate::canonical::CurrentPackageCandidate,
) -> Option<OrderAdoptionResult> {
    let position = package
        .healthy_orders()
        .iter()
        .position(|candidate| candidate.document().order_id == order.order_id)?;
    Some(OrderAdoptionResult {
        canonical_outcome: CanonicalAdoptionOutcome::Adopted,
        order: package.healthy_orders()[position].clone(),
        vendor: package.vendor_for_order(position).cloned(),
        index: package.index().clone(),
    })
}

fn result_without_refresh<F: FileSystem>(
    plan: &OrderAdoptionPlan,
    order: &OrderDocument,
    filesystem: &F,
) -> OrderAdoptionResult {
    // Adoption leaves a README the directory already held, so the adopted Order
    // carries what that document holds. There is no loaded Order to take it from
    // here, so this reads it the same way the load does.
    let readme_text = order_readme_text(&plan.root, &plan.directory, filesystem);
    OrderAdoptionResult {
        canonical_outcome: CanonicalAdoptionOutcome::Adopted,
        order: project_committed_order(&plan.root, &plan.directory, order, readme_text),
        // The Vendor the plan resolved. Every arm reaching here has no refreshed Package to
        // resolve one against, and the commit's own precondition check confirmed this
        // Vendor is still in the Package.
        vendor: plan.vendor.clone(),
        index: plan
            .prior_index
            .after_committed_membership_change_without_refresh(),
    }
}
