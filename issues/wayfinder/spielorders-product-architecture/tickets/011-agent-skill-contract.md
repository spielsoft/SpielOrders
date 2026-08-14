---
title: Define the SpielOrders agent skill contract
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

How should SpielOrders provide and maintain a versioned agent skill, comparable in role to SpielGantt's Agent Ready bundle, so agents can inspect a SpielOrder Package and safely use the CLI to initialize or adopt Packages; create and update Orders, Order Items, Vendors, and Vendor Contacts; search and validate their work; respond to diagnostics; and preserve Supporting Material without directly editing managed metadata—and what generated files, setup and refresh commands, readiness/runtime/snapshot contracts, opt-in behavior, JSON-first operating protocol, approval boundaries, and skill-version compatibility rules are required?

## Blocking

Unblocked. [Define the Rust CLI capability contract](006-cli-contract.md) and [Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md) are both closed.

## Resolution

SpielOrders provides a package-local agent skill only. It does not install or maintain a global skill. The app and CLI ship the source material used to prepare a copy inside a SpielOrder Package, so the guidance travels with the Package an agent is operating on.

Agent Ready setup is explicit and opt-in. Ordinary Package initialization and opening create no agent files. A user may request it during initialization with `spielorders init --agent-ready ...`, prepare it later with `spielorders agent prepare --root <PATH>`, or use a simple Agent Ready action alongside Open and Initialize in the GUI Project menu. The CLI and GUI use the same core preparation operation.

### Generated guidance

Preparation writes a minimal root `AGENTS.md` and one project-local skill:

```text
AGENTS.md
.agents/skills/spielorders/
  SKILL.md
  references/
    packages-and-adoption.md
    orders-and-items.md
    vendors-and-contacts.md
    search-and-inspection.md
    diagnostics-and-repair.md
```

`agent prepare` has deliberately simple behavior: it creates these files when absent and overwrites them when present. It performs no ownership-marker check, merge, preservation of edits, version comparison, compatibility classification, history, manifest maintenance, or other lifecycle management. Its concise human-readable output lists the paths created or overwritten.

The generated skill has no skill version or custom metadata. Its frontmatter contains only:

```yaml
name: spielorders
description: Working with SpielOrders packages.
```

`AGENTS.md` is an absolute-minimum pointer: it identifies the directory as a SpielOrder Package, points to `.agents/skills/spielorders/SKILL.md`, names the `spielorders` CLI as the authority for managed data, and states the managed-metadata and Supporting Material boundaries. It contains no command catalog, examples, version field, or ownership marker.

`SKILL.md` assumes a capable agent. It briefly explains what SpielOrders is for, the SpielOrders Root and Package boundary, the CLI-wide `--root <PATH>` convention, the availability of default human output and opt-in `--json` output, the essential safety rules, and when each directly linked reference is useful. Generic facts such as `--json` appear once in `SKILL.md`, not repeatedly in the references. There are no scripts, assets, README, UI metadata, or auxiliary documentation in the generated skill.

The references use progressive disclosure and concise examples. They document how to use SpielOrders and what data each operation needs; they do not prescribe an agent reasoning process, interview sequence, inspection ritual, validation ritual, or approval workflow. Agents use their own intelligence to interpret names, documents, provenance, contradictions, and incomplete evidence and to choose human or JSON output, dry runs, inspection, and validation appropriate to the task.

### Agent command surface

The complete agent-specific CLI surface is:

```text
spielorders agent prepare --root <PATH>
spielorders agent status --root <PATH>
```

These commands accept only the normal Package root option and have human-readable output; they add no command-specific `--json` flag. `agent status` reports only whether root `AGENTS.md` and `.agents/skills/spielorders/SKILL.md` exist. A valid Package returns success whether either file is present or absent. An invalid or missing Package root fails through the CLI's ordinary error behavior. Status does not read or compare contents, inspect reference files, or report skill versions or compatibility.

There is no `agent runtime` command: runtime discovery is not an agent-specific concern. There is no `agent snapshot` command: the already-decided `spielorders package show` operation is the canonical complete Package inspection, including under the CLI's ordinary `--json` output mode. The skill points agents to the existing Package, Order, Vendor, search, validation, repair, upgrade, and index commands rather than introducing agent-only duplicates.

### Command and data guidance

`packages-and-adoption.md` explains and exemplifies Package initialization, optional Agent Ready preparation, Package paths, `add-order` for a newly created Order Directory, and `adopt-order` for an eligible existing direct child. It states the facts these commands require and their effects. For legacy material, filenames and directory components are evidence rather than an automatic field grammar; the agent supplies explicit facts through ordinary CLI assignments. The reference explains existing Vendor and Reusable Free-Entry Value lookup, one-Order-at-a-time adoption, resumability from current Package state, and preservation of existing directory contents. It does not reproduce the full legacy-adoption decision or turn evidence interpretation into a fixed workflow.

`orders-and-items.md` documents Order and Order Item create, show, edit, move, remove, alignment, README, and scoped-validation commands with short examples. Agent-supplied Order facts are `title`, `date_ordered`, and `status`, plus optional Vendor, Orderer, Ordering Institution, currency, Project, and Order Group. Item facts are required Description, Quantity, and Unit Price plus optional Catalog Number. It documents accepted statuses, date and currency formats, optional-field clearing, calculated rather than supplied totals, and one-based Item positions.

`vendors-and-contacts.md` documents Vendor and Vendor Contact list, show, create, edit, move, and remove commands with short examples. A Vendor requires a Name and may receive general phone, email, website, multiline address, and purchasing notes. A Vendor Contact requires a Name and may receive role, phone, and email. The reference explains Vendor-name selection, optional-field clearing, one-based Contact positions, guarded removal of referenced Vendors, and the absence of Vendor Merge and placeholder Vendors. IDs are generated and returned by SpielOrders rather than supplied as ordinary data.

`search-and-inspection.md` documents Package show/list/value queries, Order and Vendor inspection, native Order and Vendor search, and OS-indexed Supporting Material search. It explains relevant inputs and result semantics without duplicating the global human-versus-JSON convention.

`diagnostics-and-repair.md` documents Package and scoped validation, Package and Order repair, metadata upgrade, and Package Index status and rebuild commands. It states their required inputs, outputs, dry-run availability, mutation effects, and which ordinary operations reject damaged or unsupported Package state. It leaves selection of the appropriate response to the agent and the diagnostics returned by the CLI.

Every mutating-command example may show `--dry-run` where useful. The skill explains only its established CLI meaning: validate and report the proposed mutation without writing; omission commits the named operation. It adds no agent-specific approval boundary or ceremony. This refines the future-skill guidance in [Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md): the skill teaches the available adoption operations, required data, dry-run behavior, and effects, while the agent and its surrounding authorization rules determine how to reason, review, and obtain approval.

### Safety boundaries

The skill carries two compact, non-negotiable facts:

- Never directly edit SpielOrders-managed metadata when a CLI operation exists. Use the CLI so validation, atomic writes, cross-record rules, and Package Index refresh remain consistent. `.spielorders` configuration and JSON are managed metadata; Package and Order READMEs are ordinary user-authored content. Diagnostics may identify damage for which no supported CLI mutation exists, but the skill does not invent an unsupported metadata edit.
- Supporting Material is opaque user-owned content inside an Order Directory, outside SpielOrders-managed metadata and without a required internal layout. Metadata editing, validation, repair, alignment, upgrade, and index operations preserve it. Only an explicit whole-Order removal moves or trashes the complete Order Directory containing it. `search supporting-material` provides OS-indexed lookup without making Supporting Material managed data.

This resolution specifies the future generated guidance and its CLI contract only. It does not scaffold or implement the skill.
