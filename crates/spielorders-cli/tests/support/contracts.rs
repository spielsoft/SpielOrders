//! The response contracts this suite expects the executable to emit.
//!
//! The executable's own registry is private to it, so this is the suite's statement of
//! every identity the decided command surface answers under, its version, and where its
//! golden fixtures live. The whole-surface sweeps check each command's response `contract`
//! against this list, and the fixture tree is walked against it in both directions.
//!
//! One list declares the type and its registry together, so a contract cannot be a
//! variant and missing from [`ResponseContract::REGISTERED`], and every `match` over the
//! type elsewhere in the suite stops compiling until a new contract is placed in it.
//!
//! Decision sources: Define the Rust CLI capability contract.

macro_rules! contracts {
    ($($variant:ident => $name:literal at $version:literal,)*) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum ResponseContract {
            $($variant,)*
        }

        impl ResponseContract {
            /// Every response contract the executable can emit, each exactly once.
            pub const REGISTERED: &'static [Self] = &[$(Self::$variant,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            pub fn version(self) -> u32 {
                match self {
                    $(Self::$variant => $version,)*
                }
            }
        }
    };
}

contracts! {
    Usage => "spielorders.usage" at 1,
    Init => "spielorders.init" at 1,
    AgentProvision => "spielorders.agent.provision" at 1,
    AgentDefaults => "spielorders.agent.defaults" at 1,
    PackageReadme => "spielorders.package.readme" at 1,
    PackageShow => "spielorders.package.show" at 1,
    PackageListOrders => "spielorders.package.list.orders" at 1,
    PackageListVendors => "spielorders.package.list.vendors" at 1,
    PackageValuesOrderingInstitution => "spielorders.package.values.ordering_institution" at 1,
    PackageValuesOrderer => "spielorders.package.values.orderer" at 1,
    PackageValuesProject => "spielorders.package.values.project" at 1,
    PackageValuesOrderGroup => "spielorders.package.values.order_group" at 1,
    OrderShow => "spielorders.order.show" at 1,
    OrderEdit => "spielorders.order.edit" at 1,
    OrderItemAdd => "spielorders.order.item.add" at 1,
    OrderItemEdit => "spielorders.order.item.edit" at 1,
    OrderItemMove => "spielorders.order.item.move" at 1,
    OrderItemRemove => "spielorders.order.item.remove" at 1,
    OrderReadme => "spielorders.order.readme" at 1,
    OrderValidate => "spielorders.order.validate" at 1,
    OrderAlign => "spielorders.order.align" at 1,
    OrderRemove => "spielorders.order.remove" at 1,
    AddOrder => "spielorders.add_order" at 1,
    AdoptOrder => "spielorders.adopt_order" at 1,
    VendorShow => "spielorders.vendor.show" at 1,
    AddVendor => "spielorders.add_vendor" at 1,
    VendorEdit => "spielorders.vendor.edit" at 1,
    VendorHide => "spielorders.vendor.hide" at 1,
    VendorUnhide => "spielorders.vendor.unhide" at 1,
    VendorDelete => "spielorders.vendor.delete" at 1,
    VendorContactAdd => "spielorders.vendor.contact.add" at 1,
    VendorContactEdit => "spielorders.vendor.contact.edit" at 1,
    VendorContactMove => "spielorders.vendor.contact.move" at 1,
    VendorContactRemove => "spielorders.vendor.contact.remove" at 1,
    Validate => "spielorders.validate" at 1,
    Repair => "spielorders.repair" at 1,
    OrderRepair => "spielorders.order.repair" at 1,
    IndexStatus => "spielorders.index.status" at 1,
    IndexRebuild => "spielorders.index.rebuild" at 1,
    SearchOrders => "spielorders.search.orders" at 1,
    SearchVendors => "spielorders.search.vendors" at 1,
    SearchSupportingMaterial => "spielorders.search.supporting_material" at 1,
}

impl ResponseContract {
    /// Where this contract's golden fixtures live, relative to `contracts/cli/`: the
    /// identity without its namespace, dots as directory separators, then the version.
    /// See `docs/development/traceability.md`.
    pub fn fixture_directory(self) -> String {
        let path = self
            .name()
            .strip_prefix("spielorders.")
            .expect("every contract identity is namespaced to this application")
            .replace('.', "/");
        format!("{path}/v{}", self.version())
    }
}
