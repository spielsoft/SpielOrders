---
title: Prove the real index by hand, once
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-07T14:05:00Z
closed_at: 2026-09-08T12:03:30Z
review_start: be25fb174286044864f8daa19916341cc8deddba
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

One script the owner runs by hand shows the real Spotlight provider, and the real open and
reveal, doing what the fakes promise.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)

## Blocking

Blocked by [Ticket 006](006-find-material-with-the-platform-index.md), which it exercises.

## Scope

Spotlight indexes a temporary directory late or not at all, and opening and revealing a file
launch applications on the owner's own machine. Neither belongs in a gate that must run
unattended and repeatedly. The decided rules are pinned against fakes; this script is how
the real thing is seen, once, before the milestone is accepted.

It is **opt-in and never part of `scripts/gate.sh`**. It builds a Package somewhere real,
waits for the index to catch up or says plainly that it did not, runs a query, and shows
what came back — then opens a file and reveals one, so a human can see both happen. It says
what a correct run looks like, so the owner can tell success from failure without reading
the source.

When this ticket closes, its handoff carries the script's path and its expected output, so
the owner can be handed the run rather than discovering it in a ticket.

## Acceptance criteria

- The script is not reachable from `scripts/gate.sh` and the gate's runtime is unchanged.
- It states its own expected output, including what an unindexed volume looks like, so a
  reader can judge a run without reading the code.
- It exercises the real provider, a real open, and a real reveal.
- It cleans up what it created, or says exactly what it left and where.
- The closing handoff names the script's path and its expected output.

## Exclusions

- Anything that runs unattended, and any change to the gate.
- The provider itself, which is Ticket 006's.

## Result

`scripts/smoke-spotlight.sh` builds a Package in the owner's home directory and plants
three files carrying one word: the Supporting Material the index should find, the Order's
own README, which is managed and must not come back, and a nested README wearing a managed
name, which is somebody else's and must. It waits for all three to be indexed, then prints
what SpielOrders reports under `--verbose`, so each file's kind and modification time show,
and opens the found file and reveals it — the part no test can perform.

It states its expected output before doing anything, tells an unindexed volume from an
empty answer and calls the second a failure, takes no argument, deletes nothing, and names
the Package it leaves behind. The gate does not reach it and its runtime is unchanged.
