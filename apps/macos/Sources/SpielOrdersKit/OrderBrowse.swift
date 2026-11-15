import CSpielOrders
import Foundation

public struct OrderRow: Sendable, Identifiable {
    public let id: String
    public let values: [Int32: String]
    public var directoryName: String { values[SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME] ?? "" }
    public let damage: String?
    let path: FileSystemPath
    public var directory: URL { path.url }
}

public struct OrderListing: Sendable {
    public let rows: [OrderRow]
    public let notices: [String]

    public func retainingSelection(_ selected: OrderRow?) -> OrderRow? {
        guard let selected else { return nil }
        if selected.damage == nil { return rows.first { $0.id == selected.id && $0.damage == nil } }
        let matches = rows.filter { $0.damage != nil && $0.directoryName == selected.directoryName }
        return matches.count == 1 ? matches[0] : nil
    }
}

public struct OrderDetails: Sendable {
    public let readmeText: String
    public let readmeState: Int32
    public let notices: [String]
    /// Whether the directory name differs from the name the Order's facts project, so that
    /// Align Directory Name would rename it.
    public let drifted: Bool
}

public enum OrderInspection: Sendable {
    case healthy(OrderDetails)
    case damaged([String])
}

extension PackageSession {
    public var diagnosticOnly: Bool {
        package.map { spielorders_package_state($0.pointer) == SPIELORDERS_PACKAGE_STATE_DAMAGED } ?? false
    }

    public var findings: [String] {
        guard let package else { return [] }
        return consumeEntries(spielorders_package_findings(package.pointer)).map(\.presentation)
    }

    /// The facts an Order search can be confined to, in the order a menu offers them.
    public static let orderSearchFields: [Int32] = (0..<spielorders_order_search_field_count()).map {
        spielorders_order_search_field($0)
    }

    /// The Orders matching `query` in one of `orderSearchFields`, or in every searchable fact.
    public func orders(query: String, field: Int32? = nil, caseSensitive: Bool, sortField: Int32, descending: Bool,
                       cancellation: LibraryCancellation) throws -> OrderListing {
        guard let package else { throw SessionAccessError.reopenRequired }
        let flags = caseSensitive ? UInt32(SPIELORDERS_LIST_CASE_SENSITIVE) : 0
        return try listOrders { list, outcome in
            try withInterfaceText(query) {
                spielorders_package_search_orders(package.pointer, $0, field ?? -1, flags, sortField,
                                                  descending ? 1 : 0, cancellation.token, list, outcome)
            }
        }
    }

    /// Folder Contents: the Orders whose facts match `query`, together with every Order
    /// holding a file of `material`, a Supporting Material search of this Package.
    public func folderContents(query: String, caseSensitive: Bool, material: MaterialAnswer?, sortField: Int32,
                               descending: Bool, cancellation: LibraryCancellation) throws -> OrderListing {
        guard let package else { throw SessionAccessError.reopenRequired }
        let flags = caseSensitive ? UInt32(SPIELORDERS_LIST_CASE_SENSITIVE) : 0
        return try listOrders { list, outcome in
            try withInterfaceText(query) {
                spielorders_package_search_contents(package.pointer, $0, flags, material?.search.pointer, sortField,
                                                    descending ? 1 : 0, cancellation.token, list, outcome)
            }
        }
    }

    private func listOrders(_ search: (ListOut, EntriesOut) throws -> Int32) throws -> OrderListing {
        var list: UnsafeMutablePointer<SpielordersList>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = try search(&list, &outcome)
        let notices = try checked(result, outcome).map(\.presentation)
        defer { spielorders_list_free(list) }
        guard let list else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return OrderListing(rows: orderRows(list), notices: notices)
    }

    public func inspectOrder(_ row: OrderRow, cancellation: LibraryCancellation) throws -> OrderInspection {
        guard let package else { throw SessionAccessError.reopenRequired }
        if row.damage != nil {
            var diagnosis: UnsafeMutablePointer<SpielordersDiagnosis>?
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            let result = row.path.withUnsafeFileSystemRepresentation {
                spielorders_package_diagnose_order(package.pointer, $0, cancellation.token, &diagnosis, &outcome)
            }
            let notices = try checked(result, outcome).map(\.presentation)
            defer { spielorders_diagnosis_free(diagnosis) }
            guard let diagnosis else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            guard spielorders_diagnosis_is_valid(diagnosis) == 0 else { throw SessionAccessError.selectionChanged }
            let findings = consumeEntries(spielorders_diagnosis_findings(diagnosis)).map(\.presentation)
            return .damaged(notices + findings)
        }
        let (owner, entries) = try readOrder(row, token: cancellation.token)
        defer { withExtendedLifetime(owner) {} }
        let order = owner.pointer
        let notices = entries.map(\.presentation)
        guard let readme = spielorders_order_readme(order) else {
            throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: [])
        }
        defer { spielorders_readme_free(readme) }
        return .healthy(OrderDetails(readmeText: String(cString: spielorders_readme_text(readme)),
                                     readmeState: spielorders_readme_state(readme), notices: notices,
                                     drifted: drifted(order)))
    }

    /// Whether the directory name differs from the name aligning the Order would give it. An
    /// Order whose facts project no name is not offered alignment.
    private func drifted(_ order: UnsafeMutablePointer<SpielordersOrder>) -> Bool {
        let aligned = String(cString: spielorders_order_aligned_name(order))
        let directory = String(cString: spielorders_order_field(order, SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME))
        return !aligned.isEmpty && aligned != directory
    }

    func readOrder(_ row: OrderRow, token: UInt64) throws -> (LibraryHandle<SpielordersOrder>, [LibraryEntry]) {
        guard let package else { throw SessionAccessError.reopenRequired }
        var order: UnsafeMutablePointer<SpielordersOrder>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = row.path.withUnsafeFileSystemRepresentation {
            spielorders_package_read_order(package.pointer, $0, token, &order, &outcome)
        }
        let notices = try checked(result, outcome)
        guard let order else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let owner = LibraryHandle(order, free: spielorders_order_free)
        guard row.id == "order:" + String(cString: spielorders_order_field(order, SPIELORDERS_ORDER_FIELD_ORDER_ID)) else {
            throw SessionAccessError.selectionChanged
        }
        return (owner, notices)
    }
}

func orderRows(_ list: UnsafeMutablePointer<SpielordersList>) -> [OrderRow] {
    (0..<spielorders_list_count(list)).map { index in
        func field(_ key: Int32) -> String { String(cString: spielorders_list_field(list, index, key)) }
        let damaged = spielorders_list_damaged(list, index) != 0
        let values = Dictionary(uniqueKeysWithValues: OrderColumn.allCases.map { ($0.field, field($0.field)) })
        return OrderRow(id: damaged ? "damaged:\(index)" : "order:" + field(SPIELORDERS_ORDER_FIELD_ORDER_ID),
                        values: values, damage: damaged ? field(SPIELORDERS_ORDER_FIELD_DAMAGE) : nil,
                        path: FileSystemPath(fileSystemRepresentation: spielorders_list_path(list, index)))
    }
}
