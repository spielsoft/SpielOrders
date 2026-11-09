/*
 * A C caller's walk through one disposable Package, built and run by the gate:
 * initialize it, open it, create Orders, list and read one, edit two facts and one Item
 * row in a draft, read the planned fact changes, commit, see a refused save's errors by
 * field and row, see a stale save refused without writing, see a cancelled read, and
 * free every handle received.
 *
 * Decision sources: Effect and adapter boundaries, Interface rules; Define the Rust CLI
 * capability contract; Prototype the SpielOrders desktop experience; Durable
 * managed-file writes.
 *
 * Usage: package_walkthrough <empty directory>
 */
#include "spielorders.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

static void expect(int condition, const char *what) {
  if (!condition) {
    fprintf(stderr, "FAILED: %s\n", what);
    failures++;
  }
}

static void print_entries(const char *heading, const SpielordersEntries *entries) {
  printf("%s\n", heading);
  for (int64_t i = 0; i < spielorders_entries_count(entries); i++) {
    printf("  %s field=%s row=%lld after=\"%s\"\n",
           spielorders_constant_name(spielorders_entries_code(entries, i)),
           spielorders_constant_name(spielorders_entries_field(entries, i)),
           (long long)spielorders_entries_row(entries, i),
           spielorders_entries_after(entries, i));
  }
}

static int commit(SpielordersPlan *plan, SpielordersReport **report) {
  SpielordersEntries *outcome = NULL;
  SpielordersResult result = spielorders_plan_commit(plan, report, &outcome);
  if (result != SPIELORDERS_RESULT_OK) {
    print_entries("commit refused:", outcome);
  }
  spielorders_entries_free(outcome);
  return result;
}

/* Saves a new Order titled `title` in `package`. */
static void create_order(SpielordersPackage *package, const char *title) {
  SpielordersDraft *draft = NULL;
  SpielordersPlan *plan = NULL;
  SpielordersReport *report = NULL;
  expect(spielorders_package_new_order_draft(package, &draft, NULL) == SPIELORDERS_RESULT_OK,
         "a new Order draft");
  expect(spielorders_draft_set(draft, SPIELORDERS_ORDER_FIELD_TITLE, title, NULL) ==
             SPIELORDERS_RESULT_OK,
         "setting a title");
  expect(spielorders_draft_plan(draft, 0, &plan, NULL) == SPIELORDERS_RESULT_OK,
         "planning a new Order");
  expect(commit(plan, &report) == SPIELORDERS_RESULT_OK, "creating an Order");
  spielorders_report_free(report);
  spielorders_plan_free(plan);
  spielorders_draft_free(draft);
}

/* A fresh read of the one Order matching `query`, through a fresh open of the Package at
 * `root`: an opened Package is a snapshot, and lists refresh by opening it again. */
static SpielordersOrder *read_matching(const char *root, const char *query) {
  SpielordersPackage *package = NULL;
  SpielordersList *list = NULL;
  SpielordersOrder *order = NULL;
  expect(spielorders_package_open(root, 0, &package, NULL) == SPIELORDERS_RESULT_OK, "opening");
  expect(spielorders_package_orders(package, query, 0, -1, 0, 0, &list, NULL) ==
             SPIELORDERS_RESULT_OK,
         "listing Orders");
  expect(spielorders_list_count(list) == 1, "one Order matches");
  expect(spielorders_package_read_order(package, spielorders_list_path(list, 0), 0, &order,
                                        NULL) == SPIELORDERS_RESULT_OK,
         "reading the matching Order");
  spielorders_list_free(list);
  spielorders_package_free(package);
  if (order == NULL) {
    fprintf(stderr, "no Order to go on with\n");
    exit(1);
  }
  return order;
}

/* Saves `order` with the reusable-value `field` set to the new value `text`, confirmed,
 * against that read. */
static void save_fact(SpielordersOrder *order, SpielordersConstant field, const char *text) {
  SpielordersDraft *draft = NULL;
  SpielordersPlan *plan = NULL;
  SpielordersReport *report = NULL;
  spielorders_order_draft(order, &draft);
  spielorders_draft_set(draft, field, text, NULL);
  spielorders_draft_confirm(draft, field, -1, NULL);
  expect(spielorders_draft_plan(draft, 0, &plan, NULL) == SPIELORDERS_RESULT_OK, "planning");
  expect(commit(plan, &report) == SPIELORDERS_RESULT_OK, "saving");
  spielorders_report_free(report);
  spielorders_plan_free(plan);
  spielorders_draft_free(draft);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: %s <empty directory>\n", argv[0]);
    return 2;
  }
  const char *root = argv[1];

  /* Initialize the disposable Package. */
  SpielordersPlan *plan = NULL;
  SpielordersReport *report = NULL;
  expect(spielorders_plan_initialize(root, NULL, NULL, 0, &plan, NULL) == SPIELORDERS_RESULT_OK,
         "planning initialization");
  expect(commit(plan, &report) == SPIELORDERS_RESULT_OK, "initializing");
  spielorders_report_free(report);
  spielorders_plan_free(plan);

  /* Open it and create two Orders. */
  SpielordersPackage *package = NULL;
  expect(spielorders_package_open(root, 0, &package, NULL) == SPIELORDERS_RESULT_OK, "opening");
  expect(spielorders_package_state(package) == SPIELORDERS_PACKAGE_STATE_HEALTHY, "healthy");
  create_order(package, "Laser components");
  create_order(package, "Mirror mounts");
  spielorders_package_free(package);
  package = NULL;
  expect(spielorders_package_open(root, 0, &package, NULL) == SPIELORDERS_RESULT_OK,
         "opening again");

  /* Give the laser Order one Item row. */
  SpielordersOrder *order = read_matching(root, "laser");
  SpielordersDraft *draft = NULL;
  expect(spielorders_order_draft(order, &draft) == SPIELORDERS_RESULT_OK, "a draft");
  spielorders_draft_row_insert(draft, 0, NULL);
  spielorders_draft_row_set(draft, 0, SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Fiber collimator", NULL);
  spielorders_draft_row_set(draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "4", NULL);
  spielorders_draft_row_set(draft, 0, SPIELORDERS_ITEM_FIELD_UNIT_PRICE, "3", NULL);
  expect(strcmp(spielorders_draft_total(draft), "12") == 0, "the live total");
  expect(spielorders_draft_plan(draft, 0, &plan, NULL) == SPIELORDERS_RESULT_OK, "planning a row");
  expect(commit(plan, &report) == SPIELORDERS_RESULT_OK, "saving a row");
  spielorders_order_free(order);
  order = spielorders_report_order(report);
  expect(order != NULL, "the report's refreshed Order");
  spielorders_report_free(report);
  spielorders_plan_free(plan);
  spielorders_draft_free(draft);

  /* Edit two facts and one Item row, and read the planned changes. */
  expect(spielorders_order_draft(order, &draft) == SPIELORDERS_RESULT_OK, "a second draft");
  spielorders_draft_set(draft, SPIELORDERS_ORDER_FIELD_TITLE, "Laser optics", NULL);
  spielorders_draft_set(draft, SPIELORDERS_ORDER_FIELD_CURRENCY, "USD", NULL);
  spielorders_draft_row_set(draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "5", NULL);
  expect(spielorders_draft_plan(draft, 0, &plan, NULL) == SPIELORDERS_RESULT_OK, "planning edits");
  int facts = 0;
  for (int64_t i = 0; i < spielorders_plan_change_count(plan); i++) {
    printf("change %s %s row=%lld \"%s\" -> \"%s\"\n",
           spielorders_constant_name(spielorders_plan_change_kind(plan, i)),
           spielorders_constant_name(spielorders_plan_change_field(plan, i)),
           (long long)spielorders_plan_change_row(plan, i), spielorders_plan_change_before(plan, i),
           spielorders_plan_change_after(plan, i));
    if (spielorders_plan_change_kind(plan, i) == SPIELORDERS_CHANGE_FACT) {
      facts++;
    }
  }
  expect(facts == 3, "three planned fact changes");
  expect(commit(plan, &report) == SPIELORDERS_RESULT_OK, "committing the edits");
  spielorders_report_free(report);
  spielorders_plan_free(plan);
  spielorders_draft_free(draft);
  spielorders_order_free(order);

  /* A refused save names each error by field and row. */
  order = read_matching(root, "optics");
  spielorders_order_draft(order, &draft);
  spielorders_draft_set(draft, SPIELORDERS_ORDER_FIELD_TITLE, "", NULL);
  spielorders_draft_row_set(draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY, "four", NULL);
  SpielordersEntries *outcome = NULL;
  expect(spielorders_draft_plan(draft, 0, &plan, &outcome) == SPIELORDERS_RESULT_REFUSED,
         "an invalid draft is refused");
  expect(plan == NULL, "a refused plan gives no handle");
  print_entries("refused save:", outcome);
  int title = 0, quantity = 0;
  for (int64_t i = 0; i < spielorders_entries_count(outcome); i++) {
    if (spielorders_entries_field(outcome, i) == SPIELORDERS_ORDER_FIELD_TITLE &&
        spielorders_entries_row(outcome, i) == -1) {
      title = 1;
    }
    if (spielorders_entries_field(outcome, i) == SPIELORDERS_ITEM_FIELD_QUANTITY &&
        spielorders_entries_row(outcome, i) == 0) {
      quantity = 1;
    }
  }
  expect(title && quantity, "errors located at the title and at row 0's quantity");
  expect(strcmp(spielorders_draft_field(draft, SPIELORDERS_ORDER_FIELD_TITLE), "") == 0 &&
             strcmp(spielorders_draft_row_field(draft, 0, SPIELORDERS_ITEM_FIELD_QUANTITY),
                    "four") == 0,
         "the draft keeps what was typed");
  spielorders_entries_free(outcome);
  spielorders_draft_free(draft);

  /* A save against a record changed on disk after its read is refused, writing nothing. */
  spielorders_order_draft(order, &draft);
  spielorders_draft_set(draft, SPIELORDERS_ORDER_FIELD_PROJECT, "Telescope", NULL);
  spielorders_draft_confirm(draft, SPIELORDERS_ORDER_FIELD_PROJECT, -1, NULL);
  expect(spielorders_draft_plan(draft, 0, &plan, NULL) == SPIELORDERS_RESULT_OK, "a stale plan");
  SpielordersOrder *other = read_matching(root, "optics");
  save_fact(other, SPIELORDERS_ORDER_FIELD_ORDERER, "Ada Lovelace");
  spielorders_order_free(other);
  outcome = NULL;
  expect(spielorders_plan_commit(plan, &report, &outcome) == SPIELORDERS_RESULT_REFUSED,
         "a stale save is refused");
  expect(spielorders_entries_code(outcome, 0) == SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED,
         "as preconditions_changed");
  expect(report == NULL, "a refused commit gives no report");
  spielorders_entries_free(outcome);
  spielorders_plan_free(plan);
  spielorders_draft_free(draft);
  other = read_matching(root, "optics");
  expect(strcmp(spielorders_order_field(other, SPIELORDERS_ORDER_FIELD_PROJECT), "") == 0 &&
             strcmp(spielorders_order_field(other, SPIELORDERS_ORDER_FIELD_ORDERER),
                    "Ada Lovelace") == 0,
         "the refused save wrote nothing");
  spielorders_order_free(other);

  /* A cancelled read returns cancelled. */
  uint64_t token = spielorders_cancel_new();
  spielorders_cancel_request(token);
  other = NULL;
  expect(spielorders_package_read_order(package, spielorders_order_directory(order), token,
                                        &other, NULL) == SPIELORDERS_RESULT_CANCELLED,
         "a cancelled read");
  expect(other == NULL, "a cancelled read gives no handle");
  spielorders_cancel_release(token);

  spielorders_order_free(order);
  spielorders_package_free(package);
  if (failures == 0) {
    printf("walkthrough passed\n");
  }
  return failures == 0 ? 0 : 1;
}
