---
title: Adopt exactly one existing Order Directory in place
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

One eligible physical direct-child directory can be adopted in place from a complete,
side-effect-free plan. Adoption preserves Supporting Material and an existing README,
creates only current managed metadata, and reports post-commit index failures as
warnings.

Plan evidence refuses a changed directory or reusable-value context. An unreadable
README is projected as unknown rather than empty; Ticket 026 supplied the shared fix and
an adoption-specific regression.

Primary evidence: `adoption.rs`, `order_adoption.rs`, and
`unreadable_order_readme.rs`.

No remaining M3 work is owned here.
