# SpielOrders implementation roadmap

## Current boundary

The Rust library and CLI are the implemented foundation; the
[library interface](../../crates/spielorders/include/spielorders.h) and
[contract fixtures](../../contracts/README.md) record available behavior. The native
desktop foundation links that library directly. [The product decisions](../wayfinder/spielorders-product-architecture/MAP.md)
define its capabilities. [Native desktop distribution](../../docs/adr/0001-native-desktop-distribution.md)
defines deployment constraints; [Effect and adapter boundaries](../../docs/architecture/boundaries.md)
defines ownership and API coordination.

Institutional forms, the shared purchasing workflow, collaboration, OCR, production
archive migration, structured post-MVP filters, a network service, and shared
SpielGantt foundations are outside this roadmap.

## Compatibility before release

CLI and agent tooling remain unreleased until the owner declares them released after
Agent Ready acceptance. Before release, metadata and CLI response changes revise their
existing contracts in place. The [metadata](../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
and [CLI](../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
decisions govern compatibility. A different frontend alone needs no metadata change.

## Delivery dependencies

| Milestone | Status | Prerequisites |
| --- | --- | --- |
| R1 — Rust backend reduction | Accepted | None |
| R2 — Rust library follow-ups | Accepted | None |
| R3 — Provider path remediation | Accepted | None |
| R4 — CLI test surface removal | Accepted | None |
| M6 — Native foundation and integration | Accepted | None |
| M7 — Browse and search | Accepted | None |
| M8 — Create and edit | Accepted | None |
| M9 — Removal and recovery | Accepted | M8 |
| [M10 — Agent Ready](../implementation/m10-agent-ready.md) | Not started | M9 |
| M11 — MVP acceptance | Not started | M9 and M10 |
| M12 — App Store release | Not started | M11 |

Native controls and their standard interaction behavior apply throughout the desktop;
broader visual refinement follows core functionality.

The GUI sequence runs through M9 before Agent Ready work starts in M10. The existing
CLI supplies ordinary Package operations; M10 is not another CLI implementation or
general cleanup pass. Bundling, permissions, the missing opt-in Agent Ready features,
and external-agent validation do not block GUI milestones or run alongside them.
Existing CLI regression checks remain in [the check](../../scripts/check.sh).
[AGENTS.md](../../AGENTS.md) governs concurrent worktrees and landing. The interface choice is settled by
[Effect and adapter boundaries](../../docs/architecture/boundaries.md).

## M10 — Agent Ready

Outcome: the existing CLI is delivered inside the app with proven external-agent
authorization. The only planned command additions are `agent prepare`, `agent status`,
and `init --agent-ready`, with the generated guidance and GUI action defined by
[Define the SpielOrders agent skill contract](../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md).
This is the last feature milestone before integrated MVP acceptance.

Gate: with the GUI closed, an external agent uses the exact bundled CLI and generated
guidance to inspect and mutate an authorized disposable Package under the intended
sandbox permissions. Opt-in setup works without changing ordinary initialization or
opening, examples match the existing command contracts, and a resumable adoption
rehearsal follows the
[conversion guide](../../docs/development/first-collection-conversion.md) without
changing the real archive.

## M11 — MVP acceptance

Outcome: an integrated native application and bundled CLI that pass partner review, one
acceptance pass, and the owner's sign-off.

Gate: decided capabilities, supported OS releases, cross-adapter results, filesystem
failures, first launch, preferences, lifecycle, and real archive-copy responsiveness
pass the acceptance pass on the final build.
Test metadata stays in the copy. The owner accepts the functional MVP and resolves
the provisional CLI identity selector before declaring its contract stable.

## M12 — App Store release

Outcome: the accepted MVP is packaged, signed, submitted, and distributed through the
chosen Store channel.

Gate: release configuration, entitlements, nested executables, Store materials, and
submission validation are complete; Apple accepts the release. Functional MVP acceptance
does not claim Store approval, and Store work is not deferred permission-system design.
