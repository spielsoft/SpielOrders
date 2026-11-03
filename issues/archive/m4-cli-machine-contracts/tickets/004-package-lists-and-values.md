---
title: List a Package's Orders, Vendors, and reusable values
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-02T22:06:31Z
closed_at: 2026-09-03T00:48:51Z
review_start: f9e3cf5eb39a604bb408336f9eb4baaa9b66cc5f
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`package list orders`, `package list vendors`, and the four `package values` commands
answer the narrow questions a human or a script asks between full reads: what Orders are
here, what Vendors can I use, and what spellings this Package has already established.

## Result

Six contracts at version 1: `spielorders.package.list.orders`,
`spielorders.package.list.vendors`, and `spielorders.package.values.<kind>` for each of
`ordering_institution`, `orderer`, `project`, and `order_group`.

Rules later command tickets consume. A parameter in a command shape is a segment of the
contract identity like any other, recorded in
[Decision traceability and contract-test conventions](../../../../docs/development/traceability.md).
One load and one presenter — `load_package_contents` and `present_contents` in
`crates/spielorders-cli/src/commands/package.rs` — serve every command that reports Package
contents, so two siblings cannot answer a load state differently, while the classification
of what counts as a failure stays per command. A read's observations travel in the
envelope, so a list narrowed by a Damaged Order does not read as a complete answer. All
six supply the same four fixture cells.

Pinned by `crates/spielorders-cli/tests/package_lists_and_values.rs`.
