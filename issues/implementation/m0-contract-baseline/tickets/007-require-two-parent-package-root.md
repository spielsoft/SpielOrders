---
title: Require two parent segments in the Order package locator
status: closed
assignee: codex
claimed_at: 2026-07-22
closed_at: 2026-07-22
parent: M0 — Contract baseline and project skeleton
labels:
  - implementation:m0
  - implementation:correction
---

## Outcome

Correct the version-1 canonical and indexed Order schemas so `package_root` cannot
name the SpielOrders Root as the Order Directory's immediate parent.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. External M0 validation identified a schema expression defect; the closed
decisions already determine the correction.

## Scope

- Require at least two `..` path segments in `order.json.package_root` and the
  complete Order projection embedded in `index.json`.
- Add one-purpose rejected canonical and indexed fixtures for `package_root = ".."`.
- Rerun schema, traceability, Rust, frontend, dependency, scope, and Tauri M0 gates.
- Update the M0 acceptance evidence, milestone status, and roadmap status.

## Acceptance criteria

- Both version-1 schemas reject `"package_root": ".."` and continue accepting
  `"../.."` plus deeper normalized parent-only locators.
- The invalid fixture manifest covers canonical and indexed single-parent locators.
- All recorded M0 executable gates pass after the correction.
- No M1 model, parser, semantic validation, Package behavior, or schema version is
  introduced.

## Review boundary

Review only the minimum locator cardinality, its regression fixtures, and renewed M0
evidence. Path resolution and Package semantics remain M1 work.

## Resolution

Changed both version-1 Order locator patterns to require at least two parent
segments. Added canonical and indexed one-parent rejection fixtures plus direct
regression assertions proving `".."` fails while `"../.."` and `"../../.."` pass.

Acceptance evidence (2026-07-22):

- `npm run test:schemas` — passed with 17 rejected fixtures.
- `npm run check:traceability` — passed for seven M0 tickets and four baseline test
  sources.
- Clean-copy `cargo build --workspace`, `cargo test --workspace`, and independent
  core tests — passed.
- Clean-copy frontend typechecking, tests, production build, and Tauri no-bundle
  release build — passed.
- Dependency and scope audits remained unchanged: adapters depend inward on a
  dependency-free core, with no M1 behavior introduced.
