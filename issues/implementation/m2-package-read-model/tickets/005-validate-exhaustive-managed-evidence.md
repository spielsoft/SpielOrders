---
title: Validate exhaustive managed evidence without inspecting Supporting Material
status: open
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

A Rust caller can explicitly validate all M2-governed canonical artifacts, exact
conventional READMEs, cross-record rules, and immediate reserved metadata contents
and receive one complete deterministic evidence report, without changing prior
fast-load evidence or inspecting Supporting Material.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Load a current Package Index without canonical record rereads](004-load-current-index-fast-path.md).

## Scope

- Expose exhaustive validation as a separate public Rust-core operation. It must
  not retrofit broader evidence into a prior ordinary Package load result.
- Read every governed root canonical artifact, every discovered candidate's exact
  `order.json`, and exact Package/Order READMEs; apply all local and cross-record
  rules regardless of whether ordinary opening previously used the fast path.
- List only the immediate entries of the root `.spielorders` directory and each
  discovered Order's immediate `.spielorders` directory to diagnose unexpected
  reserved content and recognizable beside-destination staging artifacts.
- Complete the stable diagnostic code/evidence vocabulary with
  `reserved_content_unexpected` and `staging_artifact_present`, including canonical
  destination validity when already known. Preserve observed content; never
  promote, clean, move, or rewrite staging evidence.
- Return every Package, Order, and document diagnostic supported by the exhaustive
  operation in the exact grouping, severity, deduplication, scope, and ordering
  contract. Package Index state/refresh and operation failure remain separate typed
  records rather than diagnostic groups.
- Preserve the exact relationship between validation invalidity and ordinary use:
  any Damaged Order makes exhaustive Package validation invalid, while healthy
  Orders in an otherwise current Package remain available outside this validation
  report; warnings never create damage.
- Inject and map every read/list/observe failure through the filesystem port. When
  an effect cannot establish a validation result, return the typed operation
  failure without fabricating a canonical diagnostic; persistently observed
  required-artifact failures remain diagnostics exactly once.

Exclude repair cases/actions, raw correction, staging promotion/cleanup, index
replacement solely for validation, native/search behavior, and all adapter
presentation.

## Canonical fixtures and fault injection

- Reuse every applicable canonical scenario: damaged Package/Order, direct-child
  warnings, unexpected reserved content, README absence/unavailability, duplicate
  identities/names, unresolved Vendor Reference, staging artifact, unsupported
  metadata, current no-op, and both preservation assets.
- Add only narrow multi-condition recipes needed to prove exhaustive grouping,
  exact duplicate elimination, complete duplicate-ID participant evidence,
  deterministic order independent of filesystem enumeration/source messages, and
  staging-plus-invalid-destination coexistence.
- The public filesystem double must inject canonicalize, kind observation,
  directory-list, and exact read failures at every authorized boundary and log all
  effects. No test may use a recursive helper whose behavior exceeds the port
  contract.

## Supporting Material preservation requirements

Exhaustive means exhaustive only inside SpielOrders ownership. The operation must
never list a general Order Directory, recurse below it, inspect a Supporting
Material link/permission/name, inventory content, or use it as diagnostic evidence.
Immediate listing of an Order `.spielorders` directory is authorized; sibling or
nested general content is not. Tests must preserve both existing opaque binary
sentinels byte-for-byte and prove neither was observed.

## Core/filesystem-port and adapter boundary

The core owns breadth, stable evidence, diagnostic grouping/order, validation
validity, and damage semantics. The filesystem port supplies only exact bytes,
no-follow facts, authorized immediate listings, and opaque errors. No CLI validate
envelope, Tauri command, React diagnostic workspace, repair action, or platform
taxonomy is created.

## Acceptance criteria

- Exhaustive validation reports all supported canonical, cross-record, exact
  README, immediate reserved-content, and staging diagnostics in one deterministic
  result.
- Package, document, and each lexically ordered Order group follow the closed
  grouping contract; errors precede warnings and exact duplicates collapse without
  collapsing per-Order duplicate-ID evidence.
- Derived-index assessment/reasons/refresh and operation failures stay outside
  diagnostic groups.
- Fast-load evidence remains unchanged and intentionally narrower before and after
  an exhaustive call.
- Missing READMEs remain nondiagnostic empty documents; unavailable documents warn
  without damaging their owners; warnings do not block ordinary availability.
- Effect logs prove only authorized immediate reserved listings and exact managed/
  README reads occur, with zero general Order Directory listing or recursion.
- No validation path mutates any byte, staging artifact, canonical file, README, or
  Supporting Material.

## Review boundary

Review the separately invoked broader-evidence operation, its complete stable
diagnostics, and its strict inspection boundary. Reject repair execution, recursive
validation, general inventory, retroactive fast-load evidence, adapter behavior,
or a speculative diagnostic/recovery framework.

## Exact fixed-point claim protocol

Require Tickets 001–004 closed, this ticket open/unclaimed, and an empty
`git status --porcelain=v1 --untracked-files=all`. Record exact `git rev-parse HEAD`
as `ticket_start_fixed_point`, add `status: in progress`, `assignee`, and
`claimed_at`, and commit the claim alone. Use that pre-claim SHA for review and
stop on any inconsistency.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. Begin with one public RED test
showing a reserved warning that ordinary fast load deliberately lacks but
exhaustive validation finds without scanning Supporting Material. Add one
RED-to-GREEN behavior at a time for breadth, grouping/order/deduplication, README
evidence, combined damage/warnings, and all port failures. Each test states the
observable defect it catches.

Commit the completed behavior before using the project-local `code-review` skill
against the exact fixed point. Resolve and separately record Standards and Spec,
rerun affected gates, commit the disposition, and close only this ticket.

## Required verification

Focused gates:

- dedicated exhaustive-validation/evidence-boundary integration suite;
- canonical load, fast path, README, diagnostics, index, paths, and filesystem-port
  suites affected by composition;
- complete authorized-boundary failure matrix; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- core/adapter, Supporting Material, mutation/repair, and M3 scope audits; and
- exact before/after hashes for both opaque sentinels.

Use pinned Node and do not update dependencies or lockfiles.

## Handoff requirement

After closure, generate the complete next-worker prompt for the first eligible
ticket—expected to be [Integrate M2 acceptance and refresh the complete selected-major toolchain](006-integrate-m2-acceptance-and-refresh.md)—with
the exact clean fixed-point protocol, one-ticket rule, applicable TDD, full gate,
refresh/approval/install-script constraints, review, preservation, and external-
validation handoff. Report a blocker/claim rather than skipping it.
