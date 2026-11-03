# SpielOrders

SpielOrders is a macOS-first desktop application with a shared Rust core and a
human-first CLI. The core owns Package rules and the `spielorders` executable exposes
them; the desktop shell is still a stub.

## Prerequisites

- The Rust toolchain selected by `rust-toolchain.toml` (installed automatically by `rustup`)
- Node.js and npm selected by `.nvmrc` and the package manifests
- macOS Tauri 2 system prerequisites (Xcode command-line tools and WebKit)

The repository pins Node in `.nvmrc` and npm in both package manifests. From a
fresh shell with [nvm](https://github.com/nvm-sh/nvm), reproduce the exact
JavaScript toolchain before installing dependencies:

```sh
nvm install
nvm use
npm install --global "$(node -p "require('./package.json').packageManager")" --ignore-scripts
node --version
npm --version
```

The final two commands must match the runtime and package-manager selectors. Both package manifests
also reject `npm install`, `npm clean-install`, and `npm run` commands when either
the Node runtime or npm package-manager version differs.

## Clean build and test

From the repository root:

```sh
cargo build --workspace
cargo test --workspace
cargo test -p spielorders-core
npm --prefix apps/desktop clean-install --ignore-scripts
npm --prefix apps/desktop run test
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri build -- --no-bundle
npm clean-install --ignore-scripts
npm run test:schemas
npm run check:traceability
```

The standalone core check compiles and tests `spielorders-core` without building
the CLI, Tauri adapter, or frontend.
[`docs/development/test-loop.md`](docs/development/test-loop.md) records the narrower
inner-loop command, the gate, and how to run one module or one test. The Tauri shell opens without a SpielOrder
Package and only confirms that the adapter topology is wired.

## Dependency direction

`spielorders-core` is the reusable, platform-independent boundary. The
`spielorders` CLI and `spielorders-desktop` Tauri crate depend on it. The React
frontend is presentation code and has no Rust dependency. Core depends on no
adapter or frontend package.
