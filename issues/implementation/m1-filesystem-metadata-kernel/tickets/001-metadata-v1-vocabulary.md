---
title: Round-trip the complete metadata-version-1 vocabulary through the Rust core
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

A Rust caller can parse `config.toml`, `version.json`, `order.json`, `vendors.json`,
and `index.json` into typed core values and serialize versioned JSON values into the
canonical bytes required by metadata version 1.

## Decision sources

- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Require two parent segments in the Order package locator](../../m0-contract-baseline/tickets/007-require-two-parent-package-root.md)

## Blocking

The corrected M0 baseline must be committed and the worktree clean. No M1 ticket
blocks this ticket.

## Scope

- Add strict typed codecs for all five managed document kinds.
- Reject non-UTF-8 JSON, a byte-order mark, unknown fields, wrong versions, and
  unsupported durable shapes.
- Permit only `orders_path` and `removed_path` in unversioned `config.toml`.
- Emit managed JSON with two-space indentation, deterministic schema field order,
  omitted absent optionals, present collections, and exactly one trailing newline.
- Consume the checked-in M0 examples and invalid fixtures directly.

Exclude filesystem discovery, semantic cross-record validation, diagnostics,
mutations, upgrades, index rebuilding, atomic writes, and adapter-facing models.

## Pre-agreed public test seam

Public core metadata codec operations accept bytes and return a typed artifact or a
typed parse/shape failure. The first tracer bullet parses the minimal `order.json`,
serializes it canonically, and reparses it. It catches a Rust contract that accepts
or emits bytes inconsistent with the published schema.

## Acceptance criteria

- All accepted M0 examples parse successfully through the matching Rust codec.
- Every schema-invalid fixture is rejected by the matching Rust codec.
- Minimal and full documents serialize to exact canonical JSON bytes.
- `package_root = ".."` is rejected while `../..` and deeper parent-only locators
  remain structurally accepted.
- Optional omissions and empty required collections round-trip without becoming
  `null`.

## Review boundary

Review the durable type vocabulary, strict codec behavior, and canonical JSON bytes
only. Do not introduce Package behavior or persistence.

## Ticket-start fixed point

`c8c1250fd7803fa9b1b80967ce30031cf03ebd6f`

## Resolution

Added public typed codecs for unversioned `config.toml` and all four version-1
managed JSON documents. The Rust vocabulary makes schema versions, UUIDv4
identities, parent-only Package locators, nonblank required text, structural dates,
currencies, normalized index paths, and finite Item numbers explicit types. JSON
parsing rejects invalid encodings, a byte-order mark, unknown fields, wrong versions,
explicit optional `null`, and unsupported shapes; serialization emits deterministic
two-space schema order with omitted absent optionals and exactly one trailing newline.

Acceptance evidence (2026-07-22):

- `cargo test -p spielorders-core --test metadata_v1` — passed 18 public-behavior
  tests covering all eight accepted examples and all 17 manifest-listed invalid
  fixtures directly under `contracts/`.
- `cargo test --workspace` — passed the complete Rust workspace suite.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- `/code-review` from the exact ticket-start fixed point reported zero Spec findings
  and no documented-standard violations. Two Standards-axis duplication judgements
  were accepted, consolidated internally, reverified, and committed without changing
  the public design.

External-validation remediation M1-V3 (2026-07-25):

- Exact clean remediation fixed point:
  `7fb3e6b58f68b178c83dba2989c4936f11b8edd0`.
- Independent RED exports kept the fixed-point schema and Rust implementation while
  adding the new public regressions. AJV and the Rust codec each accepted both
  `C:/outside` and `C:outside`, contradicting the normalized-relative index path
  contract.
- The version-1 index schema and `IndexOrderDirectory` codec now reject an initial
  ASCII drive letter plus colon. Two one-purpose rejected fixtures are listed in
  the canonical invalid manifest. The accepted full index continues to prove a
  normalized `/`-separated relative Order Directory path remains valid. The
  correction follows the M0 locator precedent without advancing the metadata
  version or adding index loading, M2 behavior, or adapter behavior.
- From an artifact-free no-hardlink copy, `npm run test:schemas` passed 4 schemas,
  8 accepted examples, 19 unique rejected fixtures, 22 scenarios, and 2
  preservation sentinels. The focused metadata suite passed 18 tests. Locked
  workspace build, 62-test workspace and independent-core runs, Clippy with
  warnings denied, traceability, both frontend tests, the frontend production
  build, and the Tauri no-bundle release build all passed. Both Supporting
  Material hashes remained unchanged.
- `cargo update --dry-run --verbose` found no compatible lockfile update, and
  authoritative npm registry checks found no selected-major package update. Rust
  1.97.1 is current. The clean-copy runtime remained Node 24.15.0 and npm 11.16.0;
  Node 24.18.0 and npm 11.18.0 remain the separately scoped M1-V4 remediation.
- The project-local code review covered exact committed range
  `7fb3e6b58f68b178c83dba2989c4936f11b8edd0...d0d2244`.
  Standards and Spec each reported zero findings. The evidence/status updates
  recorded after that implementation review do not change product behavior.
