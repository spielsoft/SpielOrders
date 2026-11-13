import AppKit
import XCTest

@MainActor final class ProblemPresentationTests: XCTestCase {
    override func tearDown() async throws {
        let app = XCUIApplication()
        if app.state != .notRunning { app.terminate() }
        try await super.tearDown()
    }

    func testWarningSurvivesRefusal() throws {
        let root = try makePackage()
        let metadata = root.appendingPathComponent(".spielorders")
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: metadata.path)
            try? FileManager.default.removeItem(at: root)
        }
        try addOrder(in: root)
        let directory = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: root.appendingPathComponent("Orders"), includingPropertiesForKeys: nil).first)
        let path = directory.appendingPathComponent("README.md")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let row = app.tables["orders.table"].staticTexts.matching(
            NSPredicate(format: "value == %@", "Lifecycle optics")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        let readme = app.textViews["Order README"]
        XCTAssertTrue(readme.waitForExistence(timeout: 10))
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: metadata.path)
        readme.click()
        readme.typeText("Committed notes")
        let warning = app.buttons["readme.savedWarnings"]
        XCTAssertTrue(warning.waitForExistence(timeout: 10))
        XCTAssertEqual(try String(contentsOf: path, encoding: .utf8), "Committed notes")

        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: metadata.path)
        try "Another writer".write(to: path, atomically: true, encoding: .utf8)
        readme.click()
        app.typeKey("a", modifierFlags: .command)
        readme.typeText("Later unsaved notes")
        XCTAssertTrue(app.buttons["readme.notSaved"].waitForExistence(timeout: 10))
        XCTAssertTrue(warning.exists, "An unwritten edit must not hide the preceding committed warning")
        XCTAssertEqual(readme.value as? String, "Later unsaved notes")
        XCTAssertEqual(try String(contentsOf: path, encoding: .utf8), "Another writer")
        warning.click()
        app.buttons["readme.warnings.details"].click()
        XCTAssertTrue(app.staticTexts["feedback.detail.0"].waitForExistence(timeout: 10))
    }
}
