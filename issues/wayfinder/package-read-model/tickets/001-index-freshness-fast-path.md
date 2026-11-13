---
title: Decide how Package Index freshness is proven without canonical rereads
status: closed
---

## Decision

The [core read model](../../../../crates/spielorders/src/canonical.rs) owns
index assessment, refresh, and loading. Its tests define concrete observation budgets
and typed outcomes.

The Package Index is derived, never a second authority. A fast load trusts it only
after valid shape/semantics, exact candidate membership, ordinary no-follow file kinds,
and conservative filesystem change evidence establish freshness. The index replacement
must be strictly newer than the projected inputs. Equal, uncertain, incomparable, or
contradictory observations require canonical reading. This assumes a friendly workspace,
not an attacker forging both index bytes and filesystem bookkeeping.

No timestamp, filesystem identity, diagnostic, or revision sidecar is added to the
metadata merely to accelerate loading. Fast-path work stays at managed artifacts,
direct-child candidate discovery, and exact conventional README paths. Supporting
Material is never scanned.

Canonical rebuilding projects only healthy Orders and current Vendors in deterministic
order. Package-wide canonical damage prevents rebuilding; Order-local damage does not
become Package damage. Required replacement failure blocks ordinary opening but remains
distinct from canonical damage.

A missing README is empty; an unreadable README is unknown, not empty. If required
README text cannot be established, do not publish a cache inventing that text. Preserve
the existing index and the actual observation. Cached text is not evidence that a README
is currently readable or writable.

[Define durable metadata schemas and version evolution](../../spielorders-product-architecture/tickets/010-metadata-schemas.md)
owns the persisted projection. A stronger cache trust scheme requires a deliberate
decision, not an adapter workaround.
