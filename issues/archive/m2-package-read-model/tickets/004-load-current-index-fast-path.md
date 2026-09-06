---
title: Load a current Package Index without canonical record rereads
status: closed
ticket_start_fixed_point: 598339792a85831ea2d8daf52e13c63210eafab4
assignee: /root
claimed_at: 2026-08-20T12:11:48Z
closed_at: 2026-08-20T12:42:09Z
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

When a valid version-1 Package Index is proven current by conservative portable
no-follow change evidence, ordinary Package opening returns the complete available
projection without opening `vendors.json` or every `order.json`, while exposing
only evidence actually observed on that fast path.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Rebuild the Package Index and preserve replacement-failure evidence](003-rebuild-package-index.md).

## Scope

- Extend the existing filesystem port only with the smallest portable no-follow
  change observation needed by the closed cutoff: entry kind and comparable
  content-or-replacement evidence, or explicit uncertainty.
- Keep observations in memory only. Do not persist timestamps, identities,
  revisions, diagnostics, or a sidecar and do not claim tamper-proof detection.
- On opening, read/validate only `config.toml`, `version.json`, and `index.json`;
  resolve the configured Orders Directory; list its direct children; and observe
  exact no-follow kind/change facts for `vendors.json`, every non-hidden ordinary
  candidate's exact `order.json`, and its exact conventional `README.md`.
- Prove freshness only when the validated deterministic index's paths exactly
  match candidate membership, schema version agrees, required files are ordinary,
  and index replacement evidence is strictly later than every canonical and
  present README input. Equal, newer, missing, unavailable, incomparable, or
  contradictory evidence selects Ticket 003's canonical rebuild path.
- Source from a proven-current index only the permitted complete healthy Order and
  Vendor facts/identities, matched Order paths, ordered Items/Contacts, cached
  Order README text, totals, resolved Vendor associations, and four reusable-value
  lists.
- Keep config/version facts, configured paths, direct-child membership/warnings,
  freshness observations, and index state sourced from their own effects.
- Deliberately omit latent Vendors/Order/README/reserved/staging diagnostics and
  every unexamined damage, upgrade-execution, operation-failure, Package README,
  or Supporting Material fact. Package and Order README references remain JIT;
  cached text is not a claim of current availability.
- Return `current` / `not_needed` on success and preserve exact deterministic
  collection/absence semantics.

Exclude exhaustive validation, eagerly reading all READMEs, a recursive change
scan, filesystem watchers, hashes/revisions, adversarial tamper guarantees, schema
changes, and adapter behavior.

## Canonical fixtures and fault injection

- Reuse the healthy, direct-child, drift, README, and stale-index scenario
  foundations plus minimal/full version-1 index examples.
- Add narrow fake observations for strictly older inputs, equal evidence, each
  newer input kind, unavailable/incomparable/contradictory evidence, membership
  add/remove/rename, path mismatch, and wrong no-follow kinds.
- The public filesystem double must record every read/list/observation. A passing
  fast-path test fails if it opens `vendors.json`, any `order.json`, any README, or
  an unlisted path. Each uncertain/failing observation must visibly route to the
  existing canonical rebuild or typed operation result as decided.
- Add a portable real-filesystem observation check only to the extent needed to
  prove the port can conservatively report comparable evidence or uncertainty;
  do not make platform timestamp precision a persisted/public contract.

## Supporting Material preservation requirements

The fast path may list only the Orders Directory direct children and observe only
the exact named managed/README inputs. It must not list an Order Directory or
recurse. General files, nested directories, and links in Supporting Material may
not contribute change evidence. Tests must prove both opaque sentinel paths are
absent from effect logs and remain byte-identical.

## Core/filesystem-port and adapter boundary

The Rust core owns cutoff comparison, membership proof, uncertainty fallback,
index sourcing, totals/Vendor/value derivation, and evidence visibility. The port
supplies no-follow observations without platform strings. Production adapters may
return uncertainty; CLI, Tauri, and React do not implement or reinterpret the
cutoff and receive no new presentation behavior.

## Acceptance criteria

- A current-index load reads the three small root artifacts, lists direct children,
  performs only authorized exact observations, and opens neither `vendors.json`
  nor any `order.json` or README.
- Freshness requires strict-later comparable evidence and exact membership/path/
  kind/version/deterministic-index validity. Every conservative failure routes to
  canonical rebuild with the correct ordered stale reason.
- The resulting healthy Order/Vendor projection, totals, live associations, and
  reusable values equal canonical loading for the same facts.
- Fast results contain no latent canonical, document, reserved/staging, or
  Supporting Material evidence and never treat `index_readme_text` as a fresh JIT
  read.
- Filesystem enumeration order, source errors, and untrusted platform detail do not
  affect the result.
- Metadata version 1 and exact index bytes remain unchanged.

## Review boundary

Review the portable cutoff, exact fast-path read budget, permitted index sourcing,
and deliberate absence of unexamined evidence. Reject a sidecar, schema revision,
recursive scan, eager README reads, duplicated adapter rule, search field, or
stronger security claim.

## Exact fixed-point claim protocol

Require Tickets 001–003 closed, this ticket open/unclaimed, and an empty
`git status --porcelain=v1 --untracked-files=all`. Record the exact
`git rev-parse HEAD` as `ticket_start_fixed_point`, claim with `status: in progress`,
`assignee`, and `claimed_at`, and commit that claim alone. Review always uses the
pre-claim SHA. Stop on any mismatch.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. The first public RED test is a
strict read-budget test for a proven-current index; it catches an implementation
that silently rereads canonical records. Add one RED-to-GREEN behavior at a time
for permitted projection, each conservative cutoff fallback, exact evidence
absence, and portable/fault-injected observations. Do not batch a port/type layer
before observable behavior.

Commit all work before the project-local `code-review` skill compares the ticket
fixed point to HEAD. Resolve and record independent Standards/Spec results, rerun
affected gates, commit the disposition, and close only this ticket.

## Required verification

Focused gates:

- dedicated current-index fast-path/read-budget integration suite;
- index rebuild, canonical projection, filesystem-port, README, and path suites;
- portable observation and complete fake-failure matrix; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- core/adapter dependency and prohibited-scope audit; and
- exact before/after hashes for both opaque sentinels.

Use pinned Node and do not update dependencies or lockfiles.

## Implementation evidence (pre-review)

- Exact pre-claim fixed point:
  `598339792a85831ea2d8daf52e13c63210eafab4`.
- Claim commit: `70b0800` (`Claim M2 Ticket 004 current-index fast path`).
- Implementation commit: `985c351` (`Implement M2 current-index fast path`).
- The first public RED strict-budget test failed with an observed read sequence of
  `config.toml`, `version.json`, `vendors.json`, and `index.json`, proving that
  ordinary opening reread canonical Vendors before the Ticket 004 change. GREEN
  now reads exactly `config.toml`, `version.json`, and `index.json`; the minimal
  fast path observes exactly config, version, index, and Vendors change facts,
  lists exactly the configured Orders Directory once, and performs no write,
  stage, sync, replacement, or cleanup effect.
- The full fast-path budget adds only each discovered candidate's exact
  `.spielorders/order.json` and conventional `README.md` change observations. It
  opens neither path, does not list an Order Directory, and does not touch an
  unlisted path or either Supporting Material sentinel.
- The dedicated `package_index_fast_path` integration suite passes `15` public
  cases covering complete permitted index projection; canonical-equivalent Item
  and Order totals, Vendor association, and four reusable-value collections;
  exact membership and path proof; strict-later success; equal evidence; newer
  config, version, Vendors, Order metadata, and present README evidence; missing,
  unavailable, incomparable, uncertain, contradictory, and injected-failure
  evidence; add/remove/rename membership; every non-file no-follow input kind;
  direct-child structural warnings; enumeration independence; exact evidence
  absence; exact effect budgets; and Supporting Material preservation.
- The dedicated `filesystem_change_observation` real-filesystem suite passes and
  proves ordinary-file kind plus comparable in-memory evidence or explicit
  uncertainty, and no-follow symbolic-link classification. No timestamp, identity,
  hash, revision, diagnostic, cutoff token, or sidecar enters persisted metadata or
  the Package projection.
- Focused Ticket 003 `package_index_rebuild` (`28` tests) and complete
  `package_index_replacement_failure` suites pass unchanged, including every
  replacement/cleanup phase, exact old-index preservation, and one Orders
  Directory listing on the pre-existing uncertain path.
- Affected `canonical_package_load`, `readme_read`, `metadata_v1`,
  `package_metadata`, `order_validation`, `atomic_replacement`, and
  `package_paths` suites pass. `cargo test -p spielorders-core --locked` and
  `cargo test --workspace --locked` pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passes. Every
  touched Rust source passes `rustfmt --edition 2024 --check`.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas` passes
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels), and `npm run check:traceability` passes.
- `git diff --check 598339792a85831ea2d8daf52e13c63210eafab4...HEAD`
  passes after the implementation commit.
- The source/dependency audit leaves the core dependencies exactly `focaccia
  2.0.0`, `serde 1.0.229`, `serde_json 1.0.151`, and `toml
  1.1.4+spec-1.1.0`. No dependency, lockfile, schema, metadata-version-1 fixture,
  CLI, Tauri, React, retry, repair, M3, Ticket 005 exhaustive-validation, recursive
  scan, watcher, search, sidecar, or Supporting Material inspection change exists.
- Independent before/after sentinel hashes are identical:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

## Code review and disposition

Review began against the exact fixed point with
`git diff 598339792a85831ea2d8daf52e13c63210eafab4...HEAD` and
`git log 598339792a85831ea2d8daf52e13c63210eafab4..HEAD`. Standards and Spec
ran independently and in parallel under the project-local `code-review` skill,
then independently revalidated remediation commit
`b4831a2b21bf6756fb2873b8154aac79fdebcdcc` (`Resolve Ticket 004 review
findings`).

### Standards

- Initial outcome: no hard documented-standard violation and one accepted
  duplicated-code judgment call. The fast and canonical paths repeated the
  direct-child warning, healthy-Order projection, four reusable-value
  derivations, and complete current-candidate assembly.
- Shared `orders_directory_entry_disallowed_diagnostic`,
  `healthy_order_from_validated`, and `current_package_candidate` helpers now own
  those projections for both paths, preventing drift without splitting the
  observable load orchestration.
- Final independent Standards validation found `0` hard violations and `0`
  remaining judgment-call smells. The per-open directory snapshot wrapper has a
  material cutoff/fallback role and keeps effects behind the narrow filesystem
  port; no dependency or toolchain file changed.

### Spec

- Initial outcome: two accepted findings. Root-level stale evidence could return
  before collecting simultaneous Order/README reasons, and Supporting Material
  preservation was not asserted around every grouped fallback case.
- The cutoff now completes all available exact candidate Order/README
  observations before sorting, deduplicating, and falling back. A regression test
  proves simultaneous newer Vendors, Order metadata, and present README evidence
  yields `vendors_changed`, `order_metadata_changed`, and
  `order_readme_changed` in contract order.
- Every ordinary-open invocation in the dedicated fast-path suite now passes
  through one preservation harness that installs both opaque sentinels, compares
  their bytes before and after, and rejects either exact path in the complete
  effect log. This applies to success and every equal, newer, missing,
  unavailable, incomparable, uncertain, contradictory, membership, path, kind,
  and injected-failure case.
- The first remediation run exposed a Ticket 003 effect-budget regression: a
  stale fast probe and canonical fallback could list the Orders Directory twice.
  `PackageOpenFileSystem` now replays each successful exact directory observation
  within one open, so canonical fallback receives the same observed membership
  with one underlying list effect. Failed lists are not cached, paths are not
  conflated, and no Order Directory or Supporting Material is listed.
- Final independent Spec validation found `0` remaining findings and no scope
  creep. Its read-only focused repetition passed `package_index_fast_path`
  (`16/16`) and `package_index_rebuild` (`28/28`).

### Post-review verification

- The dedicated `package_index_fast_path` suite passes `16` tests, including the
  new simultaneous ordered-reason regression and the per-invocation sentinel
  guard. `filesystem_change_observation` passes its portable real-filesystem
  check.
- Ticket 003 `package_index_rebuild` passes `28` tests and
  `package_index_replacement_failure` passes its full matrix, including exactly
  one underlying Orders Directory listing across uncertain fast fallback and
  canonical rebuild.
- Affected `canonical_package_load`, `readme_read`, `metadata_v1`,
  `package_metadata`, `order_validation`, `atomic_replacement`, and
  `package_paths` suites pass. `cargo test -p spielorders-core --locked` and
  `cargo test --workspace --locked` pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passes, and
  every Rust source touched since the fixed point passes
  `rustfmt --edition 2024 --check`.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas` passes
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels), and `npm run check:traceability` passes.
- `git diff --check 598339792a85831ea2d8daf52e13c63210eafab4...HEAD`
  passes after remediation. The fixed-point file audit contains only core source,
  core integration/test support, and Ticket 004 milestone/evidence files; it has
  no CLI, Tauri, React, schema, metadata-version-1 fixture, dependency, lockfile,
  toolchain, repair, M3, or Ticket 005 change.
- `cargo tree -p spielorders-core --depth 1 --locked` remains exactly `focaccia
  2.0.0`, `serde 1.0.229`, `serde_json 1.0.151`, and `toml
  1.1.4+spec-1.1.0`.
- Independent post-review sentinel hashes remain byte-identical:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

## Result

Ticket 004 is complete. Ordinary Package opening can now prove a valid
deterministic metadata-version-1 Package Index current from conservative portable
no-follow evidence and return the complete healthy Package projection as
`current` / `not_needed` after opening only `config.toml`, `version.json`, and
`index.json`. It lists only the configured Orders Directory, observes only exact
named managed and conventional README inputs, and never opens Vendors, Orders,
READMEs, or Supporting Material on the successful fast path. Every ambiguity,
failure, drift, membership/path mismatch, and wrong kind conservatively composes
with Ticket 003 canonical rebuilding and exact ordered stale reasons while
preserving deterministic projection, one-list fallback behavior, operation
evidence, and both opaque sentinels. Ticket 005 exhaustive validation remains
unimplemented and unclaimed.

## Handoff requirement

After closure, generate the complete next-worker prompt for the first eligible
ticket—expected to be [Validate exhaustive managed evidence without inspecting Supporting Material](005-validate-exhaustive-managed-evidence.md)—with
all fixed-point, one-ticket, TDD/review, preservation, verification, and handoff
requirements. Report a blocker/claim rather than selecting later work.
