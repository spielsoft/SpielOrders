---
title: Run the final M3 acceptance gate
status: closed
assignee: claude-opus-5
review_start: 653b142d8f39cdbd7a3be9b6f01272aad31d43d2
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:acceptance
---

## Outcome

One fresh, fixed-point run establishes whether the complete remediated M3 Rust-core
surface is ready for external validation. This ticket fixes only defects owned by the
integration seam; capability defects return to their smallest owning ticket.

## Blocking

No blocker: every preceding M3 execution ticket is closed from reviewed work. The
repository has one shared worktree, so do not claim this ticket while another remaining
ticket holds it.

## Scope

- Record the exact clean candidate SHA and current selected toolchains.
- Run all repository-required gates and the focused suites for Tickets 002 and 009–014.
- Compose initialization, creation, adoption, Vendor/Contact mutation, Order/Item edit,
  README write, alignment, removal, diagnosis, repair, upgrade, and index refresh through
  public Rust-core seams.
- Exercise success, no-op, rejection, collision, changed evidence, precommit failure,
  cleanup failure, rollback failure, partial upgrade, resumption, and post-commit index
  warning behavior.
- Run one milestone-wide review for cross-ticket contracts. This complements the reviews
  already completed within each ticket.
- Write only current evidence to `../ACCEPTANCE.md`.

## Acceptance criteria

- Every remaining ticket is closed and no blocking review finding is unresolved.
- The integration suite covers a non-default nested Orders layout and a real upgrade
  path rather than only default fixtures.
- No operation except explicit whole-Order removal moves or disposes of Supporting
  Material.
- Conventional READMEs retain absent, readable, and unreadable distinctions.
- Committed canonical mutations never become retry-shaped rejections because a derived
  index refresh failed.
- Dependency and toolchain currentness is checked as repository guidance requires.
- The acceptance record identifies either the closing SHA or each returned blocker.

## Exclusions

M4 CLI implementation, desktop behavior, search, agent orchestration, and speculative
M9 hardening.

## Review boundary

Cross-capability composition and milestone contracts. Do not hide capability fixes in
the gate.

## Closeout

Closed. M3 is accepted and ready for fresh external validation. The complete record —
candidate SHA, toolchains, gates, focused suites, integration coverage, preservation and
failure-path evidence, the milestone-wide review and every disposition, and the closing
SHA — is [ACCEPTANCE.md](../ACCEPTANCE.md).

The milestone-wide review ran at `01256cb..70098a5` in its designed parallel form, two
independent sub-agents with neither seeing the other's conclusions. Standards found no
blocking finding; Spec found one, and it was the integration seam's own: semantic
repair's committed-with-warning arm was exercised nowhere, so nothing failed if a
completed two-record repair started reporting itself as a retryable rejection. That is
composition coverage, which this ticket owns, and it was fixed here rather than returned
to Ticket 013 — the arm was confirmed correct, and no capability code changed.

Three milestone documentation contracts had become false and were corrected here. The
Package-valid Vendor duplication and metadata repair's over-refusing destination
evidence were routed to
[Ticket 027](027-hoist-package-valid-vendors-and-narrow-repair-evidence.md), deferred and
blocking nothing, rather than refactored during a fixed-point acceptance run. The
remaining non-blocking findings are recorded in ACCEPTANCE.md with the reason each was
accepted.

One owner decision is outstanding and does not block acceptance: `sha2` 0.11.0 is a new
major, which `AGENTS.md` reserves for the owner. It was not adopted.
