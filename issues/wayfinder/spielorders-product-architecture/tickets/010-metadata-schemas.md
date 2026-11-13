---
title: Define durable metadata schemas and version evolution
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders uses one integer metadata version for the entire SpielOrder Package. The contract begins at version `1`; versions are consecutive positive integers rather than semantic versions. Any change to a managed JSON field, representation, requiredness, accepted enumeration, validation meaning, or Package Index projection advances the one package version. Formatting, diagnostic wording, search implementation, and `config.toml` behavior do not.

Version 1 is built. `contracts/schemas/v1` holds the strict schema for `version.json`, `order.json`, `vendors.json`, and `index.json`, and `contracts/examples/v1` holds the conforming documents. Those files are the record of every field name, type, and requiredness; what follows is what they may never become.

### Package marker and configuration

The canonical root `.spielorders/version.json` carries `metadata_version` alone. It is the authoritative steady-state version and is replaced last during an upgrade. Every `order.json`, `vendors.json`, and `index.json` repeats that value as `schema_version` so a reader can parse individual files and resume an interrupted upgrade. These per-file fields do not evolve independently.

No `metadata_version` or managed-file `schema_version` advances before the release [the roadmap](../../../roadmaps/spielorders-implementation-roadmap.md) names.

`.spielorders/config.toml` remains deliberately unversioned and user-editable. It permits exactly the optional `orders_path` and `removed_path` keys, meaning `Orders` and `Removed` when omitted. Each value must be a nonempty `/`-separated relative descendant path; `.` is never valid. Absolute paths, `..`, empty components, and resolution through a filesystem link outside the SpielOrders Root are invalid. The two paths cannot overlap or occupy `.spielorders`, and `removed_path` must name an ordinary direct-child directory of the SpielOrders Root. Unknown keys are invalid. Package-version information never appears in this file.

### Common JSON representation

- Managed JSON is UTF-8 without a byte-order mark, uses `snake_case` keys and enum values, and rejects unknown object fields.
- Application writes use standard Rust serialization libraries, two-space pretty printing, deterministic schema field order, and one trailing newline. Object key order and insignificant whitespace are not validation conditions.
- Optional fields are omitted when semantically absent; application-written records do not emit `null`. A present optional string that is empty or whitespace-only is valid and means absence. Collection fields are always present, including when empty.
- Single-line application input is trimmed at both ends. Multiline text normalizes line endings to `\n` and trims leading and trailing blank space while preserving interior text. Email, phone, website, and postal-address formats are otherwise uninterpreted.
- Order IDs and Vendor IDs are opaque lowercase hyphenated UUIDv4 strings. UUID ordering, generation time, and package location have no domain meaning.
- Every supported version has checked-in strict JSON Schema Draft 2020-12 documents using `additionalProperties: false`. The Rust parser and semantic validator are runtime authority; conformance fixtures must keep them aligned with the published schemas.

### What the records mean

`order.json` carries the technical `package_root` locator owned by [Define the Rust CLI capability contract](006-cli-contract.md): a normalized relative path from the Order Directory to its SpielOrders Root, never absolute and never a domain fact. `date_ordered` is a real Gregorian calendar date. A present `currency` decorates values without conversion. Items preserve user order, have no independent IDs, and require finite JSON-number `quantity` and `unit_price` — JSON cannot encode a non-finite value, and implementations also reject values their supported floating-point type cannot represent. An Item's optional `catalog_number` is single-line text and its optional `notes` is multiline text.

Vendor Contacts are embedded, ordered, and have no independent IDs. Vendor array order is user-controlled presentation order, never identity. `hidden` is the Vendor's visibility: present and `true` makes the Vendor a Hidden Vendor, no longer offered for new use. Absence means visible and is the one canonical spelling — an explicit `false` says the same thing and normalizes to absence — so a Vendor record without the field is valid as written. Visibility never affects resolution: an Order's Vendor Reference resolves a Hidden Vendor exactly as it resolves any other, and a Vendor is deleted only when no Order references it.

`index.json` is a complete, rebuildable projection that exists to avoid scanning thousands of Order Directories for normal UI loading. Each Order projection carries the Order Directory path relative to the SpielOrders Root, which can never escape it, a complete canonical Order object, and the Order README text; `vendors` carries the complete canonical Vendor objects. The Order projection is sorted by Date Ordered descending, Order Title ascending, then Order ID; Vendor order matches `vendors.json`. The index contains no Package README, Supporting Material inventory, timestamps, filesystem metadata, diagnostics, pre-normalized search tokens, or presentation layout. Missing, malformed, stale, or wrong-version index content is rebuilt rather than repaired or treated as canonical evidence.

### Cross-record validity

Within a steady-state package, `version.json.metadata_version` and every managed JSON `schema_version` are equal. Order IDs are unique across all discovered Order Directories. Vendor IDs are unique, and Vendor Names are unique after trimming and Unicode case-insensitive comparison. Every present Order `vendor_id` resolves to exactly one Vendor. Required strings are nonblank after trimming. Reusable Free-Entry Values match on trimmed, case-insensitive comparison and reuse the existing canonical spelling. Array order is significant only where stated; JSON object order is not.

Malformed syntax, a missing or wrongly typed version, an unknown field, a failed field constraint, or an unsupported schema is diagnosed at the narrowest integrity scope established by [Define integrity, validation, and repair boundaries](008-integrity-repair.md). A missing or invalid `version.json` damages the Package. A newer package version, or an older version for which the running build no longer has a complete migration chain, remains untouched and diagnostic-only.

### Automatic, resumable upgrades

The upgrade mechanism is built together with the first metadata version after version 1, and not before: until then every version other than 1 is unsupported and diagnostic-only, and no load, command, or capability offers an upgrade. When it is built, it follows the rules below.

Opening a supported older package automatically targets the newest metadata version understood by the running build. Migrations are implemented as consecutive transformations and compose across multiple versions; arbitrary targets and downgrades are not supported. Upgrade is distinct from integrity repair: each source artifact must parse and validate under its declared supported schema, and migration may transform only facts defined by that schema without guessing missing domain meaning.

The core upgrades valid `order.json` files one at a time in deterministic Order Directory order, validating and atomically replacing each file. A file already at the one recognized target validates and is skipped, allowing restart after interruption. After every Order reaches the target, the core validates and atomically upgrades `vendors.json`, atomically advances `version.json` last, and rebuilds `index.json`. Each replacement uses standard beside-destination staging; a failed individual replacement preserves that file's previous valid contents. Package-wide rollback is neither required nor attempted because completed per-file upgrades are intentionally resumable.

While the marker is old, canonical files may validly be at the declared source version or the one upgrade target. Other version combinations are diagnosed rather than interpreted. Ordinary package use remains blocked until the upgrade completes. If a malformed or semantically invalid Order prevents completion, valid Orders already upgraded remain upgraded, the marker remains old, and the user must explicitly correct or replace the damaged metadata or move that Order Directory outside the Orders Directory before resuming. Upgrade never infers repair, discards an Order, salvages malformed JSON, or advances the marker past a failed artifact.

### Core and downstream CLI boundary

Following the SpielGantt precedent without copying its contracts, Rust core behavior owns every ordinary SpielOrder Package read and mutation: discovery, configuration, managed JSON, Package and Order READMEs, directory creation and alignment, validation, upgrades, index maintenance, repair, and search. The GUI otherwise manages only app-owned UI preferences and session state outside the Package. Desktop and CLI surfaces are thin adapters over the same typed core operations; neither frontend code nor CLI adapter code duplicates package rules. The sole deliberate exception established by the desktop decision is an expert diagnostic raw-JSON editor: it writes user-supplied text directly without interpreting or validating it at the GUI layer, then invokes Rust validation; invalid text remains damaged and no equivalent library or CLI mutation exists.

[Define the Rust CLI capability contract](006-cli-contract.md) owns the CLI's subcommands, mutation reports, validation and repair exposure, exit behavior, human output, and versioned machine-readable envelopes. On-disk metadata schemas are never reused as CLI response schemas.
