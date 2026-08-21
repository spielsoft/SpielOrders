//! Platform-independent core boundary for SpielOrders.
//!
//! M1 defines durable metadata, and M2 adds canonical Package read projection,
//! Package Index loading, and separately invoked exhaustive validation.

pub mod canonical;
pub mod filesystem;
pub mod metadata;
pub mod operations;
pub mod order;
pub mod package;
pub mod paths;
pub mod ports;
pub mod readme;

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
