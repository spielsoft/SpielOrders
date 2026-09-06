---
title: Give single-file replacements destination evidence
status: closed
review_start: 8df925ff78d798e6605f0a81aa973309f2550788
parent: M3 — Domain mutations and safe persistence
labels: ["implementation:m3", "implementation:task", "origin:external-validation"]
---

## Result

Single-file callers can require an exact or semantic destination expectation and receive
a typed changed-destination refusal. Index, README, upgrade, and record sites adopted
the primitive where their owning ticket authorized it.

Primary evidence: `filesystem.rs`, `atomic_replacement.rs`, and capability integration
suites. Metadata repair's remaining direct promotion is explicitly owned by Ticket 012.

## Reopened by external validation

[The M3 external validation record](../VALIDATION.md), finding V-1. Two single-file
sites take `DestinationExpectation::Unchecked` on the branch reached precisely because
the caller observed nothing at the destination, where
[the durable-write standard](../../../../docs/architecture/durable-writes.md) requires
`DestinationExpectation::Absent`:

- `crates/spielorders-core/src/readme_writes.rs` — a README the plan observed absent. A
  user-authored README created between that observation and the rename is destroyed and
  the call returns `Committed`.
- `crates/spielorders-core/src/canonical.rs` — the derived Package Index rebuild, with
  the same shape and the same gap.

`DestinationExpectation::Absent` already exists in `filesystem.rs`, is compared through
`verify_destination`, and is already used by metadata repair. This is adoption, not new
primitive work.

Both sites also carry an in-code comment claiming the exposure is "named as remaining
exposure in the ticket that adopted this". That is true of the index site, which
Ticket 012's handoff names explicitly. It is false of the README site, which no M3
ticket names.

## Decision sources

- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)

## Scope

- Supply `DestinationExpectation::Absent` at both absent-destination branches.
- Correct the README site's comment, which asserts a disclosure that does not exist.
- Cover each site with a test that fails against the unguarded code: a destination
  appearing between the caller's observation and the rename is destroyed while the call
  reports success.

## Acceptance criteria

- A README created after a creation plan observed nothing survives, and the write is
  refused rather than reported as committed.
- An index file created after a load observed none survives, and the rebuild reports its
  refusal the way it already reports a changed destination.
- Neither test passes against the code as it stood at `review_start`.
- No in-code comment claims a disclosure the ticket set does not contain.

## Exclusions

`removal.rs`'s bare `replace_validated_file` and swallowed cleanup failure (Tickets 022
and 010) and `semantic_repair::identity_exists` (Ticket 013) are the other two
remediation slices and are not this ticket's. Deferred Ticket 027, the M3-upgrade
fixture-builder ticket, and all M4 work are excluded.

## Review boundary

The two absent-destination replacement sites and the tests that pin them.

## Closeout

Closed. Started from a clean worktree at
`8df925ff78d798e6605f0a81aa973309f2550788`; implementation complete at `56a132b`.

Both sites reached `replace_validated_file_expecting` with
`DestinationExpectation::Unchecked` on the branch the caller took *because* it observed
nothing, so the one window the primitive exists to close stood open exactly where the
destination had no other defence. A README the plan observed absent now carries
`DestinationExpectation::Absent`: a document appearing in the window — an editor saving,
an adoption, a restore — survives, and the write is refused as a changed destination
instead of returning `Committed` over the top of it. The derived Package Index rebuild
carries the same expectation where the load observed no index, and reports it through
`PackageIndexReplacementPhase::DestinationChanged` exactly as the changed-bytes case
already did.

The README site's comment claimed the exposure was "named as remaining exposure in the
ticket that adopted this". No M3 ticket named it. The index site's identical claim was
true — Ticket 012 handed that site on explicitly — and that handoff now records it as
taken up.

### The one index write that still carries no evidence

`assess_index` carries no bytes in three states, and only one of them is absence: an
index that is a directory, and an index that is a file whose read failed, are both
plainly there. Absence is therefore taken from the assessment's own `Missing` state
rather than inferred from the absence of bytes.

The other two keep `DestinationExpectation::Unchecked`, unchanged from before this
ticket. The expectation vocabulary can say "these bytes" and "nothing"; it cannot say
"something I could not read", and giving it a variant that can is the shared primitives'
work rather than an adoption. Claiming absence there would be worse than carrying no
evidence: a rename replaces a file it never has to read, so the rebuild heals an
unreadable index today, and refusing it as a changed destination would strand the
Package permanently — with the rebuild that repairs it as the thing being refused — and
blame a concurrent writer that never wrote.

### Verification

Three cases were added, each confirmed to fail against the code it guards rather than
asserted to. The README creation returned `Committed` with the other writer's bytes
gone; the index rebuild left the destination holding this load's candidate in place of
the other writer's index; and the unreadable index was refused as a changed destination
once absence was claimed too broadly, which is the case that caught the over-broad first
attempt.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 530 core integration tests, up 3 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent, against `review_start`, both writing findings to disk as they read.

**Both axes converged independently on one blocking finding**, and it was correct: the
first attempt read "no bytes" as "observed nothing", which regressed the unreadable and
wrong-kind index states into false changed-destination refusals. It is fixed above and
is the reason the third test exists.

Four non-blocking findings, dispositioned:

- **The observation-to-expectation mapping now appears three times**, the third being
  `DiagnosedDestination::expectation` in metadata repair, so extracting it was weighed.
  Declined: the fix above split the index site into three arms, so the three sites no
  longer share a shape, and the semantics that must not diverge already live in one
  place — `verify_destination`. Hoisting metadata repair's carrier is deferred
  [Ticket 027](027-hoist-package-valid-vendors-and-narrow-repair-evidence.md)'s.
- **The two README concurrency tests share their arrangement.** Declined here: the
  duplicated lines are the arrangement that lets each read as a specification, and
  [M3-upgrade Ticket 002](../../../implementation/m3-upgrade-test-harness/tickets/002-fixture-builders.md)
  exists to give these files named fixtures and is sequenced after this work.
- **The README comment read "a document carries no bytes to compare"**, which is false
  as a general claim. Corrected to say a document *observed absent*.
- **The milestone index asserted a reopened status for Tickets 022, 010, and 013 while
  their own front matter still reads `closed`.** The index now says so plainly instead,
  because a validation reopens nothing and a driver reading statuses alone would see no
  work remaining.
