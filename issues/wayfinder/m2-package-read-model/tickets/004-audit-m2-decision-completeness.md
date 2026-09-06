---
title: Audit M2 decision completeness and hand off implementation decomposition
status: closed
closed_at: 2026-08-13
parent: Chart the M2 Package read model
labels:
  - wayfinder:task
assignee: /root
---

## Decision

The M2 read-side specification is contradiction-free and decision-complete. Three
decisions are authoritative together, and no fourth is needed:

- [Decide how Package Index freshness is proven without canonical rereads](001-index-freshness-fast-path.md)
  defines the friendly-filesystem trust boundary, the conservative no-follow change
  cutoff, the deterministic canonical rebuild, and the blocking Package Index
  Replacement Failure.
- [Define the stable M2 diagnostic vocabulary and evidence contract](002-stable-diagnostic-contract.md)
  defines stable diagnostics and grouping while keeping Package Index assessment,
  refresh outcome, and operation failures separate.
- [Define the complete M2 Package read projection and load-state contract](003-package-read-projection.md)
  composes those contracts into the shared-core load results, current-Package
  projection, README reads, upgrade reporting, and exact evidence boundary.

Nothing else in this map restates them. Where one of the three describes a surface the
core now implements, the core is the record and the decision keeps only the rules that
constrain future change.

The destination of [Chart the M2 Package read model](../MAP.md) is reached, with no
remaining fog and no open decision ticket.
