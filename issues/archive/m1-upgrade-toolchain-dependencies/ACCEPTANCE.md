# M1-upgrade acceptance evidence

Status: **passed, reviewed, and closed with M1-upgrade on 2026-07-31**

Latest gate date: 2026-07-31

Original pre-upgrade fixed point:
`80e5527034615d6b71fce63ea40b55437bff9492`

Frozen completed dependency snapshot:
`67dc5e4560df173b902b24d3463bb01ff46de23b`

Completed documentation target and Ticket 004 fixed point:
`ab02ed5ca2d0e0ed0534859f31d19f488218090c`

Latest isolated gate target, including only the Ticket 004 claim documentation:
`756792f82565bc568843b7d668e1e71ca6876e62`

## Passing complete gate rerun — 2026-07-31

### Target, ancestry, range, and isolated copy

Ticket 004 started from the exact clean completed documentation target
`ab02ed5ca2d0e0ed0534859f31d19f488218090c`. Commit
`756792f82565bc568843b7d668e1e71ca6876e62` contains only the claim state in
this milestone index and ticket. Its parent is the exact target. No executable
result, evidence file, dependency tree, or build artifact from any of the three
failed gate attempts was reused.

The original pre-upgrade fixed point is an ancestor and the exact merge base of
the completed documentation target. The range contains 36 commits. The seven
commits after frozen dependency snapshot
`67dc5e4560df173b902b24d3463bb01ff46de23b` change only this milestone's
acceptance, milestone, ticket, and roadmap documentation. The claim changes only
the milestone index and this ticket. The exact checks were:

```sh
git merge-base --is-ancestor \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  ab02ed5ca2d0e0ed0534859f31d19f488218090c
git merge-base \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  ab02ed5ca2d0e0ed0534859f31d19f488218090c
git rev-list --count \
  80e5527034615d6b71fce63ea40b55437bff9492..\
ab02ed5ca2d0e0ed0534859f31d19f488218090c
git log --oneline \
  80e5527034615d6b71fce63ea40b55437bff9492..\
ab02ed5ca2d0e0ed0534859f31d19f488218090c
git diff --name-status \
  67dc5e4560df173b902b24d3463bb01ff46de23b...\
ab02ed5ca2d0e0ed0534859f31d19f488218090c
git diff --name-status \
  ab02ed5ca2d0e0ed0534859f31d19f488218090c...\
756792f82565bc568843b7d668e1e71ca6876e62
git diff --check \
  80e5527034615d6b71fce63ea40b55437bff9492...\
756792f82565bc568843b7d668e1e71ca6876e62
```

A fresh local no-hardlink clone was created and detached at the claim commit:

```sh
git clone --no-hardlinks \
  /Users/example/Code/Apps/SpielOrders \
  /private/tmp/spielorders-m1-upgrade-gate.PFTStB
git -C /private/tmp/spielorders-m1-upgrade-gate.PFTStB \
  checkout --detach 756792f82565bc568843b7d668e1e71ca6876e62
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
find . -type d \( -name target -o -name node_modules -o -name dist \) \
  -prune -print
```

The detached HEAD was exact, both source and clone status were empty, and the
pre-install artifact scan printed nothing. The clone remained source-clean after
all gate work.

### Environment and selected toolchains

The complete gate ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple
Silicon, with Git 2.50.1 (Apple Git-155), Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`), Cargo 1.97.1
(`c980f4866141969fab6254a680546a277789d6f0`), and LLVM 22.1.6.

A fresh `zsh -f` shell sourced NVM and ran `nvm use --silent`. It selected Node
`v26.5.1` and npm `12.0.2` from
`/Users/example/.nvm/versions/node/v26.5.1/bin`. TypeScript was correctly absent
before dependency recreation. After the clean install, the selected project CLI
reported `Version 7.0.2`.

The Rust Release Team's
<https://blog.rust-lang.org/releases/latest/> page redirects to its 2026-07-16
Rust 1.97.1 announcement. Node's official
<https://nodejs.org/dist/index.json> index identifies 26.5.1, dated 2026-07-28,
as the newest release in approved line 26 and marks it as a security release. The
official npm registry identifies npm 12.0.2 and TypeScript 7.0.2. Every selected
toolchain release is current in its approved generation.

### Reproducible installs, hashes, and executable gate

Two consecutive rounds of the following commands ran under selected Node
26.5.1/npm 12.0.2:

```sh
npm clean-install
npm --prefix apps/desktop clean-install
shasum -a 256 \
  package.json package-lock.json \
  apps/desktop/package.json apps/desktop/package-lock.json
```

The original M1 registry-free reproduction commands then passed as a third clean
round from the populated local cache:

```sh
npm clean-install --offline
npm --prefix apps/desktop clean-install --offline
shasum -a 256 \
  package.json package-lock.json \
  apps/desktop/package.json apps/desktop/package-lock.json
```

Each root round installed 5 packages and each desktop round installed 37 packages.
Every round reported zero known vulnerabilities and reproduced these identical
hashes before, between, and after installation:

```text
638c9b565ac6c2ff499d461e5f66d09299bbdecc9c7b8c7ae38bb431148d3f01  package.json
e6a379b9f0b052f5df7fa804204c8ba7b25a9bc0beff05883112275fce8a2033  package-lock.json
f99cd0b64270d0485975db8634596ac8f048176762f8527459fcdc0678298e69  apps/desktop/package.json
838b919fd854325935426becbb3cb75e9b5682581d7e029f652735b27cf1b6ce  apps/desktop/package-lock.json
```

The desktop installs repeated npm's expected warning that optional `fsevents`
2.3.3 has a blocked install script. No `allowScripts`, approval, or other script
authority was added. `npm install-scripts ls` and its desktop-prefixed counterpart
reported no package requiring review.

Every required executable command passed in the target-pinned clone:

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked` | Passed from the initially empty target |
| `cargo test --workspace --locked` | Passed: 62 Rust tests plus doc tests |
| `cargo test -p spielorders-core --locked` | Passed independently: the same 62 core tests plus doc tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `apps/desktop/node_modules/.bin/tsc --noEmit --project apps/desktop/tsconfig.json` | Passed strict TypeScript 7 checking |
| `apps/desktop/node_modules/.bin/tsc --noEmit --listFilesOnly --project apps/desktop/tsconfig.json` | Covered production, tests, fake/Tauri adapters, exact CSS declaration, and Vite config |
| `npm run test:schemas` | Passed: 4 schemas, 8 accepted examples, 19 rejected fixtures, 22 scenarios, and 2 sentinels |
| `npm run check:traceability` | Passed its configured 7-ticket/4-test-file M0 scope |
| `npm --prefix apps/desktop run test` | Passed both frontend boundary tests |
| `npm --prefix apps/desktop run build` | Passed TypeScript checking and Vite 8.2.0 production build |
| `npm --prefix apps/desktop run tauri build -- --no-bundle` | Passed the release desktop-shell build |
| `npm clean-install --offline` | Passed: 5 packages, zero vulnerabilities, unchanged manifest/lockfile hashes |
| `npm --prefix apps/desktop clean-install --offline` | Passed: 37 packages, zero vulnerabilities, unchanged manifest/lockfile hashes |

The bounded command
`npm --prefix apps/desktop run tauri dev -- --no-watch` reported Vite 8.2.0 ready
in 99 ms at `http://localhost:1420/`. An independent request returned HTTP 200,
and process inspection found Vite, Tauri CLI, and
`target/debug/spielorders-desktop` alive with no Package argument or Package-load
failure. Ctrl-C bounded the run and the final process check was empty. The first
sandboxed attempt was denied permission to bind `::1:1420`; the permitted rerun
above passed and identifies that first result as an environment restriction, not a
repository failure.

### TypeScript compiler-API and language-service audit

Precise static, dynamic, and `require` import searches across repository scripts,
sources, and installed package sources outside TypeScript's own distribution found
no import of `typescript` or `typescript/lib`. `npm --prefix apps/desktop explain
typescript` identifies TypeScript 7.0.2 as a direct development CLI dependency
only. The installed graph contains the TypeScript wrapper and selected Apple
Silicon platform package, and no TypeScript 6 compatibility package.

The exact import and selection commands were:

```sh
rg -n --pcre2 \
  --glob '!**/node_modules/**' --glob '!target/**' --glob '!dist/**' \
  --glob '*.{js,jsx,ts,tsx,mjs,cjs}' \
  "(?:from\\s*|import\\s*\\(|require\\s*\\()\\s*['\"]typescript(?:/lib(?:/[^'\"]*)?)?['\"]" \
  .
rg -n --pcre2 \
  --glob '!**/node_modules/typescript/**' \
  --glob '!**/node_modules/@typescript/**' \
  --glob '*.{js,jsx,ts,tsx,mjs,cjs}' \
  "(?:from\\s*|import\\s*\\(|require\\s*\\()\\s*['\"]typescript(?:/lib(?:/[^'\"]*)?)?['\"]" \
  node_modules apps/desktop/node_modules
npm --prefix apps/desktop explain typescript
find apps/desktop/node_modules -maxdepth 3 -type d \
  \( -name typescript -o -name 'typescript-*' \) -print
```

The strict `tsconfig.json` remains byte-unchanged. Its `strict`,
`isolatedModules`, `noEmit`, Bundler module resolution, target, libraries, and type
set remain enabled. The exact `styles.css.d.ts` declaration remains selected; no
wildcard suppression or relaxed option was added.

A newly created temporary protocol client invoked
`apps/desktop/node_modules/.bin/tsc --lsp --stdio`. The server identified itself as
`typescript-go` 7.0.2, advertised diagnostic and definition providers, reported
TS2322 and TS6133 for a deliberately mistyped unsaved document, and resolved
`SessionState` in `capabilities.test.ts` to `capabilities.ts`, line 25. The client
was deleted after the check and no repository tooling or fixture was added.
The exact client command was
`node /private/tmp/spielorders-ts-lsp-gate.mjs`; it sent `initialize`,
`initialized`, `textDocument/didOpen`, `textDocument/diagnostic`, and
`textDocument/definition` requests before `shutdown` and `exit`. The diagnostic
document contained `export const gateTyped: string = 1;` and an unused local; the
definition request targeted zero-based line 2, character 10 of
`capabilities.test.ts`.

### Cargo graph and compatible-release inventory

The exact Cargo inspections were:

```sh
cargo update --dry-run --verbose
cargo metadata --locked --format-version 1 --no-deps
cargo tree --workspace --depth 1 --locked
cargo tree --duplicates --locked
cargo tree -p spielorders-core --depth 1 --locked
cargo tree -p spielorders-desktop --depth 1 --locked
for package_id in \
  generic-array@0.14.7 toml@0.8.2 \
  toml_datetime@0.6.3 toml_edit@0.20.2
do
  cargo tree --invert "$package_id" --locked
  cargo tree --target all --invert "$package_id" --locked
done
for crate_name in focaccia serde serde_json toml tauri tauri-build
do
  cargo search "$crate_name" --limit 1
done
```

The authoritative dry run refreshed crates.io, locked zero packages to the latest
Rust-1.97.1-compatible graph, did not change `Cargo.lock`, and retained its hash:

```text
0a8004d2207f919cb96bb1bbccdd5bca64616b669c99d187fd4eb1ba4e800648  Cargo.lock
```

Metadata confirms resolver 3, Edition 2024, Rust 1.97.1, the three intended
workspace members, direct caret requirements, and the unchanged adapter direction.
The direct tree is:

```text
spielorders-core -> focaccia 2.0.0, serde 1.0.229,
                    serde_json 1.0.151, TOML 1.1.4
spielorders-cli -> spielorders-core
spielorders-desktop -> spielorders-core, Tauri 2.11.5
spielorders-desktop build -> tauri-build 2.6.3
```

Live `cargo search <crate> --limit 1` queries against crates.io found each direct
release current in its approved generation: focaccia 2.0.0, serde 1.0.229,
serde_json 1.0.151, TOML `1.1.4+spec-1.1.0`, Tauri 2.11.5, and tauri-build
2.6.3. The duplicate inventory contains only maintained generations selected by
the Tauri all-target graph; no override was added.

Cargo reported only four non-updatable older-release notices, not compatible
drift: generic-array 0.14.7 through Tauri's SHA-2/code-generation chain, and TOML
0.8.2, toml_datetime 0.6.3, and toml_edit 0.20.2 through Tauri's all-target GTK
`system-deps` and proc-macro chains. Cargo selected none of their advertised newer
releases under the Rust-1.97.1-compatible graph.

### npm graph, vulnerability, selector, and release-drift inventory

The exact inspections were:

```sh
npm ls --all
npm --prefix apps/desktop ls --all
npm outdated --all --json
npm --prefix apps/desktop outdated --all --json
npm audit --audit-level=low
npm --prefix apps/desktop audit --audit-level=low
npm install-scripts ls
npm --prefix apps/desktop install-scripts ls
npm explain fast-uri
npm --prefix apps/desktop explain \
  @types/react @types/react-dom @types/node @vitejs/plugin-react vite \
  postcss rolldown @oxc-project/types @emnapi/core @emnapi/runtime \
  nanoid undici-types
```

Both installed graphs are valid and both explicit audits report zero
vulnerabilities. Official npm registry queries found this complete direct
inventory:

```sh
for package_name in \
  npm typescript ajv @tauri-apps/api react react-dom \
  @tauri-apps/cli @types/node @types/react @types/react-dom \
  @vitejs/plugin-react vite
do
  printf '%s ' "$package_name"
  npm view "$package_name" version
done
npm view @types/react time --json
npm view @types/react-dom time --json
npm view fast-uri time --json
npm view @napi-rs/wasm-runtime time --json
```

| Direct selection | Selected | Registry current | Disposition |
| --- | --- | --- | --- |
| npm | 12.0.2 | 12.0.2 | Current |
| TypeScript | 7.0.2 | 7.0.2 | Current |
| AJV | 8.20.0 | 8.20.0 | Current |
| Tauri API | 2.11.1 | 2.11.1 | Current |
| React / ReactDOM | 19.2.8 | 19.2.8 | Current |
| Tauri CLI | 2.11.4 | 2.11.4 | Current |
| Node types | 26.1.2 | 26.1.2 | Current |
| React types | 19.2.17 | 19.2.18 | Frozen post-snapshot drift |
| ReactDOM types | 19.2.3 | 19.2.4 | Frozen post-snapshot drift |
| official Vite React plugin | 6.0.5 | 6.0.5 | Current |
| Vite | 8.2.0 | 8.2.0 | Current |

The complete newer compatible release inventory after the frozen snapshot is:

| Selection | Selected | Current | Published | Selecting package and compatibility |
| --- | --- | --- | --- | --- |
| `@types/react` | 19.2.17 | 19.2.18 | 2026-07-30T21:54:03Z | Direct exact selection; also satisfies `@types/react-dom`'s `^19.2.0` peer |
| `@types/react-dom` | 19.2.3 | 19.2.4 | 2026-07-30T21:53:05Z | Direct exact selection; patch within approved major 19 |
| `fast-uri` | 3.1.4 | 3.1.5 | 2026-07-31T09:16:56Z | AJV 8.20.0 selects `^3.0.1`; 3.1.5 is compatible wanted drift |
| `@napi-rs/wasm-runtime` | 1.2.1 | 1.2.2 | 2026-07-31T08:21:54Z | optional `@rolldown/binding-wasm32-wasi@1.2.1` selects `^1.2.0`; 1.2.2 is compatible wanted drift |

All four releases postdate frozen snapshot
`67dc5e4560df173b902b24d3463bb01ff46de23b` and are recorded without
installation under the user-approved stabilization window. The complete
toolchain, direct-dependency, and compatible-transitive refresh remains mandatory
at M2 closeout.

Other behind-latest entries are selector-owned and are not compatible updates:
PostCSS 8.5.25 selects Nano ID `^3.3.16`, resolving 3.3.16 rather than
different-major 6.0.0; Node types 26.1.2 select `undici-types` `~8.3.0`,
resolving 8.3.0 rather than 8.9.0; and Rolldown's optional WASM binding selects
`@emnapi/core` and `@emnapi/runtime` exactly at 2.0.0-alpha.3 while npm's
`latest` tag names 1.11.3. Outdated entries without a current version are
uninstalled optional platform packages, preprocessors, plugins, and tools whose
wanted versions equal current registry releases. No override or speculative
dependency was added.

### Independent recounts, preservation, behavior, and scope

Independent filesystem, manifest, and source recounts found:

- 62 Rust tests: 18 metadata codecs, 8 Order validation, 8 Package/Vendor
  validation, 12 safe paths, 8 single-file replacement, 6 multi-file replacement,
  and 2 core unit/boundary tests;
- 4 version-1 schemas and 8 accepted examples;
- 19 rejected-manifest entries naming 19 unique existing fixture files;
- 22 scenarios with 22 unique names; and
- 2 opaque binary sentinels.

The independent recount and scope commands were:

```sh
rg -c '#\[test\]' crates/spielorders-core/src crates/spielorders-core/tests
rg '#\[test\]' crates/spielorders-core/src crates/spielorders-core/tests | wc -l
find contracts/schemas/v1 -type f -name '*.schema.json'
find contracts/examples/v1 -type f -name '*.json'
find contracts/fixtures/schema-invalid -type f -name '*.json' \
  ! -name manifest.json
find contracts/fixtures/assets -type f -name '*.bin'
node -e 'const fs=require("node:fs"); const path=require("node:path");
const m=JSON.parse(fs.readFileSync("contracts/fixtures/schema-invalid/manifest.json"));
const s=JSON.parse(fs.readFileSync("contracts/fixtures/scenarios.json")).scenarios;
console.log(m.length, new Set(m.map(x=>x.file)).size,
  m.filter(x=>fs.existsSync(path.join("contracts/fixtures/schema-invalid",x.file))).length,
  s.length, new Set(s.map(x=>x.name)).size);'
shasum -a 256 contracts/fixtures/assets/supporting-a.bin \
  contracts/fixtures/assets/supporting-b.bin
git diff --name-status \
  80e5527034615d6b71fce63ea40b55437bff9492...\
ab02ed5ca2d0e0ed0534859f31d19f488218090c -- \
  contracts crates/spielorders-core/src crates/spielorders-core/tests \
  apps/desktop/src apps/desktop/src-tauri/src
git diff --name-only \
  67dc5e4560df173b902b24d3463bb01ff46de23b...\
ab02ed5ca2d0e0ed0534859f31d19f488218090c
rg -n '"(ajv|vite|@vitejs/plugin-react|@cfworker/json-schema|@mantine/[^" ]*|mantine|esbuild)"|allowScripts|trustedDependencies' \
  package.json apps/desktop/package.json package-lock.json \
  apps/desktop/package-lock.json
rg -n --glob '!**/node_modules/**' --glob '!target/**' \
  '(PackageLoader|DamagedPackage|DamagedOrder|DiagnosticCatalog|rebuild_index|load_index|@mantine|@cfworker/json-schema)' \
  crates apps scripts contracts
rg -n --glob Cargo.toml --glob package.json \
  '(SpielGantt|spielgantt|spiel-gantt)' .
```

The schema and Rust suites agree on every accepted and rejected fixture.
Metadata-version-1 parsing, strict configuration, exact deterministic two-space
JSON bytes, stable field order, omitted absent optionals, present collections, and
one trailing newline all pass through the 18-test public metadata suite. The
domain suites pass identity, date, status, numeric, total, normalization,
Vendor-reference, duplicate, and reusable-value behavior. The 12-test path suite
passes configured topology, lexical/root confinement, reserved paths, nested
Orders, platform forms, symlink escape, and no-follow classification.

The 8-test single-file suite passes real atomic replacement and every recorded
staging-write, validation-read, candidate-validation, durability, replacement,
cleanup, and abandoned-evidence condition. The 6-test multi-file suite passes
complete-plan validation, exact/aliased destination rejection, caller order,
prior-byte retention, reverse rollback, and explicit multiple rollback failures.
The same public behavior evidence therefore covers configuration,
metadata/domain validation, path safety, atomic replacement, and rollback without
repair or reinterpretation.

The sentinel hashes independently match the external M1 validation record before
and after every gate:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

The complete upgrade range changes no schema, fixture, core production source, or
core public test. Its only frontend source path is the exact CSS declaration
required by TypeScript 7. Every path after the frozen snapshot is documentation.
AJV remains the strict Draft 2020-12 validator; Vite and the official React plugin
remain the frontend build/development layer. Esbuild appears only as an uninstalled
optional Vite peer. Searches and manifest inspection find no
`@cfworker/json-schema`, Mantine, direct esbuild replacement, install-script
authority, Package loader, damage/index/diagnostic M2 behavior, M3 behavior, or
shared SpielGantt dependency.

Core's metadata and Cargo trees prove it remains independent of CLI, Tauri, React,
TypeScript, Vite, and every adapter. The claim diff and complete gate change no
runtime, dependency, manifest, lockfile, source, test, schema, fixture, product
behavior, metadata version, M2/M3 behavior, or Supporting Material byte. Every
mandatory non-currency criterion passes, and the frozen-snapshot disposition is
applied only to the four compatible-release observations above.

## Fixed-point review and disposition

The project-local code-review workflow compared
`git diff ab02ed5ca2d0e0ed0534859f31d19f488218090c...HEAD` after the
claim and gate-evidence commits. Standards and Spec reviewers ran independently
and in parallel.

### Standards

The reviewer reported one hard documentation-consistency finding and zero
judgment-call smells. The gate evidence changed the result to passed while the
milestone index still recorded only an in-progress claim, contrary to
`docs/development/traceability.md`'s requirement to synchronize results. The
closing milestone status, progress, narrative, and table now record the passing
reviewed gate, resolving the finding.

### Spec

The reviewer reported three findings and no scope creep:

1. The original M1 offline clean-install commands had not been rerun. Both exact
   offline commands now pass with zero vulnerabilities and unchanged hashes. The
   strict TypeScript check, schema suite, and frontend tests also pass after that
   registry-free recreation.
2. Several evidence passages summarized commands or used placeholders. The exact
   TypeScript import/selection checks, language-service invocation and requests,
   Cargo selectors and release loop, npm release and publication queries,
   independent recounts, hashes, range checks, and prohibited-scope searches are
   now recorded above.
3. The compatible `@napi-rs/wasm-runtime` drift named its selecting family but not
   the exact package. The inventory now names
   `@rolldown/binding-wasm32-wasi@1.2.1` and its `^1.2.0` selector.

Every review finding is resolved; no waiver or deferred review action remains.
After remediation, `npm clean-install --offline` and its desktop-prefixed
counterpart, strict TypeScript checking, schema contracts, both frontend tests,
`npm run check:traceability`, current-tree and fixed-point `git diff --check`, and
source/clone status checks pass. The four dependency-file hashes and both sentinel
hashes remain unchanged. No dependency or executable product artifact changed.

## User-approved stabilization disposition — 2026-07-31

After repeated compatible releases landed during the time required to close
M1-upgrade, the user directed that this release drift be deferred to M2 closeout.
The exact completed-upgrade snapshot
`67dc5e4560df173b902b24d3463bb01ff46de23b` is therefore frozen through the
remaining M1-upgrade gate and fresh external validation.

The 2026-07-30 `@types/react` 19.2.18 and `@types/react-dom` 19.2.4 findings
remain valid authoritative observations, but no longer block M1-upgrade and must
not be installed by
[Integrate and verify the M1-upgrade acceptance gate](tickets/004-m1-upgrade-acceptance-gate.md).
Future compatible drift found by the gate or external validator is recorded rather
than repaired. M2 closeout owns one complete authoritative refresh of toolchains,
direct dependencies, and compatible locked transitives under `AGENTS.md`. New
majors still require user approval. Actual reproducibility, vulnerability, build,
test, or behavior failures remain blocking and are not waived by this disposition.

No dependency, manifest, lockfile, source, test, schema, fixture, or Supporting
Material file changed to record this policy. The latest failed attempt below
remains historical evidence; none of its unperformed checks is converted into a
pass. The complete fresh target-pinned rerun for
[Integrate and verify the M1-upgrade acceptance
gate](tickets/004-m1-upgrade-acceptance-gate.md) is recorded above.

## Latest failed gate attempt — 2026-07-30

### Blocking currentness failure and ownership

The documentation-only gate cannot close. The first authoritative direct-release
inventory from the fresh target-pinned clone found two compatible patch releases
newer than the completed target:

| Selection | Target | Authoritative current release | Owner |
| --- | --- | --- | --- |
| `@types/react` 19 | 19.2.17 | 19.2.18 | Ticket 002 |
| `@types/react-dom` 19 | 19.2.3 | 19.2.4 | Ticket 002 |

The exact successful registry command was:

```sh
zsh -f -c '
  source /Users/example/.nvm/nvm.sh
  nvm use --silent
  for package_name in \
    npm typescript ajv @tauri-apps/api react react-dom \
    @tauri-apps/cli @types/node @types/react @types/react-dom \
    @vitejs/plugin-react vite
  do
    printf "%s " "$package_name"
    npm view "$package_name" version
  done
'
```

The command queried the official npm registry and returned:

```text
npm 12.0.2
typescript 7.0.2
ajv 8.20.0
@tauri-apps/api 2.11.1
react 19.2.8
react-dom 19.2.8
@tauri-apps/cli 2.11.4
@types/node 26.1.2
@types/react 19.2.18
@types/react-dom 19.2.4
@vitejs/plugin-react 6.0.5
vite 8.2.0
```

The selected Node, npm, TypeScript, AJV, Tauri, React, Node-type, Vite-plugin,
and Vite releases remain current. The two React type-package selections do not.
Both are patch updates within the already approved major 19, so `AGENTS.md`
requires them. [Move repository tooling to Node 26 and npm
12](tickets/002-node-npm-modernization.md) is the smallest owner of the direct npm
baselines, npm graph regeneration, and complete Node/npm-focused reverification.

Ticket 004 changed no runtime, dependency, manifest, lockfile, production code,
test, schema, fixture, or product behavior. Ticket 002 is reopened, unblocked, and
unclaimed. Ticket 004 is open and blocked by it. Tickets 001 and 003 remain
closed. No code review or closing disposition was attempted after the mandatory
currentness failure.

### Environment and fresh isolated target

The source worktree was clean at the required target before claim. Ticket 004's
fixed point is exactly `67dc5e4560df173b902b24d3463bb01ff46de23b`.
Commit `abfecb2b3dd32359347681fc0673fd5339bc4cfa` contains only the Ticket
004 and milestone claim state.

Validation ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Git 2.50.1 (Apple Git-155), Rust/Cargo 1.97.1, LLVM 22.1.6, selected Node
26.5.1, and selected npm 12.0.2. TypeScript was correctly absent before the clean
install that the failed gate did not reach.

A new local clone was created with:

```sh
git clone --no-hardlinks \
  /Users/example/Code/Apps/SpielOrders \
  /private/tmp/spielorders-m1-upgrade-gate.uJk9AY
git -C /private/tmp/spielorders-m1-upgrade-gate.uJk9AY \
  checkout --detach abfecb2b3dd32359347681fc0673fd5339bc4cfa
git -C /private/tmp/spielorders-m1-upgrade-gate.uJk9AY rev-parse HEAD
git -C /private/tmp/spielorders-m1-upgrade-gate.uJk9AY \
  status --porcelain=v1 --untracked-files=all
find /private/tmp/spielorders-m1-upgrade-gate.uJk9AY -type d \
  \( -name target -o -name node_modules -o -name dist \) -prune -print
```

The detached HEAD was exact, status was empty, and `find` printed nothing. No
evidence or artifact from either prior failed gate was reused.

A fresh `zsh -f` shell sourced the repository-selected NVM runtime and reported:

```text
node=v26.5.1
npm=12.0.2
node_path=/Users/example/.nvm/versions/node/v26.5.1/bin/node
npm_path=/Users/example/.nvm/versions/node/v26.5.1/bin/npm
```

The same pre-install diagnostic reported no local TypeScript executable. The
repository toolchain reported Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`), Cargo 1.97.1
(`c980f4866141969fab6254a680546a277789d6f0`), and LLVM 22.1.6.

### Authoritative release inventory completed before the failure

The authoritative sources inspected were the Rust Release Team's
<https://blog.rust-lang.org/releases/latest/> endpoint, Node's official
<https://nodejs.org/dist/index.json> release index, the official npm registry at
<https://registry.npmjs.org/>, and crates.io through `cargo search`. Node's index
reported 26.5.1, dated 2026-07-28 and marked as a security release. The Rust
latest-release endpoint was opened, and the selected Rust/Cargo diagnostics were
recorded, but the final redirected announcement was not fetched before the
mandatory npm failure stopped the gate.

The direct Cargo release query completed in the same approved live-registry
invocation:

```sh
for crate_name in focaccia serde serde_json toml tauri tauri-build
do
  cargo search "$crate_name" --limit 1
done
```

crates.io reported every direct Cargo release current in its approved generation:
focaccia 2.0.0, serde 1.0.229, serde_json 1.0.151, TOML
`1.1.4+spec-1.1.0`, Tauri 2.11.5, and tauri-build 2.6.3.

The first sandboxed `npm view` attempt could not reach the registry and was
interrupted without a result. The approved live-registry rerun above completed
successfully. This was an environment-access observation, not a repository
failure.

### Deliberately unperformed gate work

Ticket 004 stopped immediately after the compatible direct npm updates were
identified. It did not run clean installs; compute post-install manifest or
lockfile hashes; run Cargo's update dry run, metadata, tree, duplicate, or
compatible-transitive inspections; run npm graph, outdated, audit, explain,
selector, or install-script-authority inspections; or complete the independent
Rust-currentness redirect.

It also did not run the Cargo build, Rust tests, Clippy, TypeScript no-emit or
compiler-API/language-service checks, schema/traceability commands, frontend
tests/build, Tauri build/development smoke, independent count/hash recounts, or
the final metadata-version-1, deterministic-byte, configuration, domain,
path-safety, atomic-replacement, rollback, boundary, and prohibited-scope audits.
No prior result or unperformed requirement is represented as passing.

No dependency, production, test, schema, fixture, metadata-version-1, product,
Package, CLI, Tauri, React, adapter, M2/M3, shared SpielGantt, or opaque
Supporting Material file was changed by this failed attempt.

## Prior failed gate attempt — 2026-07-30

### Blocking currentness failures and ownership

The documentation-only gate cannot close. The first authoritative currentness
checks from the fresh target-pinned clone found compatible releases newer than
the selected target:

| Selection | Target | Authoritative current release | Owner |
| --- | --- | --- | --- |
| Cargo locked transitive `http` | 1.4.2 | 1.5.0 | Ticket 001 |
| Node 26 | 26.5.0 | 26.5.1 | Ticket 002 |
| npm 12 | 12.0.1 | 12.0.2 | Ticket 002 |
| `@types/node` 26 | 26.1.1 | 26.1.2 | Ticket 002 |
| `@vitejs/plugin-react` 6 | 6.0.4 | 6.0.5 | Ticket 002 |
| Vite 8 | 8.1.5 | 8.2.0 | Ticket 002 |
| npm locked transitive PostCSS | 8.5.23 | 8.5.25 | Ticket 002 |

`cargo update --dry-run --verbose` refreshed the crates.io index and reported:

```text
Locking 1 package to latest Rust 1.97.1 compatible version
Updating http v1.4.2 -> v1.5.0
warning: not updating lockfile due to dry run
```

The lockfile remained byte-identical. `cargo tree --invert http@1.4.2 --locked`
traces the selected version through current direct Tauri 2.11.5 and
tauri-build 2.6.3: `tauri`, `tauri-runtime`, `tauri-runtime-wry`, both
target-specific `tauri-utils` resolutions, `tauri-codegen`, `tauri-macros`, and
Wry all select it. Ticket 001 owns ordinary Cargo graph regeneration and the
complete Rust-focused reverification.

Node's official release index begins with Node 26.5.1, dated 2026-07-28 and marked
as a security release. Official npm registry queries report npm 12.0.2,
`@types/node` 26.1.2, `@vitejs/plugin-react` 6.0.5, and Vite 8.2.0. These are
minor or patch updates within approved selections, so `AGENTS.md` requires them.
`npm --prefix apps/desktop outdated --all --json` also reports PostCSS 8.5.23
with wanted/latest 8.5.25. `npm explain` traces PostCSS to Vite 8.1.5's
`^8.5.17` selector. Ticket 002 owns the Node/npm selections, direct npm
baselines, npm graph regeneration, and complete Node/npm-focused
reverification.

Ticket 004 changed no runtime, dependency, manifest, lockfile, production code,
test, schema, fixture, or product behavior. Tickets 001 and 002 are reopened,
unblocked, and unclaimed. Ticket 004 is open and blocked by both. Ticket 003
remains closed because TypeScript 7.0.2 is still current. No code-review or
closing disposition was attempted after the gate failed.

### Environment and fresh isolated target

The source worktree was clean at the required target before claim. Ticket 004's
fixed point is exactly `5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f`;
commit `d4d181442023ef53b117a8f783a0f08c98a3863a` contains only the claim.

Validation ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Git 2.50.1 (Apple Git-155), Rust/Cargo 1.97.1, LLVM 22.1.6, selected Node
26.5.0, selected npm 12.0.1, and selected TypeScript 7.0.2.

A new local clone was created with:

```sh
git clone --no-hardlinks \
  /Users/example/Code/Apps/SpielOrders \
  /private/tmp/spielorders-m1-upgrade-gate.qvv1sA
git checkout --detach d4d181442023ef53b117a8f783a0f08c98a3863a
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
find . -type d \( -name target -o -name node_modules -o -name dist \) \
  -prune -print
```

The detached HEAD was exact, status was empty, and `find` printed nothing.
No result from the 2026-07-27 clone or executable attempt was reused.

### Authoritative release inventory

The exact successful currentness commands and sources were:

```sh
curl -s https://blog.rust-lang.org/releases/latest/
curl -s https://nodejs.org/dist/index.json
cargo search <crate> --limit 1
npm view <package> version
cargo update --dry-run --verbose
npm outdated --all --json
npm --prefix apps/desktop outdated --all --json
```

The Rust Release Team endpoint resolves to its Rust 1.97.1 announcement. Rust
1.97.1 and Cargo 1.97.1 therefore remain current. The Node index reports 26.5.1,
and the registry reports npm 12.0.2 and TypeScript 7.0.2.

All direct Cargo releases remain current in their approved generations:
focaccia 2.0.0, serde 1.0.229, serde_json 1.0.151, TOML
`1.1.4+spec-1.1.0`, Tauri 2.11.5, and tauri-build 2.6.3.

The complete direct npm registry inventory is:

```text
npm 12.0.2
typescript 7.0.2
ajv 8.20.0
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

AJV, both Tauri npm packages, React/ReactDOM, React type packages, and
TypeScript remain current. The five selected Node/npm/direct npm entries named
in the failure table do not.

### npm graph and vulnerability inspections

One root and desktop `npm clean-install` round recreated the selected graphs in
the fresh clone: 5 root packages and 30 desktop packages, zero vulnerabilities.
The desktop install repeated the known observation that optional `fsevents`
2.3.3 has an unapproved install script; no script authority was added.

The exact inspections were:

```sh
npm ls --all
npm --prefix apps/desktop ls --all
npm outdated --all --json
npm --prefix apps/desktop outdated --all --json
npm audit --audit-level=low
npm --prefix apps/desktop audit --audit-level=low
npm --prefix apps/desktop explain \
  @types/node @vitejs/plugin-react vite postcss
```

Both installed graphs are internally valid and both audits report zero known
vulnerabilities. Root outdated inspection reports only AJV-selected `fast-uri`
3.1.4 behind the different-major 4.1.1. Desktop inspection reports the direct
updates above, PostCSS's compatible locked update, and the already recorded
behind-latest selector families: Vite/Rolldown platform bindings and
`@oxc-project/types`, optional uninstalled Vite peers/tools, Node-types-selected
`undici-types`, and PostCSS-selected Nano ID. No override was added.

The installed TypeScript package is direct CLI-only in `npm ls --all`.
Because currentness had already failed, the complete repository and
installed-source compiler-API absence search was not performed and is not
represented as passing.

### Resolver, graph, ancestry, hashes, and scope evidence

`cargo metadata --locked --format-version 1 --no-deps` confirms Edition 2024,
Rust 1.97.1, the three intended workspace members, direct caret requirements,
and the unchanged core/adapter boundary. Root-manifest inspection confirms
resolver 3. `cargo tree --workspace --depth 1 --locked` reports:

```text
spielorders-cli -> spielorders-core
spielorders-core -> focaccia, serde, serde_json, toml
spielorders-desktop -> spielorders-core, tauri
spielorders-desktop build -> tauri-build
```

`cargo tree --duplicates --locked` records only maintained transitive duplicate
generations selected by the Tauri graph; the gate added or overrode none.

The exact repository checks were:

```sh
git merge-base --is-ancestor \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f
git merge-base \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f
git rev-list --count \
  80e5527034615d6b71fce63ea40b55437bff9492..\
5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f
git diff --check \
  80e5527034615d6b71fce63ea40b55437bff9492...\
d4d181442023ef53b117a8f783a0f08c98a3863a
git diff --name-status \
  5ebe97a717e67d155e5d5a6d08bd1db43dd8aa2f...\
d4d181442023ef53b117a8f783a0f08c98a3863a
git status --porcelain=v1 --untracked-files=all
```

The fixed point is the exact merge base of the 17-commit completed-upgrade
range. The claim commit's parent is exactly the completed target and changes
only Ticket 004. Diff checking and clone/source status passed.

The dry runs and inspections left every dependency file unchanged:

```text
b46f20711c8c81d150ea6faee159eb0e8e304b418ccfd77ad0fde424b2e2c314  Cargo.lock
35720687e78a606321e7279060c632c21b729ec8f33031e218d2ddd088e95290  package.json
e49138bb4c894e69ca42a71bbf128e2eea38fe45ccff2a8bad96e45530ebdda8  package-lock.json
669656be425097c0a9f7a4efc383dae83ebaf210ecf03a836f8061fd5ca4f6e7  apps/desktop/package.json
486ad3f833aba83f593267ae30d1f5d31878b2ef58c859df6e964a35f05e54e9  apps/desktop/package-lock.json
```

The isolated-copy Supporting Material hashes independently match the external M1
validation record:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

Manifest and installed-graph inspection confirms AJV and Vite remain selected.
There is no `@cfworker/json-schema`, project-owned esbuild replacement, Mantine,
M2/M3 behavior, or shared SpielGantt crate in the claim diff.

### Deliberately unperformed gate work

The gate stopped at mandatory compatible updates, exactly as Ticket 004 requires.
It did not run a second clean-install round, the Cargo clean build, Rust tests,
Clippy, TypeScript no-emit check, schema/traceability commands, frontend
tests/build, Tauri build/development smoke, language-service check, independent
Rust/schema/fixture/scenario recounts, or the final metadata-version-1,
deterministic-byte, path-safety, atomic-replacement, rollback, and scope audits.
No prior result or unperformed requirement is represented as passing.

No dependency, production, test, schema, fixture, metadata-version-1, product,
Package, CLI, Tauri, React, adapter, M2/M3, shared SpielGantt, or opaque
Supporting Material file was changed by this failed attempt.

## Earlier failed gate attempt — 2026-07-27

Status: **failed; gate open; returned to Ticket 001**

Gate date: 2026-07-27

Original pre-upgrade fixed point:
`80e5527034615d6b71fce63ea40b55437bff9492`

Completed Ticket 003 target and Ticket 004 start fixed point:
`b725ad7bce6b8722dc02f060c39d1d9c8505d3ee`

Isolated gate target, including only the Ticket 004 claim documentation:
`86cf0cdbb873761cbe0679d77ea3fddf55333d95`

## Blocking currentness failure

The gate cannot close. From the target-pinned copy,
`cargo update --dry-run --verbose` refreshed the authoritative crates.io index and
reported three compatible locked updates:

```text
Updating schemars v1.2.1 -> v1.2.2
Updating toml_parser v1.1.2+spec-1.1.0 -> v1.1.3+spec-1.1.0
Updating tray-icon v0.24.1 -> v0.24.2
warning: not updating lockfile due to dry run
```

This violates `AGENTS.md`, Ticket 001's requirement that Cargo's update dry run
report no uncommitted compatible update, and Ticket 004's currentness acceptance
criterion. Ticket 004 is documentation-only and did not update `Cargo.lock` or any
dependency. The failure belongs to
[Ticket 001 — Modernize Rust workspace resolution and direct dependency baselines](tickets/001-rust-cargo-modernization.md),
which owns Rust lock-graph regeneration and its clean locked verification.

Lockfile and graph inspection traces the updates without overriding them:

- `schemars` 1.2.1 is a lockfile dependency of current `serde_with` 3.21.0 in
  Tauri's all-target graph.
- `toml_parser` 1.1.2 is selected both by direct TOML 1.1.3 and by Tauri's
  `cargo_toml`/TOML and utility chains.
- `tray-icon` 0.24.1 is selected by current direct Tauri 2.11.5.

The dry run made no lockfile change. Ticket 001 has been reopened and Ticket 004
remains open and blocked. No code-review run or closing disposition was attempted
because the executable gate did not pass.

## Environment and isolated target

The source worktree was clean at the exact expected Ticket 003 baseline before
claim. The pre-upgrade fixed point is both an ancestor and the exact merge base.
The complete intervening history was inspected before Ticket 004 was claimed.

Validation ran on macOS 26.5.2 (build 25F84), Darwin 25.5.0, Apple Silicon, with
Git 2.50.1 (Apple Git-155). A fresh `zsh -f` shell selected:

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
node v26.5.0
npm 12.0.1
TypeScript 7.0.2
```

A local `git clone --no-hardlinks` was detached at
`86cf0cdbb873761cbe0679d77ea3fddf55333d95` in
`/private/tmp/spielorders-m1-upgrade-gate.TFMrVE`. Before installation it was
source-clean and contained no `target`, `node_modules`, or `dist` directory. The
source worktree was not used for builds or dependency recreation and remained
clean.

The exact history checks passed:

```sh
git status --porcelain=v1 --untracked-files=all
git merge-base --is-ancestor \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  86cf0cdbb873761cbe0679d77ea3fddf55333d95
git merge-base \
  80e5527034615d6b71fce63ea40b55437bff9492 \
  86cf0cdbb873761cbe0679d77ea3fddf55333d95
git log \
  80e5527034615d6b71fce63ea40b55437bff9492..\
86cf0cdbb873761cbe0679d77ea3fddf55333d95 --oneline
git diff --check \
  80e5527034615d6b71fce63ea40b55437bff9492...\
86cf0cdbb873761cbe0679d77ea3fddf55333d95
```

The merge base was exactly
`80e5527034615d6b71fce63ea40b55437bff9492`. The completed upgrade implementation
range is
`80e5527034615d6b71fce63ea40b55437bff9492..b725ad7bce6b8722dc02f060c39d1d9c8505d3ee`;
the isolated range adds only the Ticket 004 claim commit.

## Authoritative release checks

Checks at ticket start used the Rust Release Team's
`https://blog.rust-lang.org/releases/latest/`, Node's official
`https://nodejs.org/dist/index.json`, the official npm registry, and live
`cargo search <crate> --limit 1` queries against crates.io.

They confirmed Rust/Cargo 1.97.1, Node 26.5.0, npm 12.0.1, and TypeScript 7.0.2.
Every direct dependency is current in its approved major or release channel:

- Rust: focaccia 2.0.0, serde 1.0.229, serde_json 1.0.151, TOML
  1.1.3+spec-1.1.0, Tauri 2.11.5, and tauri-build 2.6.3.
- npm: AJV 8.20.0, Tauri API 2.11.1, React and ReactDOM 19.2.8, Tauri CLI
  2.11.4, Node types 26.1.1, React types 19.2.17, ReactDOM types 19.2.3,
  the official Vite React plugin 6.0.4, TypeScript 7.0.2, and Vite 8.1.5.

Cargo resolver 3 remains selected. These direct selections pass currentness, but
the compatible lock-graph updates above prevent the complete gate from passing.

## Reproducibility and executable results before the failure

Two consecutive root and desktop `npm clean-install` rounds passed. Explicit
`npm audit --audit-level=low` commands for both roots reported zero known
vulnerabilities. The manifest and lockfile hashes were identical before, between,
and after:

```text
35720687e78a606321e7279060c632c21b729ec8f33031e218d2ddd088e95290  package.json
e49138bb4c894e69ca42a71bbf128e2eea38fe45ccff2a8bad96e45530ebdda8  package-lock.json
669656be425097c0a9f7a4efc383dae83ebaf210ecf03a836f8061fd5ca4f6e7  apps/desktop/package.json
486ad3f833aba83f593267ae30d1f5d31878b2ef58c859df6e964a35f05e54e9  apps/desktop/package-lock.json
```

npm again reported only the known blocked optional `fsevents` 2.3.3 install
script. No script authority was added.

The following commands passed from the isolated copy before currentness failed:

- `cargo build --workspace --locked`;
- `cargo test --workspace --locked`;
- `cargo test -p spielorders-core --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `apps/desktop/node_modules/.bin/tsc --noEmit --project
  apps/desktop/tsconfig.json`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `npm --prefix apps/desktop run test`;
- `npm --prefix apps/desktop run build`; and
- `npm --prefix apps/desktop run tauri build -- --no-bundle`.

The Rust runs reported 62 tests: 2 core unit/boundary tests, 18 metadata-codec
tests, 8 Order-validation tests, 8 Package/Vendor-validation tests, 12 safe-path
tests, 8 single-file replacement tests, and 6 multi-file replacement tests.
The schema command reported 4 schemas, 8 accepted examples, 19 rejected fixtures,
22 scenarios, and 2 preservation sentinels. These are command results, not the
independent recount required to close Ticket 004; the gate stopped before that
remaining audit.

The bounded
`npm --prefix apps/desktop run tauri dev -- --no-watch` smoke also passed. Vite
8.1.5 became ready at the configured `http://localhost:1420/` URL in 90 ms, an
independent request returned HTTP 200, and process inspection showed both Vite and
`target/debug/spielorders-desktop` alive. The shell had no Package argument or
Package-loading failure. Ctrl-C bounded the run, and no matching process remained.

## Dependency observations and incomplete work

`npm outdated --all --json` found no behind-latest direct dependency. It reported
the same transitive selectors recorded by Ticket 003: AJV selects `fast-uri`
3.1.4 within `^3.0.1`; Vite selects Rolldown 1.1.5 and its 1.1.5 platform
bindings, Rolldown selects `@oxc-project/types` 0.139.0, Vite optionally peers on
uninstalled `@vitejs/devtools` 0.3.4, PostCSS selects Nano ID 3.3.16 within
major 3, and Node types select `undici-types` 8.3.0 within `~8.3.0`.

`cargo tree --workspace --depth 1` still reports:

```text
spielorders-cli -> spielorders-core
spielorders-core -> focaccia, serde, serde_json, toml
spielorders-desktop -> spielorders-core, tauri
spielorders-desktop build -> tauri-build
```

Because the Cargo currentness gate failed, Ticket 004 stopped before completing
the independent fixture/sentinel recount, compiler-API installed-graph search,
TypeScript language-service check, final preservation/scope audits, acceptance
evidence review, and post-review reruns. No unperformed requirement is represented
as passing. The prior externally validated sentinel values remain the required
comparison values for the eventual rerun:

```text
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

No dependency, production, test, schema, fixture, metadata-version-1, product,
Package, CLI, Tauri, React, adapter, M2/M3, shared SpielGantt, or opaque Supporting
Material file was changed by this failed gate.
