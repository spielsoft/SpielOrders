---
title: Put the command path first and its modifiers after it
status: closed
assignee: claude-opus-5
claimed_at: 2026-09-06T10:20:00Z
closed_at: 2026-09-06T20:06:47Z
review_start: fcb15eea5344df98ea43cea8062173fe8a788081
parent: M5 — Search and native platform services
labels:
  - implementation:m5
  - implementation:task
---

## Outcome

A command line names what it acts on before how, and every option belongs to the command
it modifies — so the parser decides the whole grammar and no rule depends on where a
caller typed an option.

## Decision sources

- [Define the Rust CLI capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Define the package-local agent skill contract](../../../wayfinder/spielorders-product-architecture/tickets/011-agent-skill-contract.md)

## Blocking

Blocks M5's decomposition: M5's tickets are written from the command-line sentence this
ticket lands, so it precedes them rather than joining them.

## Scope

Five options are declared global. Clap propagates a global into each subcommand's matches
but validates conflicts against one parse level at a time, so two of them written on
either side of a command path are never seen together, and a repeated one is one
occurrence at each level. Three defects this milestone has already paid for — a targeting
pair, an output-mode pair, and a repetition — are that single cause, and the machinery
answering it validates after parsing because the grammar it defends cannot be validated
during.

**The command path comes first and modifiers follow it.** Each option is declared on the
command it modifies, so everything lands in one parse level and the parser enforces the
grammar. Nothing precedes the command path except `--help` and `--version`, which are
reference requests rather than modifiers.

One flattened group per concern, attached to every command that takes it. A Package target
carrying `--root`, and an Order target carrying `--root` and `--uuid`, so the type says
which root a command takes — a rule the capability decision states in prose today. One
output-mode group carrying `--json`, `--quiet` and `--verbose` with its exclusion declared
on it, written once and enforced by the parser. `--dry-run` stays on the mutations that
take it.

The post-parse exclusion registry and the walk that reads it are deleted here, in the same
commit: they exist to defend the grammar this removes. The repeated-option defect dissolves
with them, because a repetition is now two occurrences at one level, which the parser
already refuses.

Dispatch changes shape. The output mode is read from the top level today so the binary
builds its output once; it now comes from the command, before that is constructed. That is
the executable's spine, not any command's body.

Command lines appear as examples in the agent-skill decision, in legacy adoption's M8
prose, and in the roadmap. Any that lead with an option is rewritten.

## Acceptance criteria

- An option written before the command path is a usage error at exit `2` carrying the
  shared usage identity, and that is asserted.
- Every excluded pair is refused at the one placement a caller can now write, each test
  derived from the group that declares the exclusion rather than from a list.
- No exclusion is decided after parsing, and no registry of exclusions remains.
- A repeated option is refused, without a rule written for repetition.
- Every command's `--help` reads as the grammar now is.
- No example anywhere shows an option before the command path.
- Only the usage cell moves, re-recorded in place at version 1.
- `scripts/gate.sh` passes, clippy included.

## Exclusions

- Which commands, options, or exclusions exist. This ticket moves where they are declared.
- `--help` and `--version`, which stay reference requests at the root.
- The archived M4 records, which describe the surface M4 delivered and stay true of it.

## Result

Every option is declared on the command it modifies. A Package target carries `--root`, an
Order target carries `--root` and `--uuid`, one output-mode group carries the three modes
with their exclusion declared on it, and `--dry-run` sits on the mutations that take it.
One command line parses at one level, so the parser decides the whole grammar and no rule
about an option depends on where a caller wrote it. Nothing precedes the command path but
`--help` and `--version`.

The registry that decided exclusions after parsing is gone, and a repeated option is
refused without a rule written for repetition. The placement generators go with it.

The command tests are in `crates/spielorders-cli/tests/output_modes_and_streams.rs` and
`order_reads_and_targeting.rs`, and the grammar rule is in
[the capability contract](../../../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md).
