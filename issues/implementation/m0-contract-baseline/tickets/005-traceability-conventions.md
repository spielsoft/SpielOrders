---
title: Establish decision traceability and contract-test conventions
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

Define a lightweight convention that lets reviewers trace every implementation issue and contract test to the closed decision names it implements.

## Decision sources

- [Chart the SpielOrders product and architecture](../../../wayfinder/spielorders-product-architecture/MAP.md)
- [SpielOrders implementation roadmap](../../../roadmaps/spielorders-implementation-roadmap.md)

## Blocking

Unblocked.

## Scope

- Define the required implementation-ticket fields: outcome, decision sources by linked name, blockers, scope, acceptance criteria, review boundary, and explicit exclusions where needed.
- Define a test naming or metadata convention that points to decision names without coupling tests to mutable line numbers or bare ticket identifiers.
- Define where shared contract fixtures live and how Rust, CLI, Tauri, and frontend tests refer to the same source fixture rather than maintain divergent copies.
- Define the rule for contradictions discovered during implementation: stop the affected behavior, create a new decision ticket, and never settle the contradiction silently in code.
- Apply the convention to every M0 ticket as the first conformance check.

## Acceptance criteria

- A reviewer can navigate from each M0 ticket to every governing closed decision and from the milestone index to each ticket.
- The convention uses decision names in human-facing text; paths or identifiers remain link targets rather than substitutes for names.
- Shared fixtures have one canonical location and a documented consumer pattern for all layers.
- The convention adds no generated traceability database, custom issue tracker, runtime dependency, or requirement to restate whole decisions in implementation tickets.
- A documented check reports a missing decision source or required ticket section before the M0 acceptance gate can pass.

## Review boundary

Review navigability, maintenance cost, and enforcement clarity. This ticket does not create product contracts or decide implementation behavior.

## Resolution

Documented the required implementation-ticket sections, decision-name convention,
baseline test markers, canonical fixture-consumer paths, and contradiction escalation
rule. Added a static repository check that follows links and verifies M0 tickets,
the milestone index, closed decision names in baseline tests, and canonical fixture
locations without adding a runtime dependency or generated database.

Acceptance evidence (2026-07-21):

- `npm run check:traceability` — passed for all six M0 tickets and four baseline test
  sources.
- The check verifies required ticket sections, linked decision names and targets,
  milestone-to-ticket navigation, recognized closed decision names in tests, and the
  three canonical contract fixture locations.
- `cargo test --workspace`, `npm --prefix apps/desktop run test`, and
  `npm run test:schemas` — all passed after applying the convention.
- Review confirmed all layers consume `contracts/` in place, contradictions stop and
  escalate to a named decision ticket, and no custom tracker, generated traceability
  database, runtime dependency, or decision restatement was added.
