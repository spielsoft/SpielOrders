import CSpielOrders
import Foundation

private let vendorFields: [Int32] = [
    SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE, SPIELORDERS_VENDOR_FIELD_EMAIL,
    SPIELORDERS_VENDOR_FIELD_WEBSITE, SPIELORDERS_VENDOR_FIELD_ADDRESS,
    SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES, SPIELORDERS_VENDOR_FIELD_VENDOR_ID, SPIELORDERS_VENDOR_FIELD_HIDDEN,
]
private let vendorListFields: [Int32] = [
    SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE, SPIELORDERS_VENDOR_FIELD_EMAIL,
    SPIELORDERS_VENDOR_FIELD_WEBSITE, SPIELORDERS_VENDOR_FIELD_HIDDEN,
]

public struct VendorRow: Sendable, Identifiable {
    public let id: String
    private let values: [Int32: String]
    public var name: String { values[SPIELORDERS_VENDOR_FIELD_NAME] ?? "" }
    public var phone: String { values[SPIELORDERS_VENDOR_FIELD_PHONE] ?? "" }
    public var email: String { values[SPIELORDERS_VENDOR_FIELD_EMAIL] ?? "" }
    public var website: String { values[SPIELORDERS_VENDOR_FIELD_WEBSITE] ?? "" }
    public var hidden: Bool { values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true" }

    init(id: String, values: [Int32: String]) {
        self.id = id
        self.values = values
    }
}

public struct VendorListing: Sendable {
    public let rows: [VendorRow]
    public let notices: [String]
}

public struct VendorDetails: Sendable {
    public let values: [Int32: String]
    public let orders: [OrderRow]
    public let notices: [String]
}

extension PackageSession {
    /// The facts a Vendor search can be confined to, in the order a menu offers them.
    public static let vendorSearchFields: [Int32] = (0..<spielorders_vendor_search_field_count()).map {
        spielorders_vendor_search_field($0)
    }

    /// The Vendors matching `query` in one of `vendorSearchFields`, or in every searchable fact.
    public func vendors(query: String, field: Int32? = nil, caseSensitive: Bool, includeHidden: Bool, sortField: Int32,
                        descending: Bool, cancellation: LibraryCancellation) throws -> VendorListing {
        guard let package else { throw SessionAccessError.reopenRequired }
        var list: UnsafeMutablePointer<SpielordersList>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let flags = (includeHidden ? UInt32(SPIELORDERS_LIST_INCLUDE_HIDDEN) : 0)
            | (caseSensitive ? UInt32(SPIELORDERS_LIST_CASE_SENSITIVE) : 0)
        let result = try withInterfaceText(query) {
            spielorders_package_search_vendors(package.pointer, $0, field ?? -1, flags, sortField, descending ? 1 : 0,
                                               cancellation.token, &list, &outcome)
        }
        let notices = try checked(result, outcome).map(\.presentation)
        defer { spielorders_list_free(list) }
        guard let list else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let rows = (0..<spielorders_list_count(list)).map { index in
            let values = Dictionary(uniqueKeysWithValues: vendorListFields.map { ($0, String(cString: spielorders_list_field(list, index, $0))) })
            return VendorRow(id: String(cString: spielorders_list_field(list, index, SPIELORDERS_VENDOR_FIELD_VENDOR_ID)), values: values)
        }
        return VendorListing(rows: rows, notices: notices)
    }

    public func inspectVendor(_ row: VendorRow, cancellation: LibraryCancellation) throws -> VendorDetails {
        guard let package else { throw SessionAccessError.reopenRequired }
        let (owner, entries) = try readVendor(id: row.id, token: cancellation.token)
        defer { withExtendedLifetime(owner) {} }
        let vendor = owner.pointer
        var notices = entries.map(\.presentation)
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let values = Dictionary(uniqueKeysWithValues: vendorFields.map { ($0, String(cString: spielorders_vendor_field(vendor, $0))) })
        var associated: UnsafeMutablePointer<SpielordersList>?
        defer { spielorders_list_free(associated) }
        let related = row.id.withCString {
            spielorders_package_orders_of_vendor(package.pointer, $0, cancellation.token, &associated, &outcome)
        }
        notices += try checked(related, outcome).map(\.presentation)
        guard let associated else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return VendorDetails(values: values, orders: orderRows(associated), notices: notices)
    }
}
