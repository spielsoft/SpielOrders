import CSpielOrders
import Foundation
import Testing
import SpielOrdersKit

/// Saves a new Order in `project` with `group`, confirming both as new values.
private func addOrder(_ session: PackageSession, _ title: String, project: String, group: String) async throws {
    let draft = try await session.newOrderDraft()
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: title)
    try draft.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: project)
    try draft.set(SPIELORDERS_ORDER_FIELD_ORDER_GROUP, to: group)
    for field in [SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP] where !draft.field(field).isEmpty {
        try draft.confirm(field)
    }
    _ = try await session.save(draft, cancellation: LibraryCancellation())
}

private func draft(_ session: PackageSession, titled title: String) async throws -> RecordDraft {
    let row = try #require(try await listedOrders(session).first { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] == title })
    return try await session.orderDraft(row, cancellation: LibraryCancellation())
}

@Test func groupsOfEachProject() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Coils", project: "Greenhouse", group: "Misting")
    try await addOrder(session, "Lenses", project: "Greenhouse", group: "Misting")
    try await addOrder(session, "Mirrors", project: "Optics", group: "Misting")
    try await addOrder(session, "Cables", project: "", group: "Spares")

    let greenhouse = try await session.groups(of: "Greenhouse")
    #expect(Dictionary(uniqueKeysWithValues: greenhouse.map { ($0.name, $0.orderCount) }) == ["Misting": 2])
    let optics = try await session.groups(of: "optics")
    #expect(Dictionary(uniqueKeysWithValues: optics.map { ($0.name, $0.orderCount) }) == ["Misting": 1])
    let unassigned = try await session.groups(of: "")
    #expect(Dictionary(uniqueKeysWithValues: unassigned.map { ($0.name, $0.orderCount) }) == ["Spares": 1])
    #expect(try await session.groups(of: "Cryogenics").isEmpty)
    let projects = try await session.projectGroups()
    #expect(projects.map { $0.project?.name } == ["Greenhouse", "Optics", nil])
    #expect(projects.map { $0.project?.orderCount } == [2, 1, nil])
    #expect(projects.map { Dictionary(uniqueKeysWithValues: $0.groups.map { ($0.name, $0.orderCount) }) }
        == [["Misting": 2], ["Misting": 1], ["Spares": 1]])
}

@Test func projectsWithoutUnassignedGroups() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Coils", project: "Greenhouse", group: "")
    let projects = try await session.projectGroups()
    #expect(projects.map { $0.project?.name } == ["Greenhouse"])
    #expect(projects.map { $0.project?.orderCount } == [1])
    #expect(projects.allSatisfy { $0.groups.isEmpty })
}

@Test func movingProjectClearsGroup() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try await openedSession(root)
    try await addOrder(session, "Coils", project: "Greenhouse", group: "Misting")
    try await addOrder(session, "Mirrors", project: "Optics", group: "Misting")
    try await addOrder(session, "Probes", project: "Cryogenics", group: "")

    let kept = try await draft(session, titled: "Coils")
    try kept.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: "Optics")
    let moved = try await session.save(kept, cancellation: LibraryCancellation())
    #expect(moved.cleared.isEmpty)
    #expect(moved.draft?.field(SPIELORDERS_ORDER_FIELD_ORDER_GROUP) == "Misting")

    let cleared = try await draft(session, titled: "Mirrors")
    try cleared.set(SPIELORDERS_ORDER_FIELD_PROJECT, to: "Cryogenics")
    let saved = try await session.save(cleared, cancellation: LibraryCancellation())
    #expect(saved.cleared == [SPIELORDERS_ORDER_FIELD_ORDER_GROUP: "Misting"])
    #expect(saved.draft?.field(SPIELORDERS_ORDER_FIELD_ORDER_GROUP) == "")
    let optics = try await session.groups(of: "Optics")
    #expect(Dictionary(uniqueKeysWithValues: optics.map { ($0.name, $0.orderCount) }) == ["Misting": 1])
    #expect(try await session.groups(of: "Cryogenics").isEmpty)
}
