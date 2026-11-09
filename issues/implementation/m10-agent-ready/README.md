---
title: M10 — Agent Ready
status: deferred
---

## Scope

The [implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md#m10--agent-ready)
defines scope, sequencing, and prerequisites. Ordinary Package commands already exist
in the [CLI command surface](../../../crates/spielorders-cli/src/cli.rs) and are covered
by [executable contract tests](../../../crates/spielorders-cli/tests/cross_command_conformance.rs).
M10 reuses them; it does not rebuild the CLI or commission general cleanup.

The missing command features are the opt-in preparation, status, and initialization
entry points in [Define the SpielOrders agent skill contract](../../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md),
with generated guidance and its GUI action. Those need decomposition; the ticket below
covers delivery of the existing executable. Work starts only after the roadmap's GUI
prerequisites are accepted.

## Acceptance gate

The roadmap defines milestone acceptance.
[AGENTS.md](../../../AGENTS.md) governs execution and review.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Deliver the existing CLI to external agents](tickets/001-bundled-cli-and-agent-access.md) | Deferred | Milestone prerequisites in the roadmap |
