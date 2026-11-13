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

@Test func adapterAgreement() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let vendor = try await session.newVendorDraft()
    try vendor.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Shared Optics")
    try vendor.insertRow(at: 0)
    try vendor.setRow(0, SPIELORDERS_CONTACT_FIELD_NAME, to: "Sales")
    try vendor.setRow(0, SPIELORDERS_CONTACT_FIELD_EMAIL, to: "sales@example.org")
    let savedVendor = try await session.save(vendor, cancellation: LibraryCancellation())
    let order = try await session.newOrderDraft()
    try order.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Precision optics")
    try order.set(SPIELORDERS_ORDER_FIELD_VENDOR, to: "Shared Optics")
    try order.insertRow(at: 0)
    try order.setRow(0, SPIELORDERS_ITEM_FIELD_DESCRIPTION, to: "Window")
    try order.setRow(0, SPIELORDERS_ITEM_FIELD_QUANTITY, to: "3")
    try order.setRow(0, SPIELORDERS_ITEM_FIELD_UNIT_PRICE, to: "0.12345")
    let savedOrder = try await session.save(order, cancellation: LibraryCancellation())
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1,
                                                   descending: false, cancellation: LibraryCancellation()).rows.first)
    let shown = try cli(["order", "show"], root: row.directory)
    #expect(shown["title"] as? String == order.field(SPIELORDERS_ORDER_FIELD_TITLE))
    #expect(shown["total"] as? String == order.total)
    #expect(shown["vendor_id"] as? String == savedVendor.recordID)
    #expect(row.id == savedOrder.recordID)
    #expect(shown["order_id"] as? String == row.values[SPIELORDERS_ORDER_FIELD_ORDER_ID])
    let items = try #require(shown["items"] as? [[String: Any]])
    #expect(items.count == 1)
    #expect(items.first?["quantity"] as? String == order.rowField(0, SPIELORDERS_ITEM_FIELD_QUANTITY))
    #expect(items.first?["unit_price"] as? String == order.rowField(0, SPIELORDERS_ITEM_FIELD_UNIT_PRICE))
    let sharedVendor = try cli(["vendor", "show", "Shared Optics"], root: root)
    let contacts = try #require(sharedVendor["contacts"] as? [[String: Any]])
    #expect(contacts.first?["email"] as? String == "sales@example.org")

    _ = try cli(["vendor", "contact", "edit", "Shared Optics", "1", "email=updated@example.org"], root: root)
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let vendorRow = try #require(try await session.vendors(query: "", caseSensitive: false, includeHidden: false,
                                                         sortField: -1, descending: false,
                                                         cancellation: LibraryCancellation()).rows.first)
    let updated = try await session.vendorDraft(id: vendorRow.id, cancellation: LibraryCancellation())
    #expect(updated.rowField(0, SPIELORDERS_CONTACT_FIELD_EMAIL) == "updated@example.org")
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
