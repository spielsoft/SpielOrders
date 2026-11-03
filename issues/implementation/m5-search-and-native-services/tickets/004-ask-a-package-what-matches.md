---
title: Ask a Package which Orders and Vendors match
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-07T10:37:00Z
closed_at: 2026-09-07T11:42:00Z
review_start: 56934dd52b1cb27df72576922ad4cb9cda069b5c
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

`search orders` and `search vendors` answer from the command line, under the response
contract every other command carries.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)

## Blocking

Blocked by [Ticket 003](003-match-an-entity-by-its-own-facts.md), whose matching these
expose.

## Scope

Both commands take their remaining arguments as keywords, whitespace-tokenized with AND
semantics **even when the shell passes one quoted multiword argument**. `--case-sensitive`
asks for the case control the core already answers. Literal text is never a phrase or a
regular expression, and regex is outside this contract.

Each command carries its own response contract identity, its recorded cells, and the exit
statuses every command shares. A search that matches nothing is an answer, not a failure.

**A core addition: a monetary fact matches by its stored spelling and by its two-decimal
spelling.** A reader sees `12.50` and searches it, so `12.50` finds an Item stored as
`12.5`. This belongs in the core, where matching is decided, so that the command line and
the desktop answer alike. Unit Price is the only searchable monetary fact: a Quantity is not
money and matches by its stored spelling alone, and the calculated totals are excluded from
matching altogether.

## Acceptance criteria

- One quoted multiword argument searches as several terms, matching what the same terms
  match when the shell splits them.
- `--case-sensitive` changes which entities match and nothing else about the response.
- A query matching nothing exits `0` with a present, empty collection.
- Each command's machine response carries the identities its human surface holds back.
- Every route to exit `2` carries the shared usage identity.
- An Item stored with a Unit Price of `12.5` is found by `12.50` and by `12.5`, and a
  Quantity stored as `2.5` is not found by `2.50`.
- Recorded cells cover what each command reaches and no more, at version 1.

## Exclusions

- What matches, which is Ticket 003's.
- Supporting Material search.
- Structured filter assignments, which are post-MVP.

## Result

`search orders` and `search vendors` answer from the command line, each under its own
response contract recording the cells a read reaches. Their arguments are joined and the
core separates the terms, so one quoted multiword argument searches as the terms a shell
would split. `--case-sensitive` changes which entities match and nothing else about the
response, a query matching nothing exits `0` with a present and empty collection, and
`--all` reaches a Hidden Vendor a query retained. Each identity is held back to
`--verbose` and carried in every machine response.

In the core, a monetary fact matches its stored spelling and the two-decimal spelling a
reader is shown, from the one place that spells money; a Quantity keeps its stored
spelling alone.

Tests: `crates/spielorders-cli/tests/entity_search.rs`,
`crates/spielorders-core/tests/entity_search.rs`, and the recorded cells under
`contracts/cli/search/`.
