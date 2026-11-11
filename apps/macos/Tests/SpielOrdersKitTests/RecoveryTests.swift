import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

@Test func packageRepairRestoresBrowsingWithoutRepairingUnrelatedOrderDamage() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Known Vendor")
    _ = try createOrder(in: root, title: "Healthy")
    let damagedOrder = try createOrder(in: root, title: "Damaged")
    let brokenOrderBytes = Data("{broken".utf8)
    let orderMetadata = damagedOrder.appendingPathComponent(".spielorders/order.json")
    try brokenOrderBytes.write(to: orderMetadata)
    let vendorMetadata = root.appendingPathComponent(".spielorders/vendors.json")
    let valid = try String(contentsOf: vendorMetadata, encoding: .utf8)
    var damaged = try #require(try JSONSerialization.jsonObject(with: Data(valid.utf8)) as? [String: Any])
    damaged["unexpected"] = "explicit correction required"
    let damagedBytes = try JSONSerialization.data(withJSONObject: damaged)
    try damagedBytes.write(to: vendorMetadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    #expect(await session.diagnosticOnly)
    await #expect(throws: (any Error).self) {
        try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                 cancellation: LibraryCancellation())
    }
    let diagnosis = try await session.diagnose(directory: nil, cancellation: LibraryCancellation())
    #expect(!diagnosis.isValid)
    let repair = try #require(diagnosis.cases.first { $0.path == ".spielorders/vendors.json" })
    let action = try #require(repair.actions.first { $0.code == SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD })
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action,
                                            values: [SPIELORDERS_TEXT_FIELD_CONTENT: valid], cancellation: LibraryCancellation())
    #expect(try Data(contentsOf: vendorMetadata) == damagedBytes)
    _ = try await session.commit(plan)
    #expect(!(await session.diagnosticOnly))
    let rows = try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                        cancellation: LibraryCancellation()).rows
    #expect(rows.filter { $0.damage == nil }.map { $0.values[SPIELORDERS_ORDER_FIELD_TITLE] } == ["Healthy"])
    #expect(rows.filter { $0.damage != nil }.map { $0.directory.path } == [damagedOrder.path])
    #expect(try Data(contentsOf: orderMetadata) == brokenOrderBytes)
}

@Test func anOfferedLocatorRepairIsExplicitAndReturnsTheOrderToLookup() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    var json = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: metadata)) as? [String: Any])
    json["package_root"] = "../../wrong"
    let damagedBytes = try JSONSerialization.data(withJSONObject: json, options: [.sortedKeys])
    try damagedBytes.write(to: metadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: directory, cancellation: LibraryCancellation())
    #expect(!diagnosis.isValid)
    #expect(!diagnosis.findings.isEmpty)
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR } })
    let action = try #require(repair.actions.first)
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action, values: [:], cancellation: LibraryCancellation())
    #expect(try Data(contentsOf: metadata) == damagedBytes)
    _ = try await session.commit(plan)
    #expect(try await session.diagnose(directory: directory, cancellation: LibraryCancellation()).isValid)
    let orders = try await session.orders(query: "Optics", caseSensitive: false, sortField: -1,
                                          descending: false, cancellation: LibraryCancellation())
    #expect(orders.rows.count == 1)
    #expect(orders.rows.first?.damage == nil)
}

@Test func aStaleRepairRefusesAndKeepsExternalMetadata() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    let original = try Data(contentsOf: metadata)
    var json = try #require(try JSONSerialization.jsonObject(with: original) as? [String: Any])
    json["package_root"] = "../../wrong"
    try JSONSerialization.data(withJSONObject: json).write(to: metadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: directory, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR } })
    let action = try #require(repair.actions.first)
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action, values: [:], cancellation: LibraryCancellation())
    try original.write(to: metadata)
    await #expect(throws: (any Error).self) { try await session.commit(plan) }
    #expect(try Data(contentsOf: metadata) == original)
}

@Test(arguments: [SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR,
                  SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR,
                  SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE])
func anUnresolvedVendorUsesOnlyTheExplicitResolution(_ code: Int32) async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    _ = try createVendor(in: root, name: "Known Vendor")
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    var json = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: metadata)) as? [String: Any])
    json["vendor_id"] = "9f8e7d6c-5b4a-4392-8271-605f4e3d2c1b"
    try JSONSerialization.data(withJSONObject: json).write(to: metadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: directory, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == code } })
    let action = try #require(repair.actions.first { $0.code == code })
    var values = try await session.repairValues(diagnosis, repair: repair, action: action)
    if code == SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR { values[SPIELORDERS_ORDER_FIELD_VENDOR] = "Known Vendor" }
    if code == SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR { values[SPIELORDERS_VENDOR_FIELD_NAME] = "Chosen New Vendor" }
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action, values: values, cancellation: LibraryCancellation())
    _ = try await session.commit(plan)
    #expect(try await session.diagnose(directory: directory, cancellation: LibraryCancellation()).isValid)
    let row = try #require(try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                                    cancellation: LibraryCancellation()).rows.first)
    let expected = code == SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR ? "Known Vendor"
        : code == SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR ? "Chosen New Vendor" : ""
    #expect(row.values[SPIELORDERS_ORDER_FIELD_VENDOR] == expected)
}

@Test func completeReplacementRequiresValidTextAndPreservesTheOriginalUntilCommit() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    let valid = try String(contentsOf: metadata, encoding: .utf8)
    var json = try #require(try JSONSerialization.jsonObject(with: Data(valid.utf8)) as? [String: Any])
    json["unrecognized_fact"] = "must be explicitly removed"
    let damaged = try JSONSerialization.data(withJSONObject: json)
    try damaged.write(to: metadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: directory, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD } })
    let action = try #require(repair.actions.first)
    let initial = try await session.repairValues(diagnosis, repair: repair, action: action)
    #expect(initial[SPIELORDERS_TEXT_FIELD_CONTENT] == String(data: damaged, encoding: .utf8))
    await #expect(throws: (any Error).self) {
        try await session.planRepair(diagnosis, repair: repair, action: action,
                                     values: [SPIELORDERS_TEXT_FIELD_CONTENT: "{broken"], cancellation: LibraryCancellation())
    }
    #expect(try Data(contentsOf: metadata) == damaged)
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action,
                                            values: [SPIELORDERS_TEXT_FIELD_CONTENT: valid], cancellation: LibraryCancellation())
    #expect(try Data(contentsOf: metadata) == damaged)
    _ = try await session.commit(plan)
    #expect(try await session.diagnose(directory: directory, cancellation: LibraryCancellation()).isValid)
}

@Test func stagingRecoveryPromotesOnlyTheChosenCandidate() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    let stage = directory.appendingPathComponent(".spielorders/.order.json.spielorders-stage")
    var json = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: metadata)) as? [String: Any])
    json["title"] = "Explicitly Promoted"
    let candidate = try JSONSerialization.data(withJSONObject: json)
    try candidate.write(to: stage)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: directory, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE } })
    let action = try #require(repair.actions.first)
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action, values: [:], cancellation: LibraryCancellation())
    #expect(try Data(contentsOf: stage) == candidate)
    _ = try await session.commit(plan)
    #expect(!FileManager.default.fileExists(atPath: stage.path))
    let rows = try await session.orders(query: "Explicitly Promoted", caseSensitive: false, sortField: -1, descending: false,
                                        cancellation: LibraryCancellation()).rows
    #expect(rows.count == 1)
}

@Test func duplicateResolutionChangesOnlyTheChosenOrderIdentity() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let first = try createOrder(in: root, title: "Original")
    let second = try createOrder(in: root, title: "Separate Order")
    let firstFile = first.appendingPathComponent(".spielorders/order.json")
    let secondFile = second.appendingPathComponent(".spielorders/order.json")
    let firstBytes = try Data(contentsOf: firstFile)
    let original = try #require(try JSONSerialization.jsonObject(with: firstBytes) as? [String: Any])
    var duplicate = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: secondFile)) as? [String: Any])
    duplicate["order_id"] = original["order_id"]
    try JSONSerialization.data(withJSONObject: duplicate).write(to: secondFile)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let diagnosis = try await session.diagnose(directory: second, cancellation: LibraryCancellation())
    let repair = try #require(diagnosis.cases.first { $0.actions.contains { $0.code == SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY } })
    let action = try #require(repair.actions.first)
    let plan = try await session.planRepair(diagnosis, repair: repair, action: action, values: [:], cancellation: LibraryCancellation())
    _ = try await session.commit(plan)
    #expect(try Data(contentsOf: firstFile) == firstBytes)
    let rows = try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                                        cancellation: LibraryCancellation()).rows
    #expect(rows.count == 2)
    #expect(rows.allSatisfy { $0.damage == nil })
    #expect(Set(rows.map(\.id)).count == 2)
}
