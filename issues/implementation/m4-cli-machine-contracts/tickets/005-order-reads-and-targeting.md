---
title: Target and read one Order Directory
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-03T20:01:40Z
closed_at: 2026-09-04T12:09:55Z
review_start: f29f153aa6ebd20fd692734eb2ff68ce2cdd44fb
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`order show` reports one Order's current facts, numbered Items, calculated totals, paths,
and local diagnostics; `order item list` gives the Items alone and `order readme` the
conventional README path. Each finds its Package through the Order's own locator, never by
walking up the tree.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)

## Blocking

Blocked by [Ticket 002](002-executable-effects-and-response-contract.md). Sequenced after
[Ticket 018](018-displayed-monetary-values.md), which owns the money renderer these use.

## Scope

- `order`-family targeting: `--root` names the Order Directory, defaults to `.`, fails when
  that path is not one, and never searches ancestors.
- Provisional `--uuid <ORDER_ID>` for MVP evaluation, mutually exclusive with `--root`.
  M9 evaluates it, so removal must stay cheap.
- **A core addition: an Order selector keyed by Order ID.** Nothing in core resolves one.
- **A core addition: a projection of one Order's own diagnostics**, reached through its
  locator and able to name a broken locator in a typed failure. No public core read returns
  either, and the adapter may not assemble them.
- `order show`, `order item list`, and `order readme`, the last in the shape
  `package readme` established.
- `order item list` shares `order show`'s response contract and renders its Items for a
  human.
- Totals are calculated, never supplied or stored. Machine responses carry the UUIDs the
  human surface omits.

## Acceptance criteria

- `order show` with no options finds its Package through the locator, including under an
  Orders Directory configured several levels below the root.
- A missing, malformed, stale, or mismatched locator fails this Order's commands with a
  typed failure naming the locator, and does not fail unrelated Orders.
- A non-Order directory fails at exit `1`, and a Package root passed to an `order` command
  fails rather than being reinterpreted.
- `--uuid` resolves an Order; `--uuid` with `--root` is a usage error at exit `2`.
- Item numbering is one-based and matches stored order; totals appear in both surfaces.
- An Order with no Items returns a present, empty collection and a present total whose
  recorded bytes are `-0.0`, unadjusted.
- A Damaged Order answers with the decided failure status and typed error, no inline
  diagnostics.
- Fixtures cover the cells each command can reach and no others; every route to exit `2`
  carries the shared usage identity.

## Exclusions

- Every Order mutation, `order align`, and `order remove`.
- `order validate` and `order repair` as separate commands; this Order's own diagnostics
  inside these three reads are this ticket's.
- Repairing a bad locator, which is Ticket 014's.
- Reading, listing, or inspecting Supporting Material.
- Removing `--uuid`, whose evaluation is M9's.

## Result

`order show`, `order item list`, and `order readme` read one Order Directory, each
reaching its Package through the Order's own locator — arithmetic on that locator's
depth, never a search upward. `--root` names the Order Directory and `--uuid` resolves an
Order against the current-directory Package; together they are a usage error.

`order item list` shares `order show`'s response contract and emits its document byte for
byte, differing only in what it renders for a human, which the owner ruled on 2026-09-04
and [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
records. Two core additions carry what the adapter may not assemble: an Order selector
keyed by Order ID, and one Order's own diagnostics reached through its locator.

Pinned by `crates/spielorders-cli/tests/order_reads_and_targeting.rs` and
`response_contract.rs`, and by `crates/spielorders-core/tests/order_reads.rs`.
