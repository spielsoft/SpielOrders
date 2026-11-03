---
title: Restore search stopping, Unicode matching, file paths, and response evidence
status: closed
assignee: codex
claimed_at: 2026-11-02T19:49:02Z
closed_at: 2026-11-02T20:40:17Z
review_start: cf56be8f6c6cceee366d01ccf2ebe5476f18c1ef
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

The search defects identified by external validation are fixed and have durable regression
checks, while the reachable Supporting Material success response has recorded contract
evidence.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)
- [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md)
- [M5 external validation](../VALIDATION.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

None. Query scope is settled; M6 implementation remains blocked on successful M5
revalidation.

## Scope

- Enforce timeout and in-flight cancellation through completion of native platform work.
- Apply Unicode caseless matching consistently to native-search terms and facts.
- Retain legal Supporting Material filenames without relaxing managed-metadata path rules.
- Record a nonempty successful Supporting Material response produced by the executable,
  with durable automatic comparison independent of live index contents.
- Refresh compatible runtime and dependency selections under the project currency rule.

## Acceptance criteria

- A started platform query that closes stdout before exiting still stops on timeout and
  in-flight cancellation, retains gathered results, and leaves no started work running.
- Case-insensitive search matches contextual sigma and expanding case folds through the
  executable; case-sensitive mode and diacritic behavior remain correct.
- A provider-reported legal backslash-containing Supporting Material filename survives
  grouping and presentation, while managed paths and unhealthy Order results remain excluded.
- A reachable nonempty Supporting Material success has a checked-in response fixture from
  a real executable run, and automatic comparisons do not require live indexing. Variable
  values use justified, validated placeholders under the existing contract conventions.
- Focused regressions fail against the defective behavior and pass after correction.
- The project gate and independent Standards and Spec review pass; compatible dependency
  drift is resolved or its authoritative selector blocker is recorded.

## Exclusions

- M6 implementation, indexing changes, recursive Supporting Material inspection, or a new
  native provider.
- New major versions without owner approval, install-script authority, or advancing
  contract or metadata versions.
- Declaring M5 externally validated; a fresh worker performs revalidation after remediation.

## Result

Native platform work remains bounded and cancellable after pipe closure, with gathered
output retained and subprocesses reaped. Entity lookup uses full Unicode folds; user-file
locations preserve native backslashes without relaxing managed paths. The nonempty
Spotlight success fixture records a real executable response and has offline byte replay.

Regressions live with the [platform effects](../../../../crates/spielorders-cli/src/effects.rs),
[executable entity search](../../../../crates/spielorders-cli/tests/entity_search.rs),
[Supporting Material search](../../../../crates/spielorders-core/tests/supporting_material_search.rs),
and [command response construction](../../../../crates/spielorders-cli/src/commands/search.rs).
Compatible selections and approved transitive updates are applied.
[M5 acceptance](../ACCEPTANCE.md) records the passing checks, review disposition, and
upstream constraints. Fresh external revalidation remains required; M6 implementation stays
blocked.
