---
title: Correct semantic Order repairs
status: closed
review_start: 37c0a84dce4c9b99acccfeed7a17923aab01b8d2
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
  - origin:external-validation
---

## Outcome

An explicit Vendor-reference, Order-identity, or physical-locator repair cannot create
a new identity conflict, duplicate Vendor, invalid aggregate, or wrong-Package write.

## Reopened by external validation

[The M3 external validation record](../VALIDATION.md), finding V-3.
`semantic_repair::identity_exists` asks whether a freshly generated Order identity is
already taken, and answers through `open_package` — the *rebuilding* load, the same one
that durably replaces `.spielorders/index.json`. Three consequences follow from one
`let ... else`, the first two of them defects in their own right:

- **A predicate that reads like a query performs a canonical write.** A repair that then
  returns `MutationReport::Rejected` may already have rewritten the derived Package
  Index. `Rejected` stops meaning "nothing was written", which is exactly what the M3
  outcome statement promises it means. The read-only `inspect_canonical_package` is what
  every other commit precondition in this crate uses.
- **Every load that is not `Available` is reported as an identity collision.** Unmanaged,
  damaged, upgrade-pending, and plain filesystem failure all return `true`, and the
  caller renders that as `SemanticRepairRejection::GeneratedIdentityCollision`. A UUIDv4
  collision is astronomically improbable, so in practice every collision this path
  reports is a mislabelled Package-evidence failure. Its implied remedy is to retry with
  a fresh identity, which can never resolve a Package-evidence failure — so the user is
  sent to do something that cannot work, forever, and each retry re-attempts the index
  write.
- **The `OperationFailure` a failed load carries is discarded**, twenty lines above the
  same call being handled correctly as `SemanticRepairWarning::IndexRefreshFailed`.

The conservative direction is right: an identity that cannot be shown free is treated as
taken rather than assumed safe. The defect is that one refusal is reported as a
different, actionable one. `metadata_repair` already carries the correct vocabulary next
door as `PackageEvidenceUnavailable { root }`.

## Decision sources

- [Define Vendor identity and historical semantics](../../../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [Define the Order record and lifecycle contract](../../../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Durable managed-file writes](../../../../docs/architecture/durable-writes.md)

## Blocking

No capability blocker: [Make metadata repair Package-valid and no-clobber](012-apply-metadata-repairs.md)
is closed, so Package-valid replacement and current destination evidence are established.
The repository has one shared worktree, so do not claim this ticket while another
remaining ticket holds it.

## Scope

- Check a generated Vendor ID for collision before it enters the aggregate.
- Use the same trimmed full-Unicode case folding as canonical Vendor validation.
- Validate a staged Vendors aggregate with Package semantics rather than deserialization
  alone.
- Resolve the Package root through the Order locator or loaded layout, including nested
  configured Orders paths.
- Retain the plan-evidence and destination-evidence refusals already established.
- Answer the generated-identity question from a read-only Package load, so no repair
  that refuses has already written a canonical artifact.
- Give a Package-evidence failure its own refusal, distinct from a genuine collision,
  and keep the conservative direction that neither is written.

## Acceptance criteria

- Generated Vendor-ID collision refuses before either managed record is written.
- Names such as `Straße` and `STRASSE` are treated consistently with canonical Vendor
  uniqueness.
- A parseable but semantically invalid Vendors candidate cannot commit.
- Every supported repair works under a nested configured Orders path and refreshes the
  correct Package.
- A rejected repair leaves the derived Package Index exactly as it found it.
- A Package that cannot be read as a current Package refuses the repair by naming that,
  not by naming an identity collision a fresh identity could resolve.
- Each new test fails against the code it guards, shown rather than asserted.
- Existing atomic two-record replacement, rollback, diagnosis, and Supporting Material
  boundaries remain intact.
- Review covers every semantic repair variant before closure.

## Exclusions

Vendor Merge, inferred repairs, broad damaged-record salvage, and new diagnostic kinds.
`vendor_aggregate_is_package_valid`, the *other* site in this file — where an invalid
Vendor aggregate is reported as `GeneratedVendorIdentityCollision` — is deferred
[Ticket 027](027-hoist-package-valid-vendors-and-narrow-repair-evidence.md)'s, as the
validation record states explicitly. The M3-upgrade fixture-builder ticket and all M4
work are excluded.

## Review boundary

The bounded semantic repair cases already exposed by diagnosis, including the
generated-identity precondition and the refusals it produces.

## Closeout

Closed. The semantic repairs were built from
`e6b5847ce4f891e10c898995c65ebb9a00e084d3` and completed at `9c3b920`; the
external-validation remediation started from a clean worktree at
`37c0a84dce4c9b99acccfeed7a17923aab01b8d2`.

The three repairs themselves stand as they were. A generated Vendor identity is judged
as a whole aggregate by the same Package-wide validator canonical validation uses, before
either managed record is written; created Vendor Names fold the way canonical Vendor
uniqueness folds them; and every repair acts on the Package the diagnosis was taken
against, at any configured Orders-path depth.

### The generated-identity question reads, and says which refusal it reached

Asking whether a generated identity was free went through `open_package`, which rebuilds
the derived Package Index and durably replaces it. So the act of deciding to refuse wrote
a canonical artifact, and a caller told `Rejected` was told something that was no longer
true of the Package. The damage was not cosmetic: in a duplicate-identity Package every
participant is quarantined, so the rebuild a refusal triggered replaced an index naming
three Orders with one naming none. `inspect_canonical_package` answers the same question
from the same discovered Orders, healthy and damaged, and writes nothing.

The question also had three answers and a `bool` to give them in. Its `else` folded
"nothing can say" into "taken", and the caller rendered that as
`GeneratedIdentityCollision`. A UUIDv4 collision is astronomically improbable, so in
practice every collision that path reported was an unmanaged, damaged, upgrade-pending,
or failed Package wearing the wrong name — and a collision's remedy, generate another
identity, resolves none of them. The verdict is now three-valued, and
`SemanticRepairRejection::PackageEvidenceUnavailable { root }` names the state under the
same variant `metadata_repair` already refuses it under. The conservative direction is
unchanged: an identity that cannot be shown free is still never written.

Removing the rebuild also dissolved a state the question used to manufacture for itself.
`PackageLoad::IndexReplacementFailed` means the Package read perfectly and only the
rebuild's write failed; through `open_package` that answered "identity unprovable" on
evidence that was in hand. There is now no write to fail.

`docs/architecture/durable-writes.md` gained the rule both defects broke. It was true of
every other capability and written down nowhere, which is why per-ticket review had no
standard to check this site against.

### Verification

Two cases were added to the public semantic repair suite, and each was shown to fail
against the code it guards rather than asserted to. Reverting the verdict to
`open_package` fails the index case alone; reporting the Package-evidence arm as a
collision fails the misreport case alone. The 15 pre-existing cases are unmodified and
still pass, which is what holds the atomic two-record replacement, rollback,
plan-evidence, destination-evidence, diagnosis, nested-`orders_path`, and Supporting
Material behavior.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 535 core integration tests, up 2 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1 with npm 12.0.2.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent, against `review_start`, both writing findings to disk as they read and both
told they shared one worktree. Each reported the mutation it wanted rather than
performing it; the two mutations above are theirs, run afterwards. Neither observed
interference.

One blocking finding, from Standards, and it was correct. The new durable-writes section
claimed a capability reporting `Rejected` "has provably left the Package as it found
it". `Rejected` does not mean that: step 6 of the same document requires a failed
restoration to travel beside the failure that initiated it,
`MultiFileReplacementError` carries rollback failures into
`SemanticRepairRejection::ReplacementFailed`, and the milestone outcome statement lists
rollback failure as its own result. A normative document asserting otherwise is worse
than silence. The section now claims only that deciding to refuse writes nothing, and
says what it does not claim.

Three further non-blocking findings were resolved. "Every precondition asks the reading
load" was too wide — an upgrade commit and Package initialization have preconditions that
read no Package — and is narrowed to preconditions that consult one. The verdict
function's summary line still described a yes/no answer. Its closing paragraph credited
the change with removing a false `IndexReplacementFailed` verdict, which is true and
concerns a state the code can no longer reach, so nothing can pin it; it belongs here
rather than in a comment.

Accepted without change. `PackageEvidenceUnavailable` carries the root and drops the
`OperationFailure` a failed load holds, which is the third consequence V-3 names. Its
`IndexReplacementFailed` half is gone by construction, and `{ root }` is exactly the
shape `metadata_repair` refuses the same state in — the finding V-3 followed through from
was "failure vocabulary diverges across capabilities", so converging on the sibling is
the point rather than a shortfall. Both axes agreed. `generated_identity_verdict` and
`metadata_repair::package_evidence` now share a guard shape and an identity sweep; that
is proven low-level duplication of the kind the roadmap admits without blocking MVP, it
crosses a review boundary this ticket does not hold, and neither site is a defect.
