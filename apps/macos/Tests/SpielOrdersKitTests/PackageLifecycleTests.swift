import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func initializationPreservesExistingContentAndUsesTheSelectedOrdersDirectory() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: root) }
    let orders = root.appendingPathComponent("All Orders", isDirectory: true)
    try FileManager.default.createDirectory(at: orders, withIntermediateDirectories: false)
    let supporting = orders.appendingPathComponent("existing-quote.txt")
    try Data("original quote".utf8).write(to: supporting)
    try "# Existing collection\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)

    let session = PackageSession()
    let result = try await session.initialize(root, ordersPath: "All Orders", removedPath: "Removed",
                                              cancellation: LibraryCancellation())

    #expect(result.readmeText == "# Existing collection\n")
    #expect(await session.readmeWritable)
    #expect(try Data(contentsOf: supporting) == Data("original quote".utf8))
    let order = try createOrder(in: root, title: "Native initialization")
    #expect(try FileManager.default.contentsOfDirectory(atPath: orders.path).contains(order.lastPathComponent))
    let reopened = PackageSession()
    #expect(try await reopened.open(root, cancellation: LibraryCancellation()) == "# Existing collection\n")
}

@Test func refusedInitializationPreservesTheCurrentPackageAndItsGuardedReadme() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    do {
        _ = try await session.initialize(root, ordersPath: "Other Orders", removedPath: "Other Removed",
                                         cancellation: LibraryCancellation())
        Issue.record("An existing Package must not be initialized again")
    } catch let failure as LibraryFailure {
        #expect(failure.result == SPIELORDERS_RESULT_REFUSED)
    }
    let saved = try await session.saveReadme("# Still active\n", cancellation: LibraryCancellation())
    #expect(saved.canEdit)
    #expect(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8) == "# Still active\n")
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("Other Orders").path))
}

@Test func failedReplacementOpenKeepsTheOriginalReadAndReopenSeesExternalChanges() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let absent = root.appendingPathComponent("absent")
    do {
        _ = try await session.open(absent, cancellation: LibraryCancellation())
        Issue.record("An absent replacement root must refuse")
    } catch let failure as LibraryFailure {
        #expect(failure.result == SPIELORDERS_RESULT_REFUSED)
    }
    _ = try await session.saveReadme("# Current session\n", cancellation: LibraryCancellation())
    try "# External writer\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
    #expect(try await session.open(root, cancellation: LibraryCancellation()) == "# External writer\n")
    _ = try await session.saveReadme("# Refreshed original\n", cancellation: LibraryCancellation())
    #expect(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8) == "# Refreshed original\n")
}
