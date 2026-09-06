# Running the Rust tests

Decision sources: [M3-upgrade — Test harness ergonomics](../../issues/implementation/m3-upgrade-test-harness/README.md) and [Make the Rust test suite cheap to iterate on](../../issues/implementation/m3-upgrade-test-harness/tickets/001-test-harness-ergonomics.md).

Two scripts. `scripts/loop.sh` is what you run after every edit. `scripts/gate.sh` is
what you run before you close anything.

## The loop

```sh
scripts/loop.sh                  # every test in spielorders-core and spielorders-cli
scripts/loop.sh order_creation:: # one module
scripts/loop.sh order_creation::an_order_that_vanishes_before_the_refresh_still_reports_its_commit
```

It is `cargo test --tests` with your arguments appended, so the argument is a test-name
filter: a module prefix selects that module, a full path selects one test. Each crate's
`tests/` directory is one integration-test target, so `--test <name>` does not resolve;
filter by module prefix instead.

`default-members` in the workspace manifest holds a bare `cargo` command to the two
crates with behavior, so the loop never builds the Tauri crate's dependency tree.

### Adding a test file

A new `crates/<crate>/tests/<name>.rs` needs `mod <name>;` in that crate's
`tests/main.rs`. The crates set `autotests = false`, so an undeclared file is compiled by
nothing and run by nothing, and nothing warns you. The test count in the run summary is
the check.

## The gate

```sh
nvm use && scripts/gate.sh
```

In order: `cargo fmt --check`; `cargo clippy --workspace --all-targets --locked -- -D
warnings`, started in the background against its own `target/lint` so it overlaps;
`cargo test --workspace`, which adds the Tauri crate and the doctests; then
`npm run test:schemas` and `npm run check:traceability`, which need the pinned Node. It
prints `gate passed` on success.

The gate also cleans the target directory itself. Cargo never removes the object files a
previous build left behind, and past a few hundred thousand entries every write into
`target/debug/deps` slows down, so the gate runs `cargo clean` when the directory has
accumulated and says so in one line.

## Timings

| | |
| --- | --- |
| loop, warm, one module | 1.4 s |
| gate, warm | 28 s |
| cold build of every test binary | 26 s |
