---
title: Redesign resumable Metadata Upgrade planning and commit
status: closed
review_start: 477a0109d4c2d829310b26b106a8ea76cadef1ca
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
---

## Outcome

One upgrade plan binds the exact consecutive transformation chain and every observable
operation. Commit reclassifies current artifacts, validates what can be validated, and
advances the Package marker only after every artifact reaches the target.

## Decision sources

- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

No capability blocker: [Make metadata repair Package-valid and no-clobber](012-apply-metadata-repairs.md)
is closed, so Package-valid replacement and current destination evidence are established.
The repository has one shared worktree, so do not claim this ticket while another
remaining ticket holds it.

## Scope

- Bind the consecutive transformation registry/chain into the plan; commit must not
  accept a second independent registry.
- Include artifact replacements, marker advance, and index rebuild in the public plan.
- Re-read and reclassify every pending or already-upgraded artifact before acting.
- Validate v1 inputs and outputs where the repository has a schema and validate Package
  semantics at the newest known version.
- Where no schema exists, require each step to declare exactly the next version and the
  chain to contain no gap, repetition, or downgrade.
- Define completion as the Package marker reaching the target version.

## Acceptance criteria

- Planning with one chain and committing with another is impossible through the public
  API.
- A changed pending artifact is not transformed twice, and a changed
  `AlreadyUpgraded` artifact is not silently skipped.
- Wrong source shape, wrong target shape where a schema exists, incoherent declared
  versions, and incomplete chains pause or reject without advancing the marker.
- The plan enumerates marker and index operations, and `is_complete()` remains false
  while the marker is old.
- Interrupted progress remains resumable without undoing completed artifacts; the
  marker advances last and index failure remains a post-commit warning.
- Configured nested Orders paths, Supporting Material preservation, and complete review
  are covered.

## Exclusions

Inventing a v2 schema, downgrade, arbitrary target selection, batch rollback, and CLI
presentation.

## Review boundary

The Metadata Upgrade plan/state machine, transformation contract, marker, and index
composition.

## Closeout

Closed. Started from a clean worktree at
`477a0109d4c2d829310b26b106a8ea76cadef1ca`; implementation complete at `3c22b2b`.

Planning and committing took a registry each, so a caller could classify every artifact
and fix a target version with one chain and then rewrite the records with another. The
chain is now a private field of the plan and the commit takes no registry, so there is
no second chain to disagree with. Two registered steps reading one version were silently
resolved by registration order, migrating with a step nobody chose; that registry is now
refused outright as `IncoherentChain`.

The commit acted on the plan's classification, which is evidence about a moment that has
passed. A record another resume had since finished was migrated a second time, and a
record that had gone back to the source version was passed over, letting the marker
advance over something that never reached the target. Every artifact the run is about to
act on is now reclassified from the bytes on disk, by the same rule planning uses. An
artifact the dry run already named as uninterpretable is the one exception and is not
reopened: the run stops there, where the plan said it would.

Nothing checked what a transformation produced. A step could stamp the target version
onto bytes that are not a record at it, or quietly return the artifact unchanged, and
the marker would advance over the result. Each step's output must now declare exactly
the version that step was to reach — the whole of what is knowable at a version with no
schema — and where this repository does publish one, both the artifacts the run reads
and the artifacts it writes must be records under it. An Order record is asked for its
own semantics only, because the Vendor aggregate is still at the source version while
Orders migrate; the Vendor aggregate is asked of the Package-wide validator.

Completion counted artifacts but not the marker, so an interrupted run whose records had
all finished reported itself complete while the Package still declared the old version —
the one fact keeping ordinary use blocked. Completion is now the marker, which can settle
it precisely because it advances last. The plan also enumerates its writes: a replacement
per outstanding artifact, then the marker advance, then the index rebuild, truncated at
the first blocker so a paused run's preview never reaches the marker, and empty when
there is nothing to carry. The marker advance itself now refuses a marker that no longer
declares the source version, asked by parsed meaning so a reformat is still the marker
the plan observed.

### Verification

The public upgrade suite went from 12 tests to 26. The new ones pin the bound chain, the
enumerated operations, both reclassification directions, an output that is not a record
at the target and one that never left its source version, an artifact declaring the
target while breaking it, an ambiguous registry, a concurrently rewritten marker and a
merely reformatted one, a nested configured `orders_path` with Supporting Material and
Order README byte-compared afterwards, a current Package holding a record this build
cannot read, and the post-commit index-refresh warning. The pre-existing tests are
unmodified apart from the dropped registry argument, which is what holds resumability,
marker-last ordering, rollback-free partial progress, and the blocked and refused pauses.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 520 core integration tests, up 13 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent, and was replayed over the full range after remediation changed the
contract. The replay reported no blocking findings on either axis and confirmed both
earlier blocking findings resolved.

Two blocking findings were resolved. Holding artifacts to the schema of the version they
declare had made completion require valid artifacts as well as the marker, so a Package
already at the newest version holding one unreadable record reported outstanding work
while the commit reported nothing to do — permanently, for a caller driving on
`is_complete()`. A record this build cannot read at the version the Package already
declares is a Damaged Order that integrity repair answers, not unfinished upgrade work;
completion is the marker, `blockers()` still names the record, and a plan with nothing to
carry stops advertising the two writes it will never perform. Separately,
`durable-writes.md` scoped `Equivalent` to Order and Vendor records and enumerated every
candidate-validation site, so both claims became false when the marker gained a semantic
guard and the chain's output gained a judgement.

Six non-blocking findings were resolved: the candidate-validation prose generalized the
Vendor aggregate's Package-wide check to Order records; a comment in `filesystem.rs`
still filed the whole upgrade under byte evidence; `UpgradeOutputFailure` also judged
inputs, so it is `UpgradeArtifactFailure` with version-neutral variants, and
`DeclaredVersion` named a field where its siblings name faults; `classify`'s closure read
as a question about arrival; the module header over-claimed that the commit re-reads
everything; and the post-commit index-refresh warning and the public `as_str` spellings
had no coverage.

Accepted without change. The three `PausedAtBlockedArtifact` sites are three distinct
conditions — the dry run's verdict, an unreadable artifact, a reclassification — reaching
one honest outcome; collapsing them behind a shared state would say less than the code
does. The per-state rule appears in both `planned_operations` and the commit loop because
one is the preview and the other is the act, and their agreement is what the operations
tests pin. `(source, target)` travelling as a pair is a type that could be born, with no
correctness consequence inside this boundary. The source-side schema check is unreachable
while version 1 is the newest, and is written symmetrically rather than special-cased so
it activates with the first real second version. A pause that wrote nothing reports
`CommittedWithWarning` rather than `Rejected`: in this module `Unchanged` means the
Package is already at the newest version, and conflating a run blocked at its first
artifact with a finished one would be the worse report. An Order Directory appearing
between planning and committing is not migrated, because the plan is the authorization
and re-enumerating would act on artifacts it never assessed; ordinary Package use is
marker-gated throughout an upgrade, and such a record is diagnosed as a Damaged Order.
