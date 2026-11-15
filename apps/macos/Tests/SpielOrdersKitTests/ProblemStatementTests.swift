import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

// Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.

/// The field labels the editor shows beside the fields these tests mark.
private let labels: [Int32: String] = [
    SPIELORDERS_ORDER_FIELD_TITLE: "Order Title", SPIELORDERS_ORDER_FIELD_DATE_ORDERED: "Date Ordered",
    SPIELORDERS_ORDER_FIELD_STATUS: "Status", SPIELORDERS_ORDER_FIELD_CURRENCY: "Currency",
    SPIELORDERS_ORDER_FIELD_ORDERER: "Orderer", SPIELORDERS_ITEM_FIELD_QUANTITY: "Quantity",
    SPIELORDERS_ITEM_FIELD_UNIT_PRICE: "Unit Price",
]

/// What the refused save of `draft` states, by field and row.
private func statements(saving draft: RecordDraft, in session: PackageSession) async throws -> [[String]] {
    do {
        _ = try await session.save(draft, cancellation: LibraryCancellation())
        Issue.record("The draft must be refused")
        return []
    } catch let refusal as SaveRefusal {
        return refusal.problems.map { problem in
            [labels[problem.field] ?? "", "\(problem.row)", problem.statement(for: labels[problem.field] ?? "")]
        }
    }
}

@Test func blankAndUnparsedValues() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "")
    try draft.insertRow(at: 0)
    try draft.setRow(0, SPIELORDERS_ITEM_FIELD_DESCRIPTION, to: "Lens")
    try draft.setRow(0, SPIELORDERS_ITEM_FIELD_QUANTITY, to: "two")
    let stated = try await statements(saving: draft, in: session)
    #expect(stated.contains(["Order Title", "-1", "Order Title is required."]))
    #expect(stated.contains(["Quantity", "0", "Quantity must be a number."]))
    #expect(stated.contains(["Unit Price", "0", "Unit Price is required."]))
}

@Test func valuesAnOrderCannotHold() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root)
    let session = try await openedSession(root)
    let draft = try await session.orderDraft(try #require(try await listedOrders(session).first),
                                             cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_DATE_ORDERED, to: "2026-02-30")
    try draft.set(SPIELORDERS_ORDER_FIELD_CURRENCY, to: "dollars")
    try draft.set(SPIELORDERS_ORDER_FIELD_STATUS, to: "lost")
    let stated = try await statements(saving: draft, in: session)
    #expect(stated.contains(["Date Ordered", "-1", "Date Ordered must be a real date."]))
    #expect(stated.contains(["Currency", "-1", "Currency must be a three-letter code, such as USD."]))
    #expect(stated.contains(["Status", "-1", "Status must be one of the Order statuses."]))
}

@Test func valuesThePackageLacks() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Elderglass")
    let session = try await openedSession(root)
    let order = try await session.newOrderDraft()
    try order.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Windows")
    try order.set(SPIELORDERS_ORDER_FIELD_ORDERER, to: "Ada")
    #expect(try await statements(saving: order, in: session) == [["Orderer", "-1", "This Orderer isn’t in the Package yet."]])
    try order.set(SPIELORDERS_ORDER_FIELD_ORDERER, to: "")
    try order.set(SPIELORDERS_ORDER_FIELD_VENDOR, to: "Nobody")
    #expect(try await statements(saving: order, in: session).map(\.last) == ["No Vendor has this name."])
    let vendor = try await session.newVendorDraft()
    try vendor.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Elderglass")
    #expect(try await statements(saving: vendor, in: session).map(\.last) == ["Another Vendor already has this name."])
}

@Test func problemsTheirFixesResolve() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Elderglass")
    let session = try await openedSession(root)
    let order = try await session.newOrderDraft()
    try order.set(SPIELORDERS_ORDER_FIELD_ORDERER, to: "Ada")
    #expect(try await refusal(saving: order, in: session).map(\.lacksValue) == [true])
    try order.set(SPIELORDERS_ORDER_FIELD_ORDERER, to: "")
    try order.set(SPIELORDERS_ORDER_FIELD_VENDOR, to: "Nobody")
    #expect(try await refusal(saving: order, in: session).map(\.lacksValue) == [true])
    let vendor = try await session.newVendorDraft()
    try vendor.set(SPIELORDERS_VENDOR_FIELD_NAME, to: "Elderglass")
    let taken = try await refusal(saving: vendor, in: session)
    #expect(taken.map(\.lacksValue) == [false])
    #expect(taken.map(\.nameTaken) == [true])
}

private func refusal(saving draft: RecordDraft, in session: PackageSession) async throws -> [DraftProblem] {
    do {
        _ = try await session.save(draft, cancellation: LibraryCancellation())
        Issue.record("The draft must be refused")
        return []
    } catch let refusal as SaveRefusal {
        return refusal.problems
    }
}

@Test func unfamiliarProblemKeepsLibraryWording() {
    let refusal = SaveRefusal(result: SPIELORDERS_RESULT_REFUSED, message: "The volume is read-only.")
    #expect(refusal.problems.map { $0.statement(for: "Order Title") } == ["The volume is read-only."])
}
