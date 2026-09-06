---
title: M1 — Filesystem and metadata kernel
status: closed; externally validated
closed_at: 2026-07-25
validated_at: 2026-07-26
progress: 7/7 tickets closed; M1-V1 through M1-V4 remediated; externally validated
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
---

## Outcome

Implement the durable metadata vocabulary, semantic-validation kernel, safe Package
path boundary, and explicitly assigned low-level atomic filesystem primitives through
the public Rust core interface. Package loading and diagnostics remain M2; domain
mutations and persistence workflows remain M3.

This decomposition was approved on 2026-07-22.

The reviewed acceptance gate's executable results are recorded in
[M1 acceptance evidence](ACCEPTANCE.md). After M1-V1 through M1-V4 were remediated,
a fresh [M1 external validation](VALIDATION.md) independently verified the complete
remediated range and passed on 2026-07-26.

## Ticket order and dependencies

| Order | Ticket | Blocked by |
| --- | --- | --- |
| 1 | [Round-trip the complete metadata-version-1 vocabulary through the Rust core](tickets/001-metadata-v1-vocabulary.md) | Closed 2026-07-22; M1-V3 remediated 2026-07-25 |
| 2 | [Validate an Order and calculate its exact totals through the core](tickets/002-order-validation-and-totals.md) | Closed 2026-07-22 |
| 3 | [Validate Package identities, Vendors, live references, and reusable values](tickets/003-package-vendor-semantics.md) | Closed 2026-07-23 |
| 4 | [Resolve safe Package paths and classify direct-child Order candidates](tickets/004-safe-package-paths.md) | Closed 2026-07-23; M1-V2 remediated 2026-07-25 |
| 5 | [Preserve one canonical file through validated atomic replacement](tickets/005-atomic-canonical-replacement.md) | Closed 2026-07-24 |
| 6 | [Restore prior canonical state after a failed multi-file replacement](tickets/006-multi-file-rollback.md) | Closed 2026-07-25; M1-V1 remediated 2026-07-25 |
| 7 | [Integrate and verify the M1 filesystem and metadata kernel acceptance gate](tickets/007-m1-acceptance-gate.md) | Closed 2026-07-25 |

```text
Corrected, committed M0
          |
          v
001 Metadata vocabulary
   /         |          \
  v          v           v
002 Order   004 Paths   005 Atomic file
  |                      |
  v                      v
003 Package/Vendor     006 Rollback
   \         |          /
    \        |         /
             v
       007 M1 gate
```

All seven implementation tickets are closed and the M1 acceptance gate passed
without waiver on 2026-07-25.

## Shared TDD seam convention

- Tests exercise public Rust core behavior rather than private helpers.
- Fakes are permitted only at the filesystem effect boundary; metadata and domain
  modules run as real code.
- Work proceeds one behavioral RED test and minimal GREEN implementation at a time.
- Every proposed test states the observable defect it would catch.
- All layers consume the canonical `contracts/` fixtures in place; no ticket copies
  them into a layer-specific fixture tree.
- Exact Rust identifiers may be refined before the first RED test. A material seam
  change after implementation starts must first be recorded in the owning ticket.

## Shared code-review fixed point

Every ticket starts only after all blockers are integrated and the worktree is clean.
Before its first edit, the implementing agent records the exact resolved
`git rev-parse HEAD` SHA in the claimed ticket. All ticket changes must be committed
before `/code-review`; review uses `git diff <fixed-point>...HEAD` and
`git log <fixed-point>..HEAD`. A floating ref such as `main` is not an acceptable
fixed point.

Ticket 001 started from the corrected standalone M0 baseline and closed on
2026-07-22. Every later ticket must likewise record its own clean exact fixed point.

## Milestone exclusions

- No Package loading, Damaged Package/Order projections, stable diagnostic catalog,
  README behavior, or Package Index staleness/rebuild behavior; those belong to M2.
- No initialization, Order/Vendor edits, alignment, removal, dry-run plans,
  Metadata Upgrade orchestration, or other persistence workflows; those belong to M3.
- No CLI, Tauri command, or frontend behavior.
- No speculative shared crate with SpielGantt.

## Resolution

Closed 2026-07-25 after the reviewed acceptance gate proved schema/Rust agreement,
semantic and filesystem composition, real no-follow containment, complete
single-file and multi-file failure behavior, exact Supporting Material preservation,
core independence from every adapter layer, and the absence of M2/M3 scope.

No downstream implementation work began as part of this milestone closure or its
external validation. M1 is implemented and externally validated. Before M2 is
decomposed or implemented, the separately bounded
[M1-upgrade toolchain and dependency modernization](../m1-upgrade-toolchain-dependencies/README.md)
must close and pass fresh external validation. M2 implementation has not begun.
