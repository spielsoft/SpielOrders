import XCTest

// Decision sources: Prototype the SpielOrders desktop experience (Merge N Orders…, merge
// sheets in precedence order); Define the Order record and lifecycle contract; Desktop acceptance.
@MainActor final class MergeTests: XCTestCase {
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

    private func replace(_ field: XCUIElement, with value: String, in app: XCUIApplication) {
        field.click()
        app.typeKey("a", modifierFlags: .command)
        field.typeText(value)
    }

    private func orderDirectories(_ root: URL) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("Orders"),
                                                    includingPropertiesForKeys: nil)
            .filter { !$0.lastPathComponent.hasPrefix(".") }
    }

    func testMergingOrdersKeepsTheFirstSelected() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Bench optics")
        try addOrder(in: root, title: "Diode optics", items: [("Laser diode", "1", "40.00")])
        let diode = try XCTUnwrap(orderDirectories(root).first { $0.lastPathComponent.contains("Diode") })
        try "Diode datasheet".write(to: diode.appendingPathComponent("Datasheet.txt"), atomically: true,
                                    encoding: .utf8)
        let app = launch(root)
        let table = app.tables["orders.table"]
        func row(_ title: String) -> XCUIElement {
            table.staticTexts.matching(NSPredicate(format: "value == %@", title)).firstMatch
        }
        XCTAssertTrue(row("Bench optics").waitForExistence(timeout: 10))

        // ⌘-click selects a second Order; the detail says how many are selected.
        row("Bench optics").click()
        XCTAssertTrue(app.textFields["order.editor.title"].waitForExistence(timeout: 10))
        XCUIElement.perform(withKeyModifiers: .command) { row("Diode optics").click() }
        XCTAssertTrue(app.staticTexts["2 Orders Selected"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.textFields["order.editor.title"].exists)

        // The sheet lists the Orders kept-first, the file that moves, and the files the kept Order has.
        row("Diode optics").rightClick()
        app.menuItems["orders.merge"].click()
        XCTAssertTrue(element("order.merge.moved.Datasheet.txt", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(element("order.merge.discarded.LifecycleQuote.txt", in: app).exists)
        XCTAssertTrue(element("order.merge.recoverable", in: app).exists)
        let merge = app.buttons["order.merge.confirm"]
        XCTAssertFalse(merge.isEnabled)

        // Reordering makes Diode the kept Order and replans; Datasheet.txt no longer moves.
        element("order.merge.position.2", in: app).click()
        app.buttons["order.merge.moveUp"].click()
        XCTAssertTrue(element("order.merge.moved.Datasheet.txt", in: app).waitForNonExistence(timeout: 10))
        app.buttons["order.merge.moveDown"].click()
        XCTAssertTrue(element("order.merge.moved.Datasheet.txt", in: app).waitForExistence(timeout: 10))

        // Merge waits for the discards to be confirmed.
        element("order.merge.confirmDiscard", in: app).click()
        XCTAssertTrue(merge.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        merge.click()

        // The kept Order is shown, holding the moved file; the other is removed recoverably.
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "Bench optics")
        XCTAssertTrue(row("Diode optics").waitForNonExistence(timeout: 10))
        let kept = try XCTUnwrap(orderDirectories(root).first)
        XCTAssertEqual(try orderDirectories(root).count, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: kept.appendingPathComponent("Datasheet.txt").path))
    }

    func testMergingVendorsKeepsTheFirstSelected() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        showDestination("vendors", in: app)
        let table = app.tables["vendors.table"]
        func row(_ name: String) -> XCUIElement {
            table.staticTexts.matching(NSPredicate(format: "value == %@", name)).firstMatch
        }
        for name in ["Acme Optics", "Acme Optics Inc"] {
            app.buttons["vendor.new"].click()
            let field = app.textFields["vendor.editor.name"]
            XCTAssertTrue(field.waitForExistence(timeout: 10))
            replace(field, with: name, in: app)
            app.buttons["editor.create"].click()
            XCTAssertTrue(row(name).waitForExistence(timeout: 10))
        }
        row("Acme Optics Inc").click()
        XCUIElement.perform(withKeyModifiers: .command) { row("Acme Optics").click() }
        row("Acme Optics").rightClick()
        app.menuItems["vendors.merge"].click()

        // The Vendor selected first is kept unless the owner reorders the list.
        let first = element("vendor.merge.position.1", in: app)
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        XCTAssertTrue(first.label.contains("Acme Optics Inc"))
        XCTAssertEqual(element("vendor.merge.count", in: app).value as? String, "No Orders will change.")
        app.buttons["vendor.merge.confirm"].click()
        XCTAssertTrue(row("Acme Optics").waitForNonExistence(timeout: 10))
        XCTAssertTrue(row("Acme Optics Inc").exists)
    }
}
