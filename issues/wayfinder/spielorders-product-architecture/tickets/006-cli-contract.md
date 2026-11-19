---
title: Define the Rust CLI capability contract
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders has one human-first, Git-style `spielorders` executable. The CLI and native desktop integration call the same library interface; neither owns or duplicates Package rules. Every ordinary Package-domain read and mutation the GUI needs is a library capability, including Package and Order README access and processing Supporting Material search hits. Each front end performs its own OS-native query. Presentation, window and session state, and app-owned preferences stay outside core. The diagnostic GUI's raw-JSON editor is the sole exception: it may write expert-supplied text directly and then call core validation, without a corresponding library or CLI mutation.

SpielGantt is precedent for human-readable defaults, opt-in versioned JSON, typed mutation reports, and a shared Rust package boundary. Its command names and JSON shapes are not SpielOrders requirements.

`contracts/cli/` records implemented response cells, `crates/spielorders-cli/src/contract.rs` is the registry of response identities and of how a version advances, and `crates/spielorders-cli/src/exit.rs` is the closed set of exits. What follows constrains that surface and its future extension.

### Targeting

The command path comes first and modifiers follow it. Options that govern targeting, output, and execution belong to the command they modify; nothing precedes the command path except `--help` and `--version`, which ask for reference rather than modifying a command. Each option is therefore declared where it applies, and one command line is parsed at one level, so no rule about it depends on where a caller wrote it.

`--root <PATH>` defaults to `.` and names the root of the command's subject: the SpielOrders Root for Package-level and Vendor commands, the Order Directory for `order` commands, and any destination directory for `agent provision`. The command fails when that exact path is not the required kind of root; it never searches ancestors. There is no separate `--order-root` option.

An Order Directory is self-locating without ancestor search. `order.json` carries a required technical `package_root` field: a normalized relative path from the Order Directory to its SpielOrders Root, never absolute and never a domain fact. Package validation verifies that the resolved root is the Package being validated and that the directory is an active direct child of its configured Orders Directory. A missing, malformed, stale, or mismatched locator damages only that Order and is explicitly repairable; only `order validate` and `order repair` of such an Order find its Package among the Order Directory's ancestors, because the locator cannot. Moving an entire Package preserves the locator.

Order commands reserve provisional `--uuid <ORDER_ID>` targeting for GUI and scripting evaluation during MVP. It resolves the ID within the current-directory SpielOrder Package, is mutually exclusive with `--root`, and may be withdrawn before the CLI contract is declared stable. Human workflows use Order Directory paths, never UUIDs or non-unique Order Titles.

### Command surface

`package list` owns Package collections so every `order` command keeps Order-root semantics. Vendor commands are Package-rooted because Vendors have no directories. There are no `create-order` or `create-vendor` aliases.

`package show` is the complete GUI-bootstrap read: one call carries everything the desktop needs to open a Package, including the four current value lists. The core derives those lists from the loaded Package Index, which carries the kept values, rather than scanning Order Directories or recomputing per dropdown.

`package readme` and `order readme` print only the conventional absolute path, whether or not the optional file exists, so humans can compose `cat`, `vi`, or another ordinary tool. The core provides typed UTF-8 reads and atomic writes for both README kinds because the GUI editor depends on them; it never generates their content.

### Assignment and collection semantics

Options govern targeting, output, and execution; domain facts use canonical `field=value` assignments. Parsing splits on the first `=`, so a quoted value may contain further equals signs. Unknown and immutable fields are rejected. An empty value clears an optional scalar; blank required facts remain invalid. An Order assignment accepts only a value the Package has: a value it lacks is refused by name unless `--add-values` on `add-order`, `adopt-order`, or `order edit` asks that every such value be added in the same change. Mutation results distinguish reuse of an existing canonical spelling from a newly added value.

Every command's assignments form one order-independent, fully validated, atomic mutation. An edit entering `ordered` without `date_ordered` takes today's local date; an explicit `date_ordered` in the same mutation overrides it, so historical correction needs no second edit.

Order Items and Vendor Contacts have no IDs and use one-based positions. `add` appends unless `position=N` is supplied; `edit`, `move`, and `remove` evaluate positions against the pre-mutation ordered collection. The core rewrites the complete validated owning Order or Vendor aggregate atomically.

A bare `item` token opens one Item group on the command line, and the assignments after it belong to that Item until the next `item` or the end of the line. It is a decided third token class beside options and assignments, not an implementation detail of one command. `adopt-order` is the one command that takes it, because adoption is the one mutation that may carry Items at creation.

Vendor Names are the ordinary Vendor selectors, resolved by the established trimmed, case-insensitive package-unique comparison; machine responses always retain Vendor IDs. `vendor hide` stops offering a Vendor for new use and `vendor unhide` offers it again; neither is rejected on reference grounds, and the record and ID survive either way. `vendor delete` removes a Vendor and its Contacts; while Orders reference it, it refuses with `vendor_referenced`, one entry per referencing Order, unless `--replacement` names the Vendor they reference instead or `--clear` leaves them with none. `vendor merge` merges Vendors into the first one named, the rest in precedence order, and `value merge` keeps the first value named. Resolution ignores visibility. Vendor lists offer only active Vendors until `--all` asks for every Vendor with its visibility. Front ends request the appropriate visibility through the library's list interface rather than implementing their own filter; the desktop Vendors table requests the complete list for its show/hide control.

`value add`, `value rename`, `value merge`, and `value delete` change one kind of value, named as `package values` names it, with `--project` for an Order Group; a deletion takes `--replacement` or `--clear` as `vendor delete` does. Each mutation that can change Orders reports how many as `orders`. [Define Vendor identity and historical semantics](004-vendor-identity-history.md) owns what merging and duplicate reconciliation may do.

### Removal and mutation boundaries

`order remove` moves the complete Order Directory — managed metadata, README, and opaque Supporting Material — to the Removed Orders Directory under its original basename. It rejects a collision instead of inventing a suffix, updates the technical Package locator, and rebuilds the Package Index. Removed contents are outside active discovery, indexing, search, validation, upgrade, and repair. That directory is created lazily, and [Define durable metadata schemas and version evolution](010-metadata-schemas.md) owns what `removed_path` may spell. The library restores a removed Order the same way in reverse: the whole directory returns to the Orders Directory under its basename, with its locator set for that depth, refused when the name is taken there, another Order carries its Order ID, or the removed Order changed after planning. The desktop uses it to undo a removal; the CLI exposes no restore command.

`order remove --delete` removes the Order exactly as `order remove` does and then sends the removed directory to the platform Trash or Recycle Bin, so nothing is permanently erased and a failed Trash move leaves the Order recoverable. The Trash move is the CLI's own platform service, as it is the desktop's. Permanent disposal remains ordinary platform trash management.

`apply` takes a file of ordinary command lines, one JSON array of strings per line, and applies the record changes among them — `order edit`, the `order item` and `vendor contact` changes, `add-vendor`, and `vendor edit` — as one batch: each line is planned against the Package as the earlier lines leave it, one refused line refuses the whole batch and names its line, and otherwise every record the batch changed is written in one guarded write with one index update. Any other command line in the file is refused as `not_batchable`. Execution, output, and Package targeting options belong to the outer `apply`, not individual lines; per-line `--dry-run`, output modes, Vendor `--root`, and provisional `--uuid` refuse. Order lines retain `--root` to name their Order Directory. The response lists every change the batch makes, each as its line would report it alone against the Package the earlier lines leave, naming that line's number in the file.

All mutating commands accept `--dry-run`, which validates and emits the same planned mutation report with `committed: false` while creating no directory, staging file, index rewrite, or cleanup side effect. Naming the mutation and omitting `--dry-run` is authorization, and commands do not prompt, with one exception: `order merge` asks before discarding each directory entry the kept Order already holds, refuses under `--json` or `--quiet` or when there is no terminal to ask, and takes `--yes` to discard them all. Preconditions still reject unsafe removal, collisions, invalid writes, and unsupported repair actions.

The core validates a complete mutation plan before its first write, and managed-file replacements keep the established beside-destination staging, validation, atomic replacement, and rollback rules. Reports distinguish changed, unchanged, rejected, and committed-with-warnings outcomes. A committed canonical mutation followed by a failed derived-index refresh stays committed, reports the warning, and does not encourage an unsafe retry.

Supporting Material stays opaque. Whole-Order removal moves it to the Removed Orders Directory; Order merge uses that same removal for the later Orders after moving their nonconflicting entries into the kept Order. A front end's Trash move acts only on the directory that removal produced. `order align` relocates the containing directory intact. Removal and alignment never inspect, enumerate, hash, or rewrite the opaque tree. Merge enumerates entry names and kinds solely to plan moves and conflicts; it never interprets file contents as domain evidence. Validation, repair, metadata edits, and index work never inspect or mutate Supporting Material.

### Validation, repair, upgrade, and index exposure

Top-level `validate` exhaustively validates the complete Package, including every Order Directory and cross-record constraint; `order validate` validates the selected Order scope. Both return all diagnostics at once, grouped at the narrowest scope.

Bare `repair` diagnoses only actionable Package-level cases and bare `order repair` only the selected Order's. Supplying `case=... action=...` applies exactly one action that diagnosis offered, using ordinary `field=value` resolutions where required. Case handles are ephemeral, never domain identities or persistent metadata. Diagnostics carry stable issue codes, evidence, allowed actions, required values, and an exact suggested command.

Core repair never guesses, applies a broad `--fix`, mutates the other scope, partially salvages malformed JSON, interprets unsupported versions, or adopts missing Order metadata. It may apply only the explicit repairs [Define integrity, validation, and repair boundaries](008-integrity-repair.md) allows. External correction followed by revalidation remains available when no safe core action exists; the desktop decision's GUI-only raw-JSON editor is such an external surface, not a new core repair.

There is no `upgrade` command until a metadata version after version 1 exists. Then `upgrade` applies the complete supported sequential Metadata Upgrade to the running build's newest version, and rerunning the ordinary command resumes. Arbitrary targets, per-file upgrades, downgrades, and package-wide rollback do not exist.

`index status` explains whether the derived Package Index is missing, malformed, wrong-version, or stale, and `index rebuild` deterministically replaces it from healthy canonical records. Neither repairs canonical metadata or inspects Supporting Material.

### Search exposure

`search orders` and `search vendors` expose the deterministic native entity lookup. Remaining arguments are whitespace-tokenized keywords with AND semantics even when the shell passes a quoted multiword argument; terms may match different searchable facts. `--case-sensitive` exposes the established case control. Literal text is never interpreted as a phrase or regular expression. Regex is excluded from MVP and the currently planned post-MVP contract; any later regex mode requires its own decision.

Post-MVP structured Order filters use assignments such as `status=ordered`, `vendor=Tarnwick`, and `project="Telescope"`, following the already decided within-filter OR and across-filter AND semantics. Supporting Material search has no case option or structured filters because its provider governs matching.

OS-native Supporting Material search is a required capability of every front end, not a GUI-only feature. Each front end queries its platform's native index — Spotlight on macOS — over the directory the core names, and hands the core the hits and provider facts; the core filters, groups, and orders them. The core performs no OS-native service. The typed result exposes every provider fact the GUI uses: provider identity and state, complete-versus-partial status, Order grouping, absolute and relative paths, file kind, modification time, rank/quality or its absence, deterministic fallback ordering facts, and provider-specific diagnostics.

Each front end gives the core one settled set of hits rather than a result stream; the core's grouping is synchronous and runs off the caller's UI thread, so no async runtime enters the core or the CLI. The CLI waits with cancellation and a bounded timeout, then prints once. Its provider timeout may return the available results marked partial, with the reason they are partial. Ctrl-C cancels the CLI query cleanly. There is no JSON-lines contract. The GUI gathers until completion or Stop, with no application-imposed timeout, as described in [Effect and adapter boundaries](../../../../docs/architecture/boundaries.md#native-services).

### Human output and process status

Human-readable output is the default, and it is never a parsing contract. Search preserves decided sorting, and Supporting Material results show Order groups and ranked paths. UUIDs, provider rank, and individual managed-file operations appear only under `--verbose`, except the entries an Order merge would discard, which a dry run lists for review; a plan's changes to its records always appear. Results go to stdout; warnings and errors go to stderr. Interactive color respects `NO_COLOR`, disappears when redirected, and never carries meaning alone. The CLI has no automatic pager or full-screen mode.

The output modes are mutually exclusive, and each is written on the command it modifies like any other option. They are default human output; `--json`, one versioned machine-readable document; `--quiet`, no output on success while retaining errors on stderr; and `--verbose`, additional human detail.

Exit status is stable, deliberately coarse, and closed: `0` for success, `1` for an operational or domain failure, `2` for a command-line usage error, and the platform signal conventions such as `130` for Ctrl-C. Three rules bind the mapping. A result is invalid exactly when some finding carries error severity, which is the predicate the core already computes, so severity alone decides an exit status and the availability of a repair never does. A read that answers exits `0` and carries its verdict as a payload member: `repair`'s bare diagnosis is an answer rather than a verdict whatever the severity of what it found, and reaches `1` only when it could not answer or when a requested repair action is refused or fails. `ok` and the exit status are one fact for every command.

### Versioned machine-readable contracts

`--json` writes exactly one UTF-8 JSON document to stdout and no incidental text. The CLI builds each payload from what the core's interface returns, as any other caller does; the CLI owns its JSON, and the core's failure and finding codes appear in it unchanged. No `contract_version` advances before the release [the roadmap](../../../roadmaps/spielorders-implementation-roadmap.md) names.

A failure retains the command's contract with `ok: false`, `data: null`, its diagnostics, and a structured error whose code is the core's own code for that failure, or the front end's own code for a failure of its own platform service. Exit status stays authoritative at process level. A usage error is not a command's response and carries no command's identity, and one requested under JSON keeps the same stdout discipline rather than contaminating the stream with generated human help.

Three rules govern the payloads. A sub-list never has its own contract identity: a show and its sub-list are one projection under one contract, differing only in what they render for a human. A validating command carries the offered repair in its own payload, in `repair`'s action vocabulary, rather than on the shared diagnostic projection. An invalid validation result is a failure like any other and carries `data: null`, so validity is `ok` and the exit status, never a payload member.

Successful mutation JSON returns both a typed mutation report and the refreshed affected result, so a caller needs no mandatory second read and no command returns the entire Package after a small edit. The GUI consumes the equivalent typed core results directly and never parses CLI JSON.

Machine-readable JSON is an output contract only. Ordinary mutation input remains the human `field=value` language; the CLI does not require request documents that resemble stored metadata.
