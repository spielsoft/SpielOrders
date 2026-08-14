# M1 acceptance evidence

Status: passed, reviewed, and closed with M1 on 2026-07-25

Ticket-start fixed point: `76bd13026da9477903aba10d39034c8f129077c5`

Decision sources: [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md), [Define the SpielOrders package topology](../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md), [Define the Order record and lifecycle contract](../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md), [Define Vendor identity and historical semantics](../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md), [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), [Define integrity, validation, and repair boundaries](../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md), [Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md), and [Require two parent segments in the Order package locator](../m0-contract-baseline/tickets/007-require-two-parent-package-root.md).

## Environment and clean copy

The gate ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with:

- Rust and Cargo 1.97.1 using the repository's `rust-toolchain.toml` override;
- Node.js 24.15.0; and
- npm 11.16.0.

The worktree was clean before the copy. The committed tree at the acceptance-gate
claim commit, `0c460fa`, was exported without `.git`, `target`, `node_modules`, or
`dist` artifacts:

```sh
git status --porcelain=v1
git diff --check
mktemp -d /private/tmp/spielorders-m1-gate.XXXXXX
git archive HEAD | tar -x -C /private/tmp/spielorders-m1-gate.qnHOvb
find /private/tmp/spielorders-m1-gate.qnHOvb -type d \
  \( -name target -o -name node_modules -o -name dist \) -print
```

The final `find` printed nothing. Both lockfiles then recreated their dependency
trees without registry access:

```sh
# working directory: repository root
npm clean-install --offline
# working directory: apps/desktop
npm clean-install --offline
```

Both installs passed and reported zero vulnerabilities.

## Executable gate results

All commands below ran in the artifact-free copy unless a different directory is
shown.

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked` | Passed from an empty `target` directory |
| `cargo test --workspace --locked` | Passed: 59 Rust tests plus doc tests |
| `cargo test -p spielorders-core --locked` | Passed independently: the same 59 core tests plus doc tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 17 rejected fixtures, 22 scenarios, and 2 preservation sentinels |
| `npm run check:traceability` | Passed: 7 M0 tickets and 4 baseline test files |
| `npm run test` from `apps/desktop` | Passed: 2 frontend boundary tests |
| `npm run build` from `apps/desktop` | Passed: TypeScript and Vite production build |
| `npm run tauri build -- --no-bundle` from `apps/desktop` | Passed: release desktop shell build |

The 59 Rust tests comprise 2 core boundary/unit tests and the complete public
integration suites: 18 metadata-codec tests, 8 Order-validation tests, 8
Package/Vendor-validation tests, 10 safe-path tests, 8 validated single-file
replacement tests, and 5 recoverable multi-file replacement tests. The focused
pre-gate run of those six integration suites passed all 57 tests before the clean
copy was made.

## Contract and semantic agreement

The JavaScript Draft 2020-12 schema suite and the public Rust codecs consume the
same files under `contracts/`. Both accept all eight minimal/full version-1
documents and reject every one of the 17 manifest-listed schema-invalid fixtures.
The codec suite additionally proves canonical two-space JSON, deterministic field
order, omitted absent optionals, present collections, and exactly one trailing
newline. It rejects a BOM, non-UTF-8 input, explicit optional `null`, unknown
fields, wrong versions, malformed identities, invalid currency/status shapes,
invalid nested values, and single-parent Package locators.

The public semantic suites prove:

- real Gregorian dates, all five accepted statuses, future dates, zero Items,
  absent optionals, finite negative and zero Item values, and exact unrounded Item
  and Order totals;
- lowercase UUIDv4 identity, every participant in duplicate Order and Vendor IDs,
  Unicode-case-folded and trimmed Vendor Name uniqueness, absent and unresolved
  Vendor References, live Vendor lookup by ID, Vendor/Contact normalization, and
  absent/reused/new Reusable Free-Entry Values;
- local validation of index-embedded Orders without adding index loading,
  freshness, or rebuild behavior; and
- default and configured topology, nested Orders paths, overlap and reserved-path
  rejection, Windows and POSIX absolute forms, parent traversal, hidden/direct-child
  classification, stale locators, real symlink escape, and no-follow link handling.

### Canonical scenario coverage

The following matrix makes every `contracts/fixtures/scenarios.json` disposition
auditable. Test names refer to public-behavior integration tests in
[`metadata_v1.rs`](../../../crates/spielorders-core/tests/metadata_v1.rs),
[`order_validation.rs`](../../../crates/spielorders-core/tests/order_validation.rs),
[`package_metadata.rs`](../../../crates/spielorders-core/tests/package_metadata.rs),
[`package_paths.rs`](../../../crates/spielorders-core/tests/package_paths.rs),
[`atomic_replacement.rs`](../../../crates/spielorders-core/tests/atomic_replacement.rs),
and
[`multi_file_replacement.rs`](../../../crates/spielorders-core/tests/multi_file_replacement.rs).

| Canonical scenario | Applicable M1 proof through public seams | Deferred portion |
| --- | --- | --- |
| `healthy package and healthy order` | `minimal_order_round_trips_through_canonical_json`, `full_order_preserves_every_version_one_fact_and_item`, `version_marker_round_trips_through_canonical_json`, `vendor_documents_preserve_ordered_vendors_contacts_and_optional_facts`, and `index_documents_preserve_complete_order_readme_and_vendor_projections` cover all eight accepted documents; `order_with_an_existing_vendor_reference_validates_and_resolves_live_facts`; `defaults_resolve_and_an_ordinary_direct_child_is_a_candidate`; `two_valid_canonical_files_commit_without_touching_unrelated_content` | Complete Package loading is M2 |
| `damaged order malformed canonical record` | `invalid_candidate_preserves_the_old_destination_and_cleans_staging` rejects malformed Order JSON through the public codec and preserves the canonical destination | Damaged Order projection and diagnostics are M2 |
| `ignored hidden direct children` | `direct_child_classification_never_follows_links` classifies hidden entries as ignored without following them | None |
| `warned direct-child file and link` | `direct_child_classification_never_follows_links` returns disallowed file/link observations and the real-symlink case | Warning codes and Package projection are M2 |
| `unexpected reserved Order content` | `only_package_and_immediate_order_metadata_subtrees_are_reserved` recognizes only the managed reserved subtrees | Warning policy is M2 |
| `invalid derived index` | `every_schema_invalid_fixture_is_rejected_by_its_public_codec` rejects `index-missing-vendors.json`, agreeing with the schema suite | Rebuild policy is M2 |
| `duplicate Order IDs` | `duplicate_order_ids_report_every_conflicting_order` reports every participant | Damage projection is M2 |
| `duplicate Vendor IDs` | `duplicate_vendor_ids_report_every_conflicting_vendor` rejects the aggregate | Damage projection is M2 |
| `duplicate Vendor Names` | `vendor_names_are_unique_after_trimming_and_full_unicode_case_folding` rejects the normalized Unicode duplicate | Damage projection is M2 |
| `unresolved Vendor Reference` | `absent_vendor_references_are_valid_and_unresolved_ids_fail` rejects the live-ID miss without inference | Repair and damage projection are M2/M3 |
| `unusual valid numeric Item values` | `zero_and_negative_item_values_remain_valid` plus `totals_keep_unrounded_floating_point_results` accept finite signed values and preserve exact arithmetic | None |
| `staging artifact` | `existing_staging_evidence_is_never_overwritten_or_deleted` and `cleanup_failure_is_reported_and_leaves_beside_destination_evidence` preserve and expose staging evidence | Stable staging diagnostics are M2 |
| `unsupported newer metadata` | `unsupported_metadata_version_is_a_shape_failure` rejects the unsupported marker without a write | Package-level diagnostic state is M2 |
| `current version no-op upgrade` | `version_marker_round_trips_through_canonical_json` accepts and preserves the current version-1 marker | Upgrade orchestration/no-op reporting is M3 |

The remaining eight canonical recipes have no executable M1 behavior: `unmanaged
root`, `damaged package missing canonical root artifact`, `README absence`, `README
unreadable`, and `stale derived index` belong to M2 Package loading and diagnostics;
`directory-name drift` and `interrupted upgrade version combination` belong to M3
mutation/upgrade orchestration; and `legacy adoption candidate with ambiguous
evidence` belongs to later adoption work. They remain declarative fixtures and were
not reinterpreted by this gate.

## Filesystem failure and preservation matrices

[Preserve one canonical file through validated atomic replacement](tickets/005-atomic-canonical-replacement.md)'s
complete public fault-injection suite passed:

- exact real-filesystem replacement of a valid staged Order;
- invalid candidate, staging-write, validation-read, durability, and atomic
  replacement failures preserving the prior destination;
- cleanup failure remaining observable beside the destination; and
- pre-existing staging evidence remaining unmodified and unpromoted.

[Restore prior canonical state after a failed multi-file replacement](tickets/006-multi-file-rollback.md)'s
complete public multi-file and rollback suite passed:

- aggregate candidate-plan rejection with no writes;
- invalid and duplicate destination rejection before filesystem effects;
- real two-file commit in caller-declared order;
- second-replacement failure with exact reverse restoration; and
- an initiating later failure with every reverse rollback failure retained in the
  typed result.

Both suites assert that unrelated entries remain unchanged. The real multi-file
test also preserves both opaque Supporting Material sentinels through the complete
operation. Their SHA-256 hashes matched before the clean export and after all gate
commands:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

## Dependency audit

From the clean source worktree, `cargo update --dry-run --verbose` refreshed the
crates.io index and found zero compatible lockfile updates. The public core's direct
resolved dependencies are `focaccia` 2.0.0, `serde` 1.0.229, `serde_json` 1.0.151,
and `toml` 0.9.12. Newer
reported `generic-array` 0.14.9, `toml` 0.8.23, `toml_datetime` 0.6.11, and
`toml_edit` 0.20.7 releases remain constrained by the current dependency graph.
`toml` 1.1.3 is a new major version and was not adopted without approval.

The clean lockfile resolves root AJV 8.20.0 and the desktop's Tauri API 2.11.1,
Tauri CLI 2.11.4, Node types 26.1.1, React/ReactDOM 19.2.8, React types 19.2.17,
ReactDOM types 19.2.3, TypeScript 5.9.3, Vite 8.1.5, and its React plugin 6.0.4.
[Restore prior canonical state after a failed multi-file replacement](tickets/006-multi-file-rollback.md)
verified both npm trees current on 2026-07-25. The acceptance gate attempted fresh
`npm outdated --json` registry queries, but sandbox DNS was unavailable and the
workspace owner's network spend cap then rejected the escalated query before npm
received a registry response. No bypass was attempted; the versions above are
therefore the newest npm releases that could be verified, and the exact locked trees
built and tested offline.

## Dependency-direction and frontend boundary audit

Exact `cargo tree --depth 1` inspection showed:

```text
spielorders-core -> focaccia, serde, serde_json, toml
spielorders-cli -> spielorders-core
spielorders-desktop -> spielorders-core, tauri
```

Core has no dependency on the CLI, Tauri, React, TypeScript, Vite, or any frontend
module. The CLI and Tauri shell remain inward adapters. Source inspection found the
only frontend Tauri API import in
`apps/desktop/src/adapters/tauri-capabilities.ts`; React-facing tests substitute
`FakeCapabilities` and keep session state frontend-local. The unchanged CLI and
Tauri entry points only reference `spielorders_core::CRATE_NAME`; no adapter owns a
Package rule.

## M1 scope audit

The complete public core source and test surface remains limited to metadata codecs,
Order and Package-metadata validation, totals and normalization, safe path/layout
inspection, effect ports, and low-level validated single/multi-file replacement.
Targeted source searches plus direct inspection found:

- no Package opening/loading or discovery read model;
- no Damaged Package/Order projection or stable diagnostic catalog;
- no README loading/editing or Package Index loading, freshness, or rebuild policy;
- no Package initialization, domain mutation, persistence workflow, Metadata
  Upgrade orchestration, alignment, removal, adoption, or dry-run mutation report;
- no CLI command, Tauri command, or frontend product behavior added by M1; and
- no shared SpielGantt crate, package, or domain abstraction.

The M0 effect-port declarations for calendar, UUID, native files, and Supporting
Material search remain substitution boundaries only. The implemented filesystem
mechanics remain SpielOrders-local. M1 therefore contains none of the excluded M2,
M3, adapter, or speculative-sharing scope.

No acceptance failure required a non-integrative change to any prerequisite
implementation ticket, and the gate passes without waiver.

## Review disposition

`/code-review` used the exact Ticket-start fixed point and the acceptance-gate ticket
as its Spec source. The Standards axis found one hard traceability violation: new
human-facing prose substituted bare tracker identifiers for full names. The Spec
axis found one partial requirement: canonical scenario coverage was summarized but
not mapped explicitly to public tests. Both findings were accepted and resolved in
`a34a8c3` by using names or identifier-free prose and adding the complete scenario
coverage matrix above.

The remediation changed evidence only, not design. The schema, traceability,
frontend, 59-test workspace, 59-test independent-core, and Clippy gates all passed
again before closure. No second review run was required by the project-local review
workflow.
