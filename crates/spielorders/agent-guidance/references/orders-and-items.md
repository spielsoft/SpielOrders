# Orders and Items

Order facts are `title`, `date_ordered`, `status`, and optional `vendor`, `orderer`,
`ordering_institution`, `currency`, `project`, and `order_group`. Dates use `YYYY-MM-DD`;
currency is an uppercase currency code. Status is `in_preparation`, `hold`, `ordered`,
`received`, or `complete`. Empty optional assignments clear a value.
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
Catalog Number is optional. `order item list` shows the ordered collection.
Order edits may require explicit introduction of a new reusable value; follow the
command's confirmation assignment syntax reported by its help and refusal.

`order readme` reads or writes the README as its help describes; `order validate`
checks the scoped Order. `order align` explicitly aligns its directory name with its
facts. It is not a side effect of editing or adoption and can break external links.
`order remove` moves the complete directory into the Package's configured recovery
directory. `order remove --delete` additionally requests native Trash; a failed handoff
leaves the recoverable directory named in the result. There is no Item-level Trash.
