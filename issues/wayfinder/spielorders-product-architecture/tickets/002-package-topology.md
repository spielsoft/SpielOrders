---
title: Define the SpielOrders package topology
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Decision

A **SpielOrder Package** is rooted at an otherwise ordinary user-chosen directory.
SpielOrders manages only the root `.spielorders/` directory, the configured Orders
Directory, and the explicitly recognized README files described below; other root content
remains unrestricted user space.

The package topology is:

```text
<SpielOrders Root>/
  README.md
  .spielorders/
    config.toml
    version.json
    index.json
    vendors.json
  <orders_path>/
    YYYY_MM_DD[_Vendor]_Title[_Orderer]/
      README.md
      .spielorders/
        order.json
      ...Supporting Material...
```

`contracts/schemas/v1` and `contracts/examples/v1` are the record of what these documents
hold, and the core crate's public surface is the record of how a Package is opened,
discovered, and aligned. What follows is what that code may never become.

### Root contract

- `.spielorders/config.toml` is required, user-editable, and deliberately unversioned.
  Its optional `orders_path` value must name a nonempty relative descendant of the
  SpielOrders Root and defaults to `Orders`. `.` is never valid: a Root that is its own
  Orders Directory has no unrestricted user space left, its README becomes a disallowed
  direct-child file, `removed_path` overlaps the Orders Directory, and every unrelated
  root directory becomes a permanently damaged presumed Order.
- `.spielorders/version.json` is the required canonical package-wide metadata-version
  marker. It is updated last during a resumable Metadata Upgrade; each other managed JSON
  repeats its version so interrupted work can be recognized and resumed.
- `.spielorders/vendors.json` is the required, versioned, canonical aggregate of shared
  Vendor records.
- `.spielorders/index.json` is required and versioned, but derived and rebuildable. It
  carries enough of the healthy Order, Order README, and Vendor projection to load the
  eventual top-level GUI without reading every `order.json`, and it never becomes a
  second canonical source of domain facts.
- Root `README.md` follows the SpielGantt convention: initialization creates it and the
  UI always presents it for editing, but a missing file loads as empty and does not
  damage the package. Its contents are entirely user-authored; SpielOrders never writes a
  generated index or any other maintained text into it.
- A directory with no `.spielorders/` at all is unmanaged, and SpielOrders offers
  initialization. A `.spielorders/` missing or malforming a required canonical root
  artifact makes the package damaged: SpielOrders reports the problem and offers
  diagnosis, and never silently initializes over it. A missing, malformed, stale, or
  wrong-version `index.json` is instead rebuilt, because it is wholly derived.
- There is no special non-Order resources directory or resource-path configuration.
  Everything outside `.spielorders/` and the configured Orders Directory remains ordinary
  user space.

### Order discovery and ownership

- Every non-hidden ordinary direct-child directory of the Orders Directory is presumed to
  be an Order Directory. Discovery is not recursive, and filesystem links are never
  followed.
- A direct child whose basename starts with `.` is ignored, whether it is a file,
  directory, or link. Any other direct-child entry that is not an ordinary directory is
  disallowed and reported as a warning.
- Every Order Directory requires a versioned `.spielorders/order.json`, the canonical
  structured Order record. A missing or malformed file makes that Order damaged.
- The Order Directory's `.spielorders/` path is wholly reserved for SpielOrders-owned
  artifacts. Unexpected content there receives an integrity diagnostic.
- Order `README.md` follows the same convention as the root README: creation writes it,
  absence loads as empty, and its contents are entirely user-authored.
- Everything else within an Order Directory is recursively opaque **Supporting
  Material**. SpielOrders core does not interpret or reorganize it; complementary OS
  services may search it and reveal it in Finder.

### Directory naming and alignment

- The aligned basename has the form `YYYY_MM_DD[_Vendor]_Title[_Orderer]`, omitting the
  components whose facts are absent. Normalization preserves letter case and Unicode
  characters.
- A newly created Order Directory is aligned automatically, provided the resulting
  basename is unique.
- Editing Date Ordered, Vendor, Order Title, or Orderer never renames a directory; a
  mismatch is ordinary allowed state and produces no integrity diagnostic. Transitioning
  an Order to `Ordered` sets Date Ordered to the current date, and the directory still
  changes only when the user invokes the explicit **Align Directory Name** action.
- Alignment renames the directory to the current projection. If that basename already
  exists, alignment stops and reports the collision rather than inventing a numeric
  suffix.
- Directory-name alignment is an optional organization aid, not a validity condition.
  Validation and repair never warn about or rewrite that drift.

Order fields, Vendor identity, the index projection, version evolution, lifecycle beyond
the naming interaction, and repair mechanics belong to their own decisions.
