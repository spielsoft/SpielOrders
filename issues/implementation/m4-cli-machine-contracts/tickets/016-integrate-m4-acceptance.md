---
title: Run the final M4 acceptance gate
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T03:05:00Z
closed_at: 2026-09-06T12:20:00Z
review_start: 47f1673de22c750e67c86ef8b9f1f103b99dd8f8
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:acceptance
---

## Outcome

One fresh, fixed-point run establishes whether the complete M4 command surface and its
machine contracts are ready for external validation. This is the only ticket that may close
M4, and its record is [ACCEPTANCE.md](../ACCEPTANCE.md).

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

Blocked by [Ticket 015](015-cross-command-contract-conformance.md), itself blocked by every
command ticket.

Also blocked by
[Ticket 023](023-roots-and-suggestions-a-command-already-knows.md), which this gate's second
run returned rather than repaired, and which carries every blocking finding that run
produced. A gate that repaired what it found would have stopped measuring, so it returns
and reopens itself for a fresh run at a new candidate. What each run observed is in
[ACCEPTANCE.md](../ACCEPTANCE.md), which the next run rewrites.

Withdrawn by the owner: driving a real version-to-version upgrade with an interruption and a
resumption. The transformation registry is empty, so nothing can be interrupted or resumed,
and this ticket's own criteria hold metadata version 1 unchanged. A *blocked* upgrade is
reachable and stays in the failure surface.

**The third run halted for an owner ruling and got one.** That run passed the gate and its
milestone-wide review returned two findings at the bar the owner reserved for a further
run: a decided repair delivered for half the damage it was decided for, and a context that
reports no Package where the command established one. It wrote no ticket for either,
because the owner asked to be told first. Both, with the suggestion finding that shares
their family, became
[Ticket 024](024-deliver-the-decided-repair-and-the-established-root.md); closing it
surfaced the same false suggestion from two further producers, which
[Ticket 025](025-one-producer-for-the-repair-suggestion.md) ended as a class rather than as
a fourth instance. The owner then ruled that the loop terminates at a fourth run: accept
the milestone unless a finding reaches that same bar, and carry the rest forward.

## Scope

- Record the exact clean candidate SHA and the selected toolchains, and run
  [`scripts/gate.sh`](../../../../docs/development/test-loop.md) at the candidate.
- **Compose the commands into one workflow no single ticket owns**: initialize over a
  non-default nested Orders layout and a renamed Removed Orders Directory, create and adopt
  Orders, create and edit Vendors and Contacts, edit Order facts and Items, align, validate,
  damage a record and diagnose it, apply a repair, run `upgrade` both ways, rebuild the
  index, and dispose of an Order both recoverably and to the Trash — through the executable
  alone, as real processes.
- Exercise the failure surface across that composition: rejection, no-op, collision, changed
  destination, precommit failure, rollback failure, blocked upgrade, and a committed
  canonical mutation whose index refresh then failed.
- **Re-establish cross-command conformance at the candidate tree.** Named here deliberately
  rather than delegated, so a regression landing after Ticket 015 closes is still caught.
- Confirm the adapter boundary holds: no Package rule is decided in the CLI crate.
- Check dependency and toolchain currency as [`AGENTS.md`](../../../../AGENTS.md) requires, reporting every
  blocked in-major update and adopting no new major without the owner.
- Run one milestone-wide two-axis review, complementing the per-ticket reviews.
- Write only current evidence to [ACCEPTANCE.md](../ACCEPTANCE.md).
- **Transition the roadmap's M4 section** to complete, on a passing gate and never earlier.

## Acceptance criteria

- Every M4 ticket is closed from reviewed work and no blocking review finding is unresolved.
- Every command M4 delivers is exercised end to end at the candidate or recorded as routed
  to a named later milestone. A missing command is visible rather than silent, and a
  registered command no decision names is equally a finding. **Amended** under the owner's
  record cap (2026-09-05), which leaves [ACCEPTANCE.md](../ACCEPTANCE.md) no room for the
  command table this criterion used to require: one inventory in the conformance suite is
  read in both directions, so each half is a test that fails rather than prose that goes
  stale.
- The composition runs over a non-default nested Orders layout and a renamed Removed Orders
  Directory, not only default fixtures.
- `--json` emits exactly one UTF-8 document and no incidental output for every command, and
  human output is nowhere a parsing contract.
- No operation except explicit whole-Order removal disposes of Supporting Material and
  nothing inspects it. The two operations that relocate a whole Order Directory intact carry
  it along without enumerating it, and both opaque sentinels are byte-identical afterwards.
- Conventional READMEs retain their absent, readable, and unreadable distinctions.
- Committed canonical mutations never become retry-shaped rejections because an index refresh
  failed.
- Metadata version 1, the checked-in schemas, the accepted and rejected fixtures, and the
  scenarios are unchanged.
- The currency inventory is complete and every blocked in-major update is named with its
  blocker.
- The milestone-wide review ran in its designed parallel form, every finding recorded with
  its disposition, and each capability defect returned to its smallest owning ticket.
- The acceptance record identifies either the closing SHA or each returned blocker.

## Exclusions

- **Capability fixes hidden in the gate.** Only defects owned by the integration seam itself
  are fixed here.
- M5 search and native providers, M6 and M7 desktop behavior, M8 agent work, and speculative
  M9 hardening.
- Adopting a new major found during the currency check.
- Declaring M4 externally validated. The gate accepts the milestone; a fresh worker validates
  it afterwards against the exact completed target.

## Result

The fourth run passed the gate and **accepted M4**, which is complete and awaiting external
validation. Every criterion's verdict, the candidate, the currency inventory, and the
boundaries no tool holds are [ACCEPTANCE.md](../ACCEPTANCE.md), rewritten whole and
committed before the milestone-wide review began.

That review ran in its designed parallel form and returned two blocking findings and four
smaller ones. None reached the bar the owner reserved for a further run — none makes a
machine response lie, loses data, or violates a closed decision — so the gate carried all
six into
[M5 Ticket 001](../../m5-search-and-native-services/tickets/001-one-answer-about-a-link-a-count-and-a-quantity.md)
rather than repairing what it was measuring. One of the six is a question for the owner
rather than a defect.

The composition and its failure surface are `crates/spielorders-cli/tests/m4_acceptance.rs`;
the inventory that proves the command surface both ways is
`crates/spielorders-cli/tests/cross_command_conformance.rs`.
