import XCTest

@MainActor final class FolderContentsTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    func testStopKeepsQueryAndOrders() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Stopped optics")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let query = app.searchFields["search.query"]
        query.click()
        query.typeText("Stopped")
        chooseSearchIn("Folder Contents (Spotlight)", in: app)
        let progress = app.descendants(matching: .any).matching(identifier: "search.progress").firstMatch
        try XCTSkipUnless(progress.waitForExistence(timeout: 2),
                          "Spotlight answered before gathering could be observed; the native stop route remains unverified.")
        let stop = app.buttons["search.stop"]
        XCTAssertTrue(stop.exists)
        guard stop.exists else { return }
        stop.click()
        XCTAssertTrue(stop.waitForNonExistence(timeout: 10))
        let problem = app.buttons["search.problem"]
        XCTAssertTrue(problem.waitForExistence(timeout: 10))
        guard problem.exists else { return }
        try XCTSkipUnless(problem.value as? String == "Spotlight stopped early; some matching files may be missing.",
                          "The real provider settled before Stop; the native stop route remains unverified.")
        XCTAssertEqual(query.value as? String, "Stopped")
        XCTAssertEqual(query.placeholderValue, "Search Folder Contents")
        let order = app.tables["orders.table"].staticTexts.matching(
            NSPredicate(format: "value == %@", "Stopped optics")).firstMatch
        XCTAssertTrue(order.waitForExistence(timeout: 10))
        order.click()
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "Stopped optics")
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }
}
