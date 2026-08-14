---
title: Chart the SpielOrders product and architecture
status: closed
labels:
  - wayfinder:map
---

## Destination

Produce a decision-complete product and architecture specification for SpielOrders: a single-user, local-first, macOS-first and cross-platform-capable desktop order-management app with a Rust CLI/core, a SpielGantt-related visual language, durable filesystem Order Directories, and a package-local agent skill for safe CLI-driven Order and Vendor workflows.

## Notes

- Use [CONTEXT.md](../../../CONTEXT.md) as the canonical domain glossary and update it whenever language is resolved.
- Plan by default; implementation code is outside this map.
- Preserve the one-Order-Directory-per-Order model and arbitrary Supporting Material without requiring a particular internal structure.
- SpielOrders-managed JSON and Markdown have native search; OS-native search covers Supporting Material and can reveal matches in Finder.
- Make SpielOrders agent-forward: agents should be able to inspect a SpielOrder Package and safely create or update Orders, Order Items, Vendors, and Vendor Contacts through the CLI without editing managed metadata directly.
- `/Users/example/Documents/Administration/Orders` is legacy/archive background, not a layout mandate.
- `/Users/example/Code/Apps/SpielGantt` is product and architecture inspiration, not a requirement to share code.
- The shared Pending Northwind orders folder is an external process and is out of scope except as background evidence.

## Decisions so far

<!-- Closed decision tickets are indexed here by name with a one-line gist. -->

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](tickets/001-shared-filesystem-foundation.md) — Defer extraction behind compatible repository boundaries; consider a future policy-neutral filesystem-mechanics crate only after both apps prove matching invariants.
- [Define the SpielOrders package topology](tickets/002-package-topology.md) — Use a configurable direct-child Orders Directory, a root JSON version marker, canonical per-Order JSON and aggregate Vendor JSON, a rebuildable global index, opaque Supporting Material, and explicit directory-name alignment.
- [Define the Order record and lifecycle contract](tickets/003-order-record-lifecycle.md) — Use immutable opaque Order IDs, a deliberately permissive record with embedded calculated-price items, reusable free-entry classifications, one archival date, and unrestricted user-assigned status transitions.
- [Define Vendor identity and historical semantics](tickets/004-vendor-identity-history.md) — Use immutable Vendor IDs, package-unique names, optional shared contact facts, live references without historical snapshots, guarded deletion, and explicit import-time duplicate reconciliation.
- [Define lookup and search behavior](tickets/005-search-lookup.md) — Keep deterministic live entity filtering distinct from an Order-grouped, file-centric Spotlight view; preserve ordinary table sorting and navigation, deduplicate by entity or path, and defer structured filters until after MVP.
- [Define integrity, validation, and repair boundaries](tickets/008-integrity-repair.md) — Isolate canonical damage, keep directory-name drift silent, rebuild derived indexes, keep deterministic resumable upgrades distinct from explicit repair, quarantine damaged Orders from search, and never validate or rewrite Supporting Material.
- [Define durable metadata schemas and version evolution](tickets/010-metadata-schemas.md) — Use strict human-readable versioned JSON under one package marker, exact Order/Vendor/full-index projections, UUIDv4 identity, and automatic sequential per-Order upgrades that advance the package version last.
- [Define the Rust CLI capability contract](tickets/006-cli-contract.md) — Use a human-first shared-core CLI with contextual roots, assignment-based mutations, recoverable Order removal, scoped validation and repair, native and OS-indexed search, and independently versioned opt-in JSON responses; add relative Package locators and remove Vendor Merge.
- [Prototype the SpielOrders desktop experience](tickets/007-desktop-experience.md) — Use a single-window BibDesk-like grid/inspector/editor experience in Tauri 2 + React + TypeScript, Mantine-first tables and controls, Finder-owned Supporting Material, explicit repair, and no premature SpielGantt frontend sharing.
- [Define legacy Order adoption and metadata bootstrapping](tickets/009-legacy-adoption.md) — Keep adoption exceptional and agent-orchestrated: initialize the Package, explicitly adopt existing directories one at a time with current-version metadata, and leave evidence interpretation, consolidated review, and resumable batching to the agent skill.
- [Define the SpielOrders agent skill contract](tickets/011-agent-skill-contract.md) — Generate an opt-in, package-local, versionless skill with minimal AGENTS guidance, progressively disclosed CLI examples, two simple agent commands, and firm managed-metadata and Supporting Material boundaries.

## Not yet specified

- None currently.

## Out of scope

- Filling or generating institutional purchasing forms.
- Replacing or operating the team's shared Pending Northwind orders workflow.
- Multi-user accounts, permissions, collaboration, synchronization, or edit-conflict resolution.
- OCR or content indexing of Supporting Material.
- Executing the legacy archive migration.
- Implementing application code as part of this planning map.
