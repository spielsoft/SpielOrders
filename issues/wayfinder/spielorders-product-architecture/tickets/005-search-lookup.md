---
title: Define lookup and search behavior
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders has two lookup engines: deterministic native entity search over managed facts and a complementary OS-indexed Supporting Material search. Native entity search means Rust-owned search, not a Swift or operating-system search API. Folder Contents combines the two in the Orders list; the library decides what matches in every scope.

### Native entity search

Native search live-filters the ordinary entity list belonging to the current view. In the Orders view it retains matching Orders; in the Vendors view it retains matching Vendors. A search covers every searchable fact or, when confined, one of them. Search changes only which existing rows are visible. It does not introduce search-specific rows, match excerpts, result counts, relevance ranking, presentation, selection, inspection, keyboard navigation, or Finder behavior. Those remain the ordinary interactions of the entity table.

The native query is deterministic:

- whitespace-separated terms use AND semantics, and terms may match across different searchable facts;
- each term matches a substring rather than requiring a whole word or prefix;
- matching is diacritic-insensitive and case-insensitive by default;
- Match Case switches between case-insensitive and case-sensitive matching without changing the other rules;
- there is no fuzzy matching, stemming, Boolean syntax, phrase grammar, or relevance scoring;
- a monetary fact matches both the spelling its record stores and its spelling rounded to two decimal places, so a Unit Price of `12.5` is found by `12.50`, while a quantity is not money and matches its stored spelling alone;
- an empty query applies no text restriction.

For an Order, searchable content comprises Order ID; every user-facing canonical Order fact; Order Item descriptions, catalog numbers, quantities, unit prices, and notes; the Order README's text; and every current shared fact of the associated Vendor, including Vendor Contacts and purchasing notes. Vendor changes therefore immediately affect which associated Orders match. For a Vendor, searchable content comprises Vendor ID and every user-facing canonical Vendor fact, including its Contacts.

A search confined to one fact applies the same term, case, and monetary rules to that fact alone. An Order search can be confined to Title, Date Ordered, Status, Vendor, Orderer, Ordering Institution, Project, Order Group, Currency, Order ID, or an Item's Description, Catalog Number, Quantity, Unit Price, or Notes; Vendor matches the referenced Vendor's Name, and an Item fact matches any of the Order's Items. A Vendor search can be confined to Name, Phone, Email, Website, Address, Purchasing Notes, Vendor ID, or a Contact's Name, Role, Phone, or Email, matching any Contact. The README, the stored Vendor reference, and the Vendor's other facts are reached only by a search of every fact. The library offers these facts in menu order.

Native search excludes the Order Directory basename, filenames, filesystem paths, Supporting Material, the Package README, technical schema versions, raw JSON keys or syntax, raw Markdown syntax as distinct from its text, calculated totals, configuration, and the derived `index.json`. The Package README has no associated Order or Vendor and therefore does not participate in entity lookup.

Native search preserves the active table sort. The default Order ordering is Date Ordered descending, then Order Title ascending, then Order ID as an invisible stable tie-breaker. Clicking a column header selects that column's ordinary sort and clicking it again reverses direction; changing a query never changes that ordering. Baseline entity-table navigation and the ordinary action for revealing an Order Directory in Finder remain for the desktop interaction decision rather than being redefined by search.

Post-MVP structured Order filters will cover Order Status, Date Ordered range, Vendor, Ordering Institution, Orderer, Project, Order Group, and Order Currency. Each optional classification filter includes its unset value, and Vendor includes `No Vendor`. Multiple selections within one filter combine with OR; different filters and the native text query combine with AND. One action clears all filters. Price ranges, item counts, generic populated-field tests, and Vendor structured filters are excluded from this contract. These filters are deliberately deferred until after the MVP release.

### OS-indexed Supporting Material search and Folder Contents

Supporting Material is searched only through Folder Contents, a scope of the Orders search rather than a separate view or window. On macOS it is labeled **Folder Contents (Spotlight)**; on a platform without an equivalent branded service the service name is omitted. The provider's query uses its native matching semantics after a short typing debounce, with no promise of matching parity with native search and no case control.

Folder Contents keeps the ordinary Orders list, its sort, and its selection. The list shows each Order whose facts match the query in every searchable fact, under the current Match Case, or that holds a matching Supporting Material file, once whichever way it matched. The selected Order's detail lists its matched files with filename, relative path within the Order Directory, file kind, and modification date; a file can be opened in its default application or revealed selected in Finder. Within an Order, files sort by OS-provided content relevance descending, with filename and then relative path as deterministic fallbacks; relevance is not displayed and never reorders Orders.

When healthy recognized Orders exist, the macOS provider submits one Spotlight query over the Package's configured Orders Directory rather than querying each Order separately. The core filters the returned paths to healthy recognized Order Directories and excludes each Order's conventional `README.md` and its reserved `.spielorders/` subtree so managed Markdown and JSON cannot reappear as file results. It does not exclude unrelated nested Supporting Material merely because a nested file or directory has a familiar name. With no healthy recognized Orders, no Spotlight query is needed.

Native search yields each matching entity once, irrespective of how many facts match. OS-indexed results are deduplicated only by standardized filesystem path: the same path appears once, while identical contents, repeated filenames, or separate copies at different paths remain distinct Supporting Material. Excluding managed artifacts keeps managed text from also appearing as a matched file.

OS-indexed search is explicitly complementary and best-effort. Folder Contents may show partial results while gathering. Disabled indexing, an unindexed or unsupported volume, denied access, and query failures receive specific explanations; SpielOrders never falls back to recursively parsing or indexing Supporting Material. When no file matched, Folder Contents says the OS index found none, not that no matching file exists. If a result moves or disappears before Open or Reveal, SpielOrders reports that it is unavailable and removes it when results refresh.

How damaged Order Directories participate in lookup remains with [Define integrity, validation, and repair boundaries](008-integrity-repair.md), because a file without a valid associated Order cannot satisfy the normal Order-group navigation contract.
