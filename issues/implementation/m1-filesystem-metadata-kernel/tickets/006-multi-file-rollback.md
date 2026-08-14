---
title: Restore prior canonical state after a failed multi-file replacement
status: closed
assignee: codex
claimed_at: 2026-07-25
closed_at: 2026-07-25
remediated_at: 2026-07-25
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:task
---

## Outcome

A caller can submit a prevalidated multi-file replacement plan and receive either a
complete commit or a recoverable failure report after already-replaced managed files
are restored where possible.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Preserve one canonical file through validated atomic replacement](005-atomic-canonical-replacement.md)
closed on 2026-07-24.

## Scope

- Validate the complete plan before its first write.
- Apply deterministic operation order using the single-file primitive.
- Retain rollback material and attempt reverse restoration after a later failure.
- Surface both the initiating failure and every rollback failure.

Exclude Package initialization, Order/Vendor mutation, Metadata Upgrade sequencing,
index-refresh policy, package-wide transaction abstractions, directory rollback,
Supporting Material, cleanup UI, and stable user-facing diagnostics.

## Pre-agreed public test seam

A public low-level batch-replacement capability returns committed or
failed-with-rollback-details. Failure injection enters only through the filesystem
port. The tracer bullet replaces two valid canonical files; later tests fail the
second replacement and then separately fail restoration of the first.

## Acceptance criteria

- An invalid complete plan performs no filesystem writes.
- A valid plan replaces every named destination.
- A later failure restores every restorable prior file byte-for-byte.
- Rollback failure is explicit and cannot be mistaken for a clean rejection.
- Files outside the named managed destinations remain untouched.
- The primitive makes no M3 decision about a domain mutation's artifact set.

## Review boundary

Review recoverable multi-file mechanics only; domain orchestration and upgrade
semantics are excluded.

## Ticket-start fixed point

`5a3770e432eb0ba99a992f1886f992eec1a6dc75`

## Resolution

Added a public ordered multi-file replacement plan and recoverable batch primitive.
The primitive validates every candidate and the complete destination structure
before any filesystem effect, rejects ambiguous duplicate destinations, retains all
prior destination bytes before its first write, and applies the caller-declared
order through Ticket 005's validated single-file replacement primitive. A later
failure restores every already-replaced destination in reverse order through that
same primitive.

The typed failure retains the initiating plan, prior-read, or replacement error
separately from an ordered collection containing every rollback failure. Clean
pre-write rejection therefore cannot be confused with incomplete restoration. The
primitive makes no Package-domain decision about which artifacts belong in a plan,
and no Package loading, domain mutation, upgrade, index, directory, Supporting
Material, UI, diagnostic-catalog, shared-crate, or Ticket 007 behavior was added.

Implementation commits:

- `87923f9` — complete-plan validation, ordered replacement, retained rollback
  material, reverse restoration, explicit failure reporting, behavioral tests, and
  required compatible dependency updates.
- `cb6ce5e` — accepted `/code-review` test-maintainability finding.

Acceptance evidence (2026-07-25, Rust 1.97.1):

- `cargo test -p spielorders-core --test multi_file_replacement` — passed 5 public
  behavior tests covering aggregate invalid-plan rejection with no filesystem
  writes, duplicate/invalid destination rejection, a real two-file commit, exact
  restoration after the second replacement fails, reverse operation/rollback
  order, explicit multiple rollback failures, unrelated-file preservation, and
  both opaque Supporting Material sentinels.
- `cargo test -p spielorders-core --test atomic_replacement` — passed Ticket 005's
  complete 8-test staging, validation-read, durability, replacement, cleanup, and
  abandoned-evidence fault-injection matrix.
- `cargo test -p spielorders-core --test package_paths` — passed all 10 safe-path
  and real-filesystem containment tests.
- `cargo test --workspace` — passed 59 Rust unit/integration tests plus doc tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- `npm --prefix apps/desktop run test` and `run build` — passed after compatible
  in-major updates to Tauri API/CLI, Node types, React/ReactDOM, TypeScript 5, Vite,
  and its React plugin. AJV was already current. A current
  `cargo update --dry-run --verbose` found zero compatible Rust updates; the newer
  `toml` 1.x and TypeScript 7 releases remain unapproved major versions.
- `/code-review` from the exact ticket-start fixed point reported zero Spec
  findings and no documented-standard violations. One Standards-axis duplication
  judgment was accepted and resolved by consolidating the two filesystem test
  fakes. The test-only cleanup did not materially change the design, so a second
  review run was not required.

## External-validation remediation — M1-V1

Remediation fixed point: `3eeb36fa46f4b8874d2e0c1f508935251f62e3b1`

External validation found that two raw `Path` spellings could name one destination
while bypassing the complete-plan duplicate check. A public real-filesystem
regression now submits `.spielorders/vendors.json` and
`.spielorders/alias-parent/../vendors.json`, requires a typed duplicate failure
identifying operation indices 0 and 1, and proves before any write that the prior
destination, an unrelated file, and beside-destination staging state are unchanged.
The RED run returned `Ok(())`; the minimal GREEN resolves each destination through
the existing filesystem canonicalization seam before the first write, uses that
canonical spelling as plan identity, and retains identity-resolution errors as
typed plan failures.

This repair remains the low-level ordered multi-file primitive owned by this ticket.
It adds no Package mutation, loading, diagnostic, index, upgrade, directory,
Supporting Material, adapter, shared-crate, or M2 behavior. Canonical identity
includes lexical and symlink resolution supplied by the filesystem; distinct hard
links remain distinct because inode identity is not part of the selected contract.

Remediation implementation commit:

- `8d6b027` — public RED regression, canonical destination identity, typed identity
  failure, Ticket 006 record, and external-validation record.

Clean-copy evidence (2026-07-25, exact commit `8d6b027`, Rust/Cargo 1.97.1,
Node.js 24.18.0, npm 11.17.0):

- `cargo test -p spielorders-core --test multi_file_replacement --locked` — passed
  6 tests, including aliased and exact duplicates, aggregate candidate validation,
  ordered commit, reverse rollback, rollback failure, unrelated content, and opaque
  Supporting Material preservation.
- The locked single-file and path suites passed 8 and 10 tests respectively.
- `cargo build --workspace --locked`, `cargo test --workspace --locked`,
  `cargo test -p spielorders-core --locked`, and locked workspace Clippy with
  warnings denied passed; the workspace and independent core runs each passed 60
  Rust tests plus doc tests.
- Schema and traceability checks passed with their existing counts. The desktop's
  2 frontend tests, production build, and Tauri no-bundle build passed.
- Both opaque Supporting Material SHA-256 values remained unchanged. The clean
  clone stayed source-clean after the gates.
- `cargo update --dry-run --verbose` found zero compatible lockfile updates. Both
  npm registry audits found zero selected-major updates; TypeScript 7.0.2, npm
  12.0.1, and `toml` 1.1.3 remain unapproved new majors. Official release data
  confirms Rust 1.97.1, Node 24.18.0, and npm 11.17.0 are current for the selected
  toolchain majors.
- The documented repository-wide `cargo fmt --all -- --check` continues to report
  only the same four trailing blank lines in unchanged M0 adapter stubs outside the
  remediation range. `cargo fmt -p spielorders-core -- --check` and
  `git diff --check` pass for the scoped repair.

`/code-review` reviewed the exact committed range
`3eeb36fa46f4b8874d2e0c1f508935251f62e3b1...ac71d13fa6ecf0faa447f1f6aeed6699f7fea7c3`.
The Standards axis reported one hard traceability finding: the milestone index and
roadmap had not inherited external validation's failed/M2-blocked state. The finding
was accepted and resolved by updating those status records without changing M1
behavior. The Spec axis reported zero findings. Because the review correction is
evidence/status prose only, no second review run was required; `git diff --check`
and the traceability check passed afterward.
