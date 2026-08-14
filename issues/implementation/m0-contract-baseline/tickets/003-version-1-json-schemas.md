---
title: Publish the metadata-version-1 JSON Schemas
status: closed
assignee: codex
claimed_at: 2026-07-21
closed_at: 2026-07-21
parent: M0 — Contract baseline and project skeleton
labels:
  - implementation:m0
  - implementation:task
---

## Outcome

Check in strict JSON Schema Draft 2020-12 documents for every versioned JSON artifact in metadata version 1.

## Decision sources

- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked.

## Scope

- Publish separate schemas for `version.json`, `order.json`, `vendors.json`, and `index.json`.
- Use JSON Schema Draft 2020-12 and reject unknown object fields with `additionalProperties: false` at every managed object level.
- Encode the accepted required fields, optional fields, collection presence, enums, scalar representations, embedded Item and Contact shapes, and metadata/schema version constants.
- Apply the later CLI refinement to version 1: every canonical Order object, including the complete Order projection embedded in `index.json`, requires the normalized relative `package_root` string.
- Keep `config.toml` outside this ticket because it is unversioned TOML, and keep CLI response contracts outside because they evolve independently from metadata.

## Acceptance criteria

- The four schemas identify Draft 2020-12 and validate their accepted minimal and fully populated examples.
- `order.json` examples lacking `package_root` fail; accepted examples contain it without changing the decided domain facts.
- Unknown fields, missing required collections, invalid status values, malformed UUID shape, lowercase or non-three-letter currency, and nonconforming date shape fail at the schema layer where expressible.
- Optional semantic absence is represented by omission rather than requiring JSON `null`; empty required collections remain valid.
- The schemas do not claim to replace Rust semantic validation for cross-record uniqueness, filesystem resolution, real calendar validity beyond schema expressiveness, finite target-language numeric representation, or live Vendor Reference resolution.

## Review boundary

Review only the published durable JSON shape for metadata version 1. Rust types, parsers, semantic validation, migrations, and CLI JSON responses are separate work.

## Resolution

Published strict, self-contained JSON Schema Draft 2020-12 documents for
`version.json`, `order.json`, `vendors.json`, and `index.json`, with minimal and
fully populated accepted examples. Canonical and indexed Orders both require the
normalized relative `package_root` locator. Added locked AJV contract tooling and
documented the boundary between schema validation and later Rust semantics.

Acceptance evidence (2026-07-21):

- `npm clean-install` — passed from the root lockfile.
- `npm run test:schemas` — compiled all four schemas and validated eight accepted
  examples.
- The same check rejected ten named cases covering missing `package_root`, unknown
  fields, absent required collections, invalid status, malformed UUIDv4, invalid
  currency, nonconforming date shape, JSON `null`, and missing nested collections.
- Review confirmed `additionalProperties: false` at every managed object level,
  required empty collections remain valid, and optional fields are omitted rather
  than nullable.
- Scope audit confirmed no `config.toml`, Rust metadata model, parser, semantic
  validator, migration, or CLI response schema was introduced.
