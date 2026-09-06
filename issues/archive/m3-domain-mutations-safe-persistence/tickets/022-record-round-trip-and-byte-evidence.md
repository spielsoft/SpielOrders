---
title: Give managed record sites semantic destination expectations
status: closed
review_start: 84b1ef9cd278a3e8b99f9bfb6de9383094ebb475
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task", "origin:external-validation"]
---

## Outcome

Managed Order and Vendor records compare destination meaning rather than formatting.
SpielOrders accepts any valid representation, emits one deterministic representation,
and does not call formatting-only edits corruption.

Byte expectations remain correct for READMEs, upgrade transformations, and the derived
index. Primary evidence lives in the durable-write standard and record mutation suites.

## Reopened by external validation

[The M3 external validation record](../VALIDATION.md), finding V-2, first half. One
managed Order record site was never given an expectation at all:

`crates/spielorders-core/src/removal.rs` rewrites the moved Order's technical locator
through the bare `replace_validated_file`, which is the `Unchecked` wrapper. Enumerated
across the crate this is the **only** remaining bare call; all nine other production
sites pass an explicit expectation.
[The durable-write standard](../../../../docs/architecture/durable-writes.md) assigns
managed Order and Vendor records replaced one at a time
`DestinationExpectation::Equivalent`, and says plainly that "no evidence means no
changed-destination protection."

The record is rewritten at the destination *after* the directory rename, at the moment
the Order has left the caller's view, so nothing else guards it.

## Decision sources

- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)

## Blocking

No capability blocker. The primitive and the expectation variant already exist, so this
is adoption. The repository has one shared worktree and the remaining defect at this
site belongs to [Ticket 010](010-remove-order.md), so the two are claimed together and
neither may be claimed while another remaining ticket holds the worktree.

## Scope

- Give the removal locator rewrite `DestinationExpectation::Equivalent`, comparing
  through the same normalized projection the plan's own document came from.
- Cover the site with a test that fails against the unguarded code, and with the
  formatting-only case the semantic rule exists to keep accepting.

## Acceptance criteria

- A record given a different meaning between the plan's observation and the locator
  rewrite survives, and the rewrite is refused rather than reported as done.
- A record reformatted, or carrying untrimmed stored text, still means what the plan
  observed, so the locator rewrite proceeds.
- The refusal test does not pass against the code as it stood at `review_start`.
- No bare `replace_validated_file` call remains in production code.

## Exclusions

The swallowed cleanup failure and the evidence-free warning variant at the same site
are [Ticket 010](010-remove-order.md)'s. `semantic_repair::identity_exists` is
[Ticket 013](013-resolve-order-repairs.md)'s. Deferred
[Ticket 027](027-hoist-package-valid-vendors-and-narrow-repair-evidence.md), the
M3-upgrade fixture-builder ticket, and all M4 work are excluded.

## Review boundary

The removal locator rewrite's destination expectation and the tests that pin it.

## Closeout

Closed. The remediation started from a clean worktree at
`84b1ef9cd278a3e8b99f9bfb6de9383094ebb475` and completed at `2e2db1a`.

The moved Order's record now carries `DestinationExpectation::Equivalent`, so the last
bare `replace_validated_file` is gone from production and every managed Order and Vendor
record replaced one at a time compares destination meaning. This site had no other guard:
it writes at the destination *after* the directory rename, when the Order has left the
caller's view and nothing looks at that record again. A record landing there in the
window — a restore, an adoption, a repair — was overwritten while the removal reported
`Committed`; it now survives, and the rewrite is refused as a changed destination.

The predicate compares through the same normalized projection the plan's own document
came from, which is what the standard is fussiest about here. `plan.document` is
`HealthyOrder::document`, built by the load from `ValidatedOrder::order`, so the
destination is parsed and validated before it is compared. Comparing the raw parse
instead would refuse a record whose stored facts carry untrimmed text — a record a person
reformatted by hand — and cost a replan for nothing.

### Verification

Two cases were added. A record given a different meaning under the rewrite: their bytes
survive intact, the refusal is reported as a changed destination rather than a write
failure, and the staging file is taken back. A record reformatted and left untrimmed
under the same rewrite: it still means what the plan observed, so the locator is
rewritten and their formatting is what is given up.

Neither was asserted. The first fails against `DestinationExpectation::Unchecked`, the
shape at `review_start`, with their record replaced by this operation's candidate. The
second fails when the predicate compares the raw parse against the normalized plan
document, which is the trap the standard names — so the projection clause is pinned by a
test rather than by a comment.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 533 core integration tests — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1 with npm 12.0.2.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent, each writing findings to disk as it read. Neither raised a blocking finding
against the expectation itself; both confirmed the variant, the projection, and that no
bare `replace_validated_file` remains. The blocking finding and the non-blocking ones
belong to the failure reporting at the same site and are dispositioned in
[Ticket 010](010-remove-order.md).
