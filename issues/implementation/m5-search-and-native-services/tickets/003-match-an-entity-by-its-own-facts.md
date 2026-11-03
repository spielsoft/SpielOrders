---
title: Match an Order or a Vendor by its own facts
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T23:31:00Z
closed_at: 2026-09-07T00:20:00Z
review_start: eed278fed0220b19da9c34c0a37f8ab2c8fbca5d
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

The core answers which Orders and Vendors of a loaded Package match a query, by the same
rule every time.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)

## Blocking

None.

## Scope

A query is whitespace-separated terms combined with AND, and a term may match a different
fact from its neighbour. Each term matches a substring rather than a word or a prefix.
Matching is diacritic-insensitive and case-insensitive unless the caller asks for case
sensitivity, which changes nothing else. There is no fuzzy matching, stemming, Boolean
syntax, phrase grammar, or relevance. An empty query restricts nothing.

An Order matches on its Order ID, its user-facing canonical facts, its Items' descriptions,
catalog numbers, quantities and unit prices, its README's text, and every current shared
fact of the Vendor it names, Contacts and purchasing notes included — so editing a Vendor
changes which Orders match. A Vendor matches on its Vendor ID and its user-facing canonical
facts.

The decision's exclusions are exclusions here: directory basenames, filenames, filesystem
paths, Supporting Material, the Package README, schema versions, raw JSON keys and syntax,
raw Markdown syntax as against its text, calculated totals, configuration, and the derived
index. An entity matches once however many of its facts match, and the caller's ordering is
preserved rather than replaced by anything search decides.

Diacritic folding takes a normalization dependency beside the case folding the core already
carries; it is not hand-rolled. README text is not in the derived index, so a search reads
each healthy Order's README. That cost is accepted and performance is not this milestone's
goal.

A Damaged Order participates in no search.

## Acceptance criteria

- Two terms matching two different facts of one entity match it; two terms where one matches
  nothing match nothing.
- A substring inside a word matches, and a term is never treated as a prefix or a phrase.
- A query differing only by diacritics or case matches by default; asking for case
  sensitivity changes case behaviour and nothing else.
- Editing a Vendor fact changes which Orders match, without touching those Orders.
- An entity whose facts match several terms appears once.
- Every excluded source is proven excluded, including a term that appears only in a
  directory basename, only in a path, and only in the derived index.
- A Damaged Order is absent from both entity results.
- The caller's ordering survives a query unchanged.

## Exclusions

- The CLI surface, which is Ticket 004's.
- Supporting Material search in any form.
- Post-MVP structured filters, and every view, label, and interaction the decision
  describes for the desktop.

## Result

`spielorders_core::search` answers which Orders and Vendors of a loaded Package match a
query. Terms are whitespace-separated substrings combined with AND, folded free of
diacritics by canonical decomposition and, unless the caller asks otherwise, of case. An
Order matches on its record's canonical facts, its Items, its README's text, and the
current shared facts of the Vendor it names; a Vendor on its identity and its shared
facts. Each answer is a subsequence of the Package's own list, so a caller's ordering
survives and every Order keeps the position its Vendor resolves by. A Damaged Order has no
facts and enters no answer. A README is read as CommonMark text, so a link's destination
and an HTML tag's attributes stay out with the markup. `unicode-normalization` and
`pulldown-cmark` join the core.

The tests are in `crates/spielorders-core/tests/entity_search.rs`.
