import CSpielOrders
import Foundation

struct RemovalOutcome: Sendable {
    let removedDirectory: URL
    let trashFailure: String?
    let warnings: [LibraryEntry]
    let requiresReopen: Bool
    var summary: String {
        let result = trashFailure == nil ? "Order moved to Trash." : "Order removed, but the Trash move failed. The Order remains recoverable."
        return requiresReopen ? result + " Reload the Package before continuing." : result
    }
    var message: String {
        var text = trashFailure.map {
            "Order removed, but the Trash move failed. It remains recoverable at " + removedDirectory.path + ".\n" + $0
        } ?? "Order moved to Trash."
        if requiresReopen { text += "\nReload the Package before continuing." }
        if !warnings.isEmpty { text += "\n" + warnings.map(\.presentation).joined(separator: "\n") }
        return text
    }
}

extension PackageSession {
    func refreshAfterCommit(_ previous: LibraryHandle<SpielordersPackage>) -> [LibraryEntry] {
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

/// A restore that the library refused or could not complete. Its details include any
/// incomplete rollback instead of promising that a failed commit had no effects.
public struct RestoreRefusal: Error, LocalizedError, Sendable {
    /// What happened, in one sentence for the owner.
    public let summary: String
    private let failure: LibraryFailure

    public var errorDescription: String? { failure.errorDescription }

    init(_ failure: LibraryFailure) {
        self.failure = failure
        let codes = Set(failure.entries.map(\.code))
        summary = if codes.contains(SPIELORDERS_FAILURE_DESTINATION_COLLISION) {
            "Couldn’t restore the Order: the Orders folder already holds a folder with its name."
        } else if codes.contains(SPIELORDERS_FAILURE_ORDER_IDENTITY_IN_USE) {
            "Couldn’t restore the Order: another Order already carries its Order ID."
        } else if !codes.isDisjoint(with: [SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED,
                                           SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED]) {
            "Couldn’t restore the Order: the Package changed. Reload the Package and try again."
        } else if !codes.isDisjoint(with: [SPIELORDERS_FAILURE_NOT_DIRECT_CHILD, SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY,
                                           SPIELORDERS_FAILURE_DAMAGED_RECORD]) {
            "Couldn’t restore the Order: it is no longer where Remove Order left it."
        } else {
            "Couldn’t restore the Order."
        }
    }
}

extension PackageSession {
    /// Returns the expected Order a removal left at `removed` to the Orders directory.
    /// Library refusals throw `RestoreRefusal`; a replaced selection is rejected before commit.
    public func restoreOrder(id: String, from removed: URL,
                             cancellation: LibraryCancellation) throws -> PackageChangeOutcome {
        guard let package else { throw SessionAccessError.reopenRequired }
        do {
            var planned: UnsafeMutablePointer<SpielordersPlan>?
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            let result = try FileSystemPath(url: removed).withUnsafeFileSystemRepresentation {
                spielorders_package_plan_restore_order(package.pointer, $0, cancellation.token, &planned, &outcome)
            }
            var warnings = try checked(result, outcome)
            guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            let plan = LibraryHandle(planned, free: spielorders_plan_free)
            guard id == "order:" + String(cString: spielorders_plan_restored_order_id(plan.pointer)) else {
                throw SessionAccessError.selectionChanged
            }
            try Task.checkCancellation()
            var report: UnsafeMutablePointer<SpielordersReport>?
            outcome = nil
            let committed = spielorders_plan_commit(plan.pointer, &report, &outcome)
            defer { spielorders_report_free(report) }
            warnings += try checked(committed, outcome)
            guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            let restored = URL(fileURLWithFileSystemRepresentation: spielorders_report_final_path(report),
                               isDirectory: true, relativeTo: nil)
            warnings += refreshAfterCommit(package)
            let requiresReopen = self.package == nil
            var messages = ["Order restored to \(restored.path)."]
            if requiresReopen { messages.append("Reload the Package before continuing.") }
            messages += warnings.map(\.presentation)
            return PackageChangeOutcome(finalDirectory: restored, requiresReopen: requiresReopen,
                                        notices: warnings.map(\.presentation),
                                        message: messages.joined(separator: "\n"),
                                        summary: requiresReopen ? "Order restored. Reload the Package before continuing."
                                            : "Order restored.")
        } catch let failure as LibraryFailure {
            throw RestoreRefusal(failure)
        }
    }
}
