import AppKit
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
        app.toolbars.buttons["New Order"].click()
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
        let heading = element("order.editor.heading", in: app)
        XCTAssertEqual(heading.value as? String, "New Order")
        app.typeKey("s", modifierFlags: .command)
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            let names = try? FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path)
            return names?.filter { !$0.hasPrefix(".") }.count == 1
        }, object: nil)
        wait(for: [saved], timeout: 10)
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Vacuum parts"),
                                             object: heading)], timeout: 10)
        XCTAssertEqual(title.value as? String, "Vacuum parts")
        XCTAssertEqual(try ordersDirectory(root).count, 1)
        XCTAssertTrue(element("order.editor.total", in: app).staticTexts.matching(NSPredicate(format: "value == %@", "7.50")).firstMatch.exists)
    }

    func testRemovedItemsComeBackWithUndoAndADirtyDraftSurvivesACancelledQuit() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
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
        app.toolbars.buttons["New Order"].click()
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
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Optics", in: app)
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tLens\t1\t2.5\t")
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.toolbars.buttons["Save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        element("order.editor.back", in: app).click()
        let rowTitle = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Optics")).firstMatch
        XCTAssertTrue(rowTitle.waitForExistence(timeout: 10))
        rowTitle.doubleClick()
        XCTAssertTrue(element("order.editor.items.row.0.description", in: app).waitForExistence(timeout: 10))
        app.toolbars.buttons["New Order"].click()
        let startingTitle = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "New Order"), object: title)
        wait(for: [startingTitle], timeout: 10)
        XCTAssertFalse(element("order.editor.items.row.0.description", in: app).exists)
    }

    func testTheVendorFieldHoldsOnlyAListedVendorOrNone() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let vendor = app.buttons["order.editor.vendor"]
        XCTAssertTrue(vendor.waitForExistence(timeout: 10))
        vendor.click()
        app.buttons["vendor.chooser.new"].click()
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Optics Supplier", in: app)
        app.typeKey("s", modifierFlags: .command)
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)
        vendor.click()
        let search = app.searchFields["vendor.chooser.search"]
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.click()
        search.typeText("12345")
        XCTAssertEqual(search.value as? String, "12345")
        search.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(search.exists)
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(search.waitForNonExistence(timeout: 5))
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)
        vendor.click()
        app.buttons["vendor.chooser.clear"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "None"), object: vendor)], timeout: 10)
        XCTAssertEqual(try ordersDirectory(root), [])
        vendor.click()
        search.click()
        search.typeText("supp")
        search.typeKey(.return, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)

        vendor.click()
        app.buttons["vendor.chooser.new"].click()
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Vacuum Supplier", in: app)
        app.typeKey("s", modifierFlags: .command)
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Vacuum Supplier"), object: vendor)], timeout: 10)
        vendor.click()
        search.click()
        search.typeText("supp")
        search.typeKey(.downArrow, modifierFlags: [])
        search.typeKey(.upArrow, modifierFlags: [])
        search.typeKey(.return, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)
        vendor.click()
        search.click()
        search.typeKey(.downArrow, modifierFlags: [])
        search.typeKey(.return, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Vacuum Supplier"), object: vendor)], timeout: 10)
    }

    func testReturnEditsTheSelectedContactWithoutSavingTheVendor() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let vendorsFile = root.appendingPathComponent(".spielorders/vendors.json")
        let originalVendors = try Data(contentsOf: vendorsFile)
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        app.buttons["order.editor.vendor"].click()
        app.buttons["vendor.chooser.new"].click()
        let add = app.buttons["vendor.sheet.contacts.add"]
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        add.click()
        app.typeText("Avery")
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
        app.typeText("Blair")
        app.typeKey(.tab, modifierFlags: [])
        XCTAssertTrue(app.buttons["vendor.sheet.save"].exists)
        XCTAssertEqual(element("vendor.sheet.contacts.row.0.name", in: app).value as? String, "Blair")
        XCTAssertEqual(try Data(contentsOf: vendorsFile), originalVendors)
    }

    func testCancellingAVendorDiscardKeepsBothDrafts() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Order draft", in: app)
        app.buttons["order.editor.vendor"].click()
        app.buttons["vendor.chooser.new"].click()
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Vendor draft", in: app)
        app.buttons["vendor.sheet.cancel"].click()
        let cancel = app.sheets.buttons["vendor.sheet.discard.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(name.value as? String, "Vendor draft")
        app.buttons["vendor.sheet.cancel"].click()
        app.buttons["vendor.sheet.discard.confirm"].click()
        XCTAssertTrue(name.waitForNonExistence(timeout: 5))
        XCTAssertEqual(title.value as? String, "Order draft")
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testARefusedSaveDuringNavigationKeepsTheEditorWithoutAnotherQuestion() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeKey(.delete, modifierFlags: [])
        app.buttons["order.editor.back"].click()
        let save = app.sheets.buttons["draft.unsaved.save"]
        XCTAssertTrue(save.waitForExistence(timeout: 5))
        save.click()
        XCTAssertTrue(element("order.editor.title.problem", in: app).waitForExistence(timeout: 10))
        XCTAssertFalse(save.exists)
        XCTAssertTrue(title.isEnabled)
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testQuittingResolvesTheVendorAndOrderDraftsSeparately() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Keep this Order", in: app)
        app.buttons["order.editor.vendor"].click()
        app.buttons["vendor.chooser.new"].click()
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Discard this Vendor", in: app)
        app.typeKey("q", modifierFlags: .command)
        let discard = app.buttons["draft.unsaved.discard"]
        XCTAssertTrue(discard.waitForExistence(timeout: 5))
        discard.click()
        XCTAssertTrue(name.waitForNonExistence(timeout: 5))
        let cancel = app.buttons["draft.unsaved.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(title.value as? String, "Keep this Order")
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testQuittingWhileHiddenStillAsksAboutAnUnsavedOrder() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Hidden draft", in: app)
        app.activate()
        guard app.wait(for: .runningForeground, timeout: 5),
              let running = NSWorkspace.shared.frontmostApplication, running.localizedName == app.label else {
            throw XCTSkip("The tested app is not foreground; quitting while hidden is unverified.")
        }
        app.typeKey("h", modifierFlags: .command)
        let hidden = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hidden == YES"), object: running)
        wait(for: [hidden], timeout: 5)
        // The Dock's Quit reaches a hidden app as this request does.
        XCTAssertTrue(running.terminate())
        let cancel = app.sheets.buttons["draft.unsaved.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(title.value as? String, "Hidden draft")
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testQuittingWithAnUntouchedVendorSheetCanCancelOrSaveTheOrder() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        app.toolbars.buttons["New Order"].click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Save this Order", in: app)
        let vendorName = element("vendor.sheet.name", in: app)
        for save in [false, true] {
            app.buttons["order.editor.vendor"].click()
            app.buttons["vendor.chooser.new"].click()
            XCTAssertTrue(vendorName.waitForExistence(timeout: 10))
            app.typeKey("q", modifierFlags: .command)
            XCTAssertTrue(vendorName.waitForNonExistence(timeout: 5))
            let answer = app.sheets.buttons[save ? "draft.unsaved.save" : "draft.unsaved.cancel"]
            XCTAssertTrue(answer.waitForExistence(timeout: 5))
            answer.click()
            if !save {
                XCTAssertEqual(title.value as? String, "Save this Order")
                XCTAssertTrue(title.isEnabled)
                XCTAssertEqual(try ordersDirectory(root), [])
            }
        }
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        XCTAssertEqual(try ordersDirectory(root).count, 1)
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
        let original = ["Sales", "Support", "Accounts", "Shipping"]
        for contact in original {
            element("vendor.editor.contacts.add", in: app).click()
            app.typeText("\(contact)\tQuotes\t555-0100\t\(contact.lowercased())@example.invalid\t")
        }
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.toolbars.buttons["Save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        let first = element("vendor.editor.contacts.row.0.name", in: app)
        element("vendor.editor.contacts.row.1.name", in: app).click()
        let last = element("vendor.editor.contacts.row.3.name", in: app)
        XCUIElement.perform(withKeyModifiers: .command) { last.click() }
        last.click(forDuration: 0.5, thenDragTo: first)
        let moved = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Support"), object: first)
        wait(for: [moved], timeout: 10)
        let reordered = ["Support", "Shipping", "Sales", "Accounts"]
        for (row, contact) in reordered.enumerated() {
            XCTAssertEqual(element("vendor.editor.contacts.row.\(row).name", in: app).value as? String, contact)
        }
        app.typeKey("z", modifierFlags: .command)
        for (row, contact) in original.enumerated() {
            XCTAssertEqual(element("vendor.editor.contacts.row.\(row).name", in: app).value as? String, contact)
        }
        app.typeKey("z", modifierFlags: [.command, .shift])
        for (row, contact) in reordered.enumerated() {
            XCTAssertEqual(element("vendor.editor.contacts.row.\(row).name", in: app).value as? String, contact)
        }
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.toolbars.buttons["Save"].wait(for: \.isEnabled, toEqual: false, timeout: 10))
        element("vendor.editor.back", in: app).click()
        app.outlines["vendors.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Optics Supplier")).firstMatch.doubleClick()
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        for (row, contact) in reordered.enumerated() {
            XCTAssertEqual(element("vendor.editor.contacts.row.\(row).name", in: app).value as? String, contact)
        }
    }
}
