---
title: Define lookup and search behavior
status: closed
parent: Chart the SpielOrders product and architecture
---

## Decision

SpielOrders has two distinct lookup experiences: deterministic native entity search and a complementary OS-indexed Supporting Material search. They do not mix results or behavior. Here, native entity search means Rust-owned search over managed facts, not a Swift or operating-system search API.

### Native entity search

Native search live-filters the ordinary entity list belonging to the current view. In the Orders view it retains matching Orders; in the Vendors view it retains matching Vendors. Search changes only which existing rows are visible. It does not introduce search-specific rows, match excerpts, result counts, relevance ranking, presentation, selection, inspection, keyboard navigation, or Finder behavior. Those remain the ordinary interactions of the entity table.

The native query is deterministic:

- whitespace-separated terms use AND semantics, and terms may match across different searchable facts;
- each term matches a substring rather than requiring a whole word or prefix;
- matching is diacritic-insensitive and case-insensitive by default;
- a visible `Aa` control switches between case-insensitive and case-sensitive matching without changing the other rules;
- there is no fuzzy matching, stemming, Boolean syntax, phrase grammar, or relevance scoring;
- a monetary fact matches both the spelling its record stores and its spelling rounded to two decimal places, so a Unit Price of `12.5` is found by `12.50`, while a quantity is not money and matches its stored spelling alone;
- an empty query applies no text restriction.

For an Order, searchable content comprises Order ID; every user-facing canonical Order fact; Order Item descriptions, catalog numbers, quantities, and unit prices; the Order README's text; and every current shared fact of the associated Vendor, including Vendor Contacts and purchasing notes. Vendor changes therefore immediately affect which associated Orders match. For a Vendor, searchable content comprises Vendor ID and every user-facing canonical Vendor fact.

Native search excludes the Order Directory basename, filenames, filesystem paths, Supporting Material, the Package README, technical schema versions, raw JSON keys or syntax, raw Markdown syntax as distinct from its text, calculated totals, configuration, and the derived `index.json`. The Package README has no associated Order or Vendor and therefore does not participate in entity lookup.

Native search preserves the active table sort. The default Order ordering is Date Ordered descending, then Order Title ascending, then Order ID as an invisible stable tie-breaker. Clicking a column header selects that column's ordinary sort and clicking it again reverses direction; changing a query never changes that ordering. Baseline entity-table navigation and the ordinary action for revealing an Order Directory in Finder remain for the desktop interaction decision rather than being redefined by search.

Post-MVP structured Order filters will cover Order Status, Date Ordered range, Vendor, Ordering Institution, Orderer, Project, Order Group, and Order Currency. Each optional classification filter includes its unset value, and Vendor includes `No Vendor`. Multiple selections within one filter combine with OR; different filters and the native text query combine with AND. One action clears all filters. Price ranges, item counts, generic populated-field tests, and Vendor structured filters are excluded from this contract. These filters are deliberately deferred until after the MVP release.

### OS-indexed Supporting Material search

Supporting Material search is a distinct view within the application's single native window, never a second application window or a section of native results. On macOS the view is labeled **Spotlight**, using the established service name. On a platform without an equivalent branded service it is labeled **Supporting File Search**.

The view accepts an ordinary literal-word query, submits it asynchronously to the OS-native index after a short typing debounce, and displays gathering or update activity. It uses the provider's native matching semantics and offers no native-search case toggle, filter controls, or promise of matching parity with deterministic entity search.

Results are file-centric but organized around their associated Orders:

- one expandable group represents each Order with one or more matching files and shows the ordinary Order summary plus the number of matching files;
- expanding the group shows one child row per file, including filename, relative path within the Order Directory, file kind, and modification date;
- activating the Order group opens that Order in SpielOrders;
- a file row can open the file in its default application or reveal that exact file selected in Finder;
- Order groups retain the ordinary active Order sort;
- files within an Order group sort by OS-provided content relevance descending, with filename and then relative path as deterministic fallbacks for equal or unavailable relevance;
- relevance is not displayed as a score and never reorders Order groups.

When healthy recognized Orders exist, the macOS provider submits one Spotlight query over the Package's configured Orders Directory rather than querying each Order separately. The core filters the returned paths to healthy recognized Order Directories and excludes each Order's conventional `README.md` and its reserved `.spielorders/` subtree so managed Markdown and JSON cannot reappear as file results. It does not exclude unrelated nested Supporting Material merely because a nested file or directory has a familiar name. With no healthy recognized Orders, no Spotlight query is needed.

Native search yields each matching entity once, irrespective of how many facts match. OS-indexed results are deduplicated only by standardized filesystem path: the same path appears once, while identical contents, repeated filenames, or separate copies at different paths remain distinct Supporting Material. Keeping the two search experiences separate and excluding managed artifacts prevents cross-engine duplicates.

OS-indexed search is explicitly complementary and best-effort. The view may show partial results while gathering. Disabled indexing, an unindexed or unsupported volume, denied access, and query failures receive specific explanations; SpielOrders never falls back to recursively parsing or indexing Supporting Material. An empty view says the OS index found no matches, not that no matching file exists. If a result moves or disappears before Open or Reveal, SpielOrders reports that it is unavailable and removes it when results refresh.

How damaged Order Directories participate in lookup remains with [Define integrity, validation, and repair boundaries](008-integrity-repair.md), because a file without a valid associated Order cannot satisfy the normal Order-group navigation contract.
