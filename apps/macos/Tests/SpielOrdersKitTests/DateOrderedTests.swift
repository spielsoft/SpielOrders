import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

// Decision sources: Prototype the SpielOrders desktop experience, Order and Vendor editing;
// Effect and adapter boundaries.

@Test func calendarDates() throws {
    let leapDay = try #require(DateOrdered.date(from: "2024-02-29"))
    #expect(DateOrdered.text(for: leapDay) == "2024-02-29")
    #expect(Calendar.current.dateComponents([.year, .month, .day], from: leapDay)
        == DateComponents(year: 2024, month: 2, day: 29))
    for text in ["", "2026-02-30", "22/08/2026", "2026-8-22"] {
        #expect(DateOrdered.date(from: text) == nil, "\(text) is not a calendar date as stored")
    }
}

@Test func chosenDateSaves() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root)
    let session = try await openedSession(root)
    let row = try #require(try await listedOrders(session).first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    let chosen = try #require(Calendar.current.date(from: DateComponents(year: 2025, month: 12, day: 31, hour: 18)))
    try draft.set(SPIELORDERS_ORDER_FIELD_DATE_ORDERED, to: DateOrdered.text(for: chosen))
    let saved = try await session.save(draft, cancellation: LibraryCancellation())
    let listed = try #require(try await listedOrders(session).first { $0.id == saved.recordID })
    #expect(listed.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] == "2025-12-31")
}

@Test(arguments: ["", "2026-02-30"])
func unusableDateRefuses(_ text: String) async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root)
    let session = try await openedSession(root)
    let row = try #require(try await listedOrders(session).first)
    let stored = row.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED]
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_DATE_ORDERED, to: text)
    #expect(draft.field(SPIELORDERS_ORDER_FIELD_DATE_ORDERED) == text)
    do {
        _ = try await session.save(draft, cancellation: LibraryCancellation())
        Issue.record("Date Ordered “\(text)” must refuse")
    } catch let refusal as SaveRefusal {
        #expect(refusal.problems.contains { $0.field == SPIELORDERS_ORDER_FIELD_DATE_ORDERED })
    }
    #expect(try await listedOrders(session).first?.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] == stored)
}
