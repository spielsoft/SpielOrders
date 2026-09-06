---
title: Validate Package identities, Vendors, live references, and reusable values
status: closed
assignee: codex
claimed_at: 2026-07-23
closed_at: 2026-07-23
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:task
---

## Outcome

A caller can validate a steady-state typed metadata set across Orders and Vendors
and canonicalize a proposed Reusable Free-Entry Value against current Package values.

## Decision sources

- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Validate an Order and calculate its exact totals through the core](002-order-validation-and-totals.md)
and Ticket 001 closed on 2026-07-22.

## Scope

- Validate Vendor and Contact facts.
- Enforce unique Order IDs, Vendor IDs, and trimmed Unicode-case-insensitive Vendor
  Names.
- Require every present Vendor Reference to resolve to exactly one Vendor.
- Validate steady-state agreement among the Package marker and artifact schema
  versions.
- Canonicalize Reusable Free-Entry Values as absent, reused with existing spelling,
  or deliberately new.
- Apply local semantic validation to canonical and index-embedded domain values.

Exclude Vendor mutations, deletion, Vendor Merge, repairs, inferred Vendors,
Package loading, diagnostic scoping, index freshness/rebuild, and upgrade-state
interpretation.

## Pre-agreed public test seam

Public Package-metadata validation operates on already parsed values. A small public
Reusable Free-Entry Value operation returns absent, reused, or new. The tracer bullet
validates one Order referencing one existing Vendor; it catches name-based inference
or a reference accepted without a live ID match.

## Acceptance criteria

- Duplicate Order IDs identify every conflicting Order value as invalid.
- Duplicate Vendor IDs or normalized Vendor Names invalidate the aggregate.
- Unresolved Vendor References fail while absent references remain valid.
- Orders retain only Vendor IDs and therefore resolve renamed Vendor facts live.
- Reusable values trim input and reuse the existing canonical spelling under the
  decided comparison.
- No validation path creates, guesses, or merges a Vendor.

## Review boundary

Review cross-record semantics over typed values only. Filesystem scope, Package
loading, diagnostics, and mutation belong elsewhere.

## Ticket-start fixed point

`fd53cb812db6886652b6d3cb3eed25a0768bc1c5`

## Resolution

Added public typed Package-metadata validation over already parsed version-1
artifacts. The validator returns normalized Order, Vendor, Contact, and optional
index-embedded domain values; reports every position participating in duplicate
Order IDs, Vendor IDs, or trimmed full-Unicode-case-folded Vendor Names; rejects
unresolved Vendor References while preserving absence; and resolves current Vendor
facts solely through Vendor ID. Version agreement is enforced by the shared
`Version1` typed boundary, which cannot represent a mismatched accepted artifact.

Added a separate public Reusable Free-Entry Value operation that trims proposed
input and returns absent, the first matching existing canonical spelling under full
Unicode case folding, or a deliberately new trimmed value. No Vendor mutation,
merge, repair, inference, Package loading, diagnostic, index freshness/rebuild,
upgrade interpretation, filesystem, persistence, or presentation behavior was
added.

Implementation commits:

- `a909143` — Package/Vendor semantics and public behavioral tests.
- `2f4f110` — accepted `/code-review` maintainability and type-safety findings.

Acceptance evidence (2026-07-23, Rust 1.97.1):

- `cargo test -p spielorders-core --test package_metadata` — passed 8 public
  behavior tests covering live/absent/unresolved references, every duplicate
  identity position, full-Unicode Vendor Name uniqueness, Vendor/Contact
  normalization, reusable values, and index-embedded Order semantics.
- `cargo test --workspace` — passed the complete Rust workspace suite.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- Dependency currency audit added current `focaccia` 2.0.0 and updated
  `foreign-types-macros` 0.2.3 to 0.2.4 and `syn` 3.0.2 to 3.0.3. A final
  `cargo update --dry-run --verbose` found no compatible lockfile updates; remaining
  older transitive releases are constrained upstream, and `toml` 1.x remains an
  unapproved major version.
- `/code-review` from the exact ticket-start fixed point reported zero Spec
  findings and no documented-standard violations. Three Standards-axis judgments
  were accepted and resolved by sharing duplicate-ID collection, sharing optional
  text normalization, and retaining typed `VendorId` in validation failures. These
  internal cleanups did not materially change the design, so a second review run
  was not required.
