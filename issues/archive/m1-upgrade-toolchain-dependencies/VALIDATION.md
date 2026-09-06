# M1-upgrade external validation

Status: **passed; externally validated**

Validation date: 2026-07-31

## Fixed points and repository guardrails

- Original pre-upgrade fixed point:
  `80e5527034615d6b71fce63ea40b55437bff9492`
- Frozen completed dependency snapshot:
  `67dc5e4560df173b902b24d3463bb01ff46de23b`
- Ticket 004 start/completed pre-gate documentation target:
  `ab02ed5ca2d0e0ed0534859f31d19f488218090c`
- Exact externally validated completed documentation target:
  `c344bbad232f6da88dc6f2a1ab294a92cda978cb`

Before validation, the source worktree was clean, the current branch was `main`,
and both `HEAD` and `main` resolved exactly to the completed target. All four
upgrade tickets and the milestone were closed, the 2026-07-31 stabilization
window was recorded, and M2 was blocked and unstarted.

The original fixed point is an ancestor and the exact merge base of the completed
target. The complete range contains 39 commits. The ten commits after the frozen
dependency snapshot change only this milestone's acceptance, milestone, ticket,
and roadmap documentation. `git diff --check` passed for the complete range. The
complete log and changed-path inventory were inspected with:

```sh
git status --porcelain=v1 --untracked-files=all
git merge-base --is-ancestor \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  c344bbad232f6da88dc6f2a1ab294a92cda978cb
git merge-base \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  c344bbad232f6da88dc6f2a1ab294a92cda978cb
git rev-list --count \
  80e5527034615d6b71fce63ea40b55437bff9492..\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
git log --reverse --format='%H %s' \
  80e5527034615d6b71fce63ea40b55437bff9492..\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
git diff --name-status \
  80e5527034615d6b71fce63ea40b55437bff9492...\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
git log --reverse --format='%H %s' \
  67dc5e4560df173b902b24d3463bb01ff46de23b..\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
git diff --name-status \
  67dc5e4560df173b902b24d3463bb01ff46de23b...\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
git diff --check \
  80e5527034615d6b71fce63ea40b55437bff9492...\
c344bbad232f6da88dc6f2a1ab294a92cda978cb
```

## Fresh isolated target and toolchains

A new local no-hardlink clone was detached exactly at the completed target in
`/private/tmp/spielorders-m1-upgrade-external.kMybFI/repo`. Its status was empty
and its pre-install scan contained no `target`, `node_modules`, or `dist`
directory. No prior gate evidence or artifact was reused.

Fresh diagnostics reported Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1
(`c980f4866`), LLVM 22.1.6, and macOS 26.5.2 on Apple Silicon. A fresh `zsh -f`
shell sourced NVM and selected Node `v26.5.1` and npm `12.0.2` from
`/Users/example/.nvm/versions/node/v26.5.1/bin`. TypeScript was absent before
dependency recreation and reported `Version 7.0.2` afterward.

The Rust Release Team's authoritative latest-release page linked Rust 1.97.1.
Node's official release index named Node 26.5.1, dated 2026-07-28, as the newest
release in line 26 and marked it as a security release. The npm registry named npm
12.0.2 and TypeScript 7.0.2. The selected toolchains remain current in their
approved generations.

## Dependency recreation and executable acceptance

Two online rounds and a final registry-free round of both exact clean-install
commands passed under Node 26.5.1/npm 12.0.2:

```sh
npm clean-install
npm --prefix apps/desktop clean-install
npm clean-install
npm --prefix apps/desktop clean-install
npm clean-install --offline
npm --prefix apps/desktop clean-install --offline
```

Each root round recreated 5 packages and each desktop round recreated 37 packages.
The offline rounds explicitly audited zero vulnerabilities. The desktop rounds
repeated the expected warning for the unapproved optional `fsevents@2.3.3`
install script; no `allowScripts`, approval, trusted dependency, or other script
authority was added. The four hashes were identical before, between, and after
every round:

```text
638c9b565ac6c2ff499d461e5f66d09299bbdecc9c7b8c7ae38bb431148d3f01  package.json
e6a379b9f0b052f5df7fa804204c8ba7b25a9bc0beff05883112275fce8a2033  package-lock.json
f99cd0b64270d0485975db8634596ac8f048176762f8527459fcdc0678298e69  apps/desktop/package.json
838b919fd854325935426becbb3cb75e9b5682581d7e029f652735b27cf1b6ce  apps/desktop/package-lock.json
```

Every required executable command passed in the detached clone:

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked` | Passed from the initially empty target |
| `cargo test --workspace --locked` | Passed: 62 Rust tests plus doc tests |
| `cargo test -p spielorders-core --locked` | Passed independently: the same 62 core tests plus doc tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `apps/desktop/node_modules/.bin/tsc --noEmit --project apps/desktop/tsconfig.json` | Passed strict TypeScript 7 checking |
| the same command with `--listFilesOnly` | Covered production code, tests, fake/Tauri adapters, exact CSS declaration, and Vite configuration |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 19 rejected fixtures, 22 scenarios, and 2 sentinels |
| `npm run check:traceability` | Passed its configured 7-ticket/4-test-file M0 scope |
| `npm --prefix apps/desktop run test` | Passed both frontend boundary tests |
| `npm --prefix apps/desktop run build` | Passed TypeScript checking and Vite 8.2.0 production build |
| `npm --prefix apps/desktop run tauri build -- --no-bundle` | Passed the release desktop-shell build |

The first sandboxed development-smoke attempt was denied permission to bind
`::1:1420`. The permitted bounded rerun of
`npm --prefix apps/desktop run tauri dev -- --no-watch` reported Vite 8.2.0 ready
in 97 ms. An independent request returned HTTP 200, and an independent process
inspection found the Vite server, Tauri CLI, and
`target/debug/spielorders-desktop` alive without a Package argument or Package
loading failure. Ctrl-C bounded the run, and the final process inspection was
empty. The initial `EPERM` was an environment restriction, not a repository
failure.

## Cargo inventory

The required Cargo inventory comprised the update dry run, locked metadata,
workspace/direct trees, duplicate generations, core and desktop direct trees, and
normal plus all-target inverted selector trees. The authoritative rerun of
`cargo update --dry-run --verbose` reported:

```text
Locking 0 packages to latest Rust 1.97.1 compatible versions
Unchanged generic-array v0.14.7 (available: v0.14.9)
Unchanged toml v0.8.2 (available: v0.8.23)
Unchanged toml_datetime v0.6.3 (available: v0.6.11)
Unchanged toml_edit v0.20.2 (available: v0.20.7)
```

The first sandboxed index refresh failed DNS and was immediately replaced by this
successful authoritative rerun. No lockfile change occurred; its hash remained:

```text
0a8004d2207f919cb96bb1bbccdd5bca64616b669c99d187fd4eb1ba4e800648  Cargo.lock
```

Locked metadata confirms resolver 3, Edition 2024, Rust 1.97.1, the three intended
workspace members, and the direct requirements. Direct trees remain:

```text
spielorders-core -> focaccia 2.0.0, serde 1.0.229,
                    serde_json 1.0.151, TOML 1.1.4
spielorders-cli -> spielorders-core
spielorders-desktop -> spielorders-core, Tauri 2.11.5
spielorders-desktop build -> tauri-build 2.6.3
```

All-target inverted trees trace `generic-array` through Tauri's SHA-2/codegen and
Wry chains and the older TOML family through Tauri's GTK `system-deps` and
proc-macro chains. These are selector-owned, non-updatable generations; no
override was added. Live authoritative `cargo search <crate> --limit 1` queries
reported every direct crate current: focaccia 2.0.0, serde 1.0.229, serde_json
1.0.151, TOML `1.1.4+spec-1.1.0`, Tauri 2.11.5, and tauri-build 2.6.3.

## npm inventory and stabilization observations

For both package roots, `npm ls --all`, `npm outdated --all --json`,
`npm audit --audit-level=low`, and `npm install-scripts ls` ran. Both installed
graphs are valid, both audits report zero vulnerabilities, and both install-script
inventories report no package requiring review. `npm explain` covered every direct
selection and every behind-latest selection, including AJV/`fast-uri`, React and
its type packages, Tauri API/CLI, Node types/`undici-types`, TypeScript, the Vite
React plugin, Vite/PostCSS/Nano ID, Rolldown, its bindings and WASM runtime, OXC
types, and the exact Emscripten-NAPI prerelease selections.

Official registry queries produced this direct inventory:

| Direct selection | Selected | Registry current | Disposition |
| --- | --- | --- | --- |
| npm | 12.0.2 | 12.0.2 | Current |
| TypeScript | 7.0.2 | 7.0.2 | Current |
| AJV | 8.20.0 | 8.20.0 | Current |
| Tauri API | 2.11.1 | 2.11.1 | Current |
| React / ReactDOM | 19.2.8 | 19.2.8 | Current |
| Tauri CLI | 2.11.4 | 2.11.4 | Current |
| Node types | 26.1.2 | 26.1.2 | Current |
| React types | 19.2.17 | 19.2.18 | Frozen compatible drift |
| ReactDOM types | 19.2.3 | 19.2.4 | Frozen compatible drift |
| official Vite React plugin | 6.0.5 | 6.0.5 | Current |
| Vite | 8.2.0 | 8.2.0 | Current |

The refreshed complete compatible post-snapshot inventory is still exactly:

| Selection | Selected | Compatible current | Published | Exact selector and justification |
| --- | --- | --- | --- | --- |
| `@types/react` | 19.2.17 | 19.2.18 | 2026-07-30T21:54:03Z | Direct exact selection; 19.2.18 also satisfies `@types/react-dom`'s `^19.2.0` peer |
| `@types/react-dom` | 19.2.3 | 19.2.4 | 2026-07-30T21:53:05Z | Direct exact selection; compatible patch in approved major 19 |
| `fast-uri` | 3.1.4 | 3.1.5 | 2026-07-31T09:16:56Z | AJV 8.20.0 selects `^3.0.1`; registry latest 4.1.2 is a different major |
| `@napi-rs/wasm-runtime` | 1.2.1 | 1.2.2 | 2026-07-31T08:21:54Z | Optional `@rolldown/binding-wasm32-wasi@1.2.1` selects `^1.2.0` |

No newer compatible release appeared after the passing gate's inventory. Under
the user-approved stabilization window, these remain observations rather than
M1-upgrade failures and were not installed. The complete refresh of toolchains,
direct dependencies, and compatible locked transitives remains mandatory at M2
closeout, with every affected clean-install, build, test, lint, audit, and smoke
gate rerun. Any new major still requires user approval.

Other behind-latest entries remain selector-owned and non-updatable: PostCSS
8.5.25 selects Nano ID `^3.3.16`; Node types 26.1.2 select `undici-types`
`~8.3.0`; and the Rolldown WASM binding selects `@emnapi/core` and
`@emnapi/runtime` 2.0.0-alpha.3. Entries without a current installed version are
uninstalled optional platform packages or optional tools whose wanted versions
equal their registry releases.

## TypeScript 7, independent evidence, and scope

Precise static, dynamic, `require`, and `typescript/lib` searches across repository
sources and installed dependency sources outside TypeScript's own distribution
returned no compiler-API import. `npm --prefix apps/desktop explain typescript`
identifies TypeScript 7.0.2 as a direct development CLI dependency. The installed
tree contains only its wrapper and Apple Silicon platform package; no TypeScript 6
compatibility package is selected.

The selected `tsc --lsp --stdio` language server identified itself as
`typescript-go` 7.0.2. An unsaved project-local document produced TS2322 and
TS6133 diagnostics, and a definition request for `SessionState` in
`capabilities.test.ts` navigated to `capabilities.ts`, zero-based line 24. No
repository file or dependency was added for this check.

Independent recounts found:

- 62 Rust tests: 18 metadata codecs, 8 Order validation, 8 Package/Vendor
  validation, 12 safe paths, 8 single-file replacement, 6 multi-file replacement,
  and 2 core unit/boundary tests;
- 4 version-1 schemas and 8 accepted examples;
- 19 rejected-manifest entries naming 19 unique existing fixture files;
- 22 scenarios with 22 unique names; and
- 2 opaque binary sentinels.

The sentinel hashes match the prior external M1 record exactly:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

The public suites and source/range inspection reproduce metadata-version-1
parsing, strict configuration, deterministic canonical bytes, domain validation,
path confinement, no-follow handling, validated atomic replacement, reverse
rollback with complete failure reporting, and schema/Rust agreement. The complete
upgrade range changes no schema, fixture, core production source, or core public
test; its only frontend source addition is the exact CSS declaration required by
TypeScript 7.

AJV remains the Draft 2020-12 validator. Vite and the official React plugin remain
the frontend build/development layer. Esbuild appears only as an uninstalled
optional Vite peer. Searches and manifest inspection find no Mantine,
`@cfworker/json-schema`, direct esbuild replacement, new install-script authority,
M2/M3 Package-loading, damage, diagnostic, index, or mutation behavior, or shared
SpielGantt dependency. Cargo trees and frontend tests confirm core remains
independent of CLI, Tauri, React, TypeScript, Vite, and every adapter.

After all validation work, the detached clone remained source-clean and every
manifest, lockfile, schema, fixture, source, test, and sentinel byte remained
unchanged.

## Code-review outcome

The project-local code-review workflow compared
`git diff 80e5527034615d6b71fce63ea40b55437bff9492...HEAD`. Standards and Spec
reviewers ran independently and in parallel.

### Standards

The reviewer reported two hard findings and zero judgment-call smells:

1. The npm lock graph contains optional `@emnapi/core` and `@emnapi/runtime`
   2.0.0-alpha.3 plus `@emnapi/wasi-threads` 2.0.1, while `AGENTS.md` requires
   user approval before adopting a new major.
2. Historical milestone and ticket prose sometimes uses shorthand such as
   `Ticket 001` through `Ticket 004`, while
   `docs/development/traceability.md` requires human-facing decision names.

Worst reported Standards issue: the apparent unapproved transitive major-version
selection.

Both findings are dispositioned through validation clarification only:

1. These packages are not repository-selected direct dependencies, overrides, or
   speculative additions. Current direct Vite 8.2.0 selects Rolldown 1.2.1;
   optional `@rolldown/binding-wasm32-wasi@1.2.1` selects the exact Emscripten-NAPI
   prereleases. The complete selector chain and versions were recorded before the
   user explicitly froze and approved exact completed dependency snapshot
   `67dc5e4560df173b902b24d3463bb01ff46de23b` for the gate and this external
   validation. That explicit snapshot authorization covers the already selected
   optional transitive graph; it does not authorize adopting any later major. The
   fresh graph, audits, builds, tests, and development smoke all pass, so this is
   not an unapproved validation-time adoption or a blocker.
2. The cited shorthand identifies the four local implementation work items and
   their chronological handbacks; it does not substitute for the closed product
   and architecture decisions in a `Decision sources` field. Each implementation
   ticket's governing decisions remain linked by full name, milestone ticket links
   use full names, and `npm run check:traceability` passes. This validation record
   uses full work-item names where a new human-facing decision reference is needed.
   The historical shorthand is not a broken decision-source link or a blocking
   traceability defect.

After disposition, Standards has zero blocking documented-standard findings. No
implementation, manifest, lockfile, or dependency change was made.

### Spec

The reviewer reported zero findings: no missing or partial requirement, no scope
creep, and no incorrect implementation. The review explicitly confirmed resolver
3, the approved Rust/Node/npm/TypeScript generations, reproducible lock graphs,
the exact CSS declaration, retained AJV/Vite selections, untouched product
contracts, and the absence of M2 behavior. Worst Spec issue: none.

Code-review summary: Standards reported two findings, both resolved by the
validation clarification above, and zero smells; Spec reported zero findings.

## Outcome

M1-upgrade is **externally validated** at exact completed target
`c344bbad232f6da88dc6f2a1ab294a92cda978cb`. Every reproducibility,
vulnerability, build, test, behavior, preservation, boundary, and scope criterion
passes. The stabilization observations remain assigned to the mandatory complete
M2-closeout refresh. No dependency or implementation artifact was changed.

M2 planning may begin in a separate task. M2 remains unstarted by this validation.
