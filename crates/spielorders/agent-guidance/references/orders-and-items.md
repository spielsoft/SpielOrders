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
Assigning `orderer`, `ordering_institution`, `project`, or `order_group` reuses an
existing value that matches case-insensitively, keeping its stored spelling, and otherwise
introduces the new value; the response reports each as `reused` or `new`. An Order Group
belongs to its Project: it is reused or introduced among that Project's groups, so the
same name under two Projects is two groups. Changing an Order's Project clears its group
unless the new Project already has a group of that name or the same command assigns one.

`order readme` reads or writes the README as its help describes; `order validate`
checks the scoped Order. `order align` explicitly aligns its directory name with its
facts. It is not a side effect of editing or adoption and can break external links.
`order remove` moves the complete directory into the Package's configured recovery
directory. `order remove --delete` additionally requests native Trash; a failed handoff
leaves the recoverable directory named in the result. There is no Item-level Trash.
