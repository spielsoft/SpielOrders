---
title: Keep command checks portable across supported targets
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

Platform-specific interrupt checks no longer prevent the ordinary command test target
from building on a non-Unix platform.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. Test portability does not depend on provider behavior or desktop wiring.

## Scope

- Confine Unix-only interrupt APIs and assertions to the platforms whose process contract
  they express, retaining ordinary command checks elsewhere.
- Preserve coverage of provider availability on platforms without a native provider.

## Acceptance criteria

- The CLI command test target compiles for a supported non-Unix target without importing
  or calling Unix-only APIs there.
- Unix builds still run the interrupt check and verify the platform signal outcome,
  absence of a completed response, and preservation of Package contents.
- Platform-independent Supporting Material command checks remain enabled on non-Unix
  builds, including the explicit unavailable-provider behavior.
- Validation includes the relevant native test run and a non-Unix test-target build check;
  any unavailable cross-target toolchain is reported rather than claimed as validated.

## Exclusions

- Claiming a native provider exists on an unsupported platform.
- Replacing platform signal conventions or redesigning CLI cancellation.
