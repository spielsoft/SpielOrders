---
title: Mutate Vendor Contacts by position
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T14:35:11Z
closed_at: 2026-09-04T20:18:44Z
review_start: d106d6877fdf19c62366aa2da3bdc69955427c73
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`vendor contact add|edit|move|remove` maintain a Vendor's ordered Contacts using one-based
positions, with the complete owning Vendor rewritten atomically each time and returned
refreshed.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)

## Blocking

Blocked by [Ticket 008](008-vendor-mutations.md), whose assignment grammar, `--dry-run`
behavior, and mutation report this follows.

## Scope

- `vendor contact add NAME field=value...`, appending unless `position=N` is supplied;
  `edit NAME POSITION field=value...`; `move NAME POSITION position=POSITION`;
  `remove NAME POSITION`.
- **Position semantics**, shared with Order Items: Contacts have no IDs, positions are
  one-based, and `edit`, `move`, and `remove` evaluate them against the pre-mutation ordered
  collection.
- A Contact requires a Name; role, phone, and email are optional and clearable.
- Each command rewrites the complete validated owning Vendor atomically and returns the
  refreshed Vendor with its referencing Order summaries.

## Acceptance criteria

- `add` without `position=` appends; `add position=1` inserts first and shifts the rest.
- `edit`, `move`, and `remove` at position N act on the entry that was at N before the
  mutation, proven where the pre- and post-mutation collections differ at that index.
- Position `0`, a negative position, a non-integer, and a position past the end are each
  rejected with a typed error naming the position, at exit `1`, having written nothing.
- `move` to the position a Contact already occupies is `changed: false`, not an error.
- A blank Contact Name is rejected; an empty value clears an optional fact, which is then
  absent from the refreshed Vendor.
- Removing a Vendor's sole Contact leaves a present, empty Contacts collection.
- `--dry-run` over each of the four leaves the Package byte-identical and reports the same
  outcome with `committed: false`.
- A Hidden Vendor's Contacts are mutable exactly as a visible Vendor's are.
- Fixtures cover the cells each command can reach and no others, unreachable cells' reasons
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- Vendor fact edits, creation, hiding, and unhiding, which are Ticket 008's.
- Order Item mutation, which is Ticket 011's, though it shares these position semantics.
- Giving Contacts identities. They are embedded, ordered, and have no independent IDs.
- Any batch or multi-Contact form. Each command is one mutation.

## Result

`vendor contact add|edit|move|remove` maintain one Vendor's ordered Contacts. Positions are
one-based and address the collection as it stood before the mutation; an `add` may also name
the place after the last Contact, the one number the others refuse. Every way of naming no
Contact is one refused mutation at exit `1` that writes nothing, with the position and the
end it was for carried as an identity. A `move` to the position a Contact already occupies
is committed and unchanged.

Each rewrites the complete owning Vendor atomically and answers with a typed report — the
Contact, both positions, and the shared mutation members — beside `vendor show`'s own
projection. A Hidden Vendor's collection is maintained exactly as a visible Vendor's is,
and a Contact assigned no Name is refused as the Contact's own missing Name.

Pinned by `crates/spielorders-cli/tests/vendor_contact_mutations.rs`, and by the four
contracts' golden fixtures compared in `crates/spielorders-cli/tests/response_contract.rs`.
