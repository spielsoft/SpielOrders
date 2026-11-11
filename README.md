# SpielOrders

SpielOrders is a local-first order manager with a macOS-native SwiftUI desktop,
a portable Rust core, and a human-first Rust CLI. The core and CLI are available;
the Swift desktop foundation links the library directly. The
[implementation roadmap](issues/roadmaps/spielorders-implementation-roadmap.md)
defines the remaining work.

The [domain glossary](CONTEXT.md) defines the product vocabulary.
The [product decisions](issues/wayfinder/spielorders-product-architecture/MAP.md)
constrain future work. [Effect and adapter boundaries](docs/architecture/boundaries.md)
define ownership across Rust and the desktop.

## Agent access

The Package menu's Agent Ready action provisions missing local guidance and obtains the
bundled CLI's separate folder grant. It shows the executable's installed path and copies
the request to give an agent. Invoke that executable in place; no global CLI installation
is needed. [The agent skill contract](issues/wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)
defines preservation and authorization boundaries.

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
