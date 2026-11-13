---
title: Define the Order record and lifecycle contract
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

### Canonical Order record

Every Order has an immutable opaque **Order ID** that remains stable across edits and
Order Directory renames. The directory path is location, not identity; duplicate embedded
IDs are integrity conflicts.

`contracts/schemas/v1/order.schema.json` is the record of what `order.json` holds. Order
Title, Date Ordered, and Order Status are required at every status; a Vendor association,
Orderer, Ordering Institution, Order Currency, Project, and Order Group are optional at
every status.

Status never certifies metadata completeness. Narrative notes live in the Order README,
shared Vendor facts live in `vendors.json`, and directory names, calculated totals,
Supporting Material inventories, lifecycle history, and additional lifecycle dates are
never duplicated into the Order record.

### Embedded Order Items and prices

Order Items have no independent identity and exist only in their user-controlled array
order within an Order. Each has a required Description, Quantity, and Unit Price, an
optional Catalog Number, and optional multiline Notes for remarks tied to that line
alone, such as specifications, quote references, or permitted substitutions; remarks about
the whole Order belong in its README. Quantity and Unit Price have no sign restriction.

Item Total Price is Quantity multiplied by Unit Price, and Order Total is the sum of Item
Total Prices. Neither total is stored or manually overridden; fees, shipping, taxes, and
discounts may be ordinary Order Items. Calculations are not rounded, and each front end
chooses the precision it displays monetary values with.

Order Currency is an optional three-letter ISO code applied uniformly as decoration to
every price in the Order. When absent, numbers are unlabeled; changing the code relabels
existing numbers without conversion or mutation.

### Dates

Date Ordered is the only canonical Order date. It defaults to the user's current local
calendar date when an Order is created and may remain a provisional archival date while
the Order is `In Preparation` or on `Hold`; month-level accuracy is sufficient. Entering
`Ordered` from another status resets Date Ordered to the then-current local date.

Neither a fact edit nor that reset renames the Order Directory. [Define the SpielOrders
package topology](002-package-topology.md) owns directory projection and alignment.

### Reusable free-entry classifications

Ordering Institution, Orderer, Project, and Order Group are optional Free-Entry Values,
at most one from each category per Order, each suggesting values already used in the
SpielOrder Package. An Order Group belongs to the Project of the Orders that use it: it
is suggested, reused, and introduced among that Project's groups, or among the groups of
Orders with no Project, and the same name under two Projects is two groups with no
uniqueness check across them. Changing an Order's Project clears its group unless the new
Project already has a group of that name or the same change names a different group.

Ordering Institution names the institution through which the Order is placed. Orderer is
a recognition and search aid, not an application account or purchasing actor. Project
names the principal scientific or administrative effort. Order Group is an intentionally
user-defined grouping within a Project with no prescribed meaning. The groups are derived
from the Orders that use them, so a Project or group exists while an Order uses it; adding
unused values, replacing a used value, and merging spellings would store them as the
Vendor records are stored.

### Lifecycle

New Orders default to `In Preparation`; imported or adopted Orders may start at any
status. `In Preparation`, `Hold`, `Ordered`, `Received`, and `Complete` are user-assigned
labels. Every direct status-to-status transition is valid, backward corrections are
allowed, and no transition history is retained. SpielOrders does not infer `Received` or
any other status from delivery completeness, Order Items, or populated metadata.
