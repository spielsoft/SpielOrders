---
title: Hoist the Package-valid Vendor check and narrow metadata repair's destination evidence
status: deferred
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
  - implementation:deferred
---

## Outcome

Two known, accepted departures in the M3 core are closed on their own, away from an
acceptance gate. Neither is a defect, and neither gates M3, M4, or any later milestone.
The roadmap's rule that proven low-level duplication may be marked for later evaluation
without blocking MVP is what admits them here.

## Decision sources

- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Durable writes](../../../../docs/architecture/durable-writes.md) — the destination-evidence rule and the carve-outs from it.

## Blocking

No blocker. The repository has one shared worktree, so do not claim this ticket while
another remaining ticket holds it.

## Scope

Two independent items. Either may be closed without the other.

1. **One `vendors_are_package_valid`.** `PackageMetadata::new(&version, &[], vendors)
   .validate()`, asking whether a candidate Vendor aggregate is a valid Package Vendor
   set, now stands in four places: `canonical.rs`, `vendors.rs`, `upgrade.rs`, and
   `semantic_repair.rs`, each with near-identical explanatory comments. Every
   per-ticket review that met it declined to hoist it, correctly, because the other
   sites were outside that ticket's review boundary. The M3 milestone review dissolved
   that boundary and named the duplication its headline finding. Extract one predicate
   over `&VendorsDocument` and route all four through it.

   While there: `semantic_repair.rs`'s copy is
   `vendor_aggregate_is_package_valid -> bool`, which discards the
   `Vec<PackageMetadataValidationFailure>` the validator returns and hard-codes
   `GeneratedVendorIdentityCollision` as the reported cause. Only a bug reaches it
   today, but any future package-wide Vendor rule would be misreported as an identity
   collision. The hoisted predicate should carry the verdict rather than flatten it.

2. **Metadata repair's destination evidence.** `metadata_repair.rs`
   `DiagnosedDestination::Content` maps to `DestinationExpectation::Exactly`, so a
   formatting-only rewrite of a *parseable* but invalid destination refuses the repair,
   where the semantic expectation managed records normally carry would accept it.
   Ticket 012 accepted this deliberately and recorded it: refusing is conservative and
   loses no canonical byte. The owner has since approved narrowing it, so byte evidence
   covers the destinations that genuinely have no parsed meaning and parseable ones are
   compared by meaning like every other managed record.

   `docs/architecture/durable-writes.md` states the current rule and its carve-outs. Any
   narrowing must land there in the same change, or the document becomes false.

## Acceptance criteria

- One predicate answers "is this Vendor aggregate a valid Package Vendor set" and all
  four former sites call it. No behavior changes; the existing suites prove it.
- A package-wide Vendor failure that is not an identity collision is no longer reported
  as one.
- Metadata repair compares a parseable diagnosed destination by meaning, and an
  unparseable one by bytes, with a test for each direction.
- `docs/architecture/durable-writes.md` and the M3 milestone README describe whatever
  rule the code ends up with.

## Exclusions

No new capability. No change to what any existing test asserts beyond the two behaviors
named above. Not a general repair-framework refactor.

## Review boundary

The four Vendor-validation sites and metadata repair's destination evidence. Do not
absorb the other findings the M3 milestone review accepted without change; they are
recorded in [ACCEPTANCE.md](../ACCEPTANCE.md) with the reasons they were accepted.

## Closeout

Not started. Deferred out of the M3 acceptance gate by owner decision rather than
carried out inside it: refactoring four call sites during a fixed-point acceptance run
risks the milestone close for tidiness that costs nothing to defer.
