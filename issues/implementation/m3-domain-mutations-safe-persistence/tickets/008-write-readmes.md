---
title: Write Package and Order READMEs atomically
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Package and Order README writes are complete. Planning is side-effect free; commit
replaces only the selected conventional document, carries exact destination evidence,
preserves arbitrary UTF-8 bytes, and refreshes the Order index when applicable.

Changed, unreadable, absent, or wrongly typed destinations are distinguished. A failed
post-commit Package load or index refresh remains a warning rather than a retry signal.

Primary evidence: `readme_writes.rs` and its integration suite.

No remaining M3 work is owned here.
