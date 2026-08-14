---
title: Rebuild the Package Index and preserve replacement-failure evidence
status: open
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

Ordinary Package opening can assess a missing, malformed, wrong-version, stale, or
not-assessable Package Index, rebuild it deterministically from canonical state,
and either return the complete available Package or a blocking Package Index
Replacement Failure that preserves independently healthy canonical evidence and
the original index assessment.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Classify and project canonical Package state](002-classify-project-canonical-package.md).

## Scope

- Add the ordinary Package-load orchestration that reads and classifies
  `.spielorders/index.json` separately from canonical Package health.
- Implement the exact Package Index state vocabulary and typed, ordered reasons:
  `current`, `missing`, `malformed`, `wrong_version`, `stale`, and
  `not_assessed`. Malformed reasons are exactly `wrong_kind`, `unreadable`,
  `invalid_encoding`, `invalid_syntax`, `invalid_shape`, `invalid_semantics`, and
  `non_deterministic_serialization`. Stale reasons are exactly
  `order_membership_changed`, `order_path_mismatch`, `managed_input_wrong_kind`,
  `vendors_changed`, `order_metadata_changed`, `order_readme_changed`, and
  `change_evidence_uncertain`; reasons deduplicate in that order.
- Validate index kind/readability/encoding/syntax/shape, embedded semantics and
  cross-record relationships, version agreement, required order sort, and exact
  deterministic serialization. A valid index for which freshness change evidence
  is not yet available is conservatively stale with
  `change_evidence_uncertain` and selects canonical rebuild.
- Rebuild from only validated current version/config/Vendors, lexically ordered
  non-hidden ordinary direct-child candidates, each exact `order.json`, and each
  exact conventional Order README already composed by Ticket 002.
- Sort healthy entries by Date Ordered descending, normalized Order Title
  ascending, then Order ID; retain Vendors in canonical order; exclude every
  Damaged Order and all diagnostics from version-1 index bytes.
- Serialize the existing version-1 `IndexDocument` with fixed field order,
  two-space JSON, and one trailing newline; validate the candidate and replace
  only `.spielorders/index.json` through the existing validated atomic replacement
  boundary.
- Return `current` / `rebuilt` after success. Do not retain a diagnostic merely
  because automatic rebuild was needed.
- Preserve the refresh vocabulary exactly as `not_needed`, `rebuilt`,
  `not_attempted`, and `replacement_failed`, without treating any outcome as a
  diagnostic severity.
- Prevent replacement on Package-wide canonical damage or upgrade-only state and
  return the supported original assessment or `not_assessed` with
  `not_attempted`.
- Map every replacement phase to the specific blocking
  `package_index_replacement_failed` operation failure: `write_stage`,
  `read_stage`, `candidate_validation`, `sync_stage`, `replace`, or
  `remove_stage`, including related cleanup failure when present.
- Preserve the pre-refresh index assessment and reasons, old index bytes, exact
  index location, independent canonical diagnostics/context, and
  `replacement_failed`. Return no ordinarily usable Package projection and never
  relabel healthy canonical state as Damaged Package.

Exclude fast-path projection sourcing, stronger/forged-filesystem guarantees,
metadata changes, a sidecar, repair, explicit M3 persistence frameworks, or adapter
retry behavior.

## Canonical fixtures and fault injection

- Reuse the canonical stale/invalid index, healthy/damaged Package and Order,
  README, duplicate, unresolved-reference, and staging-preservation foundations.
- Add narrow cases for absent, wrong-kind, unreadable, invalid encoding/syntax/
  shape/semantics, non-deterministic bytes, wrong version, membership/path/kind
  mismatch, and uncertain change evidence. These are M2 derived-index recipes, not
  new schemas.
- Reuse the existing public replacement seam and inject write-stage, validation
  read, candidate validation, durability, replacement, and cleanup failures. Each
  failure asserts exact typed phase/path evidence, original assessment, old bytes,
  and independently healthy canonical state.
- Prove repeated rebuilds from the same canonical observation produce identical
  candidate bytes regardless of directory enumeration order.

## Supporting Material preservation requirements

Rebuild inputs are strictly enumerated above. The implementation may list only the
Orders Directory direct children and may read only exact canonical metadata and
Order README paths. It may not list an Order Directory or use Supporting Material
as freshness or projection evidence. Success and every injected replacement/
cleanup failure must preserve both opaque sentinels byte-for-byte and show no
effect-log entry for them.

## Core/filesystem-port and adapter boundary

The Rust core owns index assessment/reasons, canonical selection, projection,
serialization, candidate validation, replacement decision, and failure mapping.
The existing filesystem port owns reads, no-follow kinds, directory observations,
staging, durability, replacement, and cleanup. Do not add a generic repository,
CLI/Tauri command, frontend retry state, platform error contract, or shared crate.

## Acceptance criteria

- Every index state, reason, and refresh outcome is distinct from diagnostics and
  canonical damage and appears in its prescribed deterministic order.
- Missing, malformed, wrong-version, and conservatively stale indexes rebuild to
  exact version-1 bytes containing only complete healthy Order projections,
  required cached README text, and canonical-order Vendors.
- Order-local damage permits a healthy-only rebuild and remains visible as one
  placeholder per candidate; its absence from version-1 index bytes causes a later
  membership mismatch and canonical path.
- Package-wide damage never replaces the index and does not expose a partial
  ordinary projection.
- Each replacement phase is completely injectable through the filesystem port.
  Required failure returns `index_replacement_failed`, the original assessment,
  `replacement_failed`, exact locations/phase, cleanup evidence, unchanged old
  index bytes, and no false Damaged Package/Order classification.
- No persisted byte contains timestamps, filesystem identity, diagnostics,
  revision tokens, reusable-value lists, search data, or Supporting Material.

## Review boundary

Review automatic derived-index assessment, deterministic rebuild, validated atomic
replacement, and blocking failure behavior as one vertical ordinary-load outcome.
Do not accept fast-path canonical avoidance, a schema change, new sidecar,
general persistence framework, repair/retry presentation, or unrelated adapter
work.

## Exact fixed-point claim protocol

Require Tickets 001 and 002 closed, this ticket open and unclaimed, and an empty
`git status --porcelain=v1 --untracked-files=all`. Record `git rev-parse HEAD`
exactly as `ticket_start_fixed_point`, set `status: in progress`, `assignee`, and
`claimed_at`, and commit only that claim. Use the recorded pre-claim SHA for review
and stop on any dependency, claim, or worktree mismatch.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. Start with one public
missing-index RED test that observes exact deterministic replacement and catches a
Package opener that cannot become available. Proceed one RED-to-GREEN behavior at
a time through assessment reasons, healthy-only projection, stable bytes, Package
damage, and each replacement/cleanup failure. Reuse real metadata/validators and
fake only filesystem effects.

Commit all behavior/evidence, then use the project-local `code-review` skill against
the exact fixed point. Resolve and record independent Standards and Spec findings,
rerun affected checks, commit the disposition, and close only this ticket.

## Required verification

Focused gates:

- dedicated index assessment/rebuild and replacement-failure integration suites;
- existing atomic replacement, metadata, Package/Vendor, Order, README, and path
  suites affected by composition; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- deterministic candidate-byte repetition and old-index preservation evidence;
- dependency-direction/prohibited-scope audit; and
- exact before/after hashes for both opaque sentinels.

Use the pinned Node version. Do not update dependencies, schemas, fixtures except
narrow scenario recipes, or lockfiles.

## Handoff requirement

After closure, generate the complete ready-to-paste prompt for the first eligible
ticket—expected to be [Load a current Package Index without canonical record rereads](004-load-current-index-fast-path.md)—with
the same exact fixed point, one-ticket TDD/review, preservation, gates, and handoff
rules. Never skip a blocked or claimed ticket.
