---
title: M5 external validation
status: passed
target: 9890b1ab0c0ba0b97542368b1dc3540693ea72e0
baseline: a0a37ce7939a5027e6d607537d65e0b86f836ed5
gate_runs: 4
gate_runs_failed: 0
standards_blocking: 0
standards_nonblocking: 3
spec_blocking: 0
spec_nonblocking: 2
validation_blocking: 0
---

# M5 external validation

M5 passes external revalidation. Each correction in
[Keep queued search output, final exits, and real fixture comparisons](tickets/012-complete-stopped-search-and-fixture-checks.md)
reaches the shipping code, each of its regressions fails when that correction is reverted,
and the gate passes in every run, sandboxed and with host Spotlight access. M6
implementation is no longer blocked by M5.

## Verified behavior

All probes ran in a detached clone of the target; the workspace was never mutated.

- **Success fixture.** The inventory registers the success cell only by running a byte
  comparison. That comparison projects the snapshot through the command's own response
  builder and the executable's JSON encoder, the same path `present` writes through.
  A corrupted fixture fails the inventory. Removing the comparison call fails the
  inventory's directory-set check. Registering the cell by name alone lets a corrupted
  fixture pass the whole CLI suite.
- **Queued output after a stop.** The regression drives the shipping `MetadataTools`
  runner against a real subprocess. With the stop-path drain reverted to a single read,
  the runner keeps one read buffer of the queued paths and the regression fails.
  - A probe that is not checked in had a descendant hold stdout open past the stop, either
    silent or writing continuously. The runner returns promptly with the queued paths at
    both the timeout and the cancellation boundary.
  - A drain that waits for EOF passes every checked-in test and fails that probe.
- **Observed exit.** Reverting the precedence makes the regression fail for both a
  successful and a nonzero exit. Reverted, the status is sampled before the limits and a
  crossed limit wins. With the correction, a nonzero exit stays a refusal at both
  boundaries.
- **Synchronized startup.** The probe delays the child past the bound before it writes.
  That child fails the pre-fix shape of
  `active_queries_stop_with_partial_results_even_after_stdout_closes` with empty output,
  and passes the current test. The runner tests pass on every repeated run at the clean
  target.

## Gate and boundaries

`scripts/gate.sh` with the selected toolchains exits zero in three sandboxed runs and one
run with host service access. All four runs execute the same set of tests, so no failure
needed a sandbox-or-host judgement.

Toolchains are the newest releases within their selected majors. Cargo's dry run proposes
no update; the only crates behind latest are the upstream pins that
[M5 acceptance](ACCEPTANCE.md) records. npm reports nothing outdated in either project.
Nothing changed in dependencies, install-script authority, or indexing settings. The
owner-only application-launching smoke was not rerun.

## Review

The independent parallel Standards and Spec reviews report no blocking findings. Their
non-blocking findings:

- No checked-in runner test holds a pipe open past the stop. The bounded drain is
  therefore guarded only by inspection and by this validation's probe.
- A tool that exits between the runner's last status check and the kill is reported as
  stopped.
- Comments in the CLI test suite say every response cell is tied to bytes a real process
  wrote. The in-process success comparison does not do that.
- The test-only recorded index and cancellation fakes are duplicated across test files.

Gate logs, probe scripts, mutation logs, and review findings are at
`/private/tmp/claude-502/-Users-example-Code-Apps-SpielOrders/9e5c719f-4f98-47bf-a7c8-ab4bb91d8db7/scratchpad/reval2/`.
