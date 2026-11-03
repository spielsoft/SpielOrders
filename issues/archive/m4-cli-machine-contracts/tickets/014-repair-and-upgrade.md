---
title: Apply one repair and resume one Metadata Upgrade
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T00:00:00Z
closed_at: 2026-09-05T13:47:06Z
review_start: 688e9a4130f8000db1f86449305d0dbd659ad109
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

Bare `repair` and bare `order repair` diagnose the actionable cases in their scope and, for
each, name the allowed actions, the values required, and the exact command that would apply
it. `case=… action=…` applies exactly one offered action. `upgrade` exposes the supported
sequential Metadata Upgrade to the running build's newest version.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)

## Blocking

Blocked by [Ticket 013](013-validate-and-index.md), whose diagnostic vocabulary this reuses,
and [Ticket 008](008-vendor-mutations.md), whose mutation contract it follows.

Owner ruling (2026-09-03): a fifth repair family is allowed — recalculation of a damaged
technical `package_root` locator, which the core already offers at error severity. Both the
CLI and integrity decisions are amended; the recalculation reads no stored value and infers
no domain fact.

Owner ruling (2026-09-03): the fixtures use the closed placeholder set in
[Decision traceability and contract-test conventions](../../../../docs/development/traceability.md); this ticket adds none.

Withdrawn by the owner: everything about `upgrade` that requires a second metadata version.
The transformation registry is empty, so nothing is transformed, interrupted, or resumed.
Do not register a transformation, and do not remove the upgrade-pending failure code.

## Scope

- `validate` and `order validate` carry an offered repair for each diagnostic that has one,
  in this ticket's own action vocabulary, as
  [the CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
  requires. One vocabulary for what may be done about a finding, shared with `repair`; each
  offer identifies its diagnostic by the code, paths, and identities the envelope carries,
  never by position; the collection is present and possibly empty in every payload a
  validating command carries. An invalid scope carries no payload at all, by the envelope
  rule in that same decision, and `repair` is the command that answers there.

- Bare `repair` diagnoses only actionable Package-level cases; bare `order repair` only the
  selected Order's. Neither reaches into the other's scope.
- **A core addition: Package-scoped repair diagnosis and its application.** No core
  capability produces a Package-level case, yet a real one exists on disk — a leftover staging
  artifact beside a managed Package artifact, which the integrity decision makes actionable on
  explicit confirmation. Promotion validation per artifact kind is part of it; the adapter may
  not judge which candidate is safe.
- Each case reports a stable code, its evidence, the allowed actions, the values each
  requires, and an exact suggested command, naming the assignment the caller must fill in
  where the diagnosis cannot supply a value.
- `repair case=… action=… field=value...` and `order repair case=… action=…` apply exactly
  one offered action.
- **Case handles are ephemeral.** Not identities, not persisted; a handle from a stale
  diagnosis is refused rather than resolved against a Package that has changed.
- The five allowed repairs: a confirmed replacement for parseable metadata, promotion of a
  validated staging candidate, correction of an Unresolved Vendor Reference, adoption of a
  new Order identity for the chosen side of a duplicate-ID conflict, and locator
  recalculation.
- `upgrade`, with `--dry-run` reporting source, target, the deterministic artifact order,
  already-upgraded artifacts, and blockers.

## Acceptance criteria

- Bare `repair` on a Package whose only actionable case is inside an Order reports no
  Package-level case, and bare `order repair` on that Order reports it.
- A suggested command run verbatim applies its case for every action needing no supplied
  value, asserted by executing the string; for the actions that need one it is refused with
  the typed error naming the missing resolution rather than half-applied.
- Applying an action the diagnosis did not offer is rejected, and so is one whose required
  resolution is missing.
- A case handle from a diagnosis taken before the Package changed is refused as stale.
- Each of the five repairs is exercised end to end and leaves the Package valid in that
  scope, the response reporting the remaining diagnostics. The locator repair runs on an Order
  whose locator no longer reaches its Package, which afterwards loads ordinarily.
- A repair whose diagnosed destination changed between diagnosis and commit is refused, the
  other writer's bytes survive, and the refusal is distinguishable from a write failure.
- `upgrade --dry-run` reports source, target, artifact order, already-upgraded artifacts, and
  blockers, and changes nothing; a marker naming another version is reported as such; a
  damaged record appears among the blockers rather than being skipped; and the ordinary run
  leaves the Package and its marker as they were.
- Fixtures cover the cells each command can reach and no others, unreachable cells' reasons
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- **A broad `--fix`, a repair-all, or any inferred repair.** Each application names one case
  and one action.
- Partial salvage of malformed JSON, interpreting an unsupported version, and adopting
  missing Order metadata.
- An expert raw-JSON editing surface, which is M7's GUI-only exception.
- Arbitrary upgrade targets, per-file upgrades, downgrades, and package-wide rollback.
- Rebuilding the index as a repair, and repairing canonical metadata from a rebuild. Those
  are Ticket 013's commands and neither crosses over.
- Persisting a case handle or exposing one as an identity a script may store.

## Result

Bare `repair` and `order repair` diagnose their own scope and answer with every actionable
case: its ephemeral handle, the evidence behind it, the actions it allows, the values each
needs, and the exact command that applies it. `case=… action=…` applies one, and every
allowed repair is exercised end to end through the command its own diagnosis reported.
`validate` and `order validate` carry the same offers for the findings that have one.
`upgrade` plans, reports, and resumes the registered chain, which is empty. A bare
diagnosis is an answer rather than a verdict, which the CLI capability decision now rules.
The contract cells this ticket changed were re-recorded at version 1, in place.

Command tests are in `crates/spielorders-cli/tests/repair_and_upgrade.rs`, the recorded
cells in `crates/spielorders-cli/tests/response_contract.rs`, and the core additions in
`crates/spielorders-core/tests/package_repair.rs`.
