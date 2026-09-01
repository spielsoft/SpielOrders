# Running the Rust tests

Decision sources: [M3-upgrade — Test harness ergonomics](../../issues/implementation/m3-upgrade-test-harness/README.md) and [Make the Rust test suite cheap to iterate on](../../issues/implementation/m3-upgrade-test-harness/tickets/001-test-harness-ergonomics.md).

Two commands, and they are not interchangeable. The loop is what you run after every
edit. The gate is what you run before you close anything.

## The loop

```sh
cargo test -p spielorders-core --tests
```

That is the whole `spielorders-core` suite: the library's unit tests and all 480
integration tests. From a warm tree with nothing to rebuild, the compile step is under
a tenth of a second and the run is about 12 seconds of test execution. After changing a
core source, the recompile and link add about 1.3 seconds.

Every timing on this page was taken against a target directory built only by the
commands on this page. A target directory that has accumulated is a different machine
— see "When the loop is slow anyway" below, and check that first if these numbers look
nothing like yours.

The remaining 12 seconds are the tests themselves, not the build. They are
filesystem tests that call `fsync`, so they are I/O-bound rather than CPU-bound; the
build changes here cannot shorten them.

### One module, or one test

`crates/spielorders-core/tests/` is one integration-test target named `core`. Every
former `tests/<name>.rs` file is a module of it, so a former file's tests are selected
by module prefix rather than by `--test <name>`:

```sh
cargo test -p spielorders-core --test core -- order_creation::
```

A single test is its module path plus its name:

```sh
cargo test -p spielorders-core --test core -- order_creation::an_order_that_vanishes_before_the_refresh_still_reports_its_commit
```

`cargo test -p spielorders-core --test order_creation` no longer resolves; there is no
target by that name.

### Adding a test file

Declare it in `crates/spielorders-core/tests/main.rs` as `mod <name>;`. The crate sets
`autotests = false`, so an undeclared `tests/<name>.rs` is compiled by nothing and run
by nothing, and nothing warns you. The count in the run summary is the check: it should
go up by the number of tests you added.

## The gate

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace
npm run test:schemas
npm run check:traceability
```

Run the last two on the pinned Node runtime — `nvm use` first, per the root `README.md`.

### What belongs in the gate and not in the loop

`--workspace` reaches the Tauri desktop crate and its dependency tree, and
`cargo test --workspace` additionally compiles doctests. Measured against one warm
target directory, `cargo test --workspace` takes about 31 seconds where the loop
command takes about 12. Neither the workspace test nor the workspace clippy invalidates
anything the loop needs — the loop command straight after the gate is still about 12
seconds — so this is a cost you pay, not a cost you inflict on the next iteration. Pay
it once, at the gate.

## When the loop is slow anyway

Cargo never removes the object files a previous build left behind. Each rebuild writes
a fresh set under `target/debug/deps` with new hashes and abandons the old, so the
directory grows without bound for as long as the tree is worked in. Past a few hundred
thousand entries, every write into it slows down, and the loop degrades with it.

Measured on this repository's own `target/` at 844,028 files in `debug/deps`: the relink
after a core source change took 13 to 37 seconds and kept climbing within a single
session, against 1.3 seconds in a fresh directory on the same machine, at the same load,
on the same volume. The warm no-op compile step stayed at 0.08 seconds throughout, so
the symptom only shows once you actually change something.

`cargo clean` is the fix, and the loop returns to the timings above. Linking the suite
once rather than 33 times also slowed the accumulation itself: one cold build plus three
rebuild cycles left 27,946 files in `debug/deps` before the change and 3,291 after.

## What clippy does and does not cost

On the pinned toolchain, alternating `cargo clippy` and `cargo test` against one target
directory does **not** re-link the test binaries. `cargo clippy` produces check units,
which Cargo fingerprints separately from the build units `cargo test` links, so the two
sets coexist. Measured over three alternating rounds against one target directory, each
switch cost between 0.13 and 0.18 seconds, and no test target was rebuilt.

A separate `CARGO_TARGET_DIR` for lint runs is therefore not needed to keep the loop
fast. It buys one thing that is still real: Cargo takes an exclusive lock on a target
directory, so a lint run and a test run against the same directory serialize. Give the
lint run its own directory if you want to run both at once:

```sh
CARGO_TARGET_DIR=target/lint cargo clippy --workspace --all-targets --locked -- -D warnings
```

Differing `RUSTFLAGS` between two commands does not thrash either, but it does double
the work and the disk: each distinct `RUSTFLAGS` value gets its own artifact set. The
first `RUSTFLAGS="-D warnings" cargo test --no-run` against a directory already warm for
the bare command took 7.96 seconds; afterwards both variants were cached and either
command settled at under 0.11 seconds. Prefer `cargo clippy ... -- -D warnings`, which
denies warnings through the lint driver and leaves `RUSTFLAGS` unset, over
`RUSTFLAGS="-D warnings" cargo clippy`.

## The test profile

The workspace manifest selects `debug = "line-tables-only"` for `[profile.test]`.

Post-consolidation the selection buys disk, not time: relink after a core source change
is 1.30 seconds with no profile, 1.36 with `line-tables-only`, and 1.41 with `debug = 0`
— one spread, no signal. `target/debug/deps` is where they separate: 199 MiB with no
profile, 154 MiB with `line-tables-only`, 77 MiB with `debug = 0`.

What `debug = 0` costs is the reason it was not taken. A failing test reports its panic
site under every setting, because the panic location is not debug information. But the
frames of a backtrace lose their `file:line` under `debug = 0`, keeping only function
names; under `line-tables-only` every frame keeps `file:line` and loses only the
fully-qualified spelling of its name. `line-tables-only` takes the smaller disk cut and
keeps a backtrace you can navigate.

`split-debuginfo = "unpacked"` is not set because it is already the default on macOS.
Setting it explicitly produced a byte-identical `target/debug/deps` and the same relink
times, so it would be a no-op the reader would have to reason about.
