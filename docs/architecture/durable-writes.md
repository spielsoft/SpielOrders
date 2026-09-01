# Durable managed-file writes

Decision sources: [Define the SpielOrders package topology](../../issues/wayfinder/spielorders-product-architecture/tickets/002-package-topology.md) and [Define integrity, validation, and repair boundaries](../../issues/wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md).

## Standard pattern

Managed-file replacement uses the shared primitives in
`crates/spielorders-core/src/filesystem.rs`:

1. Validate the candidate before any filesystem effect when it can arrive unvalidated.
2. Write a new staging file beside the destination.
3. Make the staging file durable.
4. Compare the destination with the evidence supplied by the caller.
5. Atomically rename the stage over the destination.
6. On failure, preserve the destination and remove only staging evidence created by
   this operation; report a failed cleanup beside the initiating failure.

A pre-existing stage is retained as evidence of an interrupted write. Repair may
diagnose and explicitly promote it.

Promotion is the second half of that pattern, performed by `promote_staged_file`
rather than by a bare rename. The first half already happened elsewhere, so the stage
cannot be re-staged — the path a fresh stage would take is the one it occupies — but
every later step still applies: the stage is made durable, the destination is compared
with the caller's evidence, and only then is it renamed into place. Nothing is cleaned
up on any path, because the stage is evidence this operation did not create.

## Candidate validation

Validate where untrusted bytes can become a candidate. Do not add a commit-time guard
after typed planning has already produced a valid document merely to defend against an
unreachable internal bug.

The semantic Order-repair guard remains necessary: that path can carry unrelated damage
through the selected repair. A Vendors aggregate a semantic repair assembles gets the
equivalent judgement, and gets it before either managed record is written rather than at
the candidate: duplicate Vendor IDs and package-duplicate Vendor Names are facts about
the set, so the assembled aggregate is asked of the same Package-wide validator that
decides whether a Package's stored `vendors.json` damages the Package. The candidate
that follows is that accepted aggregate's own bytes, so it is not re-asked.

Validating a candidate means validating it against the Package it lands in, not only
against its own schema. Metadata repair accepts a confirmed replacement or a diagnosed
staging candidate only after checking the cross-record constraints no record can be
judged on from its own bytes: that no other Order Directory already holds its Order ID,
that a present Vendor Reference resolves, and that its technical locator leads from this
Order Directory to this Package's root. A repair that skipped them would answer one
diagnosed condition by creating another.

A Metadata Upgrade validates for the same reason and one step earlier. Its candidate is
whatever a registered transformation returned, which is not typed planning's output, so
it is judged before anything is staged: it must declare exactly the version the step was
to reach, and where this repository publishes a schema for that version it must be a
record under it. The judgement happens there rather than at the candidate seam because
an output this build will not write pauses the run for the user to resolve, which is a
different answer from a failed write. The artifacts it reads are held to the same
schema, so an upgrade only ever transforms or passes over an artifact already valid
under its declared version.

The Package-wide half of that judgement reaches only as far as the artifact can answer
for. A Vendors aggregate is asked of the same Package-wide validator a semantic repair
uses, because duplicate Vendor IDs and package-duplicate Vendor Names are facts about
the set. An Order record is asked only for its own semantics: whether its Vendor
Reference resolves is a Package-wide question, and the Vendor aggregate is still at the
source version while Orders are migrating, so there is no trustworthy Vendor set to ask.
The Package Index rebuild that follows the marker advance asks it once, of the whole
migrated Package.

Index rebuild is different from record mutation. A rebuild may have no honest candidate
when an Order README exists but its text was not established. In that case
`index_document` returns no candidate and replacement is not attempted.

## Preconditions read

A commit's preconditions are questions, and a question does not write.
`inspect_canonical_package` reads a Package; `open_package` rebuilds it, and a rebuild
durably replaces the derived Package Index. A precondition that consults a Package asks
the reading load. The rebuilding load belongs after the canonical write, where its
failure is a warning beside a commit that already happened, and that is the only place
any capability calls it.

The guarantee this buys is narrow and worth stating exactly: deciding to refuse writes
nothing. It is not "a rejection changed nothing". A rejection can still report a write
that was attempted and rolled back, and step 6 above requires every restoration failure
to travel with it — `MultiFileReplacementError` carries them into
`SemanticRepairRejection::ReplacementFailed`, and other capabilities carry a
`cleanup_failure` the same way.

A load that cannot answer is its own refusal, never the answer it could not reach.
Treating "nothing can show this identity is free" as "this identity is taken" is
conservative in the right direction and still wrong to report as a collision: a collision
is answered by generating another identity, and a Package nobody can read is not answered
by any identity at all. Semantic repair and metadata repair both name that state
`PackageEvidenceUnavailable`.

## Capabilities that stage their own files

Order creation, Order adoption, and Package initialization do not call these primitives.
They stage a whole new directory or Package and publish it, so they read back what they
staged before publishing it.

That read-back is not the guard the rule above removes. It does not re-ask whether typed
planning produced a valid document; it asks whether the bytes this operation wrote are
still the bytes it wrote. Something outside SpielOrders can change a staged file between
the write and the publication, which is why
`OrderCreationCommitFailureCause::CandidateChanged` exists and is covered by
`tests/order_creation.rs`.

Directory publication is the part of that path this document does cover, under
Boundaries.

## Destination evidence

Callers choose evidence according to the subject:

- `DestinationExpectation::Equivalent` for managed Order and Vendor records replaced one
  at a time, and for the Package version marker a Metadata Upgrade advances. Record
  identity is parsed meaning, so formatting-only changes are equivalent; the marker is
  asked whether it still declares the version the upgrade planned from.
- `DestinationExpectation::Exactly` for READMEs, upgrade transformation inputs, the
  derived Package Index, and metadata repair, where exact bytes are the subject.
- `DestinationExpectation::Absent` where the caller observed nothing at the destination.

No evidence means no changed-destination protection. Every capability must make that
choice explicitly; raw destination replacement is not a substitute.

Semantic expectations must compare through the same normalized projection that planning
used. Comparing a raw parse with a normalized plan incorrectly rejects valid records with
untrimmed stored text.

Metadata repair carries bytes even though it writes an Order record, because its subject
is the damaged content the diagnosis read, and damaged content generally has no parsed
meaning to compare through: a malformed record, or one carrying an unknown field, does
not become a record at all.

Absence is an observation, not a missing one. An interrupted replacement can leave a
durable stage beside a canonical file it never created, and promoting that candidate is
the repair offered for exactly that state — so a record appearing there afterwards is a
changed destination like any other. It is checked by observing the path rather than by
reading it, because a read cannot tell "nothing is there, exactly as observed" from
"something is there and it could not be read".

## Multi-file replacement

`replace_validated_files` validates the complete plan before writing. On failure it
restores prior destinations in reverse order, but only while each destination still holds
what this operation wrote. A concurrent writer's later work is preserved and reported.

`FileReplacement` carries observed bytes or nothing; it has no semantic variant. So a
capability writing several managed records together compares every destination by bytes,
including the Order and Vendor records that single-file replacement compares by meaning.
Semantic repair is the one such capability today. This is stricter than the rule above,
not looser: a formatting-only rewrite of either record between the caller's observation
and the commit refuses the whole repair rather than overwriting it, which costs a replan
and never a canonical byte.

## Boundaries

- These primitives protect managed files, not whole directories.
- They do not provide locking or filesystem compare-and-swap. A caller must still
  revalidate its plan immediately before the first effect.
- Directory publication uses `rename_new_directory`. Its no-replace promise must be
  kept by one filesystem operation, because POSIX `rename` replaces an existing empty
  destination directory: a separate check leaves a window in which a directory created
  at that name is destroyed rather than reported. `tests/directory_publication.rs` holds
  the adapter the suite runs on to that contract against a real concurrent creation; each
  shipped adapter owes the same guarantee.
- Lazy directory creation uses `create_directory`, which must fail when anything already
  stands at the path rather than adopting it. Step 6 above is what depends on this: a
  capability tells the directory it created from one that was already there by whether
  that creation succeeded, so an adapter that accepted an existing directory would hand
  the rollback a directory the user made. `tests/directory_publication.rs` holds the
  adapter the suite runs on to that contract; each shipped adapter owes the same
  guarantee.
