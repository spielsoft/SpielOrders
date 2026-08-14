---
title: Integrate and verify the M0 acceptance gate
status: closed
assignee: codex
claimed_at: 2026-07-21
closed_at: 2026-07-21
parent: M0 — Contract baseline and project skeleton
labels:
  - implementation:m0
  - implementation:task
---

## Outcome

Integrate the independently reviewed M0 outputs and prove that they form one clean, buildable, traceable baseline for M1.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Prototype the SpielOrders desktop experience](../../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [M0 — Contract baseline and project skeleton](../README.md)

## Blocking

Unblocked. All five preceding tickets closed on 2026-07-21:

- [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](001-workspace-skeleton.md)
- [Define core ports and the typed frontend capability seam](002-boundary-seams.md)
- [Publish the metadata-version-1 JSON Schemas](003-version-1-json-schemas.md)
- [Create the canonical contract fixture corpus](004-contract-fixtures.md)
- [Establish decision traceability and contract-test conventions](005-traceability-conventions.md)

## Scope

- Run and document the clean-environment build and baseline test commands for the Rust workspace, CLI adapter, frontend, and Tauri shell.
- Run schema conformance over all accepted and rejected JSON fixtures and report fixture coverage by artifact and scenario category.
- Verify dependency direction and substitute the fake core/platform and frontend capabilities supplied by the boundary-seams ticket.
- Run the traceability check across the M0 milestone index, all M0 tickets, and baseline contract tests.
- Record the M0 completion evidence and close the milestone only if every acceptance condition passes without waivers.

## Acceptance criteria

- All build and baseline test commands pass from a clean checkout using documented prerequisites.
- Every accepted version-1 example, including `order.json.package_root`, validates; every named invalid schema fixture is rejected.
- Core remains buildable without CLI, Tauri, or frontend dependencies, and frontend tests can use the fake capability without Tauri.
- No Package rule appears in adapter or presentation layers, and no SpielGantt shared foundation has been introduced.
- The traceability check covers all six M0 tickets and their baseline tests.
- A scope audit finds no Package loading, semantic validator, production filesystem behavior, CLI command implementation, desktop workflow, migration transformation, or other M1+ behavior.

## Review boundary

Review integrated evidence and milestone readiness. Fixes discovered here should return to the owning M0 ticket when they are not purely integrative; this ticket must not become a catch-all implementation change.

## Resolution

Ran the complete M0 gate in a fresh artifact-free copy and recorded the evidence in
[M0 acceptance evidence](../ACCEPTANCE.md). Both lockfile installs, Rust builds and
tests, independent core test, frontend tests/build, schema/fixture conformance,
traceability, dependency inspection, scope audit, and Tauri no-bundle release build
passed without waiver.

Acceptance evidence (2026-07-21):

- Clean-environment commands: all passed.
- Schema corpus: 4 Draft 2020-12 schemas, 8 accepted examples, 15 named rejected
  fixtures, 22 scenarios, and 2 byte-distinct preservation sentinels passed.
- Boundaries: core has no dependency; CLI and Tauri depend inward on core; frontend
  fake tests load no Tauri API.
- Traceability: all six M0 tickets and four baseline test sources passed.
- Scope audit: no M1+ behavior or SpielGantt shared foundation was found.
