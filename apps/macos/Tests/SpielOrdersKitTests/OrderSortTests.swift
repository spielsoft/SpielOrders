import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func headerSortUsesLibraryNumericOrderAndKeepsDamagedRowsLast() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let expensive = try createOrder(in: root, title: "High total", itemPrice: "10")
    _ = try createOrder(in: root, title: "Low total", itemPrice: "2")
    try FileManager.default.createDirectory(at: expensive.deletingLastPathComponent().appendingPathComponent("Unadopted"),
                                           withIntermediateDirectories: false)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let ascending = try await session.orders(query: "", caseSensitive: false, sortField: OrderColumn.total.field, descending: false,
                                              cancellation: LibraryCancellation()).rows
    #expect(ascending.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "" } == ["Low total", "High total", ""])
    #expect(ascending.last?.damage != nil)
    let descending = try await session.orders(query: "", caseSensitive: false, sortField: OrderColumn.total.field, descending: true,
                                               cancellation: LibraryCancellation()).rows
    #expect(descending.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "" } == ["High total", "Low total", ""])
    #expect(descending.last?.damage != nil)
}

@Test func sortingRetainsTheSelectedDamagedDirectoryAndRefreshesItsDiagnosis() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root, title: "Healthy")
    for name in ["First unadopted", "Selected unadopted"] {
        try FileManager.default.createDirectory(at: order.deletingLastPathComponent().appendingPathComponent(name),
                                               withIntermediateDirectories: false)
    }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let initial = try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation())
    let selected = try #require(initial.rows.first { $0.directoryName == "Selected unadopted" })
    let sorted = try await session.orders(query: "", caseSensitive: false, sortField: OrderColumn.title.field, descending: true, cancellation: LibraryCancellation())
    let retained = try #require(sorted.retainingSelection(selected))
    #expect(retained.directoryName == "Selected unadopted")
    if case let .damaged(messages) = try await session.inspectOrder(retained, cancellation: LibraryCancellation()) {
        #expect(!messages.isEmpty)
    } else { Issue.record("Sorting must retain the selected placeholder's diagnosis") }
}
