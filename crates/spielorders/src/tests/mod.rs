//! The library's own tests, which reach its modules through the interfaces between them.
//!
//! A module keeps its own `//! Decision sources:` marker; this root declares no tests and
//! so carries none. `cargo test -p spielorders --lib -- tests::<module>::` runs just that
//! module's tests.

#[macro_use]
mod support;

mod artifact_repair;
mod canonical_package_load;
mod capability_composition;
mod draft_saves;
mod entity_search;
mod exhaustive_validation;
mod linked_roots;
mod managed_path_identity;
mod order_adoption;
mod order_alignment;
mod order_creation;
mod order_diagnosis;
mod order_fact_edits;
mod order_reads;
mod order_removal;
mod orders_directory_entry_names;
mod package_index_fast_path;
mod package_index_maintenance;
mod package_index_rebuild;
mod package_index_replacement_failure;
mod package_initialization;
mod package_paths;
mod package_values;
mod plan_evidence;
mod read_entry_points;
mod readme_read;
mod readme_writes;
mod semantic_repair;
mod supporting_material_search;
mod unreadable_order_readme;
mod unsupported_metadata_versions;
mod vendor_lifecycle;
mod vendor_reads;
