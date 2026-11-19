import AppKit
import CSpielOrders
import SwiftUI
import SpielOrdersKit

struct OrdersTable: NSViewRepresentable {
    @Bindable var model: DesktopModel

    func makeCoordinator() -> Coordinator { Coordinator(model: model) }

    func makeNSView(context: Context) -> NSScrollView {
        let table = OrdersTableView()
        table.setAccessibilityLabel("Orders")
        table.setAccessibilityIdentifier("orders.table")
        table.usesAlternatingRowBackgroundColors = true
        table.rowHeight = 22
        // A truncated value shows in full while the pointer rests on it.
        table.allowsExpansionToolTips = true
        table.allowsMultipleSelection = true
        table.allowsEmptySelection = true
        table.allowsColumnReordering = true
        table.allowsColumnResizing = true
        table.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        for field in OrderColumn.allCases {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(field.rawValue))
            column.title = field.label
            column.headerCell.setAccessibilityIdentifier("orders.column.\(field.rawValue)")
            column.minWidth = 65
            column.width = field.initialWidth
            column.isHidden = !field.initiallyVisible
            column.sortDescriptorPrototype = NSSortDescriptor(key: field.rawValue, ascending: true)
            table.addTableColumn(column)
        }
        table.autosaveName = "OrdersTable"
        table.autosaveTableColumns = true
        if table.tableColumns.allSatisfy(\.isHidden) {
            for column in table.tableColumns {
                column.isHidden = !(OrderColumn(rawValue: column.identifier.rawValue)?.initiallyVisible ?? false)
            }
        }
        for column in table.tableColumns where !column.width.isFinite || column.width < column.minWidth {
            column.width = OrderColumn(rawValue: column.identifier.rawValue)?.initialWidth ?? 140
        }
        table.delegate = context.coordinator
        table.dataSource = context.coordinator
        table.target = context.coordinator
        table.doubleAction = #selector(Coordinator.openClicked(_:))
        table.open = { [weak coordinator = context.coordinator] in coordinator?.openSelected() }
        table.remove = { [weak model] in model?.changeOrderDirectory(.remove) }
        table.headerView?.menu = context.coordinator.columnsMenu
        table.menu = context.coordinator.rowMenu
        context.coordinator.table = table
        let coordinator = context.coordinator
        Task { @MainActor in coordinator.restoreSort() }
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.documentView = table
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.update()
    }

    /// Whatever size the list offers: wide columns scroll sideways rather than widening the
    /// list, which would take the width the owner gave the detail.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSScrollView, context: Context) -> CGSize? {
        proposal.replacingUnspecifiedDimensions(by: CGSize(width: 400, height: 300))
    }

    @MainActor final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate, NSMenuDelegate, NSMenuItemValidation {
        let model: DesktopModel
        weak var table: NSTableView?
        let columnsMenu = NSMenu(title: "Columns")
        let rowMenu = NSMenu(title: "Order")
        private var rows: [OrderRow] = []
        private var revision = -1
        private var updating = false

        init(model: DesktopModel) {
            self.model = model
            super.init()
            columnsMenu.autoenablesItems = false
            columnsMenu.delegate = self
            rowMenu.delegate = self
            for column in OrderColumn.allCases {
                let item = NSMenuItem(title: column.label, action: #selector(toggleColumn(_:)), keyEquivalent: "")
                item.representedObject = column.rawValue
                item.target = self
                item.setAccessibilityIdentifier("orders.column.visibility.\(column.rawValue)")
                columnsMenu.addItem(item)
            }
        }

        func update() {
            guard let table else { return }
            updating = true
            defer { updating = false }
            if revision != model.ordersRevision {
                let previous = rows
                rows = model.orders
                revision = model.ordersRevision
                reload(from: previous, in: table)
                // An empty table is plain behind what it says instead of rows.
                table.usesAlternatingRowBackgroundColors = !rows.isEmpty
            }
            synchronizeSelection()
            if !model.busy { synchronizeSort() }
        }

        /// Shows the listing that replaced `previous`. A listing of the same length, as a saved
        /// edit leaves it, redraws only the rows that changed, so the rest of the list holds
        /// still; any other listing reloads the table.
        private func reload(from previous: [OrderRow], in table: NSTableView) {
            guard previous.count == rows.count else {
                table.reloadData()
                return
            }
            let changed = IndexSet(rows.indices.filter { index in
                let old = previous[index], new = rows[index]
                return old.id != new.id || old.damage != new.damage || old.values != new.values
            })
            table.reloadData(forRowIndexes: changed, columnIndexes: IndexSet(integersIn: 0..<table.numberOfColumns))
        }

        func numberOfRows(in tableView: NSTableView) -> Int { rows.count }

        func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
            guard rows.indices.contains(row), let tableColumn,
                  let column = OrderColumn(rawValue: tableColumn.identifier.rawValue) else { return nil }
            let cell: NSTableCellView
            if let reused = tableView.makeView(withIdentifier: tableColumn.identifier, owner: self) as? NSTableCellView {
                cell = reused
            } else {
                cell = NSTableCellView()
                cell.identifier = tableColumn.identifier
                let image = NSImageView()
                image.setAccessibilityLabel("Damaged Order")
                let text = NSTextField(labelWithString: "")
                text.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                text.lineBreakMode = .byTruncatingTail
                let stack = NSStackView(views: [image, text])
                stack.spacing = 4
                stack.translatesAutoresizingMaskIntoConstraints = false
                cell.addSubview(stack)
                cell.textField = text
                cell.imageView = image
                NSLayoutConstraint.activate([
                    stack.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 3),
                    stack.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -3),
                    stack.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
                    image.widthAnchor.constraint(equalToConstant: 12), image.heightAnchor.constraint(equalToConstant: 12),
                ])
            }
            let value = rows[row]
            let damagedTitle = value.damage != nil && column == .title
            // Only a damaged Order's title draws its symbol, so other cells skip making it.
            cell.imageView?.image = damagedTitle ? NoticeLevel.problem.image : nil
            cell.imageView?.isHidden = !damagedTitle
            cell.toolTip = damagedTitle ? "This Order’s folder needs attention. Diagnose Order for details." : nil
            cell.textField?.stringValue = damagedTitle ? value.directoryName : value.display(column)
            cell.textField?.setAccessibilityLabel(column.label)
            cell.textField?.setAccessibilityIdentifier("\(rowIdentifier(value)).\(column.rawValue)")
            return cell
        }

        private func rowIdentifier(_ row: OrderRow) -> String {
            "orders.row." + (row.damage == nil ? row.id : "damaged:" + row.directoryName)
        }

        /// A selection made while a field's save settles waits for that save; none is made
        /// while other work runs.
        func tableView(_ tableView: NSTableView,
                       selectionIndexesForProposedSelection proposed: IndexSet) -> IndexSet {
            if model.committing {
                let ids = identifiers(proposed)
                _ = model.deferNavigation { [model] in model.selectOrders(ids) }
                return tableView.selectedRowIndexes
            }
            return model.busy ? tableView.selectedRowIndexes : proposed
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !updating, let table else { return }
            model.selectOrders(identifiers(table.selectedRowIndexes))
            updating = true
            synchronizeSelection()
            updating = false
        }

        private func identifiers(_ indexes: IndexSet) -> [String] {
            indexes.filter(rows.indices.contains).map { rows[$0].id }
        }

        private func synchronizeSelection() {
            guard let table else { return }
            let selected = model.multipleSelection.isEmpty
                ? [model.selectedOrderID].compactMap { $0 }
                : model.multipleSelection
            let indexes = IndexSet(rows.indices.filter { selected.contains(rows[$0].id) })
            if table.selectedRowIndexes != indexes { table.selectRowIndexes(indexes, byExtendingSelection: false) }
        }

        func tableView(_ tableView: NSTableView, sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]) {
            guard !updating else { return }
            restoreSort()
        }

        func restoreSort() {
            guard let table else { return }
            let accepted: Bool
            if let first = table.sortDescriptors.first, let key = first.key, let column = OrderColumn(rawValue: key) {
                accepted = model.changeOrderSort(field: column.field, descending: !first.ascending)
                if table.sortDescriptors.count > 1 {
                    updating = true
                    table.sortDescriptors = [first]
                    updating = false
                }
            } else { accepted = model.changeOrderSort(field: -1, descending: false) }
            if !accepted {
                updating = true
                synchronizeSort()
                updating = false
            }
        }

        private func synchronizeSort() {
            guard let table else { return }
            let field = OrderColumn.allCases.first { $0.field == model.orderSortField }
            let descriptors = field.map { [NSSortDescriptor(key: $0.rawValue, ascending: !model.orderSortDescending)] } ?? []
            if table.sortDescriptors != descriptors { table.sortDescriptors = descriptors }
        }

        func menuNeedsUpdate(_ menu: NSMenu) {
            guard let table else { return }
            if menu === columnsMenu {
                for item in menu.items {
                    guard let id = item.representedObject as? String,
                          let column = table.tableColumn(withIdentifier: NSUserInterfaceItemIdentifier(id)) else { continue }
                    item.state = column.isHidden ? .off : .on
                    item.isEnabled = column.isHidden || table.tableColumns.filter { !$0.isHidden }.count > 1
                }
            } else {
                menu.removeAllItems()
                guard rows.indices.contains(table.clickedRow) else { return }
                let row = rows[table.clickedRow]
                // A click within several selected Orders acts on all of them.
                if model.multipleSelection.contains(row.id) {
                    let merge = NSMenuItem(title: model.mergeTitle, action: #selector(merge(_:)), keyEquivalent: "")
                    merge.target = self
                    merge.setAccessibilityIdentifier("orders.merge")
                    menu.addItem(merge)
                    return
                }
                let open = NSMenuItem(title: row.damage == nil ? "Open" : "Diagnose Order", action: #selector(open(_:)), keyEquivalent: "")
                open.representedObject = row.id
                open.target = self
                open.setAccessibilityIdentifier("\(rowIdentifier(row)).open")
                menu.addItem(open)
                let item = NSMenuItem(title: "Show in Finder", action: #selector(reveal(_:)), keyEquivalent: "")
                item.representedObject = row.id
                item.target = self
                item.setAccessibilityIdentifier("\(rowIdentifier(row)).reveal")
                menu.addItem(item)
                if row.damage == nil {
                    for (title, action, selector) in [
                        ("Align Directory Name…", "align", #selector(align(_:))),
                        ("Remove Order", "remove", #selector(remove(_:))),
                        ("Move to Trash…", "trash", #selector(trash(_:))),
                    ] {
                        if action == "trash" { menu.addItem(.separator()) }
                        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
                        item.target = self
                        item.representedObject = row.id
                        item.setAccessibilityIdentifier("\(rowIdentifier(row)).\(action)")
                        menu.addItem(item)
                    }
                }
            }
        }

        func validateMenuItem(_ item: NSMenuItem) -> Bool {
            if item.action == #selector(merge(_:)) { return model.canMergeOrders }
            guard !model.busy, model.vendorSheet == nil,
                  let id = item.representedObject as? String,
                  let row = rows.first(where: { $0.id == id }) else { return false }
            switch item.action {
            case #selector(align(_:)), #selector(remove(_:)), #selector(trash(_:)):
                return !model.dirty && row.damage == nil
            default: return true
            }
        }

        @objc private func toggleColumn(_ sender: NSMenuItem) {
            guard let id = sender.representedObject as? String,
                  let column = table?.tableColumn(withIdentifier: NSUserInterfaceItemIdentifier(id)) else { return }
            if !column.isHidden, table?.tableColumns.filter({ !$0.isHidden }).count == 1 { return }
            column.isHidden.toggle()
        }

        @objc func openClicked(_ sender: NSTableView) {
            guard rows.indices.contains(sender.clickedRow) else { return }
            model.openOrder(rows[sender.clickedRow].id)
        }

        func openSelected() {
            guard let table, table.numberOfSelectedRows == 1, rows.indices.contains(table.selectedRow) else { return }
            model.openOrder(rows[table.selectedRow].id)
        }

        @objc private func open(_ sender: NSMenuItem) {
            guard let id = sender.representedObject as? String else { return }
            model.openOrder(id)
        }

        @objc private func reveal(_ sender: NSMenuItem) {
            guard let id = sender.representedObject as? String else { return }
            model.revealOrder(id)
        }

        @objc private func merge(_ sender: NSMenuItem) {
            model.mergeOrders()
        }

        @objc private func align(_ sender: NSMenuItem) {
            model.changeOrderDirectory(.align, id: sender.representedObject as? String)
        }
        @objc private func remove(_ sender: NSMenuItem) {
            model.changeOrderDirectory(.remove, id: sender.representedObject as? String)
        }
        @objc private func trash(_ sender: NSMenuItem) {
            model.changeOrderDirectory(.trash, id: sender.representedObject as? String)
        }
    }
}

/// Return focuses the selected Order's detail; Delete removes the Order. Neither acts on
/// several selected Orders.
final class OrdersTableView: NSTableView {
    var open: () -> Void = {}
    var remove: () -> Void = {}

    override func keyDown(with event: NSEvent) {
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else {
            super.keyDown(with: event)
            return
        }
        if [36, 76].contains(event.keyCode) { open() }
        else if [51, 117].contains(event.keyCode) { remove() }
        else { super.keyDown(with: event) }
    }
}

extension OrderRow {
    /// A Status by its label, a date in the short style, a Total as money, and every other
    /// value as the library gives it.
    @MainActor func display(_ column: OrderColumn) -> String {
        let value = values[column.field] ?? ""
        switch column {
        case .status: return OrderStatus.label(value)
        case .date: return ShortDate.display(value)
        case .total: return Money.display(value, currency: values[OrderColumn.currency.field] ?? "")
        default: return value
        }
    }
}
