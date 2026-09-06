---
title: Resolve safe Package paths and classify direct-child Order candidates
status: closed
assignee: codex
claimed_at: 2026-07-23
closed_at: 2026-07-23
remediated_at: 2026-07-25
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:task
---

## Outcome

Given an explicit SpielOrders Root and typed configuration, the core resolves the
managed topology, validates an Order's relative Package locator, and classifies
immediate Orders Directory entries without following links or escaping the root.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Require two parent segments in the Order package locator](../../m0-contract-baseline/tickets/007-require-two-parent-package-root.md)

## Blocking

Unblocked. [Round-trip the complete metadata-version-1 vocabulary through the Rust core](001-metadata-v1-vocabulary.md)
closed on 2026-07-22.

## Scope

- Apply defaults and validate `orders_path` and `removed_path`.
- Reject `.`, absolute paths, parent traversal, empty components, overlap,
  `.spielorders` occupation, and link-based escape.
- Require the Removed Orders Directory to be an ordinary direct child.
- Recognize reserved root and Order `.spielorders` paths.
- Nonrecursively classify visible ordinary direct-child directories as candidates,
  hidden entries as ignored, and visible files/links as disallowed observations.
- Validate normalized parent-only `package_root` locators against the explicit root
  and configured active direct-child placement.

Exclude complete Package loading, canonical metadata reads, damage models, stable
diagnostic codes, README behavior, recursive Supporting Material traversal,
Removed Orders discovery, repairs, and all directory mutations.

## Pre-agreed public test seam

Public layout inspection accepts an explicit root, typed config, and filesystem
effect and returns resolved managed paths plus classified direct-child observations,
not a loaded Package. Public locator validation checks one Order Directory. Tests use
both the filesystem double and real symlinks in a temporary tree. The tracer bullet
resolves default `Orders` and recognizes one physical direct child.

## Acceptance criteria

- Valid nested Orders paths resolve inside the selected root.
- Root-as-Orders, traversal, overlap, reserved occupation, and link escapes fail.
- Links are never followed and cannot become Order candidates.
- Hidden entries are ignored; visible non-directory entries are classified but
  untouched.
- Classification is one level only and never examines Supporting Material.
- Correct nested locators validate; stale or mismatched locators fail locally.

## Review boundary

Review path ownership, containment, locator resolution, and direct-child
classification only—not Package loading or diagnostic policy.

## Ticket-start fixed point

`76a52e27f72140d835cc47623fdc670e7700327d`

## Resolution

Added public safe Package-layout inspection and Order-locator validation through an
expanded no-follow filesystem effect. Typed configuration validation applies the
existing defaults and rejects current-directory, absolute and Windows-prefixed,
parent-traversing, empty-component, non-slash-separated, reserved, overlapping, and
non-direct Removed paths. Every configured Orders prefix is resolved and checked
for containment, including link escape, resolved overlap, and resolved occupation
of root `.spielorders`; an existing Removed path must be an ordinary direct-child
directory.

Layout inspection returns resolved root, Orders, Removed Orders, and reserved
metadata paths plus deterministic direct-child observations. It classifies visible
ordinary directories as Order candidates, hidden entries as ignored, and visible
files, links, or other entry kinds as disallowed without following them or examining
Supporting Material. Locator validation checks one ordinary visible direct child
against the explicit root, configured active placement, and its typed parent-only
`package_root` value without loading Package metadata.

Implementation commits:

- `8445ac2` — safe Package paths, no-follow classification, reserved recognition,
  and locator validation.
- `d9f16dc` — accepted `/code-review` fix for Windows drive-relative paths.

Acceptance evidence (2026-07-23, Rust 1.97.1):

- `cargo test -p spielorders-core --test package_paths` — passed 10 public-behavior
  tests using both a filesystem double and real temporary-tree symlinks.
- `cargo test --workspace` — passed the complete Rust workspace suite.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- `cargo update --dry-run --verbose` — found zero compatible lockfile updates.
  Newer target-specific transitive `generic-array` 0.14.9, `toml` 0.8.23,
  `toml_datetime` 0.6.11, and `toml_edit` 0.20.7 remain constrained by the current
  Tauri/GTK dependency graph. Direct `toml` is current at 0.9.12; 1.1.3 is an
  unapproved major version and was not adopted.
- `/code-review` from the exact ticket-start fixed point reported zero Standards
  findings and one Spec finding. The accepted Windows drive-relative containment
  defect was fixed and all focused and full gates were rerun. The correction did not
  materially change the public design, so a second review run was not required.

## External-validation remediation — M1-V2

Remediation fixed point:
`954c81d31438cad762e43ccb0b0e0f8a9f7b6a66`

External validation found that the public reserved-path classifier compared raw
path components. It therefore classified
`<root>/.spielorders/../README.md` as Package metadata and failed to recognize
`<order>/Supporting/../.spielorders/order.json` as Order metadata.

The public contract now classifies a path by its lexically normalized location
within the already canonical Package layout. The input must be rooted at the
layout's canonical SpielOrders Root. Current-directory components are ignored;
parent components may move only within the root; and paths outside the root or
attempting to traverse above it receive no reserved classification. This is a
pure lexical operation: it performs no filesystem access, recursive Supporting
Material inspection, or link following.

The TDD work proceeded in two vertical slices through the public
`PackageLayout::reserved_path_kind` seam:

- RED: `non_normalized_package_paths_are_reserved_only_when_they_remain_in_package_metadata`
  returned `Some(PackageMetadata)` for the user-authored root README. GREEN:
  root-confined lexical normalization made that path and an above-root traversal
  unreserved.
- RED: `non_normalized_order_paths_use_lexical_location_without_inspecting_supporting_material`
  returned `None` for the normalized immediate Order metadata path. GREEN: both
  Package and Order classification consume the same normalized path. The test's
  nested Supporting Material link remains unreserved even when the filesystem
  double could resolve it back to the Order Directory.

Existing normalized Package metadata, immediate Order metadata, hidden-entry,
nested Supporting Material, one-level discovery, and no-follow link behavior
remain covered by the unchanged public tests. The repair adds no Package loading,
diagnostics, repair, mutation, README, index, directory, adapter, shared-crate, or
M2 behavior.

Remediation implementation commit:

- `644513b` — explicit non-normalized-path contract, root-confined lexical
  normalization, and public Package/Order regressions.

Clean-copy evidence (2026-07-25, exact commit `644513b`, Rust/Cargo 1.97.1,
Node.js 24.15.0, npm 11.16.0):

- An artifact-free no-hardlink clone contained no `target`, `node_modules`, or
  `dist` before both locked npm trees were recreated offline with zero
  vulnerabilities.
- The focused locked path suite passed 12 tests. Locked workspace build,
  workspace tests, independent-core tests, and Clippy with warnings denied
  passed; the workspace and independent-core runs each passed 62 Rust tests plus
  doc tests.
- Schema and traceability checks passed with 4 schemas, 8 accepted examples, 17
  unique rejected fixtures, 22 unique scenarios, 2 sentinels, 7 M0 tickets, and 4
  baseline test files. The desktop's 2 frontend tests, production build, and
  Tauri no-bundle build passed.
- Independent recounting confirmed the same schema, fixture, scenario, sentinel,
  and 62-test totals. Both opaque Supporting Material SHA-256 values remained
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`
  and
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.
  The clean clone remained source-clean after all gates.
- `cargo update --dry-run --verbose` found zero compatible lockfile updates, and
  authoritative npm registry checks found no selected-major package update. Rust
  1.97.1 is current. Node's official index identifies 24.18.0 as current in
  selected major 24, and npm's official version history identifies 11.18.0 as
  current in selected major 11. The excluded M1-V4 toolchain finding therefore
  remains unresolved; this M1-V2 remediation did not change the runtime. Node
  26, npm 12, TypeScript 7, and `toml` 1 remain unapproved new majors.
- The documented repository-wide `cargo fmt --all -- --check` observation remains
  limited to the same four trailing blank lines in unchanged M0 adapter stubs.
  `cargo fmt -p spielorders-core -- --check`, `git diff --check`, dependency
  direction, and the scoped source status pass.

The remediation `/code-review` covered exact committed range
`954c81d31438cad762e43ccb0b0e0f8a9f7b6a66...14b1ada6f6294ca922386c09746deed384aa2b69`.
The independent Standards axis reported no documented-standard violation or smell
judgment. The independent Spec axis reported one missing evidence requirement:
neither required record yet contained the review range and outcome. That finding
is accepted and resolved by this paragraph and the matching validation entry. The
correction changes evidence only, so no second review run is required; the final
12-test focused path suite, 62-test workspace suite, traceability, scoped diff, and
source worktree checks pass.
