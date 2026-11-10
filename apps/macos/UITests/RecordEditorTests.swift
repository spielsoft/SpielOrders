import XCTest

// Decision sources: Prototype the SpielOrders desktop experience, Order and Vendor editing;
// Desktop acceptance.
@MainActor final class RecordEditorTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    private func launch(_ root: URL) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        return app
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    /// Replaces a field's whole text, such as the name a new record starts with.
    private func replace(_ field: XCUIElement, with text: String, in app: XCUIApplication) {
        field.click()
        app.typeKey("a", modifierFlags: .command)
        field.typeText(text)
    }

    private func ordersDirectory(_ root: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path).filter { !$0.hasPrefix(".") }
    }

    func testANewOrderIsWrittenOnlyBySaveAndARefusedSaveKeepsTheDraft() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.buttons["order.new"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "New Order")
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeKey(.delete, modifierFlags: [])
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tGasket\ttwo\t2.5\t")
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(element("order.editor.problems", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(element("order.editor.title.problem", in: app).exists)
        XCTAssertEqual(try ordersDirectory(root), [])
        XCTAssertEqual(element("order.editor.items.row.0.description", in: app).value as? String, "Gasket")
        replace(title, with: "Vacuum parts", in: app)
        let quantity = element("order.editor.items.row.0.quantity", in: app)
        quantity.doubleClick()
        app.typeKey("a", modifierFlags: .command)
        app.typeText("3\t")
        app.typeKey("s", modifierFlags: .command)
        let heading = element("order.editor.heading", in: app)
        XCTAssertTrue(heading.wait(for: \.label, toEqual: "Vacuum parts", timeout: 10))
        XCTAssertEqual(try ordersDirectory(root).count, 1)
        XCTAssertEqual(element("order.editor.total", in: app).value as? String, "7.5")
    }

    func testRemovedItemsComeBackWithUndoAndADirtyDraftSurvivesACancelledQuit() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.buttons["order.new"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Optics", in: app)
        for name in ["First", "Second"] {
            element("order.editor.items.add", in: app).click()
            app.typeText("\t\(name)\t1\t1\t")
        }
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeKey(.delete, modifierFlags: [])
        element("order.editor.items.row.0.description", in: app).click()
        element("order.editor.items.remove", in: app).click()
        element("order.editor.items.row.0.description", in: app).click()
        element("order.editor.items.remove", in: app).click()
        XCTAssertFalse(element("order.editor.items.row.0.description", in: app).exists)
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(element("order.editor.title.problem", in: app).waitForExistence(timeout: 10))
        app.tables["order.editor.items"].click()
        app.typeKey("z", modifierFlags: .command)
        app.typeKey("z", modifierFlags: .command)
        XCTAssertEqual(element("order.editor.items.row.0.description", in: app).value as? String, "First")
        XCTAssertEqual(element("order.editor.items.row.1.description", in: app).value as? String, "Second")
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.buttons["draft.unsaved.cancel"].waitForExistence(timeout: 5))
        app.buttons["draft.unsaved.cancel"].click()
        XCTAssertTrue(title.exists)
        XCTAssertEqual(try ordersDirectory(root), [])
        app.typeKey("q", modifierFlags: .command)
        app.buttons["draft.unsaved.discard"].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    func testLeavingADirtyDraftCanSaveItAndThenLeave() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.buttons["order.new"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Cryostat", in: app)
        element("order.editor.back", in: app).click()
        let save = app.buttons["draft.unsaved.save"]
        XCTAssertTrue(save.waitForExistence(timeout: 5))
        save.click()
        let rowTitle = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Cryostat")).firstMatch
        XCTAssertTrue(rowTitle.waitForExistence(timeout: 10))
        XCTAssertFalse(title.exists)
        XCTAssertEqual(try ordersDirectory(root).count, 1)
    }

    func testANewOrderEditorDoesNotKeepThePreviousOrdersItems() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.buttons["order.new"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Optics", in: app)
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tLens\t1\t2.5\t")
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.buttons["toolbar.save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        element("order.editor.back", in: app).click()
        let rowTitle = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Optics")).firstMatch
        XCTAssertTrue(rowTitle.waitForExistence(timeout: 10))
        rowTitle.doubleClick()
        XCTAssertTrue(element("order.editor.items.row.0.description", in: app).waitForExistence(timeout: 10))
        app.buttons["order.new"].click()
        let startingTitle = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "New Order"), object: title)
        wait(for: [startingTitle], timeout: 10)
        XCTAssertFalse(element("order.editor.items.row.0.description", in: app).exists)
    }

    func testTheVendorFieldHoldsOnlyAListedVendorOrNone() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.buttons["order.new"].click()
        let vendor = app.comboBoxes["order.editor.vendor"]
        XCTAssertTrue(vendor.waitForExistence(timeout: 10))
        vendor.click()
        vendor.buttons["order.editor.vendor"].click()
        app.typeKey(.downArrow, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Optics Supplier", in: app)
        app.typeKey("s", modifierFlags: .command)
        let selected = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)
        wait(for: [selected], timeout: 10)
        replace(vendor, with: "12345", in: app)
        app.typeKey(.tab, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)],
             timeout: 10)
        replace(vendor, with: "", in: app)
        app.typeKey(.delete, modifierFlags: [])
        app.typeKey(.tab, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", ""), object: vendor)], timeout: 10)
        XCTAssertEqual(try ordersDirectory(root), [])
        replace(vendor, with: "supp", in: app)
        app.typeKey(.tab, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)],
             timeout: 10)
    }

    func testContactsCanBeDraggedAndKeepTheirOrderAfterSaving() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.radioButtons["Vendors"].click()
        app.buttons["vendor.new"].click()
        let name = element("vendor.editor.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Optics Supplier", in: app)
        for contact in ["Sales", "Support"] {
            element("vendor.editor.contacts.add", in: app).click()
            app.typeText("\(contact)\tQuotes\t555-0100\t\(contact.lowercased())@example.invalid\t")
        }
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.buttons["toolbar.save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        let first = element("vendor.editor.contacts.row.0.name", in: app)
        element("vendor.editor.contacts.row.1.name", in: app).click(forDuration: 0.5, thenDragTo: first)
        let moved = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Support"), object: first)
        wait(for: [moved], timeout: 10)
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.buttons["toolbar.save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        element("vendor.editor.back", in: app).click()
        app.outlines["vendors.table"].outlineRows.matching(NSPredicate(format: "value CONTAINS %@", "Optics Supplier")).firstMatch.doubleClick()
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        XCTAssertEqual(first.value as? String, "Support")
        XCTAssertEqual(element("vendor.editor.contacts.row.1.name", in: app).value as? String, "Sales")
    }
}
