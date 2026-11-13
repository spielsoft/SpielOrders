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
        table.allowsMultipleSelection = false
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
        table.removals = model.removals
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
                rows = model.orders
                revision = model.ordersRevision
                table.reloadData()
            }
            synchronizeSelection()
            if !model.busy { synchronizeSort() }
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
                image.image = NSImage(named: NSImage.cautionName)
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
            cell.imageView?.isHidden = !damagedTitle
            cell.toolTip = damagedTitle ? "This directory needs attention. Open Diagnostics for details." : nil
            cell.textField?.stringValue = damagedTitle ? value.directoryName : display(column, of: value)
            cell.textField?.setAccessibilityLabel(column.label)
            cell.textField?.setAccessibilityIdentifier("\(rowIdentifier(value)).\(column.rawValue)")
            return cell
        }

        /// A Status by its label, a date in the short style, a Total as money, and every
        /// other value as the library gives it.
        private func display(_ column: OrderColumn, of row: OrderRow) -> String {
            let value = row.values[column.field] ?? ""
            switch column {
            case .status: return OrderStatus.label(value)
            case .date: return ShortDate.display(value)
            case .total: return Money.display(value, currency: row.values[OrderColumn.currency.field] ?? "")
            default: return value
            }
        }

        private func rowIdentifier(_ row: OrderRow) -> String {
            "orders.row." + (row.damage == nil ? row.id : "damaged:" + row.directoryName)
        }

        func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
            if model.committing, rows.indices.contains(row) {
                let id = rows[row].id
                _ = model.deferNavigation { [model] in model.selectOrder(id) }
                return false
            }
            return !model.busy
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !updating, let table else { return }
            let id = rows.indices.contains(table.selectedRow) ? rows[table.selectedRow].id : nil
            model.selectOrder(id)
            updating = true
            synchronizeSelection()
            updating = false
        }

        private func synchronizeSelection() {
            guard let table else { return }
            let selected = rows.firstIndex { $0.id == model.selectedOrderID }
            let indexes = selected.map { IndexSet(integer: $0) } ?? []
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
                let open = NSMenuItem(title: row.damage == nil ? "Open" : "Open Diagnostics", action: #selector(open(_:)), keyEquivalent: "")
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
            guard let table, rows.indices.contains(table.selectedRow) else { return }
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

/// Return focuses the selected Order's detail; Delete removes the Order, and Undo and Redo
/// follow Remove Order's history while it has any.
final class OrdersTableView: NSTableView {
    var open: () -> Void = {}
    var remove: () -> Void = {}
    var removals: UndoManager?

    override var undoManager: UndoManager? {
        guard let removals, removals.canUndo || removals.canRedo else { return super.undoManager }
        return removals
    }

    @objc func undo(_ sender: Any?) {
        undoManager?.undo()
    }

    @objc func redo(_ sender: Any?) {
        undoManager?.redo()
    }

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
