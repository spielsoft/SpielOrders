import CSpielOrders
import Foundation
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

func createOrder(in root: URL, title: String = "Optics") throws -> URL {
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
    #expect(spielorders_draft_plan(try #require(draft), 0, &plan, nil) == SPIELORDERS_RESULT_OK)
    #expect(spielorders_plan_commit(try #require(plan), &report, nil) == SPIELORDERS_RESULT_OK)
    return URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(try #require(report)),
               isDirectory: true, relativeTo: nil)
}
