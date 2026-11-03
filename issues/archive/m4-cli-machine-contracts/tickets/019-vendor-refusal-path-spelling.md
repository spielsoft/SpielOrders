---
title: Carry the Package's spelling on every Vendor mutation refusal
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T18:29:31Z
closed_at: 2026-09-05T19:00:07Z
review_start: 46532803054e03d5fe47c04cdd5c5316bc569b2f
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

Every Vendor and Contact mutation refused at its commit reports the Vendor record's path in
this Package's own spelling for it, as every other refusal already does, and the check that
would have caught it is made over the responses rather than over the recorded cells.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md), which found this
and returned it rather than repairing a capability inside a gate.

## Scope

The four rejections a Vendor mutation can be refused with at its commit each name the Vendor
record and reach for a path rather than for the location the plan already established, so a
`--json` consumer reads `null` where a Package stands right around the path. The traceability
convention says `null` is for a path with no Package to be relative to; here there is one.
Every Vendor and Contact mutation is refused through that one function, and the Order-level
and repair-level equivalents already carry the location — this is the one family left behind
when the same defect was swept out of the `order` family.

Reproduce it by occupying the Vendor record's staging name with a directory, which is the
shape an interrupted write leaves, and asking for any Vendor or Contact mutation.

The reason no check caught it is the second half of the work: the claim is made over the
recorded golden cells, and each contract records one refusal, so every refusal condition a
contract does not record is unchecked. Make the claim over the responses commands actually
produce, so the next refusal that drops a spelling fails a test rather than a gate.

`crates/spielorders-cli/tests/m4_acceptance.rs` carries a comment naming this ticket where
the assertion belongs; remove the comment and make the assertion.

## Acceptance criteria

- Every Vendor and Contact mutation refused at its commit reports the Vendor record with
  this Package's spelling for it, in every rejection that names it.
- The recorded cells for the affected contracts are re-recorded at contract version 1 where
  they change. No `contract_version` advances: nothing has run outside a test tree.
- A refusal condition no golden cell records is covered by the conformance claim, so a path
  reported without its spelling fails a test rather than reaching a gate.
- `crates/spielorders-cli/tests/m4_acceptance.rs` asserts the spelling and no longer names
  this ticket.
- The `init` refusals stay as they are: their subject is a directory that must not be a
  Package, and the conformance suite already says so.

## Exclusions

- The adoption refusals raised before a candidate has been shown to be a direct child whose
  name this Package can express. Those are paths the command never resolved, which the
  traceability convention names as the other case `null` is for.
- Any change to what a refusal's code, scope, or message says.
- Any new response contract, and any version advance.

## Result

Every Vendor and Contact mutation refused at its commit names the Vendor record in this
Package's own spelling for it. The commit rejection hands back the core's own `Location`,
which is where the spelling is decided, and the adapter reports the record through the
Package it resolved rather than reaching for a path.

The check that would have caught it is closed too. A contract records one refusal, so
every other refusal condition was checked by nothing; the same claim is now made over the
response a real process writes when a directory stands at a managed record's staging name,
contract by contract, with the reason written down wherever that condition reaches no
commit.

No recorded cell records a commit refusal, so no fixture bytes moved and no contract
advanced past version 1.

Tests: `crates/spielorders-cli/tests/cross_command_conformance.rs`,
`crates/spielorders-cli/tests/m4_acceptance.rs`, and the Vendor suites in
`crates/spielorders-core/tests/`.
