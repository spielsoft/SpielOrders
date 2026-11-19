import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

// Decision sources: Prototype the SpielOrders desktop experience, sidebar tables; Define the
// Order record and lifecycle contract; Define Vendor identity and history.

/// Saves a new Order titled `title` with `facts`, adding each value the Package lacks.
private func addOrder(_ session: PackageSession, _ title: String, _ facts: [Int32: String]) async throws {
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: title)
    for field in [SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_PROJECT,
                  SPIELORDERS_ORDER_FIELD_ORDER_GROUP] {
        guard let value = facts[field] else { continue }
        try draft.set(field, to: value)
        if field != SPIELORDERS_ORDER_FIELD_VENDOR { try draft.confirm(field) }
    }
    _ = try await session.save(draft, cancellation: LibraryCancellation())
}

/// Each Order's value of `field`, by Order Title.
private func orderValues(_ session: PackageSession, _ field: Int32) async throws -> [String: String] {
    let rows = try await listedOrders(session)
    return Dictionary(uniqueKeysWithValues: rows.map {
        ($0.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "", $0.values[field] ?? "")
    })
}

/// The values of `field` with the Orders using each.
private func usage(_ session: PackageSession, _ field: Int32) async throws -> [String: Int] {
    Dictionary(uniqueKeysWithValues: try await session.values(field).map { ($0.name, $0.orderCount) })
}

private func commit(_ session: PackageSession, _ change: ValueChange) async throws {
    let committed = try await session.commit(change)
    #expect(committed.changed)
    #expect(!committed.requiresReopen)
}

@Test func addedValuesAreOfferedUnused() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let orderer = try await session.planAdding("Ada", to: .orderers, cancellation: LibraryCancellation())
    #expect(orderer.orderCount == 0)
    try await commit(session, orderer)
    try await commit(session, try await session.planAdding("Greenhouse", to: .projects, cancellation: LibraryCancellation()))
    let group = try await session.planAdding("Cooling", to: .groups(of: "Greenhouse"), cancellation: LibraryCancellation())
    try await commit(session, group)

    #expect(try await usage(session, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Ada": 0])
    let again = try await session.planAdding("ada", to: .orderers, cancellation: LibraryCancellation())
    #expect(!(try await session.commit(again).changed))
    #expect(try await session.groups(of: "Greenhouse").map(\.name) == ["Cooling"])
    #expect(try await session.groups(of: "").isEmpty)
    let reopened = try await openedSession(root)
    #expect(try await usage(reopened, SPIELORDERS_ORDER_FIELD_PROJECT) == ["Greenhouse": 0])
}

@Test func valueSpellingsFollowPackageMatching() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await commit(session, try await session.planAdding("Café", to: .orderers,
                                                          cancellation: LibraryCancellation()))
    #expect(valueMatches("Café", " CAFÉ "))
    #expect(valueMatches("Straße", "STRASSE"))
    let decomposed = "Cafe\u{301}"
    #expect(!valueMatches("Café", decomposed))
    let distinct = try await session.planAdding(decomposed, to: .orderers, cancellation: LibraryCancellation())
    #expect(try await session.commit(distinct).changed)
    let values = try await session.values(SPIELORDERS_ORDER_FIELD_ORDERER)
    #expect(values.count == 2)
    #expect(values.contains { Array($0.name.utf8) == Array(decomposed.utf8) })
    #expect(values.contains { Array($0.name.utf8) == Array("Café".utf8) })
}

@Test func unchangedAdditionRefreshesChoices() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let elsewhere = try await openedSession(root)
    try await commit(elsewhere, try await elsewhere.planAdding("Ada", to: .orderers,
                                                              cancellation: LibraryCancellation()))
    let reused = try await session.planAdding("Ada", to: .orderers, cancellation: LibraryCancellation())
    #expect(!(try await session.commit(reused).changed))
    #expect(try await usage(session, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Ada": 0])
}

@Test func addingRefusesBlankAndUnknownProject() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    await #expect(throws: ValueRefusal.self) {
        try await session.planAdding("  ", to: .orderers, cancellation: LibraryCancellation())
    }
    await #expect(throws: ValueRefusal.self) {
        try await session.planAdding("Cooling", to: .groups(of: "Nowhere"), cancellation: LibraryCancellation())
    }
}

@Test func renamingFollowsEveryOrder() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Lenses", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada"])
    try await addOrder(session, "Mirrors", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada"])
    try await addOrder(session, "Cables", [SPIELORDERS_ORDER_FIELD_ORDERER: "Grace"])

    #expect(!(await session.renamingMerges("Ada", into: "Ada King", in: .orderers)))
    let rename = try await session.planRenaming("Ada", to: "Ada King", in: .orderers, cancellation: LibraryCancellation())
    #expect(rename.orderCount == 2)
    try await commit(session, rename)

    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_ORDERER)
        == ["Lenses": "Ada King", "Mirrors": "Ada King", "Cables": "Grace"])
    #expect(try await usage(session, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Ada King": 2, "Grace": 1])
}

@Test func renamingOntoAnotherValueMerges() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Lenses", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada"])
    try await addOrder(session, "Cables", [SPIELORDERS_ORDER_FIELD_ORDERER: "Grace"])

    #expect(await session.renamingMerges("Ada", into: "Grace", in: .orderers))
    #expect(await session.renamingMerges("Ada", into: "grace", in: .orderers))
    #expect(!(await session.renamingMerges("Ada", into: "ADA", in: .orderers)))
}

@Test func mergingKeepsTheChosenSpelling() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Cell", [SPIELORDERS_ORDER_FIELD_PROJECT: "AbC"])
    try await addOrder(session, "Oven", [SPIELORDERS_ORDER_FIELD_PROJECT: "Ab_C"])
    try await addOrder(session, "Getter", [SPIELORDERS_ORDER_FIELD_PROJECT: "Ab_C"])

    let keepFirst = try await session.planMerging(["AbC", "Ab_C"], keeping: "AbC", in: .projects,
                                                  cancellation: LibraryCancellation())
    let keepSecond = try await session.planMerging(["AbC", "Ab_C"], keeping: "Ab_C", in: .projects,
                                                   cancellation: LibraryCancellation())
    #expect(keepFirst.orderCount == 2)
    #expect(keepSecond.orderCount == 1)
    try await commit(session, keepFirst)

    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_PROJECT)
        == ["Cell": "AbC", "Oven": "AbC", "Getter": "AbC"])
    #expect(try await usage(session, SPIELORDERS_ORDER_FIELD_PROJECT) == ["AbC": 3])
}

@Test func deletingUsedValuesNeedsAReplacement() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Lenses", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada"])
    try await addOrder(session, "Cables", [SPIELORDERS_ORDER_FIELD_ORDERER: "Grace"])
    try await commit(session, try await session.planAdding("Emmy", to: .orderers, cancellation: LibraryCancellation()))

    let unused = try await session.planDeleting("Emmy", in: .orderers, replacement: .refuse,
                                                cancellation: LibraryCancellation())
    #expect(unused.orderCount == 0)
    try await commit(session, unused)
    do {
        _ = try await session.planDeleting("Ada", in: .orderers, replacement: .refuse, cancellation: LibraryCancellation())
        Issue.record("Deleting a used value without a replacement must refuse")
    } catch let refusal as ValueRefusal {
        #expect(refusal.inUse)
    }
    let replaced = try await session.planDeleting("Ada", in: .orderers, replacement: .with("Grace"),
                                                  cancellation: LibraryCancellation())
    #expect(replaced.orderCount == 1)
    try await commit(session, replaced)
    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Lenses": "Grace", "Cables": "Grace"])

    let cleared = try await session.planDeleting("Grace", in: .orderers, replacement: .clear,
                                                 cancellation: LibraryCancellation())
    #expect(cleared.orderCount == 2)
    try await commit(session, cleared)
    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Lenses": "", "Cables": ""])
    #expect(try await usage(session, SPIELORDERS_ORDER_FIELD_ORDERER).isEmpty)
}

@Test func changedPackageRefusesAnOlderPlan() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Lenses", [SPIELORDERS_ORDER_FIELD_ORDERER: "Ada"])
    let stale = try await session.planRenaming("Ada", to: "Ada King", in: .orderers, cancellation: LibraryCancellation())
    let elsewhere = try await openedSession(root)
    try await commit(elsewhere, try await elsewhere.planAdding("Grace", to: .orderers, cancellation: LibraryCancellation()))

    do {
        _ = try await session.commit(stale)
        Issue.record("A plan made before another change must refuse")
    } catch let refusal as ValueRefusal {
        #expect(refusal.summary == "The Package changed. Reload the Package, then try again.")
    }
    #expect(try await orderValues(elsewhere, SPIELORDERS_ORDER_FIELD_ORDERER) == ["Lenses": "Ada"])
}

@Test func mergingVendorsKeepsOne() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    // The Vendor named first in the merge is kept, whichever was created first.
    let other = try createVendor(in: root, name: "Optics Supplier Inc", contactEmail: "sales@example.invalid")
    let kept = try createVendor(in: root, name: "Optics supplier")
    _ = try createOrder(in: root, title: "Lenses", vendor: "Optics supplier")
    _ = try createOrder(in: root, title: "Mirrors", vendor: "Optics Supplier Inc")
    let session = try await openedSession(root)

    #expect(try await session.vendorID(named: "optics supplier inc") == other)
    let merge = try await session.planMergingVendors([kept, other], cancellation: LibraryCancellation())
    #expect(merge.orderCount == 1)
    try await commit(session, merge)

    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_VENDOR)
        == ["Lenses": "Optics supplier", "Mirrors": "Optics supplier"])
    let vendors = try await session.vendors(query: "", caseSensitive: false, includeHidden: true, sortField: -1,
                                            descending: false, cancellation: LibraryCancellation()).rows
    #expect(vendors.map(\.id) == [kept])
    let draft = try await session.vendorDraft(id: kept, cancellation: LibraryCancellation())
    #expect(draft.rowField(0, SPIELORDERS_CONTACT_FIELD_EMAIL) == "sales@example.invalid")
}

@Test func deletingVendorsReplacesOrClears() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let first = try createVendor(in: root, name: "First supplier")
    let second = try createVendor(in: root, name: "Second supplier")
    _ = try createOrder(in: root, title: "Lenses", vendor: "First supplier")
    let session = try await openedSession(root)

    do {
        _ = try await session.planDeletingVendor(first, replacement: .refuse, cancellation: LibraryCancellation())
        Issue.record("Deleting a referenced Vendor without a replacement must refuse")
    } catch let refusal as ValueRefusal {
        #expect(refusal.inUse)
    }
    let replaced = try await session.planDeletingVendor(first, replacement: .with(second),
                                                        cancellation: LibraryCancellation())
    #expect(replaced.orderCount == 1)
    try await commit(session, replaced)
    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_VENDOR) == ["Lenses": "Second supplier"])

    let cleared = try await session.planDeletingVendor(second, replacement: .clear, cancellation: LibraryCancellation())
    try await commit(session, cleared)
    #expect(try await orderValues(session, SPIELORDERS_ORDER_FIELD_VENDOR) == ["Lenses": ""])
    let vendors = try await session.vendors(query: "", caseSensitive: false, includeHidden: true, sortField: -1,
                                            descending: false, cancellation: LibraryCancellation()).rows
    #expect(vendors.isEmpty)
}

@Test func distinctLibrarySpellingsKeepDistinctChoices() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    let names = ["Café", "Cafe\u{301}"]
    for name in names {
        let added = try await session.planAdding(name, to: .projects, cancellation: LibraryCancellation())
        try await commit(session, added)
        let group = try await session.planAdding("Summer", to: .groups(of: name), cancellation: LibraryCancellation())
        try await commit(session, group)
    }
    let projects = try await session.values(SPIELORDERS_ORDER_FIELD_PROJECT)
    #expect(projects.count == 2)
    #expect(Set(projects.map { ValueScope.groups(of: $0.name) }).count == 2)
    #expect(Set(projects.map { Replacement.with($0.name) }).count == 2)
    #expect(!valueMatches(names[0], names[1]))
    let removed = try await session.planDeleting("Summer", in: .groups(of: names[0]), replacement: .refuse,
                                                cancellation: LibraryCancellation())
    try await commit(session, removed)
    #expect(try await session.groups(of: names[0]).isEmpty)
    #expect(try await session.groups(of: names[1]).map(\.name) == ["Summer"])
}
