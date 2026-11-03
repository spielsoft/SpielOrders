//! Platform-independent core boundary for SpielOrders.
//!
//! Durable metadata, canonical Package read projection, Package Index loading,
//! deterministic entity search over a loaded Package, separately invoked exhaustive
//! validation, and the domain mutations, repairs and Metadata Upgrades that write.
//! Effects arrive through narrow ports; nothing here reaches a filesystem, a clock, or a
//! platform service on its own.

pub mod adoption;
pub mod alignment;
pub mod canonical;
pub mod creation;
pub mod digest;
pub mod evidence;
pub mod filesystem;
pub mod initialization;
pub mod metadata;
pub mod metadata_repair;
pub mod money;
pub mod mutation;
pub mod operations;
pub mod order;
pub mod order_diagnosis;
pub mod order_edits;
pub mod order_reads;
pub mod package;
pub mod package_diagnosis;
pub mod package_repair;
pub mod paths;
pub mod ports;
pub mod readme;
pub mod readme_writes;
pub mod removal;
pub mod repair_cases;
pub mod search;
pub mod semantic_repair;
pub mod supporting_material;
pub mod upgrade;
pub mod vendor_reads;
pub mod vendors;

/// Identifies the reusable SpielOrders core crate.
pub const CRATE_NAME: &str = "spielorders-core";

// Decision sources: Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation;
// Define the Rust CLI capability contract.
#[cfg(test)]
mod tests {
    #[test]
    fn core_builds_independently() {
        assert_eq!(super::CRATE_NAME, "spielorders-core");
    }
}
