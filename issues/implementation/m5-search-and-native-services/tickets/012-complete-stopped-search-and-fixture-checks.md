---
title: Keep queued search output, final exits, and real fixture comparisons
status: closed
assignee: codex
claimed_at: 2026-11-02T23:21:57Z
closed_at: 2026-11-02T23:40:49Z
review_start: a0a37ce7939a5027e6d607537d65e0b86f836ed5
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

Stopped native searches retain queued complete results, finished tools keep their final
outcome, and the fixture inventory executes the success comparison it registers.

## Decision sources

- [Define lookup and search behavior](../../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)
- [Decision traceability and contract-test conventions](../../../../docs/development/traceability.md)
- [M5 external validation](../VALIDATION.md)

## Blocking

None. M6 implementation remains blocked on successful M5 external revalidation.

## Scope

- Couple Supporting Material success registration to an actual full-response comparison.
- Drain output already queued at a stop without waiting for EOF or admitting an unbounded
  drain from a surviving writer.
- Preserve a completed tool's exit status instead of replacing it with a stop reason.
- Preserve synchronized startup in the stopping regression.

## Acceptance criteria

- The fixture inventory executes the nonempty success comparison; deleting its separate
  replay test cannot leave an unchecked fixture reported as compared.
- A stopped query retains complete output spanning several read buffers, including bytes
  queued after the last ordinary poll.
- Draining is nonblocking and finite even when a pipe remains open.
- A tool observed to have finished retains its successful or failed exit at the timeout
  or cancellation boundary. A nonzero exit remains a refusal, not a partial success.
- Regressions demonstrate each defect before correction and pass after correction.
- The existing started-query timeout/cancellation checks remain meaningful and stable;
  native-index contents do not decide their result.
- The project gate and independent Standards and Spec review pass.

## Exclusions

- The optional location-spelling refactor, M6 implementation, indexing changes, or the
  owner-only application-launching smoke.
- Contract or metadata version advances, changes to durable fixtures, or unrelated
  dependency updates.
- Declaring M5 externally validated.

## Result

Stopped queries retain a finite snapshot of queued output, and observed final exits keep
their complete-success or refusal outcome. Real-process regressions in
[platform effects](../../../../crates/spielorders-cli/src/effects.rs) cover multi-buffer
output and coincident limits while preserving synchronized startup.

The [response fixture inventory](../../../../crates/spielorders-cli/tests/response_contract.rs)
executes the success comparison before registering its key. Its pure projection shares
the executable's encoder; no separate replay test is required. A corrupted-fixture probe
detects the missing comparison without source-grep checks.

[M5 acceptance](../ACCEPTANCE.md) records passing gates and independent reviews. Fresh
external revalidation remains required; M6 implementation stays blocked.
