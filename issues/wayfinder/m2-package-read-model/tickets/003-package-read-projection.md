---
title: Define the complete M2 Package read projection and load-state contract
status: closed
closed_at: 2026-08-13
parent: Chart the M2 Package read model
labels:
  - wayfinder:grilling
assignee: /root
---

## Decision

The projection is built. The core crate's public surface is the record of every result
variant, field, and collection named below; what follows is what that projection may
never become.

### Boundary and result variants

M2 exposes one shared-core Package load result. It is a tagged result rather than
a partially populated universal Package object:

- `unmanaged` means the selected ordinary directory has no root `.spielorders`
  entry. It carries only the selected absolute root. It has no Package-relative
  paths, Package projection, diagnostics, metadata/upgrade state, or Package Index
  record. Initialization remains M3 behavior.
- `available` carries the complete current-version `PackageProjection` below. A
  current Package remains ordinarily available when it has warnings or Damaged
  Orders; only the affected Orders are quarantined.
- `diagnostic_only` carries a `DamagedPackageProjection`: the managed paths and
  version facts that were actually established, the complete diagnostics supported
  by the load, and Package Index state. It carries no Vendors, healthy Order facts,
  reusable values, or other partial canonical domain projection. This is the result
  for Package-scoped canonical errors, including an unsupported newer version or a
  version for which no complete supported chain exists.
- `upgrade_pending` carries an `UpgradeProjection` for a valid supported older
  Package or a recognized resumable source/target mixture. It is distinct from
  damage, exposes no ordinary current-version Package projection, and authorizes
  no upgrade writes in M2.
- `index_replacement_failed` carries the established managed context, diagnostics
  supported by the attempted canonical load, the retained Package Index assessment,
  and the closed contract's `package_index_replacement_failed` operation failure.
  It returns no ordinarily usable Package projection. This blocks opening without
  calling the independently read canonical state a Damaged Package.
- `operation_failed` carries the stable typed operation failure and whatever root
  or managed paths had already been established when a filesystem operation could
  not produce any of the results above. It does not infer canonical damage. A
  required canonical artifact that was successfully observed as persistently
  missing, wrong-kind, unreadable, or invalid is instead represented once by the
  applicable diagnostic, as already decided.

These variants are the core capability result, not a CLI envelope, Tauri command,
GUI session state, retry model, or presentation decision. Diagnostics, Package
Index assessment/refresh, metadata upgrade state, and operation failures remain
separate typed concepts inside the result.

### Path and managed-context projection

Every established managed location uses one `Location` value: an absolute native
path plus a normalized `/`-separated Package-relative path. The SpielOrders Root's
relative path is `.`. A relative path is absent only when Package membership has
not been established, as in `unmanaged` or an early `operation_failed` result.
No path is made absolute by following the final entry through a link.

The managed context names every established Package-owned location, and each discovered
Order entry names its own. A directory basename is obtainable from the path and is never
duplicated as an identity. Untrusted or unexamined paths and versions are absent rather
than guessed.

### Metadata and upgrade state

Managed results carry a read-only metadata state that distinguishes a current Package
from one whose upgrade is required, resumable, blocked, or unsupported, and from one
whose version evidence was untrustworthy or unexamined. It is read-only: no state it
reports authorizes a write.

Upgrade-required, resumable, and blocked projections carry source and target
versions plus per-artifact observations in the already-decided deterministic
upgrade order: Order records by bytewise lexical Package-relative Order Directory
path, then `vendors.json`, `version.json`, and the derived index step. Each
observation says only `source`, `target`, `blocked`, or `not_assessed` and points to
the exact location; blockers are the separately grouped diagnostics. It does not
contain mutation plans, staging actions, retry actions, or inferred migrated facts.

Metadata version 1 remains the only schema in this repository. Consequently the
version-1 M2 build can produce `current`, `unsupported`, or `not_assessed`; it must
not fabricate an older schema merely to exercise the other states. The other
variants become producible only when a later explicitly adopted version adds a
complete supported source/target chain. This type contract records resumability
without moving upgrade execution out of M3.

### Available current-Package projection

The available projection carries the established managed paths and metadata version, a
Package README reference without eagerly read text, the healthy Orders with their
complete normalized records and calculated totals, the Damaged Order placeholders, the
complete normalized Vendors, the four Reusable Free-Entry Value collections, the
diagnostic groups this load supports, and the Package Index record. A placeholder
carries Order paths, an optional trustworthy Order ID, and a README reference — never a
fabricated title, date, status, Vendor, total, Item, or other partial canonical fact.

The projection does not duplicate a resolved Vendor object into each Order.
An Order retains its optional Vendor ID and adapters resolve it against the one
projected Vendor collection. The core has already validated every healthy
reference. This is sufficient for later tables, inspectors, `package show`, and
value suggestions without making an adapter parse metadata or own Package rules.

Damaged Order placeholders are one-for-one with discovered damaged Order
candidates. Duplicate Order IDs therefore produce a placeholder for every
conflicting directory. Each placeholder may expose the duplicated ID because its
shape is trustworthy, while the already-decided per-Order diagnostics carry the
same complete deterministic conflict set. The ID is evidence, not a unique lookup
key. A malformed or missing ID remains absent. The projection never chooses a
survivor.

Diagnostics live once in the top-level ordered `diagnostic_groups` collection
defined by the closed diagnostic decision. A placeholder is associated with its
Order group by the exact Order Directory location; diagnostics are not copied into
both the placeholder and a second flat list. Empty groups are omitted, while the
top-level collection is always present. Derived-index reasons remain within the
index record, and operation failures remain on failed result variants rather than
inside diagnostic groups.

### Reusable Free-Entry Value lists

The four value lists derive only from normalized facts of healthy Orders in this
projection; Damaged Orders never contribute. Each entry contains `value` and
`usage_count`. Blank/absent values contribute nothing. Values equal under the
existing trimmed full-Unicode case-insensitive comparison form one entry.

When externally edited valid records contain more than one spelling in one group,
the canonical spelling is the first occurrence in deterministic healthy-Order
order. Entries then sort by bytewise UTF-8 canonical spelling. Counts are the
number of healthy Orders using the group. All four collections are present even
when empty and are derived once in core from the accepted index/canonical
projection; they are not persisted in metadata version 1.

### README references, cached projection, and explicit reads

README access is JIT. A `ReadmeReference` names the document kind and exact
location. An explicit core README read returns the same reference, `exists`, an
optional `text`, and its document diagnostic group:

- absence returns `exists: false`, present empty text, and no diagnostic;
- an ordinary readable UTF-8 file returns `exists: true`, its exact decoded text,
  and no diagnostic; and
- wrong-kind, unreadable, or invalidly encoded content returns `exists: true`, no
  text, and the one applicable `readme_unavailable` warning.

The read observes only that exact path. It does not list the containing directory,
inspect Supporting Material, mutate bytes, or reuse a prior warning as current
evidence. `exists` means the path was observed, not that it was an ordinary usable
file.

Because metadata version 1 requires Order README text in `index.json`, every
healthy Order also carries the README text the load established, when it
established one. On the fast path it comes from the validated current index. On
rebuild it is the exact decoded text, or absence as empty.

A rebuild that observed an unavailable document establishes no text. The healthy
Order carries none, and the rebuild does not run: version 1 has no spelling for a
README whose text is unknown, and the empty string is not one — writing it would
record a document that holds bytes as one that holds none, in the durable file a
later load proves itself current against. The stored index keeps whatever a load
that could read the document left there. This field is derived cache content, not
an assertion that a later explicit read will succeed. Damaged Orders have no index
text. The Package README
is never indexed; an adapter needing it composes Package load with its explicit
README read. Editing and atomic README writes remain later mutation behavior.

### Package Index record

The projection preserves the closed index-state and refresh-outcome vocabularies
exactly, and the visible pair represents the state after the attempted load:

- a trusted fast path is `current` / `not_needed`;
- a successful canonical rebuild is `current` / `rebuilt` and does not retain the
  prior derived problem as a diagnostic;
- canonical damage or an upgrade-only result retains any supported assessment
  already made, or uses `not_assessed`, with `not_attempted`;
- a rebuild declined because a canonical input the index must carry was not
  established — for example, an Order README that exists and could not be read —
  likewise retains the assessment already made, with
  `not_attempted`. The load remains available and the Order remains healthy; only
  the cache is left alone; and
- failed required replacement retains the original missing, malformed,
  wrong-version, or stale assessment and its ordered typed reasons with
  `replacement_failed`.

Malformed, wrong-version, and stale reasons use only the already-closed derived
evidence vocabulary and order. Missing, current, and not-assessed need no invented
reason. The record also carries the exact index location. It contains neither
diagnostic severity nor a claim about canonical health.

### Deterministic collections and absent evidence

Every collection has an explicit deterministic order: healthy Orders follow the
version-1 index contract's order, damaged Orders follow bytewise lexical
Package-relative Order Directory path, Vendors, Contacts, and Items preserve canonical
array order with totals aligned positionally, and reusable values, diagnostics, and
related path and identity collections keep the orders their own decisions fix. Healthy
and damaged entries stay separate rather than inventing a cross-kind sort key for
damaged facts.

All collections are present when empty. Semantically absent optional canonical
facts remain absent. Evidence that was untrusted, not established, or not examined
is absent and never represented by an empty string, zero, false, placeholder
identity, or fabricated enum value. README absence is the deliberate exception
defined above: its explicit read text is empty because absence semantically means
an empty document. Calculated totals are present for every healthy Order, including
zero for an empty Items collection.

### Fast path, rebuilding, and evidence completeness

The current-index fast path carries forward the closed decision exactly: it reads
`config.toml`, `version.json`, and `index.json`, lists the Orders Directory's direct
children, and obtains only the conservative no-follow observations needed for
membership and freshness. It does not read every `order.json` or `vendors.json`.

After complete index validation and freshness proof, core may use the index for:

- complete healthy Order and Vendor facts and their trustworthy identities;
- the healthy Order paths matched against discovered candidates;
- ordered Items and Contacts;
- cached Order README text;
- calculated Item/Order totals and resolved Vendor associations; and
- the four derived Reusable Free-Entry Value lists.

Config/version facts, configured paths, direct-child membership and structural
warnings, freshness observations, and index assessment come from their own reads
and observations, never from index content. Package README text, Package or Order
README availability at the time of an explicit read, Damaged Order placeholders,
diagnostics, upgrade execution state, reserved/staging contents, operation
failures, and all Supporting Material facts never come from version-1 index
content.

A fast-path result intentionally contains no latent diagnostics for canonical
`vendors.json` or `order.json` content that was not reread; README encoding or
readability that was not explicitly read; Package or Order reserved-directory and
staging entries that were not listed; or any Supporting Material condition. Under
the friendly-filesystem assumption, uncertain or contradictory change evidence
selects rebuilding instead of speculation. Hidden direct children and ordinary
directory-name drift remain nondiagnostic even when observed.

Canonical rebuilding reads exactly the inputs fixed by the closed index decision,
validates package-wide Vendors and every Order independently plus their
cross-record relationships, creates healthy projections and Damaged Order
placeholders, derives the four lists, and attempts deterministic validated atomic
index replacement. Package-wide damage prevents replacement. Order-local damage
does not: damaged placeholders stay visible and only healthy Orders enter the
candidate index. Package README remains JIT and Supporting Material remains wholly
unexamined.

Exhaustive validation is a separate operation. It may add only the diagnostics
supported by its broader managed-artifact, exact-README, and immediate reserved
directory inspection. It neither changes initial fast-load evidence retroactively
nor turns the Package projection into a claim that every possible warning was
searched for.

### Core and filesystem-adapter boundary

The shared Rust core owns load-result variants, paths and membership, metadata and
upgrade classification, index trust/rebuild decisions, projection and totals,
trustworthy-identity rules, Vendor resolution, value grouping, diagnostics, and
all deterministic ordering. Later CLI and desktop adapters consume these types;
they do not rebuild, reinterpret, or envelope the rules in M2.

The existing filesystem port remains the only effect boundary. M2 may extend it
with the smallest portable no-follow change observation needed by the closed index
cutoff. That observation exposes entry kind and comparable content-or-replacement
evidence, or explicitly reports uncertainty; filesystem timestamps or identities
never enter persisted metadata or the public projection. Existing `read`,
`symlink_metadata`, deterministic `read_directory`, canonicalization, staging,
durability, validated replacement, and cleanup failures retain their typed mapping.

Production adapters translate platform observations conservatively. Test doubles
can supply exact directory orders, kinds, bytes, comparable/uncertain change
evidence, and failures at every existing read, list, staging, validation,
durability, replacement, and cleanup boundary. No rule depends on platform error
strings, filesystem enumeration order, Tauri, CLI parsing, or frontend state.
