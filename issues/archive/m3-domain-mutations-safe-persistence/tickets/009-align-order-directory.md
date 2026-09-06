---
title: Bind Order alignment to current evidence
status: closed
review_start: a3c70a8d0a354b9f35f9ddd10cb14b27cdbcd7ea
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
---

## Outcome

An alignment commit renames only the Order and projected basename that its plan
authorized. A directory or governing fact changed after planning is refused before the
rename.

## Decision sources

- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)

## Blocking

No capability blocker. The repository has one shared worktree, so do not claim this
ticket while another remaining ticket holds it.

## Scope

- Record the Order identity, facts that project the basename, resolved Vendor name, and
  source/destination evidence in the plan.
- Re-resolve and confirm that evidence immediately before the first rename effect.
- Return a typed stale-evidence rejection naming what moved.

## Acceptance criteria

- Replacing the source with a different Order at the same path is refused even though
  it remains an ordinary directory.
- Editing a basename-governing fact or renaming the Vendor after planning is refused.
- Unchanged evidence preserves existing success, collision, no-op, warning, README,
  index, and Supporting Material behavior.
- Review covers the complete ticket diff before closure.

## Exclusions

Automatic alignment during edits, rename suffixes, whole-Order removal, and locking.

## Review boundary

One alignment plan/commit seam and its public suite.

## Closeout

Closed. Started from a clean worktree at
`a3c70a8d0a354b9f35f9ddd10cb14b27cdbcd7ea`; implementation complete at `eba8a53`.

An alignment plan records the Order identity, every fact the aligned basename projects
from, the resolved live Vendor Name, the source, the Package the source belongs to, and
the destination that projection produces. The commit re-resolves the Order and confirms
all of it immediately before the rename, refusing a stale plan with
`OrderAlignmentRejection::PlanEvidenceUnconfirmed`, which carries the evidence seam's own
answer so a caller learns which fact moved rather than an unexplained precondition
failure. A source that no longer resolves to a healthy Order of its Package is a
precondition change rather than stale evidence, matching how Order creation separates the
two.

The no-op arm is confirmed on the same terms. A plan that renames nothing still claims
the Order's facts project the name it already carries, so reporting `Unchanged` after a
fact edit would call the Order aligned at the moment it stopped being.

### Verification

Seven cases were added to the public alignment suite: a different Order left at the
source path though it is still an ordinary directory; the basename-governing fact edits,
table-driven over Order Title, Date Ordered, and Orderer; a cleared Vendor Reference; a
newly associated Vendor; a renamed Vendor; an already aligned Order whose facts moved;
and a source that stopped resolving to a healthy Order. Each was confirmed to fail
against the unguarded commit path rather than only against the absent rejection type.
The 13 pre-existing alignment tests are unmodified and still pass, which is what holds
the unchanged-evidence success, collision, no-op, warning, README, index, and Supporting
Material behavior.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 480 core integration tests, up 7 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1 with npm 12.0.2.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent. Neither axis reported a blocking finding.

Three non-blocking findings were resolved. `resolved_vendor_name` collapsed the Vendor
Reference and Unresolved Vendor Reference concepts that `CONTEXT.md` keeps apart, and
re-tested an `Option` its caller had already matched on. The `source` fact's comment
claimed it notices an Order resolving through an unplanned path, which resolution by
canonicalized input path cannot generally do; identity is the real discriminator.
`FactChange::Appeared` was the untested mirror of the cleared-reference case.

Accepted without change. The `destination` fact is derivable from the facts recorded
beside it and so adds no detection power, but the scope above names source and
destination evidence, and naming the projected name beside the fact that moved it is what
a caller acts on. `projected_basename::<()>` discards a generic error the projection does
not need, and a non-generic split would restructure the pre-existing plan-error type.
`path_bytes` and the `unconfirmed_facts` test helper each restate something a sibling
module spells inline, where sharing either means editing that sibling. The last three all
fall outside this ticket's review boundary.
