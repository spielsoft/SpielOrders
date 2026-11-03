//! The one integration-test target for `spielorders-cli`.

mod support;

mod cross_command_conformance;
mod displayed_monetary_values;
mod entity_search;
mod initialization;
mod m4_acceptance;
mod m5_acceptance;
mod order_alignment_and_removal;
mod order_and_item_edits;
mod order_creation_and_adoption;
mod order_reads_and_targeting;
mod output_modes_and_streams;
mod package_lists_and_values;
mod package_readme;
mod package_show;
mod platform_effects;
// The provider and the opener this build has for one platform, which a build for a platform
// with no index does not have. What every build owes is `supporting_material_search`.
#[cfg(target_vendor = "apple")]
mod platform_index_and_opener;
mod repair_and_upgrade;
mod response_contract;
mod root_targeting;
mod supporting_material_search;
mod validate_and_index;
mod vendor_contact_mutations;
mod vendor_mutations;
mod vendor_reads;
