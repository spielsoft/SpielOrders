---
title: Edit complete Order facts with atomic assignment semantics
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Order fact edits are complete and atomic. They support explicit optional-field clearing,
Vendor assignment, reusable-value reuse/introduction, and the `ordered` date rule while
leaving directory alignment explicit.

Independent review corrected unreadable-record classification and the warning-path
README projection. Later durable-write work added semantic destination evidence, and
Ticket 026 preserved unreadable README state.

Primary evidence: `order_edits.rs` and `order_fact_edits.rs`.

No remaining M3 work is owned here.
