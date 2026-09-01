---
title: Bind mutation plans to authorizing evidence
status: closed
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task"]
---

## Result

`EvidenceFingerprint` records exact relied-upon facts and reports whether each differs,
vanished, appeared, or was not re-observed. Empty evidence never confirms a plan.

Diagnostic-derived fingerprints include the complete typed payload and named facts for
useful refusal messages. Staged candidate content remains owned by Ticket 011 because
the diagnosis must first expose it.

Primary evidence: `evidence.rs` and `plan_evidence.rs`.
