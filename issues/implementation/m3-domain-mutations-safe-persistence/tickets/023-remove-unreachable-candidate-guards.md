---
title: Remove unreachable candidate-validation guards
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Candidate guards that could not fail after typed planning were removed from index,
Order-edit, Vendor, and metadata-repair write sites. Their impossible error types are
uninhabited so the boundary is compiler-visible.

The semantic Order-repair guard remains because it is reachable and prevents a repair
from writing a still-invalid record. Ticket 013 owns strengthening the separate Vendors
candidate path.
