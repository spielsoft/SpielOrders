import Foundation
import SpielOrdersKit
import Testing

// Decision source: Desktop acceptance, application lifecycle.
@Test @MainActor func approvingLeaveWaitsForAnOwnedSaveWithoutCancellingIt() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let operation = OwnedOperation(kind: .mutation) {
        do {
            try await Task.sleep(for: .milliseconds(20))
            _ = try await session.saveReadme("Saved before exit", cancellation: LibraryCancellation())
        } catch { Issue.record("Exit interrupted an approved Save: \(error)") }
    }
    await operation.settleForExit()
    #expect(try await PackageSession().open(root, cancellation: LibraryCancellation()) == "Saved before exit")
}
