import CSpielOrders
import Foundation

/// One problem a refused save located in a draft.
public struct DraftProblem: Sendable {
    let code: Int32
    /// A `SPIELORDERS_*_FIELD_*` constant, or -1 when the problem is about the whole draft.
    public let field: Int32
    /// The zero-based row, or -1 for the record's own facts.
    public let row: Int
    public let message: String

    init(code: Int32, field: Int32, row: Int, message: String) {
        self.code = code
        self.field = field
        self.row = row
        self.message = message
    }

    init(_ entry: LibraryEntry) {
        code = entry.code
        field = entry.field
        row = Int(entry.row)
        message = entry.message
    }
}

/// A save the library refused, or could not complete, with nothing written.
public struct SaveRefusal: Error, LocalizedError, Sendable {
    private let result: Int32
    public let problems: [DraftProblem]
    /// The reusable-value facts the library asks the user to confirm as new values.
    public var unconfirmedFields: [Int32] {
        problems.filter { $0.code == SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE }.map(\.field)
    }
    public var stale: Bool {
        problems.contains { [SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED, SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED].contains($0.code) }
    }
    public var errorDescription: String? {
        problems.isEmpty ? String(cString: spielorders_constant_name(result)) : problems.map(\.message).joined(separator: "\n")
    }

    /// A save that could not be attempted, for a reason outside the draft.
    public init(result: Int32, message: String) {
        self.result = result
        problems = [DraftProblem(code: -1, field: -1, row: -1, message: message)]
    }

    init(_ failure: LibraryFailure) {
        result = failure.result
        problems = failure.entries.map(DraftProblem.init)
    }
}

/// The library's Draft of one Order or Vendor, edited as typed. Every call holds the
/// draft's lock, so the draft is never used from two threads at once.
public final class RecordDraft: @unchecked Sendable {
    public enum Kind: Sendable { case order, vendor }
    public let kind: Kind
    /// The stable row identity of the record this draft edits, or nil for a new record.
    public let recordID: String?
    private let handle: LibraryHandle<SpielordersDraft>
    private let lock = NSLock()

    init(_ pointer: UnsafeMutablePointer<SpielordersDraft>, recordID: String?) {
        handle = LibraryHandle(pointer, free: spielorders_draft_free)
        kind = spielorders_draft_kind(pointer) == SPIELORDERS_DRAFT_KIND_VENDOR ? .vendor : .order
        self.recordID = recordID
    }

    public var isNew: Bool { recordID == nil }

    /// The facts this draft sets, in editor order.
    private var fields: [Int32] {
        switch kind {
        case .order: [SPIELORDERS_ORDER_FIELD_TITLE, SPIELORDERS_ORDER_FIELD_DATE_ORDERED, SPIELORDERS_ORDER_FIELD_STATUS,
                      SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_ORDER_FIELD_ORDERER,
                      SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION, SPIELORDERS_ORDER_FIELD_CURRENCY,
                      SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP]
        case .vendor: [SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE, SPIELORDERS_VENDOR_FIELD_EMAIL,
                       SPIELORDERS_VENDOR_FIELD_WEBSITE, SPIELORDERS_VENDOR_FIELD_ADDRESS,
                       SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES]
        }
    }

    /// The facts each row holds that a user edits, in column order.
    public var rowFields: [Int32] {
        switch kind {
        case .order: [SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, SPIELORDERS_ITEM_FIELD_DESCRIPTION,
                      SPIELORDERS_ITEM_FIELD_QUANTITY, SPIELORDERS_ITEM_FIELD_UNIT_PRICE]
        case .vendor: [SPIELORDERS_CONTACT_FIELD_NAME, SPIELORDERS_CONTACT_FIELD_ROLE,
                       SPIELORDERS_CONTACT_FIELD_PHONE, SPIELORDERS_CONTACT_FIELD_EMAIL]
        }
    }

    func withDraft<T>(_ body: (UnsafeMutablePointer<SpielordersDraft>) throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try body(handle.pointer)
    }

    public func field(_ field: Int32) -> String {
        withDraft { String(cString: spielorders_draft_field($0, field)) }
    }

    public var rowCount: Int { withDraft { Int(spielorders_draft_row_count($0)) } }

    public func rowField(_ row: Int, _ field: Int32) -> String {
        withDraft { String(cString: spielorders_draft_row_field($0, Int64(row), field)) }
    }

    /// The live Order Total, unrounded; "" while a row's numbers do not parse.
    public var total: String { withDraft { String(cString: spielorders_draft_total($0)) } }

    public func set(_ field: Int32, to text: String) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            let result = try withInterfaceText(text) { spielorders_draft_set(draft, field, $0, &outcome) }
            _ = try checked(result, outcome)
        }
    }

    public func setRow(_ row: Int, _ field: Int32, to text: String) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            let result = try withInterfaceText(text) { spielorders_draft_row_set(draft, Int64(row), field, $0, &outcome) }
            _ = try checked(result, outcome)
        }
    }

    public func insertRow(at row: Int) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            _ = try checked(spielorders_draft_row_insert(draft, Int64(row), &outcome), outcome)
        }
    }

    public func removeRow(at row: Int) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            _ = try checked(spielorders_draft_row_remove(draft, Int64(row), &outcome), outcome)
        }
    }

    public func moveRow(from source: Int, to destination: Int) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            _ = try checked(spielorders_draft_row_move(draft, Int64(source), Int64(destination), &outcome), outcome)
        }
    }

    /// Confirms the value `field` now holds as a new reusable value.
    public func confirm(_ field: Int32) throws {
        try withDraft { draft in
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            _ = try checked(spielorders_draft_confirm(draft, field, -1, &outcome), outcome)
        }
    }

    /// Everything the draft holds, to compare with an earlier state of the same draft.
    public var contents: [String] {
        fields.map(field) + (0..<rowCount).flatMap { row in rowFields.map { rowField(row, $0) } }
    }
}

/// What a successful save committed.
public struct SavedRecord: Sendable {
    /// The saved record's stable row identity: an Order row ID or a Vendor ID.
    public let recordID: String
    /// A draft of the record as saved, or nil when it could not be read back.
    public let draft: RecordDraft?
    public let warnings: [String]
    public let requiresReopen: Bool
}

extension PackageSession {
    public func newOrderDraft() throws -> RecordDraft {
        guard let package else { throw SessionAccessError.reopenRequired }
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        _ = try checked(spielorders_package_new_order_draft(package.pointer, &draft, &outcome), outcome)
        guard let draft else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return RecordDraft(draft, recordID: nil)
    }

    public func newVendorDraft() throws -> RecordDraft {
        guard let package else { throw SessionAccessError.reopenRequired }
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        _ = try checked(spielorders_package_new_vendor_draft(package.pointer, &draft, &outcome), outcome)
        guard let draft else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return RecordDraft(draft, recordID: nil)
    }

    public func orderDraft(_ row: OrderRow, cancellation: LibraryCancellation) throws -> RecordDraft {
        let (order, _) = try readOrder(row, token: cancellation.token)
        return try withExtendedLifetime(order) { try orderDraft(order.pointer) }
    }

    public func vendorDraft(id: String, cancellation: LibraryCancellation) throws -> RecordDraft {
        let (vendor, _) = try readVendor(id: id, token: cancellation.token)
        return try withExtendedLifetime(vendor) { try vendorDraft(vendor.pointer) }
    }

    /// The Package's current values of one reusable-value fact.
    public func values(_ field: Int32) throws -> [String] {
        guard let package else { throw SessionAccessError.reopenRequired }
        var list: UnsafeMutablePointer<SpielordersList>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        _ = try checked(spielorders_package_values(package.pointer, field, &list, &outcome), outcome)
        defer { spielorders_list_free(list) }
        guard let list else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return (0..<spielorders_list_count(list)).map {
            String(cString: spielorders_list_field(list, $0, SPIELORDERS_VALUE_FIELD_VALUE))
        }
    }

    /// Plans and commits `draft`. A refusal throws `SaveRefusal` and writes nothing.
    public func save(_ draft: RecordDraft, cancellation: LibraryCancellation) throws -> SavedRecord {
        guard let package else { throw SessionAccessError.reopenRequired }
        do {
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            var planned: UnsafeMutablePointer<SpielordersPlan>?
            let result = draft.withDraft { spielorders_draft_plan($0, cancellation.token, &planned, &outcome) }
            var warnings = try checked(result, outcome)
            guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            let plan = LibraryHandle(planned, free: spielorders_plan_free)
            try Task.checkCancellation()
            var report: UnsafeMutablePointer<SpielordersReport>?
            outcome = nil
            let committed = spielorders_plan_commit(plan.pointer, &report, &outcome)
            defer { spielorders_report_free(report) }
            warnings += try checked(committed, outcome)
            guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            var recordID = draft.recordID ?? ""
            var refreshed: RecordDraft?
            switch draft.kind {
            case .order:
                if let order = spielorders_report_order(report) {
                    defer { spielorders_order_free(order) }
                    recordID = "order:" + String(cString: spielorders_order_field(order, SPIELORDERS_ORDER_FIELD_ORDER_ID))
                    refreshed = try? orderDraft(order)
                }
            case .vendor:
                if let vendor = spielorders_report_vendor(report) {
                    defer { spielorders_vendor_free(vendor) }
                    recordID = String(cString: spielorders_vendor_field(vendor, SPIELORDERS_VENDOR_FIELD_VENDOR_ID))
                    refreshed = try? vendorDraft(vendor)
                }
            }
            if spielorders_report_kind(report) == SPIELORDERS_REPORT_COMMITTED {
                warnings += refreshPackageAfterCommit(package)
            }
            return SavedRecord(recordID: recordID, draft: refreshed, warnings: warnings.map(\.message),
                               requiresReopen: self.package == nil || refreshed == nil)
        } catch let failure as LibraryFailure {
            throw SaveRefusal(failure)
        }
    }

    /// Hides or shows a Vendor. Returns the commit's warnings.
    public func setVendorHidden(id: String, hidden: Bool, cancellation: LibraryCancellation) throws -> [String] {
        guard let package else { throw SessionAccessError.reopenRequired }
        let (vendor, read) = try readVendor(id: id, token: cancellation.token)
        var planned: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        defer { spielorders_report_free(report); spielorders_plan_free(planned) }
        let result = spielorders_vendor_plan_hidden(vendor.pointer, hidden ? 1 : 0, cancellation.token, &planned, &outcome)
        var warnings = try read + checked(result, outcome)
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        try Task.checkCancellation()
        outcome = nil
        warnings += try checked(spielorders_plan_commit(planned, &report, &outcome), outcome)
        if let report, spielorders_report_kind(report) == SPIELORDERS_REPORT_COMMITTED {
            warnings += refreshPackageAfterCommit(package)
        }
        return warnings.map(\.message)
    }

    func readVendor(id: String, token: UInt64) throws -> (LibraryHandle<SpielordersVendor>, [LibraryEntry]) {
        guard let package else { throw SessionAccessError.reopenRequired }
        var vendor: UnsafeMutablePointer<SpielordersVendor>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = try withInterfaceText(id) {
            spielorders_package_read_vendor(package.pointer, SPIELORDERS_VENDOR_SELECTOR_ID, $0, token, &vendor, &outcome)
        }
        let notices = try checked(result, outcome)
        guard let vendor else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return (LibraryHandle(vendor, free: spielorders_vendor_free), notices)
    }

    private func orderDraft(_ order: UnsafeMutablePointer<SpielordersOrder>) throws -> RecordDraft {
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        _ = try checked(spielorders_order_draft(order, &draft), nil)
        guard let draft else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return RecordDraft(draft, recordID: "order:" + String(cString: spielorders_order_field(order, SPIELORDERS_ORDER_FIELD_ORDER_ID)))
    }

    private func vendorDraft(_ vendor: UnsafeMutablePointer<SpielordersVendor>) throws -> RecordDraft {
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        _ = try checked(spielorders_vendor_draft(vendor, &draft), nil)
        guard let draft else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return RecordDraft(draft, recordID: String(cString: spielorders_vendor_field(vendor, SPIELORDERS_VENDOR_FIELD_VENDOR_ID)))
    }
}
