# Packages and adoption

`spielorders init --root "$package"` initializes an ordinary directory, preserving an
existing Package README. Supply `orders_path="All Orders"` and `removed_path=Removed`
when the existing collection uses those locations. Initialization adopts no Orders.
Ordinary initialization and opening create no agent files.
Use `agent provision --root "$package"` to write the current skill and reference files.
Each run replaces earlier copies of those files, including local edits, and touches no
other file; keep local notes in files of your own. Root AGENTS.md is never read
or written by provisioning. Ask the agent to read the local skill and add a concise
pointer to AGENTS.md without discarding existing instructions. `agent defaults` reports
the installed default resources without requiring a Package.

`spielorders add-order --root "$package" title="Laser components"` creates a new
Order directory with today's local date, status `in_preparation`, and no Items or Vendor.
Supply explicit facts to override defaults. Use `package list vendors` and `package values`
to find existing shared spellings; a value the Package lacks needs `--add-values`. IDs come
from SpielOrders, not inferred names.

For an existing eligible direct child of the configured Orders Directory:

```sh
spielorders adopt-order "$directory" --root "$package" title="Laser components" date_ordered=2026-01-15 status=ordered --dry-run
```

Adoption needs explicit Title, Date Ordered, and Status. Optional Order facts and Items
may also be supplied; `item` begins each Item's assignments. Existing README and
Supporting Material stay intact, and adoption does not rename the directory.

Legacy filenames and directory components are evidence, not a field grammar. Resolve
facts from available evidence rather than automatically parsing names. Adopt one Order
at a time. To resume, inspect the current Package and skip directories already adopted;
successful prior work is represented in its current records, not a separate import log.
