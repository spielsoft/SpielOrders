import XCTest
import CSpielOrders

// Decision sources: Desktop acceptance; Native desktop distribution.
@MainActor final class NativeWindowTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    func testPackagePickerIsAvailableInTheRealWindow() {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        XCTAssertTrue(app.buttons["Open Package…"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.windows.count, 1)
        app.buttons["Open Package…"].click()
        XCTAssertTrue(app.dialogs.firstMatch.waitForExistence(timeout: 5))
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testAboutPresentsAppIdentityAndClosesWithoutEndingTheApp() {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        app.menuBars.menuBarItems["SpielOrders"].click()
        app.menuBars.menuItems["About SpielOrders"].click()
        let about = app.windows["About SpielOrders"]
        XCTAssertTrue(about.waitForExistence(timeout: 5))
        XCTAssertTrue(about.links["Source code"].exists)
        let version = about.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Version ")).firstMatch
        XCTAssertTrue(version.exists)
        XCTAssertTrue(version.label.range(of: #"Version [0-9.]+ \([0-9]+\)"#, options: .regularExpression) != nil)
        XCTAssertTrue(about.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "A SpielSoft product")).firstMatch.exists)
        about.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(app.buttons["Open Package…"].exists)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testAuthorizedReadmeSaveSurvivesRestartAndRejectsAnotherWritersChange() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        let editor = app.textViews["Package README"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("# Native saved text\n")
        app.buttons["Save"].click()
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        XCTAssertEqual(editor.value as? String, "# Native saved text\n")

        editor.click()
        app.typeKey("a", modifierFlags: .command)
        editor.typeText("# Saved after restart\n")
        app.buttons["Save"].click()
        let savedAfterRestart = NSPredicate { _, _ in
            (try? String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8))
                == "# Saved after restart\n"
        }
        expectation(for: savedAfterRestart, evaluatedWith: nil)
        waitForExpectations(timeout: 5)

        try "# Another writer\n".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
        editor.click()
        app.typeKey("a", modifierFlags: .command)
        editor.typeText("# My unsaved text\n")
        app.buttons["Save"].click()
        XCTAssertTrue(app.staticTexts["operationMessage"].waitForExistence(timeout: 5))
        XCTAssertEqual(editor.value as? String, "# My unsaved text\n")
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8), "# Another writer\n")
        app.typeKey("q", modifierFlags: .command)
        app.buttons["Cancel"].click()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
        XCTAssertEqual(editor.value as? String, "# My unsaved text\n")
        app.typeKey("q", modifierFlags: .command)
        app.buttons["Discard Changes"].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testAnUnavailableRememberedPackageOffersRenewal() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        XCTAssertTrue(app.textViews["Package README"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        try FileManager.default.removeItem(at: root)
        app.launchArguments = []
        app.launch()
        let message = app.staticTexts["operationMessage"]
        XCTAssertTrue(message.waitForExistence(timeout: 10))
        XCTAssertTrue(message.label.contains("Package access needs renewal"))
        XCTAssertFalse(app.textViews["Package README"].exists)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testCancellingDirtyQuitDoesNotCancelAnActiveSearch() throws {
        let root = try makeNativeTestPackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        let editor = app.textViews["Package README"]
        editor.click()
        editor.typeText("Unsaved draft during search")
        app.textFields["Spotlight query"].click()
        app.textFields["Spotlight query"].typeText("LifecycleQuote")
        app.buttons["Search"].click()
        try XCTSkipUnless(app.buttons["Stop"].waitForExistence(timeout: 2),
                          "The query settled before the in-flight probe; this route remains unverified.")
        let receipt = try NativeTerminationReceipt(app: app)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 2))
        app.buttons["Cancel"].click()
        try receipt.requireActiveWork()
        XCTAssertEqual(editor.value as? String, "Unsaved draft during search")
        let settled = NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@", "Search complete", "Search timed out")
        expectation(for: settled, evaluatedWith: app.staticTexts["operationMessage"])
        waitForExpectations(timeout: 10)
        app.typeKey("q", modifierFlags: .command)
        app.buttons["Discard Changes"].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testCleanCloseSettlesAnActiveSearch() throws {
        let root = try makeNativeTestPackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        app.textFields["Spotlight query"].click()
        app.textFields["Spotlight query"].typeText("LifecycleQuote")
        app.buttons["Search"].click()
        try XCTSkipUnless(app.buttons["Stop"].waitForExistence(timeout: 2),
                          "The query settled before the in-flight probe; this route remains unverified.")
        let receipt = try NativeTerminationReceipt(app: app)
        app.windows.firstMatch.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        try receipt.requireActiveWork()
    }
}
