# SpielOrders implementation roadmap

## Current boundary

The Rust library and CLI are the implemented foundation; the
[library interface](../../crates/spielorders/include/spielorders.h) and
[contract fixtures](../../contracts/README.md) record available behavior. The native
desktop foundation links that library directly. [The product decisions](../wayfinder/spielorders-product-architecture/MAP.md)
define its capabilities. [Native desktop distribution](../../docs/adr/0001-native-desktop-distribution.md)
defines deployment constraints; [Effect and adapter boundaries](../../docs/architecture/boundaries.md)
defines ownership and API coordination.

Institutional forms, the shared purchasing workflow, collaboration, OCR, production
archive migration, structured post-MVP filters, a network service, and shared
SpielGantt foundations are outside this roadmap.

## Compatibility before release

CLI and agent tooling remain unreleased until the owner declares them released. Before
release, metadata and CLI response changes revise their
existing contracts in place. The [metadata](../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
and [CLI](../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
decisions govern compatibility. A different frontend alone needs no metadata change.

## Delivery dependencies

| Milestone | Status | Prerequisites |
| --- | --- | --- |
| R1 — Rust backend reduction | Accepted | None |
| R2 — Rust library follow-ups | Accepted | None |
| R3 — Provider path remediation | Accepted | None |
| R4 — CLI test surface removal | Accepted | None |
| M6 — Native foundation and integration | Accepted | None |
| M7 — Browse and search | Accepted | None |
| M8 — Create and edit | Accepted | None |
| M9 — Removal and recovery | Accepted | M8 |
| M10 — Agent Ready | Accepted | M9 |
| M11 — MVP acceptance | Accepted | M9 and M10 |
| M12 — Value management | Accepted | M11 |
| M13 — Bulk performance | Accepted | M12 |
| M14 — Release readiness | Accepted | M13 |
| M15 — App Store release | Not started | M14 |

Native controls and their standard interaction behavior apply throughout the desktop;
broader visual refinement follows core functionality.

## M15 — App Store release

Outcome: the accepted MVP is packaged, signed, submitted, and distributed through the
chosen Store channel.

Gate: release configuration, entitlements, nested executables, Store materials, and
submission validation are complete; Apple accepts the release. Functional MVP acceptance
does not claim Store approval, and Store work is not deferred permission-system design.
