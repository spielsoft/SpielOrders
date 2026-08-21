# M2 integrated acceptance

Status: **passed; M2 reclosed and awaiting fresh external validation**

Original acceptance date: 2026-08-21

## Reopened integrated-acceptance fixed point and refreshed currentness

Fresh external validation reopened the integration gate for the lifecycle-inventory
defect corrected below. The reopened ticket starts exactly at
`a97de65bd09e249991ab7969305ce7663ce3719e`; its claim commit is
`60e4afb7732ff11976ee1ba7b4d7618cc674a0e4`. No product, dependency, lockfile,
contract, fixture, or adapter change is required by the correction.

Authoritative release data was independently refreshed again on 2026-08-21 after
the claim. Rust's signed stable-channel manifest is dated 2026-08-20 and names
Rust 1.98.0 at commit `88d9e12ae178fab0fb5cc050a94da85685d449ea`;
the accompanying `.asc` signature is present. Node's official release index names
26.7.0 as the newest release in selected major 26. Official npm registry
packuments reproduce every selected direct version in the table below, including
npm 12.0.2, TypeScript 7.0.2, AJV 8.20.0, and Vite 8.2.2. Live crates.io index
queries reproduce all six selected direct crate versions, and
`cargo update --dry-run --verbose` locks zero packages for Rust 1.98.0.

The refreshed root npm outdated report contains only `fast-uri` 3.1.5 behind the
different-major 4.1.2. The desktop report contains only absent current optional
packages plus Nano ID 3.3.18 behind different-major 6.0.1 and `undici-types`
8.3.0 behind selector-incompatible 8.10.0. No newer compatible direct or locked
transitive selection exists, so the three existing lockfiles remain the exact
current selected-major graphs. The complete clean-copy gate was subsequently
rerun at the committed remediation target and passed as recorded in the reopened
gate section below.

## Fixed points and prior validated baselines

- [Integrate M2 acceptance and refresh the complete selected-major toolchain](tickets/006-integrate-m2-acceptance-and-refresh.md)
  review fixed point: `866f619414e57aa816184509ed8e57209c8ea62f`.
- Claim commit: `ee1584966614d62d1850ecaf1c23fd6fdd770d5b`.
- Refreshed implementation target used for this acceptance run:
  `51dcbe1c033c1ad4f5d9201328f4ba008be89624`.
- M2 began from the [exact README read](tickets/001-read-exact-conventional-readme.md)
  fixed point `3ce6265e76d61771dcc45b40bee6f6c831939d4f`.
- M1 external validation covers
  `c8c1250fd7803fa9b1b80967ce30031cf03ebd6f..7f4841c8f7bca9c3767795c0939b040cbf13a173`.
- M1-upgrade external validation covers original fixed point
  `80e5527034615d6b71fce63ea40b55437bff9492`, frozen dependency snapshot
  `67dc5e4560df173b902b24d3463bb01ff46de23b`, gate target
  `ab02ed5ca2d0e0ed0534859f31d19f488218090c`, and externally validated target
  `c344bbad232f6da88dc6f2a1ab294a92cda978cb`.

The five capability slices were closed in order. Their exact lifecycle and review
inventory is:

| Capability | Fixed point | Claim | Implementation / pre-review | Remediation | Review disposition | Closure |
|---|---|---|---|---|---|---|
| [Exact README read](tickets/001-read-exact-conventional-readme.md) | `3ce6265e76d61771dcc45b40bee6f6c831939d4f` | `e4b3f705ffe973c67c3ed8cd4cbbe59b5ee82f42` | `0cc1b6a3f32b8326b3f072915de1e6f9b3bbc984` / `8c85c1b9f35150356ad22b44db71f289f9d39591` | `d47db73cc1d4817164d8612c1cad06d064887d4d` | `6e92d1c530b76e3d56c9abdc6c61a4adc7281eba` | `eb193dbf019690c828bba07758fde019468c12c1` |
| [Canonical Package projection](tickets/002-classify-project-canonical-package.md) | `eb193dbf019690c828bba07758fde019468c12c1` | `db55e009c2e7f11080cd6cac7afef898bd409057` | `e2a60c2d7c6986f2217bce0caf36641a5c12b31f`, `a9c80bbea79f16c4ba0200687113b1c2181b4256` / `32d02663608d6170398cca61c2caf2b84ae28737` | `c96696d242df67e2cec8aec9679a9be02043f501` | `e2ad323e910e02588d9f6f26203ae864962bdc8b` | `d66e9b0bf46b5c303c1944368258ea978cf2043e` |
| [Package Index rebuild](tickets/003-rebuild-package-index.md) | `d66e9b0bf46b5c303c1944368258ea978cf2043e` | `f488e6725731cca1cf49a32ef6b22a0dfb64d1ad` | `c28258b1b5ce1a67055b0c42f91e9f2c673b857d` / `fe152b6d216d649145babb81b0de2c39211daa2f` | `dd6c332572f2d87217b3685659845b21943c3d72`, `67c376351986aff1420648456d9169784897250d` | `1b6697fe9fce37fa4ff2a7a737a881e7c0ebf1fd` | `598339792a85831ea2d8daf52e13c63210eafab4` |
| [Current-index fast path](tickets/004-load-current-index-fast-path.md) | `598339792a85831ea2d8daf52e13c63210eafab4` | `70b08006cc228fc8a92def579bcccbb9cfd0e70a` | `985c3513ef68dbe190ba43a7d57423f599c2fe95` / `cc286b214422c41650ab7c6bd9abfd395aa9c3f8` | `b4831a2b21bf6756fb2873b8154aac79fdebcdcc` | `ecd853b08009b1e1a946050b9594f482949bd3b7` | `ecd853b08009b1e1a946050b9594f482949bd3b7` |
| [Exhaustive managed validation](tickets/005-validate-exhaustive-managed-evidence.md) | `ecd853b08009b1e1a946050b9594f482949bd3b7` | `2a8959d5496978b48e14210a5a7811b9495aaf76` | `5d7a225d57b7e7daa402d4ac40bd8b3b96f799b5` / `629ac14e7ffcf5e9e6324bf4cae318f5454ab977` | `178462e872a0394198338b89f0cad15eb0177ca0` | `0c42ef3fef0ce1c6bb977a24604fcf269f15ccd9` | `1ee856a490572405eac3fb46df483ce8fbf558f9`; evidence `866f619414e57aa816184509ed8e57209c8ea62f` |

Every recorded Standards and Spec review ran from its fixed point, every accepted
finding was remediated or explicitly dispositioned, and every slice records zero
unresolved final findings. The integration gate began only after the exhaustive
managed-validation closure and closure-evidence commits were integrated.
For the current-index fast path, review disposition and closure coincide in
`ecd853b08009b1e1a946050b9594f482949bd3b7`; the preceding
`b4831a2b21bf6756fb2873b8154aac79fdebcdcc` commit contains only the review-driven
Rust source and test remediation.

## Refresh inventory and authoritative sources

Release data was refreshed on 2026-08-21 from Rust's signed stable-channel
manifest at <https://static.rust-lang.org/dist/channel-rust-stable.toml>, Node's
official release index at <https://nodejs.org/dist/index.json>, npm registry packuments
at <https://registry.npmjs.org/>, and the crates.io index/API. The selected-major
results were:

| Selection | Before | Verified current | Disposition |
|---|---:|---:|---|
| Rust/Cargo | 1.97.1 | 1.98.0 | Updated |
| Node | 26.5.1 | 26.7.0 | Updated |
| npm | 12.0.2 | 12.0.2 | Current |
| TypeScript | 7.0.2 | 7.0.2 | Current |
| AJV | 8.20.0 | 8.20.0 | Current; major 8 preserved |
| Tauri API / CLI | 2.11.1 / 2.11.4 | 2.11.1 / 2.11.4 | Current |
| React / ReactDOM | 19.2.8 / 19.2.8 | 19.2.8 / 19.2.8 | Current |
| Node types | 26.1.2 | 26.2.0 | Updated |
| React / ReactDOM types | 19.2.17 / 19.2.3 | 19.2.18 / 19.2.4 | Updated |
| Vite React plugin | 6.0.5 | 6.1.0 | Updated |
| Vite | 8.2.0 | 8.2.2 | Updated; major 8 preserved |
| `focaccia` | 2.0.0 | 2.0.0 | Current |
| `serde` | 1.0.229 | 1.0.229 | Current |
| `serde_json` | 1.0.151 | 1.0.151 | Current |
| `toml` | 1.1.4 | 1.1.4 | Current |
| `tauri` / `tauri-build` | 2.11.5 / 2.6.3 | 2.11.5 / 2.6.3 | Current |

`npm update --package-lock-only --ignore-scripts` advanced `fast-uri` 3.1.4 to
3.1.5, removing GHSA-7p8r-x3mc-p8w7, and advanced the desktop compatible graph.
Vite 8.2.2 now selects Rolldown 1.2.5; that graph no longer contains the formerly
frozen optional `@napi-rs/wasm-runtime` 1.2.1 selection, so no stale 1.2.2/1.2.3
runtime remains. `cargo update --verbose` advanced 59 compatible locked packages.
The refreshed lock hashes are:

- `Cargo.lock`: `85ba2007355e9c58bf5151ccb28265c4de64ac0b5c05966cd40df1045e507a79`;
- root `package-lock.json`: `c6d7aa1008111da085594e59ba06dd154eb6cd03892242fd6652ea912afed74f`;
- desktop `package-lock.json`: `7e27b881eb64c3a84d2aa90de019e6ebda27ea83f3753829374d20c26fdd8773`.

No new major, Mantine dependency, shared SpielGantt crate, install-script authority,
schema, fixture meaning, metadata version, product behavior, or adapter behavior
was introduced.

## Fresh target copy and complete commands

A fresh local clone was created with no hardlinks and detached exactly at the
refreshed target:

```sh
git clone --local --no-hardlinks /Users/example/Code/Apps/SpielOrders \
  /private/tmp/spielorders-m2-006.Idg47j/repo
git checkout --detach 51dcbe1c033c1ad4f5d9201328f4ba008be89624
```

The following repository/provenance commands passed and the source and clone were
empty before and after the gate:

```sh
git status --porcelain=v1 --untracked-files=all
git rev-parse HEAD
git merge-base --is-ancestor 866f619414e57aa816184509ed8e57209c8ea62f HEAD
git log --oneline 866f619414e57aa816184509ed8e57209c8ea62f..HEAD
git diff --check 866f619414e57aa816184509ed8e57209c8ea62f...HEAD
git diff --name-status 866f619414e57aa816184509ed8e57209c8ea62f...HEAD
```

The selected runtimes reported Node `v26.7.0`, npm `12.0.2`, rustc `1.98.0
(88d9e12ae 2026-08-18)`, and Cargo `1.98.0 (797e8a9bc 2026-08-05)`.
Dependency recreation passed online and then registry-free from the refreshed
cache, with no install scripts:

```sh
npm clean-install --ignore-scripts
npm --prefix apps/desktop clean-install --ignore-scripts
npm clean-install --offline --ignore-scripts
npm --prefix apps/desktop clean-install --offline --ignore-scripts
```

The empty-target Rust and focused integration gates passed:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo test -p spielorders-core --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p spielorders-core --locked \
  --test readme_read --test canonical_package_load \
  --test package_index_rebuild --test package_index_replacement_failure \
  --test package_index_fast_path --test exhaustive_validation \
  --test filesystem_change_observation
```

There are 156 workspace Rust tests and 94 tests in the seven focused M2 test
binaries. The complete workspace, core-only repetition, focused suite, and clippy
all exited zero. The integration-gate diff contains zero Rust source files, so the
required touched-Rust `rustfmt --edition 2024 --check` set is empty. A broader,
non-gating `cargo fmt --all -- --check` diagnostic found only pre-existing final
blank lines in four untouched adapter stubs; it did not identify an in-scope
touched file and no unrelated edit was made.

Schema, traceability, frontend, and Tauri gates passed:

```sh
npm run test:schemas
npm run check:traceability
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri build -- --no-bundle
```

The schema harness independently reports 4 schemas, 8 accepted examples, 19
manifest-listed rejected fixtures, 22 scenario-index entries, and 2 binary
sentinels. Traceability covers 7 M0 tickets and 4 baseline test files. Both
frontend tests passed, Vite 8.2.2 built 15 modules, and Tauri produced the
no-bundle release binary.

## Integrated M2 behavior

The public `PackageLoad` enum retains exactly six families: `Unmanaged`,
`Available`, `DiagnosticOnly`, `UpgradePending`, `IndexReplacementFailed`, and
`OperationFailed`. Acceptance establishes them as follows:

- canonical and fast-path tests execute unmanaged, available, and diagnostic-only
  damaged results and assert their distinct projections;
- metadata and public-vocabulary tests prove upgrade-pending symbolic reporting
  without inventing an unsupported old or future schema;
- the rebuild and dedicated replacement-failure suites exercise the blocking
  `IndexReplacementFailed` family through every staging, validation, durability,
  replacement, and cleanup phase; and
- canonical, fast-path, rebuild, exhaustive, README, change-observation, and
  replacement tests inject every authorized filesystem-port operation failure and
  assert `OperationFailed` or the narrower documented evidence contract.

The same suites prove complete healthy and damaged projections, every duplicate-ID
participant, stable typed diagnostics/evidence/grouping/ordering/deduplication,
exact README reads and version-1 cache non-evidence, all Package Index states,
reasons and refresh outcomes, deterministic healthy-only version-1 rebuild bytes,
replacement preservation, and exhaustive-validation non-retroactivity.

The proven-current minimal fast path has an exact read budget of only
`.spielorders/config.toml`, `.spielorders/version.json`, and
`.spielorders/index.json`; it performs four no-follow change observations including
`vendors.json`, lists only the `Orders` candidate-membership directory, performs no
write/durability/replacement/cleanup effect, and does not read `vendors.json`, any
`order.json`, or README. The representative indexed Order adds only the Order
metadata and README change observations; it never lists that Order Directory.

### Filesystem-port fault matrix

The focused test names and assertions give the following operation-to-result
matrix; every row passed in the detached copy:

| Port operation or replacement phase | Injected/observed condition | Required public result and retained evidence |
|---|---|---|
| `canonicalize` | Every authorized Package, Orders, and Order path fails | `OperationFailed` with exact `Canonicalize` location; Supporting Material untouched |
| `symlink_metadata` | Required topology/artifact observation fails | Canonical/exhaustive calls return exact `ObserveKind` `OperationFailed`; index-kind failure returns `OperationFailed` without replacement |
| `observe_file_change` | Each current-index input fails, is uncertain/incomparable/contradictory, missing, or wrong-kind | Fast trust is rejected, original reasons are retained, canonical fallback runs, and no Supporting Material path appears in effects |
| `read_directory` | Orders or each authorized reserved directory listing fails | Exact `ListDirectory` `OperationFailed`; successful fast load lists only Orders membership, exhaustive validation additionally lists only immediate reserved directories |
| `read` | Config/version/vendors/order/index/README or staged candidate read fails | Required canonical artifacts become stable damage; README becomes one warning; index becomes the exact malformed reason; staged read becomes blocking `IndexReplacementFailed` |
| `write` | Not authorized by any M2 read operation | Effect-budget assertions prove zero overwrite calls |
| `write_new` | Existing stage or injected staging write failure | Blocking `IndexReplacementFailed`; old/missing index assessment and canonical projection retained; no overwrite |
| Candidate validation | Staged bytes malformed, semantically invalid, non-deterministic, or unequal to canonical candidate | Blocking `IndexReplacementFailed`; destination unchanged; exact validation/cleanup evidence retained |
| `sync_file` | Durability failure | Blocking `IndexReplacementFailed`; old destination and healthy canonical evidence retained; stage cleanup reported |
| `replace_file` | Atomic replacement failure | Blocking `IndexReplacementFailed`; old index byte-identical; exact source/destination and phase retained |
| `remove_file` | Cleanup after an initiating failure also fails | Initiating failure plus ordered cleanup failure retained; old index and independently healthy canonical evidence retained |

The dedicated `every_replacement_and_cleanup_failure_preserves_complete_evidence_and_effect_budget`
test covers all six replacement boundaries (`WriteStage`, `ReadStage`,
`CandidateValidation`, `SyncStage`, `Replace`, and `RemoveStage`) and asserts the
exact complete effect path budget. The canonical and exhaustive failure loops
cover every authorized path for canonicalization, no-follow kind observation,
reading, and directory listing; the fast-path loop covers every change-observation
site.

### Metadata-version-1 byte and shape audit

The detached copy also ran:

```sh
for set in minimal full; do
  for artifact in version order vendors index; do
    jq -c 'keys' "contracts/examples/v1/$set/$artifact.json"
  done
done
jq -e '.metadata_version == 1 and (keys == ["metadata_version"])' \
  contracts/examples/v1/{minimal,full}/version.json
jq -e '.schema_version == 1' \
  contracts/examples/v1/{minimal,full}/{order,vendors,index}.json \
  contracts/fixtures/derived-index/*.json
sha256sum contracts/examples/v1/{minimal,full}/{version,order,vendors,index}.json \
  contracts/fixtures/derived-index/*.json
```

All ten version assertions returned true. The exact shape and byte results were:

| Artifact | Exact sorted top-level keys | SHA-256 |
|---|---|---|
| `examples/v1/minimal/version.json` | `["metadata_version"]` | `451b2e02c3c7b2ee4fbfdf6f1cef88c3fd9dbd6583a0b322d1697fcc98a429bd` |
| `examples/v1/minimal/order.json` | `["date_ordered","items","order_id","package_root","schema_version","status","title"]` | `89cc1bc88c913976b49be4d760ba51018a814dd0ca8e0a54a3d91394a94a1b03` |
| `examples/v1/minimal/vendors.json` | `["schema_version","vendors"]` | `ea8a241b769b598c6e1bc76e049657734af6d36f8ba270d12357390e04f86c32` |
| `examples/v1/minimal/index.json` | `["orders","schema_version","vendors"]` | `709c2406a171f06cb83f1aea20ce00d1cb2a84b54dc49b105722f801aaac54e8` |
| `examples/v1/full/version.json` | `["metadata_version"]` | `451b2e02c3c7b2ee4fbfdf6f1cef88c3fd9dbd6583a0b322d1697fcc98a429bd` |
| `examples/v1/full/order.json` | `["currency","date_ordered","items","order_group","order_id","orderer","ordering_institution","package_root","project","schema_version","status","title","vendor_id"]` | `53b8e3c14e04ccf902a1d90107f2e3f5caad4d4d5791144f6d1aafe99885124a` |
| `examples/v1/full/vendors.json` | `["schema_version","vendors"]` | `7751ee268f9ec273c174421970a8a5d07c77d903a4a0b9207f5d74c90ebfaaf6` |
| `examples/v1/full/index.json` | `["orders","schema_version","vendors"]` | `af542fdddf1e761e798f35ab8bdfff5311d34e451c1d84e8e964caff2f7d97ef` |
| `fixtures/derived-index/minimal-deterministic.json` | `["orders","schema_version","vendors"]` | `183f7fb885d22c770bb625094959639b5375cbbb9e9d3d29162e463e873625b6` |
| `fixtures/derived-index/wrong-order-path.json` | `["orders","schema_version","vendors"]` | `6938457cea735421f2e976f165dc4bd00b267941c8332ad0f6ab42d024fc2dc6` |

Full records add only their schema-defined optional Order fields and retain the
same version, Vendors, and Index envelope shapes.
The public `minimal_order_round_trips_through_canonical_json`,
`index_documents_preserve_complete_order_readme_and_vendor_projections`, and
`rebuild_bytes_are_enumeration_independent_sorted_and_healthy_only` tests then
prove codec round-trip and rebuilt bytes match those unchanged version-1 shapes.

## Dependency and boundary audits

These commands passed or produced the recorded selector evidence:

```sh
cargo update --dry-run --verbose
cargo metadata --locked --format-version=1 --no-deps
cargo tree --workspace --depth 1 --locked
cargo tree --duplicates --locked
cargo tree -p spielorders-core --depth 1 --locked
cargo tree -p spielorders-desktop --depth 1 --locked
cargo tree --invert generic-array@0.14.7 --locked
cargo tree --target all --invert toml@0.8.2 --locked
cargo tree --target all --invert toml_datetime@0.6.3 --locked
cargo tree --target all --invert toml_edit@0.20.2 --locked
npm ls --all
npm --prefix apps/desktop ls --all
npm outdated --all --json
npm --prefix apps/desktop outdated --all --json
npm audit --audit-level=low --json
npm --prefix apps/desktop audit --audit-level=low --json
npm install-scripts ls
npm --prefix apps/desktop install-scripts ls
npm explain fast-uri
npm --prefix apps/desktop explain nanoid
npm --prefix apps/desktop explain undici-types
npm --prefix apps/desktop explain rolldown
```

`cargo update --dry-run --verbose` locks zero further packages for Rust 1.98.0.
Its four available-but-unselected records are selector-owned: `generic-array`
0.14.7 is required exactly by `crypto-common` (`=0.14.7`), while TOML 0.8.2,
`toml_datetime` 0.6.3, and `toml_edit` 0.20.2 are target-only Tauri/GTK branches
owned by `system-deps`/`proc-macro-crate`. A precise generic-array 0.14.9 dry run
correctly fails the upstream exact selector; no override was introduced.

The npm outdated reports have only selector/platform explanations. Root
`fast-uri` 3.1.5 is current/wanted under AJV's `^3.0.1`; latest 4.1.2 is a new
major. Desktop `nanoid` 3.3.18 is current/wanted under PostCSS `^3.3.17`; latest
6.0.1 is a new major. `undici-types` 8.3.0 is current/wanted under current
`@types/node`'s narrower `~8.3.0`; 8.10.0 is outside that upstream selector.
Platform and peer-optional packages have wanted equal to latest and are absent by
design. Rolldown 1.2.5 is selected by Vite's `~1.2.4`.

Both npm audits report zero vulnerabilities. The recorded gate's two
`npm install-scripts ls` commands reported no package with an unreviewed install
script. The acceptance condition is zero granted or unreviewed install-script
authority: npm 12.0.2 may instead report desktop `fsevents` 2.3.3 as blocked
because it is not covered by `allowScripts`, which is passing denial evidence.
Any approved or granted lifecycle authority fails the gate. Locked metadata shows
exactly three workspace members; CLI and Tauri depend on core, core has only its
four domain dependencies and never depends on either adapter, and the integration
range changes zero Rust, CLI, Tauri, React, contract, schema, or fixture source
files. Manifest/lock/source searches found zero install-authority, Mantine,
SpielGantt, or prohibited dependency matches.

## Development smoke and preservation

The bounded command

```sh
npm --prefix apps/desktop run tauri dev -- --no-watch
```

reported Vite 8.2.2 ready at `http://localhost:1420/`, built and ran the native
debug binary, and independent process inspection observed both the Vite and
`spielorders-desktop` processes. An initial `127.0.0.1`-only probe returned HTTP
000 because Vite was bound to the configured `localhost` host; the authoritative
probe to the exact configured URL returned HTTP 200. Ctrl-C returned exit zero,
and an independent final process search found no survivor.

Before dependency recreation and after all builds, tests, audits, fault injection,
and smoke work, the sentinels remained:

- `supporting-a.bin`:
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
- `supporting-b.bin`:
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

Real-filesystem and test-double acceptance proves fast load, canonical fallback
and rebuild, exhaustive validation, exact README reads, replacement success, and
every injected failure neither inspect nor recurse through Supporting Material.
No contract path changed from the integration-gate fixed point.

## Original pre-review disposition

For the original
[integrated M2 acceptance and selected-major refresh](tickets/006-integrate-m2-acceptance-and-refresh.md)
lifecycle, all implementation and acceptance gates
passed at the refreshed target. There was no pre-existing non-integrative M2
behavior defect to return to an earlier ticket and no refresh compatibility change
beyond manifests/locks, so no synthetic TDD test was added. That original gate
remained in progress until its independent Standards and Spec review against
`866f619414e57aa816184509ed8e57209c8ea62f...HEAD` completed. The reopened review
instead used `a97de65bd09e249991ab7969305ce7663ce3719e...HEAD`.

## Fixed-point code review and remediation

The project-local `code-review` skill reviewed
`git diff 866f619414e57aa816184509ed8e57209c8ea62f...5eacd7fcbb7d927e87b13103d5a417291eacb21f`
and the matching two-dot log. Standards and Spec ran independently and in
parallel.

### Standards

- Initial outcome: one P2 hard traceability-language violation. The new acceptance
  document used bare tracker identifiers where
  [`docs/development/traceability.md`](../../../docs/development/traceability.md)
  requires full linked decision/capability names or meaningful prose.
- Remediation commit `8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6` replaces every bare
  identifier in the acceptance record and adds linked meaningful names.
- Independent revalidation of
  `866f619414e57aa816184509ed8e57209c8ea62f...8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6`
  found zero remaining hard violation, zero new or remaining baseline smell, and a
  clean whitespace diff.

### Spec

- Initial outcome: two P2 findings. The prior capability slices lacked exact
  lifecycle/review commit inventory, and complete port-fault/version-1 byte-shape
  evidence was asserted without an auditable matrix and exact outputs.
- The first remediation adds all five exact lifecycle rows and the complete
  operation/phase-to-result fault matrix. Independent recheck found both portions
  resolved and requested only the complete ten-artifact key/hash output.
- Remediation commit `08b8309fd1e0d166816d9d02c34318857955f66e` records all ten
  exact sorted key arrays and all ten SHA-256 values. A third fresh no-hardlink
  clone detached at that commit reproduced every key, hash, version assertion,
  sentinel hash, fixed-point whitespace check, and empty status.
- Final independent Spec recheck found no remaining or new finding.

### Post-remediation verification

The second fresh no-hardlink copy at `8a2e7f8367e0885539e2356a1bdbce4bf4e05ee6`
passed the 94 focused M2 tests, schema and traceability gates, exact version-1
shape/hash audit, fixed-point whitespace check, and preservation hashes. The third
copy at `08b8309fd1e0d166816d9d02c34318857955f66e` repeated the exact
artifact/version/sentinel output and repository guards. No implementation source,
contract, fixture, dependency, lockfile, adapter, or product behavior changed
during remediation.

## Closure

The integration gate and M2 milestone closed on 2026-08-21 after the complete
clean-copy acceptance and resolved fixed-point review. The ticket, milestone,
acceptance, and roadmap closure transition is
`3edd745c002805a668cd33d8a5d89ea6a46009f3`.

M2 is complete at the implementation layer and stops here. Fresh external
validation must reproduce the original fixed point, exact completed target, all
commands/currentness/count/preservation/boundary evidence, and both independent
code-review axes before any M3 work begins.

## Reopened integrated-acceptance refreshed gate

The reopened remediation candidate is
`06c5198741b682297865c6d1b8477e85cea834c2`, comprising the isolated claim
`60e4afb7732ff11976ee1ba7b4d7618cc674a0e4` and the lifecycle-inventory/currentness
correction. A fresh no-hardlink clone at
`/private/tmp/spielorders-m2-006-reopen.IcZ3HR/repo` was detached exactly at that
candidate. The source and clone were clean before the gate; the clone remained
clean afterward. The original reopen fixed point is an ancestor, the two-commit
log and three changed documentation paths are exact, and
`git diff --check a97de65bd09e249991ab7969305ce7663ce3719e...HEAD` passes.

The complete required gate was rerun under Node 26.7.0, npm 12.0.2, rustc 1.98.0,
and Cargo 1.98.0. Online and then offline `npm clean-install --ignore-scripts`
passed for both trees. Empty-target workspace build, all 156 workspace tests,
core-only tests, all 94 focused M2 tests, and workspace/all-target clippy with
warnings denied passed. Schema and traceability checks, both frontend tests,
the 15-module production build, Tauri no-bundle release build, and bounded Tauri
development smoke passed. The smoke independently observed Vite and the native
desktop process, received HTTP 200 from the configured `localhost:1420` URL, and
left no process after Ctrl-C.

Independent recount reproduced 4 schemas, 8 accepted examples, 19 rejected
fixtures, 22 scenarios, and 2 opaque sentinels. All ten metadata-version-1 key
arrays, hashes, and version assertions reproduced the exact table above. The
lockfiles remained:

- `Cargo.lock`: `85ba2007355e9c58bf5151ccb28265c4de64ac0b5c05966cd40df1045e507a79`;
- root `package-lock.json`: `c6d7aa1008111da085594e59ba06dd154eb6cd03892242fd6652ea912afed74f`;
- desktop `package-lock.json`: `7e27b881eb64c3a84d2aa90de019e6ebda27ea83f3753829374d20c26fdd8773`.

The final sentinel hashes remain
`a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`
and `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.

The complete dependency audit also reran. Cargo locks zero updates; the only
reported selections behind a published release are `generic-array` 0.14.7,
required by `crypto-common` with exact selector `=0.14.7`, and old TOML-family
versions confined to Tauri's Linux/GTK target and build branches. npm reports only
`fast-uri` 3.1.5 behind different-major 4.1.2, Nano ID 3.3.18 behind
different-major 6.0.1, and `undici-types` 8.3.0 held by current `@types/node`'s
`~8.3.0`; all remaining entries are absent platform/peer optionals with wanted
equal to latest. Both npm audits contain zero vulnerabilities. The lifecycle
audits grant no install-script authority; an explicitly blocked desktop
`fsevents` 2.3.3 script is passing evidence that `allowScripts` remains absent,
not an unreviewed or authorized lifecycle script.

Locked metadata and direct trees again prove CLI and Tauri depend on
`spielorders-core`, never the reverse. The reopened range contains no Rust,
contract, fixture, schema, lockfile, CLI, Tauri, React, or other implementation
change and introduces no M3 behavior, adapter rule, install-script authority,
Mantine dependency, or shared SpielGantt code. The six `PackageLoad` families,
read/effect budgets, full filesystem-port fault matrix, Supporting Material
non-inspection, deterministic version-1 bytes, and canonical/fast/exhaustive
separation were all re-exercised by the unchanged complete and focused suites.
No behavior defect or compatibility change was found, so the project TDD skill
does not call for a synthetic test for this documentation-only correction.

## Reopened fixed-point code review

The project-local `code-review` skill reviewed
`a97de65bd09e249991ab7969305ce7663ce3719e...HEAD` after the gate-evidence commit
`9db286ba49897e82fbccd88cad80c01881e817ab`. Standards and Spec ran independently
and in parallel.

### Standards

The initial review reported three P2 hard documentation-state violations: stale
present-tense prose called the ticket unclaimed in the milestone and ticket, and
stale acceptance prose said the already-passed gate still needed to run. It found
zero Fowler-baseline smells. Commit
`90e8f25dd001ec9ec140c06e856c62765e4280b7` explicitly frames the unclaimed and
original-review statements as history and makes the current claimed, gate-passed
state consistent. Independent revalidation found zero remaining or new hard
violation, zero smell, and a clean whitespace diff.

### Spec

The initial review reported two overlapping P2 findings: the stale gate/review
language conflated the original `866f6194...` review with the reopened
`a97de65b...` review, and the claim-state prose contradicted the required isolated
claim. It found no scope creep and confirmed the
[current-index fast-path](tickets/004-load-current-index-fast-path.md) lifecycle
correction was exact. Commit `90e8f25dd001ec9ec140c06e856c62765e4280b7`
resolves both.
Independent revalidation found zero remaining or new missing/partial requirement,
scope creep, or incorrect implementation. Both review axes pass without waiver.

## Reopened result and reclosure

The corrected lifecycle inventory, refreshed currentness, complete clean-copy
gate, and resolved fixed-point review all pass without waiver. The
[integrated M2 acceptance and selected-major refresh](tickets/006-integrate-m2-acceptance-and-refresh.md)
capability and M2 reclose on 2026-08-21 in transition
`e280145ae8eff2b03a856562aa2338b0cebd263f` and stop at the implementation layer.
Fresh validation-only external reproduction of the exact completed target is
required before M2 may be marked externally validated or any M3 work may begin.
