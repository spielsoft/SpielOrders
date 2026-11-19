import Foundation
import SpielOrdersKit
import Testing

// Decision sources: Define the Order record and lifecycle contract (merging Orders); Prototype
// the SpielOrders desktop experience (Merge N Orders…).

private func write(_ text: String, to path: String, in directory: URL) throws {
    let file = directory.appendingPathComponent(path)
    try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
    try text.write(to: file, atomically: true, encoding: .utf8)
}

private func entries(_ listed: [MergedEntry]) -> [[String]] {
    listed.map { [$0.order, $0.path] }
}

/// Three Orders: Bench, with its own quote; Diode, with an Item and a quotes folder; and Mounts.
/// Each later Order also has a quote at the path Bench's holds.
private struct ThreeOrders {
    let root: URL
    let session: PackageSession
    let bench: OrderRow
    let diode: OrderRow
    let mounts: OrderRow

    init() async throws {
        root = try initializedPackage()
        let bench = try createOrder(in: root, title: "Bench")
        let diode = try createOrder(in: root, title: "Diode", itemPrice: "12.00")
        let mounts = try createOrder(in: root, title: "Mounts")
        try write("Bench quote", to: "quote.txt", in: bench)
        try write("Diode quote", to: "quote.txt", in: diode)
        try write("Diode datasheet", to: "quotes/datasheet.txt", in: diode)
        try write("Mounts quote", to: "quote.txt", in: mounts)
        try write("Mounts drawing", to: "drawing.txt", in: mounts)
        session = try await openedSession(root)
        let rows = try await listedOrders(session)
        func row(_ title: String) throws -> OrderRow {
            try #require(rows.first { $0.values[OrderColumn.title.field] == title })
        }
        self.bench = try row("Bench")
        self.diode = try row("Diode")
        self.mounts = try row("Mounts")
    }
}

@Test func mergePlanListsMovedAndDiscardedEntries() async throws {
    let orders = try await ThreeOrders()
    defer { try? FileManager.default.removeItem(at: orders.root) }

    let merge = try await orders.session.planMergingOrders([orders.bench, orders.diode, orders.mounts],
                                                           cancellation: LibraryCancellation())

    #expect(merge.orders.map(\.id) == [orders.bench.id, orders.diode.id, orders.mounts.id])
    #expect(entries(merge.moved) == [[orders.diode.id, "quotes"], [orders.mounts.id, "drawing.txt"]])
    #expect(merge.moved.map(\.isFolder) == [true, false])
    #expect(entries(merge.discarded) == [
        [orders.diode.id, "README.md"], [orders.diode.id, "quote.txt"],
        [orders.mounts.id, "README.md"], [orders.mounts.id, "quote.txt"],
    ])
    #expect(merge.fillsKept)

    let reversed = try await orders.session.planMergingOrders([orders.mounts, orders.bench],
                                                              cancellation: LibraryCancellation())
    #expect(entries(reversed.moved).isEmpty)
    #expect(entries(reversed.discarded) == [[orders.bench.id, "README.md"], [orders.bench.id, "quote.txt"]])
    #expect(!reversed.fillsKept)
}

@Test func committedMergeKeepsTheFirstOrder() async throws {
    let orders = try await ThreeOrders()
    defer { try? FileManager.default.removeItem(at: orders.root) }
    let merge = try await orders.session.planMergingOrders([orders.bench, orders.diode, orders.mounts],
                                                           cancellation: LibraryCancellation())

    let committed = try await orders.session.commit(merge)

    #expect(committed.unremoved == 0)
    #expect(!committed.requiresReopen)
    let rows = try await listedOrders(orders.session)
    #expect(rows.map(\.id) == [orders.bench.id])
    let kept = try #require(rows.first).directory
    func text(_ path: String) throws -> String {
        try String(contentsOf: kept.appendingPathComponent(path), encoding: .utf8)
    }
    #expect(try text("quote.txt") == "Bench quote")
    #expect(try text("quotes/datasheet.txt") == "Diode datasheet")
    #expect(try text("drawing.txt") == "Mounts drawing")
    let draft = try await orders.session.orderDraft(rows[0], cancellation: LibraryCancellation())
    #expect(draft.rowCount == 1)
}

@Test func mergeRefusesWhenAMergedOrderChanged() async throws {
    let orders = try await ThreeOrders()
    defer { try? FileManager.default.removeItem(at: orders.root) }
    let merge = try await orders.session.planMergingOrders([orders.bench, orders.diode],
                                                           cancellation: LibraryCancellation())
    try write("Late invoice", to: "invoice.txt", in: orders.diode.directory)

    await #expect(throws: OrderMergeRefusal.self) {
        _ = try await orders.session.commit(merge)
    }

    let rows = try await listedOrders(try await openedSession(orders.root))
    #expect(Set(rows.map(\.id)) == [orders.bench.id, orders.diode.id, orders.mounts.id])
    #expect(!FileManager.default.fileExists(atPath: orders.bench.directory.appendingPathComponent("quotes").path))
}

@Test func mergeRefusesAsChangedWhenAMergedOrderIsGoneOrDamaged() async throws {
    let orders = try await ThreeOrders()
    defer { try? FileManager.default.removeItem(at: orders.root) }
    let removal = try await orders.session.planDirectoryChange(orders.diode, action: .remove,
                                                               cancellation: LibraryCancellation())
    _ = try await orders.session.commit(removal)
    try write("{", to: ".spielorders/order.json", in: orders.mounts.directory)

    for merged in [orders.diode, orders.mounts] {
        let refusal = await #expect(throws: OrderMergeRefusal.self) {
            try await orders.session.planMergingOrders([orders.bench, merged], cancellation: LibraryCancellation())
        }
        #expect(refusal?.summary
            == "The Orders changed after the merge was shown. Nothing was merged; review the merge again.")
    }
}

@Test(arguments: [true, false], [("date_ordered", "2026-02-30"), ("package_root", "not-a-locator"),
                               ("package_root", "../../..")])
func mergeRefusesAsChangedWhenAnOrderLosesItsMeaning(kept: Bool, fact: (String, String)) async throws {
    let orders = try await ThreeOrders()
    defer { try? FileManager.default.removeItem(at: orders.root) }
    let changed = kept ? orders.bench : orders.diode
    let record = changed.directory.appendingPathComponent(".spielorders/order.json")
    var document = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: record)) as? [String: Any])
    document[fact.0] = fact.1
    try JSONSerialization.data(withJSONObject: document, options: .sortedKeys).write(to: record)

    let refusal = await #expect(throws: OrderMergeRefusal.self) {
        try await orders.session.planMergingOrders([orders.bench, orders.diode], cancellation: LibraryCancellation())
    }

    #expect(refusal?.summary
        == "The Orders changed after the merge was shown. Nothing was merged; review the merge again.")
}

@Test func mergeRefusalNamesAnOrderPricedInAnotherCurrency() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Bench", itemPrice: "10.00", currency: "EUR")
    _ = try createOrder(in: root, title: "Diode", itemPrice: "12.00", currency: "USD")
    let session = try await openedSession(root)
    let rows = try await listedOrders(session)
    let bench = try #require(rows.first { $0.name == "Bench" })
    let diode = try #require(rows.first { $0.name == "Diode" })

    let refusal = await #expect(throws: OrderMergeRefusal.self) {
        try await session.planMergingOrders([bench, diode], cancellation: LibraryCancellation())
    }

    #expect(refusal?.summary == "“Diode” prices its Items in another currency, so it can’t be merged into “Bench”.")
    #expect(refusal?.localizedDescription.contains("currency_differs") == true)
}
