---
title: Adopt the TypeScript 7 CLI without requiring its future compiler API
status: closed
assignee: /root
claimed_at: 2026-07-27T16:21:47Z
closed_at: 2026-07-27T16:39:03Z
ticket_start_fixed_point: f872cc6acd9217bf8f10faa71e33ebb39708ae47
parent: M1-upgrade — Toolchain and dependency modernization
labels:
  - implementation:m1-upgrade
  - implementation:task
---

## Outcome

Type-check and build the existing React/Tauri frontend with the newest TypeScript 7
CLI while proving SpielOrders does not currently depend on the programmatic compiler
API that TypeScript 7.0 intentionally omits.

## Decision sources

- [M1-upgrade — Toolchain and dependency modernization](../README.md)
- [Prototype the SpielOrders desktop experience](../../../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)
- [Establish the Rust, CLI, Tauri, and frontend workspace skeleton](../../m0-contract-baseline/tickets/001-workspace-skeleton.md)
- [SpielOrders agent instructions](../../../../AGENTS.md)

## Blocking

Unblocked. [Move repository tooling to Node 26 and npm 12](002-node-npm-modernization.md)
is closed.

## Scope

- Adopt the newest stable TypeScript 7 release and update configuration or ambient
  declarations only where required for correct strict CLI type checking.
- Prove repository scripts and selected tools do not import the TypeScript compiler
  API programmatically.
- Preserve the separate strict `tsc --noEmit` check because Vite transpilation does
  not replace type checking.
- Verify current editor/language-server and command-line workflows for this plain
  React project.
- If a required existing tool genuinely needs the TypeScript compiler API, prefer
  the official side-by-side TypeScript 6 compatibility package when it solves that
  distinct need. If it does not, stop and record the exact blocker rather than
  silently downgrading the entire project.

Exclude adding a speculative API consumer, ESLint or another new analysis stack,
Mantine, replacing Vite with esbuild, frontend feature work, React/Tauri architecture
changes, and all M2/M3 behavior.

## Acceptance criteria

- Strict TypeScript 7 CLI checking passes for production frontend code, tests, and
  build configuration without weakening existing compiler options.
- Any CSS or other side-effect import accepted by the build has an explicit,
  accurate declaration rather than a broad suppression.
- Frontend boundary tests, the production Vite build, and Tauri no-bundle build pass
  under the completed Node 26/npm 12 ticket.
- A bounded Vite/Tauri development smoke check passes, and the selected TypeScript 7
  language server supplies ordinary diagnostics/navigation for the project.
- Repository inspection finds no unaccounted programmatic import of the TypeScript
  compiler API. If the TypeScript 6 compatibility package is needed, its consumer
  and justification are recorded exactly.
- No compiler option is relaxed, no application behavior changes, and no M2 scope,
  Mantine dependency, AJV replacement, or Vite replacement is introduced.
- Direct TypeScript and type-package versions are authoritative-current under their
  approved majors at ticket completion.
- The required fixed-point Standards/Spec code review is complete and every finding
  is resolved or explicitly dispositioned.

## Review boundary

Review TypeScript 7 CLI compatibility, strictness, declarations, tool API usage, and
frontend/Tauri build and development behavior. Do not add tooling merely to exercise
the missing compiler API or implement frontend/product features.

## Review disposition

The project-local code-review workflow compared
`git diff f872cc6acd9217bf8f10faa71e33ebb39708ae47...HEAD` after the claim and
implementation commits. Standards and Spec agents ran independently and in
parallel.

- **Standards:** zero hard violations and zero judgment-call code smells. The
  generated lockfile expansion was excluded from smell analysis; the claim,
  milestone, dependency, declaration, boundary, and traceability changes all
  comply with the documented sources.
- **Spec:** zero missing or partial requirements, zero scope creep, and zero
  incorrect implementations. The reviewer confirmed that the TypeScript 7 pin,
  strict standalone CLI workflow, exact stylesheet declaration, unchanged compiler
  options, and absence of Rust/application/excluded-scope changes implement the
  ticket as specified.

There are no findings to remediate or otherwise disposition. Worst issue on each
axis: none.

## Resolution

Ticket 003 started from the clean expected Ticket 002 baseline
`f872cc6acd9217bf8f10faa71e33ebb39708ae47` on `main`. Tickets 001 and 002
were closed, and this ticket was open, unblocked, and unclaimed before `/root`
claimed it. No synthetic TDD RED test was created: the compatibility change was a
package, lockfile, and declaration migration, and the only observed TypeScript 7
defect was the compiler's new diagnostic for the existing CSS side-effect import.
The existing public frontend and repository suites supplied behavioral preservation
evidence.

### Environment, authoritative sources, and selection

Validation ran on 2026-07-27 on macOS 26.5.2 (build 25F84), Darwin 25.5.0,
Apple Silicon, with Git 2.50.1 (Apple Git-155), Rust/Cargo 1.97.1, Node.js
26.5.0, npm 12.0.1, and TypeScript 7.0.2.

Authoritative checks at ticket start found:

- Node's official release index (<https://nodejs.org/dist/index.json>) names
  `v26.5.0` as the newest stable release in approved Current line 26.
- npm's official registry `latest` document
  (<https://registry.npmjs.org/npm/latest>) names `12.0.1`.
- Microsoft's TypeScript 7 announcement
  (<https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/>) and
  the official registry `latest` document
  (<https://registry.npmjs.org/typescript/latest>) identify stable TypeScript
  `7.0.2`, the planning-time release; no newer 7.x release existed.
- Official npm registry `latest` documents found every other direct dependency
  already current in its selected major: AJV 8.20.0; Tauri API 2.11.1 and CLI
  2.11.4; React and ReactDOM 19.2.8; Node types 26.1.1; React types 19.2.17 and
  ReactDOM types 19.2.3; the official Vite React plugin 6.0.4; and Vite 8.1.5.

The completed Ticket 002 mechanism remains unchanged: `.nvmrc` selects Node
26.5.0, both package roots select npm 12.0.1 through exact `packageManager`,
`engines`, and `devEngines` metadata, and the desktop manifest now selects
TypeScript exactly at 7.0.2. A fresh `zsh -f` shell sourced `nvm`, ran `nvm use`,
and reported Node `v26.5.0`, npm `12.0.1`, and `tsc` `Version 7.0.2`.

### Manifest, lockfile, and clean-install outcomes

`npm --prefix apps/desktop install --save-dev --save-exact typescript@7.0.2`
under npm 12 changed only the desktop TypeScript selection and its npm lock graph.
The TypeScript 7 package's official optional platform binaries account for the
lockfile expansion; the installed Apple Silicon binary is 7.0.2. No other direct
dependency changed.

Two consecutive root and desktop `npm clean-install` rounds, followed by the
post-review clean-install round, installed 5 root and 30 desktop packages. Every
install reported zero known vulnerabilities. The following hashes were identical
before, between, and after the repeated installs:

```text
35720687e78a606321e7279060c632c21b729ec8f33031e218d2ddd088e95290  package.json
e49138bb4c894e69ca42a71bbf128e2eea38fe45ccff2a8bad96e45530ebdda8  package-lock.json
669656be425097c0a9f7a4efc383dae83ebaf210ecf03a836f8061fd5ca4f6e7  apps/desktop/package.json
486ad3f833aba83f593267ae30d1f5d31878b2ef58c859df6e964a35f05e54e9  apps/desktop/package-lock.json
```

npm again reported that Vite's optional `fsevents` 2.3.3 install script was not
approved. No script authority was added; Vite production and live development
paths both passed.

`npm outdated --all --json` found no direct update. Every unavoidable
behind-latest transitive remains selected by a current direct package and was
confirmed through `npm explain` or the selecting package metadata, without an
override:

- AJV 8.20.0 selects `fast-uri` `^3.0.1`, resolving 3.1.4 rather than major
  4.1.1.
- Vite 8.1.5 selects `rolldown` `~1.1.5`, resolving 1.1.5 rather than 1.2.0.
  Rolldown 1.1.5 selects `@oxc-project/types` exactly 0.139.0 rather than 0.142.0
  and its platform bindings exactly 1.1.5 rather than 1.2.0.
- Vite 8.1.5 optionally peers on `@vitejs/devtools` `^0.3.0`; wanted 0.3.4
  trails 0.4.9, and the package is not installed.
- Vite-selected PostCSS 8.5.23 selects Nano ID `^3.3.16`, resolving 3.3.16;
  registry latest 6.0.0 is a different major.
- Node types 26.1.1 select `undici-types` `~8.3.0`, resolving 8.3.0 rather than
  8.9.0.

### TypeScript compatibility and compiler-API audit

The existing `tsconfig.json` is byte-unchanged. Its explicit strict options remain
selected, including `strict`, `isolatedModules`, `noEmit`, Bundler module
resolution, and the existing target, libraries, and type set. TypeScript 7 enables
`noUncheckedSideEffectImports` by default and initially reported TS2882 for the
real `./styles.css` import. `src/styles.css.d.ts` now declares only that exact
stylesheet module via its matching filename and `export {}`; no wildcard module,
broad suppression, or weakened compiler option was added.

`tsc --noEmit --project apps/desktop/tsconfig.json` passes. A
`--listFilesOnly` audit confirms the same invocation covers `main.tsx`,
production capability code, the fake adapter, `capabilities.test.ts`, the exact
stylesheet declaration, and `vite.config.ts`.

Repository scripts and source were searched for static, dynamic, `require`, and
`typescript/lib` imports. The complete installed root and desktop dependency graphs
were then inspected with `npm ls --all`, `npm explain typescript`, and the same
source search outside TypeScript's own distribution. TypeScript 7 is a direct CLI
dependency only. No repository script or selected tool imports its compiler API;
incidental documentation strings and Vite/Rolldown handling of TypeScript syntax or
configuration are not compiler-API consumers. The TypeScript 6 compatibility
package is therefore neither needed nor installed.

### Language service and development smoke

Without adding repository tooling, a temporary protocol client invoked the selected
`apps/desktop/node_modules/.bin/tsc --lsp --stdio`. An unsaved, deliberately
mistyped local document produced ordinary TS2322 and TS6133 diagnostics. A
`textDocument/definition` request on the existing `SessionState` import navigated
to `src/capabilities.ts`, line 25. The temporary client was deleted after the
check; no language-service fixture or dependency was added.

The bounded `npm --prefix apps/desktop run tauri dev -- --no-watch` smoke reported
Vite 8.1.5 ready at `http://localhost:1420/` in 89 ms. An independent request
returned HTTP 200; Cargo launched `target/debug/spielorders-desktop`; and process
inspection found both Vite and the shell alive with no Package argument or
Package-loading failure. The run was interrupted and left no matching process.

### Commands, preservation, and scope

All commands below ran after explicitly selecting Node 26.5.0/npm 12.0.1 and
passed before closure:

- repeated `npm clean-install` and
  `npm --prefix apps/desktop clean-install`: reproducible hashes and zero
  vulnerabilities;
- `apps/desktop/node_modules/.bin/tsc --noEmit --project
  apps/desktop/tsconfig.json`: strict TypeScript 7 check passed;
- `npm --prefix apps/desktop run test`: both frontend boundary tests passed;
- `npm --prefix apps/desktop run build`: strict TypeScript check and Vite 8.1.5
  production build passed;
- `npm --prefix apps/desktop run tauri build -- --no-bundle`: release shell build
  passed;
- the bounded Tauri/Vite development smoke described above passed;
- `npm run test:schemas`: 4 schemas, 8 accepted examples, 19 rejected fixtures,
  22 scenarios, and 2 preservation sentinels passed under strict AJV Draft
  2020-12;
- `npm run check:traceability`: configured traceability scope passed; and
- `cargo test --workspace --locked`: all 62 Rust tests plus doc tests passed.

Both opaque Supporting Material sentinels match the external M1 validation record:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

AJV remains the root strict validator; Vite and the official React plugin remain
the frontend build/development layer; and the standalone strict TypeScript check
remains in both frontend test and build scripts. TypeScript 5 is no longer selected
by the frontend manifest or lock graph. The fixed-point diff changes only the
desktop TypeScript manifest/lock selection, the exact stylesheet declaration, and
M1-upgrade tracker evidence. Rust manifests, `Cargo.lock`, compiler options,
application/schema/fixture/CLI/Tauri/React/adapter behavior, and opaque Supporting
Material are unchanged. Searches and diff inspection confirm
`@cfworker/json-schema`, a direct esbuild replacement, Mantine, acceptance-gate
implementation, M2/M3 behavior, and shared SpielGantt foundations are absent.

### Implementation and required review

The narrow implementation and review commits are:

- `6bc0f362e0d07aebe7585fe45377f7675170d631` — claim Ticket 003 and record
  the exact fixed point;
- `727707edf4fe653fe532375563f945f430f97b39` — adopt TypeScript 7.0.2 and
  the exact CSS declaration; and
- `7b15d78c0dcad6aa7fa4d2de054f189af9793fc8` — record the separate
  Standards and Spec review dispositions.

The code-review results are recorded above: both axes returned zero findings, so
there was nothing to remediate or waive. The post-review clean installs, strict
check, frontend tests/build, schema and traceability checks, 62-test Rust suite,
and Tauri no-bundle build all passed again. No excluded scope changed.
