---
title: M3 acceptance record
status: superseded by external validation; this is the gate's record, not M3's standing (M3 is externally validated)
gate_ticket: tickets/015-integrate-m3-acceptance.md
---

# M3 acceptance record

This gate accepted M3 at the tree it closes, and everything below was observed on that
tree. External validation has since superseded it three times over, and M3 is validated at a
later tree than this one — see [the Result](#result) and [VALIDATION.md](VALIDATION.md).
A prior acceptance run was
superseded before it; its chronology is in Git history and is not evidence for this tree.

## Candidate and toolchain

| | |
| --- | --- |
| Candidate SHA | `ac2824ed3433bc49212704e5e46a255a0ddd9fe0` |
| Closing SHA | `4909da57fbfa94f9380b43df52426d3cd40f7a59` |
| Worktree at the candidate | clean |
| rustc | 1.98.0 (88d9e12ae 2026-08-18) |
| cargo | 1.98.0 (797e8a9bc 2026-08-05) |
| `rust-toolchain.toml` | `channel = "1.98.0"` |
| Node | v26.8.1, matching `.nvmrc` |
| npm | 12.0.2, matching `packageManager` |

## Repository-required gates

Every gate in [the test loop procedure](../../../docs/development/test-loop.md), run at
the candidate.

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | pass |
| `cargo test --workspace` | pass — 529 tests, 0 failed, 0 ignored |
| `npm run test:schemas` | pass — 4 schemas, 8 accepted examples, 20 rejected fixtures, 22 scenarios, 2 preservation sentinels |
| `npm run check:traceability` | pass — 7 M0 tickets, 4 baseline test files |

The 529 are 527 `spielorders-core` integration tests plus 2 library unit tests. The
Node scripts were run on the pinned runtime; they fail spuriously on an older Node.

## Focused suites

`crates/spielorders-core/tests/` is one target named `core`, so a former file is
selected by module prefix.

| Capability | Module | Tests |
| --- | --- | --- |
| Order creation (Ticket 002) | `order_creation` | 19 |
| Directory publication (Ticket 002) | `directory_publication` | 5 |
| Order Directory alignment (Ticket 009) | `order_alignment` | 20 |
| Whole-Order removal (Ticket 010) | `order_removal` | 19 |
| Repair diagnosis (Ticket 011) | `order_diagnosis` | 15 |
| Metadata repair (Ticket 012) | `metadata_repair` | 21 |
| Semantic repair (Ticket 013) | `semantic_repair` | 15 |
| Metadata Upgrade (Ticket 014) | `metadata_upgrade` | 26 |
| Replacement primitives | `atomic_replacement` | 20 |
| Multi-file replacement | `multi_file_replacement` | 9 |
| Milestone integration (Ticket 015) | `m3_integration` | 10 |

All pass with 0 failed and 0 ignored.

## Milestone integration coverage

Ten tests in `m3_integration`, each composing several capabilities through public
Rust-core seams to establish a claim no single capability owns.

- `a_full_lifecycle_composes_through_public_core_seams_only`
- `every_write_capability_composes_over_a_configured_nested_layout`
- `every_whole_directory_relocation_carries_supporting_material_untouched`
- `the_three_conventional_readme_states_stand_side_by_side_in_one_package`
- `an_unresolved_vendor_reference_is_diagnosed_and_repairable_beside_another_case`
- `a_record_the_build_cannot_read_is_repaired_rather_than_counted_as_unfinished_upgrade`
- `a_refused_write_leaves_the_package_as_it_was_and_costs_no_other_capability`
- `an_interrupted_upgrade_resumes_and_hands_the_package_back_usable`
- `a_derived_index_that_cannot_be_refreshed_never_makes_a_commit_look_retryable`
- `a_rollback_that_cannot_finish_leaves_the_order_active_and_still_disposable`

The composition runs over a configured `Admin/Purchase Orders` Orders Directory and a
renamed `Retired` Removed Directory, not the default spelling, and the upgrade evidence
drives a real 0-to-1 chain rather than a default fixture.

## Preservation evidence

- **Supporting Material.** Two operations relocate an Order Directory as a whole —
  alignment, which leaves the Order active, and whole-Order disposition, which takes it
  out of active scope. Both are driven over a tree holding two opaque binary sentinels
  and a user-authored README, and the tree is compared byte for byte afterwards. Every
  other write capability is exercised over the same tree and leaves it untouched. No
  operation except explicit whole-Order removal moves or disposes of Supporting Material.
- **Conventional READMEs.** Absent, readable, and unreadable stand side by side in one
  Package and stay distinct. An unreadable README is unknown, not empty, and index
  rebuild declines to invent text for it.

## Interruption, rollback, no-clobber, and committed-with-warning

- **Interruption and resumption.** An upgrade interrupted partway resumes and hands the
  Package back usable. Completion is the marker, and the marker advances only on its own
  evidence and only when nothing was blocked.
- **Rollback.** A disposition whose directory move fails and whose undo also fails
  reports both, leaves the Order active with its opaque tree untouched, and leaves the
  disposition still available. An Order stranded half out of scope is what this forbids.
- **No-clobber.** Four refusals from four capabilities — a precommit failure, a
  destination another writer changed, a durability failure whose teardown also failed,
  and an occupied name — leave one Package byte for byte what it was, Supporting Material
  included, and every one of those operations commits afterwards.
- **Committed with warning.** Nine write capabilities in sequence each commit with the
  derived index refresh failing under them: Vendor creation, Order creation, Order edit,
  Item add, README write, adoption, semantic repair, alignment, and disposition. None
  reports a rejection a caller would answer by running the operation again, and one
  ordinary load afterwards shows every canonical change landed exactly once. Metadata
  repair answers the same question over its own diagnosed subject in `metadata_repair`.

## Milestone-wide review

`code-review` ran at fixed point `01256cb..70098a5` in its designed parallel form, with
two independent sub-agents — Standards and Spec — neither seeing the other's conclusions,
as `AGENTS.md` requires. Standards reported no blocking finding. Spec reported one.

### Blocking, resolved

**Semantic repair's committed-with-warning path was exercised nowhere.**
`SemanticRepairWarning` had six occurrences in `crates/*/src` and none in
`crates/*/tests`. `semantic_repair.rs` holds the only arm that turns a failed index
refresh into `CommittedWithWarning { IndexRefreshFailed }` rather than `Rejected`, after
both managed records are already written; a caller told a completed two-record repair
did not happen may rerun it and create a second Vendor for a reference already resolved.
Nothing failed if that arm regressed.

Resolved in the gate, where composition coverage belongs. Semantic repair is now the
ninth capability in `a_derived_index_that_cannot_be_refreshed_never_makes_a_commit_look_retryable`,
committing a `CreateAndAssignVendor` repair over the nested layout with the index
replacement failing under it, and both halves of the repair are asserted on disk. The
coverage was confirmed non-vacuous: removing the warning push makes the test fail, and
the arm was restored unmodified. No capability code changed, so nothing returned to
Ticket 013.

### Non-blocking, resolved

- The canonical-by-meaning decision in the milestone README named the derived Package
  Index as its one exception. Upgrade transformation inputs and metadata repair's
  destination are byte-compared too. The README now records the real carve-outs and
  points at `docs/architecture/durable-writes.md` for the full rule.
- `durable-writes.md`'s destination-evidence rule promised `Equivalent` for managed Order
  and Vendor records, which is false for semantic repair: `FileReplacement` carries
  observed bytes or nothing, so a capability writing several managed records together
  compares them all by bytes. It over-refuses rather than over-writes, so it is safe; the
  document now carves it out and says why it is stricter rather than looser.
- The milestone README said the gate holds the shared worktree two lines above saying the
  worktree is free. Fixed.

### Non-blocking, deferred to Ticket 027

Routed rather than fixed, by owner decision: refactoring four call sites during a
fixed-point acceptance run risks the milestone close for tidiness that costs nothing to
defer. [Ticket 027](tickets/027-hoist-package-valid-vendors-and-narrow-repair-evidence.md)
is deferred and blocks nothing.

- `PackageMetadata::new(&version, &[], vendors).validate()` now stands in four places —
  `canonical.rs`, `vendors.rs`, `upgrade.rs`, `semantic_repair.rs`. Standards called this
  its headline finding and was right that a milestone review is exactly what dissolves
  the per-ticket boundary each earlier review cited for not hoisting it.
- `semantic_repair.rs`'s copy discards the validator's failure vector and hard-codes
  `GeneratedVendorIdentityCollision`. Reachable only via a bug today, but any future
  package-wide Vendor rule would be misreported. Folded into the same ticket, since the
  hoisted predicate is where the verdict should be carried.
- Metadata repair's `DiagnosedDestination::Content -> Exactly` over-refuses a
  formatting-only rewrite of a parseable-but-invalid destination. Ticket 012 accepted it
  deliberately; the owner has since approved narrowing it.

### Non-blocking, accepted without change

- **010 re-copies 009's evidence preamble.** `removal.rs`'s `path_bytes` is byte-identical
  to `alignment.rs`'s, and the evidence blocks parallel each other. Both are capability
  source outside this gate's review boundary, and the gate's own contract forbids hiding
  capability edits in it.
- **Vendor Name equality has a third spelling** in `semantic_repair.rs` beside
  `package.rs` and `vendors.rs`. One domain rule, three call sites that could drift. Same
  boundary; worth hoisting alongside Ticket 027's predicate when someone opens that code.
- **`removal.rs` uses a derived `Debug` string as record identity** where every other
  record comparison goes through `to_json_bytes()`. A behavioural change to a closed
  capability, not a gate edit.
- **Failure vocabulary diverges across capabilities** — `ReplacementDamagesPackage`,
  `GeneratedVendorIdentityCollision`, `UpgradeArtifactFailure` are three spellings of
  "this candidate would leave the Package damaged", and `PackageRootMismatch` names what
  CONTEXT.md calls the SpielOrders Root. A cross-capability rename is the opposite of
  what an acceptance gate should attempt.
- **`FixedCalendar` is copied into more test modules** although one `mod support` now
  serves the single test binary. The copies do not all answer the same date, so hoisting
  one is a behavioural change across ten modules rather than a move.
- **A paused upgrade returns `CommittedWithWarning` with nothing upgraded** and
  `marker_advanced: false`, so the type no longer uniformly means a canonical write
  happened. Ticket 014 accepted this deliberately: in that module `Unchanged` means the
  Package is already newest, and the outcome stays distinguishable through the typed
  result.
- **`ACCEPTANCE.md` was unmodified in the reviewed range**, still reading "Pending". That
  is this document, and it is expected of a range that ends mid-gate.

No blocking finding is unresolved, and no finding returned to a capability ticket.

## Dependency and toolchain currency

Checked as `AGENTS.md` requires: in-major updates are taken by default, a new major is
the owner's to approve, and a blocked in-major update is reported rather than silently
skipped.

- `focaccia` 2.0.0 to 2.1.0 was in-major and is taken.
- `serde` 1.0.229, `serde_json` 1.0.151, `toml` 1.1.4, `tauri` 2.11.5 are newest in
  their selected majors.
- `ajv` 8.20.0 is newest in its major; `npm outdated` reports nothing.
- Node 26.8.1 and npm 12.0.2 are pinned and match `.nvmrc` and `packageManager`.
- **Blocked in-major updates, all by upstream exact requirements, none actionable here.**
  `generic-array` 0.14.7 cannot move to 0.14.9: `crypto-common` 0.1.7 requires
  `=0.14.7`, reached through `digest` 0.10.7 from `sha2` 0.10.9. `toml` 0.8.2,
  `toml_datetime` 0.6.3, and `toml_edit` 0.20.2 cannot move to 0.8.23, 0.6.11, and
  0.20.7: `proc-macro-crate` 2.0.2 pins all three with `=`, reached through
  `glib-macros` from `gtk` under `tauri`, and only on the Linux target — none is on the
  macOS build graph.
- **Outstanding owner decision.** `sha2` is at 0.10.9, the newest in its selected major.
  `0.11.0` exists and is a new major under Cargo semver, so `AGENTS.md` reserves it for
  the owner. It was not adopted. Adopting it would also release the `generic-array` pin
  above, since 0.11 moves to a newer `digest`/`crypto-common`.
- `rustup` itself offers 1.29.0 to 1.29.1. That is a host tool rather than a repository
  dependency, and is the owner's machine to update.

## Result

**Superseded by external validation.** This gate accepted M3, and its counts, gates, and
dependency claims all reproduce independently — twice, at two different targets. What it
missed was three write paths that did not obey `docs/architecture/durable-writes.md`.

Those three were remediated after the first external validation, and
[the M3 external validation record](VALIDATION.md) confirms all three closed by observing
behaviour: each fix was reverted in an isolated clone and each time exactly one test
failed, the new one. This gate's own arithmetic was never the problem, and it is not the
problem now.

**M3 is externally validated**, at `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b` rather than
at this record's tree. The second validation, at
`df83e4214b8883375b52a59e456e81ee4f6b4284`, failed on one blocking finding that this record
could not have caught because it was not about this tree's code: a normative contradiction
between closed decision documents about `orders_path = "."` that the owner's settlement
corrected in two of the three documents it lived in. The third document was corrected in
`d0fb5ee`, and [the third external validation](VALIDATION.md) confirms the settlement now
reaches every document that ever contradicted the ruling. That closure was an owner edit to
a closed decision document — never execution work, and never a rerun of this gate. See
[VALIDATION.md](VALIDATION.md) and [the milestone index](README.md).

This record stands as the gate's evidence for the tree it closed. It is not the source of
M3's standing; the validation is.

### Closing SHA

`4909da57fbfa94f9380b43df52426d3cd40f7a59` — the commit that closed Ticket 015, marked
this record accepted, and transitioned the milestone index and the roadmap. A commit
cannot contain its own hash, so the commit immediately after it records the SHA here and
does nothing else. Neither changes product behavior; the verified tree is the candidate
above.

The gate stops here. M3 has no remaining step: the owner edit the second external validation
named is landed, and the third validation-only external reproduction has accepted it. No M4
work has begun, at this record's tree or at any validated target.
