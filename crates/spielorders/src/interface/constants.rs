//! Every constant the interface passes, each family in its own range, with the name
//! tables that spell each value and read each spelling. Values are never renumbered or reused.

use crate::interface::{SpielordersConstant, SpielordersResult};
use std::ffi::CStr;

// Families.
/// The family of `SPIELORDERS_RESULT_*` constants.
pub const SPIELORDERS_FAMILY_RESULT: SpielordersConstant = SpielordersConstant(100);
/// The family of `SPIELORDERS_FAILURE_*` constants.
pub const SPIELORDERS_FAMILY_FAILURE: SpielordersConstant = SpielordersConstant(101);
/// The family of `SPIELORDERS_WARNING_*` constants.
pub const SPIELORDERS_FAMILY_WARNING: SpielordersConstant = SpielordersConstant(102);
/// The family of `SPIELORDERS_FINDING_*` constants.
pub const SPIELORDERS_FAMILY_FINDING: SpielordersConstant = SpielordersConstant(103);
/// The family of `SPIELORDERS_SEVERITY_*` constants.
pub const SPIELORDERS_FAMILY_SEVERITY: SpielordersConstant = SpielordersConstant(104);
/// The family of `SPIELORDERS_PACKAGE_STATE_*` constants.
pub const SPIELORDERS_FAMILY_PACKAGE_STATE: SpielordersConstant = SpielordersConstant(105);
/// The family of `SPIELORDERS_INDEX_STATE_*` constants.
pub const SPIELORDERS_FAMILY_INDEX_STATE: SpielordersConstant = SpielordersConstant(106);
/// The family of `SPIELORDERS_INDEX_REASON_*` constants.
pub const SPIELORDERS_FAMILY_INDEX_REASON: SpielordersConstant = SpielordersConstant(107);
/// The family of `SPIELORDERS_ORDER_STATUS_*` constants.
pub const SPIELORDERS_FAMILY_ORDER_STATUS: SpielordersConstant = SpielordersConstant(108);
/// The family of `SPIELORDERS_ORDER_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_ORDER_FIELD: SpielordersConstant = SpielordersConstant(109);
/// The family of `SPIELORDERS_ITEM_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_ITEM_FIELD: SpielordersConstant = SpielordersConstant(110);
/// The family of `SPIELORDERS_VENDOR_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_VENDOR_FIELD: SpielordersConstant = SpielordersConstant(111);
/// The family of `SPIELORDERS_CONTACT_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_CONTACT_FIELD: SpielordersConstant = SpielordersConstant(112);
/// The family of `SPIELORDERS_TEXT_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_TEXT_FIELD: SpielordersConstant = SpielordersConstant(113);
/// The family of `SPIELORDERS_VALUE_FIELD_*` constants.
pub const SPIELORDERS_FAMILY_VALUE_FIELD: SpielordersConstant = SpielordersConstant(114);
/// The family of `SPIELORDERS_README_STATE_*` constants.
pub const SPIELORDERS_FAMILY_README_STATE: SpielordersConstant = SpielordersConstant(115);
/// The family of `SPIELORDERS_DRAFT_KIND_*` constants.
pub const SPIELORDERS_FAMILY_DRAFT_KIND: SpielordersConstant = SpielordersConstant(116);
/// The family of `SPIELORDERS_VENDOR_SELECTOR_*` constants.
pub const SPIELORDERS_FAMILY_VENDOR_SELECTOR: SpielordersConstant = SpielordersConstant(117);
/// The family of `SPIELORDERS_OPERATION_*` constants.
pub const SPIELORDERS_FAMILY_OPERATION: SpielordersConstant = SpielordersConstant(119);
/// The family of `SPIELORDERS_CHANGE_*` constants.
pub const SPIELORDERS_FAMILY_CHANGE: SpielordersConstant = SpielordersConstant(120);
/// The family of `SPIELORDERS_REUSE_*` constants.
pub const SPIELORDERS_FAMILY_REUSE: SpielordersConstant = SpielordersConstant(121);
/// The family of `SPIELORDERS_REPORT_*` constants.
pub const SPIELORDERS_FAMILY_REPORT: SpielordersConstant = SpielordersConstant(122);
/// The family of `SPIELORDERS_REPAIR_ACTION_*` constants.
pub const SPIELORDERS_FAMILY_REPAIR_ACTION: SpielordersConstant = SpielordersConstant(123);
/// The family of `SPIELORDERS_REASON_*` constants.
pub const SPIELORDERS_FAMILY_REASON: SpielordersConstant = SpielordersConstant(124);
/// The family of `SPIELORDERS_PROVIDER_STATE_*` constants.
pub const SPIELORDERS_FAMILY_PROVIDER_STATE: SpielordersConstant = SpielordersConstant(125);

/// What a fallible call returns.
/// `ok`. The call did what it was asked.
pub const SPIELORDERS_RESULT_OK: SpielordersResult = SpielordersResult(0);
/// `refused`. The request was refused and nothing was written; the outcome says why.
pub const SPIELORDERS_RESULT_REFUSED: SpielordersResult = SpielordersResult(1);
/// `cancelled`. The caller cancelled the call and nothing was written.
pub const SPIELORDERS_RESULT_CANCELLED: SpielordersResult = SpielordersResult(2);
/// `bad_argument`. An unknown constant, a constant of the wrong family, text that is not UTF-8, or a poisoned Draft.
pub const SPIELORDERS_RESULT_BAD_ARGUMENT: SpielordersResult = SpielordersResult(3);
/// `internal`. The library failed internally; nothing about the request was at fault.
pub const SPIELORDERS_RESULT_INTERNAL: SpielordersResult = SpielordersResult(4);

/// Why a call refused, or why a step inside one failed: an entry code.
/// `root_is_a_link`. The selected root is a link. Links are never followed, so nothing behind it was read, renamed, disposed of, or edited.
pub const SPIELORDERS_FAILURE_ROOT_IS_A_LINK: SpielordersConstant = SpielordersConstant(1000);
/// `unmanaged_package`. The exact path given is a resolved directory and is not a SpielOrders Root.
pub const SPIELORDERS_FAILURE_UNMANAGED_PACKAGE: SpielordersConstant = SpielordersConstant(1001);
/// `package_metadata_unsupported`. The Package declares a metadata version this build does not interpret, so it is diagnostic-only and no repair applies.
pub const SPIELORDERS_FAILURE_PACKAGE_METADATA_UNSUPPORTED: SpielordersConstant =
    SpielordersConstant(1002);
/// `damaged_package`. The Package could be read and not trusted. Its diagnostics are the diagnosis.
pub const SPIELORDERS_FAILURE_DAMAGED_PACKAGE: SpielordersConstant = SpielordersConstant(1003);
/// `package_access_denied`. The filesystem denied access to what had to be observed, read, or written.
pub const SPIELORDERS_FAILURE_PACKAGE_ACCESS_DENIED: SpielordersConstant =
    SpielordersConstant(1004);
/// `package_operation_failed`. A filesystem operation failed for any other reason.
pub const SPIELORDERS_FAILURE_PACKAGE_OPERATION_FAILED: SpielordersConstant =
    SpielordersConstant(1005);
/// `package_invalid`.
pub const SPIELORDERS_FAILURE_PACKAGE_INVALID: SpielordersConstant = SpielordersConstant(1006);
/// `order_invalid`.
pub const SPIELORDERS_FAILURE_ORDER_INVALID: SpielordersConstant = SpielordersConstant(1007);
/// `cancelled`. The caller cancelled this read before it settled.
pub const SPIELORDERS_FAILURE_CANCELLED: SpielordersConstant = SpielordersConstant(1008);
/// `not_an_order_directory`. What stands at the selected path is not an Order Directory.
pub const SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY: SpielordersConstant =
    SpielordersConstant(1009);
/// `order_not_found`. The directory is not one of its Package's Orders. `root` is the Root it was looked for in, where one resolved.
pub const SPIELORDERS_FAILURE_ORDER_NOT_FOUND: SpielordersConstant = SpielordersConstant(1010);
/// `damaged_record`. The Order record could not be read as one. `field` names the first field at fault where the record was still JSON to inspect.
pub const SPIELORDERS_FAILURE_DAMAGED_RECORD: SpielordersConstant = SpielordersConstant(1011);
/// `locator_unusable`. The record's own technical locator is missing or malformed.
pub const SPIELORDERS_FAILURE_LOCATOR_UNUSABLE: SpielordersConstant = SpielordersConstant(1012);
/// `locator_reaches_no_package_holding_this_order`. The locator is well formed and the place it names holds no such Order. `package_root` is the locator as stored.
pub const SPIELORDERS_FAILURE_LOCATOR_REACHES_NO_PACKAGE_HOLDING_THIS_ORDER: SpielordersConstant =
    SpielordersConstant(1013);
/// `damaged_order`. The directory is one of its Package's Damaged Orders; `diagnostics` is the diagnosis the load quarantined at its scope.
pub const SPIELORDERS_FAILURE_DAMAGED_ORDER: SpielordersConstant = SpielordersConstant(1014);
/// `unknown_order_id`. The Package holds no Order carrying the requested Order ID.
pub const SPIELORDERS_FAILURE_UNKNOWN_ORDER_ID: SpielordersConstant = SpielordersConstant(1015);
/// `unknown_vendor`. No Vendor carries this Name.
pub const SPIELORDERS_FAILURE_UNKNOWN_VENDOR: SpielordersConstant = SpielordersConstant(1016);
/// `package_has_no_vendors`. The Package has no Vendors at all to resolve a Name against.
pub const SPIELORDERS_FAILURE_PACKAGE_HAS_NO_VENDORS: SpielordersConstant =
    SpielordersConstant(1017);
/// `unknown_vendor_id`. No Vendor carries this Vendor ID.
pub const SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID: SpielordersConstant = SpielordersConstant(1018);
/// `unresolved_vendor`. The Order references a Vendor ID its Package does not hold.
pub const SPIELORDERS_FAILURE_UNRESOLVED_VENDOR: SpielordersConstant = SpielordersConstant(1019);
/// `vendor_name_in_use`. Another Vendor already carries this Name.
pub const SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE: SpielordersConstant = SpielordersConstant(1020);
/// `missing_required_name`. A Vendor Name is required and blank.
pub const SPIELORDERS_FAILURE_MISSING_REQUIRED_NAME: SpielordersConstant =
    SpielordersConstant(1021);
/// `invalid_aggregate`. The Vendor aggregate this mutation would write does not validate.
pub const SPIELORDERS_FAILURE_INVALID_AGGREGATE: SpielordersConstant = SpielordersConstant(1022);
/// `invalid_assignments`. Every assignment refused, with why.
pub const SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS: SpielordersConstant = SpielordersConstant(1023);
/// `missing_required_facts`. A new Order lacks these required facts.
pub const SPIELORDERS_FAILURE_MISSING_REQUIRED_FACTS: SpielordersConstant =
    SpielordersConstant(1024);
/// `missing_required_item_facts`. A new Item lacks these required facts.
pub const SPIELORDERS_FAILURE_MISSING_REQUIRED_ITEM_FACTS: SpielordersConstant =
    SpielordersConstant(1025);
/// `invalid_fact_value`. A fact's value does not parse as its kind.
pub const SPIELORDERS_FAILURE_INVALID_FACT_VALUE: SpielordersConstant = SpielordersConstant(1026);
/// `invalid_facts`. The record these facts would write does not validate.
pub const SPIELORDERS_FAILURE_INVALID_FACTS: SpielordersConstant = SpielordersConstant(1027);
/// `unconfirmed_reusable_value`. A new Reusable Free-Entry Value was proposed without confirmation.
pub const SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE: SpielordersConstant =
    SpielordersConstant(1028);
/// `invalid_draft`. A whole draft broke these rules, each at the fact or the row it names, the record's own facts first.
pub const SPIELORDERS_FAILURE_INVALID_DRAFT: SpielordersConstant = SpielordersConstant(1029);
/// `invalid_item_position`. A position names no Item of this Order.
pub const SPIELORDERS_FAILURE_INVALID_ITEM_POSITION: SpielordersConstant =
    SpielordersConstant(1030);
/// `invalid_contact_position`. A position names no Contact of this Vendor.
pub const SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION: SpielordersConstant =
    SpielordersConstant(1031);
/// `position_not_a_number`. A position a caller wrote is not a whole number.
pub const SPIELORDERS_FAILURE_POSITION_NOT_A_NUMBER: SpielordersConstant =
    SpielordersConstant(1032);
/// `empty_projected_component`. The Order's facts project a basename with an empty required component.
pub const SPIELORDERS_FAILURE_EMPTY_PROJECTED_COMPONENT: SpielordersConstant =
    SpielordersConstant(1033);
/// `destination_collision`. Something already occupies the destination; nothing invents a suffix.
pub const SPIELORDERS_FAILURE_DESTINATION_COLLISION: SpielordersConstant =
    SpielordersConstant(1034);
/// `staging_collision`. A staging artifact already occupies the stage this plan would create.
pub const SPIELORDERS_FAILURE_STAGING_COLLISION: SpielordersConstant = SpielordersConstant(1035);
/// `not_direct_child`. The adoption candidate is not a direct child of the Orders Directory.
pub const SPIELORDERS_FAILURE_NOT_DIRECT_CHILD: SpielordersConstant = SpielordersConstant(1036);
/// `hidden_candidate`. The adoption candidate is a hidden entry.
pub const SPIELORDERS_FAILURE_HIDDEN_CANDIDATE: SpielordersConstant = SpielordersConstant(1037);
/// `ineligible_candidate`. The adoption candidate is not an ordinary directory.
pub const SPIELORDERS_FAILURE_INELIGIBLE_CANDIDATE: SpielordersConstant = SpielordersConstant(1038);
/// `name_unusable_candidate`. The adoption candidate's name cannot be an Order Directory name.
pub const SPIELORDERS_FAILURE_NAME_UNUSABLE_CANDIDATE: SpielordersConstant =
    SpielordersConstant(1039);
/// `already_an_order`. The adoption candidate already holds an Order.
pub const SPIELORDERS_FAILURE_ALREADY_AN_ORDER: SpielordersConstant = SpielordersConstant(1040);
/// `conflicting_reserved_state`. The adoption candidate holds reserved state that is not an Order.
pub const SPIELORDERS_FAILURE_CONFLICTING_RESERVED_STATE: SpielordersConstant =
    SpielordersConstant(1041);
/// `root_not_ordinary_directory`. The selected root is not an ordinary directory.
pub const SPIELORDERS_FAILURE_ROOT_NOT_ORDINARY_DIRECTORY: SpielordersConstant =
    SpielordersConstant(1042);
/// `already_a_package`. The selected root is already a SpielOrders Root.
pub const SPIELORDERS_FAILURE_ALREADY_A_PACKAGE: SpielordersConstant = SpielordersConstant(1043);
/// `invalid_configured_path`. A configured managed path is not one a Package may configure.
pub const SPIELORDERS_FAILURE_INVALID_CONFIGURED_PATH: SpielordersConstant =
    SpielordersConstant(1044);
/// `managed_paths_overlap`. The two configured managed paths resolve to one directory.
pub const SPIELORDERS_FAILURE_MANAGED_PATHS_OVERLAP: SpielordersConstant =
    SpielordersConstant(1045);
/// `planned_path_collision`. A configured path resolves onto a path this plan creates.
pub const SPIELORDERS_FAILURE_PLANNED_PATH_COLLISION: SpielordersConstant =
    SpielordersConstant(1046);
/// `disallowed_link`. A configured path passes through a link.
pub const SPIELORDERS_FAILURE_DISALLOWED_LINK: SpielordersConstant = SpielordersConstant(1047);
/// `unwritable_destination`. The README destination cannot be written.
pub const SPIELORDERS_FAILURE_UNWRITABLE_DESTINATION: SpielordersConstant =
    SpielordersConstant(1048);
/// `order_readme_text_unavailable`. An Order README exists and its text was not established, so an index entry has nothing truthful to record for it. The Package travels with the refusal.
pub const SPIELORDERS_FAILURE_ORDER_README_TEXT_UNAVAILABLE: SpielordersConstant =
    SpielordersConstant(1049);
/// `preconditions_changed`. The state this plan was made against changed before its first effect.
pub const SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED: SpielordersConstant =
    SpielordersConstant(1050);
/// `plan_evidence_unconfirmed`. The facts this plan was authorized against no longer stand, as the evidence seam answered.
pub const SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED: SpielordersConstant =
    SpielordersConstant(1051);
/// `invalid_generated_identity`. The identity a commit minted is not a valid identity.
pub const SPIELORDERS_FAILURE_INVALID_GENERATED_IDENTITY: SpielordersConstant =
    SpielordersConstant(1052);
/// `generated_identity_collision`. The Order ID a commit minted is already in use.
pub const SPIELORDERS_FAILURE_GENERATED_IDENTITY_COLLISION: SpielordersConstant =
    SpielordersConstant(1053);
/// `generated_vendor_identity_collision`. The Vendor ID a commit minted is already in use.
pub const SPIELORDERS_FAILURE_GENERATED_VENDOR_IDENTITY_COLLISION: SpielordersConstant =
    SpielordersConstant(1054);
/// `commit_failed`. A commit stopped partway and removed everything it created.
pub const SPIELORDERS_FAILURE_COMMIT_FAILED: SpielordersConstant = SpielordersConstant(1055);
/// `commit_failed_cleanup_incomplete`. A commit stopped partway and could not remove what is named.
pub const SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE: SpielordersConstant =
    SpielordersConstant(1056);
/// `candidate_changed`. The candidate a stage holds changed before it was promoted.
pub const SPIELORDERS_FAILURE_CANDIDATE_CHANGED: SpielordersConstant = SpielordersConstant(1057);
/// `destination_changed`. The destination changed between being read and being replaced.
pub const SPIELORDERS_FAILURE_DESTINATION_CHANGED: SpielordersConstant = SpielordersConstant(1058);
/// `candidate_invalid`. The record a commit assembled is not a valid record of its kind.
pub const SPIELORDERS_FAILURE_CANDIDATE_INVALID: SpielordersConstant = SpielordersConstant(1059);
/// `invalid_destination`. The destination names no file a stage can be written beside, or one another write in the same commit already names.
pub const SPIELORDERS_FAILURE_INVALID_DESTINATION: SpielordersConstant = SpielordersConstant(1060);
/// `ownership_uncertain`. What stands there is no longer provably what the commit created, so it is left.
pub const SPIELORDERS_FAILURE_OWNERSHIP_UNCERTAIN: SpielordersConstant = SpielordersConstant(1061);
/// `prior_content_invalid`. What the destination held before does not validate, so it was not written back.
pub const SPIELORDERS_FAILURE_PRIOR_CONTENT_INVALID: SpielordersConstant =
    SpielordersConstant(1062);
/// `index_replacement_failed`. The rebuilt Package Index could not be written; the detail is the failed commit.
pub const SPIELORDERS_FAILURE_INDEX_REPLACEMENT_FAILED: SpielordersConstant =
    SpielordersConstant(1063);
/// `unknown_case`. No diagnosed case carries this handle.
pub const SPIELORDERS_FAILURE_UNKNOWN_CASE: SpielordersConstant = SpielordersConstant(1064);
/// `unauthorized_action`. The diagnosed case does not offer this action.
pub const SPIELORDERS_FAILURE_UNAUTHORIZED_ACTION: SpielordersConstant = SpielordersConstant(1065);
/// `unrecognized_artifact`. The destination is not a managed artifact repair recognizes.
pub const SPIELORDERS_FAILURE_UNRECOGNIZED_ARTIFACT: SpielordersConstant =
    SpielordersConstant(1066);
/// `invalid_candidate`. The candidate does not validate as a record of its kind.
pub const SPIELORDERS_FAILURE_INVALID_CANDIDATE: SpielordersConstant = SpielordersConstant(1067);
/// `candidate_damages_package`. Promoting the candidate would damage the Package.
pub const SPIELORDERS_FAILURE_CANDIDATE_DAMAGES_PACKAGE: SpielordersConstant =
    SpielordersConstant(1068);
/// `staging_evidence_present`. Staging evidence beside the destination must be resolved first.
pub const SPIELORDERS_FAILURE_STAGING_EVIDENCE_PRESENT: SpielordersConstant =
    SpielordersConstant(1069);
/// `evidence_changed`. The evidence the repair was planned against changed.
pub const SPIELORDERS_FAILURE_EVIDENCE_CHANGED: SpielordersConstant = SpielordersConstant(1070);
/// `repair_resolution_missing`. The action requires values that were not supplied.
pub const SPIELORDERS_FAILURE_REPAIR_RESOLUTION_MISSING: SpielordersConstant =
    SpielordersConstant(1071);
/// `repair_resolution_unexpected`. A value was supplied that the action does not take.
pub const SPIELORDERS_FAILURE_REPAIR_RESOLUTION_UNEXPECTED: SpielordersConstant =
    SpielordersConstant(1072);
/// `indexing_disabled`. The platform has an index and it is switched off.
pub const SPIELORDERS_FAILURE_INDEXING_DISABLED: SpielordersConstant = SpielordersConstant(1073);
/// `volume_not_indexed`. The Package's volume is not indexed, or is one the provider does not index.
pub const SPIELORDERS_FAILURE_VOLUME_NOT_INDEXED: SpielordersConstant = SpielordersConstant(1074);
/// `access_denied`. Access to the index, or to the directories an answer would be about, was denied.
pub const SPIELORDERS_FAILURE_ACCESS_DENIED: SpielordersConstant = SpielordersConstant(1075);
/// `query_refused`. The provider refused this query.
pub const SPIELORDERS_FAILURE_QUERY_REFUSED: SpielordersConstant = SpielordersConstant(1076);
/// `unsupported`. This build has no Supporting Material index provider for this platform.
pub const SPIELORDERS_FAILURE_UNSUPPORTED: SpielordersConstant = SpielordersConstant(1077);
/// `panicked`. A commit stopped on an internal failure; `after` is "replaced" when it had already replaced a managed file, and "unchanged" otherwise.
pub const SPIELORDERS_FAILURE_PANICKED: SpielordersConstant = SpielordersConstant(1078);

/// What a committed change warns of beside its result: an entry code.
/// `index_refresh_failed`. The derived Package Index could not be refreshed after the commit.
pub const SPIELORDERS_WARNING_INDEX_REFRESH_FAILED: SpielordersConstant = SpielordersConstant(2000);
/// `refresh_unavailable`. No load reached a current Package holding the committed subject, so the refresh could not run or confirm it.
pub const SPIELORDERS_WARNING_REFRESH_UNAVAILABLE: SpielordersConstant = SpielordersConstant(2001);
/// `rediagnosis_unavailable`. The repaired scope could not be diagnosed again after the commit.
pub const SPIELORDERS_WARNING_REDIAGNOSIS_UNAVAILABLE: SpielordersConstant =
    SpielordersConstant(2002);
/// `staging_cleanup_failed`. A stage the commit created could not be removed after it was promoted.
pub const SPIELORDERS_WARNING_STAGING_CLEANUP_FAILED: SpielordersConstant =
    SpielordersConstant(2003);
/// `locator_update_failed`. A removed Order's technical locator could not be rewritten at its new path; the detail is the failed commit.
pub const SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED: SpielordersConstant =
    SpielordersConstant(2004);

/// What a Package load, a validation, or a diagnosis found: an entry code.
/// `configuration_path_invalid`.
pub const SPIELORDERS_FINDING_CONFIGURATION_PATH_INVALID: SpielordersConstant =
    SpielordersConstant(3000);
/// `configuration_paths_overlap`.
pub const SPIELORDERS_FINDING_CONFIGURATION_PATHS_OVERLAP: SpielordersConstant =
    SpielordersConstant(3001);
/// `configured_orders_directory_name_unusable`.
pub const SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_NAME_UNUSABLE: SpielordersConstant =
    SpielordersConstant(3002);
/// `configured_orders_directory_unavailable`.
pub const SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_UNAVAILABLE: SpielordersConstant =
    SpielordersConstant(3003);
/// `managed_artifact_invalid_encoding`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_ENCODING: SpielordersConstant =
    SpielordersConstant(3004);
/// `managed_artifact_invalid_shape`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE: SpielordersConstant =
    SpielordersConstant(3005);
/// `managed_artifact_invalid_syntax`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SYNTAX: SpielordersConstant =
    SpielordersConstant(3006);
/// `managed_artifact_missing`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_MISSING: SpielordersConstant =
    SpielordersConstant(3007);
/// `managed_artifact_unreadable`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_UNREADABLE: SpielordersConstant =
    SpielordersConstant(3008);
/// `managed_artifact_wrong_kind`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_KIND: SpielordersConstant =
    SpielordersConstant(3009);
/// `managed_artifact_wrong_version`.
pub const SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_VERSION: SpielordersConstant =
    SpielordersConstant(3010);
/// `order_id_duplicate`.
pub const SPIELORDERS_FINDING_ORDER_ID_DUPLICATE: SpielordersConstant = SpielordersConstant(3011);
/// `order_package_locator_mismatch`.
pub const SPIELORDERS_FINDING_ORDER_PACKAGE_LOCATOR_MISMATCH: SpielordersConstant =
    SpielordersConstant(3012);
/// `order_semantic_invalid`.
pub const SPIELORDERS_FINDING_ORDER_SEMANTIC_INVALID: SpielordersConstant =
    SpielordersConstant(3013);
/// `orders_directory_entry_disallowed`.
pub const SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_DISALLOWED: SpielordersConstant =
    SpielordersConstant(3014);
/// `orders_directory_entry_name_unusable`.
pub const SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_NAME_UNUSABLE: SpielordersConstant =
    SpielordersConstant(3015);
/// `readme_unavailable`.
pub const SPIELORDERS_FINDING_README_UNAVAILABLE: SpielordersConstant = SpielordersConstant(3016);
/// `reserved_content_unexpected`.
pub const SPIELORDERS_FINDING_RESERVED_CONTENT_UNEXPECTED: SpielordersConstant =
    SpielordersConstant(3017);
/// `staging_artifact_present`.
pub const SPIELORDERS_FINDING_STAGING_ARTIFACT_PRESENT: SpielordersConstant =
    SpielordersConstant(3018);
/// `vendor_id_duplicate`.
pub const SPIELORDERS_FINDING_VENDOR_ID_DUPLICATE: SpielordersConstant = SpielordersConstant(3019);
/// `vendor_name_duplicate`.
pub const SPIELORDERS_FINDING_VENDOR_NAME_DUPLICATE: SpielordersConstant =
    SpielordersConstant(3020);
/// `vendor_reference_unresolved`.
pub const SPIELORDERS_FINDING_VENDOR_REFERENCE_UNRESOLVED: SpielordersConstant =
    SpielordersConstant(3021);

/// How much an entry matters.
/// `error`. A refusal, or a finding that makes its scope invalid.
pub const SPIELORDERS_SEVERITY_ERROR: SpielordersConstant = SpielordersConstant(4000);
/// `warning`. Something to tell the user that refuses nothing.
pub const SPIELORDERS_SEVERITY_WARNING: SpielordersConstant = SpielordersConstant(4001);

/// Whether an opened Package is trusted.
/// `healthy`. Every capability is available.
pub const SPIELORDERS_PACKAGE_STATE_HEALTHY: SpielordersConstant = SpielordersConstant(5000);
/// `damaged`. Diagnostic-only: only diagnosis, validation, README reads, and repair are available.
pub const SPIELORDERS_PACKAGE_STATE_DAMAGED: SpielordersConstant = SpielordersConstant(5001);

/// The state of the stored Package Index, as opening assessed it.
/// `current`.
pub const SPIELORDERS_INDEX_STATE_CURRENT: SpielordersConstant = SpielordersConstant(6000);
/// `missing`.
pub const SPIELORDERS_INDEX_STATE_MISSING: SpielordersConstant = SpielordersConstant(6001);
/// `malformed`.
pub const SPIELORDERS_INDEX_STATE_MALFORMED: SpielordersConstant = SpielordersConstant(6002);
/// `wrong_version`.
pub const SPIELORDERS_INDEX_STATE_WRONG_VERSION: SpielordersConstant = SpielordersConstant(6003);
/// `stale`.
pub const SPIELORDERS_INDEX_STATE_STALE: SpielordersConstant = SpielordersConstant(6004);
/// `not_assessed`.
pub const SPIELORDERS_INDEX_STATE_NOT_ASSESSED: SpielordersConstant = SpielordersConstant(6005);

/// Why the stored Package Index is stale or malformed.
/// `order_membership_changed`.
pub const SPIELORDERS_INDEX_REASON_ORDER_MEMBERSHIP_CHANGED: SpielordersConstant =
    SpielordersConstant(7000);
/// `order_path_mismatch`.
pub const SPIELORDERS_INDEX_REASON_ORDER_PATH_MISMATCH: SpielordersConstant =
    SpielordersConstant(7001);
/// `managed_input_wrong_kind`.
pub const SPIELORDERS_INDEX_REASON_MANAGED_INPUT_WRONG_KIND: SpielordersConstant =
    SpielordersConstant(7002);
/// `vendors_changed`.
pub const SPIELORDERS_INDEX_REASON_VENDORS_CHANGED: SpielordersConstant = SpielordersConstant(7003);
/// `order_metadata_changed`.
pub const SPIELORDERS_INDEX_REASON_ORDER_METADATA_CHANGED: SpielordersConstant =
    SpielordersConstant(7004);
/// `order_readme_changed`.
pub const SPIELORDERS_INDEX_REASON_ORDER_README_CHANGED: SpielordersConstant =
    SpielordersConstant(7005);
/// `change_evidence_uncertain`.
pub const SPIELORDERS_INDEX_REASON_CHANGE_EVIDENCE_UNCERTAIN: SpielordersConstant =
    SpielordersConstant(7006);
/// `wrong_kind`.
pub const SPIELORDERS_INDEX_REASON_WRONG_KIND: SpielordersConstant = SpielordersConstant(7007);
/// `unreadable`.
pub const SPIELORDERS_INDEX_REASON_UNREADABLE: SpielordersConstant = SpielordersConstant(7008);
/// `invalid_encoding`.
pub const SPIELORDERS_INDEX_REASON_INVALID_ENCODING: SpielordersConstant =
    SpielordersConstant(7009);
/// `invalid_syntax`.
pub const SPIELORDERS_INDEX_REASON_INVALID_SYNTAX: SpielordersConstant = SpielordersConstant(7010);
/// `invalid_shape`.
pub const SPIELORDERS_INDEX_REASON_INVALID_SHAPE: SpielordersConstant = SpielordersConstant(7011);
/// `invalid_semantics`.
pub const SPIELORDERS_INDEX_REASON_INVALID_SEMANTICS: SpielordersConstant =
    SpielordersConstant(7012);
/// `non_deterministic_serialization`.
pub const SPIELORDERS_INDEX_REASON_NON_DETERMINISTIC_SERIALIZATION: SpielordersConstant =
    SpielordersConstant(7013);

/// An Order Status.
/// `in_preparation`.
pub const SPIELORDERS_ORDER_STATUS_IN_PREPARATION: SpielordersConstant = SpielordersConstant(8000);
/// `hold`.
pub const SPIELORDERS_ORDER_STATUS_HOLD: SpielordersConstant = SpielordersConstant(8001);
/// `ordered`.
pub const SPIELORDERS_ORDER_STATUS_ORDERED: SpielordersConstant = SpielordersConstant(8002);
/// `received`.
pub const SPIELORDERS_ORDER_STATUS_RECEIVED: SpielordersConstant = SpielordersConstant(8003);
/// `complete`.
pub const SPIELORDERS_ORDER_STATUS_COMPLETE: SpielordersConstant = SpielordersConstant(8004);

/// A fact of an Order. The first nine are what a draft sets; the rest are read-only.
/// `title`.
pub const SPIELORDERS_ORDER_FIELD_TITLE: SpielordersConstant = SpielordersConstant(9000);
/// `date_ordered`. Text such as 2026-08-22.
pub const SPIELORDERS_ORDER_FIELD_DATE_ORDERED: SpielordersConstant = SpielordersConstant(9001);
/// `status`. An Order Status name, such as ordered.
pub const SPIELORDERS_ORDER_FIELD_STATUS: SpielordersConstant = SpielordersConstant(9002);
/// `vendor`. The Vendor Name, or "" for no Vendor.
pub const SPIELORDERS_ORDER_FIELD_VENDOR: SpielordersConstant = SpielordersConstant(9003);
/// `orderer`.
pub const SPIELORDERS_ORDER_FIELD_ORDERER: SpielordersConstant = SpielordersConstant(9004);
/// `ordering_institution`.
pub const SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION: SpielordersConstant =
    SpielordersConstant(9005);
/// `currency`.
pub const SPIELORDERS_ORDER_FIELD_CURRENCY: SpielordersConstant = SpielordersConstant(9006);
/// `project`.
pub const SPIELORDERS_ORDER_FIELD_PROJECT: SpielordersConstant = SpielordersConstant(9007);
/// `order_group`.
pub const SPIELORDERS_ORDER_FIELD_ORDER_GROUP: SpielordersConstant = SpielordersConstant(9008);
/// `order_id`. Read-only.
pub const SPIELORDERS_ORDER_FIELD_ORDER_ID: SpielordersConstant = SpielordersConstant(9009);
/// `directory_name`. Read-only: the Order Directory's name.
pub const SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME: SpielordersConstant = SpielordersConstant(9010);
/// `total`. Read-only: the Order Total, unrounded.
pub const SPIELORDERS_ORDER_FIELD_TOTAL: SpielordersConstant = SpielordersConstant(9011);
/// `damage`. Read-only, on a damaged list entry: the name of its finding code.
pub const SPIELORDERS_ORDER_FIELD_DAMAGE: SpielordersConstant = SpielordersConstant(9012);
/// `vendor_id`. Read-only: the Vendor ID the Order references, or "" for none.
pub const SPIELORDERS_ORDER_FIELD_VENDOR_ID: SpielordersConstant = SpielordersConstant(9013);

/// A fact of one Order Item row.
/// `description`.
pub const SPIELORDERS_ITEM_FIELD_DESCRIPTION: SpielordersConstant = SpielordersConstant(10000);
/// `quantity`.
pub const SPIELORDERS_ITEM_FIELD_QUANTITY: SpielordersConstant = SpielordersConstant(10001);
/// `unit_price`.
pub const SPIELORDERS_ITEM_FIELD_UNIT_PRICE: SpielordersConstant = SpielordersConstant(10002);
/// `catalog_number`.
pub const SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER: SpielordersConstant = SpielordersConstant(10003);
/// `total`. Read-only: the row total, unrounded.
pub const SPIELORDERS_ITEM_FIELD_TOTAL: SpielordersConstant = SpielordersConstant(10004);

/// A fact of a Vendor.
/// `name`.
pub const SPIELORDERS_VENDOR_FIELD_NAME: SpielordersConstant = SpielordersConstant(11000);
/// `phone`.
pub const SPIELORDERS_VENDOR_FIELD_PHONE: SpielordersConstant = SpielordersConstant(11001);
/// `email`.
pub const SPIELORDERS_VENDOR_FIELD_EMAIL: SpielordersConstant = SpielordersConstant(11002);
/// `website`.
pub const SPIELORDERS_VENDOR_FIELD_WEBSITE: SpielordersConstant = SpielordersConstant(11003);
/// `address`.
pub const SPIELORDERS_VENDOR_FIELD_ADDRESS: SpielordersConstant = SpielordersConstant(11004);
/// `purchasing_notes`.
pub const SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES: SpielordersConstant =
    SpielordersConstant(11005);
/// `vendor_id`. Read-only.
pub const SPIELORDERS_VENDOR_FIELD_VENDOR_ID: SpielordersConstant = SpielordersConstant(11006);
/// `hidden`. Read-only: "true" or "false".
pub const SPIELORDERS_VENDOR_FIELD_HIDDEN: SpielordersConstant = SpielordersConstant(11007);

/// A fact of one Vendor Contact row.
/// `name`.
pub const SPIELORDERS_CONTACT_FIELD_NAME: SpielordersConstant = SpielordersConstant(12000);
/// `role`.
pub const SPIELORDERS_CONTACT_FIELD_ROLE: SpielordersConstant = SpielordersConstant(12001);
/// `phone`.
pub const SPIELORDERS_CONTACT_FIELD_PHONE: SpielordersConstant = SpielordersConstant(12002);
/// `email`.
pub const SPIELORDERS_CONTACT_FIELD_EMAIL: SpielordersConstant = SpielordersConstant(12003);

/// The one field of a TEXT draft.
/// `content`. The whole text of a file.
pub const SPIELORDERS_TEXT_FIELD_CONTENT: SpielordersConstant = SpielordersConstant(13000);

/// A fact of one reusable-value list entry.
/// `value`.
pub const SPIELORDERS_VALUE_FIELD_VALUE: SpielordersConstant = SpielordersConstant(14000);
/// `usage_count`. How many Orders use it, as decimal text.
pub const SPIELORDERS_VALUE_FIELD_USAGE_COUNT: SpielordersConstant = SpielordersConstant(14001);

/// Whether a README could be read.
/// `missing`.
pub const SPIELORDERS_README_STATE_MISSING: SpielordersConstant = SpielordersConstant(15000);
/// `present`.
pub const SPIELORDERS_README_STATE_PRESENT: SpielordersConstant = SpielordersConstant(15001);
/// `unreadable`.
pub const SPIELORDERS_README_STATE_UNREADABLE: SpielordersConstant = SpielordersConstant(15002);

/// What a draft edits.
/// `order`.
pub const SPIELORDERS_DRAFT_KIND_ORDER: SpielordersConstant = SpielordersConstant(16000);
/// `vendor`.
pub const SPIELORDERS_DRAFT_KIND_VENDOR: SpielordersConstant = SpielordersConstant(16001);
/// `text`.
pub const SPIELORDERS_DRAFT_KIND_TEXT: SpielordersConstant = SpielordersConstant(16002);

/// How a Vendor read names its Vendor.
/// `id`. By Vendor ID.
pub const SPIELORDERS_VENDOR_SELECTOR_ID: SpielordersConstant = SpielordersConstant(17000);
/// `name`. By Vendor Name, compared the way the Package compares Names.
pub const SPIELORDERS_VENDOR_SELECTOR_NAME: SpielordersConstant = SpielordersConstant(17001);

/// One filesystem operation a plan will perform.
/// `create_staging_directory`.
pub const SPIELORDERS_OPERATION_CREATE_STAGING_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19000);
/// `create_metadata_directory`.
pub const SPIELORDERS_OPERATION_CREATE_METADATA_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19001);
/// `create_removed_orders_directory`.
pub const SPIELORDERS_OPERATION_CREATE_REMOVED_ORDERS_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19002);
/// `write_order_record`.
pub const SPIELORDERS_OPERATION_WRITE_ORDER_RECORD: SpielordersConstant =
    SpielordersConstant(19003);
/// `write_readme`.
pub const SPIELORDERS_OPERATION_WRITE_README: SpielordersConstant = SpielordersConstant(19004);
/// `stage_order_record`.
pub const SPIELORDERS_OPERATION_STAGE_ORDER_RECORD: SpielordersConstant =
    SpielordersConstant(19005);
/// `stage_readme`.
pub const SPIELORDERS_OPERATION_STAGE_README: SpielordersConstant = SpielordersConstant(19006);
/// `commit_readme`.
pub const SPIELORDERS_OPERATION_COMMIT_README: SpielordersConstant = SpielordersConstant(19007);
/// `commit_order_record`.
pub const SPIELORDERS_OPERATION_COMMIT_ORDER_RECORD: SpielordersConstant =
    SpielordersConstant(19008);
/// `commit_directory`.
pub const SPIELORDERS_OPERATION_COMMIT_DIRECTORY: SpielordersConstant = SpielordersConstant(19009);
/// `remove_staging`.
pub const SPIELORDERS_OPERATION_REMOVE_STAGING: SpielordersConstant = SpielordersConstant(19010);
/// `rename_order_directory`.
pub const SPIELORDERS_OPERATION_RENAME_ORDER_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19011);
/// `move_order_directory`.
pub const SPIELORDERS_OPERATION_MOVE_ORDER_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19012);
/// `replace_order_record`.
pub const SPIELORDERS_OPERATION_REPLACE_ORDER_RECORD: SpielordersConstant =
    SpielordersConstant(19013);
/// `replace_vendor_record`.
pub const SPIELORDERS_OPERATION_REPLACE_VENDOR_RECORD: SpielordersConstant =
    SpielordersConstant(19014);
/// `replace_readme_document`.
pub const SPIELORDERS_OPERATION_REPLACE_README_DOCUMENT: SpielordersConstant =
    SpielordersConstant(19015);
/// `replace_managed_artifact`.
pub const SPIELORDERS_OPERATION_REPLACE_MANAGED_ARTIFACT: SpielordersConstant =
    SpielordersConstant(19016);
/// `promote_staging_candidate`.
pub const SPIELORDERS_OPERATION_PROMOTE_STAGING_CANDIDATE: SpielordersConstant =
    SpielordersConstant(19017);
/// `refresh_package_index`.
pub const SPIELORDERS_OPERATION_REFRESH_PACKAGE_INDEX: SpielordersConstant =
    SpielordersConstant(19018);
/// `create_directory`. Initialization creates a directory.
pub const SPIELORDERS_OPERATION_CREATE_DIRECTORY: SpielordersConstant = SpielordersConstant(19019);
/// `create_file`. Initialization creates a file.
pub const SPIELORDERS_OPERATION_CREATE_FILE: SpielordersConstant = SpielordersConstant(19020);
/// `preserve_directory`. Initialization keeps an existing directory.
pub const SPIELORDERS_OPERATION_PRESERVE_DIRECTORY: SpielordersConstant =
    SpielordersConstant(19021);
/// `preserve_file`. Initialization keeps an existing file.
pub const SPIELORDERS_OPERATION_PRESERVE_FILE: SpielordersConstant = SpielordersConstant(19022);

/// One change a plan makes to a record.
/// `fact`. A fact changes from `before` to `after`; a row of -1 is the record's own fact.
pub const SPIELORDERS_CHANGE_FACT: SpielordersConstant = SpielordersConstant(20000);
/// `row_added`. A row is added at `row`.
pub const SPIELORDERS_CHANGE_ROW_ADDED: SpielordersConstant = SpielordersConstant(20001);
/// `row_removed`. The row at `row` of the read record is removed.
pub const SPIELORDERS_CHANGE_ROW_REMOVED: SpielordersConstant = SpielordersConstant(20002);
/// `row_moved`. The row at `before` of the read record moves to `row`.
pub const SPIELORDERS_CHANGE_ROW_MOVED: SpielordersConstant = SpielordersConstant(20003);
/// `new_identity`. The commit mints a new identity for the record.
pub const SPIELORDERS_CHANGE_NEW_IDENTITY: SpielordersConstant = SpielordersConstant(20004);

/// Whether a reusable value already exists.
/// `reused`. The Package already uses this value.
pub const SPIELORDERS_REUSE_REUSED: SpielordersConstant = SpielordersConstant(21000);
/// `new`. The Package does not yet use this value; saving needs it confirmed.
pub const SPIELORDERS_REUSE_NEW: SpielordersConstant = SpielordersConstant(21001);

/// What a commit did.
/// `committed`. The plan was performed.
pub const SPIELORDERS_REPORT_COMMITTED: SpielordersConstant = SpielordersConstant(22000);
/// `unchanged`. The plan changed nothing, so nothing was written.
pub const SPIELORDERS_REPORT_UNCHANGED: SpielordersConstant = SpielordersConstant(22001);

/// One explicit action a diagnosed case offers.
/// `replace_managed_record`. Accept one user-confirmed complete replacement for parseable metadata.
pub const SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD: SpielordersConstant =
    SpielordersConstant(23000);
/// `promote_staging_candidate`. Promote the exact diagnosed staging candidate.
pub const SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE: SpielordersConstant =
    SpielordersConstant(23001);
/// `assign_existing_vendor`. Point an unresolved Vendor Reference at an existing Vendor.
pub const SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR: SpielordersConstant =
    SpielordersConstant(23002);
/// `create_and_assign_vendor`. Deliberately create one fully specified distinct Vendor and assign it.
pub const SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR: SpielordersConstant =
    SpielordersConstant(23003);
/// `clear_vendor_reference`. Clear the unresolved Vendor Reference outright.
pub const SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE: SpielordersConstant =
    SpielordersConstant(23004);
/// `adopt_new_order_identity`. Adopt a fresh identity for this side of a duplicate Order ID conflict.
pub const SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY: SpielordersConstant =
    SpielordersConstant(23005);
/// `repair_package_locator`. Recompute the technical locator from the physical Package relationship.
pub const SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR: SpielordersConstant =
    SpielordersConstant(23006);

/// Why one assignment was rejected: an entry reason.
/// `unknown_field`.
pub const SPIELORDERS_REASON_UNKNOWN_FIELD: SpielordersConstant = SpielordersConstant(24000);
/// `immutable_identity`.
pub const SPIELORDERS_REASON_IMMUTABLE_IDENTITY: SpielordersConstant = SpielordersConstant(24001);
/// `managed_technical_field`.
pub const SPIELORDERS_REASON_MANAGED_TECHNICAL_FIELD: SpielordersConstant =
    SpielordersConstant(24002);
/// `ordered_collection`.
pub const SPIELORDERS_REASON_ORDERED_COLLECTION: SpielordersConstant = SpielordersConstant(24003);
/// `duplicate`.
pub const SPIELORDERS_REASON_DUPLICATE: SpielordersConstant = SpielordersConstant(24004);
/// `blank_required_value`.
pub const SPIELORDERS_REASON_BLANK_REQUIRED_VALUE: SpielordersConstant = SpielordersConstant(24005);
/// `not_a_reusable_value`.
pub const SPIELORDERS_REASON_NOT_A_REUSABLE_VALUE: SpielordersConstant = SpielordersConstant(24006);
/// `not_a_finite_number`.
pub const SPIELORDERS_REASON_NOT_A_FINITE_NUMBER: SpielordersConstant = SpielordersConstant(24007);

/// What a front end's Supporting Material provider did with one query.
/// `complete`. It looked at everything in scope.
pub const SPIELORDERS_PROVIDER_STATE_COMPLETE: SpielordersConstant = SpielordersConstant(25000);
/// `timed_out`. It looked at part of what is in scope before its bound elapsed.
pub const SPIELORDERS_PROVIDER_STATE_TIMED_OUT: SpielordersConstant = SpielordersConstant(25001);
/// `cancelled`. It looked at part of what is in scope before the caller cancelled.
pub const SPIELORDERS_PROVIDER_STATE_CANCELLED: SpielordersConstant = SpielordersConstant(25002);
/// `indexing_disabled`. It could not look: indexing is disabled for the volume.
pub const SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED: SpielordersConstant =
    SpielordersConstant(25003);
/// `volume_not_indexed`. It could not look: the volume is not indexed.
pub const SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED: SpielordersConstant =
    SpielordersConstant(25004);
/// `access_denied`. It could not look: the directory could not be read.
pub const SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED: SpielordersConstant =
    SpielordersConstant(25005);
/// `query_refused`. It could not look: the query was refused.
pub const SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED: SpielordersConstant =
    SpielordersConstant(25006);

/// Each family: its constant, its name, and the range its members occupy.
pub(crate) const FAMILIES: &[(SpielordersConstant, &CStr, i32)] = &[
    (SPIELORDERS_FAMILY_RESULT, c"result", 0),
    (SPIELORDERS_FAMILY_FAILURE, c"failure", 1000),
    (SPIELORDERS_FAMILY_WARNING, c"warning", 2000),
    (SPIELORDERS_FAMILY_FINDING, c"finding", 3000),
    (SPIELORDERS_FAMILY_SEVERITY, c"severity", 4000),
    (SPIELORDERS_FAMILY_PACKAGE_STATE, c"package_state", 5000),
    (SPIELORDERS_FAMILY_INDEX_STATE, c"index_state", 6000),
    (SPIELORDERS_FAMILY_INDEX_REASON, c"index_reason", 7000),
    (SPIELORDERS_FAMILY_ORDER_STATUS, c"order_status", 8000),
    (SPIELORDERS_FAMILY_ORDER_FIELD, c"order_field", 9000),
    (SPIELORDERS_FAMILY_ITEM_FIELD, c"item_field", 10000),
    (SPIELORDERS_FAMILY_VENDOR_FIELD, c"vendor_field", 11000),
    (SPIELORDERS_FAMILY_CONTACT_FIELD, c"contact_field", 12000),
    (SPIELORDERS_FAMILY_TEXT_FIELD, c"text_field", 13000),
    (SPIELORDERS_FAMILY_VALUE_FIELD, c"value_field", 14000),
    (SPIELORDERS_FAMILY_README_STATE, c"readme_state", 15000),
    (SPIELORDERS_FAMILY_DRAFT_KIND, c"draft_kind", 16000),
    (
        SPIELORDERS_FAMILY_VENDOR_SELECTOR,
        c"vendor_selector",
        17000,
    ),
    (SPIELORDERS_FAMILY_OPERATION, c"operation", 19000),
    (SPIELORDERS_FAMILY_CHANGE, c"change", 20000),
    (SPIELORDERS_FAMILY_REUSE, c"reuse", 21000),
    (SPIELORDERS_FAMILY_REPORT, c"report", 22000),
    (SPIELORDERS_FAMILY_REPAIR_ACTION, c"repair_action", 23000),
    (SPIELORDERS_FAMILY_REASON, c"reason", 24000),
    (SPIELORDERS_FAMILY_PROVIDER_STATE, c"provider_state", 25000),
];

/// Every member constant and its name.
pub(crate) const NAMES: &[(SpielordersConstant, &CStr)] = &[
    (SpielordersConstant(SPIELORDERS_RESULT_OK.0), c"ok"),
    (
        SpielordersConstant(SPIELORDERS_RESULT_REFUSED.0),
        c"refused",
    ),
    (
        SpielordersConstant(SPIELORDERS_RESULT_CANCELLED.0),
        c"cancelled",
    ),
    (
        SpielordersConstant(SPIELORDERS_RESULT_BAD_ARGUMENT.0),
        c"bad_argument",
    ),
    (
        SpielordersConstant(SPIELORDERS_RESULT_INTERNAL.0),
        c"internal",
    ),
    (SPIELORDERS_FAILURE_ROOT_IS_A_LINK, c"root_is_a_link"),
    (SPIELORDERS_FAILURE_UNMANAGED_PACKAGE, c"unmanaged_package"),
    (
        SPIELORDERS_FAILURE_PACKAGE_METADATA_UNSUPPORTED,
        c"package_metadata_unsupported",
    ),
    (SPIELORDERS_FAILURE_DAMAGED_PACKAGE, c"damaged_package"),
    (
        SPIELORDERS_FAILURE_PACKAGE_ACCESS_DENIED,
        c"package_access_denied",
    ),
    (
        SPIELORDERS_FAILURE_PACKAGE_OPERATION_FAILED,
        c"package_operation_failed",
    ),
    (SPIELORDERS_FAILURE_PACKAGE_INVALID, c"package_invalid"),
    (SPIELORDERS_FAILURE_ORDER_INVALID, c"order_invalid"),
    (SPIELORDERS_FAILURE_CANCELLED, c"cancelled"),
    (
        SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY,
        c"not_an_order_directory",
    ),
    (SPIELORDERS_FAILURE_ORDER_NOT_FOUND, c"order_not_found"),
    (SPIELORDERS_FAILURE_DAMAGED_RECORD, c"damaged_record"),
    (SPIELORDERS_FAILURE_LOCATOR_UNUSABLE, c"locator_unusable"),
    (
        SPIELORDERS_FAILURE_LOCATOR_REACHES_NO_PACKAGE_HOLDING_THIS_ORDER,
        c"locator_reaches_no_package_holding_this_order",
    ),
    (SPIELORDERS_FAILURE_DAMAGED_ORDER, c"damaged_order"),
    (SPIELORDERS_FAILURE_UNKNOWN_ORDER_ID, c"unknown_order_id"),
    (SPIELORDERS_FAILURE_UNKNOWN_VENDOR, c"unknown_vendor"),
    (
        SPIELORDERS_FAILURE_PACKAGE_HAS_NO_VENDORS,
        c"package_has_no_vendors",
    ),
    (SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID, c"unknown_vendor_id"),
    (SPIELORDERS_FAILURE_UNRESOLVED_VENDOR, c"unresolved_vendor"),
    (
        SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE,
        c"vendor_name_in_use",
    ),
    (
        SPIELORDERS_FAILURE_MISSING_REQUIRED_NAME,
        c"missing_required_name",
    ),
    (SPIELORDERS_FAILURE_INVALID_AGGREGATE, c"invalid_aggregate"),
    (
        SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS,
        c"invalid_assignments",
    ),
    (
        SPIELORDERS_FAILURE_MISSING_REQUIRED_FACTS,
        c"missing_required_facts",
    ),
    (
        SPIELORDERS_FAILURE_MISSING_REQUIRED_ITEM_FACTS,
        c"missing_required_item_facts",
    ),
    (
        SPIELORDERS_FAILURE_INVALID_FACT_VALUE,
        c"invalid_fact_value",
    ),
    (SPIELORDERS_FAILURE_INVALID_FACTS, c"invalid_facts"),
    (
        SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE,
        c"unconfirmed_reusable_value",
    ),
    (SPIELORDERS_FAILURE_INVALID_DRAFT, c"invalid_draft"),
    (
        SPIELORDERS_FAILURE_INVALID_ITEM_POSITION,
        c"invalid_item_position",
    ),
    (
        SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION,
        c"invalid_contact_position",
    ),
    (
        SPIELORDERS_FAILURE_POSITION_NOT_A_NUMBER,
        c"position_not_a_number",
    ),
    (
        SPIELORDERS_FAILURE_EMPTY_PROJECTED_COMPONENT,
        c"empty_projected_component",
    ),
    (
        SPIELORDERS_FAILURE_DESTINATION_COLLISION,
        c"destination_collision",
    ),
    (SPIELORDERS_FAILURE_STAGING_COLLISION, c"staging_collision"),
    (SPIELORDERS_FAILURE_NOT_DIRECT_CHILD, c"not_direct_child"),
    (SPIELORDERS_FAILURE_HIDDEN_CANDIDATE, c"hidden_candidate"),
    (
        SPIELORDERS_FAILURE_INELIGIBLE_CANDIDATE,
        c"ineligible_candidate",
    ),
    (
        SPIELORDERS_FAILURE_NAME_UNUSABLE_CANDIDATE,
        c"name_unusable_candidate",
    ),
    (SPIELORDERS_FAILURE_ALREADY_AN_ORDER, c"already_an_order"),
    (
        SPIELORDERS_FAILURE_CONFLICTING_RESERVED_STATE,
        c"conflicting_reserved_state",
    ),
    (
        SPIELORDERS_FAILURE_ROOT_NOT_ORDINARY_DIRECTORY,
        c"root_not_ordinary_directory",
    ),
    (SPIELORDERS_FAILURE_ALREADY_A_PACKAGE, c"already_a_package"),
    (
        SPIELORDERS_FAILURE_INVALID_CONFIGURED_PATH,
        c"invalid_configured_path",
    ),
    (
        SPIELORDERS_FAILURE_MANAGED_PATHS_OVERLAP,
        c"managed_paths_overlap",
    ),
    (
        SPIELORDERS_FAILURE_PLANNED_PATH_COLLISION,
        c"planned_path_collision",
    ),
    (SPIELORDERS_FAILURE_DISALLOWED_LINK, c"disallowed_link"),
    (
        SPIELORDERS_FAILURE_UNWRITABLE_DESTINATION,
        c"unwritable_destination",
    ),
    (
        SPIELORDERS_FAILURE_ORDER_README_TEXT_UNAVAILABLE,
        c"order_readme_text_unavailable",
    ),
    (
        SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED,
        c"preconditions_changed",
    ),
    (
        SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED,
        c"plan_evidence_unconfirmed",
    ),
    (
        SPIELORDERS_FAILURE_INVALID_GENERATED_IDENTITY,
        c"invalid_generated_identity",
    ),
    (
        SPIELORDERS_FAILURE_GENERATED_IDENTITY_COLLISION,
        c"generated_identity_collision",
    ),
    (
        SPIELORDERS_FAILURE_GENERATED_VENDOR_IDENTITY_COLLISION,
        c"generated_vendor_identity_collision",
    ),
    (SPIELORDERS_FAILURE_COMMIT_FAILED, c"commit_failed"),
    (
        SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE,
        c"commit_failed_cleanup_incomplete",
    ),
    (SPIELORDERS_FAILURE_CANDIDATE_CHANGED, c"candidate_changed"),
    (
        SPIELORDERS_FAILURE_DESTINATION_CHANGED,
        c"destination_changed",
    ),
    (SPIELORDERS_FAILURE_CANDIDATE_INVALID, c"candidate_invalid"),
    (
        SPIELORDERS_FAILURE_INVALID_DESTINATION,
        c"invalid_destination",
    ),
    (
        SPIELORDERS_FAILURE_OWNERSHIP_UNCERTAIN,
        c"ownership_uncertain",
    ),
    (
        SPIELORDERS_FAILURE_PRIOR_CONTENT_INVALID,
        c"prior_content_invalid",
    ),
    (
        SPIELORDERS_FAILURE_INDEX_REPLACEMENT_FAILED,
        c"index_replacement_failed",
    ),
    (SPIELORDERS_FAILURE_UNKNOWN_CASE, c"unknown_case"),
    (
        SPIELORDERS_FAILURE_UNAUTHORIZED_ACTION,
        c"unauthorized_action",
    ),
    (
        SPIELORDERS_FAILURE_UNRECOGNIZED_ARTIFACT,
        c"unrecognized_artifact",
    ),
    (SPIELORDERS_FAILURE_INVALID_CANDIDATE, c"invalid_candidate"),
    (
        SPIELORDERS_FAILURE_CANDIDATE_DAMAGES_PACKAGE,
        c"candidate_damages_package",
    ),
    (
        SPIELORDERS_FAILURE_STAGING_EVIDENCE_PRESENT,
        c"staging_evidence_present",
    ),
    (SPIELORDERS_FAILURE_EVIDENCE_CHANGED, c"evidence_changed"),
    (
        SPIELORDERS_FAILURE_REPAIR_RESOLUTION_MISSING,
        c"repair_resolution_missing",
    ),
    (
        SPIELORDERS_FAILURE_REPAIR_RESOLUTION_UNEXPECTED,
        c"repair_resolution_unexpected",
    ),
    (SPIELORDERS_FAILURE_INDEXING_DISABLED, c"indexing_disabled"),
    (
        SPIELORDERS_FAILURE_VOLUME_NOT_INDEXED,
        c"volume_not_indexed",
    ),
    (SPIELORDERS_FAILURE_ACCESS_DENIED, c"access_denied"),
    (SPIELORDERS_FAILURE_QUERY_REFUSED, c"query_refused"),
    (SPIELORDERS_FAILURE_UNSUPPORTED, c"unsupported"),
    (SPIELORDERS_FAILURE_PANICKED, c"panicked"),
    (
        SPIELORDERS_WARNING_INDEX_REFRESH_FAILED,
        c"index_refresh_failed",
    ),
    (
        SPIELORDERS_WARNING_REFRESH_UNAVAILABLE,
        c"refresh_unavailable",
    ),
    (
        SPIELORDERS_WARNING_REDIAGNOSIS_UNAVAILABLE,
        c"rediagnosis_unavailable",
    ),
    (
        SPIELORDERS_WARNING_STAGING_CLEANUP_FAILED,
        c"staging_cleanup_failed",
    ),
    (
        SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED,
        c"locator_update_failed",
    ),
    (
        SPIELORDERS_FINDING_CONFIGURATION_PATH_INVALID,
        c"configuration_path_invalid",
    ),
    (
        SPIELORDERS_FINDING_CONFIGURATION_PATHS_OVERLAP,
        c"configuration_paths_overlap",
    ),
    (
        SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_NAME_UNUSABLE,
        c"configured_orders_directory_name_unusable",
    ),
    (
        SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_UNAVAILABLE,
        c"configured_orders_directory_unavailable",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_ENCODING,
        c"managed_artifact_invalid_encoding",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE,
        c"managed_artifact_invalid_shape",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SYNTAX,
        c"managed_artifact_invalid_syntax",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_MISSING,
        c"managed_artifact_missing",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_UNREADABLE,
        c"managed_artifact_unreadable",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_KIND,
        c"managed_artifact_wrong_kind",
    ),
    (
        SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_VERSION,
        c"managed_artifact_wrong_version",
    ),
    (
        SPIELORDERS_FINDING_ORDER_ID_DUPLICATE,
        c"order_id_duplicate",
    ),
    (
        SPIELORDERS_FINDING_ORDER_PACKAGE_LOCATOR_MISMATCH,
        c"order_package_locator_mismatch",
    ),
    (
        SPIELORDERS_FINDING_ORDER_SEMANTIC_INVALID,
        c"order_semantic_invalid",
    ),
    (
        SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_DISALLOWED,
        c"orders_directory_entry_disallowed",
    ),
    (
        SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_NAME_UNUSABLE,
        c"orders_directory_entry_name_unusable",
    ),
    (
        SPIELORDERS_FINDING_README_UNAVAILABLE,
        c"readme_unavailable",
    ),
    (
        SPIELORDERS_FINDING_RESERVED_CONTENT_UNEXPECTED,
        c"reserved_content_unexpected",
    ),
    (
        SPIELORDERS_FINDING_STAGING_ARTIFACT_PRESENT,
        c"staging_artifact_present",
    ),
    (
        SPIELORDERS_FINDING_VENDOR_ID_DUPLICATE,
        c"vendor_id_duplicate",
    ),
    (
        SPIELORDERS_FINDING_VENDOR_NAME_DUPLICATE,
        c"vendor_name_duplicate",
    ),
    (
        SPIELORDERS_FINDING_VENDOR_REFERENCE_UNRESOLVED,
        c"vendor_reference_unresolved",
    ),
    (SPIELORDERS_SEVERITY_ERROR, c"error"),
    (SPIELORDERS_SEVERITY_WARNING, c"warning"),
    (SPIELORDERS_PACKAGE_STATE_HEALTHY, c"healthy"),
    (SPIELORDERS_PACKAGE_STATE_DAMAGED, c"damaged"),
    (SPIELORDERS_INDEX_STATE_CURRENT, c"current"),
    (SPIELORDERS_INDEX_STATE_MISSING, c"missing"),
    (SPIELORDERS_INDEX_STATE_MALFORMED, c"malformed"),
    (SPIELORDERS_INDEX_STATE_WRONG_VERSION, c"wrong_version"),
    (SPIELORDERS_INDEX_STATE_STALE, c"stale"),
    (SPIELORDERS_INDEX_STATE_NOT_ASSESSED, c"not_assessed"),
    (
        SPIELORDERS_INDEX_REASON_ORDER_MEMBERSHIP_CHANGED,
        c"order_membership_changed",
    ),
    (
        SPIELORDERS_INDEX_REASON_ORDER_PATH_MISMATCH,
        c"order_path_mismatch",
    ),
    (
        SPIELORDERS_INDEX_REASON_MANAGED_INPUT_WRONG_KIND,
        c"managed_input_wrong_kind",
    ),
    (SPIELORDERS_INDEX_REASON_VENDORS_CHANGED, c"vendors_changed"),
    (
        SPIELORDERS_INDEX_REASON_ORDER_METADATA_CHANGED,
        c"order_metadata_changed",
    ),
    (
        SPIELORDERS_INDEX_REASON_ORDER_README_CHANGED,
        c"order_readme_changed",
    ),
    (
        SPIELORDERS_INDEX_REASON_CHANGE_EVIDENCE_UNCERTAIN,
        c"change_evidence_uncertain",
    ),
    (SPIELORDERS_INDEX_REASON_WRONG_KIND, c"wrong_kind"),
    (SPIELORDERS_INDEX_REASON_UNREADABLE, c"unreadable"),
    (
        SPIELORDERS_INDEX_REASON_INVALID_ENCODING,
        c"invalid_encoding",
    ),
    (SPIELORDERS_INDEX_REASON_INVALID_SYNTAX, c"invalid_syntax"),
    (SPIELORDERS_INDEX_REASON_INVALID_SHAPE, c"invalid_shape"),
    (
        SPIELORDERS_INDEX_REASON_INVALID_SEMANTICS,
        c"invalid_semantics",
    ),
    (
        SPIELORDERS_INDEX_REASON_NON_DETERMINISTIC_SERIALIZATION,
        c"non_deterministic_serialization",
    ),
    (SPIELORDERS_ORDER_STATUS_IN_PREPARATION, c"in_preparation"),
    (SPIELORDERS_ORDER_STATUS_HOLD, c"hold"),
    (SPIELORDERS_ORDER_STATUS_ORDERED, c"ordered"),
    (SPIELORDERS_ORDER_STATUS_RECEIVED, c"received"),
    (SPIELORDERS_ORDER_STATUS_COMPLETE, c"complete"),
    (SPIELORDERS_ORDER_FIELD_TITLE, c"title"),
    (SPIELORDERS_ORDER_FIELD_DATE_ORDERED, c"date_ordered"),
    (SPIELORDERS_ORDER_FIELD_STATUS, c"status"),
    (SPIELORDERS_ORDER_FIELD_VENDOR, c"vendor"),
    (SPIELORDERS_ORDER_FIELD_ORDERER, c"orderer"),
    (
        SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
        c"ordering_institution",
    ),
    (SPIELORDERS_ORDER_FIELD_CURRENCY, c"currency"),
    (SPIELORDERS_ORDER_FIELD_PROJECT, c"project"),
    (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, c"order_group"),
    (SPIELORDERS_ORDER_FIELD_ORDER_ID, c"order_id"),
    (SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME, c"directory_name"),
    (SPIELORDERS_ORDER_FIELD_TOTAL, c"total"),
    (SPIELORDERS_ORDER_FIELD_DAMAGE, c"damage"),
    (SPIELORDERS_ORDER_FIELD_VENDOR_ID, c"vendor_id"),
    (SPIELORDERS_ITEM_FIELD_DESCRIPTION, c"description"),
    (SPIELORDERS_ITEM_FIELD_QUANTITY, c"quantity"),
    (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, c"unit_price"),
    (SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, c"catalog_number"),
    (SPIELORDERS_ITEM_FIELD_TOTAL, c"total"),
    (SPIELORDERS_VENDOR_FIELD_NAME, c"name"),
    (SPIELORDERS_VENDOR_FIELD_PHONE, c"phone"),
    (SPIELORDERS_VENDOR_FIELD_EMAIL, c"email"),
    (SPIELORDERS_VENDOR_FIELD_WEBSITE, c"website"),
    (SPIELORDERS_VENDOR_FIELD_ADDRESS, c"address"),
    (
        SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES,
        c"purchasing_notes",
    ),
    (SPIELORDERS_VENDOR_FIELD_VENDOR_ID, c"vendor_id"),
    (SPIELORDERS_VENDOR_FIELD_HIDDEN, c"hidden"),
    (SPIELORDERS_CONTACT_FIELD_NAME, c"name"),
    (SPIELORDERS_CONTACT_FIELD_ROLE, c"role"),
    (SPIELORDERS_CONTACT_FIELD_PHONE, c"phone"),
    (SPIELORDERS_CONTACT_FIELD_EMAIL, c"email"),
    (SPIELORDERS_TEXT_FIELD_CONTENT, c"content"),
    (SPIELORDERS_VALUE_FIELD_VALUE, c"value"),
    (SPIELORDERS_VALUE_FIELD_USAGE_COUNT, c"usage_count"),
    (SPIELORDERS_README_STATE_MISSING, c"missing"),
    (SPIELORDERS_README_STATE_PRESENT, c"present"),
    (SPIELORDERS_README_STATE_UNREADABLE, c"unreadable"),
    (SPIELORDERS_DRAFT_KIND_ORDER, c"order"),
    (SPIELORDERS_DRAFT_KIND_VENDOR, c"vendor"),
    (SPIELORDERS_DRAFT_KIND_TEXT, c"text"),
    (SPIELORDERS_VENDOR_SELECTOR_ID, c"id"),
    (SPIELORDERS_VENDOR_SELECTOR_NAME, c"name"),
    (
        SPIELORDERS_OPERATION_CREATE_STAGING_DIRECTORY,
        c"create_staging_directory",
    ),
    (
        SPIELORDERS_OPERATION_CREATE_METADATA_DIRECTORY,
        c"create_metadata_directory",
    ),
    (
        SPIELORDERS_OPERATION_CREATE_REMOVED_ORDERS_DIRECTORY,
        c"create_removed_orders_directory",
    ),
    (
        SPIELORDERS_OPERATION_WRITE_ORDER_RECORD,
        c"write_order_record",
    ),
    (SPIELORDERS_OPERATION_WRITE_README, c"write_readme"),
    (
        SPIELORDERS_OPERATION_STAGE_ORDER_RECORD,
        c"stage_order_record",
    ),
    (SPIELORDERS_OPERATION_STAGE_README, c"stage_readme"),
    (SPIELORDERS_OPERATION_COMMIT_README, c"commit_readme"),
    (
        SPIELORDERS_OPERATION_COMMIT_ORDER_RECORD,
        c"commit_order_record",
    ),
    (SPIELORDERS_OPERATION_COMMIT_DIRECTORY, c"commit_directory"),
    (SPIELORDERS_OPERATION_REMOVE_STAGING, c"remove_staging"),
    (
        SPIELORDERS_OPERATION_RENAME_ORDER_DIRECTORY,
        c"rename_order_directory",
    ),
    (
        SPIELORDERS_OPERATION_MOVE_ORDER_DIRECTORY,
        c"move_order_directory",
    ),
    (
        SPIELORDERS_OPERATION_REPLACE_ORDER_RECORD,
        c"replace_order_record",
    ),
    (
        SPIELORDERS_OPERATION_REPLACE_VENDOR_RECORD,
        c"replace_vendor_record",
    ),
    (
        SPIELORDERS_OPERATION_REPLACE_README_DOCUMENT,
        c"replace_readme_document",
    ),
    (
        SPIELORDERS_OPERATION_REPLACE_MANAGED_ARTIFACT,
        c"replace_managed_artifact",
    ),
    (
        SPIELORDERS_OPERATION_PROMOTE_STAGING_CANDIDATE,
        c"promote_staging_candidate",
    ),
    (
        SPIELORDERS_OPERATION_REFRESH_PACKAGE_INDEX,
        c"refresh_package_index",
    ),
    (SPIELORDERS_OPERATION_CREATE_DIRECTORY, c"create_directory"),
    (SPIELORDERS_OPERATION_CREATE_FILE, c"create_file"),
    (
        SPIELORDERS_OPERATION_PRESERVE_DIRECTORY,
        c"preserve_directory",
    ),
    (SPIELORDERS_OPERATION_PRESERVE_FILE, c"preserve_file"),
    (SPIELORDERS_CHANGE_FACT, c"fact"),
    (SPIELORDERS_CHANGE_ROW_ADDED, c"row_added"),
    (SPIELORDERS_CHANGE_ROW_REMOVED, c"row_removed"),
    (SPIELORDERS_CHANGE_ROW_MOVED, c"row_moved"),
    (SPIELORDERS_CHANGE_NEW_IDENTITY, c"new_identity"),
    (SPIELORDERS_REUSE_REUSED, c"reused"),
    (SPIELORDERS_REUSE_NEW, c"new"),
    (SPIELORDERS_REPORT_COMMITTED, c"committed"),
    (SPIELORDERS_REPORT_UNCHANGED, c"unchanged"),
    (
        SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD,
        c"replace_managed_record",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE,
        c"promote_staging_candidate",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR,
        c"assign_existing_vendor",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR,
        c"create_and_assign_vendor",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE,
        c"clear_vendor_reference",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY,
        c"adopt_new_order_identity",
    ),
    (
        SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR,
        c"repair_package_locator",
    ),
    (SPIELORDERS_REASON_UNKNOWN_FIELD, c"unknown_field"),
    (SPIELORDERS_REASON_IMMUTABLE_IDENTITY, c"immutable_identity"),
    (
        SPIELORDERS_REASON_MANAGED_TECHNICAL_FIELD,
        c"managed_technical_field",
    ),
    (SPIELORDERS_REASON_ORDERED_COLLECTION, c"ordered_collection"),
    (SPIELORDERS_REASON_DUPLICATE, c"duplicate"),
    (
        SPIELORDERS_REASON_BLANK_REQUIRED_VALUE,
        c"blank_required_value",
    ),
    (
        SPIELORDERS_REASON_NOT_A_REUSABLE_VALUE,
        c"not_a_reusable_value",
    ),
    (
        SPIELORDERS_REASON_NOT_A_FINITE_NUMBER,
        c"not_a_finite_number",
    ),
    (SPIELORDERS_PROVIDER_STATE_COMPLETE, c"complete"),
    (SPIELORDERS_PROVIDER_STATE_TIMED_OUT, c"timed_out"),
    (SPIELORDERS_PROVIDER_STATE_CANCELLED, c"cancelled"),
    (
        SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED,
        c"indexing_disabled",
    ),
    (
        SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED,
        c"volume_not_indexed",
    ),
    (SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED, c"access_denied"),
    (SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, c"query_refused"),
];
