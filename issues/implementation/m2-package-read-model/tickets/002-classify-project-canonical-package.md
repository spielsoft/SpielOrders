---
title: Classify and project canonical Package state
status: open
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

A Rust caller can inspect the canonical managed inputs for one selected root and
receive deterministic unmanaged, canonical current-Package, Damaged Package,
upgrade-pending, or general operation-failed evidence, including complete healthy
Order/Vendor facts and one Damaged Order placeholder for every damaged candidate.
This is the canonical slow-path capability that the next ticket composes with the
required derived-index assessment and replacement; it is not yet a successful
ordinary open when index refresh is required.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Read one exact conventional README through the shared core](001-read-exact-conventional-readme.md).

## Scope

- Discover only the selected root, root `.spielorders` artifacts, configured Orders
  Directory direct children, each candidate's exact `.spielorders/order.json`, and
  each exact conventional Order README needed by the version-1 canonical path.
- Produce `unmanaged` only for a selected ordinary directory with a wholly absent
  root `.spielorders`; absence is nondiagnostic and carries only the absolute root.
- Build established managed context and exact locations for root, Package metadata,
  configuration, version, Vendors, index, configured Orders/Removed directories,
  Package README, and every discovered Order artifact/reference.
- Map encoding, syntax, shape, version, configuration, path, canonical read, Order
  semantic/locator/identity, Vendor identity/name, and live Vendor Reference
  failures into the stable typed diagnostic contract at the narrowest scope.
- Implement the exact canonical codes owned by this slice:
  `managed_artifact_missing`, `managed_artifact_wrong_kind`,
  `managed_artifact_unreadable`, `managed_artifact_invalid_encoding`,
  `managed_artifact_invalid_syntax`, `managed_artifact_invalid_shape`,
  `managed_artifact_wrong_version`, `configuration_path_invalid`,
  `configuration_paths_overlap`, `configured_orders_directory_unavailable`,
  `order_semantic_invalid`, `order_package_locator_mismatch`,
  `order_id_duplicate`, `vendor_id_duplicate`, `vendor_name_duplicate`,
  `vendor_reference_unresolved`, and `orders_directory_entry_disallowed`, with
  precisely the closed severity, scope, artifact/field/reason, identity, and
  location evidence. The configured-directory reasons remain exactly
  `not_directory`, `outside_package`, and `reserved_metadata`.
- Produce Damaged Package `diagnostic_only` without leaking partial Vendors,
  healthy Orders, reusable values, or fabricated managed evidence.
- For a current version-1 canonical Package, build the complete candidate
  projection: normalized healthy Order documents; trustworthy identities; paths;
  ordered Items and calculated Item/Order totals; version-1 index README text from
  the exact canonical read; complete normalized Vendors and ordered Contacts; live
  resolvable Vendor associations; all four reusable-value lists
  (`ordering_institutions`, `orderers`, `projects`, and `order_groups`); damaged
  placeholders; grouped diagnostics; and deterministic empty/absent semantics.
- Sort healthy Orders by Date Ordered descending, normalized Order Title ascending
  by UTF-8 bytes, then Order ID; sort damaged placeholders separately by normalized
  Package-relative Order Directory path. README cache text is exact decoded text,
  empty for absence, and empty with a document warning when unavailable.
- Make every duplicate Order-ID participant a damaged placeholder with the same
  complete ordered conflict set in its own Order diagnostic. Never select a
  survivor or treat the duplicate ID as a unique lookup key.
- Implement metadata-state classification and `upgrade_pending` projection shape
  for supported older and recognized resumable states using symbolic source/target
  observations only. Version 1 remains the only on-disk schema, so current code may
  produce `current`, `unsupported`, or `not_assessed` but must not invent an older
  or future artifact, executor, migration, or mutation plan.
- Return general `operation_failed` only when an effect cannot establish another
  result; persistently observed required-artifact failures are canonical
  diagnostics exactly once.
- Use stable operation code `filesystem_operation_failed` with the exact attempted
  `canonicalize`, `observe_kind`, `list_directory`, or `read` operation and exact
  primary/related locations. Platform detail remains optional and noncontractual.
- Retain Package Index as `not_assessed` / `not_attempted` in this pre-refresh
  canonical result. Do not expose a healthy candidate as successful ordinary
  Package opening until Ticket 003 composes the required index policy.

Exclude index classification/replacement, fast-path sourcing, reserved-directory
inventory, M3 upgrade execution or repair, and all adapter behavior.

## Canonical fixtures and fault injection

- Reuse in place the unmanaged, healthy, Package-damaged, Order-damaged,
  duplicate-ID, duplicate-Vendor, unresolved-reference, directory-drift,
  direct-child-warning, README, unsupported-newer, symbolic-upgrade, and unusual
  valid-value scenarios plus the minimal/full version-1 examples.
- Add only one-purpose M2 recipes needed to prove result tags, exact locations,
  complete diagnostic evidence/order, multiple duplicate-ID participants, all four
  reusable-value lists, and deterministic canonical healthy-Order sorting.
- Extend a public filesystem double to record reads, kinds, directory lists, and
  injected canonicalize/observe/list/read failures. It must allow failures at every
  read boundary without depending on platform strings or enumeration order.

## Supporting Material preservation requirements

Canonical inspection may list only the Orders Directory itself. It may read only
the exact governed metadata and conventional Order README paths. It must never list
an Order Directory, recurse, inspect a general child, or use Supporting Material as
identity, Vendor, title, date, status, diagnostic, or upgrade evidence. Tests must
materialize both opaque sentinels in different nested locations, log zero effects
against them, and prove byte-for-byte preservation.

## Core/filesystem-port and adapter boundary

The Rust core owns result/state variants, managed context, validation mappings,
trustworthy identities, totals, Vendor resolution, value grouping, diagnostics,
ordering, and absent evidence. The filesystem port supplies bytes, no-follow kinds,
direct-child entries, canonicalization, and opaque failures. No CLI JSON, Tauri,
React, retry presentation, or platform error taxonomy enters the capability.

## Acceptance criteria

- All canonical load-result families owned here are deterministic and use the
  exact tagged boundaries; package-wide damage never exposes a partial ordinary
  projection and early filesystem failure never fabricates canonical damage.
- Healthy Orders contain every normalized version-1 fact and total; Vendors retain
  canonical aggregate/contact order; live Vendor References resolve without
  duplicating Vendor objects into Orders.
- Damaged Orders are one-for-one and ordered by Package-relative directory path.
  Duplicate-ID conflicts damage every participant and carry complete conflict-set
  locations.
- Trustworthy decoded identities remain present as evidence through semantic and
  duplicate failures; missing/malformed identities remain absent. Every established
  location has absolute and Package-relative paths, while unexamined evidence is
  absent rather than represented by an empty/zero/false placeholder.
- All four reusable-value collections are present, derive only from healthy Orders,
  group by trimmed full-Unicode case-insensitive equality, retain first
  deterministic spelling, count Orders, and sort by canonical UTF-8 bytes.
- Diagnostics implement every canonical Package/Order code, stable severity/scope,
  required location/identity/field evidence, grouping, deduplication, and ordering
  exercised by canonical reads. Structural warnings do not block availability.
- Package and Order README references remain exact; canonical Order README reads
  supply version-1 cache text and warnings without damaging healthy Orders.
- All public collections are present when empty except intentionally omitted empty
  diagnostic groups; absent canonical optionals remain absent and every healthy
  zero-Item Order carries total zero.
- Supported/resumable upgrade reporting is representable and deterministically
  ordered, but no non-version-1 file or upgrade executor exists.

## Review boundary

Review complete canonical classification/projection and evidence only. Private
module choices are free, but do not accept a types-only layer, generic repository,
derived-index persistence, exhaustive reserved-content scan, repair/mutation,
adapter work, or Supporting Material inspection.

## Exact fixed-point claim protocol

Require Ticket 001 closed and integrated, this ticket open and unclaimed, and an
empty `git status --porcelain=v1 --untracked-files=all`. Record the exact
`git rev-parse HEAD` as `ticket_start_fixed_point`; set `status: in progress`,
`assignee`, and `claimed_at`; commit only that claim. Use the pre-claim SHA for
review. Stop if any blocker, claim, baseline, or worktree condition disagrees.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. Begin with a public filesystem-
port tracer bullet for unmanaged versus a minimal current canonical Package, then
advance one RED-to-GREEN behavior at a time through Package damage, complete
healthy projection, each Order-local damage family, duplicates, stable evidence,
value lists, operation failure, and symbolic upgrade-state classification. State
the observable defect each test catches; do not write all result types/tests first.

Commit the complete ticket before using the project-local `code-review` skill on
`<ticket_start_fixed_point>...HEAD`. Run Standards and Spec independently, resolve
or disposition every finding, rerun affected gates, commit the outcome, and close
only this ticket.

## Required verification

Focused gates:

- the dedicated canonical Package load/projection integration suite;
- focused README, metadata, Order, Package/Vendor, and path suites touched by the
  capability; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- a dependency-direction and prohibited-adapter/scope source audit; and
- exact before/after hashes for both opaque sentinels.

Use the pinned Node version for npm commands. Do not modify dependencies, lockfiles,
schemas, or metadata-version-1 fixtures.

## Handoff requirement

After closure, generate the complete ready-to-paste prompt for the first open,
unblocked, unclaimed M2 ticket—expected to be
[Rebuild the Package Index and preserve replacement-failure evidence](003-rebuild-package-index.md)—and
require the same exact fixed point, single-ticket TDD/review workflow, preservation,
verification, and recursive handoff. Report a blocker or claim instead of skipping
ahead.
