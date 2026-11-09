import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func browsingShowsTrustedOrdersAndDiagnosesDamagedDirectories() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let healthy = try createOrder(in: root, title: "Laser optics")
    try "# Alignment notes\n".write(to: healthy.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
    let damaged = healthy.deletingLastPathComponent().appendingPathComponent("Unadopted quote")
    try FileManager.default.createDirectory(at: damaged, withIntermediateDirectories: false)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())

    let rows = try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation()).rows
    #expect(rows.count == 2)
    let recognized = try #require(rows.first { $0.damage == nil })
    #expect(recognized.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Laser optics")
    let placeholder = try #require(rows.first { $0.damage != nil })
    #expect(placeholder.directoryName == "Unadopted quote")
    #expect(placeholder.values[SPIELORDERS_ORDER_FIELD_STATUS] == "")

    let inspected = try await session.inspectOrder(recognized, cancellation: LibraryCancellation())
    if case let .healthy(details) = inspected {
        #expect(details.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Laser optics")
        #expect(details.readmeText == "# Alignment notes\n")
    } else { Issue.record("A healthy Order must have a readable inspector") }
    let diagnosis = try await session.inspectOrder(placeholder, cancellation: LibraryCancellation())
    if case let .damaged(messages) = diagnosis { #expect(!messages.isEmpty) }
    else { Issue.record("A damaged directory must have a diagnosis, not healthy facts") }
}

@Test func theInspectorReadsItemFactsAndTheLibraryCalculatedTotal() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root, title: "Lens pair")
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var order: UnsafeMutablePointer<SpielordersOrder>?
    var draft: UnsafeMutablePointer<SpielordersDraft>?
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    defer {
        spielorders_report_free(report); spielorders_plan_free(plan); spielorders_draft_free(draft)
        spielorders_order_free(order); spielorders_package_free(package)
    }
    #expect(root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) } == SPIELORDERS_RESULT_OK)
    #expect(directory.withUnsafeFileSystemRepresentation {
        spielorders_package_read_order(package!, $0!, 0, &order, nil)
    } == SPIELORDERS_RESULT_OK)
    #expect(spielorders_order_draft(try #require(order), &draft) == SPIELORDERS_RESULT_OK)
    #expect(spielorders_draft_row_insert(try #require(draft), 0, nil) == SPIELORDERS_RESULT_OK)
    for (field, text) in [(SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, "L-3"), (SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Lens"),
                          (SPIELORDERS_ITEM_FIELD_QUANTITY, "2"), (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, "17.25")] {
        #expect(text.withCString { spielorders_draft_row_set(draft!, 0, field, $0, nil) } == SPIELORDERS_RESULT_OK)
    }
    #expect(spielorders_draft_plan(draft!, 0, &plan, nil) == SPIELORDERS_RESULT_OK)
    #expect(spielorders_plan_commit(try #require(plan), &report, nil) == SPIELORDERS_RESULT_OK)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation()).rows.first)
    let inspected = try await session.inspectOrder(row, cancellation: LibraryCancellation())
    if case let .healthy(details) = inspected {
        let item = try #require(details.items.first)
        #expect(item.catalogNumber == "L-3")
        #expect(item.description == "Lens")
        #expect(item.quantity == "2")
        #expect(item.unitPrice == "17.25")
        #expect(item.total == "34.5")
        #expect(details.values[SPIELORDERS_ORDER_FIELD_TOTAL] == "34.5")
    } else { Issue.record("A committed healthy Order must have an Items inspector") }
}

@Test func aReplacedDirectoryCannotShowAnotherOrderUnderTheCachedSelection() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let first = try createOrder(in: root, title: "First Order")
    let replacement = try createOrder(in: root, title: "Replacement Order")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let cached = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation()).rows
        .first { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] == "First Order" })
    try FileManager.default.moveItem(at: first, to: root.appendingPathComponent("parked"))
    try FileManager.default.moveItem(at: replacement, to: first)

    await #expect(throws: (any Error).self) {
        try await session.inspectOrder(cached, cancellation: LibraryCancellation())
    }
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let current = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation()).rows.first)
    if case let .healthy(details) = try await session.inspectOrder(current, cancellation: LibraryCancellation()) {
        #expect(details.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Replacement Order")
    } else { Issue.record("Reopen must allow selecting the current healthy Order") }
}

@Test func aNowHealthyDirectoryRequiresReopenBeforeReplacingItsDamagedInspector() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let healthy = try createOrder(in: root, title: "Adopted Order")
    let candidate = healthy.deletingLastPathComponent().appendingPathComponent("Awaiting adoption")
    try FileManager.default.createDirectory(at: candidate, withIntermediateDirectories: false)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let cached = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false, cancellation: LibraryCancellation()).rows.first { $0.damage != nil })
    try FileManager.default.removeItem(at: candidate)
    try FileManager.default.moveItem(at: healthy, to: candidate)

    await #expect(throws: (any Error).self) {
        try await session.inspectOrder(cached, cancellation: LibraryCancellation())
    }
}
