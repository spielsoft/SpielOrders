---
title: Define durable metadata schemas and version evolution
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

What exact durable field names, representations, schema versions, and cross-record constraints govern `config.toml`, `vendors.json`, `index.json`, and `order.json`; how are unknown and unsupported versions diagnosed; and which explicit, atomic upgrades may transform supported older schemas without being confused with integrity repair?

## Blocking

Blocked by: [Define the SpielOrders package topology](002-package-topology.md), [Define the Order record and lifecycle contract](003-order-record-lifecycle.md), [Define Vendor identity and historical semantics](004-vendor-identity-history.md), and [Define integrity, validation, and repair boundaries](008-integrity-repair.md).

## Resolution

SpielOrders uses one integer metadata version for the entire SpielOrder Package. The contract begins at version `1`; versions are consecutive positive integers rather than semantic versions. Any change to a managed JSON field, representation, requiredness, accepted enumeration, validation meaning, or Package Index projection advances the one package version. Formatting, diagnostic wording, search implementation, and `config.toml` behavior do not.

### Package marker and configuration

The canonical root `.spielorders/version.json` contains exactly:

```json
{
  "metadata_version": 1
}
```

`metadata_version` is the authoritative steady-state version and is replaced last during an upgrade. Every `order.json`, `vendors.json`, and `index.json` repeats that value as `schema_version` so a reader can parse individual files and resume an interrupted upgrade. These per-file fields do not evolve independently.

`.spielorders/config.toml` remains deliberately unversioned and user-editable. It permits two optional keys:

```toml
orders_path = "Orders"
removed_path = "Removed"
```

Omitting `orders_path` means `Orders`; omitting `removed_path` means `Removed`. Each value must be a nonempty `/`-separated relative descendant path. `.` is **not** valid, superseded by [Define the Rust CLI capability contract](006-cli-contract.md). Absolute paths, `..`, empty components, and resolution through a filesystem link outside the SpielOrders Root are invalid. The two paths cannot overlap or occupy `.spielorders`, and `removed_path` must name an ordinary direct-child directory of the SpielOrders Root. Unknown keys are invalid. Package-version information never appears in this file.

### Common JSON representation

- Managed JSON is UTF-8 without a byte-order mark, uses `snake_case` keys and enum values, and rejects unknown object fields.
- Application writes use standard Rust serialization libraries, two-space pretty printing, deterministic schema field order, and one trailing newline. Object key order and insignificant whitespace are not validation conditions.
- Optional fields are omitted when semantically absent; application-written records do not emit `null`. A present optional string that is empty or whitespace-only is valid and means absence. Collection fields are always present, including when empty.
- Single-line application input is trimmed at both ends. Multiline text normalizes line endings to `\n` and trims leading and trailing blank space while preserving interior text. Email, phone, website, and postal-address formats are otherwise uninterpreted.
- Order IDs and Vendor IDs are opaque lowercase hyphenated UUIDv4 strings. UUID ordering, generation time, and package location have no domain meaning.
- Every supported version has checked-in strict JSON Schema Draft 2020-12 documents for `version.json`, `order.json`, `vendors.json`, and `index.json`, using `additionalProperties: false`. The Rust parser and semantic validator are runtime authority; conformance fixtures must keep them aligned with the published schemas.

### `order.json` version 1

```json
{
  "schema_version": 1,
  "package_root": "../..",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Laser components",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
  "orderer": "Ada Lovelace",
  "ordering_institution": "Northwind",
  "currency": "USD",
  "project": "Telescope",
  "order_group": "Summer build",
  "items": [
    {
      "description": "Mirror mount",
      "quantity": 2.0,
      "unit_price": 84.5,
      "catalog_number": "TM-100"
    }
  ]
}
```

`schema_version`, `package_root`, `order_id`, nonblank `title`, `date_ordered`, `status`, and
`items` are required. `package_root` is the technical locator added by
[Define the Rust CLI capability contract](006-cli-contract.md): a normalized relative path
from the Order Directory to its SpielOrders Root, never absolute and never a domain fact. `date_ordered` is a real Gregorian calendar date in `YYYY-MM-DD` form. `status` is exactly `in_preparation`, `hold`, `ordered`, `received`, or `complete`. `vendor_id`, `orderer`, `ordering_institution`, `currency`, `project`, and `order_group` are optional. A present currency is exactly three uppercase ASCII letters; it decorates values without conversion.

Each item requires a nonblank `description`, finite JSON-number `quantity`, and finite JSON-number `unit_price`; `catalog_number` is optional. JSON cannot encode non-finite values, and implementations also reject values that cannot be represented as their supported floating-point type. The `items` array preserves user order and items have no independent IDs.

### `vendors.json` version 1

```json
{
  "schema_version": 1,
  "vendors": [
    {
      "vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822",
      "name": "Tarnwick",
      "phone": "+1 555 0123",
      "email": "sales@example.com",
      "website": "https://example.com",
      "address": "100 Example Avenue\nAnytown, ST 00000",
      "contacts": [
        {
          "name": "Grace Hopper",
          "role": "Sales",
          "phone": "+1 555 0100",
          "email": "grace@example.com"
        }
      ],
      "purchasing_notes": "Use institutional account.",
      "hidden": true
    }
  ]
}
```

`schema_version` and `vendors` are required. Each Vendor requires `vendor_id`, nonblank `name`, and `contacts`. `phone`, `email`, `website`, `address`, `purchasing_notes`, and `hidden` are optional. Each Vendor Contact requires a nonblank `name`; `role`, `phone`, and `email` are optional. Contacts are embedded, ordered, and have no independent IDs. Vendor array order is user-controlled presentation order, never identity.

`hidden` is the Vendor's visibility: a boolean whose presence and value of `true` make the Vendor a Hidden Vendor, no longer offered for new use. Its absence means visible, which is the one canonical spelling — an explicit `false` says the same thing and normalizes to absence — so every Vendor record written before the field existed remains valid and unchanged. Visibility never affects resolution: an Order's Vendor Reference resolves a Hidden Vendor exactly as it resolves any other, and a Vendor is never deleted.

### `index.json` version 1

```json
{
  "schema_version": 1,
  "orders": [
    {
      "order_directory": "Orders/2026_07_19_Tarnwick_Laser_components",
      "order": {
        "schema_version": 1,
        "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
        "title": "Laser components",
        "date_ordered": "2026-07-19",
        "status": "ordered",
        "items": []
      },
      "readme": "User-authored Markdown text"
    }
  ],
  "vendors": []
}
```

`index.json` is a complete, rebuildable projection used to avoid scanning thousands of Order Directories for normal UI loading. `schema_version`, `orders`, and `vendors` are required. Each Order projection requires `order_directory`, a complete canonical `order` object, and the Order README text in `readme`. `order_directory` is a normalized `/`-separated path relative to the SpielOrders Root and cannot escape it. `vendors` contains the complete canonical Vendor objects.

The Order projection is sorted by Date Ordered descending, Order Title ascending, then Order ID; Vendor order matches `vendors.json`. The index contains no Package README, Supporting Material inventory, timestamps, filesystem metadata, diagnostics, pre-normalized search tokens, or presentation layout. Missing, malformed, stale, or wrong-version index content is rebuilt rather than repaired or treated as canonical evidence.

### Cross-record validity

Within a steady-state package, `version.json.metadata_version` and every managed JSON `schema_version` are equal. Order IDs are unique across all discovered Order Directories. Vendor IDs are unique, and Vendor Names are unique after trimming and Unicode case-insensitive comparison. Every present Order `vendor_id` resolves to exactly one Vendor. Required strings are nonblank after trimming. Reusable Free-Entry Values retain their previously decided trimmed, case-insensitive canonical-spelling behavior. Array order is significant only where stated; JSON object order is not.

Malformed syntax, a missing or wrongly typed version, an unknown field, a failed field constraint, or an unsupported schema is diagnosed at the narrowest integrity scope established by [Define integrity, validation, and repair boundaries](008-integrity-repair.md). A missing or invalid `version.json` damages the Package. A newer package version or an older version for which the running build no longer has a complete migration chain remains untouched and diagnostic-only.

### Automatic, resumable upgrades

Opening a supported older package automatically targets the newest metadata version understood by the running build. Migrations are implemented as consecutive transformations and compose across multiple versions; arbitrary targets and downgrades are not supported. Upgrade is distinct from integrity repair: each source artifact must parse and validate under its declared supported schema, and migration may transform only facts defined by that schema without guessing missing domain meaning.

The core upgrades valid `order.json` files one at a time in deterministic Order Directory order, validating and atomically replacing each file. A file already at the one recognized target validates and is skipped, allowing restart after interruption. After every Order reaches the target, the core validates and atomically upgrades `vendors.json`, atomically advances `version.json` last, and rebuilds `index.json`. Each replacement uses standard beside-destination staging; a failed individual replacement preserves that file's previous valid contents. Package-wide rollback is neither required nor attempted because completed per-file upgrades are intentionally resumable.

While the marker is old, canonical files may validly be at the declared source version or the one upgrade target. Other version combinations are diagnosed rather than interpreted. Ordinary package use remains blocked until the upgrade completes. If a malformed or semantically invalid Order prevents completion, valid Orders already upgraded remain upgraded, the marker remains old, and the user must explicitly correct or replace the damaged metadata or move that Order Directory outside the Orders Directory before resuming. Upgrade never infers repair, discards an Order, salvages malformed JSON, or advances the marker past a failed artifact.

### Core and downstream CLI boundary

Following the SpielGantt precedent without copying its contracts, Rust core behavior owns every ordinary SpielOrder Package read and mutation: discovery, configuration, managed JSON, Package and Order READMEs, directory creation and alignment, validation, upgrades, index maintenance, repair, and search. The GUI otherwise manages only app-owned UI preferences and session state outside the Package. Tauri and CLI surfaces are thin adapters over the same typed core operations; neither frontend code nor CLI adapter code duplicates package rules. The sole deliberate exception established by the desktop decision is an expert diagnostic raw-JSON editor: it writes user-supplied text directly without interpreting or validating it at the GUI layer, then invokes Rust validation; invalid text remains damaged and no equivalent library or CLI mutation exists.

The downstream CLI contract starts from one Git-style `spielorders` executable with accepted first-level families `init`, `package`, `order`, `vendor`, `search`, `validate`, `upgrade`, `repair`, and `index`. Detailed subcommands, mutation reports, validation and repair exposure, exit behavior, default human-readable output, and independently versioned machine-readable CLI JSON envelopes remain the question resolved by [Define the Rust CLI capability contract](006-cli-contract.md); on-disk metadata schemas are not reused as CLI response schemas.
