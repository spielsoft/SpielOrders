import AppKit
import XCTest

@MainActor final class AutomaticEditTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    private func replace(_ field: XCUIElement, with value: String, in app: XCUIApplication) {
        field.click()
        app.typeKey("a", modifierFlags: .command)
        field.typeText(value)
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    private func record(_ root: URL) throws -> URL {
        let directory = try XCTUnwrap(FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("Orders"),
            includingPropertiesForKeys: nil).first { !$0.lastPathComponent.hasPrefix(".") })
        return directory.appendingPathComponent(".spielorders/order.json")
    }

    private func read(_ url: URL) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    private func awaitValue(_ url: URL, _ key: String, _ expected: String?) throws {
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: url),
                  let value = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { return false }
            return value[key] as? String == expected
        }, object: nil)
        wait(for: [changed], timeout: 10)
        XCTAssertEqual(try read(url)[key] as? String, expected)
    }

    private func launch(_ root: URL) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let row = app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "identifier ENDSWITH %@", ".title")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        // Selection shows the Order's editable detail directly.
        XCTAssertTrue(app.textFields["order.editor.title"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.textFields["order.editor.title"].wait(for: \.isEnabled, toEqual: true, timeout: 10))
        return app
    }

    func testFieldUndo() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let metadata = try record(root)
        let app = launch(root)
        let title = app.textFields["order.editor.title"]
        replace(title, with: "Automatic optics", in: app)
        app.typeKey(.tab, modifierFlags: [])
        try awaitValue(metadata, "title", "Automatic optics")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(metadata, "title", "Original optics")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.typeKey("z", modifierFlags: [.command, .shift])
        try awaitValue(metadata, "title", "Automatic optics")
        // Focus leaves the form for the list; the record stays shown and saved.
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Automatic optics")).firstMatch.click()
        XCTAssertTrue(title.exists)
        XCTAssertFalse(app.buttons["draft.unsaved.save"].exists)
        app.menuBars.menuBarItems["Edit"].click()
        let undo = app.menuBars.menuItems.matching(NSPredicate(format: "label BEGINSWITH %@", "Undo")).firstMatch
        XCTAssertTrue(undo.exists)
        XCTAssertTrue(undo.isEnabled)
        let redo = app.menuBars.menuItems.matching(NSPredicate(format: "label BEGINSWITH %@", "Redo")).firstMatch
        XCTAssertTrue(redo.exists)
        XCTAssertFalse(redo.isEnabled)
        undo.click()
        try awaitValue(metadata, "title", "Original optics")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.menuBars.menuBarItems["Edit"].click()
        XCTAssertTrue(redo.exists)
        XCTAssertTrue(redo.isEnabled)
        redo.click()
        try awaitValue(metadata, "title", "Automatic optics")
        app.menuBars.menuBarItems["SpielOrders"].click()
        app.menuBars.menuItems["About SpielOrders"].click()
        XCTAssertTrue(app.windows["About SpielOrders"].waitForExistence(timeout: 10))
        app.menuBars.menuBarItems["Edit"].click()
        XCTAssertTrue(undo.exists)
        XCTAssertFalse(undo.isEnabled)
        XCTAssertTrue(redo.exists)
        XCTAssertFalse(redo.isEnabled)
    }

    func testRapidFields() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let metadata = try record(root)
        let app = launch(root)
        let title = app.textFields["order.editor.title"]
        for round in 4...6 {
            title.click()
            app.typeKey("a", modifierFlags: .command)
            let name = "Rapid optics \(round)"
            let date = try read(metadata)["date_ordered"] as? String
            // One uninterrupted keyboard operation crosses the automatic-save boundary: the
            // Title, then a step of the Date Ordered field it tabs into.
            app.typeText(name + "\t")
            app.typeKey(.upArrow, modifierFlags: [])
            app.typeKey(.tab, modifierFlags: [])
            try awaitValue(metadata, "title", name)
            let stepped = XCTNSPredicateExpectation(predicate: NSPredicate { [self] _, _ in
                (try? read(metadata)["date_ordered"] as? String) != date
            }, object: nil)
            wait(for: [stepped], timeout: 10)
            XCTAssertFalse(app.buttons["editor.notSaved"].exists)
        }
    }

    func testRapidCells() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, items: [("First lens", "1", "2"), ("Second lens", "3", "5")])
        let metadata = try record(root)
        let app = launch(root)
        element("order.editor.items.row.0.description", in: app).doubleClick()
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Fluid lens\t4\t0.375\t")
        let persisted = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: metadata),
                  let record = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let items = record["items"] as? [[String: Any]], items.count == 2 else { return false }
            return items[0]["description"] as? String == "Fluid lens"
                && (items[0]["quantity"] as? NSNumber)?.stringValue == "4"
                && (items[0]["unit_price"] as? NSNumber)?.stringValue == "0.375"
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [persisted], timeout: 10), .completed)
        XCTAssertFalse(app.buttons["editor.notSaved"].exists)
    }

    func testPausedCell() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics", items: [("First lens", "1", "2")])
        let metadata = try record(root)
        let app = launch(root)
        replace(app.textFields["order.editor.title"], with: "Owning record", in: app)
        element("order.editor.items.row.0.description", in: app).doubleClick()
        try awaitValue(metadata, "title", "Owning record")
        XCTAssertTrue(app.progressIndicators["Working"].waitForNonExistence(timeout: 10))
        // Use the current responder after the save settles, without refocusing the cell.
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Cell keeps focus\t")
        let persisted = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: metadata),
                  let record = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let items = record["items"] as? [[String: Any]] else { return false }
            return record["title"] as? String == "Owning record"
                && items.first?["description"] as? String == "Cell keeps focus"
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [persisted], timeout: 10), .completed)
    }

    func testTypingUndo() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let metadata = try record(root)
        let original = try Data(contentsOf: metadata)
        let app = launch(root)
        let orderer = element("order.editor.orderer", in: app)
        replace(orderer, with: "Unconfirmed Buyer", in: app)
        app.typeKey("z", modifierFlags: .command)
        XCTAssertEqual(orderer.value as? String, "")
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertEqual(orderer.value as? String, "Unconfirmed Buyer")
        let confirm = app.buttons["order.editor.newValues.confirm"]
        let premature = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == YES"), object: confirm)
        premature.isInverted = true
        wait(for: [premature], timeout: 2)
        XCTAssertEqual(try Data(contentsOf: metadata), original)
        // Only ending the field asks whether the new reusable value should be saved.
        app.typeKey(.tab, modifierFlags: [])
        XCTAssertTrue(confirm.waitForExistence(timeout: 10))
        confirm.click()
        try awaitValue(metadata, "orderer", "Unconfirmed Buyer")
        app.tables["orders.table"].click()
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(metadata, "orderer", nil)
        XCTAssertNil(try read(metadata)["orderer"])
    }

    func testRowInverse() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, items: [("First", "1", "2"), ("Second", "3", "5")])
        let metadata = try record(root)
        let app = launch(root)
        let quantity = element("order.editor.items.row.0.quantity", in: app)
        quantity.doubleClick()
        app.typeKey("a", modifierFlags: .command)
        quantity.typeText("invalid\t")
        XCTAssertTrue(app.buttons["editor.notSaved"].waitForExistence(timeout: 10))
        app.buttons["order.editor.items.remove"].click()
        let removed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: metadata),
                  let value = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let items = value["items"] as? [[String: Any]] else { return false }
            return items.count == 1 && items.first?["description"] as? String == "Second"
        }, object: nil)
        wait(for: [removed], timeout: 10)
        XCTAssertTrue(app.textFields["order.editor.title"].wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.tables["order.editor.items"].click()
        app.typeKey("z", modifierFlags: .command)
        let restored = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: metadata),
                  let value = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let items = value["items"] as? [[String: Any]] else { return false }
            return items.count == 2
        }, object: nil)
        wait(for: [restored], timeout: 10)
        let items = try XCTUnwrap(try read(metadata)["items"] as? [[String: Any]])
        XCTAssertEqual(items.compactMap { $0["description"] as? String }, ["First", "Second"])
        XCTAssertEqual(items.compactMap { ($0["quantity"] as? NSNumber)?.doubleValue }, [1, 3])
    }

    func testItemNotes() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, items: [("Viewport", "1", "389")])
        let metadata = try record(root)
        let app = launch(root)
        func notes() -> String? {
            guard let bytes = try? Data(contentsOf: metadata),
                  let value = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let items = value["items"] as? [[String: Any]] else { return nil }
            return items.first?["notes"] as? String ?? ""
        }
        let cell = element("order.editor.items.row.0.notes", in: app)
        let indicator = element("order.editor.items.row.0.notes.indicator", in: app)
        XCTAssertTrue(element("order.editor.items.row.0.description", in: app).waitForExistence(timeout: 10))
        XCTAssertFalse(indicator.exists)
        // The Notes column is chosen from the header's column menu, which remembers the choice.
        if !cell.exists {
            element("order.editor.items.column.description", in: app).rightClick()
            element("order.editor.items.column.visibility.notes", in: app).click()
        }
        XCTAssertTrue(cell.waitForExistence(timeout: 5))
        cell.doubleClick()
        let editor = element("order.editor.items.row.0.notes.editor", in: app)
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        editor.typeText("Quote Q-7\nSubstitute allowed")
        app.typeKey(.escape, modifierFlags: [])
        let written = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            notes() == "Quote Q-7\nSubstitute allowed"
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [written], timeout: 10), .completed)
        XCTAssertTrue(indicator.waitForExistence(timeout: 5))
        app.tables["order.editor.items"].click()
        app.typeKey("z", modifierFlags: .command)
        let undone = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in notes() == "" }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [undone], timeout: 10), .completed)
        XCTAssertTrue(indicator.waitForNonExistence(timeout: 5))
    }

    func testHistoricalDate() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, date: "2020-01-02", status: "ordered")
        let metadata = try record(root)
        let app = launch(root)
        let status = app.popUpButtons["order.editor.status"]
        status.click()
        app.menuItems["Received"].click()
        try awaitValue(metadata, "status", "received")
        XCTAssertTrue(app.textFields["order.editor.title"].wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.textFields["order.editor.title"].click()
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(metadata, "status", "ordered")
        XCTAssertEqual(try read(metadata)["date_ordered"] as? String, "2020-01-02")
    }

    func testDateClearing() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, date: "2020-01-02", status: "ordered")
        let metadata = try record(root)
        let app = launch(root)
        element("order.editor.date_ordered.clear", in: app).click()
        // The library requires a date, so the cleared field stays Not Saved and empty.
        XCTAssertTrue(app.buttons["editor.notSaved"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("order.editor.date_ordered.choose", in: app).exists)
        XCTAssertEqual(try read(metadata)["date_ordered"] as? String, "2020-01-02")
    }

    func testNativeFocus() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let metadata = try record(root)
        let app = launch(root)
        let title = app.textFields["order.editor.title"]
        replace(title, with: "Focus result", in: app)
        let orderer = app.comboBoxes["order.editor.orderer"]
        orderer.click()
        try awaitValue(metadata, "title", "Focus result")
        XCTAssertTrue(orderer.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        // Type through the actual first responder, not an element helper that refocuses it.
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Expected orderer")
        XCTAssertEqual(orderer.value as? String, "Expected orderer")
        XCTAssertEqual(title.value as? String, "Focus result")
    }

    func testFailureUndo() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics", items: [("Lens", "1", "2")])
        let metadata = try record(root)
        let app = launch(root)
        let quantity = element("order.editor.items.row.0.quantity", in: app)
        quantity.doubleClick()
        app.typeKey("a", modifierFlags: .command)
        quantity.typeText("invalid\t")
        XCTAssertTrue(app.buttons["editor.notSaved"].waitForExistence(timeout: 10))
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Original optics")).firstMatch.click()
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(app.buttons["editor.notSaved"].waitForNonExistence(timeout: 10))
        replace(title, with: "After failure", in: app)
        app.typeKey(.tab, modifierFlags: [])
        try awaitValue(metadata, "title", "After failure")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(metadata, "title", "Original optics")
    }

    func testVendorEditsWithScopedSearch() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        showDestination("vendors", in: app)
        app.buttons["vendor.new"].click()
        let name = app.textFields["vendor.editor.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Scope Vendor", in: app)
        app.buttons["editor.create"].click()
        XCTAssertTrue(app.buttons["vendor.editor.visibility"].waitForExistence(timeout: 10))
        chooseSearchIn("Vendor Name", in: app)
        let search = app.searchFields["search.query"]
        search.click()
        search.typeText("Scope")
        let listed = app.tables["vendors.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Scope Vendor")).firstMatch
        XCTAssertTrue(listed.waitForExistence(timeout: 10))

        let phone = app.textFields["vendor.editor.phone"]
        let path = root.appendingPathComponent(".spielorders/vendors.json")
        for value in ["555-0100", "555-0101"] {
            XCTAssertTrue(phone.wait(for: \.isEnabled, toEqual: true, timeout: 10))
            replace(phone, with: value, in: app)
            name.click()
            let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                guard let bytes = try? Data(contentsOf: path),
                      let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                      let vendors = object["vendors"] as? [[String: Any]] else { return false }
                return vendors.first?["phone"] as? String == value
            }, object: nil)
            wait(for: [saved], timeout: 10)
            XCTAssertTrue(phone.wait(for: \.isEnabled, toEqual: true, timeout: 10))
            XCTAssertTrue(listed.waitForExistence(timeout: 10))
            XCTAssertFalse(app.buttons["editor.notSaved"].exists)
        }
        let vendors = try XCTUnwrap(try read(path)["vendors"] as? [[String: Any]])
        XCTAssertEqual(vendors.first?["phone"] as? String, "555-0101")
    }

    func testVisibilityDraft() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        showDestination("vendors", in: app)
        app.buttons["vendor.new"].click()
        let name = app.textFields["vendor.editor.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(name, with: "Visibility Vendor", in: app)
        app.buttons["editor.create"].click()
        // The created Vendor stays in its editor, now with its visibility control.
        let visibility = app.buttons["vendor.editor.visibility"]
        XCTAssertTrue(visibility.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["editor.create"].exists)
        visibility.click()
        XCTAssertTrue(app.buttons["Show Vendor"].waitForExistence(timeout: 10))
        // The heading's summary says the Vendor is hidden.
        let summary = element("vendor.editor.summary", in: app)
        XCTAssertTrue(summary.waitForExistence(timeout: 10))
        XCTAssertEqual(summary.value as? String, "No Orders · Hidden")
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        replace(app.textFields["vendor.editor.phone"], with: "555-0100", in: app)
        name.click()
        let path = root.appendingPathComponent(".spielorders/vendors.json")
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: path),
                  let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  let vendors = object["vendors"] as? [[String: Any]] else { return false }
            return vendors.first?["phone"] as? String == "555-0100"
        }, object: nil)
        wait(for: [saved], timeout: 10)
        XCTAssertFalse(app.buttons["editor.notSaved"].exists)
        let vendors = try XCTUnwrap(try read(path)["vendors"] as? [[String: Any]])
        XCTAssertEqual(vendors.first?["phone"] as? String, "555-0100")
    }
}
