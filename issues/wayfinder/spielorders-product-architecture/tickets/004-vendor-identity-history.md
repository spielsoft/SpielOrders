---
title: Define Vendor identity and historical semantics
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

How does an Order identify its one Vendor; which Vendor facts are shared; what historical facts remain fixed on old Orders; and how should Vendor rename, merge, deletion, and incomplete information behave?

## Blocking

Unblocked.

## Resolution

### Vendor identity and shared facts

Every Vendor has an immutable opaque **Vendor ID**, parallel to Order ID. Vendor Name, file position, and Order Directory text are facts or projections, never identity. A Vendor retains the same Vendor ID when any shared fact changes.

Every Vendor requires exactly one nonblank **Vendor Name**. Vendor Names are unique within a SpielOrder Package after trimming surrounding whitespace and comparing without regard to letter case. A Vendor does not have separate display, legal, or alias names.

All other Vendor facts are optional shared information:

- general phone, email, and website;
- one multiline address;
- zero or more named Vendor Contacts, each with optional role, phone, and email;
- free-form purchasing notes.

Credentials and payment-card information are never Vendor facts. Order-specific quotes, prices, catalog numbers, and correspondence belong to the relevant Order, normally as Supporting Material.

### Order association and live semantics

An Order stores at most one optional **Vendor Reference**, represented solely by Vendor ID. It stores no Vendor Name or other Vendor snapshot. Every Vendor fact shown for an Order is resolved live from the current Vendor record, including on old Orders.

Vendor association is editable at every Order Status. Assigning, correcting, changing, or clearing the Vendor Reference does not retain history. It may make the current Order facts disagree with the existing Order Directory name, which remains unchanged until the user explicitly invokes Align Directory Name.

Renaming a Vendor is an ordinary edit to the shared Vendor Name. The Vendor ID is unchanged, all referencing Orders immediately resolve the new name, and existing Order Directory names are not renamed automatically.

### Merge, deletion, and incomplete information

Vendor Merge is a rare, explicit reconciliation used when importing already managed SpielOrders records introduces a duplicate. SpielOrders asks the user what to do; it never merges automatically. The user chooses the surviving Vendor ID and resolves conflicting populated facts, imported Order references are redirected to the survivor, nonconflicting facts are combined, and the absorbed Vendor is removed. No historical alias or merge record is retained.

A Vendor referenced by any Order cannot be deleted. The user must first reassign or clear every Vendor Reference, or consolidate the Vendor through an applicable Vendor Merge. An unreferenced Vendor may be deleted after confirmation.

Vendor ID and Vendor Name are the only required Vendor facts. An Order whose Vendor is not known has no Vendor Reference; placeholder Vendors such as `Unknown` or `Vendor Not Recorded` are not created.

An **Unresolved Vendor Reference**—an Order containing a Vendor ID absent from `vendors.json`—is an integrity failure, not an unknown Vendor. SpielOrders does not infer a replacement from the Order Directory name or silently create one; repair must explicitly reassign or clear the reference. The detailed diagnostic and repair interaction remains with [Define integrity, validation, and repair boundaries](008-integrity-repair.md), while reconciliation of untracked legacy names remains with [Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md).
