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
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
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

    func testSaveRefusal() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "New Order")
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeKey(.delete, modifierFlags: [])
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tGasket\ttwo\t2.5\t")
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(element("editor.notSaved", in: app).waitForExistence(timeout: 10))
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
        app.typeKey(.return, modifierFlags: .command)
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            let names = try? FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path)
            return names?.filter { !$0.hasPrefix(".") }.count == 1
        }, object: nil)
        wait(for: [saved], timeout: 10)
        // The created Order stays in its editor, now without Create.
        XCTAssertTrue(element("editor.create", in: app).waitForNonExistence(timeout: 10))
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Vacuum parts"), object: heading)],
             timeout: 10)
        XCTAssertEqual(title.value as? String, "Vacuum parts")
        XCTAssertEqual(try ordersDirectory(root).count, 1)
        // The heading is the Title alone; the folder shows beneath, aligned when created.
        XCTAssertFalse(element("order.editor.summary", in: app).exists)
        XCTAssertEqual(element("order.editor.directory", in: app).value as? String, try ordersDirectory(root).first)
        XCTAssertFalse(element("order.editor.align", in: app).exists)
        // The library's Total sits in the Items footer.
        let total = element("order.editor.total", in: app)
        XCTAssertTrue(total.staticTexts.matching(NSPredicate(format: "value == %@", "7.50")).firstMatch.exists)
    }

    func testRowUndo() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
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
        app.typeKey(.return, modifierFlags: .command)
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
        app.buttons["draft.unsaved.confirm"].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    func testStagedNavigation() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Existing optics")
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Uncreated cryostat", in: app)
        let row = app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Existing optics")).firstMatch
        row.click()
        let cancel = app.buttons["draft.unsaved.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["draft.unsaved.save"].exists)
        cancel.click()
        XCTAssertEqual(title.value as? String, "Uncreated cryostat")
        XCTAssertEqual(try ordersDirectory(root).count, 1)
        row.click()
        app.buttons["draft.unsaved.confirm"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Existing optics"), object: title)],
             timeout: 10)
        XCTAssertFalse(app.buttons["editor.create"].exists)
        XCTAssertEqual(try ordersDirectory(root).count, 1)
    }

    func testSelectionEditing() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "First optics", date: "2026-01-02")
        try addOrder(in: root, title: "Second optics", date: "2026-01-01")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES", "-orderSortColumn", "date",
                               "-orderSortDescending", "YES"]
        app.launch()
        openPackage(root, in: app)
        let table = app.tables["orders.table"]
        table.staticTexts.matching(NSPredicate(format: "value == %@", "First optics")).firstMatch.click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "First optics")
        XCTAssertFalse(app.buttons["editor.create"].exists)
        // Arrow keys keep browsing the list: each selection shows its editor without taking focus.
        app.typeKey(.downArrow, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Second optics"), object: title)],
             timeout: 10)
        // Return focuses the shown Order's first field, so typing replaces its Title.
        app.typeKey(.return, modifierFlags: [])
        app.typeText("Renamed optics\t")
        let renamed = table.staticTexts.matching(NSPredicate(format: "value == %@", "Renamed optics")).firstMatch
        XCTAssertTrue(renamed.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["draft.unsaved.save"].exists)
    }

    func testFreshDraft() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Optics", in: app)
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tLens\t1\t2.5\t")
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(element("editor.create", in: app).waitForNonExistence(timeout: 10))
        let rowTitle = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Optics")).firstMatch
        XCTAssertTrue(rowTitle.waitForExistence(timeout: 10))
        rowTitle.doubleClick()
        XCTAssertTrue(element("order.editor.items.row.0.description", in: app).waitForExistence(timeout: 10))
        newOrder(in: app)
        let startingTitle = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "New Order"), object: title)
        wait(for: [startingTitle], timeout: 10)
        XCTAssertFalse(element("order.editor.items.row.0.description", in: app).exists)
    }

    func testProjectGroups() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Coils", project: "Greenhouse", group: "Misting")
        try addOrder(in: root, title: "Mirrors", project: "Optics", group: "Imaging")
        let app = launch(root)
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Coils")).firstMatch.click()
        let group = app.comboBoxes["order.editor.order_group"]
        XCTAssertTrue(group.waitForExistence(timeout: 10))
        // Suggestions are the groups of the Order's own Project.
        group.buttons.firstMatch.click()
        XCTAssertTrue(group.textFields["Misting"].waitForExistence(timeout: 10))
        XCTAssertFalse(group.textFields["Imaging"].exists)
        app.typeKey(.escape, modifierFlags: [])
        let project = app.comboBoxes["order.editor.project"]
        replace(project, with: "Optics", in: app)
        app.typeKey(.tab, modifierFlags: [])
        let cleared = app.buttons["order.editor.order_group.note"]
        XCTAssertTrue(cleared.waitForExistence(timeout: 10))
        XCTAssertEqual(cleared.value as? String, "Order Group cleared: “Optics” has no group named “Misting”.")
        XCTAssertEqual(group.value as? String, "")
        group.buttons.firstMatch.click()
        XCTAssertTrue(group.textFields["Imaging"].waitForExistence(timeout: 10))
    }

    func testVendorChoice() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let vendor = app.comboBoxes["order.editor.vendor"]
        beginNewVendor(named: "Optics Supplier", in: app)
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        XCTAssertEqual(name.value as? String, "Optics Supplier")
        app.typeKey(.return, modifierFlags: .command)
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)

        // Typing filters the list; the text that names no Vendor offers to add it last.
        replace(vendor, with: "supp", in: app)
        XCTAssertTrue(vendor.textFields["Optics Supplier"].waitForExistence(timeout: 5))
        XCTAssertTrue(vendor.textFields["New Vendor “supp”…"].exists)
        vendor.textFields["Optics Supplier"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Optics Supplier"), object: vendor)], timeout: 10)

        // Clearing the text chooses no Vendor.
        replace(vendor, with: "", in: app)
        app.typeKey(.delete, modifierFlags: [])
        app.typeKey(.tab, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", ""), object: vendor)], timeout: 10)
        XCTAssertEqual(try ordersDirectory(root), [])

        // Text that names no Vendor is not kept: the field returns to its value.
        replace(vendor, with: "Nobody", in: app)
        app.typeKey(.tab, modifierFlags: [])
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", ""), object: vendor)], timeout: 10)
        XCTAssertFalse(app.buttons["order.editor.vendor.problem"].exists)
        XCTAssertEqual(try ordersDirectory(root), [])

        beginNewVendor(named: "Vacuum Supplier", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        app.typeKey(.return, modifierFlags: .command)
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Vacuum Supplier"), object: vendor)], timeout: 10)
    }

    func testNewValueFromChooser() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Coils", project: "Greenhouse", group: "Misting")
        let app = launch(root)
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Coils")).firstMatch.click()
        let orderer = app.comboBoxes["order.editor.orderer"]
        XCTAssertTrue(orderer.waitForExistence(timeout: 10))
        replace(orderer, with: "Ada", in: app)
        let new = orderer.textFields["New Orderer “Ada”…"]
        XCTAssertTrue(new.waitForExistence(timeout: 5))
        new.click()
        // The value is added only after the confirmation.
        let cancel = app.sheets.buttons["order.editor.addValue.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(orderer.value as? String, "")
        replace(orderer, with: "Ada", in: app)
        orderer.textFields["New Orderer “Ada”…"].click()
        app.sheets.buttons["order.editor.addValue.confirm"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Ada"), object: orderer)], timeout: 10)

        // A group added from the field belongs to the Order's Project.
        let group = app.comboBoxes["order.editor.order_group"]
        replace(group, with: "Cooling", in: app)
        group.textFields["New Order Group “Cooling”…"].click()
        app.sheets.buttons["order.editor.addValue.confirm"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Cooling"), object: group)], timeout: 10)
        showDestination("projects", in: app)
        let cooling = app.descendants(matching: .any).matching(identifier: "projects.row.Greenhouse/Cooling.orders").firstMatch
        XCTAssertTrue(cooling.waitForExistence(timeout: 10))
    }

    func testContactEditing() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let vendorsFile = root.appendingPathComponent(".spielorders/vendors.json")
        let originalVendors = try Data(contentsOf: vendorsFile)
        let app = launch(root)
        newOrder(in: app)
        beginNewVendor(named: "Contact supplier", in: app)
        let add = app.buttons["vendor.sheet.contacts.add"]
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        add.click()
        app.typeText("Avery")
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
        app.typeText("Blair")
        app.typeKey(.tab, modifierFlags: [])
        XCTAssertTrue(app.buttons["vendor.sheet.create"].exists)
        XCTAssertEqual(element("vendor.sheet.contacts.row.0.name", in: app).value as? String, "Blair")
        XCTAssertEqual(try Data(contentsOf: vendorsFile), originalVendors)
    }

    func testItemsKeepReturnInNewOrder() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Optics", in: app)
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1")
        app.typeKey(.escape, modifierFlags: [])
        // Return edits the selected Item rather than creating the Order.
        app.typeKey(.return, modifierFlags: [])
        app.typeText("CAT-2")
        app.typeKey(.tab, modifierFlags: [])
        XCTAssertEqual(element("order.editor.items.row.0.catalog_number", in: app).value as? String, "CAT-2")
        XCTAssertTrue(app.buttons["editor.create"].exists)
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testDeleteKeyAfterRemovingItem() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Bench optics", items: [("Lens", "1", "1")])
        let app = launch(root)
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Bench optics")).firstMatch.click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        let description = element("order.editor.items.row.0.description", in: app)
        XCTAssertTrue(description.waitForExistence(timeout: 10))
        description.click()
        element("order.editor.items.remove", in: app).click()
        XCTAssertTrue(description.waitForNonExistence(timeout: 10))
        // The keyboard is nowhere after −, so Delete removes nothing more.
        app.typeKey(.delete, modifierFlags: [])
        XCTAssertFalse(element("order.editor.title", in: app).waitForNonExistence(timeout: 2))
        XCTAssertEqual(title.value as? String, "Bench optics")
        XCTAssertEqual(try ordersDirectory(root).count, 1)
    }

    func testNestedCancellation() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Order draft", in: app)
        beginNewVendor(named: "Vendor draft", in: app)
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

    func testFailedNavigation() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "First optics")
        try addOrder(in: root, title: "Second optics")
        let app = launch(root)
        let table = app.tables["orders.table"]
        table.staticTexts.matching(NSPredicate(format: "value == %@", "First optics")).firstMatch.click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeKey(.delete, modifierFlags: [])
        table.staticTexts.matching(NSPredicate(format: "value == %@", "Second optics")).firstMatch.click()
        XCTAssertTrue(element("editor.notSaved", in: app).waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "")
        XCTAssertFalse(app.buttons["draft.unsaved.save"].exists)
        element("editor.notSaved", in: app).click()
        app.buttons["editor.revert"].click()
        app.buttons["editor.revert.confirm"].click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "First optics"), object: title)], timeout: 10)
        table.staticTexts.matching(NSPredicate(format: "value == %@", "Second optics")).firstMatch.click()
        wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "Second optics"), object: title)],
             timeout: 10)
    }

    func testNestedQuit() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Keep this Order", in: app)
        beginNewVendor(named: "Vendor draft", in: app)
        let name = element("vendor.sheet.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Discard this Vendor", in: app)
        app.typeKey("q", modifierFlags: .command)
        let discard = app.buttons["draft.unsaved.confirm"]
        XCTAssertTrue(discard.waitForExistence(timeout: 5))
        discard.click()
        XCTAssertTrue(name.waitForNonExistence(timeout: 5))
        let cancel = app.buttons["draft.unsaved.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(title.value as? String, "Keep this Order")
        XCTAssertTrue(title.isEnabled)
        XCTAssertEqual(try ordersDirectory(root), [])

        // A Vendor sheet with nothing typed closes without asking; the Order draft still asks.
        beginNewVendor(named: "Untouched supplier", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(name.waitForNonExistence(timeout: 5))
        let answer = app.sheets.buttons["draft.unsaved.confirm"]
        XCTAssertTrue(answer.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["draft.unsaved.save"].exists)
        answer.click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        XCTAssertEqual(try ordersDirectory(root), [])
    }

    func testHiddenQuit() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
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

    func testContactReorder() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        showDestination("vendors", in: app)
        app.buttons["vendor.new"].click()
        let name = element("vendor.editor.name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Optics Supplier", in: app)
        let original = ["Sales", "Support", "Accounts", "Shipping"]
        for contact in original {
            element("vendor.editor.contacts.add", in: app).click()
            app.typeText("\(contact)\tQuotes\t555-0100\t\(contact.lowercased())@example.invalid\t")
        }
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(element("editor.create", in: app).waitForNonExistence(timeout: 10))
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
        // Redo commits through the same automatic save as the reorder itself.
        let path = root.appendingPathComponent(".spielorders/vendors.json")
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: path),
                  let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let vendors = object["vendors"] as? [[String: Any]],
                  let contacts = vendors.first?["contacts"] as? [[String: Any]] else { return false }
            return contacts.compactMap { $0["name"] as? String } == reordered
        }, object: nil)
        wait(for: [saved], timeout: 10)
        app.outlines["vendors.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Optics Supplier")).firstMatch.doubleClick()
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        for (row, contact) in reordered.enumerated() {
            XCTAssertEqual(element("vendor.editor.contacts.row.\(row).name", in: app).value as? String, contact)
        }
    }
}
