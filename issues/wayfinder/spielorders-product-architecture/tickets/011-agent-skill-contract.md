---
title: Define the SpielOrders agent skill contract
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders provides a package-local agent skill only. It does not install or maintain a global skill. The app and CLI ship the source material used to prepare a copy inside a SpielOrder Package, so the guidance travels with the Package an agent is operating on.

Agent Ready setup is explicit and opt-in. Ordinary Package initialization and opening
create no agent files. `spielorders agent provision --root <PATH>` plans and commits
missing Package-local guidance. The GUI Package menu provides Agent Ready using the
same library operation. The GUI first obtains a separate, persistent macOS folder
grant for the bundled CLI, which has its own sandbox. It reports authorization success,
cancellation, or failure before reporting setup complete.

### Generated guidance

Provisioning creates missing directories and files for one project-local skill:

```text
.agents/skills/spielorders/
  SKILL.md
  references/
    packages-and-adoption.md
    orders-and-items.md
    vendors-and-contacts.md
    search-and-inspection.md
    diagnostics-and-repair.md
```

Provisioning preserves every existing regular guidance file, including user edits. It
does not merge, replace, version, classify, or maintain a manifest. It refuses linked or
unsuitable destination components and overlap with configured Orders or Removed Orders
paths. Planning writes nothing and honors cancellation. Commit revalidates the Package
configuration and destinations, never replaces a concurrently created file, and reports
rollback or cleanup failures. An entirely preserved setup is unchanged. Guidance writes
do not refresh the Package Index.

The library exposes this through
`spielorders_package_plan_provision_agent_setup` and the existing Plan, operation,
commit, report, and Entries readers. `spielorders_library_agent_setup_prompt` returns a
static borrowed request asking an agent to read the local skill and add a concise
AGENTS.md pointer while preserving existing instructions. Provisioning itself never
reads or writes root AGENTS.md.

The generated skill has no skill version or custom metadata. Its frontmatter contains only:

```yaml
name: spielorders
description: Working with SpielOrders packages.
```

`SKILL.md` assumes a capable agent. It briefly explains what SpielOrders is for, the SpielOrders Root and Package boundary, the CLI-wide `--root <PATH>` convention, the availability of default human output and opt-in `--json` output, the essential safety rules, and when each directly linked reference is useful. Generic facts such as `--json` appear once in `SKILL.md`, not repeatedly in the references. There are no scripts, assets, README, UI metadata, or auxiliary documentation in the generated skill.

The references use progressive disclosure and concise examples. They document how to use SpielOrders and what data each operation needs; they do not prescribe an agent reasoning process, interview sequence, inspection ritual, validation ritual, or approval workflow. Agents use their own intelligence to interpret names, documents, provenance, contradictions, and incomplete evidence and to choose human or JSON output, dry runs, inspection, and validation appropriate to the task.

### Agent command surface

The agent-specific commands are `spielorders agent provision --root <PATH>` and
`spielorders agent defaults`. They use ordinary human/JSON output conventions;
provisioning accepts the ordinary dry-run option. Defaults is a read-only query of
installed default resource locations and requires no Package. Packaging supplies
actual macOS resource or Linux shared-data paths, using the same canonical templates
the library embeds.

The portable CLI does not prompt for sandbox authorization or know GUI permission UI.
The macOS launcher restores remembered CLI access before ordinary invocations.
Unauthorized Packages refuse through ordinary command results. There is no separate
global installation, agent status command, initialization setup flag, runtime command,
or snapshot command. `package show` remains the complete Package inspection.

### Command and data guidance

`packages-and-adoption.md` explains and exemplifies Package initialization, optional Agent Ready provisioning, Package paths, `add-order` for a newly created Order Directory, and `adopt-order` for an eligible existing direct child. It states the facts these commands require and their effects. For legacy material, filenames and directory components are evidence rather than an automatic field grammar; the agent supplies explicit facts through ordinary CLI assignments. The reference explains existing Vendor and Reusable Free-Entry Value lookup, one-Order-at-a-time adoption, resumability from current Package state, and preservation of existing directory contents. It does not reproduce the full legacy-adoption decision or turn evidence interpretation into a fixed workflow.

`orders-and-items.md` documents Order and Order Item create, show, edit, move, remove, alignment, README, and scoped-validation commands with short examples. Agent-supplied Order facts are `title`, `date_ordered`, and `status`, plus optional Vendor, Orderer, Ordering Institution, currency, Project, and Order Group. Item facts are required Description, Quantity, and Unit Price plus optional Catalog Number. It documents accepted statuses, date and currency formats, optional-field clearing, calculated rather than supplied totals, and one-based Item positions.

`vendors-and-contacts.md` documents Vendor list, show, create, edit, hide, and unhide commands and Vendor Contact list, add, edit, move, and remove commands with short examples. A Vendor requires a Name and may receive general phone, email, website, multiline address, and purchasing notes. A Vendor Contact requires a Name and may receive role, phone, and email. The reference explains Vendor-name selection, optional-field clearing, one-based Contact positions, that a Vendor is never deleted but hidden — reversibly, whether or not Orders reference it, and without affecting how it resolves for them — and the absence of Vendor Merge and placeholder Vendors. IDs are generated and returned by SpielOrders rather than supplied as ordinary data.

`search-and-inspection.md` documents Package show/list/value queries, Order and Vendor inspection, native Order and Vendor search, and OS-indexed Supporting Material search. It explains relevant inputs and result semantics without duplicating the global human-versus-JSON convention.

`diagnostics-and-repair.md` documents Package and scoped validation, Package and Order repair, metadata upgrade once a later metadata version exists, and Package Index status and rebuild commands. It states their required inputs, outputs, dry-run availability, mutation effects, and which ordinary operations reject damaged or unsupported Package state. It leaves selection of the appropriate response to the agent and the diagnostics returned by the CLI.

Every mutating-command example may show `--dry-run` where useful. The skill explains its established CLI meaning: validate and report the proposed mutation without writing; omission commits the named operation. It adds no agent-specific approval ceremony. [Define legacy Order adoption and metadata bootstrapping](009-legacy-adoption.md) owns adoption safety; the agent and its surrounding authorization rules determine how to reason, review, and obtain approval.

### Safety boundaries

The skill carries two compact, non-negotiable facts:

- Never directly edit SpielOrders-managed metadata when a CLI operation exists. Use the CLI so validation, atomic writes, cross-record rules, and Package Index refresh remain consistent. `.spielorders` configuration and JSON are managed metadata; Package and Order READMEs are ordinary user-authored content. Diagnostics may identify damage for which no supported CLI mutation exists, but the skill does not invent an unsupported metadata edit.
- Supporting Material is opaque user-owned content inside an Order Directory, outside SpielOrders-managed metadata and without a required internal layout. Metadata editing, validation, repair, alignment, upgrade, and index operations preserve it. Only an explicit whole-Order removal moves or trashes the complete Order Directory containing it. `search supporting-material` provides OS-indexed lookup without making Supporting Material managed data.
