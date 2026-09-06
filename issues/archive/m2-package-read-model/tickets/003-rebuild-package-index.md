---
title: Rebuild the Package Index and preserve replacement-failure evidence
status: closed
ticket_start_fixed_point: d66e9b0bf46b5c303c1944368258ea978cf2043e
assignee: /root
claimed_at: 2026-08-15T16:55:52Z
closed_at: 2026-08-15T17:35:28Z
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

## Implementation evidence (pre-review)

- Exact pre-claim fixed point:
  `d66e9b0bf46b5c303c1944368258ea978cf2043e`.
- Claim commit: `f488e67` (`Claim M2 Ticket 003 package index rebuild`).
- Implementation commit: `c28258b` (`Implement M2 Package Index assessment and
  rebuild`).
- Public RED-to-GREEN tracer bullets began with a missing-index ordinary open and
  advanced through every derived-index state and ordered reason, exact successful
  current/rebuilt semantics, deterministic healthy-only Order/Vendor/README bytes,
  damaged-Order exclusion, Package-wide damage suppression, every atomic
  replacement/cleanup phase, retained original assessment and old bytes, exact
  effect budgets, enumeration-order independence, and Supporting Material
  preservation. The existing filesystem port and validated atomic-replacement
  seam were the only fakes/effect boundaries.
- `cargo test -p spielorders-core --test package_index_rebuild --locked`: pass,
  `26` tests, including repeated byte-identical rebuilds from opposite directory
  enumerations and unchanged old-index bytes for `write_stage`, `read_stage`,
  `candidate_validation`, `sync_stage`, `replace`, and related `remove_stage`
  cleanup failure.
- Affected `atomic_replacement`, `metadata_v1`, `package_metadata`,
  `order_validation`, `readme_read`, `canonical_package_load`, and `package_paths`
  integration suites: pass (`89` tests total).
- `cargo test -p spielorders-core --locked`: pass.
- `cargo test --workspace --locked`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas`:
  pass (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22`
  scenarios, and `2` preservation sentinels), and
  `npm run check:traceability`: pass.
- `git diff --check d66e9b0bf46b5c303c1944368258ea978cf2043e...HEAD`:
  pass.
- Core dependency and prohibited-scope audit retains only `focaccia`, `serde`,
  `serde_json`, and `toml`; the changed files are core, one core integration suite,
  and Ticket 003 claim/evidence only. There is no CLI, Tauri, React, retry, repair,
  schema, dependency, lockfile, toolchain, M3, Ticket 004 fast-path, or Supporting
  Material inspection change. The sole new mutation call is the existing
  `replace_validated_file` boundary for `.spielorders/index.json`.
- Independent sentinel hashes before review remain exactly:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.
- No dependency, lockfile, schema, metadata-version-1 fixture, toolchain,
  install-script authority, adapter, fast-path change observation, repair, or M3
  change was made.

## Code review and disposition

Review completed at `2026-08-15T17:34:45Z` against the exact commands
`git diff d66e9b0bf46b5c303c1944368258ea978cf2043e...HEAD` and
`git log d66e9b0bf46b5c303c1944368258ea978cf2043e..HEAD`. Standards and Spec
were reviewed independently and in parallel as required by the project-local
`code-review` skill, then independently revalidated after each remediation.

### Standards

- Initial outcome: `1` hard fixture-boundary violation and `3` judgment calls.
  The two one-purpose derived-index recipes now live under `contracts/` and are
  consumed in place; one shared index-assessment routine owns observation/read/
  version/parse/semantic/deterministic decisions; staging paths come from the
  filesystem replacement owner; and operation-failure vocabulary lives in the
  purpose-named `operations` module with Ticket 001 compatibility re-exports.
- First post-remediation review found `1` traceability-marker omission and `4`
  judgment calls. The dedicated replacement suite now carries recognized decision
  sources; the duplicate failure matrix was removed from the rebuild suite;
  Package-relative staging evidence is derived from the filesystem-owned absolute
  path; index semantic validation moved beside `PackageMetadata`; and the canonical
  module description now reflects its ordinary-load refresh orchestration.
- The broader canonical-module split was dispositioned as unhelpful for this
  observable vertical slice: `open_package` must compose canonical classification,
  derived-index assessment, and required refresh into one public load result, while
  metadata semantics, replacement mechanics, and operation evidence now have their
  own owners.
- Remediation commits: `dd6c332` (`Resolve Ticket 003 review findings`) and
  `67c3763` (`Complete Ticket 003 review remediation`).
- Final independent Standards validation found no remaining hard violation or
  unresolved in-scope judgment-call smell.

### Spec

- Initial outcome: `4` findings, all accepted: staged validation did not bind the
  bytes to the canonical candidate; index kind-observation failure could select a
  reasonless malformed rebuild; embedded semantic/sort validation did not require
  normalized facts; and failure location/effect/hash evidence was partial.
- Staged bytes must now equal the canonical candidate before semantic and exact
  deterministic validation. Index kind-observation failure returns a typed general
  operation failure without replacement. Index validation requires already
  normalized embedded Orders/Vendors and the required ordering.
- The dedicated replacement-failure suite now covers all six primary/cleanup
  phases, ordered original assessment, unchanged old bytes, exact absolute and
  Package-relative index/staging locations, independently healthy canonical
  evidence, one exact Orders Directory listing, the complete authorized effect
  path budget, and byte-identical untouched Supporting Material.
- First post-remediation Spec validation found no behavioral defect or scope creep
  and one minor absolute-location/original-reasons assertion gap; commit `67c3763`
  closed that gap. Final independent Spec validation found no remaining finding.

### Post-review verification

- `cargo test -p spielorders-core --test package_index_rebuild --locked`: pass,
  `28` tests.
- `cargo test -p spielorders-core --test package_index_replacement_failure
  --locked`: pass, the complete injected failure/effect matrix.
- Affected `atomic_replacement`, `metadata_v1`, `package_metadata`,
  `order_validation`, `readme_read`, `canonical_package_load`, and `package_paths`
  suites: pass (`89` tests total).
- `cargo test -p spielorders-core --locked`: pass.
- `cargo test --workspace --locked`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- All touched Rust sources pass `rustfmt --edition 2024 --check`.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas`: pass
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels), and `npm run check:traceability`: pass.
- `git diff --check d66e9b0bf46b5c303c1944368258ea978cf2043e...HEAD`:
  pass.
- Exact focused repetitions pass for enumeration-independent byte-identical
  rebuild and the complete replacement-failure matrix. Every injected phase
  retains the old index bytes and independently healthy canonical evidence.
- Dependency/source audit leaves the core tree exactly `focaccia 2.0.0`, `serde
  1.0.229`, `serde_json 1.0.151`, and `toml 1.1.4+spec-1.1.0`; no dependency,
  lockfile, schema, metadata-version-1 fixture, toolchain, CLI, Tauri, React,
  retry, repair, M3, Ticket 004 fast-path, or Supporting Material inspection
  change exists. The only mutation remains the existing validated replacement
  boundary for `.spielorders/index.json`.
- Independent before/after sentinel commands produced identical hashes:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

## Result

Ticket 003 is complete. Ordinary Package opening now assesses the Package Index
separately from canonical health; rebuilds missing, malformed, wrong-version, and
conservatively stale indexes from complete healthy canonical projections with
exact deterministic metadata-version-1 bytes; excludes damaged Orders and all
diagnostics from persisted bytes; and returns a complete current/rebuilt Package
after successful validated atomic replacement. Every required replacement failure
blocks ordinary use while retaining the original ordered assessment, old bytes,
exact index/staging locations, cleanup phase, independently healthy canonical
context, damaged placeholders, diagnostics, and untouched Supporting Material.
Ticket 004's portable freshness observation and no-canonical-reread fast path
remain unimplemented and unclaimed.

## Handoff requirement

After closure, report the final SHA and the next eligible ticket—at that point,
[Load a current Package Index without canonical record rereads](004-load-current-index-fast-path.md).
The next worker loads its rules from the milestone index and project-local skills;
never skip a blocked or claimed ticket, and do not generate a recursive prompt.
