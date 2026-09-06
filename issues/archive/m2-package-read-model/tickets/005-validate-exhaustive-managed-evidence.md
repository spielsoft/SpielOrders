---
title: Validate exhaustive managed evidence without inspecting Supporting Material
status: closed
ticket_start_fixed_point: ecd853b08009b1e1a946050b9594f482949bd3b7
assignee: /root
claimed_at: 2026-08-20T17:18:51Z
closed_at: 2026-08-21T11:34:30Z
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

A Rust caller can explicitly validate all M2-governed canonical artifacts, exact
conventional READMEs, cross-record rules, and immediate reserved metadata contents
and receive one complete deterministic evidence report, without changing prior
fast-load evidence or inspecting Supporting Material.

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

Blocked by [Load a current Package Index without canonical record rereads](004-load-current-index-fast-path.md).

## Scope

- Expose exhaustive validation as a separate public Rust-core operation. It must
  not retrofit broader evidence into a prior ordinary Package load result.
- Read every governed root canonical artifact, every discovered candidate's exact
  `order.json`, and exact Package/Order READMEs; apply all local and cross-record
  rules regardless of whether ordinary opening previously used the fast path.
- List only the immediate entries of the root `.spielorders` directory and each
  discovered Order's immediate `.spielorders` directory to diagnose unexpected
  reserved content and recognizable beside-destination staging artifacts.
- Complete the stable diagnostic code/evidence vocabulary with
  `reserved_content_unexpected` and `staging_artifact_present`, including canonical
  destination validity when already known. Preserve observed content; never
  promote, clean, move, or rewrite staging evidence.
- Return every Package, Order, and document diagnostic supported by the exhaustive
  operation in the exact grouping, severity, deduplication, scope, and ordering
  contract. Package Index state/refresh and operation failure remain separate typed
  records rather than diagnostic groups.
- Preserve the exact relationship between validation invalidity and ordinary use:
  any Damaged Order makes exhaustive Package validation invalid, while healthy
  Orders in an otherwise current Package remain available outside this validation
  report; warnings never create damage.
- Inject and map every read/list/observe failure through the filesystem port. When
  an effect cannot establish a validation result, return the typed operation
  failure without fabricating a canonical diagnostic; persistently observed
  required-artifact failures remain diagnostics exactly once.

Exclude repair cases/actions, raw correction, staging promotion/cleanup, index
replacement solely for validation, native/search behavior, and all adapter
presentation.

## Canonical fixtures and fault injection

- Reuse every applicable canonical scenario: damaged Package/Order, direct-child
  warnings, unexpected reserved content, README absence/unavailability, duplicate
  identities/names, unresolved Vendor Reference, staging artifact, unsupported
  metadata, current no-op, and both preservation assets.
- Add only narrow multi-condition recipes needed to prove exhaustive grouping,
  exact duplicate elimination, complete duplicate-ID participant evidence,
  deterministic order independent of filesystem enumeration/source messages, and
  staging-plus-invalid-destination coexistence.
- The public filesystem double must inject canonicalize, kind observation,
  directory-list, and exact read failures at every authorized boundary and log all
  effects. No test may use a recursive helper whose behavior exceeds the port
  contract.

## Supporting Material preservation requirements

Exhaustive means exhaustive only inside SpielOrders ownership. The operation must
never list a general Order Directory, recurse below it, inspect a Supporting
Material link/permission/name, inventory content, or use it as diagnostic evidence.
Immediate listing of an Order `.spielorders` directory is authorized; sibling or
nested general content is not. Tests must preserve both existing opaque binary
sentinels byte-for-byte and prove neither was observed.

## Core/filesystem-port and adapter boundary

The core owns breadth, stable evidence, diagnostic grouping/order, validation
validity, and damage semantics. The filesystem port supplies only exact bytes,
no-follow facts, authorized immediate listings, and opaque errors. No CLI validate
envelope, Tauri command, React diagnostic workspace, repair action, or platform
taxonomy is created.

## Acceptance criteria

- Exhaustive validation reports all supported canonical, cross-record, exact
  README, immediate reserved-content, and staging diagnostics in one deterministic
  result.
- Package, document, and each lexically ordered Order group follow the closed
  grouping contract; errors precede warnings and exact duplicates collapse without
  collapsing per-Order duplicate-ID evidence.
- Derived-index assessment/reasons/refresh and operation failures stay outside
  diagnostic groups.
- Fast-load evidence remains unchanged and intentionally narrower before and after
  an exhaustive call.
- Missing READMEs remain nondiagnostic empty documents; unavailable documents warn
  without damaging their owners; warnings do not block ordinary availability.
- Effect logs prove only authorized immediate reserved listings and exact managed/
  README reads occur, with zero general Order Directory listing or recursion.
- No validation path mutates any byte, staging artifact, canonical file, README, or
  Supporting Material.

## Review boundary

Review the separately invoked broader-evidence operation, its complete stable
diagnostics, and its strict inspection boundary. Reject repair execution, recursive
validation, general inventory, retroactive fast-load evidence, adapter behavior,
or a speculative diagnostic/recovery framework.

## Exact fixed-point claim protocol

Require Tickets 001–004 closed, this ticket open/unclaimed, and an empty
`git status --porcelain=v1 --untracked-files=all`. Record exact `git rev-parse HEAD`
as `ticket_start_fixed_point`, add `status: in progress`, `assignee`, and
`claimed_at`, and commit the claim alone. Use that pre-claim SHA for review and
stop on any inconsistency.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. Begin with one public RED test
showing a reserved warning that ordinary fast load deliberately lacks but
exhaustive validation finds without scanning Supporting Material. Add one
RED-to-GREEN behavior at a time for breadth, grouping/order/deduplication, README
evidence, combined damage/warnings, and all port failures. Each test states the
observable defect it catches.

Commit the completed behavior before using the project-local `code-review` skill
against the exact fixed point. Resolve and separately record Standards and Spec,
rerun affected gates, commit the disposition, and close only this ticket.

## Required verification

Focused gates:

- dedicated exhaustive-validation/evidence-boundary integration suite;
- canonical load, fast path, README, diagnostics, index, paths, and filesystem-port
  suites affected by composition;
- complete authorized-boundary failure matrix; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- core/adapter, Supporting Material, mutation/repair, and M3 scope audits; and
- exact before/after hashes for both opaque sentinels.

Use pinned Node and do not update dependencies or lockfiles.

## Implementation evidence (pre-review)

- Exact pre-claim fixed point:
  `ecd853b08009b1e1a946050b9594f482949bd3b7`.
- Claim commit: `2a8959d` (`Claim M2 Ticket 005 exhaustive validation`).
- Implementation commit: `5d7a225` (`Implement M2 exhaustive managed validation`).
- Public RED-to-GREEN tracer bullets began with the separately invoked reserved-
  content warning that a proven-current ordinary fast load intentionally lacks,
  then advanced one observable behavior at a time through the exhaustive result
  and validity contract, exact Package and Order README evidence, Package and
  Order immediate reserved listings, recognizable staging evidence and known
  valid/invalid destinations, absent versus unavailable documents, exact warning
  deduplication, combined Package/Order/document damage, every duplicate-ID
  participant, deterministic Package/document/Order grouping, errors before
  warnings, exact effect budgets, fast-load non-retroactivity, every authorized
  canonicalize/kind/list failure, persistently observed required-artifact reads,
  enumeration/source-detail independence, Package-damage continuation, zero
  mutation, and real-filesystem Supporting Material preservation.
- `cargo test -p spielorders-core --test exhaustive_validation --locked`: pass,
  `11` public integration tests. The complete conservative failure matrix covers
  `3` canonicalize locations, `11` authorized no-follow kind observations, `3`
  authorized immediate directory listings, `4` required canonical read failures,
  and both exact README read failures with exact typed operation/diagnostic paths.
- The healthy one-Order effect-budget case reads exactly once each:
  `config.toml`, `version.json`, `vendors.json`, the exact `order.json`, Package
  `README.md`, and Order `README.md`; lists exactly the Orders Directory, root
  `.spielorders`, and that Order's `.spielorders`; and performs zero stage, write,
  sync, replace, remove, general Order Directory listing, recursion, or Supporting
  Material effect. Canonical inspection caches the already-read healthy Order
  README only inside the exhaustive call, so ordinary fast-path evidence remains
  deliberately narrower.
- Deterministic evidence covers opposite Order enumeration, Package first then
  Package README, lexical Order then adjacent Order README groups, errors before
  warnings, stable diagnostic/evidence ordering, exact duplicate collapse, and
  retained per-Order duplicate-ID diagnostics with the same complete lexical
  conflict set.
- Combined validity evidence proves every Damaged Order and every Package error
  invalidates exhaustive validation; healthy Orders remain available through the
  separate ordinary load operation; warnings, unavailable READMEs, unexpected
  reserved content, and staging artifacts never create damage; and absent READMEs
  remain nondiagnostic empty documents.
- A proven-current fast result compared equal before and after exhaustive
  validation and retained no reserved/staging/document evidence. Ticket 004's
  `package_index_fast_path` (`16` tests), `package_index_rebuild` (`28` tests), and
  complete replacement-failure suite pass unchanged, preserving the exact
  three-read fast path and one-list fallback behavior.
- `cargo test -p spielorders-core --locked`: pass (`154` tests total, including
  all canonical load, README, Package Index, metadata, Package, Order, path,
  filesystem observation, and atomic/multi-file replacement suites).
- `cargo test --workspace --locked`: pass; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`: pass; every touched Rust source passes
  `rustfmt --edition 2024 --check`.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas`: pass
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels), and `npm run check:traceability`: pass.
- `git diff --check ecd853b08009b1e1a946050b9594f482949bd3b7...HEAD`:
  pass after the implementation commit.
- Dependency/source audit retains core's direct tree exactly as `focaccia 2.0.0`,
  `serde 1.0.229`, `serde_json 1.0.151`, and `toml 1.1.4+spec-1.1.0`. The range
  contains only shared-core source/tests and Ticket 005 claim/evidence; it adds no
  CLI/Tauri/React behavior, adapter rule, dependency, lockfile, schema,
  metadata-version-1 fixture, repair/mutation plan, recursive scan, watcher,
  search, Ticket 006, or M3 work. Write-shaped methods remain only pre-existing
  filesystem-port delegations used by ordinary index replacement, never by
  exhaustive validation.
- Independent sentinel hashes before implementation and after every pre-review
  gate are identical: `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

## Code review and disposition

Review ran from the exact fixed point with
`git diff ecd853b08009b1e1a946050b9594f482949bd3b7...HEAD` and
`git log ecd853b08009b1e1a946050b9594f482949bd3b7..HEAD`. Standards and Spec
ran independently and in parallel under the project-local `code-review` skill,
then independently revalidated remediation commit
`178462e872a0394198338b89f0cad15eb0177ca0` (`Remediate Ticket 005 review
findings`).

Review disposition commit:
`0c42ef3fef0ce1c6bb977a24604fcf269f15ccd9` (`Record Ticket 005 review
disposition`). The closure commit is this ticket/index status transition and is
recorded by exact SHA in the immediately following closure-evidence commit.

### Standards

- Initial outcome: no hard documented-standard violation, one moderate
  Feature-Envy/Divergent-Change judgment finding, and one low Data-Clump/
  Divergent-Change judgment finding. Validation-only README/path cache state in
  public `HealthyOrder` and `DamagedPackageProjection` changed derived equality
  and debug output; reserved inspection also suppressed `too_many_arguments` and
  mixed specification, observation, interpretation, and group mutation.
- Remediation removed all validation-only state from public projections. The
  private `ExhaustiveInspectionEvidence` sidecar carries discovered Order paths,
  and exhaustive canonical inspection defers exact README evidence to its one
  exhaustive document pass. `ReservedDirectorySpec` now aggregates the inspection
  contract while observation and diagnostic-group merging remain separate.
- Final independent Standards validation found `0` hard violations and `0`
  remaining judgment-call smells. Traceability markers, dependency direction,
  and the filesystem-port boundary remain intact.

### Spec

- Initial outcome: two P1 and two P2 findings. A healthy Order later quarantined
  for duplicate ID could have its exact README read twice; validation-only public
  state violated ordinary-result evidence isolation; non-UTF-8 reserved names
  were silently skipped; and the failure matrices did not independently seed and
  guard both opaque sentinels.
- Exhaustive canonical inspection now records only private candidate paths and
  leaves every Package/Order README for the single exhaustive document pass. A
  duplicate-participant regression proves one exact README read, and absent versus
  empty README evidence no longer changes ordinary public Order equality.
- A non-UTF-8 immediate reserved entry now conservatively returns the existing
  typed `list_directory` operation failure at the reserved directory because a
  complete stable managed location cannot be represented. Its Unix integration
  test prevents incomplete success.
- The canonicalize/kind/list matrix and exact canonical/README read cases now seed
  both sentinels and apply one shared before/effect/after guard. Each exact read
  failure is attempted once, neither sentinel path is observed, and bytes remain
  unchanged.
- Final independent Spec validation found `0` remaining findings and no scope
  creep. Residual risk is limited to platform filesystem behavior beyond the
  exercised Unix non-UTF-8 case.

## Post-review verification

- `cargo test -p spielorders-core --test exhaustive_validation --locked`: pass,
  `13/13`, including the four review regressions and the complete conservative
  boundary-failure/preservation matrix.
- `cargo test -p spielorders-core --locked`: pass, `156` tests total. Ticket 004's
  `package_index_fast_path` remains `16/16`, `package_index_rebuild` remains
  `28/28`, and the complete replacement-failure suite passes unchanged.
- `cargo test --workspace --locked`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `rustfmt --edition 2024 --check` for every Rust file touched from the fixed
  point: pass.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas`: pass
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels); `npm run check:traceability`: pass.
- `git diff --check ecd853b08009b1e1a946050b9594f482949bd3b7...HEAD`:
  pass. The fixed-point file audit contains only shared-core source/tests and
  Ticket 005 milestone/evidence files. It has no CLI, Tauri, React, schema,
  metadata-version-1 fixture, dependency, lockfile, toolchain, repair/mutation,
  Ticket 006, or M3 change.
- `cargo tree -p spielorders-core --depth 1 --locked` remains exactly `focaccia
  2.0.0`, `serde 1.0.229`, `serde_json 1.0.151`, and `toml
  1.1.4+spec-1.1.0`.
- Independent post-review sentinel hashes remain byte-identical:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

## Result

Ticket 005 behavior is complete and review-clean. A caller can separately invoke
exhaustive managed-evidence validation to obtain deterministic Package, exact
document, lexical Order, reserved-content, and staging evidence with closed
validity/damage semantics and typed conservative failures. Ordinary and proven-
current fast loads retain their narrower evidence; validation performs no writes,
never lists a general Order Directory or recurses, and neither observes nor
changes Supporting Material. Ticket 006 integration and toolchain refresh remain
unimplemented and unclaimed.

## Exact commit record

- Claim: `2a8959d5496978b48e14210a5a7811b9495aaf76` (`Claim M2 Ticket 005
  exhaustive validation`).
- Implementation: `5d7a225d57b7e7daa402d4ac40bd8b3b96f799b5` (`Implement M2 exhaustive
  managed validation`).
- Pre-review evidence: `629ac14e7ffcf5e9e6324bf4cae318f5454ab977` (`Record Ticket 005
  pre-review evidence`).
- Remediation: `178462e872a0394198338b89f0cad15eb0177ca0` (`Remediate Ticket 005
  review findings`).
- Review disposition: `0c42ef3fef0ce1c6bb977a24604fcf269f15ccd9` (`Record Ticket 005 review
  disposition`).
- Closure: `1ee856a490572405eac3fb46df483ce8fbf558f9` (`Close M2 Ticket 005
  exhaustive validation`).

## Handoff requirement

After closure, generate the complete next-worker prompt for the first eligible
ticket—expected to be [Integrate M2 acceptance and refresh the complete selected-major toolchain](006-integrate-m2-acceptance-and-refresh.md)—with
the exact clean fixed-point protocol, one-ticket rule, applicable TDD, full gate,
refresh/approval/install-script constraints, review, preservation, and external-
validation handoff. Report a blocker/claim rather than skipping it.
