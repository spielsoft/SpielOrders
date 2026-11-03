---
title: Prove the platform adapter stops active queries
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

The production native-index adapter demonstrably stops platform work when its deadline
elapses or a caller cancels an active query.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. The production adapter can be exercised in its current location;
  shared-effects extraction is required before desktop wiring, not for this proof.

## Scope

- Exercise actual elapsed time and cancellation after platform work starts, through the
  production adapter's stopping boundary.
- Correct any behavior the checks expose and rewrite comments that overstate fake answers
  as proof that platform work stopped.

## Acceptance criteria

- A test lets the platform bound actually elapse against the adapter that must enforce it;
  a fake merely declaring a timeout does not satisfy the test.
- A test cancels a query after its platform work starts and proves that the same adapter
  stops that work; pre-cancellation or a fake declaring cancellation does not satisfy it.
- Each stopped query settles with the appropriate partial reason and retains available
  results under the decided snapshot rules.
- Checks verify cleanup of started work, do not rely on real-index contents, and cannot
  leave background work running after success or failure.
- The production stopping boundary is what the tests reach, and the suite's comments name
  only the timeout and cancellation guarantees actually demonstrated.

## Exclusions

- CLI signal-handler redesign, result streaming, or introducing an asynchronous core.
- Synthetic load, timing benchmarks, or an unattended real-index proof.
