---
title: Describe the production effect boundary accurately
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

A reader of the effect-boundary documentation can distinguish the core's effect ports,
test substitutes, and the production adapters the application actually supplies.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. Describing the current boundary does not require moving adapters.

## Scope

- Correct the claim that the application supplies only interfaces and fakes.
- Keep the enduring ownership rules and distinguish them from the implementation's current
  placement, linking to code for built behavior.

## Acceptance criteria

- The obsolete sentence in Effect and adapter boundaries is rewritten in place to agree
  with the production effects and test substitutes that exist.
- The document preserves core ownership of Package rules and adapter ownership of effects;
  it does not describe planned sharing as already implemented.
- The documentation's relative links resolve, and no implementation or behavioral test is
  changed to make the prose true.

## Exclusions

- Moving production adapters or changing their behavior.
- Reopening effect ownership or adding a shared foundation with another application.
