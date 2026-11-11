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
There is no Vendor Merge. `vendor hide` and `vendor unhide` are reversible, including
when Orders reference the Vendor. Hiding affects selection for new use, never how an
existing Order resolves its live Vendor facts. Vendors are never deleted.
