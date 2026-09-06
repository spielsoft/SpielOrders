# M4 external validation

Status: **passed. M4 is externally validated at `0183d61a060548d14478845375c100f845819a56`.**

Validation date: 2026-09-06

This is the first external validation of M4. It fixed nothing it audited, opened no
remediation ticket, reopened no closed ticket, began no M5 work, and touched no file under
`crates/`, `contracts/`, `apps/`, `docs/`, or `issues/wayfinder/`. One finding is returned;
the rest are carried.

## Fixed points

| | |
| --- | --- |
| Externally validated M3 target, the pre-M4 baseline | `d0fb5ee0107ab0871bcc0ee41c31d24b94c4df1b` |
| Candidate the fourth acceptance gate measured | `98852fb76573c7f7426fde4565d09d124f6a5adb` |
| Exact validated target | `0183d61a060548d14478845375c100f845819a56` |

`git status --porcelain` was empty and `git rev-parse HEAD` resolved exactly to the target
before validation began. The tree was neither rebased, merged, nor moved; it stayed at the
target until this record was applied.

Toolchains taken as the selectors name them: rustc 1.98.1, cargo 1.98.1 against
`rust-toolchain.toml`'s `channel = "1.98.1"`; Node v26.8.1 and npm 12.0.2 against `.nvmrc`'s
`26.8.1`.

## The gate measured a different SHA, and the proof that this one inherits it

[ACCEPTANCE.md](ACCEPTANCE.md) records its candidate as `98852fb`, two commits behind the
target. **That premise was verified rather than assumed.** `git diff --raw -M -C 98852fb HEAD`
returns exactly six entries, every one a Markdown file under `issues/`, with no mode change,
rename, copy, or gitlink:

```text
:100644 100644 M  issues/implementation/m4-cli-machine-contracts/ACCEPTANCE.md
:100644 100644 M  issues/implementation/m4-cli-machine-contracts/README.md
:100644 100644 M  issues/implementation/m4-cli-machine-contracts/tickets/016-integrate-m4-acceptance.md
:000000 100644 A  issues/implementation/m5-search-and-native-services/README.md
:000000 100644 A  issues/implementation/m5-search-and-native-services/tickets/001-…-quantity.md
:100644 100644 M  issues/roadmaps/spielorders-implementation-roadmap.md
```

`git diff --name-status 98852fb HEAD -- ':!issues/'` is empty and `git diff --check` passes.
No source, test, fixture, manifest, lockfile, script, contract, or toolchain selector is in
the range, so the gate's executable measurement stands at this target. It was reproduced here
anyway.

**The record was nevertheless treated as a claim.** Every criterion below was re-established
by driving the executable as a real process over Packages built for the purpose, not by
reading the record and agreeing.

## What was verified independently

The executable was built at the target and driven as a real process throughout: sixteen
Packages were created from nothing, over a nested `Archive/All Orders` Orders Directory and a
renamed `Retired` Removed Orders Directory as well as the conventional layout, populated with
Vendors, Contacts, Orders and Items, damaged in eleven distinct ways, repaired, and read back.

| Criterion (Ticket 016) | Independent verdict |
| --- | --- |
| Every M4 ticket closed from reviewed work, no blocking finding unresolved | pass, with one qualification — finding V-1 |
| Every command delivered end to end, every deferred one routed and asserted absent | pass — verified from outside, both directions |
| The composition runs over a nested Orders Directory and a renamed Removed one | pass |
| `--json` is one UTF-8 document and no incidental output; human output parses nothing | pass |
| Only explicit whole-Order removal disposes of Supporting Material | pass — by inode as well as by hash |
| READMEs keep absent, readable and unreadable apart | pass |
| A committed mutation whose index refresh failed is never retry-shaped | pass |
| Metadata version 1, the schemas, the fixtures and the scenarios unchanged | pass — zero changed files under three directories |
| Cross-command conformance re-established at the candidate | pass |
| The adapter decides no Package rule | pass, held by review as the record says |
| Currency complete, every blocked in-major update named | pass — re-derived, including the trap |
| The two-axis review ran in its parallel form, every finding with its disposition | pass — the six carried findings were each reproduced |
| This record identifies the closing SHA | pass |

### The command surface, read in both directions from outside

The decisions name 43 command lines: 40 in
[the capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)'s
command surface, plus `adopt-order` from
[legacy adoption](../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md),
plus the two `agent` commands. That count was derived from the decision documents before
`tests/cross_command_conformance.rs` was opened, and it matches `DECIDED_COMMANDS` exactly.

Walking the executable's own generated help tree yields 35 leaves; counting each
`package values <kind>` as its own identity, as the decisions do, gives 38 delivered command
lines. All 38 answer. All five routed lines — the three `search` commands and both `agent`
commands — are usage errors at exit `2`, and under `--json` each emits one
`spielorders.usage` document on stdout with nothing on stderr. **No command line exists that
no decision names, and no decided command is missing.**

### `--json`, on success, on failure, and on a usage error

Nineteen read commands, ten domain failures, and eight usage errors were each parsed with a
strict decoder that rejects trailing bytes. Every one emitted exactly one UTF-8 JSON document
on stdout, zero bytes on stderr, and an `ok` member agreeing with the process exit status in
both directions. Failures carry the command's own contract with `data: null`, a structured
`error` with a stable snake_case code, scope, paths and identities, and `diagnostics`.
`--json` on a failure still writes only to stdout.

Exit statuses mean what the capability decision says. The owner's amendment was confirmed
where it bites: a leftover staging artifact beside a valid canonical destination is a
`staging_artifact_present` **warning**, `order validate` exits `0` despite offering
`promote_staging_candidate`, and bare `repair` exits `0` on an invalid scope because its
diagnosis is an answer rather than a verdict.

`--quiet` is silent on success and keeps the error on stderr. Results go to stdout and
warnings and errors to stderr. No ANSI escape is emitted when redirected, with or without
`NO_COLOR`.

### Supporting Material

An Order Directory was given deliberately hostile Supporting Material: unreadable bytes
(mode `000`), a malformed `order.json`, a decoy `.spielorders/order.json`, invalid UTF-8
content, a dangling symlink, a symlink loop back into the Order, and a nested subtree.
Twenty-four operations then ran over it — validate, repair, index status and rebuild, upgrade
both ways, every read, every Order and Item edit, and every Vendor and Contact mutation.
`validate` reported the Package valid throughout, which nothing that inspected that tree could
have done, and a full snapshot over path, size, sha256, mtime, inode and mode was
**identical** afterwards.

Both relocations carry the tree without enumerating it. `order align` and `order remove` each
left the sentinels byte-identical **and at the same inode**, which is positive evidence the
directory was relocated whole rather than walked and rewritten. `order remove --delete`
submitted the directory to the platform Trash intact, same inode, erasing nothing.

The two checked-in sentinels are byte-identical to the values M3's external validation
recorded:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

### `--dry-run`

All 23 mutating commands accept `--dry-run`; no read command does. Each was run against one
Package that held an adoption candidate, a drifted Order Directory, Items, Contacts and
Supporting Material. A snapshot over every path, size, sha256, mtime, inode and mode was
**identical** afterwards. `init --dry-run` on an empty directory left it empty and reported
`committed: false` with its full eight-operation plan.

### Every repair the integrity decision promises is reachable

Each of the five authorized repairs was reached from the command line, and the exact
`command` each case supplies was executed verbatim:

- **the technical locator** — absent, malformed, and stale locators each diagnose as
  `order_package_locator_mismatch` at error severity with `evidence.reason` distinguishing
  them, offer `repair_package_locator`, and on application recalculate `../../..` under the
  nested layout, returning the Package to valid;
- **a confirmed replacement for parseable metadata** — invalid status, blank title, invalid
  calendar date, unknown field and malformed currency all offer `replace_managed_record`
  requiring `record`, applied successfully;
- **a validated staging candidate** — `promote_staging_candidate`, applied, consuming the
  staging file and restoring the canonical record;
- **an Unresolved Vendor Reference** — exactly the three the decision names,
  `assign_existing_vendor`, `create_and_assign_vendor` and `clear_vendor_reference`, applied;
- **a duplicate Order ID** — `adopt_new_order_identity`, offered on every conflicting side so
  the user designates the survivor.

The refusals hold as firmly as the repairs. Syntactically malformed JSON diagnoses
`managed_artifact_invalid_syntax` and offers **no** action; an unsupported `schema_version`
diagnoses `managed_artifact_wrong_version` and offers none. Neither partially salvages
anything. A missing `order.json` is unactionable, as the decision requires, leaving adoption
to the legacy-adoption contract. `order align` refuses an occupied destination as
`order_directory_occupied` and leaves the source directory and its metadata untouched.

The scope boundary holds in both directions: a damaged locator inside an Order Directory
appears in `order repair` and never in top-level `repair`, and Package-level cases never
appear in `order repair`.

### The index-refresh failure

With the Package's `.spielorders` made read-only, `order edit` wrote the Order record,
reported `ok: true`, `committed: true`, `changed: true`, carried
`warnings: ["index_refresh_failed"]`, exited `0`, and told the user in human form that the
Order was saved and `spielorders index rebuild` replaces the stale index. The edit was on
disk afterwards. Nothing about the response is retry-shaped.

### Fixtures against the live executable

`contracts/cli` holds 141 recorded cells. `contracts/cli/order/show/v1/success.json` was
reproduced from scratch — a Package built by hand to the fixture's facts, the projected
directory name matching character for character — and after normalizing only the Package root
and the two minted UUIDs the live document is **identical** to the recorded one. The `usage`
and `validate/invalid` cells match the live shapes likewise. Every recorded cell carries
`contract_version: 1`.

### Metadata version 1

Since the M3-validated target, `contracts/schemas`, `contracts/examples` and
`contracts/fixtures` have **zero** changed files, and no scenario changed. The only additions
under `contracts/` are the CLI response cells and a `README.md` edit. `metadata_version` is 1
and every schema pins `"const": 1`.

### Currency

The record's claim was re-derived rather than read, and **the first attempt at it was wrong
in the way the repository has been caught before**: `cargo tree` defaults to the workspace's
default members, so `-i generic-array` reports no such package and a naive grep of
`Cargo.lock` conflates versions. Read correctly, the shipped executable's graph
(`-p spielorders-cli -e normal --target all`) carries `toml v1.1.5` and `toml_datetime v1.1.1`
— current majors — and contains neither `generic-array` nor `toml_edit` at all. The held-back
copies `generic-array v0.14.7`, `toml_datetime v0.6.3` and `toml_edit v0.19.15`/`v0.20.2`
appear only once the Tauri member joins the graph. The record's statement is accurate.

### The gate

`scripts/gate.sh` passed at the target: `cargo fmt --check`, clippy over the workspace with
`-D warnings`, `cargo test --workspace --locked` at **910 passed, 0 failed, 0 ignored**,
`npm run test:schemas` validating 4 schemas, 8 accepted examples, 20 rejected fixtures, 22
scenarios and 2 preservation sentinels, and `npm run check:traceability` over 648 relative
links. Its passing is one input to this record and not the record's basis.

## The declared enforcement gaps, judged

The record names four claims no tool holds. Each was checked for whether it is honestly
described and compatible with the milestone's criteria.

**The filesystem lives in one module — honestly described.** Only
`crates/spielorders-cli/src/effects.rs` names `fs::` in the adapter. The gap's own reasoning
for leaving `std::fs` out of `clippy.toml` — that it has no closed surface, and a partial
fence would read as complete — is written into the file itself and is correct. The remaining
`.exists()` calls in the command modules are on core-returned read values, not on `Path`, so
they touch no filesystem. The claim understates nothing.

**`repair --help` states where the scope boundary falls — honestly described, and true
today.** The generated text says a case inside an Order Directory is `order repair`'s and
neither scope reaches into the other. That is exactly the behaviour observed above, in both
directions. The gap is that nothing pins it, which the record says plainly.

**Rollback failure is not reachable through the executable — honestly described, and tested
against.** The claim was attacked rather than accepted: a two-file repair
(`create_and_assign_vendor`, which writes `vendors.json` then `order.json`) was run with the
Order's metadata directory made read-only so the second write could not succeed. The whole
plan was refused before the first write. `vendors.json` and `order.json` were both unchanged,
no Vendor was created, and the failure said so exactly — *"the repair could not be written,
and every managed file holds what it held."* Plan-first validation is real, so a commit-phase
rollback does need an injected effect, and `spielorders-core`'s suite is the right home for
it.

**Generated `--help` and `--version` stay reference text under `--json` — honestly described,
and compatible.** `spielorders --json --help`, `--json --version` and `--json package show
--help` each emit human text rather than JSON. The capability decision does separate the
generated reference from the command responses, and the criterion is written about commands:
all 38 command lines emit exactly one document under `--json`. A machine caller must not
expect JSON from `--help`, which the record states. **This gap is nevertheless the one whose
description would be improved by naming its consequence** — see V-1, which is about the
content of that same generated reference rather than its format.

## Findings

### V-1 — returned — `--uuid` and `--root` are not mutually exclusive in the ordinary position, and a green test says otherwise

[The capability contract](../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
rules that provisional `--uuid` targeting "is mutually exclusive with `--root`".
[Ticket 005](tickets/005-order-reads-and-targeting.md) carries it as an acceptance criterion
in so many words: "`--uuid` with `--root` is a usage error at exit `2`". Every `order`
command's generated help states it as a fact: "Cannot be combined with `--root`."

**The executable enforces it only when `--root` is written after the subcommand.** Written in
the position the top-level help itself teaches — `spielorders [OPTIONS] <COMMAND>`, with
`--root` listed among the top-level options — the conflict is not detected:

```text
spielorders order show --json --uuid <ID> --root .     exit 2, spielorders.usage
spielorders order show --json --root . --uuid <ID>     exit 2, spielorders.usage
spielorders --json --root . order show --uuid <ID>     exit 0, spielorders.order.show, ok: true
```

The cause is a single cross-level conflict declaration in `crates/spielorders-cli/src/cli.rs`:
`--uuid` is `global = true, conflicts_with = "root"` on the `order` subcommand's own argument
struct, while `--root` is `global = true` on the top-level struct. Clap resolves that conflict
only when `--root` lands in the subcommand's own matches. The output-mode exclusions are
declared at one level and are correctly enforced in both positions, which was verified; this
is the only cross-level conflict in the CLI.

Three consequences, in ascending order of concern:

1. **`--root` silently changes meaning.** For `order` commands the decision gives it exactly
   one meaning, the Order Directory. In the unenforced position it is consumed instead as the
   Package to resolve the UUID against. `spielorders --root <PackageRoot> order edit --uuid
   <ID> project=…` **commits**.
2. **A refusal becomes a misleading diagnosis.** `spielorders --root <OrderDirectory> order
   show --uuid <ID>` does not report the two-ways-of-naming-one-Order conflict; it reports
   `package_root_damaged` about a directory the user never named as a Package.
3. **`tests/order_reads_and_targeting.rs::uuid_together_with_root_is_a_usage_error` passes**,
   because it happens to use the position where clap does catch it. The suite therefore
   certifies a property the tree does not have. That is the reason this finding is returned
   rather than carried: the gate could not have caught it, because the gate trusted the suite,
   and this is the one class of defect an external validation exists to find.

**Why it does not fail the milestone.** No data is lost and no machine response asserts a
false fact — the `context` truthfully names the Order that was resolved, and the exit status
and `ok` stay one fact. Every one of Ticket 016's thirteen criteria is met on its own terms.
`--uuid` is marked provisional by the decision that creates it, removable before the contract
is declared stable, with its evaluation deferred to M9. And the rule is enforced in one
documented position rather than absent. The bar the owner reserved — a machine response that
lies, data lost, or a closed decision violated in substance — is approached but not reached.

**Recommended disposition: fix before M5 implementation begins**, and not as a line item
inside [M5 Ticket 001](../m5-search-and-native-services/tickets/001-one-answer-about-a-link-a-count-and-a-quantity.md),
whose subject is a different family. It is plausibly a small change: one conflict declaration
moved to the level `--root` lives at, plus a test that exercises **both** argument positions.
The generated help sentence becomes true rather than aspirational at the same moment, which
matters because M4's own README commits M8's references to citing actual CLI help.

### V-2 — carried — `adopt-order`'s `DIRECTORY` resolves against the working directory, not the Orders Directory

[Legacy adoption](../../wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
says `DIRECTORY` "selects one existing eligible direct child of its configured Orders
Directory", and the generated help says "one existing child of the Orders Directory". The
argument is in fact resolved relative to the process's working directory: from the Package
root, `adopt-order legacy_candidate` is refused with *"nothing stands there"* while
`adopt-order "Archive/All Orders/legacy_candidate"` succeeds, and from inside the Orders
Directory the bare name works.

The eligibility rule the decision cares about **is** enforced, with distinguishable codes —
`order_candidate_not_a_direct_child` for a grandchild, an escape through `..`, the Package
root and a directory outside the Orders Directory; `order_candidate_ineligible` for a file;
`order_candidate_link` for a symlink. So the decision's substance holds and only its base is
surprising. Read strictly, "selects one … direct child" describes which directory is eligible
rather than what a relative path is measured from, so this is a documentation and ergonomics
gap rather than a contract violation. It is worth settling because the archive this command
exists for is adopted by an agent invoking it repeatedly with `--root` and a basename, which
is the form that does not work.

**Recommended disposition:** carry. Either resolve `DIRECTORY` against the Orders Directory or
say plainly in the help and in the decision that it is a path measured from the working
directory.

### V-3 — carried — `spielorders.repair` reports `valid: true` for a Package that `validate` reports invalid

On one Package holding one Damaged Order, `validate` returns `ok: false`, exit `1`, and
`package_invalid`; `repair` on the same path returns `scope: "package"`, `valid: true`,
`cases: []`, `unactionable: []`, exit `0`. The human form prints `Package Root: …` above
`Valid: yes`.

This is internally coherent and matches the decision, which scopes bare top-level `repair` to
"actionable **Package-level** repair cases" and rules that neither scope reaches into the
other's. `order repair` on the damaged directory correctly returns `valid: false` with the
finding in `unactionable`. Nothing is hidden and no finding is lost. But `valid` under
`scope: "package"` reads to a machine consumer as a statement about the Package, and the
command whose job is that verdict disagrees. The two are answering different questions with
the same word.

**Recommended disposition:** carry, as a naming question for the response contract while it is
still version 1. Either the field says whose validity it reports, or `repair` reports the
Package verdict alongside its own scope.

### The six findings the gate carried forward were each reproduced, and the gate's calibration holds

Not relayed from the record — each was provoked at this target and judged against the same
bar. All six are real and all six are correctly below it.

- **A link named as a root.** Reproduced. `validate` warns
  `orders_directory_entry_disallowed` — *"it is ignored and nothing is moved or removed"* —
  while `order show` and `order remove` resolve the link and act on the directory behind it.
  **The dangerous case is closed**: a link pointing outside the Package is refused
  `order_locator_stale`, so nothing foreign can be pulled into the Package, and the `context`
  truthfully reports the resolved real path rather than the link. Inconsistent, not unsafe.
- **A count naming one directory too many.** Reproduced: two conflicting directories are
  described as "carried by 3 Order Directories", three as 4. **The wrong number never reaches
  the machine document** — `evidence.conflict_locations` and `related_locations` each list
  exactly the real directories, and no count appears anywhere in the JSON. It is a human
  sentence, which the decision says is never a parsing contract.
- **A Quantity rendered as money.** Reproduced: a stored quantity of `0.001` prints as `0.00`
  and `1.125` as `1.12`, beside a correct total. The machine document carries `0.001` and
  `1.125`. Human-only.
- **One wrong-kind root, two stable codes.** Reproduced, and **this is the strongest of the
  six**: a file named as an `order` root is refused `package_unreadable` by six commands and
  `order_root_unmanaged` by `order validate` and `order repair`, and the first message names
  the internal port operation `observe_kind` and a path under a file that cannot exist. A
  consumer switching on `error.code` gets two answers for one condition. Each code is
  nonetheless stable and deterministic per command, and the response is truthful that the
  command failed. Below the bar, but the item in M5 Ticket 001 worth doing first.
- **A total of negative zero.** Reproduced: `"order_total": -0.0` for an Order with no Items.
  `-0.0 == 0.0` in every JSON consumer, and the human surface prints `0.00`. Ugly, not false.
- **The Vendor-selector question.** Reproduced: an Order's Vendor is assignable only as
  `vendor_id=<UUID>`; `vendor`, `vendor_name` and `vendor_reference` are all rejected
  `unknown_field`. Correctly held for the owner rather than changed.

## Result

**M4 passes external validation** at `0183d61a060548d14478845375c100f845819a56`.

The milestone's claim is a command surface and a machine contract, and both were checked from
outside the code that makes them. The surface is exactly the surface the decisions name, with
nothing extra and nothing missing, established by walking the executable rather than by
reading the inventory that asserts it. Every command answers under `--json` with one document
on success, on failure and on a usage error, and `ok` and the exit status never disagree.
Every repair the integrity decision authorizes is reachable and applies; every refusal it
requires refuses. Supporting Material survived twenty-four operations and two whole-directory
relocations byte-identical and at the same inode, and the checked-in sentinels are unchanged
across four milestones. Every mutating command's `--dry-run` left the Package byte-identical.
A committed mutation whose index refresh failed stayed committed and pointed at
`index rebuild`. Metadata version 1 and its schemas, examples, fixtures and scenarios have not
moved since M3's validated target.

The gate's own record proved honest everywhere it was checked, including the four gaps it
declares, one of which was actively attacked rather than accepted. Its calibration of the six
findings it carried rather than repaired is correct: each is real, each was reproduced here,
and none makes a machine response lie or loses data.

What this validation adds that the gate could not is V-1. A rule stated by a closed decision,
by a closed ticket's acceptance criterion, and by the executable's own generated help is
enforced in one argument position and not in the position the help teaches — and the test that
covers it passes because it uses the position that works. It does not fail the milestone: no
data is lost, no machine response lies, the option is provisional by decision, and all
thirteen of the gate's criteria are met. It is returned rather than carried because a passing
test that certifies an absent property is the one thing a suite cannot audit about itself, and
because the sentence it makes false is one M8 is committed to citing.
