# Native desktop distribution

The native desktop targets the Mac App Store, Apple silicon, and the current and previous
major public macOS releases. Agents use a standalone Rust CLI installed under the
prefix `~/.spielorders`: the executable at `bin/spielorders`, registered on PATH, and its
default guidance at `share/spielorders/agent-guidance`. Ordinary development defers
distribution certificates and Store submission, but uses the local signatures needed to
exercise the intended sandbox entitlements.

## Consequences

Package folders remain ordinary user-selected directories. The application must obtain
and retain authorized access rather than move Packages into private app storage or
assume that an absolute path grants permission.

GUI access and agent access are independent. The sandboxed app reaches Packages through
user-selected folder grants. The CLI is not sandboxed and runs with the invoking user's
ordinary permissions, so agents work with the GUI closed and the GUI never authorizes
the CLI. The GUI's Set Up Agent Guidance… only provisions guidance into a chosen directory.

The CLI uses a separate, explicitly user-run installer. The Store app neither installs
nor updates executable code or shell profiles, and it does not offer CLI setup; the README
documents the installers. The installer checks the
actual selected executable and matching resources and verifies resolution in a fresh
shell. The GUI makes no claim about the CLI's installation, currentness, or PATH; its own
process PATH is not evidence of what Terminal or an agent will select.

Public distribution is a GitHub-hosted [bootstrap installer](../../scripts/install.sh)
and prebuilt CLI release archives with matching guidance, published by the
[release workflow](../../.github/workflows/release.yml). The installer selects the
platform and architecture, verifies the downloaded payload's checksum before extraction
or execution, stages the complete installation, preserves unrelated prefix and shell
contents, and rolls back a failed publication or PATH check. The
[development installer](../../scripts/install-cli.py) packages a checkout's build the
same way and installs it with that installer. App signing, notarization, and Store
submission belong to the Store release.

An unsandboxed development pass does not establish Store feasibility. An observed
incompatibility is brought to the owner with evidence; it does not silently change
distribution or remove the CLI.

Apple documents [sandboxed file access](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox).
Passing local probes does not establish App Store approval.
