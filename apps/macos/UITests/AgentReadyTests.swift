import AppKit
import XCTest

@MainActor final class AgentReadyTests: XCTestCase {
    override func tearDown() async throws {
        for app in [XCUIApplication(bundleIdentifier: "com.spielsoft.SpielOrders.Development.CLI"), XCUIApplication()] {
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    func testCancellingAuthorizationLeavesThePackageUnchanged() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let instructions = root.appendingPathComponent("AGENTS.md")
        try "Keep the owner's instructions.\n".write(to: instructions, atomically: true, encoding: .utf8)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        packageMenu(in: app).click()
        app.menuItems["Agent Ready…"].click()
        let authorization = XCUIApplication(bundleIdentifier: "com.spielsoft.SpielOrders.Development.CLI")
        XCTAssertTrue(authorization.buttons["Authorize CLI"].waitForExistence(timeout: 10))
        authorization.buttons["Cancel"].click()
        XCTAssertTrue(authorization.wait(for: .notRunning, timeout: 10))
        XCTAssertTrue(app.staticTexts["Agent Ready cancelled. No guidance was written."].waitForExistence(timeout: 10))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(".agents").path))
        XCTAssertEqual(try String(contentsOf: instructions, encoding: .utf8), "Keep the owner's instructions.\n")
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testAuthorizationProvisionsGuidanceAndPreservesInstructions() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let instructions = root.appendingPathComponent("AGENTS.md")
        try "Keep the owner's instructions.\n".write(to: instructions, atomically: true, encoding: .utf8)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        packageMenu(in: app).click()
        app.menuItems["Agent Ready…"].click()
        let authorization = XCUIApplication(bundleIdentifier: "com.spielsoft.SpielOrders.Development.CLI")
        XCTAssertTrue(authorization.buttons["Authorize CLI"].waitForExistence(timeout: 10))
        authorization.typeKey("g", modifierFlags: [.command, .shift])
        authorization.typeText(root.path)
        authorization.typeKey(.return, modifierFlags: [])
        authorization.buttons["Authorize CLI"].click()
        XCTAssertTrue(authorization.wait(for: .notRunning, timeout: 10))
        let completed = app.staticTexts.matching(NSPredicate(format: "value CONTAINS %@", "CLI folder access authorized.")).firstMatch
        XCTAssertTrue(completed.waitForExistence(timeout: 10))
        let skill = root.appendingPathComponent(".agents/skills/spielorders/SKILL.md")
        XCTAssertFalse(try Data(contentsOf: skill).isEmpty)
        XCTAssertEqual(try String(contentsOf: instructions, encoding: .utf8), "Keep the owner's instructions.\n")
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testQuitCancelsAuthorizationBeforeAskingAboutTheDraft() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        app.toolbars.buttons["New Order"].click()
        let title = app.textFields["order.editor.title"]
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Authorization quit draft")
        app.typeKey(.tab, modifierFlags: [])
        XCTAssertEqual(title.value as? String, "Authorization quit draft")
        packageMenu(in: app).click()
        app.menuItems["Agent Ready…"].click()
        let authorization = XCUIApplication(bundleIdentifier: "com.spielsoft.SpielOrders.Development.CLI")
        XCTAssertTrue(authorization.buttons["Authorize CLI"].waitForExistence(timeout: 10))
        app.activate()
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(authorization.wait(for: .notRunning, timeout: 10))
        XCTAssertTrue(app.sheets.buttons["Cancel"].waitForExistence(timeout: 10))
        app.sheets.buttons["Cancel"].click()
        XCTAssertTrue(app.staticTexts["Agent Ready cancelled. No guidance was written."].waitForExistence(timeout: 5))
        XCTAssertEqual(title.value as? String, "Authorization quit draft")
        XCTAssertTrue(title.isEnabled)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(".agents").path))
        app.typeKey("q", modifierFlags: .command)
        let discard = app.sheets.buttons["draft.unsaved.discard"]
        XCTAssertTrue(discard.waitForExistence(timeout: 5))
        discard.click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path), [])
    }

}
