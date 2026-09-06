---
title: Mutate ordered Vendor Contacts by one-based position
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Vendor Contacts can be listed, appended or inserted, edited, moved, and removed by
one-based pre-mutation position. Every mutation rewrites one validated Vendor aggregate
atomically and preserves Vendor identity, Orders, READMEs, and Supporting Material.

The suite covers both collection boundaries, a one-position forward move, no-op,
invalid positions, changed destination evidence, failures, and index warnings.

Primary evidence: `vendors.rs` and `vendor_contacts.rs`.

No remaining M3 work is owned here.
