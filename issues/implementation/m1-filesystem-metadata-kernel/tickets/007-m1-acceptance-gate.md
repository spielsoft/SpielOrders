---
title: Integrate and verify the M1 filesystem and metadata kernel acceptance gate
status: closed
assignee: codex
claimed_at: 2026-07-25
closed_at: 2026-07-25
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:gate
---

## Outcome

Prove the independently reviewed M1 behaviors form one coherent Rust core kernel
and that Package loading, diagnostics, and domain mutation scope have not leaked
into M1.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Require two parent segments in the Order package locator](../../m0-contract-baseline/tickets/007-require-two-parent-package-root.md)
- [M1 — Filesystem and metadata kernel](../../../roadmaps/spielorders-implementation-roadmap.md#m1--filesystem-and-metadata-kernel)

## Blocking

Satisfied. [Restore prior canonical state after a failed multi-file replacement](006-multi-file-rollback.md)
closed on 2026-07-25 after all five earlier prerequisite implementation tickets
had closed and integrated.

## Scope

- Run schema/Rust agreement across accepted, schema-invalid, semantic, path/link,
  numeric, identity/reference, and staging scenarios.
- Run real-filesystem containment tests and the complete fault-injection matrix.
- Run core/workspace tests and traceability checks from a clean copy.
- Record M1 acceptance evidence and perform dependency and scope audits.
- Return non-integrative failures to the owning ticket.

Exclude Package loading, damage projections, stable diagnostics, README and index
loading behavior, every domain mutation/persistence workflow, CLI/Tauri/frontend
behavior, and a shared filesystem crate.

## Pre-agreed public test seam

The gate uses only public seams delivered by Tickets 001 through 006 and may not
test private helpers. Its defining regression is disagreement or failed composition
among schemas, Rust parsing, semantic validation, path safety, and atomic
preservation.

## Acceptance criteria

- Schema and Rust suites agree across every applicable canonical fixture.
- Valid edge cases remain accepted and every named semantic invalidity is rejected.
- Path/link tests prove no escape or link following.
- Failure injection proves prior valid bytes survive failed replacement and rollback
  failures remain observable.
- Supporting Material sentinels remain byte-for-byte unchanged.
- Core remains independent of CLI, Tauri, and frontend.
- The scope audit finds no M2/M3 behavior or speculative shared crate.

## Review boundary

Review integrated evidence and milestone readiness only. This gate is not a
catch-all implementation ticket.

## Ticket-start fixed point

`76bd13026da9477903aba10d39034c8f129077c5`

## Resolution

The independently reviewed public seams delivered by all six prerequisite
implementation tickets pass as one coherent M1 kernel. The clean artifact-free
gate passed 59 workspace Rust tests, the same 59 tests through an independent core
run, Clippy with warnings denied, 4 schemas, 8 accepted examples, 17 rejected
fixtures, 22 canonical scenarios, 2 frontend boundary tests, production frontend
and Tauri builds, and byte-for-byte preservation of both opaque Supporting Material
sentinels.

[M1 acceptance evidence](../ACCEPTANCE.md) records the exact commands, toolchain,
test counts, canonical-scenario coverage, complete single-file and multi-file
failure matrices, dependency currency result, dependency-direction audit, frontend
boundary audit, scope audit, and sentinel hashes.

Implementation commits:

- `0c460fa` — claim and fixed-point tracking;
- `0a1603b` — initial acceptance evidence; and
- `a34a8c3` — accepted review findings and auditable canonical-scenario mapping.

`/code-review` against the exact fixed point reported one Standards finding for
bare tracker identifiers and one Spec finding for insufficiently explicit canonical
scenario coverage. Both were accepted and resolved in `a34a8c3`; the fixes changed
evidence structure only, not the public design, so the review workflow did not
require another review run. Focused and full gates passed again after remediation.

No non-integrative defect was absorbed from a prerequisite ticket. No Package
loading, damage projection, stable diagnostic catalog, README/index loading policy,
domain mutation or persistence workflow, adapter behavior, shared SpielGantt crate,
or other M2/M3 scope was added. The acceptance gate passes without waiver.

## M1-V4 remediation evidence

M1-V4 was remediated from exact clean fixed point
`1947aa4a27e048fe956f3de09547559d998ba0ff`. Node's official release index and
npm's registry identified Node 24.18.0 and npm 11.18.0 as the newest releases in
the selected Node 24 and npm 11 majors. The corrected environment used those
versions with Rust/Cargo 1.97.1 on macOS 26.5.2, Darwin 25.5.0, Apple Silicon.
README.md deliberately selects the Node/npm majors rather than an exact patch
release, so this environment-only correction had no product behavior for the
project-local TDD workflow to exercise and required no new version guard.

The npm audit discovered one compatible unlocked transitive update:
`postcss` 8.5.21 to 8.5.23 through Vite's existing `^8.5.17` requirement.
`npm update postcss --package-lock-only --ignore-scripts` made that one lockfile
change without changing `package.json`, and authoritative registry checks then
reported `postcss` current. Other npm differences are new majors, optional packages,
or versions constrained by the existing dependency graph. `cargo update --dry-run
--verbose` refreshed crates.io and found zero compatible lockfile changes.

An artifact-free no-hardlink export recreated both npm trees from their lockfiles.
The newly selected postcss tarball first had to be added to the local npm cache;
both documented offline clean installs then passed with zero vulnerabilities.
From that clean copy, the following renewed evidence passed:

- locked workspace build, 62-test workspace run, independent 62-test core run, and
  Clippy for all targets with warnings denied;
- 4 schemas, 8 accepted examples, 19 manifest-listed unique rejected fixtures,
  22 unique scenarios, and 2 preservation sentinels;
- the configured traceability check, 2 frontend boundary tests, frontend production
  build, and Tauri no-bundle release build; and
- independent Supporting Material hashes
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`
  and
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

The repository-wide format observation is unchanged: `cargo fmt --all -- --check`
reports only trailing blank lines in four untouched M0 adapter stubs. They remain
outside this remediation. No production code, schema, fixture, domain test,
metadata version, selected major, M2/M3 behavior, or shared crate changed.

The project-local code review covered exact committed range
`1947aa4a27e048fe956f3de09547559d998ba0ff...60b65a2`. Standards reported zero
documented-standard violations and zero smell judgments. Spec reported one partial
evidence requirement: the review outcome was not yet recorded here and in the
validation record. The finding is accepted and resolved in both records. This
follow-up changes evidence only; `git diff --check` and the configured traceability
check pass afterward, and no executable gate is affected.

M1-V4 remediation did not itself externally validate the milestone. The subsequent
fresh [M1 external validation](../VALIDATION.md) reviewed the complete remediated
target and passed on 2026-07-26. M2 is now ready for decomposition; implementation
has not begun.
