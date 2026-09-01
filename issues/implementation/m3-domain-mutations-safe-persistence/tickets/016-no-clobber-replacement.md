---
title: Refuse changed destinations in shared replacement primitives
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Shared single- and multi-file replacement can carry destination evidence. A destination
changed before replacement is preserved and reported; rollback restores only bytes the
operation can still prove it wrote.

Primary evidence: `filesystem.rs`, `atomic_replacement.rs`, and
`multi_file_replacement.rs`.

Capability owners must supply the correct evidence; remaining adoption sites are owned
by Tickets 012–014.
