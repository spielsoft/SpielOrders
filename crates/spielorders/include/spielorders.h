/*
 * SpielOrders: the one interface every caller of a SpielOrders Package uses.
 *
 * Names. spielorders_<noun>_<verb>, where the noun is the handle type of the first
 * argument, the handle type the function creates, or one of the library nouns
 * library, constant, and cancel. Constants are SPIELORDERS_<FAMILY>_<NAME>.
 *
 * Handles. Every library-owned value is a handle of its own type, which a caller only
 * receives from the library, never constructs, copies, or dereferences, and frees with
 * its own spielorders_<noun>_free (freeing NULL does nothing). Using a freed handle is
 * undefined. Handles are independent: freeing one never invalidates another. Every
 * handle except a Draft or a Hits is immutable and may be shared across threads; a Draft
 * or a Hits must not be used from two threads at once.
 *
 * Arguments. A pointer is required (_Nonnull) unless its description says "or NULL";
 * passing NULL for a required one is undefined. Text is NUL-terminated UTF-8. A path is
 * the filesystem's own bytes: pass back a path the library gave you unchanged, and never
 * compare or re-encode one (Swift: URL(fileURLWithFileSystemRepresentation:)).
 *
 * Values. Numbers, dates, and money cross as text that the library parses, calculates,
 * spells, and sorts. A stored or calculated number reads as the shortest decimal that
 * parses back to exactly that value, totals included: nothing is rounded for display, so
 * a caller rounds for its own reader. A draft's field reads as typed. Counts and indices
 * are int64_t, zero-based, with -1 for none. Booleans are int32_t 0 or 1. An absent
 * optional fact reads as "", and setting "" clears it.
 *
 * Constants. Each enumeration is a family of int32_t constants in its own range, values
 * never renumbered or reused. spielorders_constant_name and
 * spielorders_constant_from_name are the name tables. Flags are bit masks, not
 * constants.
 *
 * Results. A function that can fail returns a SPIELORDERS_RESULT_* constant, and its
 * last argument is SpielordersEntries **outcome (or NULL): set to a new Entries handle
 * when the call produced any failure, warning, or finding, and to NULL otherwise. A
 * Package's own findings never change a result. No failure state is global or
 * thread-local. A function that only reads a handle never fails; given an index out of
 * range or a field it does not have, it answers "" for text, -1 for an integer or a
 * constant, 0 for a boolean, and NULL for a handle.
 *
 * Out-parameters. A function that gives a handle sets *out to it on
 * SPIELORDERS_RESULT_OK and to NULL otherwise. Whatever *out held before is overwritten
 * and never read or freed.
 *
 * Ownership. Every handle a function gives you is yours to free. Every string is
 * borrowed from the handle it came from and lives until that handle is freed, except a
 * Draft's, which live until the next call that changes that Draft.
 *
 * Panics. An internal failure returns SPIELORDERS_RESULT_INTERNAL and poisons only the
 * Draft involved, after which every fallible call on that Draft returns
 * SPIELORDERS_RESULT_BAD_ARGUMENT. A commit that fails internally reports in its outcome
 * whether it had already written.
 *
 * Cancellation. spielorders_cancel_new gives a token that is never 0 and never reused;
 * any thread may request or release it. Calls that take a token return
 * SPIELORDERS_RESULT_CANCELLED at their next effect after it is requested, having
 * written nothing; 0 means not cancellable. The token table is the library's only
 * process-wide state. A commit takes no token: it ends committed, refused before its
 * first write, or failed with its rollback reported.
 *
 * Effects. The library performs its own filesystem, clock, and identity effects; a
 * caller implements none.
 */


#ifndef SPIELORDERS_H
#define SPIELORDERS_H

/* Generated from crates/spielorders by `cargo run -p spielorders-checks -- header write`. Do not edit. */

#include <stdint.h>

#if !defined(__clang__)
#define _Nonnull
#define _Nullable
#endif

// NUL-terminated UTF-8 text, borrowed. Required.
typedef const char *_Nonnull SpielordersText;

// NUL-terminated UTF-8 text, borrowed, or NULL.
typedef const char *_Nullable SpielordersOptionalText;

// A NUL-terminated path in the filesystem's own bytes, borrowed. Required. Pass back a
// path the library gave you unchanged; never compare or re-encode one.
typedef const char *_Nonnull SpielordersPathBytes;

// A NUL-terminated path in the filesystem's own bytes, borrowed, or NULL.
typedef const char *_Nullable SpielordersOptionalPathBytes;


// The `flags` bit of a list call that matches case exactly.
#define SPIELORDERS_LIST_CASE_SENSITIVE 1

// The `flags` bit of a Vendor list call that includes Hidden Vendors.
#define SPIELORDERS_LIST_INCLUDE_HIDDEN 2

typedef struct SpielordersDiagnosisData SpielordersDiagnosisData;

typedef struct SpielordersDraftData SpielordersDraftData;

typedef struct SpielordersEntriesData SpielordersEntriesData;

typedef struct SpielordersHitsData SpielordersHitsData;

typedef struct SpielordersListData SpielordersListData;

typedef struct SpielordersOrderData SpielordersOrderData;

typedef struct SpielordersPackageData SpielordersPackageData;

typedef struct SpielordersPlanData SpielordersPlanData;

typedef struct SpielordersReadmeData SpielordersReadmeData;

typedef struct SpielordersReportData SpielordersReportData;

typedef struct SpielordersSearchData SpielordersSearchData;

typedef struct SpielordersVendorData SpielordersVendorData;

// What a fallible function returns: one of the `SPIELORDERS_RESULT_*` constants.
typedef int32_t SpielordersResult;

// One opened Package. A Damaged Package opens in diagnostic-only mode: its lists,
// reads, drafts, and plans refuse as SPIELORDERS_FAILURE_DAMAGED_PACKAGE, and its
// diagnosis, validation, README read, and repair work.
typedef struct SpielordersPackage {
  struct SpielordersPackageData *_Nonnull data;
} SpielordersPackage;

// A diagnosis of a whole Package or of one Order Directory, healthy or damaged.
typedef struct SpielordersDiagnosis {
  struct SpielordersDiagnosisData *_Nonnull data;
} SpielordersDiagnosis;

// A list of failures, warnings, and findings. Read it with the
// `spielorders_entries_*` accessors, each of which reads a NULL Entries as empty.
typedef struct SpielordersEntries {
  struct SpielordersEntriesData *_Nonnull data;
} SpielordersEntries;

// A member of one of the `SPIELORDERS_*` constant families, or -1 for none.
typedef int32_t SpielordersConstant;

// An editor's draft of one Order, one Vendor, or one file's text. Use it from one thread
// at a time. Its strings live until the next call that changes it.
typedef struct SpielordersDraft {
  struct SpielordersDraftData *_Nonnull data;
} SpielordersDraft;

// A validated change that has written nothing: its operations, the changes it makes to
// its record, and the evidence its commit re-checks.
typedef struct SpielordersPlan {
  struct SpielordersPlanData *_Nonnull data;
} SpielordersPlan;

// A fresh read of one healthy Order: its facts, its Item rows, and its README.
typedef struct SpielordersOrder {
  struct SpielordersOrderData *_Nonnull data;
} SpielordersOrder;

// A fresh read of one Vendor, Hidden or not, and its Contact rows.
typedef struct SpielordersVendor {
  struct SpielordersVendorData *_Nonnull data;
} SpielordersVendor;

// The rows of one list, in the order asked for.
typedef struct SpielordersList {
  struct SpielordersListData *_Nonnull data;
} SpielordersList;

// What a front end's Supporting Material provider found, gathered one hit at a time
// for spielorders_package_search_material. Like a Draft, it changes in place and must
// not be used from two threads at once.
typedef struct SpielordersHits {
  struct SpielordersHitsData *_Nonnull data;
} SpielordersHits;

// The eligible files a Supporting Material search found, in the order they are shown.
typedef struct SpielordersSearch {
  struct SpielordersSearchData *_Nonnull data;
} SpielordersSearch;

// One conventional README: the Package's or one Order's. An unreadable README leaves
// its Order readable and refuses only a plan to write it.
typedef struct SpielordersReadme {
  struct SpielordersReadmeData *_Nonnull data;
} SpielordersReadme;

// What a commit did, and the record it refreshed.
typedef struct SpielordersReport {
  struct SpielordersReportData *_Nonnull data;
} SpielordersReport;

// The family of `SPIELORDERS_RESULT_*` constants.
#define SPIELORDERS_FAMILY_RESULT 100

// The family of `SPIELORDERS_FAILURE_*` constants.
#define SPIELORDERS_FAMILY_FAILURE 101

// The family of `SPIELORDERS_WARNING_*` constants.
#define SPIELORDERS_FAMILY_WARNING 102

// The family of `SPIELORDERS_FINDING_*` constants.
#define SPIELORDERS_FAMILY_FINDING 103

// The family of `SPIELORDERS_SEVERITY_*` constants.
#define SPIELORDERS_FAMILY_SEVERITY 104

// The family of `SPIELORDERS_PACKAGE_STATE_*` constants.
#define SPIELORDERS_FAMILY_PACKAGE_STATE 105

// The family of `SPIELORDERS_INDEX_STATE_*` constants.
#define SPIELORDERS_FAMILY_INDEX_STATE 106

// The family of `SPIELORDERS_INDEX_REASON_*` constants.
#define SPIELORDERS_FAMILY_INDEX_REASON 107

// The family of `SPIELORDERS_ORDER_STATUS_*` constants.
#define SPIELORDERS_FAMILY_ORDER_STATUS 108

// The family of `SPIELORDERS_ORDER_FIELD_*` constants.
#define SPIELORDERS_FAMILY_ORDER_FIELD 109

// The family of `SPIELORDERS_ITEM_FIELD_*` constants.
#define SPIELORDERS_FAMILY_ITEM_FIELD 110

// The family of `SPIELORDERS_VENDOR_FIELD_*` constants.
#define SPIELORDERS_FAMILY_VENDOR_FIELD 111

// The family of `SPIELORDERS_CONTACT_FIELD_*` constants.
#define SPIELORDERS_FAMILY_CONTACT_FIELD 112

// The family of `SPIELORDERS_TEXT_FIELD_*` constants.
#define SPIELORDERS_FAMILY_TEXT_FIELD 113

// The family of `SPIELORDERS_VALUE_FIELD_*` constants.
#define SPIELORDERS_FAMILY_VALUE_FIELD 114

// The family of `SPIELORDERS_README_STATE_*` constants.
#define SPIELORDERS_FAMILY_README_STATE 115

// The family of `SPIELORDERS_DRAFT_KIND_*` constants.
#define SPIELORDERS_FAMILY_DRAFT_KIND 116

// The family of `SPIELORDERS_VENDOR_SELECTOR_*` constants.
#define SPIELORDERS_FAMILY_VENDOR_SELECTOR 117

// The family of `SPIELORDERS_OPERATION_*` constants.
#define SPIELORDERS_FAMILY_OPERATION 119

// The family of `SPIELORDERS_CHANGE_*` constants.
#define SPIELORDERS_FAMILY_CHANGE 120

// The family of `SPIELORDERS_REUSE_*` constants.
#define SPIELORDERS_FAMILY_REUSE 121

// The family of `SPIELORDERS_REPORT_*` constants.
#define SPIELORDERS_FAMILY_REPORT 122

// The family of `SPIELORDERS_REPAIR_ACTION_*` constants.
#define SPIELORDERS_FAMILY_REPAIR_ACTION 123

// The family of `SPIELORDERS_REASON_*` constants.
#define SPIELORDERS_FAMILY_REASON 124

// The family of `SPIELORDERS_PROVIDER_STATE_*` constants.
#define SPIELORDERS_FAMILY_PROVIDER_STATE 125

// What a fallible call returns.
// `ok`. The call did what it was asked.
#define SPIELORDERS_RESULT_OK 0

// `refused`. The request was refused and nothing was written; the outcome says why.
#define SPIELORDERS_RESULT_REFUSED 1

// `cancelled`. The caller cancelled the call and nothing was written.
#define SPIELORDERS_RESULT_CANCELLED 2

// `bad_argument`. An unknown constant, a constant of the wrong family, text that is not UTF-8, or a poisoned Draft.
#define SPIELORDERS_RESULT_BAD_ARGUMENT 3

// `internal`. The library failed internally; nothing about the request was at fault.
#define SPIELORDERS_RESULT_INTERNAL 4

// Why a call refused, or why a step inside one failed: an entry code.
// `root_is_a_link`. The selected root is a link. Links are never followed, so nothing behind it was read, renamed, disposed of, or edited.
#define SPIELORDERS_FAILURE_ROOT_IS_A_LINK 1000

// `unmanaged_package`. The exact path given is a resolved directory and is not a SpielOrders Root.
#define SPIELORDERS_FAILURE_UNMANAGED_PACKAGE 1001

// `package_metadata_unsupported`. The Package declares a metadata version this build does not interpret, so it is diagnostic-only and no repair applies.
#define SPIELORDERS_FAILURE_PACKAGE_METADATA_UNSUPPORTED 1002

// `damaged_package`. The Package could be read and not trusted. Its diagnostics are the diagnosis.
#define SPIELORDERS_FAILURE_DAMAGED_PACKAGE 1003

// `package_access_denied`. The filesystem denied access to what had to be observed, read, or written.
#define SPIELORDERS_FAILURE_PACKAGE_ACCESS_DENIED 1004

// `package_operation_failed`. A filesystem operation failed for any other reason.
#define SPIELORDERS_FAILURE_PACKAGE_OPERATION_FAILED 1005

// `package_invalid`.
#define SPIELORDERS_FAILURE_PACKAGE_INVALID 1006

// `order_invalid`.
#define SPIELORDERS_FAILURE_ORDER_INVALID 1007

// `cancelled`. The caller cancelled this read before it settled.
#define SPIELORDERS_FAILURE_CANCELLED 1008

// `not_an_order_directory`. What stands at the selected path is not an Order Directory.
#define SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY 1009

// `order_not_found`. The directory is not one of its Package's Orders. `root` is the Root it was looked for in, where one resolved.
#define SPIELORDERS_FAILURE_ORDER_NOT_FOUND 1010

// `damaged_record`. The Order record could not be read as one. `field` names the first field at fault where the record was still JSON to inspect.
#define SPIELORDERS_FAILURE_DAMAGED_RECORD 1011

// `locator_unusable`. The record's own technical locator is missing or malformed.
#define SPIELORDERS_FAILURE_LOCATOR_UNUSABLE 1012

// `locator_reaches_no_package_holding_this_order`. The locator is well formed and the place it names holds no such Order. `package_root` is the locator as stored.
#define SPIELORDERS_FAILURE_LOCATOR_REACHES_NO_PACKAGE_HOLDING_THIS_ORDER 1013

// `damaged_order`. The directory is one of its Package's Damaged Orders; `diagnostics` is the diagnosis the load quarantined at its scope.
#define SPIELORDERS_FAILURE_DAMAGED_ORDER 1014

// `unknown_order_id`. The Package holds no Order carrying the requested Order ID.
#define SPIELORDERS_FAILURE_UNKNOWN_ORDER_ID 1015

// `unknown_vendor`. No Vendor carries this Name.
#define SPIELORDERS_FAILURE_UNKNOWN_VENDOR 1016

// `package_has_no_vendors`. The Package has no Vendors at all to resolve a Name against.
#define SPIELORDERS_FAILURE_PACKAGE_HAS_NO_VENDORS 1017

// `unknown_vendor_id`. No Vendor carries this Vendor ID.
#define SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID 1018

// `unresolved_vendor`. The Order references a Vendor ID its Package does not hold.
#define SPIELORDERS_FAILURE_UNRESOLVED_VENDOR 1019

// `vendor_name_in_use`. Another Vendor already carries this Name.
#define SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE 1020

// `missing_required_name`. A Vendor Name is required and blank.
#define SPIELORDERS_FAILURE_MISSING_REQUIRED_NAME 1021

// `invalid_aggregate`. The Vendor aggregate this mutation would write does not validate.
#define SPIELORDERS_FAILURE_INVALID_AGGREGATE 1022

// `invalid_assignments`. Every assignment refused, with why.
#define SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS 1023

// `missing_required_facts`. A new Order lacks these required facts.
#define SPIELORDERS_FAILURE_MISSING_REQUIRED_FACTS 1024

// `missing_required_item_facts`. A new Item lacks these required facts.
#define SPIELORDERS_FAILURE_MISSING_REQUIRED_ITEM_FACTS 1025

// `invalid_fact_value`. A fact's value does not parse as its kind.
#define SPIELORDERS_FAILURE_INVALID_FACT_VALUE 1026

// `invalid_facts`. The record these facts would write does not validate.
#define SPIELORDERS_FAILURE_INVALID_FACTS 1027

// `unconfirmed_reusable_value`. A new Reusable Free-Entry Value was proposed without confirmation.
#define SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE 1028

// `invalid_draft`. A whole draft broke these rules, each at the fact or the row it names, the record's own facts first.
#define SPIELORDERS_FAILURE_INVALID_DRAFT 1029

// `invalid_item_position`. A position names no Item of this Order.
#define SPIELORDERS_FAILURE_INVALID_ITEM_POSITION 1030

// `invalid_contact_position`. A position names no Contact of this Vendor.
#define SPIELORDERS_FAILURE_INVALID_CONTACT_POSITION 1031

// `position_not_a_number`. A position a caller wrote is not a whole number.
#define SPIELORDERS_FAILURE_POSITION_NOT_A_NUMBER 1032

// `empty_projected_component`. The Order's facts project a basename with an empty required component.
#define SPIELORDERS_FAILURE_EMPTY_PROJECTED_COMPONENT 1033

// `destination_collision`. Something already occupies the destination; nothing invents a suffix.
#define SPIELORDERS_FAILURE_DESTINATION_COLLISION 1034

// `staging_collision`. A staging artifact already occupies the stage this plan would create.
#define SPIELORDERS_FAILURE_STAGING_COLLISION 1035

// `not_direct_child`. The adoption candidate is not a direct child of the Orders Directory.
#define SPIELORDERS_FAILURE_NOT_DIRECT_CHILD 1036

// `hidden_candidate`. The adoption candidate is a hidden entry.
#define SPIELORDERS_FAILURE_HIDDEN_CANDIDATE 1037

// `ineligible_candidate`. The adoption candidate is not an ordinary directory.
#define SPIELORDERS_FAILURE_INELIGIBLE_CANDIDATE 1038

// `name_unusable_candidate`. The adoption candidate's name cannot be an Order Directory name.
#define SPIELORDERS_FAILURE_NAME_UNUSABLE_CANDIDATE 1039

// `already_an_order`. The adoption candidate already holds an Order.
#define SPIELORDERS_FAILURE_ALREADY_AN_ORDER 1040

// `conflicting_reserved_state`. The adoption candidate holds reserved state that is not an Order.
#define SPIELORDERS_FAILURE_CONFLICTING_RESERVED_STATE 1041

// `root_not_ordinary_directory`. The selected root is not an ordinary directory.
#define SPIELORDERS_FAILURE_ROOT_NOT_ORDINARY_DIRECTORY 1042

// `already_a_package`. The selected root is already a SpielOrders Root.
#define SPIELORDERS_FAILURE_ALREADY_A_PACKAGE 1043

// `invalid_configured_path`. A configured managed path is not one a Package may configure.
#define SPIELORDERS_FAILURE_INVALID_CONFIGURED_PATH 1044

// `managed_paths_overlap`. The two configured managed paths resolve to one directory.
#define SPIELORDERS_FAILURE_MANAGED_PATHS_OVERLAP 1045

// `planned_path_collision`. A configured path resolves onto a path this plan creates.
#define SPIELORDERS_FAILURE_PLANNED_PATH_COLLISION 1046

// `disallowed_link`. A configured path passes through a link.
#define SPIELORDERS_FAILURE_DISALLOWED_LINK 1047

// `unwritable_destination`. The README destination cannot be written.
#define SPIELORDERS_FAILURE_UNWRITABLE_DESTINATION 1048

// `order_readme_text_unavailable`. An Order README exists and its text was not established, so an index entry has nothing truthful to record for it. The Package travels with the refusal.
#define SPIELORDERS_FAILURE_ORDER_README_TEXT_UNAVAILABLE 1049

// `preconditions_changed`. The state this plan was made against changed before its first effect.
#define SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED 1050

// `plan_evidence_unconfirmed`. The facts this plan was authorized against no longer stand, as the evidence seam answered.
#define SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED 1051

// `invalid_generated_identity`. The identity a commit minted is not a valid identity.
#define SPIELORDERS_FAILURE_INVALID_GENERATED_IDENTITY 1052

// `generated_identity_collision`. The Order ID a commit minted is already in use.
#define SPIELORDERS_FAILURE_GENERATED_IDENTITY_COLLISION 1053

// `generated_vendor_identity_collision`. The Vendor ID a commit minted is already in use.
#define SPIELORDERS_FAILURE_GENERATED_VENDOR_IDENTITY_COLLISION 1054

// `commit_failed`. A commit stopped partway and removed everything it created.
#define SPIELORDERS_FAILURE_COMMIT_FAILED 1055

// `commit_failed_cleanup_incomplete`. A commit stopped partway and could not remove what is named.
#define SPIELORDERS_FAILURE_COMMIT_FAILED_CLEANUP_INCOMPLETE 1056

// `candidate_changed`. The candidate a stage holds changed before it was promoted.
#define SPIELORDERS_FAILURE_CANDIDATE_CHANGED 1057

// `destination_changed`. The destination changed between being read and being replaced.
#define SPIELORDERS_FAILURE_DESTINATION_CHANGED 1058

// `candidate_invalid`. The record a commit assembled is not a valid record of its kind.
#define SPIELORDERS_FAILURE_CANDIDATE_INVALID 1059

// `invalid_destination`. The destination names no file a stage can be written beside, or one another write in the same commit already names.
#define SPIELORDERS_FAILURE_INVALID_DESTINATION 1060

// `ownership_uncertain`. What stands there is no longer provably what the commit created, so it is left.
#define SPIELORDERS_FAILURE_OWNERSHIP_UNCERTAIN 1061

// `prior_content_invalid`. What the destination held before does not validate, so it was not written back.
#define SPIELORDERS_FAILURE_PRIOR_CONTENT_INVALID 1062

// `index_replacement_failed`. The rebuilt Package Index could not be written; the detail is the failed commit.
#define SPIELORDERS_FAILURE_INDEX_REPLACEMENT_FAILED 1063

// `unknown_case`. No diagnosed case carries this handle.
#define SPIELORDERS_FAILURE_UNKNOWN_CASE 1064

// `unauthorized_action`. The diagnosed case does not offer this action.
#define SPIELORDERS_FAILURE_UNAUTHORIZED_ACTION 1065

// `unrecognized_artifact`. The destination is not a managed artifact repair recognizes.
#define SPIELORDERS_FAILURE_UNRECOGNIZED_ARTIFACT 1066

// `invalid_candidate`. The candidate does not validate as a record of its kind.
#define SPIELORDERS_FAILURE_INVALID_CANDIDATE 1067

// `candidate_damages_package`. Promoting the candidate would damage the Package.
#define SPIELORDERS_FAILURE_CANDIDATE_DAMAGES_PACKAGE 1068

// `staging_evidence_present`. Staging evidence beside the destination must be resolved first.
#define SPIELORDERS_FAILURE_STAGING_EVIDENCE_PRESENT 1069

// `evidence_changed`. The evidence the repair was planned against changed.
#define SPIELORDERS_FAILURE_EVIDENCE_CHANGED 1070

// `repair_resolution_missing`. The action requires values that were not supplied.
#define SPIELORDERS_FAILURE_REPAIR_RESOLUTION_MISSING 1071

// `repair_resolution_unexpected`. A value was supplied that the action does not take.
#define SPIELORDERS_FAILURE_REPAIR_RESOLUTION_UNEXPECTED 1072

// `indexing_disabled`. The platform has an index and it is switched off.
#define SPIELORDERS_FAILURE_INDEXING_DISABLED 1073

// `volume_not_indexed`. The Package's volume is not indexed, or is one the provider does not index.
#define SPIELORDERS_FAILURE_VOLUME_NOT_INDEXED 1074

// `access_denied`. Access to the index, or to the directories an answer would be about, was denied.
#define SPIELORDERS_FAILURE_ACCESS_DENIED 1075

// `query_refused`. The provider refused this query.
#define SPIELORDERS_FAILURE_QUERY_REFUSED 1076

// `unsupported`. This build has no Supporting Material index provider for this platform.
#define SPIELORDERS_FAILURE_UNSUPPORTED 1077

// `panicked`. A commit stopped on an internal failure; `after` is "replaced" when it had already replaced a managed file, and "unchanged" otherwise.
#define SPIELORDERS_FAILURE_PANICKED 1078

// What a committed change warns of beside its result: an entry code.
// `index_refresh_failed`. The derived Package Index could not be refreshed after the commit.
#define SPIELORDERS_WARNING_INDEX_REFRESH_FAILED 2000

// `refresh_unavailable`. No load reached a current Package holding the committed subject, so the refresh could not run or confirm it.
#define SPIELORDERS_WARNING_REFRESH_UNAVAILABLE 2001

// `rediagnosis_unavailable`. The repaired scope could not be diagnosed again after the commit.
#define SPIELORDERS_WARNING_REDIAGNOSIS_UNAVAILABLE 2002

// `staging_cleanup_failed`. A stage the commit created could not be removed after it was promoted.
#define SPIELORDERS_WARNING_STAGING_CLEANUP_FAILED 2003

// `locator_update_failed`. A removed Order's technical locator could not be rewritten at its new path; the detail is the failed commit.
#define SPIELORDERS_WARNING_LOCATOR_UPDATE_FAILED 2004

// What a Package load, a validation, or a diagnosis found: an entry code.
// `configuration_path_invalid`.
#define SPIELORDERS_FINDING_CONFIGURATION_PATH_INVALID 3000

// `configuration_paths_overlap`.
#define SPIELORDERS_FINDING_CONFIGURATION_PATHS_OVERLAP 3001

// `configured_orders_directory_name_unusable`.
#define SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_NAME_UNUSABLE 3002

// `configured_orders_directory_unavailable`.
#define SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_UNAVAILABLE 3003

// `managed_artifact_invalid_encoding`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_ENCODING 3004

// `managed_artifact_invalid_shape`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE 3005

// `managed_artifact_invalid_syntax`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SYNTAX 3006

// `managed_artifact_missing`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_MISSING 3007

// `managed_artifact_unreadable`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_UNREADABLE 3008

// `managed_artifact_wrong_kind`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_KIND 3009

// `managed_artifact_wrong_version`.
#define SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_VERSION 3010

// `order_id_duplicate`.
#define SPIELORDERS_FINDING_ORDER_ID_DUPLICATE 3011

// `order_package_locator_mismatch`.
#define SPIELORDERS_FINDING_ORDER_PACKAGE_LOCATOR_MISMATCH 3012

// `order_semantic_invalid`.
#define SPIELORDERS_FINDING_ORDER_SEMANTIC_INVALID 3013

// `orders_directory_entry_disallowed`.
#define SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_DISALLOWED 3014

// `orders_directory_entry_name_unusable`.
#define SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_NAME_UNUSABLE 3015

// `readme_unavailable`.
#define SPIELORDERS_FINDING_README_UNAVAILABLE 3016

// `reserved_content_unexpected`.
#define SPIELORDERS_FINDING_RESERVED_CONTENT_UNEXPECTED 3017

// `staging_artifact_present`.
#define SPIELORDERS_FINDING_STAGING_ARTIFACT_PRESENT 3018

// `vendor_id_duplicate`.
#define SPIELORDERS_FINDING_VENDOR_ID_DUPLICATE 3019

// `vendor_name_duplicate`.
#define SPIELORDERS_FINDING_VENDOR_NAME_DUPLICATE 3020

// `vendor_reference_unresolved`.
#define SPIELORDERS_FINDING_VENDOR_REFERENCE_UNRESOLVED 3021

// How much an entry matters.
// `error`. A refusal, or a finding that makes its scope invalid.
#define SPIELORDERS_SEVERITY_ERROR 4000

// `warning`. Something to tell the user that refuses nothing.
#define SPIELORDERS_SEVERITY_WARNING 4001

// Whether an opened Package is trusted.
// `healthy`. Every capability is available.
#define SPIELORDERS_PACKAGE_STATE_HEALTHY 5000

// `damaged`. Diagnostic-only: only diagnosis, validation, README reads, and repair are available.
#define SPIELORDERS_PACKAGE_STATE_DAMAGED 5001

// The state of the stored Package Index, as opening assessed it.
// `current`.
#define SPIELORDERS_INDEX_STATE_CURRENT 6000

// `missing`.
#define SPIELORDERS_INDEX_STATE_MISSING 6001

// `malformed`.
#define SPIELORDERS_INDEX_STATE_MALFORMED 6002

// `wrong_version`.
#define SPIELORDERS_INDEX_STATE_WRONG_VERSION 6003

// `stale`.
#define SPIELORDERS_INDEX_STATE_STALE 6004

// `not_assessed`.
#define SPIELORDERS_INDEX_STATE_NOT_ASSESSED 6005

// Why the stored Package Index is stale or malformed.
// `order_membership_changed`.
#define SPIELORDERS_INDEX_REASON_ORDER_MEMBERSHIP_CHANGED 7000

// `order_path_mismatch`.
#define SPIELORDERS_INDEX_REASON_ORDER_PATH_MISMATCH 7001

// `managed_input_wrong_kind`.
#define SPIELORDERS_INDEX_REASON_MANAGED_INPUT_WRONG_KIND 7002

// `vendors_changed`.
#define SPIELORDERS_INDEX_REASON_VENDORS_CHANGED 7003

// `order_metadata_changed`.
#define SPIELORDERS_INDEX_REASON_ORDER_METADATA_CHANGED 7004

// `order_readme_changed`.
#define SPIELORDERS_INDEX_REASON_ORDER_README_CHANGED 7005

// `change_evidence_uncertain`.
#define SPIELORDERS_INDEX_REASON_CHANGE_EVIDENCE_UNCERTAIN 7006

// `wrong_kind`.
#define SPIELORDERS_INDEX_REASON_WRONG_KIND 7007

// `unreadable`.
#define SPIELORDERS_INDEX_REASON_UNREADABLE 7008

// `invalid_encoding`.
#define SPIELORDERS_INDEX_REASON_INVALID_ENCODING 7009

// `invalid_syntax`.
#define SPIELORDERS_INDEX_REASON_INVALID_SYNTAX 7010

// `invalid_shape`.
#define SPIELORDERS_INDEX_REASON_INVALID_SHAPE 7011

// `invalid_semantics`.
#define SPIELORDERS_INDEX_REASON_INVALID_SEMANTICS 7012

// `non_deterministic_serialization`.
#define SPIELORDERS_INDEX_REASON_NON_DETERMINISTIC_SERIALIZATION 7013

// An Order Status.
// `in_preparation`.
#define SPIELORDERS_ORDER_STATUS_IN_PREPARATION 8000

// `hold`.
#define SPIELORDERS_ORDER_STATUS_HOLD 8001

// `ordered`.
#define SPIELORDERS_ORDER_STATUS_ORDERED 8002

// `received`.
#define SPIELORDERS_ORDER_STATUS_RECEIVED 8003

// `complete`.
#define SPIELORDERS_ORDER_STATUS_COMPLETE 8004

// A fact of an Order. The first nine are what a draft sets; the rest are read-only.
// `title`.
#define SPIELORDERS_ORDER_FIELD_TITLE 9000

// `date_ordered`. Text such as 2026-08-22.
#define SPIELORDERS_ORDER_FIELD_DATE_ORDERED 9001

// `status`. An Order Status name, such as ordered.
#define SPIELORDERS_ORDER_FIELD_STATUS 9002

// `vendor`. The Vendor Name, or "" for no Vendor.
#define SPIELORDERS_ORDER_FIELD_VENDOR 9003

// `orderer`.
#define SPIELORDERS_ORDER_FIELD_ORDERER 9004

// `ordering_institution`.
#define SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION 9005

// `currency`.
#define SPIELORDERS_ORDER_FIELD_CURRENCY 9006

// `project`.
#define SPIELORDERS_ORDER_FIELD_PROJECT 9007

// `order_group`.
#define SPIELORDERS_ORDER_FIELD_ORDER_GROUP 9008

// `order_id`. Read-only.
#define SPIELORDERS_ORDER_FIELD_ORDER_ID 9009

// `directory_name`. Read-only: the Order Directory's name.
#define SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME 9010

// `total`. Read-only: the Order Total, unrounded.
#define SPIELORDERS_ORDER_FIELD_TOTAL 9011

// `damage`. Read-only, on a damaged list entry: the name of its finding code.
#define SPIELORDERS_ORDER_FIELD_DAMAGE 9012

// `vendor_id`. Read-only: the Vendor ID the Order references, or "" for none.
#define SPIELORDERS_ORDER_FIELD_VENDOR_ID 9013

// A fact of one Order Item row.
// `description`.
#define SPIELORDERS_ITEM_FIELD_DESCRIPTION 10000

// `quantity`.
#define SPIELORDERS_ITEM_FIELD_QUANTITY 10001

// `unit_price`.
#define SPIELORDERS_ITEM_FIELD_UNIT_PRICE 10002

// `catalog_number`.
#define SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER 10003

// `total`. Read-only: the row total, unrounded.
#define SPIELORDERS_ITEM_FIELD_TOTAL 10004

// A fact of a Vendor.
// `name`.
#define SPIELORDERS_VENDOR_FIELD_NAME 11000

// `phone`.
#define SPIELORDERS_VENDOR_FIELD_PHONE 11001

// `email`.
#define SPIELORDERS_VENDOR_FIELD_EMAIL 11002

// `website`.
#define SPIELORDERS_VENDOR_FIELD_WEBSITE 11003

// `address`.
#define SPIELORDERS_VENDOR_FIELD_ADDRESS 11004

// `purchasing_notes`.
#define SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES 11005

// `vendor_id`. Read-only.
#define SPIELORDERS_VENDOR_FIELD_VENDOR_ID 11006

// `hidden`. Read-only: "true" or "false".
#define SPIELORDERS_VENDOR_FIELD_HIDDEN 11007

// A fact of one Vendor Contact row.
// `name`.
#define SPIELORDERS_CONTACT_FIELD_NAME 12000

// `role`.
#define SPIELORDERS_CONTACT_FIELD_ROLE 12001

// `phone`.
#define SPIELORDERS_CONTACT_FIELD_PHONE 12002

// `email`.
#define SPIELORDERS_CONTACT_FIELD_EMAIL 12003

// The one field of a TEXT draft.
// `content`. The whole text of a file.
#define SPIELORDERS_TEXT_FIELD_CONTENT 13000

// A fact of one reusable-value list entry.
// `value`.
#define SPIELORDERS_VALUE_FIELD_VALUE 14000

// `usage_count`. How many Orders use it, as decimal text.
#define SPIELORDERS_VALUE_FIELD_USAGE_COUNT 14001

// Whether a README could be read.
// `missing`.
#define SPIELORDERS_README_STATE_MISSING 15000

// `present`.
#define SPIELORDERS_README_STATE_PRESENT 15001

// `unreadable`.
#define SPIELORDERS_README_STATE_UNREADABLE 15002

// What a draft edits.
// `order`.
#define SPIELORDERS_DRAFT_KIND_ORDER 16000

// `vendor`.
#define SPIELORDERS_DRAFT_KIND_VENDOR 16001

// `text`.
#define SPIELORDERS_DRAFT_KIND_TEXT 16002

// How a Vendor read names its Vendor.
// `id`. By Vendor ID.
#define SPIELORDERS_VENDOR_SELECTOR_ID 17000

// `name`. By Vendor Name, compared the way the Package compares Names.
#define SPIELORDERS_VENDOR_SELECTOR_NAME 17001

// One filesystem operation a plan will perform.
// `create_staging_directory`.
#define SPIELORDERS_OPERATION_CREATE_STAGING_DIRECTORY 19000

// `create_metadata_directory`.
#define SPIELORDERS_OPERATION_CREATE_METADATA_DIRECTORY 19001

// `create_removed_orders_directory`.
#define SPIELORDERS_OPERATION_CREATE_REMOVED_ORDERS_DIRECTORY 19002

// `write_order_record`.
#define SPIELORDERS_OPERATION_WRITE_ORDER_RECORD 19003

// `write_readme`.
#define SPIELORDERS_OPERATION_WRITE_README 19004

// `stage_order_record`.
#define SPIELORDERS_OPERATION_STAGE_ORDER_RECORD 19005

// `stage_readme`.
#define SPIELORDERS_OPERATION_STAGE_README 19006

// `commit_readme`.
#define SPIELORDERS_OPERATION_COMMIT_README 19007

// `commit_order_record`.
#define SPIELORDERS_OPERATION_COMMIT_ORDER_RECORD 19008

// `commit_directory`.
#define SPIELORDERS_OPERATION_COMMIT_DIRECTORY 19009

// `remove_staging`.
#define SPIELORDERS_OPERATION_REMOVE_STAGING 19010

// `rename_order_directory`.
#define SPIELORDERS_OPERATION_RENAME_ORDER_DIRECTORY 19011

// `move_order_directory`.
#define SPIELORDERS_OPERATION_MOVE_ORDER_DIRECTORY 19012

// `replace_order_record`.
#define SPIELORDERS_OPERATION_REPLACE_ORDER_RECORD 19013

// `replace_vendor_record`.
#define SPIELORDERS_OPERATION_REPLACE_VENDOR_RECORD 19014

// `replace_readme_document`.
#define SPIELORDERS_OPERATION_REPLACE_README_DOCUMENT 19015

// `replace_managed_artifact`.
#define SPIELORDERS_OPERATION_REPLACE_MANAGED_ARTIFACT 19016

// `promote_staging_candidate`.
#define SPIELORDERS_OPERATION_PROMOTE_STAGING_CANDIDATE 19017

// `refresh_package_index`.
#define SPIELORDERS_OPERATION_REFRESH_PACKAGE_INDEX 19018

// `create_directory`. Initialization creates a directory.
#define SPIELORDERS_OPERATION_CREATE_DIRECTORY 19019

// `create_file`. Initialization creates a file.
#define SPIELORDERS_OPERATION_CREATE_FILE 19020

// `preserve_directory`. Initialization keeps an existing directory.
#define SPIELORDERS_OPERATION_PRESERVE_DIRECTORY 19021

// `preserve_file`. Initialization keeps an existing file.
#define SPIELORDERS_OPERATION_PRESERVE_FILE 19022

// One change a plan makes to a record.
// `fact`. A fact changes from `before` to `after`; a row of -1 is the record's own fact.
#define SPIELORDERS_CHANGE_FACT 20000

// `row_added`. A row is added at `row`.
#define SPIELORDERS_CHANGE_ROW_ADDED 20001

// `row_removed`. The row at `row` of the read record is removed.
#define SPIELORDERS_CHANGE_ROW_REMOVED 20002

// `row_moved`. The row at `before` of the read record moves to `row`.
#define SPIELORDERS_CHANGE_ROW_MOVED 20003

// `new_identity`. The commit mints a new identity for the record.
#define SPIELORDERS_CHANGE_NEW_IDENTITY 20004

// Whether a reusable value already exists.
// `reused`. The Package already uses this value.
#define SPIELORDERS_REUSE_REUSED 21000

// `new`. The Package does not yet use this value; saving needs it confirmed.
#define SPIELORDERS_REUSE_NEW 21001

// What a commit did.
// `committed`. The plan was performed.
#define SPIELORDERS_REPORT_COMMITTED 22000

// `unchanged`. The plan changed nothing, so nothing was written.
#define SPIELORDERS_REPORT_UNCHANGED 22001

// One explicit action a diagnosed case offers.
// `replace_managed_record`. Accept one user-confirmed complete replacement for parseable metadata.
#define SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD 23000

// `promote_staging_candidate`. Promote the exact diagnosed staging candidate.
#define SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE 23001

// `assign_existing_vendor`. Point an unresolved Vendor Reference at an existing Vendor.
#define SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR 23002

// `create_and_assign_vendor`. Deliberately create one fully specified distinct Vendor and assign it.
#define SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR 23003

// `clear_vendor_reference`. Clear the unresolved Vendor Reference outright.
#define SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE 23004

// `adopt_new_order_identity`. Adopt a fresh identity for this side of a duplicate Order ID conflict.
#define SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY 23005

// `repair_package_locator`. Recompute the technical locator from the physical Package relationship.
#define SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR 23006

// Why one assignment was rejected: an entry reason.
// `unknown_field`.
#define SPIELORDERS_REASON_UNKNOWN_FIELD 24000

// `immutable_identity`.
#define SPIELORDERS_REASON_IMMUTABLE_IDENTITY 24001

// `managed_technical_field`.
#define SPIELORDERS_REASON_MANAGED_TECHNICAL_FIELD 24002

// `ordered_collection`.
#define SPIELORDERS_REASON_ORDERED_COLLECTION 24003

// `duplicate`.
#define SPIELORDERS_REASON_DUPLICATE 24004

// `blank_required_value`.
#define SPIELORDERS_REASON_BLANK_REQUIRED_VALUE 24005

// `not_a_reusable_value`.
#define SPIELORDERS_REASON_NOT_A_REUSABLE_VALUE 24006

// `not_a_finite_number`.
#define SPIELORDERS_REASON_NOT_A_FINITE_NUMBER 24007

// What a front end's Supporting Material provider did with one query.
// `complete`. It looked at everything in scope.
#define SPIELORDERS_PROVIDER_STATE_COMPLETE 25000

// `timed_out`. It looked at part of what is in scope before its bound elapsed.
#define SPIELORDERS_PROVIDER_STATE_TIMED_OUT 25001

// `cancelled`. It looked at part of what is in scope before the caller cancelled.
#define SPIELORDERS_PROVIDER_STATE_CANCELLED 25002

// `indexing_disabled`. It could not look: indexing is disabled for the volume.
#define SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED 25003

// `volume_not_indexed`. It could not look: the volume is not indexed.
#define SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED 25004

// `access_denied`. It could not look: the directory could not be read.
#define SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED 25005

// `query_refused`. It could not look: the query was refused.
#define SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED 25006

// A new cancellation token: never 0 and never reused. Pass it to calls that take a
// token, request it from any thread to cancel them, and release it when done.
uint64_t spielorders_cancel_new(void);

// Cancels every call running or later started with `token`: each returns
// SPIELORDERS_RESULT_CANCELLED at its next effect, having written nothing. An unknown
// or released token does nothing. Safe from any thread.
void spielorders_cancel_request(uint64_t token);

// Releases `token`. Calls already running with it are unaffected; later requests of it
// do nothing. An unknown or released token does nothing.
void spielorders_cancel_release(uint64_t token);

// Validates all of `package`: every Order Directory and every cross-record constraint.
// Works on a Damaged Package.
SpielordersResult spielorders_package_validate(const struct SpielordersPackage *_Nonnull package,
                                               uint64_t token,
                                               struct SpielordersDiagnosis *_Nullable *_Nonnull diagnosis,
                                               struct SpielordersEntries *_Nullable *_Nullable outcome);

// Validates one Order Directory of `package`, damaged or not.
SpielordersResult spielorders_package_validate_order(const struct SpielordersPackage *_Nonnull package,
                                                     SpielordersPathBytes order_directory,
                                                     uint64_t token,
                                                     struct SpielordersDiagnosis *_Nullable *_Nonnull diagnosis,
                                                     struct SpielordersEntries *_Nullable *_Nullable outcome);

// Diagnoses the repairs `package`'s own artifacts offer. Works on a Damaged Package.
SpielordersResult spielorders_package_diagnose(const struct SpielordersPackage *_Nonnull package,
                                               uint64_t token,
                                               struct SpielordersDiagnosis *_Nullable *_Nonnull diagnosis,
                                               struct SpielordersEntries *_Nullable *_Nullable outcome);

// Diagnoses the repairs one Order Directory of `package` offers, damaged or not.
SpielordersResult spielorders_package_diagnose_order(const struct SpielordersPackage *_Nonnull package,
                                                     SpielordersPathBytes order_directory,
                                                     uint64_t token,
                                                     struct SpielordersDiagnosis *_Nullable *_Nonnull diagnosis,
                                                     struct SpielordersEntries *_Nullable *_Nullable outcome);

// 1 when no finding of `diagnosis` is an error, otherwise 0.
int32_t spielorders_diagnosis_is_valid(const struct SpielordersDiagnosis *_Nonnull diagnosis);

// Every finding of `diagnosis`, as a new Entries handle the caller frees, or NULL when
// there are none.
struct SpielordersEntries *_Nullable spielorders_diagnosis_findings(const struct SpielordersDiagnosis *_Nonnull diagnosis);

// How many repair cases `diagnosis` offers.
int64_t spielorders_diagnosis_case_count(const struct SpielordersDiagnosis *_Nonnull diagnosis);

// The `SPIELORDERS_FINDING_*` case `case` was diagnosed from, or -1 out of range.
SpielordersConstant spielorders_diagnosis_case_code(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                    int64_t case_);

// The Package-relative path of the evidence case `case` was diagnosed from, or "".
SpielordersText spielorders_diagnosis_case_path(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                int64_t case_);

// The handle that names case `case` to a later repair: its code, its path, and the
// digest of the evidence it was diagnosed from, so a handle names nothing once that
// evidence changes; "" out of range.
SpielordersText spielorders_diagnosis_case_handle(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                  int64_t case_);

// How many explicit actions case `case` offers.
int64_t spielorders_diagnosis_action_count(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                           int64_t case_);

// The `SPIELORDERS_REPAIR_ACTION_*` of action `action` of case `case`, or -1.
SpielordersConstant spielorders_diagnosis_action(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                 int64_t case_,
                                                 int64_t action);

// How many values action `action` of case `case` requires; its draft holds them.
int64_t spielorders_diagnosis_action_field_count(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                 int64_t case_,
                                                 int64_t action);

// Required value `field` of action `action` of case `case`: the draft field that holds
// it, or -1.
SpielordersConstant spielorders_diagnosis_action_field(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                       int64_t case_,
                                                       int64_t action,
                                                       int64_t field);

// A draft holding the values action `action` of case `case` requires: a TEXT draft
// prefilled with the file's current text for a whole-file replacement (empty where that
// text is not UTF-8), an ORDER draft holding the Vendor to assign, or a VENDOR draft
// holding the new Vendor's Name. BAD_ARGUMENT for an action that requires no values.
SpielordersResult spielorders_diagnosis_draft(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                              int64_t case_,
                                              int64_t action,
                                              struct SpielordersDraft *_Nullable *_Nonnull draft,
                                              struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans action `action` of case `case` with the values `draft` (or NULL, for an action
// that requires none) holds. Its commit refuses if the evidence the case was diagnosed
// from changed.
SpielordersResult spielorders_diagnosis_plan_repair(const struct SpielordersDiagnosis *_Nonnull diagnosis,
                                                    int64_t case_,
                                                    int64_t action,
                                                    const struct SpielordersDraft *_Nullable draft,
                                                    uint64_t token,
                                                    struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                    struct SpielordersEntries *_Nullable *_Nullable outcome);

// Frees `diagnosis`. Freeing NULL does nothing.
void spielorders_diagnosis_free(struct SpielordersDiagnosis *_Nullable diagnosis);

// A draft of `order`, saved against this read: a save refuses if the Order changed on
// disk after it.
SpielordersResult spielorders_order_draft(const struct SpielordersOrder *_Nonnull order,
                                          struct SpielordersDraft *_Nullable *_Nonnull draft);

// A draft of `vendor`, saved against this read.
SpielordersResult spielorders_vendor_draft(const struct SpielordersVendor *_Nonnull vendor,
                                           struct SpielordersDraft *_Nullable *_Nonnull draft);

// A draft of a new Order in `package`, holding its Title, Date Ordered, and Status
// defaults.
// Saving it creates the Order with its facts and Item rows.
SpielordersResult spielorders_package_new_order_draft(const struct SpielordersPackage *_Nonnull package,
                                                      struct SpielordersDraft *_Nullable *_Nonnull draft,
                                                      struct SpielordersEntries *_Nullable *_Nullable outcome);

// A draft of a new Vendor in `package`, holding a Name no Vendor of the Package uses.
// Saving it creates the Vendor with its facts and Contact rows.
SpielordersResult spielorders_package_new_vendor_draft(const struct SpielordersPackage *_Nonnull package,
                                                       struct SpielordersDraft *_Nullable *_Nonnull draft,
                                                       struct SpielordersEntries *_Nullable *_Nullable outcome);

// A draft adopting the existing directory `directory` as a new Order of `package`. It
// holds no defaults and never takes today's date: every fact is the user's.
SpielordersResult spielorders_package_adoption_draft(const struct SpielordersPackage *_Nonnull package,
                                                     SpielordersPathBytes directory,
                                                     uint64_t token,
                                                     struct SpielordersDraft *_Nullable *_Nonnull draft,
                                                     struct SpielordersEntries *_Nullable *_Nullable outcome);

// The `SPIELORDERS_DRAFT_KIND_*` of `draft`.
SpielordersConstant spielorders_draft_kind(const struct SpielordersDraft *_Nonnull draft);

// Sets `field` of `draft` to `text` exactly as typed, and applies the live rules:
// choosing Ordered as the Status takes today's Date Ordered, except in an adoption
// draft. BAD_ARGUMENT for a field the draft does not have or text that is not UTF-8;
// value errors come from checking or planning the draft.
SpielordersResult spielorders_draft_set(struct SpielordersDraft *_Nonnull draft,
                                        SpielordersConstant field,
                                        SpielordersText text,
                                        struct SpielordersEntries *_Nullable *_Nullable outcome);

// Inserts an empty row before `row`; `row` equal to the row count appends.
// BAD_ARGUMENT for a diagnosis draft, which holds no rows.
SpielordersResult spielorders_draft_row_insert(struct SpielordersDraft *_Nonnull draft,
                                               int64_t row,
                                               struct SpielordersEntries *_Nullable *_Nullable outcome);

// Removes row `row`.
SpielordersResult spielorders_draft_row_remove(struct SpielordersDraft *_Nonnull draft,
                                               int64_t row,
                                               struct SpielordersEntries *_Nullable *_Nullable outcome);

// Moves row `from` so that it becomes row `to`.
SpielordersResult spielorders_draft_row_move(struct SpielordersDraft *_Nonnull draft,
                                             int64_t from,
                                             int64_t to,
                                             struct SpielordersEntries *_Nullable *_Nullable outcome);

// Sets `field` of row `row` to `text` exactly as typed, as `spielorders_draft_set` does.
SpielordersResult spielorders_draft_row_set(struct SpielordersDraft *_Nonnull draft,
                                            int64_t row,
                                            SpielordersConstant field,
                                            SpielordersText text,
                                            struct SpielordersEntries *_Nullable *_Nullable outcome);

// Confirms the value `field` now holds as a new reusable value. `field` is
// SPIELORDERS_ORDER_FIELD_ORDERER, _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP and
// `row` is -1. Changing the field again withdraws the confirmation.
SpielordersResult spielorders_draft_confirm(struct SpielordersDraft *_Nonnull draft,
                                            SpielordersConstant field,
                                            int64_t row,
                                            struct SpielordersEntries *_Nullable *_Nullable outcome);

// The text `field` of `draft` holds, or the live SPIELORDERS_ORDER_FIELD_TOTAL; "" for
// a field the draft does not have.
SpielordersText spielorders_draft_field(const struct SpielordersDraft *_Nonnull draft,
                                        SpielordersConstant field);

// How many rows `draft` holds.
int64_t spielorders_draft_row_count(const struct SpielordersDraft *_Nonnull draft);

// The text `field` of row `row` holds, or its live SPIELORDERS_ITEM_FIELD_TOTAL; "" out
// of range.
SpielordersText spielorders_draft_row_field(const struct SpielordersDraft *_Nonnull draft,
                                            int64_t row,
                                            SpielordersConstant field);

// The live Order Total of an Order draft, unrounded; "" while a row's numbers do not
// parse, and for other drafts.
SpielordersText spielorders_draft_total(const struct SpielordersDraft *_Nonnull draft);

// Every error saving `draft` would refuse with now, each located by field and row,
// without making a plan. OK when there are none.
SpielordersResult spielorders_draft_check(const struct SpielordersDraft *_Nonnull draft,
                                          struct SpielordersEntries *_Nullable *_Nullable outcome);

// Frees `draft`. Freeing NULL does nothing.
void spielorders_draft_free(struct SpielordersDraft *_Nullable draft);

// How many entries `entries` (or NULL) holds; 0 for NULL.
int64_t spielorders_entries_count(const struct SpielordersEntries *_Nullable entries);

// The code of entry `index`: a `SPIELORDERS_FAILURE_*`, `SPIELORDERS_WARNING_*`, or
// `SPIELORDERS_FINDING_*` constant. `entries` may be NULL. -1 when out of range.
SpielordersConstant spielorders_entries_code(const struct SpielordersEntries *_Nullable entries,
                                             int64_t index);

// The `SPIELORDERS_SEVERITY_*` of entry `index`. `entries` may be NULL. -1 when out of
// range.
SpielordersConstant spielorders_entries_severity(const struct SpielordersEntries *_Nullable entries,
                                                 int64_t index);

// A message for entry `index` a caller may show. `entries` may be NULL. "" when out of
// range.
SpielordersText spielorders_entries_message(const struct SpielordersEntries *_Nullable entries,
                                            int64_t index);

// The field entry `index` is about: a `SPIELORDERS_*_FIELD_*` constant of the record or
// row it locates, or -1. `entries` may be NULL.
SpielordersConstant spielorders_entries_field(const struct SpielordersEntries *_Nullable entries,
                                              int64_t index);

// Why entry `index` rejected its field: a `SPIELORDERS_REASON_*` constant, or -1.
// `entries` may be NULL.
SpielordersConstant spielorders_entries_reason(const struct SpielordersEntries *_Nullable entries,
                                               int64_t index);

// The zero-based row entry `index` is about, or -1 for the record's own facts.
// `entries` may be NULL.
int64_t spielorders_entries_row(const struct SpielordersEntries *_Nullable entries, int64_t index);

// The path entry `index` is about: Package-relative, or as named where it has no
// Package-relative spelling; or "". `entries` may be NULL.
SpielordersText spielorders_entries_path(const struct SpielordersEntries *_Nullable entries,
                                         int64_t index);

// What a fact was, for a changed fact; or "". `entries` may be NULL.
SpielordersText spielorders_entries_before(const struct SpielordersEntries *_Nullable entries,
                                           int64_t index);

// What a fact is now, for a changed fact; a finding's evidence; a rejected value; or "".
// `entries` may be NULL.
SpielordersText spielorders_entries_after(const struct SpielordersEntries *_Nullable entries,
                                          int64_t index);

// Frees `entries`. Freeing NULL does nothing.
void spielorders_entries_free(struct SpielordersEntries *_Nullable entries);

// This library's version, as text.
SpielordersText spielorders_library_version(void);

// A suggested agent request after provisioning. Borrowed static UTF-8; never freed.
SpielordersText spielorders_library_agent_setup_prompt(void);

// The name of `value`: the snake_case spelling of any `SPIELORDERS_*` constant,
// such as "damaged_package" for SPIELORDERS_FAILURE_DAMAGED_PACKAGE, or "" for a value
// that is no constant.
SpielordersText spielorders_constant_name(SpielordersConstant value);

// The member of `family` (a `SPIELORDERS_FAMILY_*` constant) that `name` spells, or -1
// when the family has no such member or `name` is not UTF-8.
SpielordersConstant spielorders_constant_from_name(SpielordersConstant family,
                                                   SpielordersText name);

// The Orders matching `query` (or NULL), in the Package's own order or sorted by
// `sort_field` (a `SPIELORDERS_ORDER_FIELD_*`, or -1). `flags`:
// SPIELORDERS_LIST_CASE_SENSITIVE. Damaged Order Directories appear, after every
// healthy Order, only when the query is NULL or empty.
SpielordersResult spielorders_package_orders(const struct SpielordersPackage *_Nonnull package,
                                             SpielordersOptionalText query_text,
                                             uint32_t flags,
                                             SpielordersConstant sort,
                                             int32_t descending,
                                             uint64_t token,
                                             struct SpielordersList *_Nullable *_Nonnull list,
                                             struct SpielordersEntries *_Nullable *_Nullable outcome);

// The Vendors matching `query` (or NULL), in the Package's own order or sorted by
// `sort_field` (a `SPIELORDERS_VENDOR_FIELD_*`, or -1). `flags`:
// SPIELORDERS_LIST_CASE_SENSITIVE, SPIELORDERS_LIST_INCLUDE_HIDDEN.
SpielordersResult spielorders_package_vendors(const struct SpielordersPackage *_Nonnull package,
                                              SpielordersOptionalText query_text,
                                              uint32_t flags,
                                              SpielordersConstant sort,
                                              int32_t descending,
                                              uint64_t token,
                                              struct SpielordersList *_Nullable *_Nonnull list,
                                              struct SpielordersEntries *_Nullable *_Nullable outcome);

// The healthy Orders whose Vendor is `vendor_id`, in the Package's own order.
SpielordersResult spielorders_package_orders_of_vendor(const struct SpielordersPackage *_Nonnull package,
                                                       SpielordersText vendor_id,
                                                       uint64_t token,
                                                       struct SpielordersList *_Nullable *_Nonnull list,
                                                       struct SpielordersEntries *_Nullable *_Nullable outcome);

// The current values of one reusable-value field (SPIELORDERS_ORDER_FIELD_ORDERER,
// _ORDERING_INSTITUTION, _PROJECT, or _ORDER_GROUP), each row read with
// `SPIELORDERS_VALUE_FIELD_*`.
SpielordersResult spielorders_package_values(const struct SpielordersPackage *_Nonnull package,
                                             SpielordersConstant field,
                                             struct SpielordersList *_Nullable *_Nonnull list,
                                             struct SpielordersEntries *_Nullable *_Nullable outcome);

// How many rows `list` holds.
int64_t spielorders_list_count(const struct SpielordersList *_Nonnull list);

// 1 when row `index` is a Damaged Order Directory, otherwise 0.
int32_t spielorders_list_damaged(const struct SpielordersList *_Nonnull list, int64_t index);

// A field of row `index`. A damaged row answers only SPIELORDERS_ORDER_FIELD_ORDER_ID
// (its conflicting identity, for a duplicate Order ID), _DIRECTORY_NAME, and _DAMAGE.
// "" for an absent fact, a field the row does not have, or out of range.
SpielordersText spielorders_list_field(const struct SpielordersList *_Nonnull list,
                                       int64_t index,
                                       SpielordersConstant field);

// The Order Directory of row `index` of an Order list; "" for other lists or out of
// range.
SpielordersPathBytes spielorders_list_path(const struct SpielordersList *_Nonnull list,
                                           int64_t index);

// Frees `list`. Freeing NULL does nothing.
void spielorders_list_free(struct SpielordersList *_Nullable list);

// The one directory a front end's Supporting Material provider searches for `package`:
// its Orders Directory. "" where there is no healthy Order, so no query is needed, and
// for a Damaged Package.
SpielordersPathBytes spielorders_package_material_directory(const struct SpielordersPackage *_Nonnull package);

// A new, empty set of hits from the provider named `provider`, such as "spotlight".
// SPIELORDERS_RESULT_BAD_ARGUMENT when `provider` is not UTF-8.
SpielordersResult spielorders_hits_new(SpielordersText provider,
                                       struct SpielordersHits *_Nullable *_Nonnull hits);

// Adds one file the provider reported, with the facts it stated: `kind` in the
// provider's own words, `modified` as whole seconds since the Unix epoch written in
// decimal digits (a fraction does not parse), and `rank` as the provider's decimal
// relevance, each "" where the provider stated none. Every
// path the provider reported is added as it reported it, including ones the library
// will leave out. SPIELORDERS_RESULT_BAD_ARGUMENT for a fact that does not parse.
SpielordersResult spielorders_hits_add(struct SpielordersHits *_Nonnull hits,
                                       SpielordersPathBytes path,
                                       SpielordersText kind,
                                       SpielordersText modified,
                                       SpielordersText rank);

// Adds something the provider said about this query, in its own words. A search reads
// each back with spielorders_search_diagnostic; a refusal carries each as an entry
// after its failure.
SpielordersResult spielorders_hits_diagnostic(struct SpielordersHits *_Nonnull hits,
                                              SpielordersText said);

// Frees `hits`. Freeing NULL does nothing.
void spielorders_hits_free(struct SpielordersHits *_Nullable hits);

// The Supporting Material of `package` among `hits`, from a provider in `state` (a
// `SPIELORDERS_PROVIDER_STATE_*`): one file per filesystem entry, however the provider
// spelled its path, only under a healthy Order and never one of its managed documents,
// each answered in the Package's own spelling, grouped by Order in the Package's own
// Order order, and within an Order by rank descending, then filename, then path. `hits` (or
// NULL) is NULL where the caller has no provider for its platform, which refuses as
// SPIELORDERS_FAILURE_UNSUPPORTED. A state in which the provider could not look refuses
// with its failure.
SpielordersResult spielorders_package_search_material(const struct SpielordersPackage *_Nonnull package,
                                                      const struct SpielordersHits *_Nullable hits,
                                                      SpielordersConstant state,
                                                      uint64_t token,
                                                      struct SpielordersSearch *_Nullable *_Nonnull search,
                                                      struct SpielordersEntries *_Nullable *_Nullable outcome);

// The provider that answered `search`, in its own name.
SpielordersText spielorders_search_provider(const struct SpielordersSearch *_Nonnull search);

// Whether the provider looked at everything in scope: SPIELORDERS_PROVIDER_STATE_COMPLETE,
// _TIMED_OUT, or _CANCELLED. A search whose token was cancelled is never complete.
SpielordersConstant spielorders_search_state(const struct SpielordersSearch *_Nonnull search);

// How many files `search` found.
int64_t spielorders_search_count(const struct SpielordersSearch *_Nonnull search);

// The Order Directory file `index` is grouped under; consecutive files share it.
SpielordersPathBytes spielorders_search_order_directory(const struct SpielordersSearch *_Nonnull search,
                                                        int64_t index);

// The absolute path of file `index`.
SpielordersPathBytes spielorders_search_path(const struct SpielordersSearch *_Nonnull search,
                                             int64_t index);

// The path of file `index` within its Order Directory.
SpielordersText spielorders_search_relative_path(const struct SpielordersSearch *_Nonnull search,
                                                 int64_t index);

// The filename of file `index`.
SpielordersText spielorders_search_filename(const struct SpielordersSearch *_Nonnull search,
                                            int64_t index);

// The kind of file `index`, in the provider's own words, or "".
SpielordersText spielorders_search_kind(const struct SpielordersSearch *_Nonnull search,
                                        int64_t index);

// When file `index` was modified, as whole seconds since the Unix epoch, or "".
SpielordersText spielorders_search_modified(const struct SpielordersSearch *_Nonnull search,
                                            int64_t index);

// The provider's decimal relevance for file `index`, or "" where it stated none.
SpielordersText spielorders_search_rank(const struct SpielordersSearch *_Nonnull search,
                                        int64_t index);

// How many things the provider said about this query, in its own words.
int64_t spielorders_search_diagnostic_count(const struct SpielordersSearch *_Nonnull search);

// Thing `index` the provider said about this query, in its own words, in the order it
// was supplied; "" out of range.
SpielordersText spielorders_search_diagnostic(const struct SpielordersSearch *_Nonnull search,
                                              int64_t index);

// Frees `search`. Freeing NULL does nothing.
void spielorders_search_free(struct SpielordersSearch *_Nullable search);

// Opens the Package at `root`, writing nothing. Refuses a root that is a link before
// reading anything behind it, and a folder that is not a SpielOrders Root. A Damaged
// Package opens with state SPIELORDERS_PACKAGE_STATE_DAMAGED.
SpielordersResult spielorders_package_open(SpielordersPathBytes root,
                                           uint64_t token,
                                           struct SpielordersPackage *_Nullable *_Nonnull package,
                                           struct SpielordersEntries *_Nullable *_Nullable outcome);

// Opens the Package an Order Directory belongs to, found through the Order's stored
// locator. A Damaged Order Directory, including one whose record or locator is what is
// damaged, opens the Package that holds it, where it can be validated and repaired.
SpielordersResult spielorders_package_open_from_order(SpielordersPathBytes order_directory,
                                                      uint64_t token,
                                                      struct SpielordersPackage *_Nullable *_Nonnull package,
                                                      struct SpielordersEntries *_Nullable *_Nullable outcome);

// The `SPIELORDERS_PACKAGE_STATE_*` of `package`.
SpielordersConstant spielorders_package_state(const struct SpielordersPackage *_Nonnull package);

// The Root of `package`.
SpielordersPathBytes spielorders_package_root(const struct SpielordersPackage *_Nonnull package);

// The findings opening `package` made, as a new Entries handle the caller frees, or
// NULL when there are none. Findings never change a call's result.
struct SpielordersEntries *_Nullable spielorders_package_findings(const struct SpielordersPackage *_Nonnull package);

// The `SPIELORDERS_INDEX_STATE_*` of the stored Package Index, as opening assessed it
// without writing.
SpielordersConstant spielorders_package_index_state(const struct SpielordersPackage *_Nonnull package);

// How many reasons the stored Package Index is stale or malformed for.
int64_t spielorders_package_index_reason_count(const struct SpielordersPackage *_Nonnull package);

// Reason `index` the stored Package Index is stale or malformed: a
// `SPIELORDERS_INDEX_REASON_*` constant, or -1 when out of range.
SpielordersConstant spielorders_package_index_reason(const struct SpielordersPackage *_Nonnull package,
                                                     int64_t index);

// Reads the healthy Order at `order_directory` afresh, with the findings local to it.
// The read is the evidence a draft of it saves against.
SpielordersResult spielorders_package_read_order(const struct SpielordersPackage *_Nonnull package,
                                                 SpielordersPathBytes order_directory,
                                                 uint64_t token,
                                                 struct SpielordersOrder *_Nullable *_Nonnull order,
                                                 struct SpielordersEntries *_Nullable *_Nullable outcome);

// Reads one Vendor afresh, Hidden or not. `selector` is a
// `SPIELORDERS_VENDOR_SELECTOR_*` constant saying whether `text` is its ID or its Name.
SpielordersResult spielorders_package_read_vendor(const struct SpielordersPackage *_Nonnull package,
                                                  SpielordersConstant selector,
                                                  SpielordersText text,
                                                  uint64_t token,
                                                  struct SpielordersVendor *_Nullable *_Nonnull vendor,
                                                  struct SpielordersEntries *_Nullable *_Nullable outcome);

// Reads the Package README afresh. Works on a Damaged Package.
SpielordersResult spielorders_package_read_readme(const struct SpielordersPackage *_Nonnull package,
                                                  uint64_t token,
                                                  struct SpielordersReadme *_Nullable *_Nonnull readme,
                                                  struct SpielordersEntries *_Nullable *_Nullable outcome);

// Frees `package`. Freeing NULL does nothing; handles read from it stay valid.
void spielorders_package_free(struct SpielordersPackage *_Nullable package);

// Plans missing Package-local agent guidance without reading or changing root AGENTS.md.
// Existing regular guidance is preserved. Commit rechecks configuration and destinations.
SpielordersResult spielorders_package_plan_provision_agent_setup(const struct SpielordersPackage *_Nonnull package,
                                                                 uint64_t token,
                                                                 struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                                 struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans saving `draft`: every rule it breaks refuses together, each entry located by
// field and row. The plan of an edit is guarded by the read the draft was made from.
SpielordersResult spielorders_draft_plan(const struct SpielordersDraft *_Nonnull draft,
                                         uint64_t token,
                                         struct SpielordersPlan *_Nullable *_Nonnull plan,
                                         struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans renaming the Order Directory of `order` to the name its facts project.
SpielordersResult spielorders_order_plan_align(const struct SpielordersOrder *_Nonnull order,
                                               uint64_t token,
                                               struct SpielordersPlan *_Nullable *_Nonnull plan,
                                               struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans removing `order` to the Package's Removed Orders Directory, where it stays
// recoverable. Moving it on to the platform Trash is the caller's own act, on the
// directory the committed report's final path names.
SpielordersResult spielorders_order_plan_remove(const struct SpielordersOrder *_Nonnull order,
                                                uint64_t token,
                                                struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans hiding (`hidden` 1) or unhiding (0) `vendor`.
SpielordersResult spielorders_vendor_plan_hidden(const struct SpielordersVendor *_Nonnull vendor,
                                                 int32_t hidden,
                                                 uint64_t token,
                                                 struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                 struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans replacing the text of `readme` with `text`, guarded by the bytes `readme` read.
// Refuses for a README that could not be read.
SpielordersResult spielorders_readme_plan_write(const struct SpielordersReadme *_Nonnull readme,
                                                SpielordersText text,
                                                uint64_t token,
                                                struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans rebuilding the derived Package Index of `package`.
SpielordersResult spielorders_package_plan_rebuild_index(const struct SpielordersPackage *_Nonnull package,
                                                         uint64_t token,
                                                         struct SpielordersPlan *_Nullable *_Nonnull plan,
                                                         struct SpielordersEntries *_Nullable *_Nullable outcome);

// Plans initializing a new Package at `root`. `orders_path` and `removed_path` (or NULL
// for the conventional ones) are Package-relative directories.
SpielordersResult spielorders_plan_initialize(SpielordersPathBytes root,
                                              SpielordersOptionalPathBytes orders_path,
                                              SpielordersOptionalPathBytes removed_path,
                                              uint64_t token,
                                              struct SpielordersPlan *_Nullable *_Nonnull plan,
                                              struct SpielordersEntries *_Nullable *_Nullable outcome);

// How many filesystem operations committing `plan` performs.
int64_t spielorders_plan_operation_count(const struct SpielordersPlan *_Nonnull plan);

// The `SPIELORDERS_OPERATION_*` of operation `index`, or -1 out of range.
SpielordersConstant spielorders_plan_operation_kind(const struct SpielordersPlan *_Nonnull plan,
                                                    int64_t index);

// The path operation `index` acts on; "" out of range.
SpielordersPathBytes spielorders_plan_operation_path(const struct SpielordersPlan *_Nonnull plan,
                                                     int64_t index);

// How many changes `plan` makes to the record its draft edits.
int64_t spielorders_plan_change_count(const struct SpielordersPlan *_Nonnull plan);

// The `SPIELORDERS_CHANGE_*` of change `index`, or -1 out of range.
SpielordersConstant spielorders_plan_change_kind(const struct SpielordersPlan *_Nonnull plan,
                                                 int64_t index);

// The field change `index` changes: a `SPIELORDERS_*_FIELD_*` constant, or -1.
SpielordersConstant spielorders_plan_change_field(const struct SpielordersPlan *_Nonnull plan,
                                                  int64_t index);

// The zero-based draft row change `index` is about (for a removed row, the row of the
// read record), or -1 for the record's own facts.
int64_t spielorders_plan_change_row(const struct SpielordersPlan *_Nonnull plan, int64_t index);

// What change `index` changes from; "" when nothing was there.
SpielordersText spielorders_plan_change_before(const struct SpielordersPlan *_Nonnull plan,
                                               int64_t index);

// What change `index` changes to, as the plan normalized it; "" when nothing will be.
SpielordersText spielorders_plan_change_after(const struct SpielordersPlan *_Nonnull plan,
                                              int64_t index);

// For a change to a reusable-value field, SPIELORDERS_REUSE_REUSED or
// SPIELORDERS_REUSE_NEW; otherwise -1.
SpielordersConstant spielorders_plan_change_reuse(const struct SpielordersPlan *_Nonnull plan,
                                                  int64_t index);

// Commits `plan`. Takes no token: it ends committed, refused before its first write
// (SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED when anything it was planned against
// changed), or failed with its rollback reported. Warnings arrive in the outcome with
// SPIELORDERS_RESULT_OK. A plan may be committed once; a second commit refuses as its
// evidence no longer stands.
SpielordersResult spielorders_plan_commit(const struct SpielordersPlan *_Nonnull plan,
                                          struct SpielordersReport *_Nullable *_Nonnull report,
                                          struct SpielordersEntries *_Nullable *_Nullable outcome);

// Frees `plan`. Freeing NULL does nothing.
void spielorders_plan_free(struct SpielordersPlan *_Nullable plan);

// SPIELORDERS_REPORT_COMMITTED or SPIELORDERS_REPORT_UNCHANGED.
SpielordersConstant spielorders_report_kind(const struct SpielordersReport *_Nonnull report);

// A fresh read of the Order the commit wrote, as a new handle the caller frees, or NULL
// when it wrote none or removed it.
struct SpielordersOrder *_Nullable spielorders_report_order(const struct SpielordersReport *_Nonnull report);

// A fresh read of the Vendor the commit wrote, as a new handle the caller frees, or
// NULL when it wrote none.
struct SpielordersVendor *_Nullable spielorders_report_vendor(const struct SpielordersReport *_Nonnull report);

// Where the committed Order Directory now is, or "". Lists refresh by opening the
// Package again.
SpielordersPathBytes spielorders_report_final_path(const struct SpielordersReport *_Nonnull report);

// Frees `report`. Freeing NULL does nothing.
void spielorders_report_free(struct SpielordersReport *_Nullable report);

// A fact of `order`: a `SPIELORDERS_ORDER_FIELD_*` constant. "" for an absent fact or a
// field of another family.
SpielordersText spielorders_order_field(const struct SpielordersOrder *_Nonnull order,
                                        SpielordersConstant field);

// How many Item rows `order` holds.
int64_t spielorders_order_item_count(const struct SpielordersOrder *_Nonnull order);

// A fact of Item row `row` of `order`: a `SPIELORDERS_ITEM_FIELD_*` constant. "" when
// out of range.
SpielordersText spielorders_order_item_field(const struct SpielordersOrder *_Nonnull order,
                                             int64_t row_index,
                                             SpielordersConstant field);

// The Order Directory of `order`.
SpielordersPathBytes spielorders_order_directory(const struct SpielordersOrder *_Nonnull order);

// The README of `order`, read with it; always a handle the caller frees.
struct SpielordersReadme *_Nullable spielorders_order_readme(const struct SpielordersOrder *_Nonnull order);

// Frees `order`. Freeing NULL does nothing.
void spielorders_order_free(struct SpielordersOrder *_Nullable order);

// A fact of `vendor`: a `SPIELORDERS_VENDOR_FIELD_*` constant. "" for an absent fact or
// a field of another family.
SpielordersText spielorders_vendor_field(const struct SpielordersVendor *_Nonnull vendor,
                                         SpielordersConstant field);

// How many Contact rows `vendor` holds.
int64_t spielorders_vendor_contact_count(const struct SpielordersVendor *_Nonnull vendor);

// A fact of Contact row `row` of `vendor`: a `SPIELORDERS_CONTACT_FIELD_*` constant. ""
// when out of range.
SpielordersText spielorders_vendor_contact_field(const struct SpielordersVendor *_Nonnull vendor,
                                                 int64_t row_index,
                                                 SpielordersConstant field);

// Frees `vendor`. Freeing NULL does nothing.
void spielorders_vendor_free(struct SpielordersVendor *_Nullable vendor);

// The `SPIELORDERS_README_STATE_*` of `readme`.
SpielordersConstant spielorders_readme_state(const struct SpielordersReadme *_Nonnull readme);

// The text of `readme`; "" when it is missing or unreadable.
SpielordersText spielorders_readme_text(const struct SpielordersReadme *_Nonnull readme);

// Where `readme` is, whether or not it exists.
SpielordersPathBytes spielorders_readme_path(const struct SpielordersReadme *_Nonnull readme);

// Frees `readme`. Freeing NULL does nothing.
void spielorders_readme_free(struct SpielordersReadme *_Nullable readme);

#endif  /* SPIELORDERS_H */
