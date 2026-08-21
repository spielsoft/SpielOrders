---
title: M2 — Package loading, diagnostics, and Package Index
status: closed; awaiting fresh external validation
progress: 6/6 tickets closed
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m2
---

## Outcome

Implement the complete decision-closed M2 read side as shared Rust-core
capabilities over the existing filesystem port: exact conventional README reads,
canonical Package classification and projection, deterministic Package Index
assessment/rebuild, a no-canonical-reread current-index fast path, and exhaustive
validation. Finish with integrated acceptance and the mandatory complete
dependency/toolchain refresh.

This milestone does not reopen the closed
[M2 Package read-model map](../../wayfinder/m2-package-read-model/MAP.md), change
metadata version 1, or authorize M3 or adapter behavior.

## Authoritative decisions

- [Decide how Package Index freshness is proven without canonical rereads](../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Audit M2 decision completeness and hand off implementation decomposition](../../wayfinder/m2-package-read-model/tickets/004-audit-m2-decision-completeness.md)

The later [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
refinements are authoritative: `orders_path = "."` is invalid and Vendor Merge is
absent. No ticket may restore either superseded statement.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Read one exact conventional README through the shared core](tickets/001-read-exact-conventional-readme.md) | Closed 2026-08-14 | None |
| 2 | [Classify and project canonical Package state](tickets/002-classify-project-canonical-package.md) | Closed 2026-08-15 | None; Ticket 001 closed |
| 3 | [Rebuild the Package Index and preserve replacement-failure evidence](tickets/003-rebuild-package-index.md) | Closed 2026-08-15 | None; Ticket 002 closed |
| 4 | [Load a current Package Index without canonical record rereads](tickets/004-load-current-index-fast-path.md) | Closed 2026-08-20 | None; Ticket 003 closed |
| 5 | [Validate exhaustive managed evidence without inspecting Supporting Material](tickets/005-validate-exhaustive-managed-evidence.md) | Closed 2026-08-21 | [Load a current Package Index without canonical record rereads](tickets/004-load-current-index-fast-path.md) closed |
| 6 | [Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md) | Closed 2026-08-21 | [Validate exhaustive managed evidence without inspecting Supporting Material](tickets/005-validate-exhaustive-managed-evidence.md) closed |

```text
001 Exact README read
        |
        v
002 Canonical classification and projection
        |
        v
003 Index assessment, rebuild, and replacement failure
        |
        v
004 Current-index fast load
        |
        v
005 Exhaustive managed-evidence validation
        |
        v
006 Integrated M2 acceptance and complete closeout refresh
```

The dependency graph is deliberately linear. Each slice establishes the public
core behavior and filesystem-port evidence needed by the next slice, and each can
be reviewed from its own exact fixed point without concurrent ownership of shared
load, diagnostic, or test-double seams.

Fresh external validation of completed target
`f9ff9ad901438dc88c88e26136cebd3fabe418ce` passed every executable and
preservation gate but found one Spec evidence defect in
[Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)'s
exact prior-slice review inventory. At reopen, that capability was the first and only
open, unblocked, unclaimed ticket. Its prior closure remains historical. The
reopened worker claimed it from the exact clean post-reopen fixed point, corrected
and revalidated only that ticket, and reclosed it after the complete gate and
review passed. M3 and earlier behavior slices remained out of scope.

## Shared operating contract

- Select only the first milestone-order ticket that is open, has every blocker
  closed, and has no assignee. No later ticket may be claimed while an earlier
  eligible ticket remains open or claimed.
- Each worker claims and implements exactly one ticket. It must not begin, partly
  implement, or claim the following ticket.
- Each worker updates only its own ticket and the milestone index entries directly
  affected by that ticket's claim, blockers, status, result, and progress. The
  first five named capability slices never close the milestone; only
  [Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)
  may do so after its complete refreshed gate passes.
- Before any edit, require an empty
  `git status --porcelain=v1 --untracked-files=all`, resolve the exact
  `git rev-parse HEAD`, and verify every blocker is closed. Record that pre-claim
  SHA as `ticket_start_fixed_point`, set `status: in progress`, `assignee`, and
  `claimed_at`, then commit the claim alone. If the worktree is not clean or the
  milestone state disagrees with the ticket, stop and report it.
- Use the project-local `implement` and `tdd` skills. Work one observable
  RED-to-GREEN tracer bullet at a time through a public Rust-core seam with the
  filesystem port as the only fake. Every test must state the behavior defect it
  would catch. Do not batch speculative tests or test private helpers.
- Consume canonical material under [`contracts/`](../../../contracts/README.md)
  in place. Add only narrowly justified M2 fixture recipes or assets that prove a
  closed behavior not expressible with the existing corpus. Do not copy fixtures
  into a layer-specific tree and do not invent an older or future metadata schema.
- Preserve metadata version 1 and its deterministic bytes. Do not add a timestamp,
  filesystem identity, diagnostic, revision, reusable-value list, or search field
  to `index.json`; do not remove its `readme` field or add a sidecar.
- Supporting Material is wholly outside inspection and ownership. No work may list
  general Order Directory contents, recurse into them, inventory, parse, validate,
  rewrite, move, delete, or use Supporting Material as evidence. Every relevant
  real-filesystem or fault test preserves both opaque sentinels byte-for-byte.
- The Rust core owns Package rules. The existing filesystem port is the sole
  effect boundary and may gain only the smallest portable no-follow change
  observation required by the closed freshness decision. CLI, Tauri, React, and
  future desktop adapters remain consumers and receive no M2 commands, envelopes,
  retry presentation, or session state.
- Keep commits narrow. Run the ticket's focused checks regularly and its complete
  repository gates at the end. Commit all implementation and evidence before
  invoking the project-local `code-review` skill against
  `git diff <ticket_start_fixed_point>...HEAD` and
  `git log <ticket_start_fixed_point>..HEAD`.
- Code review must run Standards and Spec independently and in parallel as required
  by the skill. Resolve or explicitly disposition every finding, rerun affected
  focused and repository gates, commit the disposition, and record both axes in
  the owning ticket before closure.
- A contradiction between closed decisions blocks only the affected behavior. Do
  not settle it in a type, fixture, or test expectation; stop and report the
  conflicting full decision names and evidence.

## Shared exclusions

- M3 initialization, domain mutation, repair execution, README writes, Metadata
  Upgrade execution, mutation plans, or persistence beyond the automatic derived
  Package Index replacement decided for M2.
- CLI commands, CLI JSON envelopes, Tauri commands, React state or presentation,
  retry presentation, or adapter-owned Package rules.
- Native entity search, Supporting Material search, recursive fallback indexing,
  file open/reveal behavior, or any inspection of Supporting Material.
- Metadata/schema/version changes, an invented migration schema, a Package Index
  sidecar, or new search fields.
- Mantine or another unused frontend dependency, install-script authority, a
  shared SpielGantt crate/frontend package, or a generic repository, repair,
  persistence, or filesystem framework.

## Fixture and preservation contract

The canonical scenarios already supply unmanaged, healthy, Package-damaged,
Order-damaged, duplicate-identity, unresolved-reference, README, index,
reserved/staging, unsupported-version, symbolic-upgrade, and opaque-preservation
foundations. Ticket-local additions must be one-purpose recipes for exact result
variants, stable evidence and ordering, comparable versus uncertain change
observations, fast-path read budgets, rebuild bytes, or replacement phases.

No fixture may treat a general Order Directory listing as evidence. Authorized
inspection is limited to Package-owned metadata paths, the Orders Directory's
direct children, exact conventional README paths, and—only during exhaustive
validation—the immediate entries of Package and Order `.spielorders` directories.
Both `supporting-a.bin` and `supporting-b.bin` must retain the hashes recorded by
the externally validated M1 and M1-upgrade evidence.

## Decision coverage and ownership audit

| Closed contract area | Primary owning ticket | Integrating tickets |
| --- | --- | --- |
| Exact Package/Order README references, JIT UTF-8 reads, absent-as-empty, document warnings, and cache non-evidence | 001 | 002, 004, 005, 006 |
| Managed context; unmanaged, diagnostic-only, upgrade-pending, and general operation-failed classification; canonical diagnostics | 002 | 003, 006 |
| Complete healthy Order/Vendor projection, trustworthy identities, totals, Vendor resolution, damaged placeholders, and four reusable-value lists | 002 | 003, 004, 006 |
| Derived-index states/reasons, deterministic healthy-only rebuild, refresh outcomes, and blocking replacement failure | 003 | 004, 006 |
| Portable no-follow cutoff, exact current-index read budget, permitted index sourcing, and deliberately absent fast evidence | 004 | 005, 006 |
| Separate exhaustive broader-evidence operation, reserved/staging warnings, and complete deterministic grouping | 005 | 006 |
| Integrated preservation/boundary evidence, complete selected-major refresh, and external-validation handoff | 006 | None |

The audit checked every section of all three closed M2 decisions and the closed
completeness audit. No contract is uncovered. Shared concepts are introduced by
the first observable capability that needs them and extended only by the ticket
owning the next observable condition; no two tickets independently implement the
same behavior. The linear graph has no cycle. No ticket is a types-only layer,
generic repository/framework, speculative adapter slice, schema/version change, or
M3 behavior. The
[integrated M2 acceptance and selected-major refresh](tickets/006-integrate-m2-acceptance-and-refresh.md)
capability owns the one complete closeout refresh, so it is neither omitted nor
duplicated.

## Handoff chain

At the end of every ticket, inspect this index and all ticket frontmatter in
milestone order. Provide a complete ready-to-paste prompt for a fresh worker to
claim and implement only the first open, unblocked, unclaimed ticket. The prompt
must repeat the clean fixed-point protocol, one-ticket rule, implement/TDD/review
workflow, preservation and boundary constraints, every required verification,
permission to update/close only that worker's ticket and the milestone status as
appropriate, and this same handoff requirement. If no ticket is eligible, report
the exact blocker or current claim instead of selecting later work.

When
[Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)
closes, stop the implementation chain. M2 is complete only after fresh external
validation reproduces the completed target. Its handoff must therefore be a
validation-only prompt that prohibits fixes, dependency changes, and M3 work.

## Milestone acceptance and closeout refresh

[Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)
is the final integrated M2 acceptance capability. It must begin from the frozen
M1-upgrade release-drift inventory:

- `@types/react` 19.2.17 -> 19.2.18;
- `@types/react-dom` 19.2.3 -> 19.2.4;
- `fast-uri` 3.1.4 -> 3.1.5; and
- optional `@napi-rs/wasm-runtime` 1.2.1 -> 1.2.2.

That inventory is a starting point, not a ceiling. At that capability's start, refresh
authoritative release data for Rust, Cargo, Node, npm, TypeScript, every direct
dependency, and every compatible locked transitive. Adopt the newest available
minor and patch within every selected major automatically under
[`AGENTS.md`](../../../AGENTS.md). Ask the user before any new major. Grant no
install-script authority and do not weaken the retained AJV/Vite or adapter
boundaries.

After the refresh, a fresh target-pinned no-hardlink copy must pass every affected
clean install, offline reproduction, locked build, core/workspace test, lint,
schema, traceability, dependency/audit, frontend, Tauri, bounded development-smoke,
preservation, dependency-direction, behavior-boundary, and prohibited-scope check.
Integrated M2 evidence must additionally prove the fast-path read budget,
deterministic healthy-only rebuild, every load-result family, exhaustive evidence
boundary, blocking replacement failure with healthy canonical state preserved,
portable filesystem-port fault injection, and byte-identical opaque sentinels.

Only after the fixed-point Standards and Spec review is resolved may
[Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)
close the milestone and update the roadmap to completed M2 awaiting external
validation. A fresh worker must then externally validate the exact completed
target before any M3 work begins.

## Result

All six vertical slices are closed. The complete selected-major refresh and
[integrated acceptance](ACCEPTANCE.md) passed from fresh no-hardlink target copies,
the fixed-point Standards and Spec review has no unresolved finding, and M2 is
complete at the implementation layer. Fresh external validation of the exact
completed target is required before M3 may begin. The milestone closure transition
for the first lifecycle was `3edd745c002805a668cd33d8a5d89ea6a46009f3`;
the reopened closure transition is
`e280145ae8eff2b03a856562aa2338b0cebd263f`.
