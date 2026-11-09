import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test func aReadmeSavedThroughSwiftIsReadBackThroughTheLibrary() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let editor = PackageSession()
    let initial = try await editor.open(root, cancellation: LibraryCancellation())
    #expect(initial == "")
    let committed = try await editor.saveReadme("# Native purchasing\n", cancellation: LibraryCancellation())
    #expect(committed.canEdit)
    #expect(committed.message == "Saved.")
    let reader = PackageSession()
    let saved = try await reader.open(root, cancellation: LibraryCancellation())
    #expect(saved == "# Native purchasing\n")
    _ = try await editor.saveReadme("# A second edit\n", cancellation: LibraryCancellation())
    #expect(try await reader.open(root, cancellation: LibraryCancellation()) == "# A second edit\n")
}

@Test func aStaleReadmeSavePreservesTheOtherWritersText() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let editor = PackageSession()
    _ = try await editor.open(root, cancellation: LibraryCancellation())
    try "# Another writer\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
    do {
        _ = try await editor.saveReadme("# My draft\n", cancellation: LibraryCancellation())
        Issue.record("a stale original must not authorize overwriting another writer")
    } catch let refusal as LibraryFailure {
        #expect(refusal.result == SPIELORDERS_RESULT_REFUSED)
    }
    let reader = PackageSession()
    #expect(try await reader.open(root, cancellation: LibraryCancellation()) == "# Another writer\n")
}
