---
title: Decide a global option's exclusions where the caller cannot write around them
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T14:02:11Z
closed_at: 2026-09-06T17:24:06Z
review_start: 48c6276dd273449687ebcdfd02aa57fbefcab3fc
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A decided mutual exclusion holds wherever a caller writes the options, and the tests that
say so exercise every placement the command line allows.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Target and read one Order Directory](005-order-reads-and-targeting.md)

## Blocking

Returned by M4's external validation as V-1, recorded in [VALIDATION.md](../VALIDATION.md).

## Scope

Two decided exclusions do not hold. `--uuid` and `--root` are mutually exclusive, and the
four output modes are mutually exclusive with `--quiet` silent on success. Both are
enforced by `conflicts_with` between options declared global, which cannot see across the
boundary between the top level and a subcommand — so the exclusion fires or does not
according to where the caller wrote the option:

- `order show --root X --uuid ID` exits `2`; `--root X order show --uuid ID` exits `0`, and
  the same shape lets an `order edit` commit.
- `package show --root X --json --quiet` exits `2`; `--json package show --root X --quiet`
  exits `0` and prints the whole JSON document, so a caller who asked for silence gets one.

**Decide every exclusion among global options once, after parsing**, in the place that
already turns a parse failure into the usage response, where the whole command line is in
hand and no boundary exists. **Do not add per-pair `conflicts_with` rules**: that is the
mechanism that failed, and another instance of it is another position away from working.

**The tests are half the ticket.** For each excluded pair, one test drives the executable
with the options in **every placement the command line allows** — before the subcommand,
after it, and split across it — and asserts each as exit `2` carrying the shared usage
identity. The two existing tests certify the property from the one position where it holds;
they are **replaced** by these, not kept beside them.

The conformance suite already walks every command. If it can enumerate the excluded pairs
from the registry rather than from a list written by hand, do that, so the next global
option is covered by construction rather than by someone remembering this ticket.

## Acceptance criteria

- Every decided exclusion among global options is refused at exit `2` with the shared usage
  identity in every placement a caller can write, including split across the subcommand.
- `--quiet` is silent on success in every placement, and no placement lets a mutation commit
  that another placement refuses.
- No exclusion is enforced by a `conflicts_with` rule between global options.
- The replaced single-position tests are gone, not superseded in place.
- Each fix is verified by reverting it and observing the new tests fail.
- No `contract_version` advances; any cell that changes is re-recorded in place at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Which options exist, which pairs are excluded, or what a usage error says.
- Non-global options, whose conflicts are already decided where they are declared.
- V-2 and V-3, which the validation carried to M5.

## Result

Every exclusion among global options is decided in `cli::parse` from one registry, over
the whole match tree — so a pair written on either side of a subcommand is refused where a
`conflicts_with` rule saw neither, and none of those declarations is left. Each refusal
keeps the sentence it shipped, composed from the parser's own spelling of the two options,
and a unit test refuses an exclusion naming an option the parser does not declare.

Both pairs are asserted from the executable in every placement the command line allows,
generated from the command path in `tests/support`, by
`tests/output_modes_and_streams.rs` and `tests/order_reads_and_targeting.rs` — the second
of which snapshots the Package against the placement that used to commit an edit.
`tests/cross_command_conformance.rs` reads its pairs out of the registry. Each fix was
verified by reverting it and watching only its own tests fail.
