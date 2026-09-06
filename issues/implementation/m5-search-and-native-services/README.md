---
title: M5 — Search and native platform services
status: not started
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m5
---

## Outcome

Deterministic native Order and Vendor lookup, and the asynchronous OS-indexed Supporting
Material search port with its macOS provider, exposed through the three `search` commands
under the response contract M4 established.

## Governing decision sources

- [Define lookup and search behavior](../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../docs/architecture/boundaries.md)

The deliverables and the acceptance gate are the M5 section of the
[SpielOrders implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md).

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Answer one way about a link, and state a count and a quantity as they are](tickets/001-one-answer-about-a-link-a-count-and-a-quantity.md) | Ready for agent | None |

The milestone is otherwise undecomposed. Ticket 001 is here rather than in M4 because the
[M4 acceptance gate](../m4-cli-machine-contracts/tickets/016-integrate-m4-acceptance.md)
carried its review's findings forward instead of repairing what it was measuring.
