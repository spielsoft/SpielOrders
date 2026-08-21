# SpielOrders implementation roadmap

Status: M0 complete; M1 externally validated; M1-upgrade externally validated; M2 externally validated; M3 blocked

Source: [Chart the SpielOrders product and architecture](../wayfinder/spielorders-product-architecture/MAP.md)

## Purpose and boundary

This roadmap turns the closed product-and-architecture decisions into an implementation sequence. It defines work packages, dependencies, and acceptance gates; it does not implement SpielOrders, estimate calendar time, or reopen settled product decisions.

The MVP is a single-user, local-first, macOS-first desktop application backed by a shared Rust core and a human-first Rust CLI. It preserves one ordinary Order Directory per Order, treats Supporting Material as opaque user-owned content, and supports an opt-in package-local agent skill.

Institutional form generation, the shared Pending Northwind workflow, multi-user collaboration or synchronization, OCR/content indexing, execution of the legacy archive migration, post-MVP structured Order filters, and premature shared foundations with SpielGantt remain outside this roadmap.

## Decision authority and refinements

The closed decision tickets are authoritative. Where a later ticket refines an earlier one, implementation follows the later contract:

- Version-1 `order.json` includes the required normalized relative `package_root` locator.
- `config.toml` permits `orders_path` and `removed_path`; `orders_path = "."` is invalid, and `removed_path` names an ordinary direct child of the SpielOrders Root.
- Vendor Merge is absent. Duplicate legacy Vendor evidence is reconciled before creation by selecting an existing Vendor, creating one deliberately distinct Vendor, or leaving the Order without a Vendor.
- The CLI also includes `adopt-order`, `agent prepare`, and `agent status`, and `init` may opt into Agent Ready preparation with `--agent-ready`.
- The generated agent skill is deliberately versionless and overwrites its generated files on preparation.

Schema, CLI response, and frontend capability contracts are separate. On-disk `metadata_version`, per-command `contract_version`, application release versions, and generated guidance must never be treated as one version stream.

## Delivery shape

Work proceeds through ten dependency-ordered product milestones plus one bounded
pre-M2 modernization gate. A milestone is complete only when its acceptance gate
passes; downstream work may start against typed interfaces and fixtures, but it may
not declare completion against mocks alone.

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

The numbered sequence below is the default execution order. The dependency graph, not the numbering alone, determines what can safely run in parallel.

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

Establish boundaries before behavior is added.

**Status: complete 2026-07-22.** The original gate passed on 2026-07-21, and
[Require two parent segments in the Order package locator](../implementation/m0-contract-baseline/tickets/007-require-two-parent-package-root.md)
closed the externally identified schema defect. The renewed
[M0 acceptance evidence](../implementation/m0-contract-baseline/ACCEPTANCE.md) passes
without waiver.

Implementation tickets: [M0 — Contract baseline and project skeleton](../implementation/m0-contract-baseline/README.md)

Deliverables:

- A Rust workspace with separate core-library and CLI-adapter boundaries, plus a Tauri 2 shell whose React/TypeScript frontend talks through a typed capability interface.
- Checked-in JSON Schema Draft 2020-12 documents for metadata version 1: `version.json`, `order.json`, `vendors.json`, and `index.json`.
- Canonical valid, invalid, and edge-case fixtures for Package, Order, Vendor, Vendor Contact, Order Item, index, diagnostic, and migration behavior.
- Explicit ports for filesystem operations, local calendar date, UUID generation, native file reveal/open, OS-indexed search, and app preferences so core rules and platform effects can be tested independently.
- A traceability convention linking implementation issues and tests to the relevant closed decision ticket by name.

Acceptance gate:

- The workspace builds with empty adapters, schemas validate the accepted version-1 examples including `package_root`, and no Package rule lives in the CLI, Tauri command layer, or React presentation code.

## M1 — Filesystem and metadata kernel

Implement the durable representation and ownership boundary without yet exposing the full product surface.

**Status: externally validated 2026-07-26.** The seven-ticket
[M1 — Filesystem and metadata kernel](../implementation/m1-filesystem-metadata-kernel/README.md)
closed after the reviewed acceptance gate passed without waiver. The milestone
delivers the typed metadata-version-1 vocabulary and codecs, Order and Package/Vendor
semantics, exact totals and normalization, safe managed paths and no-follow
classification, validated atomic replacement, recoverable multi-file rollback, and
complete acceptance evidence. Core remains independent of every adapter layer, both
opaque Supporting Material sentinels remain byte-identical, and no M2/M3 behavior or
shared SpielGantt crate entered the milestone. After M1-V1 through M1-V4 were
remediated, a fresh
[external validation](../implementation/m1-filesystem-metadata-kernel/VALIDATION.md)
independently verified the complete remediated range and passed.

Implementation tickets: [M1 — Filesystem and metadata kernel](../implementation/m1-filesystem-metadata-kernel/README.md)

Deliverables:

- Typed representations and strict parsing/serialization for `config.toml`, `version.json`, `order.json`, `vendors.json`, and `index.json`.
- Path validation for the SpielOrders Root, configured Orders Directory, Removed Orders Directory, direct-child Order discovery, normalized relative Package locators, reserved `.spielorders` paths, hidden entries, and disallowed links.
- Domain validation for immutable lowercase UUIDv4 identities, required and optional facts, status/date/currency rules, finite numeric Item values, Vendor uniqueness, live Vendor References, and Reusable Free-Entry Value normalization.
- Deterministic two-space JSON output with one trailing newline, omitted absent optionals, present collections, and stable field order.
- Calculated Item Total Price and Order Total behavior with rounding confined to presentation.
- Beside-destination staging, validation, atomic replacement, and recoverable multi-file rollback primitives. These mechanics stay local to SpielOrders until proven duplication justifies a policy-neutral shared crate.

Acceptance gate:

- Schema-conformance and Rust semantic-validation suites agree across valid and invalid fixtures; path/link tests cannot escape the SpielOrders Root; failure-injection tests demonstrate that failed canonical replacements preserve the prior valid state.

## M1-upgrade — Toolchain and dependency modernization

Modernize the newly validated foundation before beginning another product milestone.

**Status: externally validated 2026-07-31.** All four
implementation and acceptance-gate tickets are closed.
After the first two failed target-pinned gate attempts returned compatible Cargo
and Node/npm currentness updates without repair, their owning tickets were
remediated and independently reviewed. The third fresh attempt found compatible
React and ReactDOM type-package patch updates and returned them to reopened
[Move repository tooling to Node 26 and npm 12](../implementation/m1-upgrade-toolchain-dependencies/tickets/002-node-npm-modernization.md)
without repair. The user-authorized stabilization disposition deferred that release
drift to M2 closeout, passed independent documentation review, and reclosed the
owning ticket. The approved
[M1-upgrade — Toolchain and dependency modernization](../implementation/m1-upgrade-toolchain-dependencies/README.md)
is a four-ticket pre-M2 gate starting from the exact externally validated M1
documentation baseline. It does not reopen M1 behavior or authorize M2 work.
The user froze the completed dependency snapshot through the completed gate and
this external validation; compatible release drift is recorded and deferred to
the mandatory M2-closeout refresh.

The final fresh no-hardlink, target-pinned gate passed every build, test, lint,
schema, traceability, frontend, Tauri, development-smoke, vulnerability,
preservation, behavior, and scope criterion. Its independent fixed-point Standards
and Spec review completed with every finding resolved. The release inventory
records four compatible post-snapshot releases without installing them:
`@types/react` 19.2.18, `@types/react-dom` 19.2.4, `fast-uri` 3.1.5, and optional
`@napi-rs/wasm-runtime` 1.2.2. M2 closeout retains the mandatory complete refresh.
A fresh
[external validation](../implementation/m1-upgrade-toolchain-dependencies/VALIDATION.md)
independently reproduced the exact completed target, refreshed the release
inventory, reran the complete executable and preservation gate, and passed without
a blocking finding or dependency/implementation change.

Implementation tickets:
[M1-upgrade — Toolchain and dependency modernization](../implementation/m1-upgrade-toolchain-dependencies/README.md)

Deliverables:

- Cargo resolver 3 and current approved Rust dependency generations, including the
  TOML major-1 migration, with existing metadata/configuration behavior preserved.
- Node 26 Current and npm 12 as reproducible build tooling, with clean lockfile
  regeneration and selected-major currentness.
- TypeScript 7 CLI type checking without inventing a dependency on its not-yet-shipped
  programmatic compiler API.
- Retention of AJV for strict Draft 2020-12 contract tests and Vite for the React/Tauri
  build and development loop; Mantine remains absent until consumed.
- Integrated clean-copy evidence that M1 behavior, Supporting Material, dependency
  direction, and scope remain unchanged.

Acceptance gate:

- Every clean build, test, lint, schema, traceability, frontend, Tauri, and development
  smoke gate passes under the frozen selected toolchains; authoritative dependency
  audits completely inventory release drift for M2 closeout; both opaque sentinels
  are byte-identical; and no M2 behavior is present. After the gate closes, a fresh
  external validation must pass before M2 is decomposed or implemented.

## M2 — Package loading, diagnostics, and Package Index

Create the read model that every adapter shares.

**Status: externally validated 2026-08-21. M3 remains blocked.** M1 and the
approved pre-M2 M1-upgrade
modernization are externally validated. The closed
[M2 Package read-model map](../wayfinder/m2-package-read-model/MAP.md) records the
authoritative index freshness, stable diagnostic, and complete Package
projection/load-state contracts. Its final
[decision-completeness audit](../wayfinder/m2-package-read-model/tickets/004-audit-m2-decision-completeness.md)
found no live contradiction or missing product decision. The first closure of the
six-slice [M2 implementation milestone](../implementation/m2-package-read-model/README.md)
delivered exact README reads, canonical projection, deterministic Package Index
assessment/rebuild, current-index fast loading, exhaustive managed validation, and
the complete selected-major refresh; all passed the first integrated acceptance
and fixed-point review. Fresh external reproduction passed every
executable, currentness, preservation, architecture, and scope gate but found that
the lifecycle table for [Integrate M2 acceptance and refresh the complete selected-major toolchain](../implementation/m2-package-read-model/tickets/006-integrate-m2-acceptance-and-refresh.md)
did not record the exact review-disposition commit for
[Load a current Package Index without canonical record rereads](../implementation/m2-package-read-model/tickets/004-load-current-index-fast-path.md).
The integrated acceptance capability corrected that evidence, passed a complete
refreshed gate and resolved fixed-point review, and reclosed M2 for fresh external
validation.

Deliverables:

- Package opening and discovery that distinguishes unmanaged directories, Damaged Packages, healthy Orders, Damaged Order placeholders, warnings, unsupported versions, and resumable-upgrade state.
- Narrowly scoped stable diagnostics for syntax, schema, semantic, identity, Vendor Reference, reserved-content, staging-artifact, README, configuration, and index conditions.
- Complete `package show` core projection: paths, versions, Package README, healthy Orders, Damaged Order placeholders, Vendors, diagnostics, upgrade/index state, and the four Reusable Free-Entry Value lists.
- Deterministic Package Index rebuild and staleness/status detection. The index contains complete healthy Order projections plus Order README text and Vendors, and remains wholly derived.
- Typed UTF-8 reads for conventional Package and Order READMEs, treating absence as empty and unreadable/non-text content as a document warning rather than Package damage.
- At M2 closeout, refresh Rust/Cargo, Node/npm, TypeScript, every direct dependency,
  and compatible locked transitives under `AGENTS.md`; begin with all release drift
  recorded by the frozen M1-upgrade gate and external validation, and rerun every
  affected clean-install, build, test, lint, audit, and development smoke check.

Acceptance gate:

- Loading a representative Package does not require reading every `order.json` when a valid current index exists; rebuilding from canonical state is deterministic; damaged Orders remain visible diagnostically but never enter the index.
- The deferred dependency/toolchain refresh is complete, authoritative currentness
  checks find no remaining required compatible update, and every affected
  repository gate passes without changing M2 product contracts.

## M3 — Domain mutations and safe persistence

Complete the Rust core capabilities required by both CLI and desktop adapters.

Deliverables:

- Package initialization at the newest metadata version with configurable Orders and Removed Orders paths and byte-preserving README behavior.
- Order creation, explicit one-directory adoption, show/edit, Item list/add/edit/move/remove, README read/write, directory-name alignment, recoverable removal, and platform-trash disposition.
- Vendor creation/show/edit/removal and Contact list/add/edit/move/remove, including guarded deletion while referenced.
- Atomic assignment semantics, one-based ordered child positions, explicit optional-field clearing, canonical Reusable Free-Entry Value reuse/introduction reporting, and the `Ordered` status/date rule.
- Complete dry-run mutation plans with no filesystem side effects, and typed committed/unchanged/rejected/committed-with-warning reports.
- Exhaustive Package and scoped Order validation; explicit one-case repair operations; sequential resumable Metadata Upgrade; index status and rebuild.

Acceptance gate:

- Integration tests cover every accepted mutation, collision and rejection path, index-refresh failure after a committed canonical mutation, README preservation, Supporting Material preservation, and interruption/resumption of multi-version upgrades. No operation except explicit whole-Order removal moves or disposes of Supporting Material.

## M4 — CLI and independently versioned machine contracts

Expose the full shared core through one Git-style `spielorders` executable.

Deliverables:

- The complete decided command families and shorthands: initialization; Package reads and value lists; Order, Item, Vendor, and Contact operations; `add-order`, `add-vendor`, and `adopt-order`; native and Supporting Material search; validation, repair, upgrade, and index operations.
- Exact-root targeting via `--root`, provisional mutually exclusive Order `--uuid` targeting for MVP evaluation, canonical `field=value` mutations, and `--dry-run` on every mutation.
- Concise default human output; `--quiet`, `--verbose`, and one-document `--json` modes; stdout/stderr discipline; color behavior; cancellation; and stable exit statuses `0`, `1`, and `2` plus platform signal conventions.
- Independently versioned per-command JSON response contracts with stable codes, typed diagnostics/errors, absolute and relevant relative paths, present empty collections, and refreshed affected entities after successful mutations.
- Generated help and version output, plus golden fixtures for every command's successful, warning, rejected, invalid, empty-result, and usage-error JSON response.

Acceptance gate:

- The CLI can perform every ordinary Package-domain operation required by the future GUI without parsing or rewriting managed metadata itself. Contract tests prove `--json` emits exactly one UTF-8 document and no incidental output, and human output is never used as a parsing contract.

## M5 — Search and native platform services

Implement the two deliberately separate lookup systems.

Deliverables:

- Deterministic native Order and Vendor search with whitespace-tokenized AND terms, substring matching, diacritic-insensitive case-insensitive defaults, optional case sensitivity, complete accepted searchable facts, stable sorting, and one result per entity.
- Exclusion of paths, directory basenames, Supporting Material, Package README, calculated totals, configuration, schema syntax, and derived-index details from native entity search.
- An asynchronous cancellable OS-indexed Supporting Material search port returning settled snapshots, provider state, partial/complete status, Order groups, standardized-path deduplication, file facts, native rank when available, deterministic fallback ordering, and provider-specific diagnostics.
- A macOS Spotlight implementation scoped to healthy recognized Order Directories that excludes conventional Order READMEs and reserved `.spielorders` subtrees, plus open/reveal-in-Finder operations and stale-result handling.
- Explicit unavailable-provider behavior on unsupported platforms; native provider implementations become release gates for those platforms rather than prompting a recursive fallback indexer.

Acceptance gate:

- Search fixtures prove native semantics and live Vendor-fact matching. macOS integration tests prove scoping, exclusions, grouping, deduplication, cancellation, partial/error states, stale-path removal, file open, and exact Finder reveal. Damaged Orders participate in neither search engine.

## M6 — Desktop shell and read experience

Build the accepted single-window read experience against the typed capability interface.

Deliverables:

- Package open/reopen/initialize flows and one active-Package session in a Tauri 2 desktop shell.
- Toolbar destinations for Orders, Vendors, and Package README; New Order; Package menu; and a persisted inspector toggle and width.
- Dense Mantine-based Orders and Vendors tables with decided default columns, stable sorting, selection, compact inspectors, column customization/preferences, contextual actions, and keyboard navigation.
- Native-search and Spotlight peer modes with preserved query text, `Aa` behavior, result counts, Order-grouped Supporting Material results, and open/reveal actions.
- Damaged Order placeholders and diagnostic summaries in the unfiltered Orders grid.
- Plain-text Markdown editors with lightweight syntax highlighting for Package and Order READMEs.

Acceptance gate:

- Component tests run against a fake capability implementation with no Tauri dependency. A packaged macOS smoke test can initialize/open a Package, browse thousands-scale indexed data, search in both modes, inspect Orders and Vendors, edit READMEs, and reveal an Order Directory or matching file in Finder.

## M7 — Desktop editing, removal, and recovery

Complete the ordinary and diagnostic desktop workflows.

Deliverables:

- Explicit-save Order and Vendor editors with dirty-state handling, leave confirmation, preserved drafts, inline rejected-save feedback, and focus on the first invalid field.
- New-Order drafting without filesystem creation until successful Save; searchable Vendor selection with in-context New Vendor creation; and all five ordinary statuses with the decided Date Ordered behavior.
- Spreadsheet-like Item editing with keyboard movement, drag ordering, pending deletion, undo, and calculated totals.
- Align Directory Name, recoverable Remove Order, visually separated Move to Trash, macOS Delete-key confirmation, and collision/rejection presentation.
- A separate diagnostic workspace showing evidence and only allowed repairs, exact dry-run review, revalidation, and the expert raw-JSON correction surface. There is no Fix All or inferred repair.
- Equivalent dense table, inspector, and editor behavior for Vendors and Contacts, including referenced-Vendor removal protection.

Acceptance gate:

- End-to-end tests exercise successful and rejected edits, unsaved navigation, Vendor creation from an Order draft, Item ordering, alignment collision, removal collision, trash cancellation/failure, explicit repairs, still-invalid raw correction, and recovery of a repaired Order into the index and ordinary lookup.

## M8 — Agent Ready and legacy-adoption support

Add the opt-in agent-facing layer only after the CLI contracts it documents are stable enough to test.

Deliverables:

- `agent prepare` and `agent status`, `init --agent-ready`, and the corresponding GUI Agent Ready action using the same core preparation capability.
- Exact generated bundle: minimal root `AGENTS.md`; `.agents/skills/spielorders/SKILL.md`; and the five decided progressive-disclosure references. Preparation creates or overwrites those files without merge, version, ownership, compatibility, or manifest logic.
- Guidance covering Package/adoption operations, Orders/Items, Vendors/Contacts, search/inspection, and diagnostics/repair, with short examples matching actual CLI help and golden JSON contracts.
- Firm guidance that agents use the CLI for managed metadata, preserve Supporting Material, treat legacy names as evidence rather than grammar, avoid placeholder Vendors and Vendor Merge, and resume adoption from current Package state.
- A dry-run-only adoption rehearsal against synthetic legacy fixtures, including aliases, links, ambiguous names, invalid dates, contradictions, partial metadata, duplicate evidence, existing READMEs, and index-refresh failure. The real legacy archive is not modified.

Acceptance gate:

- Generated files match checked-in snapshots; every documented command is executable against fixtures; repeated preparation predictably overwrites the generated bundle; status checks only the two decided paths; and the synthetic rehearsal demonstrates one-Order-at-a-time, resumable, Supporting-Material-preserving adoption without executing the real migration.

## M9 — Hardening and MVP release candidate

Verify the assembled product against the closed map rather than adding scope.

Deliverables:

- Cross-layer conformance tests proving CLI and Tauri adapters produce equivalent typed core outcomes for ordinary reads and mutations.
- Filesystem fault-injection coverage for permission failures, interrupted staging, rollback failure, stale indexes, unsupported schemas, partial upgrades, collisions, links, and disappearing search results.
- Performance and responsiveness evaluation using a synthetic Package at or above the contemplated archive scale (approximately 1,530 Orders) with representative Vendors, Items, READMEs, Damaged Orders, and Supporting Material search results.
- macOS packaging, permissions, file-open/Finder integration, Spotlight availability diagnostics, application preference persistence, and clean first-run behavior.
- A final decision-traceability audit covering every closed map ticket and every stated out-of-scope boundary.
- Evaluation of the provisional CLI `--uuid` selector. Retain it only if MVP GUI or scripting evidence justifies it; otherwise remove it before declaring the CLI contract stable.

Acceptance gate:

- All milestone gates pass in a clean environment; no known failure can silently corrupt canonical metadata or Supporting Material; the desktop and CLI expose the decided MVP capabilities; the synthetic archive-scale Package remains responsive; and release notes state deferred structured filters and non-macOS native-provider work clearly.

## Cross-cutting issue rules

Each implementation issue should be independently reviewable and include:

- the closed decision ticket names it implements;
- the user-visible or contract-level outcome, not only internal components;
- dependencies on earlier roadmap issues;
- test fixtures and acceptance criteria, including negative and interruption cases where persistence is involved;
- confirmation that Supporting Material and managed-metadata ownership boundaries are preserved; and
- any discovered contradiction with the closed decisions, escalated as a new decision ticket rather than silently resolved in code.

Do not create speculative shared crates, frontend packages, generic repair frameworks, a recursive Supporting Material indexer, a browser deployment, a legacy import wizard, or post-MVP filters as incidental implementation work. Proven low-level duplication may be marked for later evaluation without blocking MVP.

## Recommended next execution slice

Run only fresh external validation of
[Integrate M2 acceptance and refresh the complete selected-major toolchain](../implementation/m2-package-read-model/tickets/006-integrate-m2-acceptance-and-refresh.md)
at the exact completed target supplied by its handoff. Reproduce the complete M2
gate in a target-pinned no-hardlink copy and run the project-local Standards and
Spec reviews independently. Do not fix failures, change dependencies, reinterpret
evidence, or begin M3. Only after every gate and review passes may the validation
worker create or update M2 external-validation evidence and mark only M2's roadmap
status externally validated.
