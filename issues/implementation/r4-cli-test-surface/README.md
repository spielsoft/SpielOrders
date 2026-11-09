---
title: R4 — CLI test surface removal
status: closed
---

## Scope

Remove production surface in the Rust workspace that only tests reach, under the
[AGENTS.md](../../../AGENTS.md) rule that production code never exists to serve tests.
The C interface is unchanged.

## Acceptance gate

The ticket closes under [AGENTS.md](../../../AGENTS.md); the owner accepts the milestone.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Make the CLI a binary its tests drive from outside](tickets/001-cli-tests-through-the-executable.md) | Closed | None |
