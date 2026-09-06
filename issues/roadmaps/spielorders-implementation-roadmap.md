# SpielOrders implementation roadmap

Status: M0 through M4 are complete and externally validated; the bounded [M3-upgrade test harness milestone](../implementation/m3-upgrade-test-harness/README.md) is in progress.

Source: [Chart the SpielOrders product and architecture](../wayfinder/spielorders-product-architecture/MAP.md)

## Purpose and boundary

This roadmap turns the closed product-and-architecture decisions into an implementation sequence. It defines work packages, dependencies, and acceptance gates; it does not implement SpielOrders, estimate calendar time, or reopen settled product decisions.

The MVP is a single-user, local-first, macOS-first desktop application backed by a shared Rust core and a human-first Rust CLI. It preserves one ordinary Order Directory per Order, treats Supporting Material as opaque user-owned content, and supports an opt-in package-local agent skill.

Institutional form generation, the shared Pending Northwind workflow, multi-user collaboration, OCR, execution of the legacy archive migration, post-MVP structured Order filters, and premature shared foundations with SpielGantt are outside it.

### Versioning before release

Until the owner declares the CLI and the agent support tooling released, which cannot happen before M4 and M8 both close, every interface, format, and protocol may be revised in place without regard to versioning, and every `contract_version` and `metadata_version` is 1. From that release on, the owner manages real Orders with agents and the CLI, and the compatibility rules take effect.

Nothing has run outside a test tree and no SpielOrders Package exists, so there is no consumer a change can break, and a second version of something nobody has read makes the first read as history when it is still a draft. A change that would otherwise advance a version instead edits version 1 in place and re-records whatever the change makes stale.

The declaration is a dated owner line in this roadmap's M8 section, so the marker is answerable from the repository rather than from anyone's memory. Until that line exists, the first regime is in force.

## Delivery shape

```text
M0 Contract baseline
  -> M1 Filesystem and metadata kernel
     -> M1-upgrade Toolchain and dependency modernization
        -> M2 Package loading, diagnostics, and index
        -> M3 Domain mutations and atomic persistence
           -> M4 CLI and machine contracts
              -> M5 Search and native platform services
           -> M6 Desktop read experience
              -> M7 Desktop editing and recovery
M4 + M7 -> M8 Agent Ready and adoption support
M4 + M5 + M7 + M8 -> M9 hardening -> MVP release candidate
```

The graph, not the numbering, determines what may run.

## Decision-to-milestone traceability

| Closed decision | Primary implementation milestones |
| --- | --- |
| [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md) | M0, M1, M9 |
| [Define the SpielOrders package topology](../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md) | M1, M2, M3 |
| [Define the Order record and lifecycle contract](../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md) | M1, M3, M7 |
| [Define Vendor identity and historical semantics](../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md) | M1, M3, M7, M8 |
| [Define lookup and search behavior](../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md) | M5, M6 |
| [Define the Rust CLI capability contract](../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md) | M3, M4, M5, M8 |
| [Prototype the SpielOrders desktop experience](../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md) | M0, M6, M7, M9 |
| [Define integrity, validation, and repair boundaries](../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md) | M1, M2, M3, M5, M7, M9 |
| [Define legacy Order adoption and metadata bootstrapping](../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md) | M3, M4, M8 |
| [Define durable metadata schemas and version evolution](../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md) | M0, M1, M2, M3, M4, M9 |
| [Define the SpielOrders agent skill contract](../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md) | M4, M8 |

## M0 — Contract baseline and project skeleton

Status: complete 2026-07-22. Index: [M0 — Contract baseline and project skeleton](../archive/m0-contract-baseline/README.md), [acceptance record](../archive/m0-contract-baseline/ACCEPTANCE.md).

## M1 — Filesystem and metadata kernel

Status: externally validated 2026-07-26. Index: [M1 — Filesystem and metadata kernel](../archive/m1-filesystem-metadata-kernel/README.md), [acceptance record](../archive/m1-filesystem-metadata-kernel/ACCEPTANCE.md).

## M1-upgrade — Toolchain and dependency modernization

Status: externally validated 2026-07-31. Index: [M1-upgrade — Toolchain and dependency modernization](../archive/m1-upgrade-toolchain-dependencies/README.md), [acceptance record](../archive/m1-upgrade-toolchain-dependencies/ACCEPTANCE.md).

## M2 — Package loading, diagnostics, and Package Index

Status: externally validated 2026-08-21. Index: [M2 — Package read model](../archive/m2-package-read-model/README.md), [acceptance record](../archive/m2-package-read-model/ACCEPTANCE.md).

## M3 — Domain mutations and safe persistence

Status: externally validated 2026-09-01. Index: [M3 — Domain mutations and safe persistence](../archive/m3-domain-mutations-safe-persistence/README.md), [acceptance record](../archive/m3-domain-mutations-safe-persistence/ACCEPTANCE.md).

## M4 — CLI and independently versioned machine contracts

Status: externally validated 2026-09-06. Index: [M4 — CLI and machine contracts](../implementation/m4-cli-machine-contracts/README.md), [acceptance record](../implementation/m4-cli-machine-contracts/ACCEPTANCE.md).

## M5 — Search and native platform services

Status: not started. Index: [M5 — Search and native platform services](../implementation/m5-search-and-native-services/README.md), which holds the one ticket the M4 gate carried forward.

Deliverables:

- Deterministic native Order and Vendor search with whitespace-tokenized AND terms, substring matching, diacritic-insensitive case-insensitive defaults, optional case sensitivity, complete accepted searchable facts, stable sorting, and one result per entity.
- Exclusion of paths, directory basenames, Supporting Material, Package README, calculated totals, configuration, schema syntax, and derived-index details from native entity search.
- An asynchronous cancellable OS-indexed Supporting Material search port returning settled snapshots, provider state, partial/complete status, Order groups, standardized-path deduplication, file facts, native rank when available, deterministic fallback ordering, and provider-specific diagnostics.
- A macOS Spotlight implementation scoped to healthy recognized Order Directories that excludes conventional Order READMEs and reserved `.spielorders` subtrees, plus open and reveal-in-Finder operations and stale-result handling.
- Explicit unavailable-provider behavior on unsupported platforms; native provider implementations become release gates for those platforms rather than prompting a recursive fallback indexer.
- The `search orders`, `search vendors`, and `search supporting-material` CLI commands, carrying the response contract, exit statuses, and golden-fixture obligations M4 established. They are M5's, not M4's.

Acceptance gate:

- Search fixtures prove native semantics and live Vendor-fact matching. macOS integration tests prove scoping, exclusions, grouping, deduplication, cancellation, partial and error states, stale-path removal, file open, and exact Finder reveal. Damaged Orders participate in neither search engine.

## M6 — Desktop shell and read experience

Status: not started.

Deliverables:

- Package open, reopen, and initialize flows and one active-Package session in a Tauri 2 desktop shell.
- Toolbar destinations for Orders, Vendors, and Package README; New Order; Package menu; and a persisted inspector toggle and width.
- Dense Mantine-based Orders and Vendors tables with decided default columns, stable sorting, selection, compact inspectors, column customization and preferences, contextual actions, and keyboard navigation. The Vendors table omits Hidden Vendors by default and carries the show/hide control that reveals them.
- Native-search and Spotlight peer modes with preserved query text, `Aa` behavior, result counts, Order-grouped Supporting Material results, and open and reveal actions.
- Damaged Order placeholders and diagnostic summaries in the unfiltered Orders grid.
- Plain-text Markdown editors with lightweight syntax highlighting for Package and Order READMEs.

Acceptance gate:

- Component tests run against a fake capability implementation with no Tauri dependency. A packaged macOS smoke test can initialize or open a Package, browse thousands-scale indexed data, search in both modes, inspect Orders and Vendors, edit READMEs, and reveal an Order Directory or matching file in Finder.

## M7 — Desktop editing, removal, and recovery

Status: not started.

Deliverables:

- Explicit-save Order and Vendor editors with dirty-state handling, leave confirmation, preserved drafts, inline rejected-save feedback, and focus on the first invalid field.
- New-Order drafting without filesystem creation until successful Save; searchable Vendor selection with in-context New Vendor creation; and all five ordinary statuses with the decided Date Ordered behavior.
- Spreadsheet-like Item editing with keyboard movement, drag ordering, pending deletion, undo, and calculated totals.
- Align Directory Name, recoverable Remove Order, visually separated Move to Trash, macOS Delete-key confirmation, and collision and rejection presentation.
- A separate diagnostic workspace showing evidence and only allowed repairs, exact dry-run review, revalidation, and the expert raw-JSON correction surface. There is no Fix All or inferred repair.
- Equivalent dense table, inspector, and editor behavior for Vendors and Contacts, including a reversible hide/unhide toggle behind the M6 show/hide control, permitted whether or not Orders reference the Vendor.

Acceptance gate:

- End-to-end tests exercise successful and rejected edits, unsaved navigation, Vendor creation from an Order draft, Item ordering, alignment collision, removal collision, trash cancellation and failure, explicit repairs, still-invalid raw correction, and recovery of a repaired Order into the index and ordinary lookup.

## M8 — Agent Ready and legacy-adoption support

Status: not started.

Deliverables:

- `agent prepare` and `agent status`, `init --agent-ready`, and the corresponding GUI Agent Ready action using the same core preparation capability.
- Exact generated bundle: minimal root `AGENTS.md`; `.agents/skills/spielorders/SKILL.md`; and the five decided progressive-disclosure references. Preparation creates or overwrites those files without merge, version, ownership, compatibility, or manifest logic.
- Guidance covering Package and adoption operations, Orders and Items, Vendors and Contacts, search and inspection, and diagnostics and repair, with short examples matching actual CLI help and golden JSON contracts.
- Firm guidance that agents use the CLI for managed metadata, preserve Supporting Material, treat legacy names as evidence rather than grammar, avoid placeholder Vendors and Vendor Merge, and resume adoption from current Package state.
- A dry-run-only adoption rehearsal against synthetic legacy fixtures, including aliases, links, ambiguous names, invalid dates, contradictions, partial metadata, duplicate evidence, existing READMEs, and index-refresh failure. The real legacy archive is not modified.

Acceptance gate:

- Generated files match checked-in snapshots; every documented command is executable against fixtures; repeated preparation predictably overwrites the generated bundle; status checks only the two decided paths; and the synthetic rehearsal demonstrates one-Order-at-a-time, resumable, Supporting-Material-preserving adoption without executing the real migration.

## M9 — Hardening and MVP release candidate

Status: not started.

Deliverables:

- Cross-layer conformance tests proving CLI and Tauri adapters produce equivalent typed core outcomes for ordinary reads and mutations.
- Filesystem fault-injection coverage for permission failures, interrupted staging, rollback failure, stale indexes, unsupported schemas, partial upgrades, collisions, links, and disappearing search results.
- Performance and responsiveness evaluation using a synthetic Package at or above the contemplated archive scale with representative Vendors, Items, READMEs, Damaged Orders, and Supporting Material search results.
- macOS packaging, permissions, file-open and Finder integration, Spotlight availability diagnostics, application preference persistence, and clean first-run behavior.
- A final decision-traceability audit covering every closed map ticket and every stated out-of-scope boundary.
- Evaluation of the provisional CLI `--uuid` selector. Retain it only if MVP GUI or scripting evidence justifies it; otherwise remove it before declaring the CLI contract stable.

Acceptance gate:

- All milestone gates pass in a clean environment; no known failure can silently corrupt canonical metadata or Supporting Material; the desktop and CLI expose the decided MVP capabilities; the synthetic archive-scale Package remains responsive; and release notes state deferred structured filters and non-macOS native-provider work clearly.
