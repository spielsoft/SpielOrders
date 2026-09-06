//! The one integration-test target for `spielorders-core`.
//!
//! Each former `tests/<name>.rs` binary is a module here, so the suite links once
//! instead of once per file. A module keeps its own `//! Decision sources:` marker —
//! this root declares no tests and so carries none — and
//! `cargo test -p spielorders-core --test core -- <module>::` runs just that module's
//! tests. See `docs/development/test-loop.md`.
//!
//! A new `tests/<name>.rs` MUST be declared here. The crate sets `autotests = false`,
//! so a file this list omits is compiled by nothing and run by nothing, and no warning
//! says so.

mod support;

mod atomic_replacement;
mod canonical_package_load;
mod directory_publication;
mod exhaustive_validation;
mod filesystem_change_observation;
mod m3_integration;
mod managed_path_identity;
mod metadata_repair;
mod metadata_upgrade;
mod metadata_v1;
mod multi_file_replacement;
mod order_adoption;
mod order_alignment;
mod order_creation;
mod order_diagnosis;
mod order_fact_edits;
mod order_item_collection;
mod order_reads;
mod order_removal;
mod order_validation;
mod orders_directory_entry_names;
mod package_index_fast_path;
mod package_index_maintenance;
mod package_index_rebuild;
mod package_index_replacement_failure;
mod package_initialization;
mod package_metadata;
mod package_paths;
mod package_repair;
mod plan_evidence;
mod readme_read;
mod readme_writes;
mod semantic_repair;
mod unreadable_order_readme;
mod vendor_contacts;
mod vendor_lifecycle;
mod vendor_reads;
