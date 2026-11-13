import CSpielOrders
import AppKit
import Foundation
import XCTest

func makePackage(in parent: URL = FileManager.default.temporaryDirectory) throws -> URL {
    try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
    let root = parent.appendingPathComponent("SpielOrdersNative-" + UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    defer { spielorders_plan_free(plan); spielorders_report_free(report) }
    let planned = root.withUnsafeFileSystemRepresentation { spielorders_plan_initialize($0!, nil, nil, 0, &plan, nil) }
    XCTAssertEqual(planned, SPIELORDERS_RESULT_OK)
    XCTAssertEqual(spielorders_plan_commit(try XCTUnwrap(plan), &report, nil), SPIELORDERS_RESULT_OK)
    return root
}

func addOrder(in root: URL, title: String = "Lifecycle optics", date: String? = nil, status: String? = nil,
              orderer: String? = nil, project: String? = nil, group: String? = nil,
              items: [(String, String, String)] = []) throws {
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var draft: UnsafeMutablePointer<SpielordersDraft>?
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    defer {
        spielorders_report_free(report); spielorders_plan_free(plan)
        spielorders_draft_free(draft); spielorders_package_free(package)
    }
    XCTAssertEqual(root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) }, SPIELORDERS_RESULT_OK)
    XCTAssertEqual(spielorders_package_new_order_draft(try XCTUnwrap(package), &draft, nil), SPIELORDERS_RESULT_OK)
    let editing = try XCTUnwrap(draft)
    XCTAssertEqual(title.withCString { spielorders_draft_set(editing, SPIELORDERS_ORDER_FIELD_TITLE, $0, nil) }, SPIELORDERS_RESULT_OK)
    if let status {
        XCTAssertEqual(status.withCString { spielorders_draft_set(editing, SPIELORDERS_ORDER_FIELD_STATUS, $0, nil) }, SPIELORDERS_RESULT_OK)
    }
    let facts = [(SPIELORDERS_ORDER_FIELD_ORDERER, orderer), (SPIELORDERS_ORDER_FIELD_PROJECT, project),
                 (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, group)]
    for case let (field, value?) in facts {
        XCTAssertEqual(value.withCString { spielorders_draft_set(editing, field, $0, nil) }, SPIELORDERS_RESULT_OK)
    }
    if let date {
        XCTAssertEqual(date.withCString { spielorders_draft_set(editing, SPIELORDERS_ORDER_FIELD_DATE_ORDERED, $0, nil) }, SPIELORDERS_RESULT_OK)
    }
    for (row, item) in items.enumerated() {
        XCTAssertEqual(spielorders_draft_row_insert(editing, Int64(row), nil), SPIELORDERS_RESULT_OK)
        for (field, value) in [(SPIELORDERS_ITEM_FIELD_DESCRIPTION, item.0),
                               (SPIELORDERS_ITEM_FIELD_QUANTITY, item.1), (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, item.2)] {
            XCTAssertEqual(value.withCString { spielorders_draft_row_set(editing, Int64(row), field, $0, nil) }, SPIELORDERS_RESULT_OK)
        }
    }
    XCTAssertEqual(spielorders_draft_plan(try XCTUnwrap(draft), 0, &plan, nil), SPIELORDERS_RESULT_OK)
    XCTAssertEqual(spielorders_plan_commit(try XCTUnwrap(plan), &report, nil), SPIELORDERS_RESULT_OK)
    let directory = URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(try XCTUnwrap(report)),
                        isDirectory: true, relativeTo: nil)
    try "Disposable quote".write(to: directory.appendingPathComponent("LifecycleQuote.txt"), atomically: true, encoding: .utf8)
}

@MainActor func openPackage(_ root: URL, in app: XCUIApplication) {
    app.typeKey("o", modifierFlags: .command)
    app.typeKey("g", modifierFlags: [.command, .shift])
    app.typeText(root.path)
    app.typeKey(.return, modifierFlags: [])
    app.buttons["Open Package"].click()
    let window = app.windows["package.window"]
    XCTAssertTrue(window.waitForExistence(timeout: 10))
    let opened = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", root.lastPathComponent), object: window)
    XCTAssertEqual(XCTWaiter.wait(for: [opened], timeout: 10), .completed)
}

@MainActor func packageMenu(in app: XCUIApplication) -> XCUIElement {
    app.menuBars.menuBarItems["Package"]
}

@MainActor func fileMenu(in app: XCUIApplication) -> XCUIElement {
    app.menuBars.menuBarItems["File"]
}

@MainActor func newOrder(in app: XCUIApplication) {
    fileMenu(in: app).click()
    app.menuBars.menuItems["New Order"].click()
}

/// Chooses a destination in the sidebar: `orders`, `vendors`, `orderers`, `institutions`,
/// `projects`, or `packageReadme`.
@MainActor func showDestination(_ name: String, in app: XCUIApplication) {
    let destination = app.descendants(matching: .any).matching(identifier: "navigation." + name).firstMatch
    XCTAssertTrue(destination.waitForExistence(timeout: 10))
    destination.click()
}

@MainActor func showPackageReadme(in app: XCUIApplication) {
    if !app.textViews["Package README"].exists {
        showDestination("packageReadme", in: app)
    }
    XCTAssertTrue(app.textViews["Package README"].waitForExistence(timeout: 10))
}

/// Chooses `title` from the search field's Search In menu.
@MainActor func chooseSearchIn(_ title: String, in app: XCUIApplication) {
    let menu = app.descendants(matching: .any).matching(identifier: "search.scope").firstMatch
    XCTAssertTrue(menu.waitForExistence(timeout: 10))
    menu.click()
    let item = app.menuItems[title]
    XCTAssertTrue(item.waitForExistence(timeout: 5))
    item.click()
}
