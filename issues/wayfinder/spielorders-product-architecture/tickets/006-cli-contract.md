---
title: Define the Rust CLI capability contract
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Decision

SpielOrders has one human-first, Git-style `spielorders` executable. The CLI and Tauri backend are adapters over the same typed Rust core; neither owns or duplicates Package rules. Every ordinary Package-domain read and mutation the GUI needs is a core capability, including Package and Order README access and OS-native Supporting Material search. Presentation, window and session state, and app-owned preferences stay outside core. The diagnostic GUI's raw-JSON editor is the sole exception: it may write expert-supplied text directly and then call core validation, without a corresponding library or CLI mutation.

SpielGantt is precedent for human-readable defaults, opt-in versioned JSON, typed mutation reports, and a shared Rust package boundary. Its command names and JSON shapes are not SpielOrders requirements.

Every command in this contract is built. `contracts/cli/` records each command's response cells, `crates/spielorders-cli/src/contract.rs` is the registry of response identities and of how a version advances, and `crates/spielorders-cli/src/exit.rs` is the closed set of exits. What follows is what that surface may never become.

### Targeting

The command path comes first and modifiers follow it. Options that govern targeting, output, and execution belong to the command they modify; nothing precedes the command path except `--help` and `--version`, which ask for reference rather than modifying a command. Each option is therefore declared where it applies, and one command line is parsed at one level, so no rule about it depends on where a caller wrote it.

`--root <PATH>` defaults to `.` and names the root of the command's subject: the SpielOrders Root for Package-level and Vendor commands, the Order Directory for `order` commands. The command fails when that exact path is not the required kind of root; it never searches ancestors. There is no separate `--order-root` option.

An Order Directory is self-locating without ancestor search. `order.json` carries a required technical `package_root` field: a normalized relative path from the Order Directory to its SpielOrders Root, never absolute and never a domain fact. Package validation verifies that the resolved root is the Package being validated and that the directory is an active direct child of its configured Orders Directory. A missing, malformed, stale, or mismatched locator damages only that Order and is explicitly repairable. Moving an entire Package preserves the locator.

Order commands reserve provisional `--uuid <ORDER_ID>` targeting for GUI and scripting evaluation during MVP. It resolves the ID within the current-directory SpielOrder Package, is mutually exclusive with `--root`, and may be withdrawn before the CLI contract is declared stable. Human workflows use Order Directory paths, never UUIDs or non-unique Order Titles.

### Command surface

`package list` owns Package collections so every `order` command keeps Order-root semantics. Vendor commands are Package-rooted because Vendors have no directories. There are no `create-order`, `create-vendor`, or Vendor Merge aliases.

`package show` is the complete GUI-bootstrap read: one call carries everything the desktop needs to open a Package, including the four current Reusable Free-Entry Value lists. The core derives those lists from the loaded Package Index rather than scanning Order Directories or recomputing per dropdown. Materializing them in `index.json` would leave them derived data and would require the normal metadata-version change.

`package readme` and `order readme` print only the conventional absolute path, whether or not the optional file exists, so humans can compose `cat`, `vi`, or another ordinary tool. The core provides typed UTF-8 reads and atomic writes for both README kinds because the GUI editor depends on them; it never generates their content.

### Assignment and collection semantics

Options govern targeting, output, and execution; domain facts use canonical `field=value` assignments. Parsing splits on the first `=`, so a quoted value may contain further equals signs. Unknown and immutable fields are rejected. An empty value clears an optional scalar; blank required facts remain invalid. A CLI assignment of a new Reusable Free-Entry Value is itself the explicit confirmation that value requires, and mutation results distinguish reuse of an existing canonical spelling from introduction of a new one.

Every command's assignments form one order-independent, fully validated, atomic mutation. An edit entering `ordered` without `date_ordered` takes today's local date; an explicit `date_ordered` in the same mutation overrides it, so historical correction needs no second edit.

Order Items and Vendor Contacts have no IDs and use one-based positions. `add` appends unless `position=N` is supplied; `edit`, `move`, and `remove` evaluate positions against the pre-mutation ordered collection. The core rewrites the complete validated owning Order or Vendor aggregate atomically.

A bare `item` token opens one Item group on the command line, and the assignments after it belong to that Item until the next `item` or the end of the line. It is a decided third token class beside options and assignments, not an implementation detail of one command. `adopt-order` is the one command that takes it, because adoption is the one mutation that may carry Items at creation.

Vendor Names are the ordinary Vendor selectors, resolved by the established trimmed, case-insensitive package-unique comparison; machine responses always retain Vendor IDs. A Vendor is never deleted: `vendor hide` stops offering one for new use and `vendor unhide` offers it again, neither is rejected on reference grounds, and the record and ID survive either way. Resolution ignores visibility. Vendor lists offer only active Vendors until `--all` asks for every Vendor with its visibility. The core does not filter — it exposes both the complete list with visibility and the offered-only list — so that default is a presentation choice the CLI makes, and the desktop Vendors table consumes the complete list for its show/hide control.

Vendor Merge is absent from the product and the core contract; [Define Vendor identity and historical semantics](004-vendor-identity-history.md) owns what duplicate reconciliation may do instead.

### Removal and mutation boundaries

`order remove` moves the complete Order Directory — managed metadata, README, and opaque Supporting Material — to the Removed Orders Directory under its original basename. It rejects a collision instead of inventing a suffix, updates the technical Package locator, and rebuilds the Package Index. Removed contents are outside active discovery, indexing, search, validation, upgrade, and repair. That directory is created lazily, and [Define durable metadata schemas and version evolution](010-metadata-schemas.md) owns what `removed_path` may spell.

`order remove --delete` sends the complete directory to the platform Trash or Recycle Bin instead, and never permanently erases it. Permanent disposal remains ordinary platform trash management.

All mutating commands accept `--dry-run`, which validates and emits the same planned mutation report with `committed: false` while creating no directory, staging file, index rewrite, or cleanup side effect. Commands never prompt and there is no `--yes`: naming the mutation and omitting `--dry-run` is authorization. Preconditions still reject unsafe removal, collisions, invalid writes, and unsupported repair actions.

The core validates a complete mutation plan before its first write, and managed-file replacements keep the established beside-destination staging, validation, atomic replacement, and rollback rules. Reports distinguish changed, unchanged, rejected, and committed-with-warnings outcomes. A committed canonical mutation followed by a failed derived-index refresh stays committed, reports the warning, and does not encourage an unsafe retry.

Supporting Material stays opaque. **Disposal is `remove`'s exclusive act**: the only core mutations that may dispose of Supporting Material — to the Removed Orders Directory, or to the platform Trash under `--delete` — are the user's explicit whole-Order `remove` operations. **Relocating a container intact is a different act from inspecting or mutating its contents**, and two operations relocate a whole Order Directory: whole-Order disposition, and `order align`, which renames the containing directory. Neither inspects, enumerates, hashes, or rewrites the opaque tree, and neither treats its contents as domain evidence. Validation, repair, alignment, metadata edits, rollback cleanup, and index work never inspect or mutate Supporting Material.

### Validation, repair, upgrade, and index exposure

Top-level `validate` exhaustively validates the complete Package, including every Order Directory and cross-record constraint; `order validate` validates the selected Order scope. Both return all diagnostics at once, grouped at the narrowest scope.

Bare `repair` diagnoses only actionable Package-level cases and bare `order repair` only the selected Order's. Supplying `case=... action=...` applies exactly one action that diagnosis offered, using ordinary `field=value` resolutions where required. Case handles are ephemeral, never domain identities or persistent metadata. Diagnostics carry stable issue codes, evidence, allowed actions, required values, and an exact suggested command.

Core repair never guesses, applies a broad `--fix`, mutates the other scope, partially salvages malformed JSON, interprets unsupported versions, or adopts missing Order metadata. It may apply only the explicit repairs [Define integrity, validation, and repair boundaries](008-integrity-repair.md) allows. External correction followed by revalidation remains available when no safe core action exists; the desktop decision's GUI-only raw-JSON editor is such an external surface, not a new core repair.

`upgrade` applies the complete supported sequential Metadata Upgrade to the running build's newest version, and rerunning the ordinary command resumes. Arbitrary targets, per-file upgrades, downgrades, and package-wide rollback do not exist.

`index status` explains whether the derived Package Index is missing, malformed, wrong-version, or stale, and `index rebuild` deterministically replaces it from healthy canonical records. Neither repairs canonical metadata or inspects Supporting Material.

### Search exposure

`search orders` and `search vendors` expose the deterministic native entity lookup. Remaining arguments are whitespace-tokenized keywords with AND semantics even when the shell passes a quoted multiword argument; terms may match different searchable facts. `--case-sensitive` exposes the established case control. Literal text is never interpreted as a phrase or regular expression. Regex is excluded from MVP and the currently planned post-MVP contract; any later regex mode requires its own decision.

Post-MVP structured Order filters use assignments such as `status=ordered`, `vendor=Tarnwick`, and `project="Telescope"`, following the already decided within-filter OR and across-filter AND semantics. Supporting Material search has no case option or structured filters because its provider governs matching.

OS-native Supporting Material search is a required cross-platform core and CLI capability, not a GUI-only feature. `search supporting-material` uses Spotlight on macOS and the applicable native index provider on other platforms. The typed result exposes every provider fact the GUI uses: provider identity and state, complete-versus-partial status, Order grouping, absolute and relative paths, file kind, modification time, rank/quality or its absence, deterministic fallback ordering facts, and provider-specific diagnostics.

The core operation is a synchronous call taking a cancellation and a bounded timeout, returning one settled snapshot rather than a result stream. It runs off the caller's UI thread — the desktop shell calls it from a worker — so no async runtime enters the core or the CLI. The CLI waits and prints once. A bounded provider timeout may return available results with `complete: false` and a specific diagnostic. Ctrl-C cancels the CLI query cleanly. There is no JSON-lines contract.

### Human output and process status

Human-readable output is the default, and it is never a parsing contract. Search preserves decided sorting, and Supporting Material results show Order groups and ranked paths. UUIDs, provider rank, and individual managed-file operations appear only under `--verbose`. Results go to stdout; warnings and errors go to stderr. Interactive color respects `NO_COLOR`, disappears when redirected, and never carries meaning alone. The CLI has no automatic pager or full-screen mode.

The output modes are mutually exclusive, and each is written on the command it modifies like any other option. They are default human output; `--json`, one versioned machine-readable document; `--quiet`, no output on success while retaining errors on stderr; and `--verbose`, additional human detail.

Exit status is stable, deliberately coarse, and closed: `0` for success, `1` for an operational or domain failure, `2` for a command-line usage error, and the platform signal conventions such as `130` for Ctrl-C. Three rules bind the mapping. A result is invalid exactly when some finding carries error severity, which is the predicate the core already computes, so severity alone decides an exit status and the availability of a repair never does. A read that answers exits `0` and carries its verdict as a payload member: `repair`'s bare diagnosis is an answer rather than a verdict whatever the severity of what it found, and reaches `1` only when it could not answer or when a requested repair action is refused or fails. `ok` and the exit status are one fact for every command.

### Versioned machine-readable contracts

`--json` writes exactly one UTF-8 JSON document to stdout and no incidental text. Every command owns an independently evolving response contract rather than reusing an on-disk metadata schema, and no `contract_version` advances before the release [the roadmap](../../../roadmaps/spielorders-implementation-roadmap.md) names.

A failure retains the command's contract with `ok: false`, `data: null`, its diagnostics, and a structured error. Exit status stays authoritative at process level. A usage error is not a command's response and carries no command's identity, and one requested under JSON keeps the same stdout discipline rather than contaminating the stream with generated human help.

Three rules govern the payloads. A sub-list never has its own contract identity: a show and its sub-list are one projection under one contract, differing only in what they render for a human. A validating command carries the offered repair in its own payload, in `repair`'s action vocabulary, rather than on the shared diagnostic projection. An invalid validation result is a failure like any other and carries `data: null`, so validity is `ok` and the exit status, never a payload member.

Successful mutation JSON returns both a typed mutation report and the refreshed affected result, so a caller needs no mandatory second read and no command returns the entire Package after a small edit. The GUI consumes the equivalent typed core results directly and never parses CLI JSON.

Machine-readable JSON is an output contract only. Ordinary mutation input remains the human `field=value` language; the CLI does not require request documents that resemble stored metadata.
