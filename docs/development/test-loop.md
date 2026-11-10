# Running tests

## Check

[The check script](../../scripts/check.sh) is what a builder runs many times a day:
Rust formatting, clippy, the Rust tests, the headless Swift tests, and the repository
checks (schemas, documentation links, generated-header freshness, and a C Package
walkthrough linked to the static library). It builds nothing for Release and signs
nothing. On a warm tree it takes just under a minute.

```sh
scripts/check.sh
scripts/check.sh order_creation::
```

Arguments are test-name filters: with them, the script runs only the Rust and Swift
tests whose names match. The library's tests are one module tree inside it, and the
CLI's integration suite is one target; select modules by prefix rather than selecting a
separate test binary. The CLI is an executable with no library target: its integration
suite drives the built executable, and its unit tests sit beside the code in the
executable's own test build.

The [workspace](../../Cargo.toml) contains the library, the CLI, and repository checks.
A new library test file needs a module declaration in the library's `src/tests/mod.rs`,
and a new CLI integration-test file one in the CLI's `tests/main.rs`: automatic
integration-test discovery is disabled there. Confirm that the new behavior test
actually runs, not merely that compilation succeeds.

## Gate

```sh
scripts/gate.sh
```

[The gate script](../../scripts/gate.sh) runs the check and then what only a milestone
needs: UI-test compilation, the signed Release Validation app, and its bundle and
sandbox entitlement checks. On a warm tree with no app change it takes about a minute; a
Swift change adds the rebuild of both app configurations. The Validation
app has its own identity and container under `target/native-validation`; it is not the
production app or an XCUITest-permissioned app. [Desktop acceptance](desktop-acceptance.md)
operates it.

The gate does not operate the desktop. `SPIELORDERS_EXCLUSIVE_UI=1 scripts/gate.sh` also
runs the UI tests, which take the foreground and need the
[Desktop acceptance](desktop-acceptance.md) prerequisites. A gate pass does not establish
native interaction, sandbox-granted folder access, or App Store acceptance.

Run one repository check with
`cargo run --package spielorders-checks --locked -- schemas` or
`cargo run --package spielorders-checks --locked -- docs` from the repository root.
Build the app without launching it using `xcodebuild -project
apps/macos/SpielOrders.xcodeproj -scheme SpielOrders -configuration Debug
-derivedDataPath target/native build`.

Use a fresh target directory when measuring compilation costs. Accumulated abandoned
artifacts can dominate linking and filesystem work; measure before changing the test
architecture. The check script owns its cleanup policy.
