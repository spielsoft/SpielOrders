import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func initialization() async throws {
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
    #expect(try await session.readme(of: nil, cancellation: LibraryCancellation()).readable)
    #expect(try Data(contentsOf: supporting) == Data("original quote".utf8))
    let order = try createOrder(in: root, title: "Native initialization")
    #expect(try FileManager.default.contentsOfDirectory(atPath: orders.path).contains(order.lastPathComponent))
    let reopened = PackageSession()
    #expect(try await reopened.open(root, cancellation: LibraryCancellation()) == "# Existing collection\n")
}

@Test func initializationRefusal() async throws {
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
    let saved = try await session.saveReadme("# Still active\n", of: nil, cancellation: LibraryCancellation())
    #expect(saved.canEdit)
    #expect(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8) == "# Still active\n")
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("Other Orders").path))
}

@Test func unreadableInitialization() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: root) }
    let readme = root.appendingPathComponent("README.md")
    let original = Data([0xff, 0xfe])
    try original.write(to: readme)
    let session = PackageSession()
    let initialized = try await session.initialize(root, ordersPath: "", removedPath: "",
                                                 cancellation: LibraryCancellation())
    #expect(initialized.canBrowseRecords)
    #expect(initialized.readmeText == nil)
    #expect(!initialized.message.contains("before continuing"))
    #expect(try await session.readme(of: nil, cancellation: LibraryCancellation()).readable == false)
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Independent of the README")
    let saved = try await session.save(draft, cancellation: LibraryCancellation())
    #expect(!saved.requiresReopen)
    #expect(try Data(contentsOf: readme) == original)
    #expect(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                    descending: false, cancellation: LibraryCancellation()).rows.map(\.id) == [saved.recordID])
}

@Test func replacementOpen() async throws {
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
    let metadata = root.appendingPathComponent(".spielorders")
    let held = root.appendingPathComponent("held-metadata")
    try FileManager.default.moveItem(at: metadata, to: held)
    do {
        _ = try await session.open(root, cancellation: LibraryCancellation())
        Issue.record("A directory without Package metadata must refuse")
    } catch let failure as LibraryFailure {
        #expect(failure.result == SPIELORDERS_RESULT_REFUSED)
    }
    try FileManager.default.moveItem(at: held, to: metadata)
    _ = try await session.saveReadme("# Current session\n", of: nil, cancellation: LibraryCancellation())
    try "# External writer\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
    #expect(try await session.open(root, cancellation: LibraryCancellation()) == "# External writer\n")
    _ = try await session.saveReadme("# Refreshed original\n", of: nil, cancellation: LibraryCancellation())
    #expect(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8) == "# Refreshed original\n")
}
