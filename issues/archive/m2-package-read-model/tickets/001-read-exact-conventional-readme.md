---
title: Read one exact conventional README through the shared core
status: closed
ticket_start_fixed_point: 3ce6265e76d61771dcc45b40bee6f6c831939d4f
assignee: /root
claimed_at: 2026-08-14T20:23:38Z
closed_at: 2026-08-14T20:48:01Z
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

## Implementation evidence (pre-review)

- Exact pre-claim fixed point:
  `3ce6265e76d61771dcc45b40bee6f6c831939d4f`.
- Claim commit: `e4b3f70` (`Claim M2 Ticket 001 README read`).
- Implementation commit: `0cc1b6a` (`Implement exact JIT README reads`).
- Public RED-to-GREEN tracer bullets covered absent-as-empty first, exact UTF-8,
  no-follow wrong-kind handling, injected read failure, invalid encoding, injected
  observation failure, version-1 cache non-evidence, absolute and normalized
  Package-relative locations, exact effect budgets, stable vocabulary/equality/order,
  and byte-preserving real-filesystem sentinel coverage. The filesystem port was
  the only fake seam; no standalone filesystem-port implementation changed.
- `cargo test -p spielorders-core --test readme_read --locked`: pass, 11 tests.
- `cargo test -p spielorders-core --locked`: pass.
- `cargo test --workspace --locked`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: the first run
  found one test-only `useless_vec`; after replacing it with an array, the focused
  suite and the complete clippy command passed.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas`: pass
  (`4` schemas, `8` accepted examples, `19` rejected fixtures, `22` scenarios,
  and `2` preservation sentinels), and `npm run check:traceability`: pass.
- `git diff --check 3ce6265e76d61771dcc45b40bee6f6c831939d4f...HEAD`: pass.
- Independent sentinel hashes before and after the implementation are identical:
  `supporting-a.bin` =
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`;
  `supporting-b.bin` =
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`.
- No dependency, lockfile, schema, metadata-version, adapter, CLI, Tauri, React,
  Package-load/index, README-write, M3, install-script, or SpielGantt change was
  made.

## Code review and disposition

Review completed at `2026-08-14T20:47:14Z` against the exact commands
`git diff 3ce6265e76d61771dcc45b40bee6f6c831939d4f...HEAD` and
`git log 3ce6265e76d61771dcc45b40bee6f6c831939d4f..HEAD`. Standards and Spec
were reviewed independently and in parallel as required by the project-local
`code-review` skill.

### Standards

- Outcome: `0` documented-standard violations and `2` smell-baseline judgement
  calls.
- Accepted Duplicated Code finding: the three repeated unavailable-warning
  assertion blocks now use one focused `assert_unavailable` helper.
- Dispositioned Refused Bequest finding: no second narrow filesystem trait was
  introduced. Ticket 001 and the M2 shared operating contract explicitly require
  the existing filesystem port to remain the sole effect/fake boundary, so the
  documented repository/spec rule overrides this heuristic.
- Post-disposition outcome: no unresolved Standards finding.

### Spec

- Outcome before remediation: `2` findings.
- Accepted exact-reference finding: `ReadmeReference::new` now rejects missing
  Package-relative evidence, non-`README.md` paths, mismatched absolute/relative
  paths, and Package/Order kind mismatches; the observation-failure fixture now
  uses an exact established Package README reference.
- Accepted ordering finding: `Location` ordering now uses normalized
  Package-relative paths when both are established and falls back to absolute
  paths only when relative evidence is absent. The public test reverses absolute
  and relative lexical order to prove the closed rule.
- Remediation commit: `d47db73` (`Resolve Ticket 001 review findings`).
- Review-disposition evidence commit: `6e92d1c`
  (`Record Ticket 001 review disposition`).
- Post-remediation outcome: no unresolved Spec finding and no scope creep retained.

### Post-review verification

- `cargo test -p spielorders-core --test readme_read --locked`: pass, 12 tests.
- `cargo test -p spielorders-core --locked`: pass.
- `cargo test --workspace --locked`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- Under `.nvmrc` Node `v26.5.1` and npm `12.0.2`, `npm run test:schemas` and
  `npm run check:traceability`: pass.
- `git diff --check 3ce6265e76d61771dcc45b40bee6f6c831939d4f...HEAD`: pass.
- Sentinel hashes remain
  `a78997f8accf7ad0fcb84540b6f7855762155f63244a39da5b361fd619d362f9`
  (`supporting-a.bin`) and
  `7efa163282540f2e49f63c02b4ca968f5e03af57815161d50cb51fa565b75b42`
  (`supporting-b.bin`), identical to the recorded before values.

## Result

Ticket 001 is complete. The shared Rust core now reads exactly one established
conventional Package or Order README through the existing filesystem port with the
closed absence, UTF-8, wrong-kind, unreadable, invalid-encoding, stable evidence,
path, effect-budget, cache non-evidence, and Supporting Material preservation
semantics. Ticket 002 and every later M2 capability remain unimplemented.

## Handoff requirement

After closure, report the final SHA and the next eligible ticket—at that point,
[Classify and project canonical Package state](002-classify-project-canonical-package.md).
The next worker derives claim, implementation, verification, and review rules from
the milestone index and project-local skills; do not copy them into a prompt.
