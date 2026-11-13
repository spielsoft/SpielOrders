# First order collection conversion

## Source and test boundary

Choose the source archive explicitly with the owner. Conversion testing operates only
on an isolated disposable copy; the source remains unchanged. Copy physical files rather
than hard-linking them, preserve symbolic links without following them, and verify that
any link used during testing resolves inside the copy. Never traverse links back to the
source or another external location.

Test-only metadata is not a proposed production conversion and must never be copied
back to the source. If a retained test copy already contains fabricated metadata, make a
further copy and separate those test artifacts from copied user content before starting
a first-use rehearsal. Do not treat test facts as adoption evidence.

## Existing organization and initialization

Inspect the copied archive's instructions and structure to identify its physical Order
collection. Alias views of that collection are not additional Orders to adopt.
Resources, summary documents, maintenance scripts, and other root-level content remain
ordinary user-owned space outside the configured Orders Directory.

Choose the archive root as the Package Root and configure the relative Orders Directory
to match its physical collection. For example, a collection named `Purchases` needs
that explicit configuration rather than the conventional default. Open the archive
root, not its collection subdirectory or an alias view. Initialization does not adopt
existing folders: unmanaged candidates remain Damaged Order placeholders, not healthy
empty Orders, and do not participate in search.

Preserve relative aliases and root-level material. Copied instructions or scripts can
still contain absolute paths back to the source; use explicit copy roots and do not run
copied archive-maintenance automation during application smoke testing. Do not rebuild
hand-maintained summary documents.

A broken alias and a similarly named physical directory are not proof of the same
identity. Obtain evidence before proposing any correction. Alias views outside the
configured Orders Directory are not validated by successful Package validation.

## Adoption evidence and owner review

A directory basename or summary row is candidate evidence, not automatic confirmation
of Date Ordered, Order Title, or shared Vendor identity. Names may contain inferred
dates, abbreviated vendors, ambiguous separators, or invalid calendar dates. Review
transaction evidence before accepting facts; preserve uncertainty rather than inventing
Status, Orderer, Items, prices, or currency.

Use supported SpielOrders operations after reviewing the evidence. Do not hand-write
managed metadata or align directory names as a side effect of adoption. Existing aliases
and summary hyperlinks may depend on those names; confirm any intended rename
separately. Keep supporting files opaque to application discovery rather than converting
their contents into canonical facts through undocumented bulk inference.

The [adoption decision](../../issues/wayfinder/spielorders-product-architecture/tickets/009-legacy-adoption.md)
defines the consolidated proposal and explicit approval required before adoption writes.

## Verification

Record the selected initialization configuration, adoption inputs and evidence,
unresolved facts, alias compatibility, and observed application behavior. Inventories and
test results belong in the accepting commit's verdict under
[Desktop acceptance](desktop-acceptance.md), not in permanent archive-specific guidance.
Verify preservation of copied user content separately from intended metadata changes.
A successful smoke test does not establish that test-only facts are correct for
production adoption.
