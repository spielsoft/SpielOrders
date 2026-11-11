# M10 — Agent Ready

The [roadmap](../roadmaps/spielorders-implementation-roadmap.md#m10--agent-ready)
accepts the prerequisite GUI milestones.

## Outcome

An external agent invokes the CLI bundled with the installed native app to inspect and
mutate an authorized Package while the GUI is closed, with no separate installation. The
opt-in preparation, status, and initialization entry points, their generated guidance,
and their GUI action exist as the agent skill contract defines them.

Ordinary Package commands already exist in the [CLI command surface](../../crates/spielorders-cli/src/cli.rs)
and are covered by [executable contract tests](../../crates/spielorders-cli/tests/cross_command_conformance.rs).
M10 reuses them; it does not rebuild the CLI or commission general cleanup.

## Decisions

- [Define the SpielOrders agent skill contract](../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)
- [Define the Rust CLI capability contract](../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Native desktop distribution](../../docs/adr/0001-native-desktop-distribution.md)
- [Effect and adapter boundaries](../../docs/architecture/boundaries.md)
- [First collection conversion](../../docs/development/first-collection-conversion.md)

## Slices

- [ ] Bundle the existing executable inside the app and establish external launch and
  persistent Package authorization. Change code only where installed delivery
  demonstrably requires it; put no domain logic in a launcher. Resolve any distribution
  or permission choice with the owner rather than changing product policy.
- [ ] `agent prepare`, `agent status`, and `init --agent-ready`, with the generated
  guidance and its GUI action, as the agent skill contract defines them.
- [ ] A resumable adoption rehearsal on an isolated archive copy under the conversion
  guide.

## Acceptance checks

- An external agent-style process invokes the exact bundled CLI with the GUI closed and
  inspects and mutates a disposable Package. Help output or a GUI-child invocation alone
  is not proof.
- The verdict names the actual executable, entitlements, launch context, authorization,
  and results. An unsandboxed pass or privilege elevation is not sandbox proof.
- Authorized access survives a new invocation; absent, denied, or stale authorization
  produces an honest refusal without changing the Package.
- The installed app supplies the executable in place, with no separate installer or
  required global copy. GUI operations still use directly linked Rust.
- Opt-in setup leaves ordinary initialization and opening unchanged, and the guidance's
  examples match the existing command contracts.
- Existing executable contract checks and the gate pass. Delivery introduces no
  unrelated command or behavior change.
- Tests write only disposable data; the source archive and owner preferences remain
  untouched. Unverified Store behavior is not reported as established.

## Exclusions

General CLI cleanup or refactoring, rebuilding implemented Package commands, App Store
submission, an alternate distribution channel, a backend service, sandbox disabling,
changes to the settled GUI architecture, and production archive conversion.
