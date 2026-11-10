import CSpielOrders
import Foundation

public struct RemovalOutcome: Sendable {
    let removedDirectory: URL
    let trashFailure: String?
    let warnings: [LibraryEntry]
    public let requiresReopen: Bool
    public var message: String {
        var text = trashFailure.map {
            "Order removed, but the Trash move failed. It remains recoverable at " + removedDirectory.path + ".\n" + $0
        } ?? "Order moved to Trash."
        if requiresReopen { text += "\nReopen the Package before continuing." }
        if !warnings.isEmpty { text += "\n" + warnings.map(\.message).joined(separator: "\n") }
        return text
    }
}

extension PackageSession {
    public func removeToTrash(
        _ directory: URL, cancellation: LibraryCancellation
    ) throws -> RemovalOutcome {
        guard let package else { throw SessionAccessError.reopenRequired }
        guard directory.isFileURL else {
            throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: [])
        }
        var order: UnsafeMutablePointer<SpielordersOrder>?
        var plan: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        defer {
            spielorders_report_free(report)
            spielorders_plan_free(plan)
            spielorders_order_free(order)
        }
        let read = try directory.withUnsafeFileSystemRepresentation { path in
            guard let path else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
            return spielorders_package_read_order(package.pointer, path, cancellation.token, &order, &outcome)
        }
        var warnings = try checked(read, outcome)
        guard let order else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        outcome = nil
        let planned = spielorders_order_plan_remove(order, cancellation.token, &plan, &outcome)
        warnings += try checked(planned, outcome)
        guard let plan else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        try Task.checkCancellation()
        outcome = nil
        let committed = spielorders_plan_commit(plan, &report, &outcome)
        warnings += try checked(committed, outcome)
        guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let removed = URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(report),
                          isDirectory: true, relativeTo: nil)
        warnings += refreshPackageAfterCommit(package)
        do {
            try FileManager.default.trashItem(at: removed, resultingItemURL: nil)
            return RemovalOutcome(removedDirectory: removed, trashFailure: nil,
                                  warnings: warnings, requiresReopen: self.package == nil)
        } catch {
            return RemovalOutcome(removedDirectory: removed, trashFailure: error.localizedDescription,
                                  warnings: warnings, requiresReopen: self.package == nil)
        }
    }

    func refreshPackageAfterCommit(_ previous: LibraryHandle<SpielordersPackage>) -> [LibraryEntry] {
        package = nil
        var refreshed: UnsafeMutablePointer<SpielordersPackage>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_package_open(spielorders_package_root(previous.pointer), 0, &refreshed, &outcome)
        let warnings: [LibraryEntry]
        do { warnings = try checked(result, outcome) }
        catch let failure as LibraryFailure { warnings = failure.entries }
        catch { return [] }
        if let refreshed { package = LibraryHandle(refreshed, free: spielorders_package_free) }
        return warnings
    }
}
