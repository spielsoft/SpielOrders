---
title: Define the Rust CLI capability contract
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

Within the accepted single Git-style `spielorders` executable and its first-level `init`, `package`, `order`, `vendor`, `search`, `validate`, `upgrade`, `repair`, and `index` families, which exact subcommands, mutation reports, validation and repair exposure, exit behavior, default human-readable output, and independently versioned machine-readable JSON envelopes make the GUI replaceable without duplicating Rust core package rules?

## Blocking

Unblocked.

## Resolution

SpielOrders has one human-first, Git-style `spielorders` executable. The CLI and Tauri backend are adapters over the same typed Rust core; neither owns or duplicates Package rules. Every ordinary Package-domain read and mutation required by the GUI must be a core capability, including Package and Order README access and OS-native Supporting Material search. Presentation, window/session state, and app-owned preferences remain outside core. The diagnostic GUI's explicitly accepted raw-JSON editor is the sole exception: it may write expert-supplied text directly and then call core validation, without adding a corresponding library or CLI mutation.

SpielGantt establishes useful precedent for human-readable defaults, opt-in versioned JSON, typed mutation reports, and a shared Rust package boundary. Its command names and JSON shapes are not SpielOrders requirements.

### Targeting and metadata refinements

`--root <PATH>` defaults to `.` and names the root of the command's subject: the SpielOrders Root for Package-level and Vendor commands, and the Order Directory for `order` commands. The command fails when that exact path is not the required kind of root; it never searches ancestors. The separate `--order-root` option does not exist.

To make an Order Directory self-locating without ancestor search, version-1 `order.json` gains a required technical `package_root` field. It is a normalized relative path from the Order Directory to its SpielOrders Root, never an absolute path or a domain fact. Package validation verifies that the resolved root is the Package being validated and that the directory is an active direct child of its configured Orders Directory. A missing, malformed, stale, or mismatched locator damages only that Order and is explicitly repairable. Moving an entire Package preserves the locator; an explicit Order removal updates it when necessary.

This requirement refines the version-1 schema established by [Define durable metadata schemas and version evolution](010-metadata-schemas.md) before implementation; it is not a migration or a Git-like version history.

Order commands also reserve provisional `--uuid <ORDER_ID>` targeting for GUI or scripting evaluation during MVP. It resolves the ID from the current-directory SpielOrder Package and is mutually exclusive with `--root`. It may be removed before the CLI contract is declared stable if MVP usage does not justify it. Human workflows use Order Directory paths, never UUIDs or non-unique Order Titles.

### Command surface

The required human-facing command tree is:

```text
spielorders init [orders_path=Orders] [removed_path=Removed]

spielorders add-order [TITLE | field=value...]
spielorders add-vendor [NAME | field=value...]

spielorders package show
spielorders package readme
spielorders package list orders
spielorders package list vendors
spielorders package values project
spielorders package values orderer
spielorders package values ordering_institution
spielorders package values order_group

spielorders order show
spielorders order edit field=value...
spielorders order readme
spielorders order align
spielorders order remove [--delete]
spielorders order validate
spielorders order repair [case=... action=... field=value...]

spielorders order item list
spielorders order item add field=value...
spielorders order item edit POSITION field=value...
spielorders order item move POSITION position=POSITION
spielorders order item remove POSITION

spielorders vendor show NAME
spielorders vendor edit NAME field=value...
spielorders vendor remove NAME

spielorders vendor contact list NAME
spielorders vendor contact add NAME field=value...
spielorders vendor contact edit NAME POSITION field=value...
spielorders vendor contact move NAME POSITION position=POSITION
spielorders vendor contact remove NAME POSITION

spielorders search orders KEYWORD...
spielorders search vendors KEYWORD...
spielorders search supporting-material KEYWORD...

spielorders validate
spielorders repair [case=... action=... field=value...]
spielorders upgrade [--dry-run]
spielorders index status
spielorders index rebuild
```

Standard generated `--help` and `--version` complete the reference. `package list` owns Package collections so every `order` command retains Order-root semantics. Vendor commands remain Package-rooted because Vendors have no directories.

`add-order "Laser components"` is the title-only shorthand for `add-order title="Laser components"`. The forms cannot be combined. Unspecified Order fields receive their established defaults: current local Date Ordered, `in_preparation`, no Vendor, and no Order Items. `add-vendor "Tarnwick"` is the equivalent Name shorthand. There are no `create-order`, `create-vendor`, or Vendor Merge aliases.

`package show` is the complete GUI-bootstrap read. Its typed result includes Package paths and metadata versions, Package README, healthy Orders, Damaged Order placeholders, Vendors, diagnostics, upgrade and index state, and the four current Reusable Free-Entry Value lists. The core derives those lists once from the loaded Package Index rather than scanning Order Directories or recomputing per dropdown. If later measurement justifies materializing the lists in `index.json`, they remain derived data and require the normal metadata-version change.

`package readme` and `order readme` print only the conventional absolute path, whether or not the optional file presently exists, so humans can compose `cat`, `vi`, or another ordinary tool. Their JSON results also identify the relevant relative path and existence. The Rust core nevertheless provides typed UTF-8 reads and atomic writes for both README kinds because the GUI editor depends on them; it never generates their content.

### Assignment and collection semantics

CLI options govern targeting, output, and execution. Domain facts use canonical `field=value` assignments. Parsing splits on the first `=` so quoted values may contain additional equals signs. Unknown and immutable fields are rejected. An empty value clears an optional scalar; blank required facts remain invalid. A CLI assignment of a new Reusable Free-Entry Value is itself explicit confirmation. Matching values reuse the existing trimmed, case-insensitive canonical spelling, and mutation results distinguish reuse from introduction.

Every command's assignments form one order-independent, fully validated atomic mutation. When an edit enters `ordered` without `date_ordered`, the core applies today's local date. An explicit `date_ordered` in the same mutation overrides that automatic value, enabling historical correction without a second edit.

Order Items and Vendor Contacts have no IDs and use one-based positions. `add` appends unless `position=N` is supplied; `edit`, `move`, and `remove` evaluate positions against the pre-mutation ordered collection. The core rewrites the complete validated owning Order or Vendor aggregate atomically.

Vendor Names are the ordinary Vendor selectors and resolve by the established trimmed, case-insensitive package-unique comparison. Machine responses always retain Vendor IDs. `vendor show` includes summaries of referencing Orders. `vendor remove` is rejected while any Order references the Vendor.

Vendor Merge is removed from the product and core contract. The earlier merge provision in [Define Vendor identity and historical semantics](004-vendor-identity-history.md) is superseded. Duplicate reconciliation during adoption must use an existing Vendor association, a deliberately distinct Vendor, or removal of an unreferenced duplicate; it cannot consolidate two Vendor records.

### Removal and mutation boundaries

`.spielorders/config.toml` now permits optional `orders_path` and `removed_path` keys, defaulting to `Orders` and `Removed`. `orders_path = "."` is no longer valid; it must be a nonempty relative descendant of the SpielOrders Root. `removed_path` must name an ordinary direct-child directory of the SpielOrders Root. The paths cannot overlap, escape the root, occupy `.spielorders`, or resolve through disallowed links. The Removed Orders Directory is created lazily.

`order remove` moves the complete Order Directory, including its managed metadata, README, and opaque Supporting Material, to `<removed_path>/<original-basename>`. It rejects a collision instead of inventing a suffix, updates the technical Package locator when necessary, and rebuilds the Package Index. Removed contents are outside active discovery, indexing, search, validation, upgrade, and repair.

`order remove --delete` sends the complete directory to the platform Trash or Recycle Bin instead of the Removed Orders Directory. It never permanently erases it. Permanent disposal remains ordinary platform trash management.

All mutating commands accept `--dry-run`. A dry run performs validation and emits the same planned mutation report with `committed: false`, but creates no directory, staging file, index rewrite, or cleanup side effect. Commands never prompt and there is no `--yes`: naming the mutation and omitting `--dry-run` is authorization. Preconditions still reject unsafe removal, collisions, referenced Vendor removal, invalid writes, and unsupported repair actions.

The core validates a complete mutation plan before its first write. Managed-file replacements retain the established beside-destination staging, validation, atomic replacement, and rollback rules. Reports distinguish changed, unchanged, rejected, and committed-with-warnings outcomes. A committed canonical mutation followed by a failed derived-index refresh remains committed, reports the warning, and avoids encouraging an unsafe retry.

Supporting Material stays opaque. The only core mutations that may move or dispose of it are the user's explicit whole-Order `remove` operations; validation, repair, alignment, metadata edits, rollback cleanup, and index work never inspect or mutate it.

### Validation, repair, upgrade, and index exposure

Top-level `validate` exhaustively validates the complete Package, including every Order Directory and cross-record constraint. `order validate` validates the selected Order scope. Both return all diagnostics at once, grouped at the narrowest scope.

Bare top-level `repair` diagnoses only actionable Package-level repair cases. Bare `order repair` diagnoses only the selected Order's actionable cases. Supplying `case=... action=...` applies exactly one action offered by that diagnosis, using ordinary `field=value` resolutions where required. Case handles are ephemeral, not domain identities or persistent metadata. Diagnostics provide stable issue codes, evidence, allowed actions, required values, and an exact suggested command.

Core repair never guesses, applies a broad `--fix`, mutates the other scope, partially salvages malformed JSON, interprets unsupported versions, or adopts missing Order metadata. It may apply only the explicit repairs allowed by the integrity decision: a fully valid confirmed replacement for parseable metadata, promotion of a validated staging candidate, explicit correction of an Unresolved Vendor Reference, or explicit adoption of a new Order identity for the chosen side of a duplicate-ID conflict. External correction and later validation remain available when no safe core action exists; the desktop decision treats its GUI-only raw-JSON editor as such an external correction surface, not as a new core repair.

`upgrade` applies the complete supported sequential Metadata Upgrade to the running build's newest version. `--dry-run` reports source, target, deterministic artifact order, already-upgraded artifacts, and blockers. Rerunning the ordinary command resumes; arbitrary targets, per-file upgrades, downgrades, and package-wide rollback do not exist.

`index status` explains whether the derived Package Index is missing, malformed, wrong-version, or stale. `index rebuild` deterministically replaces it from healthy canonical records. Neither command repairs canonical metadata or inspects Supporting Material.

### Search exposure

`search orders` and `search vendors` expose the deterministic native entity lookup. Remaining arguments are whitespace-tokenized keywords with AND semantics even when the shell passes a quoted multiword argument; terms may match different searchable facts. `--case-sensitive` exposes the established case control. Literal text is never interpreted as a phrase or regular expression. Regex is excluded from MVP and the currently planned post-MVP contract; any later regex mode requires its own decision.

Post-MVP structured Order filters use assignments such as `status=ordered`, `vendor=Tarnwick`, and `project="Telescope"`, following the already decided within-filter OR and across-filter AND semantics. Supporting Material search has no case option or structured filters because its provider governs matching.

OS-native Supporting Material search is a required cross-platform core and CLI capability, not a GUI-only feature. `search supporting-material` uses Spotlight on macOS and the applicable native index provider on other platforms. The typed result exposes every provider fact the GUI uses: provider identity and state, complete-versus-partial status, Order grouping, absolute and relative paths, file kind, modification time, rank/quality or its absence, deterministic fallback ordering facts, and provider-specific diagnostics.

The core operation is asynchronous and cancellable so it never blocks the GUI thread, but it returns one settled snapshot rather than a result stream. The CLI waits and prints once. A bounded provider timeout may return available results with `complete: false` and a specific diagnostic. Ctrl-C cancels the CLI query cleanly. There is no JSON-lines contract.

### Human output and process status

Human-readable output is the default. Reads use concise labeled summaries or tables. `order show` includes numbered Items and calculated totals; `vendor show` includes numbered Contacts and referencing Orders. Search preserves decided sorting; Supporting Material results show Order groups and ranked paths. Mutations report the human entity name, affected facts and paths, and index outcome. UUIDs, provider rank, and individual managed-file operations appear only under `--verbose`.

Validation and repair group diagnostics by Package or Order Directory and show severity, stable code, explanation, and the suggested next command. Results go to stdout; warnings and errors go to stderr. Interactive color respects `NO_COLOR`, disappears when redirected, and never carries meaning alone. The CLI has no automatic pager or full-screen mode, and human output is not a parsing contract.

The mutually exclusive global output modes are:

- default human output;
- `--json`, one versioned machine-readable document;
- `--quiet`, no output on success while retaining errors on stderr; and
- `--verbose`, additional human detail.

Exit status is stable and deliberately coarse:

- `0`: success, including an empty search, idempotent no-op, or a committed mutation with a noncanonical derived-index warning;
- `1`: operational or domain failure, rejected mutation, invalid validation result, actionable repair findings, unsupported metadata state, or other failed operation;
- `2`: command-line usage error; and
- platform signal conventions, such as `130` for Ctrl-C.

### Versioned machine-readable contracts

`--json` writes exactly one UTF-8 JSON document to stdout and no incidental text. Every command owns an independently evolving response contract rather than reusing an on-disk metadata schema. The common root is:

```json
{
  "contract": "spielorders.order.show",
  "contract_version": 1,
  "ok": true,
  "context": {
    "package_root": "/absolute/package",
    "order_root": "/absolute/package/Orders/2026_07_20_Laser_components"
  },
  "data": {},
  "diagnostics": []
}
```

`contract` identifies the exact command shape and `contract_version` is that response's consecutive positive integer. It is independent of Package `metadata_version`, individual managed-file `schema_version`, CLI release version, and every other response contract. Incompatible field, representation, requiredness, enum, diagnostic-meaning, or payload change advances only the affected response contract. Formatting and human wording do not. Contracts use `snake_case`, stable enum and diagnostic codes, JSON `null` for absent optional output facts, present empty collections, absolute paths plus relevant Package-relative paths, and domain UUIDs even when the human surface omits them.

Failures under `--json` retain the command's contract with `ok: false`, `data: null`, diagnostics, and a structured `error` carrying a stable code, human message, scope, and relevant paths or identities. Exit status remains authoritative at process level. Usage errors requested under JSON follow the same stdout discipline rather than contaminating the stream with generated human help.

Read-contract payloads are:

- `package.show`: the complete typed Package bootstrap projection described above;
- `package.list.orders` and `package.list.vendors`: ordered entity summaries with identities and roots where applicable;
- `package.values.<kind>`: the canonical-spelling current values and usage counts;
- `package.readme` and `order.readme`: absolute path, relevant relative path, and existence;
- `order.show` and `order.item.list`: complete current Order facts, ordered Items, calculated Item Totals and Order Total, paths, and local diagnostics;
- `vendor.show` and `vendor.contact.list`: complete current Vendor facts, ordered Contacts, and referencing Order summaries;
- `search.orders` and `search.vendors`: each matching entity once in the decided deterministic order, plus query semantics and active filters;
- `search.supporting-material`: the complete one-shot provider/result contract described above;
- `validate` and `order.validate`: validity, scope, stable typed diagnostics, and examined Package/Order context;
- `repair` and `order.repair`: actionable cases, evidence, allowed actions, and required resolution fields;
- `upgrade`: source, target, progress, skipped/completed artifacts, blockers, and final marker state; and
- `index.status`: current index state and typed reasons.

Successful mutation JSON includes both a typed mutation report and the refreshed affected result. The report contains `committed`, `changed`, typed managed operations, old and new paths where relevant, canonical-versus-derived outcomes, index status, and warnings. Order and Item mutations return the complete refreshed Order projection; Vendor and Contact mutations return the complete refreshed Vendor and referencing Order summaries; removal returns the removed entity summary and disposition; repair returns remaining diagnostics in its scope; upgrade and index rebuild return refreshed Package/version/index state. This avoids a mandatory second read without returning the entire Package after every small edit. The GUI consumes the equivalent typed core results directly and never parses CLI JSON.

Machine-readable JSON is an output contract only. Ordinary mutation input remains the human `field=value` language; the CLI does not require request documents that resemble stored metadata.
