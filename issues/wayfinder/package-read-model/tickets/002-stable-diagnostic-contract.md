---
title: Define the stable diagnostic vocabulary and evidence contract
status: closed
---

## Decision

[Core diagnostics](../../../../crates/spielorders/src/readme.rs) and their tests
define codes, typed evidence, scope, and deterministic ordering. Adapters consume that
evidence rather than parsing human messages or manufacturing a second taxonomy.

Canonical damage, usable warnings, derived-index assessment, refresh outcome, and an
operation that failed are distinct concepts. A failed index replacement does not
invent canonical damage. A document-specific warning does not damage its owning
Package or Order.

Report the narrowest established scope and only trustworthy identities and locations.
A duplicate identity does not make one participant the chosen survivor. Every conflicting
Order retains its own path-based diagnostic presence. Unknown or unexamined facts remain
absent; source-library wording and enumeration order do not determine identity, equality,
or ordering.

Read-path evidence is bounded by what that operation actually examined. A fast load
cannot diagnose unread canonical bytes, README contents, reserved directory entries,
or Supporting Material. An explicit README read examines that exact document.
Exhaustive validation is separately invoked and still never inspects Supporting Material.

[Define integrity, validation, and repair boundaries](../../spielorders-product-architecture/tickets/008-integrity-repair.md)
owns damage policy and repair authority. The
[CLI contract](../../spielorders-product-architecture/tickets/006-cli-contract.md)
owns machine response compatibility; human wording is not that contract.
