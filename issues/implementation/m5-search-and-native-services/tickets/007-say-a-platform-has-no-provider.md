---
title: Say plainly that a platform has no provider
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-07T13:20:00Z
closed_at: 2026-09-07T15:05:00Z
review_start: 5991ee65d5e859bffe39c252d868bbe29b624882
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

Where the platform offers no index, Supporting Material search says so and stops, rather
than inventing an answer.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

Blocked by [Ticket 005](005-ask-for-supporting-material.md), whose port reports it.

## Scope

Supporting Material search is best-effort and complementary. On a platform with no native
index, the answer names that state specifically — as do a disabled index, an unindexed or
unsupported volume, denied access, and a query the provider refused. Each is its own
explanation rather than one generic failure, because a caller can act on some and not
others.

SpielOrders never falls back to walking or indexing Supporting Material itself. A missing
native provider is a stated outcome and a release gate for that platform, not a prompt to
build a recursive scanner.

An empty answer from a working provider says the index found nothing, which is not the same
sentence as no provider being able to look.

## Acceptance criteria

- Each unavailable state reaches its own diagnostic, distinguishable by a consumer.
- No path walks or indexes Supporting Material when a provider is unavailable.
- An empty result from a working provider and an unavailable provider are different
  answers, and a test tells them apart.
- The response carries an exit status consistent with every other command's.

## Exclusions

- The macOS provider, which is Ticket 006's.
- Implementing a provider for any other platform.

## Result

A build with no native index provider for its platform answers `search supporting-material`
with the state that prevented one: exit `1`, `data: null`, `supporting_material_unsupported`,
no provider named, and the sentence saying nothing weaker stands behind it. Each unavailable
state carries its own code and its own sentence, asserted distinct. Nothing walks — the
fixture Package holds a file whose name and text carry the word being asked for, and no
answer reports it.

This command's tests are compiled for every build. Each claim asks the build which provider
it composed, and one test holds that answer against the platform the run is on, so no branch
rests on an unchecked fact. The provider and opener one platform has are
`platform_index_and_opener.rs`. The provider-absent branches pass as real runs with the
shipped provider forced away, and `x86_64-unknown-linux-gnu` lints clean.

Tests: `crates/spielorders-cli/tests/supporting_material_search.rs`.
