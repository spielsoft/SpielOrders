---
title: Preserve unknown Order README text
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

An absent Order README projects as an empty document; a present README that cannot be
read or decoded projects as unknown. SpielOrders never caches invented empty text in the
Package Index.

When current index data cannot be rebuilt honestly, rebuild is not attempted and the
stored index remains intact. Creation remains distinct because it writes the empty
README itself and therefore knows its text.

The rule is covered across load, fast path, creation, adoption, Order edits, alignment,
removal, and index rebuild by `unreadable_order_readme.rs`.
