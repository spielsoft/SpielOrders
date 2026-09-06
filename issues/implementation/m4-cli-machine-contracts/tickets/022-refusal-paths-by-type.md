---
title: Make a bare path unable to reach a refusal's reported paths
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T20:12:39Z
closed_at: 2026-09-05T20:55:02Z
review_start: 64de4526de9029a066685a6acedd7d99d83a963e
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A refusal reports a path the way a success does, and the type system is what makes that
true, so the next command cannot reintroduce the defect two tickets have now fixed by
hand.

## Decision sources

- [The fixture convention](../../../../docs/development/traceability.md), which decides
  when a reported path carries its Package-relative spelling
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [`AGENTS.md`](../../../../AGENTS.md), whose Records section decides that architecture is
  enforced by visibility, by types, and by clippy

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md). Accepting the
milestone while the convention is knowingly broken across nine command lines would make
the acceptance record false the day it is written, and the owner drives this CLI from
agents after M4 and M8 — a refusal's paths are what an agent acts on.

## Scope

The `order` family drops the Package-relative spelling on the refusal it gives when the
record or the Package moves between the targeting read and the commit. Two functions in
the order commands take a bare path, so they cannot produce the spelling even in
principle, and between them they cover the Order and Item mutations, alignment, removal,
creation and adoption.

**Fix it by type rather than by test.** Both take the located type the success paths
already project from, so a bare path cannot reach a refusal projection at all and the
compiler is what holds the rule. Do not build a harness that drives the race: it is a
timing window, and a test that manufactures one pins the harness rather than the contract.

**Audit every `resolved(` call site once, here.** A site inside an established Package
goes through the located type. A site that is legitimately null — a directory refused for
not being a Root, a candidate refused before eligibility, anything reached before a Root
is resolved — keeps its null and gains a one-line comment at the call site saying why no
Package surrounds that path. Those comments are the record; no inventory belongs in this
ticket or in a document, because a list of call sites is stale on the next commit.

Where a refusal can be driven through a real process, assert its spelling from one, as the
Vendor commit refusals now are. Where it cannot, the type is the evidence.

## Acceptance criteria

- No function that builds a refusal's reported paths accepts a bare path where a Package
  has been established; passing one is a compile error.
- Every remaining site that reports a path without a Package-relative spelling carries a
  comment at that site saying why no Package surrounds it.
- A refusal reachable through a real process is asserted through one, spelling included.
- The projection is covered by a unit test rather than by a manufactured race.
- No fixture byte moves except a cell that was recording a null the convention forbids,
  re-recorded at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Any change to which refusals exist, their codes, their exit statuses, or their wording.
  This ticket changes how a path is carried, not what is refused.
- The core's own failure types beyond the located value a refusal needs.
- Making a race reachable, in production code or in a test.

## Result

A refusal reports a path the way a success does, and the compiler holds it: the two
functions that build the `order` family's race-window refusals take the located value
their success paths project from, so a bare path cannot reach a refusal projection at
all, and the core's Order mutation rejections carry a `Location` where they carried a
`PathBuf`.

Every `resolved(` call site in the adapter was audited. The order capabilities'
filesystem refusals now carry the core's `FailedPath`, which already tells an operation
inside an established Root from one reached before there is a Root; every remaining
rootless site says at that site why no Package surrounds it.

Where no process can open the race, the type is the evidence and a unit test on the
projection in `commands/order.rs` shows the spelling arriving through it. The
occupied-destination and adoption refusals are asserted from real processes.
