# Canonical contract fixtures

This directory is the canonical fixture corpus shared by Rust, CLI, and desktop
contract tests. Accepted minimal and full JSON documents live in
[`../examples/v1`](../examples/v1); one-purpose rejected documents live in
[`schema-invalid`](schema-invalid); filesystem and cross-record recipes live in
[`scenarios.json`](scenarios.json). Consumers resolve all paths relative to
`contracts/` and must not copy fixtures into layer-specific test directories.

Scenario recipes are declarative so tests can materialize ordinary files,
directories, links, and unreadable/non-text entries portably. A `copy` source points
to one canonical contract file, and `generated` names the smallest semantic overlay a
future test harness must build.

The two `assets/supporting-*.bin` sentinels are deliberately different. Tests that
materialize them must compare their bytes before and after metadata work. They are
opaque Supporting Material and must never be parsed as metadata.

Every scenario records schema validity, semantic validity, narrowest integrity
scope, warning state, and preservation expectations. The top-level
`decision_sources` names the closed decisions governing every entry. No fixture
contains real archive data or credentials.
