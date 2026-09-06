---
title: Vendor lifecycle capability
status: superseded
superseded_by: tickets/019-vendor-visibility-not-deletion.md
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

The original guarded-deletion scope is obsolete. Vendors are never deleted; they can be
created, shown, edited, hidden, and unhidden. Hidden Vendors remain resolvable and retain
their immutable IDs.

The current capability and governing decisions were completed and reviewed by Ticket
019. Primary evidence: `vendors.rs` and `vendor_lifecycle.rs`.

Do not restore Vendor deletion or its reference guard.
