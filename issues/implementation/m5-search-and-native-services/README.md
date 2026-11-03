---
title: M5 — Search and native platform services
status: externally validated
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m5
---

## Outcome

Deterministic native Order and Vendor lookup, and OS-indexed Supporting
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
| 1 | [Answer one way about a link, and state a count and a quantity as they are](tickets/001-one-answer-about-a-link-a-count-and-a-quantity.md) | Closed | None |
| 2 | [Put the command path first and its modifiers after it](tickets/002-command-path-first-modifiers-after.md) | Closed | None |
| 3 | [Match an Order or a Vendor by its own facts](tickets/003-match-an-entity-by-its-own-facts.md) | Closed | None |
| 4 | [Ask a Package which Orders and Vendors match](tickets/004-ask-a-package-what-matches.md) | Closed | Ticket 3 |
| 5 | [Ask for Supporting Material, and answer when nothing can](tickets/005-ask-for-supporting-material.md) | Closed | None |
| 6 | [Find Supporting Material with the platform's own index](tickets/006-find-material-with-the-platform-index.md) | Closed | Ticket 5 |
| 7 | [Say plainly that a platform has no provider](tickets/007-say-a-platform-has-no-provider.md) | Closed | Ticket 5 |
| 8 | [Prove the real index by hand, once](tickets/008-prove-the-real-index-by-hand.md) | Closed | Ticket 6 |
| 9 | [Report an index that cannot answer rather than an empty result](tickets/010-report-an-index-that-cannot-answer.md) | Closed | Ticket 7 |
| 10 | [Run the M5 acceptance gate](tickets/009-run-the-m5-acceptance-gate.md) | Closed | Tickets 4, 7, 8, 9 |
| 11 | [Restore search stopping, Unicode matching, file paths, and response evidence](tickets/011-repair-search-validation-findings.md) | Closed | None |
| 12 | [Keep queued search output, final exits, and real fixture comparisons](tickets/012-complete-stopped-search-and-fixture-checks.md) | Closed | None |

Tickets 3 and 5 open two independent chains — entity search and the Supporting Material
port — which meet only at the gate. Ticket 8 is a script the owner runs by hand; the gate
names that run and does not stand in for it. Ticket 010 is numbered after the gate and
ordered before it.

The internal acceptance gate is recorded in [ACCEPTANCE.md](ACCEPTANCE.md). The independent
verdict is [M5 external validation](VALIDATION.md), which passes; that is the prerequisite
for M6 implementation.
