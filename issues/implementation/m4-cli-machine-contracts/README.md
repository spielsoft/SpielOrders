---
title: M4 — CLI and independently versioned machine contracts
status: complete, awaiting external validation
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m4
---

## Outcome

One human-first, Git-style `spielorders` executable exposes the shared Rust core so that
every ordinary Package-domain read and mutation the future GUI needs can be performed from a
terminal, without the adapter parsing, rewriting, or re-deciding managed metadata.

Alongside the commands, M4 delivers the machine contract the GUI, the agent skill, and every
future script read: independently versioned per-command JSON responses, stable diagnostic and
error codes, stable exit statuses, and golden fixtures for the response cells each command
can actually reach.

The milestone reopens no closed decision, changes no metadata version, and implements no M5
search, M6 or M7 desktop behavior, or M8 agent work.

## Governing decision sources

- [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Define legacy Order adoption and metadata bootstrapping](../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define the SpielOrders agent skill contract](../../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)

Individual tickets also link
[Define the SpielOrders package topology](../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md),
[Define the Order record and lifecycle contract](../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md),
[Define Vendor identity and historical semantics](../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md),
and [Define integrity, validation, and repair boundaries](../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
where they govern a slice. The governing architecture is
[Effect and adapter boundaries](../../../docs/architecture/boundaries.md) and
[Durable managed-file writes](../../../docs/architecture/durable-writes.md); the acceptance
gate is the M4 section of the
[SpielOrders implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md),
which carries the gate and leaves the deliverables to this index, as it does for every
milestone that has one.

## Ticket order and dependencies


| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Adopt sha2 0.11.0 and refresh the M4 dependency graph](tickets/001-dependency-graph-and-sha2.md) | Closed | None |
| 1b | [Refuse an uncanonicalizable selected root without panicking](tickets/017-uncanonicalizable-selected-root.md) | Closed | None |
| 2 | [Run the spielorders executable against a real Package](tickets/002-executable-effects-and-response-contract.md) | Closed | None |
| 3 | [Read a complete Package in one command](tickets/003-package-show.md) | Closed | [Run the spielorders executable against a real Package](tickets/002-executable-effects-and-response-contract.md) |
| 4 | [List a Package's Orders, Vendors, and reusable values](tickets/004-package-lists-and-values.md) | Closed | [Read a complete Package in one command](tickets/003-package-show.md) |
| 4b | [Display monetary values to two decimal places](tickets/018-displayed-monetary-values.md) | Closed | None |
| 5 | [Target and read one Order Directory](tickets/005-order-reads-and-targeting.md) | Closed | [Run the spielorders executable against a real Package](tickets/002-executable-effects-and-response-contract.md) |
| 6 | [Read one Vendor and its Contacts](tickets/006-vendor-reads.md) | Closed | [Run the spielorders executable against a real Package](tickets/002-executable-effects-and-response-contract.md) |
| 7 | [Initialize a Package from the command line](tickets/007-init.md) | Closed | [Run the spielorders executable against a real Package](tickets/002-executable-effects-and-response-contract.md) |
| 8 | [Create and edit a Vendor through canonical assignments](tickets/008-vendor-mutations.md) | Closed | [Read one Vendor and its Contacts](tickets/006-vendor-reads.md) |
| 9 | [Mutate Vendor Contacts by position](tickets/009-vendor-contact-mutations.md) | Closed | [Create and edit a Vendor through canonical assignments](tickets/008-vendor-mutations.md) |
| 10 | [Create and adopt Orders from the command line](tickets/010-add-and-adopt-orders.md) | Closed | [Target and read one Order Directory](tickets/005-order-reads-and-targeting.md), [Create and edit a Vendor through canonical assignments](tickets/008-vendor-mutations.md) |
| 11 | [Edit one Order's facts and Items](tickets/011-order-and-item-edits.md) | Closed | [Target and read one Order Directory](tickets/005-order-reads-and-targeting.md), [Create and edit a Vendor through canonical assignments](tickets/008-vendor-mutations.md) |
| 12 | [Align, remove, and trash one Order Directory](tickets/012-align-remove-and-trash.md) | Closed | [Create and adopt Orders from the command line](tickets/010-add-and-adopt-orders.md) |
| 13 | [Validate a Package and maintain its derived index](tickets/013-validate-and-index.md) | Closed | [Read a complete Package in one command](tickets/003-package-show.md) |
| 14 | [Apply one repair and resume one Metadata Upgrade](tickets/014-repair-and-upgrade.md) | Closed | [Validate a Package and maintain its derived index](tickets/013-validate-and-index.md), [Create and edit a Vendor through canonical assignments](tickets/008-vendor-mutations.md) |
| 15 | [Conform every command to one response contract](tickets/015-cross-command-contract-conformance.md) | Closed | Every command ticket, 2 through 14 |
| 15a | [Carry the Package's spelling on every Vendor mutation refusal](tickets/019-vendor-refusal-path-spelling.md) | Closed | None |
| 15b | [Report the derived index's real state in the Package bootstrap read](tickets/020-package-show-index-state.md) | Closed | None |
| 15c | [Hold the adapter's boundaries by visibility rather than by reading its source](tickets/021-adapter-boundaries-without-source-greps.md) | Closed | None |
| 15d | [Make a bare path unable to reach a refusal's reported paths](tickets/022-refusal-paths-by-type.md) | Closed | None |
| 15e | [Answer with the Root the command already established](tickets/023-roots-and-suggestions-a-command-already-knows.md) | Closed | None |
| 15f | [Deliver the decided locator repair, and answer with the roots a command established](tickets/024-deliver-the-decided-repair-and-the-established-root.md) | Closed | None |
| 15g | [Compose every repair suggestion in one place that knows whether repair would act](tickets/025-one-producer-for-the-repair-suggestion.md) | Closed | None |
| 16 | [Run the final M4 acceptance gate](tickets/016-integrate-m4-acceptance.md) | Closed | [Conform every command to one response contract](tickets/015-cross-command-contract-conformance.md), the tickets its runs returned, and [Make a bare path unable to reach a refusal's reported paths](tickets/022-refusal-paths-by-type.md) |

```text
001 sha2 and dependency graph
        |
        v
002 Executable, production effects, response contract   (package readme)
        |
        +--> 003 package show --> 004 package lists and values
        |            |
        |            +--> 013 validate and index --> 014 repair and upgrade
        |                                                    ^
        +--> 005 Order reads and targeting -------------\    |
        |                                                \   |
        +--> 006 Vendor reads --> 008 Vendor mutations ---+---+
        |                              |      |
        +--> 007 init                  |      +--> 009 Vendor Contacts
                                       |
                     010 add-order and adopt-order --> 012 align, remove, trash
                     011 Order and Item edits
                                       |
                                       v
                     015 Cross-command contract conformance
                                       |
                                       v
                     016 Integrated M4 acceptance gate
                                       ^
                                       |
017 Uncanonicalizable selected root ---+   (no blocking edge either way; closed)
018 Displayed monetary values ---------+   (no blocker; sequenced before 005)
019 Vendor refusal path spelling ------+   (returned by the gate's first run)
020 Package show index state ----------+   (returned by the gate's first run)
021 Adapter boundaries without greps --+   (returned by the gate's first run)
022 Refusal paths by type -------------+   (opened by 019's review)
023 Roots a command already established+   (returned by the gate's second run)
```

Every ticket lettered after its order number carries no blocking edge into the command
chain and is still in the chain to the gate, because the gate requires every M4 ticket
closed. What the gate measured is [ACCEPTANCE.md](ACCEPTANCE.md).

## What M4 excludes

- **The three `search` commands.** Owner ruling (2026-09-01): they are M5's, not M4's. M5
  follows M4, so a CLI surface here would expose an engine that does not exist until M5;
  M5's deliverables now name them.
- **`agent prepare`, `agent status`, and `init --agent-ready`**, which are M8's. M4's
  obligation runs the other way: M8's references cite actual CLI help and golden JSON
  contracts, so M4 leaves those stable and adds no agent-only duplicates.
- M5 native providers, Finder integration, and post-MVP structured filters.
- M6 and M7 desktop shell, tables, editors, and the raw-JSON surface. The Tauri crate stays
  a stub.
- Metadata version 1, its schemas, its bytes, and its fixtures under
  [`contracts/`](../../../contracts/README.md). M4 adds CLI response fixtures and never
  reuses an on-disk schema as a response schema.
- A shared SpielGantt crate, a CLI framework of this repository's own making, an import
  wizard, or a recursive Supporting Material indexer.

## Fixture and contract-root conventions

See [Decision traceability and contract-test conventions](../../../docs/development/traceability.md).

## Milestone acceptance

## Remediation after external validation

External validation passed the milestone and returned one finding.

| Ticket | Finding | Status |
| --- | --- | --- |
| [Decide a global option's exclusions where the caller cannot write around them](tickets/026-global-exclusions-in-every-position.md) | V-1 | Closed |

Only [Run the final M4 acceptance gate](tickets/016-integrate-m4-acceptance.md) may close M4,
and [ACCEPTANCE.md](ACCEPTANCE.md) is its record. No earlier ticket may close the milestone,
transition the roadmap, or declare M4 ready for external validation. After the gate passes
and its review resolves, a fresh worker externally validates the exact completed target
before any M5 work begins.
