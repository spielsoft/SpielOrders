import XCTest

// Decision sources: Prototype the SpielOrders desktop experience, sidebar tables; Desktop acceptance.
@MainActor final class ValueTableTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    private func launch(_ root: URL) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        return app
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    private func orders(of name: String, in app: XCUIApplication) -> XCUIElement {
        element("orderers.row.\(name).orders", in: app)
    }

    func testAddRenameMergeDelete() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Lenses", orderer: "Ada Lovelace")
        try addOrder(in: root, title: "Mounts", orderer: "A. Lovelace")
        try addOrder(in: root, title: "Probes", orderer: "Grace Hopper")
        let app = launch(root)
        showDestination("orderers", in: app)
        XCTAssertTrue(orders(of: "Ada Lovelace", in: app).waitForExistence(timeout: 10))

        // + names a new value in place; Return adds it, used by no Order.
        element("orderers.new", in: app).click()
        let naming = element("orderers.row.New Orderer.name.naming", in: app)
        XCTAssertTrue(naming.waitForExistence(timeout: 5))
        app.typeText("Emmy Noether\r")
        XCTAssertTrue(orders(of: "Emmy Noether", in: app).waitForExistence(timeout: 10))
        XCTAssertEqual(orders(of: "Emmy Noether", in: app).value as? String, "0")

        // Double-click renames in place; an unused value changes no Order and asks nothing.
        element("orderers.row.Emmy Noether.name", in: app).doubleClick()
        XCTAssertTrue(element("orderers.row.Emmy Noether.name.naming", in: app).waitForExistence(timeout: 5))
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Emmy\r")
        XCTAssertTrue(orders(of: "Emmy", in: app).waitForExistence(timeout: 10))

        // Merge N … keeps the chosen spelling, saying how many Orders change.
        element("orderers.row.Ada Lovelace.name", in: app).click()
        XCUIElement.perform(withKeyModifiers: .command) {
            element("orderers.row.A. Lovelace.name", in: app).click()
        }
        element("orderers.row.A. Lovelace.name", in: app).rightClick()
        app.menuItems["orderers.merge"].click()
        let count = element("value.merge.count", in: app)
        XCTAssertTrue(count.waitForExistence(timeout: 5))
        app.radioButtons["Ada Lovelace"].click()
        XCTAssertEqual(count.value as? String, "1 Order will change.")
        app.buttons["value.merge.confirm"].click()
        XCTAssertTrue(orders(of: "A. Lovelace", in: app).waitForNonExistence(timeout: 10))
        XCTAssertEqual(orders(of: "Ada Lovelace", in: app).value as? String, "2")

        // Delete… of a used value asks for a replacement; every affected Order follows.
        element("orderers.row.Grace Hopper.name", in: app).rightClick()
        app.menuItems["orderers.delete"].click()
        let replacement = element("value.replace.replacement", in: app)
        XCTAssertTrue(replacement.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["value.replace.confirm"].isEnabled)
        replacement.click()
        app.menuItems["Ada Lovelace"].click()
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"),
                                              object: app.buttons["value.replace.confirm"])
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed)
        XCTAssertEqual(element("value.replace.count", in: app).value as? String, "1 Order will change.")
        app.buttons["value.replace.confirm"].click()
        XCTAssertTrue(orders(of: "Grace Hopper", in: app).waitForNonExistence(timeout: 10))
        XCTAssertEqual(orders(of: "Ada Lovelace", in: app).value as? String, "3")

        // Deleting an unused value only confirms it.
        element("orderers.row.Emmy.name", in: app).rightClick()
        app.menuItems["orderers.delete"].click()
        app.sheets.buttons["value.delete.confirm"].click()
        XCTAssertTrue(orders(of: "Emmy", in: app).waitForNonExistence(timeout: 10))
    }

    func testAddingUndoes() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        showDestination("orderers", in: app)
        app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(element("orderers.row.New Orderer.name.naming", in: app).waitForExistence(timeout: 5))
        app.typeText("Ada Lovelace\r")
        XCTAssertTrue(orders(of: "Ada Lovelace", in: app).waitForExistence(timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(orders(of: "Ada Lovelace", in: app).waitForNonExistence(timeout: 10))
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(orders(of: "Ada Lovelace", in: app).waitForExistence(timeout: 10))
    }

    func testReplacementDoesNotExpandAfterApproval() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "First lens", orderer: "Ada")
        try addOrder(in: root, title: "Cable", orderer: "Grace")
        let app = launch(root)
        showDestination("orderers", in: app)
        let ada = element("orderers.row.Ada.name", in: app)
        XCTAssertTrue(ada.waitForExistence(timeout: 10))
        ada.rightClick()
        app.menuItems["orderers.delete"].click()
        let replacement = element("value.replace.replacement", in: app)
        XCTAssertTrue(replacement.waitForExistence(timeout: 5))
        replacement.click()
        app.menuItems["Grace"].click()
        let count = element("value.replace.count", in: app)
        let counted = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %@", "1 Order will change."), object: count)
        XCTAssertEqual(XCTWaiter.wait(for: [counted], timeout: 10), .completed)

        // Another writer adds a user after the sheet has planned the chosen replacement.
        try addOrder(in: root, title: "Second lens", orderer: "Ada")
        app.buttons["value.replace.confirm"].click()
        XCTAssertTrue(app.sheets["value.change.result"].waitForExistence(timeout: 10))
        let directories = try FileManager.default.contentsOfDirectory(
            at: root.appendingPathComponent("Orders"), includingPropertiesForKeys: nil)
        let orderers = try directories.filter { !$0.lastPathComponent.hasPrefix(".") }.map { directory in
            let data = try Data(contentsOf: directory.appendingPathComponent(".spielorders/order.json"))
            let record = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            return record["orderer"] as? String
        }
        XCTAssertEqual(orderers.compactMap { $0 }.filter { $0 == "Ada" }.count, 2)
        XCTAssertEqual(orderers.compactMap { $0 }.filter { $0 == "Grace" }.count, 1)
    }

    func testClearingUsedValueRequiresExplicitNone() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Lens", orderer: "Ada")
        let app = launch(root)
        showDestination("orderers", in: app)
        let ada = element("orderers.row.Ada.name", in: app)
        XCTAssertTrue(ada.waitForExistence(timeout: 10))
        ada.rightClick()
        app.menuItems["orderers.delete"].click()
        let replacement = element("value.replace.replacement", in: app)
        XCTAssertTrue(replacement.waitForExistence(timeout: 5))
        let delete = app.buttons["value.replace.confirm"]
        XCTAssertFalse(delete.isEnabled)
        XCTAssertFalse(element("value.replace.count", in: app).exists)
        replacement.click()
        app.menuItems["None"].click()
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: delete)
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed)
        XCTAssertEqual(element("value.replace.count", in: app).value as? String, "1 Order will change.")
        delete.click()
        XCTAssertTrue(ada.waitForNonExistence(timeout: 10))
        let directory = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: root.appendingPathComponent("Orders"), includingPropertiesForKeys: nil).first)
        let bytes = try Data(contentsOf: directory.appendingPathComponent(".spielorders/order.json"))
        let record = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
        XCTAssertNil(record["orderer"])
    }

    func testProjectGroupAddition() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Coils", project: "Greenhouse", group: "Misting")
        let app = launch(root)
        showDestination("projects", in: app)
        let greenhouse = element("projects.row.Greenhouse.name", in: app)
        XCTAssertTrue(greenhouse.waitForExistence(timeout: 10))
        greenhouse.click()
        element("projects.new", in: app).click()
        XCTAssertTrue(element("projects.row.Greenhouse/New Order Group.name.naming", in: app).waitForExistence(timeout: 5))
        app.typeText("Cooling\r")
        let cooling = element("projects.row.Greenhouse/Cooling.orders", in: app)
        XCTAssertTrue(cooling.waitForExistence(timeout: 10))
        XCTAssertEqual(cooling.value as? String, "0")
    }
}
