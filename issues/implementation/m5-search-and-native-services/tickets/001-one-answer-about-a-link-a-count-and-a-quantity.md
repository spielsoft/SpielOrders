---
title: Answer one way about a link, and state a count and a quantity as they are
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T21:10:00Z
closed_at: 2026-09-06T23:58:00Z
review_start: 9e7bd82189ec86626d2cf4a0ea8f39e38c5ccd76
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

One command surface answers one way about a path that is a link, and every sentence it
prints about a count, a quantity, or a wrong-kind root says what is actually so.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)

## Blocking

Unblocked. Opened by the fourth run of the
[M4 acceptance gate](../../../archive/m4-cli-machine-contracts/tickets/016-integrate-m4-acceptance.md),
whose milestone-wide review returned every item below. A gate that repaired what it found
would have stopped measuring, so it carried them here and accepted M4.

## Scope

- **A link named as a root is refused, everywhere, under one code.** Links are never
  followed and never adopted. `init` and `adopt-order` already refuse one; every
  Package-level command and every `order` command resolves one and acts on what stands
  behind it, disposal of a whole Order Directory included, and those are the defect. The
  sentence calling a link in the Orders Directory ignored says what it is ignored *for*.
- **A count that names one directory more than exist.** The duplicate-Order-ID explanation
  adds one to a list that already holds every conflicting directory, so two directories are
  reported as three. The user picks the surviving Order from that sentence.
- **A quantity is not money.** An Order Item's Quantity is rendered by the one renderer
  that exists to give monetary values two decimal places, so a stored Quantity below a
  hundredth reads as zero and one with three decimals reads rounded. The machine document
  carries the stored value.
- **One wrong-kind root, two stable codes.** A file named as an `order` root is refused as
  an unreadable Package by most of the family and as an unmanaged Order root by validation
  and repair, and the first message names an internal port operation and a path that cannot
  exist.
- **A total of negative zero.** An Order with no Items reports its Order Total as `-0.0`,
  which is what an empty sum of that kind is and not what a total looks like.
- **`vendor=` takes a Vendor Name.** Vendor Names are the ordinary Vendor selectors,
  resolved trimmed and case-insensitively, unique within a Package, and resolution ignores
  visibility; machine responses keep the identities. An assignment reachable only by
  identity, in a surface that holds identities back to `--verbose`, is the defect. No
  identity-based assignment survives on the command line.

## Acceptance criteria

- A link named as a root is refused under one code by `init`, by the Package-level
  commands, and by every `order` command, and a test proves the same path gets the same
  answer from each.
- No whole Order Directory is disposed of, renamed, or edited through a link, and the
  sentence calling a link ignored says what it is ignored *for*.
- The duplicate-Order-ID explanation names exactly as many directories as carry that ID,
  proven for two and for three, and a test pins the sentence.
- A stored Quantity is displayed as the stored value; only monetary values pass through the
  monetary renderer, and the doc comment claiming as much becomes true.
- A wrong-kind `order` root reaches one stable code across the family, and no message names
  an internal port operation.
- An Order with no Items reports a total of zero, and the recorded cells that carry the
  present value are re-recorded at version 1.
- Every response contract is still version 1, and `metadata_version` is still 1.
- An Order's Vendor is assignable by Vendor Name, matching trimmed and case-insensitively
  and resolving a Hidden Vendor like any other, and no identity-based assignment remains.

## Exclusions

- M5 search itself — the three `search` commands, the native providers, and the OS-indexed
  Supporting Material port.
- Advancing any contract version. Version 1 is edited in place until the owner declares the
  CLI and the agent support tooling released.
- Re-litigating anything the M4 gate accepted.

## Result

A `--root` naming a link is refused under `root_is_a_link` by `init`, by every
Package-level command, and by every `order` command, before anything behind it is
opened; a file named as an `order` root reaches `order_root_unmanaged` across the
family, and no message names a port operation. The duplicate-Order-ID sentence counts
the directories that carry the ID, an Order Item's Quantity is displayed as the record
stores it, an Order with no Items totals zero, and a link in the Orders Directory is
reported as ignored for Order discovery. `vendor=` takes a Vendor Name — trimmed,
case-insensitive, Hidden Vendors included — for `add-order`, `adopt-order`,
`order edit`, and `repair`'s `assign_existing_vendor`; no identity-based assignment
remains. Six recorded cells are re-recorded in place at version 1 from real runs.

The whole-surface sweeps are in `crates/spielorders-cli/tests/cross_command_conformance.rs`;
the rest in `displayed_monetary_values.rs`, `validate_and_index.rs`,
`order_and_item_edits.rs`, and `order_creation_and_adoption.rs`.
