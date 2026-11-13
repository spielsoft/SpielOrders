import Foundation
import SpielOrdersKit
import Testing

// Decision source: Desktop acceptance, application lifecycle.
@Test @MainActor func saveSettlement() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let operation = OwnedOperation(kind: .mutation) {
        do {
            try await Task.sleep(for: .milliseconds(20))
            _ = try await session.saveReadme("Saved before exit", of: nil, cancellation: LibraryCancellation())
        } catch { Issue.record("Exit interrupted an approved Save: \(error)") }
    }
    await operation.settleForExit()
    #expect(try await PackageSession().open(root, cancellation: LibraryCancellation()) == "Saved before exit")
}

@Test @MainActor func waitingForReadDoesNotCancelIt() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    var restored: String?
    let operation = OwnedOperation(kind: .read) {
        do {
            await Task.yield()
            try Task.checkCancellation()
            restored = try await PackageSession().open(root, cancellation: LibraryCancellation())
        } catch { Issue.record("Waiting cancelled a live Package read: \(error)") }
    }
    await operation.wait()
    #expect(restored != nil)
}
