---
title: Modernize Rust workspace resolution and direct dependency baselines
status: closed
assignee: /root
claimed_at: 2026-07-30T17:09:31Z
closed_at: 2026-07-30T17:15:50Z
ticket_start_fixed_point: 41afb7a5f04efa09fcae17b4d9c4f505bdd1d8c9
reopened_at: 2026-07-30T17:01:39Z
reopened_by_gate: 004-m1-upgrade-acceptance-gate
parent: M1-upgrade — Toolchain and dependency modernization
labels:
  - implementation:m1-upgrade
  - implementation:task
---

## Outcome

Build the externally validated M1 kernel with Edition 2024's current resolver and
the approved current Rust dependency generations, including TOML major 1, without
changing any metadata, domain, filesystem, or adapter behavior.

## Decision sources

- [M1-upgrade — Toolchain and dependency modernization](../README.md)
- [M1 external validation](../../m1-filesystem-metadata-kernel/VALIDATION.md)
- [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](../../m0-contract-baseline/tickets/001-workspace-skeleton.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

Unblocked. Start from a clean worktree at or after the M1 external-validation
documentation commit. Record the exact ticket-start fixed point before editing.

Reopened by Ticket 004 on 2026-07-27. The target-pinned acceptance gate found
compatible locked updates for `schemars`, `toml_parser`, and `tray-icon`. This
ticket again owns lock-graph regeneration and the complete Rust-focused
reverification. Its prior Resolution remains the historical record of the first
closure; a new claim must record a new ticket-start fixed point and append the
remediation Resolution.

Reopened again by Ticket 004 on 2026-07-30. A fresh target-pinned
`cargo update --dry-run --verbose` found the compatible locked update
`http` 1.4.2 to 1.5.0. Ticket 004 made no dependency or lockfile repair. This
ticket remains the smallest owner of Cargo lock-graph regeneration and its
complete Rust-focused reverification. A new claim must record a new ticket-start
fixed point and append another remediation Resolution.

## Scope

- Select Cargo resolver 3 for the Edition 2024 virtual workspace.
- Move the core's direct TOML dependency from major 0 to the newest stable release
  in approved major 1 and make only the compatibility changes needed to preserve
  the existing public configuration contract.
- Update direct Rust and Tauri manifest baselines to the current releases actually
  validated in their already selected majors, including `focaccia`, `serde`,
  `serde_json`, `tauri`, and `tauri-build`.
- Retain the newest stable Rust/Cargo toolchain; update a newly available compatible
  stable point release automatically under `AGENTS.md`.
- Regenerate the lock graph without overrides, forced downgrades, or speculative
  dependency additions.

Exclude metadata-version changes, schema or fixture changes, Package behavior,
domain-rule changes, new capabilities, adapter work, M2/M3 behavior, and any shared
crate.

## Acceptance criteria

- A clean locked workspace build, workspace test run, independent core test run, and
  Clippy run with warnings denied all pass.
- The complete existing configuration parsing and deterministic serialization
  behavior passes unchanged through public core tests and canonical fixtures.
- Resolver inspection confirms version 3 governs the virtual workspace.
- The direct dependency graph contains the approved current releases, and an
  authoritative currentness check finds no newer compatible direct release.
- Cargo's update dry run reports no uncommitted compatible update after the lockfile
  is regenerated.
- Both Supporting Material sentinel hashes remain identical to the externally
  validated M1 record.
- No schema, fixture, Node package, frontend source, product behavior, or M2 file is
  changed.
- The required fixed-point Standards/Spec code review is complete and every finding
  is resolved or explicitly dispositioned.

## Review boundary

Review resolver semantics, Rust dependency selection, TOML compatibility, lockfile
reproducibility, and preservation of existing public behavior. Do not review or
implement unrelated cleanup, new product behavior, or later upgrade tickets.

## Resolution

Ticket 001 started from the clean fixed point
`b982a8e72b95833fd1545ce2fd874ed287d12dca` on `main`. The ticket was open,
unblocked, and unclaimed before `/root` claimed it. No TDD RED test was created:
the authorized changes were dependency and resolver declarations, and the TOML
major migration exposed no public compatibility defect.

### Environment and authoritative currentness

Validation ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), LLVM 22.1.6, and Git
2.50.1 (Apple Git-155).

Authoritative checks on 2026-07-26 established:

- the Rust Release Team's current-release endpoint
  (<https://blog.rust-lang.org/releases/latest/>) resolved to Rust 1.97.1;
- Cargo's resolver reference
  (<https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions>)
  identifies resolver 3 as Edition 2024's resolver, and its workspace reference
  (<https://doc.rust-lang.org/cargo/reference/workspaces.html#virtual-workspace>)
  requires an explicit resolver in a virtual workspace; and
- live `cargo search <crate> --limit 1` queries against the authoritative crates.io
  registry reported TOML `1.1.3+spec-1.1.0`, focaccia 2.0.0, serde 1.0.229,
  serde_json 1.0.151, Tauri 2.11.5, and tauri-build 2.6.3.

These exactly match the newest releases in the approved generations. No additional
new major was considered or adopted.

### Dependency and lockfile outcome

Commit `d7d79da823b2201a860a8c7a28a7704d505274af` selects resolver 3 and records
full direct manifest baselines for the releases above. The existing Rust 1.97.1
toolchain and workspace `rust-version` were already current and remained unchanged.
TOML 1.x compiled through the existing public `Config::from_toml_bytes` seam with
no source compatibility change.

`cargo update --verbose` regenerated the ordinary graph with zero package updates.
The lockfile's only changed edge redirects `spielorders-core` from TOML
`0.9.12+spec-1.1.0` to the already resolved TOML `1.1.3+spec-1.1.0` package.
There are no `[patch]` or `[replace]` sections, forced versions, downgrades, or new
direct/speculative dependencies. The dry run's older available transitive notices
are selected behind current direct packages: `generic-array` through Tauri's
`tauri-codegen`/SHA-2 chain, and TOML 0.8-era packages through Tauri's
all-target GTK `system-deps`/proc-macro chains. Cargo found no compatible lockfile
update for them, so no transitive override was added.

Implementation commits:

- `2e3e8bd49853335d2bcb3dd9c99a797d37d77c7d` — claim Ticket 001 and pin its
  fixed point;
- `d7d79da823b2201a860a8c7a28a7704d505274af` — modernize the resolver,
  direct baselines, and lock edge.

### Commands and results

- Preflight `git status --porcelain=v1 --untracked-files=all`,
  `git rev-parse HEAD`, ticket-frontmatter inspection, history, and merge-base
  checks passed at the exact fixed point.
- `cargo test -p spielorders-core --test metadata_v1 --locked` passed all 18
  focused configuration, canonical-fixture, and deterministic serialization tests.
- `cargo clean` followed by `cargo build --workspace --locked` passed from an empty
  10.0 GiB workspace target.
- `cargo test --workspace --locked` passed 62 Rust tests plus doc tests.
- `cargo test -p spielorders-core --locked` independently passed the same 62 core
  tests plus doc tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- `npm run test:schemas` passed 4 schemas, 8 accepted examples, 19 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` passed after the review disposition and milestone
  update.
- `cargo tree -p spielorders-core --depth 1 --locked` reported focaccia 2.0.0,
  serde 1.0.229, serde_json 1.0.151, and TOML 1.1.3; the corresponding desktop
  inspection reported Tauri 2.11.5 and tauri-build 2.6.3.
- `cargo metadata --locked --format-version 1 --no-deps` confirmed the direct
  caret requirements, Edition 2024, Rust 1.97.1, and the unchanged workspace
  boundary.
- Root-manifest inspection and Cargo's authoritative virtual-workspace semantics
  confirm `resolver = "3"` governs the workspace.
- `cargo update --dry-run --verbose` refreshed crates.io and locked zero compatible
  updates; the committed lockfile was unchanged.
- `git diff --check` passed. The fixed-point diff contains only the root Cargo
  manifest, the core and desktop Rust manifests, `Cargo.lock`, this ticket, and the
  milestone index. It contains no schema, fixture, Node package, frontend, product,
  adapter-source, M2/M3, or shared SpielGantt change.

The working-tree and fixed-point SHA-256 hashes are identical:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

### Fixed-point review and disposition

The project-local code-review workflow compared
`git diff b982a8e72b95833fd1545ce2fd874ed287d12dca...HEAD` after the
implementation commits. Standards and Spec reviewers ran independently and in
parallel.

- **Standards:** one hard documentation-consistency finding, zero smell judgment
  calls. The claim/status was not yet reflected in the milestone index, contrary
  to `docs/development/traceability.md`. Disposition: resolved by the closing
  milestone progress/table update committed with this Resolution.
- **Spec:** zero missing or partial requirements, zero scope creep, and zero
  incorrect implementations. No disposition was required.

All acceptance criteria pass. Metadata-version-1 meaning, configuration, schemas,
canonical fixtures, domain/filesystem/serialization behavior, adapters, opaque
Supporting Material, Node/frontend packages, M2/M3 scope, and shared SpielGantt
foundations remain unchanged.

## Remediation Resolution

The reopened ticket started from the exact clean fixed point
`aa1d354a4f768eee8545f3db7d21bd650b1104ff` on `main`. Ticket 001 was open,
unblocked, and unclaimed, and Ticket 004 was open and blocked only by Ticket 001
before `/root` claimed this remediation. Commit
`acb09fdbef8a343e5e042b26c304fca6cc5a4e34` records that claim and fixed point.
No synthetic TDD RED test was added because the authorized remediation changes
dependency declarations and resolution only; the existing public configuration
and metadata tests are the behavioral compatibility seam.

### Environment and authoritative currentness

Validation ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), LLVM 22.1.6, Git
2.50.1 (Apple Git-155), Node 26.5.0, and npm 12.0.1.

Authoritative checks on 2026-07-29 established:

- the Rust Release Team's current-release endpoint
  (<https://blog.rust-lang.org/releases/latest/>) still resolved to Rust 1.97.1;
- live `cargo search <crate> --limit 1` queries against crates.io reported
  focaccia 2.0.0, serde 1.0.229, serde_json 1.0.151, TOML
  `1.1.4+spec-1.1.0`, Tauri 2.11.5, and tauri-build 2.6.3 as the current direct
  releases in their approved generations; and
- the same crates.io queries confirmed the gate-reported schemars 1.2.2,
  toml_parser `1.1.3+spec-1.1.0`, and tray-icon 0.24.2 releases, plus the newly
  available compatible camino 1.2.5, displaydoc 0.2.7, and tao-macros 0.1.4
  releases found by the fresh lock-graph audit.

TOML 1.1.4 is a newer release in the already approved major 1, so `AGENTS.md`
required adopting it automatically. No unapproved major, Rust toolchain, Node,
npm, or TypeScript selection was changed.

### Dependency and lockfile outcome

Commit `f9e0a9200f90b91bc735311dda4d9ac3714368d5` raises the direct core TOML
baseline from 1.1.3 to 1.1.4 and regenerates the ordinary Cargo graph. Cargo
selected seven Rust-1.97.1-compatible updates:

- camino 1.2.4 to 1.2.5;
- displaydoc 0.2.6 to 0.2.7;
- schemars 1.2.1 to 1.2.2;
- tao-macros 0.1.3 to 0.1.4;
- TOML `1.1.3+spec-1.1.0` to `1.1.4+spec-1.1.0`;
- toml_parser `1.1.2+spec-1.1.0` to `1.1.3+spec-1.1.0`; and
- tray-icon 0.24.1 to 0.24.2.

There are no `[patch]` or `[replace]` sections, overrides, forced versions,
downgrades, or speculative dependencies. Selector inspection traces camino through
Tauri's cargo_metadata chain, displaydoc through the ICU/URL chains, TOML and
toml_parser through the direct core and Tauri utility/build chains, tao-macros
through Tao/Wry, schemars through serde_with's locked optional graph, and tray-icon
through Tauri's locked optional graph.

The final `cargo update --dry-run --verbose` locked zero packages and left the
lockfile SHA-256 unchanged at
`b46f20711c8c81d150ea6faee159eb0e8e304b418ccfd77ad0fde424b2e2c314`.
Its behind-latest notices remain non-updatable transitives selected by current
direct packages: generic-array 0.14.7 through the Tauri SHA-2 chain, and TOML
0.8-era packages through Tauri's all-target GTK system-deps/proc-macro chains. No
transitive override was added.

### Commands and results

- Preflight worktree, exact-HEAD, ticket-state, history, fixed-point, and
  `git diff --check` checks passed.
- `cargo test -p spielorders-core --test metadata_v1 --locked` passed all 18
  focused public configuration, canonical-fixture, and deterministic
  serialization tests.
- `cargo clean` removed 4.2 GiB of prior artifacts, after which
  `cargo build --workspace --locked` passed from an empty target.
- `cargo test --workspace --locked` passed 62 Rust tests plus doc tests.
- `cargo test -p spielorders-core --locked` independently passed the same 62
  core tests plus doc tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- `npm run test:schemas`, executed with the repository-selected Node 26.5.0 and
  npm 12.0.1, passed 4 schemas, 8 accepted examples, 19 rejected fixtures, 22
  scenarios, and 2 preservation sentinels. The shell's first Node 24 invocation
  was correctly rejected by `devEngines` before tests ran; selecting the required
  installed runtime required no repository change.
- `npm run check:traceability` passed under the same selected Node/npm runtime.
- Direct core and desktop `cargo tree --depth 1 --locked` inspections reported
  the six current direct releases above. Selector-level inverted trees,
  `cargo metadata --locked --format-version 1 --no-deps`, and manifest inspection
  confirmed the dependency paths, Edition 2024, Rust 1.97.1, resolver 3, and the
  unchanged core/adapter workspace boundary.
- `cargo update --dry-run --verbose` refreshed crates.io, locked zero compatible
  updates, and did not change `Cargo.lock`.
- Fixed-point name/status, prohibited-path, manifest-override, and
  `git diff --check` audits found no schema, fixture, Node package, frontend,
  product source/test, adapter source, metadata-version-1, M2/M3, or shared
  SpielGantt change.

The working-tree and fixed-point SHA-256 sentinel hashes are identical:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

### Fixed-point review and disposition

The project-local code-review workflow compared
`git diff aa1d354a4f768eee8545f3db7d21bd650b1104ff...HEAD` after the claim and
implementation commits. Standards and Spec reviewers ran independently and in
parallel.

- **Standards:** one hard documentation-consistency finding and zero smell
  judgment calls. The claim was not yet reflected in the milestone index as
  required by `docs/development/traceability.md`. Disposition: resolved by the
  closing milestone progress, table, and handoff update committed with this
  Resolution; traceability is rerun afterward.
- **Spec:** zero missing or partial requirements, zero scope creep, and zero
  incorrect implementations. The reviewer explicitly confirmed that the three
  additional compatible transitive refreshes belong to ordinary lock-graph
  regeneration. No further disposition was required.

All reopened Ticket 001 acceptance criteria pass. Metadata-version-1 meaning,
configuration behavior, schemas, fixtures, deterministic serialization,
domain/filesystem behavior, adapters, opaque Supporting Material, Node/npm and
TypeScript selections, product behavior, M2/M3 scope, and shared SpielGantt
foundations remain unchanged.

## Second Remediation Resolution

The second reopened remediation started from the exact clean fixed point
`41afb7a5f04efa09fcae17b4d9c4f505bdd1d8c9` on `main`. Modernize Rust
workspace resolution and direct dependency baselines and
[Move repository tooling to Node 26 and npm 12](002-node-npm-modernization.md)
were open, unblocked, and unclaimed;
[Adopt the TypeScript 7 CLI without requiring its future compiler API](003-typescript-7-cli.md)
was closed; and
[Integrate and verify the M1-upgrade acceptance gate](004-m1-upgrade-acceptance-gate.md)
was open and blocked by the Rust/Cargo and Node/npm tickets before `/root` claimed
this ticket. Commit `f4bc21e7f0e1ad0307a495ee7b11dee08eb2324f` records only
that claim and fixed point. No synthetic TDD RED test was added because the
authorized remediation changes dependency resolution text only; the existing
public configuration, metadata, filesystem, and serialization tests remain the
behavioral compatibility seam.

### Environment and authoritative currentness

Validation ran on 2026-07-30 on macOS 26.5.2 (build 25F84), Darwin 25.5.0,
Apple Silicon, with Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1
(`c980f4866`), LLVM 22.1.6, and Git 2.50.1 (Apple Git-155).

The Rust Release Team's current-release index and announcement
(<https://blog.rust-lang.org/releases/latest/>) identify Rust 1.97.1 as current.
Live `cargo search <crate> --limit 1` queries against crates.io reported every
direct Cargo release already current in its approved generation: focaccia 2.0.0,
serde 1.0.229, serde_json 1.0.151, TOML `1.1.4+spec-1.1.0`, Tauri
2.11.5, and tauri-build 2.6.3. The repository's Rust/Cargo selection and direct
manifest baselines therefore required no change, and no new major was considered
or adopted.

### Dependency and lockfile outcome

Commit `9fba73c34b440437ccaf88395448a7639f767cab` regenerates the ordinary
Rust-1.97.1-compatible graph. It changes only `Cargo.lock`, updating `http`
1.4.2 to 1.5.0 and its crates.io checksum. No other compatible graph update was
available. `cargo tree --invert http@1.5.0 --locked` traces the selected release
through current direct Tauri 2.11.5 and tauri-build 2.6.3, including the Tauri
runtime, utility, code-generation, macro, and Wry chains.

There are no `[patch]` or `[replace]` sections, overrides, forced versions,
downgrades, or speculative dependencies. The final
`cargo update --dry-run --verbose` refreshed crates.io, locked zero packages, and
left the lockfile unchanged at:

```text
0a8004d2207f919cb96bb1bbccdd5bca64616b669c99d187fd4eb1ba4e800648  Cargo.lock
```

Its behind-latest notices remain non-updatable transitives selected by current
direct packages: generic-array 0.14.7 through Tauri's SHA-2/code-generation
chain, and TOML 0.8.2, toml_datetime 0.6.3, and toml_edit 0.20.2 through
Tauri's all-target GTK system-deps and proc-macro chains. No transitive override
was added.

### Commands, results, hashes, and scope

- Preflight `git rev-parse HEAD`,
  `git status --porcelain=v1 --untracked-files=all`, ticket-state inspection,
  history inspection, and `git diff --check` passed at the exact fixed point.
- `cargo test -p spielorders-core --test metadata_v1 --locked` passed all 18
  focused public configuration, canonical-fixture, and deterministic
  serialization tests.
- `cargo clean` removed 3.1 GiB. The first clean locked build attempt reached the
  new crate download and was blocked only by sandbox DNS. The immediately approved
  rerun downloaded `http` 1.5.0 and
  `cargo build --workspace --locked` passed from that same empty target.
- `cargo test --workspace --locked` passed 62 Rust tests plus doc tests.
- `cargo test -p spielorders-core --locked` independently passed the same 62
  core tests plus doc tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Under the repository-selected Node 26.5.0/npm 12.0.1,
  `npm run test:schemas` passed 4 schemas, 8 accepted examples, 19 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels, and
  `npm run check:traceability` passed.
- Direct core and desktop `cargo tree --depth 1 --locked` inspections reported the
  six current direct releases above. Inverted trees, the duplicate-generation
  inventory, `cargo metadata --locked --format-version 1 --no-deps`, and manifest
  inspection confirmed the dependency paths, Edition 2024, Rust 1.97.1, resolver
  3, and the unchanged core/adapter workspace boundary.
- Fixed-point name/status, prohibited-path, manifest-override, and
  `git diff --check` audits found no schema, fixture, Node package, frontend,
  production source, test, adapter source, metadata-version-1, M2/M3, or shared
  SpielGantt change.

All Node/npm dependency files remain byte-identical:

```text
35720687e78a606321e7279060c632c21b729ec8f33031e218d2ddd088e95290  package.json
e49138bb4c894e69ca42a71bbf128e2eea38fe45ccff2a8bad96e45530ebdda8  package-lock.json
669656be425097c0a9f7a4efc383dae83ebaf210ecf03a836f8061fd5ca4f6e7  apps/desktop/package.json
486ad3f833aba83f593267ae30d1f5d31878b2ef58c859df6e964a35f05e54e9  apps/desktop/package-lock.json
```

The working-tree and fixed-point SHA-256 Supporting Material hashes are
identical:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

### Fixed-point review and disposition

The project-local code-review workflow compared
`git diff 41afb7a5f04efa09fcae17b4d9c4f505bdd1d8c9...HEAD` after the claim,
implementation, and evidence commits. Standards and Spec reviewers ran
independently and in parallel.

- **Standards:** two hard decision-traceability findings and zero judgment-call
  smells. New milestone and Resolution prose used bare tracker identifiers instead
  of full linked ticket names, contrary to `docs/development/traceability.md`.
  Disposition: resolved in the cited milestone and Resolution hunks by using full
  names and links.
- **Spec:** zero missing or partial requirements, zero scope creep, and zero
  incorrect implementations. No disposition was required.

After review remediation, the focused 18-test metadata suite, 62-test workspace
suite plus doc tests, independent 62-test core suite plus doc tests,
warnings-denied Clippy, schema contracts, traceability, `git diff --check`, and
the authoritative Cargo update dry run all passed again. The dry run still locked
zero packages, the lockfile hash remained unchanged, and both Supporting Material
hashes still matched the fixed point.

All second-remediation acceptance criteria pass. Metadata-version-1 meaning,
configuration behavior, schemas, fixtures, deterministic serialization,
domain/filesystem behavior, adapters, Node/npm dependencies, TypeScript selection,
product behavior, tests, M2/M3 scope, and shared SpielGantt foundations remain
unchanged.
