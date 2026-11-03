---
title: Ask for Supporting Material, and answer when nothing can
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-07T00:37:00Z
closed_at: 2026-09-07T02:05:00Z
review_start: fb2ae9b5719c9c9d0b387f6a0d3c1002a4857f8a
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

One port asks the platform's index for Supporting Material and returns a single settled
answer, and `search supporting-material` says plainly when no provider can answer.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

None.

## Scope

**The port is a synchronous call.** It takes a query, a cancellation, and a bounded timeout,
and returns one settled snapshot. It is not a stream and it returns no future: no async
runtime enters the core or the CLI. "Asynchronous" in the decisions means the work runs off
the caller's UI thread, which the desktop shell arranges by calling this from a worker.

The snapshot carries every provider fact the desktop will need: which provider answered and
what state it is in, whether the answer is complete or partial, the Orders the files group
under, each file's absolute and Package-relative path, its kind and modification time, its
native rank or the fact that the provider offers none, the facts the deterministic fallback
ordering sorts by, and any provider-specific diagnostic. Results deduplicate by standardized
path alone: one path appears once, while identical contents or repeated filenames at
different paths stay distinct.

A timeout returns what was gathered, marked incomplete, with a diagnostic saying so.
Cancellation returns what was gathered the same way. Neither is a failure.

A fake provider, driven by tests, stands behind the port so every rule above is proven
without a platform index. `search supporting-material` answers through the port and, where
no provider exists, says which platform state prevented an answer — it never falls back to
walking the filesystem itself.

## Acceptance criteria

- The port's signature takes a cancellation and a timeout and returns a settled snapshot;
  neither the core nor the CLI gains an async runtime dependency.
- A snapshot carries provider identity and state, completeness, Order grouping, both path
  spellings, kind, modification time, rank or its declared absence, the fallback ordering
  facts, and diagnostics.
- Two files with identical contents at different paths are two results; one path reached
  twice is one.
- A timeout and a cancellation each return the gathered results marked incomplete with a
  diagnostic, at exit `0`.
- With no provider available, the command names the state that prevented an answer and
  walks nothing.
- Recorded cells cover what the command reaches, at version 1.

## Exclusions

- Any real platform provider, which is Ticket 006's.
- Open and reveal.
- Every view, label, grouping control, and debounce the decision describes for the desktop.

## Result

One synchronous core call asks a platform index for a Package's Supporting Material and
returns one settled snapshot: which provider answered and its state, whether the answer is
complete, the Orders the files group under, each file's absolute and Package-relative path,
its filename, kind, modification time, and rank or the absence of one, and the provider's
diagnostics. Deduplication is by standardized path alone, an Order's own README and
reserved subtree are excluded by their locations rather than their names, and a cancelled
request never reports completion.

`search supporting-material` answers through it, and where no provider can look it names
the platform state that prevented one, at exit `1`, walking nothing — which is every run of
this build, since no platform provider is compiled in.

Tests: `crates/spielorders-core/tests/supporting_material_search.rs`,
`crates/spielorders-cli/tests/supporting_material_search.rs`, and the unit tests beside
`crates/spielorders-cli/src/commands/search.rs`.
