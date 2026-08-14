# SpielOrders agent instructions

## Dependency and toolchain currency

- Keep every toolchain, runtime, library, and other dependency on the newest
  available minor and patch release within its currently selected major version.
  Apply those updates by default without waiting for separate approval.
- Ask the user before adopting a new major version because that may change public
  contracts, compatibility, or migration requirements.
- Never use “do not update” as the silent fallback. If an in-major update is blocked,
  report the blocker and the newest version that could be verified instead.
