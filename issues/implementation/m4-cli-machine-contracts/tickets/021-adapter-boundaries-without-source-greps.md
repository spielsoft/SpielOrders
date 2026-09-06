---
title: Hold the adapter's boundaries by visibility rather than by reading its source
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T19:34:22Z
closed_at: 2026-09-05T20:01:36Z
review_start: ae846cff551a57f817698f54a98df3d144fe873c
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

The boundaries the CLI crate claims about itself are enforced the way the process document
says architecture is enforced, and the suite that reads the crate's own source text for
them is gone.

## Decision sources

- [`AGENTS.md`](../../../../AGENTS.md), whose Records section decides how architecture is
  enforced
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md), whose
milestone-wide review found this and returned it.

## Scope

`AGENTS.md` says structural tests that grep the source tree are not added, and that
architecture is enforced by visibility, by types, and by clippy. The adapter's boundary
suite reads every `.rs` file under its own `src/` as text and asserts over substrings: one
effect module, one stream module, no direct process exit, no panic on given input, one
environment reason, no signal handler, no ordering decided in the adapter, one money
rendering. Two of its claims were already retreated for forcing code to be written around
them.

Nothing here was added in breach: every claim predates the rule, which landed afterwards.
What the rule leaves is a suite it forbids and a remedy that is not presently available —
the crate's library declares every module public, so visibility enforces none of these
boundaries today. Making the rule true therefore means making visibility real, not merely
deleting tests: the executable is the crate's product and the library exists for its own
test target, so what that target genuinely needs is a small surface rather than eight
public modules.

Some claims survive as behaviour and should be kept that way — the decided exit statuses,
the two streams, and the two-decimal money rule are all observable from a real process, and
the output-mode and monetary suites already assert them there. Some are structural only.
Say which is which, keep the behavioural ones where behaviour can hold them, and let the
rest go with the file.

Where a claim can be held by neither visibility nor behaviour, that is a finding for the
owner rather than a reason to keep the grep: report it and stop, naming the claim.

## Acceptance criteria

- No test in either crate reads the repository's own source text to make an assertion.
- The CLI library exposes only what its test target and the executable actually use; every
  module the tests do not need is crate-private.
- The claims that are observable from a real process are asserted from one, and each has a
  named home. No claim silently disappears: a claim that is dropped is named with the
  reason it cannot be held.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Amending `AGENTS.md`. If the rule is wrong, that is the owner's ruling and not this
  ticket's to take.
- Any change to what the adapter does. This ticket changes what enforces it, not it.
- The core crate's own suite, which this finding is not about.

## Result

The library exposes only what its test target and the executable use — the platform
effects, the value kinds, and the response contract identities — and `unreachable_pub`
keeps it there; `run` hands `main` an `ExitCode` so the closed set of statuses is
crate-private too. The suite that read the crate's own source is gone, and every claim it
made is named where the claim is made: the exits, the panics and the stream macros by
clippy denials at both crate roots, the process-ending, signal and environment spellings
clippy has no lint for by the crate's `clippy.toml`, and the streams, the money and the
orderings by the process-level suites that already assert them from a real process.

One retreat is recorded rather than held. `src/effects.rs` says why nothing fences `fs::`
to that module, and that a reader is what does.
