//! Every mutation as one request: a [`Change`] names its subject the way a user names it,
//! [`plan`] resolves that subject and plans without writing, and [`commit`] performs the
//! plan through its own re-checked evidence. A plan never committed is the dry run.
//!
//! Decision sources: Define the Rust CLI capability contract; Effect and adapter
//! boundaries; Durable managed-file writes.

use crate::{
    Answer, Coded, Failure, Warning,
    adoption::{
        OrderAdoptionInput, OrderAdoptionPlan, OrderAdoptionResult, OrderItemInput,
        commit_order_adoption, plan_order_adoption,
    },
    agent_setup::{AgentSetupPlan, commit_agent_setup, plan_agent_setup},
    alignment::{
        OrderAlignmentPlan, OrderAlignmentResult, commit_order_alignment, plan_order_alignment,
    },
    batch::{Batch, BatchPlan, BatchResult, commit_batch},
    canonical::{
        PackageIndexRebuildPlan, commit_package_index_rebuild, plan_package_index_rebuild,
    },
    creation::{
        OrderCreationInput, OrderCreationPlan, OrderCreationResult, commit_order_creation,
        plan_order_creation,
    },
    initialization::{
        InitializationOptions, PackageInitializationPlan, commit_package_initialization,
        plan_package_initialization,
    },
    metadata::{OrderDocument, Vendor},
    mutation::{MutationReport, OperationKind, PlannedOperation},
    operations::refuse_linked_root,
    order_edits::{OrderEditPlan, OrderEditResult, commit_order_edit, plan_order_save},
    order_merge::{OrderMergePlan, OrderMergeResult, commit_order_merge, plan_order_merge},
    order_reads::read_order,
    package::ReusableValues,
    ports::Effects,
    readme::Location,
    readme_writes::{
        ReadmeOriginal, ReadmeWritePlan, commit_readme_write, plan_order_readme_write,
        plan_package_readme_write,
    },
    removal::{OrderRemovalPlan, OrderRemovalResult, commit_order_removal, plan_order_removal},
    repair::{RepairPlan, RepairScope, commit_repair, plan_repair},
    repair_cases::{RepairAction, RepairResolution},
    restoration::{OrderRestorePlan, OrderRestoreResult, commit_order_restore, plan_order_restore},
    search::ReusableValueKind,
    value_changes::{
        InUse, ValueChangePlan, ValueChangeResult, ValueRequest, commit_value_change,
        plan_value_change,
    },
    vendor_reads::read_vendor,
    vendors::{
        VendorMutationPlan, VendorMutationResult, commit_vendor_mutation, plan_vendor_creation,
        plan_vendor_delete, plan_vendor_hide, plan_vendor_save, plan_vendor_unhide,
    },
};
use std::path::{Path, PathBuf};

/// One canonical `field=value` assignment. `confirmed` asks that the value be added to the
/// Package's values where the Package lacks it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Assignment {
    pub(crate) field: String,
    pub(crate) value: String,
    pub(crate) confirmed: bool,
}

/// One order-independent, atomic assignment set, the one shape every mutation's facts take.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Assignments {
    pub(crate) entries: Vec<Assignment>,
}

impl Assignments {
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns `value`, reusing an existing canonical spelling or clearing; a value the
    /// Package lacks is refused.
    pub fn assign(self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.with(field.into(), value.into(), false)
    }

    /// Assigns `value`, adding it to the Package's values where the Package lacks it. A
    /// field that holds no such value refuses the request.
    pub fn introduce(self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.with(field.into(), value.into(), true)
    }

    fn with(mut self, field: String, value: String, confirmed: bool) -> Self {
        self.entries.push(Assignment {
            field,
            value,
            confirmed,
        });
        self
    }

    pub(crate) fn pairs(&self) -> Vec<(String, String)> {
        self.entries
            .iter()
            .map(|entry| (entry.field.clone(), entry.value.clone()))
            .collect()
    }
}

/// The Order a caller names: its Directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderRef {
    Directory(PathBuf),
}

impl OrderRef {
    /// The root a caller selected, which is refused when it is a link.
    fn selected(&self) -> &Path {
        match self {
            Self::Directory(directory) => directory,
        }
    }

    /// The Directory of the healthy Order this names, read the way an Order read reads it,
    /// so a mutation refuses a root that is not an Order Directory as a read does.
    pub(crate) fn healthy_directory<F: crate::ports::FileSystem>(
        &self,
        filesystem: &F,
    ) -> Result<PathBuf, Failure> {
        let Self::Directory(directory) = self;
        let read = read_order(directory, filesystem);
        Ok(read
            .result?
            .order
            .paths
            .directory
            .absolute_path
            .to_path_buf())
    }

    /// The Order Directory this names, damaged or not: a Directory is itself, and an ID is
    /// the Directory of the Order it resolves to.
    pub(crate) fn any_directory<F: crate::ports::FileSystem>(
        &self,
        filesystem: &F,
    ) -> Result<PathBuf, Failure> {
        let _ = filesystem;
        let Self::Directory(directory) = self;
        Ok(directory.clone())
    }
}

/// The Vendor a caller names: its Name, resolved by the Package's own comparison and
/// regardless of visibility, or its Vendor ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VendorRef {
    Name(String),
    Id(String),
}

/// What a Package-level or Order-level capability is about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Scope {
    Package(PathBuf),
    Order(OrderRef),
}

impl Scope {
    fn selected(&self) -> &Path {
        match self {
            Self::Package(root) => root,
            Self::Order(order) => order.selected(),
        }
    }
}

/// Every mutation a caller may ask of a Package. An editor passes the `original` it read,
/// and the commit refuses when that original no longer stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Absent paths take the conventional Orders and Removed Orders directories.
    InitializePackage {
        root: PathBuf,
        orders_path: Option<String>,
        removed_path: Option<String>,
    },
    RebuildIndex {
        root: PathBuf,
    },
    ProvisionAgent {
        root: PathBuf,
    },
    /// `items`, where an editor's new Order has any, is its complete ordered Item list,
    /// and every rule its facts and rows break is refused together as
    /// [`Failure::InvalidDraft`].
    CreateOrder {
        root: PathBuf,
        assignments: Assignments,
        items: Vec<Assignments>,
    },
    AdoptOrder {
        root: PathBuf,
        directory: PathBuf,
        assignments: Assignments,
        items: Vec<Assignments>,
    },
    /// An editor's whole draft of an Order: `assignments` over its facts, and `items` as
    /// its complete ordered Item list, one row per Item. Every rule the draft breaks is
    /// refused together as [`Failure::InvalidDraft`].
    SaveOrder {
        order: OrderRef,
        assignments: Assignments,
        items: Vec<Assignments>,
        original: OrderDocument,
    },
    AlignOrder {
        order: OrderRef,
    },
    /// Restores the removed Order at `removed`, in the Removed Orders Directory of the
    /// Package at `root`, to its Orders Directory under the same name.
    RestoreOrder {
        root: PathBuf,
        removed: PathBuf,
    },
    RemoveOrder {
        order: OrderRef,
    },
    /// Merges `orders` in precedence order: the first is kept, and each later one only
    /// fills the facts, Items, and directory entries the result still lacks before what
    /// remains of it is removed.
    MergeOrders {
        orders: Vec<OrderRef>,
    },
    /// `contacts`, where an editor's new Vendor has any, is its complete ordered Contact
    /// list, and every rule its facts and rows break is refused together as
    /// [`Failure::InvalidDraft`].
    CreateVendor {
        root: PathBuf,
        assignments: Assignments,
        contacts: Vec<Assignments>,
    },
    SetVendorHidden {
        root: PathBuf,
        vendor: VendorRef,
        hidden: bool,
    },
    /// Removes a Vendor and its Contacts. `in_use` says what each Order referencing it
    /// references instead: refused while one does, another Vendor, or none.
    DeleteVendor {
        root: PathBuf,
        vendor: VendorRef,
        in_use: InUse<VendorRef>,
    },
    /// Every Order referencing one of `vendors` references `keep` instead, which gains
    /// their Contacts; the others are deleted.
    MergeVendors {
        root: PathBuf,
        vendors: Vec<VendorRef>,
        keep: VendorRef,
    },
    /// Keeps `value` among the Package's values of `kind`; an Order Group belongs to
    /// `project`, or to the Orders with no Project for `None`.
    AddValue {
        root: PathBuf,
        kind: ReusableValueKind,
        project: Option<String>,
        value: String,
    },
    /// Respells `value` as `to` in every Order that uses it; renaming onto another existing
    /// value merges the two under `to`.
    RenameValue {
        root: PathBuf,
        kind: ReusableValueKind,
        project: Option<String>,
        value: String,
        to: String,
    },
    /// Makes `values` one value spelled `keep`, which every Order using one of them uses.
    MergeValues {
        root: PathBuf,
        kind: ReusableValueKind,
        project: Option<String>,
        values: Vec<String>,
        keep: String,
    },
    /// Takes `value` out of the Package; `in_use` says what each Order using it uses
    /// instead.
    DeleteValue {
        root: PathBuf,
        kind: ReusableValueKind,
        project: Option<String>,
        value: String,
        in_use: InUse<String>,
    },
    /// An editor's whole draft of the Vendor `original`: `assignments` over its facts, and
    /// `contacts` as its complete ordered Contact list, one row per Contact. Every rule the
    /// draft breaks is refused together as [`Failure::InvalidDraft`].
    SaveVendor {
        root: PathBuf,
        assignments: Assignments,
        contacts: Vec<Assignments>,
        original: Vendor,
    },
    WriteReadme {
        scope: Scope,
        original: ReadmeOriginal,
        text: String,
    },
    /// Applies one action a diagnosed case in `scope` offers, with the values it requires.
    Repair {
        scope: Scope,
        case: String,
        action: RepairAction,
        values: Assignments,
    },
}

impl Change {
    fn selected(&self) -> &Path {
        match self {
            Self::InitializePackage { root, .. }
            | Self::RebuildIndex { root }
            | Self::ProvisionAgent { root }
            | Self::CreateOrder { root, .. }
            | Self::AdoptOrder { root, .. }
            | Self::CreateVendor { root, .. }
            | Self::SetVendorHidden { root, .. }
            | Self::DeleteVendor { root, .. }
            | Self::MergeVendors { root, .. }
            | Self::AddValue { root, .. }
            | Self::RenameValue { root, .. }
            | Self::MergeValues { root, .. }
            | Self::DeleteValue { root, .. }
            | Self::RestoreOrder { root, .. }
            | Self::SaveVendor { root, .. } => root,
            Self::SaveOrder { order, .. }
            | Self::AlignOrder { order }
            | Self::RemoveOrder { order, .. } => order.selected(),
            Self::MergeOrders { orders } => {
                orders.first().map_or(Path::new(""), OrderRef::selected)
            }
            Self::WriteReadme { scope, .. } | Self::Repair { scope, .. } => scope.selected(),
        }
    }
}

/// A validated change that has not written anything. Its operations and the record it
/// would write are read through its accessors; the evidence its commit re-checks stays
/// private.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan(PlanKind);

#[derive(Clone, Debug, PartialEq)]
enum PlanKind {
    InitializePackage(PackageInitializationPlan),
    ProvisionAgent(Box<AgentSetupPlan>),
    RebuildIndex(Box<PackageIndexRebuildPlan>),
    CreateOrder(Box<OrderCreationPlan>),
    AdoptOrder(Box<OrderAdoptionPlan>),
    EditOrder(Box<OrderEditPlan>),
    AlignOrder(Box<OrderAlignmentPlan>),
    RemoveOrder(Box<OrderRemovalPlan>),
    MergeOrders(Box<OrderMergePlan>),
    Batch(Box<BatchPlan>),
    RestoreOrder(Box<OrderRestorePlan>),
    Vendor(Box<VendorMutationPlan>),
    Values(Box<ValueChangePlan>),
    WriteReadme(Box<ReadmeWritePlan>),
    Repair(Box<RepairPlan>),
}

/// The subject a committed change refreshed, for the changes that have one.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeResult {
    InitializePackage,
    ProvisionAgent,
    RebuildIndex,
    CreateOrder(Box<OrderCreationResult>),
    AdoptOrder(Box<OrderAdoptionResult>),
    EditOrder(Box<OrderEditResult>),
    AlignOrder(Box<OrderAlignmentResult>),
    RemoveOrder(Box<OrderRemovalResult>),
    MergeOrders(Box<OrderMergeResult>),
    Batch(Box<BatchResult>),
    RestoreOrder(Box<OrderRestoreResult>),
    Vendor(Box<VendorMutationResult>),
    Values(Box<ValueChangeResult>),
    WriteReadme,
    Repair,
}

/// What a change did. A commit that was refused is a [`Failure`], not a report.
#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    Unchanged {
        result: ChangeResult,
    },
    Committed {
        result: ChangeResult,
    },
    CommittedWithWarning {
        result: ChangeResult,
        warnings: Vec<Warning>,
    },
}

impl Report {
    fn of<R: Into<ChangeResult>>(report: MutationReport<R>) -> Self {
        Self::with(report, Into::into)
    }

    fn with<R>(report: MutationReport<R>, result: impl FnOnce(R) -> ChangeResult) -> Self {
        match report {
            MutationReport::Unchanged { result: done } => Self::Unchanged {
                result: result(done),
            },
            MutationReport::Committed { result: done } => Self::Committed {
                result: result(done),
            },
            MutationReport::CommittedWithWarning {
                result: done,
                warnings,
            } => Self::CommittedWithWarning {
                result: result(done),
                warnings,
            },
        }
    }
}

fn planned(operations: &[PlannedOperation]) -> Vec<(String, &Location)> {
    operations
        .iter()
        .map(|operation| (operation.kind.code().to_owned(), &operation.location))
        .collect()
}

/// Each capability's own plan and result, as the one [`Plan`] and [`ChangeResult`] they are.
macro_rules! variants {
    ($outer:ident, $kind:ident { $($variant:ident($inner:ty)),* $(,)? }) => {
        $(impl From<$inner> for $outer {
            fn from(inner: $inner) -> Self {
                $kind::$variant(Box::new(inner)).into()
            }
        })*
    };
}

impl Plan {
    fn of(plan: impl Into<PlanKind>) -> Self {
        Self(plan.into())
    }

    /// Each filesystem operation committing this plan performs: the name of its kind, and
    /// the path it acts on.
    pub(crate) fn operations(&self) -> Vec<(String, &Location)> {
        match &self.0 {
            PlanKind::InitializePackage(plan) => plan
                .batch
                .operations
                .iter()
                .map(|operation| {
                    (
                        format!(
                            "{}_{}",
                            operation.kind.code(),
                            operation.destination_kind.code()
                        ),
                        &operation.location,
                    )
                })
                .collect(),
            PlanKind::RebuildIndex(plan) => planned(&plan.operations),
            PlanKind::ProvisionAgent(plan) => plan
                .batch
                .operations
                .iter()
                .map(|operation| {
                    (
                        format!(
                            "{}_{}",
                            operation.kind.code(),
                            operation.destination_kind.code()
                        ),
                        &operation.location,
                    )
                })
                .collect(),
            PlanKind::CreateOrder(plan) => planned(&plan.operations),
            PlanKind::AdoptOrder(plan) => planned(&plan.operations),
            PlanKind::EditOrder(plan) => planned(&plan.operations),
            PlanKind::AlignOrder(plan) => planned(&plan.operations),
            PlanKind::RemoveOrder(plan) => planned(&plan.operations),
            PlanKind::MergeOrders(plan) => planned(&plan.operations),
            PlanKind::Batch(plan) => planned(&plan.operations),
            PlanKind::RestoreOrder(plan) => planned(&plan.operations),
            PlanKind::Vendor(plan) => planned(&plan.operations),
            PlanKind::Values(plan) => planned(&plan.operations),
            PlanKind::WriteReadme(plan) => planned(&plan.operations),
            PlanKind::Repair(plan) => match &**plan {
                RepairPlan::Artifact(plan) => planned(&plan.operations),
                RepairPlan::Semantic(plan) => planned(&plan.operations),
            },
        }
    }

    /// How many Order records committing this plan rewrites.
    pub fn order_count(&self) -> usize {
        match &self.0 {
            PlanKind::Values(plan) => plan.order_count(),
            PlanKind::Batch(plan) => plan.order_count(),
            _ => self
                .operations()
                .iter()
                .filter(|(kind, _)| {
                    [
                        OperationKind::ReplaceOrderRecord.code(),
                        OperationKind::WriteOrderRecord.code(),
                        OperationKind::CommitOrderRecord.code(),
                    ]
                    .contains(&kind.as_str())
                })
                .count(),
        }
    }

    /// For each operation in [`Self::operations`] order, whether it acts on a directory.
    pub(crate) fn operation_directories(&self) -> Vec<bool> {
        match &self.0 {
            PlanKind::MergeOrders(plan) => plan
                .operations
                .iter()
                .map(|operation| operation.directory)
                .collect(),
            _ => vec![false; self.operations().len()],
        }
    }

    pub(crate) fn restored_order_id(&self) -> Option<&str> {
        match &self.0 {
            PlanKind::RestoreOrder(plan) => Some(plan.order_id()),
            _ => None,
        }
    }

    /// The Order record saving this plan writes, the Vendor it references, and how it
    /// spells its reusable values; `None` for a plan that writes no Order record.
    pub(crate) fn order_record(
        &self,
    ) -> Option<(&OrderDocument, Option<&Vendor>, &ReusableValues)> {
        match &self.0 {
            PlanKind::CreateOrder(plan) => Some((
                &plan.facts.document,
                plan.vendor.as_ref(),
                &plan.reusable_values,
            )),
            PlanKind::AdoptOrder(plan) => {
                Some((&plan.document, plan.vendor.as_ref(), &plan.reusable_values))
            }
            PlanKind::EditOrder(plan) => Some((
                &plan.facts.document,
                plan.vendor.as_ref(),
                &plan.reusable_values,
            )),
            _ => None,
        }
    }

    /// The Vendor record saving this plan writes; `None` for a plan that writes none.
    pub(crate) fn vendor_record(&self) -> Option<&Vendor> {
        match &self.0 {
            PlanKind::Vendor(plan) => Some(&plan.subject.document),
            _ => None,
        }
    }
}

variants!(PlanKind, PlanKind {
    ProvisionAgent(AgentSetupPlan),
    RebuildIndex(PackageIndexRebuildPlan),
    CreateOrder(OrderCreationPlan),
    AdoptOrder(OrderAdoptionPlan),
    EditOrder(OrderEditPlan),
    AlignOrder(OrderAlignmentPlan),
    RemoveOrder(OrderRemovalPlan),
    MergeOrders(OrderMergePlan),
    Batch(BatchPlan),
    RestoreOrder(OrderRestorePlan),
    Vendor(VendorMutationPlan),
    Values(ValueChangePlan),
    WriteReadme(ReadmeWritePlan),
    Repair(RepairPlan),
});

variants!(ChangeResult, ChangeResult {
    CreateOrder(OrderCreationResult),
    AdoptOrder(OrderAdoptionResult),
    EditOrder(OrderEditResult),
    AlignOrder(OrderAlignmentResult),
    RemoveOrder(OrderRemovalResult),
    MergeOrders(OrderMergeResult),
    Batch(BatchResult),
    RestoreOrder(OrderRestoreResult),
    Vendor(VendorMutationResult),
    Values(ValueChangeResult),
});

impl From<PackageInitializationPlan> for PlanKind {
    fn from(plan: PackageInitializationPlan) -> Self {
        Self::InitializePackage(plan)
    }
}

/// Plans `change` without writing anything.
///
/// A selected root that is a link is refused before anything else, including any
/// refusal of the change's own input, so nothing behind the link is opened.
pub fn plan<E: Effects>(change: &Change, effects: &E) -> Answer<Plan> {
    let filesystem = effects.file_system();
    if let Err(failure) = refuse_linked_root(change.selected(), filesystem) {
        return Err(failure).into();
    }
    let today = || effects.today();
    match change {
        Change::InitializePackage {
            root,
            orders_path,
            removed_path,
        } => {
            let conventional = InitializationOptions::default();
            let options = InitializationOptions::new(
                orders_path.as_deref().unwrap_or(conventional.orders_path()),
                removed_path
                    .as_deref()
                    .unwrap_or(conventional.removed_path()),
            );
            plan_package_initialization(root, &options, filesystem).map(Plan::of)
        }
        Change::RebuildIndex { root } => plan_package_index_rebuild(root, filesystem).map(Plan::of),
        Change::ProvisionAgent { root } => plan_agent_setup(root, filesystem).map(Plan::of),
        Change::CreateOrder {
            root,
            assignments,
            items,
        } => {
            let input = if items.is_empty() {
                OrderCreationInput::from_assignments(assignments)
            } else {
                OrderCreationInput::from_draft(assignments, items)
            };
            settled(input, |input| {
                plan_order_creation(root, &input, &today, filesystem).map(Plan::of)
            })
        }
        Change::AdoptOrder {
            root,
            directory,
            assignments,
            items,
        } => {
            let input = OrderAdoptionInput::from_assignments(assignments).and_then(|input| {
                items.iter().try_fold(input, |input, item| {
                    Ok(input.item(OrderItemInput::from_assignments(item)?))
                })
            });
            settled(input, |input| {
                plan_order_adoption(root, directory, &input, filesystem).map(Plan::of)
            })
        }
        Change::SaveOrder {
            order,
            assignments,
            items,
            original,
        } => settled(order.healthy_directory(filesystem), |directory| {
            plan_order_save(&directory, assignments, items, &today, filesystem)
                .map(|plan| Plan::of(plan.require_original(Some(original))))
        }),
        Change::AlignOrder { order } => settled(order.healthy_directory(filesystem), |directory| {
            plan_order_alignment(&directory, filesystem).map(Plan::of)
        }),
        Change::RemoveOrder { order } => {
            settled(order.healthy_directory(filesystem), |directory| {
                plan_order_removal(&directory, filesystem).map(Plan::of)
            })
        }
        Change::MergeOrders { orders } => {
            let directories = orders
                .iter()
                .map(|order| order.healthy_directory(filesystem))
                .collect::<Result<Vec<_>, _>>();
            settled(directories, |directories| {
                plan_order_merge(&directories, filesystem).map(Plan::of)
            })
        }
        Change::RestoreOrder { root, removed } => {
            plan_order_restore(root, removed, filesystem).map(Plan::of)
        }
        Change::CreateVendor {
            root,
            assignments,
            contacts,
        } => plan_vendor_creation(root, assignments, contacts, filesystem).map(Plan::of),
        Change::SetVendorHidden {
            root,
            vendor,
            hidden,
        } => settled(vendor_id(root, vendor, filesystem), |id| {
            if *hidden {
                plan_vendor_hide(root, &id, filesystem)
            } else {
                plan_vendor_unhide(root, &id, filesystem)
            }
            .map(Plan::of)
        }),
        Change::DeleteVendor {
            root,
            vendor,
            in_use,
        } => settled(vendor_id(root, vendor, filesystem), |id| {
            let in_use = match in_use {
                InUse::Refuse => return plan_vendor_delete(root, &id, filesystem).map(Plan::of),
                InUse::Replace(replacement) => {
                    vendor_id(root, replacement, filesystem).map(InUse::Replace)
                }
                InUse::Clear => Ok(InUse::Clear),
            };
            settled(in_use, |in_use| {
                plan_value_change(
                    root,
                    &ValueRequest::DeleteVendor { vendor: id, in_use },
                    filesystem,
                )
                .map(Plan::of)
            })
        }),
        Change::MergeVendors {
            root,
            vendors,
            keep,
        } => {
            let resolved = vendors
                .iter()
                .map(|vendor| vendor_id(root, vendor, filesystem))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|vendors| Ok((vendors, vendor_id(root, keep, filesystem)?)));
            settled(resolved, |(vendors, keep)| {
                plan_value_change(
                    root,
                    &ValueRequest::MergeVendors { vendors, keep },
                    filesystem,
                )
                .map(Plan::of)
            })
        }
        Change::AddValue {
            root,
            kind,
            project,
            value,
        } => plan_value_change(
            root,
            &ValueRequest::Add {
                kind: *kind,
                project: project.clone(),
                value: value.clone(),
            },
            filesystem,
        )
        .map(Plan::of),
        Change::RenameValue {
            root,
            kind,
            project,
            value,
            to,
        } => plan_value_change(
            root,
            &ValueRequest::Replace {
                kind: *kind,
                project: project.clone(),
                values: vec![value.clone()],
                with: to.clone(),
            },
            filesystem,
        )
        .map(Plan::of),
        Change::MergeValues {
            root,
            kind,
            project,
            values,
            keep,
        } => plan_value_change(
            root,
            &ValueRequest::Replace {
                kind: *kind,
                project: project.clone(),
                values: values.clone(),
                with: keep.clone(),
            },
            filesystem,
        )
        .map(Plan::of),
        Change::DeleteValue {
            root,
            kind,
            project,
            value,
            in_use,
        } => plan_value_change(
            root,
            &ValueRequest::Delete {
                kind: *kind,
                project: project.clone(),
                value: value.clone(),
                in_use: in_use.clone(),
            },
            filesystem,
        )
        .map(Plan::of),
        Change::SaveVendor {
            root,
            assignments,
            contacts,
            original,
        } => plan_vendor_save(
            root,
            original.vendor_id.as_str(),
            assignments,
            contacts,
            filesystem,
        )
        .map(|plan| Plan::of(plan.require_original(Some(original)))),
        Change::WriteReadme {
            scope,
            original,
            text,
        } => match scope {
            Scope::Package(root) => plan_package_readme_write(root, original, text, filesystem),
            Scope::Order(order) => settled(order.healthy_directory(filesystem), |directory| {
                plan_order_readme_write(&directory, original, text, filesystem)
            }),
        }
        .map(Plan::of),
        Change::Repair {
            scope,
            case,
            action,
            values,
        } => {
            let subject = match scope {
                Scope::Package(root) => Ok((RepairScope::Package, root.clone())),
                Scope::Order(order) => order
                    .any_directory(filesystem)
                    .map(|directory| (RepairScope::Order, directory)),
            };
            settled(subject, |(scope, subject)| {
                settled(
                    RepairResolution::from_assignments(*action, values),
                    |resolution| {
                        plan_repair(scope, &subject, case, &resolution, filesystem).map(Plan::of)
                    },
                )
            })
        }
    }
}

/// Plans committing every change `batch` holds as one write.
pub(crate) fn plan_batch(batch: &Batch) -> Answer<Plan> {
    crate::batch::plan_batch(batch).map(Plan::of)
}

/// Commits `plan`, re-checking the evidence it was planned against before its first
/// effect.
pub fn commit<E: Effects>(plan: Plan, effects: &E) -> Answer<Report> {
    let filesystem = effects.file_system();
    let new_uuid = || effects.new_uuid();
    match plan.0 {
        PlanKind::InitializePackage(plan) => commit_package_initialization(plan, filesystem)
            .map(|report| Report::with(report, |()| ChangeResult::InitializePackage)),
        PlanKind::RebuildIndex(plan) => commit_package_index_rebuild(*plan, filesystem)
            .map(|report| Report::with(report, |()| ChangeResult::RebuildIndex)),
        PlanKind::ProvisionAgent(plan) => commit_agent_setup(*plan, filesystem)
            .map(|report| Report::with(report, |()| ChangeResult::ProvisionAgent)),
        PlanKind::CreateOrder(plan) => {
            commit_order_creation(*plan, &new_uuid, filesystem).map(Report::of)
        }
        PlanKind::AdoptOrder(plan) => {
            commit_order_adoption(*plan, &new_uuid, filesystem).map(Report::of)
        }
        PlanKind::EditOrder(plan) => commit_order_edit(*plan, filesystem).map(Report::of),
        PlanKind::AlignOrder(plan) => commit_order_alignment(*plan, filesystem).map(Report::of),
        PlanKind::RemoveOrder(plan) => commit_order_removal(*plan, effects).map(Report::of),
        PlanKind::RestoreOrder(plan) => commit_order_restore(*plan, filesystem).map(Report::of),
        PlanKind::MergeOrders(plan) => commit_order_merge(*plan, effects).map(Report::of),
        PlanKind::Batch(plan) => commit_batch(*plan, filesystem).map(Report::of),
        PlanKind::Vendor(plan) => {
            commit_vendor_mutation(*plan, &new_uuid, filesystem).map(Report::of)
        }
        PlanKind::Values(plan) => commit_value_change(*plan, filesystem).map(Report::of),
        PlanKind::WriteReadme(plan) => commit_readme_write(*plan, filesystem)
            .map(|report| Report::with(report, |()| ChangeResult::WriteReadme)),
        PlanKind::Repair(plan) => commit_repair(*plan, &new_uuid, filesystem)
            .map(|report| Report::with(report, |()| ChangeResult::Repair)),
    }
}

/// The answer `then` gives for a resolved input, or the refusal that resolved none.
fn settled<T, U>(resolved: Result<T, Failure>, then: impl FnOnce(T) -> Answer<U>) -> Answer<U> {
    match resolved {
        Ok(value) => then(value),
        Err(failure) => Err(failure).into(),
    }
}

/// The Vendor ID `vendor` names in the Package at `root`.
fn vendor_id<F: crate::ports::FileSystem>(
    root: &Path,
    vendor: &VendorRef,
    filesystem: &F,
) -> Result<String, Failure> {
    match vendor {
        VendorRef::Name(_) => Ok(read_vendor(root, vendor, filesystem)?
            .vendor
            .vendor_id
            .as_str()
            .to_owned()),
        VendorRef::Id(id) => Ok(id.clone()),
    }
}
