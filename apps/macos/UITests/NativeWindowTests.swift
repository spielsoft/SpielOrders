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
        XCTAssertTrue(packageMenu(in: app).waitForExistence(timeout: 10))
        XCTAssertEqual(app.windows.count, 1)
        packageMenu(in: app).click()
        app.menuItems["Open Package…"].click()
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
        XCTAssertTrue(packageMenu(in: app).exists)
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
        showPackageReadme(in: app)
        let editor = app.textViews["Package README"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("# Native saved text\n")
        app.buttons["Save"].click()
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        app.launchArguments = []
        app.launch()
        showPackageReadme(in: app)
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
        showPackageReadme(in: app)
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

    func testCleanCloseSettlesAnActiveSearch() throws {
        let root = try makeNativeTestPackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        app.textFields["Search query"].click()
        app.textFields["Search query"].typeText("LifecycleQuote")
        app.descendants(matching: .any).matching(identifier: "Spotlight").firstMatch.click()
        try XCTSkipUnless(app.buttons["Stop"].waitForExistence(timeout: 2),
                          "The query settled before the in-flight probe; this route remains unverified.")
        let receipt = try NativeTerminationReceipt(app: app)
        app.windows.firstMatch.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        try receipt.requireActiveWork()
    }

    func testLatestLookupAndModeSwitchKeepTheCurrentQuery() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        let query = app.textFields["Search query"]
        XCTAssertTrue(query.waitForExistence(timeout: 10))
        query.click()
        query.typeText("Lifecycle")
        app.typeKey("a", modifierFlags: .command)
        query.typeText("no matching purchase")
        XCTAssertTrue(app.staticTexts["0 of 1 orders shown."].waitForExistence(timeout: 10))
        app.descendants(matching: .any).matching(identifier: "Spotlight").firstMatch.click()
        app.descendants(matching: .any).matching(identifier: "Spotlight").firstMatch.click()
        XCTAssertEqual(query.value as? String, "no matching purchase")
        XCTAssertTrue(app.staticTexts["0 of 1 orders shown."].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "Case sensitive").firstMatch.isEnabled)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testOrderReadmeUndoNavigationCancelAndExplicitSave() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        let first = app.tables["Orders"].tableRows.firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        first.click()
        let edit = app.buttons["Edit README"]
        XCTAssertTrue(edit.waitForExistence(timeout: 10))
        edit.click()
        let editor = app.textViews["Order README"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("**Order notes**")
        app.typeKey("z", modifierFlags: .command)
        XCTAssertEqual(editor.value as? String, "")
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertEqual(editor.value as? String, "**Order notes**")
        app.descendants(matching: .any).matching(identifier: "Vendors").firstMatch.click()
        app.buttons["Cancel"].click()
        XCTAssertEqual(editor.value as? String, "**Order notes**")
        app.typeKey("s", modifierFlags: .command)
        let saved = NSPredicate { _, _ in !app.buttons["Save"].isEnabled }
        expectation(for: saved, evaluatedWith: nil)
        waitForExpectations(timeout: 10)
        app.buttons["Back to Orders"].click()
        XCTAssertTrue(app.staticTexts["**Order notes**"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testTheActiveLibrarySortSurvivesRestart() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root, title: "Newer optics", date: "2026-02-01")
        try addNativeTestOrder(in: root, title: "Older optics", date: "2026-01-01")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-orderSortColumn", "", "-orderSortDescending", "NO"]
        app.launch()
        openNativeTestPackage(root, in: app)
        app.tables["Orders"].buttons["Date"].click()
        let first = app.tables["Orders"].tableRows.firstMatch
        XCTAssertTrue(first.staticTexts["Older optics"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(first.staticTexts["Older optics"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testTabLeavingASettledSpotlightQueryDoesNotSearchAgain() throws {
        let root = try makeNativeTestPackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addNativeTestOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", ""]
        app.launch()
        openNativeTestPackage(root, in: app)
        let query = app.textFields["Search query"]
        query.click()
        query.typeText("LifecycleQuote")
        app.descendants(matching: .any).matching(identifier: "Spotlight").firstMatch.click()
        let message = app.staticTexts["operationMessage"]
        try XCTSkipUnless(message.waitForExistence(timeout: 20) && message.label == "Search complete.",
                          "The real provider did not complete; the focus route remains unverified.")
        query.click()
        app.typeKey(.tab, modifierFlags: [])
        let resubmitted = expectation(for: NSPredicate { _, _ in app.buttons["Stop"].exists }, evaluatedWith: nil)
        resubmitted.isInverted = true
        waitForExpectations(timeout: 1)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testInspectorWidthSurvivesHidingAndRestart() throws {
        let root = try makeNativeTestPackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-inspectorVisible", "YES"]
        app.launch()
        openNativeTestPackage(root, in: app)
        let split = app.splitGroups.firstMatch
        let divider = split.splitters.firstMatch
        XCTAssertTrue(divider.waitForExistence(timeout: 10))
        let start = divider.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 60, dy: 0)))
        let width = split.frame.maxX - divider.frame.maxX
        app.buttons["Inspector"].click()
        app.buttons["Inspector"].click()
        XCTAssertEqual(split.frame.maxX - divider.frame.maxX, width, accuracy: 2)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(divider.waitForExistence(timeout: 10))
        XCTAssertEqual(split.frame.maxX - divider.frame.maxX, width, accuracy: 2)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }
}
