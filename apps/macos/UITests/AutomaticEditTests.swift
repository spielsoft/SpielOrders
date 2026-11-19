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

    /// The metadata of the Order whose stored Title is `title`.
    private func record(_ root: URL, titled title: String) throws -> URL {
        let directories = try FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("Orders"),
                                                                      includingPropertiesForKeys: nil)
        let records = directories.filter { !$0.lastPathComponent.hasPrefix(".") }
            .map { $0.appendingPathComponent(".spielorders/order.json") }
        return try XCTUnwrap(records.first { (try? read($0)["title"] as? String) == title })
    }

    private func expectValue(_ field: XCUIElement, _ value: String) {
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: field)
        XCTAssertEqual(XCTWaiter.wait(for: [shown], timeout: 10), .completed)
    }

    func testUndoInField() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let metadata = try record(root)
        let original = try Data(contentsOf: metadata)
        let app = launch(root)
        let title = app.textFields["order.editor.title"]
        // Undo while typing reverses the typing, and leaving the field then writes nothing.
        replace(title, with: "Typed optics", in: app)
        app.typeKey("z", modifierFlags: .command)
        expectValue(title, "Original optics")
        app.typeKey(.tab, modifierFlags: [])
        let written = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            (try? Data(contentsOf: metadata)) != original
        }, object: nil)
        written.isInverted = true
        wait(for: [written], timeout: 2)
        expectValue(title, "Original optics")

        // Return saves while the field keeps focus; Undo then reverses the saved edit, and the
        // field offers nothing more to undo.
        replace(title, with: "Saved optics", in: app)
        app.typeKey(.return, modifierFlags: [])
        try awaitValue(metadata, "title", "Saved optics")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(metadata, "title", "Original optics")
        expectValue(title, "Original optics")
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        app.menuBars.menuBarItems["Edit"].click()
        let undo = app.menuBars.menuItems["edit.undo"]
        let redo = app.menuBars.menuItems["edit.redo"]
        XCTAssertFalse(undo.isEnabled)
        XCTAssertTrue(redo.isEnabled)
        XCTAssertEqual(redo.label, "Redo Edit Order")
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey("z", modifierFlags: .command)
        expectValue(title, "Original optics")
        app.typeKey("z", modifierFlags: [.command, .shift])
        try awaitValue(metadata, "title", "Saved optics")
        expectValue(title, "Saved optics")
    }

    // Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.
    func testUndoAcrossSelection() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "First optics")
        try addOrder(in: root, title: "Second optics")
        let first = try record(root, titled: "First optics")
        let second = try record(root, titled: "Second optics")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let table = app.tables["orders.table"]
        func row(_ title: String) -> XCUIElement {
            table.staticTexts.matching(NSPredicate(format: "value == %@", title)).firstMatch
        }
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(row("First optics").waitForExistence(timeout: 10))
        row("First optics").click()
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        replace(title, with: "First edited", in: app)
        app.typeKey(.tab, modifierFlags: [])
        try awaitValue(first, "title", "First edited")

        // Selecting another Order keeps the edit in the window's Undo history.
        row("Second optics").click()
        expectValue(title, "Second optics")
        app.menuBars.menuBarItems["Edit"].click()
        let undo = app.menuBars.menuItems["Undo Edit Order"]
        XCTAssertTrue(undo.waitForExistence(timeout: 5))
        XCTAssertTrue(undo.isEnabled)
        undo.click()
        // Undo shows the edited Order and reverses the edit through its automatic save.
        try awaitValue(first, "title", "First optics")
        expectValue(title, "First optics")
        XCTAssertEqual(try read(second)["title"] as? String, "Second optics")

        row("Second optics").click()
        expectValue(title, "Second optics")
        app.typeKey("z", modifierFlags: [.command, .shift])
        try awaitValue(first, "title", "First edited")
        expectValue(title, "First edited")

        // Remove Order joins the same history, wherever the keyboard is.
        row("Second optics").rightClick()
        table.menuItems["Remove Order"].click()
        XCTAssertTrue(row("Second optics").waitForNonExistence(timeout: 10))
        row("First edited").click()
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        title.click()
        app.menuBars.menuBarItems["Edit"].click()
        let restore = app.menuBars.menuItems["Undo Remove Order"]
        XCTAssertTrue(restore.waitForExistence(timeout: 5))
        restore.click()
        XCTAssertTrue(row("Second optics").waitForExistence(timeout: 10))
    }

    func testReadmeUndoAcrossSelection() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "First optics")
        try addOrder(in: root, title: "Second optics")
        let first = try record(root, titled: "First optics")
        let notes = first.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("README.md")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let table = app.tables["orders.table"]
        func row(_ title: String) -> XCUIElement {
            table.staticTexts.matching(NSPredicate(format: "value == %@", title)).firstMatch
        }
        func awaitNotes(_ text: String) {
            let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                (try? String(contentsOf: notes, encoding: .utf8)) == text
            }, object: nil)
            XCTAssertEqual(XCTWaiter.wait(for: [saved], timeout: 10), .completed)
        }
        XCTAssertTrue(row("First optics").waitForExistence(timeout: 10))
        row("First optics").click()
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        replace(title, with: "First edited", in: app)
        app.typeKey(.tab, modifierFlags: [])
        try awaitValue(first, "title", "First edited")
        let readme = app.textViews["Order README"]
        readme.click()
        readme.typeText("Later README edit")
        awaitNotes("Later README edit")
        row("Second optics").click()
        expectValue(title, "Second optics")
        app.typeKey("z", modifierFlags: .command)
        awaitNotes("")
        expectValue(title, "First edited")
        XCTAssertEqual(try read(first)["title"] as? String, "First edited")
        app.typeKey("z", modifierFlags: .command)
        try awaitValue(first, "title", "First optics")
        app.typeKey("z", modifierFlags: [.command, .shift])
        try awaitValue(first, "title", "First edited")
        app.typeKey("z", modifierFlags: [.command, .shift])
        awaitNotes("Later README edit")

        // Undo is still a guarded README save, never an overwrite of another writer.
        try "Another writer".write(to: notes, atomically: true, encoding: .utf8)
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(app.buttons["readme.notSaved"].waitForExistence(timeout: 10))
        XCTAssertEqual(try String(contentsOf: notes, encoding: .utf8), "Another writer")
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

    func testSavedDateHoldsForm() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Steady optics", date: "2024-03-14", items: [("First lens", "1", "2")])
        let metadata = try record(root)
        let app = launch(root)
        let title = app.textFields["order.editor.title"]
        let date = element("order.editor.date_ordered", in: app)
        let cell = element("order.editor.items.row.0.description", in: app)
        XCTAssertTrue(cell.waitForExistence(timeout: 10))
        XCTAssertFalse(element("order.editor.align", in: app).exists)
        let shown = [title.frame, date.frame, cell.frame]
        date.click()
        app.typeKey(.upArrow, modifierFlags: [])
        func awaitStep(from stored: String?) {
            let stepped = XCTNSPredicateExpectation(predicate: NSPredicate { [self] _, _ in
                (try? read(metadata)["date_ordered"] as? String) != stored
            }, object: nil)
            wait(for: [stepped], timeout: 10)
        }
        awaitStep(from: "2024-03-14")
        // The saved date no longer matches the folder's name, and the save has settled once
        // Align is offered; the form beneath holds still.
        XCTAssertTrue(element("order.editor.align", in: app).waitForExistence(timeout: 10))
        XCTAssertEqual([title.frame, date.frame, cell.frame], shown)
        XCTAssertEqual(cell.value as? String, "First lens")
        // The date field keeps the keyboard: the next step goes to it.
        let stored = try read(metadata)["date_ordered"] as? String
        app.typeKey(.upArrow, modifierFlags: [])
        awaitStep(from: stored)
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
        let problem = app.buttons["order.editor.orderer.problem"]
        let premature = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == YES"), object: problem)
        premature.isInverted = true
        wait(for: [premature], timeout: 2)
        XCTAssertEqual(try Data(contentsOf: metadata), original)
        // Ending the field on text that names no Orderer keeps nothing: the field returns to
        // its value and nothing is written.
        app.typeKey(.tab, modifierFlags: [])
        expectValue(orderer, "")
        XCTAssertFalse(problem.exists)
        XCTAssertEqual(try Data(contentsOf: metadata), original)
        replace(orderer, with: "Unconfirmed Buyer", in: app)
        let new = orderer.textFields["New Orderer “Unconfirmed Buyer”…"]
        XCTAssertTrue(new.waitForExistence(timeout: 5))
        new.click()
        app.sheets.buttons["order.editor.addValue.confirm"].click()
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
        // The Vendors table still lists the hidden Vendor, marked.
        let mark = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier ENDSWITH '.hidden' AND value == 'Hidden'")).firstMatch
        XCTAssertTrue(mark.waitForExistence(timeout: 10))
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
