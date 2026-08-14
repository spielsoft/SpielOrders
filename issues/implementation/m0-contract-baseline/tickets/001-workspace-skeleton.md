---
title: Establish the Rust, CLI, Tauri, and frontend workspace skeleton
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

Create one buildable project skeleton whose dependency direction makes the Rust core authoritative and keeps the CLI, Tauri, and React layers as adapters or presentation.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Prototype the SpielOrders desktop experience](../../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)

## Blocking

Unblocked.

## Scope

- Create a Rust workspace with a reusable core library boundary and a separate `spielorders` CLI binary adapter.
- Create a Tauri 2 desktop shell backed by React and TypeScript; browser deployment is not a target.
- Establish dependency direction so the CLI and Tauri layers may depend on core, while core depends on neither adapter and no Rust layer depends on frontend code.
- Provide documented clean-environment build and test entry points for the Rust workspace and desktop shell.
- Keep SpielOrders-owned workspace members local. Reuse architectural precedent from SpielGantt only as reference; do not extract or consume a new shared foundation.

## Acceptance criteria

- A clean checkout can build the Rust workspace, CLI binary, frontend, and Tauri shell using the documented commands.
- The core library can be compiled and tested without compiling the CLI, Tauri adapter, or frontend.
- Dependency inspection shows no cycle and no adapter dependency from core.
- Launching or smoke-testing the shell requires no valid SpielOrder Package and exercises no Package rule.
- The review contains no domain model, filesystem repository behavior, CLI command tree, desktop workflow, schema implementation, or speculative SpielGantt abstraction.

## Review boundary

Review build topology, dependency direction, toolchain configuration, and clean-build reproducibility only. Behavioral Package work belongs to later milestones.

## Resolution

Implemented a Rust workspace containing the independent `spielorders-core` library,
the `spielorders` CLI adapter, and a Tauri 2 desktop adapter plus React/TypeScript
frontend. Added locked toolchains, documented clean-build commands, and a shell that
starts without reading or requiring a SpielOrder Package.

Acceptance evidence (2026-07-21):

- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed.
- `cargo test -p spielorders-core` — passed independently.
- `cargo tree` inspection — core has no dependencies; CLI and desktop depend on core;
  core has no adapter dependency.
- `npm --prefix apps/desktop clean-install` — passed from `package-lock.json`.
- `npm --prefix apps/desktop run test` and `run build` — passed.
- `npm --prefix apps/desktop run tauri build -- --no-bundle` — passed and produced the
  macOS shell binary.
- Scope audit found no domain model, repository behavior, CLI command tree, Tauri
  commands, desktop workflow, schema, migration, or SpielGantt abstraction.
