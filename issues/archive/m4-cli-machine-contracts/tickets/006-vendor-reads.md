---
title: Read one Vendor and its Contacts
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T12:16:50Z
closed_at: 2026-09-04T12:44:57Z
review_start: 3c203a4429ca9c2d47291b49efacf01a36a25fcd
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`vendor show NAME` reports a Vendor's current facts, numbered Contacts, and summaries of the
Orders referencing it. `vendor contact list NAME` gives the Contacts alone. A Hidden Vendor
answers both as any other does: hiding stops a Vendor being offered for new use and changes
nothing about resolving it.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)

## Blocking

Blocked by [Ticket 002](002-executable-effects-and-response-contract.md).

## Scope

- Vendor commands are Package-rooted, because Vendors have no directories. `NAME` is the
  selector, resolved by the established trimmed, case-insensitive, package-unique
  comparison; an unresolvable Name is a typed error, not an empty result.
- **A core addition: public Vendor resolution keyed by Name.** Both public Vendor reads take
  an ID and the Name predicate is private, so the adapter cannot spell the comparison and
  must not become a fourth spelling of it.
- **A core addition: a Vendor read carrying its own load's diagnostics**, rather than a
  second Package load, which would break the one-load-one-presenter rule Ticket 004 settled.
- `vendor show NAME`: the facts, ordered Contacts numbered from one, and the referencing
  Order summaries. `vendor contact list NAME`: those Contacts alone.
- A Hidden Vendor resolves and reports identically to a visible one, with visibility a
  reported fact. Neither command is refused on visibility or reference grounds.
- Machine responses retain the Vendor and Order IDs the human surface omits.

## Acceptance criteria

- A Name differing only in case or surrounding whitespace resolves to the same Vendor, and
  the response reports the stored canonical spelling.
- A Hidden Vendor returns the same facts, Contacts, and referencing Orders as a visible one,
  and an Order referencing it resolves it normally.
- Referencing summaries list exactly the Orders resolving here; a Vendor nothing references
  returns a present, empty collection.
- A Vendor with no Contacts returns a present, empty Contacts collection.
- An unresolvable Name fails at exit `1` with a typed error distinguishable from an empty
  result, and from a Package that has no Vendors.
- Fixtures cover the cells both commands can reach and no others; every route to exit `2`
  carries the shared usage identity.
- No Vendor resolution, name comparison, or reference count happens in adapter code.

## Exclusions

- Every Vendor and Contact mutation, including `hide` and `unhide`.
- **Vendor Merge.** Absent from the product. No merge, alias table, fuzzy-name suggestion,
  or "did you mean" that behaves as one.
- Vendor deletion. A Vendor is never deleted.
- Vendor listing and its visibility default, which are Ticket 004's.
- Selecting a Vendor by ID. Names are the selector; IDs are returned, not supplied.

## Result

`vendor show NAME` reports one Vendor's current shared facts, its Contacts numbered from
one, and summaries of the Orders that resolve here. `vendor contact list NAME` renders the
Contacts of that same response and shares its contract, emitting the document byte for
byte. Vendor commands are Package-rooted and select by Name; a Hidden Vendor answers both
as any other does, with visibility a reported fact.

Two core additions carry what the adapter may not assemble: a Vendor read keyed by Name,
and that read's own load's diagnostics — so a referencing list shortened by a Damaged
Order arrives beside the observation that explains it. The Name comparison and the Vendor
Reference filter now have one home each in the core.

Pinned by `crates/spielorders-cli/tests/vendor_reads.rs` and `response_contract.rs`, and
by `crates/spielorders-core/tests/vendor_reads.rs`.
