# SpielOrders

SpielOrders is a macOS-first desktop application with a shared Rust core and a
human-first CLI. The M0 skeleton intentionally contains no Package behavior.

## Prerequisites

- Rust 1.97.1 (installed automatically by `rustup` from `rust-toolchain.toml`)
- Node.js 26.5.1 and npm 12.0.2
- macOS Tauri 2 system prerequisites (Xcode command-line tools and WebKit)

The repository pins Node in `.nvmrc` and npm in both package manifests. From a
fresh shell with [nvm](https://github.com/nvm-sh/nvm), reproduce the exact
JavaScript toolchain before installing dependencies:

```sh
nvm install
nvm use
npm install --global npm@12.0.2
node --version
npm --version
```

The final two commands must print `v26.5.1` and `12.0.2`. Both package manifests
also reject `npm install`, `npm clean-install`, and `npm run` commands when either
the Node runtime or npm package-manager version differs.

## Clean build and test

From the repository root:

```sh
cargo build --workspace
cargo test --workspace
cargo test -p spielorders-core
npm --prefix apps/desktop clean-install
npm --prefix apps/desktop run test
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri build -- --no-bundle
npm clean-install
npm run test:schemas
npm run check:traceability
```

The standalone core check compiles and tests `spielorders-core` without building
the CLI, Tauri adapter, or frontend. The Tauri shell opens without a SpielOrder
Package and only confirms that the adapter topology is wired.

## Dependency direction

`spielorders-core` is the reusable, platform-independent boundary. The
`spielorders` CLI and `spielorders-desktop` Tauri crate depend on it. The React
frontend is presentation code and has no Rust dependency. Core depends on no
adapter or frontend package.
