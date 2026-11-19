import AppKit
import CSpielOrders
import SwiftUI

/// The rows of a draft as a plain spreadsheet: Items of an Order or Contacts of a
/// Vendor. Double-click or Return edits a cell, Tab advances, and dragging reorders.
/// Optional columns are chosen from the header's menu; multiline text is edited in a popover.
/// The table grows with its rows, to a limit past which it scrolls when it has one.
struct DraftRowsTable: NSViewRepresentable {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    let columns: [RowColumn]
    let label: String
    let identifier: String
    /// What the table says while it has no rows, such as “No Items”.
    let placeholder: String
    /// The currency money columns are shown in.
    var currency = ""
    /// The most rows the table's height shows before it scrolls; nil shows every row.
    var rowLimit: Int? = 10

    /// The fewest rows the table's height shows, room for a first row.
    private static let leastRows = 2

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView scroll: NSScrollView, context: Context) -> CGSize? {
        guard let table = scroll.documentView as? NSTableView else { return nil }
        let rows = min(max(editor.rowCount, Self.leastRows), rowLimit ?? .max)
        let header = table.headerView?.frame.height ?? 0
        let content = NSSize(width: proposal.width ?? table.frame.width,
                             height: header + CGFloat(rows) * (table.rowHeight + table.intercellSpacing.height))
        // A legacy scroller takes room only while it shows; an overlay scroller never does.
        let scroller = scroll.horizontalScroller?.isHidden == false ? NSScroller.self : nil
        let frame = NSScrollView.frameSize(forContentSize: content,
                                           horizontalScrollerClass: scroller,
                                           verticalScrollerClass: nil, borderType: scroll.borderType,
                                           controlSize: .regular, scrollerStyle: scroll.scrollerStyle)
        return CGSize(width: content.width, height: frame.height)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let table = RowsTableView()
        table.coordinator = context.coordinator
        table.registerForDraggedTypes([.rowIndex])
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        table.delegate = context.coordinator
        table.dataSource = context.coordinator
        table.target = context.coordinator
        table.doubleAction = #selector(Coordinator.editClickedCell(_:))
        context.coordinator.table = table
        let scroll = RowColumn.scrollView(for: table, columns: columns, label: label, identifier: identifier)
        scroll.placeholder.stringValue = placeholder
        scroll.placeholder.setAccessibilityIdentifier("\(identifier).placeholder")
        if columns.contains(where: \.optional) {
            table.headerView?.menu = context.coordinator.columnsMenu
        }
        if columns.contains(where: \.multiline) {
            table.menu = context.coordinator.rowMenu
        }
        return scroll
    }

    static func dismantleNSView(_ scroll: NSScrollView, coordinator: Coordinator) {
        coordinator.closeNotes()
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        context.coordinator.isEnabled = context.environment.isEnabled
        if let manager = scroll.undoManager { editor.undoManager = manager }
        context.coordinator.update()
    }

    @MainActor final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate,
                                        NSMenuDelegate {
        var parent: DraftRowsTable
        weak var table: NSTableView?
        var isEnabled = true
        let columnsMenu = NSMenu(title: "Columns")
        let rowMenu = NSMenu(title: "Row")
        private var notes: NotesPopover?
        private var problem: NSPopover?
        private var revision = -1
        private var rowsRevision = -1
        private var focusSerial = 0
        private var synchronizing = false

        init(_ parent: DraftRowsTable) {
            self.parent = parent
            super.init()
            columnsMenu.delegate = self
            rowMenu.delegate = self
        }

        private var editor: RecordEditor { parent.editor }
        private weak var activeText: NSTextField?
        private var activeCell: (row: Int, field: Int32, revision: Int)?

        func update() {
            guard let table else { return }
            synchronizing = true
            defer { synchronizing = false }
            (table.enclosingScrollView as? RowsScrollView)?.placeholder.isHidden = editor.rowCount > 0
            // An empty table is plain behind its placeholder rather than striped.
            table.usesAlternatingRowBackgroundColors = editor.rowCount > 0
            table.isEnabled = isEnabled
            notes?.isEditable = isEnabled
            if !isEnabled, editingCell != nil { table.window?.makeFirstResponder(table) }
            if rowsRevision != editor.rowsRevision {
                // Rows moved under any cell being edited, so that edit ends first.
                if editingCell != nil { table.window?.makeFirstResponder(table) }
                closeNotes()
                rowsRevision = editor.rowsRevision
                revision = editor.revision
                table.reloadData()
                fitColumns()
            } else if revision != editor.revision {
                revision = editor.revision
                // A value changed, as a save or another field leaves it: the rows keep their
                // views, and the cells below redraw in place, so the table never flickers.
                if editingCell == nil {
                    if table.numberOfRows != editor.rowCount { table.reloadData() }
                    fitColumns()
                }
            }
            refreshVisibleCells()
            let wanted = editor.selectedRows.filteredIndexSet { $0 < table.numberOfRows }
            if table.selectedRowIndexes != wanted { table.selectRowIndexes(wanted, byExtendingSelection: false) }
            if isEnabled, let request = editor.focusRequest, request.serial != focusSerial, request.row >= 0,
               let column = parent.columns.firstIndex(where: { $0.field == request.field && $0.editable }) {
                focusSerial = request.serial
                guard request.row < table.numberOfRows else { return }
                let shown = table.column(withIdentifier: NSUserInterfaceItemIdentifier(RecordFieldLabel.key(parent.columns[column].field)))
                if let edited = editingCell, edited.row == request.row, edited.column == shown { return }
                DispatchQueue.main.async { [weak self] in
                    guard let self, isEnabled, let current = editor.focusRequest,
                          current.serial == request.serial, current.field == request.field,
                          current.row == request.row else { return }
                    if let edited = editingCell, edited.row == request.row, edited.column == shown { return }
                    table.scrollRowToVisible(request.row)
                    editCell(column: shown, row: request.row)
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
                for index in table.tableColumns.indices {
                    let cell = table.view(atColumn: index, row: row, makeIfNecessary: false)
                    if let indicator = cell as? NotesIndicatorCell {
                        configure(indicator, row: row)
                    } else if let cell = cell as? RowCell {
                        configure(cell, row: row, column: index,
                                  updateText: !(row == edited?.row && index == edited?.column))
                    }
                }
            }
        }

        /// Sizes each column that is not flexible to its widest text, its title included, up to
        /// its width, and gives the flexible columns the rest.
        private func fitColumns() {
            guard let table else { return }
            let measure = NSTextFieldCell(textCell: "")
            for (index, column) in table.tableColumns.enumerated() {
                guard let definition = definition(index), !definition.flexible else { continue }
                let widest = (0..<editor.rowCount).map { row in
                    measure.stringValue = definition.display(editor.rowField(row, definition.field),
                                                             currency: parent.currency)
                    return measure.cellSize.width + RowCell.inset
                }.max() ?? 0
                column.width = min(max(widest, column.headerCell.cellSize.width), definition.width)
            }
            table.sizeToFit()
        }

        /// The definition of the table's column at `index`; none for the notes indicator.
        private func definition(_ index: Int) -> RowColumn? {
            guard let table, table.tableColumns.indices.contains(index) else { return nil }
            let key = table.tableColumns[index].identifier.rawValue
            return parent.columns.first { RecordFieldLabel.key($0.field) == key }
        }

        /// The shown columns a user edits, in display order.
        private var editableColumns: [Int] {
            guard let table else { return [] }
            return table.tableColumns.indices.filter { index in
                !table.tableColumns[index].isHidden && definition(index)?.editable == true
            }
        }

        func numberOfRows(in tableView: NSTableView) -> Int { editor.rowCount }

        func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
            guard let tableColumn else { return nil }
            if tableColumn.identifier == NotesIndicatorCell.identifier {
                let reused = tableView.makeView(withIdentifier: tableColumn.identifier, owner: self)
                let cell = reused as? NotesIndicatorCell
                    ?? NotesIndicatorCell(target: self, action: #selector(openNotes(_:)))
                configure(cell, row: row)
                return cell
            }
            let reused = tableView.makeView(withIdentifier: tableColumn.identifier, owner: self) as? RowCell
            let cell = reused ?? RowCell(identifier: tableColumn.identifier, delegate: self,
                                         target: self, action: #selector(showProblem(_:)))
            configure(cell, row: row, column: tableView.column(withIdentifier: tableColumn.identifier))
            return cell
        }

        private func configure(_ cell: RowCell, row: Int, column index: Int, updateText: Bool = true) {
            guard let text = cell.textField, let column = definition(index) else { return }
            let problems = editor.statements(at: column.field, row: row)
            let value = editor.rowField(row, column.field)
            if updateText { text.stringValue = column.display(value, currency: parent.currency) }
            text.isEditable = column.editable && isEnabled && text.currentEditor() != nil
            text.isSelectable = text.isEditable
            column.style(text, row: row, table: parent.identifier)
            let message = problems.isEmpty ? nil : problems.joined(separator: "\n")
            if let message {
                text.textColor = .problemText
                text.toolTip = message
            } else if column.multiline, !value.isEmpty {
                text.toolTip = value
            } else {
                text.toolTip = nil
            }
            text.setAccessibilityHelp(message)
            let key = RecordFieldLabel.key(column.field)
            cell.mark(message, label: "\(RecordFieldLabel.of(column.field)) problem",
                      identifier: "\(parent.identifier).row.\(row).\(key).problem")
        }

        /// Opens the popover of the problem marked in the clicked cell, with what fixes it.
        @objc func showProblem(_ sender: NSButton) {
            guard let table else { return }
            let row = table.row(for: sender)
            guard row >= 0, let column = definition(table.column(for: sender)) else { return }
            let problems = editor.statements(at: column.field, row: row)
            guard !problems.isEmpty else { return }
            problem?.close()
            problem = sender.showNotice(level: .problem, title: RecordFieldLabel.of(column.field), lines: problems,
                                        actions: parent.model.repairs(for: editor))
        }

        private func configure(_ cell: NotesIndicatorCell, row: Int) {
            guard let column = parent.columns.first(where: \.multiline) else { return }
            let key = RecordFieldLabel.key(column.field)
            let identifier = "\(parent.identifier).row.\(row).\(key).indicator"
            cell.show(editor.rowField(row, column.field), identifier: identifier)
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !synchronizing, let table else { return }
            editor.selectedRows = table.selectedRowIndexes
        }

        // Editing writes each change into the draft as typed.
        func controlTextDidBeginEditing(_ notification: Notification) {
            guard let table, let text = notification.object as? NSTextField, let field = field(of: text) else { return }
            let row = table.row(for: text)
            if row >= 0 {
                activeText = text
                activeCell = (row, field, editor.rowsRevision)
                editor.trackFocus(field, row: row)
            }
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let text = notification.object as? NSTextField, activeText === text,
                  let cell = activeCell, cell.revision == editor.rowsRevision, cell.row < editor.rowCount else { return }
            editor.setRow(cell.row, cell.field, text.stringValue)
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            guard let text = notification.object as? NSTextField else { return }
            let cell = activeText === text ? activeCell : nil
            if activeText === text { activeText = nil; activeCell = nil }
            let valid = cell.map { $0.revision == editor.rowsRevision && $0.row < editor.rowCount } ?? false
            if let cell, valid {
                editor.setRow(cell.row, cell.field, text.stringValue)
                if let column = parent.columns.first(where: { $0.field == cell.field }) {
                    text.stringValue = column.display(editor.rowField(cell.row, cell.field), currency: parent.currency)
                }
            }
            text.isEditable = false
            text.isSelectable = false
            text.truncates = true
            if valid { editor.finishField() }
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard let table, let text = control as? NSTextField else { return false }
            let row = table.row(for: text)
            let column = table.column(for: text)
            switch selector {
            case #selector(NSResponder.insertTab(_:)):
                return advance(fromRow: row, column: column, forward: true)
            case #selector(NSResponder.insertBacktab(_:)):
                return advance(fromRow: row, column: column, forward: false)
            case #selector(NSResponder.cancelOperation(_:)):
                table.window?.makeFirstResponder(table)
                return true
            default:
                return false
            }
        }

        /// Edits the next or previous editable cell, continuing on the next or previous row.
        private func advance(fromRow row: Int, column: Int, forward: Bool) -> Bool {
            guard let table else { return false }
            let editable = editableColumns
            guard let position = editable.firstIndex(of: column) else { return false }
            let next: (row: Int, column: Int)?
            if forward {
                next = position + 1 < editable.count ? (row, editable[position + 1])
                    : row + 1 < table.numberOfRows ? (row + 1, editable[0]) : nil
            } else {
                next = position > 0 ? (row, editable[position - 1])
                    : row > 0 ? (row - 1, editable[editable.count - 1]) : nil
            }
            guard let next else { return false }
            table.window?.makeFirstResponder(table)
            table.selectRowIndexes([next.row], byExtendingSelection: false)
            if let field = definition(next.column)?.field {
                editor.requestFocus(field, row: next.row)
                focusSerial = editor.focusRequest?.serial ?? focusSerial
                editCell(column: next.column, row: next.row)
            }
            return true
        }

        private func field(of text: NSTextField) -> Int32? {
            guard let table else { return nil }
            return definition(table.column(for: text))?.field
        }

        func editSelection() {
            guard isEnabled, let table, table.selectedRow >= 0, let first = editableColumns.first else { return }
            editCell(column: first, row: table.selectedRow)
        }

        /// Opens the selected row's Notes, where the rows have them.
        func editSelectedNotes() {
            guard isEnabled, parent.columns.contains(where: \.multiline), let row = table?.selectedRow, row >= 0 else {
                NSSound.beep()
                return
            }
            editNotes(row: row)
        }

        @objc func editClickedCell(_ table: NSTableView) {
            let clicked = table.tableColumns.indices.contains(table.clickedColumn)
                ? table.tableColumns[table.clickedColumn] : nil
            if clicked?.identifier == NotesIndicatorCell.identifier {
                editNotes(row: table.clickedRow)
            } else {
                editCell(column: table.clickedColumn, row: table.clickedRow)
            }
        }

        private func editCell(column: Int, row: Int) {
            guard isEnabled, let table, row >= 0, row < table.numberOfRows,
                  let definition = definition(column), definition.editable else { return }
            if definition.multiline {
                editNotes(row: row)
                return
            }
            guard let cell = table.view(atColumn: column, row: row, makeIfNecessary: true) as? NSTableCellView,
                  let text = cell.textField else { return }
            // A cell being edited holds the value exactly as typed, not as displayed.
            text.stringValue = editor.rowField(row, definition.field)
            text.isEditable = true
            text.isSelectable = true
            text.truncates = false
            table.editColumn(column, row: row, with: nil, select: true)
            if text.currentEditor() != nil { editor.trackFocus(definition.field, row: row) }
        }

        @objc func openNotes(_ sender: NSView) {
            editNotes(row: table?.row(for: sender) ?? -1)
        }

        /// Opens the row's multiline text in a popover beside its cell, or beside the
        /// indicator while its column is hidden. Text is written as typed and commits on close.
        private func editNotes(row: Int) {
            guard isEnabled, let table, row >= 0, row < table.numberOfRows,
                  let column = parent.columns.first(where: \.multiline) else { return }
            let key = RecordFieldLabel.key(column.field)
            let shown = table.column(withIdentifier: NSUserInterfaceItemIdentifier(key))
            let anchor = shown >= 0 && !table.tableColumns[shown].isHidden
                ? shown : table.column(withIdentifier: NotesIndicatorCell.identifier)
            guard anchor >= 0 else { return }
            closeNotes()
            table.selectRowIndexes([row], byExtendingSelection: false)
            let popover = NotesPopover(row: row, rowsRevision: editor.rowsRevision,
                                       notes: editor.rowField(row, column.field),
                                       identifier: "\(parent.identifier).row.\(row).\(key).editor")
            popover.onChange = { [weak self, weak popover] text in
                guard let self, let popover, current(popover) else { return }
                editor.setRow(popover.row, column.field, text)
            }
            popover.onTab = { [weak self, weak popover] forward in
                guard let self, let popover else { return }
                popover.close()
                _ = advance(fromRow: popover.row, column: shown, forward: forward)
            }
            popover.onClose = { [weak self, weak popover] in
                guard let self, let popover else { return }
                if notes === popover { notes = nil }
                if current(popover) { editor.finishField() }
            }
            notes = popover
            popover.show(relativeTo: table.frameOfCell(atColumn: anchor, row: row), of: table)
        }

        /// Whether the popover's row still holds the Item it was opened for.
        private func current(_ popover: NotesPopover) -> Bool {
            popover.rowsRevision == editor.rowsRevision && popover.row < editor.rowCount
        }

        func closeNotes() {
            notes?.close()
            problem?.close()
        }

        func menuNeedsUpdate(_ menu: NSMenu) {
            guard let table else { return }
            menu.removeAllItems()
            if menu === columnsMenu {
                for column in parent.columns where column.optional {
                    let key = RecordFieldLabel.key(column.field)
                    let item = NSMenuItem(title: RecordFieldLabel.of(column.field),
                                          action: #selector(toggleColumn(_:)), keyEquivalent: "")
                    item.target = self
                    item.representedObject = key
                    let shown = table.tableColumn(withIdentifier: NSUserInterfaceItemIdentifier(key))?.isHidden == false
                    item.state = shown ? .on : .off
                    item.setAccessibilityIdentifier("\(parent.identifier).column.visibility.\(key)")
                    menu.addItem(item)
                }
            } else if isEnabled, let row = [table.clickedRow, table.selectedRow].first(where: { $0 >= 0 }) {
                // A menu opened from the keyboard or VoiceOver has no clicked row; it acts on the selected one.
                let item = NSMenuItem(title: "Edit Notes…", action: #selector(editClickedNotes(_:)),
                                      keyEquivalent: "\r")
                item.keyEquivalentModifierMask = .option
                item.target = self
                item.representedObject = row
                item.setAccessibilityIdentifier("\(parent.identifier).row.\(row).editNotes")
                menu.addItem(item)
            }
        }

        @objc private func toggleColumn(_ sender: NSMenuItem) {
            guard let key = sender.representedObject as? String,
                  let column = table?.tableColumn(withIdentifier: NSUserInterfaceItemIdentifier(key)) else { return }
            column.isHidden.toggle()
        }

        @objc private func editClickedNotes(_ sender: NSMenuItem) {
            guard let row = sender.representedObject as? Int else { return }
            editNotes(row: row)
        }

        func deleteSelection() {
            guard isEnabled else { return }
            table?.window?.makeFirstResponder(table)
            if !parent.model.deferNavigation(afterFailure: true, { [editor] in editor.removeSelection() }) {
                editor.removeSelection()
            }
        }

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
            tableView.window?.makeFirstResponder(tableView)
            if !parent.model.deferNavigation(afterFailure: true, { [editor] in editor.moveRows(sources, above: row) }) {
                editor.moveRows(sources, above: row)
            }
            return true
        }
    }
}

/// A column of a draft's row table.
struct RowColumn {
    let field: Int32
    /// The width a flexible column starts at, or the most that a column sized to its text takes.
    let width: CGFloat
    var editable = true
    var numeric = false
    /// An amount, shown as money in the table's currency.
    var money = false
    var flexible = false
    /// Multiline text, edited in a popover; its rows are marked by an indicator column.
    var multiline = false
    /// Hidden until chosen from the header's column menu.
    var optional = false

    /// Money in `currency`, multiline text on one line, and every other value as the library
    /// gives it.
    func display(_ value: String, currency: String) -> String {
        if money { return Money.display(value, currency: currency) }
        if multiline { return value.split(separator: "\n").joined(separator: " ¶ ") }
        return value
    }

    @MainActor func style(_ text: NSTextField, row: Int, table identifier: String) {
        text.alignment = numeric ? .right : .natural
        text.textColor = editable ? .labelColor : .secondaryLabelColor
        text.setAccessibilityLabel(RecordFieldLabel.of(field))
        text.setAccessibilityIdentifier("\(identifier).row.\(row).\(RecordFieldLabel.key(field))")
    }

    /// The table's look and columns, inside the scroll view it is shown in.
    @MainActor static func scrollView(for table: NSTableView, columns: [RowColumn], label: String,
                                      identifier: String) -> RowsScrollView {
        table.setAccessibilityLabel(label)
        table.setAccessibilityIdentifier(identifier)
        table.usesAlternatingRowBackgroundColors = true
        table.rowHeight = 22
        // A truncated value shows in full while the pointer rests on it.
        table.allowsExpansionToolTips = true
        table.allowsMultipleSelection = true
        table.allowsEmptySelection = true
        table.allowsColumnReordering = false
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        if columns.contains(where: \.multiline) {
            table.addTableColumn(NotesIndicatorCell.column())
        }
        for column in columns {
            table.addTableColumn(column.tableColumn(in: identifier))
        }
        if columns.contains(where: \.optional) {
            // Column visibility and widths are remembered per user, like the Orders table's.
            table.autosaveName = identifier
            table.autosaveTableColumns = true
        }
        let scroll = RowsScrollView()
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .bezelBorder
        scroll.documentView = table
        return scroll
    }

    @MainActor private func tableColumn(in identifier: String) -> NSTableColumn {
        let key = RecordFieldLabel.key(field)
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(key))
        column.title = RecordFieldLabel.of(field)
        column.headerCell.setAccessibilityIdentifier("\(identifier).column.\(key)")
        column.width = width
        // Flexible columns hold most of the text. The default columns fit the narrowest detail;
        // an optional column shown beyond it scrolls the table rather than squeezing them.
        column.minWidth = flexible ? 120 : 50
        column.resizingMask = flexible ? [.autoresizingMask, .userResizingMask] : .userResizingMask
        column.isHidden = optional
        if numeric { column.headerCell.alignment = .right }
        return column
    }
}

/// The scroll view of a draft's rows, with quiet text centered over the rows while there are none.
final class RowsScrollView: NSScrollView {
    let placeholder = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        placeholder.textColor = .secondaryLabelColor
        addSubview(placeholder)
    }

    required init?(coder: NSCoder) { nil }

    override func tile() {
        super.tile()
        // The scroll view is flipped, so the rows lie below the header's bottom edge.
        var rows = contentView.frame
        if let header = (documentView as? NSTableView)?.headerView {
            let edge = convert(header.bounds, from: header).maxY
            rows = rows.divided(atDistance: max(edge - rows.minY, 0), from: .minYEdge).remainder
        }
        placeholder.sizeToFit()
        let size = placeholder.frame.size
        placeholder.setFrameOrigin(NSPoint(x: rows.midX - size.width / 2, y: rows.midY - size.height / 2))
    }
}

/// A row's cell: its text, with a problem symbol at the trailing edge while the last refused
/// save located a problem there.
final class RowCell: NSTableCellView {
    private let symbol: NSButton
    /// Where the text ends without the symbol, and beside it.
    private let plainEnd: NSLayoutConstraint
    private let markedEnd: NSLayoutConstraint

    init(identifier: NSUserInterfaceItemIdentifier, delegate: NSTextFieldDelegate, target: AnyObject,
         action: Selector) {
        let text = NSTextField(string: "")
        symbol = NSButton(image: NoticeLevel.problem.image, target: target, action: action)
        plainEnd = text.trailingAnchor.constraint(equalTo: symbol.trailingAnchor)
        markedEnd = text.trailingAnchor.constraint(equalTo: symbol.leadingAnchor, constant: -2)
        super.init(frame: .zero)
        self.identifier = identifier
        text.isBordered = false
        text.drawsBackground = false
        text.truncates = true
        text.delegate = delegate
        text.translatesAutoresizingMaskIntoConstraints = false
        symbol.isBordered = false
        symbol.isHidden = true
        symbol.translatesAutoresizingMaskIntoConstraints = false
        symbol.setContentHuggingPriority(.required, for: .horizontal)
        addSubview(text)
        addSubview(symbol)
        textField = text
        NSLayoutConstraint.activate([
            text.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
            text.centerYAnchor.constraint(equalTo: centerYAnchor),
            symbol.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -2),
            symbol.centerYAnchor.constraint(equalTo: centerYAnchor),
            plainEnd,
        ])
    }

    required init?(coder: NSCoder) { nil }

    /// The room around a cell's text: its margins and the text field's own.
    static let inset: CGFloat = 8

    /// Shows the symbol with `message` as its tooltip, or hides it for nil.
    func mark(_ message: String?, label: String, identifier: String) {
        symbol.isHidden = message == nil
        symbol.toolTip = message
        symbol.setAccessibilityLabel(label)
        symbol.setAccessibilityValue(message)
        symbol.setAccessibilityIdentifier(identifier)
        let marked = message != nil
        NSLayoutConstraint.deactivate([marked ? plainEnd : markedEnd])
        NSLayoutConstraint.activate([marked ? markedEnd : plainEnd])
    }
}

private extension NSTextField {
    /// Whether the text ends in an ellipsis where it overflows, rather than scrolling to follow
    /// the insertion point as it does while being edited.
    var truncates: Bool {
        get { lineBreakMode == .byTruncatingTail }
        set {
            cell?.isScrollable = !newValue
            lineBreakMode = newValue ? .byTruncatingTail : .byClipping
        }
    }
}

/// The narrow leading cell that marks a row with notes; clicking it opens them.
final class NotesIndicatorCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("notesIndicator")
    private let button: NSButton

    init(target: AnyObject, action: Selector) {
        let image = NSImage(systemSymbolName: "note.text", accessibilityDescription: "Notes") ?? NSImage()
        button = NSButton(image: image, target: target, action: action)
        super.init(frame: .zero)
        identifier = Self.identifier
        button.isBordered = false
        button.contentTintColor = .secondaryLabelColor
        button.setAccessibilityLabel("Notes")
        button.translatesAutoresizingMaskIntoConstraints = false
        addSubview(button)
        NSLayoutConstraint.activate([
            button.centerXAnchor.constraint(equalTo: centerXAnchor),
            button.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { nil }

    /// Shows the mark only for a row with notes, with the notes as its tooltip.
    func show(_ notes: String, identifier: String) {
        button.isHidden = notes.isEmpty
        button.toolTip = notes.isEmpty ? nil : notes
        button.setAccessibilityIdentifier(identifier)
    }

    static func column() -> NSTableColumn {
        let column = NSTableColumn(identifier: identifier)
        column.title = ""
        column.headerToolTip = "Notes"
        column.width = 20
        column.minWidth = 20
        column.maxWidth = 20
        column.resizingMask = []
        return column
    }
}

private extension NSPasteboard.PasteboardType {
    static let rowIndex = NSPasteboard.PasteboardType("com.spielsoft.SpielOrders.draft-row")
}

/// Return begins editing the selected row, Option-Return opens its Notes, and Delete removes
/// the selected rows.
final class RowsTableView: NSTableView {
    weak var coordinator: DraftRowsTable.Coordinator?

    override func keyDown(with event: NSEvent) {
        let modifiers = event.modifierFlags.intersection([.command, .control, .option, .shift])
        if modifiers == .option, [36, 76].contains(event.keyCode) {
            coordinator?.editSelectedNotes()
            return
        }
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else {
            super.keyDown(with: event)
            return
        }
        switch event.keyCode {
        case 36, 76: coordinator?.editSelection()
        case 51, 117: coordinator?.deleteSelection()
        default: super.keyDown(with: event)
        }
    }
}
