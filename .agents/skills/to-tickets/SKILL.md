---
name: to-tickets
description: Break a plan, spec, or the current conversation into vertical slices with their dependencies, published as one brief with a slice checklist, or as separate tickets when several agents or an external tracker need them.
---

# To Tickets

Break a plan, spec, or conversation into **slices**: narrow, complete paths through every
layer, each one verifiable on its own, with the slices that must come before it named.

## Gather context

Work from what is already in the conversation. If the user passes a reference, read it in
full. Explore the code to learn its current state, use the project's glossary vocabulary,
and respect the decisions in the area you are touching. Look for a prefactor that makes
the work easier: make the change easy, then make the easy change.

## Draft the slices

- Each slice cuts through every layer the behavior needs, such as schema, interface, UI,
  and tests. A slice of one layer is not a slice.
- A finished slice can be demonstrated or verified by itself.
- Size a slice by what is coherent to verify, not by a line or session budget. One
  capable agent can carry a whole milestone of them.
- Any prefactoring comes first.
- Say which slices block which. Most sets are a short chain or have no edges at all.

Describe each slice by its observable outcome and the checks that show it works, plus
what it excludes when a reader could mistake the boundary. Link the decisions that govern
it instead of copying them. Leave out predicted types, response shapes, and file paths:
implementation discovers those, and a prediction that turns out wrong becomes a
contradiction someone has to resolve. Describe the outcome, not how the decision was
reached or who made it.

**Wide refactors are the exception to vertical slicing.** A wide refactor is one
mechanical change, such as renaming a column or retyping a shared symbol, whose blast
radius fans across the codebase so that no vertical slice can land green. Sequence it as
expand and contract. First add the new form beside the old so nothing breaks. Then
migrate call sites in batches sized by blast radius, each keeping the checks green
because the old form still exists. Finally delete the old form once no caller remains.
When even the batches cannot stay green alone, let them share an integration branch and
promise green only at a final integrate-and-verify step.

## Confirm only when a choice is open

A request to plan already-approved work authorizes publishing the plan. Present the
breakdown for approval first only when the source is exploratory or a choice would change
scope, ownership, or sequencing, and then ask just the question that choice needs.

## Publish

Use the form the repository's `AGENTS.md` names. Where it names none:

- **One builder.** Write one brief: the outcome, links to the governing decisions, the
  slices as a checklist in build order, the acceptance checks, and the exclusions. The
  builder ticks a slice in the commit that completes it.
- **Several agents in parallel, or an external tracker.** Publish one ticket per slice,
  blockers first so each can reference the ones before it, using the tracker's native
  blocking links where it has them.

<brief-template>

# <Milestone or feature>

## Outcome

What a user can do when this is finished.

## Decisions

Links to the decisions that govern the work.

## Slices

- [ ] <Slice>: the behavior it makes work and the check that shows it.

## Acceptance

The checks that show the whole works together.

## Exclusions

What a reader might expect here that belongs elsewhere.

</brief-template>

<ticket-template>

## What to build

The end-to-end behavior this slice makes work, from the user's perspective.

## Acceptance criteria

- [ ] Criterion

## Blocked by

A reference to each blocking ticket, or "None".

</ticket-template>
