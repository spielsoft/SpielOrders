---
title: Diagnose unusable Orders Directory entry names
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Package load, fast-path assessment, exhaustive validation, and adoption diagnose an
Orders Directory entry whose filesystem name cannot be expressed as a normalized
Package-relative location. They do not panic or inspect the entry.

Such a diagnostic may carry an absolute location without a Package-relative spelling,
because the missing spelling is the condition being reported.

Primary evidence: `orders_directory_entry_names.rs` and canonical load tests.
