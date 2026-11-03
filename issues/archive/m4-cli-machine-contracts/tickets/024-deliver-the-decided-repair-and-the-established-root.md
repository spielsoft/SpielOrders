---
title: Deliver the decided locator repair, and answer with the roots a command established
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T01:18:16Z
closed_at: 2026-09-06T01:58:50Z
review_start: a1674be9089463f4f52052917c15f6f50e1d2561
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A locator a closed decision calls repairable is repairable, a response names every root
the command resolved, and a suggested command is one that would do something.

## Decision sources

- [Diagnose and repair Package integrity](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [The fixture convention](../../../../docs/development/traceability.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md), whose third run
halted and referred these rather than repairing them, because none is the gate's to repair.

## Scope

**A decided repair delivered for half the damage it was decided for.** The integrity
decision names a `package_root` locator missing, malformed, stale, or resolving to another
Package as repairable by recalculation. Only the last two reach that repair: the locator's
deserializer refuses anything but two-or-more parent-only segments, so an absent or
malformed one fails the whole record's shape and the only action offered is to author a
complete replacement by hand, while the Order sits at error severity.

Fix it at the seam and by type: a record whose locator is absent or malformed but is
otherwise a valid Order parses far enough to be diagnosed as a locator case and reaches the
recalculation repair, by the locator field carrying its own parse outcome rather than
failing its parent. **Nothing else in the record's validation loosens.**

**A context that reports no Package where the command established one.** An `order`
command's two-root context is built from a constructor that fails if either root has no
Package-relative spelling, so one unspellable root reports both as `null` and discards a
Root the command resolved — against the rule stated beside both the projection and its
caller. Each root takes its spelling independently.

Its guard is worse than the defect: the one test for the claim hand-builds both arguments,
so it passes with the fix reverted. **A test that passes with the fix reverted is deleted,
not kept.**

**A suggestion naming a command that would answer with nothing to do.** `validate` tells a
user to run `repair` on a Package whose only error-severity finding is Order-scoped, where
`repair` answers no cases; `upgrade` says the same for a record at a version the capability
contract forbids repair from interpreting. The text travels in `error.message` under
`--json`, so it is a false statement in a machine response. Both suggest `repair` only
where `repair` on that scope would offer an action, and otherwise say what the user can
actually do.

## Acceptance criteria

- Each of the four locator damages the integrity decision names — absent, malformed, stale,
  and resolving to another Package — is diagnosed as a locator case and reaches the
  recalculation repair, asserted through the executable rather than against the core.
- A record whose locator is absent or malformed and which is otherwise invalid still fails
  for that other reason; no validation the record already performed is relaxed.
- A response carrying one root without a Package-relative spelling still names the other.
- The test guarding that fails when the fix is reverted, whether by driving a real process
  or by asserting the projection with one spelled and one unspelled root.
- `validate` and `upgrade` suggest `repair` only where it would offer an action.
- No `contract_version` advances; a cell that changes is re-recorded at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Any other loosening of record validation, or any change to which damages are diagnosable.
- The unreachable branches the third run reported as sound: they stay as they are.
- Anything the third run recorded as below the bar, which is M5's.

## Result

The `package_root` locator carries its own parse outcome instead of failing its record, so
a locator that is absent or malformed is diagnosed as the locator case that offers
recalculation — the repair
[Diagnose and repair Package integrity](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
decided for all four damages. Nothing else loosens: a record invalid for another reason
still fails for it, an indexed Order still requires a locator, and the upgrade and repair
write guards still refuse one that has none.

An `order` command names each root it established on its own, asserted over the projection
with one root that has no JSON spelling, because APFS holds no such name.

`validate` and `upgrade` name a repair command only where that command would answer with a
case. One cell re-records at contract version 1. Nothing advanced.

Tests: the CLI suite's repair, validating, and `order` files, and the core's metadata file.
