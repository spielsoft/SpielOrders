# Vendors and Contacts

A Vendor requires `name` and may have `phone`, `email`, `website`, multiline `address`,
and `purchasing_notes`. A Contact requires `name`; `role`, `phone`, and `email` are
optional. Empty optional assignments clear fields. Quote multiline values as shell text.
IDs are generated and returned by SpielOrders, not supplied as ordinary facts.

```sh
spielorders add-vendor --root "$package" name="Optics Supplier" website=https://example.com
spielorders vendor show "Optics Supplier" --root "$package"
spielorders vendor edit "Optics Supplier" --root "$package" phone="+1 555 0100"
spielorders vendor contact add "Optics Supplier" --root "$package" name="Sales desk" email=sales@example.com
spielorders vendor contact add "Optics Supplier" --root "$package" name="Accounts" role=billing
spielorders vendor contact move "Optics Supplier" 1 position=2 --root "$package"
```

`vendor contact list`, `edit`, and `remove` inspect and change Contacts; positions are
one-based. Select an Order's Vendor by existing Name, not a fabricated placeholder.
`vendor hide` and `vendor unhide` are reversible, including when Orders reference the
Vendor. Hiding affects selection for new use, never how an existing Order resolves its
live Vendor facts. `vendor delete` removes a Vendor and its Contacts; while Orders
reference it, it refuses and lists them unless `--replacement NAME` re-points them to
another Vendor or `--clear` leaves them with none. `vendor merge` makes duplicates one
Vendor. Names are in precedence order: the first is kept with its facts, each later one
fills only the facts and Contacts (matched by Name) it still lacks, their Orders reference
it, and the others are deleted. Deletion and merging cannot be undone; the
response's `orders` is how many Orders changed.

```sh
spielorders add-vendor --root "$package" name="Optics Supplier Inc"
spielorders vendor merge "Optics Supplier" "Optics Supplier Inc" --root "$package" --dry-run
spielorders vendor delete "Optics Supplier Inc" --replacement "Optics Supplier" --root "$package"
```
