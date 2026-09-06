---
title: Define core ports and the typed frontend capability seam
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

Make every nondeterministic or platform-owned dependency explicit and give the frontend one typed capability seam that can be backed by Tauri in production and a fake in tests.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Prototype the SpielOrders desktop experience](../../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](001-workspace-skeleton.md) closed on 2026-07-21.

## Scope

- Define narrow core-facing ports for filesystem operations, local calendar date, UUID generation, OS-indexed Supporting Material search, and platform file open/reveal behavior.
- Keep application preferences and session state outside core; expose them through a frontend-side capability where presentation needs them.
- Define the typed frontend capability interface that isolates Tauri invocation and native services from React state and components.
- Supply compile-time or minimal fake implementations sufficient to prove substitution and dependency direction, without implementing production behavior.
- Record which side owns cancellation and asynchronous boundaries for OS-indexed search, without defining the later provider payload or search algorithm prematurely.

## Acceptance criteria

- Core code can be compiled against test doubles for every nondeterministic or platform-owned dependency named by M0.
- React-facing code can be compiled and minimally tested against a fake capability implementation without importing Tauri APIs.
- Tauri-specific types do not escape the production capability adapter, and frontend or Mantine types do not enter Rust core contracts.
- Filesystem mechanics remain SpielOrders-local; no shared crate or frontend package with SpielGantt is introduced.
- Interfaces contain no Package behavior, durable schema duplication, CLI JSON contract, native-search semantics, or desktop presentation policy.

## Review boundary

Review ownership, substitutability, dependency direction, and async/cancellation seams. Defer concrete operations and payload breadth until the milestone that implements them.

## Resolution

Added core-facing ports for byte-oriented filesystem effects, local calendar date,
UUIDv4 generation, asynchronous OS-indexed Supporting Material search cancellation,
and native file open/reveal. Added one React-facing capability interface, a Tauri-only
adapter module, frontend-local session state, and reusable fake capabilities. The
boundary note records async and cancellation ownership while deferring M5 payloads.

Acceptance evidence (2026-07-21):

- `cargo test -p spielorders-core` — passed with one fake substituting for every core
  effect port.
- `npm --prefix apps/desktop run test` — passed two tests using fake capabilities and
  frontend-local session state without loading Tauri.
- `cargo test --workspace` and `npm --prefix apps/desktop run build` — passed.
- Import audit found Tauri APIs only in `src/adapters/tauri-capabilities.ts`; no
  frontend or Mantine type enters Rust.
- Scope audit found no Package behavior, schema duplication, CLI JSON contract,
  search semantics, presentation policy, shared SpielGantt crate, or production
  filesystem implementation.
