---
title: M5 acceptance record
status: remediation checks passed; fresh external revalidation required
gate_ticket: tickets/012-complete-stopped-search-and-fixture-checks.md
candidate: 2c281b8740bb91f0293cdaf0b88185f59c14d2a5
---

# M5 acceptance record

Search remediation passes internal checks. M5 still requires fresh external revalidation;
this record does not supply it.

## Candidate and checks

The clean candidate is `2c281b8740bb91f0293cdaf0b88185f59c14d2a5`. Toolchains match the
selectors: Rust/Cargo 1.99.1, Node 26.13.0, npm 12.2.1.

`scripts/gate.sh` passes with host Spotlight access: formatting, workspace clippy with
warnings denied, workspace tests, schema contracts, and traceability. Desktop tests and
frontend build evidence and Tauri's `--no-bundle --debug` build apply unchanged: this
follow-up changes no desktop or core input and launches no application.

## Remediation evidence

[Restore search stopping, Unicode matching, file paths, and response evidence](tickets/011-repair-search-validation-findings.md)
owns the fixes. Focused RED runs demonstrate the original stdout-EOF timeout failure,
Greek-sigma search omission, legal backslash filename omission, and missing success cell.
The corresponding regressions pass, including cancellation after work starts, subprocess
reaping, expanding Unicode folds, case-sensitive controls, and native filename response
projection. Strict managed-path checks still pass.

[Keep queued search output, final exits, and real fixture comparisons](tickets/012-complete-stopped-search-and-fixture-checks.md)
retains the synchronized child-start handshake. Real-process RED checks expose 8,192 bytes
returned from 11,640 queued bytes, and a completed query mislabeled timed out. The corrected
checks retain all queued paths and preserve success and nonzero exits at both stop boundaries.
The drain reads a finite nonblocking snapshot, never waits for EOF, and leaves no reader thread.

The success fixture is captured from the shipped executable querying real Spotlight over
an owned synthetic Package: `coherence` finds `quote-coherence.txt` inside a healthy Order.
The Order facts and file modification time are fixed. Only the Package root is substituted;
no placeholder is added. The inventory executes a complete byte comparison before registering
success, using the production projection and encoder without live indexing. An isolated
corrupted-fixture probe passes against the defective inventory and fails against the corrected
one. Follow-up logs are at `/private/tmp/spielorders-m5-followup.gdaZYJ/`; capture evidence is
at `/private/tmp/spielorders-m5-fix.Y3GBML/`.

Durable schemas, examples, scenarios, and preservation fixtures are unchanged. Contract
and metadata versions remain at their initial value. Both Supporting Material sentinel
hashes match the failed validation's inputs.

## Native handoff evidence

The owner's `scripts/smoke-spotlight.sh` attestation supplies the native open/reveal proof:
FOUND reports the nested Vendor README and text file, excludes the Order README, then
OPENED and REVEALED bring TextEdit and Finder forward. Remediation does not change the
opener. This run neither repeats that owner-only script nor changes indexing settings.

## Currency and authority

Official Rust/Node release indexes, npm packuments, and crates.io verify current direct
selectors within their selected majors. Whole-graph Cargo and npm updates are applied;
Cargo's dry run proposes no further resolvable update and npm reports no installed package
behind its allowed selector. Both npm audits report no vulnerabilities. Lifecycle scripts
remain unapproved; blocked `fsevents` is denial evidence.

Upstream selectors hold `generic-array` at 0.14.7 rather than 0.14.9 (`crypto-common` exact
requirement), and the old TOML branch at `toml` 0.8.2, `toml_datetime` 0.6.3, and
`toml_edit` 0.20.2 rather than 0.8.23, 0.6.11, and 0.20.7 (`proc-macro-crate` exact pins).
Those TOML copies serve the non-macOS GTK graph. Rolldown pins `@oxc-project/types` to
0.156.0 rather than 0.157.0; Node types require `undici-types ~8.9.0`, excluding 8.12.3.
Newer `fast-uri` and `nanoid` majors are outside upstream selectors.

## Review

Independent parallel Standards and Spec reviews report no findings on the follow-up.
Owner-approved transitive selections remain unchanged. Query scope follows
[Define lookup and search behavior](../../wayfinder/spielorders-product-architecture/tickets/005-search-lookup.md).
