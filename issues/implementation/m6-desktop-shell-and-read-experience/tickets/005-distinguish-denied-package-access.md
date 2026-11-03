---
title: Distinguish denied Package access from other read failures
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

A caller denied access while loading a Package receives that explanation instead of an
indistinguishable generic read failure.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. Existing Package loading can be corrected before desktop wiring.

## Scope

- Preserve denied-access evidence from filesystem effects through Package loading and
  callers that expose the failure, including Supporting Material search before it asks
  the provider.
- Keep other read failures and canonical damage distinct from denied access.

## Acceptance criteria

- A Package directory the process may not read is distinguished from a read that fails
  for another reason wherever a caller can reach that difference.
- Core checks use controlled denied and non-denied failures; executable checks verify
  the caller-visible difference without depending solely on the test runner's privileges.
- Supporting Material search reports denied Package access specifically even when loading
  prevents the provider from being asked.
- Existing machine-readable failure discipline, scope, and known location evidence are
  preserved; human errors explain the denial without becoming a parsing contract.
- Failed reads do not mutate the Package or trigger fallback inspection of Supporting
  Material, and affected response checks and comments agree with the corrected behavior.

## Exclusions

- Permission repair, privilege escalation, or changes to operating-system access controls.
- Reclassifying canonical metadata damage as denied access.
