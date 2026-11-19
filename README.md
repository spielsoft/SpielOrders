# SpielOrders

SpielOrders is a local-first order manager with a macOS-native SwiftUI desktop,
a portable Rust core, and a human-first Rust CLI. The Swift desktop links the library
directly. The [implementation roadmap](issues/roadmaps/spielorders-implementation-roadmap.md)
defines the remaining work.

The [domain glossary](CONTEXT.md) defines the product vocabulary.
The [demo Package](examples/README.md) provides fictional content for trying the app.
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

Install or update the CLI and its guidance from the latest release on macOS (Apple
silicon) or Linux (x86-64 or arm64):

```sh
curl -fsSL https://github.com/spielsoft/SpielOrders/releases/latest/download/install.sh | sh
```

Options go after `sh -s --`: `--version <TAG>` installs that release, and
`--prefix <PATH>` selects a dedicated installation directory other than `~/.spielorders`.
The
[installer](scripts/install.sh) needs only `curl` and needs no GitHub account. It verifies
the release archive's checksum before extracting or running anything, stages the whole
installation, and registers `~/.spielorders/bin` in the profiles of your login shell
(zsh or bash), replacing only its marked SpielOrders block. Other settings and profile
links are preserved. It confirms that a fresh login shell finds the installed CLI before
it reports success. If that check or anything else fails, it puts back everything it
changed. Start a new shell afterwards. Repeating the command updates an unchanged
installation; locally modified installation files cause a refusal rather than being
overwritten.

From a development checkout with Rust, Python, and zsh or bash available, build and
install the CLI with:

```sh
python3 scripts/install-cli.py
```

It packages the build as a release is packaged and installs that archive with the same
installer, taking the same `--prefix`. A `v*` tag runs the
[release workflow](.github/workflows/release.yml), which publishes the archives, their
checksums, and the installer.

## Restoring a Package

After copying or restoring a Package, rebuild its derived index before opening it in the
app or making edits: run `spielorders index rebuild --root "/path/to/Package"`.
Copying can make an old index appear newer than the records it describes. The rebuild
reads the canonical records; it neither repairs them nor changes Supporting Material.

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
