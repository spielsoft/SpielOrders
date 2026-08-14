---
title: Audit M2 decision completeness and hand off implementation decomposition
status: closed
closed_at: 2026-08-13
parent: Chart the M2 Package read model
labels:
  - wayfinder:task
assignee: /root
---

## Question

Do the resolved M2 read-side decisions, existing architecture contracts, and
roadmap acceptance criteria now form a contradiction-free specification that can
be decomposed into vertical implementation tickets without inventing product
behavior?

## Blocking

Unblocked. [Define the complete M2 Package read projection and load-state contract](003-package-read-projection.md)
is closed, so the index, diagnostic, and complete projection/load-state contracts
are all available for the decision-completeness audit and implementation handoff.

## Resolution

### Audit result

The M2 read-side specification is contradiction-free, decision-complete, and ready
for bounded implementation decomposition. No material product fork remains, no
settled MVP choice needs reopening, and no domain-language change was found.

The following three decisions are authoritative together:

- [Decide how Package Index freshness is proven without canonical rereads](001-index-freshness-fast-path.md)
  defines the friendly-filesystem trust boundary, conservative no-follow change
  cutoff, deterministic canonical rebuild, and blocking Package Index Replacement
  Failure.
- [Define the stable M2 diagnostic vocabulary and evidence contract](002-stable-diagnostic-contract.md)
  defines stable diagnostics and grouping while keeping Package Index assessment,
  refresh outcome, and operation failures separate.
- [Define the complete M2 Package read projection and load-state contract](003-package-read-projection.md)
  composes those contracts into the shared-core load results, current-Package
  projection, README reads, upgrade reporting, and exact evidence boundary.

The older product-architecture statements that allowed `orders_path = "."` and
Vendor Merge are not live contradictions. [Define the Rust CLI capability contract](../../spielorders-product-architecture/tickets/006-cli-contract.md)
explicitly supersedes both, the implementation roadmap records those refinements,
and externally validated M1 implements the refined configuration behavior and no
Vendor Merge.

### Evidence reviewed

The audit reviewed:

- `AGENTS.md`, the Wayfinder, grilling, and domain-modeling skill instructions,
  `CONTEXT.md`, this map and ticket, all three closed M2 decisions, and the complete
  implementation roadmap;
- the closed product-and-architecture map and all of its decisions, with particular
  attention to Package topology, Order and Vendor identity, CLI/core capability
  ownership, integrity and repair, legacy adoption, durable schemas and upgrades,
  and desktop/core separation;
- `contracts/fixtures/scenarios.json`, the canonical fixture guidance, all four
  metadata-version-1 schemas, and the minimal and full version-1 examples;
- the existing Rust core metadata, validation, totals, Package/Vendor semantics,
  path/layout, filesystem replacement, rollback, and effect-port APIs together with
  their public tests, failure types, real-filesystem adapter, and canonical fixture
  consumption;
- the M1 milestone, acceptance evidence, and external validation, including the
  validated core/adapter direction, deterministic bytes, path/no-follow behavior,
  replacement fault injection, and opaque Supporting Material sentinels; and
- the M1-upgrade milestone and external validation, including the frozen selected
  toolchains, reproduced clean gates, retained install-script boundary, recorded
  compatible release drift, and mandatory complete M2-closeout refresh.

The initial worktree changes are all related and preservable: they are the external
M1-upgrade validation record, its milestone/roadmap status update, and this M2 map.
No unrelated implementation, schema, dependency, lockfile, or generated change was
present.

### Completeness and consistency findings

#### Load states, canonical trust, and projections

- The tagged load result completely distinguishes `unmanaged`, ordinarily
  `available`, Damaged Package `diagnostic_only`, supported older or resumable
  `upgrade_pending`, blocking `index_replacement_failed`, and early
  `operation_failed` outcomes. A wholly absent root `.spielorders` remains
  nondiagnostic unmanaged state; package-scoped canonical errors never leak a
  partial ordinary projection.
- Damaged Orders remain one-for-one deterministic placeholders beside healthy
  Orders. Duplicate Order IDs damage every conflicting candidate, retain the
  trustworthy duplicate ID only as evidence, share the complete ordered conflict
  set through per-Order diagnostics, and never select a survivor.
- Healthy Orders expose complete normalized version-1 records, declared identities,
  paths, Items, exact calculated Item Total Prices and Order Total, cached index
  README text, and live validated Vendor associations. Vendors expose complete
  normalized records, trustworthy identities, and canonical Vendor/Contact order.
- The four Reusable Free-Entry Value collections are all present, derive only from
  healthy Orders, group by the established trimmed full-Unicode case-insensitive
  comparison, retain first deterministic spelling, count Orders, and sort by
  bytewise UTF-8 canonical spelling.
- Every established managed location has the required absolute native path and
  normalized Package-relative path. Missing expected artifacts still name their
  exact locations; unestablished or unexamined evidence stays absent rather than
  being guessed.
- Healthy Orders, damaged placeholders, Vendors, Contacts, Items, diagnostics,
  related evidence, derived-index reasons, and reusable values all have explicit
  deterministic ordering. Collections are present when empty except intentionally
  omitted empty diagnostic groups, and semantically absent optionals remain absent.

#### READMEs and evidence visibility

- Package and Order README references identify exact conventional locations without
  eager text. Exact JIT reads distinguish absent-as-empty, readable UTF-8, and one
  document warning for wrong-kind, unreadable, or invalid encoding without damaging
  the owner.
- Metadata version 1 retains cached Order README text in healthy index projections.
  Fast loading may use the validated cache; rebuilding reads each exact conventional
  Order README; explicit later reads do not treat the cache as current evidence.
  Package README text is never indexed.
- The fast path intentionally lacks latent canonical, README, reserved-content, and
  staging evidence it did not read or list. Canonical rebuilding reports only the
  evidence its required canonical and README reads support. Exhaustive validation
  alone broadens inspection to all governed canonical artifacts, exact READMEs, and
  immediate reserved directories.

#### Diagnostics, index state, failures, and upgrades

- Canonical diagnostics, diagnostic groups, Package Index assessment, refresh
  outcome, and operation failures are distinct typed concepts. The stable code,
  severity, scope, location, identity, field, evidence, deduplication, grouping,
  and ordering contracts cover the decided Package, Order, document, reserved,
  staging, configuration, identity, and Vendor Reference conditions.
- The current-index path validates version-1 shape, embedded semantics and
  relationships, deterministic bytes and sort order, candidate membership and
  paths, required ordinary file kinds, version agreement, and a strictly later
  no-follow change cutoff before using the index. Uncertain, equal,
  incomparable, contradictory, or newer canonical evidence selects rebuilding.
- Only complete healthy Order and Vendor facts, trustworthy identities, matched
  healthy Order paths, ordered children, cached Order README text, totals, resolved
  Vendor associations, and the four reusable-value lists may derive from a current
  index. Config/version facts, configured paths, membership/structural warnings,
  freshness, damage, diagnostics, upgrade execution state, operation failures,
  Package README content, and Supporting Material facts may not.
- Canonical rebuilding uses only validated current Package artifacts, lexically
  ordered direct-child Order candidates, exact `order.json` files, and exact
  conventional Order READMEs. It projects only healthy Orders, preserves damaged
  placeholders outside version-1 index bytes, validates deterministic candidate
  bytes, and uses the established validated atomic replacement seam.
- Package-wide canonical damage prevents rebuilding and leaves index assessment
  `not_assessed` or another already-supported assessment with refresh
  `not_attempted`. Order-local damage permits a healthy-only candidate but forces
  later canonical slow paths because version 1 stores neither damaged placeholders
  nor their diagnostics.
- Package Index Replacement Failure remains a blocking operation result with the
  original assessment, `replacement_failed`, exact phase/path evidence, and any
  cleanup failure. It preserves old index bytes and does not relabel independently
  healthy canonical facts as Damaged Package.
- Supported older and resumable-upgrade states report only established source,
  target, deterministic per-artifact observations, and separate blockers. Because
  version 1 is the repository's only schema, current M2 does not invent an older or
  future schema, migration fixture, upgrade executor, mutation plan, or upgrade
  write. Symbolic fixture evidence is sufficient until a real consecutive schema
  chain is deliberately adopted.

#### Boundaries, fixtures, acceptance, and closeout

- The shared Rust core owns every load, validation, projection, identity,
  diagnostic, ordering, freshness, rebuild, and replacement decision. The existing
  filesystem port remains the sole effect boundary and needs only the smallest
  portable no-follow change observation exposing comparable evidence or explicit
  uncertainty. Production adapters translate platform facts conservatively; test
  doubles inject observations and failures. CLI, Tauri, React, command envelopes,
  retry presentation, and session state remain outside M2.
- Supporting Material remains wholly outside inspection and ownership. Fast load,
  rebuild, exhaustive validation, diagnostics, README reads, staging cleanup, and
  index work neither list general Order contents nor recurse, inventory, parse,
  validate, rewrite, move, or delete Supporting Material.
- The canonical fixture corpus already supplies unmanaged, healthy, damaged,
  duplicate-identity, README, derived-index, reserved/staging, unsupported-version,
  symbolic-upgrade, and preservation foundations. M2 implementation must add only
  narrowly justified cases needed to prove exact result variants, diagnostics and
  ordering, current versus uncertain change evidence, permitted index sourcing,
  JIT README behavior, canonical rebuild bytes, and each replacement phase/failure.
  Existing fixtures remain consumed in place.
- Integrated M2 acceptance must demonstrate a current-index load without opening
  every `order.json` or `vendors.json`, deterministic canonical rebuilds, complete
  damage/placeholders and exhaustive validation, exact evidence visibility per
  path, replacement-failure blocking without canonical misclassification, port
  fault injection, core independence from adapters, byte-identical preservation of
  both opaque sentinels, and absence of M3 or presentation behavior.
- M2 closeout must perform the complete dependency/toolchain refresh required by
  `AGENTS.md`: begin with all drift frozen and recorded by M1-upgrade, refresh Rust,
  Cargo, Node, npm, TypeScript, every direct dependency and compatible locked
  transitive within its selected major, request approval before any new major,
  grant no install-script authority, and rerun every affected clean-install,
  build, test, lint, schema, traceability, audit, Tauri, development-smoke,
  preservation, boundary, and scope gate.

### Nonblocking implementation choices

Implementation may choose exact Rust module and identifier names, owned versus
borrowed internal representations, helper composition, and the concrete portable
change-observation value shape. It may split fixture builders and public test files
where reviewability requires. Those choices are nonblocking only while they
preserve the closed result variants, stable vocabulary and evidence, deterministic
behavior, current-index read budget, filesystem-port substitution, version-1 bytes,
and core/adapter and Supporting Material boundaries.

The implementation decomposition must use independently reviewable vertical
outcomes—each exposing and testing a complete core capability through the
filesystem port—not speculative internal layers such as a types-only foundation,
generic repository framework, or adapter envelope. Exact ticket boundaries and
their dependency graph are deliberately left to that bounded planning slice.

### Readiness conclusion and next slice

The destination of [Chart the M2 Package read model](../MAP.md) is reached. The map
may close with no remaining fog or decision ticket.

Select exactly one next slice: **Decompose the decision-complete M2 read side into
vertical implementation tickets**. That planning slice must create the ordered M2
implementation ticket chain, trace every ticket to the three closed M2 decisions
and governing architecture, include a final integrated acceptance/refresh gate,
and end with the ready-to-paste prompt for the first implementation ticket. It must
not implement any ticket.
