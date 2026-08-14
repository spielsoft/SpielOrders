---
title: Load a current Package Index without canonical record rereads
status: open
parent: M2 — Package loading, diagnostics, and Package Index
labels:
  - implementation:m2
  - implementation:task
---

## Outcome

When a valid version-1 Package Index is proven current by conservative portable
no-follow change evidence, ordinary Package opening returns the complete available
projection without opening `vendors.json` or every `order.json`, while exposing
only evidence actually observed on that fast path.

## Decision sources

- [Decide how Package Index freshness is proven without canonical rereads](../../../wayfinder/m2-package-read-model/tickets/001-index-freshness-fast-path.md)
- [Define the stable M2 diagnostic vocabulary and evidence contract](../../../wayfinder/m2-package-read-model/tickets/002-stable-diagnostic-contract.md)
- [Define the complete M2 Package read projection and load-state contract](../../../wayfinder/m2-package-read-model/tickets/003-package-read-projection.md)
- [Decide whether SpielOrders and SpielGantt share a filesystem-bucket foundation](../../../wayfinder/spielorders-product-architecture/tickets/001-shared-filesystem-foundation.md)
- [Define the SpielOrders package topology](../../../wayfinder/spielorders-product-architecture/tickets/002-package-topology.md)
- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define integrity, validation, and repair boundaries](../../../wayfinder/spielorders-product-architecture/tickets/008-integrity-repair.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Rebuild the Package Index and preserve replacement-failure evidence](003-rebuild-package-index.md).

## Scope

- Extend the existing filesystem port only with the smallest portable no-follow
  change observation needed by the closed cutoff: entry kind and comparable
  content-or-replacement evidence, or explicit uncertainty.
- Keep observations in memory only. Do not persist timestamps, identities,
  revisions, diagnostics, or a sidecar and do not claim tamper-proof detection.
- On opening, read/validate only `config.toml`, `version.json`, and `index.json`;
  resolve the configured Orders Directory; list its direct children; and observe
  exact no-follow kind/change facts for `vendors.json`, every non-hidden ordinary
  candidate's exact `order.json`, and its exact conventional `README.md`.
- Prove freshness only when the validated deterministic index's paths exactly
  match candidate membership, schema version agrees, required files are ordinary,
  and index replacement evidence is strictly later than every canonical and
  present README input. Equal, newer, missing, unavailable, incomparable, or
  contradictory evidence selects Ticket 003's canonical rebuild path.
- Source from a proven-current index only the permitted complete healthy Order and
  Vendor facts/identities, matched Order paths, ordered Items/Contacts, cached
  Order README text, totals, resolved Vendor associations, and four reusable-value
  lists.
- Keep config/version facts, configured paths, direct-child membership/warnings,
  freshness observations, and index state sourced from their own effects.
- Deliberately omit latent Vendors/Order/README/reserved/staging diagnostics and
  every unexamined damage, upgrade-execution, operation-failure, Package README,
  or Supporting Material fact. Package and Order README references remain JIT;
  cached text is not a claim of current availability.
- Return `current` / `not_needed` on success and preserve exact deterministic
  collection/absence semantics.

Exclude exhaustive validation, eagerly reading all READMEs, a recursive change
scan, filesystem watchers, hashes/revisions, adversarial tamper guarantees, schema
changes, and adapter behavior.

## Canonical fixtures and fault injection

- Reuse the healthy, direct-child, drift, README, and stale-index scenario
  foundations plus minimal/full version-1 index examples.
- Add narrow fake observations for strictly older inputs, equal evidence, each
  newer input kind, unavailable/incomparable/contradictory evidence, membership
  add/remove/rename, path mismatch, and wrong no-follow kinds.
- The public filesystem double must record every read/list/observation. A passing
  fast-path test fails if it opens `vendors.json`, any `order.json`, any README, or
  an unlisted path. Each uncertain/failing observation must visibly route to the
  existing canonical rebuild or typed operation result as decided.
- Add a portable real-filesystem observation check only to the extent needed to
  prove the port can conservatively report comparable evidence or uncertainty;
  do not make platform timestamp precision a persisted/public contract.

## Supporting Material preservation requirements

The fast path may list only the Orders Directory direct children and observe only
the exact named managed/README inputs. It must not list an Order Directory or
recurse. General files, nested directories, and links in Supporting Material may
not contribute change evidence. Tests must prove both opaque sentinel paths are
absent from effect logs and remain byte-identical.

## Core/filesystem-port and adapter boundary

The Rust core owns cutoff comparison, membership proof, uncertainty fallback,
index sourcing, totals/Vendor/value derivation, and evidence visibility. The port
supplies no-follow observations without platform strings. Production adapters may
return uncertainty; CLI, Tauri, and React do not implement or reinterpret the
cutoff and receive no new presentation behavior.

## Acceptance criteria

- A current-index load reads the three small root artifacts, lists direct children,
  performs only authorized exact observations, and opens neither `vendors.json`
  nor any `order.json` or README.
- Freshness requires strict-later comparable evidence and exact membership/path/
  kind/version/deterministic-index validity. Every conservative failure routes to
  canonical rebuild with the correct ordered stale reason.
- The resulting healthy Order/Vendor projection, totals, live associations, and
  reusable values equal canonical loading for the same facts.
- Fast results contain no latent canonical, document, reserved/staging, or
  Supporting Material evidence and never treat `index_readme_text` as a fresh JIT
  read.
- Filesystem enumeration order, source errors, and untrusted platform detail do not
  affect the result.
- Metadata version 1 and exact index bytes remain unchanged.

## Review boundary

Review the portable cutoff, exact fast-path read budget, permitted index sourcing,
and deliberate absence of unexamined evidence. Reject a sidecar, schema revision,
recursive scan, eager README reads, duplicated adapter rule, search field, or
stronger security claim.

## Exact fixed-point claim protocol

Require Tickets 001–003 closed, this ticket open/unclaimed, and an empty
`git status --porcelain=v1 --untracked-files=all`. Record the exact
`git rev-parse HEAD` as `ticket_start_fixed_point`, claim with `status: in progress`,
`assignee`, and `claimed_at`, and commit that claim alone. Review always uses the
pre-claim SHA. Stop on any mismatch.

## Implement, TDD, and code-review workflow

Use the project-local `implement` skill and the project-local `tdd` skill. The first public RED test is a
strict read-budget test for a proven-current index; it catches an implementation
that silently rereads canonical records. Add one RED-to-GREEN behavior at a time
for permitted projection, each conservative cutoff fallback, exact evidence
absence, and portable/fault-injected observations. Do not batch a port/type layer
before observable behavior.

Commit all work before the project-local `code-review` skill compares the ticket
fixed point to HEAD. Resolve and record independent Standards/Spec results, rerun
affected gates, commit the disposition, and close only this ticket.

## Required verification

Focused gates:

- dedicated current-index fast-path/read-budget integration suite;
- index rebuild, canonical projection, filesystem-port, README, and path suites;
- portable observation and complete fake-failure matrix; and
- `cargo test -p spielorders-core --locked`.

Repository gates:

- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `npm run test:schemas`;
- `npm run check:traceability`;
- `git diff --check <ticket_start_fixed_point>...HEAD`;
- core/adapter dependency and prohibited-scope audit; and
- exact before/after hashes for both opaque sentinels.

Use pinned Node and do not update dependencies or lockfiles.

## Handoff requirement

After closure, generate the complete next-worker prompt for the first eligible
ticket—expected to be [Validate exhaustive managed evidence without inspecting Supporting Material](005-validate-exhaustive-managed-evidence.md)—with
all fixed-point, one-ticket, TDD/review, preservation, verification, and handoff
requirements. Report a blocker/claim rather than selecting later work.
