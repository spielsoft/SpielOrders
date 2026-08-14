---
title: Preserve one canonical file through validated atomic replacement
status: closed
assignee: codex
claimed_at: 2026-07-24
closed_at: 2026-07-24
parent: M1 — Filesystem and metadata kernel
labels:
  - implementation:m1
  - implementation:task
---

## Outcome

A core caller can replace one managed file using beside-destination staging,
candidate validation before atomic replacement, and exact preservation of the prior
valid destination when replacement fails.

## Decision sources

- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Unblocked. [Round-trip the complete metadata-version-1 vocabulary through the Rust core](001-metadata-v1-vocabulary.md)
closed on 2026-07-22.

## Scope

- Expand the filesystem effect only as required for beside-destination staging,
  durability, atomic rename/replacement, and cleanup.
- Validate staged candidate bytes before replacement.
- Inject deterministic failures at every filesystem boundary.
- Preserve unrelated files and opaque Supporting Material.

Exclude domain mutation APIs, Package initialization, README workflows, index
refresh, upgrades, directory mutations, automatic promotion or deletion of abandoned
staging evidence, and any shared SpielGantt crate or database abstraction.

## Pre-agreed public test seam

A public low-level validated-replacement capability accepts a destination, candidate
bytes, and artifact validation behavior through the filesystem port. Tests observe
destination bytes and the typed result. The tracer bullet replaces one valid
`order.json` in a real temporary directory. Only the filesystem boundary is faked
for failure injection.

## Acceptance criteria

- A valid candidate atomically becomes the exact destination bytes.
- Candidate-validation failure preserves the old destination byte-for-byte.
- Staging write, validation read, durability, or replacement failures preserve the
  prior valid destination.
- Staging occurs beside the destination on the same filesystem.
- No unrelated entry or Supporting Material is read, moved, or rewritten.
- The implementation remains SpielOrders-local.

## Review boundary

Review one-file durability and failure behavior only; no persistence workflow or
domain operation belongs here.

## Ticket-start fixed point

`dacf53c5a14f155db67b8a9b21b543f0d6d31dd5`

## Resolution

Added a public SpielOrders-local validated single-file replacement primitive. It
creates a non-overwriting staging file beside the destination, rereads and validates
the staged bytes, makes the staged file durable, and delegates atomic replacement to
the filesystem effect. Typed failures distinguish invalid destinations, staging
writes, validation reads, candidate validation, durability, and replacement while
retaining any cleanup failure. Failed operations preserve the prior destination;
owned staging is cleaned where possible, while pre-existing or uncleanable staging
evidence is never promoted, overwritten, or silently deleted.

Expanded the filesystem effect only with create-new staging, file durability, atomic
file replacement, and cleanup operations. No Package loading, domain mutation,
initialization, README/index workflow, upgrade, directory mutation, shared crate, or
database abstraction was added.

Implementation commits:

- `924866e` — validated atomic replacement, port expansion, real tracer bullet,
  deterministic fault injection, and compatible dependency update.
- `d845e6d` — accepted `/code-review` traceability and test-maintainability findings.

Acceptance evidence (2026-07-24, Rust 1.97.1):

- `cargo test -p spielorders-core --test atomic_replacement` — passed 8 public
  behavior tests covering exact real-filesystem replacement, invalid candidate
  preservation, staging-write/read/durability/replacement failure injection,
  cleanup failure reporting, beside-destination staging, unrelated-content and
  Supporting Material preservation, and abandoned-evidence preservation.
- `cargo test -p spielorders-core --test package_paths` — passed 10 safe-path and
  real-filesystem containment tests after the filesystem-port expansion.
- `cargo test --workspace` — passed the complete Rust workspace suite.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `npm run test:schemas` — passed 4 schemas, 8 accepted examples, 17 rejected
  fixtures, 22 scenarios, and 2 preservation sentinels.
- `npm run check:traceability` — passed for 7 M0 tickets and 4 baseline test files.
- Dependency currency audit updated compatible transitive `cc` 1.3.0 to 1.4.0.
  A final `cargo update --dry-run --verbose` found zero compatible updates, and both
  npm trees reported no outdated packages. Remaining older transitive releases are
  constrained upstream; `toml` 1.1.3 is an unapproved major version and was not
  adopted.
- `/code-review` from the exact ticket-start fixed point reported zero Spec
  findings. The Standards axis reported one milestone claim-tracking violation and
  two duplication judgments; all were accepted and resolved by updating milestone
  state, sharing real-filesystem test support, and consolidating failure-scenario
  setup and preservation assertions. These changes did not alter the public design,
  so a second review run was not required.
