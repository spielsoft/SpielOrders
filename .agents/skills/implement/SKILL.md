---
name: implement
description: "Implement one approved spec or implementation ticket, with scoped context, proportionate verification, review, and a concise handoff. Do not use for unresolved product decisions or external-validation-only work."
---

# Implement

Implement exactly the work the user selected. For a ticketed milestone, complete
one independently reviewable ticket and stop.

## Orient and claim

- Follow the reading rule in [`AGENTS.md`](../../../AGENTS.md): this file, the milestone
  index's ticket table, the one ticket, and the decision documents that ticket links.
  Nothing else is read by default.
- Verify the worktree, ownership, blockers, and fixed point required by the local
  tracker. Claim the ticket before implementation and record the fixed point when
  the tracker requires it. Stop if the ticket is already claimed or blocked.
- Wayfinder is for unresolved decision maps, not implementation-ticket execution.

## Build the ticket

- First check whether the work qualifies for the small-change lane in
  [`AGENTS.md`](../../../AGENTS.md). If it does, take it and stop reading this skill.
- Use `/tdd` for observable behavior additions and defects at an agreed public
  seam. An already approved ticket supplies planning approval; ask the user only
  when a material product or interface choice remains unresolved.
- Do not invent a RED test for documentation-only, evidence-only, or lock-graph-
  only work.
- Keep changes within the selected ticket. Return a pre-existing defect to its
  smallest owner instead of absorbing unrelated work.
- Run `scripts/loop.sh` during development and `scripts/gate.sh` once the
  implementation is stable.

## Review, close, and hand off

- Commit the implementation, then run the project `code-review` skill against the
  ticket's fixed point in its two-axis parallel form. Fix every blocking finding. If the
  parallel mechanism is unavailable, stop and report; never substitute one reader.
- Write the `## Result`, at most 150 words: what shipped, where the tests are, and links
  to any follow-up tickets, in the present tense: what exists now, not what was routed,
  deferred, or changed along the way.
- Set the ticket frontmatter closed and update its row in the milestone table. Commit
  that closure.
- Report the closing SHA, the gate result, and the next eligible ticket. Nothing else.
- Never copy the ticket, milestone rules, commands, versions, or hashes into a
  ready-to-paste recursive prompt.

Commit the completed work to the current branch unless the user or repository
explicitly requests a different handoff.
