---
title: Compose every repair suggestion in one place that knows whether repair would act
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T02:13:24Z
closed_at: 2026-09-06T02:39:04Z
review_start: f6384c5d261d63ebbf461d4776881b2a0a13cf41
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A response never tells a user to run a command that would answer with nothing to do,
because only one function says so and it asks first.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Diagnose and repair Package integrity](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [The fixture convention](../../../../docs/development/traceability.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md).

## Scope

Two sites still say "`spielorders repair` offers the actions" unconditionally for a damaged
Package: the Package-load failure and the Order-diagnosis arm that reports a Package it
cannot judge an Order against. A Package whose Vendor aggregate is syntactically malformed
answers `package show` with that sentence while `repair` on it answers no cases, and the
sentence travels in `error.message` under `--json`, so it is a false statement in a machine
response rather than awkward wording.

**This is the fourth time this defect has been found, which is the thing to fix.** It was
found at `validate`, then at `validate` and `upgrade`, and now here, because the sentence is
composed at several sites and each finding named the site in front of it. Repairing these
two produces the fourth and fifth instance of a fix, not the end of the defect.

**So the deliverable is one producer.** Every sentence naming a repair command — human
surface and `error.message` alike — comes from a single function that asks
`offered_repair_commands` what that scope authorizes and says something true when the answer
is empty. The two sites above route through it, and so do the `validate` and `upgrade` sites
Ticket 024 already fixed, so that a fifth producer cannot be written by hand.

Hold that by construction rather than by inspection: after this ticket, a site composing
such a sentence outside the one function should be a compile error or an unavailable
spelling, not something a reader has to notice. **If a text-composition site survives that
does not go through it, the ticket is not done.** Do not reach for a test that greps the
source; that mechanism was removed from this repository and visibility, types and clippy
are what replaced it.

About twenty recorded cells carry this sentence. Only the message bytes move, and no
contract advances — re-record them in place at version 1, from real runs rather than by
editing bytes.

## Acceptance criteria

- One function produces every sentence that names a repair command, and it consults what the
  scope actually authorizes before naming one.
- Where a scope authorizes no action, the sentence says what the user can do instead and
  names no command that would answer with nothing.
- A Package whose Vendor aggregate is malformed, and an Order that cannot be judged against
  its damaged Package, each say something a subsequent command confirms — asserted through
  the executable, by running what the response suggests.
- Composing such a sentence anywhere else is prevented by construction, not by review.
- The `validate` and `upgrade` sentences behave as before and now come from the same place.
- No `contract_version` advances; every changed cell is re-recorded in place at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Which repairs exist, what they do, or when a diagnostic authorizes one.
- Any wording change to a sentence that names no command.
- The unreachable arms earlier runs examined and found sound.

## Result

`commands::repair_suggestion` is the only place this adapter names a repair command.
`RepairSuggestion`'s field is private, so no other module can build one, and the
constructor that promises a case asks `repair_actions` what the scope authorizes first;
the command names are spelled there and nowhere else in shipped source. Every site routes
through it — the two this ticket named, `validate`'s two, and the refusals that named
`spielorders repair` for a stage standing inside an Order's own reserved directory. One
refusal now names no command, because nothing diagnoses the file it is about.

The cells carrying the false suggestion were re-recorded in place at version 1, from real
runs, and no contract advanced. The claim is asserted through the executable in
`tests/repair_and_upgrade.rs`, by running the scope each message names or declines to
name.

The `--help` reference in `cli` names the sibling command and stays there: it is composed
before any Package is resolved.
