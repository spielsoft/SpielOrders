import CSpielOrders
import Foundation
import Testing
@testable import SpielOrdersKit

// Decision sources: Prototype the SpielOrders desktop experience, Order and Vendor editing;
// Effect and adapter boundaries.

private func openedSession(_ root: URL) async throws -> PackageSession {
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    return session
}

private func listedOrders(_ session: PackageSession) async throws -> [OrderRow] {
    try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                             cancellation: LibraryCancellation()).rows
}

private func orderDirectories(_ root: URL) throws -> [String] {
    try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path)
        .filter { !$0.hasPrefix(".") }
}

private func fill(_ draft: RecordDraft, row: Int, _ values: [Int32: String]) throws {
    for (field, value) in values { try draft.setRow(row, field, to: value) }
}

@Test func aNewOrderIsCreatedOnlyBySavingAndIsListedWithItsTotal() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let draft = try await session.newOrderDraft()
    #expect(draft.isNew)
    #expect(!draft.field(SPIELORDERS_ORDER_FIELD_DATE_ORDERED).isEmpty)
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Vacuum parts")
    try draft.insertRow(at: 0)
    try fill(draft, row: 0, [SPIELORDERS_ITEM_FIELD_DESCRIPTION: "Gasket", SPIELORDERS_ITEM_FIELD_QUANTITY: "3",
                             SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "2.5"])
    #expect(draft.total == "7.5")
    #expect(try orderDirectories(root).isEmpty)
    let saved = try await session.save(draft, cancellation: LibraryCancellation())
    #expect(!saved.requiresReopen)
    let listed = try await listedOrders(session)
    #expect(listed.map(\.id) == [saved.recordID])
    #expect(listed.first?.values[SPIELORDERS_ORDER_FIELD_TOTAL] == "7.5")
    #expect(saved.draft?.isNew == false)
    #expect(saved.draft?.contents == draft.contents)
}

@Test func aRefusedSaveLocatesEveryErrorWritesNothingAndKeepsTheDraft() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "")
    try draft.insertRow(at: 0)
    try fill(draft, row: 0, [SPIELORDERS_ITEM_FIELD_DESCRIPTION: "Lens", SPIELORDERS_ITEM_FIELD_QUANTITY: "two",
                             SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "4"])
    let before = draft.contents
    do {
        _ = try await session.save(draft, cancellation: LibraryCancellation())
        Issue.record("A blank Title and an unparseable Quantity must refuse")
    } catch let refusal as SaveRefusal {
        let located = refusal.problems.map { [$0.field, Int32($0.row)] }
        #expect(located.contains([SPIELORDERS_ORDER_FIELD_TITLE, -1]))
        #expect(located.contains([SPIELORDERS_ITEM_FIELD_QUANTITY, 0]))
        #expect(!refusal.stale)
    }
    #expect(draft.contents == before)
    #expect(try orderDirectories(root).isEmpty)
}

@Test func aStaleEditRefusesAndLeavesTheOtherWritersFacts() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Original")
    let mine = try await openedSession(root)
    let row = try #require(try await listedOrders(mine).first)
    let draft = try await mine.orderDraft(row, cancellation: LibraryCancellation())
    let theirs = try await openedSession(root)
    let other = try await theirs.orderDraft(try #require(try await listedOrders(theirs).first), cancellation: LibraryCancellation())
    try other.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Their title")
    _ = try await theirs.save(other, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "My title")
    do {
        _ = try await mine.save(draft, cancellation: LibraryCancellation())
        Issue.record("A draft of an older read must not overwrite a newer save")
    } catch let refusal as SaveRefusal {
        #expect(refusal.stale)
    }
    #expect(draft.field(SPIELORDERS_ORDER_FIELD_TITLE) == "My title")
    let reader = try await openedSession(root)
    #expect(try await listedOrders(reader).first?.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Their title")
}

@Test func removedAndReorderedItemsCommitTogether() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Optics")
    for (row, name) in ["A", "B", "C"].enumerated() {
        try draft.insertRow(at: row)
        try fill(draft, row: row, [SPIELORDERS_ITEM_FIELD_DESCRIPTION: name, SPIELORDERS_ITEM_FIELD_QUANTITY: "1",
                                   SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "1"])
    }
    let created = try await session.save(draft, cancellation: LibraryCancellation())
    let edit = try #require(created.draft)
    try edit.removeRow(at: 1)
    try edit.moveRow(from: 1, to: 0)
    _ = try await session.save(edit, cancellation: LibraryCancellation())
    let reader = try await openedSession(root)
    let row = try #require(try await listedOrders(reader).first)
    guard case let .healthy(details) = try await reader.inspectOrder(row, cancellation: LibraryCancellation()) else {
        Issue.record("The saved Order must be healthy")
        return
    }
    #expect(details.items.map(\.description) == ["C", "A"])
}

@Test func anEditIntroducingAReusableValueNeedsConfirmationAndAReusedOneDoesNot() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Existing")
    let session = try await openedSession(root)
    let created = try await session.newOrderDraft()
    try created.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Created")
    try created.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: "Greenhouse")
    _ = try await session.save(created, cancellation: LibraryCancellation())
    let row = try #require(try await listedOrders(session).first { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] == "Existing" })
    let edit = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try edit.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: "Optics")
    try edit.set(SPIELORDERS_ORDER_FIELD_ORDERER, to: "Ada")
    do {
        _ = try await session.save(edit, cancellation: LibraryCancellation())
        Issue.record("An edit introducing new values must ask for confirmation")
    } catch let refusal as SaveRefusal {
        #expect(Set(refusal.unconfirmedFields) == [SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDERER])
    }
    for field in [SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDERER] { try edit.confirm(field) }
    let saved = try #require(try await session.save(edit, cancellation: LibraryCancellation()).draft)
    #expect(try await session.values(SPIELORDERS_ORDER_FIELD_PROJECT) == ["Greenhouse", "Optics"])
    try saved.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: "greenhouse")
    _ = try await session.save(saved, cancellation: LibraryCancellation())
}

@Test func aVendorCreatedAndRenamedInADraftIsResolvedLiveByItsOrders() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let vendor = try await session.newVendorDraft()
    #expect(vendor.kind == .vendor)
    try vendor.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Thor")
    try vendor.insertRow(at: 0)
    try vendor.setRow(0, SPIELORDERS_CONTACT_FIELD_NAME, to: "Sales")
    let created = try await session.save(vendor, cancellation: LibraryCancellation())
    let order = try await session.newOrderDraft()
    try order.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Mounts")
    try order.set(SPIELORDERS_ORDER_FIELD_VENDOR, to: "Thor")
    _ = try await session.save(order, cancellation: LibraryCancellation())
    let rename = try await session.vendorDraft(id: created.recordID, cancellation: LibraryCancellation())
    #expect(rename.rowField(0, SPIELORDERS_CONTACT_FIELD_NAME) == "Sales")
    try rename.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Tarnwick")
    _ = try await session.save(rename, cancellation: LibraryCancellation())
    #expect(try await listedOrders(session).first?.values[SPIELORDERS_ORDER_FIELD_VENDOR] == "Tarnwick")
}

@Test func aHiddenVendorLeavesTheOfferedListAndStillResolvesForItsOrders() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let id = try createVendor(in: root, name: "Elderglass")
    _ = try createOrder(in: root, title: "Windows", vendor: "Elderglass")
    let session = try await openedSession(root)
    _ = try await session.setVendorHidden(id: id, hidden: true, cancellation: LibraryCancellation())
    let offered = try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: -1,
                                            descending: false, cancellation: LibraryCancellation())
    #expect(offered.rows.isEmpty)
    #expect(try await listedOrders(session).first?.values[SPIELORDERS_ORDER_FIELD_VENDOR] == "Elderglass")
    _ = try await session.setVendorHidden(id: id, hidden: false, cancellation: LibraryCancellation())
    let shown = try await session.vendors(query: "", caseSensitive: false, includeHidden: false, sortField: -1,
                                          descending: false, cancellation: LibraryCancellation())
    #expect(shown.rows.map(\.id) == [id])
}
