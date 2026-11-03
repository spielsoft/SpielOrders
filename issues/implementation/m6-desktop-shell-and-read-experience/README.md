---
title: M6 — Desktop shell and read experience
status: not started
parent: SpielOrders implementation roadmap
labels:
  - implementation:milestone
  - implementation:m6
---

## Scope

A Tauri desktop shell over the same core the CLI uses, with the Orders and Vendors tables,
both search modes, Damaged Order placeholders, and the two README editors — reached through
platform adapters that live where both adapters can depend on them.

Governing decision sources:

- [Define lookup and search behavior](../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../docs/architecture/boundaries.md)

Only the follow-up under [Say only what the code does, wherever M5's gate found otherwise](tickets/001-say-only-what-the-code-does.md)
is decomposed here. That parent remains an open umbrella, not a selectable implementation
ticket; publishing its children does not satisfy its acceptance criteria. Its children
exclude contract or metadata version advances and desktop implementation.

The remaining desktop deliverables are not decomposed here. The shared platform-effects
extraction in the [SpielOrders implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md#m6--desktop-shell-and-read-experience)
must precede desktop wiring; it does not block independent fixes to the existing CLI.

## Acceptance gate

The milestone acceptance gate is the M6 section of the
[SpielOrders implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md#m6--desktop-shell-and-read-experience).
Successful external validation of [M5 — Search and native platform services](../m5-search-and-native-services/README.md)
is a prerequisite for every implementation child below and is pending.

## Ticket order and dependencies

| Order | Ticket | Status | Blocked by |
| --- | --- | --- | --- |
| — | [Say only what the code does, wherever M5's gate found otherwise](tickets/001-say-only-what-the-code-does.md) | Open umbrella | Not selectable for implementation; child outcomes |
| 1 | [Describe the production effect boundary accurately](tickets/002-describe-production-effect-boundary.md) | Open | M5 external validation pending |
| 2 | [Name the producer of real-index success responses](tickets/003-name-real-index-success-producer.md) | Open | M5 external validation pending |
| 3 | [Reserve provider absence for builds without a provider](tickets/004-reserve-provider-absence.md) | Open | M5 external validation pending |
| 4 | [Distinguish denied Package access from other read failures](tickets/005-distinguish-denied-package-access.md) | Open | M5 external validation pending |
| 5 | [Prove the platform adapter stops active queries](tickets/006-prove-platform-query-stopping.md) | Open | M5 external validation pending |
| 6 | [Use the offered Vendor list in production](tickets/007-use-offered-vendor-list.md) | Open | M5 external validation pending |
| 7 | [Keep command checks portable across supported targets](tickets/008-keep-command-checks-portable.md) | Open | M5 external validation pending |
| 8 | [Check command responses without assuming an enabled index](tickets/009-check-runtime-index-states.md) | Open | M5 external validation pending |
