import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func stageRefusal() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root, title: "Original optics")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                   descending: false, cancellation: LibraryCancellation()).rows.first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Retained optics draft")
    let stage = directory.appendingPathComponent(".spielorders/.order.json.spielorders-stage")
    let evidence = Data("Existing staging evidence".utf8)
    try evidence.write(to: stage, options: .withoutOverwriting)
    do {
        _ = try await session.save(draft, cancellation: LibraryCancellation())
        Issue.record("An occupied staging path must refuse without overwriting it")
    } catch let refusal as SaveRefusal {
        #expect(refusal.localizedDescription.contains("\(directory.lastPathComponent)/.spielorders/order.json"))
    }
    #expect(draft.field(SPIELORDERS_ORDER_FIELD_TITLE) == "Retained optics draft")
    #expect(try Data(contentsOf: stage) == evidence)
    let reader = PackageSession()
    _ = try await reader.open(root, cancellation: LibraryCancellation())
    #expect(try await reader.orders(query: "", caseSensitive: false, sortField: -1,
                                    descending: false, cancellation: LibraryCancellation()).rows.first?.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Original optics")
    try FileManager.default.removeItem(at: stage)
    let retried = try await session.save(draft, cancellation: LibraryCancellation())
    #expect(retried.draft?.field(SPIELORDERS_ORDER_FIELD_TITLE) == "Retained optics draft")
}
