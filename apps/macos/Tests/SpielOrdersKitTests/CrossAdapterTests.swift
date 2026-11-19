import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

/// Runs the portable CLI built by the repository check, not the installed app's CLI.
private func cli(_ arguments: [String], root: URL) throws -> [String: Any] {
    var repository = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { repository.deleteLastPathComponent() }
    let process = Process()
    process.executableURL = repository.appendingPathComponent("target/debug/spielorders")
    process.arguments = arguments + ["--root", root.path, "--json"]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = output
    try process.run()
    let bytes = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    #expect(process.terminationStatus == 0, "\(String(decoding: bytes, as: UTF8.self))")
    let response = try #require(try JSONSerialization.jsonObject(with: bytes) as? [String: Any])
    return try #require(response["data"] as? [String: Any])
}

@Test func staleAdapterDrafts() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let vendorID = try createVendor(in: root, name: "Optics", contactEmail: "sales@example.org")
    let directory = try createOrder(in: root, title: "Original title", vendor: "Optics")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                   descending: false, cancellation: LibraryCancellation()).rows.first)
    let order = try await session.orderDraft(row, cancellation: LibraryCancellation())
    let vendor = try await session.vendorDraft(id: vendorID, cancellation: LibraryCancellation())
    try order.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Unsaved Swift title")
    try vendor.set(SPIELORDERS_VENDOR_FIELD_PHONE, to: "Unsaved Swift phone")
    _ = try cli(["order", "edit", "title=CLI title"], root: directory)
    _ = try cli(["vendor", "edit", "Optics", "phone=CLI phone"], root: root)
    for draft in [order, vendor] {
        do {
            _ = try await session.save(draft, cancellation: LibraryCancellation())
            Issue.record("A stale Swift draft must not overwrite the CLI writer")
        } catch let refusal as SaveRefusal {
            #expect(refusal.stale)
        }
    }
    #expect(order.field(SPIELORDERS_ORDER_FIELD_TITLE) == "Unsaved Swift title")
    #expect(vendor.field(SPIELORDERS_VENDOR_FIELD_PHONE) == "Unsaved Swift phone")
    #expect(try cli(["order", "show"], root: directory)["title"] as? String == "CLI title")
    #expect(try cli(["vendor", "show", "Optics"], root: root)["phone"] as? String == "CLI phone")
    _ = try await session.open(root, cancellation: LibraryCancellation())
    #expect(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                     cancellation: LibraryCancellation()).rows.first?.values[SPIELORDERS_ORDER_FIELD_TITLE] == "CLI title")
    #expect(try await session.vendorDraft(id: vendorID, cancellation: LibraryCancellation()).field(SPIELORDERS_VENDOR_FIELD_PHONE) == "CLI phone")
}
