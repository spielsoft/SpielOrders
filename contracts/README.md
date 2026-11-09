# Durable metadata contracts

The schemas in `schemas/v1` are strict JSON Schema Draft 2020-12 documents for
metadata version 1. Minimal and fully populated accepted examples live in
`examples/v1`.

One version-1 field is deliberately absent from the fully populated Vendor example:
`hidden`, whose absence *is* the canonical spelling of a visible Vendor. Many Rust
suites load `examples/v1/full` as a live SpielOrder Package, and a Hidden Vendor there
would quietly change what those suites exercise. The field's accepted and rejected
shapes are covered instead by [Rust schema checks](../crates/spielorders-checks/src/schemas.rs) and by
`fixtures/schema-invalid/vendor-non-boolean-hidden.json`.

Decision sources: [Define the SpielOrders package topology](../issues/wayfinder/spielorders-product-architecture/tickets/002-package-topology.md), [Define the Order record and lifecycle contract](../issues/wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md), [Define Vendor identity and historical semantics](../issues/wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md), [Define the Rust CLI capability contract](../issues/wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), and [Define durable metadata schemas and version evolution](../issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).

Schema validation covers document shape and locally expressible scalar constraints.
It does not replace Rust semantic validation for real Gregorian dates, finite
target-language numeric representation, filesystem resolution, cross-record identity
or Vendor Name uniqueness, live Vendor Reference resolution, Package-wide version
agreement, or index ordering and freshness.

`config.toml` is unversioned and is therefore not represented here.

CLI response contracts are versioned one per command rather than together, so they
live in their own subtree: `cli/<contract path>/v<contract_version>/`, holding one golden
response document per response kind. `docs/development/traceability.md` records the
convention.

