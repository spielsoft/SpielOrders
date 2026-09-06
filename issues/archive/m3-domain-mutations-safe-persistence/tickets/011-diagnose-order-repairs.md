---
title: Bind repair cases to the evidence they diagnose
status: closed
review_start: 263e293c7fc09e3b59dd0892b77f448cd9b9c016
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
---

## Outcome

A repair-case handle identifies one diagnosed condition and its evidence, not merely a
diagnostic code at a path. Changed evidence produces a different handle and makes the
old confirmation unusable.

## Decision sources

- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)

## Blocking

No capability blocker. The repository has one shared worktree, so do not claim this
ticket while another remaining ticket holds it.

## Scope

- Derive repair handles from the complete evidence fingerprint established by Ticket
  017.
- Add stable staged-candidate content evidence to
  `DiagnosticEvidence::StagingArtifactPresent`; a digest is sufficient and avoids
  putting arbitrary candidate bytes in a diagnostic.
- Keep handles deterministic for identical snapshots and ephemeral across changes.

## Acceptance criteria

- A changed unresolved Vendor ID, duplicate participant set, invalid record, or staged
  candidate produces a different handle at the same code and path.
- Identical diagnosis snapshots produce identical handles.
- Diagnosis remains side-effect free and preserves stable diagnostic ordering and
  location vocabulary.
- Existing repair actions and scope boundaries do not widen.
- Review covers the diagnostic evidence vocabulary and downstream handle consumers.

## Exclusions

Persisted handles, broad repair orchestration, and applying any repair.

## Review boundary

Scoped diagnosis, staged-candidate evidence, and deterministic case handles.

## Closeout

Closed. Started from a clean worktree at
`263e293c7fc09e3b59dd0892b77f448cd9b9c016`; implementation complete at `ab88349`.

A repair handle named a diagnostic code and a path, so it was identical across every
condition that ever occupied that path. `metadata_repair` and `semantic_repair`
re-confirm a user's choice by looking that handle up in the current diagnosis, so a
confirmation to assign one unresolved Vendor, replace one invalid record, or promote one
staged candidate authorized acting on whatever had since replaced it.

The handle is now derived from `EvidenceFingerprint::of_diagnostic` rather than
restating which facts distinguish a condition. One place decides that, so a handle and a
plan confirmed against the same diagnosis cannot disagree, and Ticket 017's seam gained
its first `src` consumer instead of a parallel implementation.
`EvidenceFingerprint::digest` renders a fingerprint as a bounded name; `confirm` still
compares the facts themselves, because a lossy comparison is the defect that seam
exists to prevent.

Staged candidates needed evidence the diagnosis did not carry, which is why Ticket 017
left that gap here. `DiagnosticEvidence::StagingArtifactPresent` names which candidate
is staged by a digest of its bytes rather than the bytes, and records absence when the
entry has no readable content instead of a stand-in that would confirm.

### Verification

Five cases were added. Four cover the discriminated evidence families — a changed
unresolved Vendor, a changed duplicate-participant set, a changed diagnosed invalidity,
and a changed staged candidate — each at one unchanged code and path. The fifth is the
one that proves the outcome rather than the mechanism: a confirmation is refused through
`plan_metadata_repair` once its evidence moved while the same condition still stands.
Confirmed against the old handle, that case fails by promoting the unconfirmed
candidate. The staging fingerprint contract in `plan_evidence` replaced the assertions
that recorded the now-closed gap, and the diagnostic evidence assertions in
`exhaustive_validation` now name the staged content rather than only its presence.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 486 core integration tests, up 6 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent. Neither axis reported a blocking finding.

Six non-blocking findings were resolved. `ContentDigest::of` length-framed its input, so
it agreed with no other tool that digests a file; the frame belongs only to
`DigestParts`, which has several parts to keep apart. `EvidenceFingerprint`'s doc argued
against digests without noting that `digest` is one, and that method is `pub(crate)`
because only the handle consumes it. Two spellings of a Location's path composed into
one handle, so a non-UTF-8 path was empty in the name and lossy in the evidence; they
share one renderer. The staging read no longer claims to guarantee that a link is never
followed, since the listing's reported kind is what gates it. The handle lookup both
repair-facing test modules had copied moved to `support`.

Accepted without change. A staged entry that is a directory, a link, or unreadable
records one absent fact rather than three distinguishable ones, so two such candidates
confirm against each other; the promotion that follows then fails on the same
unreadable content rather than writing anything. Diagnosing a staged candidate now reads
it whole, which is what a content digest costs.

### Handed on

Binding a confirmed *replacement* to the record bytes it was authored against is not
here. `OrderSemanticInvalid` and `ManagedArtifact` evidence carry the diagnosed
invalidity, not the record's content, so two records differing only in content share a
handle. This ticket's scope names staged-candidate content evidence alone and its
exclusions bar applying any repair; the record-content case is Ticket 012's no-clobber
work, which is also where the confirm-before-effect idiom reaches a third copy and
becomes worth extracting.

`sha2` is pinned at `0.10.9`, the newest release in its selected major. `0.11.0` exists
and is a new major under Cargo semver, so it was not adopted here; it is a candidate for
the milestone-end dependency sweep.
