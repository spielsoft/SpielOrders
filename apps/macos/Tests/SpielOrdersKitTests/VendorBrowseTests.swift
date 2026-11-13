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
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let selected = try #require(try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: -1, descending: false,
                                                          cancellation: LibraryCancellation()).rows.first)
    try withVendor(in: root, id: id) { vendor in
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        var plan: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        defer { spielorders_report_free(report); spielorders_plan_free(plan); spielorders_draft_free(draft) }
        #expect(spielorders_vendor_draft(vendor, &draft) == SPIELORDERS_RESULT_OK)
        let edited = try #require(draft)
        #expect(spielorders_draft_set(edited, SPIELORDERS_VENDOR_FIELD_NAME, "Renamed supplier", nil) == SPIELORDERS_RESULT_OK)
        #expect(spielorders_draft_plan(edited, 0, &plan, nil) == SPIELORDERS_RESULT_OK)
        let planned = try #require(plan)
        #expect(spielorders_plan_commit(planned, &report, nil) == SPIELORDERS_RESULT_OK)
    }
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
    try withVendor(in: root, id: hidden) { vendor in
        var plan: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        defer { spielorders_report_free(report); spielorders_plan_free(plan) }
        #expect(spielorders_vendor_plan_hidden(vendor, 1, 0, &plan, nil) == SPIELORDERS_RESULT_OK)
        let planned = try #require(plan)
        #expect(spielorders_plan_commit(planned, &report, nil) == SPIELORDERS_RESULT_OK)
    }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let ordinary = try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: SPIELORDERS_VENDOR_FIELD_NAME,
                                              descending: false, cancellation: LibraryCancellation())
    #expect(ordinary.rows.map(\.name) == ["Alpha"])
    let all = try await session.vendors(query: "", caseSensitive: false, includeHidden: true, sortField: SPIELORDERS_VENDOR_FIELD_NAME,
                                         descending: true, cancellation: LibraryCancellation())
    #expect(all.rows.map(\.name) == ["Zulu", "Alpha"])
    let row = try #require(all.rows.first)
    #expect(row.hidden)
    #expect(try await session.inspectVendor(row, cancellation: LibraryCancellation()).values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true")
}

private func withVendor(in root: URL, id: String,
                        _ body: (UnsafeMutablePointer<SpielordersVendor>) throws -> Void) throws {
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var vendor: UnsafeMutablePointer<SpielordersVendor>?
    defer { spielorders_vendor_free(vendor); spielorders_package_free(package) }
    #expect(root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) } == SPIELORDERS_RESULT_OK)
    #expect(id.withCString {
        spielorders_package_read_vendor(package!, SPIELORDERS_VENDOR_SELECTOR_ID, $0, 0, &vendor, nil)
    } == SPIELORDERS_RESULT_OK)
    try body(try #require(vendor))
}
