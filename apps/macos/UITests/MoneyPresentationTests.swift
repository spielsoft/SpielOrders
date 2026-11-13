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

    func testStagedPricesKeepTheirPrecisionAndCurrency() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        newOrder(in: app)
        let currency = element("order.editor.currency", in: app)
        XCTAssertTrue(currency.waitForExistence(timeout: 10))
        replace(currency, with: "USD", in: app)
        element("order.editor.items.add", in: app).click()
        let precise = "1.23456789012345678901234567890123456789123456789"
        app.typeText("CAT-1\tPrecision sample\t1\t" + precise + "\t")
        currency.click()
        let price = element("order.editor.items.row.0.unit_price", in: app)
        expect(price, displays: literal(precise))
        price.doubleClick()
        XCTAssertEqual(price.value as? String, precise)

        app.typeKey("a", modifierFlags: .command)
        app.typeText("1e-7\t")
        currency.click()
        let style = currencyStyle()
        style.minimumFractionDigits = 2
        style.maximumFractionDigits = 7
        expect(price, displays: try XCTUnwrap(style.string(from: NSDecimalNumber(string: "0.0000001"))))
        price.doubleClick()
        XCTAssertEqual(price.value as? String, "1e-7")

        app.typeKey("a", modifierFlags: .command)
        app.typeText("-1e-200\t")
        currency.click()
        expect(price, displays: literal("-1e-200"))
        price.doubleClick()
        XCTAssertEqual(price.value as? String, "-1e-200")
        // This remains an uncreated draft; display does not write a rounded replacement.
        XCTAssertTrue(element("editor.create", in: app).exists)
    }

    private func currencyStyle() -> NumberFormatter {
        let style = NumberFormatter()
        style.locale = .current
        style.numberStyle = .currency
        style.currencyCode = "USD"
        return style
    }

    private func literal(_ amount: String) -> String {
        let style = currencyStyle()
        if amount.hasPrefix("-") {
            return style.negativePrefix + amount.dropFirst() + style.negativeSuffix
        }
        return style.positivePrefix + amount + style.positiveSuffix
    }

    private func element(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    private func replace(_ field: XCUIElement, with text: String, in app: XCUIApplication) {
        field.click()
        app.typeKey("a", modifierFlags: .command)
        field.typeText(text)
    }

    private func expect(_ field: XCUIElement, displays value: String) {
        let displayed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: field)
        XCTAssertEqual(XCTWaiter.wait(for: [displayed], timeout: 10), .completed)
    }
}
