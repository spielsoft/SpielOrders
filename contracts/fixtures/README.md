# Canonical contract fixtures

This directory is the canonical fixture corpus shared by later Rust, CLI, Tauri,
and frontend contract tests. Accepted minimal and full JSON documents live in
[`../examples/v1`](../examples/v1); one-purpose rejected documents live in
[`schema-invalid`](schema-invalid); filesystem and cross-record recipes live in
[`scenarios.json`](scenarios.json). Consumers resolve all paths relative to
`contracts/` and must not copy fixtures into layer-specific test directories.

Scenario recipes are declarative so tests can materialize ordinary files,
directories, links, and unreadable/non-text entries portably. A `copy` source points
to one canonical contract file; `generated` names the smallest semantic overlay a
future test harness must build; `symbolic` records upgrade combinations that cannot
be instantiated without inventing an unaccepted metadata schema.

The two `assets/supporting-*.bin` sentinels are deliberately different. Tests that
materialize them must compare their bytes before and after metadata work. They are
opaque Supporting Material and must never be parsed as metadata.

Every scenario records schema validity, semantic validity, narrowest integrity
scope, warning state, upgrade state, and preservation expectations. The top-level
`decision_sources` names the closed decisions governing every entry. No fixture
contains real archive data, credentials, or a version-0/speculative version-2
migration.
