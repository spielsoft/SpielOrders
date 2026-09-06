---
title: Read a complete Package in one command
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-02T19:05:00Z
closed_at: 2026-09-02T23:55:00Z
review_start: 87d7e50c34e7f5513b3fc53c7ea022e9a27034c5
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`spielorders package show` is the complete GUI-bootstrap read: one invocation returns
everything a client needs to open a Package. A Package that is damaged, unmanaged, or
awaiting an upgrade answers this command too, because saying so is the answer.

## Result

`package show` answers `spielorders.package.show` v1 with the managed paths, the declared
metadata version and the core's own metadata state, the Package README, healthy Orders
with core-calculated totals and resolved Vendors, Damaged Order placeholders, Vendors with
visibility and Contacts, the four Reusable Free-Entry Value lists, the derived index
record, and the load's diagnostics at the narrowest scope the core assigned. Human output
is a labelled summary with UUIDs held back to `--verbose`.

Rules later command tickets consume. The core's typed load state decides the exit status
and the `error.code`; the adapter infers none of it. Metadata state takes precedence over
damage, because the action differs: `upgrade` or a newer build, never `repair`. A failed
read's diagnostics are its answer and travel in the failed envelope. Order-local damage
stays Order-local. Every location appears exactly once in the payload.

Pinned by `crates/spielorders-cli/tests/package_show.rs`.
