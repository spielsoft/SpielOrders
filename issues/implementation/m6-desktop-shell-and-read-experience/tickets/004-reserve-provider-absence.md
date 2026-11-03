---
title: Reserve provider absence for builds without a provider
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

A Supporting Material provider cannot make a build that ships it report that the build
has no provider.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. This is an existing core-and-CLI boundary fix, not desktop wiring.

## Scope

- Enforce the difference between a missing provider and a present provider unable to
  answer, through the typed core result and CLI presentation.
- Correct tests or comments that currently admit the contradictory state.

## Acceptance criteria

- The provider-absence state is reachable only when the build supplies no provider; a
  supplied provider cannot return that state through its answer contract.
- A supplied provider still reports indexing disabled, volume not indexed, access denied,
  and query refusal distinctly, with its identity retained.
- Core boundary checks cover provider presence and absence, and CLI checks prove the
  corresponding machine-readable refusal and successful-answer behavior.
- Human presentation never describes a supplied provider as absent, without making human
  wording a parsing contract.

## Exclusions

- Adding native providers for other platforms.
- Recursive Supporting Material indexing or fallback search.
- Changing which platform provider the build supplies.
