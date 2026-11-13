---
name: spielorders
description: Working with SpielOrders packages.
---

# SpielOrders

SpielOrders manages local purchasing records, their Items, shared Vendors, and Contacts.
A SpielOrders Root is the Package directory containing `.spielorders/config.toml`;
the configured Orders Directory holds its Order directories. Root-level Resources and
other ordinary files remain user-owned.

Use the installed `spielorders` CLI. Examples use that command name; substitute its
quoted executable path when it is not on PATH. The CLI runs with the invoking user's
ordinary filesystem permissions; an access refusal is not permission to bypass them.
`agent defaults` locates the installed reference guidance for comparison with local edits.

The command path comes first, followed by its options and facts. `--root <PATH>` selects
exactly the Package Root, the Order directory for `order` commands, or the destination
directory for `agent provision`; no ancestor is searched. The default is the current
directory. Human output is the default; commands offering `--json` return
machine-readable responses. `--help` is the live command reference.
`--dry-run`, where offered, validates and reports without writing; omitting it commits
the named change. `agent provision` writes the current guidance, replacing earlier copies of its files.
`agent defaults` reports the installed default resource locations without opening a Package.

Never edit `.spielorders` configuration or JSON directly when a CLI operation exists.
The CLI enforces validation, guarded writes, cross-record rules, and index refresh.
Package and Order READMEs are ordinary Markdown content. Diagnostics do not authorize
inventing an unsupported metadata edit.

Supporting Material is opaque user-owned content inside an Order directory, outside
managed metadata, with no required internal layout. Metadata operations preserve it;
explicit whole-Order removal moves or trashes the entire directory. Indexed search does
not make those files managed data.

Read only the reference useful for the request:

- [Packages and adoption](references/packages-and-adoption.md): initialization, paths,
  opt-in setup, existing directories, and resumable legacy adoption.
- [Orders and Items](references/orders-and-items.md): facts, dates, statuses, line items,
  editing, alignment, removal, and README operations.
- [Vendors and Contacts](references/vendors-and-contacts.md): shared facts, selection,
  hidden Vendors, and ordered Contacts.
- [Search and inspection](references/search-and-inspection.md): Package snapshots,
  reusable values, native search, and indexed files.
- [Diagnostics and repair](references/diagnostics-and-repair.md): validation, explicit
  offered repairs, and index maintenance.
