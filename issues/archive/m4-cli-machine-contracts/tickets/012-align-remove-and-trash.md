---
title: Align, remove, and trash one Order Directory
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T17:05:00Z
closed_at: 2026-09-04T17:35:00Z
review_start: b463c4f433fd1f0c8d0dcb348d8779341794a819
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`order align` renames an Order Directory to match its facts. `order remove` moves the
complete Order Directory into the Removed Orders Directory, and `order remove --delete`
sends it to the platform Trash instead. These are the only commands that may move Supporting
Material, and they move it as part of the whole directory, intact.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)

## Blocking

Blocked by [Ticket 010](010-add-and-adopt-orders.md), whose Order Directory vocabulary and
locator handling these extend.

## Scope

- `order align`: rename the Order Directory to the aligned name derived from the Order's
  facts, refusing a collision rather than inventing a suffix, leaving the Order active.
- `order remove`: move the complete directory to `<removed_path>/<original-basename>`,
  refusing a collision, updating the locator where necessary, creating the Removed Orders
  Directory lazily, and rebuilding the index. Removed contents leave active discovery,
  indexing, search, validation, upgrade, and repair.
- `order remove --delete`: submit the complete directory to the platform Trash instead. It
  never erases; permanent disposal stays ordinary platform trash management.
- **The `NativeTrash` production effect**, at the effect boundary Ticket 002 established.
  Its mechanism is this ticket's choice under two constraints: it submits an intact
  directory and never erases, and it offers no enumeration or emptying. A platform without
  a supported Trash reports that specifically rather than falling back to deletion.
- Report the removed entity and its disposition, and report a rollback honestly.
- `--dry-run` on both commands.

## Acceptance criteria

- Alignment renames the directory, the Order stays active and readable at its new path, and
  its locator still resolves to this Package.
- An alignment or removal onto an occupied name is refused with a typed collision error; no
  suffix is invented and nothing is moved.
- A removed Order arrives complete at its destination with managed metadata, README, and
  both opaque sentinels byte-identical, the Removed Orders Directory created at that moment
  only if it did not exist.
- After removal the Order is absent from `package show`, the index, validation, and every
  other active operation, and the Package is otherwise unchanged.
- `--delete` submits the intact directory, it is no longer at its original path, and nothing
  was erased — asserted against the real effect where one exists, reported as specifically
  unsupported where none does.
- A disposition whose move fails and whose undo also fails reports both, leaves the Order
  active with its opaque tree untouched, and leaves the disposition available. An Order
  stranded half out of scope is what this forbids.
- A committed removal whose index refresh then fails exits `0`, reports the warning, stays
  committed, and does not look like something to retry.
- `--dry-run` over alignment, removal, disposal, and each collision refusal leaves the
  Package byte-identical: no rename, no staged file, no created directory.
- Fixtures cover the cells both commands can reach and no others, unreachable cells' reasons
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- **Permanent erasure in any form.** No force flag, no empty-trash operation, and no
  fallback that deletes when the Trash is unavailable.
- Inspecting, inventorying, parsing, or hashing Supporting Material in order to move it.
- Restoring a removed Order. Recovery is moving the directory back with ordinary tools.
- Automatic alignment after an edit. Drift is silent until the user asks.
- Bulk alignment or bulk removal. Each command acts on one Order.
- Order and Item fact mutation, which is Ticket 011's.

## Result

`order align` renames one Order Directory to the name its facts project, and
`order remove` takes the whole directory out of the Package — into its Removed Orders
Directory, or to the platform Trash under `--delete`. Both ask the core capabilities that
own the move, so a directory arrives whole or does not move at all.

The `NativeTrash` production effect joins the one effect boundary as the platform's own
request to trash an item: it reports where the directory landed, and has no delete
fallback.

The behaviour is in `crates/spielorders-cli/tests/order_alignment_and_removal.rs`, the
shipped Trash adapter in `tests/platform_effects.rs`, and both contracts' cells in
`tests/response_contract.rs` against `contracts/cli/order/align` and
`contracts/cli/order/remove`.

Review moved both plans onto core `Location`s, so every reported path inside a Package
carries its Package-relative spelling.
