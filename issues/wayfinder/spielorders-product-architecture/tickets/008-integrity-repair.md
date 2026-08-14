---
title: Define integrity, validation, and repair boundaries
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

Which inconsistencies must SpielOrders detect or repair across directory names, managed metadata, Vendor references, lifecycle fields, and user-controlled supporting files, while never treating arbitrary user content as application-owned?

## Blocking

Unblocked.

## Resolution

SpielOrders validates only the package structure and artifacts within its declared ownership boundary. It isolates damage to the narrowest canonical scope, preserves all suspect material for explicit review, and never treats repair as permission to infer domain facts or rewrite user-owned Supporting Material.

### Diagnostic scopes and ordinary operation

- A **Damaged Package** has untrustworthy package-wide canonical state, including missing, unreadable, malformed, unsupported-version, or semantically invalid `version.json`, `config.toml`, or `vendors.json`, or an inaccessible or invalid configured Orders Directory. It opens in diagnostic-only mode; ordinary package mutations and search are rejected until package-wide integrity is restored.
- A **Damaged Order** is isolated to its containing Order Directory. Missing, unreadable, malformed, unsupported-version, or semantically invalid `order.json`; an unresolved Vendor Reference; or a duplicate Order ID quarantines the affected directory without preventing healthy Orders in the same current-version package from being viewed or changed. A Damaged Order encountered while an older package is upgrading instead pauses ordinary package use until the user resolves it.
- Validation reports the package as invalid while any Damaged Order exists, even though healthy Orders remain usable.
- The unfiltered main Orders view retains a clearly marked placeholder for every Damaged Order Directory. Duplicate-ID conflicts keep every conflicting row visible and prominently identify the invalid identity field, following the BibDesk precedent discussed during resolution. Damaged placeholders are removed whenever a native search query is applied and never participate in Spotlight results.
- Warnings identify noncanonical but usable structure and never block ordinary work. Rejected mutations are failed operations that leave the prior canonical state unchanged.

### Order discovery and directory names

- Every non-hidden ordinary direct-child directory of the configured Orders Directory receives a main-view presence as either a healthy Order or a Damaged Order. A missing, unreadable, malformed, or wrongly typed immediate `.spielorders/order.json` produces a Damaged Order.
- Non-hidden direct-child files, aliases, and symlinks are ignored with a structural warning; links are never followed. Hidden direct children remain ignored without diagnostics.
- An Order Directory basename that differs from the current `YYYY_MM_DD[_Vendor]_Title[_Orderer]` projection is ordinary allowed state, not a warning or integrity failure. SpielOrders continues to identify the Order by its embedded Order ID.
- **Align Directory Name** remains an optional explicit organization action. Validation and repair never rename for drift. A destination collision or failed rename rejects alignment and leaves the current directory and metadata unchanged.

### Managed metadata and validation

- Outside a supported Metadata Upgrade, `.spielorders/index.json` is the only managed artifact SpielOrders may replace or rebuild automatically because it is wholly derived from canonical records.
- SpielOrders never automatically reconstructs, replaces, or guesses canonical `version.json`, `config.toml`, `vendors.json`, or `order.json`. Opening automatically performs a supported, deterministic, resumable Metadata Upgrade to the running build's newest version; upgrade remains distinct from repair because every source artifact must already be valid under its declared schema.
- Within a supported schema, validation rejects malformed syntax, unknown fields, wrong types, missing or blank required facts, invalid calendar dates, invalid Order Status values, malformed three-letter Order Currency codes, duplicate IDs, package-duplicate Vendor Names under their case-insensitive comparison, and unresolved Vendor References.
- Validation accepts contract-valid but unusual facts: future or provisional Date Ordered values, any direct Order Status transition, zero Order Items, negative or zero Quantity and Unit Price, and absent optional facts. It emits no status-dependent completeness or plausibility warnings.
- An edit that would introduce invalid canonical state is rejected before replacement and preserves the previous canonical file.
- Unexpected content inside a reserved `.spielorders/` directory produces a warning when required canonical artifacts remain intact. If an unexpected entry occupies a required canonical path, the applicable Package or Order is damaged. SpielOrders never deletes, moves, follows, or silently adopts unexpected entries.

### Vendor and identity failures

- A malformed or internally inconsistent `vendors.json`, including duplicate Vendor IDs or package-duplicate Vendor Names, damages the package because the aggregate Vendor set cannot be trusted. Vendor Merge is never inferred as repair.
- An Unresolved Vendor Reference damages only its Order. Explicit repair requires choosing an existing Vendor, deliberately creating a specified Vendor, or clearing the reference. Directory names, contact text, other Orders, and Supporting Material are never evidence for an inferred replacement.
- Every Order Directory sharing a duplicate Order ID is a Damaged Order. Explicit repair requires the user to designate the one Order that retains the existing ID. Each remaining directory must be moved outside the Orders Directory using ordinary filesystem management or explicitly adopted as a new Order with a newly generated immutable Order ID. SpielOrders never chooses the survivor or regenerates IDs automatically.

### Partial writes and explicit repair

- Managed-file replacements are staged beside their destinations, validated, and atomically renamed into place. A multi-file operation validates its full plan first and, if a later replacement fails, restores already-replaced managed files where possible. Rollback failures are themselves reported.
- Metadata Upgrade is the deliberate exception to package-wide rollback: valid Orders upgrade atomically one at a time and remain upgraded after interruption, Vendor metadata follows, `version.json` advances last, and the derived Package Index is rebuilt. Ordinary package use remains blocked until the upgrade completes; the user decides how to handle any damaged Order that prevents completion.
- New Order creation remains outside discovery until committed. Recognizable abandoned staging directories and files are diagnosed; they are never silently promoted.
- A leftover staging artifact beside a valid canonical destination is a cleanup warning. If its canonical destination is missing or invalid, the affected Package or Order is damaged. Recovery may present a valid candidate but requires explicit confirmation before promoting it.
- When a canonical file parses under a supported schema, repair may prefill recognized facts, identify invalid or unsupported facts, and write only a user-confirmed fully valid replacement. Unknown fields may be removed only through that confirmation.
- SpielOrders does not partially salvage syntactically malformed JSON or interpret unsupported schema versions. It may offer confirmed recovery from a validated staging artifact or an external-correction surface. The desktop GUI's expert raw-JSON editor is one such surface: it writes the user's text without GUI or core-repair interpretation and then revalidates; invalid text remains damaged.
- A missing `order.json` is not reconstructed by integrity repair. Deliberately adopting that directory as a new Order belongs to the legacy-adoption contract.

### User-authored files and search

- Supporting Material is outside validation and repair authority. SpielOrders performs no recursive integrity scan, schema or filename validation, inventory, link inspection, permission warning, or orphan detection over it. Validation, rollback, cleanup, and repair never create, rewrite, move, rename, or delete it.
- Spotlight may search Supporting Material and SpielOrders may open or reveal returned paths; those access operations grant no ownership. Spotlight queries are scoped to healthy recognized Order Directories, and stale results under damaged directories are discarded.
- Package and Order READMEs are conventional user-authored documents, distinct from both canonical metadata and Supporting Material. A missing README is normal and loads as empty. An unreadable or non-text README produces a document-specific warning and disables editing that document without damaging its Package or Order; repair never replaces or converts its contents.
- Damaged Orders are excluded from the Package Index, native entity search, and Spotlight Order groups. Their diagnostic placeholders can still reveal the containing directories in Finder. Restoring canonical validity and rebuilding the Package Index returns them to ordinary lookup without touching Supporting Material.
