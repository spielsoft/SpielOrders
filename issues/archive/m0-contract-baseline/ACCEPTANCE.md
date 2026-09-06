# M0 acceptance evidence

Status: passed 2026-07-21; corrected and revalidated 2026-07-22

Decision sources: [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md), [Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md), [Prototype the SpielOrders desktop experience](../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md), and [Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).

## Clean environment

The source tree was copied to a fresh `/private/tmp/spielorders-m0-gate.*`
directory while excluding `target`, `node_modules`, `dist`, and `.DS_Store`.
All commands below ran in that artifact-free copy using Rust 1.96.0, Node 24.15.0,
and npm 11.16.0. Both npm dependency trees were recreated from checked-in lockfiles.

## Results

| Check | Result |
| --- | --- |
| `cargo build --workspace` | Passed from an empty target directory |
| `cargo test --workspace` | Passed |
| `cargo test -p spielorders-core` | Passed independently |
| `npm clean-install` | Passed; 0 reported vulnerabilities |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 17 rejected fixtures, 22 scenarios, 2 preservation sentinels |
| `npm run check:traceability` | Passed: 7 M0 tickets, 4 baseline test sources |
| `npm --prefix apps/desktop clean-install` | Passed; 0 reported vulnerabilities |
| `npm --prefix apps/desktop run test` | Passed: fake capabilities and frontend-local session state |
| `npm --prefix apps/desktop run build` | Passed |
| `npm --prefix apps/desktop run tauri build -- --no-bundle` | Passed; produced the macOS shell binary |

## Dependency and scope audit

`cargo tree` showed that `spielorders-core` has no dependency, while the CLI and
Tauri crates each depend inward on core. Tauri APIs appear only in the production
frontend adapter; fake frontend tests run without loading Tauri.

Source inspection found no Tauri command, CLI command parser/tree, Rust metadata
model, Package discovery/loading, production filesystem implementation, semantic
validator, mutation/persistence, repair, search algorithm/provider, migration
transformation, desktop workflow, or SpielGantt shared package. The only executable
logic beyond empty adapters is M0 boundary substitution and contract tooling.

The accepted version-1 examples include required `order.json.package_root` in both
canonical and indexed Orders. Strict Draft 2020-12 schemas reject unknown fields and
named invalid fixtures. Fixture recipes include damaged, unsupported, interrupted,
and preservation states without defining metadata version 0 or a version-2 migration.

## Locator correction and renewed gate

External validation after the original gate found that the canonical and indexed
Order schemas accepted `"package_root": ".."`. That contradicted the final topology:
`orders_path = "."` is forbidden, so an active Order Directory is below a nonempty
Orders Directory and requires at least `"../.."` to reach the SpielOrders Root.

On 2026-07-22, [Require two parent segments in the Order package locator](tickets/007-require-two-parent-package-root.md)
changed both patterns to require two or more parent segments and added canonical and
indexed rejection fixtures for the single-parent form. Direct regression checks prove
`".."` is rejected while `"../.."` and `"../../.."` are accepted.

The entire clean-environment command set, dependency inspection, and scope audit
listed above were rerun from a fresh artifact-free copy and passed without waiver.

M0 therefore passes in its corrected state without waiver.
