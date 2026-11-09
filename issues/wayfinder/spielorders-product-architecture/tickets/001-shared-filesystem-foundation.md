---
title: Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders remains independent of SpielGantt. A future sharing seam is policy-neutral filesystem mechanics, not domain schemas, CLI contracts, search, or frontend models.

Extract only an operation demonstrated in both products with matching behavior, including path safety, collision refusal, durability, partial failure, and opaque-content preservation. The shared code accepts caller-owned serialization and domain policy; it does not impose a universal Package schema or GUI framework. Similar visual design is not evidence for shared code.
