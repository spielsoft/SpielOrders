---
title: Create and adopt Orders from the command line
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T15:40:31Z
closed_at: 2026-09-04T16:26:48Z
review_start: b055b273135d8c3aa82ebf499f64d58c657acb50
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`add-order` creates a new Order Directory with current metadata. `adopt-order DIRECTORY`
brings an existing eligible direct child of the Orders Directory under management without
touching anything already inside it. Both are Package-rooted, both accept `--dry-run`, and
both leave every existing file byte-for-byte as it was.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)

## Blocking

Blocked by [Ticket 005](005-order-reads-and-targeting.md), whose Order projection these
return, and [Ticket 008](008-vendor-mutations.md), whose mutation contract they follow.

Owner rulings (2026-09-03), all three settled and needing nothing further:

- The golden fixtures use `<ORDER_ID>` and `<TODAY>` from the closed placeholder set in
  [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md); this ticket adds no other.
- The adoption decision's "basename collision in selection" named the unusable-name
  refusal: a candidate whose filesystem name has no normalized Package-relative spelling.
  Adoption adopts in place, so collision refusal stays with alignment.
- `add-order` accepts `status=`, so every editable Order fact is assignable and creation
  agrees with adoption. `in_preparation` remains the default.

## Scope

- `add-order [TITLE | field=value...]`, the title-only form being shorthand and the two
  uncombinable. Every assignable fact may be given, `status` included; the rest take their
  established defaults.
- **A core addition: a status the caller may set at creation**, keeping `in_preparation` as
  the default. Creation hardcodes it today while adoption already requires one.
- **The `LocalCalendar` and `UuidGenerator` production effects**, at the effect boundary
  Ticket 002 established.
- `adopt-order DIRECTORY field=value...`, selecting one eligible direct child of the
  configured Orders Directory and requiring explicit title, date, and status.
- Both write only the current-version Order record with its core-calculated locator,
  preserve an existing Order `README.md` byte-for-byte, create an empty conventional one only
  when absent, touch nothing else, and refresh the derived index.
- Report reuse versus introduction for each Reusable Free-Entry Value assigned, naming the
  canonical spelling a reused value matched.
- Report adoption's refusals distinguishably: an ineligible directory, a link, an unusable
  name, an already managed Order, and conflicting or partial reserved state.

## Acceptance criteria

- Both `add-order` forms produce the same Order; combining them exits `2`.
- An Order created with no other assignments carries today's date, `in_preparation`, no
  Vendor Reference, and a present empty Items collection; with `status=ordered` it is created
  at `ordered`. One test covers both.
- `adopt-order` missing title, date, or status is rejected naming the missing fact, having
  written nothing.
- Adoption never parses the basename nor inspects the contents: a candidate whose name
  embeds an impossible calendar date is adopted with the supplied date, uncorrected.
- A candidate holding Supporting Material and a user-authored `README.md` is adopted with
  both byte-identical and nothing else created, renamed, or removed.
- Each refusal above leaves the candidate untouched and is distinguishable from the others.
- A generated Order ID is checked before commit and a collision refused, not overwritten.
- The written locator resolves from the new Order Directory to this Package's root, including
  under a configured nested `orders_path`.
- A committed creation or adoption whose index refresh then fails exits `0`, reports the
  warning, stays committed, and one ordinary load afterwards shows the Order once.
- `--dry-run` over each command and each refusal leaves the Package and the candidate
  byte-identical, reporting `committed: false` or that refusal respectively.
- Fixtures cover the cells both commands can reach and no others; every route to exit `2`
  carries the shared usage identity.

## Exclusions

- **Any legacy filename-to-domain parser.** Directory components are evidence for a human or
  an agent, never a field grammar, not even as a suggested default.
- **A batch importer, wizard, proposal, review, or resume command.** Package-wide adoption is
  this primitive invoked repeatedly, with no transaction and no package-wide rollback.
- Creating a Vendor inside `adopt-order`, an alias registry, and automatic duplicate-name
  matching.
- Order Directory renaming. A candidate is adopted in place; `order align` is Ticket 012's.
- Inspecting, inventorying, parsing, normalizing, or hashing Supporting Material for any
  purpose, including to decide eligibility.

## Result

`add-order "Laser components"` creates an Order Directory carrying today's local Date
Ordered, `in_preparation`, no Vendor Reference, and a present empty Items collection
unless the command line says otherwise — `status` among what it may say. `adopt-order
DIRECTORY` brings one eligible child of the Orders Directory under management where it
stands, reading nothing off its name or its contents, and each of its refusals is its own
answer. Both take `--dry-run`, and both answer with a typed mutation report beside
`order show`'s own complete Order projection.

The core gained a status at creation, the canonical assignment language for an Order with
no record yet, the Order and the Vendor a plan would leave, and its two vocabularies'
spellings. The executable gained the local-calendar effect.

Pinned by `crates/spielorders-cli/tests/order_creation_and_adoption.rs`,
`platform_effects.rs`, and both contracts' golden fixtures in `response_contract.rs`.
