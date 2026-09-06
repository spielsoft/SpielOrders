---
title: Answer one way about a link, and state a count and a quantity as they are
status: ready for agent
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
[M4 acceptance gate](../../m4-cli-machine-contracts/tickets/016-integrate-m4-acceptance.md),
whose milestone-wide review returned every item below. A gate that repaired what it found
would have stopped measuring, so it carried them here and accepted M4.

## Scope

- **One answer about a link named as a root.** `init` refuses a root that resolves through
  a link, `adopt-order` refuses a link candidate saying links are never followed, and
  validation reports a link standing in the Orders Directory as ignored — while every
  `order` command and every Package-level command resolves one and acts on what stands
  behind it, disposal of a whole Order Directory included. Settle which answer is right,
  make every command give that one, and say so where the decisions are read.
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
- **Ask the owner one question**: an Order's Vendor can be assigned only by the Vendor's
  identity, in a surface that reserves identities for `--verbose`, while Vendor Names are
  the ordinary Vendor selectors everywhere else. Do not change the assignment vocabulary
  before that ruling.

## Acceptance criteria

- A link named as a root reaches one outcome across `init`, the Package-level commands, and
  every `order` command, and a test proves the same path gets the same answer from each.
- No whole Order Directory is disposed of, renamed, or edited through a path the same
  Package reports as ignored, unless the settled answer is that it may be — in which case
  the sentence calling it ignored says what it is ignored *for*.
- The duplicate-Order-ID explanation names exactly as many directories as carry that ID,
  proven for two and for three, and a test pins the sentence.
- A stored Quantity is displayed as the stored value; only monetary values pass through the
  monetary renderer, and the doc comment claiming as much becomes true.
- A wrong-kind `order` root reaches one stable code across the family, and no message names
  an internal port operation.
- An Order with no Items reports a total of zero, and the recorded cells that carry the
  present value are re-recorded at version 1.
- Every response contract is still version 1, and `metadata_version` is still 1.
- The owner's answer on the Vendor selector is recorded in the decision it belongs to, and
  this ticket implements it or names the ticket that will.

## Exclusions

- M5 search itself — the three `search` commands, the native providers, and the OS-indexed
  Supporting Material port.
- Advancing any contract version. Version 1 is edited in place until the owner declares the
  CLI and the agent support tooling released.
- Re-litigating anything the M4 gate accepted.
