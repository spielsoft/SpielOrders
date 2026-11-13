import AppKit
import XCTest

@MainActor final class MVPWorkflowTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    private func replace(_ field: XCUIElement, with text: String, in app: XCUIApplication) {
        field.click()
        app.typeKey("a", modifierFlags: .command)
        field.typeText(text)
    }

    func testInitializationChoosers() throws {
        let parent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = parent.appendingPathComponent("Package")
        let outside = parent.appendingPathComponent("Outside")
        let purchases = root.appendingPathComponent("Purchases")
        let removed = root.appendingPathComponent("Recoverable")
        for directory in [purchases, removed, outside] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        defer { try? FileManager.default.removeItem(at: parent) }
        let notes = purchases.appendingPathComponent("notes.txt")
        let original = Data("Existing collection notes".utf8)
        try original.write(to: notes)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        func choose(_ url: URL, prompt: String) {
            app.typeKey("g", modifierFlags: [.command, .shift])
            app.typeText(url.path)
            app.typeKey(.return, modifierFlags: [])
            app.buttons[prompt].click()
        }
        fileMenu(in: app).click()
        app.menuItems["New Package…"].click()
        choose(root, prompt: "Choose Package Folder")
        XCTAssertTrue(app.buttons["package.initialize.confirm"].waitForExistence(timeout: 10))
        for (key, directory) in [("ordersDirectory", purchases), ("removedDirectory", removed)] {
            let identifier = "package.initialize." + key
            let field = app.textFields[identifier]
            replace(field, with: "Planned", in: app)
            let picker = app.buttons[identifier + ".choose"]
            picker.click()
            let panel = element(identifier + ".picker", in: app)
            XCTAssertTrue(panel.waitForExistence(timeout: 10))
            panel.buttons["Cancel"].click()
            XCTAssertTrue(picker.wait(for: \.isEnabled, toEqual: true, timeout: 10))
            XCTAssertEqual(field.value as? String, "Planned")
            picker.click()
            choose(outside, prompt: "Choose Directory")
            let cancel = app.buttons[identifier + ".cancelChoice"]
            XCTAssertTrue(cancel.waitForExistence(timeout: 10))
            cancel.click()
            XCTAssertTrue(picker.wait(for: \.isEnabled, toEqual: true, timeout: 10))
            XCTAssertEqual(field.value as? String, "Planned")
            picker.click()
            choose(directory, prompt: "Choose Directory")
            let selected = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", directory.lastPathComponent), object: field)
            XCTAssertEqual(XCTWaiter.wait(for: [selected], timeout: 10), .completed)
        }
        let metadata = root.appendingPathComponent(".spielorders")
        XCTAssertFalse(FileManager.default.fileExists(atPath: metadata.path))
        XCTAssertEqual(try Data(contentsOf: notes), original)
        app.buttons["package.initialize.confirm"].click()
        let config = metadata.appendingPathComponent("config.toml")
        let initialized = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: config.path)
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [initialized], timeout: 10), .completed)
        let newOrder = app.buttons["order.new"]
        XCTAssertTrue(newOrder.waitForExistence(timeout: 10))
        XCTAssertTrue(newOrder.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        let text = try String(contentsOf: config, encoding: .utf8)
        XCTAssertTrue(text.contains("orders_path = \"Purchases\""))
        XCTAssertTrue(text.contains("removed_path = \"Recoverable\""))
        XCTAssertEqual(try Data(contentsOf: notes), original)
    }

    func testUnreadableInitialization() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let readme = root.appendingPathComponent("README.md")
        let original = Data([0xff, 0xfe])
        try original.write(to: readme)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        fileMenu(in: app).click()
        app.menuItems["New Package…"].click()
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(root.path)
        app.typeKey(.return, modifierFlags: [])
        app.buttons["Choose Package Folder"].click()
        let initialize = app.buttons["package.initialize.confirm"]
        XCTAssertTrue(initialize.waitForExistence(timeout: 10))
        initialize.click()
        let newOrder = app.buttons["order.new"]
        XCTAssertTrue(newOrder.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        newOrder.click()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Independent of the README", in: app)
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(element("editor.create", in: app).waitForNonExistence(timeout: 10))
        let row = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Independent of the README")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        XCTAssertEqual(try Data(contentsOf: readme), original)
    }

    func testSaveRetry() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Original optics")
        let orders = root.appendingPathComponent("Orders")
        let directory = try XCTUnwrap(FileManager.default.contentsOfDirectory(at: orders,
            includingPropertiesForKeys: nil).first { !$0.lastPathComponent.hasPrefix(".") })
        let record = directory.appendingPathComponent(".spielorders/order.json")
        let original = try Data(contentsOf: record)
        let stage = directory.appendingPathComponent(".spielorders/.order.json.spielorders-stage")
        let evidence = Data("Existing staging evidence".utf8)
        try evidence.write(to: stage, options: .withoutOverwriting)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let row = app.staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Original optics")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.doubleClick()
        let title = element("order.editor.title", in: app)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        replace(title, with: "Retained optics draft", in: app)
        app.typeKey(.tab, modifierFlags: [])
        let problem = app.buttons["editor.notSaved"]
        XCTAssertTrue(problem.waitForExistence(timeout: 10))
        XCTAssertTrue((problem.value as? String)?.contains("/.spielorders/order.json") == true)
        XCTAssertEqual(title.value as? String, "Retained optics draft")
        XCTAssertEqual(try Data(contentsOf: record), original)
        XCTAssertEqual(try Data(contentsOf: stage), evidence)
        try FileManager.default.removeItem(at: stage)
        problem.click()
        app.buttons["editor.retry"].click()
        let persisted = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let bytes = try? Data(contentsOf: record),
                  let saved = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { return false }
            return saved["title"] as? String == "Retained optics draft"
        }, object: nil)
        wait(for: [persisted], timeout: 10)
        XCTAssertTrue(title.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        let saved = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: record)) as? [String: Any])
        XCTAssertEqual(saved["title"] as? String, "Retained optics draft")
    }
}
