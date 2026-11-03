---
title: Check command responses without assuming an enabled index
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

The command suite checks what a native-index query actually answered, so a disabled or
unavailable machine index cannot make a correct build fail.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. The existing command outcomes can be checked without changing
  provider contracts or moving production effects.

## Scope

- Apply the distinction between a provider shipped by the build and an index able to
  answer this run across Supporting Material command, response, and conformance checks.
- Keep meaningful assertions for successful, partial, unavailable, and provider-absent
  outcomes instead of skipping the command when the environment cannot answer.

## Acceptance criteria

- No automated test fails solely because the host index is disabled, its volume is not
  indexed, or a query settles with a legitimate unavailable or partial outcome.
- A build that ships a provider must still name it when answering and cannot satisfy the
  checks by claiming that no provider exists.
- Each reached outcome verifies the appropriate response identity, success/failure and
  process-status agreement, payload presence or absence, and applicable stream discipline.
- Available complete and partial answers are checked distinctly, without assuming that a
  provider's presence guarantees completion or any particular indexed file.
- Controlled checks exercise the runtime-state branches, while real-provider invocations
  remain independent of index contents. Obsolete comments are rewritten to match.

## Exclusions

- Silencing, skipping, or treating all Supporting Material failures as acceptable.
- The real-index nonempty-success producer, owned by
  [Name the producer of real-index success responses](003-name-real-index-success-producer.md).
- Changing provider availability or search semantics to satisfy the test environment.
