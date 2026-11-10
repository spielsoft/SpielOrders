# First order collection conversion

## Source and test boundary

The first-use source is `/Users/example/Documents/Administration/Orders`.
Conversion testing operates only on an isolated copy; the source remains unchanged.
The disposable acceptance copy is
`/Users/example/Documents/Administration/SpielOrders-M6-Test.M3Zj4f/Orders`. Test-only metadata is not a proposed
production conversion and must never be copied back to the source.

## Existing organization

The source creation instructions and archive README identify `All Orders` as the physical
collection. Institution directories are symbolic-link views of those same Orders, not
additional collections to adopt. Root-level Resources and archive material remain ordinary
user-owned content outside the configured Orders Directory.

The inspected collection has no Package marker or canonical Order records. Existing
folders therefore require explicit adoption; their supporting documents alone do not
make them recognized Orders. The source instructions prohibit hand-written SpielOrders
metadata. Use SpielOrders capabilities after reviewing evidence.

## Package initialization

Initialize the copy's archive root with `All Orders` as its configured Orders Directory.
The conventional default would leave the real collection outside discovery. Open that
archive root, not `All Orders` itself or an institution view, as the Package Root.
Initialization does not adopt the existing folders. Unadopted directories are Damaged
Order placeholders, not empty healthy Orders, and do not participate in either search.

Preserve relative aliases and root-level material. Verify that aliases inside the copy
resolve inside the copy. Copied instructions and scripts can contain absolute paths back
to the source; use explicit copy roots and do not run copied archive-maintenance automation
during application smoke testing. Do not rebuild the hand-maintained summary workbook.

The institutional alias
`Northwind/2026_08_14_Elderglass_Contracting_Ring_Optics_Controls_Spielman` has a missing target.
`2026_08_14_Elderglass_Contracting_Ring_Liquid_Lenses_Spielman` is a plausible candidate,
not a confirmed identity; obtain evidence before changing the real archive. Institutional views are outside the configured Orders
Directory, so successful Package validation does not establish that their links work.

## Adoption evidence and owner review

The archive README reports inferred dates and shortened vendor names. A directory basename
or summary row is candidate evidence, not automatic confirmation of Date Ordered or shared
Vendor identity. Review the actual order evidence before accepting those facts. Preserve
uncertainty rather than inventing Status, Orderer, Items, prices, or currency.

Do not align directory names as a side effect of adoption. The existing institutional links
and summary hyperlinks depend on those names. Confirm any intended rename separately.
Keep supporting files opaque to application discovery; do not turn document contents into
canonical facts through an undocumented bulk inference.

## Verification and retained evidence

Record initialization configuration, adoption inputs and their evidence, unresolved facts,
alias compatibility, and observed application behavior. Numerical inventories and test results
belong in the accepting commit's verdict under [Desktop acceptance](desktop-acceptance.md), not here.
Verify copy preservation separately from canonical metadata changes. A successful smoke
does not establish that test-only facts are correct for production adoption.
