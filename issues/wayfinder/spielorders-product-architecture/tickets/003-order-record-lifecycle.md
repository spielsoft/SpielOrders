---
title: Define the Order record and lifecycle contract
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

Which facts belong to an Order and its embedded Order Items; which are required at each lifecycle stage; how do dates, prices, Order Currency, Ordering Institution, Project, and Order Group behave; and which transitions are valid among `In Preparation`, `Hold`, `Ordered`, `Received`, and `Complete`?

## Blocking

Unblocked.

## Resolution

### Canonical Order record

Every Order has an immutable opaque **Order ID** that remains stable across edits and Order Directory renames. The directory path is location, not identity; duplicate embedded IDs are integrity conflicts.

The canonical `order.json` contains a technical schema version and these domain facts:

- required at every status: Order ID, Order Title, Date Ordered, and Order Status;
- optional at every status: one Vendor association, Orderer, Ordering Institution, Order Currency, Project, and Order Group;
- zero or more embedded Order Items.

Status never certifies metadata completeness. Narrative notes live in the Order README, shared Vendor facts live in `vendors.json`, and directory names, calculated totals, Supporting Material inventories, lifecycle history, and additional lifecycle dates are not duplicated into the Order record.

### Embedded Order Items and prices

Order Items have no independent identity and exist only in their user-controlled array order within an Order. Each stores a required Description, required floating-point Quantity, required floating-point Unit Price, and optional Catalog Number. Quantity and Unit Price have no sign restriction.

Item Total Price is calculated as Quantity multiplied by Unit Price. Order Total is the sum of Item Total Prices. Neither total is stored or manually overridden; fees, shipping, taxes, and discounts may be ordinary Order Items. Calculations are not rounded, while displayed monetary values use two decimal places.

Order Currency is an optional three-letter ISO code applied uniformly as decoration to every price in the Order. When absent, numbers are unlabeled; changing the code relabels existing numbers without conversion or mutation.

### Dates and directory projection

Date Ordered is the only canonical Order date. It defaults to the user's current local calendar date when an Order is created and may remain a provisional archival date while the Order is `In Preparation` or on `Hold`; month-level accuracy is sufficient. Entering `Ordered` from another status resets Date Ordered to the then-current local date.

The aligned Order Directory basename is `YYYY_MM_DD[_Vendor]_Title[_Orderer]`. Date Ordered and Order Title are required; the Vendor and Orderer components are omitted when absent. Creation succeeds only when the projected basename is unique. Fact edits and the `Ordered` date update do not rename the directory; only the explicit **Align Directory Name** action does so, and a collision stops alignment without inventing a suffix.

### Reusable free-entry classifications

Ordering Institution, Orderer, Project, and Order Group are optional free-entry values, with at most one value from each category per Order. Each field suggests values already used in the SpielOrder Package. Surrounding whitespace is trimmed and matching is case-insensitive, so an existing value's canonical spelling is reused. A genuinely new value requires explicit confirmation.

Ordering Institution names the institution through which the Order is placed. Orderer is a recognition and search aid, not an application account or purchasing actor. Project names the principal scientific or administrative effort. Order Group is an intentionally user-defined grouping with no prescribed meaning; its reuse interaction is provisional and may be revisited if the suggestion list becomes unwieldy.

### Lifecycle

New Orders default to `In Preparation`; imported or adopted Orders may start at any status. `In Preparation`, `Hold`, `Ordered`, `Received`, and `Complete` are user-assigned labels. Every direct status-to-status transition is valid, backward corrections are allowed, and no transition history is retained. SpielOrders does not infer `Received` or any other status from delivery completeness, Order Items, or populated metadata.
