---
title: Define Vendor identity and historical semantics
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

### Vendor identity and shared facts

Every Vendor has an immutable opaque **Vendor ID**, parallel to Order ID. Vendor Name,
file position, and Order Directory text are facts or projections, never identity. A
Vendor retains the same Vendor ID when any shared fact changes.

Every Vendor requires exactly one nonblank **Vendor Name**. Vendor Names are unique
within a SpielOrder Package after trimming surrounding whitespace and comparing without
regard to letter case. A Vendor does not have separate display, legal, or alias names.

`contracts/schemas/v1/vendors.schema.json` is the record of the optional shared facts a
Vendor and its Vendor Contacts carry. Credentials and payment-card information are never
among them. Order-specific quotes, prices, catalog numbers, and correspondence belong to
the relevant Order, normally as Supporting Material.

### Order association and live semantics

An Order stores at most one optional **Vendor Reference**, represented solely by Vendor
ID. It stores no Vendor Name or other Vendor snapshot. Every Vendor fact shown for an
Order is resolved live from the current Vendor record, including on old Orders.

Vendor association is editable at every Order Status. Assigning, correcting, changing, or
clearing the Vendor Reference retains no history and never renames an Order Directory.

Renaming a Vendor is an ordinary edit to the shared Vendor Name: the Vendor ID is
unchanged and every referencing Order immediately resolves the new name.

### Duplicate evidence, visibility, and incomplete information

SpielOrders has no Vendor Merge operation. When legacy or adoption evidence appears to
duplicate an existing Vendor, the user must select the existing Vendor, create a
deliberately distinct Vendor, or leave the Order without a Vendor. SpielOrders never
combines Vendor facts or redirects managed Order references as a merge.

A Vendor that no Order references may be **deleted**, removing its record and Contacts
from `vendors.json`; deletion is refused while any healthy Order references it, naming
each such Order, and cannot be undone. A Vendor still in use that the user no longer
wants offered for new use is **hidden** instead, which is permitted whether or not Orders
reference it and is reversible at any time. Either way every Vendor Reference keeps
resolving the Vendor's current shared facts, and SpielOrders itself never orphans one.
Hiding governs what is offered, never what is resolved.

Vendor ID and Vendor Name are the only required Vendor facts. An Order whose Vendor is
not known has no Vendor Reference; placeholder Vendors such as `Unknown` or `Vendor Not
Recorded` are not created.

An **Unresolved Vendor Reference** — an Order containing a Vendor ID absent from
`vendors.json` — is an integrity failure, not an unknown Vendor. SpielOrders does not
infer a replacement from the Order Directory name or silently create one; repair must
explicitly reassign or clear the reference. [Define integrity, validation, and repair
boundaries](008-integrity-repair.md) owns the diagnostic and repair interaction, and
[Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md) owns
reconciliation of untracked legacy names.
