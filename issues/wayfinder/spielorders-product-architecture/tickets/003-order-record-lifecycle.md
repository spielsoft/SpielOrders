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

### Reusable values

Ordering Institution, Orderer, Project, and Order Group are optional values, at most one
from each category per Order. A Package's values are the ones `vendors.json` keeps beside
the Vendors and the ones its healthy Orders use, so a Package whose record keeps none
still has every value its Orders use. An Order accepts only an existing value; a new value
is added deliberately, on its own or by the change that first uses it. An Order Group
belongs to a Project, or to the Orders with no Project: it is chosen among that Project's
groups, and the same name under two Projects is two groups with no uniqueness check across
them. Changing an Order's Project clears its group unless the new Project already has a
group of that name or the same change names a different group.

Values compare trimmed and case-insensitively, and a kept spelling is preferred over an
Order's. Renaming a value changes every Order that uses it, and renaming onto an existing
value merges the two. Merging keeps one chosen spelling, which every affected Order then
uses. Deleting a used value requires a replacement, or none, since every value field is
optional, and every affected Order uses the replacement. A Project's Orders keep their
groups when it is renamed, merged, replaced, or cleared, and those groups then belong to
the resulting Project, or to the Orders with no Project. Each of these changes reports how
many Orders it changes.

Ordering Institution names the institution through which the Order is placed. Orderer is
a recognition and search aid, not an application account or purchasing actor. Project
names the principal scientific or administrative effort. Order Group is an intentionally
user-defined grouping within a Project with no prescribed meaning.

### Merging Orders

Orders are merged by naming them in precedence order. The first is kept with its Order
ID, Title, Date Ordered, Status, and every fact it has; each later Order in turn fills only
what the result still lacks: a missing optional fact, an Order Group that comes with its
Project or joins the same Project, and each Item the result does not already hold. Every
entry of a later Order's directory that the result does not hold is moved into the kept
Order at the same relative path, and an entry it already holds — the README included —
stays behind. What remains of each later Order is removed as an Order removal removes
one, so every entry left behind stays recoverable. A merge never renames the kept Order's
directory.

### Lifecycle

New Orders default to `In Preparation`; imported or adopted Orders may start at any
status. `In Preparation`, `Hold`, `Ordered`, `Received`, and `Complete` are user-assigned
labels. Every direct status-to-status transition is valid, backward corrections are
allowed, and no transition history is retained. SpielOrders does not infer `Received` or
any other status from delivery completeness, Order Items, or populated metadata.
