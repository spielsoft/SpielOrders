---
title: Make the Rust test suite cheap to iterate on
status: closed
review_start: 958d278e16e9ded642814ebfb5ce1c4307a527c1
parent: M3-upgrade — Test harness ergonomics
labels:
  - implementation:m3-upgrade
  - implementation:task
---

## Outcome

An agent or developer changing `spielorders-core` gets a full core-suite result in
seconds, repeatably, without a relink. The suite still runs every test it runs today and
still asserts exactly what it asserts today.

## Decision sources

- [Agent instructions](../../../../AGENTS.md) — dependency and toolchain currency, and
  the standing authorization for parallel review sub-agents.
- [Traceability](../../../../docs/development/traceability.md) — what a test file must
  still declare after any reorganization.
- [Boundaries](../../../../docs/architecture/boundaries.md) — the core/adapter split the
  test layout must keep visible.

## Blocking

No capability blocker. The repository has one shared worktree, so do not claim this
ticket while an M3 execution ticket holds it.

## Scope

Four parts, ordered so the cheapest and least invasive land first. Parts 1 and 2 touch
no test content at all.

1. **Inner-loop procedure.** Record the loop command and the gate command in
   `docs/development/`, and state the clippy interaction plainly: alternating
   `cargo clippy` and `cargo test` against one target directory re-links everything,
   because clippy compiles through `clippy-driver` and carries different fingerprints.
   Give the loop a way to avoid that cost — a separate `CARGO_TARGET_DIR` for lint runs
   is the obvious candidate — and say which command belongs in a gate rather than a loop.
   `--workspace` drags in the Tauri desktop crate and belongs only in a gate.

2. **Debuginfo profile.** Add a `[profile.test]` selection. `debug = 0` and
   `debug = "line-tables-only"` are both plausible; choose on measurement, not taste, and
   consider `split-debuginfo = "unpacked"`. Report the wall-clock delta, the
   `target/debug/deps` size delta, and whether a failing test still yields a usable
   backtrace, because that is what the setting trades away.

3. **One integration-test harness.** Collapse the 33 integration-test binaries in
   `crates/spielorders-core/tests/` into a single target with `mod` declarations, turning
   33 link steps into 1. This is the largest structural win and needs the most care:
   - Every test then shares one process. Find and fix anything relying on process
     isolation — current working directory, environment variables, or a global set once
     per binary.
   - `mod` names must not collide, and `support` must remain usable from every module
     without reintroducing per-file `#![allow(dead_code)]` breakage.
   - `cargo test --test <name>` filtering changes shape. Update part 1's documented
     procedure to match, including how to run one former file's tests.
   - Each former file's `//! Decision sources:` marker must survive where traceability
     expects it.

4. **Fixture builders.** Inline filesystem fixture construction dominates the test
   sources; `support::minimal_package` already exists but is used in a minority of
   places. Extract the repeated fixture shapes into `support/` so a test reads as its
   assertions plus a named fixture. This part changes test files that M3's remaining
   execution tickets also extend, so it is the most likely to conflict and the most
   separable — close parts 1 to 3 and hand this back rather than holding the worktree for
   it.

## Acceptance criteria

- The documented loop command runs the complete `spielorders-core` test suite from a warm
  tree with a compile step of well under a second, and the documented procedure is
  reproducible by following only what is written down.
- A `[profile.test]` selection is in place, with before and after wall-clock and
  `target/debug/deps` size recorded, and the backtrace consequence stated.
- Integration tests link once rather than 33 times, evidenced by the build output.
- Test parity is proven: the same number of tests run, with the same names, and no test
  is deleted, ignored, weakened, or made to assert less. A name-level diff of the test
  inventory before and after is the evidence.
- The full workspace gate still passes, and `cargo fmt --check`, clippy with warnings
  denied, and both contract scripts on the pinned Node runtime still pass.
- Every improvement claim carries a measurement taken the same way before and after. The
  baseline to beat is in the milestone README.

## Exclusions

No product behavior change. No change to what any test asserts. No M3 capability work.
No new dependency without the `AGENTS.md` major-version rule. Do not restructure the
Tauri desktop crate's own tests, and do not reduce the workspace gate's coverage in order
to make it faster.

## Review boundary

Review the test harness layout, the profile and manifest changes, and the developer
procedure. Do not absorb M3 capability work, and do not review the domain behavior the
tests cover except to confirm it is unchanged.

## Closeout

Closed. Scope parts 1 to 3 landed; part 4 was handed back unstarted, as that part
authorizes, and is now
[Let a core test read as its assertions plus a named fixture](002-fixture-builders.md).

Started from a clean worktree at `958d278e16e9ded642814ebfb5ce1c4307a527c1`.

### Measurements

Before and after taken the same way and interleaved in one run, on one machine at one
load, in two fresh target directories on the same volume. Fresh matters: see the
accumulation finding below.

| Measure | Before | After |
| --- | --- | --- |
| Executables linked for the core test suite | 34 | 2 |
| Relink after a core source change, median of 3 | 3.20 s | 1.32 s |
| Full edit-and-test loop after a core source change, median of 3 | 24.29 s | 13.50 s |
| Warm no-op compile step | 0.06 s | 0.08 s |
| `target/debug/deps` | 533 MiB | 154 MiB |
| Files in `target/debug/deps`, after one cold build and three rebuilds | 27,946 | 3,291 |
| Cold build of the suite | 10.65 s | 9.70 s |

Test execution itself is unchanged at about 12 s and always was: the suite calls
`fsync`, so it is I/O-bound, and no build change shortens it.

`[profile.test]` selects `debug = "line-tables-only"`, chosen on measurement. Once the
suite links once, the setting buys disk and not time — relink is 1.30 s unset, 1.36 s
with line tables and 1.41 s with `debug = 0`, which is one spread — so the axis that
separates them is `target/debug/deps`: 199, 154 and 77 MiB. `debug = 0` takes the larger
cut and drops `file:line` from every backtrace frame, keeping only frame names; line
tables keep `file:line` and lose only a name's fully-qualified spelling. The panic site
an assertion failure reports survives under all three. `split-debuginfo = "unpacked"` is
left unset: it measured byte-identical, being already the macOS default.

### Test parity

475 tests before, 475 after: 473 integration and 2 library unit tests. A name-level diff
of the two inventories, normalized to `<former file>::<test>`, is empty — no test added,
removed, renamed, ignored, or filtered out, and `0 ignored` in both. Every changed line
in the 33 test sources is an import path or a blank line; the only non-import edits in
the whole set are two `support::QueuedUuids` call sites that became `crate::support::`.
All 33 former files keep their `//! Decision sources:` marker in place.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass, as do `npm run test:schemas` and
`npm run check:traceability` on Node 26.8.1 with npm 12.0.2.

### What the measurements corrected

The clippy mechanism this milestone was opened on does not reproduce, and the real cause
of the variable-cost symptom is unbounded object-file accumulation in
`target/debug/deps`. Both are recorded in the milestone README and in
[the test loop procedure](../../../../docs/development/test-loop.md). This repository's
own `target/debug/deps` held 844,028 files at closing time and is the reason a relink
there costs 13 to 37 s rather than 1.3 s; `cargo clean` restores it, and that has been
left to the owner rather than done here.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent. Two blocking findings, both resolved: the root `README.md` called the
standalone core check the inner-loop command when the loop command is narrower, and the
milestone README still asserted the disproven clippy mechanism. The Spec axis also
reported measurements an order of magnitude worse than those claimed; it was right about
the numbers and they were remeasured, which is what surfaced the accumulation cause
above. Accepted non-blocking: `support/mod.rs` and `support/package_index.rs` keep
`#![allow(dead_code)]`, which one target rather than 33 would now make meaningful to
remove, but removing it is fixture work belonging to Ticket 002; and this ticket's
procedure doc cites this milestone rather than a closed wayfinder decision, there being
no closed decision that governs a developer procedure.
