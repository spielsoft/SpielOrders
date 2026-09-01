---
title: Mutate ordered Order Items by one-based position
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Order Items can be listed, appended or inserted, edited, moved, and removed by one-based
pre-mutation position. Mutations recalculate totals, rewrite one complete Order record,
and inherit the reviewed Order-edit persistence and warning behavior.

The suite exercises every entry point against unreadable and unobservable records,
nonempty READMEs, position boundaries, subject reporting, failures, and no-op behavior.

Primary evidence: `order_edits.rs` and `order_item_collection.rs`.

No remaining M3 work is owned here.
