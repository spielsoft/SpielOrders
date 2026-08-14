---
title: Integrate and verify the M1-upgrade acceptance gate
status: closed
assignee: /root
claimed_at: 2026-07-31T14:24:15Z
closed_at: 2026-07-31T14:38:16Z
ticket_start_fixed_point: ab02ed5ca2d0e0ed0534859f31d19f488218090c
parent: M1-upgrade — Toolchain and dependency modernization
labels:
  - implementation:m1-upgrade
  - implementation:gate
---

## Outcome

Prove from a clean target-pinned copy that the completed Rust, Node/npm, and
TypeScript modernization composes without changing externally validated M1 behavior,
dependency boundaries, deterministic artifacts, or opaque Supporting Material, and
that the resulting commit is safe to externally validate before M2.

## Decision sources

- [M1-upgrade — Toolchain and dependency modernization](../README.md)
- [M1 external validation](../../m1-filesystem-metadata-kernel/VALIDATION.md)
- [M1 acceptance evidence](../../m1-filesystem-metadata-kernel/ACCEPTANCE.md)
- [SpielOrders implementation roadmap](../../../roadmaps/spielorders-implementation-roadmap.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

Unblocked. [Modernize Rust workspace resolution and direct dependency
baselines](001-rust-cargo-modernization.md), [Move repository tooling to Node 26 and
npm 12](002-node-npm-modernization.md), and [Adopt the TypeScript 7 CLI without
requiring its future compiler API](003-typescript-7-cli.md) are closed. The
user-approved frozen-snapshot disposition is independently reviewed and recorded.
The complete fresh rerun from exact fixed point
`ab02ed5ca2d0e0ed0534859f31d19f488218090c` passed under that policy. The
ticket is closed and ready for fresh external validation.

## Failed gate attempt — 2026-07-30

Ticket 004 was claimed from the exact clean completed-upgrade target
`67dc5e4560df173b902b24d3463bb01ff46de23b` at 2026-07-30T23:38:07Z. The
claim commit and isolated target were
`abfecb2b3dd32359347681fc0673fd5339bc4cfa`. The complete environment,
authoritative direct-release, and handback evidence is recorded in
[M1-upgrade acceptance evidence](../ACCEPTANCE.md).

The documentation-only gate found compatible `@types/react` and
`@types/react-dom` patch updates owned by
[Move repository tooling to Node 26 and npm 12](002-node-npm-modernization.md)
and stopped immediately without repair, installation, code review, closure,
milestone completion, or roadmap advancement. After that owning ticket is
remediated, independently reviewed, and reclosed, Ticket 004 must be claimed again
from the resulting exact target and rerun in full. No result from this failed
attempt may substitute for that clean rerun.

## Prior failed gate attempt — 2026-07-30

Ticket 004 was claimed from the exact clean completed-upgrade target
`5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f` at 2026-07-30T16:55:05Z. The
claim commit and isolated target were
`d4d181442023ef53b117a8f783a0f08c98a3863a`. The complete currentness,
audit, selector, and handback evidence is recorded in
[M1-upgrade acceptance evidence](../ACCEPTANCE.md).

The documentation-only gate found compatible updates owned by Tickets 001 and
002 and stopped without repair, code review, closure, milestone completion, or
roadmap advancement. After both owning tickets are remediated, independently
reviewed, and reclosed, Ticket 004 must be claimed again from the resulting exact
target and rerun in full. No executable or audit result from this failed attempt
may substitute for that clean rerun.

## Earlier failed gate attempt — 2026-07-27

Ticket 004 was claimed from the exact clean Ticket 003 baseline
`b725ad7bce6b8722dc02f060c39d1d9c8505d3ee` at 2026-07-27T16:53:06Z. The claim
commit and isolated target were
`86cf0cdbb873761cbe0679d77ea3fddf55333d95`. The complete executable,
currentness, and handback evidence is recorded in
[M1-upgrade acceptance evidence](../ACCEPTANCE.md).

The gate stopped without repair, code review, closure, milestone completion, or
roadmap advancement. Ticket 001 is the smallest owning upgrade ticket. After it is
remediated, reviewed, and reclosed, Ticket 004 must be claimed again from the new
exact target and rerun in full; no partial result from this attempt may substitute
for that clean rerun.

## Scope

- Determine and record the exact completed-upgrade target and compare it with the
  pre-upgrade fixed point `80e5527034615d6b71fce63ea40b55437bff9492`.
- Recreate all dependency trees in a clean temporary copy pinned to that target.
- Run every M1 acceptance command plus current repository-wide build, test, lint,
  traceability, frontend, Tauri, and development smoke checks.
- Independently recalculate Rust test, schema, accepted/rejected fixture, scenario,
  and sentinel counts instead of copying earlier evidence.
- Verify exact current toolchain/direct-dependency releases from authoritative
  sources under the approved majors and release channels.
- Audit the graph for justified transitives, duplicate generations, vulnerabilities,
  retained AJV/Vite selections, and absence of unused Mantine.
- Record exact commands, environment, results, counts, hashes, currentness sources,
  known observations, and scope audit in `ACCEPTANCE.md`.
- If and only if the gate passes, close this ticket, mark the milestone acceptance
  complete, and update the roadmap to say M1-upgrade is complete and ready for
  external validation.

This gate may change acceptance/planning documentation only. It must not repair a
dependency, production, test, schema, or fixture failure. Return a failure to the
smallest owning upgrade ticket and keep the gate open.

The user-approved 2026-07-31 stabilization window freezes the exact completed
dependency snapshot through this gate and fresh external validation. Continue to
inventory authoritative release drift, but do not treat later compatible releases
alone as failures or repair them here. Record them for the mandatory M2-closeout
refresh. Actual reproducibility, vulnerability, build, test, or behavior failures
remain blocking; new majors still require user approval.

## Acceptance criteria

- The exact upgrade target descends from the fixed point and contains only the
  planned, reviewed upgrade tickets and their evidence.
- Clean locked Cargo and npm builds reproduce under the selected toolchains.
- Workspace/core tests, Clippy, schema contracts, traceability, frontend tests/build,
  Tauri no-bundle build, and bounded development smoke checks all pass.
- Independent counts cover all Rust tests, four schemas, every accepted/rejected
  fixture, all canonical scenarios, and both opaque preservation sentinels.
- Configuration, metadata/domain validation, deterministic serialization, path
  confinement, atomic replacement, rollback/fault behavior, and schema/Rust
  agreement remain unchanged.
- Supporting Material hashes match the external M1 validation record exactly.
- Core remains independent of CLI, Tauri, React, TypeScript, and other adapters.
- AJV and Vite remain selected; no `@cfworker/json-schema`, direct esbuild
  replacement, unused Mantine, M2/M3 behavior, or shared SpielGantt crate is present.
- Authoritative release checks inventory every newer compatible direct and
  transitive release after the frozen target, identify each exact selecting
  package and compatibility justification, and assign the complete refresh to M2
  closeout without changing this documentation-only gate.
- The required fixed-point Standards/Spec code review of the gate evidence is
  complete and every finding is resolved or explicitly dispositioned.

## Review boundary

Review integrated reproducibility, preservation, dependency currentness, evidence
quality, and readiness for independent external validation. Do not fix failures,
change dependencies, begin M2, or reinterpret a failed command as a pass.

## Gate handoff

When this ticket closes, stop the implementation chain. Report that M1-upgrade is
complete and ready for external validation, then provide a ready-to-paste prompt for
a fresh validation-only agent. That prompt must name the original pre-upgrade fixed
point, the exact completed target, all upgrade tickets and recorded resolutions, the
project-local code-review skill, every acceptance command, independent dependency
currency verification, the user-approved frozen-snapshot disposition and M2-closeout
refresh obligation, documentation-only outcome recording, and an explicit ban on
fixes and M2 work.

## Resolution

Ticket 004 started from the exact clean completed documentation target
`ab02ed5ca2d0e0ed0534859f31d19f488218090c` on `main`. Tickets 001 through
003 were closed, this ticket was open, unblocked, and unclaimed, the milestone was
3/4 closed, M2 was blocked and unstarted, and the user-approved stabilization
window was recorded before `/root` claimed the gate. Commit
`756792f82565bc568843b7d668e1e71ca6876e62` records only that claim and
fixed point. No TDD RED test was created because this documentation-only gate
changes no observable behavior and is prohibited from repairing failures.

A new local no-hardlink clone detached at the claim commit contained no `target`,
`node_modules`, or `dist` artifact. The complete target-pinned gate passed there:
ancestry/range/scope checks; fresh toolchain diagnostics; repeated online and
offline clean installs; locked Rust build, workspace/core tests, and Clippy;
strict TypeScript 7 checking and language-service behavior; schemas and
traceability; frontend tests/build; Tauri release and live development smoke;
Cargo/npm graph, audit, currentness, duplicate, selector, and script-authority
inspections; independent counts and hashes; and every required configuration,
metadata/domain, deterministic-byte, path-safety, replacement, rollback,
core-boundary, preservation, and prohibited-scope audit.

Commit `65b4bab3bec8cefd48a9b78a77b93247748068a2` records the passing gate
evidence in [M1-upgrade acceptance evidence](../ACCEPTANCE.md). The complete
compatible post-snapshot release inventory is `@types/react` 19.2.18,
`@types/react-dom` 19.2.4, `fast-uri` 3.1.5, and optional
`@napi-rs/wasm-runtime` 1.2.2. None was installed. Their exact selectors and
publication times are recorded for the mandatory M2-closeout refresh. Cargo has
no compatible Rust-1.97.1 update, both npm audits report zero vulnerabilities,
and every manifest, lockfile, schema, fixture, source, test, and sentinel remains
unchanged by this gate.

The project-local code-review workflow compared
`git diff ab02ed5ca2d0e0ed0534859f31d19f488218090c...HEAD`. Standards and
Spec reviewers ran independently and in parallel:

- **Standards:** one hard milestone-result synchronization finding and zero
  judgment-call smells. This closure synchronizes the milestone status, progress,
  table, and narrative.
- **Spec:** three evidence findings and no scope creep. The missing original M1
  offline clean installs now pass; all summarized or placeholder audit commands
  are recorded exactly; and the optional WASM drift now names exact selector
  `@rolldown/binding-wasm32-wasi@1.2.1` with range `^1.2.0`.

After review remediation, both offline clean installs, strict TypeScript checking,
schema contracts, frontend tests, traceability, fixed-point/current-tree diff
checks, hashes, and source/clone status checks pass. Every finding is resolved,
every acceptance criterion passes, and no waiver remains. M1-upgrade is complete
and ready for fresh external validation. The implementation chain stops here; M2
remains blocked and unstarted.
