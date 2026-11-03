---
title: Validate a Package and maintain its derived index
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T21:05:00Z
closed_at: 2026-09-04T23:40:00Z
review_start: baeb2c488b8b132527c4b600c3f42e2b70fca1eb
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`validate` validates a complete Package and `order validate` one Order's scope, both
returning every diagnostic at once, grouped at the narrowest scope, with stable codes and
evidence. `index status` explains what is wrong with the derived Package Index and
`index rebuild` deterministically replaces it from healthy canonical records.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Ticket 003](003-package-show.md), whose diagnostic presentation and Package load
these extend.

Owner ruling (2026-09-03): exit status is decided by severity alone and never by whether a
repair is available — `1` when the result is invalid, `0` otherwise, including a valid
Package whose findings are all warnings. The CLI decision is amended to say so. Deciding the
status on severity is not permission to suppress a finding.

Withdrawn by the owner: reporting that a Package is upgrade-pending. The transformation
registry is empty and the newest metadata version is `1`, so no Package can be pending one.
The reachable half — a marker naming another version — is Ticket 003's shipped behavior.

## Scope

- `validate`: the complete Package, every Order Directory and every cross-record constraint.
  `order validate`: the selected Order's scope, targeted as every `order` command is. Both
  return every diagnostic at once rather than stopping at the first, grouped at the narrowest
  scope that owns it, each carrying severity, a stable code, an explanation, and the
  suggested next command.
- **The diagnostic vocabulary as it appears on the wire**, established here for the CLI, in
  a form Ticket 014's repair output and `package show` both reuse rather than restate.
- `index status`: whether the index is current, missing, malformed, wrong-version, or stale,
  with its typed reason. `index rebuild`: deterministic replacement from healthy canonical
  records, with `--dry-run`.
- **Three core additions**, reachability rather than new vocabulary since the states and
  reasons exist: an assessment that does not write and can report `stale`, a rebuild not
  fused to a load, and a plan form of it. `index status` must not write.
- A non-finite computed Item Total or Order Total makes an Order semantically invalid,
  carrying its own reason: overflow of finite stored numbers is the only route to one, so no
  stored value is at fault.

## Acceptance criteria

- A Package with several unrelated problems reports all of them in one run, each under the
  narrowest scope that owns it.
- `validate` on a healthy Package exits `0` with a present, empty diagnostics collection; on
  an error-severity finding it exits `1`, the diagnostics being the answer.
- A Package whose only finding is a warning exits `0` with that warning and any repair it
  offers reported in full, shown on a leftover staging artifact beside a valid destination.
- `order validate` reports only the selected Order's scope, not a sibling's damage, and
  reports a Package-level problem as Package scope.
- Every diagnostic carries a stable code, and the same condition produces the same code
  across `validate`, `order validate`, and `package show`.
- `index status` distinguishes all five states with their typed reasons, and a stale index
  is not reported as damage to canonical metadata.
- `index rebuild` is deterministic — the same healthy Package rebuilds to identical bytes —
  and Damaged Orders are absent from the result while staying visible diagnostically. A
  rebuild whose replacement fails preserves the existing index and reports the failure, and
  `--dry-run` leaves it byte-identical.
- Neither index command repairs canonical metadata or inspects Supporting Material.
- Fixtures cover the cells each command can reach and no others, unreachable cells' reasons
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- **Repair.** Diagnosing is this ticket's; applying anything is Ticket 014's. `index rebuild`
  is not a repair for canonical damage and must not be suggested as one.
- **Metadata Upgrade**, including registering a transformation.
- A `--fix`, `--repair`, or any option that acts on a finding. Validation reports.
- Inventing a diagnostic the core does not produce, or suppressing one it does.
- Changing the index projection. It is metadata version 1 and not this milestone's to change.

## Result

`validate` and `order validate` return every diagnostic at once, with the core's own
codes and evidence, each under the narrowest scope that owns it. Validity is `ok`, which
is the exit status, which is the severity predicate — so an invalid result is a failed
response whose diagnostics are the answer, and `data` carries what the run examined.
`index status` reports every index state with its typed reasons and does not touch the
file; `index rebuild` replaces it deterministically from healthy canonical records, with
`--dry-run`. A calculated total that is not finite is its Order's own semantic
invalidity, naming the total rather than a stored value.

The command tests are in `crates/spielorders-cli/tests/validate_and_index.rs`, the
recorded cells in `crates/spielorders-cli/tests/response_contract.rs`, and the core
additions in `crates/spielorders-core/tests/package_index_maintenance.rs`.
