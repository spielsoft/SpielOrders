---
title: Use the offered Vendor list in production
status: open
parent: Say only what the code does, wherever M5's gate found otherwise
labels:
  - implementation:m6
  - implementation:task
---

## Outcome

Production Vendor listing consumes the core's offered-only Vendor read rather than
leaving that promised capability unused beside duplicate presentation filtering.

## Decision sources

- [M6 — Desktop shell and read experience](../README.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

- Successful external validation of [M5 — Search and native platform services](../../m5-search-and-native-services/README.md); pending.
- No child dependency. The existing CLI supplies a production consumer without desktop work.

## Scope

- Connect ordinary offered-only Vendor listing to the core capability already provided for
  that purpose.
- Preserve complete Vendor reads and identity resolution independently of visibility.

## Acceptance criteria

- The offered-only core Vendor list has a production caller in ordinary CLI Vendor listing;
  its use is not confined to a test or a demonstration.
- Executable checks prove the default list omits Hidden Vendors and the complete-list
  option retains every Vendor with its visibility.
- Showing or resolving a Hidden Vendor, including a referenced Vendor, remains possible;
  hiding does not delete the Vendor or invalidate an Order reference.
- The complete and offered-only core reads remain available as the capability contract
  promises, and comments describe the actual division between core reads and presentation.

## Exclusions

- Desktop Vendor controls or new Vendor-selection behavior.
- Vendor deletion, merge, or changes to hide/unhide semantics.
