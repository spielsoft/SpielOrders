---
title: Edit one Order's facts and Items
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T17:42:54Z
closed_at: 2026-09-04T18:52:00Z
review_start: e7a751f5cdc2b953690644b11eba33f1a21f15f8
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`order edit field=value...` changes an Order's facts and `order item add|edit|move|remove`
maintain its ordered Items. Each is one order-independent atomic mutation over the complete
Order record, each accepts `--dry-run`, and each returns the refreshed Order projection with
recalculated totals.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)

## Blocking

Blocked by [Ticket 005](005-order-reads-and-targeting.md), whose targeting and Order
projection these use, and [Ticket 008](008-vendor-mutations.md), whose mutation contract
they follow.

## Scope

- `order edit field=value...` over the Order named by `--root` or `--uuid`. Mutable facts are
  Title, Date Ordered, Status, Vendor Reference, Orderer, Ordering Institution, Currency,
  Project, and Order Group. The Order ID and the technical locator are immutable.
- **The automatic Date Ordered rule.** Entering `ordered` without a date applies today's
  local date; an explicit date in the same mutation overrides it, so a historical correction
  takes one edit rather than two.
- Status accepts exactly the five established values, with unrestricted transitions.
- The four Item commands, positions one-based and evaluated against the pre-mutation ordered
  collection, `add` appending unless `position=N` is given.
- An Item requires Description, Quantity, and Unit Price; Catalog Number is optional. Totals
  are calculated, never supplied or stored.
- Report reuse versus introduction for each Reusable Free-Entry Value assigned.
- **A core addition: a typed rejection naming the offending field** for a status, date, or
  currency the core will not accept. The core's rejection is a unit variant today, and the
  adapter decides no Package rule, so it cannot fill the gap.
- `adopt-order` accepts Order Items through the same Item-assignment vocabulary this ticket
  defines, as
  [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
  requires; it is the only mutation that may take Items at creation, since `add-order`
  defaults to none.

## Acceptance criteria

- Every mutable fact can be set, changed, and where optional cleared, the cleared fact
  absent from the refreshed Order. The Order ID and locator are rejected as immutable,
  naming the field.
- `status=ordered` with no date sets today's; the same edit carrying a date stores that one;
  an edit to `ordered` on an Order already `ordered` leaves its date alone; and the response
  distinguishes an assigned date from an applied one.
- A Vendor Reference can be set to an existing Vendor and cleared, and one resolving to
  nothing is rejected.
- A status outside the five, a malformed date, a non-three-letter currency, and a blank
  Title are each rejected with a typed error naming the field, having written nothing.
- Item positions behave as Contacts' do, and position `0`, a negative, a non-integer, and a
  position past the end are each rejected.
- A non-finite or unrepresentable Quantity or Unit Price is rejected, and the refreshed
  totals recalculate from the mutated Items.
- Removing the last Item leaves a present, empty Items collection.
- Several assignments in one command are one atomic mutation: an invalid fifth assignment
  writes nothing at all.
- A committed edit whose index refresh then fails exits `0`, reports the warning, stays
  committed, and one ordinary load shows the change landed once.
- `--dry-run` over each command leaves the Order byte-identical, Supporting Material
  included, and over each rejection produces that rejection rather than a report.
- An adoption with Items writes them in the given order with calculated totals, and an
  adoption with none writes a present empty collection.
- Fixtures cover the cells each command can reach and no others; every route to exit `2`
  carries the shared usage identity.

## Exclusions

- Order Directory renaming and disposal, which are Ticket 012's. An edit that changes Title
  or Date Ordered does not rename the directory; drift is silent until alignment is asked
  for.
- A README write command in any form. The decided command tree has none: `package readme`
  and `order readme` report paths so a human can compose an ordinary editor.
- Giving Items identities, and any batch or multi-Item form of the four Item commands.
  Each of them addresses one Item; the Items an adoption carries are Scope above.
- Vendor creation from inside an Order edit.
- `order validate` and `order repair`, which are Tickets 013 and 014.

## Result

`order edit field=value...` and `order item add|edit|move|remove` ship, each under its own
response contract identity, each answering with `order show`'s projection of the Order it
left beside a typed report. The core names which Order fact a refused value belongs to, and
publishes the Item-assignment vocabulary the four Item commands already shared privately —
so `adopt-order` takes Items, which
[Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
has always said it does.

Behavior is pinned in `crates/spielorders-cli/tests/order_and_item_edits.rs`, the recorded
cells in `crates/spielorders-cli/tests/response_contract.rs`, and the core rules in
`crates/spielorders-core/tests/order_fact_edits.rs` and
`crates/spielorders-core/tests/order_item_collection.rs`.

A malformed Date Ordered is still refused as an invalid assembled record by `add-order` and
`adopt-order`, and by name only here;
[Conform every command to one response contract](015-cross-command-contract-conformance.md)
owns that.
