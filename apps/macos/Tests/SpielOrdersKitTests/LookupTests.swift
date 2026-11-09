import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func ordinaryLookupUsesLibraryCaseMatchingAndKeepsTheRequestedSort() async throws {
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

@Test func vendorLookupUsesLibraryCaseMatchingAndSort() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Optics Alpha")
    _ = try createVendor(in: root, name: "optics Zulu")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let insensitive = try await session.vendors(query: "optics", caseSensitive: false, includeHidden: false,
                                                  sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: true,
                                                  cancellation: LibraryCancellation())
    #expect(insensitive.rows.map(\.name) == ["optics Zulu", "Optics Alpha"])
    let sensitive = try await session.vendors(query: "optics", caseSensitive: true, includeHidden: false,
                                                sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: true,
                                                cancellation: LibraryCancellation())
    #expect(sensitive.rows.map(\.name) == ["optics Zulu"])
}
