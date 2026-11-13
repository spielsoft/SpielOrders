import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func readmeTarget() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Optics notes")
    let editor = PackageSession()
    _ = try await editor.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await editor.orders(query: "", caseSensitive: false, sortField: -1,
                                                   descending: false, cancellation: LibraryCancellation()).rows.first)
    #expect(try await editor.readme(of: row, cancellation: LibraryCancellation()).text == "")
    let saved = try await editor.saveReadme("# Order notes\n", of: row, cancellation: LibraryCancellation())
    #expect(saved.refreshedText == "# Order notes\n")
    if case let .healthy(details) = try await editor.inspectOrder(row, cancellation: LibraryCancellation()) {
        #expect(details.readmeText == "# Order notes\n")
    } else { Issue.record("The Order must remain readable after its README save") }
    #expect(try await editor.readme(of: nil, cancellation: LibraryCancellation()).text == "")
    let packageSaved = try await editor.saveReadme("# Package notes\n", of: nil, cancellation: LibraryCancellation())
    #expect(packageSaved.refreshedText == "# Package notes\n")
    if case let .healthy(details) = try await editor.inspectOrder(row, cancellation: LibraryCancellation()) {
        #expect(details.readmeText == "# Order notes\n")
    }
}

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test func readmeRoundTrip() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let editor = PackageSession()
    let initial = try await editor.open(root, cancellation: LibraryCancellation())
    #expect(initial == "")
    let committed = try await editor.saveReadme("# Native purchasing\n", of: nil, cancellation: LibraryCancellation())
    #expect(committed.canEdit)
    #expect(committed.message == "Saved.")
    let reader = PackageSession()
    let saved = try await reader.open(root, cancellation: LibraryCancellation())
    #expect(saved == "# Native purchasing\n")
    _ = try await editor.saveReadme("# A second edit\n", of: nil, cancellation: LibraryCancellation())
    #expect(try await reader.open(root, cancellation: LibraryCancellation()) == "# A second edit\n")
}

@Test func staleReadme() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let editor = PackageSession()
    _ = try await editor.open(root, cancellation: LibraryCancellation())
    try "# Another writer\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
    do {
        _ = try await editor.saveReadme("# My draft\n", of: nil, cancellation: LibraryCancellation())
        Issue.record("a stale original must not authorize overwriting another writer")
    } catch let refusal as LibraryFailure {
        #expect(refusal.result == SPIELORDERS_RESULT_REFUSED)
    }
    let reader = PackageSession()
    #expect(try await reader.open(root, cancellation: LibraryCancellation()) == "# Another writer\n")
}

@Test(arguments: ["stale", "nul", "cancelled"])
func readmeRefusal(reason: String) async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root, title: "Guarded notes")
    let path = directory.appendingPathComponent("README.md")
    try "Original".write(to: path, atomically: true, encoding: .utf8)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                    descending: false, cancellation: LibraryCancellation()).rows.first)
    #expect(try await session.readme(of: row, cancellation: LibraryCancellation()).text == "Original")
    let cancellation = LibraryCancellation()
    let intended = reason == "nul" ? "My\0draft" : "My draft"
    let expected = reason == "stale" ? "Another writer" : "Original"
    if reason == "stale" { try expected.write(to: path, atomically: true, encoding: .utf8) }
    if reason == "cancelled" { cancellation.request() }
    do {
        _ = try await session.saveReadme(intended, of: row, cancellation: cancellation)
        Issue.record("The save must refuse without replacing the current README")
    } catch let refusal as LibraryFailure {
        let expectedResult = reason == "nul" ? SPIELORDERS_RESULT_BAD_ARGUMENT
            : reason == "cancelled" ? SPIELORDERS_RESULT_CANCELLED : SPIELORDERS_RESULT_REFUSED
        #expect(refusal.result == expectedResult)
    }
    if case let .healthy(details) = try await session.inspectOrder(row, cancellation: LibraryCancellation()) {
        #expect(details.readmeText == expected)
    } else { Issue.record("A refused README save must not damage the Order") }
}

@Test func readmeWarnings() async throws {
    let root = try initializedPackage()
    let metadata = root.appendingPathComponent(".spielorders")
    defer {
        try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: metadata.path)
        try? FileManager.default.removeItem(at: root)
    }
    _ = try createOrder(in: root, title: "Warning notes")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                    descending: false, cancellation: LibraryCancellation()).rows.first)
    _ = try await session.readme(of: row, cancellation: LibraryCancellation())
    try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: metadata.path)
    let saved = try await session.saveReadme("# Committed notes\n", of: row, cancellation: LibraryCancellation())
    #expect(saved.canEdit)
    #expect(saved.hasWarnings)
    #expect(saved.refreshedText == "# Committed notes\n")
    #expect(saved.message.hasPrefix("Saved with warnings."))
    if case let .healthy(details) = try await session.inspectOrder(row, cancellation: LibraryCancellation()) {
        #expect(details.readmeText == "# Committed notes\n")
    } else { Issue.record("An index warning must not turn the committed README into damage") }
}

// Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.
@Test func orderReadmeBesideFacts() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Shared directory")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                    descending: false, cancellation: LibraryCancellation()).rows.first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    _ = try await session.readme(of: row, cancellation: LibraryCancellation())
    _ = try await session.saveReadme("# Notes first\n", of: row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Renamed beside its notes")
    let saved = try await session.save(draft, cancellation: LibraryCancellation())
    #expect(!saved.requiresReopen)
    let notes = try await session.saveReadme("# Notes after facts\n", of: row, cancellation: LibraryCancellation())
    #expect(notes.canEdit)
    if case let .healthy(details) = try await session.inspectOrder(row, cancellation: LibraryCancellation()) {
        #expect(details.readmeText == "# Notes after facts\n")
    } else { Issue.record("Neither save may damage the Order") }
    let current = try await session.orderDraft(row, cancellation: LibraryCancellation())
    #expect(current.field(SPIELORDERS_ORDER_FIELD_TITLE) == "Renamed beside its notes")
}

@Test func readmesKeepTheirOwnOriginals() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let first = try createOrder(in: root, title: "First notes")
    let second = try createOrder(in: root, title: "Second notes")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let rows = try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                        cancellation: LibraryCancellation()).rows
    let firstRow = try #require(rows.first { $0.directory.lastPathComponent == first.lastPathComponent })
    let secondRow = try #require(rows.first { $0.directory.lastPathComponent == second.lastPathComponent })
    let secondReadme = second.appendingPathComponent("README.md")
    let secondOriginal = try? Data(contentsOf: secondReadme)
    _ = try await session.readme(of: firstRow, cancellation: LibraryCancellation())
    _ = try await session.readme(of: secondRow, cancellation: LibraryCancellation())
    _ = try await session.saveReadme("# First only\n", of: firstRow, cancellation: LibraryCancellation())
    _ = try await session.saveReadme("# Package only\n", of: nil, cancellation: LibraryCancellation())
    #expect(try String(contentsOf: first.appendingPathComponent("README.md"), encoding: .utf8) == "# First only\n")
    #expect((try? Data(contentsOf: secondReadme)) == secondOriginal)
    #expect(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8) == "# Package only\n")
}
