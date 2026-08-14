---
title: Chart the M2 Package read model
status: closed
closed_at: 2026-08-13
labels:
  - wayfinder:map
---

## Destination

Produce a decision-complete M2 read-side specification sufficient to decompose
Package loading, stable diagnostics, conventional README reads, Package Index fast
loading/rebuild/staleness, and the shared `package show` projection into
implementation tickets without inventing M3 behavior.

## Notes

- Use [CONTEXT.md](../../../CONTEXT.md) as the canonical domain glossary and update
  it only when resolving a domain-language change.
- Plan by default. This map does not authorize implementation, schema changes,
  dependency changes, lockfile regeneration, or M3 behavior.
- The closed [SpielOrders product and architecture map](../spielorders-product-architecture/MAP.md),
  the [implementation roadmap](../../roadmaps/spielorders-implementation-roadmap.md),
  and the externally validated M1/M1-upgrade artifacts govern this work.
- Preserve the existing metadata-version-1 schemas, deterministic bytes,
  Supporting Material boundary, core/adapter dependency direction, and M1
  acceptance evidence.
- A current Package Index must support the decided fast path without reading every
  `order.json`; the index remains wholly derived and contains no filesystem
  timestamps or diagnostic state.
- M2 closeout must perform the complete dependency/toolchain refresh required by
  `AGENTS.md`, beginning with all release drift frozen and recorded by M1-upgrade.
- Resolve at most one decision ticket per work session. Each kickoff or continuation
  prompt must end by generating the prompt for the next M2 slice.

## Decisions so far

<!-- Closed decision tickets are indexed here by name with a one-line gist. -->

- [Decide how Package Index freshness is proven without canonical rereads](tickets/001-index-freshness-fast-path.md)
  — MVP trusts a valid version-1 index behind a conservative no-follow filesystem
  change cutoff, otherwise deterministically rebuilds it from canonical managed
  inputs and blocks opening if required replacement fails.
- [Define the stable M2 diagnostic vocabulary and evidence contract](tickets/002-stable-diagnostic-contract.md)
  — Use a minimal compositional code set with two scoped severities and typed path
  and identity evidence, while keeping index state, refresh outcome, and failed
  operations distinct from canonical damage.
- [Define the complete M2 Package read projection and load-state contract](tickets/003-package-read-projection.md)
  — Use distinct unmanaged, available, diagnostic-only, upgrade-pending, and failed
  load results around one deterministic current-Package projection, with JIT README
  reads and evidence bounded to the actual fast, rebuild, or exhaustive path.
- [Audit M2 decision completeness and hand off implementation decomposition](tickets/004-audit-m2-decision-completeness.md)
  — The three M2 contracts and governing architecture form one contradiction-free,
  implementation-ready read-side specification; the next slice is bounded vertical
  implementation-ticket decomposition.

## Not yet specified

- None. The map destination is reached and the implementation-decomposition handoff
  is recorded by the closed completeness audit.

## Out of scope

- M3 mutations, write orchestration, repair execution, metadata upgrades, and
  persistence beyond the already-decided automatic derived-index rebuild.
- CLI, Tauri command, React presentation, search, or native platform behavior.
- New metadata schemas or metadata-version changes.
- Inspecting, inventorying, validating, rewriting, or relocating Supporting
  Material.
- Adding Mantine, replacing AJV or Vite, granting install-script authority, or
  creating a shared SpielGantt dependency.
