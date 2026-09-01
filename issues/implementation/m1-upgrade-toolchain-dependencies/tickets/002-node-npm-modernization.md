---
title: Move repository tooling to Node 26 and npm 12
status: closed
assignee: /root
claimed_at: 2026-07-31T11:31:44Z
closed_at: 2026-07-31T11:35:09Z
ticket_start_fixed_point: 394628587cf4e1092d66d549e972ec9ea32ac592
reopened_at: 2026-07-31T01:29:09Z
reopened_by_gate: 004-m1-upgrade-acceptance-gate
parent: M1-upgrade — Toolchain and dependency modernization
labels:
  - implementation:m1-upgrade
  - implementation:task
---

## Outcome

Make clean repository, contract-test, frontend, and Tauri development workflows
reproducible under the approved Node 26 Current release line and npm 12 while
retaining the selected AJV and Vite tool families.

## Decision sources

- [M1-upgrade — Toolchain and dependency modernization](../README.md)
- [M1 external validation](../../m1-filesystem-metadata-kernel/VALIDATION.md)
- [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](../../m0-contract-baseline/tickets/001-workspace-skeleton.md)
- [Prototype the SpielOrders desktop experience](../../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

Unblocked. Start from a clean worktree at or after the M1 external-validation
documentation commit. Record the exact ticket-start fixed point before editing.

Reopened by Ticket 004 on 2026-07-30. The fresh target-pinned gate found current
compatible releases newer than the selected Node 26.5.0, npm 12.0.1,
`@types/node` 26.1.1, `@vitejs/plugin-react` 6.0.4, and Vite 8.1.5 selections.
It also found the compatible locked transitive update PostCSS 8.5.23 to 8.5.25.
Ticket 004 made no runtime, dependency, or lockfile repair. This ticket owns the
runtime/package-manager selections, direct npm baselines, npm graph regeneration,
and the complete Node/npm-focused reverification. A new claim must record a new
ticket-start fixed point and append a remediation Resolution.

Reopened again by Ticket 004 on 2026-07-30. The next fresh target-pinned direct
release inventory found compatible `@types/react` 19.2.18 and
`@types/react-dom` 19.2.4 patch releases newer than the selected 19.2.17 and
19.2.3 baselines. Ticket 004 stopped immediately and made no dependency,
manifest, or lockfile repair. This ticket remains the smallest owner of direct npm
baselines, npm graph regeneration, and complete Node/npm-focused reverification.
A new claim must record a new ticket-start fixed point and append another
remediation Resolution.

## Scope

- Adopt the newest stable Node 26 and npm 12 releases, verifying exact current
  versions from authoritative sources at ticket start.
- Provide repository-owned runtime selection and contributor guidance sufficient to
  reproduce the selected major and current point releases.
- Regenerate the root and desktop npm lock graphs using npm 12 and clean installs.
- Update existing npm dependencies to newest compatible releases in their selected
  majors as required by `AGENTS.md`.
- Retain AJV as the strict Draft 2020-12 contract validator.
- Retain Vite and the official React plugin as the frontend production/development
  layer. Esbuild may remain only where selected transitively by that maintained
  toolchain.

Exclude TypeScript 7 itself, Mantine, replacement of AJV or Vite, project-owned
bundler/server infrastructure, product behavior, Rust dependency changes, and all
M2/M3 work.

For the third reopening only, the user-approved 2026-07-31 stabilization window
governs compatible releases published after frozen target
`67dc5e4560df173b902b24d3463bb01ff46de23b`. This reopening records and reviews
the deferral; it must not install those releases or regenerate either npm graph.
The complete compatible refresh moves to M2 closeout.

## Acceptance criteria

- The selected Node and npm versions are documented and reproducibly selected from
  a fresh shell, and package metadata rejects or clearly diagnoses unsupported
  contributor toolchains as appropriate.
- Clean root and desktop installs succeed from the committed lockfiles under npm 12,
  report no known vulnerabilities, and a repeated clean install leaves both
  manifests and lockfiles unchanged.
- All schema contract fixtures and inline cases pass with AJV under Node 26.
- Frontend boundary tests, the production Vite build, and the Tauri no-bundle build
  pass under Node 26/npm 12 with the pre-ticket TypeScript generation.
- A bounded Vite development-server/Tauri development integration smoke check
  confirms the configured development URL becomes available and the shell can start
  without a SpielOrders Package.
- Direct npm dependencies are current within their selected majors; any behind-latest
  transitive is traced to its selecting current direct package and recorded rather
  than overridden.
- AJV, Vite, the React plugin, and both Supporting Material sentinel hashes are
  preserved; `@cfworker/json-schema`, direct esbuild replacement, and Mantine are
  absent.
- The required fixed-point Standards/Spec code review is complete and every finding
  is resolved or explicitly dispositioned.

For the third reopening, the direct-currentness criterion is dispositioned by the
user-approved frozen snapshot and explicit M2-closeout obligation. Closure requires
consistent documentation and fixed-point review, not dependency or executable-gate
changes. All prior implementation acceptance evidence remains historical and is not
rerun or reinterpreted by this documentation-only disposition.

## Review boundary

Review runtime selection, clean-install reproducibility, lockfile effects, current
selected-major npm dependencies, and retained validator/build-tool behavior. Do not
perform TypeScript 7 migration, Rust modernization, frontend feature work, or M2.

## Resolution

Ticket 002 started from the clean fixed point
`6361b224130ad7a498ba3d0bfd86833d2f4f28e8` on `main`. Ticket 001 was closed,
and this ticket was open, unblocked, and unclaimed before `/root` claimed it. No
synthetic TDD RED test was created because the authorized changes were runtime
selection, package metadata, contributor guidance, and npm lockfile text; the
existing public contract and adapter suites supplied the behavioral preservation
evidence, and no compatibility defect appeared.

### Environment, release sources, and selection

Validation ran on 2026-07-27 on macOS 26.5.2 (build 25F84), Darwin 25.5.0,
Apple Silicon, with Git 2.50.1 (Apple Git-155), Rust/Cargo 1.97.1, Node.js
26.5.0, and npm 12.0.1.

Authoritative checks at ticket start found:

- Node's official release index
  (<https://nodejs.org/dist/index.json>) names `v26.5.0` as the newest stable
  release in approved Current line 26.
- npm's official registry `latest` document
  (<https://registry.npmjs.org/npm/latest>) names `12.0.1`.
- `npm view <package>@<selected-major> version --json`, against the official npm
  registry, found every direct dependency already current in its selected major:
  AJV 8.20.0; Tauri API 2.11.1 and CLI 2.11.4; React and ReactDOM 19.2.8;
  Node types 26.1.1; React types 19.2.17 and ReactDOM types 19.2.3; the official
  Vite React plugin 6.0.4; TypeScript 5.9.3; and Vite 8.1.5.

The repository now selects Node exactly through `.nvmrc`, selects npm exactly
through each independently installable package root's `packageManager` metadata,
and documents the fresh-shell `nvm install`, `nvm use`, and
`npm install --global npm@12.0.1` reproduction sequence in `README.md`. Exact
`engines` declarations diagnose older clients, while npm's `devEngines` checks
reject `install`, `ci`, and `run` under a mismatched runtime or package manager.
A fresh `zsh -f` shell selected `v26.5.0` and `12.0.1`. Node 24.15.0 failed with
`EBADDEVENGINES` for the runtime, and npm 11.18.0 running on Node 26 failed
separately for the package manager.

### Lockfiles, installs, and dependency evidence

Both lock graphs were regenerated under Node 26.5.0/npm 12.0.1 with
`npm install --package-lock-only --ignore-scripts --offline`, at the root and with
`--prefix apps/desktop`, followed by clean installs. npm 12 added the two root
package engine declarations to each lockfile; all dependency selections stayed
unchanged.

Two consecutive online `npm clean-install` and
`npm --prefix apps/desktop clean-install` passes installed 5 root and 29 desktop
packages, respectively, and every pass reported zero known vulnerabilities. The
four files had identical hashes before, between, and after both clean-install
passes:

```text
35720687e78a606321e7279060c632c21b729ec8f33031e218d2ddd088e95290  package.json
e49138bb4c894e69ca42a71bbf128e2eea38fe45ccff2a8bad96e45530ebdda8  package-lock.json
7da41d9cbe072a5f8112d3492028db0e8d2e3f805cf14c016c9c0df781b9fb4e  apps/desktop/package.json
49e7d4f55cb98923deb60293dcc61de66c2e4bab11f72217829aa151d1d6a450  apps/desktop/package-lock.json
```

npm 12 reported that the install script of Vite's optional `fsevents` 2.3.3
dependency was not approved. No new script authority was granted: both production
and development Vite paths passed, including live file-server startup, so the
maintained fallback is sufficient.

`npm outdated --all --json` found no direct selected-major update. Each
behind-latest transitive remains on the selector of a current direct package and
was confirmed with `npm explain`, without an override:

- AJV 8.20.0 selects `fast-uri` `^3.0.1`, resolving 3.1.4 rather than major 4.1.1.
- Vite 8.1.5 selects `rolldown` `~1.1.5`; Rolldown 1.1.5 selects
  `@oxc-project/types` exactly 0.139.0 and its platform bindings exactly 1.1.5,
  while their registry latest releases are 0.141.0 and 1.2.0.
- Vite 8.1.5 optionally peers on `@vitejs/devtools` `^0.3.0`, whose wanted
  0.3.4 trails 0.4.9; it is not installed.
- Vite 8.1.5 selects PostCSS 8.5.23, which selects Nano ID `^3.3.16`; 3.3.16 is
  current for that range while registry latest 6.0.0 is a different major.
- Node types 26.1.1 select `undici-types` `~8.3.0`, resolving 8.3.0 rather than
  8.9.0.

TypeScript 5.9.3 is the newest release in the selected major and intentionally
trails registry latest 7.0.2; Ticket 003 owns that separately approved major
migration.

### Commands and behavior preservation

All commands below ran after explicitly selecting Node 26.5.0/npm 12.0.1:

- `npm run test:schemas` passed 4 schemas, 8 accepted examples, 19 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels using strict AJV Draft
  2020-12.
- `npm run check:traceability` passed its configured 7-ticket/4-test-file scope.
- `npm --prefix apps/desktop run test` passed both frontend boundary tests.
- `apps/desktop/node_modules/.bin/tsc --noEmit --project apps/desktop/tsconfig.json`
  passed with the pre-ticket TypeScript 5.9.3 generation.
- `npm --prefix apps/desktop run build` passed the TypeScript check and Vite 8.1.5
  production build.
- `npm --prefix apps/desktop run tauri build -- --no-bundle` passed the release
  desktop-shell build.
- `npm --prefix apps/desktop run tauri dev -- --no-watch` reported Vite ready at
  configured URL `http://localhost:1420`; an independent probe returned HTTP 200,
  Cargo launched `target/debug/spielorders-desktop`, and process inspection found
  both Vite and the desktop shell alive with no Package argument or Package-loading
  failure. The bounded run was interrupted and left no matching process.

The two opaque Supporting Material sentinels still match the external M1 validation
record exactly:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

### Implementation and required review

The narrow implementation commits are:

- `12e88c4f3aaf7f3ed4e8a99a37db70ee319f83ac` — claim Ticket 002 and record
  its fixed point.
- `a9d2e32db36cdce266dcb2578fe42b1bb4837634` — add the runtime selection,
  package-manager selection and enforcement, contributor guidance, and npm-12
  lockfile metadata.
- `bdacd497f784e22d7012d866377d2a30eb2c266c` — resolve and record the separate
  Standards and Spec review dispositions.

The project-local code-review workflow compared
`git diff 6361b224130ad7a498ba3d0bfd86833d2f4f28e8...HEAD` and ran Standards and
Spec agents independently and in parallel:

- **Standards:** one hard finding observed that the milestone table still said
  Open after the claim. This Resolution commit changes it to In progress, resolving
  the documented traceability violation. One judgment call identified duplicate
  runtime-policy metadata in the root and desktop manifests. It is retained and
  explicitly dispositioned: these are independent npm package roots, and each must
  reject a mismatched toolchain before its own `install`, `ci`, or `run`; a shared
  script would run too late and weaken the accepted guard.
- **Spec:** zero findings: no missing or partial requirement, incorrect
  implementation, or scope creep.

AJV remains the direct strict validator; Vite and the official React plugin remain
the frontend build/development layer; TypeScript remains 5.9.3. Searches and
fixed-point diff inspection confirm `@cfworker/json-schema`, direct esbuild
replacement, Mantine, TypeScript 7, Rust changes, schema/fixture/product changes,
Ticket 003 work, acceptance-gate work, M2/M3 behavior, and shared SpielGantt
foundations are absent.

After the review-disposition commit, the complete gate was rerun under
Node 26.5.0/npm 12.0.1. Both online clean installs again reported zero
vulnerabilities; schema contracts, traceability, both frontend boundary tests, the
standalone strict TypeScript 5.9.3 check, the production Vite build, and the Tauri
no-bundle build all passed. Manifests and lockfiles remained unchanged, sentinel
hashes still matched, and the worktree was clean before this closing edit.

## Remediation Resolution — 2026-07-30 Node/npm currentness

This remediation was reclaimed from the exact clean fixed point
`759d717c2dd2ac33862880938a5ac9f4f065c4a1`.
[Modernize Rust workspace resolution and direct dependency baselines](001-rust-cargo-modernization.md)
and [Adopt the TypeScript 7 CLI without requiring its future compiler API](003-typescript-7-cli.md)
were closed; this ticket was open, unblocked, and unclaimed; and
[Integrate and verify the M1-upgrade acceptance gate](004-m1-upgrade-acceptance-gate.md)
was open and blocked only by this remediation. Commit
`d9a50e155bc2065f6ea6132c0ed1a79ae7e19d13` contains only the claim and milestone
status update. No synthetic TDD RED test was created because the remediation changes
only dependency and toolchain text. No compatibility defect appeared through a
public seam.

### Environment and authoritative release refresh

The gate ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Git 2.50.1 (Apple Git-155), Rust/Cargo 1.97.1, LLVM 22.1.6, selected Node
26.5.1, selected npm 12.0.2, TypeScript 7.0.2, and Vite 8.2.0.

The exact authoritative release commands were:

```sh
curl -s https://nodejs.org/dist/index.json
npm view <package> version
```

Node's official release index reports Node 26.5.1, dated 2026-07-28, as the newest
release in approved line 26 and marks it as a non-LTS security release. The official
npm registry reports this complete direct inventory:

```text
npm 12.0.2
ajv 8.20.0
typescript 7.0.2
@tauri-apps/api 2.11.1
react 19.2.8
react-dom 19.2.8
@tauri-apps/cli 2.11.4
@types/node 26.1.2
@types/react 19.2.17
@types/react-dom 19.2.3
@vitejs/plugin-react 6.0.5
vite 8.2.0
```

No newer compatible direct release exists. The repository now selects Node 26.5.1
through `.nvmrc`, selects npm 12.0.2 through both independently installable package
roots, and documents the same versions in contributor guidance. The direct desktop
baselines moved only from Node types 26.1.1 to 26.1.2, the official React plugin
6.0.4 to 6.0.5, and Vite 8.1.5 to 8.2.0. TypeScript remains 7.0.2; AJV remains
8.20.0.

A fresh `zsh -f` shell running `nvm use --silent` selected:

```text
node=v26.5.1
npm=12.0.2
node_path=/Users/example/.nvm/versions/node/v26.5.1/bin/node
npm_path=/Users/example/.nvm/versions/node/v26.5.1/bin/npm
```

An offline package-lock-only install under Node 24.18.0/npm 11.18.0 exited 1 with
`EBADDEVENGINES` and identified the required Node 26.5.1 runtime. A separate run of
npm 11.18.0 on selected Node 26.5.1 exited 1 and identified required npm 12.0.2.
The exact selection and mismatch commands were:

```sh
zsh -f -c \
  'source /Users/example/.nvm/nvm.sh; nvm use --silent; node --version; npm --version'
PATH=/Users/example/.nvm/versions/node/v24.18.0/bin:/usr/bin:/bin \
  /Users/example/.nvm/versions/node/v24.18.0/bin/npm \
  install --package-lock-only --ignore-scripts --offline
PATH=/Users/example/.nvm/versions/node/v26.5.1/bin:/usr/bin:/bin \
  /Users/example/.nvm/versions/node/v24.18.0/bin/npm \
  install --package-lock-only --ignore-scripts --offline
```

### npm graph regeneration and reproducibility

npm 12.0.2 regenerated both lock graphs. Vite 8.2.0 selected Rolldown 1.2.1,
PostCSS 8.5.25, `@oxc-project/types` 0.142.0, Rolldown platform bindings 1.2.1,
`@napi-rs/wasm-runtime` 1.2.1, and the exact optional WASM support versions required
by that maintained graph. No override or direct transitive dependency was added.
The lock-only PostCSS refresh used Vite's `^8.5.23` selector.

The exact regeneration and clean-install commands were:

```sh
npm install --package-lock-only --ignore-scripts
npm --prefix apps/desktop install --package-lock-only --ignore-scripts
npm --prefix apps/desktop update postcss --package-lock-only --ignore-scripts
npm clean-install
npm --prefix apps/desktop clean-install
npm clean-install
npm --prefix apps/desktop clean-install
```

Each final root round installed 5 packages and each final desktop round installed
37 packages. Every round audited zero vulnerabilities and left all four manifest
and lockfile hashes identical:

```text
638c9b565ac6c2ff499d461e5f66d09299bbdecc9c7b8c7ae38bb431148d3f01  package.json
e6a379b9f0b052f5df7fa804204c8ba7b25a9bc0beff05883112275fce8a2033  package-lock.json
f99cd0b64270d0485975db8634596ac8f048176762f8527459fcdc0678298e69  apps/desktop/package.json
838b919fd854325935426becbb3cb75e9b5682581d7e029f652735b27cf1b6ce  apps/desktop/package-lock.json
```

The desktop clean installs repeated npm 12's known warning that optional
`fsevents` 2.3.3 has a blocked install script. No `allowScripts` metadata, approval,
or other install-script authority was added.

### Graph, audit, selector, and currentness evidence

The final inspection commands were:

```sh
npm ls --all
npm --prefix apps/desktop ls --all
npm outdated --all --json
npm --prefix apps/desktop outdated --all --json
npm audit --audit-level=low
npm --prefix apps/desktop audit --audit-level=low
npm --prefix apps/desktop install-scripts ls
npm explain fast-uri
npm --prefix apps/desktop explain \
  @types/node @vitejs/plugin-react vite postcss rolldown \
  @oxc-project/types @emnapi/core @emnapi/runtime nanoid undici-types
```

Both installed graphs are valid and both explicit audits report zero
vulnerabilities. No direct or compatible installed update remains. The nonempty
outdated reports are fully selector-owned and were not overridden:

- AJV 8.20.0 selects `fast-uri` `^3.0.1`, resolving current/wanted 3.1.4 rather
  than different-major latest 4.1.1.
- PostCSS 8.5.25 selects Nano ID `^3.3.16`, resolving current/wanted 3.3.16
  rather than different-major latest 6.0.0.
- Node types 26.1.2 select `undici-types` `~8.3.0`, resolving current/wanted 8.3.0
  rather than 8.9.0.
- Rolldown's optional WASM binding 1.2.1 requires `@emnapi/core` and
  `@emnapi/runtime` exactly at 2.0.0-alpha.3. npm's `latest` tags still name
  1.11.3, while the installed prereleases satisfy the exact current Rolldown
  binding and the `@napi-rs/wasm-runtime` peer range.
- Currentless entries in the desktop outdated report are optional, uninstalled
  platform bindings, preprocessors, plugins, and tools. Their wanted versions
  equal their latest releases.

### Executable behavior and development smoke

After explicitly selecting Node 26.5.1/npm 12.0.2:

- `apps/desktop/node_modules/.bin/tsc --noEmit --project
  apps/desktop/tsconfig.json` passed strict TypeScript 7 checking.
- `npm run test:schemas` passed 4 schemas, 8 accepted examples, 19 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels with AJV.
- `npm run check:traceability` passed its configured 7-ticket/4-test-file scope.
- `npm --prefix apps/desktop run test` passed both frontend boundary tests.
- `npm --prefix apps/desktop run build` passed the TypeScript check and Vite 8.2.0
  production build.
- `npm --prefix apps/desktop run tauri build -- --no-bundle` passed the release
  desktop-shell build.

The bounded `npm --prefix apps/desktop run tauri dev -- --no-watch` integration
smoke reported Vite 8.2.0 ready in 136 ms at `http://localhost:1420/`. An
independent request returned HTTP 200. Process inspection found both Vite and
`target/debug/spielorders-desktop` alive with no Package argument or
Package-loading failure. Ctrl-C bounded the run, and a final process search found
no matching process.

The exact live probes were:

```sh
curl -s -o /dev/null -w 'vite_http_status=%{http_code}\n' \
  http://localhost:1420/
ps -ax -o pid=,command= |
  rg 'vite|target/debug/spielorders-desktop' |
  rg -v 'rg '
```

### Preservation and scope evidence

The two opaque Supporting Material sentinels still match the external M1 validation:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

Rust-owned selections remained byte-identical:

```text
0a8004d2207f919cb96bb1bbccdd5bca64616b669c99d187fd4eb1ba4e800648  Cargo.lock
7b406ced7bed388e694b26046de091a825fb233aac867d21749a5dde563a30bc  Cargo.toml
24a5037a8f0a264369ef37bb46e019ade6c524beaead1c28f77a429f856a066b  rust-toolchain.toml
```

Fixed-point path inspection shows implementation commit
`f33e7d4607f67d311667d5e9d421c45d691cb233` changes only `.nvmrc`,
`README.md`, the two package manifests, and the two npm lockfiles. No Rust,
production source, test, schema, fixture, metadata-version-1, or Supporting Material
path changed. AJV, Vite, and the official React plugin remain selected.
`@cfworker/json-schema`, direct esbuild replacement, Mantine, new install-script
authority, acceptance-gate implementation, M2/M3 behavior, and a shared SpielGantt
foundation are absent.

The exact final hash and scope commands were:

```sh
shasum -a 256 \
  package.json package-lock.json \
  apps/desktop/package.json apps/desktop/package-lock.json \
  Cargo.lock Cargo.toml rust-toolchain.toml
shasum -a 256 \
  contracts/fixtures/assets/supporting-a.bin \
  contracts/fixtures/assets/supporting-b.bin
git diff --name-status 759d717c2dd2ac33862880938a5ac9f4f065c4a1
git diff --check 759d717c2dd2ac33862880938a5ac9f4f065c4a1
git diff --name-only 759d717c2dd2ac33862880938a5ac9f4f065c4a1 -- \
  Cargo.lock Cargo.toml rust-toolchain.toml crates contracts \
  apps/desktop/src apps/desktop/src-tauri
rg -n \
  '"(ajv|vite|@vitejs/plugin-react|@cfworker/json-schema|@mantine/[^" ]*|mantine|esbuild)"' \
  package.json apps/desktop/package.json package-lock.json \
  apps/desktop/package-lock.json
```

### Fixed-point review and disposition

The project-local code-review workflow compared:

```sh
git diff 759d717c2dd2ac33862880938a5ac9f4f065c4a1...HEAD
git log 759d717c2dd2ac33862880938a5ac9f4f065c4a1..HEAD --oneline
```

Standards and Spec agents ran independently and in parallel against the claim,
implementation, and evidence commits.

- **Standards:** one hard finding and zero judgment-call smells. The newly added
  remediation evidence used bare tracker identifiers in human-facing prose, contrary
  to `docs/development/traceability.md`. The finding is resolved above by using the
  full linked names of the affected implementation and acceptance-gate work. The
  affected `npm run check:traceability` check and both fixed-point/current
  `git diff --check` inspections passed afterward.
- **Spec:** zero findings. The reviewer found no missing or partial requirement,
  scope creep, or incorrect implementation.

Every review finding is resolved; no waiver or deferred review action remains.

### Closure

The narrow remediation history is:

- `d9a50e155bc2065f6ea6132c0ed1a79ae7e19d13` — claim from the exact fixed point;
- `f33e7d4607f67d311667d5e9d421c45d691cb233` — refresh runtime selections,
  direct baselines, and npm lock graphs;
- `f8e85f5c06f7b46262f68d624b510a2057bca223` — record the complete gate
  evidence; and
- `dbca297c02c25a89ed8c52f7f4e8657794ea81cb` — resolve and record the
  independent Standards and Spec review.

Every acceptance criterion passes, the affected post-review checks pass, and no
waiver remains. The worktree was clean before this closing edit. The acceptance
gate remains open, unclaimed, and unimplemented by this remediation.

## User-authorized deferral disposition — 2026-07-31

The third gate handback was claimed from exact clean fixed point
`394628587cf4e1092d66d549e972ec9ea32ac592`. Commit
`0f2cb5c7989392fc53e11c619d5da2829d18e86b` records only the claim and
milestone state.

The official npm registry had reported `@types/react` 19.2.18 and
`@types/react-dom` 19.2.4 after the completed M1-upgrade target selected 19.2.17
and 19.2.3. The user explicitly deferred this compatible release drift, and any
further compatible drift found during M1-upgrade closeout, to M2 closeout. This is
a milestone-specific stabilization decision and does not change the general
currency policy in `AGENTS.md`.

No npm dependency or lock graph is changed by this disposition. The completed and
already reviewed Node/npm implementation remains the frozen M1-upgrade snapshot.
The M1-upgrade gate and fresh external validation must report authoritative release
drift but must not reopen this ticket solely for later compatible releases. The M2
acceptance gate owns a complete refresh of toolchains, direct dependencies, and
compatible locked transitives, with all affected clean-install, build, test, audit,
and smoke checks.

Actual reproducibility, vulnerability, build, test, or behavior failures remain
blocking and return to their smallest owner. New majors still require user
approval. This disposition changes documentation only and adds no runtime,
dependency, manifest, lockfile, source, test, schema, fixture, product, M2
behavior, install-script authority, or Supporting Material change.

### Fixed-point review and disposition

The project-local code-review workflow compared
`git diff 394628587cf4e1092d66d549e972ec9ea32ac592...HEAD`. Standards and
Spec reviewers ran independently and in parallel.

- **Standards:** one hard traceability finding and zero judgment-call smells. Two
  new acceptance-evidence sentences used a bare tracker identifier instead of the
  full linked work-item name. Both are corrected to link
  [Integrate and verify the M1-upgrade acceptance
  gate](004-m1-upgrade-acceptance-gate.md).
- **Spec:** two consistency findings and no scope creep. The original active scope
  still required installing current compatible packages, and the gate blocker did
  not distinguish documentation review from dependency remediation. The
  third-reopening scope and acceptance override now explicitly prohibit installing
  post-snapshot compatible releases, and the gate blocker now names only this
  disposition's review and reclosure.

Every finding is resolved. `npm run check:traceability`, fixed-point
`git diff --check`, and current-tree `git diff --check` pass afterward. No
dependency or executable product gate was affected or rerun.

### Closure

The narrow documentation history is:

- `0f2cb5c7989392fc53e11c619d5da2829d18e86b` — claim the third
  handback from the exact fixed point;
- `3bfab86d8cada29809cd5d4662e2711d0c7b12a1` — record the frozen
  M1-upgrade snapshot and mandatory M2-closeout refresh; and
- `0154858f18bfc5d69a79cf7627b7dd5a94164024` — resolve and record the
  independent Standards and Spec findings.

The user-authorized deferral is complete and reviewed. This ticket recloses
without changing a dependency or executable artifact. The
[Integrate and verify the M1-upgrade acceptance
gate](004-m1-upgrade-acceptance-gate.md) is open, unblocked, and unclaimed.
