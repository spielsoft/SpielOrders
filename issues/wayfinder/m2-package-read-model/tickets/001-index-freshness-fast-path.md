---
title: Decide how Package Index freshness is proven without canonical rereads
status: closed
closed_at: 2026-08-13
parent: Chart the M2 Package read model
labels:
  - wayfinder:grilling
assignee: /root
---

## Decision

The read model is built. The core crate's public surface is the record of the index
states, refresh outcomes, and load results named below; what follows is what that core
may never become.

### Trust boundary and version-1 constraint

For MVP, a Package Index is trusted through a conservative filesystem change
cutoff. SpielOrders assumes a friendly workspace: users and ordinary tools may
edit or replace managed files outside SpielOrders, and those changes must be
detected, but the filesystem's no-follow change evidence may be trusted. The MVP
does not claim tamper-proof detection when a person deliberately forges both valid
derived bytes and filesystem bookkeeping. A stronger revision or content-witness
mechanism may be reconsidered after MVP.

This decision preserves metadata version 1. No timestamp, filesystem identity,
diagnostic, revision token, or search field is added to `index.json`, and no
sidecar metadata is introduced. In particular, the required `readme` projection
remains in the version-1 index for compatibility; from the release the roadmap names on,
removing it is a metadata-version change, and it is deferred until after MVP.

### Current-index fast path

Package opening reads and validates the small `.spielorders/config.toml`,
`.spielorders/version.json`, and `.spielorders/index.json` artifacts. It resolves
the configured Orders Directory, lists only its direct children, and obtains
portable no-follow change observations for `.spielorders/vendors.json`, every
non-hidden ordinary direct-child Order candidate's exact
`.spielorders/order.json`, and its exact conventional `README.md`. It does not
open `vendors.json` or every `order.json` on this path.

An existing index is **current** and may supply the Vendor and healthy Order
projections without canonical rereads only when all of the following hold:

- the index is an ordinary readable file whose version-1 shape, embedded domain
  semantics, cross-record relationships, required sort order, and deterministic
  serialization are valid;
- its schema version agrees with the current Package marker;
- its indexed Order paths match the current configured Orders Directory and the
  complete set of non-hidden ordinary direct-child Order candidates exactly;
- `vendors.json` and every represented `order.json` remain ordinary files; and
- the index replacement time is strictly later than every projected canonical
  file's latest ordinary content-or-replacement change and every present
  conventional Order README's latest ordinary change.

Equal, unavailable, incomparable, contradictory, or newer input evidence never
proves freshness; it selects the canonical slow path. This conservatism may cost a
small startup delay on a filesystem with coarse or unavailable timestamps, which
is acceptable for MVP. Adding, removing, renaming, replacing, or ordinarily
editing an Order, `order.json`, Order README, or `vendors.json` therefore prevents
fast-path trust on the next Package open.

The permitted fast-path filesystem work is confined to Package-owned metadata,
the Orders Directory's direct-child discovery boundary, and exact conventional
README paths. It may inspect existence, no-follow kind, and the portable change
facts needed by the cutoff. It never lists an Order Directory's general contents,
recurses beneath one, or inspects, inventories, validates, or rewrites Supporting
Material.

The version-1 README text remains a cache. Conventional README access is JIT: an
operation that actually requests one document may read that exact README and
report its document warning then rather than forcing all README contents to be
read during Package opening. Missing remains empty. A rebuild necessarily reads
each exact Order README because version 1 requires the projection; unreadable or
non-UTF-8 content contributes empty projected text and a document warning without
damaging or excluding the otherwise healthy Order. Latent README warnings need
not all appear in the initial fast-path Package projection.

### Missing, malformed, wrong-version, and stale indexes

The core classifies an absent index as **missing**; unreadable, invalidly encoded,
syntactically invalid, structurally invalid, semantically invalid, or
non-deterministically encoded index content as **malformed**; and a recognizable
index schema version that disagrees with the Package marker or running contract as
**wrong version**. An otherwise valid index is **stale** when directory membership,
paths, file kinds, or the conservative change cutoff do not prove that its
projection still represents the observed inputs.

Missing, malformed, wrong-version, and stale are derived-state assessments, not
canonical Package or Order damage. Each normally selects the same deterministic
canonical rebuild. A valid index manually forged to contain stale facts while
also carrying forged trustworthy-newer filesystem evidence is outside the friendly
workspace guarantee.

### Deterministic canonical rebuild

A rebuild uses only:

- validated current `version.json`, `config.toml`, and `vendors.json`;
- the lexically ordered non-hidden ordinary direct-child Order candidates;
- each candidate's exact `.spielorders/order.json`; and
- each candidate's exact conventional `README.md`, with absence projected as
  empty and unreadable content handled as the document warning described above.

The core applies all existing local and cross-record validation before projecting
the healthy subset. Healthy Order entries use the already-decided Date Ordered
descending, Order Title ascending, then Order ID order. Vendors retain canonical
`vendors.json` order. The candidate `IndexDocument` is serialized with the existing
metadata-version-1 deterministic writer: fixed field order, two-space JSON, and
one trailing newline. Current time, filesystem metadata, diagnostics, search
tokens, and Supporting Material never enter the output bytes.

The candidate is validated and atomically replaces `.spielorders/index.json`
through the established managed-file replacement boundary. A successful rebuild
leaves Package Index state `current` and records refresh outcome `rebuilt`.

### Canonical damage and replacement failure

If package-wide canonical state cannot be trusted—including invalid
`config.toml`, `version.json`, `vendors.json`, or configured Orders Directory—the
core has no trustworthy rebuild input. It does not replace the index, reports a
Damaged Package in diagnostic-only mode, and reports the index as `not_assessed`
with refresh `not_attempted` where appropriate.

Order-local damage does not prevent a healthy-only rebuild. Every Damaged Order is
excluded from the index and retained as a diagnostic placeholder. Because version
1 persists neither damaged placeholders nor diagnostics, any discovered Order
candidate absent from the index forces the canonical slow path on subsequent
opens until that Order is repaired or removed from active discovery. No sidecar is
added merely to accelerate this uncommon state.

Failure to persist a required rebuilt index is a blocking **Package Index
Replacement Failure**. It is distinct from Damaged Package because canonical facts
may remain healthy, but ordinary Package opening must not continue. The core
returns the typed filesystem failure and affected index path so an adapter may
offer Retry or Quit; this decision does not prescribe that presentation. The old
index bytes remain whatever they were, its original assessment remains visible,
and refresh outcome is `replacement_failed`.

### Stable state vocabulary and core boundary

Diagnostics and projections keep Package Index state and refresh outcome as two
separate typed concepts. Neither is ever collapsed into the other, and neither is
canonical damage.

The shared Rust core owns cutoff comparison, directory-membership proof, state
classification, deterministic projection, validation, and the decision to replace.
The existing filesystem port is extended only as needed to expose portable
no-follow change observations and the already-owned filesystem effects. A
platform adapter supplies comparable trusted observations or conservatively
requests the slow path; test doubles supply exact observations and injected read
or replacement failures. CLI, Tauri, and frontend layers do not duplicate these
rules.
