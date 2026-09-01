---
title: Finish safe Order creation
status: closed
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
---

## Result

An Order-creation plan stays authorized only while the Package facts it used still hold.
A changed Orders Directory, Vendor name, or reusable-value outcome refuses the plan
before identity generation and before any filesystem effect, naming the evidence that
moved.

Commit publishes exactly one complete Order and reports the derived index that each
refresh outcome actually established. A refresh that ran but cannot see the created
Order carries that refresh's own assessment. A failed index replacement keeps the
canonical evidence the completed scan assessed, and leaves the stored index untouched.
Only a refresh that never completed reports stale, not-attempted evidence. Wherever no
refreshed Package can supply the Order, the report carries creation's own projection of
what it committed.

Directory publication never replaces what already holds the destination name.
`rename_new_directory` is one filesystem operation that refuses an occupied destination
in the kernel, so a directory appearing at that name — an empty one included — is
preserved and reported as a collision. A separate check before the rename cannot promise
that, because POSIX `rename` replaces an existing empty destination directory.

`tests/order_creation.rs` and `tests/directory_publication.rs` cover the rule.
`docs/architecture/durable-writes.md` states what directory publication guarantees and
what a capability staging its own files reads back.

## Closeout

- Closing SHA: `bd56e8f`. Resumed from `22be3f2`.
- Focused verification: `order_creation` 19/19 and `directory_publication` 4/4. Required
  gates from a clean formatted tree: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (474 passing), and both
  contract scripts on the pinned Node runtime.
- Review disposition: Standards and Spec ran as independent parallel axes over
  `1619788..bd56e8f`. Spec found nothing blocking. Standards found one blocking
  documentation contradiction — the deleted read-back exemption — now resolved. Its
  second blocking claim, that the "canonical by parsed meaning" decision was lost, was
  checked and rejected: that decision and its Package Index exception are recorded in
  this milestone's index.
- Accepted without change: the exclusive rename exists only in the suite's adapter, so
  no shipped adapter is yet bound by a test; the architecture doc records the obligation
  each one owes. `creation_evidence`'s accessor chains and `PlanEvidenceUnconfirmed`
  holding an outcome its comment excludes both predate this ticket's resumption, as does
  `PreconditionsChanged` having no test anywhere in the workspace.
- Next ready ticket: [Bind Order alignment to current evidence](009-align-order-directory.md).
  Ticket 011 is also unblocked.
