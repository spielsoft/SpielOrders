import CSpielOrders
import Foundation
import SpielOrdersKit
import Testing

func initializedPackage() throws -> URL {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    let planned = root.withUnsafeFileSystemRepresentation {
        spielorders_plan_initialize($0!, nil, nil, 0, &plan, nil)
    }
    defer { spielorders_plan_free(plan); spielorders_report_free(report) }
    #expect(planned == SPIELORDERS_RESULT_OK)
    #expect(spielorders_plan_commit(try #require(plan), &report, nil) == SPIELORDERS_RESULT_OK)
    return root
}

func createOrder(in root: URL, title: String = "Optics", itemPrice: String? = nil, vendor: String? = nil) throws -> URL {
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var draft: UnsafeMutablePointer<SpielordersDraft>?
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    defer {
        spielorders_report_free(report)
        spielorders_plan_free(plan)
        spielorders_draft_free(draft)
        spielorders_package_free(package)
    }
    let opened = root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) }
    #expect(opened == SPIELORDERS_RESULT_OK)
    #expect(spielorders_package_new_order_draft(try #require(package), &draft, nil) == SPIELORDERS_RESULT_OK)
    let set = title.withCString { spielorders_draft_set(draft!, SPIELORDERS_ORDER_FIELD_TITLE, $0, nil) }
    #expect(set == SPIELORDERS_RESULT_OK)
    if let vendor {
        #expect(vendor.withCString { spielorders_draft_set(draft!, SPIELORDERS_ORDER_FIELD_VENDOR, $0, nil) } == SPIELORDERS_RESULT_OK)
    }
    if let itemPrice {
        #expect(spielorders_draft_row_insert(draft!, 0, nil) == SPIELORDERS_RESULT_OK)
        for (field, value) in [(SPIELORDERS_ITEM_FIELD_DESCRIPTION, "Part"),
                               (SPIELORDERS_ITEM_FIELD_QUANTITY, "1"), (SPIELORDERS_ITEM_FIELD_UNIT_PRICE, itemPrice)] {
            #expect(value.withCString { spielorders_draft_row_set(draft!, 0, field, $0, nil) } == SPIELORDERS_RESULT_OK)
        }
    }
    #expect(spielorders_draft_plan(try #require(draft), 0, &plan, nil) == SPIELORDERS_RESULT_OK)
    #expect(spielorders_plan_commit(try #require(plan), &report, nil) == SPIELORDERS_RESULT_OK)
    return URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(try #require(report)),
               isDirectory: true, relativeTo: nil)
}

func createVendor(in root: URL, name: String, contactEmail: String? = nil) throws -> String {
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var draft: UnsafeMutablePointer<SpielordersDraft>?
    var plan: UnsafeMutablePointer<SpielordersPlan>?
    var report: UnsafeMutablePointer<SpielordersReport>?
    defer {
        spielorders_report_free(report); spielorders_plan_free(plan); spielorders_draft_free(draft)
        spielorders_package_free(package)
    }
    #expect(root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) } == SPIELORDERS_RESULT_OK)
    #expect(spielorders_package_new_vendor_draft(try #require(package), &draft, nil) == SPIELORDERS_RESULT_OK)
    #expect(name.withCString { spielorders_draft_set(draft!, SPIELORDERS_VENDOR_FIELD_NAME, $0, nil) } == SPIELORDERS_RESULT_OK)
    if let contactEmail {
        #expect(spielorders_draft_row_insert(draft!, 0, nil) == SPIELORDERS_RESULT_OK)
        #expect(spielorders_draft_row_set(draft!, 0, SPIELORDERS_CONTACT_FIELD_NAME, "Sales", nil) == SPIELORDERS_RESULT_OK)
        #expect(contactEmail.withCString {
            spielorders_draft_row_set(draft!, 0, SPIELORDERS_CONTACT_FIELD_EMAIL, $0, nil)
        } == SPIELORDERS_RESULT_OK)
    }
    #expect(spielorders_draft_plan(draft!, 0, &plan, nil) == SPIELORDERS_RESULT_OK)
    #expect(spielorders_plan_commit(try #require(plan), &report, nil) == SPIELORDERS_RESULT_OK)
    let committedReport = try #require(report)
    let vendor = try #require(spielorders_report_vendor(committedReport))
    defer { spielorders_vendor_free(vendor) }
    return String(cString: spielorders_vendor_field(vendor, SPIELORDERS_VENDOR_FIELD_VENDOR_ID))
}

func openedSession(_ root: URL) async throws -> PackageSession {
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    return session
}

func listedOrders(_ session: PackageSession) async throws -> [OrderRow] {
    try await session.orders(query: "", caseSensitive: false, sortField: -1, descending: false,
                             cancellation: LibraryCancellation()).rows
}
