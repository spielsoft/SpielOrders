---
title: Replace Vendor deletion with Vendor visibility
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Vendors are never deleted. `hidden` is an optional version-1 Vendor field; absence means
visible. Hide and unhide are reversible, keep the Vendor ID and facts, and never break
existing Order references.

Core exposes both all Vendors with visibility and the offered-only list. The CLI decision
defaults `package show` and `package list vendors` to visible Vendors and uses `--all`
for the complete list.

The Vendor identity, CLI, metadata-schema, and agent-skill decisions contain this final
rule. Primary evidence: `vendors.rs`, `vendor_lifecycle.rs`, the v1 schemas, and index
projection tests.

The former Ticket 004 deletion scope is superseded.
