---
title: M3-upgrade — Test harness ergonomics
status: in progress
progress: 1 of 2 tickets closed
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m3-upgrade
---

## Outcome

Make the Rust test suite cheap to run so that every remaining implementation ticket
pays seconds per iteration instead of minutes. Nothing about what the suite asserts may
change.

This is a bounded engineering milestone in the shape of
[M1-upgrade](../m1-upgrade-toolchain-dependencies/README.md). It is not M3 product work,
it does not close M3, and it may not touch a domain capability. M3's own remaining
execution tickets are listed only in
[the M3 index](../m3-domain-mutations-safe-persistence/README.md).

## Why this exists

The suite is fast; building it is not. Measured on 2026-08-31 at
`217b2109e3afed166943daedba9ed66e614fb09a`:

| Observation | Value |
| --- | --- |
| Test execution, whole workspace, self-reported | 13.04 s across 39 binaries |
| `cargo test -p spielorders-core --tests`, warm, nothing to rebuild | 11.97 s and 11.99 s wall; compile step 0.06 s |
| One already-built test binary through cargo | 0.71 s |
| Integration test binaries in `spielorders-core` | 33, so 33 separate link steps |
| Executables in `target/debug/deps` | 241, totalling 1.2 GB |
| `target/` | 25 GB |
| `[profile.test]` in any manifest | absent, so full packed debuginfo on every link |
| Merged doctest compilation | 22.63 s of the 22.84 s doctest phase |
| First-exec penalty on a freshly linked small binary | about 37 ms, negligible at this scale |

Two separate costs are hiding behind one symptom. The larger is link and debuginfo work
across 33 binaries. The other is self-inflicted, and a 19-minute run and a 12-second run
were the same suite in the same repository.

**Corrected by Ticket 001.** The second cost was diagnosed above as clippy: that
`cargo clippy` compiles through `clippy-driver`, carries different fingerprints than
`cargo test`, and so re-links the whole set when the two alternate against one target
directory. That does not reproduce on the pinned toolchain. Clippy emits check units,
which Cargo fingerprints apart from the build units `cargo test` links, so both sets
coexist; measured over three alternating rounds, each switch cost 0.13 to 0.18 s and
rebuilt no test target. Differing `RUSTFLAGS` does not thrash either — each value simply
gets its own cached artifact set.

The variable-cost symptom is real, and Ticket 001 found its actual cause: Cargo never
removes the object files a previous build abandons, so `target/debug/deps` grows without
bound, and past a few hundred thousand entries every write into it slows down. This
repository's `target/debug/deps` held 844,028 files. A relink there took 13 to 37 s and
climbed within one session, against 1.3 s in a fresh directory on the same machine at
the same load. `cargo clean` restores it. The "first-exec penalty … negligible at this
scale" row above is a measurement of that degradation, not of binary size: the same
25 MB binary took 23 s on first execution inside the accumulated directory and 0.4 s
elsewhere. Linking once rather than 33 times also slows the accumulation by about eight
times per unit of work.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| 1 | [Make the Rust test suite cheap to iterate on](tickets/001-test-harness-ergonomics.md) | Closed | None |
| 2 | [Let a core test read as its assertions plus a named fixture](tickets/002-fixture-builders.md) | Ready for agent | None |

## Shared operating contract

- Work exactly this one ticket. Do not begin M3 capability work or any later milestone.
- Start from a clean worktree and record the exact starting SHA. Claim the ticket before
  editing anything.
- Use the project-local `implement` skill. Use `code-review` before closure with its
  designed parallel Standards and Spec sub-agents, as `AGENTS.md` requires.
- Every claimed improvement must carry a before and after measurement taken the same
  way. A structural change that is not measured is not finished.
- No test may be deleted, weakened, or made to assert less. Prove test-count and
  behavior parity rather than asserting it.
- The repository has one shared worktree. Do not claim this ticket while an M3 execution
  ticket holds it, and do not hold it longer than the work needs.

## Sequencing

This milestone competes with M3's seven remaining execution tickets for the shared
worktree, and its later scope edits the same test files those tickets extend. The
recommended order is to take the parts that touch no test content first, land them, and
run the harness consolidation before the remaining M3 tickets rather than after, so
those tickets inherit the fast loop instead of each paying the slow one. The ticket
states which parts are separable; a worker that cannot land the whole ticket should
close the measured parts and hand the remainder back rather than holding the worktree.

Ticket 001 did exactly that: parts 1 to 3 are closed and the harness is in place, so
M3's remaining tickets already inherit the fast loop. Its part 4 was handed back
unstarted as Ticket 002, which edits the test files those M3 tickets extend and is
therefore worth sequencing after them rather than before.
