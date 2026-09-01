---
title: Define Vendor identity and historical semantics
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

How does an Order identify its one Vendor; which Vendor facts are shared; what historical facts remain fixed on old Orders; and how should Vendor rename, merge, visibility, and incomplete information behave?

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

### Duplicate evidence, visibility, and incomplete information

SpielOrders has no Vendor Merge operation. When legacy or adoption evidence appears
to duplicate an existing Vendor, the user must select the existing Vendor, create a
deliberately distinct Vendor, or leave the Order without a Vendor. SpielOrders never
combines Vendor facts or redirects managed Order references as a merge.

A Vendor is never deleted. A Vendor the user no longer wants offered for new use is
**hidden** instead, which is permitted whether or not Orders reference it — the usual
reason to hide one is precisely that it is no longer used for new Orders — and is
reversible at any time. Its record and Vendor ID stay in `vendors.json` permanently, so
every Vendor Reference keeps resolving the Vendor's current shared facts and SpielOrders
itself can never orphan one. Hiding governs what is offered, never what is resolved.

Vendor ID and Vendor Name are the only required Vendor facts. An Order whose Vendor is not known has no Vendor Reference; placeholder Vendors such as `Unknown` or `Vendor Not Recorded` are not created.

An **Unresolved Vendor Reference**—an Order containing a Vendor ID absent from `vendors.json`—is an integrity failure, not an unknown Vendor. SpielOrders does not infer a replacement from the Order Directory name or silently create one; repair must explicitly reassign or clear the reference. The detailed diagnostic and repair interaction remains with [Define integrity, validation, and repair boundaries](008-integrity-repair.md), while reconciliation of untracked legacy names remains with [Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md).
