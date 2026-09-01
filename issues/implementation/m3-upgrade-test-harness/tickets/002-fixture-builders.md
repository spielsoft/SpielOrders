---
title: Let a core test read as its assertions plus a named fixture
status: ready-for-agent
parent: M3-upgrade — Test harness ergonomics
labels:
  - implementation:m3-upgrade
  - implementation:task
---

## Outcome

A `spielorders-core` integration test names the Package shape it needs and then asserts
against it. Inline filesystem construction no longer sits between the reader and what a
test is actually checking. Nothing about what any test asserts changes.

## Decision sources

- [Agent instructions](../../../../AGENTS.md) — the standing authorization for parallel
  review sub-agents.
- [Traceability](../../../../docs/development/traceability.md) — test and fixture names
  describe durable public behavior, and what a test file must still declare.
- [Make the Rust test suite cheap to iterate on](001-test-harness-ergonomics.md)
  — this ticket is that ticket's Scope part 4, handed back unstarted so the shared
  worktree could be released.

## Blocking

No capability blocker. Ticket 001 is closed and its harness is in place. The repository
has one shared worktree, and this ticket edits the same test files M3's remaining
execution tickets extend, so do not claim it while an M3 execution ticket holds the
worktree — and expect to rebase rather than to hold it.

## Scope

Inline filesystem fixture construction dominates the test sources. `support::
minimal_package` already exists and is used in a minority of places. Extract the
repeated fixture shapes into `crates/spielorders-core/tests/support/` so a test reads as
its assertions plus a named fixture.

- Find the shapes that actually repeat before extracting. A builder used twice is not
  yet a fixture.
- Fixture names describe the Package or Order state they produce, in the language
  `CONTEXT.md` defines — not the test that first needed them.
- Each former file's `//! Decision sources:` marker stays where traceability expects it.

## Acceptance criteria

- Test parity is proven: the same number of tests run, with the same names, and no test
  is deleted, ignored, weakened, or made to assert less. A name-level diff of the test
  inventory before and after is the evidence.
- The reduction is measured, not asserted: report the change in total test-source lines
  and in the number of inline construction sites replaced.
- The full workspace gate still passes, and `cargo fmt --check`, clippy with warnings
  denied, and both contract scripts on the pinned Node runtime still pass.

## Exclusions

No product behavior change. No change to what any test asserts. No M3 capability work.
Do not restructure the integration-test target Ticket 001 established, and do not
extract a fixture whose only caller is one test.

## Review boundary

Review the fixture extraction and the support module. Do not absorb M3 capability work,
and do not review the domain behavior the tests cover except to confirm it is unchanged.

## Closeout

Pending.
