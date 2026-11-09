# Native desktop distribution

The native desktop targets the Mac App Store, Apple silicon, and the current and previous
major public macOS releases. The Rust CLI ships inside the installed app bundle for
agents to invoke in place, with no separate installation or required global copy.
Ordinary development defers distribution certificates and Store submission, but uses
the local signatures needed to exercise the intended sandbox entitlements.

## Consequences

Package folders remain ordinary user-selected directories. The application must obtain
and retain authorized access rather than move Packages into private app storage or
assume that an absolute path grants permission.

GUI access and external-agent access have separate launch and authorization contexts.
A GUI-spawned helper, bundle location, or remembered GUI bookmark does not prove external
agent access. The [implementation roadmap](../../issues/roadmaps/spielorders-implementation-roadmap.md)
schedules their runtime proofs independently; CLI feasibility is not a GUI prerequisite.
An unsandboxed development pass does not establish Store feasibility. An observed
incompatibility is brought to the owner with evidence; it does not silently change
distribution or remove the CLI.

Apple documents [embedded command-line tools](https://developer.apple.com/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app)
and [sandboxed file access](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox).
Passing local probes does not establish App Store approval.
