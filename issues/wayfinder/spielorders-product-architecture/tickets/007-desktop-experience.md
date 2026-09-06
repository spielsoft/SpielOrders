---
title: Prototype the SpielOrders desktop experience
status: closed
parent: Chart the SpielOrders product and architecture
labels:
  - wayfinder:prototype
assignee: /root
---

## Decision

The accepted experience is captured in the [interactive SpielOrders desktop prototype](../prototypes/spielorders-desktop-prototype.html). It is a disposable design artifact, not production application code.

### Window and navigation

SpielOrders is a macOS-first, single-window desktop application with one active SpielOrder Package. The toolbar has a functional Package menu showing the current directory name and actions to open, reopen, or initialize a Package; it is not a passive Package label. Orders, Vendors, and the Package `README.md` are toolbar destinations. **New Order** and a visible trailing inspector-toggle button complete the ordinary toolbar. There is no permanent sidebar and no real browser product surface.

The inspector is open by default, can be toggled from the top-right toolbar control, and remembers its visibility and width. The default window is deliberately wider than the SpielGantt precedent. The visual language is restrained and macOS-like: labels, icons, headings, and actions exist only when they perform a decided function.

### Orders table and lookup

The primary Orders view is a dense, informative grid designed for hundreds or thousands of Orders. Its vertically scrolling body and the inspector reach the same bottom edge. A left-aligned line above the grid reports **“N of M orders shown.”** Single selection drives the compact inspector; double-click, Return, or contextual **Open** enters the full editor.

The default visible columns are Date, Status, Order Title, Vendor, and Orderer. Right-clicking the header opens a checkbox menu containing every available column; optional columns include Institution, Project, Order Group, Directory Name, Currency, Order ID, and Total. Total is not visible by default. Column visibility, order, and widths are app preferences and persist independently of Package data.

Ordinary search live-filters the grid and retains its current sort. The visible **Aa** button controls case sensitivity. **Spotlight** is a peer control beside the search field, not a sidebar destination: toggling it preserves the query and reruns that text through OS-native Supporting Material search; Aa is unavailable in that mode. Toggling back reruns native search. Spotlight results are file-centric, grouped by Order, and can open or reveal the exact matched file. Structured-filter UI remains deliberately undefined for later user testing and post-MVP work.

Damaged Order placeholders appear only in the unfiltered grid with one diagnostic marker, not a fabricated Status or a fully red row. Their compact inspector shows the trusted directory name and diagnosis; damaged Orders do not participate in either search mode.

### Selection, removal, and Finder

The compact Order inspector contains ordinary facts, Directory Name, a short Items table with its Total in the footer, the plain `README.md` text, and one **Reveal in Finder** action. It has no redundant Open button or Supporting Material heading. Supporting Material remains a Finder-owned opaque directory tree; the application does not embed a browser, upload surface, or inventory.

The row context menu contains Open, Reveal in Finder, Align Directory Name, Remove Order, and a visually separated Move to Trash action. The application Order menu exposes the same appropriate commands. The ordinary macOS Delete key invokes recoverable Remove Order with confirmation. The main Orders table has no permanent plus/minus controls. **Align Directory Name** is also a standard action in the Order editor, where the current directory name is visible.

### Order and Vendor editing

The full Order editor uses explicit Save (`Cmd-S`). Save is enabled only when the draft is dirty; leaving with unsaved changes prompts. A new Order opens this ordinary editor with Date and Status defaults and creates no directory until the minimum required facts allow a successful Save. It never opens the diagnostic editor.

Status is an ordinary dropdown containing only the five domain statuses. Selecting Ordered fills the draft Date Ordered with today when needed, and the user may change it before Save. Vendor is a searchable selector whose first action is **New Vendor…**, followed by **No Vendor** and existing Vendors. Creating a Vendor in a sheet preserves the Order draft and selects the result.

Order Items are edited as a plain spreadsheet inside the full editor, using the same restrained row-and-column language as the main Orders table rather than separate rounded input controls. Catalog Number precedes Description; Description receives most of the width, while Quantity and Unit Price remain compact numeric columns without increment/decrement steppers. Double-click or Return begins editing, Tab advances, drag changes order, and conventional plus/minus controls appear only beneath this Items table to add or remove whole rows. Deletion is pending until Save and `Cmd-Z` restores it without confirmation. The calculated Order Total belongs in the table footer.

Validation is Save-time feedback, not a preliminary modal or a high-level GUI validator. A rejected Save preserves the draft, shows inline field or item errors plus a concise **Couldn’t save** summary, and focuses the first error. Successful saves are silent. Package and Order `README.md` surfaces are plain-text Markdown editors with lightweight syntax highlighting, no rendered preview, and no formatting toolbar.

Vendors use the same dense table → compact inspector → explicit full editor pattern as Orders. Shared Vendor facts remain live; guarded domain operations stay in the Rust core.

### Damage and repair

Double-clicking a Damaged Order opens a separate diagnostic workspace. Each diagnosis shows evidence and only the allowed explicit actions. The user selects one action, supplies its facts, reviews the exact planned change, applies it, and the Rust core revalidates. There is no generic Fix or Fix All.

The GUI has one deliberate architectural escape hatch: the diagnostic workspace may directly edit and write raw managed JSON, bypassing the Rust library and CLI mutation APIs, and then ask the Rust core to revalidate. This is not a core repair action, does not attempt partial salvage, and has no GUI-level validity gate. If the written text is still invalid, the Order remains damaged. The exception exists for expert recovery and is expected to be rarely used; all ordinary reads and mutations continue through the typed Rust core.

### Frontend architecture

The initial implementation uses **Tauri 2, React, and TypeScript**. Tauri is a desktop shell only; browser deployment is unsupported. A typed frontend capability interface isolates Tauri invocation and native Finder/OS-search services from presentation state so the frontend remains testable and the shell boundary replaceable.

Use **Mantine Core, Mantine Hooks, Mantine UI, and Mantine tables throughout the initial frontend**, including Orders, Vendors, and Order Items. SpielOrders owns search, sorting, selection, draft, and persisted-column state behind small local table-component contracts, and Mantine-specific types do not escape those boundaries. TanStack Table or another engine is introduced only after an observed Mantine limitation, without changing callers.

SpielOrders does not create a shared frontend package with SpielGantt initially. It may follow compatible Mantine theme tokens and architectural patterns, but Orders, Vendors, navigation, inspectors, and table models remain local. When concrete duplicated low-level code exists, mark that code `TODO(shared-frontend-foundation)` and evaluate extracting only the proven duplicate into a policy-neutral shared foundation. No abstraction is created in anticipation of duplication.
