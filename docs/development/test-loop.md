# Running tests

## Rust loop

[The loop script](../../scripts/loop.sh) owns the fast Rust command:

```sh
scripts/loop.sh
scripts/loop.sh order_creation::
```

Arguments are test-name filters. The library's tests are one module tree inside it, and
the CLI's integration suite is one target; select modules by prefix rather than
selecting a separate test binary. The CLI is an executable with no library target: its
integration suite drives the built executable, and its unit tests sit beside the code
in the executable's own test build.

The [workspace](../../Cargo.toml) contains the library, the CLI, and repository checks.
A new library test file needs a module declaration in the library's `src/tests/mod.rs`,
and a new CLI integration-test file one in the CLI's `tests/main.rs`: automatic
integration-test discovery is disabled there. Confirm that the new behavior test
actually runs, not merely that compilation succeeds.

## Gate

```sh
scripts/gate.sh
```

[The gate script](../../scripts/gate.sh) owns the exact checks: Rust formatting,
clippy, tests, schema and documentation checks, generated-header freshness, and a C
Package walkthrough linked to the static library. On macOS it also runs the
[native checks](../../scripts/native-checks.sh): Swift behavior tests, UI-test
compilation, and a signed Release validation app with sandbox entitlement checks.
The validation app has its own identity and container under
`target/native-validation`; it is not the production app or an XCUITest-permissioned
app. Use that artifact for coordinated Release observations.

The default gate does not operate the desktop. Interactive checks require the
[Desktop acceptance](desktop-acceptance.md#native-interaction) prerequisites and an
explicit `SPIELORDERS_EXCLUSIVE_UI=1 scripts/gate.sh` invocation. A headless gate pass
does not establish native interaction, sandbox-granted folder access, or App Store
acceptance.

Run the repository checks independently with
`cargo run --package spielorders-checks --locked -- schemas` or
`cargo run --package spielorders-checks --locked -- docs` from the repository root.
Build the app without launching it using `xcodebuild -project
apps/macos/SpielOrders.xcodeproj -scheme SpielOrders -configuration Debug
-derivedDataPath target/native build`.

Use a fresh target directory when measuring compilation costs. Accumulated abandoned
artifacts can dominate linking and filesystem work; measure before changing the test
architecture. The gate owns its cleanup policy.
