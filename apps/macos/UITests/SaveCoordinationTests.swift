import AppKit
import XCTest

@MainActor final class SaveCoordinationTests: XCTestCase {
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

    func testCreatedOrderReadme() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = launch(root)
        newOrder(in: app)
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.click()
        app.typeKey("a", modifierFlags: .command)
        title.typeText("New optics")
        app.buttons["editor.create"].click()
        let readme = app.textViews["Order README"]
        XCTAssertTrue(readme.waitForExistence(timeout: 10))
        readme.click()
        readme.typeText("# Notes on the new Order\n")
        // Leaving the README starts its save; the following ended field saves too.
        title.click()
        app.typeKey("a", modifierFlags: .command)
        app.typeText("Renamed optics\t")
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard let directory = try? FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("Orders"),
                    includingPropertiesForKeys: nil).first(where: { !$0.lastPathComponent.hasPrefix(".") }),
                  let bytes = try? Data(contentsOf: directory.appendingPathComponent(".spielorders/order.json")),
                  let record = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { return false }
            return record["title"] as? String == "Renamed optics"
                && (try? String(contentsOf: directory.appendingPathComponent("README.md"), encoding: .utf8))
                    == "# Notes on the new Order\n"
        }, object: nil)
        wait(for: [saved], timeout: 10)
        app.typeKey("z", modifierFlags: .command)
        let undone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "New optics"), object: title)
        wait(for: [undone], timeout: 10)
    }

    func testReadmeRetryIsExplicit() throws {
        let root = try makePackage()
        let orders = root.appendingPathComponent("Orders")
        var directory: URL?
        defer {
            if let directory { try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: directory.path) }
            try? FileManager.default.removeItem(at: root)
        }
        try addOrder(in: root)
        directory = try XCTUnwrap(FileManager.default.contentsOfDirectory(at: orders, includingPropertiesForKeys: nil)
            .first { !$0.lastPathComponent.hasPrefix(".") })
        let folder = try XCTUnwrap(directory)
        let path = folder.appendingPathComponent("README.md")
        try "Saved notes".write(to: path, atomically: true, encoding: .utf8)
        let app = launch(root)
        app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Lifecycle optics")).firstMatch.click()
        let readme = app.textViews["Order README"]
        XCTAssertTrue(readme.waitForExistence(timeout: 10))
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: folder.path)
        readme.click()
        app.typeKey("a", modifierFlags: .command)
        readme.typeText("Retained notes")
        XCTAssertTrue(app.buttons["readme.notSaved"].waitForExistence(timeout: 10))
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: folder.path)
        let premature = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            (try? String(contentsOf: path, encoding: .utf8)) == "Retained notes"
        }, object: nil)
        premature.isInverted = true
        wait(for: [premature], timeout: 3)
        XCTAssertEqual(readme.value as? String, "Retained notes")
        app.buttons["readme.notSaved"].click()
        app.buttons["readme.retry"].click()
        let saved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            (try? String(contentsOf: path, encoding: .utf8)) == "Retained notes"
        }, object: nil)
        wait(for: [saved], timeout: 10)
        XCTAssertTrue(app.buttons["readme.notSaved"].waitForNonExistence(timeout: 10))
    }
}
