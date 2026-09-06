---
title: Refuse managed paths that name one filesystem directory
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Package loading and initialization resolve configured Orders and Removed Orders paths
through the filesystem and refuse overlap, containment, aliases, or spellings that the
volume maps to one directory. Volumes that keep two spellings distinct continue to
accept them.

An Orders Directory whose resolved name cannot be expressed is reported as
`configured_orders_directory_name_unusable` rather than panicking.

Primary evidence: `paths.rs`, `managed_path_identity.rs`, and initialization tests.
