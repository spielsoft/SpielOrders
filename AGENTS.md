# SpielOrders agent instructions

SpielOrders is a pre-release, local-first order manager whose owner is also its user. The
goal of every session is working software the owner can use. Use your judgment: read
whatever helps, decide what the repository and the decisions already answer, and say what
you decided and why. The rules here are the ones judgment cannot derive.

The [product decisions](issues/wayfinder/spielorders-product-architecture/MAP.md) say what
to build, the [roadmap](issues/roadmaps/spielorders-implementation-roadmap.md) says in
what order, and [Effect and adapter boundaries](docs/architecture/boundaries.md) says who
owns what.

## Invariants

- The library owns Package rules. A front end presents, and never recomputes a rule.
- Three surfaces are public: the declarations in `spielorders.h`, the on-disk metadata,
  and the CLI response contracts. A declaration the Swift app consumes changes only by
  addition; a breaking change needs the owner's approval.
- Nothing is versioned before release. A change revises the existing contract in place.
- Production code never exists to serve tests: no target, module, visibility, parameter,
  default, branch, field, conformance, or hook that only tests reach. Tests express
  behavior through a public interface and assert outcomes, never how an action was
  performed. The `test-cleanup` skill is the standard a test is kept to.
- The owner's data and machine come first. Never touch the real archive or the production
  app identity. Do not generate synthetic load. Take over the pointer, keyboard, or
  foreground only with the owner's agreement; driving the app in the background through
  its accessibility interface needs none. Release background jobs from a `trap`.
- Keep every toolchain and library on the newest minor and patch release within its
  major. Ask the owner before adopting a new major, and report a blocked update.

## Ways of working

**Direct change.** The default for anything you can state in a sentence that leaves the
three public surfaces as they are. Commit it with its test after `scripts/check.sh`
passes. There is no size limit.

**Milestone.** One builder owns a milestone from its brief to its acceptance.

1. Write the brief: one file, `issues/implementation/<milestone>.md`, holding the outcome,
   links to the decisions that govern it, the slices as a checklist, the acceptance
   checks, and the exclusions. Start building once it is written; show it to the owner
   first only if writing it required a product choice.
2. Build the slices in the order you judge best. Commit each coherent step, and tick a
   slice in the commit that completes it. Run `scripts/check.sh` as you go.
3. Run `scripts/gate.sh`, then hand the milestone's commit range to partner review.
4. After review findings are settled, run [Desktop acceptance](docs/development/desktop-acceptance.md)
   once on the final build.
5. The owner uses the app. Their sign-off accepts the milestone: delete the brief and
   reduce the milestone to one line in the roadmap.

A change to a public surface belongs in a milestone, or gets partner review of its own.

**Spike.** To answer a feasibility question before a design is committed, work on a
throwaway `spike/<question>` branch with no tests, review, or gate. The branch is never
merged. Its output is the answer, written into the decision or roadmap it settles.

## Partner review

Every milestone is reviewed by the other model, in a fresh session, using the
`code-review` skill. The reviewer reads the brief, the decisions, and the diff, and runs
whatever it needs. It looks for wrong behavior, lost data, a violated decision or
invariant, a reachable failure with no test, and a test that cannot fail.

The reviewer fixes an unambiguous defect directly, with its test. A finding that turns on
a product or interface choice goes to the owner. The builder does not write while the
reviewer holds the tree.

## Worktrees

Nobody builds in the main checkout. Every agent works on a branch in its own worktree
under `.agents/worktrees/`, and the branch is the claim on its work. It lands on `main` by
fast-forward and the worktree is removed afterwards. Stage the files you changed by name,
so a commit never carries another agent's work. Before starting, check that no other
branch is editing the same files.

## Questions

Decide what the repository answers. Ask the owner, or the peer session the owner
designates, only for a product choice no decision covers: one question at a time, with a
recommendation, while continuing everything that does not depend on the answer.

## Records

- Documentation keeps current constraints, future work, and lessons the code cannot
  teach. Git records how the state was reached. Delete a record when its work is done.
- Commit when code or a current fact changes. No commit exists only to record progress,
  a claim, or evidence.
- One home per fact. Everywhere else links to it by name.
- A document states the current decision in the present tense, with no dates, amendments,
  or rulings. When a decision changes, its sentence is rewritten and the commit body says
  why.
- A decision document describes what is not yet built. Once a surface is built and
  accepted, the document keeps the rules that constrain future change and points at the
  code for what exists.
- No counts, hashes, test totals, or version numbers in prose.
- A decision decides; code chooses. A fence in code enforces a sentence a decision
  states. Architecture is held by types, visibility, and clippy, never by a test that
  reads source.

If a document is wrong and the correct behavior is unambiguous, fix it in the same
commit. If two decisions conflict on a product question, stop the affected behavior and
ask.

This file changes only on a ruling from the owner or the peer session, named by date in
the commit body.
