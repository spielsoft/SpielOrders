import AppKit
import XCTest

// Decision source: Prototype the SpielOrders desktop experience, Selection, removal, and Finder.
@MainActor final class MoneyPresentationTests: XCTestCase {
    override func tearDown() async throws {
        await MainActor.run {
            let app = XCUIApplication()
            if app.state != .notRunning { app.terminate() }
        }
        try await super.tearDown()
    }

    func testPricesShowTwoDecimalsAndEditAsTyped() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        newOrder(in: app)
        let currency = element("order.editor.currency", in: app)
        XCTAssertTrue(currency.waitForExistence(timeout: 10))
        currency.click()
        app.typeKey("a", modifierFlags: .command)
        currency.typeText("USD")
        element("order.editor.items.add", in: app).click()
        app.typeText("CAT-1\tPrecision sample\t1\t1.23456\t")
        currency.click()

        // An ISO currency shows the amount rounded to two decimals in the system's currency style.
        let price = element("order.editor.items.row.0.unit_price", in: app)
        let rounded = Decimal(string: "1.23")!.formatted(.currency(code: "USD").precision(.fractionLength(2)))
        let displayed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", rounded), object: price)
        XCTAssertEqual(XCTWaiter.wait(for: [displayed], timeout: 10), .completed)

        // Editing the cell shows the amount exactly as typed.
        price.doubleClick()
        XCTAssertEqual(price.value as? String, "1.23456")
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }
}
