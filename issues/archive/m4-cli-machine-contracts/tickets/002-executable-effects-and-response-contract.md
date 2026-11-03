---
title: Run the spielorders executable against a real Package
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-02T15:16:09Z
closed_at: 2026-09-02T18:15:00Z
review_start: 95c559a4038c0b377a6a53c06fae48d69917679f
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A user runs `spielorders package readme` in a Package and gets the conventional absolute
path on stdout at exit `0`, and under `--json` exactly one UTF-8 document carrying that
command's versioned response contract. The same command outside a Package fails at exit
`1`, and a misspelled option at exit `2`.

## Result

`spielorders` is a Git-style executable with generated help and version, the production
`PlatformFileSystem` effect, and `package readme` answering `spielorders.package.readme`
v1. The command is the subject; the shared response spine is the deliverable.

Every rule the later command tickets consume is stated where it is implemented, not here:
the response envelope in `crates/spielorders-cli/src/response.rs`, the `contract` identity
registry and the versioning rule in `contract.rs`, the four output modes and the
stdout/stderr split in `output.rs`, the three exit statuses in `exit.rs`, and the effect
boundary in `effects.rs`. The golden-fixture path derivation, the cell set, and the shared
`spielorders.usage` identity are in
[Decision traceability and contract-test conventions](../../../../docs/development/traceability.md).

Pinned by `crates/spielorders-cli/tests/`: `package_readme`, `output_modes_and_streams`,
`root_targeting`, `response_contract`, `platform_effects`, and
`effect_and_stream_boundaries`, which holds the crate to one effect boundary and one
stream boundary as sets rather than as diffs.
