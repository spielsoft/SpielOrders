import Foundation
import CSpielOrders
import SpielOrdersKit
import Testing

// Decision sources: Prototype the SpielOrders desktop experience, Order and Vendor editing;
// Define integrity, validation, and repair boundaries, Diagnostic scopes.

private func same(_ left: URL?, _ right: URL) -> Bool {
    left?.resolvingSymlinksInPath().path == right.resolvingSymlinksInPath().path
}

@Test func damagedOrderFindingsLocateTheirOrder() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createOrder(in: root, title: "Healthy")
    let damaged = try createOrder(in: root, title: "Damaged")
    try Data("{broken".utf8).write(to: damaged.appendingPathComponent(".spielorders/order.json"))
    let session = try await openedSession(root)

    let finding = try #require(await session.findings.first)
    #expect(same(finding.order, damaged))
    #expect(finding.statement == "This Order’s file can’t be read.")
    #expect(finding.details.contains("error: managed_artifact_invalid_syntax"))

    let package = try await session.diagnose(directory: nil, cancellation: LibraryCancellation())
    #expect(package.orders.count == 1)
    #expect(same(package.orders.first, damaged))
    let affected = try #require(package.orders.first)
    let order = try await session.diagnose(directory: affected, cancellation: LibraryCancellation())
    #expect(!order.isValid)
    #expect(order.orders.isEmpty)
}

@Test func warningsLocateTheirOrderOrNone() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    try Data("Quote".utf8).write(to: order.deletingLastPathComponent().appendingPathComponent("Quote.txt"))
    let readme = order.appendingPathComponent("README.md")
    try? FileManager.default.removeItem(at: readme)
    try FileManager.default.createDirectory(at: readme, withIntermediateDirectories: false)
    let session = try await openedSession(root)

    let findings = await session.findings
    let loose = try #require(findings.first { $0.order == nil })
    #expect(loose.details.hasPrefix("warning:"))
    #expect(loose.statement == "The Orders folder holds something that isn’t an Order.")
    let unreadable = try #require(findings.first { $0.order != nil })
    #expect(unreadable.details.hasPrefix("warning:"))
    #expect(same(unreadable.order, order))
    #expect(unreadable.statement == "This Order’s README can’t be read.")

    let diagnosis = try await session.diagnose(directory: nil, cancellation: LibraryCancellation())
    #expect(diagnosis.orders.count == 1)
    #expect(same(diagnosis.orders.first, order))
}

@Test(arguments: [true, false])
func diagnosisUsesCurrentOrderProblems(damagedAtOpen: Bool) async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    let record = order.appendingPathComponent(".spielorders/order.json")
    let original = try Data(contentsOf: record)
    let damaged = Data("{broken".utf8)
    if damagedAtOpen { try damaged.write(to: record) }
    let session = try await openedSession(root)
    try (damagedAtOpen ? original : damaged).write(to: record)

    let diagnosis = try await session.diagnose(directory: nil, cancellation: LibraryCancellation())

    if damagedAtOpen {
        #expect(diagnosis.orders.isEmpty)
    } else {
        #expect(diagnosis.orders.count == 1)
        #expect(same(diagnosis.orders.first, order))
    }
}

@Test func diagnosisKeepsRepairAndEditorEvidence() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    let session = try await openedSession(root)
    let row = try #require(try await listedOrders(session).first)
    let draft = try await session.orderDraft(row, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_ORDER_FIELD_TITLE, to: "Pending edit")
    _ = try await session.readme(of: nil, cancellation: LibraryCancellation())
    let readme = root.appendingPathComponent("README.md")
    let externalReadme = Data("An external README edit".utf8)
    try externalReadme.write(to: readme)
    let record = order.appendingPathComponent(".spielorders/order.json")
    var facts = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: record)) as? [String: Any])
    facts["title"] = "External title"
    let externalRecord = try JSONSerialization.data(withJSONObject: facts, options: .sortedKeys)
    try externalRecord.write(to: record)
    let vendors = root.appendingPathComponent(".spielorders/vendors.json")
    let original = try String(contentsOf: vendors, encoding: .utf8)
    var broken = try #require(JSONSerialization.jsonObject(with: Data(original.utf8)) as? [String: Any])
    broken["unexpected"] = "Explicit correction required"
    try JSONSerialization.data(withJSONObject: broken).write(to: vendors)

    let diagnosis = try await session.diagnose(directory: nil, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.path == ".spielorders/vendors.json" })
    let action = try #require(repair.actions.first { $0.code == SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD })
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action,
        values: [SPIELORDERS_TEXT_FIELD_CONTENT: original], cancellation: LibraryCancellation())
    _ = try await session.commit(plan)

    let refusal = await #expect(throws: SaveRefusal.self) {
        try await session.save(draft, cancellation: LibraryCancellation())
    }
    #expect(refusal?.recordChanged == true)
    let readmeRefusal = await #expect(throws: (any Error).self) {
        try await session.saveReadme("Pending README edit", of: nil, cancellation: LibraryCancellation())
    }
    #expect(readmeRefusal?.localizedDescription.contains("preconditions_changed") == true)
    #expect(try Data(contentsOf: readme) == externalReadme)
    #expect(try Data(contentsOf: record) == externalRecord)
}

@Test func savingVendorDoesNotReportAnotherOrdersProblem() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let vendor = try createVendor(in: root, name: "North Supplies")
    let damaged = try createOrder(in: root)
    try Data("{broken".utf8).write(to: damaged.appendingPathComponent(".spielorders/order.json"))
    let session = try await openedSession(root)
    let draft = try await session.vendorDraft(id: vendor, cancellation: LibraryCancellation())
    try draft.set(SPIELORDERS_VENDOR_FIELD_PHONE, to: "555-0100")

    let saved = try await session.save(draft, cancellation: LibraryCancellation())

    #expect(saved.warnings.isEmpty)
    #expect(saved.draft?.field(SPIELORDERS_VENDOR_FIELD_PHONE) == "555-0100")
    #expect(!(await session.findings).isEmpty)
}
