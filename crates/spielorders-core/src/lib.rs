//! Platform-independent core boundary for SpielOrders.
//!
//! M1 adds the durable metadata vocabulary while Package behavior remains deferred.

pub mod filesystem;
pub mod metadata;
pub mod order;
pub mod package;
pub mod paths;
pub mod ports;

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
