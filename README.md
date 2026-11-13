# SpielOrders

SpielOrders is a local-first order manager with a macOS-native SwiftUI desktop,
a portable Rust core, and a human-first Rust CLI. The Swift desktop links the library
directly. The [implementation roadmap](issues/roadmaps/spielorders-implementation-roadmap.md)
defines the remaining work.

The [domain glossary](CONTEXT.md) defines the product vocabulary.
The [product decisions](issues/wayfinder/spielorders-product-architecture/MAP.md)
constrain future work. [Effect and adapter boundaries](docs/architecture/boundaries.md)
define ownership across Rust and the desktop.

## Agent access

Agents use the standalone `spielorders` CLI, installed under the prefix `~/.spielorders`
as `bin/spielorders` with its default guidance in `share/spielorders/agent-guidance`. It
runs with the invoking user's ordinary permissions. File → Set Up Agent Guidance… provisions
missing local guidance into any existing directory and nothing more.
[The agent skill contract](issues/wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)
defines provisioning and refresh.

From a development checkout with Rust, Python, and zsh available, install the CLI with:

```sh
python3 scripts/install-cli.py
```

The installer builds the CLI, installs its matching guidance, and registers
`~/.spielorders/bin` in the effective zsh profile directory. It replaces only its marked
SpielOrders block, preserves unrelated settings and profile links, and verifies lookup
in a fresh shell before reporting success. Start a new shell afterwards. Repeating the
command updates an unchanged installation; locally modified installation files cause a
refusal rather than being overwritten. `--prefix <PATH>` selects a different dedicated
installation directory. This is a development installer, not a published download.

## Development

Use the Rust toolchain selected by `rust-toolchain.toml`:

```sh
cargo build --workspace --locked
scripts/check.sh
scripts/gate.sh
```

[Running tests](docs/development/test-loop.md) explains the check and the gate.
For native integration, `cargo build -p spielorders --release --locked` builds
`target/release/libspielorders.a`. Import the generated
[C header](crates/spielorders/include/spielorders.h) through a Swift module map or
bridging header and link that library. The
[C Package walkthrough](crates/spielorders/tests/c/package_walkthrough.c) is a worked
caller; [Effect and adapter boundaries](docs/architecture/boundaries.md) governs
integration and ownership.

The [native Xcode project](apps/macos/SpielOrders.xcodeproj/project.pbxproj) builds the
SwiftUI app and its UI-test target. The check runs the headless Swift tests and the gate
builds the app and its UI tests on macOS; [Running tests](docs/development/test-loop.md)
distinguishes both from [Desktop acceptance](docs/development/desktop-acceptance.md).

The [first-collection conversion guide](docs/development/first-collection-conversion.md)
records the boundary between the real archive and disposable test copies.
