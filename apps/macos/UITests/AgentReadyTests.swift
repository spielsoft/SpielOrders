import AppKit
import XCTest

@MainActor final class AgentReadyTests: XCTestCase {
    override func tearDown() async throws {
        let app = XCUIApplication()
        if app.state != .notRunning { app.terminate() }
        try await super.tearDown()
    }

    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathExtension("splord")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        return url
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        return app
    }

    private func choose(_ root: URL, in app: XCUIApplication, prompt: String) {
        XCTAssertTrue(app.buttons[prompt].waitForExistence(timeout: 10))
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10), "A requested permission panel must receive focus before entering its folder.")
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(root.path)
        app.typeKey(.return, modifierFlags: [])
        app.buttons[prompt].click()
    }

    private func begin(in app: XCUIApplication) {
        fileMenu(in: app).click()
        let action = app.menuItems["file.agentReady"]
        XCTAssertTrue(action.isEnabled)
        action.click()
    }

    private func notice(_ title: String, in app: XCUIApplication) {
        XCTAssertTrue(app.staticTexts[title].waitForExistence(timeout: 10))
        app.buttons["agentReady.dismiss"].click()
    }

    func testGuidanceWithoutPackage() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let instructions = root.appendingPathComponent("AGENTS.md")
        try "Keep the owner's instructions.\n".write(to: instructions, atomically: true, encoding: .utf8)
        let app = launch()
        let ready = app.buttons["welcome.agentReady"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertTrue(ready.isEnabled)
        ready.click()
        choose(root, in: app, prompt: "Choose Folder")
        notice("“\(root.lastPathComponent)” is ready for agents", in: app)
        XCTAssertTrue(app.buttons["welcome.open"].exists)
        XCTAssertEqual(app.windows["package.window"].label, "SpielOrders")
        let skill = root.appendingPathComponent(".agents/skills/spielorders/SKILL.md")
        XCTAssertFalse(try Data(contentsOf: skill).isEmpty)
        XCTAssertEqual(try String(contentsOf: instructions, encoding: .utf8), "Keep the owner's instructions.\n")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(".spielorders").path))
    }

    func testProvisioningElsewhereKeepsDraftAndPackage() throws {
        let package = try makePackage()
        let target = try directory()
        defer {
            try? FileManager.default.removeItem(at: package)
            try? FileManager.default.removeItem(at: target)
        }
        let app = launch()
        openPackage(package, in: app)
        newOrder(in: app)
        let title = app.textFields["order.editor.title"]
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Keep this draft")
        app.typeKey(.tab, modifierFlags: [])
        begin(in: app)
        XCTAssertTrue(app.buttons["Choose Folder"].waitForExistence(timeout: 10))
        app.sheets.buttons["Cancel"].click()
        // A cancelled folder choice reports nothing and writes nothing.
        XCTAssertFalse(app.buttons["agentReady.dismiss"].exists)
        XCTAssertEqual(title.value as? String, "Keep this draft")
        XCTAssertTrue(title.isEnabled)
        XCTAssertEqual(app.windows["package.window"].label, package.lastPathComponent)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: package.appendingPathComponent("Orders").path).isEmpty)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: target.path).isEmpty)
        begin(in: app)
        choose(target, in: app, prompt: "Choose Folder")
        notice("“\(target.lastPathComponent)” is ready for agents", in: app)
        XCTAssertEqual(title.value as? String, "Keep this draft")
        XCTAssertTrue(title.isEnabled)
        XCTAssertEqual(app.windows["package.window"].label, package.lastPathComponent)
        XCTAssertFalse(FileManager.default.fileExists(atPath: package.appendingPathComponent(".agents").path))
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: package.appendingPathComponent("Orders").path).isEmpty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: target.appendingPathComponent(".agents/skills/spielorders/SKILL.md").path))
    }

    func testGuidanceConflict() throws {
        let target = try directory()
        defer { try? FileManager.default.removeItem(at: target) }
        let conflict = target.appendingPathComponent(".agents")
        let original = Data("Owner file".utf8)
        try original.write(to: conflict)
        let app = launch()
        begin(in: app)
        choose(target, in: app, prompt: "Choose Folder")
        XCTAssertTrue(app.staticTexts["Agent guidance incomplete"].waitForExistence(timeout: 10))
        app.buttons["agentReady.details"].click()
        let diagnostic = app.staticTexts["feedback.detail.0"]
        XCTAssertTrue(diagnostic.waitForExistence(timeout: 10))
        XCTAssertTrue((diagnostic.value as? String)?.contains("destination_collision") == true)
        app.buttons["feedback.done"].click()
        XCTAssertEqual(try Data(contentsOf: conflict), original)
        XCTAssertFalse(FileManager.default.fileExists(atPath: target.appendingPathComponent(".spielorders").path))
    }

}
