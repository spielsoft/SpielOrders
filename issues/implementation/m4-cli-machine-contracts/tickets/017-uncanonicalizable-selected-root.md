---
title: Refuse an uncanonicalizable selected root without panicking
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-02T14:00:35Z
closed_at: 2026-09-02T15:07:43Z
review_start: b310b2290d23b6585feb2094e4807b65369f5369
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

A caller-supplied Package root the filesystem cannot canonicalize is reported as a typed
load failure, whatever that path looks like. No public core entry point aborts the process
because the caller passed a relative path.

## Result

`FailedPath` in `crates/spielorders-core/src/operations.rs` is an operation failure's
subject: `Located(Location)` where this application can express the path as one, and
`Unlocated(PathBuf)` where it cannot, so a root that will not canonicalize is named back
exactly as it arrived. `OperationFailure::filesystem` takes `impl Into<FailedPath>`, so
every construction site is unchanged; `primary_location()` is now `Option<&Location>` and
`primary_path()` is total. `Location` is untouched.

Sixteen of the twenty-one public core entry points that take a caller-supplied Package
Root route through the repaired branch, and each now returns its own typed failure naming
the caller's path where it previously aborted the process. Five never reach it.

The rule later tickets consume: no public core entry point aborts because a caller passed
a path the filesystem cannot resolve.

Pinned by `no_public_entry_point_aborts_on_a_root_it_cannot_resolve` in
`crates/spielorders-core/tests/canonical_package_load.rs`.
