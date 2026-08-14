---
title: Read one exact conventional README through the shared core
status: open
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

A Rust caller can request exactly one Package or Order README and receive its exact
reference, observed existence, UTF-8 text or unavailable-document warning, without
listing the containing directory or treating cached index text as current evidence.

## Decision sources

- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

None. This is the first open M2 ticket and may start only from a clean worktree
containing the committed decision-complete M2 decomposition.

## Scope

- Expose one public Rust-core README-read capability for an exact Package or Order
  `ReadmeReference` supplied with its established location.
- Establish the common `Location` representation needed by this capability:
  absolute native path plus normalized Package-relative path when Package
  membership is established; never follow the final entry merely to make it
  absolute.
- Return the exact closed read result semantics: absent is `exists: false` with
  present empty text and no diagnostic; readable UTF-8 is `exists: true` with exact
  decoded text; wrong-kind, unreadable, or invalid encoding is `exists: true` with
  absent text and one `readme_unavailable` document warning.
- Establish the stable common diagnostic envelope, document scope, severity,
  locations, typed evidence, deterministic equality/order, and only the code and
  evidence variants exercised by this complete capability. Later tickets extend
  the same vocabulary alongside their observable behavior.
- Ensure Package and Order README references are exact and Package README text is
  never sourced from metadata version 1. An Order's version-1 `index.readme` value
  is not accepted as evidence by this operation.

Exclude README writes, eager reads during Package opening, Markdown rendering,
editor state, CLI path-only output, Tauri commands, frontend presentation, and all
Package/index orchestration.

## Canonical fixtures and fault injection

- Reuse the canonical `README absence` and `README unreadable` scenarios in place.
- Add only narrowly scoped public Rust fixtures needed to distinguish Package from
  Order references, ordinary UTF-8 text, wrong-kind, injected read failure, and
  invalid UTF-8. Do not invent README metadata or copy the binary assets.
- The filesystem double must record every effect and prove the capability observes
  and, when appropriate, reads only the exact requested path. Inject observation
  and read failures through the existing filesystem port, not private mocks or OS
  error-string matching.

## Supporting Material preservation requirements

The operation may observe only the exact conventional README path. It must not list
the Package root or Order Directory, recurse, inspect any sibling, or classify any
other content. Tests must place both opaque sentinels beside or below an Order and
prove their bytes are unchanged and neither path appears in the effect log.

## Core/filesystem-port and adapter boundary

The Rust core owns the reference, absence semantics, UTF-8 decision, diagnostic
code/evidence, and deterministic result. The existing filesystem port supplies
no-follow kind and bytes or an opaque error. No CLI envelope, Tauri command, React
model, editor behavior, or retry presentation is introduced.

## Acceptance criteria

- Package and Order references carry exact absolute and Package-relative locations;
  unestablished relative evidence is absent rather than fabricated.
- The four observable outcomes—absent, readable UTF-8, wrong kind, and unavailable
  bytes/read—match the closed result and warning contract exactly.
- Missing returns present empty text; unavailable returns absent text. Collections
  in the warning envelope remain present when empty.
- A later explicit read never accepts version-1 cached Order README text as current
  evidence.
- Effect logs prove no directory listing, sibling read, recursive operation, link
  following, write, replacement, or cleanup occurs.
- Diagnostic equality and ordering ignore platform/source messages and use only
  stable typed evidence.

## Review boundary

Review one exact JIT document read and the minimum stable evidence vocabulary it
makes observable. Reject a diagnostics-only type foundation, Package loading,
index work, README mutation, adapter behavior, or inspection beyond the requested
path.

## Exact fixed-point claim protocol

Before editing, verify every earlier milestone is closed, this ticket is open and
unclaimed, and `git status --porcelain=v1 --untracked-files=all` is empty. Record
the exact output of `git rev-parse HEAD` as `ticket_start_fixed_point`; then set
`status: in progress`, `assignee`, and `claimed_at` in this ticket and commit that
claim alone. Use the recorded pre-claim SHA—not `main` or another floating ref—for
all later review commands. Stop on any mismatch or dirty state.

## Implement, TDD, and code-review workflow

Read and use the project-local `implement` skill and the project-local `tdd` skill. Start with one public
core RED test for absent-as-empty and state the real defect it catches; make it
GREEN minimally, then add one behavior at a time for exact UTF-8, each unavailable
reason, evidence, path facts, read budget, and sentinel preservation. Fakes are
allowed only at the filesystem port.

After all behavior and evidence are committed, use the project-local `code-review`
skill against `git diff <ticket_start_fixed_point>...HEAD` and
`git log <ticket_start_fixed_point>..HEAD`. Resolve and record Standards and Spec
findings separately, rerun affected checks, and commit the final disposition before
closing only this ticket.

## Required verification

Focused gates:

- the dedicated public Rust README-read integration test;
- any directly affected filesystem-port unit test; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`; and
- independent before/after hashes for both opaque Supporting Material sentinels.

Run under the repository-pinned Node version where npm is involved. Do not update a
dependency or lockfile in this ticket.

## Handoff requirement

After closure, inspect the milestone index and every M2 ticket in order. Generate a
complete ready-to-paste prompt for a fresh worker to claim and implement only the
first open, unblocked, unclaimed ticket—expected to be
[Classify and project canonical Package state](002-classify-project-canonical-package.md)—under
the same fixed-point, one-ticket, TDD, preservation, review, verification, and
handoff rules. If it is blocked or claimed, report that state instead.
