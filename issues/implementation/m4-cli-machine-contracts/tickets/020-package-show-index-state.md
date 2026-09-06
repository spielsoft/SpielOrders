---
title: Report the derived index's real state in the Package bootstrap read
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T19:07:48Z
closed_at: 2026-09-05T19:27:32Z
review_start: 1560763cfe25deb26bdf4e1a7e5ed8c9c75e6539
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`package show` answers what state the derived Package Index is in, so the read the CLI
decision calls the complete GUI bootstrap does not require a second command to learn it.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md), whose
milestone-wide review found this and returned it.

## Scope

The CLI capability decision says `package show` is the complete GUI-bootstrap read and that
its typed result includes upgrade and index state. It reports the index's location and then
`not_assessed` in every case: over a Package whose stored index is malformed, `index status`
answers `malformed` and `package show` still answers `not_assessed`. A GUI that opened a
Package would have to run a second command to learn what the first one's contract says it
already carries.

The reason recorded beside the projection is that the load which assesses the index also
durably rebuilds it, and durable writes are reserved for a place a canonical write has
already happened. That reason does not hold: the core has a non-rebuilding assessment, it
is what `index status` uses, and `index status` writes nothing — asking what state the
index is in is not allowed to be the thing that changes it, and it is not. So the
constraint the comment states is not the constraint the core has, and the comment is part
of what this ticket corrects.

What stays true is the rest of that paragraph: a mutation that wrote nothing still answers
`not_assessed`, because a dry run and an unchanged plan reach no assessing load at all, and
a mutation that did replace a record answers with what its own refresh produced. Only the
reading path changes.

## Acceptance criteria

- `package show` reports the derived index's real state and its typed reasons over a
  Package whose index is current, missing, malformed, at a version this build does not
  project, and stale — the same states `index status` distinguishes, from the same core
  assessment, so the two commands cannot come to disagree.
- `package show` writes nothing. A read that assessed the index by rebuilding it would be
  the defect this ticket must not introduce, and the test says so from a byte comparison of
  the Package rather than from a claim.
- A mutation's reported index state is unchanged: `not_assessed` where the mutation wrote
  nothing, and the refreshed state where it did.
- The recorded `package.show` cells are re-recorded at contract version 1. No
  `contract_version` advances: nothing has run outside a test tree.
- The comment that states the superseded constraint is corrected in the same commit.

## Exclusions

- Upgrade state, which `package show` already reports and this ticket does not touch.
- Any change to `index status`, `index rebuild`, or the core's index assessment.
- Making `package show` repair, rebuild, or refresh anything.

## Result

`package show` takes the core's non-rebuilding index assessment — the one `index status`
answers from — so the complete GUI-bootstrap read says whether the derived index is
current, missing, malformed, at a version this build does not project, or stale, with the
typed reasons behind it, so the two cannot come to disagree about one Package. The read
still writes nothing: not a byte, and not a modification time. The narrower
Package reads keep the load that asks the stored index nothing, and a mutation that wrote
nothing still answers `not_assessed`.

The comment justifying the old answer by a constraint the core does not have is corrected
beside the projection.

The recorded `package.show` cells are re-recorded in place at contract version 1: the
current Packages report `current`, and the one carrying a Damaged Order reports `stale`.

Tests: `crates/spielorders-cli/tests/package_show.rs`, with the mutation half beside the
refresh claims in `crates/spielorders-cli/tests/vendor_mutations.rs`.
