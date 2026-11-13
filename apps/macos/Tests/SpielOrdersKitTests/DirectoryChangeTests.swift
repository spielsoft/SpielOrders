import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

private func listed(_ session: PackageSession) async throws -> [OrderRow] {
    try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                             cancellation: LibraryCancellation()).rows
}

@Test func directoryChanges() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let evidence = directory.appendingPathComponent("quote.bin")
    let bytes = Data([0, 127, 255, 11])
    try bytes.write(to: evidence)
    let renamed = directory.deletingLastPathComponent().appendingPathComponent("Unaligned")
    try FileManager.default.moveItem(at: directory, to: renamed)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await listed(session).first)
    let alignment = try await session.planDirectoryChange(row, action: .align, cancellation: LibraryCancellation())
    #expect(!alignment.operations.isEmpty)
    #expect(FileManager.default.fileExists(atPath: renamed.path))
    #expect(!FileManager.default.fileExists(atPath: directory.path))
    _ = try await session.commit(alignment)
    #expect(try Data(contentsOf: evidence) == bytes)
    #expect(!FileManager.default.fileExists(atPath: renamed.path))
    let aligned = try #require(try await listed(session).first)
    let removal = try await session.planDirectoryChange(aligned, action: .remove, cancellation: LibraryCancellation())
    #expect(FileManager.default.fileExists(atPath: directory.path))
    let removed = try await session.commit(removal)
    let recovery = try #require(removed.finalDirectory)
    #expect(try Data(contentsOf: recovery.appendingPathComponent("quote.bin")) == bytes)
    #expect(try await listed(session).isEmpty)
    #expect(removed.message.contains("recoverable"))
    #expect(!removed.requiresReopen)
}

@Test func staleDirectoryPlan() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let original = try createOrder(in: root)
    let moved = original.deletingLastPathComponent().appendingPathComponent("Different")
    try FileManager.default.moveItem(at: original, to: moved)
    try FileManager.default.createDirectory(at: original, withIntermediateDirectories: false)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await listed(session).first { $0.damage == nil })
    await #expect(throws: (any Error).self) {
        try await session.planDirectoryChange(row, action: .align, cancellation: LibraryCancellation())
    }
    let plan = try await session.planDirectoryChange(row, action: .remove, cancellation: LibraryCancellation())
    let metadata = moved.appendingPathComponent(".spielorders/order.json")
    var changed = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: metadata)) as? [String: Any])
    changed["title"] = "Changed after preview"
    let external = try JSONSerialization.data(withJSONObject: changed, options: [.sortedKeys])
    try external.write(to: metadata)
    await #expect(throws: (any Error).self) { try await session.commit(plan) }
    #expect(try Data(contentsOf: metadata) == external)
    #expect(FileManager.default.fileExists(atPath: moved.path))
}

@Test func cancelledDirectoryPlan() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await listed(session).first)
    let cancellation = LibraryCancellation()
    cancellation.request()
    await #expect(throws: (any Error).self) {
        try await session.planDirectoryChange(row, action: .remove, cancellation: cancellation)
    }
    #expect(FileManager.default.fileExists(atPath: directory.path))
}

private func details(_ session: PackageSession) async throws -> OrderDetails? {
    let row = try #require(try await listed(session).first)
    guard case let .healthy(details) = try await session.inspectOrder(row, cancellation: LibraryCancellation()) else {
        return nil
    }
    return details
}

@Test func directoryDrift() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Mirror mounts")
    let session = try await openedSession(root)
    #expect(try await details(session)?.drifted == false)
    let row = try #require(try await listed(session).first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Kinematic mirror mounts")
    _ = try await session.save(draft, cancellation: LibraryCancellation())
    #expect(try await details(session)?.drifted == true)
    let edited = try #require(try await listed(session).first)
    let alignment = try await session.planDirectoryChange(edited, action: .align, cancellation: LibraryCancellation())
    _ = try await session.commit(alignment)
    #expect(try await details(session)?.drifted == false)
}

@Test func removalRestore() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root, title: "Beam splitter")
    let session = try await openedSession(root)
    let row = try #require(try await listed(session).first)
    let removal = try await session.planDirectoryChange(row, action: .remove, cancellation: LibraryCancellation())
    let removed = try #require(try await session.commit(removal).finalDirectory)
    #expect(try await listed(session).isEmpty)

    let restored = try await session.restoreOrder(id: row.id, from: removed, cancellation: LibraryCancellation())
    #expect(!restored.requiresReopen)
    #expect(restored.finalDirectory?.lastPathComponent == directory.lastPathComponent)
    #expect(try await listed(session).map(\.id) == [row.id])
    #expect(!FileManager.default.fileExists(atPath: removed.path))

    // The same removal cannot be restored twice; the restored Order stays as it is.
    await #expect(throws: RestoreRefusal.self) {
        try await session.restoreOrder(id: row.id, from: removed, cancellation: LibraryCancellation())
    }
    #expect(try await listed(session).map(\.id) == [row.id])
}

@Test func occupiedRestore() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root, title: "Beam splitter")
    let session = try await openedSession(root)
    let row = try #require(try await listed(session).first)
    let removal = try await session.planDirectoryChange(row, action: .remove, cancellation: LibraryCancellation())
    let removed = try #require(try await session.commit(removal).finalDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)

    do {
        _ = try await session.restoreOrder(id: row.id, from: removed, cancellation: LibraryCancellation())
        Issue.record("A restore onto an occupied name must refuse")
    } catch let refusal as RestoreRefusal {
        #expect(refusal.summary.contains("already holds a folder with its name"))
    }
    #expect(FileManager.default.fileExists(atPath: removed.path))
}

@Test func replacedRemoval() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "First optics")
    _ = try createOrder(in: root, title: "Second optics")
    let session = try await openedSession(root)
    let rows = try await listed(session)
    let first = try #require(rows.first { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] == "First optics" })
    let second = try #require(rows.first { $0.id != first.id })
    let firstPlan = try await session.planDirectoryChange(first, action: .remove, cancellation: LibraryCancellation())
    let firstPath = try #require(try await session.commit(firstPlan).finalDirectory)
    let secondPlan = try await session.planDirectoryChange(second, action: .remove, cancellation: LibraryCancellation())
    let secondPath = try #require(try await session.commit(secondPlan).finalDirectory)
    try FileManager.default.moveItem(at: firstPath, to: root.appendingPathComponent("parked"))
    try FileManager.default.moveItem(at: secondPath, to: firstPath)

    await #expect(throws: (any Error).self) {
        try await session.restoreOrder(id: first.id, from: firstPath, cancellation: LibraryCancellation())
    }
    #expect(try await listed(session).isEmpty)
    #expect(FileManager.default.fileExists(atPath: firstPath.path))
    _ = try await session.restoreOrder(id: second.id, from: firstPath, cancellation: LibraryCancellation())
    #expect(try await listed(session).map(\.id) == [second.id])
}

@Test func directoryWarnings() async throws {
    let root = try initializedPackage()
    let metadata = root.appendingPathComponent(".spielorders")
    defer {
        try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: metadata.path)
        try? FileManager.default.removeItem(at: root)
    }
    _ = try createOrder(in: root, title: "Initial optics")
    let session = try await openedSession(root)
    let row = try #require(try await listed(session).first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Projected optics")
    _ = try await session.save(draft, cancellation: LibraryCancellation())
    let plan = try await session.planDirectoryChange(row, action: .align, cancellation: LibraryCancellation())
    try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: metadata.path)
    let changed = try await session.commit(plan)
    #expect(!changed.requiresReopen)
    #expect(changed.notices.contains { $0.contains("index_refresh_failed") })
    #expect(changed.finalDirectory?.lastPathComponent.contains("Projected_optics") == true)
}
