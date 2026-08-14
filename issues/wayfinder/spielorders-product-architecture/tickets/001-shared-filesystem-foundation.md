---
title: Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:research
assignee: /root/shared_foundation_research
---

## Question

Based on SpielGantt's actual Rust/core and frontend boundaries, should SpielOrders extract and share a `FolderBuckets`/`FilesystemDatabase` foundation now, defer extraction behind compatible interfaces, or remain independent—and what is the narrowest coherent shared responsibility if sharing is justified?

## Blocking

Unblocked.

## Resolution

Defer extraction behind compatible application/repository interfaces. The narrow future sharing seam is policy-neutral filesystem bucket mechanics, not domain schemas, CLI/Tauri contracts, search, or frontend models. See [Shared filesystem-bucket foundation](../research/001-shared-filesystem-foundation.md).
