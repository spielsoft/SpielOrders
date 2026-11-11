import AppKit
import CSpielOrders
import SwiftUI

/// The rows of a draft as a plain spreadsheet: Items of an Order or Contacts of a
/// Vendor. Double-click or Return edits a cell, Tab advances, and dragging reorders.
struct DraftRowsTable: NSViewRepresentable {
    struct Column {
        let field: Int32
        let width: CGFloat
        var editable = true
        var numeric = false
        var flexible = false
    }

    let editor: RecordEditor
    let columns: [Column]
    let label: String
    let identifier: String

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let table = RowsTableView()
        table.coordinator = context.coordinator
        table.setAccessibilityLabel(label)
        table.setAccessibilityIdentifier(identifier)
        table.usesAlternatingRowBackgroundColors = true
        table.rowHeight = 22
        table.allowsMultipleSelection = true
        table.allowsEmptySelection = true
        table.allowsColumnReordering = false
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        for column in columns {
            let tableColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(RecordFieldLabel.key(column.field)))
            tableColumn.title = RecordFieldLabel.of(column.field)
            tableColumn.headerCell.setAccessibilityIdentifier("\(identifier).column.\(RecordFieldLabel.key(column.field))")
            tableColumn.width = column.width
            tableColumn.minWidth = 50
            tableColumn.resizingMask = column.flexible ? [.autoresizingMask, .userResizingMask] : .userResizingMask
            if column.numeric { tableColumn.headerCell.alignment = .right }
            table.addTableColumn(tableColumn)
        }
        table.registerForDraggedTypes([.rowIndex])
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        table.delegate = context.coordinator
        table.dataSource = context.coordinator
        table.target = context.coordinator
        table.doubleAction = #selector(Coordinator.editClickedCell(_:))
        context.coordinator.table = table
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .bezelBorder
        scroll.documentView = table
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        context.coordinator.isEnabled = context.environment.isEnabled
        if let manager = scroll.undoManager { editor.undoManager = manager }
        context.coordinator.update()
    }

    @MainActor final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate {
        var parent: DraftRowsTable
        weak var table: NSTableView?
        var isEnabled = true
        private var revision = -1
        private var rowsRevision = -1
        private var focusSerial = 0
        private var synchronizing = false

        init(_ parent: DraftRowsTable) { self.parent = parent }

        private var editor: RecordEditor { parent.editor }

        func update() {
            guard let table else { return }
            synchronizing = true
            defer { synchronizing = false }
            table.isEnabled = isEnabled
            if !isEnabled, editingCell != nil { table.window?.makeFirstResponder(table) }
            if rowsRevision != editor.rowsRevision {
                // Rows moved under any cell being edited, so that edit ends first.
                if editingCell != nil { table.window?.makeFirstResponder(table) }
                rowsRevision = editor.rowsRevision
                revision = editor.revision
                table.reloadData()
            } else if revision != editor.revision {
                revision = editor.revision
                if editingCell == nil { table.reloadData() }
            }
            refreshVisibleCells()
            let wanted = editor.selectedRows.filteredIndexSet { $0 < table.numberOfRows }
            if table.selectedRowIndexes != wanted { table.selectRowIndexes(wanted, byExtendingSelection: false) }
            if let request = editor.focusRequest, request.serial != focusSerial, request.row >= 0,
               let column = parent.columns.firstIndex(where: { $0.field == request.field && $0.editable }) {
                focusSerial = request.serial
                guard request.row < table.numberOfRows else { return }
                let shown = table.column(withIdentifier: NSUserInterfaceItemIdentifier(RecordFieldLabel.key(parent.columns[column].field)))
                DispatchQueue.main.async { [weak self] in
                    self?.table?.scrollRowToVisible(request.row)
                    self?.editCell(column: shown, row: request.row)
                }
            }
        }

        /// The cell whose text field is being edited, which a reload must not replace.
        private var editingCell: (row: Int, column: Int)? {
            guard let table, let fieldEditor = table.window?.firstResponder as? NSTextView, fieldEditor.isFieldEditor,
                  let text = fieldEditor.delegate as? NSTextField, text.isDescendant(of: table) else { return nil }
            return (table.row(for: text), table.column(for: text))
        }

        private func refreshVisibleCells() {
            guard let table else { return }
            let edited = editingCell
            for row in 0..<table.numberOfRows {
                for (index, _) in table.tableColumns.enumerated() {
                    if let cell = table.view(atColumn: index, row: row, makeIfNecessary: false) as? NSTableCellView {
                        configure(cell, row: row, column: index,
                                  updateText: !(row == edited?.row && index == edited?.column))
                    }
                }
            }
        }

        func numberOfRows(in tableView: NSTableView) -> Int { editor.rowCount }

        func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
            guard let tableColumn else { return nil }
            let cell = (tableView.makeView(withIdentifier: tableColumn.identifier, owner: self) as? NSTableCellView) ?? makeCell(tableColumn)
            configure(cell, row: row, column: tableView.column(withIdentifier: tableColumn.identifier))
            return cell
        }

        private func makeCell(_ tableColumn: NSTableColumn) -> NSTableCellView {
            let cell = NSTableCellView()
            cell.identifier = tableColumn.identifier
            let text = NSTextField(string: "")
            text.isBordered = false
            text.drawsBackground = false
            text.lineBreakMode = .byTruncatingTail
            text.cell?.isScrollable = true
            text.delegate = self
            text.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(text)
            cell.textField = text
            NSLayoutConstraint.activate([
                text.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
                text.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -2),
                text.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
            return cell
        }

        private func configure(_ cell: NSTableCellView, row: Int, column index: Int, updateText: Bool = true) {
            guard let table, table.tableColumns.indices.contains(index), let text = cell.textField,
                  let column = parent.columns.first(where: { RecordFieldLabel.key($0.field) == table.tableColumns[index].identifier.rawValue })
            else { return }
            let value = editor.rowField(row, column.field)
            let problems = editor.problems(at: column.field, row: row)
            if updateText { text.stringValue = column.field == SPIELORDERS_ITEM_FIELD_TOTAL ? Money.display(value) : value }
            text.isEditable = column.editable && isEnabled && text.currentEditor() != nil
            text.isSelectable = text.isEditable
            text.alignment = column.numeric ? .right : .natural
            text.textColor = problems.isEmpty ? (column.editable ? .labelColor : .secondaryLabelColor) : .systemRed
            text.toolTip = problems.isEmpty ? nil : problems.map(\.message).joined(separator: "\n")
            text.setAccessibilityLabel(RecordFieldLabel.of(column.field))
            text.setAccessibilityIdentifier("\(parent.identifier).row.\(row).\(RecordFieldLabel.key(column.field))")
            if !problems.isEmpty { text.setAccessibilityHelp(text.toolTip) } else { text.setAccessibilityHelp(nil) }
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !synchronizing, let table else { return }
            editor.selectedRows = table.selectedRowIndexes
        }

        // Editing writes each change into the draft as typed.
        func controlTextDidChange(_ notification: Notification) {
            guard let table, let text = notification.object as? NSTextField, let field = field(of: text) else { return }
            let row = table.row(for: text)
            guard row >= 0 else { return }
            editor.setRow(row, field, text.stringValue)
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            guard let text = notification.object as? NSTextField else { return }
            text.isEditable = false
            text.isSelectable = false
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard let table, let text = control as? NSTextField else { return false }
            let editable = table.tableColumns.indices.filter { index in
                parent.columns.first { RecordFieldLabel.key($0.field) == table.tableColumns[index].identifier.rawValue }?.editable == true
            }
            let row = table.row(for: text)
            let column = table.column(for: text)
            guard let position = editable.firstIndex(of: column) else { return false }
            let next: (Int, Int)?
            switch selector {
            case #selector(NSResponder.insertTab(_:)):
                next = position + 1 < editable.count ? (row, editable[position + 1])
                    : row + 1 < table.numberOfRows ? (row + 1, editable[0]) : nil
            case #selector(NSResponder.insertBacktab(_:)):
                next = position > 0 ? (row, editable[position - 1])
                    : row > 0 ? (row - 1, editable[editable.count - 1]) : nil
            case #selector(NSResponder.cancelOperation(_:)):
                table.window?.makeFirstResponder(table)
                return true
            default: return false
            }
            guard let next else { return false }
            table.window?.makeFirstResponder(table)
            table.selectRowIndexes([next.0], byExtendingSelection: false)
            editCell(column: next.1, row: next.0)
            return true
        }

        private func field(of text: NSTextField) -> Int32? {
            guard let table else { return nil }
            let index = table.column(for: text)
            guard table.tableColumns.indices.contains(index) else { return nil }
            return parent.columns.first { RecordFieldLabel.key($0.field) == table.tableColumns[index].identifier.rawValue }?.field
        }

        func beginEditingSelectedRow() {
            guard isEnabled, let table, table.selectedRow >= 0,
                  let first = table.tableColumns.firstIndex(where: { tableColumn in
                      parent.columns.first { RecordFieldLabel.key($0.field) == tableColumn.identifier.rawValue }?.editable == true
                  }) else { return }
            editCell(column: first, row: table.selectedRow)
        }

        @objc func editClickedCell(_ table: NSTableView) {
            editCell(column: table.clickedColumn, row: table.clickedRow)
        }

        private func editCell(column: Int, row: Int) {
            guard isEnabled, let table, row >= 0, row < table.numberOfRows,
                  table.tableColumns.indices.contains(column),
                  parent.columns.first(where: { RecordFieldLabel.key($0.field) == table.tableColumns[column].identifier.rawValue })?.editable == true,
                  let cell = table.view(atColumn: column, row: row, makeIfNecessary: true) as? NSTableCellView,
                  let text = cell.textField else { return }
            text.isEditable = true
            text.isSelectable = true
            table.editColumn(column, row: row, with: nil, select: true)
        }

        func deleteSelectedRows() { if isEnabled { editor.removeSelectedRows() } }

        func tableView(_ tableView: NSTableView, pasteboardWriterForRow row: Int) -> (any NSPasteboardWriting)? {
            guard isEnabled else { return nil }
            let item = NSPasteboardItem()
            item.setString(String(row), forType: .rowIndex)
            return item
        }

        func tableView(_ tableView: NSTableView, validateDrop info: any NSDraggingInfo, proposedRow row: Int,
                       proposedDropOperation dropOperation: NSTableView.DropOperation) -> NSDragOperation {
            guard isEnabled, info.draggingSource as? NSTableView === tableView else { return [] }
            if dropOperation == .on { tableView.setDropRow(row, dropOperation: .above) }
            return .move
        }

        func tableView(_ tableView: NSTableView, acceptDrop info: any NSDraggingInfo, row: Int,
                       dropOperation: NSTableView.DropOperation) -> Bool {
            guard isEnabled, info.draggingSource as? NSTableView === tableView,
                  let items = info.draggingPasteboard.pasteboardItems else { return false }
            let sources = IndexSet(items.compactMap { $0.string(forType: .rowIndex).flatMap(Int.init) })
            guard sources.count == items.count else { return false }
            editor.moveRows(sources, above: row)
            return true
        }
    }
}

private extension NSPasteboard.PasteboardType {
    static let rowIndex = NSPasteboard.PasteboardType("com.spielsoft.SpielOrders.draft-row")
}

/// Return begins editing the selected row; Delete removes the selected rows.
final class RowsTableView: NSTableView {
    weak var coordinator: DraftRowsTable.Coordinator?

    override func keyDown(with event: NSEvent) {
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else {
            super.keyDown(with: event)
            return
        }
        switch event.keyCode {
        case 36, 76: coordinator?.beginEditingSelectedRow()
        case 51, 117: coordinator?.deleteSelectedRows()
        default: super.keyDown(with: event)
        }
    }
}

enum Money {
    private static let format: NumberFormatter = {
        let format = NumberFormatter()
        format.numberStyle = .decimal
        format.minimumFractionDigits = 2
        format.maximumFractionDigits = 2
        return format
    }()

    /// The display precision of an unrounded library amount.
    static func display(_ value: String) -> String {
        guard !value.isEmpty else { return "" }
        let number = NSDecimalNumber(string: value, locale: Locale(identifier: "en_US_POSIX"))
        return number == .notANumber ? value : format.string(from: number) ?? value
    }
}
