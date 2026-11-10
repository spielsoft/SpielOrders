---
name: implement
description: "Build approved work, from one change up to a whole milestone, and hand it off ready for review. Do not use for unresolved product decisions."
---

# Implement

Build the work the user selected, all of it, and hand it off ready for review. The
repository's `AGENTS.md` says how work is organized there, how it is checked, and who
reviews it; follow it where it differs from this skill.

## Orient

Read what helps: the brief or spec, the decisions it links, and the code you will change.
Approved work is planning approval. Decide what the repository and its decisions already
answer, and ask only when a product or interface choice is still open. Keep building
everything that does not depend on the answer.

## Build

- Work in vertical slices: each one a complete path a user or caller can exercise.
- Test behavior at a public seam, as `/tdd` describes. Work with no observable behavior,
  such as documentation, needs no invented test.
- Commit each coherent step. Uncommitted work is the only work that is ever lost.
- Fix a defect you meet on the way when it is small and in your path. Report one that
  is not, and leave it alone.
- Run the fast checks as you go and the full gate once the work is stable.

## Hand off

Report what now exists, in the present tense; which checks ran and passed; what you could
not verify, said plainly; and each judgment call you made, with its reason. Name the
commit range for review. Do not paste the spec, commands, or a prompt for the next agent.
