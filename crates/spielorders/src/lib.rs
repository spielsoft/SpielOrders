//! The SpielOrders library: durable metadata, the canonical Package read projection, the
//! Package Index, entity and Supporting Material search, exhaustive validation, and the
//! mutations and repairs that write.
//!
//! Its only public items are the interface, which every caller uses the same way: the C
//! header, Swift, and the Rust CLI. Everything else is private to this crate. The core
//! reaches its effects through narrow ports, which the platform module implements with
//! the production filesystem, clock, and identities.

mod adoption;
mod agent_setup;
mod alignment;
mod answer;
mod canonical;
mod change;
mod code;
mod creation;
mod creation_batch;
mod diagnosis;
mod digest;
mod drafting;
mod evidence;
mod filesystem;
mod initialization;
mod metadata;
mod mutation;
mod operations;
mod order;
mod order_edits;
mod order_reads;
mod package;
mod paths;
mod ports;
mod readme;
mod readme_writes;
mod reads;
mod removal;
mod repair;
mod repair_cases;
mod search;
mod semantic_repair;
mod supporting_material;
mod vendor_reads;
mod vendors;

mod interface;
pub use interface::*;

mod platform;

#[cfg(test)]
mod tests;

pub(crate) use answer::{Answer, Failure, Warning};
pub(crate) use canonical::Package;
pub(crate) use change::{
    Assignments, Change, ChangeResult, OrderRef, Plan, Report, Scope, VendorRef, commit, plan,
};
pub(crate) use code::Coded;
pub(crate) use diagnosis::Diagnosis;
pub(crate) use drafting::{
    NEW_ORDER_TITLE, draft_date_ordered, draft_item_total, draft_order_total, new_order_defaults,
    new_vendor_name,
};
pub(crate) use order_reads::OrderRead;
pub(crate) use readme::ReadmeRead;
pub(crate) use reads::{diagnose, open, read_order, read_readme, read_vendor, validate};
pub(crate) use supporting_material::{
    ProviderReport, SupportingMaterialSnapshot, search_supporting_material,
};
pub(crate) use vendor_reads::VendorRead;

/// The data a caller reads: records, projections, findings and the repairs they offer,
/// and what a failure or a committed change reports.
pub(crate) mod model {
    pub(crate) use crate::{
        canonical::{DamagedPackageProjection, HealthyOrder, PackageIndexRecord},
        evidence::{EvidenceOutcome, FactChange},
        metadata::{OrderDocument, OrderItem, Vendor, VendorContact},
        operations::FailedPath,
        readme::{Diagnostic, DiagnosticEvidence, Location, Severity},
        repair_cases::RepairAction,
        search::{
            CaseSensitivity, OrderListEntry, ReusableValueKind, SearchQuery, VendorVisibility,
        },
        supporting_material::{
            Completeness, PartialReason, ProviderAnswer, ProviderDiagnostic, ProviderHit,
            ProviderUnavailableReason,
        },
    };
}
