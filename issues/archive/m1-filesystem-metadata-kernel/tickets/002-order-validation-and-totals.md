---
title: Validate an Order and calculate its exact totals through the core
status: closed
assignee: codex
claimed_at: 2026-07-22
closed_at: 2026-07-22
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:task
---

## Outcome

A caller can submit a typed Order to the core, receive every Order-local semantic
failure, and obtain unrounded Item Total Prices and Order Total for a valid Order.

## Decision sources

- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Round-trip the complete metadata-version-1 vocabulary through the Rust core](001-metadata-v1-vocabulary.md)
closed on 2026-07-22.

## Scope

- Validate lowercase hyphenated UUIDv4 Order and referenced Vendor IDs.
- Validate nonblank required facts, real Gregorian dates, status, uppercase
  three-letter currency, and finite supported Quantity and Unit Price values.
- Apply the decided single-line and multiline normalization rules.
- Calculate each Item Total Price and the Order Total without rounding.

Exclude lifecycle-transition mutation, local-date effects, directory projection or
alignment, presentation rounding, Vendor lookup, Package-wide uniqueness,
filesystem I/O, persistence, and Item editing workflows.

## Pre-agreed public test seam

Public Order validation and total-calculation behavior operates on typed metadata.
The first tracer bullet validates the minimal accepted Order and returns an empty
total. Priority regressions catch impossible dates, blank facts, constructed
non-finite Rust values, and accidental presentation rounding in core arithmetic.

## Acceptance criteria

- Minimal and full accepted Orders validate.
- Impossible dates, malformed identities, blank required facts, and non-finite
  supported-float values fail.
- Future dates, all five statuses, zero Items, and negative or zero finite numeric
  values remain valid.
- Item and Order totals equal their exact unrounded floating-point calculations.
- Validation creates no status-dependent completeness or plausibility warning.

## Review boundary

Review Order-local semantic truth and calculations only; Package and mutation
behavior remain outside this ticket.

## Ticket-start fixed point

`1bcd83468ada53546cbb4a89cfbfe726bcbf6235`

## Resolution

Added public Order-local validation that returns a normalized typed Order together
with array-ordered Item Total Prices and an unrounded Order Total. Validation
enforces real Gregorian dates, including century leap-year rules, while the typed
metadata boundary continues to reject malformed identities, blank required facts,
unsupported statuses and currencies, and constructed non-finite Item values.
Order single-line facts are trimmed and blank optional facts become absent. No
status-dependent completeness, plausibility, Vendor lookup, Package, filesystem,
persistence, lifecycle-mutation, or presentation behavior was added.

Acceptance evidence (2026-07-22, Rust 1.97.1):

- `cargo test -p spielorders-core --test order_validation` — passed 8 public
  behavior tests covering minimal/full Orders, Gregorian dates, normalization, all
  five statuses, future dates, zero Items, signed finite values, and unrounded
  totals.
- `cargo test -p spielorders-core --test metadata_v1` — passed 18 metadata tests,
  including malformed identities, blank required facts, currency/status shape,
  and constructed non-finite Item-value rejection.
- `cargo test --workspace` — passed the complete Rust workspace suite.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- `/code-review` from the exact ticket-start fixed point reported zero Spec
  findings. The Standards axis reported one redundant already-GREEN regression and
  two maintainability judgments; all three were accepted and resolved without a
  material public-design change, so a second review run was not required.
