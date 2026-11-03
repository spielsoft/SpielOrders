---
title: Create and edit a Vendor through canonical assignments
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T14:02:11Z
closed_at: 2026-09-04T15:41:06Z
review_start: 69dbae42c3c621997698df543837879ffc3c1fca
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`add-vendor "Tarnwick"` creates a Vendor, `vendor edit` changes its facts, and
`vendor hide`/`vendor unhide` control whether it is offered for new use. Each accepts
`--dry-run`, reports what it did as a typed mutation report, and returns the refreshed
Vendor so no second read is needed. This ticket owns the mutation contract every later
mutating command follows.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)

## Blocking

Blocked by [Ticket 006](006-vendor-reads.md), whose Vendor resolution and refreshed-Vendor
payload this extends.

Owner ruling (2026-09-03): the golden fixture's placeholder set is extended rather than the
`success` cell abandoned, for a response carrying an identity minted in the child process.
This ticket applies `<VENDOR_ID>` and adds no other placeholder. See
[Decision traceability and contract-test conventions](../../../../docs/development/traceability.md).

## Scope

- **The canonical assignment language**, owned here: facts arrive as `field=value`, split on
  the first `=`; unknown and immutable fields are rejected; an empty value clears an optional
  scalar; options never carry domain facts.
- **`--dry-run` on every mutation**, owned here: full validation, the same report with
  `committed: false`, and no directory, staging file, index rewrite, or cleanup effect.
- **The typed mutation report**, owned here: committed, changed, the typed managed
  operations, old and new paths where relevant, canonical-versus-derived outcomes, index
  status, warnings, and the refreshed entity beside the report.
- **The outcome vocabulary**, owned here and distinguishable in both surfaces: committed,
  unchanged, rejected, and committed-with-warnings. A committed canonical mutation whose
  index refresh failed stays committed and must not look retry-shaped.
- `add-vendor "Tarnwick"` is the Name shorthand; the two forms cannot be combined, and there
  is no create or merge alias.
- `vendor edit NAME field=value...`: one order-independent atomic mutation over the whole
  aggregate. `vendor hide`/`unhide`: reversible, never refused on reference grounds.
- **The `UuidGenerator` production effect**, at the effect boundary Ticket 002 established.

## Acceptance criteria

- Both `add-vendor` forms produce the same Vendor; supplying both at once exits `2`.
- A quoted value containing `=` survives intact, and only the first `=` splits.
- An unknown field, an immutable field, and a blank required fact are each rejected with a
  typed error naming the field, at exit `1`, having written nothing.
- `field=` clears an optional scalar, absent rather than empty in the refreshed Vendor.
- A Name duplicating an existing Vendor after trimming and case-insensitive comparison is
  rejected, with no merge offered, suggested, or performed.
- `hide` on a referenced Vendor succeeds, those Orders still resolve its facts, `unhide`
  restores it, and the Vendor ID never changes. Hiding an already Hidden Vendor is
  `changed: false`, not an error.
- A committed mutation whose index refresh then fails exits `0`, reports the warning, and
  stays committed, driven by a real refresh failure.
- `--dry-run` over a valid mutation reports the real run's outcome with `committed: false`;
  over each rejection it produces that rejection instead. All leave the Package
  byte-identical, including creating no staging file.
- Fixtures cover the cells each command can reach and no others, unreachable cells' reasons
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- Vendor Contacts, which are Ticket 009's, though they share this aggregate.
- **Vendor deletion and Vendor Merge.** Both absent. No `vendor remove`, merge, alias
  registry, or duplicate-name auto-resolution.
- Placeholder Vendors. An Order without an established Vendor simply has no reference.
- Reusable Free-Entry Values. They are four Order facts; no Vendor mutation involves one.
- Order and Item mutation, which is Ticket 011's.
- A request-document input format. Mutation input stays the `field=value` language.

## Result

`add-vendor "Tarnwick"` creates a Vendor, `vendor edit` changes its shared facts, and
`vendor hide`/`unhide` govern whether it is offered for new use. Each takes `--dry-run`,
and each answers with a typed mutation report — committed, changed, the canonical outcome,
the managed operations, the index status, and warnings — beside the same complete Vendor
projection the reads render. The Name shorthand is expanded into the assignment it stands
for, so both `add-vendor` forms are one path. A committed mutation whose index refresh
fails stays committed at exit `0`.

The core gained the spellings of its Vendor mutation vocabularies, the outcome a plan
implies, the index a plan observed, and the Package-load projections a refused mutation
reports.

Pinned by `crates/spielorders-cli/tests/vendor_mutations.rs`, and by the four contracts'
golden fixtures compared in `crates/spielorders-cli/tests/response_contract.rs`.
