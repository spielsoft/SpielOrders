//! The registry of response contract identities.
//!
//! Every command owns an independently evolving response contract. `contract` names
//! the exact command shape and `contract_version` is that response's own consecutive
//! positive integer — independent of Package `metadata_version`, of any managed file's
//! `schema_version`, of the release version, and of every other response contract.
//!
//! This module is the only place this executable spells a `contract` string. A command emits an
//! envelope by naming a variant here, so two commands cannot collide on an identity
//! and no command can spell one inline. That is the whole point of a registry: the
//! identities have to be comparable in one place to be kept distinct.
//!
//! **Nothing below is in force yet.** No `contract_version` advances before the release
//! the implementation roadmap names, under "Versioning before release". Until then a
//! change to a member, a payload, an enum value or a diagnostic's meaning edits version 1
//! and re-records that contract's cells. From that release on, the rules below take
//! effect.
//!
//! **Advancing a version.** An incompatible change to a field, its representation, its
//! requiredness, an enum value, a diagnostic's meaning, or the payload advances only the
//! affected contract. Formatting and human wording advance nothing.
//!
//! **Incompatible is the operative word.** A new enum or diagnostic value reporting a
//! condition previously unreportable is additive and advances nothing, because a consumer
//! branching on the old values already tolerates one it does not know. Changing, removing,
//! or re-meaning an existing value advances the affected contract, as does adding or
//! removing a member or changing its type or requiredness.
//!
//! Decision sources: Define the Rust CLI capability contract.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResponseContract {
    /// Every usage error, which reached no command and so carries no command's identity.
    Usage,
    /// `spielorders agent provision`
    AgentProvision,
    /// `spielorders agent defaults`
    AgentDefaults,
    /// `spielorders init`
    Init,
    /// `spielorders package readme`
    PackageReadme,
    /// `spielorders package show`
    PackageShow,
    /// `spielorders package list orders`
    PackageListOrders,
    /// `spielorders package list vendors`
    PackageListVendors,
    /// `spielorders package values ordering_institution`
    PackageValuesOrderingInstitution,
    /// `spielorders package values orderer`
    PackageValuesOrderer,
    /// `spielorders package values project`
    PackageValuesProject,
    /// `spielorders package values order_group`
    PackageValuesOrderGroup,
    /// `spielorders order show`, and `spielorders order item list`
    OrderShow,
    /// `spielorders order edit`
    OrderEdit,
    /// `spielorders order item add`
    OrderItemAdd,
    /// `spielorders order item edit`
    OrderItemEdit,
    /// `spielorders order item move`
    OrderItemMove,
    /// `spielorders order item remove`
    OrderItemRemove,
    /// `spielorders order validate`
    OrderValidate,
    /// `spielorders order readme`
    OrderReadme,
    /// `spielorders order align`
    OrderAlign,
    /// `spielorders order remove`, with or without `--delete`
    OrderRemove,
    /// `spielorders order merge`
    OrderMerge,
    /// `spielorders apply`
    Apply,
    /// `spielorders vendor show`, and `spielorders vendor contact list`
    VendorShow,
    /// `spielorders add-order`
    AddOrder,
    /// `spielorders adopt-order`
    AdoptOrder,
    /// `spielorders add-vendor`
    AddVendor,
    /// `spielorders vendor edit`
    VendorEdit,
    /// `spielorders vendor hide`
    VendorHide,
    /// `spielorders vendor unhide`
    VendorUnhide,
    /// `spielorders vendor delete`
    VendorDelete,
    /// `spielorders vendor merge`
    VendorMerge,
    /// `spielorders value add`
    ValueAdd,
    /// `spielorders value rename`
    ValueRename,
    /// `spielorders value merge`
    ValueMerge,
    /// `spielorders value delete`
    ValueDelete,
    /// `spielorders vendor contact add`
    VendorContactAdd,
    /// `spielorders vendor contact edit`
    VendorContactEdit,
    /// `spielorders vendor contact move`
    VendorContactMove,
    /// `spielorders validate`
    Validate,
    /// `spielorders repair`, diagnosing and applying alike
    Repair,
    /// `spielorders order repair`
    OrderRepair,
    /// `spielorders index status`
    IndexStatus,
    /// `spielorders index rebuild`
    IndexRebuild,
    /// `spielorders vendor contact remove`
    VendorContactRemove,
    /// `spielorders search orders`
    SearchOrders,
    /// `spielorders search vendors`
    SearchVendors,
    /// `spielorders search supporting-material`
    SearchSupportingMaterial,
}

impl ResponseContract {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Usage => "spielorders.usage",
            Self::AgentProvision => "spielorders.agent.provision",
            Self::AgentDefaults => "spielorders.agent.defaults",
            Self::Init => "spielorders.init",
            Self::PackageReadme => "spielorders.package.readme",
            Self::PackageShow => "spielorders.package.show",
            Self::PackageListOrders => "spielorders.package.list.orders",
            Self::PackageListVendors => "spielorders.package.list.vendors",
            Self::PackageValuesOrderingInstitution => {
                "spielorders.package.values.ordering_institution"
            }
            Self::PackageValuesOrderer => "spielorders.package.values.orderer",
            Self::PackageValuesProject => "spielorders.package.values.project",
            Self::PackageValuesOrderGroup => "spielorders.package.values.order_group",
            Self::OrderShow => "spielorders.order.show",
            Self::OrderEdit => "spielorders.order.edit",
            Self::OrderItemAdd => "spielorders.order.item.add",
            Self::OrderItemEdit => "spielorders.order.item.edit",
            Self::OrderItemMove => "spielorders.order.item.move",
            Self::OrderItemRemove => "spielorders.order.item.remove",
            Self::OrderReadme => "spielorders.order.readme",
            Self::OrderValidate => "spielorders.order.validate",
            Self::OrderAlign => "spielorders.order.align",
            Self::OrderRemove => "spielorders.order.remove",
            Self::OrderMerge => "spielorders.order.merge",
            Self::Apply => "spielorders.apply",
            Self::AddOrder => "spielorders.add_order",
            Self::AdoptOrder => "spielorders.adopt_order",
            Self::VendorShow => "spielorders.vendor.show",
            Self::AddVendor => "spielorders.add_vendor",
            Self::VendorEdit => "spielorders.vendor.edit",
            Self::VendorHide => "spielorders.vendor.hide",
            Self::VendorUnhide => "spielorders.vendor.unhide",
            Self::VendorDelete => "spielorders.vendor.delete",
            Self::VendorMerge => "spielorders.vendor.merge",
            Self::ValueAdd => "spielorders.value.add",
            Self::ValueRename => "spielorders.value.rename",
            Self::ValueMerge => "spielorders.value.merge",
            Self::ValueDelete => "spielorders.value.delete",
            Self::VendorContactAdd => "spielorders.vendor.contact.add",
            Self::VendorContactEdit => "spielorders.vendor.contact.edit",
            Self::VendorContactMove => "spielorders.vendor.contact.move",
            Self::VendorContactRemove => "spielorders.vendor.contact.remove",
            Self::Validate => "spielorders.validate",
            Self::Repair => "spielorders.repair",
            Self::OrderRepair => "spielorders.order.repair",
            Self::IndexStatus => "spielorders.index.status",
            Self::IndexRebuild => "spielorders.index.rebuild",
            Self::SearchOrders => "spielorders.search.orders",
            Self::SearchVendors => "spielorders.search.vendors",
            Self::SearchSupportingMaterial => "spielorders.search.supporting_material",
        }
    }

    pub(crate) fn version(self) -> u32 {
        match self {
            Self::Usage => 1,
            Self::AgentProvision | Self::AgentDefaults => 1,
            Self::Init => 1,
            Self::PackageReadme => 1,
            Self::PackageShow => 1,
            Self::PackageListOrders => 1,
            Self::PackageListVendors => 1,
            Self::PackageValuesOrderingInstitution => 1,
            Self::PackageValuesOrderer => 1,
            Self::PackageValuesProject => 1,
            Self::PackageValuesOrderGroup => 1,
            Self::OrderShow => 1,
            Self::OrderEdit => 1,
            Self::OrderItemAdd => 1,
            Self::OrderItemEdit => 1,
            Self::OrderItemMove => 1,
            Self::OrderItemRemove => 1,
            Self::OrderReadme => 1,
            Self::OrderValidate => 1,
            Self::OrderAlign => 1,
            Self::OrderRemove => 1,
            Self::OrderMerge => 1,
            Self::Apply => 1,
            Self::AddOrder => 1,
            Self::AdoptOrder => 1,
            Self::VendorShow => 1,
            Self::AddVendor => 1,
            Self::VendorEdit => 1,
            Self::VendorHide => 1,
            Self::VendorUnhide => 1,
            Self::VendorDelete => 1,
            Self::VendorMerge => 1,
            Self::ValueAdd | Self::ValueRename | Self::ValueMerge | Self::ValueDelete => 1,
            Self::VendorContactAdd => 1,
            Self::VendorContactEdit => 1,
            Self::VendorContactMove => 1,
            Self::VendorContactRemove => 1,
            Self::Validate => 1,
            Self::Repair => 1,
            Self::OrderRepair => 1,
            Self::IndexStatus => 1,
            Self::IndexRebuild => 1,
            Self::SearchOrders => 1,
            Self::SearchVendors => 1,
            Self::SearchSupportingMaterial => 1,
        }
    }
}
