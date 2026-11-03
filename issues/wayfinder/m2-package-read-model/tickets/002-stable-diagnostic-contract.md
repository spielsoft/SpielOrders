---
title: Define the stable M2 diagnostic vocabulary and evidence contract
status: closed
closed_at: 2026-08-13
parent: Chart the M2 Package read model
labels:
  - wayfinder:grilling
assignee: /root
---

## Decision

The vocabulary is built. The core crate's public surface is the record of every code,
its severity, its default scope, and the evidence it requires; what follows is what
that vocabulary may never become.

### MVP boundary and compatibility

M2 provides the smallest stable evidence contract needed by the already-decided
Package read paths. It does not build a speculative recovery framework for failures
that have not been observed. Diagnostics describe canonical damage or a usable
warning; Package Index assessment and refresh are separate typed state; and a
failed attempted operation is a separate typed operation failure. None is persisted
in metadata version 1.

Diagnostic codes are compositional condition codes. Typed evidence supplies the
artifact, path, identity, field, or filesystem phase instead of multiplying one
code per file kind. Code spelling, meaning, severity, default scope, and required
evidence are compatibility promises to M2 adapters and tests. Adding a new code or
a new optional evidence fact is compatible. From the release the roadmap names on,
renaming, removing, repurposing, or changing the severity, scope, or required
evidence of an existing code is an incompatible change to every output contract
that exposes it. Human wording,
source-library messages, and formatting are not stable contracts.

This contract deliberately stops at observed MVP needs. Suggested repairs,
ephemeral repair-case handles, commands, GUI presentation, CLI JSON envelopes,
platform error taxonomies, and speculative recovery states remain outside M2.

### Separate typed concepts

M2 uses one shared evidence-scope vocabulary with the narrowest applicable kind:

- `package`: package-wide canonical state or usable Package structure;
- `order`: one discovered Order Directory, identified by its path even when its
  canonical identity cannot be trusted;
- `document`: one exact Package or Order README;
- `derived_index`: Package Index assessment and refresh only; and
- `operation`: one attempted operation that did not produce its required result.

Only Package, Order, and document scopes carry diagnostics. Derived-index and
operation scopes use their separate typed records. This prevents a malformed or
stale derived index, or a failed attempt to replace it, from becoming canonical
damage by vocabulary alone.

Diagnostics have exactly two severities:

- `error` means canonical facts at the diagnostic's scope cannot be trusted. A
  Package-scoped error produces Damaged Package diagnostic-only use. An
  Order-scoped error produces a Damaged Order placeholder while healthy Orders
  remain ordinarily usable. M2 has no document-scoped error.
- `warning` never blocks ordinary Package or Order use. A document warning may
  make only that exact README unavailable for reading or editing. Structural
  warnings leave the observed content untouched.

There is no informational diagnostic severity. Healthy state, index state, refresh
success, and absence that is defined as normal are ordinary typed facts, not
diagnostics.

### Common evidence envelope

Every diagnostic has `code`, `severity`, `scope`, `primary_location`,
`related_locations`, and one typed `evidence` variant. Every operation failure has
`code`, operation scope, the path it failed on, `related_locations`, and typed
operation evidence. Collections are present even when empty.

A failure's path is a `primary_location` wherever this application can express it as
one, and the caller's own text where the failure was the resolution itself;
`primary_location` is absent in exactly that case, and nowhere else. Resolving a
caller-supplied path is an operation that can fail before there is anything to locate,
and a path that cannot be resolved cannot be stated as an absolute location — so it is
never asserted as one. Every diagnostic still carries a `primary_location`, because a
diagnostic describes something already located.

Each location carries an absolute path and, when an established SpielOrders Root
contains that path, its normalized Package-relative path. The absolute path names
the exact observed or attempted location and does not imply that the entry exists
or was canonicalized by following links. Package-relative path is absent when
Package membership has not been established, and when the entry has no normalized
Package-relative spelling at all; it is never fabricated, for an outside path or
for one this application cannot express. Missing artifacts still name their exact
expected locations.

Identity evidence is present only after the value has decoded as a valid
metadata-version-1 identity:

- a trustworthy Order ID is included for an otherwise decoded Order, including
  semantic failures and duplicate-ID evidence;
- a malformed or missing Order identity is absent, and the Order remains scoped by
  its directory path;
- trustworthy Vendor IDs are included when the decoded Vendor aggregate makes
  them available;
- an Unresolved Vendor Reference includes both the trustworthy owning Order ID and
  the referenced Vendor ID; and
- a duplicate value remains trustworthy as observed evidence but is not treated as
  a unique lookup identity.

Field locations use stable metadata field paths only when the decoder or semantic
validator can identify one. They are absent for encoding and general syntax
failures. Raw parser text and platform error text may be retained as optional
non-contractual detail, but grouping, ordering, deduplication, tests, and adapter
behavior never depend on it.

### Stable diagnostic codes

The stable diagnostic vocabulary is a closed set of compositional condition codes,
recorded in the core crate beside the typed evidence each one requires. The rules
below govern what belongs in it.

`orders_directory_entry_name_unusable` reports a direct Orders Directory child
whose filesystem name has no normalized Package-relative spelling. A name
containing `\` is the reachable Unix case. `configured_orders_directory_name_unusable`
reports a configured Orders path that resolves through a symbolic link to such a
real name. These locations carry absolute paths and no fabricated Package-relative
paths. The existing `configured_orders_directory_unavailable` reasons remain
`not_directory`, `outside_package`, and `reserved_metadata`.

`managed_artifact_invalid_shape` covers requiredness, unknown fields, wrong types,
invalid enumerations, malformed UUID shape, invalid currency shape, invalid local
path shape, and other strict version-1 decode constraints. It does not hide the
cross-record and identity conditions that have their own codes. Configuration
encoding, syntax, and shape failures use the corresponding `managed_artifact_*`
code with artifact kind `config`; its path-resolution semantics use the explicit
configuration codes.

The aggregate `vendors.json` failures are Package-scoped. Each duplicate Order ID
produces one `order_id_duplicate` diagnostic in every affected Order scope. Each
copy contains the same complete conflict set, ordered by Package-relative Order
Directory path. This intentional per-Order evidence is not deduplicated across
scopes because every Damaged Order placeholder must explain itself.

### Conditions that do not diagnose

The following produce no diagnostic:

- a wholly absent root `.spielorders` directory, which means unmanaged;
- an absent Package or Order README, which reads as empty;
- hidden direct children of the Orders Directory;
- Order Directory name drift;
- future or provisional dates and other unusual but contract-valid Order facts;
- absent optional facts, zero Items, and finite zero or negative Item values;
- any Supporting Material fact, because Supporting Material is never inspected;
  and
- a missing, malformed, wrong-version, or stale Package Index that is successfully
  rebuilt.

Unexpected reserved content and recognizable staging artifacts are preserved and
warned about only when observed within an authorized reserved-directory inspection.
If such an entry occupies a required canonical path, the applicable canonical
error is emitted as well. A staging candidate never becomes canonical evidence and
is never promoted in M2.

### Package Index assessment and refresh evidence

Package Index state remains exactly `current`, `missing`, `malformed`,
`wrong_version`, `stale`, or `not_assessed`. Refresh outcome remains exactly
`not_needed`, `rebuilt`, `not_attempted`, or `replacement_failed`. These are not
diagnostic codes or severities and are never persisted in version-1 index content.

When reasons are requested, they are stable, typed derived-index evidence:

- `malformed`: `wrong_kind`, `unreadable`, `invalid_encoding`, `invalid_syntax`,
  `invalid_shape`, `invalid_semantics`, or `non_deterministic_serialization`;
- `wrong_version`: recognizable observed index version plus the Package/running
  version when known; and
- `stale`: `order_membership_changed`, `order_path_mismatch`,
  `managed_input_wrong_kind`, `vendors_changed`, `order_metadata_changed`,
  `order_readme_changed`, or `change_evidence_uncertain`.

Reasons are deduplicated and emitted in the order written above. `current`,
`missing`, and `not_assessed` need no invented reason. A successful rebuild reports
the decided refresh outcome and current post-refresh state without retaining a
warning merely because automatic rebuilding was needed.

### Operation failures and index replacement

The general stable operation-failure code is `filesystem_operation_failed`. Its
evidence requires the attempted filesystem operation and the exact primary and
related locations; the core crate spells the operations it can report. Platform or
test-double error detail remains optional and non-contractual. A persistent failure
to read a required canonical artifact is instead represented by the applicable
canonical diagnostic; the same event is not duplicated as an operation failure.

A required Package Index replacement failure uses the more specific stable code
`package_index_replacement_failed`. It is an operation-scoped blocking result
carrying the absolute and Package-relative `index.json` location, the pre-refresh
index assessment, refresh outcome `replacement_failed`, the phase that failed, and a
cleanup failure as related operation evidence when one also occurred. The phases the
core spells are exactly the ones it can reach: a public, spelled failure code that
nothing can emit is worse than no code, so a phase that becomes unreachable is
removed rather than retained.

It blocks Package opening, leaves the old index bytes and original assessment
visible, and does not emit or imply Damaged Package, Damaged Order, or canonical
metadata diagnostics. Canonical facts remain whatever the independent canonical
evidence says. Adapters may offer Retry or Quit later; M2 does not prescribe that
presentation or a retry system.

### Deterministic grouping, deduplication, and ordering

Diagnostics are grouped and emitted as follows:

1. Package scope, then the Package README document scope;
2. Order scopes by bytewise lexical Package-relative Order Directory path, with
   that Order's document scope immediately after its Order diagnostics.

Within one scope, errors precede warnings, then records sort by stable code,
primary Package-relative path (absolute path only when relative is absent), stable
field path, and normalized typed-evidence key. Related locations and identity
collections use deterministic lexical path or canonical metadata order as defined
by their evidence variant.

An exact duplicate has the same code, severity, scope, primary location, stable
field, identities, and typed evidence and appears once. Source messages and
filesystem enumeration order never affect equality or order. The deliberately
separate per-Order duplicate-ID records are not exact duplicates because their
Order scopes differ.

Derived-index evidence is ordered within its own typed record rather than inserted
into diagnostic groups. A failed operation is returned as the operation result,
not appended as though the Package had diagnosed itself.

### Read-path visibility

- A current-index fast path reads `config.toml`, `version.json`, and `index.json`
  and lists direct Order candidates. It may return diagnostics supported by those
  exact reads and the direct-child observations. It does not diagnose latent
  `vendors.json`, `order.json`, README, reserved-content, or staging conditions.
- Canonical rebuilding reads the canonical inputs and exact Order READMEs required
  by the closed index decision. It returns canonical and cross-record diagnostics
  plus Order README warnings supported by those reads. It need not list reserved
  directories merely to search for unexpected content.
- An explicit Package or Order README read observes only that exact path. Missing
  returns empty; wrong-kind, unreadable, or non-UTF-8 content returns
  `readme_unavailable` without damaging its owner.
- Exhaustive validation reads every governed canonical artifact and exact README,
  validates all cross-record rules, and lists only Package- and immediate
  Order-reserved `.spielorders` directories to diagnose unexpected or staging
  content. It never lists or recurses through general Order Directory contents and
  never inspects Supporting Material.

Different operations may therefore return different complete evidence sets for
what they actually examined. M2 does not pretend that a fast path performed
exhaustive validation, and it does not force latent README warnings into ordinary
initial loading.

### Core, filesystem adapter, and glossary boundary

The shared Rust core owns stable codes, severity, scope, trusted-identity rules,
typed evidence, deduplication, ordering, index assessment reasons, and the mapping
from existing M1 metadata/path/replacement failures into this contract. The
filesystem adapter supplies bytes, no-follow kinds, deterministic directory
observations, portable change evidence required by the closed index decision, and
opaque source errors. Test doubles can supply exact observations and inject every
existing read, staging, durability, replacement, and cleanup failure without an OS
filesystem.

No M2 rule depends on platform error strings, filesystem enumeration order,
timestamps embedded in diagnostics, CLI behavior, or adapter-specific recovery.
The existing core-to-port dependency direction remains unchanged.

No term enters `CONTEXT.md`. Diagnostic, severity, scope, and operation failure are
general technical vocabulary; Package Index state and refresh outcome are read-side
contract terms rather than new domain concepts. The existing Damaged Package,
Damaged Order, Package Index, Order ID, Vendor ID, Vendor Reference, and README
glossary terms remain sufficient.
