# Durable metadata contracts

The schemas in `schemas/v1` are strict JSON Schema Draft 2020-12 documents for
metadata version 1. Minimal and fully populated accepted examples live in
`examples/v1`.

Decision sources: [Define the SpielOrders package topology](../issues/wayfinder/spielorders-product-architecture/tickets/002-package-topology.md), [Define the Order record and lifecycle contract](../issues/wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md), [Define Vendor identity and historical semantics](../issues/wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md), [Define the Rust CLI capability contract](../issues/wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), and [Define durable metadata schemas and version evolution](../issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).

Schema validation covers document shape and locally expressible scalar constraints.
It does not replace Rust semantic validation for real Gregorian dates, finite
target-language numeric representation, filesystem resolution, cross-record identity
or Vendor Name uniqueness, live Vendor Reference resolution, Package-wide version
agreement, or index ordering and freshness.

`config.toml` is unversioned and is therefore not represented here. CLI response
contracts have independent versions and must not reuse these schemas.

