---
title: Run the M5 acceptance gate
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-08T18:55:00Z
closed_at: 2026-09-08T20:12:00Z
review_start: 3cd09717e6a43e174e3916ab9476deadcd63a960
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

One fresh run establishes whether M5's search surface is ready for external validation, and
its record is the milestone's acceptance record.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

Blocked by [Ticket 004](004-ask-a-package-what-matches.md),
[Ticket 007](007-say-a-platform-has-no-provider.md),
[Ticket 008](008-prove-the-real-index-by-hand.md), and
[Ticket 010](010-report-an-index-that-cannot-answer.md).

## Scope

Record the clean candidate and the toolchains, run the gate at it, and compose the three
search commands into one workflow no single ticket owns: a Package with Orders, Vendors and
Supporting Material, searched by entity and by file, over a non-default Orders layout.

Exercise the surface the tickets each saw only their own part of — a term matching through a
Vendor into its Orders, a Damaged Order absent from both engines, deduplication by path, a
timeout and a cancellation, and an unavailable provider.

**The record names the owner's smoke run** of Ticket 008's script: which script, and that it
was run and what it showed. The gate does not run it and does not stand in for it.

Check dependency and toolchain currency, run one milestone-wide two-axis review, and write
only current evidence to the acceptance record, at most 600 words, rewritten in place in one
commit. A capability defect is returned to the ticket that owns it, never repaired here.
Transition the roadmap's M5 section on a passing gate and never earlier.

## Acceptance criteria

- Every M5 ticket is closed from reviewed work, with no blocking review finding unresolved.
- Every command M5 delivers is exercised end to end at the candidate.
- Entity search and Supporting Material search never mix results.
- A Damaged Order appears in neither engine.
- The acceptance record names the smoke run, the candidate, and either the closing SHA or
  each returned blocker.
- The currency inventory is complete and every blocked in-major update is named.
- `metadata_version`, the checked-in schemas, the fixtures and the scenarios are unchanged,
  and every contract is still version 1.

## Exclusions

- Capability fixes, which are returned to the ticket that owns them.
- Running the smoke script, which is the owner's.
- Declaring M5 externally validated; a fresh worker does that afterwards.

## Result

The gate passed at a clean candidate and **accepted M5**, which is complete and awaiting
external validation. Every criterion's verdict, the candidate, the smoke run the owner
performed by hand, the currency inventory, and the boundaries no tool holds are
[ACCEPTANCE.md](../ACCEPTANCE.md), written whole in one commit.

The composition no single ticket owns is `crates/spielorders-cli/tests/m5_acceptance.rs`:
both engines over one Package holding Orders, Vendors, Supporting Material and a Damaged
Order, over a non-default Orders layout, through real processes.

The milestone-wide two-axis review returned one blocking finding — an adapter comment
placing a scope narrowing in the wrong crate — fixed here because it is a claim rather than
a capability. Nothing returned reached the bar that stops a close, and eight findings
outside M5's scope went whole to
[M6 Ticket 001](../../m6-desktop-shell-and-read-experience/tickets/001-say-only-what-the-code-does.md).
