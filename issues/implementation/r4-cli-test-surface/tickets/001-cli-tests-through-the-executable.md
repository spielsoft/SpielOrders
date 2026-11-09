---
title: Make the CLI a binary its tests drive from outside
status: closed
assignee: claude-r4-worker
claimed_at: 2026-11-07T18:04:13Z
review_start: f14553d74be12c22a9c73d343c4afec945544961
closed_at: 2026-11-07T18:17:54Z
parent: R4 — CLI test surface removal
---

## Outcome

`spielorders-cli` ships an executable and nothing its tests alone reach. Its library
target, which exists only so the integration suite can import internals, is gone or
reduced to what the executable itself needs. The suite drives the built executable and
states the expected behavior itself instead of reading it out of production code.

## Decision sources

- [AGENTS.md](../../../../AGENTS.md), Ticket execution: production code never exists to
  serve tests; tests use real seams and assert outcomes.
- [CLI contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Traceability](../../../../docs/development/traceability.md) for where response
  fixtures live.
- [Running tests](../../../../docs/development/test-loop.md)

## Blocking

None.

## Scope

Compiling the CLI with its public modules made crate-private shows the items no
production path reaches: the option-exclusion reader, the response-contract registry
slice, and the contract fixture-path method. The public modules themselves, and the
public value-kind, platform, response, and command items the suite imports, exist only
for the suite. Remove all of it. Where the suite derives expectations from production
code (contract registry, fixture homes, exclusions, value kinds), state the expectation
in test code, keeping any compile-time completeness check inside the tests. Where the
suite calls internal functions directly, drive the executable or move the check into a
unit test beside the code. Update the lib documentation and `docs/development/test-loop.md`
if the target layout changes.

## Acceptance criteria

- With every non-test item crate-private, a non-test build of `spielorders-cli` reports
  no dead code, and no `pub` item exists that only tests use.
- No production item, parameter, branch, or visibility remains that only tests reach.
- Every behavior the suite asserted before is still asserted, through the executable or
  a unit test beside the code; no assertion is dropped silently. A removed assertion is
  named in the Result with the reason.
- Response fixtures and contracts are unchanged; no `contract_version` changes.
- The two-axis code review and `scripts/gate.sh` pass.

## Exclusions

The `spielorders` library and its C interface, `contracts/`, metadata, the Swift app,
the checks crate, CLI behavior, and AGENTS.md.

## Result

`spielorders-cli` is an executable with no library target; its modules are private and
nothing in them is reached only by tests. The suite states its own contract registry, in
`tests/support/contracts.rs`, where one list declares the type and the registry, plus the
decided exclusions and the value kinds. Unit tests in `src/commands.rs` compare the
Supporting Material answer cells and runtime index states, with Packages built by the
executable's own commands. Unit tests in `src/cli.rs` hold the parser's exclusion groups
to the decided sets.

Two assertions are removed. The check that the suite's `material_answered` helper
rejects unrelated codes tested a test helper, not the executable. The compile-time link
from the executable's contract type to the suite's matches is gone because the suite now
owns its registry. An unlisted contract is still caught through its unclaimed fixture
directory.
