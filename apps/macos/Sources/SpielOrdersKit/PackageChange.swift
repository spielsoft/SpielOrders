import CSpielOrders
import Foundation

public enum DirectoryAction: Sendable {
    case align, remove, trash

    public var title: String {
        switch self {
        case .align: "Align Directory Name"
        case .remove: "Remove Order"
        case .trash: "Move to Trash"
        }
    }
}

/// A library-authored preview. Holding it writes nothing; committing checks its evidence.
public final class PackageChange: Sendable {
    let handle: LibraryHandle<SpielordersPlan>
    let source: LibraryHandle<SpielordersPackage>
    let notices: [LibraryEntry]
    let action: DirectoryAction?
    public let operations: [String]

    init(_ pointer: UnsafeMutablePointer<SpielordersPlan>, source: LibraryHandle<SpielordersPackage>,
         notices: [LibraryEntry], action: DirectoryAction?) {
        handle = LibraryHandle(pointer, free: spielorders_plan_free)
        self.source = source
        self.notices = notices
        self.action = action
        operations = (0..<spielorders_plan_operation_count(pointer)).map { index in
            let kind = spielorders_plan_operation_kind(pointer, index)
            let path = String(cString: spielorders_plan_operation_path(pointer, index))
            return "\(String(cString: spielorders_constant_name(kind))): \(path)"
        }
    }
}

public struct PackageChangeOutcome: Sendable {
    public let finalDirectory: URL?
    public let requiresReopen: Bool
    public let notices: [String]
    public let message: String
    public let summary: String
}

extension PackageSession {
    public func planDirectoryChange(_ row: OrderRow, action: DirectoryAction,
                                    cancellation: LibraryCancellation) throws -> PackageChange {
        guard let package else { throw SessionAccessError.reopenRequired }
        let (order, notices) = try readOrder(row, token: cancellation.token)
        var pointer: UnsafeMutablePointer<SpielordersPlan>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = action == .align
            ? spielorders_order_plan_align(order.pointer, cancellation.token, &pointer, &outcome)
            : spielorders_order_plan_remove(order.pointer, cancellation.token, &pointer, &outcome)
        let warnings = try notices + checked(result, outcome)
        guard let pointer else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return PackageChange(pointer, source: package, notices: warnings, action: action)
    }

    public func commit(_ change: PackageChange) throws -> PackageChangeOutcome {
        guard package === change.source else { throw SessionAccessError.reopenRequired }
        try Task.checkCancellation()
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_plan_commit(change.handle.pointer, &report, &outcome)
        defer { spielorders_report_free(report) }
        var notices = try change.notices + checked(result, outcome)
        guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let path = spielorders_report_final_path(report)
        let directory = path.pointee == 0 ? nil
            : URL(fileURLWithFileSystemRepresentation: path, isDirectory: true, relativeTo: nil)
        notices += refreshAfterCommit(change.source, report: report)
        let requiresReopen = package == nil
        if change.action == .trash, let directory {
            let trashFailure: String?
            do {
                try FileManager.default.trashItem(at: directory, resultingItemURL: nil)
                trashFailure = nil
            } catch { trashFailure = error.localizedDescription }
            let removal = RemovalOutcome(removedDirectory: directory, trashFailure: trashFailure,
                                         warnings: notices, requiresReopen: requiresReopen)
            return PackageChangeOutcome(finalDirectory: trashFailure == nil ? nil : directory,
                                        requiresReopen: requiresReopen, notices: notices.map(\.presentation),
                                        message: removal.message, summary: removal.summary)
        }
        var messages: [String]
        switch change.action {
        case .align: messages = ["Directory name aligned."]
        case .remove: messages = ["Order removed. It remains recoverable at \(directory?.path ?? "the Removed Orders folder")."]
        default: messages = ["Repair committed."]
        }
        if requiresReopen { messages.append("Reload the Package before continuing.") }
        messages += notices.map(\.presentation)
        return PackageChangeOutcome(finalDirectory: directory, requiresReopen: requiresReopen,
                                    notices: notices.map(\.presentation),
                                    message: messages.joined(separator: "\n"),
                                    summary: requiresReopen ? "Change committed. Reload the Package before continuing."
                                        : change.action == .align ? "Directory name aligned."
                                        : change.action == .remove ? "Order removed. Edit → Undo restores it." : "Repair committed.")
    }
}
