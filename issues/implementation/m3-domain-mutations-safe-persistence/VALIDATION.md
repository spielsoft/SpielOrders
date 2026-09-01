# M3 external validation

Status: **passed. M3 is externally validated at `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b`.**

Validation date: 2026-09-01

This is the third external validation of M3 and the second of the remediated tree. It is a
**scoped re-validation**: the second validation validated `df83e42` and failed it on one
blocking finding, W-1, a stale statement in a governing decision document. This record
establishes whether W-1 is genuinely closed. It fixed nothing it audited.

## Fixed points

| | |
| --- | --- |
| Pre-M3 fixed point (externally validated M2 target, and the exact merge base) | `18101a3c419d6794f7768b99820ac2d1976a836d` |
| Target validated by the second external validation, which failed | `df83e4214b8883375b52a59e456e81ee4f6b4284` |
| Exact validated target | `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b` |

`df83e42` is an ancestor of this target. The source worktree was clean and `HEAD` resolved
exactly to the target before validation began, and remained clean until this record and the
three M3 records it governs were applied.

## Why this validation was scoped, and the proof that it may be

`.claude/skills/implement/SKILL.md` and this repository's practice both hold that a later
documentation-only commit does not invalidate a completed executable gate when source,
tests, fixtures, manifests, lockfiles, scripts, contracts, and toolchain selectors are
unchanged. The project-local `code-review` skill states the same rule for review: "For a
follow-up diff that changes only documentation/status/evidence, review those changed claims
and their directly affected sources. Do not automatically replay a completed code review
over the original implementation range."

**That premise was verified rather than assumed.** `git diff --raw -M -C df83e42..HEAD`
returns exactly five entries, every one a `100644`→`100644` modification of a Markdown file
under `issues/`, with no addition, deletion, rename, copy, mode change, or gitlink:

```text
M  issues/implementation/m3-domain-mutations-safe-persistence/ACCEPTANCE.md
M  issues/implementation/m3-domain-mutations-safe-persistence/README.md
M  issues/implementation/m3-domain-mutations-safe-persistence/VALIDATION.md
M  issues/roadmaps/spielorders-implementation-roadmap.md
M  issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md
```

No path outside `issues/**/*.md` appears. The five commits are `2d0ae29`, `81d96ff`,
`301d38f`, `a5015a5`, and `d0fb5ee`, each touching one file for one reason.

The three lockfile and two Supporting Material sentinel hashes are byte-identical to the
values the second validation recorded, which is an independent confirmation of the same
premise from the artifacts rather than from the diff:

```text
59fec15faa932361de1d8b923a9b7baa80d18a5c3672d1942831f1c3711e0269  Cargo.lock
f11f72a8907f70bed809a3de1a5e1b884bea8980417ff567c3cfe64900d56af5  package-lock.json
7edf354696280a87e6709a1fe7146d96ce16c118363c90f8bf6c37c3e479bef1  apps/desktop/package-lock.json
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

Supporting Material has now survived three milestones, one remediation, and one settlement
unchanged. `git diff --check df83e42..HEAD` passes.

## W-1 is closed

The fix is commit `d0fb5ee`, eleven lines in
[Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).
It makes three assertions, each claimed to align that document with behaviour that already
shipped. **Each was checked for truth, not merely for presence** — against
[Define the Rust CLI capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md),
the checked-in JSON Schemas under `contracts/schemas/v1/`, the conformance fixtures and
examples, and the shipped Rust and the tests that pin it. All three are true.

**Assertion 1 — `orders_path` may not be `.`. True.** `006`'s "Removal and mutation
boundaries" section rules that "`orders_path = \".\"` is no longer valid; it must be a
nonempty relative descendant of the SpielOrders Root". `paths.rs::validate_configured_path`
refuses it as `ConfiguredPathFailure::CurrentDirectory`, pinned by
`package_paths::unsafe_configured_path_shapes_are_rejected_before_filesystem_traversal`,
by `package_initialization`, and end-to-end by `canonical_package_load`, which feeds the
literal `orders_path = "."` and asserts the Package is damaged. `010` now extends the
refusal to `removed_path`, which `006` does not state; the same predicate refuses it, so
the generalization is accurate rather than invented.

**Assertion 2 — `config.toml` permits `removed_path` as well as `orders_path`. True, and
every clause of the new sentence holds.** Checked individually against the `Config` struct's
serde attributes, `validate_configured_path`, and `resolve_managed_paths`: two optional keys;
`deny_unknown_fields` makes unknown keys invalid; `default_orders_path` and
`default_removed_path` return exactly `Orders` and `Removed`; an empty value fails as
`EmptyComponent`; absolute paths fail including the `\`-prefixed and Windows-drive forms;
`..` fails as `ParentTraversal`; a link that resolves outside the root fails as
`OrdersDirectoryOutsideRoot`, because every path prefix is canonicalized rather than only
the leaf; overlap is refused both lexically and after canonicalization; `.spielorders` is
reserved; and `removed_path` must be a direct child (`NotDirectChild`) that, if present, is
an ordinary directory (`RemovedOrdersDirectoryNotOrdinary`). Package-version information
does not appear in the type. Every one of those refusals is pinned by a named test.

**Assertion 3 — version-1 `order.json` requires a technical `package_root` locator. True.**
`006`'s "Targeting and metadata refinements" section adds it as "a normalized relative path
from the Order Directory to its SpielOrders Root, never an absolute path or a domain fact",
and `010` reproduces that wording and attributes it to `006`, which `006` itself requires
("This requirement refines the version-1 schema … before implementation"). It is in the
`required` list of `contracts/schemas/v1/order.schema.json` and non-optional as
`OrderDocument::package_root`. The example value `"../.."` added to `010` satisfies the
schema's `^(?:\.\./)+\.\.$` pattern and `PackageRoot`'s "two or more parent-only segments"
deserializer, matches `contracts/examples/v1/minimal/order.json` verbatim, and is
semantically right: the locator is measured from the Order Directory, not from `order.json`,
so a single-component `orders_path` yields exactly two parents, which is what
`locator_resolves_to_root` reads.

**No other document contradicts the ruling.** This sweep was run independently rather than
trusted from any record, because the first settlement failed precisely by correcting some
documents and asserting it had corrected all of them. All 35 files containing `orders_path`
were inspected; every assertion about a `.` value across Markdown, Rust, JSON, and TOML was
enumerated; the old phrasing and its paraphrases were searched for and return zero hits; all
34 Markdown files mentioning `config.toml` were read for a competing grammar, and only
`002-package-topology.md`, `006-cli-contract.md`, and `010-metadata-schemas.md` state one —
all three now agreeing. `CONTEXT.md`, `AGENTS.md`, `README.md`, `WayfinderPrompt.md`,
`docs/`, and the contract fixtures carry no competing claim. **There is no fourth dissenting
document.**

**The settlement commit does not repeat its predecessor's error.** `8df925f` claimed "only
that one sentence dissented", an unqualified completeness claim about the corpus, and it was
false. `d0fb5ee` instead says "The code, the checked-in JSON Schema, and the conformance
fixtures already agree with all three. Only this document dissented" — scoped to the three
claims it makes, and true as scoped. It asserts no global cleanliness it does not have.

## Verification performed at this target

Every repository-required gate was run at the target in the source worktree, which was clean
throughout:

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed — 537 tests, 0 failed, 0 ignored |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 20 rejected fixtures, 22 scenarios, 2 sentinels |
| `npm run check:traceability` | Passed: 7 M0 tickets, 4 baseline test files |

The 537 are 535 `spielorders-core` integration tests plus 2 library unit tests, exactly the
count the second validation recorded, as an unchanged source tree requires. The Node scripts
ran on the pinned Node 26.8.1 and npm 12.0.2.

`crates/spielorders-cli/src/main.rs` is still the two-line stub. **No M4 work has begun.**
Ticket 027 and the M3-upgrade fixture-builder ticket are untouched, and no closed ticket was
reopened.

## Checks deliberately not run, and why

Stated plainly so the scope of this record is not mistaken for the scope of the second one:

- **No rebuild from an absent target in a fresh no-hardlink clone.** The second validation
  did this at `df83e42`; no source, manifest, lockfile, or toolchain selector has changed
  since, as the inventory above proves.
- **No mutation sweep.** The second validation closed V-1, V-2, and V-3 by reverting each
  fix in an isolated clone and observing exactly one test break each time. Those results
  stand unchanged at this target because the sources do.
- **No dependency currentness sweep and no lockfile recreation.** No manifest or lockfile is
  in the range. The second validation's three currentness observations remain on record and
  remain routed to the M4 dependency check.
- **No desktop frontend test, Vite build, or `tauri build`.** `cargo test --workspace`
  compiled the Tauri crate; the frontend toolchain is untouched by a Markdown range.
- **No replay of the code review over the M3 implementation range**, which the `code-review`
  skill directs against for a documentation-only follow-up.

## Independent code review

The project-local `code-review` workflow ran over `df83e42..HEAD` only. Standards and Spec
ran as two sub-agents in one message, in the foreground, neither seeing the other's
conclusions, as `AGENTS.md` requires. Both were told they share this one worktree and were
required to report any mutation they wanted performed rather than performing it; neither
needed one, and both confirmed the tree clean at start and finish. Both wrote findings to
disk as they produced them. Neither was told what this validator had already concluded.

One disclosed deviation, the same one both previous validations took: the skill's 400-word
cap per axis was lifted, because the subject is quoted normative prose where precision beats
brevity. Every other element of its designed form was kept.

- **Spec**: 1 blocking, 5 non-blocking. It verified all three assertions clause by clause,
  ran its own independent `orders_path` corpus sweep, and compared every illustrative JSON
  block in `010` against its schema's `required` list. Its blocking finding is R-1 below,
  which this record downgrades for the reason given there.
- **Standards**: 9 blocking, 11 non-blocking. All nine are one root cause with one remedy —
  the four live M3 records still described the owner edit as outstanding after `d0fb5ee`
  landed it. That is the record carry-forward this validation performs, and applying it is
  the whole of the remedy. Its worst issue, the roadmap's "Recommended next execution slice"
  instructing a reader to perform a landed commit, is corrected by this record's companion
  edits.

**This validator reached R-1 independently, before either axis reported.** Every finding
below was verified directly against the code, schemas, and fixtures rather than relayed.

## Findings

### R-1 — non-blocking — `010`'s `index.json` example omits the required `package_root`

`issues/wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md`, the
`index.json` version 1 section.

The embedded `order` object in that section's example carries `schema_version`, `order_id`,
`title`, `date_ordered`, `status`, and `items`, and no `package_root`. That document is
rejected by `contracts/schemas/v1/index.schema.json`, whose `$defs/order` requires
`package_root` and is referenced by `$defs/order_entry`. The repository ships that exact
shape as a **negative** fixture — `contracts/fixtures/schema-invalid/index-missing-package-root.json`,
registered in the manifest — so the conformance suite asserts those bytes must fail. It is
the same field, the same schema, and the same class of staleness that `d0fb5ee` closed one
section higher.

**Recorded, and not blocking.** Three things separate it from W-1:

1. **The governing prose is true.** `010` says each Order projection requires "a complete
   canonical `order` object", and `010`'s `order.json` section now lists `package_root` as
   required, so a reader implementing from this document alone derives the correct grammar.
   W-1 was blocking because the normative sentence itself stated a false rule and a reader
   who loaded `010` alone would implement the wrong one. Here only an illustration is
   under-populated.
2. **Provenance.** `git log -S` places the block at the initial commit `0ea9a10`. It
   predates M2's externally validated target and two prior external validations passed over
   it. It is not M3's regression, and it is the same calibration the second validation
   applied to the two `010` items it recorded non-blocking.
3. **The shipped artifacts all agree.** Schema, negative fixture, both positive examples,
   and `canonical.rs`'s index projection are correct and mutually consistent, and
   `npm run test:schemas` validates them.

**Owner:** [Define durable metadata schemas and version evolution](../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md).
No execution ticket owns a closed wayfinder decision document and a validator must not edit
one. The next owner touch of that document should close it, for the same reason the second
validation gave: a narrowing must land in the same change or the document becomes false.

### Non-blocking findings worth recording

- **`010`'s new grammar sentence omits one shipped refusal.** Any configured value containing
  `\` is refused outright as `ConfiguredPathFailure::PlatformSeparator`, which "`/`-separated"
  does not cover — on POSIX, `Orders\Active` is a legal single directory name that satisfies
  the stated grammar and is nevertheless refused. `002-package-topology.md` and
  `006-cli-contract.md` are silent too, and the M1 ticket does record it, so nothing is
  ambiguous to an implementer. Also unmentioned, though resolution-time rather than grammar:
  `OrdersDirectoryNotDirectory`, `OrdersDirectoryNameUnusable`, and `RootNotDirectory`.
  Predates M1's validated target. Owner: the same decision document.
- **"cannot … occupy `.spielorders`" is enforced at the first component only**, deliberately
  and with a written rationale in `paths.rs`: reserving the name at every depth would refuse
  `Orders/.spielorders`, which resolves and loads today and damages nothing. Read as the
  Root's own metadata directory — the only reading the section supports — the clause is true.
  `010` inherits the wording verbatim from `006`, so it is no less precise than its governing
  source.
- **`010` drops `006`'s "The Removed Orders Directory is created lazily."** Read strictly,
  "must name an ordinary direct-child directory" implies existence, where
  `resolve_managed_paths` treats absence as an ordinary answer. Editorial completeness only.
- **`010`'s version-advance rule still reads absolutely** — "Any change to a managed JSON
  field, representation, requiredness … advances the one package version" — while `006` added
  a required version-1 field without advancing it. `006` carves this out explicitly as a
  pre-implementation refinement, and `d0fb5ee` improved matters by naming `006` at the point
  the field is introduced. No live inconsistency; the rule simply carries no carve-out.
- **`010`'s `removed_path` clauses carry no supersession pointer** where the other two
  refinements in the same hunk cite `006` by name and link. The repository's sanctioned
  mechanism for correcting a closed decision is an in-place edit with the supersession
  recorded at the statement itself, which `002-package-topology.md` models and which
  `check-traceability.mjs` effectively mandates by requiring every wayfinder decision file to
  be `status: closed`. Content correct; attribution incomplete for one clause.
- **`@types/node` is now due for an in-major refresh.** The second validation recorded it
  pinned at 26.4.0 against a registry `latest` of 26.4.1 published 28 minutes before `df83e42`
  closed, and routed it to the M4 dependency check along with transitive `libredox` and
  `smallvec`. That routing stands and this validation did not re-run the sweep. Noted only
  because `AGENTS.md` applies in-major updates by default, so the refresh is ordinary work
  whenever the tree is next touched — it gates nothing and blocks nothing here.
- **The second validation's own non-blocking findings remain on record and are not
  re-litigated here.** `metadata_repair::staging_path_of`'s diverging copy of
  `filesystem::staging_path`, `creation.rs`'s unconditional `.spielorders` rollback removal,
  `boundaries.md`'s omission of `NativeTrash` and `DirectoryFileSystem`, the now-callerless
  public `replace_validated_file`, `traceability.md`'s seven-section rule against the
  milestone index's authorized compression, `PreconditionsChanged`'s vocabulary imprecision,
  `orphaned_stage`'s scope, `004-vendor-identity-history.md`'s in-place rewrite, Order
  creation's hard-coded `in_preparation`, and the stale `README.md` and crate-level docs all
  stand exactly as that record left them. None blocks.

## Result

**M3 passes external validation** at `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b`.

W-1 is genuinely closed. The owner's ruling now reaches every document that ever contradicted
it, verified by an independent corpus sweep rather than by trusting the settlement's own
claim — which is the check the previous settlement failed. The three statements the fix
substituted are each true against the shipped code, the checked-in schemas, the conformance
fixtures, and the governing CLI contract, checked clause by clause; none replaced a false
statement with a differently false one. The range that produced the fix is documentation
only, proven by a full path inventory and corroborated by five unchanged integrity hashes, so
the second validation's executable gate at `df83e42` stands, and the gate reproduces green at
this target anyway.

What remains in `010` is one under-populated example, of a class that predates M2's validated
target and that two prior external validations passed over, and whose governing prose is
correct. It is recorded against that decision document for the owner's next touch. It does
not block.

The validator began no M4 work, fixed nothing, reopened no closed ticket, and touched neither
the deferred Ticket 027 nor the M3-upgrade fixture-builder ticket.
