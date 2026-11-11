import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

private func listed(_ session: PackageSession) async throws -> [OrderRow] {
    try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                             cancellation: LibraryCancellation()).rows
}

@Test func directoryChangesPreviewBeforeWritingAndKeepTheWholeOrderRecoverable() async throws {
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

@Test func collisionAndChangedEvidenceRefuseWithoutMovingOrOverwriting() async throws {
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

@Test func aCancelledDirectoryPlanWritesNothing() async throws {
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
