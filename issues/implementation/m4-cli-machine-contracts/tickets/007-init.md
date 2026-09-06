---
title: Initialize a Package from the command line
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-04T12:59:32Z
closed_at: 2026-09-04T13:32:49Z
review_start: f18e3daaa81982d36051d6c76f8630477c44673d
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

`spielorders init` turns an ordinary directory into a Package at the newest metadata version
the running build understands, optionally with non-default Orders and Removed Orders paths.
An existing root `README.md` survives byte-for-byte. `--dry-run` shows what would be created
and creates nothing.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define durable metadata schemas and version evolution](../../../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)

## Blocking

Blocked by [Ticket 002](002-executable-effects-and-response-contract.md).

## Scope

- `init [orders_path=…] [removed_path=…]` against the root named by `--root`, creating the
  managed configuration, the version marker, an empty Vendors aggregate, and a derived index
  at the current version.
- Preserve an existing root `README.md` byte-for-byte; create an empty conventional Package
  README only when absent.
- `--dry-run` validates the whole plan and reports it with `committed: false`, creating no
  directory, staging file, index write, or cleanup effect.
- Surface every configuration path rule as its own typed rejection: each path a nonempty
  `/`-separated relative descendant; `orders_path = "."` invalid; `removed_path` an ordinary
  direct child of the root; neither overlapping the other, escaping the root, occupying the
  reserved subtree, or resolving through a disallowed link.
- Report rollback honestly: created artifacts are rolled back where possible, and a failed
  rollback is reported beside the failure that caused it.
- Report the created paths and the resulting metadata version in both surfaces.

## Acceptance criteria

- A plain directory becomes a Package that `package show` immediately reads. The
  configuration is unversioned by closed decision, so a default run writes an empty table.
- Non-default paths are honored, including an `orders_path` nested several levels below the
  root, and the result is usable by the Order commands.
- An existing root `README.md` is unchanged byte-for-byte; a Package initialized without one
  gains an empty conventional Package README.
- Each path rule above is rejected with a typed error identifying the rule broken, at exit
  `1`, having written nothing.
- Initializing a directory that is already a Package is refused, and it is unchanged.
- `--dry-run` over a valid plan reports what the real run would with `committed: false`, and
  over each rejection produces that rejection instead. Both leave the directory
  byte-identical, including creating no staging file.
- The Removed Orders Directory is not created eagerly. Lazy creation is Ticket 012's.
- Fixtures cover the cells `init` can reach and no others, each unreachable cell's reason
  recorded; every route to exit `2` carries the shared usage identity.

## Exclusions

- **`--agent-ready`** and any generated agent bundle. That is M8's.
- Adoption of existing directories as Orders, which is Ticket 010's.
- Writing an older schema version to exercise the upgrade path.
- Any interactive prompt or wizard.
- Reading anything inside the directory beyond the managed paths and the Package README.
- **Inventing a second mutation contract.** `init` takes `--dry-run`, `committed: false`,
  the typed report, and the outcome vocabulary from the CLI decision; Ticket 008 owns them
  as the cross-command artifact.

## Result

`spielorders init [orders_path=…] [removed_path=…]` plans and commits one Package
initialization through the core capability that owns it. An existing Package README
survives byte for byte, the Removed Orders Directory stays lazy, and `--dry-run` reports
the same document at `committed: false` while touching nothing. Each configuration path
rule is refused with its own stable code and the key that broke it; a failed rollback is
distinguished from a complete one and names what remains. A key `init` does not have is a
usage error, because no capability exists to adjudicate it.

The response establishes the mutation shape the later mutations take: a typed report
beside the refreshed affected result, recorded under `contracts/cli/init`.

Pinned by `crates/spielorders-cli/tests/initialization.rs` and `response_contract.rs`.

Two documents are corrected in the same commits: the envelope's rule for
`context.package_root`, and which initialization refusals have resolved a root.
