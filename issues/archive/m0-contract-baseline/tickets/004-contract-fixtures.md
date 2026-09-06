---
title: Create the canonical contract fixture corpus
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

Create a canonical, reviewable fixture corpus that later Rust, CLI, Tauri, and frontend tests can share without copying or reinterpreting the accepted contracts.

## Decision sources

- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define legacy Order adoption and metadata bootstrapping](../../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Publish the metadata-version-1 JSON Schemas](003-version-1-json-schemas.md) closed on 2026-07-21.

## Scope

- Add minimal and fully populated valid fixtures for `version.json`, `order.json`, `vendors.json`, and `index.json`, including empty collections and omitted optional facts.
- Add one-purpose invalid fixtures for unknown fields, missing/wrong versions, wrong types, invalid enumerations and formats, malformed identities, absent required collections, and invalid nested Items or Contacts.
- Add filesystem-shaped Package scenarios for unmanaged roots, healthy Packages, Damaged Packages, healthy Orders, Damaged Orders, ignored hidden entries, warned direct-child links/files, unexpected reserved content, README absence/unreadability, stale or invalid indexes, and opaque Supporting Material sentinels.
- Add cross-record and edge-case scenario manifests for duplicate Order IDs, duplicate Vendor IDs/Names, unresolved Vendor References, directory-name drift, unusual-but-valid numeric values, staging artifacts, unsupported newer metadata, and interrupted-upgrade version combinations.
- Provide a current-version/no-op upgrade fixture and an unsupported-future fixture. Do not invent metadata version 0 or a speculative version-2 transformation merely to exercise migration machinery.
- State expected validity, narrowest damage scope or warning category, and preservation expectations for each scenario without prematurely assigning the complete stable diagnostic-code catalog.

## Acceptance criteria

- Every JSON fixture expected to be valid passes the checked-in schema; every schema-invalid fixture fails for its named reason.
- Each fixture tests one primary contract condition or clearly documents why several files are needed for one cross-record scenario.
- Fixture identities and paths are deterministic and contain no real archive data or personal credentials.
- Supporting Material sentinels are distinguishable byte-for-byte so later tests can prove that metadata work does not inspect or rewrite them.
- The fixture index links every scenario to its closed decision sources by name and distinguishes schema validity, semantic validity, integrity scope, warning state, and upgrade state.

## Review boundary

Review fixture coverage, isolation, provenance, and expected contract outcomes. No production parser, validator, repair, search, or migration behavior belongs in this ticket.

## Resolution

Created a canonical contract corpus containing minimal and full accepted documents,
15 one-purpose schema-invalid documents, 22 declarative filesystem/cross-record/
upgrade/adoption scenarios, and two byte-distinct opaque Supporting Material
sentinels. Scenario recipes support portable materialization of files, directories,
links, unreadable content, generated semantic overlays, and symbolic upgrade states.

Acceptance evidence (2026-07-21):

- `npm run test:schemas` — all accepted examples passed their checked-in schema and
  all 15 rejected fixtures failed with their named JSON Schema keyword.
- The fixture-index audit verified 22 unique scenarios, required validity/integrity/
  warning/upgrade/preservation fields, existing source assets, and linked closed
  decision names for every scenario.
- The preservation audit proved the two Supporting Material sentinels differ
  byte-for-byte.
- Coverage includes unmanaged, healthy, damaged, hidden/link/file, reserved-content,
  README, stale/invalid index, identity/reference, directory drift, numeric edge,
  staging, unsupported, interrupted/no-op upgrade, and legacy-adoption states.
- Data audit found no real archive path, credential, metadata version 0, speculative
  version-2 transformation, production parser, repair, search, or migration behavior.
