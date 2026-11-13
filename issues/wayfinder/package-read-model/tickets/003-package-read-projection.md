---
title: Define the complete Package read projection and load-state contract
status: closed
---

## Decision

The [Rust read model](../../../../crates/spielorders/src/canonical.rs) is the
record of load variants, projection fields, and supported paths. Do not mirror its
type inventory in planning prose.

Keep unmanaged selection, available current data, diagnostic-only damage, pending
upgrade, index replacement failure, and failed operations distinct. Missing context
is not an empty healthy Package. Adapters present those results without reconstructing
Package rules.

Paths describe location, not identity. Only established membership permits a
Package-relative location. Healthy Orders resolve current Vendor facts and calculated
values through core. Damaged Orders remain path-based placeholders, not partially
trusted editable entities.

A fast load supplies only the facts its index and observations establish. An explicit
README read supplies current document availability; cached text cannot grant editing
authority. Exhaustive validation may establish additional evidence but does not
retroactively change what an earlier fast load examined.

The filesystem port supplies observations and effects. It does not choose load policy.
Swift, CLI, and other adapters do not implement their own freshness, diagnosis,
projection, Vendor resolution, or deterministic search rules.

[Define the SpielOrders package topology](../../spielorders-product-architecture/tickets/002-package-topology.md),
[Define durable metadata schemas and version evolution](../../spielorders-product-architecture/tickets/010-metadata-schemas.md),
and [Define integrity, validation, and repair boundaries](../../spielorders-product-architecture/tickets/008-integrity-repair.md)
own the corresponding domain constraints.
