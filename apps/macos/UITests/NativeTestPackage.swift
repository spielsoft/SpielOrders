import CSpielOrders
import AppKit
import Foundation
import OSLog
import XCTest

func makeNativeTestPackage(in parent: URL = FileManager.default.temporaryDirectory) throws -> URL {
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

func addNativeTestOrder(in root: URL, title: String = "Lifecycle optics", date: String? = nil) throws {
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
    if let date {
        XCTAssertEqual(date.withCString { spielorders_draft_set(editing, SPIELORDERS_ORDER_FIELD_DATE_ORDERED, $0, nil) }, SPIELORDERS_RESULT_OK)
    }
    XCTAssertEqual(spielorders_draft_plan(try XCTUnwrap(draft), 0, &plan, nil), SPIELORDERS_RESULT_OK)
    XCTAssertEqual(spielorders_plan_commit(try XCTUnwrap(plan), &report, nil), SPIELORDERS_RESULT_OK)
    let directory = URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(try XCTUnwrap(report)),
                        isDirectory: true, relativeTo: nil)
    try "Disposable quote".write(to: directory.appendingPathComponent("LifecycleQuote.txt"), atomically: true, encoding: .utf8)
}

@MainActor func openNativeTestPackage(_ root: URL, in app: XCUIApplication) {
    app.typeKey("o", modifierFlags: .command)
    app.typeKey("g", modifierFlags: [.command, .shift])
    app.typeText(root.path)
    app.typeKey(.return, modifierFlags: [])
    app.buttons["Open Package"].click()
    XCTAssertTrue(app.buttons["Save"].waitForExistence(timeout: 10))
}

@MainActor func packageMenu(in app: XCUIApplication) -> XCUIElement {
    app.toolbars.menuButtons.firstMatch
}

@MainActor func showPackageReadme(in app: XCUIApplication) {
    if !app.textViews["Package README"].exists {
        let destination = app.descendants(matching: .any).matching(identifier: "Package README").firstMatch
        XCTAssertTrue(destination.waitForExistence(timeout: 10))
        destination.click()
    }
    XCTAssertTrue(app.textViews["Package README"].waitForExistence(timeout: 10))
}

@MainActor struct NativeTerminationReceipt {
    private let process: Int32
    private let subsystem: String
    private let since: Date

    init(app: XCUIApplication) throws {
        app.activate()
        guard app.wait(for: .runningForeground, timeout: 5),
              let running = NSWorkspace.shared.frontmostApplication,
              running.localizedName == app.label, let identifier = running.bundleIdentifier else {
            throw XCTSkip("The tested app is not foreground; termination overlap is unverified.")
        }
        process = running.processIdentifier
        subsystem = identifier
        since = Date()
    }

    func requireActiveWork() throws {
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while true {
            do {
                let store = try OSLogStore.local()
                let entries = try store.getEntries(at: store.position(date: since), matching:
                    NSPredicate(format: "processIdentifier == %d AND subsystem == %@ AND category == %@",
                                process, subsystem, "Lifecycle"))
                if let receipt = entries.compactMap({ ($0 as? OSLogEntryLog)?.composedMessage })
                    .last(where: { $0.hasPrefix("termination-request active-work=") }) {
                    try XCTSkipUnless(receipt == "termination-request active-work=1",
                                      "Work settled before AppKit received termination; overlap is unverified.")
                    return
                }
            } catch let skip as XCTSkip { throw skip }
            catch { throw XCTSkip("The tested process's lifecycle receipt is unavailable: \(error)") }
            guard ContinuousClock.now < deadline else {
                throw XCTSkip("No lifecycle receipt from the tested process; overlap is unverified.")
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
    }
}
