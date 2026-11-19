import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

@Test func vendorInspection() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let id = try createVendor(in: root, name: "Optics supplier", contactEmail: "sales@example.invalid")
    _ = try createOrder(in: root, title: "Lens purchase", vendor: "Optics supplier")
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let listed = try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: -1, descending: false,
                                           cancellation: LibraryCancellation())
    let row = try #require(listed.rows.first)
    #expect(row.id == id)
    #expect(row.name == "Optics supplier")
    let details = try await session.inspectVendor(row, cancellation: LibraryCancellation())
    #expect(details.values[SPIELORDERS_VENDOR_FIELD_NAME] == "Optics supplier")
    let draft = try await session.vendorDraft(id: row.id, cancellation: LibraryCancellation())
    #expect(draft.rowField(0, SPIELORDERS_CONTACT_FIELD_EMAIL) == "sales@example.invalid")
    #expect(details.orders.first?.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Lens purchase")
}

@Test func vendorIdentity() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let id = try createVendor(in: root, name: "Original supplier")
    let session = try await openedSession(root)
    let selected = try #require(try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: -1,
                                                          descending: false, cancellation: LibraryCancellation()).rows.first)
    let elsewhere = try await openedSession(root)
    let rename = try await elsewhere.vendorDraft(id: id, cancellation: LibraryCancellation())
    try rename.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Renamed supplier")
    _ = try await elsewhere.save(rename, cancellation: LibraryCancellation())
    let replacement = try createVendor(in: root, name: "Original supplier")
    #expect(replacement != selected.id)
    let inspected = try await session.inspectVendor(selected, cancellation: LibraryCancellation())
    #expect(inspected.values[SPIELORDERS_VENDOR_FIELD_VENDOR_ID] == id)
    #expect(inspected.values[SPIELORDERS_VENDOR_FIELD_NAME] == "Renamed supplier")
}

@Test func hiddenVendors() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Alpha")
    let hidden = try createVendor(in: root, name: "Zulu")
    _ = try createOrder(in: root, title: "Windows", vendor: "Zulu")
    let session = try await openedSession(root)
    func rows(includeHidden: Bool, descending: Bool) async throws -> [VendorRow] {
        try await session.vendors(query: "", caseSensitive: false, includeHidden: includeHidden,
                                  sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: descending,
                                  cancellation: LibraryCancellation()).rows
    }

    _ = try await session.setVendorHidden(id: hidden, hidden: true, cancellation: LibraryCancellation())
    #expect(try await rows(includeHidden: false, descending: false).map(\.name) == ["Alpha"])
    let all = try await rows(includeHidden: true, descending: true)
    #expect(all.map(\.name) == ["Zulu", "Alpha"])
    let row = try #require(all.first)
    #expect(row.hidden)
    #expect(try await session.inspectVendor(row, cancellation: LibraryCancellation()).values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true")
    #expect(try await listedOrders(session).first?.values[SPIELORDERS_ORDER_FIELD_VENDOR] == "Zulu")

    _ = try await session.setVendorHidden(id: hidden, hidden: false, cancellation: LibraryCancellation())
    #expect(try await rows(includeHidden: false, descending: false).map(\.name) == ["Alpha", "Zulu"])
}
