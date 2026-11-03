---
title: Conform every command to one response contract
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T14:10:00Z
closed_at: 2026-09-05T17:29:27Z
review_start: 839f3eb08eb6d5db41c510a488b167379171a9c4
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

Every command the executable registers obeys the same machine contract, established by
inventory rather than by sampling. A client that has learned one command's envelope, exit
statuses, path representation, and empty-collection behavior has learned all of them, and
the golden-fixture matrix proves it for every command and every cell that command can reach.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the SpielOrders agent skill contract](../../../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)
- [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md)

## Blocking

Blocked by every command ticket, 002 through 014. Tickets 017 and 018 are not command
tickets and carry no blocking edge here, but both must close before the gate.

## Scope

- **Build the command inventory first, from the decisions, not from the code.** Enumerate
  the CLI decision's command tree, add `adopt-order`, which that tree omits, and record —
  rather than perform — the routing of `search` to M5 and the `agent` family to M8. Count
  each `package values <kind>` separately, because a parameter is a segment of the identity.
  **The inventory runs in both directions**: a decided command the executable lacks is a
  finding, and so is a registered command no decision names.
- Establish conformance across that set, correcting whatever does not conform: one UTF-8
  document on stdout under `--json` with no incidental output on success, failure, and usage
  error; `contract` identities unique and matching their command; `contract_version` a
  consecutive positive integer per contract; the envelope's members present as required and
  failures carrying `ok: false`, `data: null`, and a structured `error`; `snake_case` keys
  and enum values, stable codes, `null` for absent optionals, present empty collections,
  absolute plus relevant relative paths, and domain UUIDs; exit statuses mapping as decided;
  `--dry-run` accepted and side-effect-free on every mutating command; the four output modes
  mutually exclusive and `--quiet` silent on success; `--root` never searching ancestors;
  and human output nowhere a parsing contract.
- **Complete the golden-fixture matrix** over that set, recording why wherever a cell is
  genuinely unreachable, so an absent fixture and an impossible one do not look alike.
  `usage-error` is not a column: it carries the shared identity with its single fixture.
- **Walk `contracts/cli/` in the reverse direction**, which nothing does today: a directory
  no registered contract claims is a finding.
- Consolidate the mutation-kind switches in the vendor commands onto the core's canonical
  outcome, and the Vendor and Contact matrix tests onto one shape, only where the
  consolidation removes more than it adds; otherwise record nothing and leave it.
- Re-record every refusal cell carrying a null Package-relative path where
  [the fixture convention](../../../../docs/development/traceability.md) now requires one.
  No `contract_version` advances: the field is already optional.
- Give `add-order` and `adopt-order` the field-naming refusal `order edit` has for a
  malformed Date Ordered or Currency, so two commands do not describe one rule differently.
- Add the conformance checks as executable repository checks over the whole set, so a
  command added later is checked by construction.
- Correct the stale command count in the source comments that state it.
- Confirm generated help and version output is present and accurate for every command, since
  M8's generated references cite it.

## Acceptance criteria

- The inventory exists as evidence: every decided command, its contract name, and its
  disposition — delivered here, or routed with the owning milestone named.
- Each conformance claim is checked over the entire set by an executable check, not by a
  per-command test. A check that only examined a hand-written list does not satisfy this.
- Every `--json` invocation across the set writes exactly one document and nothing else,
  asserted by capturing real process streams.
- No two commands share a `contract` identity, and no version was advanced for formatting or
  wording.
- Every mutating command accepts `--dry-run` and leaves the Package byte-identical under it,
  including the ones whose own ticket already tested it.
- The fixture matrix is complete, and each deliberate hole carries its recorded reason.
- An executable check fails for a `contracts/cli/` directory no registered contract claims.
- Every conformance defect is fixed here or, where it is a capability defect, returned to its
  owning ticket with the evidence, and the ticket records which and why.
- `scripts/gate.sh` passes.

## Exclusions

- **New commands and new capabilities.** A decided command that was never built is a finding
  routed to a new ticket, not work absorbed here.
- **Capability defects.** A command that conforms and returns the wrong domain answer is its
  owning ticket's. Fixing it here would hide it.
- Renaming a contract, restructuring a payload, or bumping a version for tidiness.
- The `search` and `agent` families, inventoried only to record their routing.
- Milestone acceptance, which is Ticket 016's.

## Result

Every decided command is inventoried in both directions and held to one contract as a set:
the envelope, the exit statuses, `--dry-run`, the output modes, `--root`, the case rules, the
path representation, and the domain identities. The fixture matrix says, contract by
contract, which cells are recorded and why the rest are unreachable, and `contracts/cli/` is
walked in reverse. Every conformance defect found was fixed here — the Package's spelling on
a refusal inside one, a suggested command a shell could not run, a field-naming refusal two
Order commands described differently — and none was a capability defect to return.
Fixtures the changes touched were re-recorded at version 1, in place.

The set-wide checks are in `crates/spielorders-cli/tests/cross_command_conformance.rs` and
each contract's recorded bytes in `crates/spielorders-cli/tests/response_contract.rs`.
