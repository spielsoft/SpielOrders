---
title: Adopt sha2 0.11.0 and refresh the M4 dependency graph
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-02T01:46:36Z
closed_at: 2026-09-02T04:12:00Z
review_start: 5a691d237cabef39aa18e7848a23c014d3efaa23
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

M4 builds on the dependency graph it will close on. The one new Cargo major the owner has
approved is adopted, the exact pin it was holding down is released, and every other
selected major is at its newest release or has a reported reason it is not.

## Decision sources

- [SpielOrders agent instructions](../../../../AGENTS.md)

## Result

`spielorders-core` selects `sha2` 0.11.0. The 0.11 `Digest` API is source-compatible, so
`crates/spielorders-core/src/digest.rs` is unchanged, and the digest strings were measured
byte-identical against an independent SHA-256 on both versions. Nothing durable ever
carried a digest.

Two facts a later dependency bump needs. The `=0.14.7` `generic-array` pin is released
from every crate this repository owns; it survives only on Tauri's build-time proc-macro
path, `tauri-macros` to `tauri-codegen` to `sha2 ^0.10`, which is host-built, absent from
the shipped binary, and not removable by any published upstream release. And read the
resolved graph per target: `cargo tree --target all` unions every target's `cfg`, Android
included, so it reports edges that never activate.

Toolchains and every direct selection are newest in their selected major, per the
dependency-currency rule in [SpielOrders agent instructions](../../../../AGENTS.md). The
existing suites supply the preservation evidence; no test module is this ticket's.
