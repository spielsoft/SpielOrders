---
title: Deliver the existing CLI to external agents
status: deferred
parent: M10 — Agent Ready
---

## Outcome

An external agent invokes the CLI bundled with the installed native app to inspect and
mutate an authorized Package while the GUI is closed. CLI delivery does not require a
separate installation.

## Decision sources

- [Milestone scope and acceptance](../README.md)
- [Native desktop distribution](../../../../docs/adr/0001-native-desktop-distribution.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)
- [CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [First collection conversion](../../../../docs/development/first-collection-conversion.md)

## Blocking

The milestone prerequisites in the roadmap. Do not start delivery probes or packaging
during GUI development.

## Scope

Bundle the existing executable and establish external launch and persistent Package
authorization. The CLI's Package operations and native-service adapters already exist;
reuse them, changing code only where installed delivery demonstrably requires it. Resolve any
required distribution or permission choice with the owner rather than changing product
policy. Preserve the existing library interface and command contracts; do not introduce
domain logic in a launcher.

## Acceptance criteria

- An external agent-style process invokes the exact bundled CLI with the GUI closed
  and inspects and mutates a disposable Package. Help output or GUI-child invocation
  alone is not proof.
- Evidence records the actual executable, entitlements, launch context, authorization,
  and results. An unsandboxed pass or privilege elevation is not sandbox proof.
- Authorized access survives a new invocation; absent, denied, or stale authorization
  produces an honest refusal without changing the Package.
- The installed app supplies the executable in place, with no separate installer or
  required global copy. GUI operations still use directly linked Rust.
- Existing executable contract checks and the repository gate pass, with independent
  Standards and Spec review. Delivery introduces no unrelated command or behavior change.
- Tests write only disposable data; the source archive and owner preferences remain
  untouched. Unverified Store behavior is not reported as established.

## Exclusions

General CLI cleanup or refactoring, rebuilding implemented Package commands, App Store
submission, an alternate distribution channel, a backend service, sandbox disabling,
changes to the settled GUI architecture, and production archive conversion. Agent
guidance and preparation/status behavior require their own decomposition.
