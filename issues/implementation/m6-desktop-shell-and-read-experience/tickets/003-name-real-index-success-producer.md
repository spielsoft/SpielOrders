---
title: Name the producer of real-index success responses
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

A recorded Supporting Material success response has a named real-index producer and
repeatable comparison rules, without pretending an unattended run controls the native index.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. The existing opt-in native-service proof is the real-index producer.

## Scope

- Record the reachable Supporting Material success cell under the existing CLI capability
  contract and response-cell conventions, using a real index finding files.
- Use the existing opt-in native-service proof rather than introduce an unattended index
  dependency.

## Acceptance criteria

- Every reachable Supporting Material response cell has a recorded response. The nonempty
  success cell names the opt-in real-index producer and how its machine-readable response
  is checked; naming a producer does not substitute for recording the cell.
- The producer checks a successful nonempty response from the shipped executable rather
  than treating an empty answer, unavailable index, or handwritten response as success.
- Response-cell checks and comments distinguish producing a real-index response from
  comparing a recorded response automatically. Existing recording requirements are not
  weakened to excuse an omitted reachable cell.
- Recorded responses obey the canonical comparison and placeholder conventions. Any
  necessary placeholder addition explains why a real response needs it.
- Automatic contract checks remain independent of real-index contents, and the opt-in
  success proof remains outside the unattended gate.

## Exclusions

- Changing Supporting Material search semantics or weakening its response contract.
- Making native-index contents an unattended test prerequisite.
- General runtime-index-state tolerance, owned by
  [Check command responses without assuming an enabled index](009-check-runtime-index-states.md).
