---
title: Make metadata repair Package-valid and no-clobber
status: closed
review_start: 01256cb97f1e64fba76c320c7f262828361d0094
parent: M3 — Domain mutations and safe persistence
labels:
  - implementation:m3
  - implementation:task
---

## Outcome

One confirmed metadata or staging repair commits only a candidate that restores the
selected Package scope and only while the diagnosed destination still holds.

## Decision sources

- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

No capability blocker: the shared no-clobber replacement primitives and the
evidence-bound repair handles this builds on are complete. The repository has one shared
worktree, so do not claim this ticket while another remaining ticket holds it.

## Scope

- Validate a replacement against Package semantics, including duplicate identities,
  Vendor resolution, physical `package_root`, and cross-record constraints.
- Resolve the Package root through the Order locator or loaded layout rather than a
  fixed two-parent walk, including nested configured Orders paths.
- Carry the diagnosed destination evidence into ordinary replacement.
- Bring staging-candidate promotion under the same destination-evidence contract; do
  not call the raw replacement port directly.

## Acceptance criteria

- A standalone-valid record that leaves the Package damaged is refused before writing.
- Nested `orders_path` repairs refresh the real Package and never an inferred ancestor.
- A destination changed after diagnosis is preserved and reported distinctly from a
  write failure for both ordinary replacement and staging promotion.
- A valid confirmed repair commits once, rediagnoses the intended scope, refreshes the
  correct index, and preserves Supporting Material.
- Review inventories every destination-mutating path owned by metadata repair.

## Exclusions

Semantic Vendor/identity repair, partial JSON salvage, batch repair, and upgrade.

## Review boundary

One metadata-repair action at a time, including its Package validation and promotion
paths.

## Closeout

Closed. Started from a clean worktree at
`01256cb97f1e64fba76c320c7f262828361d0094`; implementation complete at `a7b5e0f`.

A confirmed replacement was accepted on the strength of its own schema alone, so a
record that is a perfectly valid Order could still quarantine two Order Directories at
once, leave a Vendor Reference nothing resolves, or carry a locator that leads out of
the Package. Every candidate — confirmed replacement and diagnosed staging candidate
alike — is now judged against the Package before any effect: no other Order Directory
may already hold its Order ID, a present Vendor Reference must resolve, and its
technical locator must lead from this Order Directory to this Package's SpielOrders
Root. Quarantined Orders count for identity, because an Order damaged *by* a duplicate
still holds the identity it was quarantined for. `PackageConstraint` names which of the
three a refusal broke, apart from `InvalidReplacement`, which stays the answer for a
record that is not a valid Order at all.

The Package came from counting two parents, which names the Package only when the
Orders Directory sits directly below the root. A configured `orders_path` may sit at any
depth, so a repair under one refreshed an ordinary folder of the user's instead — or an
ancestor Package, had one been there. The root is now the one the diagnosis was taken
against, reported by `OrderDiagnosis::package_root`, so the repair provably acts on the
Package its authorization came from.

Both write paths carry the destination evidence the diagnosis observed. Absence is one
of the two observations rather than a gap: an interrupted replacement can leave a
durable stage beside a canonical record it never created, and promoting that candidate
is the repair offered for exactly that state. Promotion no longer calls the raw
replacement port; `promote_staged_file` performs the half of the standard pattern that
remains — make the stage durable, compare the destination, rename — and cleans up
nothing, because the stage is evidence it did not create.
`MetadataRepairRejection::DestinationChanged` reports a destination that moved after
diagnosis apart from a write failure on both paths, and retains a staging cleanup
failure where the replacement path produced one.

Metadata repair carries byte evidence rather than the semantic expectation managed
records normally use, and `docs/architecture/durable-writes.md` records why: a diagnosed
destination is damaged content, and damaged content generally has no parsed meaning to
compare through.

### Destination-mutating path inventory

Metadata repair owns three, and each is accounted for:

- the managed artifact under `ReplaceManagedArtifact`, through
  `replace_validated_file_expecting` with the diagnosed destination evidence;
- the same artifact under `PromoteStagingCandidate`, through `promote_staged_file` with
  the same evidence, behind the reread-and-revalidate guard on the staged candidate; and
- the derived Package Index under `RefreshPackageIndex`, through `open_package`, whose
  replacement evidence the Package loader owns and whose failure is reported as a
  warning beside a committed canonical write.

No direct `FileSystem` write, rename, or removal remains in `metadata_repair.rs`, and
the Package evidence consulted while planning is read through `inspect_canonical_package`,
which performs no writes.

### Verification

Seven cases were added to the repair suite and four to the primitive suite. The Package
constraints are table-driven over a taken identity, an unresolvable Vendor Reference, and
a locator leading elsewhere; each was confirmed to fail individually against the
unguarded planner. Both repair actions are exercised under a configured
`Purchases/Active` layout, asserting the plan names the real Package's index, that index
carries the repair, and nothing appears at the inferred ancestor. Three cases drive a
concurrent writer into the window between plan and commit — ordinary replacement,
promotion over an existing record, and promotion onto a destination the diagnosis found
empty — each asserting the other writer's bytes survive and the candidate remains. The
primitive suite pins publishing a stage by rename, refusing a changed destination while
keeping the candidate, reporting an unreadable destination distinctly, and both
primitives reading absence the same way. The pre-existing repair tests are unmodified
apart from fixture generalization.

`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace` all pass — 497 core integration tests, up 11 — as do
`npm run test:schemas` and `npm run check:traceability` on Node 26.8.1.

### Review disposition

`code-review` ran with its two designed parallel sub-agents, Standards and Spec
independent. Neither axis reported a blocking finding.

Five non-blocking findings were resolved. The expectation match kept an arm for bytes an
absence check never reads, which is the unreachable-guard shape Ticket 023 retired; the
byte-reading arms now ask the caller's own question through one reader and that arm is
gone. The locator check existed in three places once this candidate check joined the
Package loader and the configured-path validation, so one predicate now answers it for
all three. `PackageEvidence` knows the Order Directory it was gathered for rather than
being handed it again to answer, and names the "another Order Directory" test once
instead of twice. The two undocumented promotion failures say what each leaves behind,
`PackageEvidenceUnavailable` names the SpielOrders Root it holds, and the reread stage is
compared without cloning the plan's candidate. `promote_staged_file` and the absence
expectation gained direct tests in the module that owns these primitives, where their
coverage had been only indirect.

Accepted without change. Byte evidence refuses a formatting-only rewrite of a
parseable-but-invalid destination, where the semantic expectation managed records
normally carry would accept it; refusing is conservative, loses nothing, and is recorded
in `docs/architecture/durable-writes.md` beside the standing rule it departs from.

### Handed on

The derived Package Index still replaces with `DestinationExpectation::Unchecked` when no
index file exists, where the absence expectation added here would now fit. That site is
the Package loader's, it is pre-existing, and the index is the one derived artifact
SpielOrders owns outright — so it belongs to whoever next opens that code, not here.
**Taken up by [Ticket 020](020-single-file-destination-evidence.md); that site now
carries the absence expectation.**

Ticket 011 handed on that the confirm-before-effect idiom reaches a third copy here and
becomes worth extracting. The destination half of it was extracted: `verify_destination`
is now the one place both primitives compare a destination against a caller's evidence.
The plan-evidence half was not. Routing the staged-candidate reread through
`EvidenceFingerprint` would rework a guard Ticket 011 has already tested, and this
ticket's scope names neither that guard nor the sibling copy in `semantic_repair`, which
is Ticket 013's.
