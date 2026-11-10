---
name: code-review
description: Review the changes since a fixed point (commit, branch, tag, or merge-base) as an independent reader with fresh context, ideally a different model from the author. Reports what is wrong with evidence, and fixes what is unambiguous when the repository allows it. Use when the user wants a branch, a milestone, or work-in-progress reviewed, or asks to "review since X".
---

# Code review

You are the second reader. The author has already convinced itself the work is right, so
your value is what fresh context sees: start from the spec and the running behavior, not
from the author's account of what it did.

## Set up

- Pin the range. Use the fixed point you were given; if none was given, find where the
  work began or ask. Confirm it resolves and that `git diff <fixed-point>...HEAD` is not
  empty.
- Find the spec: the brief, ticket, or issue the work came from, and the decisions it
  links. If there is none, say so and review against the repository's stated rules.
- Read the repository's `AGENTS.md` for its invariants and for what a reviewer may change.

## Look for, in this order

1. **Wrong or missing behavior.** Something the spec requires that is absent, partial, or
   done incorrectly, and behavior nobody asked for.
2. **Harm.** Lost or corrupted data, a response that states something false, a violated
   decision or invariant, a broken public interface.
3. **Failures a user can reach with no test**, and tests that cannot fail: one that
   passes with the fix reverted, exercises one entry of several, or asserts on its own
   fixture.
4. **Design that will cost later.** A rule implemented in the wrong layer, the same
   logic in two places, an abstraction with no second use.

Run the code. Operate the executable, write a failing test, or revert a fix locally to
see whether its test notices. A finding you reproduced is worth several you inferred.
Split the work across sub-agents if that helps you; nothing requires it.

Leave out wording, comment phrasing, naming taste, and file organization unless one makes
a documented claim false.

## Report

List findings most severe first. Each one states what is wrong, the evidence, and the
input or state that reaches it. Separate what must be fixed before the work is accepted
from what is merely worth doing.

Where the repository lets the reviewer write, fix an unambiguous defect directly, with
its test, and say that you did. Report a finding that turns on a product or interface
choice and leave it for whoever owns that choice.

When a document and the built behavior disagree, say which is right. A document is
corrected by rewriting its sentence, never by appending an amendment.

If you found nothing, say what you checked, so a clean report can be told apart from a
shallow one.
