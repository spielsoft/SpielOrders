# M12 — Value management

## Outcome

Vendors, Orderers, Ordering Institutions, Projects, and each Project's Order Groups are
values the Package keeps, not text that exists only while an Order uses it. An Order field
accepts only an existing value, chosen by filtering as you type, and a new value is added
deliberately. The sidebar tables add, rename, merge, and delete values, and every Order
that uses a changed value follows it. The CLI keeps the same rule.

## Decisions

- [Desktop experience](../wayfinder/spielorders-product-architecture/tickets/007-desktop-experience.md)
- [Order record lifecycle](../wayfinder/spielorders-product-architecture/tickets/003-order-record-lifecycle.md)
- [Vendor identity and history](../wayfinder/spielorders-product-architecture/tickets/004-vendor-identity-history.md)
- [CLI contract](../wayfinder/spielorders-product-architecture/tickets/006-cli-contract.md)
- [Metadata schemas](../wayfinder/spielorders-product-architecture/tickets/010-metadata-schemas.md)
- [Effect and adapter boundaries](../../docs/architecture/boundaries.md)

## Slices

- [ ] Keep Orderers, Ordering Institutions, Projects, and Project-owned Order Groups as
  stored values beside Vendors, revising the metadata format in place; an existing Package
  gains the values its Orders use.
- [ ] Accept only existing values in Order drafts and the CLI, replacing new-value
  confirmation. The CLI refuses an unknown value by name, offers a command that adds a
  value, and offers an explicit option on Order creation, editing, and adoption that adds
  the named new values in the same guarded change.
- [ ] Add, rename, merge, and delete values through guarded library changes that report
  how many Orders change. Renaming onto an existing name becomes a merge; deleting a used
  value requires a replacement, or none where the field is optional; merging keeps one
  chosen spelling. Vendors gain the same rename, merge, and replacement deletion.
- [ ] Choose Vendor, Orderer, Ordering Institution, Project, and Order Group with one
  filtering-as-you-type chooser that accepts only existing values. When the text matches
  nothing, the list's last item reads New Orderer “text”… (or the field's kind) and adds
  the value after a confirmation; New Vendor opens the Vendor sheet with the name filled
  in. Order Group lists the chosen Project's groups. The Vendor popover is removed.
- [ ] Give each sidebar table a + bar that adds a value (in Projects, a group of the
  selected Project, otherwise a Project), in-place rename by double-click, a right-click
  Delete… sheet that asks for a replacement when the value is used, and Merge N … for a
  multi-row selection with a sheet that asks which spelling to keep. Each sheet states how
  many Orders change.
- [ ] Settle partner findings, run the gate and Desktop acceptance, and present the app for
  the owner's live use and sign-off.

## Acceptance

- Typing in an Order's Orderer field narrows its list to matching existing values; text
  that matches nothing cannot be committed, and the New Orderer item adds it only after
  confirmation. The same holds for Institution, Project, Order Group, and Vendor.
- Order Group offers only the current Project's groups, and a group added from the field
  belongs to that Project.
- Adding a value from a sidebar table makes it available in every chooser without any
  Order using it.
- Renaming a value changes it in every Order that uses it; renaming onto an existing value
  offers a merge instead.
- Merging AbC and Ab_C keeps the chosen spelling, every affected Order uses it, and the
  other value is gone. The sheet's count matches the Orders that changed.
- Deleting an unused value removes it; deleting a used value requires a replacement (or
  none where allowed), and every affected Order uses the replacement.
- The CLI refuses an unknown value by name, and succeeds when the value is added first or
  added in the same command.
- An existing Package opens with its values present and no Order changed.
- The repository check and gate pass; partner review reports what it checked and what
  remains unverified; the owner signs off.

## Exclusions

Undo of bulk value changes, which confirm with an Order count instead. Structured filters,
App Store work, and the public CLI installer remain later. No production archive access or
synthetic load.
