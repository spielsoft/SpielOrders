# Driving agent work in one shared worktree

Decision sources: [SpielOrders agent instructions](../../AGENTS.md) and the
[M3 acceptance record](../../issues/implementation/m3-domain-mutations-safe-persistence/ACCEPTANCE.md).

This page is for whoever drives a milestone: a person handing tickets to fresh agents one
at a time, or a session doing that on their behalf. It covers what goes wrong when work is
delegated, not what the work is. For the commands themselves see
[Running the Rust tests](test-loop.md).

Everything here was paid for during M3, which took six worker sessions to close four
tickets. Not one of the six was lost to a mistake about the domain.

## One worktree, one writer

The repository has a single shared worktree. A ticket claims it and releases it at close,
so at most one ticket may be in progress across every milestone at once — including the
bounded harness milestone, which competes for the same tree.

Two writers in that tree corrupt each other silently. The second writer sees ordinary
uncommitted edits, cannot tell them from abandoned scratch work, and reverting them is a
reasonable thing for it to do.

Before starting a worker, confirm no ticket anywhere is `in progress`. Before starting a
*replacement* worker, also confirm the previous one is no longer running — which is a
different question, and a harder one.

## A quiet tree is not a finished worker

**A clean worktree with no new commits is not evidence that a worker has stopped.** It
looks exactly the same whether the worker died or is simply thinking between turns.

Establish that a worker is finished only from the process itself: an explicit final
report, or an agent listing that shows it is no longer running. A remark that nothing
appears to be running is a reason to check, not a fact to act on.

During M3 a driver inferred death from a quiet tree, started a replacement, and put two
writers in the one worktree. The replacement noticed within three minutes and stopped
itself without committing, which is the only reason the cost was one destroyed test
function rather than a corrupted ticket. Recovery was routed back to the surviving owner,
because writing into a worktree someone else holds is the original mistake repeated.

To abandon a worker, stop it explicitly and confirm it stopped. Then start the replacement.

## Commit as you go

Every interruption during M3 was survivable for exactly one reason: the interrupted worker
had committed. Uncommitted work is the only work that was ever lost.

A worker that batches its ticket into one commit at the end is one watchdog timeout away
from losing hours. Commit each coherent step. The claim commit, each behavioural change,
the review remediation, and the closure are all separate anyway.

This also makes a resume cheap. `git log <review_start>..HEAD` tells the next worker what
was already done, so it can continue instead of starting over and re-deciding settled
questions.

## Resume, do not restart

An interrupted ticket is usually most of a finished ticket. Read the tree before replacing
the work:

- Keep the ticket's original `review_start`. It is the review fixed point and it does not
  move because a worker was replaced.
- Establish what is already committed and say so explicitly in the resume instruction.
  A worker told only "finish this" will re-derive what already exists.
- Classify the uncommitted remainder before touching it. Comment-only refinement, an
  in-flight behavioural change, and abandoned scratch are three different situations.
- Have the resuming worker verify the previous worker's acceptance criteria against the
  tree rather than against its commit subjects. Doing so during M3 found two real defects
  a commit message had implied were handled.

When naming an artifact that must not come back, name it by its exact identifier and say
whether the underlying scenario may still be tested. "The scratch probe must not return"
was read as forbidding the properly named test correctly derived from that probe, and the
test was deleted.

## Review is the fragile step

Review is where delegated work fails, and it fails quietly. The mechanics are normative
and live in [SpielOrders agent instructions](../../AGENTS.md); the operational consequences
are these.

**Findings must be written down as they are produced.** A review's value exists only in the
reviewing agent's context until something records it. M3 lost a completed Spec axis that
way — substantial findings, reported to nobody, discovered only because the parent died
immediately afterwards. Have each axis append to a file as it reads.

**A driver may run the axes itself.** Running the two axes directly, against a stated fixed
point, with neither axis seeing the other's conclusions, is the designed parallel form —
the form is about two independent readers, not about who invoked them. When workers cannot
hold a review to completion, this is the escalation, and it is not a downgrade. Substituting
one reader for two is the downgrade, and that remains blocked.

**Reviewers verifying non-vacuity share the worktree.** A reviewer that proves a test
really fails by mutating the source is a writer, and two parallel axes doing it at once are
two writers in the one tree. Observed during M3 remediation: one axis saw the other's
mutation mid-flight and reported transient failures. It discounted them correctly, which is
the good case — the bad case is an axis discounting a *real* failure as the other's
interference, and reporting clean. Either give each axis its own clone, or have them report
the mutation they want performed rather than performing it. Tell them they are sharing a
tree so a surprising failure is interpreted rather than assumed.

**Do not pre-suppress findings.** A reviewer told in advance which findings are already
accepted is no longer independent. Let both axes report clean and apply known dispositions
when aggregating. The milestone-wide review that closed M3 found a blocking defect —
semantic repair's committed-with-warning path had no test at all — that every per-ticket
review had passed over, because each saw only its own slice.

## What the milestone review is for

Per-ticket review reads one ticket's range. The milestone review reads across them, and
should be pointed at what no single-slice reader could see: a contract one ticket relies on
that another quietly changed, a shape that only became duplication once its third occurrence
landed, vocabulary that drifted between capabilities, and acceptance criteria that are
nominally met and substantively not.

It is also the only reader positioned to dissolve a per-ticket review boundary. When a
ticket declines a cleanup because the fix lives outside its boundary — a legitimate
refusal — the milestone review is where that decision is revisited. Revisiting it does not
mean doing it there: deferring the work to a named follow-up ticket keeps an acceptance
gate from carrying a refactor's risk.
