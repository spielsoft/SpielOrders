---
title: Make whole-Order removal evidence-bound and rollback-complete
status: closed
review_start: 84b1ef9cd278a3e8b99f9bfb6de9383094ebb475
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
  - origin:external-validation
---

## Outcome

Whole-Order removal acts only on the Order its plan authorized. A failed recoverable
removal leaves no unreported directory that the operation created.

## Reopened by external validation

[The M3 external validation record](../VALIDATION.md), finding V-2, second half. The
locator rewrite in `crates/spielorders-core/src/removal.rs` ends in `.is_ok()`, which
discards the whole `ValidatedReplacementError` — the initiating failure and the
`cleanup_failure` beside it — against
[the durable-write standard](../../../../docs/architecture/durable-writes.md)'s rule
that a failed cleanup is reported beside the failure that initiated it.

`OrderRemovalWarning::LocatorUpdateFailed` is a unit variant carrying neither path nor
error, so an orphaned `.order.json.spielorders-stage` can be left inside the removed
Order Directory with nothing naming it. This ticket's own outcome is that removal is
rollback-complete, and a cleanup failure that vanishes is not.

## Decision sources

- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)

## Blocking

No capability blocker. [Bind Order alignment to current evidence](009-align-order-directory.md)
is closed, so the whole-directory subject confirmation it establishes is available to
reuse. The repository has one shared worktree, so do not claim this ticket while another
remaining ticket holds it. The remaining defect at the locator rewrite belongs to
[Ticket 022](022-record-round-trip-and-byte-evidence.md), so the two are claimed
together.

## Scope

- Bind the plan to the selected Order, source directory, and projected destination.
- Confirm the evidence before platform Trash or recoverable move effects.
- Track whether the operation lazily created the Removed Orders Directory.
- On collision or move failure, remove that newly created directory when safe; if
  cleanup fails, report it beside the initiating failure.
- Carry the locator rewrite's own failure and its failed staging cleanup out to the
  caller, named by path, instead of collapsing both into a bare `bool`.

## Acceptance criteria

- A different Order substituted at the planned source is never moved or trashed.
- A destination collision and an injected rename failure leave no empty Removed Orders
  Directory created by the rejected operation.
- Cleanup never removes a pre-existing directory or concurrent evidence.
- A locator rewrite that fails, and whose staging cleanup also fails, reports both: why
  the rewrite did not happen, and the staging file still standing beside the record.
- Existing success, warning, locator, README, index, and Supporting Material behavior
  remains intact.
- Review covers both Trash and recoverable dispositions before closure.

## Exclusions

Permanent deletion, recursive inspection, retention policy, and platform Trash adapter
implementation. The missing destination expectation at the same site is
[Ticket 022](022-record-round-trip-and-byte-evidence.md)'s, and
`semantic_repair::identity_exists` is [Ticket 013](013-resolve-order-repairs.md)'s.
Deferred [Ticket 027](027-hoist-package-valid-vendors-and-narrow-repair-evidence.md),
the M3-upgrade fixture-builder ticket, and all M4 work are excluded.

## Review boundary

The whole-Order removal plan/commit seam and rollback ownership, including the locator
rewrite's failure reporting.

## Closeout

Closed. The removal capability was built from `2990c7f39cb24a387711ae83502e69a460709885`
and completed at `369a167`; the external-validation remediation started from a clean
worktree at `84b1ef9cd278a3e8b99f9bfb6de9383094ebb475` and completed at `2e2db1a`.

A removal plan records the Order identity, the source, the Package the source belongs
to, and the projected destination, and the commit re-resolves the Order and confirms all
of it before its first effect, refusing a stale plan with
`OrderRemovalRejection::PlanEvidenceUnconfirmed`. The recoverable disposition also binds
the Order record, because it is the only disposition that rewrites that record and the
copy it writes is the plan's own. A source that no longer resolves to a healthy Order of
its Package is a precondition change rather than stale evidence, matching alignment.

The commit records the Removed Orders Directory it creates and undoes exactly that on a
collision or move failure, while it is still the empty ordinary directory it created. A
pre-existing directory is never a candidate and concurrent evidence retains it; a
rollback that cannot be completed is reported as `RemovedDirectoryCleanup::Failed`
beside the rejection that initiated it.

### The locator rewrite reports what it left behind

The rewrite that corrects a moved Order's technical locator is a durable replacement
like any other, and it ended in `.is_ok()`. That discarded the initiating failure and,
with it, the staging file the replacement's own cleanup could not remove — inside a
directory the removal has just taken out of active scope, where no diagnosis reaches it.
`OrderRemovalWarning::LocatorUpdateFailed` now carries `LocatorUpdateFailure`: the
record the rewrite was to replace, the primitive's own failure, and the orphaned stage
with the error that stopped it being taken back, named only where one is really still
there.

The record path is a path to look at rather than a claim about its content, because two
of the arms are refusals: a changed destination means another writer's record is
standing there, and an unreadable one means nobody knows. The replacement's error is
carried whole rather than re-spelled, which is how `order_edits` and `vendors` already
report this primitive. That costs the warning its `PartialEq` for a filesystem error
that has none, which `OrderRemovalRejection` and `RemovedDirectoryCleanup` had already
given up for the same reason.

### Verification

The public removal suite holds the plan/commit seam: a different Order left at the
planned source, over both dispositions; an Order edited after planning, on the
depth-changing disposition that rewrites the record; the rollback itself over an erroring
move and a reported collision; a pre-existing Removed Orders Directory; concurrent
evidence arriving inside a created one; and a rollback that cannot complete. Each was
confirmed to fail against the unguarded commit path, and each cleanup guard separately by
removing it. The remediation adds a rewrite that fails at the rename while its staging
cleanup also fails, and asserts that both the initiating failure and the stage still on
disk are reported; it was confirmed to fail when the replacement's error is discarded,
which is the shape `.is_ok()` had. The pre-existing removal tests are unmodified apart
from one `assert_eq!` the lost `PartialEq` forced into an equivalent slice pattern, and
they are what hold the success, warning, locator, README, index, and Supporting Material
behavior.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 533 core integration tests — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1 with npm 12.0.2.

### Review disposition

`code-review` ran both times with its two designed parallel sub-agents, Standards and
Spec independent, each writing findings to disk as it read.

The remediation pass drew one blocking finding, and it was correct: `record()` promised
the record still carried the locator from before the move, which the two refusal arms the
same change introduced make false — a caller acting on it would rewrite over the
concurrent writer the refusal had just protected. Corrected above. Three non-blocking
findings were resolved: the staging path was stored on every arm and rendered by `Debug`
where no stage existed; the candidate closure named no per-capability failure type and
stated nothing about why no candidate validation is owed, alone among the crate's
replacement sites; and both ticket documents carried a stale close and a claim, copied
from the validation record, that `into_cleanup_failure` has no caller — `metadata_repair`
has called it since before this range.

Accepted without change from that pass. `failure()` returns an `Option` for a projection
that cannot fail in practice: the alternative is a panic after a whole directory has
already moved, which is a worse answer than a warning, and the state was reachable in the
same way before this change. `replace_validated_file` now has no production caller and is
exercised only by `atomic_replacement.rs`; it remains a legitimate public primitive and
removing it is `filesystem.rs`'s to decide, not this seam's.

The original pass reported one blocking finding. The rollback's claim that it never
removes a pre-existing directory rested on `create_directory` refusing an existing path,
which no port stated: an adapter using
`create_dir_all` satisfied the port as written, would report success for a directory the
user made, and the rollback would then be entitled to remove it. The obligation is now
stated on `DirectoryFileSystem::create_directory`, recorded in the durable-writes
boundaries beside the `rename_new_directory` obligation it parallels, and held against
the shipped adapter by `tests/directory_publication.rs`, which fails when that adapter
adopts. Nothing shipped was wrong; the guarantee was unwritten.

Three non-blocking findings from it were resolved. Binding the Order record on the Trash
disposition refused a removal for an edit that disposition never reads, so it is now
recorded only where something writes it. A comment claimed the emptiness check closes
the concurrent-arrival window it only narrows. Two test names led with a local helper
rather than the behavior.

Accepted without change from it. `path_bytes` and the opening facts of `removal_evidence`
restate what `alignment.rs` spells inline; hoisting them means editing that sibling,
which Ticket 009 dispositioned the same way. `order_record` is rendered through `Debug`,
as `evidence.rs` already renders a diagnostic payload, and is bound whole because it is
rewritten whole, so a refusal names the record rather than a field. `RemovedDirectoryCleanup`
shortens the Removed Orders Directory the way the pre-existing sibling variant in the
same enum does. `PlanEvidenceUnconfirmed` carries no path, matching alignment. The last
four all fall outside this ticket's review boundary or follow the seam it reuses.
