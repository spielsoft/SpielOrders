---
title: Answer with the Root the command already established
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-05T23:39:43Z
closed_at: 2026-09-06T00:21:57Z
review_start: 5edcc4bb564597f35ecc878b58930715c4e64cad
parent: M4 — CLI and independently versioned machine contracts
labels:
  - implementation:m4
  - implementation:task
---

## Outcome

Every response names the Package it was about wherever the command established one, and
every suggested command acts on that Package rather than on whichever one the shell happens
to be standing in.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md),
  which requires an exact suggested command and reserves UUIDs for `--verbose`
- [The fixture convention](../../../../docs/development/traceability.md), which decides when
  a reported path carries its Package-relative spelling
- [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md)

## Blocking

Blocks [Run the final M4 acceptance gate](016-integrate-m4-acceptance.md), whose second run
found all of this and returned it rather than repairing it. Everything here is one theme,
so it is one ticket: **a response contradicts, or throws away, something the command
holds.** The owner drives this CLI from agents after M4 and M8, and a Package-relative
spelling is unusable without the Root it is relative to.

## Scope

Four faces of that theme, in the order they matter.

**A refusal that spells a path relative to a Package while denying the Package.** Targeting
a directory that is inside a Package but is not one of its Orders refuses with the path
spelled the Package's way and a context saying no Root was established. The Root *was*
established — the spelling is proof — and the core's own failure value for that case has
nowhere to carry it. A consumer is handed a relative path and told there is nothing to
resolve it against. One recorded cell carries this and re-records.

**Refusals raised after the Package resolved that report no Root.** Three in the repair
family: a minted identity colliding with a record whose message says "this Package", a
multi-file replacement that failed with nothing left unrestored — while the branch beside
it, for the same failure, does name the Root — and a path with no JSON spelling. That last
one also reports the unspellable path under the refusal's paths, which is precisely what
the shared projection for that same failure code says not to do and why. One code, two
shapes, neither pinned.

**A suggested command that targets the wrong Package.** A Package-scoped diagnostic's human
suggestion names the bare command with no Root, while the Order-scoped one beside it names
the Root, and the machine surface for the same diagnosis names it too. Run from anywhere
but the Package, the suggestion diagnoses somewhere else. The existing test asserts only
that the line contains the command, so the broken form satisfies it.

**Non-blocking, and here because it is the same surface:** three diagnostic explanations
interpolate a domain UUID into default human output, which the capability decision reserves
for `--verbose`. Weigh it — an identity may be the only handle a finding has — and either
fix it or say in this ticket's Result which sentence of that decision won and why.

**Fix by type where a type can hold it**, as the two tickets before this one did. Nothing
here needs a manufactured race: every case is reachable through a real process.

## Acceptance criteria

- No response reports a Package-relative spelling while its context names no Root, and the
  converse is asserted over every recorded cell, not only the forward direction — written so
  it fails at today's tree before the fix lands.
- Every refusal raised after a Root was established names that Root, and every one that does
  not carries a comment at that site saying why no Package surrounds it.
- One failure code has one shape wherever it is produced.
- A Package-scoped diagnostic's suggested command runs verbatim from any working directory
  and acts on the Package it was printed about, asserted from a real process started
  somewhere else.
- The UUID question is answered in the Result either way, with the sentence it turned on.
- No `contract_version` advances and no fixture byte moves except a cell recording a
  contradiction, re-recorded at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Which refusals exist, their codes, their exit statuses, or their wording. This ticket
  changes what a response carries, not what is refused.
- The response envelope, the exit statuses, and every contract version, all of which stay
  as they are.
- Anything the acceptance gate itself owns: the composition, the acceptance record, and the
  roadmap transition are that gate's to run again afterwards.

## Result

An `order` command refusing a directory a Package holds but does not Order names the Root it
spelled that path against, carried on the core's own failure value. The conformance walk
asks the rule in both directions over every recorded cell, so the case it exists for can
fail it. Three repair refusals raised after a Package resolved name it, and
`path_not_representable` has one producer and therefore one shape. A Package-scoped
diagnostic suggests a command naming its Root, asserted by running the printed characters
through a shell from somewhere else.

The UUID question went to `--verbose`: "UUIDs ... appear only under `--verbose`" is specific
where "show ... explanation" is not, and an explanation is not an identity. The three
sentences say what is wrong and where; `--verbose` adds which record.

One cell re-records at contract version 1. Nothing advanced.

Tests: the CLI suite's cross-command conformance, validating, and output-mode files.
