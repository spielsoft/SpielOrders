---
title: Display monetary values to two decimal places
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-03T17:57:09Z
closed_at: 2026-09-03T19:35:50Z
review_start: dd3ac32803f0b9b1ccb0fb38675b71a2901aecf0
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

Every monetary value this executable shows a human carries two decimal places, and one
place decides that. A total of twelve reads as `12.00`, and no total reads as `-0`.

## Result

`crates/spielorders-cli/src/money.rs` is the crate's one rendering of a monetary value for
a human, reached by `human_orders`, which `package show` and `package list orders` share
so the two cannot describe one Order two ways.

Rules Tickets 005 and 011 consume. The two-decimal rule and "calculations are not rounded"
govern different objects, so rounding happens in the renderer and nowhere else: the core's
number reaches every machine response unrounded, which is why no `contracts/cli/` byte
moved and no `contract_version` advanced. The sign of a zero is decided from the value
rather than the rendered text: everything numerically equal to zero renders `0.00`, and a
nonzero credit that merely rounds to zero keeps its sign as `-0.00`.

Owner ruling (2026-09-04): a non-finite total is rejected at mutation and diagnosed at
validation, never rendered.

Pinned by `crates/spielorders-cli/tests/displayed_monetary_values.rs`, and
`effect_and_stream_boundaries.rs` holds the crate to one money site.
