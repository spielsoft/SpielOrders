---
title: Define the SpielOrders package topology
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:grilling
assignee: /root
---

## Question

What root directory, child directories, and SpielOrders-owned files constitute the data layout for Order Directories, shared Vendor information, and optional non-Order resources; which Supporting Material remains opaque; and what invariant relates an Order title to its directory name?

## Blocking

Unblocked.

## Resolution

A **SpielOrder Package** is rooted at an otherwise ordinary user-chosen directory. SpielOrders manages only the root `.spielorders/` directory, the configured Orders Directory, and the explicitly recognized README files described below; other root content remains unrestricted user space.

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

### Root contract

- `.spielorders/config.toml` is required, user-editable, and deliberately unversioned. Its optional `orders_path` value must be a relative descendant of the SpielOrders Root and defaults to `Orders`. A value of `.` is **not** valid, superseded by [Define the Rust CLI capability contract](006-cli-contract.md): `orders_path` must name a nonempty relative descendant of the SpielOrders Root. Making the Root its own Orders Directory contradicts this document — the Package README becomes a disallowed direct-child file, `removed_path` overlaps the Orders Directory, and every unrelated root directory becomes a presumed Order that is permanently damaged, so the "unrestricted user space" this section grants would have nowhere left to exist.
- `.spielorders/version.json` is the required canonical package-wide metadata-version marker. It is updated last during a resumable Metadata Upgrade; each other managed JSON repeats its version so interrupted work can be recognized and resumed.
- `.spielorders/vendors.json` is the required, versioned, canonical aggregate of shared Vendor records.
- `.spielorders/index.json` is required and versioned, but derived and rebuildable. It contains the complete healthy Order, Order README, and Vendor projection needed to load the eventual top-level GUI without reading every `order.json`; it never becomes a second canonical source of domain facts.
- Root `README.md` follows the SpielGantt convention: normal initialization creates it and the UI always presents it for editing, but a missing file loads as empty and does not damage the package. Its contents are entirely user-authored and contain no generated index or other programmatically maintained text.
- If `.spielorders/` is wholly absent when a directory is opened, SpielOrders treats the directory as unmanaged and offers initialization. If `.spielorders/` exists but a required canonical root artifact is missing or malformed, SpielOrders treats the package as damaged, reports the problem, and offers diagnosis rather than silently initializing it. A missing, malformed, stale, or wrong-version `index.json` is instead rebuilt because it is wholly derived.
- There is no special non-Order resources directory or resource-path configuration. Everything outside `.spielorders/` and the configured Orders Directory remains ordinary user space.

### Order discovery and ownership

- Every non-hidden ordinary direct-child directory of the Orders Directory is presumed to be an Order Directory. Discovery is not recursive.
- A direct child whose basename starts with `.` is ignored, regardless of whether it is a file, directory, or link. This covers macOS and tool artifacts such as `.DS_Store` and `._...` files.
- Any other direct-child entry that is not an ordinary directory—including a file, alias, or symlink—is disallowed and reported as a warning. Filesystem links are never followed during discovery.
- Every Order Directory requires a versioned `.spielorders/order.json`, which is the canonical structured Order record. A missing or malformed file makes that Order damaged.
- The Order Directory's `.spielorders/` path is wholly reserved for SpielOrders-owned artifacts. Unexpected content there receives an integrity diagnostic.
- Order `README.md` also follows the SpielGantt convention: normal Order creation creates it and the UI always presents it for editing, but absence loads as empty. Its contents are entirely user-authored and contain no programmatically determined content.
- Everything else within an Order Directory is recursively opaque **Supporting Material**. SpielOrders core does not interpret or reorganize it; complementary OS services may search it and reveal it in Finder.

### Directory naming and alignment

- The aligned basename has the form `YYYY_MM_DD[_Vendor]_Title[_Orderer]`. `YYYY_MM_DD` comes from **Date Ordered**; the Vendor component is omitted when no Vendor is assigned; `Title` comes from **Order Title**; and the final component is the optional free-text **Orderer** value exactly as entered before filename normalization.
- For each textual component, surrounding whitespace is trimmed, whitespace runs become `_`, existing underscores are preserved, repeated underscores collapse, and filesystem-forbidden characters are removed or replaced with `_`. Letter case and Unicode characters are preserved.
- Creating an Order requires Date Ordered and Order Title. Date Ordered defaults to the user's current local calendar date and may initially be provisional; Vendor may be absent. A newly created Order Directory is aligned automatically, provided the resulting basename is unique. Editing Date Ordered, Vendor, Order Title, or Orderer does not rename it automatically; a mismatch is ordinary allowed state and produces no integrity diagnostic.
- Transitioning an Order to `Ordered` sets Date Ordered to the current date. The directory still changes only when the user invokes the explicit **Align Directory Name** action or shortcut.
- Alignment renames the directory to the current projection. If that basename already exists, alignment stops and reports the collision rather than inventing a numeric suffix.
- Directory-name alignment is an optional organization aid, not a validity condition. SpielOrders may make the explicit **Align Directory Name** action available when the current basename differs from the projection, but validation and repair do not warn about or rewrite that drift.

The exact Order fields, Vendor schema and identity rules, index projection, version evolution, lifecycle semantics beyond the naming interaction, and repair mechanics are defined by their dedicated downstream decisions.
