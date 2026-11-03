---
title: M4 acceptance record
status: gate passed; M4 accepted, awaiting external validation
gate_ticket: tickets/016-integrate-m4-acceptance.md
---

# M4 acceptance record

The gate's criteria and verdicts at one candidate, and the boundaries no tool holds.

**The gate passed and M4 is accepted.** External validation is a fresh worker's, against
this exact target.

| | |
| --- | --- |
| Candidate | `98852fb76573c7f7426fde4565d09d124f6a5adb`, worktree clean |
| Toolchains | rustc, cargo 1.98.1; Node 26.8.1, npm 12.0.2 — matching `rust-toolchain.toml`, `.nvmrc` and `packageManager` |
| `scripts/gate.sh` | passed |

## Criteria

| Criterion | Verdict |
| --- | --- |
| Every M4 ticket closed from reviewed work, no blocking finding unresolved | pass — every finding dispositioned, none at the bar reserved for a further run |
| Every command delivered end to end, every deferred one routed and asserted absent | pass — one inventory, read both ways |
| The composition runs over a nested Orders Directory and a renamed Removed one | pass — `tests/m4_acceptance.rs`, real processes |
| `--json` is one UTF-8 document and no incidental output; human output parses nothing | pass |
| Only explicit whole-Order removal disposes of Supporting Material | pass |
| READMEs keep absent, readable and unreadable apart | pass |
| A committed mutation whose index refresh failed is never retry-shaped | pass |
| Metadata version 1, the schemas, the fixtures and the scenarios unchanged | pass — only CLI response fixtures changed |
| Cross-command conformance re-established here rather than delegated | pass |
| The adapter decides no Package rule | pass |
| Currency complete, every blocked in-major update named | pass — below |
| The two-axis review ran in its parallel form, every finding with its disposition | pass — two blocking, carried; the smells dropped |
| This record identifies the closing SHA or each returned blocker | pass — the candidate above |

## Carried forward

Into [M5 Ticket 001](../../implementation/m5-search-and-native-services/tickets/001-one-answer-about-a-link-a-count-and-a-quantity.md), whole: a link named as a root, which `init` refuses and every other
command follows; a duplicate-Order-ID count naming one directory too many; a Quantity
rendered as money; one wrong-kind root reaching two stable codes; an empty Order's total of
negative zero; and one owner question about assigning a Vendor by Name.

## Currency

Everything the three manifests select is newest in its major, and none has a newer major
this build may take. Four transitive crates stay behind — `generic-array`, `toml`,
`toml_datetime`, `toml_edit` — each held by an exact `=` requirement, reached only through
Tauri, and absent from every shipped target's graph. `libc` 1.0.0-alpha.4 and `rustup`
1.29.1 are named rather than taken: a prerelease of a new major, and one no manifest
selects.

## Enforcement gaps

Claims no tool holds:

- **The filesystem lives in one module.** `std::fs` is deliberately absent from
  `crates/spielorders-cli/clippy.toml`: it has no closed surface, and `Path` reaches the
  filesystem without naming `fs`, so such a list would fence part and read as complete.
  That only `src/effects.rs` names `fs::` is held by review.
- **`repair --help` states where the scope boundary falls.** Its generated reference sends
  a case inside an Order Directory to `order repair`, is composed before any Package is
  resolved, and has nothing to consult — so review, not a test, keeps it true.
- **Rollback failure is not reachable through the executable.** Every mutation validates its
  whole plan first, so a commit-phase rollback needs an injected effect.
  `spielorders-core`'s suite holds it.
- **Generated `--help` and `--version` stay reference text under `--json`** — the capability
  decision calls them reference, not command responses.
