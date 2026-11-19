import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func orderLookup() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let first = try createOrder(in: root, title: "Laser Alpha")
    _ = try createOrder(in: root, title: "laser Zulu")
    try FileManager.default.createDirectory(at: first.deletingLastPathComponent().appendingPathComponent("Laser quote"),
                                            withIntermediateDirectories: false)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let insensitive = try await session.orders(query: "laser", caseSensitive: false, sortField: OrderColumn.title.field,
                                                descending: true, cancellation: LibraryCancellation())
    #expect(insensitive.rows.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] } == ["laser Zulu", "Laser Alpha"])
    #expect(insensitive.rows.allSatisfy { $0.damage == nil })
    let sensitive = try await session.orders(query: "laser", caseSensitive: true, sortField: OrderColumn.title.field,
                                              descending: true, cancellation: LibraryCancellation())
    #expect(sensitive.rows.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] } == ["laser Zulu"])
    let cleared = try await session.orders(query: "", caseSensitive: true, sortField: OrderColumn.title.field,
                                            descending: true, cancellation: LibraryCancellation())
    #expect(cleared.rows.count == 3)
    #expect(cleared.rows.last?.damage != nil)
}

@Test func fieldScopedOrderLookup() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Mirror mount", itemPrice: "12.5")
    _ = try createOrder(in: root, title: "Part bin")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    func titles(_ query: String, in field: Int32?) async throws -> [String?] {
        try await session.orders(query: query, field: field, caseSensitive: false, sortField: OrderColumn.title.field,
                                 descending: false, cancellation: LibraryCancellation())
            .rows.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] }
    }

    #expect(try await titles("part", in: nil) == ["Mirror mount", "Part bin"])
    #expect(try await titles("part", in: SPIELORDERS_ORDER_FIELD_TITLE) == ["Part bin"])
    #expect(try await titles("part", in: SPIELORDERS_ITEM_FIELD_DESCRIPTION) == ["Mirror mount"])
    #expect(try await titles("12.50", in: SPIELORDERS_ITEM_FIELD_UNIT_PRICE) == ["Mirror mount"])
    for field in PackageSession.orderSearchFields {
        _ = try await titles("", in: field)
    }
}

@Test func fieldScopedVendorLookup() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Optics Alpha", contactEmail: "beam@optics.example")
    _ = try createVendor(in: root, name: "Beam Works")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    func names(_ query: String, in field: Int32?, caseSensitive: Bool = false) async throws -> [String] {
        try await session.vendors(query: query, field: field, caseSensitive: caseSensitive, includeHidden: false,
                                  sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: false,
                                  cancellation: LibraryCancellation())
            .rows.map(\.name)
    }

    #expect(try await names("beam", in: nil) == ["Beam Works", "Optics Alpha"])
    #expect(try await names("beam", in: nil, caseSensitive: true) == ["Optics Alpha"])
    #expect(try await names("beam", in: SPIELORDERS_VENDOR_FIELD_NAME) == ["Beam Works"])
    #expect(try await names("beam", in: SPIELORDERS_CONTACT_FIELD_EMAIL) == ["Optics Alpha"])
    for field in PackageSession.vendorSearchFields {
        _ = try await names("", in: field)
    }
}
