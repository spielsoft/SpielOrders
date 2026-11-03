---
title: Report an index that cannot answer rather than an empty result
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-08T18:10:00Z
closed_at: 2026-09-08T18:50:08Z
review_start: b192ebc178a8c96e1269da5b3df16f6634f1c2ea
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

On a platform that ships an index, a query made while that index cannot answer says so and
exits unavailable, instead of reporting a search that completed and matched nothing.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

Blocks [Ticket 009](009-run-the-m5-acceptance-gate.md).

## Scope

The decision promises a specific explanation for disabled indexing, for an unindexed or
unsupported volume, and for denied access. Every one of those explanations exists and every
one is held to the response the port returns by a prepared provider — and the provider this
build ships constructs none of them. The states the decision names are therefore reachable
in a fake and unreachable in the product, and an index that is switched off is reported as a
search that completed and found no match: an answer that is false about the world rather
than incomplete about it.

Ask the platform what state its index is in before asking it for results, about the volume
holding the Orders Directory being searched. An index reported as switched off is the
disabled-indexing answer. A volume reported as unindexed and a volume the platform declines
to report on at all are both the unindexed-volume answer, because the difference between
them is invisible to a caller and inventing one would claim more than was learned. Each
settles as an unavailable answer carrying its own explanation, through the response shape
and the exit status an unavailable provider already has.

An Orders Directory this process cannot read is the denied-access answer, and the filesystem
is what establishes it. The index is not asked whether a directory is readable.

A directory the user has excluded from indexing privately cannot be told from one holding no
match, through any interface the platform offers. That is the case the decision's own hedge
already covers: an empty answer says the index reported no matches, not that no such file
exists. Record at that site what the platform was asked and that this is its limit, so the
next reader does not reopen a closed question.

## Acceptance criteria

- A query whose volume has indexing switched off answers unavailable with the
  disabled-indexing explanation, and never as a complete result.
- A query on a volume that is not indexed, and one on a volume the platform will not report
  on, both answer unavailable with the unindexed-volume explanation.
- An Orders Directory the process cannot read answers unavailable with the denied-access
  explanation, established without asking the index.
- Each of those answers carries the exit status an unavailable provider already carries, and
  the response shape, its fixtures and its schemas are unchanged.
- Every state above is proven deterministically at the effects boundary, on any platform,
  without switching the owner's own index off.
- A reader at the privacy-exclusion site finds what the platform was asked and why the empty
  answer stands.

## Exclusions

- Any change to the response shape, its fixtures, its schemas, or any contract version.
- A step added to the milestone's by-hand script; switching a person's index off is not
  something a script does to their laptop.
- A fallback that walks or indexes Supporting Material itself, which the decision forbids.

## Result

The provider settles what prevented an answer before asking for one: `mdutil -s` over the
volume holding the Orders Directory, and a readability check on that directory. A
switched-off index, an unindexed volume, and a volume the platform will not report on each
answer unavailable carrying their own explanation, and both tools share the one bound the
request carries, so a cancellation during the state ask answers partial rather than claiming
anything about the volume. A privately excluded directory still arrives as an empty answer,
and the site says what the platform was asked.

Tests are in `crates/spielorders-cli/tests/platform_index_and_opener.rs` and beside the code
in `effects.rs`.

No command line reaches the denied-access answer: the Package load lists the Orders Directory
first and reports it unreadable, as every other Package read does. The provider's answer
serves a caller searching a Package it loaded earlier.
