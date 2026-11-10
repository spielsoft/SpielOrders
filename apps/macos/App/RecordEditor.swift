import AppKit
import CSpielOrders
import Observation
import SpielOrdersKit

/// One open editor over a library Draft. Swift keeps only presentation state: the
/// baseline to tell a dirty draft, the problems of the last refused save, and focus.
@MainActor @Observable final class RecordEditor {
    private(set) var draft: RecordDraft
    private var baseline: [String]
    /// Changes whenever the draft changes, so views read it again.
    private(set) var revision = 0
    /// Changes whenever rows are added, removed, or moved, or the draft is replaced.
    private(set) var rowsRevision = 0
    private(set) var problems: [DraftProblem] = []
    private(set) var refused = false
    /// The field and row to focus, with a serial so the same location can be asked twice.
    private(set) var focusRequest: (field: Int32, row: Int, serial: Int)?
    var selectedRows = IndexSet()
    @ObservationIgnored weak var undoManager: UndoManager?

    init(_ draft: RecordDraft) {
        self.draft = draft
        baseline = draft.contents
    }

    var kind: RecordDraft.Kind { draft.kind }
    var isNew: Bool { draft.isNew }
    var dirty: Bool {
        _ = revision
        return draft.contents != baseline
    }

    func field(_ field: Int32) -> String {
        _ = revision
        return draft.field(field)
    }

    func set(_ field: Int32, _ text: String) {
        guard draft.field(field) != text else { return }
        apply { try draft.set(field, to: text) }
    }

    var rowCount: Int {
        _ = revision
        return draft.rowCount
    }

    func rowField(_ row: Int, _ field: Int32) -> String {
        _ = revision
        return draft.rowField(row, field)
    }

    var total: String {
        _ = revision
        return draft.total
    }

    func setRow(_ row: Int, _ field: Int32, _ text: String) {
        guard draft.rowField(row, field) != text else { return }
        apply { try draft.setRow(row, field, to: text) }
    }

    func addRow() {
        let row = selectedRows.last.map { $0 + 1 } ?? draft.rowCount
        insert([:], at: row, actionName: kind == .order ? "Add Item" : "Add Contact")
        selectedRows = [row]
        focusRequest = (draft.rowFields[0], row, (focusRequest?.serial ?? 0) + 1)
    }

    func removeSelectedRows() {
        for row in selectedRows.sorted(by: >) where row < draft.rowCount { remove(at: row) }
        selectedRows = []
    }

    func moveRow(from source: Int, to destination: Int) {
        guard source != destination else { return }
        apply(rows: true) { try draft.moveRow(from: source, to: destination) }
        undoManager?.registerUndo(withTarget: self) { $0.moveRow(from: destination, to: source) }
        undoManager?.setActionName(kind == .order ? "Move Item" : "Move Contact")
        selectedRows = [destination]
    }

    private func remove(at row: Int) {
        let values = Dictionary(uniqueKeysWithValues: draft.rowFields.map { ($0, draft.rowField(row, $0)) })
        apply(rows: true) { try draft.removeRow(at: row) }
        undoManager?.registerUndo(withTarget: self) {
            $0.insert(values, at: row, actionName: $0.kind == .order ? "Remove Item" : "Remove Contact")
            $0.selectedRows = [row]
        }
        undoManager?.setActionName(kind == .order ? "Remove Item" : "Remove Contact")
    }

    private func insert(_ values: [Int32: String], at row: Int, actionName: String) {
        apply(rows: true) {
            try draft.insertRow(at: row)
            for field in draft.rowFields { if let value = values[field], !value.isEmpty { try draft.setRow(row, field, to: value) } }
        }
        undoManager?.registerUndo(withTarget: self) { $0.remove(at: row) }
        undoManager?.setActionName(actionName)
    }

    private func apply(rows: Bool = false, _ change: () throws -> Void) {
        // The library refuses only text that cannot cross the interface; the field keeps
        // the draft's own text.
        do { try change() } catch { NSSound.beep() }
        revision &+= 1
        if rows { rowsRevision &+= 1 }
    }

    /// Problems located at `field` of `row` (-1 for the record's own facts).
    func problems(at field: Int32, row: Int = -1) -> [DraftProblem] {
        problems.filter { $0.field == field && $0.row == row }
    }

    func refuse(_ refusal: SaveRefusal) {
        problems = refusal.problems
        refused = true
        revision &+= 1
        if let first = refusal.problems.first(where: { $0.field >= 0 }) {
            focusRequest = (first.field, first.row, (focusRequest?.serial ?? 0) + 1)
        }
    }

    /// Replaces the draft with the one a successful save read back.
    func saved(_ draft: RecordDraft) {
        undoManager?.removeAllActions(withTarget: self)
        self.draft = draft
        baseline = draft.contents
        problems = []
        refused = false
        revision &+= 1
        rowsRevision &+= 1
    }

    func clearProblems() {
        problems = []
        refused = false
        revision &+= 1
    }

    func close() { undoManager?.removeAllActions(withTarget: self) }
}

enum OrderStatus {
    static let all: [(name: String, label: String)] = [
        (SPIELORDERS_ORDER_STATUS_IN_PREPARATION, "In Preparation"), (SPIELORDERS_ORDER_STATUS_HOLD, "Hold"),
        (SPIELORDERS_ORDER_STATUS_ORDERED, "Ordered"), (SPIELORDERS_ORDER_STATUS_RECEIVED, "Received"),
        (SPIELORDERS_ORDER_STATUS_COMPLETE, "Complete"),
    ].map { (String(cString: spielorders_constant_name($0.0)), $0.1) }
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
        case SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER: "Catalog Number"
        case SPIELORDERS_ITEM_FIELD_DESCRIPTION: "Description"
        case SPIELORDERS_ITEM_FIELD_QUANTITY: "Quantity"
        case SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "Unit Price"
        case SPIELORDERS_ITEM_FIELD_TOTAL: "Total"
        case SPIELORDERS_VENDOR_FIELD_NAME: "Vendor Name"
        case SPIELORDERS_VENDOR_FIELD_PHONE: "Phone"
        case SPIELORDERS_VENDOR_FIELD_EMAIL: "Email"
        case SPIELORDERS_VENDOR_FIELD_WEBSITE: "Website"
        case SPIELORDERS_VENDOR_FIELD_ADDRESS: "Address"
        case SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES: "Purchasing Notes"
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
