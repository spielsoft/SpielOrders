---
name: implement
description: "Implement one approved spec or implementation ticket, with scoped context, proportionate verification, review, and a concise handoff. Do not use for unresolved product decisions or external-validation-only work."
---

# Implement

Implement exactly the work the user selected. For a ticketed milestone, complete
one independently reviewable ticket and stop.

## Orient and claim

- Read `AGENTS.md`, the milestone index, the selected ticket, and the outcome or
  frontmatter of its direct blockers. Read linked decisions and prior-ticket detail
  only when the selected ticket or changed code needs them.
- Do not ingest every milestone ticket, historical evidence record, or command
  transcript by default. Search and zoom in as questions arise.
- Verify the worktree, ownership, blockers, and fixed point required by the local
  tracker. Claim the ticket before implementation and record the fixed point when
  the tracker requires it. Stop if the ticket is already claimed or blocked.
- Wayfinder is for unresolved decision maps, not implementation-ticket execution.

## Build the ticket

- Use `/tdd` for observable behavior additions and defects at an agreed public
  seam. An already approved ticket supplies planning approval; ask the user only
  when a material product or interface choice remains unresolved.
- Do not invent a RED test for documentation-only, evidence-only, or lock-graph-
  only work.
- Keep changes within the selected ticket. Return a pre-existing defect to its
  smallest owner instead of absorbing unrelated work.
- Run focused checks during development. Run the ticket's affected repository
  gates once after the implementation is stable.

## Verify proportionately

Classify the final diff before choosing the closeout checks:

- **Behavior or executable inputs:** run focused tests plus the affected build,
  lint, type, contract, and repository gates. Run a required full suite once at
  the end, then rerun only gates affected by review remediation.
- **Dependency or toolchain selection:** refresh authoritative currentness, apply
  the repository's selected-major policy, recreate the affected dependency trees,
  and run the builds and tests those selections can affect.
- **Documentation, status, or evidence only:** run diff/whitespace, link,
  traceability, and consistency checks that cover the changed claims. Do not
  repeat clean installs, builds, or executable suites unless the documents are
  executable inputs or the change invalidates prior executable evidence.

A later documentation-only commit does not invalidate a completed executable gate
when source, tests, fixtures, manifests, lockfiles, scripts, contracts, and
toolchain selectors are unchanged. Record the gate's target and prove the later
path inventory instead of restarting it.

## Review, close, and hand off

- Commit the implementation and evidence, then use `/code-review` against the ticket's
  fixed point. Run it in its designed parallel sub-agent form, Standards and Spec
  independent. Resolve blocking findings and explicitly record any accepted
  non-blocking findings. Rerun only affected checks.
- A single-reader pass does not satisfy this. If the parallel mechanism is unavailable,
  stop and report it as a blocker rather than substituting a weaker form — a review
  that silently runs at reduced strength is worse than one that visibly did not run.
- Commit the ticket resolution and tracker status as required by the repository.
- Hand off a pointer, not a generated runbook: report the completed ticket, final
  SHA, next eligible ticket (or blocker), and the skill to invoke. The fresh agent
  should derive current instructions from repository sources.
- Never copy the ticket, milestone rules, commands, versions, hashes, or this
  handoff rule into a ready-to-paste recursive prompt.
- When a milestone closes, give the validation worker only the milestone path,
  implementation fixed point, completed target, permitted writes, and prohibition
  on later-milestone work. The validation plan remains in the repository.

Commit the completed work to the current branch unless the user or repository
explicitly requests a different handoff.
