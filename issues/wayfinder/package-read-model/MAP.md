---
title: Chart the Package read model
status: closed
---

## Package read constraints

These decisions constrain the implemented Rust read model. Code and contract tests
define concrete result types and evidence shapes.

- [Decide how Package Index freshness is proven without canonical rereads](tickets/001-index-freshness-fast-path.md)
- [Define the stable diagnostic vocabulary and evidence contract](tickets/002-stable-diagnostic-contract.md)
- [Define the complete Package read projection and load-state contract](tickets/003-package-read-projection.md)

The [product decisions](../spielorders-product-architecture/MAP.md) govern Package
ownership, metadata, search, and repair. No desktop framework is part of this boundary.
