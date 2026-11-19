import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

/// Saves a new Order titled `title` with `values`, confirming each as a new value.
private func addOrder(_ session: PackageSession, _ title: String, _ values: [Int32: String]) async throws {
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: title)
    for field in [SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP] {
        guard let value = values[field] else { continue }
        try draft.set(field, to: value)
        try draft.confirm(field)
    }
    _ = try await session.save(draft, cancellation: LibraryCancellation())
}

private func titles(_ session: PackageSession, using value: String, in scope: ValueScope) async throws -> Set<String> {
    let orders = try await session.orders(using: value, in: scope, cancellation: LibraryCancellation())
    return Set(orders.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "" })
}

@Test func ordersUsingEachValue() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Coils", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada Lovelace",
                                          SPIELORDERS_ORDER_FIELD_PROJECT: "Greenhouse",
                                          SPIELORDERS_ORDER_FIELD_ORDER_GROUP: "Misting"])
    try await addOrder(session, "Lenses", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada Lovelace",
                                           SPIELORDERS_ORDER_FIELD_PROJECT: "Greenhouse"])
    try await addOrder(session, "Mirrors", [SPIELORDERS_ORDER_FIELD_ORDERER: "Grace Hopper",
                                            SPIELORDERS_ORDER_FIELD_PROJECT: "Optics",
                                            SPIELORDERS_ORDER_FIELD_ORDER_GROUP: "Misting"])
    try await addOrder(session, "Cables", [SPIELORDERS_ORDER_FIELD_ORDER_GROUP: "Misting"])

    #expect(try await titles(session, using: "ada lovelace", in: .orderers) == ["Coils", "Lenses"])
    #expect(try await titles(session, using: "Greenhouse", in: .projects) == ["Coils", "Lenses"])
    #expect(try await titles(session, using: "Misting", in: .groups(of: "Greenhouse")) == ["Coils"])
    #expect(try await titles(session, using: "Misting", in: .groups(of: "Optics")) == ["Mirrors"])
    #expect(try await titles(session, using: "Misting", in: .groups(of: "")) == ["Cables"])
    #expect(try await titles(session, using: "Emmy Noether", in: .orderers).isEmpty)
}
