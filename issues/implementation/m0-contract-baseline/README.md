---
title: M0 — Contract baseline and project skeleton
status: closed
closed_at: 2026-07-22
progress: 7/7 tickets closed; corrected acceptance passed
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
---

## Outcome

Establish the buildable project boundaries, version-1 metadata schemas, contract fixtures, test seams, and traceability rules that M1 and every later milestone can implement against without relocating Package rules into adapters.

This milestone implements only the baseline described by [M0 — Contract baseline and project skeleton](../../roadmaps/spielorders-implementation-roadmap.md#m0--contract-baseline-and-project-skeleton). It does not implement Package loading, domain validation, persistence, CLI commands, desktop behavior, or migrations.

## Ticket order and dependencies

| Order | Ticket | Blocking |
| --- | --- | --- |
| 1 | [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](tickets/001-workspace-skeleton.md) | Closed 2026-07-21 |
| 2 | [Define core ports and the typed frontend capability seam](tickets/002-boundary-seams.md) | Closed 2026-07-21 |
| 3 | [Publish the metadata-version-1 JSON Schemas](tickets/003-version-1-json-schemas.md) | Closed 2026-07-21 |
| 4 | [Create the canonical contract fixture corpus](tickets/004-contract-fixtures.md) | Closed 2026-07-21 |
| 5 | [Establish decision traceability and contract-test conventions](tickets/005-traceability-conventions.md) | Closed 2026-07-21 |
| 6 | [Integrate and verify the M0 acceptance gate](tickets/006-m0-acceptance-gate.md) | Closed 2026-07-21; gate passed |
| 7 | [Require two parent segments in the Order package locator](tickets/007-require-two-parent-package-root.md) | Closed 2026-07-22; correction gate passed |

The initial frontier is therefore **Establish the Rust, CLI, Tauri, and frontend workspace skeleton**, **Publish the metadata-version-1 JSON Schemas**, and **Establish decision traceability and contract-test conventions**. They may be implemented and reviewed independently.

```text
Workspace skeleton -> Boundary seams ---\
                                       \
Version-1 schemas -> Contract fixtures ----> M0 acceptance gate
                                       /
Traceability conventions -------------/
```

## Milestone acceptance

**Passed 2026-07-21; corrected and revalidated 2026-07-22.** See
[M0 acceptance evidence](ACCEPTANCE.md).

M0 closes only when all six tickets are closed and the final acceptance ticket demonstrates that:

- the empty Rust core and CLI adapter, Tauri 2 shell, and React/TypeScript frontend build through documented clean-environment commands;
- the accepted version-1 examples validate against checked-in strict JSON Schema Draft 2020-12 documents, including the required Order `package_root` locator;
- adapters depend on typed boundaries and contain no Package rule;
- canonical fixtures cover the accepted valid, invalid, damaged, unsupported, and interrupted-state categories without inventing a version-0 migration; and
- every implementation ticket and contract test can be traced to closed decisions by name.

## Milestone exclusions

- No Package or Order discovery behavior.
- No Rust metadata models or semantic validator; those begin in M1.
- No atomic filesystem implementation, mutation, CLI command behavior, Tauri command behavior, or production UI.
- No extraction or shared code package with SpielGantt.
- No stable diagnostic-code catalog beyond categories already required by the decisions.
- No schema version beyond metadata version 1.
