---
title: Find Supporting Material with the platform's own index
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-07T11:25:13Z
closed_at: 2026-09-07T13:05:00Z
review_start: 91c774e10d5de5a6eb5b95ecbeaca5f8de3b7da2
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

On macOS the port is answered by Spotlight, and a found file can be opened or revealed
where it stands.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

Blocked by [Ticket 005](005-ask-for-supporting-material.md), whose port this implements.

## Scope

The provider is one `mdfind` run scoped to the Package's Orders Directory, its results kept
only where they fall inside a healthy recognized Order Directory. Each Order's conventional
README and its reserved subtree are excluded, so managed Markdown and JSON never return as
file results; a nested file is not excluded merely for having a familiar name. No
Objective-C binding is taken as a dependency.

`mdfind` reports no relevance, so the provider records rank as absent and the deterministic
fallback ordering decides, which the decision already allows.

Opening a file and revealing it are the platform's own `open` and `open -R`. A result that
has moved or vanished before either is reported unavailable rather than acted on, and drops
out when results refresh.

An interrupted query leaves nothing behind and the process exits by the platform's
convention. How that is arranged is this ticket's choice — a handler that trips the
cancellation and lets the provider return, or no handler at all — and the behaviour is what
is tested.

## Acceptance criteria

- A file inside a healthy Order Directory is found; one outside the Orders Directory, one
  under a Damaged Order, an Order README, and a file in the reserved subtree are not.
- A nested file whose name resembles a managed one is still found.
- Rank is reported absent and ordering falls to the deterministic facts.
- Open and reveal each act on the exact file, and a file removed beforehand is reported
  unavailable instead.
- An interrupted query exits by the platform's convention and leaves nothing behind.
- The dependency graph gains no Objective-C binding.

## Exclusions

- The port's shape, which is Ticket 005's.
- Behaviour on platforms with no provider, which is Ticket 007's.
- Proving the real index by hand, which is Ticket 008's.

## Result

On macOS the port is answered by one `mdfind` run scoped to the directory holding every
Order Directory the request named. It reports every path the query wrote and no fact beside
the path, so relevance, file kind and modification time are absent and the deterministic
ordering decides. Keeping results to a healthy Order and leaving out an Order's own managed
documents stay in the core; the shipped composition is asserted over paths prepared as the
query reports them. `effects::native_files` opens and reveals a result through `open` and
`open -R`, and a file that has gone since the index reported it is reported unavailable
rather than handed over. An interrupted query ends by the platform's own default disposition
and writes nothing.

`search supporting-material` now records its `empty-result`, `warning` and `invalid` cells,
and the conformance matrix says why `success` cannot be.

Tests: `crates/spielorders-cli/tests/supporting_material_search.rs`.
