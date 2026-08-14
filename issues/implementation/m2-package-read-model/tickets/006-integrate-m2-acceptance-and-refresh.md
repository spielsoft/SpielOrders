---
title: Integrate M2 acceptance and refresh the complete selected-major toolchain
status: open
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

Blocked by [Validate exhaustive managed evidence without inspecting Supporting Material](005-validate-exhaustive-managed-evidence.md).
Tickets 001 through 005 must all be closed and integrated before this gate is
claimed.

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
change observation added by Ticket 004. Dependency trees and source audits must
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

When this ticket and the milestone close, stop the M2 implementation chain. Produce
a complete ready-to-paste prompt for a fresh validation-only worker to inspect the
original M2 fixed point and exact completed target, reproduce every command and
independent count/currentness/preservation/boundary audit above, and use the
project-local `code-review` skill against the complete range. The prompt must
prohibit fixes, dependency changes, M3 work, and reinterpretation of failures; it
may update only external-validation and roadmap status documentation after a pass.
