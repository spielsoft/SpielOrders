---
title: Integrate M2 acceptance and refresh the complete selected-major toolchain
status: closed
ticket_start_fixed_point: a97de65bd09e249991ab7969305ce7663ce3719e
assignee: /root
claimed_at: 2026-08-21T13:11:21Z
closed_at: 2026-08-21T13:21:40Z
reopened_at: 2026-08-21T12:55:39Z
reopened_by_gate: fresh-m2-external-validation
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:gate
---

## Outcome

Prove from a clean target-pinned copy that every closed M2 read-side capability
composes through the shared Rust core and filesystem port, then complete the
mandatory Rust/Cargo, Node/npm, TypeScript, direct-dependency, and compatible
locked-transitive refresh without changing M2 contracts or preservation
boundaries. Close M2 only when the refreshed target passes the complete gate and
is ready for fresh external validation.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Audit M2 decision completeness and hand off implementation decomposition](../../../wayfinder/m2-package-read-model/tickets/004-audit-m2-decision-completeness.md)
- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [M1 — Filesystem and metadata kernel](../../m1-filesystem-metadata-kernel/README.md)
- [M1 external validation](../../m1-filesystem-metadata-kernel/VALIDATION.md)
- [M1-upgrade — Toolchain and dependency modernization](../../m1-upgrade-toolchain-dependencies/README.md)
- [M1-upgrade external validation](../../m1-upgrade-toolchain-dependencies/VALIDATION.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

Unblocked. [Validate exhaustive managed evidence without inspecting Supporting Material](005-validate-exhaustive-managed-evidence.md)
and the four earlier named capability slices remain closed and integrated. This reopened lifecycle
was claimed from the exact clean post-reopen documentation fixed point before any
remediation or refreshed gate work; the frontmatter records that fixed point,
assignee, and claim time.

Fresh external validation on 2026-08-21 reopened this ticket after the independent
Spec review found that the
[current-index fast-path](004-load-current-index-fast-path.md) row in
`ACCEPTANCE.md` does not record the exact review-disposition commit. The prior
integrated-acceptance claim, implementation, review, and closure sections below
remain the historical record of the first closure. At reopen, this capability was
the first unblocked, unclaimed owner of the
evidence correction, complete refreshed gate, independent review, and reclosure.
It was then claimed, passed the refreshed gate and review, and reclosed.

## Scope

- Pin and inventory the exact completed pre-gate M2 target, the complete M2 diff,
  every ticket resolution/review, and the original externally validated M1/
  M1-upgrade baselines.
- Run integrated public-core acceptance for every load result: unmanaged,
  available, diagnostic-only Damaged Package, upgrade-pending symbolic reporting,
  blocking index-replacement-failed, and general operation-failed.
- Prove complete healthy and damaged projections, every duplicate-ID participant,
  stable diagnostics/grouping, exact README reads and version-1 cache semantics,
  all index states/outcomes/reasons, deterministic healthy-only rebuild,
  replacement failure, fast-path read budget/sourcing/absent evidence, exhaustive
  validation, and complete filesystem-port fault injection.
- Begin dependency currentness from the frozen M1-upgrade drift inventory:
  `@types/react` 19.2.18, `@types/react-dom` 19.2.4, `fast-uri` 3.1.5, and optional
  `@napi-rs/wasm-runtime` 1.2.2 were the compatible-current targets recorded
  against selected versions 19.2.17, 19.2.3, 3.1.4, and 1.2.1.
- Refresh authoritative release information at ticket start; the frozen list is a
  starting point and may be stale. Refresh Rust/Cargo, Node/npm, TypeScript, every
  direct dependency, and every compatible locked transitive to the newest minor or
  patch in its selected major. Adopt such compatible releases automatically under
  `AGENTS.md`.
- Stop and ask the user before adopting any new major. If an in-major update is
  blocked, record the blocker, authoritative source, selected version, newest
  verified compatible version, selector, and impact; never silently leave it
  behind.
- Preserve AJV and Vite selections unless a separately approved major decision
  changes them. Do not add Mantine merely for currency and do not grant
  `allowScripts`, trusted dependency, approval, or any other install-script
  authority.
- Recreate both npm trees and Rust build artifacts in a fresh target-pinned
  no-hardlink copy; run all complete acceptance and audit commands below. Record
  exact commands, versions, sources, hashes, counts, read budgets, dependency
  graphs, failures/dispositions, and scope evidence in M2 acceptance documentation.
- Return any non-integrative M2 behavior defect to its smallest owning ticket and
  keep this gate open. Resolve refresh-only compatibility work in this ticket using
  observable regressions where behavior changes; do not absorb M3 or adapter work.
- Only after all gates and fixed-point review pass, close this ticket, mark the
  milestone complete, and update the implementation roadmap to say M2 is complete
  and awaiting fresh external validation.

Exclude M3 behavior, CLI/Tauri/React features, new product scope, new metadata
versions, new major versions without approval, install-script authority, and a
shared SpielGantt foundation.

## Canonical fixtures and fault injection

- Consume the complete canonical corpus in place and independently recount schemas,
  accepted/rejected documents, scenarios, and opaque sentinels.
- Exercise every narrowly added M2 recipe and the complete public filesystem-port
  observation/read/list/staging/durability/replacement/cleanup failure matrix.
- Use a representative multi-Order current Package with healthy Orders, every
  Damaged Order family, duplicate-ID conflict sets, Vendors/Contacts/Items, all four
  reusable values, exact READMEs, structural/staging warnings, and current/stale/
  uncertain index observations.
- Do not add fixtures merely to mirror a private type or helper. Do not invent an
  older/future schema to make upgrade variants executable.

## Supporting Material preservation requirements

The full gate must prove fast load, canonical rebuild, exhaustive validation,
README reads, index replacement success, and every injected failure never list
general Order Directory contents or inspect/recurse through Supporting Material.
Both existing binary sentinels must remain byte-identical to the hashes recorded in
M1 and M1-upgrade external validation before and after dependency recreation,
builds, tests, smoke checks, and fault injection.

## Core/filesystem-port and adapter boundary

The integrated API must leave all Package rules in `spielorders-core`; the existing
filesystem port remains the sole effect boundary with only the portable no-follow
change observation added by
[Load a current Package Index without canonical record rereads](004-load-current-index-fast-path.md).
Dependency trees and source audits must
prove CLI and Tauri depend on core, never the reverse, and React sees no M2 rule.
No CLI envelope/command, Tauri command, React state, retry presentation, or platform
error contract is part of this gate.

## Acceptance criteria

- Every M2 ticket is closed, independently reviewed from its exact fixed point, and
  its Standards and Spec findings are resolved/recorded.
- Integrated tests prove all six load-result families and the exact separation of
  diagnostics, index assessment, refresh outcome, and operation failures.
- A proven-current index opens without reading `vendors.json`, any `order.json`, or
  README; its projection matches canonical loading and deliberately omits
  unexamined evidence.
- Canonical fallback/rebuild bytes are deterministic, healthy-only, version 1, and
  unchanged in shape. Damaged placeholders/diagnostics remain outside the index.
- Every replacement failure preserves the old index, original assessment, healthy
  canonical evidence, exact phase/cleanup evidence, and blocks ordinary opening
  without false Package damage.
- Exhaustive validation finds all and only authorized broader evidence and never
  retroactively changes a fast-load result.
- Rust/Cargo, Node/npm, TypeScript, all direct dependencies, and compatible locked
  transitives are current within selected majors. Any verified in-major blocker is
  reported with the newest verified version and keeps this ticket and milestone
  open. No unapproved major or install-script authority is present.
- Every clean-install, build, test, lint, schema, traceability, audit, Tauri,
  development-smoke, preservation, dependency-direction, boundary, and prohibited-
  scope gate passes in the fresh target copy.
- Both sentinels match the externally validated hashes and no schema, fixture
  meaning, metadata version, M3 behavior, adapter behavior, Mantine dependency, or
  shared SpielGantt code has entered M2.

## Review boundary

Review integrated M2 correctness/evidence and the complete compatible refresh.
This gate is not a catch-all product ticket. Return owned behavior failures to the
smallest earlier M2 ticket; do not reinterpret failures, grant authority, adopt a
major, or add later-milestone behavior to make the gate pass.

## Exact fixed-point claim protocol

Require Tickets 001–005 closed, this ticket open/unclaimed, and a completely empty
`git status --porcelain=v1 --untracked-files=all`. Record exact `git rev-parse HEAD`
as `ticket_start_fixed_point`, add `status: in progress`, `assignee`, and
`claimed_at`, and commit only the claim before refreshing releases or artifacts.
Use the pre-claim SHA for the final review. Stop on any disagreement.

## Implement, TDD, and code-review workflow

Read and use the project-local `implement` skill. Use the project-local `tdd` skill
for any refresh compatibility change or integrated behavior defect: one public
RED-to-GREEN tracer bullet at a time, with the defect stated. Dependency text or
lock-graph changes alone do not justify synthetic RED tests. Return pre-existing
M2 behavior defects to the owning ticket rather than fixing them here.

Commit the complete refresh and acceptance evidence before invoking the project-
local `code-review` skill against `<ticket_start_fixed_point>...HEAD`. Run Standards
and Spec independently and in parallel, resolve/disposition every finding, rerun
all affected gates in the clean target copy, commit the outcome, and only then
close this ticket and milestone.

## Required verification

Run under the refreshed repository-selected toolchains in a fresh target-pinned
no-hardlink copy. At minimum record and pass:

- empty source/clone status, exact ancestry/range/log/path inventory, and
  `git diff --check <ticket_start_fixed_point>...HEAD`;
- fresh `npm clean-install` and `npm --prefix apps/desktop clean-install`, followed
  by a registry-free/offline repetition from the refreshed lockfiles;
- `cargo build --workspace --locked` from an empty target;
- `cargo test --workspace --locked` and
  `cargo test -p spielorders-core --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- strict desktop TypeScript checking, frontend tests/build, and
  `npm --prefix apps/desktop run tauri build -- --no-bundle`;
- `npm run test:schemas` and `npm run check:traceability`;
- bounded `npm --prefix apps/desktop run tauri dev -- --no-watch` development smoke
  with independent HTTP/process evidence and clean shutdown;
- `cargo update --dry-run --verbose`, locked metadata, direct/workspace/duplicate/
  inverted dependency trees, authoritative direct-crate currentness, and transitive
  selector explanations;
- root and desktop `npm ls --all`, `npm outdated --all --json`,
  `npm audit --audit-level=low`, `npm install-scripts ls`, authoritative registry
  currentness, and `npm explain` for every behind-latest selection;
- independent schema/example/rejected-fixture/scenario/test/sentinel counts;
- focused integrated M2 suites for exact README, canonical load/projection, index
  rebuild/failure, current-index read budget, exhaustive validation, and all port
  faults; and
- explicit metadata-version-1 byte/shape, Supporting Material, core/adapter,
  install-script-authority, M3/adapter/search/Mantine/shared-crate, and complete scope
  audits.

Any sandbox/network denial must be rerun authoritatively when permitted and
recorded; it is not silently treated as a pass. No failed or stale earlier gate may
substitute for this complete rerun.

## Handoff requirement

When this ticket and the milestone close, stop the M2 implementation chain. Hand
the fresh validation-only worker the milestone path, original fixed point, exact
completed target, permitted documentation writes, and prohibition on M3 work.
The worker reads verification requirements from this ticket and `ACCEPTANCE.md`;
do not duplicate them in a generated prompt.

## Implementation evidence (pre-review)

- Fixed-point claim: `866f619414e57aa816184509ed8e57209c8ea62f`.
- Claim commit: `ee1584966614d62d1850ecaf1c23fd6fdd770d5b` (isolated claim for
  integrated M2 acceptance and selected-major refresh).
- Refresh commit: `51dcbe1c033c1ad4f5d9201328f4ba008be89624` (`Refresh M2
  selected-major toolchains and dependencies`).
- Rust/Cargo moved from 1.97.1 to current 1.98.0; Node moved from 26.5.1 to
  current 26.7.0. npm 12.0.2 and TypeScript 7.0.2 remain current.
- Direct npm refreshes are Node types 26.2.0, React types 19.2.18, ReactDOM types
  19.2.4, Vite React plugin 6.1.0, and Vite 8.2.2. Compatible lock refreshes
  include `fast-uri` 3.1.5 and Rolldown 1.2.5. All direct Rust crates remain
  current; 59 compatible Cargo lock entries advanced.
- No product or adapter behavior changed. Under the TDD skill, dependency and lock
  changes alone do not justify a synthetic RED test; the unchanged public behavior
  passed the complete integration suite.
- Fresh no-hardlink acceptance ran detached at the refresh commit in
  `/private/tmp/spielorders-m2-006.Idg47j/repo`. Both online and offline npm clean
  installs, empty-target Rust build, 156 workspace tests, core-only tests, 94
  focused M2 tests, clippy, schemas, traceability, two frontend tests, production
  frontend build, Tauri no-bundle release build, and bounded Tauri development
  smoke passed.
- Both npm audits report zero vulnerabilities and the install-script audits grant
  no lifecycle authority. A desktop `fsevents` 2.3.3 script explicitly blocked
  for lack of `allowScripts` is passing denial evidence, not granted or unreviewed
  authority. Cargo and npm behind-latest records are fully explained by upstream
  selectors or absent platform/peer-optionals; no override or new-major authority
  was added.
- The canonical corpus independently contains 4 schemas, 8 accepted examples, 19
  rejected fixtures, 22 scenarios, and 2 sentinels. The sentinels retained the M1/
  M1-upgrade external-validation hashes before and after the full gate.
- Complete commands, versions, sources, lock hashes, dependency trees, read/effect
  budgets, fault coverage, failed-probe disposition, scope audits, and preservation
  evidence are recorded in [M2 integrated acceptance](../ACCEPTANCE.md).
- Pre-review outcome: every acceptance gate passes, no M2 behavior defect was
  found, and no M3, schema, contract, adapter, Mantine, SpielGantt, or install-script
  scope entered the range.

## Code review and disposition

The project-local `code-review` skill reviewed the fixed range beginning at
`866f619414e57aa816184509ed8e57209c8ea62f`. Standards and Spec ran
independently and in parallel against pre-review evidence commit
`5eacd7fcbb7d927e87b13103d5a417291eacb21f`.

### Standards

- Initial outcome: one P2 hard traceability-language finding and no baseline smell
  finding. Bare tracker identifiers in the new acceptance record substituted for
  meaningful linked decision/capability names.
- Accepted and remediated in `8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6`
  (integrated-acceptance review-evidence remediation).
- Final independent revalidation found zero remaining hard violation and zero new
  or remaining smell.

### Spec

- Initial outcome: two P2 evidence findings. Exact prior-slice lifecycle/review
  commits were not inventoried, and port-fault/version-1 byte-shape evidence lacked
  a complete operation matrix and exact output.
- The lifecycle inventory and fault matrix were accepted and remediated in
  `8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6`. Recheck found only the need
  to reproduce all ten exact artifact key arrays and hashes.
- The last evidence gap was remediated in
  `08b8309fd1e0d166816d9d02c34318857955f66e` (completed integrated-
  acceptance version-one evidence). Final independent recheck found no remaining
  or new Spec finding.

### Post-review verification

- A fresh no-hardlink copy detached at the first remediation passed all 94 focused
  M2 tests, schema and traceability checks, exact metadata-version-1 key/version/
  hash audit, fixed-point whitespace check, clean-status guard, and both sentinel
  hashes.
- A third fresh no-hardlink copy detached at the final remediation reproduced all
  ten exact key arrays and hashes, ten version assertions, both sentinel hashes,
  fixed-point whitespace, and empty status.
- No code, contract, fixture, dependency, lockfile, adapter, or product behavior
  changed during review remediation. Every Standards and Spec finding is resolved;
  closure is now eligible.

## Result and closure

M2 integrated acceptance is complete. The refreshed selected-major toolchains and
dependency graphs pass every required clean-copy gate, all six public load-result
families and the complete read-side/fault/preservation boundaries compose through
`spielorders-core`, and the fixed-point Standards and Spec review has no unresolved
finding.

Exact lifecycle before closure:

- Claim: `ee1584966614d62d1850ecaf1c23fd6fdd770d5b`.
- Refresh: `51dcbe1c033c1ad4f5d9201328f4ba008be89624`.
- Pre-review acceptance evidence: `5eacd7fcbb7d927e87b13103d5a417291eacb21f`.
- Review remediation: `8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6` and
  `08b8309fd1e0d166816d9d02c34318857955f66e`.
- Review disposition: `345f0fa007722b6952be884f704159fccf45dccb`.
- Closure: `3edd745c002805a668cd33d8a5d89ea6a46009f3`.

The closure commit is the ticket, milestone, acceptance, and roadmap transition
identified above. This follow-up records its exact SHA without changing product
behavior or widening the milestone.
M2 stops here and awaits fresh validation-only external reproduction. No M3 work
has begun.

## External-validation reopen

Fresh validation-only external reproduction started from the original
integrated-acceptance fixed point `866f619414e57aa816184509ed8e57209c8ea62f`
and exact completed target
`f9ff9ad901438dc88c88e26136cebd3fabe418ce` in a clean no-hardlink detached copy.
All executable, currentness, count, preservation, architecture, and prohibited-
scope gates passed. The project-local code-review skill's Standards axis passed
with zero findings, but the independent Spec axis reported one P2 evidence
finding:

- `issues/implementation/m2-package-read-model/ACCEPTANCE.md` records the
  [current-index fast path](004-load-current-index-fast-path.md)'s review
  disposition as `Recorded with remediation` rather than the exact commit.
  Remediation commit `b4831a2b21bf6756fb2873b8154aac79fdebcdcc`
  changes only Rust source and tests. The current-index fast path's final
  Standards/Spec disposition was recorded in closure commit
  `ecd853b08009b1e1a946050b9594f482949bd3b7`, so the lifecycle table must record
  that exact disposition commit and note that disposition and closure coincide.

The validation worker made no repository, implementation, dependency, lockfile,
contract, roadmap, or validation-status change. The integrated M2 acceptance and
selected-major refresh capability is the smallest owner because its scope and
acceptance criteria require an exact inventory of every
prior ticket resolution and review. The reopened worker must correct only the
owned evidence defect, independently refresh all authoritative release data under
`AGENTS.md`, rerun the complete target-pinned integrated acceptance gate, run
Standards and Spec independently and in parallel, and close the integrated
acceptance capability and M2 only if every result passes. Reclosure must hand off a
new exact completed target to a fresh
validation-only external worker. M3 remains blocked.

## Reopened implementation evidence (pre-review)

- Reopen fixed point: `a97de65bd09e249991ab7969305ce7663ce3719e`.
- Isolated claim: `60e4afb7732ff11976ee1ba7b4d7618cc674a0e4`.
- Evidence correction and refreshed-currentness candidate:
  `06c5198741b682297865c6d1b8477e85cea834c2`.
- The [current-index fast-path](004-load-current-index-fast-path.md) lifecycle row
  now records exact review disposition and closure
  `ecd853b08009b1e1a946050b9594f482949bd3b7`; Rust remediation
  `b4831a2b21bf6756fb2873b8154aac79fdebcdcc` is separately and accurately scoped.
- Authoritative Rust, Node, npm, and crates.io release data was refreshed on
  2026-08-21. Every direct selection is current in its selected major, and every
  behind-latest transitive is selector-, target-, platform-, or different-major
  constrained. No manifest or lockfile changed.
- A fresh no-hardlink clone detached at the candidate passed online/offline npm
  recreation, empty-target Rust build, 156 workspace tests, core repetition, 94
  focused M2 tests, clippy, schemas, traceability, frontend tests/build, Tauri
  no-bundle build, and bounded HTTP/process/shutdown development smoke.
- Both npm audits are clean and the install-script audits grant no lifecycle
  authority; explicitly blocked desktop `fsevents` 2.3.3 confirms that
  `allowScripts` remains absent. Exact dependency trees prove core/adapter
  direction and explain `generic-array`, TOML-family, `fast-uri`, Nano ID,
  `undici-types`, and Rolldown selections without overrides.
- Independent counts, all ten version-1 shape/hash assertions, both sentinel
  hashes, all six load-result families, port-fault coverage, read/effect budgets,
  and Supporting Material preservation reproduced exactly. The detached clone is
  clean and the reopen range changes only this ticket, the milestone index, and
  the integrated acceptance record.
- No implementation or compatibility defect exists, so no synthetic TDD test was
  added for the documentation-only evidence correction.

## Reopened code review and disposition

The project-local `code-review` skill reviewed the fixed range from
`a97de65bd09e249991ab7969305ce7663ce3719e` after pre-review evidence commit
`9db286ba49897e82fbccd88cad80c01881e817ab`. Standards and Spec ran independently
and in parallel.

### Standards

- Initial outcome: three P2 hard stale-state documentation violations and zero
  Fowler-baseline smells. Milestone, ticket, and acceptance prose contradicted the
  current claimed and gate-passed state.
- Remediation `90e8f25dd001ec9ec140c06e856c62765e4280b7` distinguishes
  historical pre-claim/original-review state from the reopened current state.
- Independent revalidation found zero remaining or new hard violation, zero
  smell, and a clean whitespace diff.

### Spec

- Initial outcome: two overlapping P2 documentation-state contradictions, no
  scope creep, and confirmation that the
  [current-index fast-path](004-load-current-index-fast-path.md) lifecycle
  correction is exact.
- The same remediation resolves the stale gate/review fixed point and claim-state
  contradictions.
- Independent revalidation found zero remaining or new missing/partial
  requirement, scope creep, or incorrect implementation. Both axes pass without
  waiver, which made reclosure eligible.

## Reopened result and reclosure

The owned lifecycle-inventory defect is corrected, authoritative selected-major
currentness is refreshed, the complete detached no-hardlink gate passes, and the
fixed-point Standards and Spec review passes after all initial findings were
resolved in `90e8f25dd001ec9ec140c06e856c62765e4280b7`. The review disposition is
`51a650466d19924b4c03ea512967234380d1984c`. The integrated M2 acceptance and
selected-major refresh capability and M2 reclose without waiver in closure
transition `e280145ae8eff2b03a856562aa2338b0cebd263f`
and await a new validation-only external reproduction. No M3 work begins.

That external reproduction subsequently passed at exact target
`18101a3c419d6794f7768b99820ac2d1976a836d`; see
[M2 external validation](../VALIDATION.md).
