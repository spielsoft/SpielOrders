# M2 external validation

Status: **passed; externally validated**

Validation date: 2026-08-21

## Fixed points and repository guardrails

- Original M2 fixed point:
  `866f619414e57aa816184509ed8e57209c8ea62f`
- External-validation reopen fixed point:
  `a97de65bd09e249991ab7969305ce7663ce3719e`
- Reclosure transition:
  `e280145ae8eff2b03a856562aa2338b0cebd263f`
- Exact externally validated target:
  `18101a3c419d6794f7768b99820ac2d1976a836d`

The source worktree was clean and `HEAD` resolved exactly to the completed
target before validation. Both fixed points are ancestors. The integrated
acceptance capability and all six M2 implementation capabilities were closed,
M2 was reclosed awaiting this validation, and M3 remained blocked. Both complete
and reopened-range `git diff --check` checks passed.

The reopened range changes only the M2 acceptance record, milestone index,
integrated acceptance capability, and corresponding roadmap documentation. It
contains the isolated claim, correction/currentness, pre-review evidence,
remediation, disposition, closure, and validation-handoff corrections recorded
by the milestone. No source, test, script, manifest, lockfile, contract, fixture,
toolchain selector, dependency, or M3 file changed in the reopened range.

## Fresh isolated target

All executable validation ran in the new no-hardlink clone
`/private/tmp/spielorders-m2-external.yY7LA8/repo`, detached exactly at
`18101a3c419d6794f7768b99820ac2d1976a836d`. Before recreation, its status was
empty and it contained no inherited `target`, `node_modules`, or desktop `dist`
tree.

The baseline and final integrity hashes matched exactly:

```text
85ba2007355e9c58bf5151ccb28265c4de64ac0b5c05966cd40df1045e507a79  Cargo.lock
c6d7aa1008111da085594e59ba06dd154eb6cd03892242fd6652ea912afed74f  package-lock.json
7e27b881eb64c3a84d2aa90de019e6ebda27ea83f3753829374d20c26fdd8773  apps/desktop/package-lock.json
a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9  supporting-a.bin
7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42  supporting-b.bin
```

## Toolchain and dependency currentness

Fresh authoritative checks used Rust's signed stable-channel manifest, Node's
official release index, npm registry packuments, and crates.io. The selected
versions are current within every approved major:

- Rust and Cargo 1.98.0, rustc commit
  `88d9e12ae178fab0fb5cc050a94da85685d449ea`;
- Node 26.7.0, npm 12.0.2, and TypeScript 7.0.2;
- AJV 8.20.0, Tauri API 2.11.1, Tauri CLI 2.11.4, React and ReactDOM
  19.2.8, Node types 26.2.0, React types 19.2.18, ReactDOM types 19.2.4,
  Vite React plugin 6.1.0, and Vite 8.2.2;
- focaccia 2.0.0, serde 1.0.229, serde_json 1.0.151, TOML 1.1.4,
  Tauri 2.11.5, and tauri-build 2.6.3.

`cargo update --dry-run --verbose` locked zero packages. The older locked
generations are selector-owned: `generic-array` 0.14.7 is exact-selected by
`crypto-common`, while TOML 0.8.2, `toml_datetime` 0.6.3, and `toml_edit`
0.20.2 are confined to Tauri target/build branches. Locked metadata and direct,
duplicate, and inverted trees confirm the intended dependency directions.

Both npm graphs are valid and both vulnerability audits report zero findings.
Behind-latest installed entries are selector-owned: AJV selects `fast-uri`
3.1.5 within major 3, PostCSS selects Nano ID 3.3.18 within major 3, Node types
select `undici-types` 8.3.0 through `~8.3.0`, and Vite selects Rolldown 1.2.5
through `~1.2.4`. Other outdated entries are absent platform, peer, or optional
packages whose wanted release equals latest. Install-script inventories report
zero granted or unreviewed authority; no `allowScripts`, trusted dependency,
approval, or equivalent authority exists.

## Dependency recreation and executable gate

Online and offline lockfile recreation passed with scripts disabled:

```sh
npm clean-install --ignore-scripts
npm --prefix apps/desktop clean-install --ignore-scripts
npm clean-install --offline --ignore-scripts
npm --prefix apps/desktop clean-install --offline --ignore-scripts
```

Every required executable gate passed:

| Gate | Result |
| --- | --- |
| `cargo build --workspace --locked` | Passed from an initially absent target |
| `cargo test --workspace --locked` | Passed; 156 Rust test declarations independently reconciled |
| `cargo test -p spielorders-core --locked` | Passed; the same 156 core tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| seven focused M2 Rust test binaries | Passed; 94 declarations independently reconciled |
| `npm run test:schemas` | Passed: 4 schemas, 8 examples, 19 rejected fixtures, 22 scenarios, 2 sentinels |
| `npm run check:traceability` | Passed: 7 M0 tickets and 4 baseline test files |
| `npm --prefix apps/desktop test` | Passed: 2 frontend tests |
| `npm --prefix apps/desktop run build` | Passed: TypeScript and Vite production build, 15 modules |
| `npm --prefix apps/desktop run tauri build -- --no-bundle` | Passed; release native application produced |

The bounded development smoke started Vite 8.2.2 and the native
`spielorders-desktop` process. An independent request to the exact configured
`http://localhost:1420/` URL returned HTTP 200. Ctrl-C stopped the run, and a
final independent process inspection found neither process alive.

## Metadata, behavior, and preservation

Independent fixture recounts found 4 schemas, 8 accepted example documents, 19
unique manifest entries naming 19 existing rejected fixtures, 22 scenarios with
22 unique names, and 2 opaque sentinels. Both version documents contain exactly
`metadata_version: 1`; all example order, vendors, and index documents and both
derived indexes use schema version 1.

The sorted key arrays and all ten expected hashes matched exactly:

| Artifact | Keys | SHA-256 |
| --- | --- | --- |
| minimal version | `metadata_version` | `451b2e02c3c7b2ee4fbfdf6f1cef88c3fd9dbd6583a0b322d1697fcc98a429bd` |
| minimal order | `date_ordered`, `items`, `order_id`, `package_root`, `schema_version`, `status`, `title` | `89cc1bc88c913976b49be4d760ba51018a814dd0ca8e0a54a3d91394a94a1b03` |
| minimal vendors | `schema_version`, `vendors` | `ea8a241b769b598c6e1bc76e049657734af6d36f8ba270d12357390e04f86c32` |
| minimal index | `orders`, `schema_version`, `vendors` | `709c2406a171f06cb83f1aea20ce00d1cb2a84b54dc49b105722f801aaac54e8` |
| full version | `metadata_version` | `451b2e02c3c7b2ee4fbfdf6f1cef88c3fd9dbd6583a0b322d1697fcc98a429bd` |
| full order | `currency`, `date_ordered`, `items`, `order_group`, `order_id`, `orderer`, `ordering_institution`, `package_root`, `project`, `schema_version`, `status`, `title`, `vendor_id` | `53b8e3c14e04ccf902a1d90107f2e3f5caad4d4d5791144f6d1aafe99885124a` |
| full vendors | `schema_version`, `vendors` | `7751ee268f9ec273c174421970a8a5d07c77d903a4a0b9207f5d74c90ebfaaf6` |
| full index | `orders`, `schema_version`, `vendors` | `af542fdddf1e761e798f35ab8bdfff5311d34e451c1d84e8e964caff2f7d97ef` |
| derived minimal | `orders`, `schema_version`, `vendors` | `183f7fb885d22c770bb625094959639b5375cbbb9e9d3d29162e463e873625b6` |
| derived wrong-order-path | `orders`, `schema_version`, `vendors` | `6938457cea735421f2e976f165dc4bd00b267941c8332ad0f6ab42d024fc2dc6` |

Source, effects, and assertions were inspected independently of acceptance prose.
The public suites reproduce all six `PackageLoad` families; complete healthy and
damaged projections; stable diagnostics and duplicate-ID participation;
deterministic Package Index assessment and healthy-only rebuild; exact README
reads; current-index fast-load budgets and deliberately absent evidence;
exhaustive-validation non-retroactivity; replacement and cleanup failure
preservation; and the complete filesystem-port fault matrix.

The current-index fast path reads only its authorized managed evidence and lists
only direct Order candidate membership. Canonical fallback, exhaustive
validation, index replacement, README loading, and every injected failure avoid
recursing through or inspecting Supporting Material. Both opaque sentinel hashes
remained byte-identical across the complete gate.

## Architecture and scope

Manifest, metadata, tree, source, and range inspection confirm:

- Package rules remain in `spielorders-core`;
- the `FileSystem` port remains the sole filesystem effect boundary;
- CLI and Tauri depend on core, never the reverse;
- React contains no M2 domain rule;
- the reopened range is documentation-only; and
- neither range introduces install-script authority, Mantine, a shared
  SpielGantt foundation, another schema or metadata version, prohibited adapter
  behavior, native/search behavior, repair or mutation behavior, retry
  presentation, M3 work, or unrelated scope.

Existing decision-source comments naming SpielGantt introduce no dependency or
shared implementation foundation.

## Independent code review

The project-local code-review workflow compared the complete original M2 range
and separately inspected the reopen range. Standards and Spec reviewers ran
independently and in parallel.

### Standards

Result: **0 hard violations and 0 Fowler judgment-call smells.**

The reviewer checked `AGENTS.md`, architecture and traceability standards, the M2
operating contract, and the complete Fowler smell baseline. The final target's
meaningful capability links, validation-only direction, exact context path, and
install-script denial wording resolved every previously reported documentation
issue. Worst issue: none.

### Spec

Result: **0 missing or partial requirements, 0 scope creep findings, and 0
incorrect implementation or evidence findings.**

The reviewer checked all six implementation capabilities, milestone acceptance,
the M2 operating contract and decisions, and the external-validation handoff.
Worst issue: none.

Lifecycle inspection confirmed the current-index fast-path remediation at
`b4831a2b21bf6756fb2873b8154aac79fdebcdcc`, with review disposition and closure
coinciding at `ecd853b08009b1e1a946050b9594f482949bd3b7`. It also confirmed the
reopened remediation `90e8f25dd001ec9ec140c06e856c62765e4280b7`, disposition
`51a650466d19924b4c03ea512967234380d1984c`, closure
`e280145ae8eff2b03a856562aa2338b0cebd263f`, follow-up `bb8c545...`,
documentation correction `5a53c273...`, review remediation `974ff633...`, and
final target `18101a3...`.

No non-blocking editorial or traceability-polish findings remain at the validated
target.

## Final guards

After the complete gate, the detached clone remained source-clean at the exact
target. Both ancestry checks, both range whitespace checks, the reopened
four-document path inventory, all three lockfile hashes, and both sentinel hashes
matched their baselines. The source worktree also remained clean until this sole
authorized external-validation record and M2 roadmap-status update were applied.

M2 is externally validated. M3 remains blocked and no M3 work began.
