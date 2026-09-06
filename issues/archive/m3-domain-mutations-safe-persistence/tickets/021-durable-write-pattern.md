---
title: Establish the durable managed-file write pattern
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

Managed-file writes validate an untrusted candidate before effects, stage beside the
destination, make the stage durable, compare supplied destination evidence, and replace
atomically. Failures preserve the destination and report any failed stage cleanup.

Candidates already produced by validated typed state do not need a second unreachable
guard. The current standard is [Durable writes](../../../../docs/architecture/durable-writes.md).
