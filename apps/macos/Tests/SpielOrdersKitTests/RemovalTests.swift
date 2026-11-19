import Foundation
import SpielOrdersKit
import Testing

/// Sets or clears (`-N`) `directory`'s access control list with chmod.
private func chmod(_ arguments: [String], _ directory: URL) throws {
    let process = try Process.run(URL(fileURLWithPath: "/bin/chmod"), arguments: arguments + [directory.path])
    process.waitUntilExit()
    #expect(process.terminationStatus == 0)
}

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test func trashRecovery() async throws {
    let root = try initializedPackage()
    let removed = root.appendingPathComponent("Removed")
    defer {
        try? chmod(["-N"], removed)
        try? FileManager.default.removeItem(at: root)
    }
    _ = try createOrder(in: root, title: "Bench")
    // An Order can enter Removed Orders but nothing can leave it, so the Trash move fails.
    try FileManager.default.createDirectory(at: removed, withIntermediateDirectories: false)
    try chmod(["+a", "everyone deny delete_child"], removed)
    let session = try await openedSession(root)
    let row = try #require(try await listedOrders(session).first)

    let change = try await session.planDirectoryChange(row, action: .trash, cancellation: LibraryCancellation())
    let outcome = try await session.commit(change)

    let recovery = try #require(outcome.finalDirectory)
    #expect(recovery.deletingLastPathComponent().standardizedFileURL == removed.standardizedFileURL)
    #expect(FileManager.default.fileExists(atPath: recovery.appendingPathComponent(".spielorders/order.json").path))
    #expect(outcome.summary == "Order removed, but the Trash move failed. The Order remains recoverable.")
    #expect(outcome.message.contains("remains recoverable at " + recovery.path))
    #expect(try await listedOrders(session).isEmpty)
}
