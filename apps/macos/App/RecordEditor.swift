import AppKit
import CSpielOrders
import Observation
import SpielOrdersKit

/// One open editor over a library Draft. Swift keeps only presentation state: the
/// baseline to tell a dirty draft, what the last save refused or warned about, and focus.
@MainActor @Observable final class RecordEditor {
    private(set) var draft: RecordDraft
    private var baseline: RecordValues
    private var written = false
    /// Changes whenever the draft changes, so views read it again.
    private(set) var revision = 0
    /// Changes when row structure changes; adopting a saved read keeps the active cell.
    private(set) var rowsRevision = 0
    private(set) var problems: [DraftProblem] = []
    /// Why the last save wrote nothing, with the library's result; nil once nothing is refused.
    private(set) var failure: Notice?
    /// The warnings of the saves since the owner last started one.
    private(set) var warnings: [Notice] = []
    /// What a save did to a field that the owner should notice, until the next edit.
    private(set) var consequences: [Int32: String] = [:]
    private var needsReopen = false
    var onFinish: (() -> Void)?
    private var undoPending = false
    private var changeRegistered = false
    private var saving = false
    private var submitted: RecordValues?
    private var submittedTotal = ""
    private var input: [Input] = []
    private var finishPending = false
    private var inputChanged = false
    /// The field and row to focus, with a serial so the same location can be asked twice.
    private(set) var focusRequest: (field: Int32, row: Int, serial: Int)?
    var selectedRows = IndexSet()
    @ObservationIgnored weak var undoManager: UndoManager?
    @ObservationIgnored weak var window: NSWindow?

    init(_ draft: RecordDraft) {
        self.draft = draft
        baseline = RecordValues(draft)
    }

    var kind: RecordDraft.Kind { draft.kind }
    var refused: Bool { failure != nil }
    var isNew: Bool { draft.isNew && !written }
    var dirty: Bool {
        _ = revision
        return !input.isEmpty || (submitted ?? RecordValues(draft)) != baseline
    }

    /// Asks the form to focus its first field.
    func focus() {
        let field = kind == .order ? SPIELORDERS_ORDER_FIELD_TITLE : SPIELORDERS_VENDOR_FIELD_NAME
        requestFocus(field, row: -1)
    }

    func requestFocus(_ field: Int32, row: Int) {
        focusRequest = (field, row, (focusRequest?.serial ?? 0) + 1)
    }

    func trackFocus(_ field: Int32, row: Int) {
        guard focusRequest?.field != field || focusRequest?.row != row else { return }
        // Observing native focus must not ask the control to focus/select itself again.
        focusRequest = (field, row, focusRequest?.serial ?? 0)
    }

    func field(_ field: Int32) -> String {
        _ = revision
        for value in input.reversed() {
            switch value {
            case let .value(key, nil, text) where key == field: return text
            case let .restore(values): return values.fields[field] ?? ""
            default: continue
            }
        }
        if let submitted { return submitted.fields[field] ?? "" }
        return draft.field(field)
    }

    func set(_ field: Int32, _ text: String) {
        if submitted != nil { buffer(.value(field: field, row: nil, text: text)); return }
        // Naming the same value still carries intent to the library, for example keeping
        // an Order Group while changing Project. It need not redraw unchanged text.
        guard self.field(field) != text else {
            do { try draft.set(field, to: text) } catch { NSSound.beep() }
            return
        }
        apply { try draft.set(field, to: text) }
        if saving { inputChanged = true }
    }

    var rowCount: Int {
        _ = revision
        for value in input.reversed() {
            if case let .restore(values) = value { return values.rows.count }
        }
        return submitted?.rows.count ?? draft.rowCount
    }

    func rowField(_ row: Int, _ field: Int32) -> String {
        _ = revision
        for value in input.reversed() {
            switch value {
            case let .value(key, index, text) where index == row && key == field: return text
            case let .restore(values): return values.rows.indices.contains(row) ? values.rows[row][field] ?? "" : ""
            default: continue
            }
        }
        if let submitted { return submitted.rows.indices.contains(row) ? submitted.rows[row][field] ?? "" : "" }
        return draft.rowField(row, field)
    }

    var total: String {
        _ = revision
        if input.contains(where: {
            switch $0 {
            case .value(_, nil, _): false
            default: true
            }
        }) { return "" }
        if submitted != nil { return submittedTotal }
        return draft.total
    }

    func setRow(_ row: Int, _ field: Int32, _ text: String) {
        guard rowField(row, field) != text else { return }
        if submitted != nil { buffer(.value(field: field, row: row, text: text)); return }
        apply { try draft.setRow(row, field, to: text) }
        if saving { inputChanged = true }
    }

    private enum Input {
        case value(field: Int32, row: Int?, text: String)
        case restore(RecordValues)
    }

    private func buffer(_ value: Input) {
        // Preserve setter order: even repeated Status setters can affect another field.
        input.append(value)
        consequences = [:]
        revision &+= 1
    }

    func beginSave() {
        if !isNew {
            if let fieldEditor = window?.fieldEditor(false, for: nil) {
                undoManager?.removeAllActions(withTarget: fieldEditor)
            }
            registerChange()
        }
        submitted = RecordValues(draft)
        submittedTotal = draft.total
        saving = true
    }

    private func acceptInput() {
        submitted = nil
        let pending = input
        input = []
        let before = RecordValues(draft)
        for value in pending {
            switch value {
            case let .value(field, row?, text): setRow(row, field, text)
            case let .value(field, nil, text): set(field, text)
            case let .restore(values): apply(rows: true) { try values.apply(to: draft) }
            }
        }
        inputChanged = RecordValues(draft) != before
    }

    func finishSave() {
        saving = false
        let finish = finishPending && inputChanged
        finishPending = false
        inputChanged = false
        if finish { finishField() }
    }

    func addRow() {
        let previous = RecordValues(draft)
        let row = selectedRows.last.map { $0 + 1 } ?? draft.rowCount
        apply(rows: true) { try draft.insertRow(at: row) }
        selectedRows = [row]
        focusRequest = (draft.rowFields[0], row, (focusRequest?.serial ?? 0) + 1)
        if isNew { registerUndo(previous) }
    }

    func removeSelection() {
        let previous = RecordValues(draft)
        for row in selectedRows.sorted(by: >) where row < draft.rowCount { remove(at: row) }
        selectedRows = []
        focusRequest = nil
        if isNew { registerUndo(previous) }
        finishField()
    }

    func moveRows(_ sources: IndexSet, above row: Int) {
        let previous = RecordValues(draft)
        let sources = sources.filteredIndexSet { $0 < draft.rowCount }
        guard !sources.isEmpty, (0...draft.rowCount).contains(row) else { return }
        let destination = row - sources.count(in: 0..<row)
        var order = Array(0..<draft.rowCount)
        var wanted = order.filter { !sources.contains($0) }
        wanted.insert(contentsOf: sources, at: destination)
        guard order != wanted else { return }
        undoManager?.beginUndoGrouping()
        defer { undoManager?.endUndoGrouping() }
        for (target, original) in wanted.enumerated() {
            guard let current = order.firstIndex(of: original), current != target else { continue }
            moveRow(from: current, to: target)
            order.insert(order.remove(at: current), at: target)
        }
        selectedRows = IndexSet(integersIn: destination..<(destination + sources.count))
        if isNew { registerUndo(previous) }
        finishField()
    }

    private func moveRow(from source: Int, to destination: Int) {
        guard source != destination else { return }
        apply(rows: true) { try draft.moveRow(from: source, to: destination) }
        selectedRows = [destination]
    }

    private func remove(at row: Int) {
        apply(rows: true) { try draft.removeRow(at: row) }
    }

    private func apply(rows: Bool = false, _ change: () throws -> Void) {
        // The library refuses only text that cannot cross the interface; the field keeps
        // the draft's own text.
        do { try change() } catch { NSSound.beep() }
        consequences = [:]
        revision &+= 1
        if rows { rowsRevision &+= 1 }
    }

    /// Problems located at `field` of `row` (-1 for the record's own facts).
    func problems(at field: Int32, row: Int = -1) -> [DraftProblem] {
        problems.filter { $0.field == field && $0.row == row }
    }

    /// Keeps the input a save refused, with where `refusal` located its problems and `failure`
    /// for the record's heading.
    func refuse(_ refusal: SaveRefusal, failure: Notice) {
        if !isNew { registerChange() }
        let laterInput = !input.isEmpty
        if submitted != nil { acceptInput() }
        problems = refusal.problems
        self.failure = failure
        needsReopen = false
        revision &+= 1
        if !laterInput, let first = refusal.problems.first(where: { $0.field >= 0 }) {
            focusRequest = (first.field, first.row, (focusRequest?.serial ?? 0) + 1)
        }
    }

    /// Keeps input that cannot be saved before Reload, with the details of the failure before.
    func requireReopen(_ message: String) {
        guard !needsReopen else { return }
        refuse(.unavailable(message), failure: Notice(message, details: failure?.details ?? ""))
        needsReopen = true
    }

    func warn(_ warnings: [String]) {
        for warning in Notice.list(warnings) { warn(warning) }
    }

    func warn(_ warning: Notice) {
        if !warnings.contains(warning) { warnings.append(warning) }
    }

    func clearWarnings() {
        warnings = []
    }

    /// Marks `field` with what a save did to it, until the next edit.
    func note(_ consequence: String, at field: Int32) {
        consequences[field] = consequence
    }

    func finishField() {
        if saving { finishPending = true; return }
        if !dirty { undoPending = false; changeRegistered = false }
        onFinish?()
    }

    private func registerChange() {
        guard !changeRegistered, !undoPending else { return }
        registerUndo(baseline)
        changeRegistered = true
    }

    private func registerUndo(_ values: RecordValues) {
        undoManager?.registerUndo(withTarget: self) { $0.restore(values) }
        undoManager?.setActionName(kind == .order ? "Edit Order" : "Edit Vendor")
    }

    private func restore(_ values: RecordValues) {
        registerUndo(RecordValues(self))
        if submitted != nil {
            buffer(.restore(values))
            rowsRevision &+= 1
        } else {
            apply(rows: true) { try values.apply(to: draft) }
        }
        undoPending = true
        changeRegistered = false
        problems = []
        failure = nil
        needsReopen = false
        finishField()
    }

    /// A commit is saved even when rereading fails; its obsolete guard is never retried.
    func saved(_ draft: RecordDraft?) {
        if let draft { self.draft = draft }
        baseline = RecordValues(self.draft)
        written = true
        undoPending = input.contains { if case .restore = $0 { true } else { false } }
        changeRegistered = false
        problems = []
        failure = nil
        needsReopen = false
        acceptInput()
        revision &+= 1
    }

    func clearProblems() {
        problems = []
        failure = nil
        needsReopen = false
        revision &+= 1
    }

    func close() {
        undoManager?.removeAllActions(withTarget: self)
    }
}

/// Raw owning-record values, not domain interpretation or a filesystem snapshot.
private struct RecordValues: Equatable {
    let fields: [Int32: String]
    let rows: [[Int32: String]]

    init(_ draft: RecordDraft) {
        fields = Dictionary(uniqueKeysWithValues: draft.fields.map { ($0, draft.field($0)) })
        rows = (0..<draft.rowCount).map { row in
            Dictionary(uniqueKeysWithValues: draft.rowFields.map { ($0, draft.rowField(row, $0)) })
        }
    }

    @MainActor init(_ editor: RecordEditor) {
        fields = Dictionary(uniqueKeysWithValues: editor.draft.fields.map { ($0, editor.field($0)) })
        rows = (0..<editor.rowCount).map { row in
            Dictionary(uniqueKeysWithValues: editor.draft.rowFields.map { ($0, editor.rowField(row, $0)) })
        }
    }

    func apply(to draft: RecordDraft) throws {
        for field in draft.fields {
            if draft.kind == .order, field == SPIELORDERS_ORDER_FIELD_DATE_ORDERED { continue }
            try draft.set(field, to: fields[field] ?? "")
        }
        // Setting Ordered supplies today's date; an inverse restores the intended date last.
        if draft.kind == .order {
            try draft.set(SPIELORDERS_ORDER_FIELD_DATE_ORDERED, to: fields[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] ?? "")
        }
        while draft.rowCount > 0 { try draft.removeRow(at: draft.rowCount - 1) }
        for (row, values) in rows.enumerated() {
            try draft.insertRow(at: row)
            for field in draft.rowFields { try draft.setRow(row, field, to: values[field] ?? "") }
        }
    }
}

enum OrderStatus {
    static let all: [(name: String, label: String)] = [
        (SPIELORDERS_ORDER_STATUS_IN_PREPARATION, "In Preparation"), (SPIELORDERS_ORDER_STATUS_HOLD, "Hold"),
        (SPIELORDERS_ORDER_STATUS_ORDERED, "Ordered"), (SPIELORDERS_ORDER_STATUS_RECEIVED, "Received"),
        (SPIELORDERS_ORDER_STATUS_COMPLETE, "Complete"),
    ].map { (String(cString: spielorders_constant_name($0.0)), $0.1) }

    static func label(_ value: String) -> String {
        all.first { $0.name == value }?.label ?? value
    }
}

enum RecordFieldLabel {
    static func of(_ field: Int32) -> String {
        switch field {
        case SPIELORDERS_ORDER_FIELD_TITLE: "Order Title"
        case SPIELORDERS_ORDER_FIELD_DATE_ORDERED: "Date Ordered"
        case SPIELORDERS_ORDER_FIELD_STATUS: "Status"
        case SPIELORDERS_ORDER_FIELD_VENDOR: "Vendor"
        case SPIELORDERS_ORDER_FIELD_ORDERER: "Orderer"
        case SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION: "Ordering Institution"
        case SPIELORDERS_ORDER_FIELD_CURRENCY: "Currency"
        case SPIELORDERS_ORDER_FIELD_PROJECT: "Project"
        case SPIELORDERS_ORDER_FIELD_ORDER_GROUP: "Order Group"
        case SPIELORDERS_ORDER_FIELD_ORDER_ID: "Order ID"
        case SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER: "Catalog Number"
        case SPIELORDERS_ITEM_FIELD_DESCRIPTION: "Description"
        case SPIELORDERS_ITEM_FIELD_QUANTITY: "Quantity"
        case SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "Unit Price"
        case SPIELORDERS_ITEM_FIELD_TOTAL: "Total"
        case SPIELORDERS_ITEM_FIELD_NOTES: "Notes"
        case SPIELORDERS_VENDOR_FIELD_NAME: "Vendor Name"
        case SPIELORDERS_VENDOR_FIELD_PHONE: "Phone"
        case SPIELORDERS_VENDOR_FIELD_EMAIL: "Email"
        case SPIELORDERS_VENDOR_FIELD_WEBSITE: "Website"
        case SPIELORDERS_VENDOR_FIELD_ADDRESS: "Address"
        case SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES: "Purchasing Notes"
        case SPIELORDERS_VENDOR_FIELD_VENDOR_ID: "Vendor ID"
        case SPIELORDERS_CONTACT_FIELD_NAME: "Name"
        case SPIELORDERS_CONTACT_FIELD_ROLE: "Role"
        case SPIELORDERS_CONTACT_FIELD_PHONE: "Phone"
        case SPIELORDERS_CONTACT_FIELD_EMAIL: "Email"
        default: ""
        }
    }

    /// A stable identifier fragment from the library's own field name.
    static func key(_ field: Int32) -> String { String(cString: spielorders_constant_name(field)) }
}
