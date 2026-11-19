# Orders and Items

Order facts are `title`, `date_ordered`, `status`, and optional `vendor`, `orderer`,
`ordering_institution`, `currency`, `project`, and `order_group`. Dates use `YYYY-MM-DD`;
currency is an uppercase currency code. Status is `in_preparation`, `hold`, `ordered`,
`received`, or `complete`. Empty optional assignments clear a value. An `order edit`
that changes status to `ordered` without `date_ordered` sets today's date; supply the
known `date_ordered` in the same command to keep a historical date.
SpielOrders calculates totals; never supply them as facts.

```sh
spielorders order show --root "$directory"
spielorders order edit --root "$directory" title="Updated title" vendor="Known Vendor"
spielorders order item add --root "$directory" description="Optical mount" quantity=2 unit_price=12.50 catalog_number=OM
spielorders order item edit 1 --root "$directory" quantity=3
spielorders order item move 1 position=2 --root "$directory"
spielorders order item remove 1 --root "$directory"
```

Item positions are one-based. Description, Quantity, and Unit Price are required;
Catalog Number and `notes` are optional. `notes` holds multiline remarks tied to that one
Item, such as a specification, quote reference, or permitted substitution; keep remarks
about the whole Order in its README. `order item list` shows the ordered collection.
Assigning `orderer`, `ordering_institution`, `project`, or `order_group` accepts only a
value the Package has, matched case-insensitively and stored in its existing spelling. A
value the Package lacks is refused by name; add it first, or pass `--add-values` to
`add-order`, `adopt-order`, or `order edit` to add every such value in the same change. The
response reports each value as `reused` or `new`. An Order Group belongs to its Project: it
is chosen among that Project's groups, so the same name under two Projects is two groups. Changing an Order's Project clears its group
unless the new Project already has a group of that name or the same command assigns one.

`value add`, `rename`, `merge`, and `delete` change a kind's values; every Order using a
changed value follows it, and the response's `orders` is how many Orders changed. An
`order_group` command takes `--project` for the Project the group belongs to. Renaming
onto an existing value merges the two; `value merge` keeps the first spelling named. Deleting a used value needs `--replacement VALUE`
or `--clear`. A Project's groups go with its Orders.

```sh
spielorders value add orderer "Emmy Noether" --root "$package"
spielorders value add orderer "E. Noether" --root "$package"
spielorders value merge orderer "Emmy Noether" "E. Noether" --root "$package" --dry-run
spielorders value delete orderer "E. Noether" --replacement "Emmy Noether" --root "$package"
spielorders value add project "Clock" --root "$package"
spielorders value rename project "Clock" "Telescope" --root "$package"
```

`order readme` prints the path of the Order's README, to read or edit with ordinary
tools; `order validate` checks the scoped Order. `order align` explicitly aligns its directory name with its
facts. It is not a side effect of editing or adoption and can break external links.
`order remove` moves the complete directory into the Package's configured recovery
directory. `order remove --delete` additionally requests native Trash; a failed handoff
leaves the recoverable directory named in the result. There is no Item-level Trash.

Many edits go faster as one batch:

```sh
printf '%s\n' "[\"order\", \"edit\", \"currency=USD\", \"--root\", \"$directory\"]" | spielorders apply - --root "$package" --dry-run
```

`order merge` merges other Orders into the one `--root` names, in precedence order: the
kept Order keeps its facts and gains only the facts, Items, and directory entries it
lacks, and what remains of each other Order goes to the recovery directory. An Order
that brings Items priced in another currency is refused (`currency_differs`); no currency
counts as a currency of its own, so set the missing one first. An Order with no Items
takes the currency of the first Order that brings some. It asks before discarding each
entry the kept Order already holds; pass `--yes` when no one is there to answer, after
reviewing `--dry-run`.

```sh
spielorders add-order --root "$package" title="Duplicate order"
spielorders order merge "$package"/Orders/*_Duplicate_order --root "$directory" --dry-run
```
