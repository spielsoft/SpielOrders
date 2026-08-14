---
title: M1-upgrade — Toolchain and dependency modernization
status: closed
closed_at: 2026-07-31T14:38:16Z
validated_at: 2026-07-31
progress: 4/4 tickets closed; externally validated
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m1-upgrade
---

## Outcome

Move the newly established SpielOrders baseline onto the approved current Rust,
Node, npm, Cargo-resolution, Rust-library, and TypeScript generations before M2
begins. Preserve the externally validated M1 product behavior, schemas, fixtures,
filesystem safety, adapter boundaries, and opaque Supporting Material bytes.

This is a bounded pre-M2 modernization milestone, not M2 implementation and not a
reopening of M1 product scope. The exact clean starting point is the documentation
commit that recorded successful M1 external validation:
`80e5527034615d6b71fce63ea40b55437bff9492`.

## Approved direction

The versions below were current stable releases when this milestone was planned on
2026-07-26. Each implementing agent must verify authoritative current release data
again at ticket start. Newer minor or patch releases within an approved major are
adopted automatically under `AGENTS.md`; a newer major requires fresh user approval.

### User-approved stabilization window

On 2026-07-31, after three target-pinned gate attempts were invalidated by
compatible releases published during M1-upgrade closeout, the user froze the exact
completed-upgrade dependency snapshot at
`67dc5e4560df173b902b24d3463bb01ff46de23b` through the remainder of the
M1-upgrade gate and its fresh external validation. This is a narrow exception to
the ordinary in-major currency rule in `AGENTS.md`, not a global change to that
rule.

The gate and external validator must still refresh authoritative release
information and record every newer compatible release, selecting package, and
compatibility observation. Release drift after the frozen target is not an
M1-upgrade blocker and must not reopen an implementation ticket solely for
currency. The complete compatible refresh is mandatory at M2 closeout. A new major
still requires explicit user approval, and any actual build, test, vulnerability,
or behavior failure remains blocking at the milestone where it is observed.

| Area | Approved target generation | Planning-time release |
| --- | --- | --- |
| Rust and Cargo | Current stable | 1.97.1 |
| Cargo resolver | Resolver 3 | 3 |
| Node.js build runtime | Current release line 26 | 26.5.0 |
| npm | Major 12 | 12.0.1 |
| Core TOML library | Major 1 | 1.1.3 |
| TypeScript CLI | Major 7 | 7.0.2 |

Other direct Rust, Tauri, React, type-package, and frontend dependencies remain on
their newest compatible releases in their selected majors. Direct manifest
baselines should name the current releases actually validated instead of leaving
misleading older lower bounds.

The following selections are deliberate:

- Retain AJV as the strict Draft 2020-12 contract-test validator. Do not replace it
  with `@cfworker/json-schema`; SpielOrders has no Cloudflare Worker or dynamic-code
  restriction that makes AJV unsuitable.
- Retain Vite and the official React plugin as the frontend build and development
  layer. Do not replace them with a project-owned esbuild build/server script.
- Do not add Mantine until an implementation ticket consumes it.
- TypeScript 7 is selected for its CLI and language-service behavior. It has no
  programmatic compiler API in 7.0. If an actual required tool imports that API,
  use the official TypeScript 6 compatibility package side by side when sufficient;
  otherwise stop and record the blocker rather than silently downgrading the whole
  toolchain.
- Node 26 is a build-time runtime for this repository, not a runtime shipped inside
  the Tauri application. Its Current rather than LTS status is therefore accepted.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Modernize Rust workspace resolution and direct dependency baselines](tickets/001-rust-cargo-modernization.md) | Closed | None |
| 2 | [Move repository tooling to Node 26 and npm 12](tickets/002-node-npm-modernization.md) | Closed | None |
| 3 | [Adopt the TypeScript 7 CLI without requiring its future compiler API](tickets/003-typescript-7-cli.md) | Closed | None |
| 4 | [Integrate and verify the M1-upgrade acceptance gate](tickets/004-m1-upgrade-acceptance-gate.md) | Closed | None |

```text
001 Rust/Cargo -------------------------\
                                         \
002 Node/npm -> 003 TypeScript 7 ----------> 004 M1-upgrade gate
```

All four tickets are closed, and the fresh
[external validation](VALIDATION.md) passed. The implementation chain remains
stopped; M2 planning may begin in a separate task, and M2 is unstarted here.

Ticket 004's 2026-07-27 target-pinned attempt found three compatible Cargo
lock-graph updates and returned the failure to Ticket 001 without repair. See
[M1-upgrade acceptance evidence](ACCEPTANCE.md). Ticket 001 was reclaimed at
`aa1d354a4f768eee8545f3db7d21bd650b1104ff`, remediated, independently
reviewed, and reclosed after its complete Rust-focused gate passed. The acceptance
gate was then reclaimed at `5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f`.
Its fresh 2026-07-30 target-pinned attempt found another compatible Cargo
lock-graph update plus compatible Node, npm, direct npm, and npm lock-graph
updates. It returned those failures to
[Modernize Rust workspace resolution and direct dependency baselines](tickets/001-rust-cargo-modernization.md)
and [Move repository tooling to Node 26 and npm 12](tickets/002-node-npm-modernization.md)
without repair. Both owning implementation tickets have now been remediated,
independently reviewed, and reclosed.
[Integrate and verify the M1-upgrade acceptance gate](tickets/004-m1-upgrade-acceptance-gate.md)
was reclaimed from exact fixed point
`67dc5e4560df173b902b24d3463bb01ff46de23b`. Its third fresh target-pinned
attempt found compatible patch releases for the React and ReactDOM type packages
and returned them to
[Move repository tooling to Node 26 and npm 12](tickets/002-node-npm-modernization.md)
without repair. The user-authorized stabilization disposition is now recorded,
independently reviewed, and closed without a dependency or lockfile change. The
gate started from exact fixed point
`ab02ed5ca2d0e0ed0534859f31d19f488218090c`. Its new no-hardlink,
target-pinned rerun passed every mandatory criterion, independently inventoried
four compatible post-snapshot releases for M2 closeout, and completed the required
parallel Standards and Spec review with every finding resolved. None of the three
failed attempts' evidence substituted for the passing rerun.

## Shared operating contract

- Work on exactly one ticket. Do not begin a later ticket or any M2 work.
- Start only from a clean worktree after every blocking ticket is closed.
- Claim the ticket before edits by recording `status: in progress`, `assignee`,
  `claimed_at`, and the exact output of `git rev-parse HEAD` as its fixed point.
  In this local tracker, an open ticket with no assignee is unclaimed.
- Read and use the project-local `implement` skill. Use the `tdd` skill when the
  ticket changes observable behavior or when a compatibility defect needs a public
  behavioral regression; dependency text changes alone do not justify synthetic
  RED tests.
- Work in narrow, independently reviewable commits. Do not combine another upgrade
  ticket merely because the package manager can update it at the same time.
- Run focused checks during work and every acceptance command named by the ticket
  at the end.
- Commit the implementation, then use the project-local `code-review` skill against
  the ticket's exact fixed point with separate Standards and Spec subagents. Resolve
  or explicitly disposition every finding, rerun affected gates, and commit the
  review disposition.
- Record commands, results, dependency-currentness sources, commits, review outcome,
  and any justified transitive exception in the ticket Resolution before closing it.
- Never adopt a new major that is not approved above without asking the user.
- Preserve metadata-version-1 meaning, deterministic bytes, both opaque Supporting
  Material sentinels, the core/adapter boundary, and the absence of M2/M3 behavior.

## Agent handoff chain

Every implementation-ticket agent must end by inspecting the milestone index and
ticket frontmatter in milestone order:

1. Identify the first open, unblocked, unclaimed M1-upgrade ticket.
2. Provide a complete ready-to-paste prompt for a fresh agent to implement exactly
   that ticket under this same operating contract.
3. Make that suggested prompt require its agent to repeat this handoff procedure, so
   the chain remains intact without relying on conversation history.
4. When Tickets 001 through 003 are closed, the next prompt must target
   [Integrate and verify the M1-upgrade acceptance gate](tickets/004-m1-upgrade-acceptance-gate.md).
5. When the acceptance-gate ticket is closed, stop the implementation chain, report
   that M1-upgrade is complete and ready for external validation, and provide the
   validation-only prompt required by that gate.

If no ticket is eligible because an open ticket is blocked or claimed, report the
exact blocker or claim instead of selecting later work. Never use the handoff chain
to begin M2.

## Milestone acceptance

M1-upgrade closes only when all four tickets are closed and the gate proves from a
clean target-pinned copy that:

- the frozen selected toolchains and dependency graphs reproduce exactly, while
  authoritative release drift is completely inventoried for the mandatory M2
  closeout refresh;
- locked clean installs, Rust builds/tests/lints, schema contracts, traceability,
  frontend tests/build, Tauri build, and development smoke checks pass;
- the four schemas, accepted/rejected fixtures, scenarios, and preservation
  sentinels retain their expected independently recalculated counts and behavior;
- no dependency replacement silently changes the intended validator, frontend
  development layer, metadata contract, or product behavior;
- Supporting Material bytes and the M1 core/adapter boundaries remain unchanged; and
- no M2 implementation, Mantine dependency, shared SpielGantt foundation, or other
  adjacent scope is present.

The passing, reviewed gate is recorded in `ACCEPTANCE.md`. A fresh
[external validation](VALIDATION.md) independently reproduced the complete target
and passed without a blocking finding. M1-upgrade is externally validated; M2
planning may begin in a separate task, and M2 remains unstarted here.
