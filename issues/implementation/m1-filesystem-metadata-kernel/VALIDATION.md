# M1 external validation

Status: **passed; externally validated**

Validation date: 2026-07-26

## Reviewed range and repository guardrails

- Pre-M1 fixed point: `c8c1250fd7803fa9b1b80967ce30031cf03ebd6f`
- Completed remediated M1 target:
  `7f4841c8f7bca9c3767795c0939b040cbf13a173`
- Exact reviewed range:
  `c8c1250fd7803fa9b1b80967ce30031cf03ebd6f..7f4841c8f7bca9c3767795c0939b040cbf13a173`

Before validation, `HEAD` and `main` both resolved to the completed target, the
source worktree was clean, and the fixed point was both an ancestor of the target
and their exact merge base. There were zero commits and zero changed paths from the
target to `HEAD`. The reviewed range contains 34 commits and a nonempty 37-file
diff. No later implementation change required reconsidering the range.

The project-local `code-review` skill was read and used. Its mandated comparison,
`git diff c8c1250f...7f4841c`, is identical to the exact two-dot change range
because the fixed point is the merge base. Standards and Spec reviews ran
independently and in parallel. The `implement`, `tdd`, and `to-tickets` skills were
not used during validation.

## Environment and isolated target copy

Validation ran on:

- macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon;
- Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), and LLVM 22.1.6;
- Node.js 24.18.0 and npm 11.18.0; and
- Git 2.50.1 (Apple Git-155).

The login shell initially selected an older locally installed Node 24.15.0/npm
11.16.0 pair. Validation located and explicitly selected the installed remediated
Node 24.18.0/npm 11.18.0 toolchain before running any executable gate used as
evidence.

A local no-hardlink clone was created at
`/private/tmp/spielorders-m1-final-validation.vZI0g2`, detached at the exact target.
It was source-clean and contained no `target`, `node_modules`, or `dist` directory
before dependency installation. Both npm dependency trees were recreated offline
from their lockfiles with zero vulnerabilities. The clone remained source-clean
after every gate.

## Commands and results

Every acceptance command recorded in
[M1 acceptance evidence](ACCEPTANCE.md) ran in the isolated target copy.

| Command | Result |
| --- | --- |
| `git status --porcelain=v1 --untracked-files=all` | Passed in source and isolated copy |
| `git merge-base --is-ancestor <fixed> <target>` | Passed; merge base equals fixed point |
| `git log <target>..HEAD` and `git diff <target>..HEAD` | Empty |
| `git diff --check <fixed>..<target>` | Passed |
| `npm clean-install --offline` (root) | Passed: 5 packages; zero vulnerabilities |
| `npm clean-install --offline` (`apps/desktop`) | Passed: 29 packages; zero vulnerabilities |
| `cargo build --workspace --locked` | Passed from an empty `target` |
| `cargo test --workspace --locked` | Passed: 62 Rust tests plus doc tests |
| `cargo test -p spielorders-core --locked` | Passed independently: the same 62 core tests plus doc tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 19 rejected fixtures, 22 scenarios, 2 sentinels |
| `npm run check:traceability` | Passed its configured scope: 7 M0 tickets and 4 baseline test files |
| `npm run test` (`apps/desktop`) | Passed: 2 frontend boundary tests |
| `npm run build` (`apps/desktop`) | Passed: TypeScript and Vite production build |
| `npm run tauri build -- --no-bundle` (`apps/desktop`) | Passed: release desktop shell build |
| `cargo update --dry-run --verbose` | Passed after refreshing crates.io: zero compatible lockfile updates |
| `cargo tree --workspace --depth 1` | Passed; dependency direction recorded below |
| Authoritative selected-major release queries | Passed; dependency evidence recorded below |
| `cargo fmt --all -- --check` | Same four pre-M1 trailing-blank-line differences; nonblocking observation |

The format differences are confined to
`apps/desktop/src-tauri/build.rs`, both Tauri Rust entry points, and
`crates/spielorders-cli/src/main.rs`. None changed in the M1 range. Formatting is
not a recorded repository gate, and the M1 core package is formatted. This remains
a pre-existing observation, not an M1 finding or waiver.

## Independently recalculated evidence

Independent filesystem, manifest, and test-source counts found:

- 4 version-1 schema documents;
- 8 accepted JSON examples;
- 19 rejected-manifest entries naming 19 unique existing files;
- 22 scenarios with 22 unique names;
- 2 opaque binary sentinels; and
- 62 Rust tests: 18 metadata codecs, 8 Order validation, 8 Package/Vendor
  validation, 12 safe paths, 8 single-file replacement, 6 multi-file replacement,
  and 2 core boundary/unit tests.

The JavaScript Draft 2020-12 suite and Rust public codecs both accept every
accepted example and reject every manifest-listed invalid fixture, including both
M1-V3 Windows index-path fixtures. The Rust suite additionally verifies malformed
encoding and BOM input, explicit optional `null`, unknown fields, wrong versions,
constructed non-finite numbers, UUIDv4 text, currency and status shapes, and exact
parent-only locator cardinality.

Exact minimal/full round trips prove deterministic two-space JSON, stable
declaration/field order, omitted absent optionals, present required collections,
and exactly one trailing newline. JSON object order remains non-semantic for input.

All 22 canonical scenarios remain accounted for by the acceptance matrix: 14 have
M1 public-seam evidence and 8 are explicitly and correctly deferred to their
owning later milestones. No deferred behavior was silently reinterpreted as M1.

The sentinel hashes matched between the completed source target and the isolated
copy after every gate:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

## Ticket-by-ticket and roadmap validation

| Ticket | Independent disposition |
| --- | --- |
| 001 — metadata-version-1 vocabulary | Passed. All five document codecs, accepted/rejected agreement, strict bytes, optionals, collections, versions, UUIDs, currency, locator cardinality, and M1-V3 Windows index-path rejection reproduce. |
| 002 — Order validation and totals | Passed. Real Gregorian dates, all statuses, future dates, signed finite values, zero Items, normalization, and exact unrounded Item and Order totals reproduce. M1 validates UUIDv4 text; generation remains a later mutation concern. |
| 003 — Package/Vendor semantics | Passed. Duplicate participants, trimmed full-Unicode Vendor-name uniqueness, live/absent/unresolved Vendor References, Vendor/Contact normalization, index-local semantics, and absent/reused/new Reusable Free-Entry Values reproduce. No Vendor is inferred, created, or merged. |
| 004 — safe Package paths | Passed. Defaults/configuration, root confinement, nested paths, overlap, symlink escape, no-follow classification, hidden/direct-child behavior, reserved paths, locators, platform forms, and M1-V2 non-normalized reserved-path cases reproduce. |
| 005 — validated atomic replacement | Passed. Real beside-destination replacement and every injected staging-write, validation-read, candidate-validation, durability, replacement, and cleanup failure preserve prior valid state as specified. Abandoned evidence and unrelated content remain untouched. |
| 006 — multi-file rollback | Passed. Full-plan candidate validation, exact and M1-V1 aliased duplicate rejection, destination-identity errors, caller order, retained prior bytes, reverse rollback, and explicit multiple rollback failures reproduce. |
| 007 — integrated M1 gate | Passed. All executable gates, composition evidence, dependency/scope audits, and independent counts pass on the complete remediated target. |

Every roadmap deliverable is present through the intended public core seams. The
acceptance gate passes without waiver.

## Required behavior audits

### Domain and metadata behavior

Public tests and source inspection verify:

- lowercase hyphenated UUIDv4 identity validation;
- real Gregorian Date Ordered values, every status, and no status-dependent
  plausibility/completeness rule;
- uppercase three-letter currency decoration without numeric conversion;
- finite signed Quantity and Unit Price values, exact unrounded Item Total Price,
  and exact unrounded Order Total;
- single-line and multiline normalization;
- duplicate Order/Vendor identity participants and normalized Unicode Vendor Name
  uniqueness;
- absent and live Vendor References, unresolved-reference rejection, and lookup of
  current Vendor facts by Vendor ID; and
- absent, reused-canonical-spelling, and deliberately new Reusable Free-Entry
  Values.

### Filesystem boundary, rollback, and preservation

The safe-path suites use both a filesystem double and real temporary-tree symlinks.
They prove configured topology cannot traverse, overlap, resolve through links
outside the root, occupy reserved Package metadata, or accept Windows absolute and
drive-relative forms. Direct-child discovery is one level, ignores hidden entries,
reports visible non-directories, never follows links, and never examines Supporting
Material.

M1-V2's root-confined lexical normalization correctly distinguishes normalized
Package metadata, immediate Order metadata, ordinary root content, above-root
traversal, and nested Supporting Material. Order locators must resolve from one
ordinary active direct child to the explicit root.

Single-file replacement stages beside the destination, rereads and validates the
candidate, makes it durable, and atomically replaces it. Every recorded failure
preserves prior valid destination bytes. Multi-file replacement validates its whole
plan before writing, canonicalizes destination identity, rejects M1-V1 lexical and
symlink aliases, retains all prior bytes, rolls back in reverse order, and reports
each rollback failure separately. Neither implementation performs a recursive
Supporting Material operation.

### Boundaries, scope, and traceability

`cargo tree --workspace --depth 1` reports:

```text
spielorders-core -> focaccia, serde, serde_json, toml
spielorders-cli -> spielorders-core
spielorders-desktop -> spielorders-core, tauri
```

Core has no dependency on CLI, Tauri, React, TypeScript, Vite, or any frontend
module. The only frontend Tauri import remains in the adapter. Source/diff
inspection found no Package loader, damage projection, stable diagnostic catalog,
README/index loading or rebuild workflow, domain mutation/persistence workflow,
M2/M3 behavior, or shared SpielGantt crate. SpielGantt appears in core only in
decision-source comments.

All seven M1 tickets are closed and contain Outcome, named Decision sources,
Blocking, Scope, Acceptance criteria, Review boundary, exact ticket-start fixed
point, and Resolution sections. All six M1 integration suites contain named
decision-source markers. The configured traceability script still enforces only
its M0 scope, so M1 traceability was inspected independently rather than inferred
from that command's pass.

## M1-V1 through M1-V4 remediation verification

- **M1-V1:** Passed. Canonical destination identity is resolved before the first
  write; aliased operations identify both indices and leave prior destination,
  staging evidence, and unrelated content unchanged.
- **M1-V2:** Passed. Reserved-path classification uses root-confined lexical
  normalization and covers both original false-positive and false-negative cases
  without filesystem access or recursive inspection.
- **M1-V3:** Passed. Schema and Rust reject `C:/outside` and `C:outside` index
  paths through two manifest-listed one-purpose fixtures while retaining accepted
  normalized relative paths.
- **M1-V4:** Passed. The complete gate ran under current selected-major Node
  24.18.0/npm 11.18.0; postcss resolves to current 8.5.23; Cargo and direct registry
  audits find no compatible selected-major update.

## Code-review outcome

The two required axes are reported separately.

### Standards

**Hard violations: 0.** The review found no documented-standard breach. The diff
preserves the core/adapter boundary, named decision-source convention, canonical
fixture use, domain vocabulary, and in-major dependency/toolchain policy.

**Judgment calls: 2.**

1. Possible Primitive Obsession: normalized optional domain facts remain
   `Option<String>`, so `order.rs` and `package.rs` remember which fields use
   single-line versus multiline normalization. Small value types could own these
   invariants.
2. Possible Duplicated Code: `PackageRoot`, `DateOrdered`, `Currency`, and
   `IndexOrderDirectory` repeat accessor/serialization plumbing. A shared pattern
   could remove it, though explicit implementations may be clearer.

Worst Standards issue: possible Primitive Obsession; no blocking documented
standard violation.

### Spec

**Reviewer findings: 3.**

1. High: M1-V3 appeared to tighten version-1 validation without advancing the
   metadata version.
2. Medium: the prior failed validation record's ready prompt still named the
   pre-remediation target and therefore could not perform the requested fresh run.
3. Medium: the repository documents Node/npm majors rather than an exact patch
   pin, and the login shell initially selected older installed patches.

Worst Spec issue as reported: the apparent M1-V3 version-evolution conflict.

Code-review summary: Standards has 0 hard findings and 2 judgment calls, worst
possible Primitive Obsession; Spec reported 3 findings, worst High.

### Independent disposition of Spec findings

The validator checked each reviewer finding against the decision hierarchy and
actual clean-target evidence rather than accepting it uncritically:

1. **M1-V3 is not a versioned-contract change.** The governing version-1 decision
   already states that `index.order_directory` is a normalized relative path that
   cannot escape the SpielOrders Root. Windows absolute and drive-relative paths
   were therefore never contract-valid. M1-V3 corrects schema/runtime expressions
   to the unchanged normative meaning; it does not change a managed field,
   representation, requiredness, accepted enumeration, validation meaning, or
   index projection. The accepted M0 schema-expression correction establishes the
   same pre-release correction precedent without a version advance. No conforming
   version-1 document becomes invalid. Disposition: not a blocker.
2. **The stale prompt was historical record drift.** It was part of the failed
   pre-remediation record and did not alter product behavior. This validation was
   explicitly corrected before it began, reviewed the complete target and all
   remediation commits, and replaces that obsolete prompt below. Disposition:
   resolved by this validation record; not a blocker.
3. **An exact patch pin is not required.** `README.md` deliberately selects Node
   24/npm 11 majors, while `AGENTS.md` requires agents to use their newest available
   minor/patch releases. The required versions were installed locally, explicitly
   selected, and used for the complete clean-copy gate. Reproducibility of the
   selected majors remains documented, and no repository rule requires an exact
   Node/npm patch pin. Disposition: clean-target execution satisfies the policy;
   not a blocker.

After disposition, the Spec review has zero blocking missing, partial, incorrect,
or scope-creep findings.

## Dependency and toolchain currency

Authoritative checks on 2026-07-26 establish:

- Rust's current release is 1.97.1, matching `rust-toolchain.toml`
  (<https://blog.rust-lang.org/releases/latest/>).
- Node's official release index identifies 24.18.0 as newest within selected major
  24, and the complete gate used it (<https://nodejs.org/dist/index.json>).
- npm registry history identifies 11.18.0 as newest within selected major 11, and
  the complete gate used it. npm 12.0.1 is a new major and was not adopted
  (<https://registry.npmjs.org/npm>).
- `cargo update --dry-run --verbose` refreshed crates.io and locked zero compatible
  updates. Direct crates.io metadata matches `focaccia` 2.0.0, `serde` 1.0.229,
  `serde_json` 1.0.151, `toml` 0.9.12, `tauri` 2.11.5, and `tauri-build` 2.6.3.
  `toml` 1.1.3 is a new major and was not adopted.
- Direct npm registry `latest` metadata matches AJV 8.20.0, Tauri API 2.11.1,
  Tauri CLI 2.11.4, Node types 26.1.1, React/ReactDOM 19.2.8, React types
  19.2.17, ReactDOM types 19.2.3, Vite React plugin 6.0.4, Vite 8.1.5, and
  postcss 8.5.23. TypeScript 5.9.3 is newest within selected major 5.
- TypeScript 7.0.2, npm 12.0.1, Node 26, and `toml` 1.1.3 are unapproved new
  majors and were correctly not adopted.

Dependency and toolchain currency is conclusively verified under `AGENTS.md`.

## Outcome

M1 is **externally validated** at completed target
`7f4841c8f7bca9c3767795c0939b040cbf13a173`. Every ticket resolution, roadmap
deliverable, and acceptance-gate requirement passes on the complete remediated
range. There are no waivers and no blocking findings.

M2 planning may begin. M2 implementation has not begun.

## Next-work pointer

The historical next action was planning-only decomposition of M2 with the
project-local `to-tickets` skill. The implementation roadmap and M2 milestone
index became the canonical instructions; no generated prompt or copy of their
requirements was needed. M2 has since been implemented and externally validated.
