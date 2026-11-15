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

    func testPackagePicker() {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        XCTAssertTrue(packageMenu(in: app).waitForExistence(timeout: 10))
        XCTAssertEqual(app.windows.count, 1)
        fileMenu(in: app).click()
        app.menuBars.menuItems["Open Package…"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForExistence(timeout: 5))
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testClose() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        app.menuBars.menuBarItems["File"].click()
        let close = app.menuBars.menuItems["Close Window"]
        XCTAssertTrue(close.exists)
        guard close.exists else { return }
        app.typeKey(.escape, modifierFlags: [])

        app.menuBars.menuBarItems["SpielOrders"].click()
        app.menuBars.menuItems["About SpielOrders"].click()
        let about = app.windows["About SpielOrders"]
        XCTAssertTrue(about.waitForExistence(timeout: 5))
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(about.waitForNonExistence(timeout: 5))
        XCTAssertTrue(packageMenu(in: app).exists)

        newOrder(in: app)
        let title = app.textFields["order.editor.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.click()
        app.typeKey("a", modifierFlags: .command)
        title.typeText("Unsaved Close draft")
        app.typeKey("w", modifierFlags: .command)
        let cancel = app.buttons["draft.unsaved.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertEqual(title.value as? String, "Unsaved Close draft")
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("Orders").path)
            .filter { !$0.hasPrefix(".") }, [])
        app.menuBars.menuBarItems["File"].click()
        close.click()
        let discard = app.buttons["draft.unsaved.confirm"]
        XCTAssertTrue(discard.waitForExistence(timeout: 5))
        discard.click()
        XCTAssertTrue(app.buttons["welcome.open"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.windows["package.window"].label, "SpielOrders")
        XCTAssertNotEqual(app.state, .notRunning)
        XCTAssertFalse(app.textFields["order.editor.title"].exists)
        app.menuBars.menuBarItems["SpielOrders"].click()
        app.menuBars.menuItems["About SpielOrders"].click()
        let reopenedAbout = app.windows["About SpielOrders"]
        XCTAssertTrue(reopenedAbout.waitForExistence(timeout: 5))
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(reopenedAbout.waitForNonExistence(timeout: 5))
        XCTAssertNotEqual(app.state, .notRunning)
        XCTAssertTrue(app.buttons["welcome.open"].exists)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.buttons["welcome.open"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.tables["orders.table"].exists)
        app.typeKey("o", modifierFlags: .command)
        XCTAssertTrue(app.sheets.firstMatch.waitForExistence(timeout: 5))
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    func testCommandScope() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Existing optics")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let row = app.tables["orders.table"].staticTexts.matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@ AND value == %@",
            "orders.row.", ".title", "Existing optics")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        XCTAssertTrue(app.textViews["order.readme.editor"].waitForExistence(timeout: 10))
        app.menuBars.menuBarItems["Order"].click()
        XCTAssertTrue(app.menuBars.menuBarItems["Order"].menuItems["Remove Order"].isEnabled)
        app.typeKey(.escape, modifierFlags: [])

        newOrder(in: app)
        XCTAssertTrue(app.textFields["order.editor.title"].waitForExistence(timeout: 10))
        assertOrderCommandsUnavailable(in: app)
        app.buttons["editor.cancel"].click()

        packageMenu(in: app).click()
        app.menuItems["Diagnose Package"].click()
        XCTAssertTrue(app.buttons["diagnosis.back"].waitForExistence(timeout: 10))
        assertOrderCommandsUnavailable(in: app)
        showDestination("orders", in: app)
        XCTAssertTrue(app.buttons["diagnosis.back"].waitForNonExistence(timeout: 10))

        showDestination("vendors", in: app)
        XCTAssertTrue(app.buttons["vendor.new"].waitForExistence(timeout: 10))
        assertOrderCommandsUnavailable(in: app)
    }

    func testDestinationSidebar() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        let destinations = app.descendants(matching: .any).matching(identifier: "navigation.destinations").firstMatch
        XCTAssertTrue(app.buttons["welcome.open"].waitForExistence(timeout: 10))
        XCTAssertFalse(destinations.exists)
        openPackage(root, in: app)
        XCTAssertTrue(destinations.waitForExistence(timeout: 10))

        app.typeKey("2", modifierFlags: .command)
        XCTAssertTrue(app.buttons["vendor.new"].waitForExistence(timeout: 10))
        app.typeKey("s", modifierFlags: [.control, .command])
        XCTAssertTrue(destinations.waitForNonExistence(timeout: 10))
        XCTAssertTrue(app.buttons["vendor.new"].exists)
        app.typeKey("1", modifierFlags: .command)
        XCTAssertTrue(app.buttons["order.new"].waitForExistence(timeout: 10))
        app.typeKey("s", modifierFlags: [.control, .command])
        XCTAssertTrue(destinations.waitForExistence(timeout: 10))

        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(app.buttons["welcome.open"].waitForExistence(timeout: 10))
        XCTAssertFalse(destinations.exists)
    }

    func testValueDestinations() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Lenses", orderer: "Ada Lovelace")
        try addOrder(in: root, title: "Mounts", orderer: "Ada Lovelace")
        try addOrder(in: root, title: "Probes", orderer: "Grace Hopper")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        showDestination("orderers", in: app)
        let ada = app.descendants(matching: .any).matching(identifier: "orderers.row.Ada Lovelace.orders").firstMatch
        XCTAssertTrue(ada.waitForExistence(timeout: 10))
        XCTAssertEqual(ada.value as? String, "2")
        let grace = app.descendants(matching: .any).matching(identifier: "orderers.row.Grace Hopper.orders").firstMatch
        XCTAssertEqual(grace.value as? String, "1")
        // The library offers no search over values, so the view has no search field.
        XCTAssertFalse(app.descendants(matching: .any).matching(identifier: "search.query").firstMatch.exists)
        app.typeKey("4", modifierFlags: .command)
        let institutions = app.descendants(matching: .any).matching(identifier: "institutions.table").firstMatch
        XCTAssertTrue(institutions.waitForExistence(timeout: 10))
        XCTAssertFalse(ada.exists)
    }

    func testProjectsDestination() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Coils", project: "Greenhouse", group: "Misting")
        try addOrder(in: root, title: "Lenses", project: "Greenhouse", group: "Misting")
        try addOrder(in: root, title: "Mirrors", project: "Optics", group: "Misting")
        try addOrder(in: root, title: "Cables", group: "Spares")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        app.typeKey("5", modifierFlags: .command)
        XCTAssertTrue(projectCell("Greenhouse", in: app).waitForExistence(timeout: 10))
        // The same group name under two Projects is two groups, each with its own Orders.
        XCTAssertEqual(projectCell("Greenhouse", in: app).value as? String, "2")
        XCTAssertEqual(projectCell("Greenhouse/Misting", in: app).value as? String, "2")
        XCTAssertEqual(projectCell("Optics/Misting", in: app).value as? String, "1")
        XCTAssertEqual(projectCell("No Project/Spares", in: app).value as? String, "1")
        app.typeKey("6", modifierFlags: .command)
        XCTAssertTrue(app.textViews["Package README"].waitForExistence(timeout: 10))
    }

    private func projectCell(_ key: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: "projects.row.\(key).orders").firstMatch
    }

    func testDamagedPackage() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("{broken".utf8).write(to: root.appendingPathComponent(".spielorders/vendors.json"))
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        XCTAssertTrue(app.buttons["diagnosis.refresh"].waitForExistence(timeout: 10))
        let destinations = app.descendants(matching: .any).matching(identifier: "navigation.destinations").firstMatch
        XCTAssertTrue(destinations.exists)
        XCTAssertFalse(destinations.isEnabled)
        fileMenu(in: app).click()
        XCTAssertFalse(app.menuBars.menuItems["New Order"].isEnabled)
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertFalse(app.buttons["diagnosis.back"].isEnabled)
    }

    private func assertOrderCommandsUnavailable(in app: XCUIApplication) {
        app.menuBars.menuBarItems["Order"].click()
        for command in ["Open", "Show in Finder", "Align Directory Name…", "Diagnose Order", "Remove Order", "Move to Trash…"] {
            let item = app.menuBars.menuBarItems["Order"].menuItems[command]
            XCTAssertTrue(item.exists)
            XCTAssertFalse(item.isEnabled)
        }
        app.typeKey(.escape, modifierFlags: [])
    }

    func testOrderIdentity() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Shared title", date: "2020-01-01")
        try addOrder(in: root, title: "Shared title", date: "2026-01-01")
        var package: UnsafeMutablePointer<SpielordersPackage>?
        var list: UnsafeMutablePointer<SpielordersList>?
        defer { spielorders_list_free(list); spielorders_package_free(package) }
        XCTAssertEqual(root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) }, SPIELORDERS_RESULT_OK)
        XCTAssertEqual(spielorders_package_orders(try XCTUnwrap(package), "", 0, SPIELORDERS_ORDER_FIELD_DATE_ORDERED,
                                                 0, 0, &list, nil), SPIELORDERS_RESULT_OK)
        let orders = try XCTUnwrap(list)
        let id = String(cString: spielorders_list_field(orders, 0, SPIELORDERS_ORDER_FIELD_ORDER_ID))
        let date = String(cString: spielorders_list_field(orders, 0, SPIELORDERS_ORDER_FIELD_DATE_ORDERED))
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-orderSortColumn", "date", "-orderSortDescending", "NO"]
        app.launch()
        openPackage(root, in: app)
        let table = app.tables["orders.table"]
        XCTAssertTrue(table.waitForExistence(timeout: 10))
        let dates = table.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@", "orders.row.", ".date"))
        let before = dates.allElementsBoundByIndex.map(\.identifier)
        XCTAssertEqual(before.count, 2)
        app.buttons["orders.column.date"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).click()
        let after = dates.allElementsBoundByIndex.map(\.identifier)
        XCTAssertEqual(after, Array(before.reversed()))
        table.staticTexts["orders.row.order:" + id + ".title"].click()
        let field = app.descendants(matching: .any).matching(identifier: "order.editor.date_ordered").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        let stored = try XCTUnwrap(Date.ISO8601FormatStyle(timeZone: .current).year().month().day().parse(date))
        XCTAssertEqual(table.staticTexts["orders.row.order:" + id + ".date"].value as? String,
                       DateFormatter.localizedString(from: stored, dateStyle: .short, timeStyle: .none))
        let editor = app.textViews["order.readme.editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("Notes saved by Quit")
        // Quit finishes the pending README save without asking.
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
        let readmes = try FileManager.default.subpathsOfDirectory(atPath: root.path)
            .filter { $0.hasSuffix("/README.md") }
            .map { try String(contentsOf: root.appendingPathComponent($0), encoding: .utf8) }
        XCTAssertTrue(readmes.contains("Notes saved by Quit"))
    }

    func testAbout() {
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        app.menuBars.menuBarItems["SpielOrders"].click()
        app.menuBars.menuItems["About SpielOrders"].click()
        let about = app.windows["About SpielOrders"]
        XCTAssertTrue(about.waitForExistence(timeout: 5))
        XCTAssertTrue(about.links["Source code"].exists)
        let version = about.staticTexts.matching(NSPredicate(format: "value BEGINSWITH %@", "Version ")).firstMatch
        XCTAssertTrue(version.exists)
        XCTAssertTrue((version.value as? String)?.range(of: #"Version [0-9.]+ \([0-9]+\)"#, options: .regularExpression) != nil)
        XCTAssertTrue(about.staticTexts.matching(NSPredicate(format: "value CONTAINS %@", "A SpielSoft product")).firstMatch.exists)
        about.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(packageMenu(in: app).exists)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testReadmePersistence() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        showPackageReadme(in: app)
        let editor = app.textViews["Package README"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("# Native saved text\n")
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
        // Typing pauses, and the README saves on its own.
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
        XCTAssertTrue(app.buttons["readme.notSaved"].waitForExistence(timeout: 5))
        XCTAssertEqual(editor.value as? String, "# My unsaved text\n")
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8), "# Another writer\n")
        // A refused save blocks Quit, as a refused record does.
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 5))
        XCTAssertEqual(editor.value as? String, "# My unsaved text\n")
        app.buttons["readme.notSaved"].click()
        app.buttons["readme.revert"].click()
        app.buttons["readme.revert.confirm"].click()
        let reverted = NSPredicate { _, _ in editor.value as? String == "# Another writer\n" }
        expectation(for: reverted, evaluatedWith: nil)
        waitForExpectations(timeout: 5)
        XCTAssertFalse(app.buttons["readme.notSaved"].exists)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testReadmeIsolation() throws {
        let first = try makePackage()
        let second = try makePackage()
        defer {
            try? FileManager.default.removeItem(at: first)
            try? FileManager.default.removeItem(at: second)
        }
        let template = "# Shared README\n"
        for root in [first, second] {
            try template.write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
        }
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(first, in: app)
        showPackageReadme(in: app)
        let editor = app.textViews["Package README"]
        for reopen in [false, true] {
            editor.click()
            app.typeKey("a", modifierFlags: .command)
            editor.typeText("# Private draft\n")
            app.typeKey("a", modifierFlags: .command)
            editor.typeText(template)
            if reopen {
                fileMenu(in: app).click()
                app.menuItems["Reload Package"].click()
            } else {
                openPackage(second, in: app)
            }
            editor.click()
            app.typeKey("z", modifierFlags: .command)
            XCTAssertEqual(editor.value as? String, template)
        }
        for root in [first, second] {
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("README.md"), encoding: .utf8), template)
        }
    }

    func testBookmarkRenewal() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }

        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        showPackageReadme(in: app)
        XCTAssertTrue(app.textViews["Package README"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        try FileManager.default.removeItem(at: root)
        app.launchArguments = []
        app.launch()
        let alert = app.descendants(matching: .any).matching(identifier: "package.access.result").firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Package access needs renewal"].exists)
        XCTAssertFalse(app.textViews["Package README"].exists)
        app.buttons["package.access.dismiss"].click()
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testSearchSettlement() throws {
        let root = try makePackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        chooseSearchIn("Folder Contents (Spotlight)", in: app)
        app.searchFields["search.query"].click()
        app.searchFields["search.query"].typeText("LifecycleQuote")
        let progress = app.descendants(matching: .any).matching(identifier: "search.progress").firstMatch
        try XCTSkipUnless(progress.waitForExistence(timeout: 2),
                          "Spotlight answered before the in-flight probe; this route remains unverified.")
        app.windows.firstMatch.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(app.buttons["welcome.open"].waitForExistence(timeout: 5))
        XCTAssertFalse(progress.exists)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testQuerySwitching() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let query = app.searchFields["search.query"]
        XCTAssertTrue(query.waitForExistence(timeout: 10))
        query.click()
        query.typeText("Lifecycle")
        app.typeKey("a", modifierFlags: .command)
        query.typeText("no matching purchase")
        XCTAssertTrue(app.staticTexts["0 of 1 Order"].waitForExistence(timeout: 10))
        chooseSearchIn("Folder Contents (Spotlight)", in: app)
        XCTAssertEqual(query.placeholderValue, "Search Folder Contents")
        chooseSearchIn("All Fields", in: app)
        XCTAssertEqual(query.value as? String, "no matching purchase")
        XCTAssertTrue(app.staticTexts["0 of 1 Order"].waitForExistence(timeout: 10))
        chooseSearchIn("Order Title", in: app)
        app.typeKey("a", modifierFlags: .command)
        query.typeText("Lifecycle")
        XCTAssertTrue(app.staticTexts["1 of 1 Order"].waitForExistence(timeout: 10))
        chooseSearchIn("Match Case", in: app)
        app.typeKey("a", modifierFlags: .command)
        query.typeText("lifecycle")
        XCTAssertTrue(app.staticTexts["0 of 1 Order"].waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testReadmeNavigation() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let first = app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "label == %@ AND value == %@", "Order Title", "Lifecycle optics")).firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        first.click()
        let editor = app.textViews["Order README"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        editor.typeText("**Order notes**")
        app.typeKey("z", modifierFlags: .command)
        XCTAssertEqual(editor.value as? String, "")
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertEqual(editor.value as? String, "**Order notes**")
        // Leaving the Order saves its README without asking.
        showDestination("vendors", in: app)
        XCTAssertTrue(app.buttons["vendor.new"].waitForExistence(timeout: 10))
        showDestination("orders", in: app)
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        first.click()
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        XCTAssertEqual(editor.value as? String, "**Order notes**")
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testSortPersistence() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Newer optics", date: "2026-02-01")
        try addOrder(in: root, title: "Older optics", date: "2026-01-01")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-orderSortColumn", "", "-orderSortDescending", "NO"]
        app.launch()
        openPackage(root, in: app)
        app.tables["orders.table"].buttons["orders.column.date"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).click()
        let first = app.tables["orders.table"].tableRows.firstMatch
        XCTAssertTrue(first.staticTexts.matching(NSPredicate(format: "value == %@", "Older optics")).firstMatch.waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(first.staticTexts.matching(NSPredicate(format: "value == %@", "Older optics")).firstMatch.waitForExistence(timeout: 10))
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testSearchFocus() throws {
        let root = try makePackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        chooseSearchIn("Folder Contents (Spotlight)", in: app)
        let query = app.searchFields["search.query"]
        query.click()
        query.typeText("LifecycleQuote")
        let progress = app.descendants(matching: .any).matching(identifier: "search.progress").firstMatch
        try XCTSkipUnless(progress.waitForExistence(timeout: 2) && progress.waitForNonExistence(timeout: 20),
                          "The real provider did not answer; the focus route remains unverified.")
        query.click()
        app.typeKey(.tab, modifierFlags: [])
        let resubmitted = expectation(for: NSPredicate { _, _ in progress.exists }, evaluatedWith: nil)
        resubmitted.isInverted = true
        waitForExpectations(timeout: 1)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testMaterialActions() throws {
        let root = try makePackage(in: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0])
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        chooseSearchIn("Folder Contents (Spotlight)", in: app)
        app.searchFields["search.query"].click()
        app.searchFields["search.query"].typeText("LifecycleQuote")
        let order = app.tables["orders.table"].staticTexts.matching(
            NSPredicate(format: "label == %@ AND value == %@", "Order Title", "Lifecycle optics")).firstMatch
        try XCTSkipUnless(order.waitForExistence(timeout: 20),
                          "The real provider has not indexed the file; matched-file actions remain unverified.")
        order.click()
        let open = app.buttons["Open LifecycleQuote.txt"]
        try XCTSkipUnless(open.waitForExistence(timeout: 20),
                          "The real provider has not indexed the file; matched-file actions remain unverified.")
        XCTAssertTrue(app.buttons["Show LifecycleQuote.txt in Finder"].exists)
    }

    func testRemovalUndo() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root)
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let order = app.tables["orders.table"].staticTexts.matching(
            NSPredicate(format: "label == %@ AND value == %@", "Order Title", "Lifecycle optics")).firstMatch
        XCTAssertTrue(order.waitForExistence(timeout: 10))
        let orders = root.appendingPathComponent("Orders")
        func directories() throws -> [URL] {
            try FileManager.default.contentsOfDirectory(at: orders, includingPropertiesForKeys: nil)
                .filter { !$0.lastPathComponent.hasPrefix(".") }
        }

        // Remove Order commits without a review and Undo restores the same folder.
        order.rightClick()
        app.tables["orders.table"].menuItems["Remove Order"].click()
        XCTAssertTrue(order.waitForNonExistence(timeout: 10))
        XCTAssertFalse(app.sheets.buttons["order.directory.confirmation.cancel"].exists)
        XCTAssertEqual(try directories(), [])

        // Reload retains removal recovery, including after a restore asked for fresh Package evidence.
        app.menuBars.menuBarItems["File"].click()
        app.menuItems["Reload Package"].click()
        XCTAssertTrue(app.tables["orders.table"].waitForExistence(timeout: 10))
        let undo = app.menuBars.menuItems["Undo Remove Order"]
        app.menuBars.menuBarItems["Edit"].click()
        XCTAssertTrue(undo.waitForExistence(timeout: 5))
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: undo)
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed)
        undo.click()
        XCTAssertTrue(order.waitForExistence(timeout: 10))
        XCTAssertEqual(try directories().count, 1)
        XCTAssertEqual(try String(contentsOf: directories()[0].appendingPathComponent("LifecycleQuote.txt"),
                                  encoding: .utf8), "Disposable quote")

        // Redo removes it again; Move to Trash still asks first.
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(order.waitForNonExistence(timeout: 10))
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(order.waitForExistence(timeout: 10))
        order.rightClick()
        app.tables["orders.table"].menuItems["Move to Trash…"].click()
        let cancel = app.sheets.buttons["order.directory.confirmation.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        cancel.click()
        XCTAssertTrue(order.waitForExistence(timeout: 10))
        XCTAssertEqual(try directories().count, 1)
        // A cancelled review changes nothing, so Redo Remove Order stays.
        app.menuBars.menuBarItems["Edit"].click()
        XCTAssertTrue(app.menuBars.menuItems["Redo Remove Order"].isEnabled)
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }

    func testClearSearch() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Optics")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let row = app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Optics")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        let search = app.searchFields["search.query"]
        search.click()
        search.typeText("no matching order")
        XCTAssertTrue(row.waitForNonExistence(timeout: 10))
        search.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(row.waitForExistence(timeout: 10))
    }

    func testSidebarPersistence() throws {
        let root = try makePackage()
        defer { try? FileManager.default.removeItem(at: root) }
        try addOrder(in: root, title: "Persistent detail")
        let app = XCUIApplication()
        app.launchArguments = ["-packageBookmark", "", "-sidebarVisible", "YES"]
        app.launch()
        openPackage(root, in: app)
        let split = app.splitGroups["workspace.split"]
        let divider = split.splitters["list.width"]
        XCTAssertTrue(divider.waitForExistence(timeout: 10))
        let start = divider.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 60, dy: 0)))
        let width = divider.frame.minX - split.frame.minX
        let title = app.tables["orders.table"].staticTexts.matching(NSPredicate(format: "value == %@", "Persistent detail")).firstMatch
        title.click()
        let detail = app.descendants(matching: .any).matching(identifier: "order.editor.title").firstMatch
        XCTAssertTrue(detail.waitForExistence(timeout: 10))
        app.buttons["Hide Sidebar"].click()
        XCTAssertTrue(detail.exists)
        app.buttons["Show Sidebar"].click()
        XCTAssertEqual(divider.frame.minX - split.frame.minX, width, accuracy: 2)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(divider.waitForExistence(timeout: 10))
        XCTAssertEqual(divider.frame.minX - split.frame.minX, width, accuracy: 2)
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }
}
