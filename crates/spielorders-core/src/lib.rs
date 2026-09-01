//! Platform-independent core boundary for SpielOrders.
//!
//! M1 defines durable metadata, and M2 adds canonical Package read projection,
//! Package Index loading, and separately invoked exhaustive validation.

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
pub mod mutation;
pub mod operations;
pub mod order;
pub mod order_diagnosis;
pub mod order_edits;
pub mod package;
pub mod paths;
pub mod ports;
pub mod readme;
pub mod readme_writes;
pub mod removal;
pub mod semantic_repair;
pub mod upgrade;
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
